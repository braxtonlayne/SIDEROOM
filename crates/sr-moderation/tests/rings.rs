//! Postgres tests of reports about status rings (PRF-RNG-001): who can report one, the evidence copy, the hold,
//! the logged staff read, and that the case can still act after the ring is gone. Run with a scratch Postgres that
//! may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-moderation --test rings -- --ignored --test-threads=1`

use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use sqlx::PgPool;
use sr_admin::Caller;
use sr_core::PlatformPermissions as P;
use sr_db::{
    accounts::{self, NewAccount},
    testing::TestDb,
};
use sr_moderation::{
    ModError, Moderation, Transition,
    model::{CaseState, NewReport, ReportCategory, Routing, TargetKind, Who},
};

static SEQ: AtomicI64 = AtomicI64::new(0);

fn uid() -> i64 {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + SEQ.fetch_add(1, Ordering::Relaxed) % 1000
}

async fn account(pool: &PgPool, name: &str) -> i64 {
    let id = uid();
    let ok = accounts::create_account(
        pool,
        &NewAccount {
            id,
            person_id: uid(),
            username: name,
            display_name: None,
            email: &format!("{name}@example.test"),
            password_hash: "x",
            state: "active",
            email_verified: true,
            initial_credits: 0,
            flags: 0,
        },
        uid(),
    )
    .await
    .unwrap();
    assert!(ok);
    id
}

async fn befriend(pool: &PgPool, a: i64, b: i64) {
    for (x, y) in [(a, b), (b, a)] {
        sqlx::query("INSERT INTO relationships (user_id, target_id, type) VALUES ($1, $2, 1)")
            .bind(x)
            .bind(y)
            .execute(pool)
            .await
            .unwrap();
    }
}

async fn ring(pool: &PgPool, author: i64, kind: &str, body: &str, with_object: bool) -> i64 {
    let id = uid();
    sqlx::query(
        "INSERT INTO storage_mounts (id, owner_kind, owner_id, label, provider, endpoint, bucket, access_key_id, secret_key_enc) \
         VALUES (1, 'platform', 0, 'Instance', 's3', 'https://s3.example.test', 'b', 'k', '\\x00') ON CONFLICT DO NOTHING",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO status_rings (id, author_id, kind, body, mount_id, object_key, content_type, bytes, audience, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'friends', now() + interval '24 hours')",
    )
    .bind(id)
    .bind(author)
    .bind(kind)
    .bind(body)
    .bind(with_object.then_some(1_i64))
    .bind(with_object.then(|| format!("rings/{id}")))
    .bind(with_object.then_some("image/jpeg"))
    .bind(with_object.then_some(100_i64))
    .execute(pool)
    .await
    .unwrap();
    id
}

fn staff(user_id: i64, perms: P) -> Caller {
    Caller {
        user_id,
        permissions: perms,
        is_owner: false,
        ip: Some("127.0.0.1".into()),
    }
}

fn report(id: i64) -> NewReport {
    NewReport {
        target_kind: TargetKind::StatusRing,
        target_id: id,
        channel_id: None,
        guild_id: None,
        category: ReportCategory::Harassment,
        note: "this ring is nasty".into(),
        moderators_may_see_name: false,
    }
}

