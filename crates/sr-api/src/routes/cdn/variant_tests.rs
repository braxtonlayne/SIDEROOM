//! `?size=` / `format=` variants (FND-STOR-003) over the in-memory backend.

use super::*;
use axum::{
    body::Body,
    http::{Request, Response, StatusCode},
};
use bytes::Bytes;
use http_body_util::BodyExt;
use image::{Delay, Frame, ImageFormat, Rgba, RgbaImage, codecs::gif::GifEncoder};
use sr_storage::{ObjectStoreStorage, img::Limits};
use tower::ServiceExt;

const SECRET: &[u8] = b"0123456789abcdef0123456789abcdef";
const P: &str = "/cdn/attachments/1/2/a.png";
const HASH: &str = "0123456789abcdef0123456789abcdef";

struct Fx {
    app: Router,
    signer: CdnSigner,
    storage: Arc<ObjectStoreStorage>,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = RgbaImage::from_fn(w, h, |x, y| {
        Rgba([(x % 255) as u8, (y % 255) as u8, 7, 255])
    });
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut out), ImageFormat::Png)
        .unwrap();
    out
}

fn gif(w: u32, h: u32, n: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = GifEncoder::new(&mut out);
        for i in 0..n {
            let img = RgbaImage::from_fn(w, h, |x, _| {
                if (x + i) % 2 == 0 {
                    Rgba([255, 0, 0, 255])
                } else {
                    Rgba([0, 0, 255, 255])
                }
            });
            enc.encode_frame(Frame::from_parts(
                img,
                0,
                0,
                Delay::from_numer_denom_ms(50, 1),
            ))
            .unwrap();
        }
    }
    out
}

async fn fx(objects: &[(&str, Vec<u8>)], limits: Limits) -> Fx {
    let storage = Arc::new(ObjectStoreStorage::in_memory());
    for (k, v) in objects {
        storage.put(k, Bytes::from(v.clone())).await.unwrap();
    }
    let signer = CdnSigner::new(SECRET);
    let state = AppState::builder()
        .extension(Arc::new(
            CdnDeps::new(storage.clone(), signer.clone()).with_limits(limits),
        ))
        .build();
    Fx {
        app: layered(state),
        signer,
        storage,
    }
}

impl Fx {
    fn variant(&self, path: &str, size: Option<u32>, fmt: Option<&str>) -> String {
        self.signer
            .signed_variant_url(path, size, fmt, now() + 3600)
    }
    async fn send(&self, method: &str, uri: &str, hdrs: &[(&str, &str)]) -> Response<Body> {
        let mut r = Request::builder().method(method).uri(uri);
        for (k, v) in hdrs {
            r = r.header(*k, *v);
        }
        self.app
            .clone()
            .oneshot(r.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }
    async fn get(&self, uri: &str) -> Response<Body> {
        self.send("GET", uri, &[]).await
    }
}

async fn body(r: Response<Body>) -> Bytes {
    r.into_body().collect().await.unwrap().to_bytes()
}

fn h<'a>(r: &'a Response<Body>, n: &str) -> &'a str {
    r.headers()
        .get(n)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

fn dims(b: &[u8]) -> (u32, u32) {
    image::ImageReader::new(std::io::Cursor::new(b))
        .with_guessed_format()
        .unwrap()
        .into_dimensions()
        .unwrap()
}

#[tokio::test]
async fn resizes_and_caches_under_a_derived_key() {
    let f = fx(&[("chat/1/2/a.png", png(400, 200))], Limits::default()).await;
    let url = f.variant(P, Some(64), None);
    let r = f.get(&url).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(h(&r, "content-type"), "image/png");
    assert_eq!(h(&r, "vary"), "Origin, Accept");
    assert!(h(&r, "content-disposition").starts_with("inline"));
    let b = body(r).await;
    assert_eq!(dims(&b), (64, 32));
    // Stored once under the derived key, byte-identical.
    let cached = f.storage.get("chat/1/2/a.png@64.png").await.unwrap();
    assert_eq!(cached, b);

    // Second request is served from the cache: replace the cached object with
    // a recognisable one and see it come back.
    let marker = Bytes::from(png(16, 16));
    f.storage
        .put("chat/1/2/a.png@64.png", marker.clone())
        .await
        .unwrap();
    let r = f.get(&url).await;
    assert_eq!(body(r).await, marker);
}

