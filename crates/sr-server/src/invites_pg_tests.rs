//! The `invites.prune` job against real Postgres (OPS-INV-001; run with `SR_TEST_DATABASE_URL`, `--ignored`,
//! `--test-threads=1`).

use std::sync::Arc;

use sqlx::PgPool;
use sr_admin::AdminRepository;
use sr_db::admin_repo::PgAdminRepo;
use sr_db::testing::TestDb;

use crate::invites_jobs::run_once;

async fn account(pool: &PgPool, id: i64, verified: bool, seen: bool, age_days: i32) {
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash, email_verified_at, \
                               last_seen_at, created_at) \
         VALUES ($1, $1, $2, $3, 'x', CASE WHEN $4 THEN now() END, CASE WHEN $5 THEN now() END, \
                 now() - make_interval(days => $6))",
    )
    .bind(id)
    .bind(format!("u{id}"))
    .bind(format!("u{id}@example.test"))
    .bind(verified)
    .bind(seen)
    .bind(age_days)
    .execute(pool)
    .await
    .unwrap();
}

/// `redeemer` joined with a code of `issuer`.
async fn redeem(pool: &PgPool, id: i64, issuer: i64, redeemer: i64) {
    sqlx::query(
        "INSERT INTO platform_invites (id, code, issuer_account_id, state, redeemed_by_account_id, redeemed_at) \
         VALUES ($1, $2, $3, 'redeemed', $4, now())",
    )
    .bind(id)
    .bind(format!("R{id:09}"))
    .bind(issuer)
    .bind(redeemer)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE accounts SET invited_by_invite_id = $1 WHERE id = $2")
        .bind(id)
        .bind(redeemer)
        .execute(pool)
        .await
        .unwrap();
}

async fn open_code(pool: &PgPool, id: i64, issuer: i64, age_days: i32, emergency: bool) {
    sqlx::query(
        "INSERT INTO platform_invites (id, code, issuer_account_id, emergency, created_at) \
         VALUES ($1, $2, $3, $4, now() - make_interval(days => $5))",
    )
    .bind(id)
    .bind(format!("O{id:09}"))
    .bind(issuer)
    .bind(emergency)
    .bind(age_days)
    .execute(pool)
    .await
    .unwrap();
}

async fn state_of(pool: &PgPool, id: i64) -> String {
    sqlx::query_scalar("SELECT state::text FROM platform_invites WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn graph(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM invite_graph")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn marked(pool: &PgPool) -> Vec<(i64, i32)> {
    sqlx::query_as(
        "SELECT root_account_id, subtree_size FROM invite_inactive_roots ORDER BY root_account_id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

/// R invited A, D, E and F; A invited B and C. Open codes of every age.
async fn world(pool: &PgPool) {
    account(pool, 1, true, true, 200).await; // R
    account(pool, 2, false, false, 40).await; // A: never activated, old enough
    account(pool, 3, false, false, 40).await; // B: below A, never activated
    account(pool, 4, true, true, 40).await; // C: below A, activated
    account(pool, 5, true, true, 40).await; // D: verified
    account(pool, 6, false, false, 5).await; // E: never activated but young
    account(pool, 7, false, true, 40).await; // F: unverified but seen online
    for (id, issuer, redeemer) in [
        (101, 1, 2),
        (102, 1, 5),
        (103, 1, 6),
        (104, 1, 7),
        (105, 2, 3),
        (106, 2, 4),
    ] {
        redeem(pool, id, issuer, redeemer).await;
    }
    open_code(pool, 201, 1, 45, false).await; // old: revoked
    open_code(pool, 202, 2, 31, false).await; // old: revoked
    open_code(pool, 203, 1, 5, false).await; // young: kept
    open_code(pool, 204, 1, 90, true).await; // emergency: kept
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_prune_job_revokes_old_unused_codes_and_never_touches_the_graph() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    world(pool).await;
    let edges = graph(pool).await;
    assert_eq!(edges, 6);

    let o = run_once(pool).await.unwrap();
    assert_eq!(o.codes_revoked, 2);
    assert_eq!(state_of(pool, 201).await, "revoked");
    assert_eq!(state_of(pool, 202).await, "revoked");
    assert_eq!(state_of(pool, 203).await, "open");
    assert_eq!(state_of(pool, 204).await, "open");
    // The graph: the same row count before and after, in the pass's own numbers and from outside.
    assert_eq!((o.graph_rows_before, o.graph_rows_after), (edges, edges));
    assert_eq!(graph(pool).await, edges);
    // Every redeemed invite is still redeemed, with its redeemer.
    let redeemed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform_invites WHERE state = 'redeemed' AND redeemed_by_account_id IS NOT NULL",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(redeemed, edges);

    // A (never activated, 40 days old) roots a subtree of three; B (below A) is its own inactive root.
    // E is too young, F was seen online, D and C verified.
    assert_eq!(marked(pool).await, vec![(2, 3), (3, 1)]);
    assert_eq!((o.roots_marked, o.roots_cleared), (2, 0));

    // A second pass has nothing left to do.
    let again = run_once(pool).await.unwrap();
    assert_eq!(
        (again.codes_revoked, again.roots_marked, again.roots_cleared),
        (0, 0, 0)
    );
    assert_eq!(graph(pool).await, edges);

    // Admin > Invites reads the marks and the last pass.
    let repo: Arc<PgAdminRepo> = Arc::new(PgAdminRepo::new(pool.clone(), Arc::new(|| 1)));
    let (roots, total) = repo.inactive_invite_roots(10).await.unwrap();
    assert_eq!(total, 2);
    let a = roots.iter().find(|r| r.account_id == 2).unwrap();
    assert_eq!((a.subtree_size, a.invited_by_account_id), (3, Some(1)));
    let last = repo.last_prune_run().await.unwrap().unwrap();
    assert_eq!(
        (last.codes_revoked, last.roots_marked, last.graph_rows),
        (0, 0, edges)
    );

    // A activates: the next pass lifts its mark (B keeps its own), and still no edge is lost.
    sqlx::query("UPDATE accounts SET email_verified_at = now() WHERE id = 2")
        .execute(pool)
        .await
        .unwrap();
    let lifted = run_once(pool).await.unwrap();
    assert_eq!((lifted.roots_marked, lifted.roots_cleared), (0, 1));
    assert_eq!(marked(pool).await, vec![(3, 1)]);
    assert_eq!(graph(pool).await, edges);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_age_is_a_setting() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    world(pool).await;
    sqlx::query(
        "INSERT INTO platform_settings (key, value) VALUES ('invites.unused_code_days', '60')",
    )
    .execute(pool)
    .await
    .unwrap();
    // 60 days: the 45 and 31 day old codes are young enough; so is every account.
    let o = run_once(pool).await.unwrap();
    assert_eq!((o.codes_revoked, o.roots_marked), (0, 0));
    assert_eq!(state_of(pool, 201).await, "open");
    assert!(marked(pool).await.is_empty());

    sqlx::query("UPDATE platform_settings SET value = '44' WHERE key = 'invites.unused_code_days'")
        .execute(pool)
        .await
        .unwrap();
    // 44 days: only the 45 day old code goes; the accounts are 40 days old, so none is marked.
    let o = run_once(pool).await.unwrap();
    assert_eq!(o.codes_revoked, 1);
    assert_eq!(state_of(pool, 201).await, "revoked");
    assert_eq!(state_of(pool, 202).await, "open");
}