async fn log_count(pool: &PgPool, action: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM staff_access_log WHERE action = $1")
        .bind(action)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_reported_ring_is_frozen_and_its_case_can_still_act_after_it_is_gone() {
    let db = TestDb::new().await.expect("test db");
    let pool = db.pool().clone();
    let m = Moderation::new(pool.clone(), Arc::new(uid));
    let (author, friend, stranger, staff_id) = (
        account(&pool, "ringauthor").await,
        account(&pool, "ringfriend").await,
        account(&pool, "ringstranger").await,
        account(&pool, "ringstaff").await,
    );
    befriend(&pool, author, friend).await;
    let photo = ring(&pool, author, "photo", "caption", true).await;
    let s = staff(staff_id, P::MANAGE_CASES | P::STAFF_BASIC);
    let who = Who::Staff(&s);

    // A ring the reporter may not see answers like one that does not exist; one's own cannot be reported.
    let hidden = m.file_report(stranger, &report(photo)).await.err();
    let absent = m.file_report(stranger, &report(42)).await.err();
    assert_eq!(hidden, Some(ModError::NotFound("ring")));
    assert_eq!(hidden, absent);
    assert!(matches!(
        m.file_report(author, &report(photo)).await.err(),
        Some(ModError::Invalid(_))
    ));

    // The friend reports it.
    m.file_report(friend, &report(photo)).await.unwrap();
    let (case, kind, routing): (i64, String, String) =
        sqlx::query_as("SELECT id, target_kind, routing FROM moderation_cases")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(kind, "status_ring");
    assert_eq!(Routing::parse(&routing), Some(Routing::Platform));

    // The ring is frozen by a system hold of its own scope.
    let hold: Option<i64> = sqlx::query_scalar("SELECT hold_id FROM status_rings WHERE id = $1")
        .bind(photo)
        .fetch_one(&pool)
        .await
        .unwrap();
    let hold = hold.expect("the report froze the ring");
    let scope: (String, i64) =
        sqlx::query_as("SELECT scope_kind, scope_id FROM moderation_holds WHERE id = $1")
            .bind(hold)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(scope, ("status_ring".to_owned(), photo));

    // Evidence: private, says what the ring said, never a viewer list.
    let before = log_count(&pool, "moderation.evidence.read").await;
    let d = m.get_case(&who, case).await.unwrap();
    assert_eq!(d.evidence.len(), 1);
    assert_eq!(d.evidence[0].kind, "target_summary");
    let e = &d.evidence[0].payload;
    assert_eq!(e["kind"], "status_ring");
    assert_eq!(e["ring_kind"], "photo");
    assert_eq!(e["text"], "caption");
    assert_eq!(e["stored_on"], "instance_bucket");
    assert!(!e.to_string().contains("viewer"));
    assert_eq!(d.holds.len(), 1);
    assert_eq!(d.holds[0].scope_kind.as_deref(), Some("status_ring"));
    assert_eq!(
        log_count(&pool, "moderation.evidence.read").await,
        before + 1,
        "private evidence is a logged read"
    );

    // The logged read of the file: staff only, a reason, and a row first.
    let weak = staff(uid(), P::STAFF_BASIC);
    assert!(
        m.read_held_ring(&Who::Staff(&weak), photo, "look")
            .await
            .is_err()
    );
    assert!(matches!(
        m.read_held_ring(&who, photo, "  ").await.err(),
        Some(ModError::ReasonRequired | ModError::Invalid(_))
    ));
    let reads = log_count(&pool, "moderation.ring.read").await;
    let held = m.read_held_ring(&who, photo, "case review").await.unwrap();
    assert_eq!(
        (held.mount_id, held.object_key.clone()),
        (1, format!("rings/{photo}"))
    );
    assert_eq!(log_count(&pool, "moderation.ring.read").await, reads + 1);
    // A ring nobody reported is not readable by staff.
    let other = ring(&pool, author, "photo", "", true).await;
    assert_eq!(
        m.read_held_ring(&who, other, "curious").await.err(),
        Some(ModError::NotFound("ring"))
    );

    // The ring passes its day: it is invisible to readers but the hold keeps the row.
    sqlx::query("UPDATE status_rings SET created_at = now() - interval '25 hours', expires_at = now() - interval '1 hour' WHERE id = $1")
        .bind(photo)
        .execute(&pool)
        .await
        .unwrap();
    assert!(m.read_held_ring(&who, photo, "after expiry").await.is_ok());
    // The reported ring cannot be reported again once hidden.
    assert_eq!(
        m.file_report(friend, &report(photo)).await.err(),
        Some(ModError::NotFound("ring"))
    );

    // The hold ends, the job deletes the ring, and the case still has its evidence and can still act.
    m.release_hold(&who, hold, Some("decided")).await.unwrap();
    sqlx::query("DELETE FROM status_rings WHERE id = $1")
        .bind(photo)
        .execute(&pool)
        .await
        .unwrap();
    let after = m.get_case(&who, case).await.unwrap();
    assert_eq!(after.evidence.len(), 1);
    assert_eq!(after.evidence[0].payload["text"], "caption");
    m.add_note(&who, case, "ring removed, author warned")
        .await
        .unwrap();
    for to in [CaseState::Actioned, CaseState::Closed] {
        m.set_state(
            &who,
            case,
            &Transition {
                to,
                outcome: Some("warned the author".into()),
                reporter_line: Some("We took action.".into()),
            },
        )
        .await
        .unwrap();
    }
}