#[tokio::test]
async fn never_upscales() {
    let f = fx(&[("chat/1/2/a.png", png(400, 200))], Limits::default()).await;
    let r = f.get(&f.variant(P, Some(2048), None)).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(dims(&body(r).await), (400, 200));
}

#[tokio::test]
async fn format_param_and_accept_negotiation() {
    let f = fx(&[("chat/1/2/a.png", png(100, 100))], Limits::default()).await;
    for (fmt, mime) in [
        ("jpg", "image/jpeg"),
        ("jpeg", "image/jpeg"),
        ("webp", "image/webp"),
        ("gif", "image/gif"),
        ("png", "image/png"),
    ] {
        let r = f.get(&f.variant(P, Some(32), Some(fmt))).await;
        assert_eq!(r.status(), StatusCode::OK, "{fmt}");
        assert_eq!(h(&r, "content-type"), mime);
        assert_eq!(h(&r, "vary"), "Origin", "an explicit format ignores Accept");
    }
    // Accept: webp picks WebP for a PNG source.
    let url = f.variant(P, Some(32), None);
    let r = f
        .send(
            "GET",
            &url,
            &[("accept", "image/avif,image/webp,*/*;q=0.8")],
        )
        .await;
    assert_eq!(h(&r, "content-type"), "image/webp");
    // No WebP in Accept: PNG.
    let r = f.send("GET", &url, &[("accept", "image/png")]).await;
    assert_eq!(h(&r, "content-type"), "image/png");
    // Format only (no size): converted at the original size.
    let r = f.get(&f.variant(P, None, Some("webp"))).await;
    assert_eq!(h(&r, "content-type"), "image/webp");
    assert_eq!(dims(&body(r).await), (100, 100));
    assert!(f.storage.head("chat/1/2/a.png@orig.webp").await.is_ok());
}

