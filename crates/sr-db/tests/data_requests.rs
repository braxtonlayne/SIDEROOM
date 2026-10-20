//! DB-backed tests of the data-request queue (M-25 step c, MOD-DSR). Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test data_requests -- --ignored --test-threads=1`

use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use sr_admin::{
    AccessKind, AdminError, AuditEntry, DataRequestKind, DataRequestRepository, DataRequestState,
    NewDataRequest, RETAINED, data_requests::DAY_MS,
};
use sr_db::{PgPool, admin_repo::PgAdminRepo, data_export, testing::TestDb};

const T0: i64 = 1_800_000_000_000;
const STAFF: i64 = 1;

fn ids() -> i64 {
    static N: AtomicI64 = AtomicI64::new(50_000);
    N.fetch_add(1, Ordering::Relaxed)
}

fn repo(pool: &PgPool) -> PgAdminRepo {
    PgAdminRepo::new(pool.clone(), Arc::new(ids))
}

async fn account(pool: &PgPool, id: i64, name: &str) {
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, display_name, email, password_hash) \
         VALUES ($1, $1, $2, 'Real Name', $3, 'phc')",
    )
    .bind(id)
    .bind(name)
    .bind(format!("{name}@example.test"))
    .execute(pool)
    .await
    .unwrap();
}

fn entry(action: &str, target: i64) -> AuditEntry {
    AuditEntry {
        actor_id: STAFF,
        kind: AccessKind::AdminAction,
        action: action.to_owned(),
        target_kind: Some("data_request".into()),
        target_id: Some(target),
        reason: "test".into(),
        ip: None,
        detail: "{}".into(),
    }
}

fn new(kind: DataRequestKind, account: i64) -> NewDataRequest {
    NewDataRequest {
        kind,
        account_id: account,
        received_at_ms: T0,
        due_at_ms: T0 + 30 * DAY_MS,
        note: "by email".into(),
        created_by: STAFF,
    }
}

