//! Postgres tests for the browser upload ledger (BYOS-BRW-001, migration 1440).
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test mount_uploads_repo -- --ignored --test-threads=1`

use sr_db::mount_uploads_repo::PgUploadLedger;
use sr_db::testing::TestDb;
use sr_storage::uploads::{MountUpload, PART_SIZE, UploadLedger, UploadState};

async fn account(pool: &sqlx::PgPool, id: i64) {
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(id + 100_000)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $2, $3, $4, 'x')",
    )
    .bind(id)
    .bind(id + 100_000)
    .bind(format!("u{id}"))
    .bind(format!("u{id}@example.test"))
    .execute(pool)
    .await
    .unwrap();
}

async fn mount_row(pool: &sqlx::PgPool, id: i64, owner: i64) {
    sqlx::query(
        "INSERT INTO storage_mounts (id, owner_kind, owner_id, label, provider, endpoint, bucket, access_key_id, secret_key_enc) \
         VALUES ($1, 'user', $2, 'm', 'garage', 'http://127.0.0.1:3900', 'b', 'AK', $3)",
    )
    .bind(id)
    .bind(owner)
    .bind(vec![0u8])
    .execute(pool)
    .await
    .unwrap();
}

fn upload(id: i64, mount: i64, user: i64, at: i64) -> MountUpload {
    MountUpload {
        id,
        mount_id: mount,
        user_id: user,
        key: format!("Music/{id}.flac"),
        s3_upload_id: format!("s3-{id}"),
        size: 40 * 1024 * 1024,
        part_size: PART_SIZE,
        part_count: 3,
        content_type: "audio/flac".into(),
        state: UploadState::Open,
        created_at: at,
        updated_at: at,
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn ledger_lifecycle_sweep_selection_and_hourly_part_notes() {
    let db = TestDb::new().await.expect("db");
    let pool = db.pool().clone();
    let l = PgUploadLedger::new(pool.clone());
    account(&pool, 11).await;
    account(&pool, 12).await;
    mount_row(&pool, 900, 11).await;
    mount_row(&pool, 901, 12).await;

    let t0 = 1_800_000_000_000;
    l.insert(upload(1, 900, 11, t0)).await.unwrap();
    l.insert(upload(2, 900, 11, t0 + 1_000)).await.unwrap();
    l.insert(upload(3, 901, 12, t0 + 2_000)).await.unwrap();

    let got = l.get(900, 1).await.unwrap().unwrap();
    assert_eq!(got.key, "Music/1.flac");
    assert_eq!(got.s3_upload_id, "s3-1");
    assert_eq!(got.part_count, 3);
    assert_eq!(got.state, UploadState::Open);
    assert_eq!(got.created_at, t0);
    // Another mount's id is not found under this mount.
    assert!(l.get(901, 1).await.unwrap().is_none());

    assert_eq!(l.count_open(11).await.unwrap(), 2);
    let open = l.list_open(900, 11).await.unwrap();
    assert_eq!(open.iter().map(|u| u.id).collect::<Vec<_>>(), vec![2, 1]);
    assert!(l.list_open(900, 12).await.unwrap().is_empty());

    // Touching an upload by issuing parts keeps it out of the sweep.
    let day = 24 * 3600 * 1000;
    l.note_parts(11, 1, 2, t0 + day).await.unwrap();
    let stale = l.open_before(t0 + day / 2 + 1_500, 10).await.unwrap();
    assert_eq!(stale.iter().map(|u| u.id).collect::<Vec<_>>(), vec![2, 3]);
    assert_eq!(l.parts_since(11, t0).await.unwrap(), 2);
    assert_eq!(l.parts_since(11, t0 + day + 1).await.unwrap(), 0);
    assert_eq!(l.parts_since(12, t0).await.unwrap(), 0);
    assert_eq!(l.purge_part_notes(t0 + day + 1).await.unwrap(), 1);

    // finish is once-only and scoped to the mount.
    assert!(!l.finish(901, 2, UploadState::Aborted, t0).await.unwrap());
    assert!(
        l.finish(900, 2, UploadState::Aborted, t0 + day)
            .await
            .unwrap()
    );
    assert!(
        !l.finish(900, 2, UploadState::Completed, t0 + day)
            .await
            .unwrap()
    );
    assert_eq!(l.count_open(11).await.unwrap(), 1);
    assert_eq!(
        l.get(900, 2).await.unwrap().unwrap().state,
        UploadState::Aborted
    );
}
