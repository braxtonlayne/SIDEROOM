//! OPS-INV-001: Lock the Doors and the invite quotas against real Postgres. Run with a scratch database:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test invite_controls -- --ignored --test-threads=1`

use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use serde_json::json;
use sr_admin::{AdminError, AdminService, Caller};
use sr_auth::{
    invite::InviteCode,
    store::{AuthStore, InviteBlock},
};
use sr_core::PlatformPermissions as P;
use sr_db::{
    PgPool,
    admin_repo::PgAdminRepo,
    auth_repo::{IdSource, PgAuthRepo},
    auth_store::PgAuthStore,
    member_invites,
    registration::{NewRegistration, Rejection, register_with_invite},
    testing::TestDb,
};

const NOW: i64 = 2_000_000_000;

fn uid() -> i64 {
    static N: AtomicI64 = AtomicI64::new(1_000);
    N.fetch_add(1, Ordering::Relaxed)
}

async fn account(pool: &PgPool, id: i64, verified: bool) {
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash, email_verified_at, created_at) \
         VALUES ($1, $1, $2, $3, 'x', CASE WHEN $4 THEN now() END, to_timestamp($5))",
    )
    .bind(id)
    .bind(format!("u{id}"))
    .bind(format!("u{id}@example.test"))
    .bind(verified)
    .bind((NOW - 30 * 86_400) as f64)
    .execute(pool)
    .await
    .unwrap();
}

async fn credits(pool: &PgPool, account: i64, n: i32) {
    sqlx::query(
        "INSERT INTO invite_credits_ledger (id, account_id, delta, reason) VALUES ($1, $2, $3, 'initial_grant')",
    )
    .bind(uid())
    .bind(account)
    .bind(n)
    .execute(pool)
    .await
    .unwrap();
}

async fn invite(pool: &PgPool, issuer: i64) -> InviteCode {
    let code = InviteCode::generate().unwrap();
    sqlx::query("INSERT INTO platform_invites (id, code, issuer_account_id) VALUES ($1, $2, $3)")
        .bind(uid())
        .bind(code.canonical())
        .bind(issuer)
        .execute(pool)
        .await
        .unwrap();
    code
}

fn reg(code: &InviteCode) -> NewRegistration {
    let id = uid();
    NewRegistration {
        discord_id: None,
        identity_id: uid(),
        account_id: id,
        person_id: uid(),
        ledger_ids: [uid(), uid()],
        code: code.canonical().to_owned(),
        username: format!("new{id}"),
        display_name: None,
        email: format!("new{id}@example.test"),
        password_hash: "x".into(),
        account_cap: i64::MAX,
        initial_credits: 5,
    }
}

fn staff(actor: i64) -> Caller {
    Caller {
        user_id: actor,
        permissions: P::OWNER,
        is_owner: true,
        ip: Some("127.0.0.1".into()),
    }
}

fn admin(pool: &PgPool) -> AdminService {
    AdminService::new(Arc::new(PgAdminRepo::new(pool.clone(), Arc::new(uid))))
}

fn auth_store(pool: &PgPool) -> PgAuthStore {
    let ids: IdSource = Arc::new(uid);
    PgAuthStore::new(
        pool.clone(),
        PgAuthRepo::new(pool.clone(), ids.clone(), [7; 32]),
        ids,
    )
}

