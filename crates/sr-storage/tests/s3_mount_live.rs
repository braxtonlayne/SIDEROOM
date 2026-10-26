//! BYOS-SCP-002 against a real S3 service (Garage in `storage.yml`): a mount with a key prefix lists one
//! level, presigns a read that carries the prefix, and a presigned ranged `GET` answers 206 with a readable
//! `content-range`. The same env as `s3_live.rs`; skipped without `SR_TEST_S3_ENDPOINT`.
//!
//! `SR_TEST_S3_ENDPOINT=http://127.0.0.1:3900 SR_TEST_S3_BUCKET=... SR_TEST_S3_ACCESS_KEY=... \
//!  SR_TEST_S3_SECRET_KEY=... cargo test -p sr-storage --test s3_mount_live -- --ignored`

use std::time::Duration;

use bytes::Bytes;
use sr_storage::ObjectStorage;
use sr_storage::PresignMethod;
use sr_storage::mounts::{
    Mount, MountError, MountOwner, MountStatus, Provider, SealedSecret, Sealer, Secret,
};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

/// Identity "sealing": the test holds the secret in the clear.
struct Plain;
impl Sealer for Plain {
    fn seal(&self, _aad: &[u8], plain: &[u8]) -> Result<Vec<u8>, MountError> {
        Ok(plain.to_vec())
    }
    fn open(&self, _aad: &[u8], blob: &[u8]) -> Result<Vec<u8>, MountError> {
        Ok(blob.to_vec())
    }
}

fn mount(prefix: &str) -> Mount {
    let sealed = SealedSecret::seal(&Plain, 7, &Secret::new(env("SR_TEST_S3_SECRET_KEY"))).unwrap();
    Mount {
        id: 7,
        owner: MountOwner::User(1),
        label: "live".into(),
        provider: Provider::Garage,
        endpoint: env("SR_TEST_S3_ENDPOINT"),
        region: std::env::var("SR_TEST_S3_REGION").unwrap_or_else(|_| "garage".into()),
        bucket: env("SR_TEST_S3_BUCKET"),
        prefix: prefix.into(),
        path_style: true,
        access_key_id: env("SR_TEST_S3_ACCESS_KEY"),
        secret_key_enc: sealed,
        key_hint: "live".into(),
        status: MountStatus::Active,
        cors_ok_at: Some(1),
        last_test_at: None,
        last_test: None,
        created_at: 0,
        updated_at: 0,
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_S3_ENDPOINT"]
async fn mount_lists_a_level_and_serves_a_presigned_range() {
    if std::env::var("SR_TEST_S3_ENDPOINT").is_err() {
        return;
    }
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let prefix = format!("sr-live-test/mount-{n}");
    let m = mount(&prefix);
    let s = m.storage(&Plain).unwrap();
    let data: Vec<u8> = (0..100_000u32).map(|i| (i % 251) as u8).collect();
    s.put(&m.bucket_key("albums/a.mp3"), Bytes::from(data.clone()))
        .await
        .unwrap();
    s.put(&m.bucket_key("top.txt"), Bytes::from_static(b"hi"))
        .await
        .unwrap();

    // One level under the mount prefix: a folder and an object.
    let page = s
        .list_dir(&format!("{prefix}/"), true, None, 1000)
        .await
        .unwrap();
    assert_eq!(page.folders, vec![format!("{prefix}/albums/")]);
    assert_eq!(page.objects.len(), 1);
    assert_eq!(page.objects[0].key, format!("{prefix}/top.txt"));
    assert_eq!(page.objects[0].size, 2);

    // The presigned URL carries the prefix and serves a range.
    let url = s
        .presign(
            PresignMethod::Get,
            &m.bucket_key("albums/a.mp3"),
            Duration::from_secs(60),
        )
        .await
        .unwrap();
    assert!(url.contains(&format!("{prefix}/albums/a.mp3")), "{url}");
    let r = reqwest::Client::new()
        .get(&url)
        .header("Range", "bytes=10-19")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 206);
    assert_eq!(
        r.headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok()),
        Some("bytes 10-19/100000")
    );
    assert_eq!(r.bytes().await.unwrap().as_ref(), &data[10..20]);

    let meta = s.head_meta(&m.bucket_key("albums/a.mp3")).await.unwrap();
    assert_eq!(meta.size, 100_000);
    s.delete(&m.bucket_key("albums/a.mp3")).await.unwrap();
    s.delete(&m.bucket_key("top.txt")).await.unwrap();
}