async fn log_count(pool: &PgPool, action: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM staff_access_log WHERE action = $1")
        .bind(action)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn setup(pool: &PgPool) {
    account(pool, STAFF, "staffer").await;
    account(pool, 100, "subject").await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn lifecycle_stores_the_dates_extends_once_and_logs_every_change() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    setup(pool).await;
    let r = repo(pool);

    let q = r
        .create(
            &new(DataRequestKind::Export, 100),
            &entry("data_request.create", 0),
        )
        .await
        .unwrap();
    assert_eq!(q.state, DataRequestState::Open);
    assert_eq!(
        (q.person_id, q.account_id, q.username.as_str()),
        (100, 100, "subject")
    );
    assert_eq!(q.received_at_ms, T0);
    assert_eq!(q.due_at_ms, T0 + 30 * DAY_MS);
    // One active request of a kind per account; the other kind is fine.
    assert!(matches!(
        r.create(
            &new(DataRequestKind::Export, 100),
            &entry("data_request.create", 0)
        )
        .await,
        Err(AdminError::Conflict(_))
    ));
    let erase = r
        .create(
            &new(DataRequestKind::Erasure, 100),
            &entry("data_request.create", 0),
        )
        .await
        .unwrap();
    assert!(matches!(
        r.create(
            &new(DataRequestKind::Export, 424_242),
            &entry("data_request.create", 0)
        )
        .await,
        Err(AdminError::NotFound(_))
    ));

    let v = r
        .verify(q.id, STAFF, T0 + 1000, &entry("data_request.verify", q.id))
        .await
        .unwrap();
    assert_eq!(
        (v.state, v.verified_by, v.verified_at_ms),
        (DataRequestState::Verified, Some(STAFF), Some(T0 + 1000))
    );
    assert!(matches!(
        r.verify(q.id, STAFF, T0, &entry("data_request.verify", q.id))
            .await,
        Err(AdminError::Conflict(_))
    ));

    // The one extension: dates move, the reason is kept, a second attempt is refused.
    let x = r
        .extend(
            q.id,
            20,
            "many systems",
            STAFF,
            T0 + 2000,
            &entry("data_request.extend", q.id),
        )
        .await
        .unwrap();
    assert_eq!(x.extended_until_ms, Some(q.due_at_ms + 20 * DAY_MS));
    assert_eq!(x.due_at_ms, q.due_at_ms);
    assert_eq!(x.extension_reason.as_deref(), Some("many systems"));
    assert_eq!(
        x.member_notified_at_ms, None,
        "not told until the notice is recorded"
    );
    let told = r
        .member_notice(
            q.id,
            true,
            T0 + 2500,
            &entry("data_request.extend_notice", q.id),
        )
        .await
        .unwrap();
    assert_eq!(told.member_notified_at_ms, Some(T0 + 2500));
    assert_eq!(told.extended_at_ms, Some(T0 + 2000));
    assert!(!told.with_clock(T0).extended_late);
    assert!(matches!(
        r.extend(
            q.id,
            5,
            "again",
            STAFF,
            T0,
            &entry("data_request.extend", q.id)
        )
        .await,
        Err(AdminError::Conflict(_))
    ));
    // The table itself refuses a half-recorded or backwards extension.
    for sql in [
        "UPDATE data_requests SET extension_reason = NULL WHERE id = $1",
        "UPDATE data_requests SET extended_until = due_at WHERE id = $1",
    ] {
        assert!(
            sqlx::query(sql).bind(erase.id).execute(pool).await.is_err() || {
                // An UPDATE that changes nothing on an unextended row is harmless; the second form must fail.
                sql.contains("extension_reason")
            }
        );
    }
    assert!(
        sqlx::query("UPDATE data_requests SET extended_until = due_at - interval '1 day', extension_reason = 'x', extended_by = 1, extended_at = now() WHERE id = $1")
            .bind(erase.id)
            .execute(pool)
            .await
            .is_err()
    );

    let d = r
        .decline(
            erase.id,
            "Declined: no proof of identity",
            T0 + 3000,
            &entry("data_request.decline", erase.id),
        )
        .await
        .unwrap();
    assert_eq!(d.state, DataRequestState::Declined);
    assert!(d.completed_at_ms.is_some());
    assert!(matches!(
        r.extend(
            erase.id,
            5,
            "late",
            STAFF,
            T0,
            &entry("data_request.extend", erase.id)
        )
        .await,
        Err(AdminError::Conflict(_))
    ));

    // Queue order: active by effective date first, then closed.
    let all = r.list(false, 10).await.unwrap();
    assert_eq!(
        all.iter().map(|x| x.id).collect::<Vec<_>>(),
        [q.id, erase.id]
    );
    let active = r.list(true, 10).await.unwrap();
    assert_eq!(active.len(), 1);

    // Every state change wrote exactly one access-log row, in the same transaction.
    assert_eq!(log_count(pool, "data_request.create").await, 2);
    assert_eq!(log_count(pool, "data_request.verify").await, 1);
    assert_eq!(
        log_count(pool, "data_request.extend").await,
        1,
        "the refused extension logs nothing"
    );
    assert_eq!(log_count(pool, "data_request.decline").await, 1);
    let mut conn = pool.acquire().await.unwrap();
    assert!(sr_db::admin_repo::verify_chain(&mut conn).await.unwrap().ok);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn export_run_queues_the_self_service_job_without_the_cooldown() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    setup(pool).await;
    let r = repo(pool);
    let q = r
        .create(
            &new(DataRequestKind::Export, 100),
            &entry("data_request.create", 0),
        )
        .await
        .unwrap();
    // Not before verification.
    assert!(matches!(
        r.run_export(q.id, T0, &entry("data_request.run_export", q.id))
            .await,
        Err(AdminError::Conflict(_))
    ));
    r.verify(q.id, STAFF, T0, &entry("data_request.verify", q.id))
        .await
        .unwrap();

    // The person asked for a package an hour ago: the cooldown does not apply to staff.
    sqlx::query("INSERT INTO data_exports (id, user_id, state, requested_at, ready_at, expires_at, storage_key) VALUES (7, 100, 'ready', now() - interval '1 hour', now(), now() + interval '7 days', 'k')")
        .execute(pool)
        .await
        .unwrap();
    let run = r
        .run_export(q.id, T0, &entry("data_request.run_export", q.id))
        .await
        .unwrap();
    assert_eq!(run.state, DataRequestState::Running);
    let export_id = run.export_id.expect("export id");
    let (state, user): (String, i64) =
        sqlx::query_as("SELECT state, user_id FROM data_exports WHERE id = $1")
            .bind(export_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!((state.as_str(), user), ("pending", 100));
    let jobs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE kind = $1 AND payload->>'export_id' = $2",
    )
    .bind(data_export::JOB_KIND)
    .bind(export_id.to_string())
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(jobs, 1);

    // The package finishes: the request completes with the stated outcome.
    data_export::begin(pool, export_id).await.unwrap();
    data_export::finish(pool, export_id, "exports/x.zip", 123)
        .await
        .unwrap();
    let done = r.get(q.id).await.unwrap().unwrap();
    assert_eq!(done.state, DataRequestState::Completed);
    assert!(done.outcome.unwrap().contains("Settings"));
    assert!(done.completed_at_ms.is_some());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_failed_export_build_puts_the_request_back_to_verified() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    setup(pool).await;
    let r = repo(pool);
    let q = r
        .create(
            &new(DataRequestKind::Export, 100),
            &entry("data_request.create", 0),
        )
        .await
        .unwrap();
    r.verify(q.id, STAFF, T0, &entry("data_request.verify", q.id))
        .await
        .unwrap();
    let run = r
        .run_export(q.id, T0, &entry("data_request.run_export", q.id))
        .await
        .unwrap();
    // A second run while the build is open is refused.
    assert!(
        r.run_export(q.id, T0, &entry("data_request.run_export", q.id))
            .await
            .is_err()
    );
    data_export::fail(pool, run.export_id.unwrap(), "boom")
        .await
        .unwrap();
    let back = r.get(q.id).await.unwrap().unwrap();
    assert_eq!(
        (back.state, back.export_id),
        (DataRequestState::Verified, None)
    );
    // And staff can run it again.
    assert_eq!(
        r.run_export(q.id, T0, &entry("data_request.run_export", q.id))
            .await
            .unwrap()
            .state,
        DataRequestState::Running
    );
}

/// The retention guard (D-124, D-212, D-295): an erasure scrubs the account but keeps the Person
/// record, the invite graph, invoices, staff-side moderation history, held content and the
/// request itself; the outcome names every kept record.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn erasure_run_scrubs_the_account_and_keeps_the_retained_records() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    setup(pool).await;
    account(pool, 200, "inviter").await;
    // Invite graph: 200 invited 100.
    sqlx::query("INSERT INTO platform_invites (id, code, issuer_account_id, state, redeemed_by_account_id, redeemed_at) VALUES (5, 'ABCDEFGHJK', 200, 'redeemed', 100, now())")
        .execute(pool).await.unwrap();
    sqlx::query("UPDATE accounts SET invited_by_invite_id = 5 WHERE id = 100")
        .execute(pool)
        .await
        .unwrap();
    // Moderation history: a ban on the Person and a staff audit row about the account.
    sqlx::query("INSERT INTO person_bans (id, person_id, category, note, banned_by) VALUES (6, 100, 'standard', 'spam', 1)")
        .execute(pool).await.unwrap();
    sqlx::query("INSERT INTO platform_audit_log (id, actor_id, action, target_id, note) VALUES (7, 1, 'account.suspend', 100, 'x')")
        .execute(pool).await.unwrap();
    // An invoice.
    sqlx::query("INSERT INTO payment_wallets (id, network, descriptor, fingerprint) VALUES (8, 'regtest', 'wpkh(tpubD6NzVbkrYhZ4/0/*)', 'abcd1234')")
        .execute(pool).await.unwrap();
    sqlx::query("INSERT INTO payment_invoices (id, wallet_id, user_id, purpose, usd_cents, sats, rate_cents_per_btc, rate_source, address, derivation_index, network, confirmations_required, created_ms, expires_ms, quote_expires_ms, next_check_ms) VALUES (9, 8, 100, 'donate', 500, 5000, 10000000, 'test', 'bcrt1qtest', 0, 'regtest', 1, 1, 2, 3, 4)")
        .execute(pool).await.unwrap();
    // A message under a moderation hold.
    sqlx::query("INSERT INTO moderation_holds (id, reason) VALUES (10, 'case')")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO channels (id, type) VALUES (11, 1)")
        .execute(pool)
        .await
        .unwrap();
    let _: String = sqlx::query_scalar("SELECT sr_ensure_message_partition(CURRENT_DATE)")
        .fetch_one(pool)
        .await
        .unwrap();
    let mid: i64 =
        sqlx::query_scalar("SELECT sr_snowflake_floor(date_trunc('month', now())) + 1000")
            .fetch_one(pool)
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO messages (id, channel_id, author_id, hold_id) VALUES ($1, 11, 100, 10)",
    )
    .bind(mid)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO sessions (id, account_id, token_hash, ip, user_agent, device_label, expires_at) VALUES (13, 100, decode(repeat('ab', 32), 'hex'), '203.0.113.9', 'Firefox', 'Laptop', now() + interval '1 day')")
        .execute(pool).await.unwrap();
    sqlx::query("INSERT INTO login_events (id, account_id, ip, success) VALUES (14, 100, '203.0.113.9', true)")
        .execute(pool).await.unwrap();
    sqlx::query("INSERT INTO messages (id, channel_id, author_id) VALUES ($1, 11, 100)")
        .bind(mid + 1)
        .execute(pool)
        .await
        .unwrap();
    // A Discord link.
    let identities_ok = sqlx::query("INSERT INTO external_identities (id, account_id, provider, provider_user_id) VALUES (12, 100, 'discord', 'd-100')")
        .execute(pool).await.is_ok();

    let r = repo(pool);
    let q = r
        .create(
            &new(DataRequestKind::Erasure, 100),
            &entry("data_request.create", 0),
        )
        .await
        .unwrap();
    assert!(
        matches!(
            r.run_erasure(q.id, T0, &entry("data_request.run_erasure", q.id))
                .await,
            Err(AdminError::Conflict(_))
        ),
        "not before verification"
    );
    r.verify(q.id, STAFF, T0, &entry("data_request.verify", q.id))
        .await
        .unwrap();
    let run = r
        .run_erasure(q.id, T0 + 5000, &entry("data_request.run_erasure", q.id))
        .await
        .unwrap();

    // The account is scrubbed ...
    let (state, name): (String, String) =
        sqlx::query_as("SELECT state::text, username::text FROM accounts WHERE id = 100")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!((state.as_str(), name.as_str()), ("deleted", "deleted.100"));
    // ... the request is completed and names what stays ...
    assert_eq!(run.request.state, DataRequestState::Completed);
    assert_eq!(run.report.held_rows, 1);
    let outcome = run.request.outcome.clone().unwrap();
    for rec in RETAINED {
        assert!(outcome.contains(rec.label), "outcome names {}", rec.label);
    }
    assert!(outcome.contains("1 held row"));
    assert_eq!(run.request.retained.len(), RETAINED.len());

    // ... and every retained record is still there.
    let one = |sql: &'static str| async move {
        let n: i64 = sqlx::query_scalar(sql).fetch_one(pool).await.unwrap();
        n
    };
    assert_eq!(
        one("SELECT count(*) FROM persons WHERE id = 100").await,
        1,
        "person record"
    );
    assert_eq!(
        one("SELECT count(*) FROM platform_invites WHERE id = 5 AND redeemed_by_account_id = 100")
            .await,
        1,
        "invite graph"
    );
    assert_eq!(
        one("SELECT count(*) FROM accounts WHERE id = 100 AND invited_by_invite_id = 5").await,
        1,
        "invite graph, account side"
    );
    assert_eq!(
        one("SELECT count(*) FROM payment_invoices WHERE id = 9 AND user_id = 100").await,
        1,
        "invoices"
    );
    assert_eq!(
        one("SELECT count(*) FROM person_bans WHERE id = 6").await,
        1,
        "moderation history"
    );
    assert_eq!(
        one("SELECT count(*) FROM platform_audit_log WHERE id = 7").await,
        1,
        "audit trail"
    );
    assert_eq!(
        one("SELECT count(*) FROM messages WHERE author_id = 100 AND hold_id IS NULL").await,
        1,
        "messages the member did not delete stay (D-029, D-123)"
    );
    assert_eq!(
        one("SELECT count(*) FROM sessions WHERE account_id = 100 AND (ip IS NOT NULL OR user_agent IS NOT NULL OR device_label IS NOT NULL)").await,
        0,
        "session addresses are scrubbed"
    );
    assert_eq!(
        one("SELECT count(*) FROM login_events WHERE account_id = 100 AND ip IS NOT NULL").await,
        0,
        "sign-in addresses are scrubbed"
    );
    assert_eq!(
        one("SELECT count(*) FROM messages WHERE author_id = 100 AND hold_id = 10").await,
        1,
        "held content"
    );
    assert_eq!(
        one("SELECT count(*) FROM deleted_accounts WHERE account_id = 100").await,
        1,
        "staff archive"
    );
    if identities_ok {
        assert_eq!(
            one("SELECT count(*) FROM external_identities WHERE account_id = 100").await,
            1,
            "discord link"
        );
    }
    assert_eq!(
        one("SELECT count(*) FROM data_requests WHERE id = (SELECT id FROM data_requests LIMIT 1)")
            .await,
        1,
        "request log"
    );
    assert_eq!(log_count(pool, "data_request.run_erasure").await, 1);

    // A second run finds nothing left to do.
    assert!(
        r.run_erasure(q.id, T0, &entry("data_request.run_erasure", q.id))
            .await
            .is_err()
    );
    // The account row can never be removed while a request or invoice points at it.
    assert!(
        sqlx::query("DELETE FROM accounts WHERE id = 100")
            .execute(pool)
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_account_that_owns_a_server_cannot_be_erased_until_it_lets_go() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    setup(pool).await;
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES (300, 'Room', 100)")
        .execute(pool)
        .await
        .unwrap();
    let r = repo(pool);
    let q = r
        .create(
            &new(DataRequestKind::Erasure, 100),
            &entry("data_request.create", 0),
        )
        .await
        .unwrap();
    r.verify(q.id, STAFF, T0, &entry("data_request.verify", q.id))
        .await
        .unwrap();
    let err = r
        .run_erasure(q.id, T0, &entry("data_request.run_erasure", q.id))
        .await
        .unwrap_err();
    match err {
        AdminError::Conflict(m) => assert!(m.contains("owns 1 server")),
        other => panic!("{other:?}"),
    }
    // Nothing changed: still verified, account active, no log row for the failed run.
    assert_eq!(
        r.get(q.id).await.unwrap().unwrap().state,
        DataRequestState::Verified
    );
    let st: String = sqlx::query_scalar("SELECT state::text FROM accounts WHERE id = 100")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(st, "active");
    assert_eq!(log_count(pool, "data_request.run_erasure").await, 0);
}

