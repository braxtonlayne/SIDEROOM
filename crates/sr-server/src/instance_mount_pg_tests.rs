//! BYOS-INS-001 against real Postgres: the seed of mount 1, its refusal, and migration 1420's defaults
//! (run with `SR_TEST_DATABASE_URL`, `--ignored`, `--test-threads=1`).

use sqlx::Row;
use sr_core::config::{Secret, StorageConfig};
use sr_db::{mounts_repo::PgMountRepo, sealed::SecretBox, testing::TestDb};
use sr_storage::mounts::{INSTANCE_MOUNT_ID, MountOwner, MountResolver};

use crate::instance_mount::{InstanceMount, ensure_instance_mount};

fn cfg(bucket: &str) -> StorageConfig {
    StorageConfig {
        bucket: bucket.into(),
        access_key: Secret::new("GKINSTANCE0001"),
        secret_key: Secret::new("instance-secret-value"),
        ..StorageConfig::default()
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn mount_one_is_seeded_sealed_once_and_a_changed_bucket_is_refused() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let repo = PgMountRepo::new(pool.clone());
    let sb = SecretBox::new([9; 32]);

    assert_eq!(
        ensure_instance_mount(&repo, &sb, &cfg("sideroom"))
            .await
            .unwrap(),
        InstanceMount::Seeded
    );
    assert_eq!(
        ensure_instance_mount(&repo, &sb, &cfg("sideroom"))
            .await
            .unwrap(),
        InstanceMount::Present
    );
    let m = repo.by_id(INSTANCE_MOUNT_ID).await.unwrap().unwrap();
    assert_eq!(m.owner, MountOwner::Platform);
    assert_eq!(m.key_hint, "0001");
    let secret = m.secret_key_enc.open(&sb, m.id).unwrap();
    assert_eq!(secret.expose(), "instance-secret-value");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM storage_mounts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);

    let err = ensure_instance_mount(&repo, &sb, &cfg("elsewhere"))
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("set-instance-mount"), "{err}");

    // Migration 1420: rows written without naming a mount belong to mount 1 and the instance source.
    sqlx::query(
        "INSERT INTO video_assets (id, filename, original_key, original_ext, original_size) \
         VALUES (910001, 'a.mp4', 'originals/910001/source.mp4', 'mp4', 1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let row = sqlx::query("SELECT mount_id, source FROM video_assets WHERE id = 910001")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.get::<i64, _>("mount_id"), 1);
    assert_eq!(row.get::<String, _>("source"), "instance");
    for table in [
        "video_titles",
        "video_episodes",
        "reading_series",
        "reading_chapters",
    ] {
        let c: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM information_schema.columns WHERE table_name = $1 \
             AND column_name IN ('mount_id', 'source')",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(c, 2, "{table}");
    }
}
