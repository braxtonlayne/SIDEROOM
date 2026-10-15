//! Endpoint tests over the in-memory payment repository, a fake chain and fake rates:
//! authentication matrices, the member flows, privacy of the boards, the staff flows and their
//! access-log trail. No database, no network, no real keys (the wallet is the public BIP-84
//! vector as a `tpub`).

use super::{BusPaymentEvents, DonationsDeps, router};
use crate::{
    ApiSettings, AppState,
    auth::{csrf, memory::MemorySessions},
    build_router,
    routes::admin::{AdminDeps, router as admin_router},
};
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sr_admin::{AccountState, AdminService, Caller, SYSTEM_USER_ID, memory::MemoryAdminRepo};
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_core::PlatformPermissions as P;
use sr_payments::{
    EntitlementService, PaymentService,
    chain::FakeChain,
    descriptor::{Network, base58check_decode, base58check_encode},
    memory::MemoryRepo,
    rates::{FakeRate, RateService, RateSettings},
    repo::{PaymentRepo, RepoSource},
};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};
use tower::ServiceExt;

const OWNER: i64 = 10;
const MONEY: i64 = 11;
const BASIC: i64 = 12;
const ALICE: i64 = 20;
const BOB: i64 = 21;
const CAROL: i64 = 22;
const GUILD: i64 = 700;

const BIP84_ZPUB: &str = "zpub6rFR7y4Q2AijBEqTUquhVz398htDFrtymD9xYYfG1m4wAcvPhXNfE3EfH1r1ADqtfSdVCToUG868RvUUkgDKf31mGDtKsAYz2oz2AGutZYs";

fn tpub() -> String {
    let mut raw = base58check_decode(BIP84_ZPUB).unwrap();
    raw[..4].copy_from_slice(&[0x04, 0x35, 0x87, 0xCF]);
    base58check_encode(&raw)
}

struct Env {
    app: Router,
    tokens: HashMap<i64, String>,
    chain: FakeChain,
    repo: Arc<MemoryRepo>,
    svc: Arc<PaymentService>,
    deps: Arc<DonationsDeps>,
}

async fn env(with_wallet: bool) -> Env {
    let admin_repo = Arc::new(MemoryAdminRepo::default());
    admin_repo.add_account(
        SYSTEM_USER_ID,
        "sideroom",
        AccountState::Deactivated,
        None,
        None,
    );
    admin_repo.add_account(OWNER, "owner", AccountState::Active, None, None);
    for (id, name) in [
        (MONEY, "money"),
        (BASIC, "basic"),
        (ALICE, "alice"),
        (BOB, "bob"),
        (CAROL, "carol"),
    ] {
        admin_repo.add_account(id, name, AccountState::Active, None, Some(OWNER));
    }
    admin_repo.add_role(1, "Owner", 3, P::OWNER);
    admin_repo.add_role(2, "Treasurer", 2, P::STAFF_BASIC | P::MANAGE_DONATIONS);
    admin_repo.add_role(3, "Basic", 1, P::STAFF_BASIC);
    admin_repo.set_owner(OWNER);
    let admin_svc = AdminService::new(admin_repo.clone());
    let op = Caller::operator(SYSTEM_USER_ID);
    admin_svc
        .appoint(&op, MONEY, &[2], Some("test"))
        .await
        .unwrap();
    admin_svc
        .appoint(&op, BASIC, &[3], Some("test"))
        .await
        .unwrap();

    let repo = Arc::new(MemoryRepo::new());
    repo.add_member(ALICE, GUILD, true);
    repo.add_member(BOB, GUILD, false);
    repo.add_profile(ALICE, "Alice");
    repo.add_profile(BOB, "Bob");
    repo.add_profile(CAROL, "Carol");
    let chain = FakeChain::new(100);
    let clock = Arc::new(AtomicI64::new(super::now_ms()));
    let now: Arc<dyn Fn() -> i64 + Send + Sync> = {
        let c = clock.clone();
        Arc::new(move || c.load(Ordering::SeqCst))
    };
    let rates = Arc::new(RateService::new(
        vec![Arc::new(FakeRate::new("fake", 6_500_000))],
        RateSettings::default(),
    ));
    let ents = Arc::new(EntitlementService::new(
        Arc::new(RepoSource(repo.clone())),
        now.clone(),
        Duration::from_millis(1),
    ));
    let next = Arc::new(AtomicI64::new(5000));
    let bus: Arc<dyn sr_events::EventBus> = Arc::new(sr_events::InProcessBus::new());
    let svc = Arc::new(PaymentService::new(
        repo.clone(),
        Arc::new(chain.clone()),
        rates,
        Arc::new(BusPaymentEvents::new(bus)),
        ents,
        now,
        Arc::new(move || next.fetch_add(1, Ordering::SeqCst)),
    ));
    if with_wallet {
        svc.set_wallet(&format!("wpkh({}/0/*)", tpub()), Network::Signet, 20, OWNER)
            .await
            .unwrap();
    }
    let deps = Arc::new(DonationsDeps::new(svc.clone()));

    let sessions = Arc::new(MemorySessions::new());
    let mut tokens = HashMap::new();
    for (n, uid) in [OWNER, MONEY, BASIC, ALICE, BOB, CAROL]
        .into_iter()
        .enumerate()
    {
        let t = SessionToken::generate().unwrap();
        sessions
            .create(SessionRecord {
                id: 100 + n as i64,
                user_id: uid,
                token_hash: t.hash(),
                created_at: 0,
                last_seen_at: i64::MAX / 4,
                expires_at: i64::MAX / 4,
                device_name: None,
            })
            .await
            .unwrap();
        tokens.insert(uid, t.expose().to_owned());
    }
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(settings)
        .sessions(sessions)
        .extension(deps.clone())
        .extension(Arc::new(AdminDeps::new(admin_svc)))
        .build();
    Env {
        app: build_router(state, router().merge(admin_router())),
        tokens,
        chain,
        repo,
        svc,
        deps,
    }
}