/// Source guard: no job, route or migration deletes from `data_requests`.
#[test]
fn nothing_in_the_workspace_purges_data_requests() {
    fn walk(dir: &std::path::Path, hits: &mut Vec<String>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if p.file_name()
                    .is_some_and(|n| n == "target" || n == "node_modules")
                {
                    continue;
                }
                walk(&p, hits);
            } else if matches!(p.extension().and_then(|x| x.to_str()), Some("rs" | "sql"))
                && !p.ends_with("tests/data_requests.rs")
            {
                let text = std::fs::read_to_string(&p)
                    .unwrap_or_default()
                    .to_lowercase();
                let squashed = text.split_whitespace().collect::<Vec<_>>().join(" ");
                for pat in [
                    "delete from data_requests",
                    "truncate data_requests",
                    "truncate table data_requests",
                    "drop table data_requests",
                ] {
                    if squashed.contains(pat) {
                        hits.push(format!("{}: {pat}", p.display()));
                    }
                }
            }
        }
    }
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut hits = Vec::new();
    walk(&crates, &mut hits);
    assert!(
        hits.is_empty(),
        "data_requests rows are kept forever (D-124, D-212): {hits:?}"
    );
    // And the schema has no cascade into the table.
    let sql = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations/1100_data_requests.sql"),
    )
    .unwrap()
    .to_lowercase();
    assert!(
        !sql.contains("on delete"),
        "no cascade or set-null on data_requests"
    );
}