async fn setting(pool: &PgPool, key: &str, value: serde_json::Value) {
    sqlx::query(
        "INSERT INTO platform_settings (key, value) VALUES ($1, $2) \
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await
    .unwrap();
}

/// `(action, reason)` of the access log rows with this action, oldest first.
async fn log_rows(pool: &PgPool, action: &str) -> Vec<(String, String)> {
    sqlx::query_as("SELECT action, reason FROM staff_access_log WHERE action = $1 ORDER BY seq")
        .bind(action)
        .fetch_all(pool)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_lock_refuses_registration_even_with_a_valid_invite_and_is_audited() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    let (op, issuer) = (uid(), uid());
    account(pool, op, true).await;
    account(pool, issuer, true).await;
    credits(pool, issuer, 5).await;
    let svc = admin(pool);
    let store = auth_store(pool);

    // Off by default: the doors are open and registration works.
    assert!(!store.invite_controls().await.unwrap().locked);
    let first = invite(pool, issuer).await;
    register_with_invite(pool, &reg(&first))
        .await
        .unwrap()
        .expect("open doors register");

    // A reason is required, and without it nothing changes.
    assert_eq!(
        svc.set_invites_locked(&staff(op), true, None)
            .await
            .unwrap_err(),
        AdminError::ReasonRequired
    );
    assert!(!store.invite_controls().await.unwrap().locked);
    assert!(log_rows(pool, "invites.lock").await.is_empty());

    // Lock the Doors.
    let controls = svc
        .set_invites_locked(&staff(op), true, Some("wave of bad signups"))
        .await
        .unwrap();
    assert!(controls.locked);
    assert!(store.invite_controls().await.unwrap().locked);
    assert_eq!(
        log_rows(pool, "invites.lock").await,
        vec![("invites.lock".to_owned(), "wave of bad signups".to_owned())]
    );

    // A valid, open invite registers nobody; nothing is written (the invite stays open, no account exists).
    let valid = invite(pool, issuer).await;
    let accounts_before: i64 = sqlx::query_scalar("SELECT count(*) FROM accounts")
        .fetch_one(pool)
        .await
        .unwrap();
    let r = reg(&valid);
    assert_eq!(
        register_with_invite(pool, &r).await.unwrap(),
        Err(Rejection::Locked)
    );
    // The store the routes use says the same.
    assert_eq!(
        store
            .register(sr_auth::store::RegisterTx {
                account_id: r.account_id,
                new_person_id: r.person_id,
                ledger_ids: r.ledger_ids,
                code: valid.clone(),
                username: r.username.clone(),
                display_name: None,
                email: r.email.clone(),
                password_hash: "x".into(),
                discord_id: None,
                identity_id: uid(),
                account_cap: u64::MAX,
                initial_credits: 5,
                now: NOW,
            })
            .await
            .unwrap(),
        Err(sr_auth::store::RegisterRejection::Locked)
    );
    let accounts_after: i64 = sqlx::query_scalar("SELECT count(*) FROM accounts")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(accounts_before, accounts_after);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state::text FROM platform_invites WHERE code = $1")
            .bind(valid.canonical())
            .fetch_one(pool)
            .await
            .unwrap(),
        "open"
    );

    // Existing members are unaffected: the issuer still sees and manages their invites.
    let overview = member_invites::overview(pool, issuer, NOW).await.unwrap();
    assert!(overview.blocked.is_none());
    assert_eq!(
        member_invites::create(pool, issuer, uid(), "BBBBBBBBBB", "", NOW)
            .await
            .unwrap(),
        Ok(())
    );

    // Setting the same value again writes no second row.
    svc.set_invites_locked(&staff(op), true, Some("still closed"))
        .await
        .unwrap();
    assert_eq!(log_rows(pool, "invites.lock").await.len(), 1);

    // Lifting the lock is audited too, and registration works again.
    let controls = svc
        .set_invites_locked(&staff(op), false, Some("incident over"))
        .await
        .unwrap();
    assert!(!controls.locked);
    assert_eq!(
        log_rows(pool, "invites.unlock").await,
        vec![("invites.unlock".to_owned(), "incident over".to_owned())]
    );
    register_with_invite(pool, &reg(&valid))
        .await
        .unwrap()
        .expect("the lifted lock lets the invite through");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn changing_the_lock_through_the_settings_patch_is_audited_as_a_lock() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    let op = uid();
    account(pool, op, true).await;
    let svc = admin(pool);

    svc.patch_settings(
        &staff(op),
        &[("invites.locked".to_owned(), json!(true))],
        Some("via limits"),
    )
    .await
    .unwrap();
    assert!(auth_store(pool).invite_controls().await.unwrap().locked);
    assert_eq!(log_rows(pool, "invites.lock").await.len(), 1);
    assert_eq!(log_rows(pool, "settings.patch").await.len(), 1);

    // A patch that leaves the lock as it was adds no lock row.
    svc.patch_settings(
        &staff(op),
        &[("invites.locked".to_owned(), json!(true))],
        None,
    )
    .await
    .unwrap();
    assert_eq!(log_rows(pool, "invites.lock").await.len(), 1);
    svc.patch_settings(
        &staff(op),
        &[("invites.locked".to_owned(), json!(false))],
        None,
    )
    .await
    .unwrap();
    assert_eq!(log_rows(pool, "invites.unlock").await.len(), 1);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_quotas_are_settings_and_registration_and_creation_obey_them() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    let (op, a) = (uid(), uid());
    account(pool, op, true).await;
    account(pool, a, true).await;
    credits(pool, a, 5).await;
    let store = auth_store(pool);
    let svc = admin(pool);

    // The credit seed: nothing set means "use the configured seed"; the setting wins once saved.
    assert_eq!(store.invite_controls().await.unwrap().default_credits, None);
    let c = svc.invite_controls(&staff(op)).await.unwrap();
    assert_eq!(
        (
            c.locked,
            c.default_credits,
            c.max_outstanding,
            c.unused_code_days
        ),
        (false, 5, 20, 30)
    );
    svc.patch_settings(
        &staff(op),
        &[
            ("invites.default_credits".to_owned(), json!(8)),
            ("invites.max_outstanding".to_owned(), json!(2)),
            ("invites.unused_code_days".to_owned(), json!(10)),
        ],
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        store.invite_controls().await.unwrap().default_credits,
        Some(8)
    );
    let c = svc.invite_controls(&staff(op)).await.unwrap();
    assert_eq!(
        (c.default_credits, c.max_outstanding, c.unused_code_days),
        (8, 2, 10)
    );
    // Out of range values are refused.
    assert!(
        svc.patch_settings(
            &staff(op),
            &[("invites.max_outstanding".to_owned(), json!(0))],
            None
        )
        .await
        .is_err()
    );

    // The seed reaches the new account's ledger when the route passes it on.
    let code = invite(pool, a).await;
    let mut r = reg(&code);
    r.initial_credits = store
        .invite_controls()
        .await
        .unwrap()
        .default_credits
        .unwrap();
    register_with_invite(pool, &r).await.unwrap().unwrap();
    let seeded: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(delta), 0)::BIGINT FROM invite_credits_ledger WHERE account_id = $1",
    )
    .bind(r.account_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(seeded, 8);

    // max_outstanding = 2 caps the unused codes of an account that has credits to spare.
    for n in 0..2 {
        assert_eq!(
            member_invites::create(pool, a, uid(), &format!("CCCCCCCCC{n}"), "", NOW)
                .await
                .unwrap(),
            Ok(()),
            "code {n}"
        );
    }
    assert_eq!(
        member_invites::create(pool, a, uid(), "CCCCCCCCC9", "", NOW)
            .await
            .unwrap(),
        Err(InviteBlock::AtOutstandingMax)
    );
    assert_eq!(
        member_invites::overview(pool, a, NOW)
            .await
            .unwrap()
            .blocked,
        Some(InviteBlock::AtOutstandingMax)
    );

    // Raising the quota lets the next one through; lowering it keeps the codes already made.
    setting(pool, "invites.max_outstanding", json!(3)).await;
    assert_eq!(
        member_invites::create(pool, a, uid(), "CCCCCCCCC9", "", NOW)
            .await
            .unwrap(),
        Ok(())
    );
    setting(pool, "invites.max_outstanding", json!(1)).await;
    assert_eq!(
        member_invites::overview(pool, a, NOW)
            .await
            .unwrap()
            .open
            .len(),
        3
    );

    // The credit balance still caps first: an account with one credit and one code is out of credits.
    setting(pool, "invites.max_outstanding", json!(50)).await;
    let b = uid();
    account(pool, b, true).await;
    credits(pool, b, 1).await;
    assert_eq!(
        member_invites::create(pool, b, uid(), "DDDDDDDDD1", "", NOW)
            .await
            .unwrap(),
        Ok(())
    );
    assert_eq!(
        member_invites::create(pool, b, uid(), "DDDDDDDDD2", "", NOW)
            .await
            .unwrap(),
        Err(InviteBlock::NoCredits)
    );
}
