//! Postgres tests for the legal side of mounts (migration 1460, BYOS-LEG-001): a delisted mount cannot be moved out of
//! `delisted` by the connection test or by the owner's patch, and the scope check refuses it for everyone.
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test storage_legal -- --ignored --test-threads=1`

use sr_db::mounts_repo::PgMountRepo;
use sr_db::sealed::SecretBox;
use sr_db::storage_legal;
use sr_db::testing::TestDb;
use sr_storage::mounts::{
    MountInput, MountOwner, MountPatch, MountRepo, MountResolver, MountStatus, NewMount, Provider,
    Secret,
};

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_delisted_mount_stays_delisted_until_staff_relist_it() {
    let db = TestDb::new().await.expect("db");
    let repo = PgMountRepo::new(db.pool().clone());
    let sb = SecretBox::new([9; 32]);
    let m = repo
        .create(
            NewMount::seal(
                &sb,
                6001,
                MountInput {
                    owner: MountOwner::User(1001),
                    label: "Home".into(),
                    provider: Provider::Garage,
                    endpoint: "http://127.0.0.1:3900".into(),
                    region: "garage".into(),
                    bucket: "shelf".into(),
                    prefix: String::new(),
                    path_style: true,
                    access_key_id: "GK0123456789abcd".into(),
                },
                &Secret::new("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"),
            )
            .unwrap(),
            Some(2),
        )
        .await
        .unwrap();
    assert_eq!(m.status, MountStatus::Active);

    let mut conn = db.pool().acquire().await.unwrap();
    let l = storage_legal::delist_mount(&mut conn, 6001, 42, "notice")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(l.status, "delisted");
    // The instance bucket and unknown mounts cannot be delisted.
    assert!(
        storage_legal::delist_mount(&mut conn, 6002, 42, "x")
            .await
            .unwrap()
            .is_none()
    );

    // The connection test and the owner's patch try to set 'active'; the mount stays delisted.
    repo.record_test(6001, 1, "{}".into(), Some(MountStatus::Active))
        .await
        .unwrap();
    let after = repo
        .update(
            6001,
            MountPatch {
                status: Some(MountStatus::Active),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.status, MountStatus::Delisted);
    assert_eq!(
        repo.by_id(6001).await.unwrap().unwrap().status,
        MountStatus::Delisted
    );

    // Relisting restores the status it had before.
    let r = storage_legal::relist_mount(&mut conn, 6001)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(r.status, "active");
    assert!(
        storage_legal::relist_mount(&mut conn, 6001)
            .await
            .unwrap()
            .is_none()
    );
}