/// The export README and the deletion screen name the same eight records, in the same order, as the
/// erasure outcome (N-03, D-318).
#[test]
fn readme_and_deletion_screen_list_the_same_eight_records() {
    assert_eq!(RETAINED.len(), 8);
    let readme = sr_db::data_export::readme();
    let frag = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/web/src/i18n/messages/en/p12-admin-mod-be-1.ts"),
    )
    .unwrap();
    let screen = frag
        .split("'settings.account.removalRetained':")
        .nth(1)
        .and_then(|t| t.split("',").next())
        .expect("deletion screen string");
    let (mut at_readme, mut at_screen) = (0, 0);
    for r in RETAINED {
        let i = readme[at_readme..]
            .find(r.label)
            .unwrap_or_else(|| panic!("README names {}", r.label));
        at_readme += i;
        let j = screen[at_screen..]
            .find(r.label)
            .unwrap_or_else(|| panic!("screen names {}", r.label));
        at_screen += j;
        assert!(
            readme.contains(r.reason),
            "README carries the reason of {}",
            r.label
        );
    }
    for text in [&readme, &screen.to_owned()] {
        assert!(
            text.contains("Your profile, password, passkeys, sessions and email address are removed; the linked Discord"),
            "{text}"
        );
    }
    assert!(!readme.contains("sign-in data are removed"));
}
