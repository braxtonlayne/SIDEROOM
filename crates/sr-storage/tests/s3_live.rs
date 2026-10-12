//! Live S3 checks for `ObjectStoreStorage::s3` against a real S3-compatible server
//! (Garage, MinIO, ...). Ignored by default; see docs/dev/storage-live-test.md.
//!
//! ```text
//! SR_TEST_S3_ENDPOINT=http://127.0.0.1:9000 SR_TEST_S3_BUCKET=sr-test \
//! SR_TEST_S3_ACCESS_KEY=... SR_TEST_S3_SECRET_KEY=... \
//!   cargo test -p sr-storage --test s3_live -- --ignored --test-threads=1
//! ```
//!
//! Optional: `SR_TEST_S3_REGION` (default `garage`). The bucket must exist. Every
//! test writes under a unique `sr-live-test/<random>/` prefix and cleans up.
//! The endpoint must be plain `http://` (the tests speak raw HTTP/1.1).

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use image::DynamicImage;
use image::codecs::jpeg::JpegEncoder;
use sr_storage::upload::{sniff_mime, strip_metadata};
use sr_storage::{ObjectStorage, ObjectStoreStorage, PresignMethod, StorageError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set"))
}

fn region() -> String {
    std::env::var("SR_TEST_S3_REGION").unwrap_or_else(|_| "garage".into())
}

fn storage() -> ObjectStoreStorage {
    ObjectStoreStorage::s3(
        &env("SR_TEST_S3_ENDPOINT"),
        &env("SR_TEST_S3_BUCKET"),
        &region(),
        &env("SR_TEST_S3_ACCESS_KEY"),
        &env("SR_TEST_S3_SECRET_KEY"),
    )
    .expect("build s3 storage")
}

/// A unique key prefix per test run.
fn prefix(test: &str) -> String {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("sr-live-test/{test}-{n}")
}

struct Resp {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Resp {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Minimal HTTP/1.1 client over plain TCP (no extra dependencies).
async fn http(method: &str, url: &str, extra: &[(&str, &str)], body: &[u8]) -> Resp {
    let rest = url
        .strip_prefix("http://")
        .unwrap_or_else(|| panic!("live tests need an http:// URL, got {url}"));
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let mut s = TcpStream::connect(authority).await.expect("connect");
    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n");
    if !body.is_empty() || method == "PUT" {
        req.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    for (k, v) in extra {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes()).await.expect("write head");
    s.write_all(body).await.expect("write body");
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).await.expect("read response");
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("response head");
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let mut lines = head.lines();
    let status: u16 = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse().ok())
        .expect("status line");
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect();
    let mut body = raw[split + 4..].to_vec();
    if headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("transfer-encoding") && v.contains("chunked"))
    {
        body = dechunk(&body);
    }
    Resp {
        status,
        headers,
        body,
    }
}

fn dechunk(mut b: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let Some(i) = b.windows(2).position(|w| w == b"\r\n") else {
            return out;
        };
        let size = usize::from_str_radix(
            String::from_utf8_lossy(&b[..i])
                .split(';')
                .next()
                .unwrap_or("0")
                .trim(),
            16,
        )
        .unwrap_or(0);
        if size == 0 {
            return out;
        }
        out.extend_from_slice(&b[i + 2..i + 2 + size]);
        b = &b[i + 2 + size + 2..];
    }
}

