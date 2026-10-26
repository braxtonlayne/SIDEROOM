//! RDY-MOD-001, the half of the phase 7 gate a browser cannot run: doc 09's windows for a data-subject request.
//! The Playwright spec (`e2e/tests/moderation-gate.spec.ts`) exports and erases a member's data through the admin
//! routes in one sitting; the dates cannot be waited for there, so this file moves the clock instead.
//!
//!  * the due date is the arrival plus `compliance.data_request_deadline_days`, a request is overdue only after its
//!    effective due date (the boundary is exact), and the one extension of at most 60 days moves it and is marked
//!    late when it was granted after the original date;
//!  * the staff dashboard counts a request as overdue from the same dates (the SQL side has no injectable clock,
//!    so the rows are shifted back by the span the service clock moved);
//!  * an erasure run after the deadline still completes, the held message stays and its hold is flagged, the
//!    deleted messages get the 30-day purge date of the day they were hidden, and releasing the hold afterwards
//!    keeps that date instead of starting a new window.
//!
//! Run with a scratch Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-moderation --test deadline_windows -- --ignored --test-threads=1`

use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use sqlx::PgPool;
use sr_admin::{
    AdminError, AdminService, Caller, DataRequestKind, DataRequestState, ErasureOptions,
    data_requests::{DAY_MS, MAX_EXTENSION_DAYS},
};
use sr_core::{Clock, PlatformPermissions as P};
use sr_db::{
    accounts::{self, NewAccount},
    admin_repo::PgAdminRepo,
    self_delete,
    testing::TestDb,
};
use sr_moderation::{
    Moderation,
    model::{NewReport, ReportCategory, TargetKind, Who},
};

static SEQ: AtomicI64 = AtomicI64::new(0);

fn uid() -> i64 {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + SEQ.fetch_add(1, Ordering::Relaxed) % 1000
}

fn real_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// A clock the test moves by hand.
struct ManualClock(AtomicI64);

