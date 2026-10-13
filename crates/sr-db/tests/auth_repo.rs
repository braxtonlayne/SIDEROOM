//! DB-backed tests for the auth repositories, accounts helpers and the
//! partition manager. Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db -- --ignored`

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use sr_auth::{
    email_token::{EmailToken, EmailTokenPurpose},
    invite::InviteCode,
    repo::{
        EmailTokenRepository, InviteRecord, InviteRepository, InviteStatus, RecoveryCodeRepository,
        SessionRecord, SessionRepository, TotpRepository,
    },
    session::SessionToken,
};
use sr_db::{
    DbConfig, PgPool,
    accounts::{self, NewAccount},
    auth_repo::{AuthRepoError, PgAuthRepo},
    partitions,
};

fn uid() -> i64 {
    static N: AtomicI64 = AtomicI64::new(0);
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + N.fetch_add(1, Ordering::Relaxed) % 1000
}

async fn pool() -> PgPool {
    let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
    let pool = sr_db::connect(&DbConfig::new(url)).await.expect("connect");
    sr_db::migrate(&pool).await.expect("migrate");
    pool
}

fn repo(pool: &PgPool) -> PgAuthRepo {
    PgAuthRepo::new(pool.clone(), Arc::new(uid), [7u8; 32])
}

async fn account(pool: &PgPool) -> i64 {
    let id = uid();
    let name = format!("t{id}");
    let email = format!("{name}@example.test");
    let ok = accounts::create_account(
        pool,
        &NewAccount {
            flags: 0,
            id,
            person_id: uid(),
            // usernames are limited to 32 chars
            username: &name[..name.len().min(32)],
            display_name: None,
            email: &email,
            password_hash: "x",
            state: "active",
            email_verified: true,
            initial_credits: 5,
        },
        uid(),
    )
    .await
    .unwrap();
    assert!(ok);
    id
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn sessions_roundtrip_and_revoke() {
    let pool = pool().await;
    let r = repo(&pool);
    let user = account(&pool).await;
    let t = now();
    let tok = SessionToken::generate().unwrap();
    let rec = SessionRecord {
        id: uid(),
        user_id: user,
        token_hash: tok.hash(),
        created_at: t,
        last_seen_at: t,
        expires_at: t + 3600,
        device_name: Some("Firefox on Linux".into()),
    };
    SessionRepository::create(&r, rec.clone()).await.unwrap();
    // F-02: a login counts as the account being seen.
    let seen: bool =
        sqlx::query_scalar("SELECT last_seen_at IS NOT NULL FROM accounts WHERE id = $1")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(seen, "creating a session sets accounts.last_seen_at");
    let found = r.find_by_hash(&tok.hash()).await.unwrap().unwrap();
    assert_eq!(found, rec);

    r.touch(rec.id, t + 10, t + 7200).await.unwrap();
    let live = r.list_for_user(user, t + 20).await.unwrap();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].expires_at, t + 7200);
    assert!(r.list_for_user(user, t + 8000).await.unwrap().is_empty());

    // Other users cannot revoke it.
    assert!(
        !SessionRepository::revoke(&r, user + 1, rec.id)
            .await
            .unwrap()
    );
    assert!(SessionRepository::revoke(&r, user, rec.id).await.unwrap());
    assert!(!SessionRepository::revoke(&r, user, rec.id).await.unwrap());
    assert!(r.find_by_hash(&tok.hash()).await.unwrap().is_none());

    let a = SessionToken::generate().unwrap();
    let b = SessionToken::generate().unwrap();
    for (tk, id) in [(&a, uid()), (&b, uid())] {
        SessionRepository::create(
            &r,
            SessionRecord {
                id,
                user_id: user,
                token_hash: tk.hash(),
                created_at: t,
                last_seen_at: t,
                expires_at: t + 100,
                device_name: None,
            },
        )
        .await
        .unwrap();
    }
    let keep = r.find_by_hash(&a.hash()).await.unwrap().unwrap().id;
    assert_eq!(r.revoke_all_except(user, Some(keep)).await.unwrap(), 1);
    assert!(r.find_by_hash(&a.hash()).await.unwrap().is_some());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn totp_encrypted_and_replay_protected() {
    let pool = pool().await;
    let r = repo(&pool);
    let user = account(&pool).await;
    r.put_pending(user, b"12345678901234567890").await.unwrap();

    // Ciphertext at rest is not the plaintext.
    let raw: Vec<u8> = sqlx_secret(&pool, user).await;
    assert!(!raw.windows(20).any(|w| w == b"12345678901234567890"));

    let got = TotpRepository::get(&r, user).await.unwrap().unwrap();
    assert_eq!(got.secret, b"12345678901234567890");
    assert!(!got.confirmed);
    assert!(!r.consume_step(user, 5).await.unwrap(), "unconfirmed");

    r.confirm(user, 100).await.unwrap();
    assert!(matches!(
        r.confirm(user, 101).await,
        Err(AuthRepoError::NotFound)
    ));
    assert!(matches!(
        r.put_pending(user, b"other").await,
        Err(AuthRepoError::AlreadyConfirmed)
    ));
    assert!(!r.consume_step(user, 100).await.unwrap(), "replay");
    assert!(r.consume_step(user, 101).await.unwrap());
    assert_eq!(
        TotpRepository::get(&r, user)
            .await
            .unwrap()
            .unwrap()
            .last_used_step,
        Some(101)
    );
    TotpRepository::delete(&r, user).await.unwrap();
    assert!(TotpRepository::get(&r, user).await.unwrap().is_none());
}