#[tokio::test]
async fn signature_covers_size_and_format() {
    let f = fx(&[("chat/1/2/a.png", png(100, 100))], Limits::default()).await;
    // A bare-path signature (an old URL) cannot be used to request a variant.
    let bare = f.signer.signed_url(P, now() + 3600);
    let r = f.get(&format!("{bare}&size=32")).await;
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
    let r = f.get(&format!("{bare}&format=webp")).await;
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
    // But it still serves the original.
    let r = f.get(&bare).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(body(r).await.as_ref(), png(100, 100).as_slice());
    // A signed variant cannot be retargeted.
    let url = f.variant(P, Some(32), Some("png"));
    let r = f.get(&url.replace("size=32", "size=64")).await;
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
    let r = f.get(&url.replace("format=png", "format=jpg")).await;
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn rejects_bad_sizes_and_formats() {
    let f = fx(&[("chat/1/2/a.png", png(100, 100))], Limits::default()).await;
    for s in [0u32, 8, 100, 4097, 8192] {
        let r = f.get(&f.variant(P, Some(s), None)).await;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST, "size {s}");
    }
    let r = f.get(&f.variant(P, Some(32), Some("avif"))).await;
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    // Non-numeric size fails the signature check and never reaches storage.
    let r = f.get(&format!("{P}?size=abc&ex=1&sig=x")).await;
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn pixel_budget_gives_413() {
    let limits = Limits {
        max_pixels: 1000,
        ..Limits::default()
    };
    let f = fx(&[("chat/1/2/a.png", png(100, 100))], limits).await;
    let r = f.get(&f.variant(P, Some(32), None)).await;
    assert_eq!(r.status(), StatusCode::PAYLOAD_TOO_LARGE);
    // Nothing was cached.
    assert!(f.storage.head("chat/1/2/a.png@32.png").await.is_err());
    // The byte budget too.
    let limits = Limits {
        max_bytes: 50,
        ..Limits::default()
    };
    let f = fx(&[("chat/1/2/a.png", png(100, 100))], limits).await;
    let r = f.get(&f.variant(P, Some(32), None)).await;
    assert_eq!(r.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn non_images_are_415_and_missing_is_404() {
    let f = fx(
        &[("chat/1/2/a.png", b"<html>not an image</html>".to_vec())],
        Limits::default(),
    )
    .await;
    let r = f.get(&f.variant(P, Some(32), None)).await;
    assert_eq!(r.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let r = f
        .get(&f.variant("/cdn/attachments/1/2/none.png", Some(32), None))
        .await;
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn animated_gif_is_preserved() {
    let f = fx(&[("emoji/9/3.gif", gif(64, 64, 4))], Limits::default()).await;
    let url = f.variant("/cdn/emoji/9/3.gif", Some(32), None);
    let r = f.get(&url).await;
    assert_eq!(h(&r, "content-type"), "image/gif");
    let b = body(r).await;
    assert_eq!(dims(&b), (32, 32));
    let (_, d) = sr_storage::img::decode(&b, &Limits::default()).unwrap();
    assert!(d.is_animated());
    // Explicit PNG: first frame.
    let r = f
        .get(&f.variant("/cdn/emoji/9/3.gif", Some(32), Some("png")))
        .await;
    assert_eq!(h(&r, "content-type"), "image/png");
}

#[tokio::test]
async fn etag_304_and_head() {
    let f = fx(&[("chat/1/2/a.png", png(100, 100))], Limits::default()).await;
    let url = f.variant(P, Some(32), None);
    let r = f.get(&url).await;
    let tag = h(&r, "etag").to_owned();
    assert!(tag.starts_with('"'));
    let r = f.send("GET", &url, &[("if-none-match", &tag)]).await;
    assert_eq!(r.status(), StatusCode::NOT_MODIFIED);
    let r = f.send("HEAD", &url, &[]).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert!(h(&r, "content-length").parse::<u64>().unwrap() > 0);
    assert!(body(r).await.is_empty());
    // Different variants have different tags.
    let other = f.get(&f.variant(P, Some(64), None)).await;
    assert_ne!(h(&other, "etag"), tag);
}

#[tokio::test]
async fn public_avatars_need_no_signature_and_take_the_extension_as_format() {
    let key = format!("avatars/7/{HASH}");
    let f = fx(&[(key.as_str(), png(200, 200))], Limits::default()).await;
    let base = format!("/cdn/avatars/7/{HASH}");

    // Same format as stored, no size: the stored object itself.
    let r = f.get(&format!("{base}.png")).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(h(&r, "content-type"), "image/png");
    assert_eq!(
        h(&r, "cache-control"),
        "public, max-age=31536000, immutable"
    );
    assert_eq!(body(r).await.as_ref(), png(200, 200).as_slice());

    // Other extension / size: a variant, cached under the derived key.
    let r = f.get(&format!("{base}.webp?size=64")).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(h(&r, "content-type"), "image/webp");
    assert_eq!(h(&r, "vary"), "Origin");
    assert_eq!(dims(&body(r).await), (64, 64));
    assert!(f.storage.head(&format!("{key}@64.webp")).await.is_ok());
    let r = f.get(&format!("{base}.jpg?size=128")).await;
    assert_eq!(h(&r, "content-type"), "image/jpeg");

    // The bare hash (no extension) works too.
    let r = f.get(&base).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(h(&r, "content-type"), "image/png");

    // A bad size is 400 even unsigned; an unknown object is 404.
    let r = f.get(&format!("{base}.png?size=100")).await;
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    let r = f
        .get("/cdn/avatars/7/ffffffffffffffffffffffffffffffff.png")
        .await;
    assert_eq!(r.status(), StatusCode::NOT_FOUND);

    // A signature that is present must be right.
    let r = f.get(&format!("{base}.png?ex=1&sig=bad")).await;
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
    // A valid one is accepted and uses the private cache policy.
    let url = f.signer.signed_url(&format!("{base}.png"), now() + 600);
    let r = f.get(&url).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert!(h(&r, "cache-control").starts_with("private"));
}

#[tokio::test]
async fn attachments_still_require_a_signature() {
    let f = fx(&[("chat/1/2/a.png", png(50, 50))], Limits::default()).await;
    assert_eq!(f.get(P).await.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        f.get(&format!("{P}?size=32")).await.status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn animated_public_icon_keeps_animation_as_gif() {
    let key = "icons/5/a_0123456789abcdef0123456789abcdef";
    let f = fx(&[(key, gif(64, 64, 3))], Limits::default()).await;
    let r = f
        .get("/cdn/icons/5/a_0123456789abcdef0123456789abcdef.gif?size=16")
        .await;
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(h(&r, "content-type"), "image/gif");
    let (_, d) = sr_storage::img::decode(&body(r).await, &Limits::default()).unwrap();
    assert!(d.is_animated());
}