impl Env {
    async fn call(
        &self,
        who: Option<i64>,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method.clone())
            .uri(format!("/api/v1{path}"))
            .header("origin", "http://localhost:8080");
        if let Some(uid) = who {
            let raw = &self.tokens[&uid];
            req = req.header("cookie", format!("sr_session={raw}"));
            if method != Method::GET {
                req = req.header("x-sr-csrf", csrf::token_for(raw));
            }
        }
        let body = match body {
            Some(b) => {
                req = req.header("content-type", "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        let resp = self
            .app
            .clone()
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn get(&self, who: i64, path: &str) -> (StatusCode, Value) {
        self.call(Some(who), Method::GET, path, None).await
    }

    async fn post(&self, who: i64, path: &str, body: Value) -> (StatusCode, Value) {
        self.call(Some(who), Method::POST, path, Some(body)).await
    }

    /// Credit an invoice by paying it on the fake chain and polling.
    async fn pay(&self, invoice: &Value, txid: &str, height: u32) {
        let id: i64 = invoice["id"].as_str().unwrap().parse().unwrap();
        self.chain.pay_confirmed(
            invoice["address"].as_str().unwrap(),
            txid,
            invoice["sats"].as_i64().unwrap() as u64,
            height,
        );
        self.chain.set_tip(height);
        let inv = self.repo.invoice(id).await.unwrap().unwrap();
        self.svc.poll_invoice(&inv).await.unwrap();
    }
}

fn patron(months: u32) -> Value {
    json!({ "purpose": "patron", "months": months })
}

type Call = (Method, &'static str, Option<Value>);

#[tokio::test]
async fn every_route_needs_a_session_and_admin_routes_need_the_permission() {
    let e = env(true).await;
    let member: Vec<Call> = vec![
        (Method::GET, "/donations/config", None),
        (Method::POST, "/donations/invoices", Some(patron(1))),
        (Method::GET, "/donations/invoices", None),
        (Method::GET, "/donations/invoices/1", None),
        (Method::POST, "/donations/invoices/1/check", None),
        (Method::POST, "/donations/invoices/1/cancel", None),
        (Method::GET, "/donations/me", None),
        (Method::PATCH, "/donations/me/preferences", Some(json!({}))),
        (Method::GET, "/guilds/700/support", None),
        (
            Method::PATCH,
            "/guilds/700/support",
            Some(json!({"hidden_from_leaderboard": true})),
        ),
        (Method::GET, "/donations/leaderboards/donors", None),
        (Method::GET, "/donations/leaderboards/contributors/movies", None),
    ];
    for (m, p, b) in &member {
        let (s, _) = e.call(None, m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{m} {p}");
    }
    let staff: Vec<Call> = vec![
        (Method::GET, "/admin/donations/status", None),
        (
            Method::PUT,
            "/admin/donations/wallet",
            Some(json!({"descriptor": "x", "network": "signet", "reason": "r"})),
        ),
        (
            Method::POST,
            "/admin/donations/wallet/test",
            Some(json!({"descriptor": "x", "network": "signet"})),
        ),
        (Method::GET, "/admin/donations/config", None),
        (
            Method::PATCH,
            "/admin/donations/config",
            Some(json!({"changes": {}, "reason": "r"})),
        ),
        (Method::GET, "/admin/donations/invoices", None),
        (Method::GET, "/admin/donations/invoices/1", None),
        (
            Method::POST,
            "/admin/donations/invoices/1/resolve",
            Some(json!({"resolution": "dismiss", "note": "n", "reason": "r"})),
        ),
        (Method::GET, "/admin/donations/entitlements", None),
        (
            Method::POST,
            "/admin/donations/entitlements",
            Some(json!({"kind": "patron", "user_id": "20", "days": 3, "reason": "r"})),
        ),
        (Method::DELETE, "/admin/donations/entitlements/1", None),
        (Method::GET, "/admin/donations/rates", None),
        (Method::GET, "/admin/donations/report", None),
        (Method::GET, "/admin/donations/leaderboard/donors", None),
        (
            Method::POST,
            "/admin/donations/leaderboard/hide",
            Some(json!({"user_id": "20", "hidden": true, "reason": "r"})),
        ),
    ];
    for (m, p, b) in &staff {
        let (s, _) = e.call(None, m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{m} {p}");
        let (s, _) = e.call(Some(ALICE), m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "member {m} {p}");
        let (s, body) = e.call(Some(BASIC), m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "basic staff {m} {p}: {body}");
    }
}

#[tokio::test]
async fn config_reports_tiers_rate_and_the_hosting_bar() {
    let e = env(true).await;
    let (s, c) = e.get(ALICE, "/donations/config").await;
    assert_eq!(s, StatusCode::OK, "{c}");
    assert_eq!(c["enabled"], true);
    assert_eq!(c["network"], "signet");
    assert_eq!(c["patron"]["price_cents"], 500);
    assert_eq!(c["levels"].as_array().unwrap().len(), 3);
    assert_eq!(c["levels"][1]["threshold_cents"], 2500);
    assert_eq!(c["rate"]["cents_per_btc"], 6_500_000);
    assert_eq!(c["hosting"]["cost_cents"], 12800);
    assert_eq!(c["hosting"]["donated_this_month_cents"], 0);
    assert_eq!(c["free_perks"]["upload_mib"], 30);

    // Without a wallet donations are off and creating an invoice says so.
    let off = env(false).await;
    let (_, c) = off.get(ALICE, "/donations/config").await;
    assert_eq!(c["enabled"], false);
    let (s, body) = off.post(ALICE, "/donations/invoices", patron(1)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], 900001);
}

#[tokio::test]
async fn invoice_flow_create_check_pay_receipt_and_privacy() {
    let e = env(true).await;
    let (s, inv) = e.post(ALICE, "/donations/invoices", patron(3)).await;
    assert_eq!(s, StatusCode::CREATED, "{inv}");
    assert_eq!(inv["state"], "open");
    assert_eq!(inv["usd_cents"], 1500);
    assert!(inv["address"].as_str().unwrap().starts_with("tb1q"));
    assert!(inv["bip21"].as_str().unwrap().starts_with("bitcoin:tb1q"));
    assert!(inv["bip21"].as_str().unwrap().contains("amount="));
    assert_eq!(inv["rate"]["cents_per_btc"], 6_500_000);
    let id = inv["id"].as_str().unwrap().to_owned();

    // Others cannot see it.
    let (s, _) = e.get(BOB, &format!("/donations/invoices/{id}")).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = e
        .post(BOB, &format!("/donations/invoices/{id}/cancel"), json!({}))
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Mempool: "I've paid" sees it.
    e.chain.pay(
        inv["address"].as_str().unwrap(),
        "feedface0001",
        inv["sats"].as_u64().unwrap(),
    );
    let (s, seen) = e
        .post(ALICE, &format!("/donations/invoices/{id}/check"), json!({}))
        .await;
    assert_eq!(s, StatusCode::OK, "{seen}");
    assert_eq!(seen["state"], "seen");
    assert_eq!(seen["txs"][0]["txid"], "feedface0001");
    // A paid invoice cannot be cancelled.
    let (s, _) = e
        .post(
            ALICE,
            &format!("/donations/invoices/{id}/cancel"),
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);

    // Confirmed through the watcher; the receipt and the entitlement follow.
    e.chain.confirm("feedface0001", 101);
    let inv_id: i64 = id.parse().unwrap();
    let stored = e.repo.invoice(inv_id).await.unwrap().unwrap();
    e.svc.poll_invoice(&stored).await.unwrap();
    let (_, done) = e.get(ALICE, &format!("/donations/invoices/{id}")).await;
    assert_eq!(done["state"], "confirmed");
    assert!(done["credited_at_ms"].is_i64());
    assert!(
        done["explorer_url"]
            .as_str()
            .unwrap()
            .contains("feedface0001")
    );
    let (_, me) = e.get(ALICE, "/donations/me").await;
    assert_eq!(me["patron"]["active"], true);
    assert_eq!(me["limits"]["patron"], true);
    assert_eq!(me["limits"]["downloads_per_day"], 20);
    assert_eq!(me["preferences"]["hide_name"], false);

    let (_, list) = e.get(ALICE, "/donations/invoices").await;
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert_eq!(list["items"][0]["id"], id);
    let (_, none) = e.get(BOB, "/donations/invoices").await;
    assert!(none["items"].as_array().unwrap().is_empty());

    // Cancel an unpaid one.
    let (_, second) = e.post(ALICE, "/donations/invoices", patron(1)).await;
    let (s, c) = e
        .post(
            ALICE,
            &format!(
                "/donations/invoices/{}/cancel",
                second["id"].as_str().unwrap()
            ),
            json!({}),
        )
        .await;
    assert_eq!(
        (s, c["state"].as_str()),
        (StatusCode::OK, Some("cancelled"))
    );
}

#[tokio::test]
async fn invoice_validation() {
    let e = env(true).await;
    for body in [
        json!({ "purpose": "nonsense" }),
        json!({ "purpose": "patron" }),
        json!({ "purpose": "patron", "months": 13 }),
        json!({ "purpose": "donate", "usd_cents": 5 }),
        json!({ "purpose": "donate" }),
        json!({ "purpose": "invite_credit", "credits": 0 }),
        json!({ "purpose": "server_level", "usd_cents": 1000 }),
        json!({ "purpose": "donate", "usd_cents": 500, "guild_id": "700" }),
    ] {
        let (s, resp) = e.post(ALICE, "/donations/invoices", body.clone()).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{body} -> {resp}");
    }
    // Contributing to a server you are not in looks like an unknown server.
    let (s, _) = e
        .post(
            CAROL,
            "/donations/invoices",
            json!({ "purpose": "server_level", "usd_cents": 1000, "guild_id": "700" }),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // Invite credits at the configured price.
    let (s, inv) = e
        .post(
            ALICE,
            "/donations/invoices",
            json!({ "purpose": "invite_credit", "credits": 2 }),
        )
        .await;
    assert_eq!(
        (s, inv["usd_cents"].as_i64()),
        (StatusCode::CREATED, Some(4000))
    );
    // More than ten open invoices is refused.
    for _ in 0..10 {
        let (s, _) = e
            .post(
                BOB,
                "/donations/invoices",
                json!({ "purpose": "donate", "usd_cents": 300 }),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED);
    }
    let (s, _) = e
        .post(
            BOB,
            "/donations/invoices",
            json!({ "purpose": "donate", "usd_cents": 300 }),
        )
        .await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn preferences_server_support_and_leaderboards_respect_privacy() {
    let e = env(true).await;
    // Alice (owner) gives $12 openly, Bob gives $6 anonymously; Carol donates $20.
    let (_, a) = e
        .post(
            ALICE,
            "/donations/invoices",
            json!({ "purpose": "server_level", "usd_cents": 1200, "guild_id": "700" }),
        )
        .await;
    let (_, b) = e
        .post(
            BOB,
            "/donations/invoices",
            json!({ "purpose": "server_level", "usd_cents": 600, "guild_id": "700", "anonymous": true }),
        )
        .await;
    let (_, c) = e
        .post(
            CAROL,
            "/donations/invoices",
            json!({ "purpose": "donate", "usd_cents": 2000 }),
        )
        .await;
    e.pay(&a, "aaaa0000aaaa", 101).await;
    e.pay(&b, "bbbb0000bbbb", 102).await;
    e.pay(&c, "cccc0000cccc", 103).await;

    // Server support: level 1 ($18 >= $10), contributors with Bob anonymised.
    let (s, g) = e.get(BOB, "/guilds/700/support").await;
    assert_eq!(s, StatusCode::OK, "{g}");
    assert_eq!(g["level"], 1);
    assert_eq!(g["pool_cents"], 1800);
    assert_eq!(g["next_threshold_cents"], 2500);
    assert_eq!(g["perks"]["video_senders"], 8);
    assert_eq!(g["is_owner"], false);
    let top = g["top_contributors"].as_array().unwrap();
    assert_eq!(top[0]["name"], "Alice");
    assert_eq!(top[1]["name"], "Anonymous");
    assert!(top[1]["user_id"].is_null());
    // Outsiders do not see it; only the owner can hide the server.
    assert_eq!(
        e.get(CAROL, "/guilds/700/support").await.0,
        StatusCode::NOT_FOUND
    );
    let hide = json!({ "hidden_from_leaderboard": true });
    assert_eq!(
        e.call(
            Some(BOB),
            Method::PATCH,
            "/guilds/700/support",
            Some(hide.clone())
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );

    // Donor board: Carol $20, Alice $12, Bob $6.
    let (s, board) = e
        .get(ALICE, "/donations/leaderboards/donors?window=lifetime")
        .await;
    assert_eq!(s, StatusCode::OK, "{board}");
    let names: Vec<_> = board["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["Carol", "Alice", "Bob"]);
    assert_eq!(board["me"]["rank"], 2);
    assert_eq!(board["items"][0]["cents"], 2000);

    // Carol hides her name, Bob hides his amount.
    let (s, p) = e
        .call(
            Some(CAROL),
            Method::PATCH,
            "/donations/me/preferences",
            Some(json!({ "hide_name": true })),
        )
        .await;
    assert_eq!((s, p["hide_name"].as_bool()), (StatusCode::OK, Some(true)));
    e.call(
        Some(BOB),
        Method::PATCH,
        "/donations/me/preferences",
        Some(json!({ "hide_amount": true })),
    )
    .await;
    // The page is cached for an hour: a change shows up once the cache is dropped.
    e.deps.boards.invalidate_all();
    let (_, board) = e
        .get(ALICE, "/donations/leaderboards/donors?window=lifetime")
        .await;
    let items = board["items"].as_array().unwrap();
    assert_eq!(items[0]["name"], "Anonymous patron");
    assert!(items[0]["user_id"].is_null() && items[0]["avatar"].is_null());
    assert_eq!(items[0]["cents"], 2000, "only the name is hidden");
    assert_eq!(items[2]["name"], "Bob");
    assert!(items[2]["cents"].is_null(), "the hidden amount is not sent");
    assert!(
        !board.to_string().contains("\"22\""),
        "Carol's id never appears"
    );
    let (_, limited) = e.get(ALICE, "/donations/leaderboards/donors?limit=1").await;
    assert_eq!(limited["items"].as_array().unwrap().len(), 1);
    // Servers board and the owner hiding the server.
    let (_, servers) = e.get(ALICE, "/donations/leaderboards/servers").await;
    assert_eq!(servers["items"][0]["guild_id"], "700");
    assert_eq!(servers["items"][0]["cents"], 1800);
    let (s, _) = e
        .call(
            Some(ALICE),
            Method::PATCH,
            "/guilds/700/support",
            Some(hide),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    e.deps.boards.invalidate_all();
    let (_, servers) = e.get(ALICE, "/donations/leaderboards/servers").await;
    assert!(servers["items"].as_array().unwrap().is_empty());
    // Unknown board / window.
    assert_eq!(
        e.get(ALICE, "/donations/leaderboards/nope").await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        e.get(ALICE, "/donations/leaderboards/donors?window=7d")
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn contributor_boards_rank_cache_and_honour_the_opt_out() {
    use sr_payments::repo::ContributionKind as K;
    let e = env(true).await;
    e.repo.add_contributions(ALICE, K::Movies, 3);
    e.repo.add_contributions(BOB, K::Movies, 5);
    e.repo.add_contributions(CAROL, K::Movies, 5);
    e.repo.add_contributions(ALICE, K::Chapters, 40);
    let (s, b) = e
        .get(ALICE, "/donations/leaderboards/contributors/movies")
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let names: Vec<_> = b["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| (i["name"].as_str().unwrap(), i["count"].as_i64().unwrap()))
        .collect();
    assert_eq!(names, [("Bob", 5), ("Carol", 5), ("Alice", 3)], "ties by account id");
    assert_eq!(b["me"]["rank"], 3);
    assert_eq!(b["kind"], "movies");
    let (_, ch) = e
        .get(ALICE, "/donations/leaderboards/contributors/chapters?limit=1")
        .await;
    assert_eq!(ch["items"][0]["count"], 40);
    assert_eq!(ch["me"]["rank"], 1);
    let (_, ep) = e
        .get(ALICE, "/donations/leaderboards/contributors/episodes")
        .await;
    assert!(ep["items"].as_array().unwrap().is_empty());
    assert!(ep["me"].is_null());
    // Bob opts out: he leaves the board at once (the cache is dropped on a change).
    let (s, p) = e
        .call(
            Some(BOB),
            Method::PATCH,
            "/donations/me/preferences",
            Some(json!({"hide_contributions": true})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{p}");
    assert_eq!(p["hide_contributions"], true);
    let (_, me) = e.get(BOB, "/donations/me").await;
    assert_eq!(me["preferences"]["hide_contributions"], true);
    let (_, b2) = e
        .get(BOB, "/donations/leaderboards/contributors/movies")
        .await;
    assert!(b2["me"].is_null(), "an opted-out account has no rank");
    assert!(
        b2["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["name"] != "Bob")
    );
    assert_eq!(
        e.get(ALICE, "/donations/leaderboards/contributors/nope").await.0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn staff_wallet_config_invoices_resolution_and_grants() {
    let e = env(false).await;
    // The wallet: a private key is refused, a watch-only key is accepted and never echoed back.
    let secret = format!("wpkh({}{}/0/*)", "xprv", "JUNKJUNKJUNKJUNK");
    let (s, body) = e
        .call(
            Some(MONEY),
            Method::PUT,
            "/admin/donations/wallet",
            Some(json!({ "descriptor": secret, "network": "mainnet", "reason": "setup" })),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(
        !body.to_string().contains("JUNKJUNK"),
        "the refusal does not echo the input"
    );
    let desc = format!("wpkh({}/0/*)", tpub());
    let (s, t) = e
        .post(
            MONEY,
            "/admin/donations/wallet/test",
            json!({ "descriptor": desc, "network": "signet" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{t}");
    assert_eq!(t["addresses"].as_array().unwrap().len(), 5);
    assert_eq!(
        e.call(
            Some(MONEY),
            Method::PUT,
            "/admin/donations/wallet",
            Some(json!({ "descriptor": desc, "network": "signet" }))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST,
        "a reason is required"
    );
    let (s, w) = e
        .call(
            Some(MONEY),
            Method::PUT,
            "/admin/donations/wallet",
            Some(json!({ "descriptor": desc, "network": "signet", "reason": "initial setup", "gap_limit": 5 })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{w}");
    assert_eq!(w["addresses"].as_array().unwrap().len(), 3);
    let (s, st) = e.get(MONEY, "/admin/donations/status").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(st["enabled"], true);
    assert_eq!(st["wallet"]["fingerprint"], w["fingerprint"]);
    assert_eq!(st["wallet"]["gap_limit"], 5);
    let text = st.to_string();
    assert!(
        !text.contains(&tpub()) && !text.contains("wpkh"),
        "status never shows the descriptor"
    );

    // Configuration: validated, reason required, applied at once.
    let patch =
        |changes: Value, reason: Option<&str>| json!({ "changes": changes, "reason": reason });
    let (s, _) = e
        .call(
            Some(MONEY),
            Method::PATCH,
            "/admin/donations/config",
            Some(patch(json!({ "patron_price_cents": 700 }), None)),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    for bad in [
        json!({ "tolerance_bps": 9999 }),
        json!({ "no_such_field": 1, "grace_days": "x" }),
        json!({ "levels": [null, null, null] }),
    ] {
        let (s, resp) = e
            .call(
                Some(MONEY),
                Method::PATCH,
                "/admin/donations/config",
                Some(patch(bad.clone(), Some("x"))),
            )
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}: {resp}");
    }
    let (s, cfg) = e
        .call(
            Some(MONEY),
            Method::PATCH,
            "/admin/donations/config",
            Some(patch(
                json!({ "patron_price_cents": 700, "free": { "upload_mib": 25 } }),
                Some("price change"),
            )),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{cfg}");
    assert_eq!(cfg["patron_price_cents"], 700);
    assert_eq!(cfg["free"]["upload_mib"], 25);
    assert_eq!(
        cfg["free"]["audio_kbps"], 96,
        "a merge patch keeps the other fields"
    );
    let (_, c) = e.get(ALICE, "/donations/config").await;
    assert_eq!(c["patron"]["price_cents"], 700);

    // An underpaid invoice shows up for staff; detail is access-logged; resolution credits once.
    let (_, inv) = e.post(ALICE, "/donations/invoices", patron(1)).await;
    assert_eq!(inv["usd_cents"], 700);
    let id = inv["id"].as_str().unwrap().to_owned();
    e.chain.pay_confirmed(
        inv["address"].as_str().unwrap(),
        "dead0000beef",
        inv["sats"].as_u64().unwrap() / 2,
        101,
    );
    e.chain.set_tip(101);
    e.svc
        .poll_invoice(&e.repo.invoice(id.parse().unwrap()).await.unwrap().unwrap())
        .await
        .unwrap();
    let (s, list) = e
        .get(MONEY, "/admin/donations/invoices?needs_attention=true")
        .await;
    assert_eq!(s, StatusCode::OK, "{list}");
    assert_eq!(list["items"][0]["state"], "underpaid");
    assert_eq!(list["items"][0]["user_name"], "Alice");
    let (_, found) = e.get(MONEY, "/admin/donations/invoices?q=dead0000").await;
    assert_eq!(found["items"].as_array().unwrap().len(), 1);
    let (_, none) = e
        .get(MONEY, "/admin/donations/invoices?state=confirmed")
        .await;
    assert!(none["items"].as_array().unwrap().is_empty());
    assert_eq!(
        e.get(MONEY, "/admin/donations/invoices?state=bogus")
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let (s, d) = e
        .get(MONEY, &format!("/admin/donations/invoices/{id}"))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert!(
        d["audit"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["kind"] == "created")
    );
    assert_eq!(d["derivation_index"], 0);
    let resolve = |res: &str, note: &str, reason: Option<&str>| json!({ "resolution": res, "note": note, "reason": reason });
    let path = format!("/admin/donations/invoices/{id}/resolve");
    assert_eq!(
        e.post(MONEY, &path, resolve("mark_paid", "", Some("r")))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        e.post(MONEY, &path, resolve("mark_paid", "n", None))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        e.post(MONEY, &path, resolve("bogus", "n", Some("r")))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let (s, r) = e
        .post(
            MONEY,
            &path,
            resolve("mark_paid", "rest paid off-chain", Some("ticket 12")),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["state"], "resolved");
    assert_eq!(r["resolution"], "mark_paid");
    assert_eq!(r["resolution_note"], "rest paid off-chain");
    assert_eq!(
        e.post(MONEY, &path, resolve("dismiss", "n", Some("r")))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let (_, me) = e.get(ALICE, "/donations/me").await;
    assert_eq!(me["patron"]["active"], true);

    // Grants: list, grant, revoke.
    let (s, g) = e
        .post(
            MONEY,
            "/admin/donations/entitlements",
            json!({ "kind": "patron", "user_id": "21", "days": 14, "reason": "giveaway" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{g}");
    let gid = g["id"].as_str().unwrap().to_owned();
    assert_eq!(g["source"], "admin");
    assert_eq!(
        e.post(
            MONEY,
            "/admin/donations/entitlements",
            json!({ "kind": "patron", "user_id": "21", "reason": "r" })
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        e.post(
            MONEY,
            "/admin/donations/entitlements",
            json!({ "kind": "patron", "user_id": "21", "days": 5 })
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let (_, bob) = e.get(BOB, "/donations/me").await;
    assert_eq!(bob["patron"]["active"], true);
    let (_, lst) = e
        .get(
            MONEY,
            "/admin/donations/entitlements?kind=patron&active=true",
        )
        .await;
    assert_eq!(lst["items"].as_array().unwrap().len(), 2);
    let gpath = format!("/admin/donations/entitlements/{gid}");
    let (s, _) = e.call(Some(MONEY), Method::DELETE, &gpath, None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "revocation needs a reason");
    let (s, r) = e
        .call(
            Some(MONEY),
            Method::DELETE,
            &gpath,
            Some(json!({ "reason": "mistake" })),
        )
        .await;
    assert_eq!((s, r["revoked_at_ms"].is_i64()), (StatusCode::OK, true));
    assert_eq!(
        e.call(
            Some(MONEY),
            Method::DELETE,
            &gpath,
            Some(json!({ "reason": "again" }))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );

    // Rates, report, leaderboard moderation.
    let (s, rates) = e.get(MONEY, "/admin/donations/rates").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(rates["sources"][0]["name"], "fake");
    let (s, rep) = e.get(MONEY, "/admin/donations/report").await;
    assert_eq!(s, StatusCode::OK, "{rep}");
    assert_eq!(rep["credited_cents"], 700);
    assert_eq!(rep["by_purpose"][0]["purpose"], "patron");
    assert_eq!(
        e.get(MONEY, "/admin/donations/report?from_ms=10&to_ms=5")
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let (s, board) = e
        .get(MONEY, "/admin/donations/leaderboard/donors?window=lifetime")
        .await;
    assert_eq!(s, StatusCode::OK, "{board}");
    assert_eq!(board["items"][0]["name"], "Alice");
    let h = |hidden: bool| json!({ "user_id": "20", "hidden": hidden, "reason": "spam name" });
    assert_eq!(
        e.post(MONEY, "/admin/donations/leaderboard/hide", h(true))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    let (_, board) = e
        .get(MONEY, "/admin/donations/leaderboard/donors?window=lifetime")
        .await;
    assert!(board["items"].as_array().unwrap().is_empty());
    assert_eq!(
        e.post(
            MONEY,
            "/admin/donations/leaderboard/hide",
            json!({ "hidden": true, "reason": "r" })
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );

    // The access log carries the trail of the writes and the reads.
    let (s, log) = e.get(OWNER, "/admin/access-log?limit=100").await;
    assert_eq!(s, StatusCode::OK, "{log}");
    let actions: Vec<String> = log["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["action"].as_str().unwrap().to_owned())
        .collect();
    for a in [
        "donations.wallet.set",
        "donations.config.set",
        "donations.invoice.view",
        "donations.invoice.resolve",
        "donations.entitlement.grant",
        "donations.entitlement.revoke",
        "donations.leaderboard.hide",
    ] {
        assert!(actions.iter().any(|x| x == a), "missing {a} in {actions:?}");
    }
    assert!(
        !log.to_string().contains(&tpub()),
        "the log never holds the descriptor"
    );
}