impl Clock for ManualClock {
    fn now_unix_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
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

fn staff(user_id: i64) -> Caller {
    Caller {
        user_id,
        permissions: P::MANAGE_TAKEDOWNS | P::MANAGE_CASES | P::MANAGE_ACCOUNTS,
        is_owner: false,
        ip: Some("127.0.0.1".into()),
    }
}

struct World {
    db: TestDb,
    svc: AdminService,
    clock: Arc<ManualClock>,
    staff: Caller,
    start: i64,
}

impl World {
    fn pool(&self) -> &PgPool {
        self.db.pool()
    }
    fn at(&self, ms: i64) {
        self.clock.0.store(ms, Ordering::SeqCst);
    }
}

async fn world() -> World {
    let db = TestDb::new().await.expect("test db");
    let pool = db.pool().clone();
    let staff_id = account(&pool, "staff1").await;
    let repo = Arc::new(PgAdminRepo::new(pool, Arc::new(uid)));
    let start = real_now_ms();
    let clock = Arc::new(ManualClock(AtomicI64::new(start)));
    let svc = AdminService::new(repo.clone())
        .with_extras(repo.clone())
        .with_data_requests(repo)
        .with_clock(clock.clone());
    World {
        db,
        svc,
        clock,
        staff: staff(staff_id),
        start,
    }
}

async fn overdue_in_dashboard(w: &World) -> i64 {
    let m = Moderation::new(w.pool().clone(), Arc::new(uid));
    m.dashboard(&w.staff, Some(30))
        .await
        .unwrap()
        .data_requests
        .unwrap()
        .overdue
}

/// Moves a request's dates back by `days`, the way the service clock moved forward.
async fn age_request(pool: &PgPool, id: i64, days: i64) {
    sqlx::query(
        "UPDATE data_requests SET received_at = received_at - make_interval(days => $2::int), \
         due_at = due_at - make_interval(days => $2::int), \
         extended_until = extended_until - make_interval(days => $2::int), \
         extended_at = extended_at - make_interval(days => $2::int) WHERE id = $1",
    )
    .bind(id)
    .bind(days as i32)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_due_date_and_the_overdue_flag_follow_the_clock_to_the_millisecond() {
    let w = world().await;
    let subject = account(w.pool(), "subject").await;
    // Arrived five days ago, thirty days to answer.
    let q = w
        .svc
        .create_data_request(
            &w.staff,
            DataRequestKind::Erasure,
            subject,
            Some(w.start - 5 * DAY_MS),
            Some("by email"),
            30,
        )
        .await
        .unwrap();
    assert_eq!(
        q.due_at_ms,
        w.start + 25 * DAY_MS,
        "arrival plus thirty days"
    );
    assert_eq!(q.effective_due_ms, q.due_at_ms);
    assert!(!q.overdue);
    assert_eq!(q.state, DataRequestState::Open);

    let overdue_at = |ms: i64| {
        w.at(ms);
        let svc = &w.svc;
        let staff = &w.staff;
        async move {
            svc.list_data_requests(staff, true, None)
                .await
                .unwrap()
                .into_iter()
                .find(|r| r.id == q.id)
                .expect("the request is in the queue")
                .overdue
        }
    };
    // Two days before, on the due date itself and one millisecond after it.
    assert!(!overdue_at(q.due_at_ms - 2 * DAY_MS).await);
    assert!(
        !overdue_at(q.due_at_ms).await,
        "the due date is still in time"
    );
    assert!(
        overdue_at(q.due_at_ms + 1).await,
        "a millisecond later it is late"
    );
    assert!(overdue_at(q.due_at_ms + 40 * DAY_MS).await);

    // The dashboard counts from the same dates: not overdue today, overdue once the rows have aged by the 26 days.
    assert_eq!(overdue_in_dashboard(&w).await, 0);
    age_request(w.pool(), q.id, 26).await;
    assert_eq!(overdue_in_dashboard(&w).await, 1);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_one_extension_moves_the_deadline_and_a_late_one_is_marked() {
    let w = world().await;
    let early = account(w.pool(), "early").await;
    let late = account(w.pool(), "late").await;
    let reason = Some("complex: three servers and a thread archive");

    // An extension before the date: due plus the days asked for; nothing late about it.
    let a = w
        .svc
        .create_data_request(
            &w.staff,
            DataRequestKind::Export,
            early,
            Some(w.start - 20 * DAY_MS),
            None,
            30,
        )
        .await
        .unwrap();
    for bad in [0, MAX_EXTENSION_DAYS + 1] {
        assert!(
            matches!(
                w.svc.extend_data_request(&w.staff, a.id, bad, reason).await,
                Err(AdminError::Invalid(_))
            ),
            "{bad} days is out of range"
        );
    }
    assert!(matches!(
        w.svc.extend_data_request(&w.staff, a.id, 30, None).await,
        Err(AdminError::ReasonRequired)
    ));
    let a = w
        .svc
        .extend_data_request(&w.staff, a.id, MAX_EXTENSION_DAYS, reason)
        .await
        .unwrap();
    assert_eq!(
        a.extended_until_ms,
        Some(a.due_at_ms + MAX_EXTENSION_DAYS * DAY_MS)
    );
    assert_eq!(a.effective_due_ms, a.due_at_ms + 60 * DAY_MS);
    assert!(!a.extended_late);
    assert!(
        matches!(
            w.svc.extend_data_request(&w.staff, a.id, 5, reason).await,
            Err(AdminError::Conflict(_))
        ),
        "an extension is allowed once"
    );
    // The original date passes without the request being overdue; the extended one makes it so.
    w.at(a.due_at_ms + DAY_MS);
    let listed = w
        .svc
        .list_data_requests(&w.staff, true, None)
        .await
        .unwrap();
    assert!(!listed.iter().find(|r| r.id == a.id).unwrap().overdue);
    w.at(a.effective_due_ms + 1);
    let listed = w
        .svc
        .list_data_requests(&w.staff, true, None)
        .await
        .unwrap();
    assert!(listed.iter().find(|r| r.id == a.id).unwrap().overdue);

    // An extension granted after the original date is marked late and does not make the record clean.
    w.at(w.start);
    let b = w
        .svc
        .create_data_request(
            &w.staff,
            DataRequestKind::Export,
            late,
            Some(w.start - 40 * DAY_MS),
            None,
            30,
        )
        .await
        .unwrap();
    assert!(b.overdue, "forty days ago plus thirty is ten days ago");
    let b = w
        .svc
        .extend_data_request(&w.staff, b.id, 30, reason)
        .await
        .unwrap();
    assert!(b.extended_late);
    assert!(!b.overdue, "the new date is twenty days away");
    assert_eq!(b.effective_due_ms, w.start + 20 * DAY_MS);
    let notices: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM staff_access_log WHERE action = 'data_request.extend_notice'",
    )
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(notices, 2, "each extension leaves its notice row");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_erasure_after_the_deadline_completes_and_the_purge_window_starts_the_day_messages_were_hidden()
 {
    let w = world().await;
    let pool = w.pool().clone();
    let reporter = account(&pool, "reporter").await;
    let subject = account(&pool, "subject").await;
    let (g, c) = (uid(), uid());
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Club', $2)")
        .bind(g)
        .bind(reporter)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO channels (id, guild_id, type, name) VALUES ($1, $2, 0, 'general')")
        .bind(c)
        .bind(g)
        .execute(&pool)
        .await
        .unwrap();
    for member in [reporter, subject] {
        sqlx::query(
            "INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(g)
        .bind(member)
        .execute(&pool)
        .await
        .unwrap();
    }
    let _: String = sqlx::query_scalar(
        "SELECT sr_ensure_message_partition((now() - interval '10 minutes')::date)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let base: i64 =
        sqlx::query_scalar("SELECT sr_snowflake_floor(now() - interval '10 minutes') + 1000")
            .fetch_one(&pool)
            .await
            .unwrap();
    let (held, plain) = (base + 1, base + 2);
    for id in [held, plain] {
        sqlx::query(
            "INSERT INTO messages (id, channel_id, author_id, content) VALUES ($1, $2, $3, 'line')",
        )
        .bind(id)
        .bind(c)
        .bind(subject)
        .execute(&pool)
        .await
        .unwrap();
    }

    // A report on the first message opens a case; the case holds it as evidence.
    let m = Moderation::new(pool.clone(), Arc::new(uid));
    let rep = m
        .file_report(
            reporter,
            &NewReport {
                target_kind: TargetKind::Message,
                target_id: held,
                channel_id: Some(c),
                guild_id: Some(g),
                category: ReportCategory::Harassment,
                note: "this is bad".into(),
                moderators_may_see_name: false,
            },
        )
        .await
        .unwrap();
    let case: i64 = sqlx::query_scalar("SELECT case_id FROM moderation_reports WHERE id = $1")
        .bind(rep.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(case > 0, "the report opened a case");
    // Filing the report froze the reported message as evidence: the case already holds it.
    let hold: i64 = sqlx::query_scalar("SELECT hold_id FROM messages WHERE id = $1")
        .bind(held)
        .fetch_one(&pool)
        .await
        .unwrap();

    // The request arrived 40 days ago, so it is already 10 days late when staff run it.
    let q = w
        .svc
        .create_data_request(
            &w.staff,
            DataRequestKind::Erasure,
            subject,
            Some(w.start - 40 * DAY_MS),
            Some("by email"),
            30,
        )
        .await
        .unwrap();
    assert!(q.overdue);
    w.svc
        .verify_data_request(&w.staff, q.id, Some("replied from the account address"))
        .await
        .unwrap();
    let run = w
        .svc
        .run_data_erasure(
            &w.staff,
            q.id,
            ErasureOptions {
                delete_messages: false,
                invoice_years: 10,
            },
        )
        .await
        .unwrap();
    assert_eq!(run.request.state, DataRequestState::Completed);
    assert!(!run.request.overdue, "a completed request is not overdue");
    assert_eq!(run.report.held_rows, 1);
    assert_eq!(
        run.report.held_holds, 1,
        "the hold is flagged, not released"
    );
    let state: String = sqlx::query_scalar("SELECT state::text FROM accounts WHERE id = $1")
        .bind(subject)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state, "deleted");
    assert_eq!(
        overdue_in_dashboard(&w).await,
        0,
        "a completed request leaves the overdue count"
    );

    // The queued delete job hides both messages; the purge date is thirty days from this moment, none for the held one.
    let hidden_at = w.start + 41 * DAY_MS;
    let out = self_delete::hide(
        &pool,
        subject,
        self_delete::Scope {
            servers: true,
            dms: true,
            before_ms: None,
        },
        hidden_at,
    )
    .await
    .unwrap();
    assert_eq!(
        out.hidden
            .iter()
            .map(|h| h.message_ids.len())
            .sum::<usize>(),
        2,
        "both messages are hidden at once; only the held one gets no purge date"
    );
    let purge_in_days = |id: i64| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, Option<i64>>(
                "SELECT (extract(epoch FROM (purge_after - deleted_at)) / 86400)::bigint FROM messages WHERE id = $1",
            )
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert_eq!(
        purge_in_days(plain).await,
        Some(30),
        "doc 09: purged 30 days after the deletion"
    );
    assert_eq!(
        purge_in_days(held).await,
        None,
        "a held message never gets a purge date"
    );

    // Releasing the hold later starts no new window: the date is the deletion day plus thirty.
    let who_caller = w.staff.clone();
    let who = Who::Staff(&who_caller);
    m.release_hold(&who, hold, Some("case closed"))
        .await
        .unwrap();
    assert_eq!(purge_in_days(held).await, Some(30));
    let same: bool = sqlx::query_scalar(
        "SELECT (SELECT purge_after FROM messages WHERE id = $1) = (SELECT purge_after FROM messages WHERE id = $2)",
    )
    .bind(held)
    .bind(plain)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(same, "both messages are purged on the same day");
}