async fn sqlx_secret(pool: &PgPool, user: i64) -> Vec<u8> {
    use sqlx::Row;
    sqlx::query("SELECT secret_enc FROM mfa_totp WHERE account_id = $1")
        .bind(user)
        .fetch_one(pool)
        .await
        .unwrap()
        .get(0)
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn recovery_codes_single_use() {
    let pool = pool().await;
    let r = repo(&pool);
    let user = account(&pool).await;
    r.replace_all(user, &batch(2)).await.unwrap();
    let unused = r.list_unused(user).await.unwrap();
    assert_eq!(unused.len(), 2);
    assert!(
        RecoveryCodeRepository::consume(&r, user, unused[0].0, now())
            .await
            .unwrap()
    );
    assert!(
        !RecoveryCodeRepository::consume(&r, user, unused[0].0, now())
            .await
            .unwrap()
    );
    assert_eq!(r.list_unused(user).await.unwrap().len(), 1);
    r.replace_all(user, &batch(1)).await.unwrap();
    assert_eq!(r.list_unused(user).await.unwrap().len(), 1);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn invites_lifecycle() {
    let pool = pool().await;
    let r = repo(&pool);
    let user = account(&pool).await;
    let code = InviteCode::generate().unwrap();
    let rec = InviteRecord {
        code: code.canonical().to_owned(),
        issuer_id: user,
        note: Some("for Sam".into()),
        status: InviteStatus::Open,
        created_at: now(),
    };
    InviteRepository::create(&r, rec.clone()).await.unwrap();
    assert!(matches!(
        InviteRepository::create(&r, rec.clone()).await,
        Err(AuthRepoError::Duplicate)
    ));
    assert_eq!(r.find(&code).await.unwrap().unwrap(), rec);
    assert_eq!(r.count_open(user).await.unwrap(), 1);
    assert!(!InviteRepository::revoke(&r, user + 1, &code).await.unwrap());
    assert!(InviteRepository::revoke(&r, user, &code).await.unwrap());
    assert!(!InviteRepository::revoke(&r, user, &code).await.unwrap());
    assert_eq!(
        r.find(&code).await.unwrap().unwrap().status,
        InviteStatus::Revoked
    );
    assert_eq!(r.count_open(user).await.unwrap(), 0);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn email_tokens_single_use_and_superseded() {
    let pool = pool().await;
    let r = repo(&pool);
    let user = account(&pool).await;
    let p = EmailTokenPurpose::ResetPassword;
    let t = now();
    let first = EmailToken::generate(p).unwrap();
    let second = EmailToken::generate(p).unwrap();
    r.put(user, p, &first.hash(), t + 100).await.unwrap();
    r.put(user, p, &second.hash(), t + 100).await.unwrap();
    assert_eq!(
        EmailTokenRepository::consume(&r, p, &first.hash(), t)
            .await
            .unwrap(),
        None
    );
    // Wrong purpose does not match.
    assert_eq!(
        EmailTokenRepository::consume(&r, EmailTokenPurpose::VerifyEmail, &second.hash(), t)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        EmailTokenRepository::consume(&r, p, &second.hash(), t)
            .await
            .unwrap(),
        Some(user)
    );
    assert_eq!(
        EmailTokenRepository::consume(&r, p, &second.hash(), t)
            .await
            .unwrap(),
        None
    );
    // Expired.
    let third = EmailToken::generate(p).unwrap();
    r.put(user, p, &third.hash(), t + 10).await.unwrap();
    assert_eq!(
        EmailTokenRepository::consume(&r, p, &third.hash(), t + 11)
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn reset_2fa_clears_factors_and_sessions() {
    let pool = pool().await;
    let r = repo(&pool);
    let user = account(&pool).await;
    r.put_pending(user, b"secret").await.unwrap();
    r.replace_all(user, &batch(1)).await.unwrap();
    let tok = SessionToken::generate().unwrap();
    let t = now();
    SessionRepository::create(
        &r,
        SessionRecord {
            id: uid(),
            user_id: user,
            token_hash: tok.hash(),
            created_at: t,
            last_seen_at: t,
            expires_at: t + 100,
            device_name: None,
        },
    )
    .await
    .unwrap();
    let res = accounts::reset_2fa(&pool, user).await.unwrap();
    assert_eq!(
        (res.totp, res.recovery_codes, res.sessions_revoked),
        (1, 1, 1)
    );
    assert!(TotpRepository::get(&r, user).await.unwrap().is_none());
    assert!(r.find_by_hash(&tok.hash()).await.unwrap().is_none());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn partition_manager_is_idempotent_and_routes_rows() {
    let pool = pool().await;
    let table = format!("pt_{}", uid());
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE TABLE {table} (id BIGINT NOT NULL, body TEXT) PARTITION BY RANGE (id)"
    )))
    .execute(&pool)
    .await
    .unwrap();
    let now_ms = sr_core::snowflake::SIDEROOM_EPOCH_MS + 40 * 86_400_000; // Feb 2026
    let names = partitions::ensure_ahead(&pool, &table, now_ms, 2)
        .await
        .unwrap();
    assert_eq!(names.len(), 3);
    assert_eq!(
        partitions::ensure_ahead(&pool, &table, now_ms, 2)
            .await
            .unwrap(),
        names
    );
    // An id minted in Feb lands in the Feb partition.
    let id = (40_i64 * 86_400_000) << 22;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO {table} (id) VALUES ($1)"
    )))
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    let in_feb: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM {}",
        names[0]
    )))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(in_feb, 1);
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP TABLE {table}")))
        .execute(&pool)
        .await
        .unwrap();
}

