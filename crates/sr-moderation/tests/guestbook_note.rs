//! Postgres tests of reports about guestbook notes (PRF-PRO-001, D-506, D-507): who may report one, the snapshot
//! (never the visitor's address hash), the routing to platform staff, and the logged staff removal. Run with a
//! scratch Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-moderation --test guestbook_note -- --ignored --test-threads=1`

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
    ModError, Moderation,
    model::{NewReport, ReportCategory, Routing, TargetKind, Who},
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

/// The owner's page and guestbook for friends.
async fn friends_guestbook(pool: &PgPool, owner: i64) {
    sqlx::query(
        "INSERT INTO profile_privacy (user_id, preset, page_tier, links_tier, top8_tier, now_playing_tier, bio_tier, \
           guestbook_tier, media_tier, ring_tier) \
         VALUES ($1, 'friends', 'friends', 'friends', 'friends', 'friends', 'friends', 'friends', 'friends', 'friends')",
    )
    .bind(owner)
    .execute(pool)
    .await
    .unwrap();
}

async fn note(pool: &PgPool, profile: i64, author: Option<i64>, status: &str, body: &str) -> i64 {
    let id = uid();
    sqlx::query(
        "INSERT INTO profile_guestbook (id, profile_id, author_id, body, status, anonymous, ip_hash) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(profile)
    .bind(author)
    .bind(body)
    .bind(status)
    .bind(author.is_none())
    .bind(author.is_none().then_some("SECRET-ADDRESS-HASH"))
    .execute(pool)
    .await
    .unwrap();
    id
}

fn report(id: i64) -> NewReport {
    NewReport {
        target_kind: TargetKind::GuestbookNote,
        target_id: id,
        channel_id: None,
        guild_id: None,
        category: ReportCategory::Spam,
        note: "spam on my page".into(),
        moderators_may_see_name: false,
    }
}

fn staff(user_id: i64, perms: P) -> Caller {
    Caller {
        user_id,
        permissions: perms,
        is_owner: false,
        ip: Some("127.0.0.1".into()),
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
async fn the_owner_reports_a_held_note_and_a_friend_an_approved_one() {
    let db = TestDb::new().await.expect("test db");
    let pool = db.pool().clone();
    let m = Moderation::new(pool.clone(), Arc::new(uid));
    let (owner, friend, writer) = (
        account(&pool, "gbowner").await,
        account(&pool, "gbfriend").await,
        account(&pool, "gbwriter").await,
    );
    friends_guestbook(&pool, owner).await;
    befriend(&pool, owner, friend).await;
    befriend(&pool, owner, writer).await;
    let held = note(&pool, owner, None, "pending", "buy cheap pills").await;
    let approved = note(&pool, owner, Some(writer), "approved", "you are awful").await;

    // The owner reports the held note: routed to platform staff, no subject account (a visitor).
    m.file_report(owner, &report(held)).await.unwrap();
    let (kind, routing, subject): (String, String, Option<i64>) = sqlx::query_as(
        "SELECT target_kind, routing, subject_account_id FROM moderation_cases WHERE target_id = $1",
    )
    .bind(held)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(kind, "guestbook_note");
    assert_eq!(Routing::parse(&routing), Some(Routing::Platform));
    assert_eq!(subject, None);

    // A friend the guestbook admits reports the approved note; its author is the subject.
    m.file_report(friend, &report(approved)).await.unwrap();
    let subject: Option<i64> =
        sqlx::query_scalar("SELECT subject_account_id FROM moderation_cases WHERE target_id = $1")
            .bind(approved)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(subject, Some(writer));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_stranger_gets_the_missing_note_404_and_the_snapshot_has_no_address_hash() {
    let db = TestDb::new().await.expect("test db");
    let pool = db.pool().clone();
    let m = Moderation::new(pool.clone(), Arc::new(uid));
    let (owner, friend, writer, stranger, staff_id) = (
        account(&pool, "gb2owner").await,
        account(&pool, "gb2friend").await,
        account(&pool, "gb2writer").await,
        account(&pool, "gb2stranger").await,
        account(&pool, "gb2staff").await,
    );
    friends_guestbook(&pool, owner).await;
    befriend(&pool, owner, friend).await;
    befriend(&pool, owner, writer).await;
    let held = note(&pool, owner, None, "pending", "visitor spam").await;
    let approved = note(&pool, owner, Some(writer), "approved", "a signed note").await;

    // A stranger (not admitted by the friends-only guestbook) and anyone on a held note: the missing-note 404.
    let missing = m.file_report(stranger, &report(uid())).await.err();
    assert_eq!(missing, Some(ModError::NotFound("note")));
    assert_eq!(
        m.file_report(stranger, &report(approved)).await.err(),
        missing
    );
    assert_eq!(m.file_report(friend, &report(held)).await.err(), missing);
    // The author may not report their own note.
    assert!(matches!(
        m.file_report(writer, &report(approved)).await.err(),
        Some(ModError::Invalid(_))
    ));
    // Blocking cuts a friend off too.
    sqlx::query("INSERT INTO relationships (user_id, target_id, type) VALUES ($1, $2, 2)")
        .bind(owner)
        .bind(stranger)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        m.file_report(stranger, &report(approved)).await.err(),
        missing
    );

    // The owner reports the held note: the snapshot is the text and the facts, never the address hash.
    m.file_report(owner, &report(held)).await.unwrap();
    let case: i64 = sqlx::query_scalar("SELECT id FROM moderation_cases WHERE target_id = $1")
        .bind(held)
        .fetch_one(&pool)
        .await
        .unwrap();
    let s = staff(staff_id, P::MANAGE_CASES | P::STAFF_BASIC);
    let who = Who::Staff(&s);
    let d = m.get_case(&who, case).await.unwrap();
    assert_eq!(d.evidence.len(), 1);
    assert_eq!(d.evidence[0].kind, "target_summary");
    let e = &d.evidence[0].payload;
    assert_eq!(e["kind"], "guestbook_note");
    assert_eq!(e["body"], "visitor spam");
    assert_eq!(e["status"], "pending");
    assert_eq!(e["anonymous"], true);
    assert_eq!(e["profile_id"], owner.to_string());
    assert!(e["author_id"].is_null());
    let all = e.to_string();
    assert!(
        !all.contains("SECRET-ADDRESS-HASH") && !all.contains("ip_hash"),
        "{all}"
    );
    assert!(d.holds.is_empty(), "a note report places no hold");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn staff_removal_is_logged_before_the_delete_and_a_second_one_is_404() {
    let db = TestDb::new().await.expect("test db");
    let pool = db.pool().clone();
    let m = Moderation::new(pool.clone(), Arc::new(uid));
    let (owner, staff_id) = (
        account(&pool, "gb3owner").await,
        account(&pool, "gb3staff").await,
    );
    let held = note(&pool, owner, None, "pending", "remove me").await;

    let weak = staff(uid(), P::STAFF_BASIC);
    assert_eq!(
        m.remove_guestbook_note(&Who::Staff(&weak), held, "spam")
            .await
            .err(),
        Some(ModError::MissingPermission("MANAGE_CASES"))
    );
    let s = staff(staff_id, P::MANAGE_CASES | P::STAFF_BASIC);
    let who = Who::Staff(&s);
    let before = log_count(&pool, "moderation.guestbook_note.delete").await;
    // D-528: no reason, no removal and no log row; the note is still there.
    for blank in ["", "   "] {
        assert_eq!(
            m.remove_guestbook_note(&who, held, blank).await.err(),
            Some(ModError::ReasonRequired)
        );
    }
    assert_eq!(
        log_count(&pool, "moderation.guestbook_note.delete").await,
        before
    );
    let still: i64 = sqlx::query_scalar("SELECT count(*) FROM profile_guestbook WHERE id = $1")
        .bind(held)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(still, 1);
    m.remove_guestbook_note(&who, held, "spam").await.unwrap();
    assert_eq!(
        log_count(&pool, "moderation.guestbook_note.delete").await,
        before + 1
    );
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM profile_guestbook WHERE id = $1")
        .bind(held)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
    assert_eq!(
        m.remove_guestbook_note(&who, held, "spam").await.err(),
        Some(ModError::NotFound("note"))
    );
    assert_eq!(
        log_count(&pool, "moderation.guestbook_note.delete").await,
        before + 1,
        "nothing is logged for a note that is already gone"
    );
}
