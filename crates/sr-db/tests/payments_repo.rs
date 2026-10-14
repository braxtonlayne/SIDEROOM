//! Postgres-backed tests of the payments repository: the same scenarios as the in-memory
//! service tests (crediting exactly once, reorg revert, gap limit, pools and levels with grace),
//! plus the SQL-only parts (migration cutover, leaderboards, constraints).
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test payments_repo -- --ignored --test-threads=1`
//!
//! No chain, wallet or network: the wallet is the public BIP-84 test vector as a `tpub`, the chain is
//! `FakeChain`.

use std::{
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};

use sr_db::{
    PgPool,
    accounts::{self, NewAccount},
    media_repo::{EntitlementDownloads, PgDownloadEntitlements},
    payments_repo::PgPaymentRepo,
    testing::TestDb,
};
use sr_media::downloads::DownloadEntitlements;
use sr_payments::{
    DonationConfig, EntitlementService, Entitlements, InvoiceState, PayError, PaymentService,
    Purpose,
    chain::FakeChain,
    descriptor::{Network, base58check_decode, base58check_encode},
    entitlements::GrantKind,
    rates::{FakeRate, RateService, RateSettings},
    repo::{PaymentRepo, RepoSource},
    service::{InvoiceRequest, RecordingEvents},
};

const DAY: i64 = 86_400_000;
/// BIP-84 test vector account key (public test data), re-serialised with `tpub` version bytes.
const BIP84_ZPUB: &str = "zpub6rFR7y4Q2AijBEqTUquhVz398htDFrtymD9xYYfG1m4wAcvPhXNfE3EfH1r1ADqtfSdVCToUG868RvUUkgDKf31mGDtKsAYz2oz2AGutZYs";

fn tpub() -> String {
    let mut raw = base58check_decode(BIP84_ZPUB).unwrap();
    raw[..4].copy_from_slice(&[0x04, 0x35, 0x87, 0xCF]);
    base58check_encode(&raw)
}

fn next_id() -> i64 {
    static N: AtomicI64 = AtomicI64::new(0);
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + N.fetch_add(1, Ordering::Relaxed) % 1000
}

async fn account(pool: &PgPool) -> i64 {
    let id = next_id();
    let name = format!("t{id}");
    assert!(
        accounts::create_account(
            pool,
            &NewAccount {
                id,
                person_id: next_id(),
                username: &name,
                display_name: None,
                email: &format!("{name}@example.test"),
                password_hash: "x",
                state: "active",
                email_verified: true,
                initial_credits: 0,
                flags: 0,
            },
            next_id(),
        )
        .await
        .unwrap()
    );
    id
}

async fn guild(pool: &PgPool, owner: i64, members: &[i64]) -> i64 {
    let id = next_id();
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Test Guild', $2)")
        .bind(id)
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
    for m in std::iter::once(&owner).chain(members) {
        sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
            .bind(id)
            .bind(m)
            .execute(pool)
            .await
            .unwrap();
    }
    id
}

struct Rig {
    _db: TestDb,
    pool: PgPool,
    repo: Arc<PgPaymentRepo>,
    svc: PaymentService,
    chain: FakeChain,
    events: Arc<RecordingEvents>,
    clock: Arc<AtomicI64>,
}

async fn rig(gap: u32) -> Rig {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let repo = Arc::new(PgPaymentRepo::new(pool.clone(), Arc::new(next_id)));
    let chain = FakeChain::new(100);
    let events = Arc::new(RecordingEvents::default());
    let clock = Arc::new(AtomicI64::new(1_700_000_000_000));
    let c = clock.clone();
    let now: Arc<dyn Fn() -> i64 + Send + Sync> = Arc::new(move || c.load(Ordering::SeqCst));
    let rates = Arc::new(RateService::new(
        vec![Arc::new(FakeRate::new("fake", 6_500_000))],
        RateSettings {
            cache_secs: 0,
            ..RateSettings::default()
        },
    ));
    let ents = Arc::new(EntitlementService::new(
        Arc::new(RepoSource(repo.clone())),
        now.clone(),
        Duration::from_millis(1),
    ));
    let svc = PaymentService::new(
        repo.clone(),
        Arc::new(chain.clone()),
        rates,
        events.clone(),
        ents,
        now,
        Arc::new(next_id),
    );
    let admin = account(&pool).await;
    svc.set_wallet(
        &format!("wpkh({}/0/*)", tpub()),
        Network::Signet,
        gap,
        admin,
    )
    .await
    .unwrap();
    Rig {
        _db: db,
        pool,
        repo,
        svc,
        chain,
        events,
        clock,
    }
}

fn patron(months: u32) -> InvoiceRequest {
    InvoiceRequest {
        purpose: Purpose::Patron,
        usd_cents: None,
        months: Some(months),
        guild_id: None,
        credits: None,
        anonymous: false,
    }
}

async fn poll(r: &Rig, id: i64) -> sr_payments::Invoice {
    let inv = r.repo.invoice(id).await.unwrap().unwrap();
    r.svc.poll_invoice(&inv).await.unwrap().invoice
}

async fn live_patron_grants(pool: &PgPool, user: i64) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM entitlement_grants WHERE kind = 'patron' AND user_id = $1 AND revoked_ms IS NULL",
    )
    .bind(user)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn lifecycle_credit_once_and_reorg_revert() {
    let r = rig(20).await;
    let user = account(&r.pool).await;
    let inv = r.svc.create_invoice(user, patron(2)).await.unwrap();
    assert_eq!(inv.usd_cents, 1000);
    assert!(inv.address.starts_with("tb1q"));
    assert_eq!(poll(&r, inv.id).await.state, InvoiceState::Open);

    r.chain.pay(&inv.address, "tx1aaaaaaaa", inv.sats as u64);
    assert_eq!(poll(&r, inv.id).await.state, InvoiceState::Seen);
    r.chain.confirm("tx1aaaaaaaa", 101);
    assert_eq!(poll(&r, inv.id).await.state, InvoiceState::Confirmed);
    for _ in 0..3 {
        poll(&r, inv.id).await;
    }
    assert_eq!(live_patron_grants(&r.pool, user).await, 1);
    let credits: i64 =
        sqlx::query_scalar("SELECT count(*) FROM payment_credits WHERE invoice_id = $1")
            .bind(inv.id)
            .fetch_one(&r.pool)
            .await
            .unwrap();
    assert_eq!(credits, 1);
    assert!(r.svc.entitlements().user(user).await.unwrap().patron.active);
    // Patron gets the download perk through the new source.
    let dl = EntitlementDownloads::new(r.svc.entitlements().clone(), None);
    assert_eq!(dl.allowance(user).await.unwrap().unwrap().per_day, 20);

    // Reorg: back to the mempool, then gone, then re-mined.
    r.chain.unconfirm("tx1aaaaaaaa");
    assert_eq!(poll(&r, inv.id).await.state, InvoiceState::Seen);
    assert_eq!(live_patron_grants(&r.pool, user).await, 0);
    r.svc.entitlements().invalidate_user(user).await;
    assert!(
        !r.svc
            .entitlements()
            .user(user)
            .await
            .unwrap()
            .patron
            .perks_on()
    );
    assert!(dl.allowance(user).await.unwrap().is_none());
    r.chain.drop_tx("tx1aaaaaaaa");
    assert_eq!(poll(&r, inv.id).await.state, InvoiceState::Open);
    r.chain
        .pay_confirmed(&inv.address, "tx1aaaaaaaa", inv.sats as u64, 105);
    r.chain.set_tip(105);
    assert_eq!(poll(&r, inv.id).await.state, InvoiceState::Confirmed);
    assert_eq!(live_patron_grants(&r.pool, user).await, 1);
    let txs = r.repo.invoice_txs(inv.id).await.unwrap();
    assert_eq!(txs.len(), 1);
    let audit: Vec<String> = r
        .repo
        .invoice_audit(inv.id)
        .await
        .unwrap()
        .into_iter()
        .map(|a| a.kind)
        .collect();
    for k in [
        "created",
        "tx_seen",
        "credited",
        "reverted",
        "tx_dropped",
        "state",
    ] {
        assert!(audit.iter().any(|a| a == k), "missing audit {k}: {audit:?}");
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn concurrent_pollers_credit_once() {
    let r = rig(20).await;
    let user = account(&r.pool).await;
    let inv = r.svc.create_invoice(user, patron(1)).await.unwrap();
    r.chain
        .pay_confirmed(&inv.address, "txconcurrent", inv.sats as u64, 101);
    r.chain.set_tip(101);
    let svc = Arc::new(r.svc);
    let mut handles = Vec::new();
    for _ in 0..6 {
        let svc = svc.clone();
        let repo = r.repo.clone();
        handles.push(tokio::spawn(async move {
            let inv = repo.invoice(inv.id).await.unwrap().unwrap();
            svc.poll_invoice(&inv).await.map(|_| ())
        }));
    }
    for h in handles {
        let _ = h.await.unwrap();
    }
    assert_eq!(live_patron_grants(&r.pool, user).await, 1);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn gap_limit_and_unique_addresses() {
    let r = rig(2).await;
    let (a, b, c) = (
        account(&r.pool).await,
        account(&r.pool).await,
        account(&r.pool).await,
    );
    let first = r.svc.create_invoice(a, patron(1)).await.unwrap();
    let second = r.svc.create_invoice(b, patron(1)).await.unwrap();
    assert_ne!(first.address, second.address);
    assert_eq!(
        r.svc.create_invoice(c, patron(1)).await.unwrap_err(),
        PayError::GapLimit
    );
    r.chain.pay(&first.address, "txgapaaaa", first.sats as u64);
    poll(&r, first.id).await;
    let third = r.svc.create_invoice(c, patron(1)).await.unwrap();
    assert_eq!(third.derivation_index, 2);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn invite_credits_late_and_resolution() {
    let r = rig(20).await;
    let user = account(&r.pool).await;
    let staff = account(&r.pool).await;
    let inv = r
        .svc
        .create_invoice(
            user,
            InvoiceRequest {
                purpose: Purpose::InviteCredit,
                usd_cents: None,
                months: None,
                guild_id: None,
                credits: Some(2),
                anonymous: false,
            },
        )
        .await
        .unwrap();
    let balance = |pool: PgPool| async move {
        sqlx::query_scalar::<_, Option<i64>>(
            "SELECT SUM(delta)::bigint FROM invite_credits_ledger WHERE account_id = $1",
        )
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap()
        .unwrap_or(0)
    };
    // Late: pay after expiry.
    r.clock.fetch_add(61 * 60_000, Ordering::SeqCst);
    assert_eq!(poll(&r, inv.id).await.state, InvoiceState::Expired);
    r.chain
        .pay_confirmed(&inv.address, "txlateaaaa", inv.sats as u64, 101);
    r.chain.set_tip(101);
    r.clock.fetch_add(60_000, Ordering::SeqCst);
    let late = poll(&r, inv.id).await;
    assert_eq!(late.state, InvoiceState::LatePaid);
    assert!(late.late);
    assert_eq!(balance(r.pool.clone()).await, 2);
    assert!(!r.events.alerts.lock().unwrap().is_empty());
    // Staff note a refund done outside the system: the credit is reversed, nothing is spent.
    let out = r
        .svc
        .resolve(
            inv.id,
            sr_payments::invoice::ResolutionKind::RefundNoted,
            "refunded",
            staff,
        )
        .await
        .unwrap();
    assert_eq!(out.invoice.state, InvoiceState::Resolved);
    assert_eq!(balance(r.pool.clone()).await, 0);
    assert!(matches!(
        r.svc
            .resolve(
                inv.id,
                sr_payments::invoice::ResolutionKind::MarkPaid,
                "x",
                staff
            )
            .await,
        Err(PayError::Conflict(_))
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn server_pool_level_grace_and_leaderboards() {
    let r = rig(20).await;
    let (owner, member, outsider) = (
        account(&r.pool).await,
        account(&r.pool).await,
        account(&r.pool).await,
    );
    let g = guild(&r.pool, owner, &[member]).await;
    let req = |cents| InvoiceRequest {
        purpose: Purpose::ServerLevel,
        usd_cents: Some(cents),
        months: None,
        guild_id: Some(g),
        credits: None,
        anonymous: false,
    };
    assert_eq!(
        r.svc.create_invoice(outsider, req(1000)).await.unwrap_err(),
        PayError::NotFound
    );
    let a = r.svc.create_invoice(owner, req(600)).await.unwrap();
    let mut anon_req = req(500);
    anon_req.anonymous = true;
    let b = r.svc.create_invoice(member, anon_req).await.unwrap();
    r.chain
        .pay_confirmed(&a.address, "txpoolaaaa", a.sats as u64, 101);
    r.chain
        .pay_confirmed(&b.address, "txpoolbbbb", b.sats as u64, 102);
    r.chain.set_tip(102);
    poll(&r, a.id).await;
    poll(&r, b.id).await;
    r.svc.entitlements().invalidate_guild(g).await;
    let e = r.svc.entitlements().guild(g).await.unwrap();
    assert_eq!((e.pool_cents, e.level), (1100, 1));
    let boost: i16 = sqlx::query_scalar("SELECT boost_level FROM guilds WHERE id = $1")
        .bind(g)
        .fetch_one(&r.pool)
        .await
        .unwrap();
    assert_eq!(boost, 1);
    assert_eq!(
        r.svc
            .entitlements()
            .limits(member, Some(g))
            .await
            .unwrap()
            .guild_level,
        1
    );
    assert_eq!(
        r.svc
            .entitlements()
            .limits(member, None)
            .await
            .unwrap()
            .guild_level,
        0
    );

    // Leaderboards: servers, contributors (the anonymous one is flagged), donors.
    let now = r.clock.load(Ordering::SeqCst);
    let servers = r
        .repo
        .leaderboard_servers(Some(now - 30 * DAY), 10)
        .await
        .unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].cents, 1100);
    let top = r
        .repo
        .top_contributors(g, now - 30 * DAY, 10)
        .await
        .unwrap();
    assert_eq!(top.len(), 2);
    assert!(
        top.iter()
            .find(|t| t.user_id == Some(member))
            .unwrap()
            .hide_name
    );
    assert!(
        !top.iter()
            .find(|t| t.user_id == Some(owner))
            .unwrap()
            .hide_name
    );
    let donors = r.repo.leaderboard_donors(None, 10).await.unwrap();
    assert_eq!(donors[0].user_id, Some(owner));
    assert_eq!(
        r.repo.donor_rank(member, None).await.unwrap(),
        Some((2, 500))
    );
    // Owner hides the server; staff hide a donor.
    r.repo.set_guild_hidden(g, true).await.unwrap();
    assert!(
        r.repo
            .leaderboard_servers(None, 10)
            .await
            .unwrap()
            .is_empty()
    );
    r.repo
        .set_staff_hidden(Some(owner), None, true)
        .await
        .unwrap();
    assert_eq!(r.repo.leaderboard_donors(None, 10).await.unwrap().len(), 1);
    assert_eq!(r.repo.donated_since(0).await.unwrap(), 1100);

    // 31 days later the pool is empty: held for the grace period, then dropped.
    r.clock.fetch_add(31 * DAY, Ordering::SeqCst);
    r.svc.housekeeping().await.unwrap();
    r.svc.entitlements().invalidate_guild(g).await;
    assert_eq!(r.svc.entitlements().guild(g).await.unwrap().level, 1);
    r.clock.fetch_add(4 * DAY, Ordering::SeqCst);
    assert_eq!(r.svc.housekeeping().await.unwrap().len(), 1);
    r.svc.entitlements().invalidate_guild(g).await;
    assert_eq!(r.svc.entitlements().guild(g).await.unwrap().level, 0);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn grants_config_prefs_and_constraints() {
    let r = rig(20).await;
    let user = account(&r.pool).await;
    let staff = account(&r.pool).await;
    // Staff grant, then revoke.
    let spec = sr_payments::repo::GrantSpec {
        kind: GrantKind::Patron,
        user_id: Some(user),
        guild_id: None,
        days: Some(10),
        usd_cents: None,
    };
    let grant = r
        .svc
        .admin_grant(spec, false, staff, "contest")
        .await
        .unwrap();
    assert!(r.svc.entitlements().user(user).await.unwrap().patron.active);
    let listed = r
        .repo
        .list_grants(&sr_payments::repo::GrantFilter {
            user_id: Some(user),
            active_only: true,
            limit: 10,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
    r.svc.admin_revoke(grant.id, staff, "oops").await.unwrap();
    assert!(matches!(
        r.svc.admin_revoke(grant.id, staff, "again").await,
        Err(PayError::Conflict(_))
    ));
    assert!(matches!(
        r.svc.admin_revoke(1, staff, "x").await,
        Err(PayError::NotFound)
    ));

    // Configuration round trip.
    let mut cfg = DonationConfig::default();
    cfg.patron_price_cents = 700;
    r.repo.set_config(&cfg, staff, 1).await.unwrap();
    assert_eq!(r.repo.config().await.unwrap().patron_price_cents, 700);
    r.svc.entitlements().invalidate_config().await;
    let inv = r.svc.create_invoice(user, patron(1)).await.unwrap();
    assert_eq!(inv.usd_cents, 700);

    // Preferences.
    r.repo
        .set_donor_prefs(
            user,
            sr_payments::repo::DonorPrefs {
                hide_name: true,
                hide_amount: false,
                hide_badge: true,
            },
        )
        .await
        .unwrap();
    assert!(r.repo.donor_prefs(user).await.unwrap().hide_badge);

    // The schema refuses private-key-looking descriptors and a second active wallet.
    let bad = sqlx::query(
        "INSERT INTO payment_wallets (id, network, descriptor, fingerprint, active) \
         VALUES ($1, 'mainnet', $2, '12345678', FALSE)",
    )
    .bind(next_id())
    .bind(format!("wpkh({}{}/0/*)", "xprv", "JUNKJUNKJUNK"))
    .execute(&r.pool)
    .await;
    assert!(bad.is_err());
    let second_active = sqlx::query(
        "INSERT INTO payment_wallets (id, network, descriptor, fingerprint, active) \
         VALUES ($1, 'mainnet', 'wpkh(xpubJUNK/0/*)', 'abcdef12', TRUE)",
    )
    .bind(next_id())
    .execute(&r.pool)
    .await;
    assert!(second_active.is_err());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn legacy_download_grants_cut_over() {
    let r = rig(20).await;
    let (old_user, expired_user, forever_user) = (
        account(&r.pool).await,
        account(&r.pool).await,
        account(&r.pool).await,
    );
    sqlx::query(
        "INSERT INTO cinema_download_grants (user_id, per_day, expires_at) VALUES \
         ($1, 5, now() + interval '10 days'), ($2, 5, now() - interval '1 day'), ($3, 50, NULL)",
    )
    .bind(old_user)
    .bind(expired_user)
    .bind(forever_user)
    .execute(&r.pool)
    .await
    .unwrap();
    // The legacy table alone still works through the fallback.
    let legacy = PgDownloadEntitlements::new(r.pool.clone());
    let both = EntitlementDownloads::new(r.svc.entitlements().clone(), Some(legacy));
    assert_eq!(both.allowance(old_user).await.unwrap().unwrap().per_day, 5);
    assert_eq!(
        both.allowance(forever_user).await.unwrap().unwrap().per_day,
        50
    );
    assert!(both.allowance(expired_user).await.unwrap().is_none());
    // Without the fallback they are not Patrons (the migration only ran on the empty table here).
    let new_only = EntitlementDownloads::new(r.svc.entitlements().clone(), None);
    assert!(new_only.allowance(old_user).await.unwrap().is_none());
    // Re-running the migration's INSERT copies the unexpired rows as Patron grants.
    let sql = include_str!("../migrations/0542_download_grants_cutover.sql");
    sqlx::raw_sql(sql).execute(&r.pool).await.unwrap();
    r.svc.entitlements().invalidate_user(old_user).await;
    r.svc.entitlements().invalidate_user(forever_user).await;
    r.svc.entitlements().invalidate_user(expired_user).await;
    assert_eq!(
        new_only.allowance(old_user).await.unwrap().unwrap().per_day,
        20
    );
    assert!(new_only.allowance(forever_user).await.unwrap().is_some());
    assert!(new_only.allowance(expired_user).await.unwrap().is_none());
}
