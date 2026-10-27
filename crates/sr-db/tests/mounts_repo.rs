//! Postgres tests for `storage_mounts` (migration 1400) through `PgMountRepo` (BYOS-MNT-001).
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test mounts_repo -- --ignored --test-threads=1`

use sqlx::Row;
use sr_db::mounts_repo::PgMountRepo;
use sr_db::sealed::SecretBox;
use sr_db::testing::TestDb;
use sr_storage::mounts::{
    MountError, MountInput, MountOwner, MountPatch, MountRepo, MountResolver, MountStatus,
    NewMount, Provider, SealedSecret, Secret,
};

const SECRET: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";

fn input(owner: MountOwner, label: &str) -> MountInput {
    MountInput {
        owner,
        label: label.into(),
        provider: Provider::Garage,
        endpoint: "http://127.0.0.1:3900".into(),
        region: "garage".into(),
        bucket: "shelf".into(),
        prefix: String::new(),
        path_style: true,
        access_key_id: "GK0123456789abcd".into(),
    }
}

fn new_mount(sb: &SecretBox, id: i64, owner: MountOwner, label: &str) -> NewMount {
    NewMount::seal(sb, id, input(owner, label), &Secret::new(SECRET)).unwrap()
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn create_seal_round_trip_caps_labels_and_soft_delete() {
    let db = TestDb::new().await.expect("db");
    let repo = PgMountRepo::new(db.pool().clone());
    let sb = SecretBox::new([9; 32]);
    let me = MountOwner::User(1001);

    let m = repo
        .create(new_mount(&sb, 5001, me, "Home"), Some(2))
        .await
        .unwrap();
    assert_eq!(
        (m.status, m.key_hint.as_str()),
        (MountStatus::Active, "abcd")
    );
    assert!(m.cors_ok_at.is_none() && m.last_test.is_none());

    // The column holds ciphertext only, bound to the row id.
    let raw: Vec<u8> = sqlx::query("SELECT secret_key_enc FROM storage_mounts WHERE id = 5001")
        .fetch_one(db.pool())
        .await
        .unwrap()
        .get(0);
    assert!(!raw.windows(8).any(|w| w == &SECRET.as_bytes()[..8]));
    assert_eq!(m.secret_key_enc.open(&sb, 5001).unwrap().expose(), SECRET);
    assert!(m.secret_key_enc.open(&sb, 5002).is_err());

    // Label unique per owner, case-insensitive; id unique; cap enforced.
    assert!(matches!(
        repo.create(new_mount(&sb, 5002, me, "home"), Some(2)).await,
        Err(MountError::LabelTaken)
    ));
    assert!(matches!(
        repo.create(new_mount(&sb, 5001, me, "Other"), Some(2))
            .await,
        Err(MountError::IdTaken)
    ));
    repo.create(new_mount(&sb, 5003, me, "Work"), Some(2))
        .await
        .unwrap();
    assert!(matches!(
        repo.create(new_mount(&sb, 5004, me, "Third"), Some(2))
            .await,
        Err(MountError::CapReached(2))
    ));
    // Another owner has its own cap and label space.
    repo.create(new_mount(&sb, 5005, MountOwner::Guild(7), "Home"), Some(5))
        .await
        .unwrap();
    // The platform mount takes id 1, no cap.
    repo.create(new_mount(&sb, 1, MountOwner::Platform, "Instance"), None)
        .await
        .unwrap();

    assert_eq!(repo.for_owner(me).await.unwrap().len(), 2);
    assert_eq!(repo.for_owner(MountOwner::Platform).await.unwrap().len(), 1);

    // Update: status and a credential pair; a clash on label is typed.
    let new_secret = SealedSecret::seal(&sb, 5001, &Secret::new("another")).unwrap();
    let patched = repo
        .update(
            5001,
            MountPatch {
                status: Some(MountStatus::Failing),
                credentials: Some(("AKIAXXXXXXXXWXYZ".into(), new_secret, "WXYZ".into())),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(patched.status, MountStatus::Failing);
    assert_eq!(
        (patched.access_key_id.as_str(), patched.key_hint.as_str()),
        ("AKIAXXXXXXXXWXYZ", "WXYZ")
    );
    assert_eq!(
        patched.secret_key_enc.open(&sb, 5001).unwrap().expose(),
        "another"
    );
    let clash = MountPatch {
        label: Some("WORK".into()),
        ..Default::default()
    };
    assert!(matches!(
        repo.update(5001, clash).await,
        Err(MountError::LabelTaken)
    ));

    // Soft delete frees the slot and the label; the row stays.
    assert!(repo.soft_delete(5001).await.unwrap());
    assert!(!repo.soft_delete(5001).await.unwrap());
    assert!(repo.by_id(5001).await.unwrap().is_none());
    assert!(
        repo.update(5001, MountPatch::default())
            .await
            .unwrap()
            .is_none()
    );
    let kept: i64 = sqlx::query_scalar("SELECT count(*) FROM storage_mounts WHERE id = 5001")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(kept, 1);
    repo.create(new_mount(&sb, 5006, me, "Home"), Some(2))
        .await
        .unwrap();

    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn concurrent_creates_cannot_pass_the_cap() {
    let db = TestDb::new().await.expect("db");
    let repo = PgMountRepo::new(db.pool().clone());
    let sb = SecretBox::new([9; 32]);
    let me = MountOwner::User(2002);
    let mut tasks = Vec::new();
    for n in 0..6_i64 {
        let repo = repo.clone();
        let nm = new_mount(&sb, 7000 + n, me, &format!("m{n}"));
        tasks.push(tokio::spawn(async move { repo.create(nm, Some(3)).await }));
    }
    let mut ok = 0;
    for t in tasks {
        if t.await.unwrap().is_ok() {
            ok += 1;
        }
    }
    assert_eq!(ok, 3);
    assert_eq!(repo.for_owner(me).await.unwrap().len(), 3);
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn test_results_cors_flag_listing_and_rotation() {
    let db = TestDb::new().await.expect("db");
    let repo = PgMountRepo::new(db.pool().clone());
    let old = SecretBox::new([3; 32]);
    let me = MountOwner::User(3003);
    repo.create(new_mount(&old, 8001, me, "A"), None)
        .await
        .unwrap();
    repo.create(new_mount(&old, 8002, me, "B"), None)
        .await
        .unwrap();

    assert!(
        repo.record_test(
            8001,
            1_700_000_000_000,
            r#"{"rows":[]}"#.into(),
            Some(MountStatus::Failing)
        )
        .await
        .unwrap()
    );
    let m = repo.by_id(8001).await.unwrap().unwrap();
    assert_eq!(m.status, MountStatus::Failing);
    assert_eq!(m.last_test_at, Some(1_700_000_000_000));
    assert!(m.last_test.unwrap().contains("rows"));
    assert!(!repo.record_test(999, 1, "{}".into(), None).await.unwrap());

    assert!(
        repo.set_cors_ok(8002, Some(1_700_000_001_000))
            .await
            .unwrap()
    );
    assert_eq!(
        repo.by_id(8002).await.unwrap().unwrap().cors_ok_at,
        Some(1_700_000_001_000)
    );
    assert!(repo.set_cors_ok(8002, None).await.unwrap());
    assert!(
        repo.by_id(8002)
            .await
            .unwrap()
            .unwrap()
            .cors_ok_at
            .is_none()
    );

    repo.update(
        8002,
        MountPatch {
            status: Some(MountStatus::Disabled),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(repo.list_enabled().await.unwrap().len(), 1);
    assert_eq!(repo.list_all().await.unwrap().len(), 2);

    // Rotation: re-seal every row under a new key; the old key alone no longer opens them.
    let rotated = SecretBox::new([4; 32]).with_previous_keys(vec![[3; 32]]);
    for m in repo.list_all().await.unwrap() {
        let sealed = m.secret_key_enc.reseal(&rotated, m.id).unwrap();
        assert!(repo.set_sealed(m.id, sealed).await.unwrap());
    }
    let only_new = SecretBox::new([4; 32]);
    for m in repo.list_all().await.unwrap() {
        assert_eq!(
            m.secret_key_enc.open(&only_new, m.id).unwrap().expose(),
            SECRET
        );
        assert!(m.secret_key_enc.open(&old, m.id).is_err());
    }
    db.cleanup().await.unwrap();
}
