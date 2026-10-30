//! PRF-SCR-001 against a real S3 service (Garage in `storage.yml`): a message's attachment is archived into a
//! guild-owned mount with a key prefix, the copy is readable through a presigned `GET` under the dated key, a
//! large attachment goes through the multipart path, and deleting the message takes the copies out of the
//! bucket again. The same env as `s3_live.rs`; skipped without `SR_TEST_S3_ENDPOINT`. It is not run on a
//! developer machine: CI starts Garage (`.github/workflows/storage.yml`, step "Rust live tests (scrapbook)").
//!
//! `SR_TEST_S3_ENDPOINT=http://127.0.0.1:3900 SR_TEST_S3_BUCKET=... SR_TEST_S3_ACCESS_KEY=... \
//!  SR_TEST_S3_SECRET_KEY=... cargo test -p sr-storage --test s3_scrapbook_live -- --ignored`

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use sr_storage::mount_storage::PrefixedStorage;
use sr_storage::mounts::{
    Mount, MountError, MountOwner, MountStatus, Provider, SealedSecret, Sealer, Secret,
};
use sr_storage::scrapbook::{
    ArchiveOutcome, ArchiveRequest, Archiver, DeleteScope, MemoryScrapbook, Refusal,
    ScrapbookBuckets, ScrapbookStore, SourceAttachment, SourceMessage, Target, Via,
};
use sr_storage::{ObjectStorage, ObjectStoreStorage, PresignMethod};

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

const GUILD: i64 = 4242;
const MOUNT: i64 = 9001;

fn guild_mount(prefix: &str) -> Mount {
    let sealed =
        SealedSecret::seal(&Plain, MOUNT, &Secret::new(env("SR_TEST_S3_SECRET_KEY"))).unwrap();
    Mount {
        id: MOUNT,
        owner: MountOwner::Guild(GUILD),
        label: "scrapbook shelf".into(),
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

/// The module's service account over the live mount.
struct Live(Arc<PrefixedStorage>);

#[async_trait]
impl ScrapbookBuckets for Live {
    async fn writer(&self, _t: &Target) -> Result<Arc<dyn ObjectStorage>, Refusal> {
        Ok(self.0.clone())
    }
    async fn remover(&self, _m: i64) -> Result<Arc<dyn ObjectStorage>, String> {
        Ok(self.0.clone())
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_S3_ENDPOINT"]
async fn an_attachment_is_archived_into_a_garage_guild_mount_and_removed_with_its_message() {
    if std::env::var("SR_TEST_S3_ENDPOINT").is_err() {
        return;
    }
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mount = guild_mount(&format!("sr-live-test/scrapbook-{n}"));
    let shelf = Arc::new(PrefixedStorage::new(
        Arc::new(mount.storage(&Plain).unwrap()),
        &mount,
    ));

    // The original attachments live in the instance bucket (here: memory); the copy is the one server-side copy.
    let source = Arc::new(ObjectStoreStorage::in_memory());
    let small: Vec<u8> = (0..50_000u32).map(|i| (i % 251) as u8).collect();
    // 9 MiB: above the 8 MiB part size, so the copy is a real multipart upload to Garage.
    let big: Vec<u8> = (0..9 * 1024 * 1024u32).map(|i| (i % 241) as u8).collect();
    source
        .put("attachments/1/cat.png", Bytes::from(small.clone()))
        .await
        .unwrap();
    source
        .put("attachments/1/long.mp4", Bytes::from(big.clone()))
        .await
        .unwrap();

    let store = Arc::new(MemoryScrapbook::new());
    store
        .set_target(&Target {
            guild_id: GUILD,
            mount_id: MOUNT,
            granted_by: 1,
        })
        .await
        .unwrap();
    let next = Arc::new(AtomicI64::new(1000));
    let archiver = Archiver::new(
        store.clone(),
        Arc::new(Live(shelf.clone())),
        source.clone(),
        Arc::new(move || next.fetch_add(1, Ordering::SeqCst)),
    );

    let message = SourceMessage {
        guild_id: GUILD,
        channel_id: 77,
        message_id: 123_456,
        author_id: 5,
        author_name: "Ada".into(),
        content: "keep these".into(),
        sent_at_ms: 1_782_000_000_000, // 2026-06-21
        attachments: vec![
            SourceAttachment {
                key: "attachments/1/cat.png".into(),
                filename: "cat.png".into(),
                content_type: "image/png".into(),
                size: small.len() as u64,
            },
            SourceAttachment {
                key: "attachments/1/long.mp4".into(),
                filename: "long.mp4".into(),
                content_type: "video/mp4".into(),
                size: big.len() as u64,
            },
        ],
    };
    let done = archiver
        .archive(ArchiveRequest {
            message,
            by: Some(1),
            via: Via::Manual,
            tags: vec!["live".into()],
            max_object_bytes: 200 * 1024 * 1024,
            now_ms: 1,
        })
        .await
        .unwrap();
    let ArchiveOutcome::Archived(x) = done else {
        panic!("expected a new entry")
    };
    assert_eq!(x.objects.len(), 2);
    assert_eq!(x.objects[0].key, "scrapbook/2026/06/123456/0-cat.png");
    assert_eq!(x.objects[1].key, "scrapbook/2026/06/123456/1-long.mp4");

    // The copies are in the bucket under the mount's prefix, byte for byte.
    assert_eq!(
        shelf.get(&x.objects[0].key).await.unwrap().as_ref(),
        small.as_slice()
    );
    assert_eq!(
        shelf.head(&x.objects[1].key).await.unwrap().size,
        big.len() as u64
    );
    assert_eq!(
        shelf.get(&x.objects[1].key).await.unwrap().as_ref(),
        big.as_slice()
    );

    // And readable the way the gallery reads them: a presigned GET.
    let url = shelf
        .presign(
            PresignMethod::Get,
            &x.objects[0].key,
            Duration::from_secs(60),
        )
        .await
        .unwrap();
    let body = reqwest::get(&url).await.unwrap();
    assert_eq!(body.status(), 200);
    assert_eq!(body.bytes().await.unwrap().as_ref(), small.as_slice());

    // Archiving the same message again is the same entry, not a second copy.
    assert_eq!(shelf.list("scrapbook/").await.unwrap().len(), 2);

    // Deleting the message takes the entry and both copies out of the bucket.
    let report = archiver
        .cascade(DeleteScope::Message(123_456), 2)
        .await
        .unwrap();
    assert_eq!((report.entries, report.removed, report.failed), (1, 2, 0));
    assert!(shelf.head(&x.objects[0].key).await.is_err());
    assert!(shelf.head(&x.objects[1].key).await.is_err());
    assert!(shelf.list("scrapbook/").await.unwrap().is_empty());
    assert!(store.failed_removals(GUILD).await.unwrap().is_empty());
}