fn pattern(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

#[tokio::test]
#[ignore = "needs SR_TEST_S3_ENDPOINT"]
async fn crud_range_head_list() {
    let s = storage();
    let p = prefix("crud");
    let (a, b) = (format!("{p}/a/one.bin"), format!("{p}/b/two.bin"));
    s.put(&a, Bytes::from_static(b"0123456789")).await.unwrap();
    s.put(&b, Bytes::from_static(b"xy")).await.unwrap();
    assert_eq!(s.get(&a).await.unwrap(), Bytes::from_static(b"0123456789"));
    assert_eq!(
        s.get_range(&a, 2, 5).await.unwrap(),
        Bytes::from_static(b"234")
    );
    assert_eq!(s.head(&a).await.unwrap().size, 10);
    assert_eq!(s.head(&a).await.unwrap().key, a);
    let mut keys: Vec<_> = s
        .list(&p)
        .await
        .unwrap()
        .into_iter()
        .map(|o| o.key)
        .collect();
    keys.sort();
    assert_eq!(keys, [a.clone(), b.clone()]);
    // Overwrite replaces.
    s.put(&a, Bytes::from_static(b"new")).await.unwrap();
    assert_eq!(s.head(&a).await.unwrap().size, 3);
    s.delete(&a).await.unwrap();
    s.delete(&b).await.unwrap();
    assert!(matches!(s.get(&a).await, Err(StorageError::NotFound(_))));
    assert!(matches!(s.head(&a).await, Err(StorageError::NotFound(_))));
    assert!(s.list(&p).await.unwrap().is_empty());
}

#[tokio::test]
#[ignore = "needs SR_TEST_S3_ENDPOINT"]
async fn presigned_put_then_get_and_range() {
    let s = storage();
    let key = format!("{}/up/file name.bin", prefix("presign"));
    let data = pattern(100_000);
    let put = s
        .presign(PresignMethod::Put, &key, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(put.starts_with("http://"), "{put}");
    let r = http(
        "PUT",
        &put,
        &[("Content-Type", "application/octet-stream")],
        &data,
    )
    .await;
    assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
    assert_eq!(s.head(&key).await.unwrap().size, 100_000);

    let get = s
        .presign(PresignMethod::Get, &key, Duration::from_secs(60))
        .await
        .unwrap();
    let r = http("GET", &get, &[], &[]).await;
    assert_eq!(r.status, 200);
    assert_eq!(r.body, data);
    let r = http("GET", &get, &[("Range", "bytes=10-19")], &[]).await;
    assert_eq!(r.status, 206);
    assert_eq!(r.body, &data[10..20]);
    assert_eq!(r.header("content-range"), Some("bytes 10-19/100000"));

    // A tampered signature is rejected.
    let bad = get.replace("X-Amz-Signature=", "X-Amz-Signature=00");
    assert_eq!(http("GET", &bad, &[], &[]).await.status, 403);
    // A GET URL cannot be used to PUT.
    assert_eq!(http("PUT", &get, &[], b"x").await.status, 403);
    s.delete(&key).await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_S3_ENDPOINT"]
async fn presigned_urls_expire() {
    let s = storage();
    let key = format!("{}/expire.bin", prefix("expire"));
    s.put(&key, Bytes::from_static(b"hello")).await.unwrap();
    let get = s
        .presign(PresignMethod::Get, &key, Duration::from_secs(2))
        .await
        .unwrap();
    assert_eq!(http("GET", &get, &[], &[]).await.status, 200);
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert_eq!(http("GET", &get, &[], &[]).await.status, 403);
    let put = s
        .presign(PresignMethod::Put, &key, Duration::from_secs(1))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(http("PUT", &put, &[], b"late").await.status, 403);
    assert_eq!(s.get(&key).await.unwrap(), Bytes::from_static(b"hello"));
    s.delete(&key).await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_S3_ENDPOINT"]
async fn multipart_over_64_mib() {
    let s = storage();
    let key = format!("{}/big.bin", prefix("multipart"));
    const PART: usize = 8 * 1024 * 1024;
    let total = 72 * 1024 * 1024 + 123; // > 64 MiB, ragged last part
    let data = pattern(total);
    let mut m = s.multipart_create(&key).await.unwrap();
    for chunk in data.chunks(PART) {
        m.put_part(Bytes::copy_from_slice(chunk)).await.unwrap();
    }
    m.complete().await.unwrap();
    assert_eq!(s.head(&key).await.unwrap().size, total as u64);
    let off = (PART - 5) as u64;
    assert_eq!(
        s.get_range(&key, off, off + 10).await.unwrap().as_ref(),
        &data[off as usize..off as usize + 10]
    );
    assert_eq!(s.get(&key).await.unwrap().as_ref(), data.as_slice());
    s.delete(&key).await.unwrap();

    // Abort leaves no object.
    let key2 = format!("{}/aborted.bin", prefix("multipart"));
    let mut m = s.multipart_create(&key2).await.unwrap();
    m.put_part(Bytes::from(pattern(PART))).await.unwrap();
    m.abort().await.unwrap();
    assert!(matches!(
        s.head(&key2).await,
        Err(StorageError::NotFound(_))
    ));
}

/// A 4x2 JPEG carrying EXIF orientation `tag` and a GPS-looking marker.
fn jpeg_with_orientation(tag: u16) -> Vec<u8> {
    let img = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(4, 2, image::Rgb([200, 10, 10])));
    let mut base = Vec::new();
    JpegEncoder::new(&mut base).encode_image(&img).unwrap();
    let mut tiff = b"II*\0".to_vec();
    tiff.extend_from_slice(&8u32.to_le_bytes());
    tiff.extend_from_slice(&1u16.to_le_bytes());
    tiff.extend_from_slice(&0x0112u16.to_le_bytes());
    tiff.extend_from_slice(&3u16.to_le_bytes());
    tiff.extend_from_slice(&1u32.to_le_bytes());
    tiff.extend_from_slice(&tag.to_le_bytes());
    tiff.extend_from_slice(&[0, 0]);
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff.extend_from_slice(b"GPS-SECRET");
    let mut payload = b"Exif\0\0".to_vec();
    payload.extend(tiff);
    let mut out = base[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
    out.extend(payload);
    out.extend_from_slice(&base[2..]);
    out
}

/// The attachment completion pipeline against real storage: presigned PUT, sniff
/// the first bytes via a ranged read, strip, overwrite, read back.
#[tokio::test]
#[ignore = "needs SR_TEST_S3_ENDPOINT"]
async fn exif_strip_roundtrip_on_real_jpeg() {
    let s = storage();
    let key = format!("{}/photo.jpg", prefix("exif"));
    let original = jpeg_with_orientation(6);
    let url = s
        .presign(PresignMethod::Put, &key, Duration::from_secs(60))
        .await
        .unwrap();
    let r = http("PUT", &url, &[("Content-Type", "image/jpeg")], &original).await;
    assert_eq!(r.status, 200);

    let info = s.head(&key).await.unwrap();
    assert_eq!(info.size, original.len() as u64);
    let head = s.get_range(&key, 0, info.size.min(512)).await.unwrap();
    assert_eq!(sniff_mime(&head), Some("image/jpeg"));
    let whole = s.get(&key).await.unwrap();
    assert_eq!(whole.as_ref(), original.as_slice());
    let clean = strip_metadata(&whole).unwrap();
    assert_ne!(clean.len(), whole.len());
    s.put(&key, clean.into()).await.unwrap();

    let stored = s.get(&key).await.unwrap();
    assert!(!stored.windows(10).any(|w| w == b"GPS-SECRET"));
    assert!(!stored.windows(4).any(|w| w == b"Exif"));
    let img = image::load_from_memory(&stored).unwrap();
    assert_eq!((img.width(), img.height()), (2, 4), "orientation 6 applied");
    s.delete(&key).await.unwrap();
}

/// Wrong credentials and a missing bucket surface as errors, not panics or hangs.
#[tokio::test]
#[ignore = "needs SR_TEST_S3_ENDPOINT"]
async fn bad_credentials_and_bucket_error_cleanly() {
    let bad = ObjectStoreStorage::s3(
        &env("SR_TEST_S3_ENDPOINT"),
        &env("SR_TEST_S3_BUCKET"),
        &region(),
        &env("SR_TEST_S3_ACCESS_KEY"),
        "definitely-not-the-secret",
    )
    .unwrap();
    assert!(matches!(
        bad.head(".sr-readyz").await,
        Err(StorageError::Backend(_))
    ));
    let nobucket = ObjectStoreStorage::s3(
        &env("SR_TEST_S3_ENDPOINT"),
        "sr-no-such-bucket-xyz",
        &region(),
        &env("SR_TEST_S3_ACCESS_KEY"),
        &env("SR_TEST_S3_SECRET_KEY"),
    )
    .unwrap();
    assert!(nobucket.head(".sr-readyz").await.is_err());
    // The readiness sentinel on the real bucket is "not found", which counts as ready.
    assert!(matches!(
        storage().head(".sr-readyz").await,
        Ok(_) | Err(StorageError::NotFound(_))
    ));
}