/// `n` recovery-code digests (any distinct values will do for these tests).
fn batch(n: usize) -> Vec<sr_auth::session::TokenHash> {
    sr_auth::recovery::generate_batch()
        .expect("recovery batch")
        .hashes
        .into_iter()
        .take(n)
        .collect()
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn account_identity_edits_enforce_uniqueness_and_email_change_tokens() {
    use sr_auth::store::AuthStore;
    let pool = pool().await;
    let store = sr_db::auth_store::PgAuthStore::new(pool.clone(), repo(&pool), Arc::new(uid));
    let (a, b) = (account(&pool).await, account(&pool).await);
    let a_rec = store.find_by_id(a).await.unwrap().unwrap();
    let b_rec = store.find_by_id(b).await.unwrap().unwrap();

    // Display name: set, clear.
    store.set_display_name(a, Some("Ada L.")).await.unwrap();
    assert_eq!(
        store
            .find_by_id(a)
            .await
            .unwrap()
            .unwrap()
            .display_name
            .as_deref(),
        Some("Ada L.")
    );
    store.set_display_name(a, None).await.unwrap();
    assert_eq!(
        store.find_by_id(a).await.unwrap().unwrap().display_name,
        None
    );

    // Username: unique, case-insensitive (citext).
    assert!(!store.set_username(a, &b_rec.username).await.unwrap());
    assert!(
        !store
            .set_username(a, &b_rec.username.to_uppercase())
            .await
            .unwrap()
    );
    let fresh = format!("n{}", uid());
    let fresh = &fresh[..fresh.len().min(32)];
    assert!(store.set_username(a, fresh).await.unwrap());
    assert_eq!(store.find_by_id(a).await.unwrap().unwrap().username, fresh);
    assert!(store.username_exists(fresh).await.unwrap());

    // Email change: the token remembers the new address; single use; supersedes.
    let t = now();
    let first = EmailToken::generate(EmailTokenPurpose::ChangeEmail).unwrap();
    let second = EmailToken::generate(EmailTokenPurpose::ChangeEmail).unwrap();
    let new_email = format!("new{}@example.test", uid());
    store
        .email_change_put(a, "stale@example.test", &first.hash(), t + 100)
        .await
        .unwrap();
    store
        .email_change_put(a, &new_email, &second.hash(), t + 100)
        .await
        .unwrap();
    assert_eq!(
        store.email_change_consume(&first.hash(), t).await.unwrap(),
        None
    );
    // A verify-email token is not a change token.
    let verify = EmailToken::generate(EmailTokenPurpose::VerifyEmail).unwrap();
    store
        .email_token_put(a, EmailTokenPurpose::VerifyEmail, &verify.hash(), t + 100)
        .await
        .unwrap();
    assert_eq!(
        store.email_change_consume(&verify.hash(), t).await.unwrap(),
        None
    );
    assert_eq!(
        store.email_change_consume(&second.hash(), t).await.unwrap(),
        Some((a, new_email.clone()))
    );
    assert_eq!(
        store.email_change_consume(&second.hash(), t).await.unwrap(),
        None
    );
    // Expired.
    let third = EmailToken::generate(EmailTokenPurpose::ChangeEmail).unwrap();
    store
        .email_change_put(a, "late@example.test", &third.hash(), t + 10)
        .await
        .unwrap();
    assert_eq!(
        store
            .email_change_consume(&third.hash(), t + 11)
            .await
            .unwrap(),
        None
    );

    // Applying it: taken addresses are refused, free ones become verified.
    assert!(!store.set_email(a, &b_rec.email).await.unwrap());
    assert!(store.set_email(a, &new_email).await.unwrap());
    let after = store.find_by_id(a).await.unwrap().unwrap();
    assert_eq!(after.email, new_email);
    assert!(after.email_verified);
    assert_ne!(after.email, a_rec.email);
}

fn passkey(account_id: i64, cred: u8, name: &str) -> sr_auth::store::PasskeyRecord {
    sr_auth::store::PasskeyRecord {
        id: uid(),
        account_id,
        credential_id: vec![cred; 20],
        cose_key: vec![0xA5, 1, 2],
        alg: -7,
        sign_count: 0,
        backup_eligible: true,
        backup_state: false,
        aaguid: vec![0; 16],
        name: name.to_owned(),
        created_at: now(),
        last_used_at: None,
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn passkey_store_roundtrip_ownership_and_counter_cas() {
    use sr_auth::store::AuthStore;
    let pool = pool().await;
    let store = sr_db::auth_store::PgAuthStore::new(pool.clone(), repo(&pool), Arc::new(uid));
    let (a, b) = (account(&pool).await, account(&pool).await);

    // The user handle is stable: the first candidate wins.
    let h1 = store.passkey_user_handle(a, &[1u8; 32]).await.unwrap();
    let h2 = store.passkey_user_handle(a, &[2u8; 32]).await.unwrap();
    assert_eq!((h1.clone(), h2), (vec![1u8; 32], vec![1u8; 32]));
    assert_ne!(store.passkey_user_handle(b, &[3u8; 32]).await.unwrap(), h1);

    let tag = u8::try_from(uid() % 200).unwrap();
    let rec = passkey(a, tag, "Laptop");
    assert!(store.passkey_insert(rec.clone()).await.unwrap());
    // Same credential id: refused, whoever asks.
    assert!(
        !store
            .passkey_insert(passkey(b, tag, "Clone"))
            .await
            .unwrap()
    );
    let found = store
        .passkey_find(&rec.credential_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.id, rec.id);
    assert_eq!(found.name, "Laptop");
    assert!(found.backup_eligible && !found.backup_state);
    assert_eq!(found.aaguid, vec![0u8; 16]);
    assert_eq!(store.passkey_list(a).await.unwrap().len(), 1);
    assert!(store.passkey_list(b).await.unwrap().is_empty());

    // Ownership on rename and delete.
    assert!(!store.passkey_rename(b, rec.id, "mine").await.unwrap());
    assert!(store.passkey_rename(a, rec.id, "Phone").await.unwrap());
    assert!(!store.passkey_delete(b, rec.id).await.unwrap());
    assert_eq!(store.passkey_list(a).await.unwrap()[0].name, "Phone");

    // Counter compare-and-set: the second writer with the old value loses.
    let t = now();
    assert!(
        store
            .passkey_record_use(rec.id, 0, 5, true, t)
            .await
            .unwrap()
    );
    assert!(
        !store
            .passkey_record_use(rec.id, 0, 6, true, t)
            .await
            .unwrap()
    );
    let after = store
        .passkey_find(&rec.credential_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((after.sign_count, after.backup_state), (5, true));
    assert_eq!(after.last_used_at, Some(t));

    // Name constraint (1 to 64 characters) holds in the schema too.
    assert!(
        store
            .passkey_insert(passkey(a, tag.wrapping_add(1), ""))
            .await
            .is_err()
    );

    assert!(store.passkey_delete(a, rec.id).await.unwrap());
    assert!(
        store
            .passkey_find(&rec.credential_id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn reset_2fa_removes_passkeys() {
    use sr_auth::store::AuthStore;
    let pool = pool().await;
    let store = sr_db::auth_store::PgAuthStore::new(pool.clone(), repo(&pool), Arc::new(uid));
    let user = account(&pool).await;
    let tag = u8::try_from(uid() % 200).unwrap();
    assert!(store.passkey_insert(passkey(user, tag, "k")).await.unwrap());
    accounts::reset_2fa(&pool, user).await.unwrap();
    assert!(store.passkey_list(user).await.unwrap().is_empty());
}
