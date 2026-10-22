//! Endpoint tests through the full router (`tower::oneshot`) with in-memory
//! stubs.

use super::{
    AccountRecord, AccountState, AuthConfig, AuthService, AuthStore,
    mail::{MemoryMailer, OutgoingMail},
    memory::MemoryAuthStore,
};
use crate::{ApiSettings, AppState, auth::csrf, auth::memory::MemorySessions, build_router};
use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Method, Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sr_auth::{
    invite::InviteCode,
    password::{PasswordHasher, PasswordParams},
    totp::{self, TotpSecret},
};
use sr_core::ManualClock;
use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};
use tower::ServiceExt;

const NOW_MS: i64 = 1_800_000_000_000;
const PW: &str = "correct horse battery staple";
const NEW_PW: &str = "a brand new long passphrase 42";
const UA_FIREFOX: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:128.0) Gecko/20100101 Firefox/128.0";
const UA_CHROME: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/120 Safari/537.36";

fn cheap_hasher() -> PasswordHasher {
    PasswordHasher::new(PasswordParams {
        m_cost_kib: 64,
        t_cost: 1,
        p_cost: 1,
    })
    .unwrap()
}

struct H {
    app: Router,
    state: AppState,
    store: Arc<MemoryAuthStore>,
    mailer: Arc<MemoryMailer>,
    clock: Arc<ManualClock>,
    hasher: PasswordHasher,
    bus: Arc<sr_events::InProcessBus>,
}

fn harness_with(cap: u64) -> H {
    let sessions = Arc::new(MemorySessions::new());
    let clock = Arc::new(ManualClock::new(NOW_MS));
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let store = Arc::new(MemoryAuthStore::new(sessions.clone()));
    let mailer = Arc::new(MemoryMailer::new());
    let next = Arc::new(AtomicI64::new(10_000));
    let ids: super::IdSource = Arc::new(move || next.fetch_add(1, Ordering::SeqCst));
    let bus = Arc::new(sr_events::InProcessBus::new());
    let svc = Arc::new(
        AuthService::new(store.clone(), mailer.clone(), ids)
            .with_bus(bus.clone())
            .with_hasher(cheap_hasher())
            .with_config(AuthConfig {
                account_cap: cap,
                trust_forwarded_for: true,
                ..AuthConfig::default()
            }),
    );
    let state = AppState::builder()
        .sessions(sessions)
        .clock(clock.clone())
        .settings(settings)
        .extension(svc)
        .build();
    let app = build_router(state.clone(), super::router());
    H {
        app,
        state,
        store,
        mailer,
        clock,
        hasher: cheap_hasher(),
        bus,
    }
}

fn harness() -> H {
    harness_with(20_000)
}

struct Resp {
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
}

impl Resp {
    /// Value of the last `Set-Cookie` for `name`.
    fn cookie(&self, name: &str) -> Option<String> {
        self.headers
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .rev()
            .find_map(|c| {
                let (k, rest) = c.split_once('=')?;
                (k == name).then(|| rest.split(';').next().unwrap_or("").to_owned())
            })
    }
}

struct Call<'a> {
    h: &'a H,
    method: Method,
    uri: String,
    body: Option<Value>,
    cookies: Vec<String>,
    csrf: Option<String>,
    ip: String,
    origin: Option<String>,
    ua: Option<String>,
}

impl H {
    fn call(&self, method: Method, uri: &str) -> Call<'_> {
        Call {
            h: self,
            method,
            uri: uri.to_owned(),
            body: None,
            cookies: vec![],
            csrf: None,
            ip: "198.51.100.1".to_owned(),
            origin: None,
            ua: None,
        }
    }
    fn get(&self, uri: &str) -> Call<'_> {
        self.call(Method::GET, uri)
    }
    fn post(&self, uri: &str, body: Value) -> Call<'_> {
        self.call(Method::POST, uri).json(body)
    }

    fn add_account(&self, id: i64, username: &str, email: &str, state: AccountState) {
        self.store.add_account(AccountRecord {
            id,
            person_id: id,
            username: username.to_owned(),
            display_name: None,
            email: email.to_owned(),
            email_verified: true,
            password_hash: self.hasher.hash(PW).unwrap(),
            state,
            age_confirmed: None,
            age_confirmed_at: None,
            locale: "en-US".into(),
            flags: 0,
        });
    }

    /// An issuer with `credits` credits and one open invite.
    fn issuer_with_invite(&self, id: i64, credits: i32) -> InviteCode {
        self.add_account(
            id,
            &format!("issuer{id}"),
            &format!("issuer{id}@x.test"),
            AccountState::Active,
        );
        self.store.grant(id, credits, "initial_grant");
        let code = InviteCode::generate().unwrap();
        self.store.add_invite(&code, id);
        code
    }

    fn now(&self) -> u64 {
        u64::try_from(self.state.now_secs()).unwrap()
    }

    fn account(&self, id: i64) -> AccountRecord {
        self.store.account(id).unwrap()
    }

    async fn wait_mail(&self, n: usize) -> Vec<OutgoingMail> {
        for _ in 0..200 {
            let sent = self.mailer.sent();
            if sent.len() >= n {
                return sent;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("expected {n} mails, got {}", self.mailer.sent().len());
    }

    /// Log in a seeded account and return its session cookie value.
    async fn login(&self, who: &str) -> String {
        let r = self
            .post(
                "/api/v1/sr/auth/login",
                json!({"login": who, "password": PW}),
            )
            .send()
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
        assert_eq!(r.body["status"], "ok");
        r.cookie("sr_session").expect("session cookie")
    }
}

/// The token in the first link line of a mail containing `marker`.
fn link_token(m: &OutgoingMail, marker: &str) -> String {
    m.text
        .lines()
        .find(|l| l.contains(marker))
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_owned()
}

impl<'a> Call<'a> {
    fn session(mut self, token: &str) -> Self {
        self.cookies.push(format!("sr_session={token}"));
        self.csrf = Some(csrf::token_for(token));
        self
    }
    fn session_no_csrf(mut self, token: &str) -> Self {
        self.cookies.push(format!("sr_session={token}"));
        self
    }
    fn cookie(mut self, name: &str, value: &str) -> Self {
        self.cookies.push(format!("{name}={value}"));
        self
    }
    fn ip(mut self, ip: &str) -> Self {
        ip.clone_into(&mut self.ip);
        self
    }
    fn origin(mut self, o: &str) -> Self {
        self.origin = Some(o.to_owned());
        self
    }
    fn ua(mut self, ua: &str) -> Self {
        self.ua = Some(ua.to_owned());
        self
    }
    fn json(mut self, v: Value) -> Self {
        self.body = Some(v);
        self
    }

    async fn send(self) -> Resp {
        let mut b = Request::builder()
            .method(self.method)
            .uri(&self.uri)
            .header("x-forwarded-for", &self.ip);
        if !self.cookies.is_empty() {
            b = b.header(header::COOKIE, self.cookies.join("; "));
        }
        if let Some(c) = &self.csrf {
            b = b.header("x-sr-csrf", c);
        }
        if let Some(o) = &self.origin {
            b = b.header(header::ORIGIN, o);
        }
        if let Some(u) = &self.ua {
            b = b.header(header::USER_AGENT, u);
        }
        let req = match self.body {
            Some(v) => b
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(v.to_string()))
                .unwrap(),
            None => b.body(Body::empty()).unwrap(),
        };
        let resp = self.h.app.clone().oneshot(req).await.unwrap();
        let (parts, body) = resp.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        Resp {
            status: parts.status,
            headers: parts.headers,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        }
    }
}

fn reg_body(code: &InviteCode, username: &str, email: &str) -> Value {
    json!({
        "code": code.formatted(),
        "email": email,
        "username": username,
        "display_name": "Sam",
        "password": PW,
        "accepted_house_notes": true, "confirmed_min_age": 13,
    })
}

fn invite_uri(code: &str) -> String {
    format!("/api/v1/sr/invites/{code}")
}

// ---- invite check ---------------------------------------------------------

#[tokio::test]
async fn invite_check_valid_invalid_and_no_oracle() {
    let h = harness();
    let open = h.issuer_with_invite(1, 5);
    let used = InviteCode::generate().unwrap();
    h.store.add_invite(&used, 1);
    let revoked = InviteCode::generate().unwrap();
    h.store.add_invite(&revoked, 1);
    h.store.revoke_invite(&revoked);

    let ok = h.get(&invite_uri(&open.formatted())).send().await;
    assert_eq!(ok.status, StatusCode::OK);
    assert_eq!(ok.body, json!({"valid": true}));

    // Lenient input (case, no dash).
    let lower = h
        .get(&invite_uri(&open.canonical().to_lowercase()))
        .send()
        .await;
    assert_eq!(lower.body, json!({"valid": true}));

    // Redeem `used` directly through the store.
    h.store
        .register(super::RegisterTx {
            discord_id: None,
            identity_id: 0,
            account_id: 3,
            new_person_id: 3,
            ledger_ids: [1, 2],
            code: used.clone(),
            username: "u3".into(),
            display_name: None,
            email: "u3@x.test".into(),
            password_hash: "x".into(),
            account_cap: 100,
            initial_credits: 5,
            now: 0,
        })
        .await
        .unwrap()
        .unwrap();

    let unknown = InviteCode::generate().unwrap();
    let mut bodies = vec![];
    for c in [&unknown, &used, &revoked] {
        let r = h.get(&invite_uri(&c.formatted())).send().await;
        assert_eq!(r.status, StatusCode::OK);
        bodies.push(r.body);
    }
    // Unknown, used and revoked are the same answer.
    assert!(bodies.iter().all(|b| *b == json!({"valid": false})));
    let junk = h.get(&invite_uri("not-a-code")).send().await;
    assert_eq!(junk.body, json!({"valid": false}));
}

#[tokio::test]
async fn invite_check_closed_at_cap_for_every_code() {
    let h = harness_with(2);
    let code = h.issuer_with_invite(1, 5);
    h.add_account(2, "second", "s@x.test", AccountState::Active);
    for c in [
        code.formatted(),
        InviteCode::generate().unwrap().formatted(),
    ] {
        let r = h.get(&invite_uri(&c)).send().await;
        assert_eq!(r.body, json!({"valid": false, "reason": "closed"}));
    }
}

#[tokio::test]
async fn invite_check_is_rate_limited_per_ip() {
    let h = harness();
    let uri = invite_uri("AAAAA-AAAAA");
    for _ in 0..10 {
        assert_eq!(h.get(&uri).send().await.status, StatusCode::OK);
    }
    let r = h.get(&uri).send().await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(r.body["retry_after"].as_i64().unwrap() >= 1);
    assert!(r.headers.contains_key(header::RETRY_AFTER));
    // Another IP is unaffected; time heals.
    assert_eq!(
        h.get(&uri).ip("203.0.113.9").send().await.status,
        StatusCode::OK
    );
    h.clock.advance_ms(61_000);
    assert_eq!(h.get(&uri).send().await.status, StatusCode::OK);
}

#[tokio::test]
async fn invite_check_daily_limit() {
    let h = harness();
    let uri = invite_uri("AAAAA-AAAAA");
    for i in 0..50 {
        // Stay under 10/min.
        if i % 10 == 0 {
            h.clock.advance_ms(61_000);
        }
        assert_eq!(h.get(&uri).send().await.status, StatusCode::OK, "{i}");
    }
    h.clock.advance_ms(61_000);
    assert_eq!(
        h.get(&uri).send().await.status,
        StatusCode::TOO_MANY_REQUESTS
    );
}

// ---- username -----------------------------------------------------------

#[tokio::test]
async fn username_availability() {
    let h = harness();
    h.add_account(1, "taken", "t@x.test", AccountState::Active);
    let q = |u: &str| format!("/api/v1/sr/usernames/check?username={u}");
    assert_eq!(
        h.get(&q("free_one")).send().await.body,
        json!({"available": true})
    );
    assert_eq!(
        h.get(&q("TAKEN")).send().await.body,
        json!({"available": false})
    );
    assert_eq!(
        h.get(&q("admin")).send().await.body,
        json!({"available": false})
    );
    assert_eq!(
        h.get(&q("a")).send().await.body,
        json!({"available": false})
    );
    assert_eq!(
        h.get("/api/v1/sr/usernames/check").send().await.status,
        StatusCode::BAD_REQUEST
    );
}

// ---- registration ---------------------------------------------------------

#[tokio::test]
async fn register_happy_path_consumes_invite_and_credits_ledger() {
    let h = harness();
    let code = h.issuer_with_invite(1, 5);
    let r = h
        .post(
            "/api/v1/sr/register",
            reg_body(&code, "Sam", "Sam@Example.com"),
        )
        .ua(UA_FIREFOX)
        .send()
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.body);
    let id: i64 = r.body["id"].as_str().unwrap().parse().unwrap();
    let session = r.cookie("sr_session").unwrap();
    assert!(
        r.headers
            .get_all(header::SET_COOKIE)
            .iter()
            .any(|c| c.to_str().unwrap().contains("HttpOnly"))
    );

    // Auto-login works.
    let me = h.get("/api/v1/users/@me").session(&session).send().await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body["id"], id.to_string());

    // Ledger: issuer -1, new account +5 initial grant.
    assert_eq!(h.store.balance(1), 4);
    assert_eq!(h.store.ledger_for(id), vec![(5, "initial_grant")]);
    assert!(h.store.ledger_for(1).contains(&(-1, "redemption")));
    assert_eq!(h.account(id).username, "sam");
    assert_eq!(h.account(id).email, "sam@example.com");

    // The invite is spent.
    let again = h
        .post(
            "/api/v1/sr/register",
            reg_body(&code, "other", "o@example.com"),
        )
        .send()
        .await;
    assert_eq!(again.status, StatusCode::FORBIDDEN);

    // Verification mail and token.
    let mails = h.wait_mail(1).await;
    assert_eq!(mails[0].to, "sam@example.com");
    let token = link_token(&mails[0], "/verify-email/");
    assert!(!h.account(id).email_verified);
    let v = h
        .post("/api/v1/sr/auth/verify-email", json!({"token": token}))
        .send()
        .await;
    assert_eq!(v.status, StatusCode::NO_CONTENT);
    assert!(h.account(id).email_verified);
    let v2 = h
        .post("/api/v1/sr/auth/verify-email", json!({"token": token}))
        .send()
        .await;
    assert_eq!(v2.status, StatusCode::BAD_REQUEST);
}

/// ENT-INV-009: the issuer's own sessions hear `SR_INVITE_CREDITS_UPDATE` with the new balance.
#[tokio::test]
async fn redemption_dispatches_invite_credits_update_to_the_issuer() {
    let h = harness();
    let code = h.issuer_with_invite(1, 5);
    let mut issuer =
        sr_events::EventBus::subscribe(&*h.bus, sr_events::Topic::User(sr_core::UserId::new(1)));
    let r = h
        .post("/api/v1/sr/register", reg_body(&code, "sam", "sam@x.test"))
        .send()
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.body);
    let ev = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        futures::StreamExt::next(&mut issuer),
    )
    .await
    .expect("an event within 5 s")
    .unwrap();
    let sr_events::DomainEvent::UserScoped { name, payload, .. } = ev else {
        panic!("expected a user-scoped event, got {ev:?}");
    };
    assert_eq!(name, "SR_INVITE_CREDITS_UPDATE");
    assert_eq!(payload["credits"], 4);
}

#[tokio::test]
async fn register_validation_errors_are_field_scoped() {
    let h = harness();
    let code = h.issuer_with_invite(1, 5);
    let r = h
        .post(
            "/api/v1/sr/register",
            json!({
                "code": code.formatted(), "email": "nope", "username": "x",
                "password": "password123", "accepted_house_notes": false,
            }),
        )
        .send()
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.body["code"], 50035);
    for f in ["email", "username", "password", "rules", "age"] {
        assert!(
            r.body["errors"][f]["_errors"][0]["message"].is_string(),
            "{f}"
        );
    }
    // Nothing was consumed.
    assert_eq!(h.store.balance(1), 5);
    assert_eq!(
        h.get(&invite_uri(&code.formatted())).send().await.body,
        json!({"valid": true})
    );
}

#[tokio::test]
async fn registration_needs_the_age_confirmation_to_reach_the_minimum() {
    let h = harness();
    let code = h.issuer_with_invite(1, 5);
    for age in [json!(null), json!(12)] {
        let mut body = reg_body(&code, "ageless", "ageless@x.test");
        body["confirmed_min_age"] = age;
        let r = h.post("/api/v1/sr/register", body).send().await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        assert_eq!(
            r.body["errors"]["age"]["_errors"][0]["message"],
            "Please confirm you are at least 13."
        );
    }
    // Nothing was consumed, and the right confirmation goes through.
    assert_eq!(h.store.balance(1), 5);
    let r = h
        .post(
            "/api/v1/sr/register",
            reg_body(&code, "of_age", "of_age@x.test"),
        )
        .send()
        .await;
    assert_eq!(r.status, StatusCode::CREATED);
}

#[tokio::test]
async fn register_rejections_are_generic_for_bad_invites() {
    let h = harness();
    let revoked = h.issuer_with_invite(1, 5);
    h.store.revoke_invite(&revoked);
    let broke = h.issuer_with_invite(2, 0);
    let unknown = InviteCode::generate().unwrap();
    let mut seen = vec![];
    for (i, c) in [&revoked, &broke, &unknown].into_iter().enumerate() {
        let r = h
            .post(
                "/api/v1/sr/register",
                reg_body(c, &format!("user{i}"), &format!("u{i}@x.test")),
            )
            .send()
            .await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);
        seen.push(r.body);
    }
    assert!(seen.iter().all(|b| *b == seen[0]));
    assert_eq!(seen[0]["code"], 900006);
}

#[tokio::test]
async fn register_username_and_email_conflicts_do_not_burn_the_invite() {
    let h = harness();
    let a = h.issuer_with_invite(1, 5);
    let b = InviteCode::generate().unwrap();
    h.store.add_invite(&b, 1);
    assert_eq!(
        h.post("/api/v1/sr/register", reg_body(&a, "sam", "sam@x.test"))
            .send()
            .await
            .status,
        StatusCode::CREATED
    );
    let dup_user = h
        .post("/api/v1/sr/register", reg_body(&b, "SAM", "other@x.test"))
        .send()
        .await;
    assert_eq!(dup_user.status, StatusCode::BAD_REQUEST);
    assert!(dup_user.body["errors"]["username"].is_object());
    let dup_mail = h
        .post("/api/v1/sr/register", reg_body(&b, "other", "SAM@x.test"))
        .send()
        .await;
    assert!(dup_mail.body["errors"]["email"].is_object());
    assert_eq!(
        h.post("/api/v1/sr/register", reg_body(&b, "other", "other@x.test"))
            .send()
            .await
            .status,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn register_banned_person_gets_generic_error() {
    let h = harness();
    let code = h.issuer_with_invite(1, 5);
    h.store.ban_email("bad@x.test");
    let mut alerts = sr_events::EventBus::subscribe(&*h.bus, sr_events::Topic::Platform);
    let r = h
        .post("/api/v1/sr/register", reg_body(&code, "bad", "bad@x.test"))
        .send()
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert_eq!(r.body["code"], 900006);
    assert_eq!(h.store.balance(1), 5);
    // Staff are alerted on the platform topic (open question 142).
    let ev = futures::StreamExt::next(&mut alerts).await.unwrap();
    assert!(matches!(
        ev,
        sr_events::DomainEvent::StaffAlert {
            kind: sr_events::StaffAlertKind::BannedPersonAttempt,
            ..
        }
    ));
}

#[tokio::test]
async fn register_at_cap_is_closed() {
    let h = harness_with(1);
    let code = h.issuer_with_invite(1, 5);
    let r = h
        .post("/api/v1/sr/register", reg_body(&code, "sam", "sam@x.test"))
        .send()
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert_eq!(r.body["code"], 900005);
}

#[tokio::test]
async fn register_is_rate_limited_per_ip() {
    let h = harness();
    let code = InviteCode::generate().unwrap();
    for i in 0..5 {
        let r = h
            .post(
                "/api/v1/sr/register",
                reg_body(&code, &format!("user{i}"), &format!("u{i}@x.test")),
            )
            .send()
            .await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);
    }
    let r = h
        .post("/api/v1/sr/register", reg_body(&code, "user9", "u9@x.test"))
        .send()
        .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
}

/// ENT-INV-010 against the in-memory store: 50 parallel redemptions of
/// different codes at cap-1 produce exactly one success.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parallel_redemptions_at_cap_minus_one() {
    let h = Arc::new(harness_with(2));
    // The issuer is the only active account; plenty of credits and 50 codes.
    h.add_account(1, "issuer1", "issuer1@x.test", AccountState::Active);
    h.store.grant(1, 500, "initial_grant");
    let codes: Vec<InviteCode> = (0..50)
        .map(|_| {
            let c = InviteCode::generate().unwrap();
            h.store.add_invite(&c, 1);
            c
        })
        .collect();
    let mut tasks = vec![];
    for (i, code) in codes.into_iter().enumerate() {
        let h = h.clone();
        tasks.push(tokio::spawn(async move {
            h.post(
                "/api/v1/sr/register",
                reg_body(&code, &format!("user{i}"), &format!("u{i}@x.test")),
            )
            .ip(&format!("10.0.0.{i}"))
            .send()
            .await
            .status
        }));
    }
    let mut created = 0;
    for t in tasks {
        if t.await.unwrap() == StatusCode::CREATED {
            created += 1;
        }
    }
    assert_eq!(created, 1);
    assert_eq!(h.store.balance(1), 499);
}

// ---- login --------------------------------------------------------------

#[tokio::test]
async fn login_success_and_generic_failures() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);

    for who in ["alice", "ALICE", "alice@x.test", " Alice@X.test "] {
        let s = h.login(who).await;
        let me = h.get("/api/v1/users/@me").session(&s).send().await;
        assert_eq!(me.body["id"], "1");
    }

    let wrong = h
        .post(
            "/api/v1/sr/auth/login",
            json!({"login": "alice", "password": "nope"}),
        )
        .send()
        .await;
    let unknown = h
        .post(
            "/api/v1/sr/auth/login",
            json!({"login": "ghost", "password": "nope"}),
        )
        .send()
        .await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(unknown.status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.body, unknown.body);
    assert!(wrong.cookie("sr_session").is_none());
}

const UA_EDGE: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120 Safari/537.36 Edg/120.0";

async fn login_from(h: &H, ua: &str) -> Resp {
    let r = h
        .post(
            "/api/v1/sr/auth/login",
            json!({"login": "alice", "password": PW}),
        )
        .ua(ua)
        .send()
        .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    r
}

/// Give the background mail task a moment, then list what was sent.
async fn mails_after_a_moment(h: &H) -> Vec<OutgoingMail> {
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    h.mailer.sent()
}

/// ENT-AUTH-015: only the first sight of a device label sends the alert; the very first device does not.
#[tokio::test]
async fn a_sign_in_from_a_new_device_sends_one_alert() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);

    login_from(&h, UA_FIREFOX).await;
    assert!(
        mails_after_a_moment(&h).await.is_empty(),
        "the first device never alerts"
    );

    login_from(&h, UA_CHROME).await;
    let mails = h.wait_mail(1).await;
    assert_eq!(mails[0].to, "alice@x.test");
    assert_eq!(mails[0].subject, "New sign-in to your SIDE ROOM account");
    assert!(
        mails[0].text.contains("Chrome on macOS"),
        "{}",
        mails[0].text
    );
    assert!(
        mails[0].text.contains("2027-01-15 08:00 UTC"),
        "{}",
        mails[0].text
    );

    login_from(&h, UA_CHROME).await;
    login_from(&h, UA_FIREFOX).await;
    assert_eq!(
        mails_after_a_moment(&h).await.len(),
        1,
        "known devices stay quiet"
    );
}

#[tokio::test]
async fn new_device_alerts_can_be_switched_off_and_need_a_verified_address() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let s = h.login("alice").await;

    let on = h
        .get("/api/v1/sr/auth/login-alerts")
        .session(&s)
        .send()
        .await;
    assert_eq!(on.status, StatusCode::OK);
    assert_eq!(on.body["enabled"], true);

    let off = h
        .call(Method::PUT, "/api/v1/sr/auth/login-alerts")
        .json(json!({"enabled": false}))
        .session(&s)
        .send()
        .await;
    assert_eq!(off.status, StatusCode::NO_CONTENT);
    let now = h
        .get("/api/v1/sr/auth/login-alerts")
        .session(&s)
        .send()
        .await;
    assert_eq!(now.body["enabled"], false);

    login_from(&h, UA_FIREFOX).await;
    login_from(&h, UA_CHROME).await;
    login_from(&h, UA_EDGE).await;
    assert!(
        mails_after_a_moment(&h).await.is_empty(),
        "alerts are off, nothing is sent"
    );

    // Turned back on, an unverified address still gets nothing (it may not be theirs).
    let back = h
        .call(Method::PUT, "/api/v1/sr/auth/login-alerts")
        .json(json!({"enabled": true}))
        .session(&s)
        .send()
        .await;
    assert_eq!(back.status, StatusCode::NO_CONTENT);
    let mut unverified = h.account(1);
    unverified.email_verified = false;
    h.store.add_account(unverified);
    let opera =
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/120 Safari/537.36 OPR/105.0";
    login_from(&h, opera).await;
    assert!(mails_after_a_moment(&h).await.is_empty());

    // Signed out, the switch is not reachable.
    let anon = h.get("/api/v1/sr/auth/login-alerts").send().await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_lockout_and_recovery_over_time() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let bad = || {
        h.post(
            "/api/v1/sr/auth/login",
            json!({"login": "alice", "password": "nope"}),
        )
    };
    for _ in 0..5 {
        assert_eq!(bad().send().await.status, StatusCode::UNAUTHORIZED);
    }
    // The sixth failure still answers 401 but starts a lock.
    assert_eq!(bad().send().await.status, StatusCode::UNAUTHORIZED);
    let locked = h
        .post(
            "/api/v1/sr/auth/login",
            json!({"login": "alice", "password": PW}),
        )
        .send()
        .await;
    assert_eq!(locked.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(locked.body["retry_after"].as_i64().unwrap() > 0);
    // Unknown names lock the same way (no existence oracle).
    for _ in 0..6 {
        h.post(
            "/api/v1/sr/auth/login",
            json!({"login": "ghost", "password": "x"}),
        )
        .send()
        .await;
    }
    let ghost = h
        .post(
            "/api/v1/sr/auth/login",
            json!({"login": "ghost", "password": "x"}),
        )
        .send()
        .await;
    assert_eq!(ghost.status, StatusCode::TOO_MANY_REQUESTS);

    h.clock.advance_ms(31_000);
    let ok = h
        .post(
            "/api/v1/sr/auth/login",
            json!({"login": "alice", "password": PW}),
        )
        .send()
        .await;
    assert_eq!(ok.status, StatusCode::OK);
}

#[tokio::test]
async fn login_blocked_states_are_named_only_after_the_right_password() {
    let h = harness();
    h.add_account(1, "dee", "dee@x.test", AccountState::Deactivated);
    h.add_account(2, "ban", "ban@x.test", AccountState::PlatformBanned);
    h.add_account(3, "gone", "gone@x.test", AccountState::Deleted);
    let go = |who: &str, pw: &str| {
        h.post(
            "/api/v1/sr/auth/login",
            json!({"login": who, "password": pw}),
        )
        .ip(&format!("10.1.1.{}", who.len()))
        .send()
    };
    assert_eq!(go("dee", PW).await.body["code"], 900007);
    assert_eq!(go("ban", PW).await.body["code"], 900008);
    assert_eq!(go("dee", "wrong").await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(go("gone", PW).await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_rehashes_outdated_parameters() {
    let h = harness();
    let old = PasswordHasher::new(PasswordParams {
        m_cost_kib: 128,
        t_cost: 1,
        p_cost: 1,
    })
    .unwrap();
    h.store.add_account(AccountRecord {
        id: 1,
        person_id: 1,
        username: "alice".into(),
        display_name: None,
        email: "a@x.test".into(),
        email_verified: true,
        password_hash: old.hash(PW).unwrap(),
        state: AccountState::Active,
        age_confirmed: None,
        age_confirmed_at: None,
        locale: "en-US".into(),
        flags: 0,
    });
    h.login("alice").await;
    let stored = h.account(1).password_hash;
    assert!(!h.hasher.needs_rehash(&stored));
    assert!(h.hasher.verify(PW, &stored).unwrap());
}

#[tokio::test]
async fn origin_guard_rejects_foreign_origins() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let body = json!({"login": "alice", "password": PW});
    let evil = h
        .post("/api/v1/sr/auth/login", body.clone())
        .origin("https://evil.test")
        .send()
        .await;
    assert_eq!(evil.status, StatusCode::FORBIDDEN);
    assert!(evil.cookie("sr_session").is_none());
    let ok = h
        .post("/api/v1/sr/auth/login", body)
        .origin("http://localhost:8080")
        .send()
        .await;
    assert_eq!(ok.status, StatusCode::OK);
}

// ---- TOTP and recovery codes ------------------------------------------------

/// Enrol TOTP for a logged-in session; returns (secret, recovery codes).
async fn enrol_totp(h: &H, session: &str) -> (TotpSecret, Vec<String>) {
    let e = h
        .post("/api/v1/sr/auth/totp/enroll", json!({}))
        .session(session)
        .send()
        .await;
    assert_eq!(e.status, StatusCode::OK, "{:?}", e.body);
    assert!(
        e.body["otpauth_uri"]
            .as_str()
            .unwrap()
            .starts_with("otpauth://totp/")
    );
    let key = e.body["manual_entry_key"].as_str().unwrap();
    let raw = data_encoding::BASE32_NOPAD.decode(key.as_bytes()).unwrap();
    let secret = TotpSecret::from_bytes(&raw).unwrap();

    // A wrong code does not confirm.
    let bad = h
        .post("/api/v1/sr/auth/totp/verify", json!({"code": "000000"}))
        .session(session)
        .send()
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);

    let code = totp::current_code(&secret, "SIDE ROOM", "alice", h.now()).unwrap();
    let ok = h
        .post("/api/v1/sr/auth/totp/verify", json!({"code": code}))
        .session(session)
        .send()
        .await;
    assert_eq!(ok.status, StatusCode::OK, "{:?}", ok.body);
    let codes: Vec<String> = ok.body["recovery_codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(codes.len(), 10);
    // Turning two-factor on mails the account a security notice (ENT-AUTH-014); callers count other mails.
    let notice = h.wait_mail(1).await;
    assert_eq!(notice[0].subject, "Your SIDE ROOM sign-in security changed");
    assert!(
        notice[0]
            .text
            .contains("An authenticator app was added on your SIDE ROOM account")
    );
    h.mailer.take();
    (secret, codes)
}

async fn login_expecting_mfa(h: &H) -> String {
    let r = h
        .post(
            "/api/v1/sr/auth/login",
            json!({"login": "alice", "password": PW}),
        )
        .send()
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body["status"], "mfa_required");
    assert_eq!(r.body["methods"], json!(["totp", "recovery"]));
    assert!(r.cookie("sr_session").is_none(), "no session before 2FA");
    r.cookie("sr_mfa").expect("pending cookie")
}

fn totp_body(code: &str) -> Value {
    json!({"method": "totp", "code": code, "trust_device": false})
}

fn recovery_body(code: &str) -> Value {
    json!({"method": "recovery", "code": code, "trust_device": false})
}

#[tokio::test]
async fn totp_enrol_login_replay_and_recovery() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let s = h.login("alice").await;
    let (secret, recovery) = enrol_totp(&h, &s).await;

    // Enrolling twice is refused.
    let again = h
        .post("/api/v1/sr/auth/totp/enroll", json!({}))
        .session(&s)
        .send()
        .await;
    assert_eq!(again.status, StatusCode::BAD_REQUEST);

    // Next login needs the second factor.
    let mfa = login_expecting_mfa(&h).await;
    let wrong = h
        .post("/api/v1/sr/auth/2fa", totp_body("123456"))
        .cookie("sr_mfa", &mfa)
        .send()
        .await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    // The step used at enrolment is a replay.
    let same_step = totp::current_code(&secret, "SIDE ROOM", "alice", h.now()).unwrap();
    let r = h
        .post("/api/v1/sr/auth/2fa", totp_body(&same_step))
        .cookie("sr_mfa", &mfa)
        .send()
        .await;
    assert_eq!(
        r.status,
        StatusCode::UNAUTHORIZED,
        "replayed enrolment step"
    );

    h.clock.advance_ms(60_000);
    let code = totp::current_code(&secret, "SIDE ROOM", "alice", h.now()).unwrap();
    let ok = h
        .post(
            "/api/v1/sr/auth/2fa",
            json!({"method": "totp", "code": code, "trust_device": true}),
        )
        .cookie("sr_mfa", &mfa)
        .ua(UA_CHROME)
        .send()
        .await;
    assert_eq!(ok.status, StatusCode::OK, "{:?}", ok.body);
    let session = ok.cookie("sr_session").unwrap();
    assert_eq!(ok.cookie("sr_mfa").as_deref(), Some(""), "pending cleared");
    assert_eq!(
        h.get("/api/v1/users/@me")
            .session(&session)
            .send()
            .await
            .status,
        StatusCode::OK
    );

    // The pending login is single-use.
    let reuse = h
        .post("/api/v1/sr/auth/2fa", totp_body(&code))
        .cookie("sr_mfa", &mfa)
        .send()
        .await;
    assert_eq!(reuse.status, StatusCode::UNAUTHORIZED);

    // A used TOTP code cannot start another login.
    let mfa2 = login_expecting_mfa(&h).await;
    let replayed = h
        .post("/api/v1/sr/auth/2fa", totp_body(&code))
        .cookie("sr_mfa", &mfa2)
        .send()
        .await;
    assert_eq!(replayed.status, StatusCode::UNAUTHORIZED);

    // Recovery code: works once.
    let rec = h
        .post(
            "/api/v1/sr/auth/2fa",
            recovery_body(&recovery[0].to_lowercase()),
        )
        .cookie("sr_mfa", &mfa2)
        .send()
        .await;
    assert_eq!(rec.status, StatusCode::OK, "{:?}", rec.body);
    assert_eq!(rec.body["recovery_codes_remaining"], 9);
    let mfa3 = login_expecting_mfa(&h).await;
    let used = h
        .post("/api/v1/sr/auth/2fa", recovery_body(&recovery[0]))
        .cookie("sr_mfa", &mfa3)
        .send()
        .await;
    assert_eq!(used.status, StatusCode::UNAUTHORIZED);
    let other = h
        .post("/api/v1/sr/auth/2fa", recovery_body(&recovery[1]))
        .cookie("sr_mfa", &mfa3)
        .send()
        .await;
    assert_eq!(other.status, StatusCode::OK);
}

#[tokio::test]
async fn second_factor_needs_pending_login_limits_attempts_and_expires() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let s = h.login("alice").await;
    let (secret, _) = enrol_totp(&h, &s).await;

    let body = totp_body("123456");
    assert_eq!(
        h.post("/api/v1/sr/auth/2fa", body.clone())
            .send()
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        h.post("/api/v1/sr/auth/2fa", body.clone())
            .cookie("sr_mfa", "junk")
            .send()
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    let unknown_method = {
        let mfa = login_expecting_mfa(&h).await;
        h.post(
            "/api/v1/sr/auth/2fa",
            json!({"method": "sms", "code": "1", "trust_device": false}),
        )
        .cookie("sr_mfa", &mfa)
        .send()
        .await
    };
    assert_eq!(unknown_method.status, StatusCode::BAD_REQUEST);

    h.clock.advance_ms(60_000);
    let mfa = login_expecting_mfa(&h).await;
    for _ in 0..5 {
        h.post("/api/v1/sr/auth/2fa", body.clone())
            .cookie("sr_mfa", &mfa)
            .send()
            .await;
    }
    // After too many wrong codes the pending login is gone, even for the
    // right code.
    let code = totp::current_code(&secret, "SIDE ROOM", "alice", h.now()).unwrap();
    let r = h
        .post("/api/v1/sr/auth/2fa", totp_body(&code))
        .cookie("sr_mfa", &mfa)
        .send()
        .await;
    assert!(
        r.status == StatusCode::UNAUTHORIZED || r.status == StatusCode::TOO_MANY_REQUESTS,
        "{:?}",
        r.status
    );
    assert!(r.cookie("sr_session").is_none());

    // Pending logins expire after five minutes.
    h.clock.advance_ms(16 * 60_000);
    let mfa = login_expecting_mfa(&h).await;
    h.clock.advance_ms(6 * 60_000);
    let code = totp::current_code(&secret, "SIDE ROOM", "alice", h.now()).unwrap();
    let r = h
        .post("/api/v1/sr/auth/2fa", totp_body(&code))
        .cookie("sr_mfa", &mfa)
        .send()
        .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn disable_and_regenerate_require_the_password() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let s = h.login("alice").await;

    // Regenerate before enabling is refused.
    let early = h
        .post("/api/v1/sr/auth/recovery-codes", json!({"password": PW}))
        .session(&s)
        .send()
        .await;
    assert_eq!(early.status, StatusCode::BAD_REQUEST);

    let (_, old) = enrol_totp(&h, &s).await;
    let wrong = h
        .post(
            "/api/v1/sr/auth/recovery-codes",
            json!({"password": "nope"}),
        )
        .session(&s)
        .send()
        .await;
    assert_eq!(wrong.status, StatusCode::FORBIDDEN);
    assert_eq!(wrong.body["code"], 900004);
    let fresh = h
        .post("/api/v1/sr/auth/recovery-codes", json!({"password": PW}))
        .session(&s)
        .send()
        .await;
    assert_eq!(fresh.status, StatusCode::OK);
    assert_eq!(fresh.body["recovery_codes"].as_array().unwrap().len(), 10);

    // Old codes no longer work.
    let mfa = login_expecting_mfa(&h).await;
    let r = h
        .post("/api/v1/sr/auth/2fa", recovery_body(&old[0]))
        .cookie("sr_mfa", &mfa)
        .send()
        .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);

    // Disable.
    let del = |pw: &str| {
        h.call(Method::DELETE, "/api/v1/sr/auth/totp")
            .json(json!({"password": pw}))
            .session(&s)
            .send()
    };
    assert_eq!(del("nope").await.status, StatusCode::FORBIDDEN);
    assert_eq!(del(PW).await.status, StatusCode::NO_CONTENT);
    h.login("alice").await; // no second factor any more
}

// ---- sessions -----------------------------------------------------------

#[tokio::test]
async fn sessions_list_revoke_logout() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let body = json!({"login": "alice", "password": PW});
    let s1 = h
        .post("/api/v1/sr/auth/login", body.clone())
        .ua(UA_FIREFOX)
        .send()
        .await
        .cookie("sr_session")
        .unwrap();
    h.clock.advance_ms(1_000);
    let s2 = h
        .post("/api/v1/sr/auth/login", body)
        .ua(UA_CHROME)
        .send()
        .await
        .cookie("sr_session")
        .unwrap();

    let list = h.get("/api/v1/sr/auth/sessions").session(&s2).send().await;
    assert_eq!(list.status, StatusCode::OK);
    let rows = list.body.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["device_name"], "Chrome on macOS");
    assert_eq!(rows[0]["current"], true);
    assert_eq!(rows[1]["device_name"], "Firefox on Windows");
    assert_eq!(rows[1]["current"], false);
    assert!(rows[0]["created_at"].as_str().unwrap().ends_with('Z'));
    let other_id = rows[1]["id"].as_str().unwrap().to_owned();

    let del = |id: &str| {
        h.call(Method::DELETE, &format!("/api/v1/sr/auth/sessions/{id}"))
            .session(&s2)
            .send()
    };
    assert_eq!(del("999999").await.status, StatusCode::NOT_FOUND);
    assert_eq!(del("abc").await.status, StatusCode::NOT_FOUND);
    assert_eq!(del(&other_id).await.status, StatusCode::NO_CONTENT);
    assert_eq!(
        h.get("/api/v1/users/@me").session(&s1).send().await.status,
        StatusCode::UNAUTHORIZED
    );

    // Logout needs the CSRF header.
    let no_csrf = h
        .post("/api/v1/sr/auth/logout", json!({}))
        .session_no_csrf(&s2)
        .send()
        .await;
    assert_eq!(no_csrf.status, StatusCode::FORBIDDEN);
    let out = h
        .post("/api/v1/sr/auth/logout", json!({}))
        .session(&s2)
        .send()
        .await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    assert_eq!(out.cookie("sr_session").as_deref(), Some(""));
    assert_eq!(
        h.get("/api/v1/users/@me").session(&s2).send().await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn logout_all_revokes_everything_and_sessions_are_user_scoped() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    h.add_account(2, "bob", "bob@x.test", AccountState::Active);
    let a1 = h.login("alice").await;
    let a2 = h.login("alice").await;
    let b = h.login("bob").await;

    // Bob cannot revoke Alice's sessions by id.
    let list = h.get("/api/v1/sr/auth/sessions").session(&a1).send().await;
    let id = list.body[0]["id"].as_str().unwrap().to_owned();
    let r = h
        .call(Method::DELETE, &format!("/api/v1/sr/auth/sessions/{id}"))
        .session(&b)
        .send()
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);

    let r = h
        .post("/api/v1/sr/auth/logout-all", json!({}))
        .session(&a1)
        .send()
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    for s in [&a1, &a2] {
        assert_eq!(
            h.get("/api/v1/users/@me").session(s).send().await.status,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        h.get("/api/v1/users/@me").session(&b).send().await.status,
        StatusCode::OK
    );
    assert_eq!(
        h.get("/api/v1/sr/auth/sessions").send().await.status,
        StatusCode::UNAUTHORIZED
    );
}

// ---- password reset -------------------------------------------------------

#[tokio::test]
async fn forgot_password_has_no_account_oracle() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let known = h
        .post("/api/v1/sr/auth/forgot", json!({"email": "Alice@x.test"}))
        .send()
        .await;
    let unknown = h
        .post("/api/v1/sr/auth/forgot", json!({"email": "ghost@x.test"}))
        .send()
        .await;
    let junk = h
        .post("/api/v1/sr/auth/forgot", json!({"email": "not an email"}))
        .send()
        .await;
    for r in [&known, &unknown, &junk] {
        assert_eq!(r.status, StatusCode::NO_CONTENT);
        assert_eq!(r.body, Value::Null);
    }
    let mails = h.wait_mail(1).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(h.mailer.sent().len(), 1, "only the real account gets mail");
    assert_eq!(mails[0].to, "alice@x.test");
    assert!(mails[0].text.contains("/reset/"));
}

#[tokio::test]
async fn reset_flow_single_use_revokes_sessions() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let old_session = h.login("alice").await;
    h.post("/api/v1/sr/auth/forgot", json!({"email": "alice@x.test"}))
        .send()
        .await;
    let token = link_token(&h.wait_mail(1).await[0], "/reset/");

    // Weak password: field error, token survives.
    let weak = h
        .post(
            "/api/v1/sr/auth/reset",
            json!({"token": token, "password": "password123"}),
        )
        .send()
        .await;
    assert_eq!(weak.status, StatusCode::BAD_REQUEST);
    assert!(weak.body["errors"]["password"].is_object());
    // Junk token.
    let junk = h
        .post(
            "/api/v1/sr/auth/reset",
            json!({"token": "junk", "password": PW}),
        )
        .send()
        .await;
    assert_eq!(junk.status, StatusCode::BAD_REQUEST);
    assert!(junk.body["errors"]["token"].is_object());

    let ok = h
        .post(
            "/api/v1/sr/auth/reset",
            json!({"token": token, "password": NEW_PW}),
        )
        .send()
        .await;
    assert_eq!(ok.status, StatusCode::NO_CONTENT);
    // Single use.
    let again = h
        .post(
            "/api/v1/sr/auth/reset",
            json!({"token": token, "password": NEW_PW}),
        )
        .send()
        .await;
    assert_eq!(again.status, StatusCode::BAD_REQUEST);

    // Old sessions are gone; old password fails; new works.
    assert_eq!(
        h.get("/api/v1/users/@me")
            .session(&old_session)
            .send()
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    let old = h
        .post(
            "/api/v1/sr/auth/login",
            json!({"login": "alice", "password": PW}),
        )
        .send()
        .await;
    assert_eq!(old.status, StatusCode::UNAUTHORIZED);
    let new = h
        .post(
            "/api/v1/sr/auth/login",
            json!({"login": "alice", "password": NEW_PW}),
        )
        .send()
        .await;
    assert_eq!(new.status, StatusCode::OK);
}

#[tokio::test]
async fn reset_token_expires_after_30_minutes() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    h.post("/api/v1/sr/auth/forgot", json!({"email": "alice@x.test"}))
        .send()
        .await;
    let token = link_token(&h.wait_mail(1).await[0], "/reset/");
    h.clock.advance_ms(31 * 60_000);
    let r = h
        .post(
            "/api/v1/sr/auth/reset",
            json!({"token": token, "password": PW}),
        )
        .send()
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn reset_keeps_two_factor() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let s = h.login("alice").await;
    enrol_totp(&h, &s).await;
    h.post("/api/v1/sr/auth/forgot", json!({"email": "alice@x.test"}))
        .send()
        .await;
    let token = link_token(&h.wait_mail(1).await[0], "/reset/");
    h.post(
        "/api/v1/sr/auth/reset",
        json!({"token": token, "password": NEW_PW}),
    )
    .send()
    .await;
    let r = h
        .post(
            "/api/v1/sr/auth/login",
            json!({"login": "alice", "password": NEW_PW}),
        )
        .send()
        .await;
    assert_eq!(r.body["status"], "mfa_required");
}

// ---- email verification, age gate -------------------------------------------

#[tokio::test]
async fn resend_verification_has_a_cooldown() {
    let h = harness();
    h.store.add_account(AccountRecord {
        id: 1,
        person_id: 1,
        username: "alice".into(),
        display_name: None,
        email: "alice@x.test".into(),
        email_verified: false,
        password_hash: h.hasher.hash(PW).unwrap(),
        state: AccountState::Active,
        age_confirmed: None,
        age_confirmed_at: None,
        locale: "en-US".into(),
        flags: 0,
    });
    let s = h.login("alice").await;
    let send = || {
        h.post("/api/v1/sr/auth/verify-email/resend", json!({}))
            .session(&s)
            .send()
    };
    assert_eq!(send().await.status, StatusCode::NO_CONTENT);
    h.wait_mail(1).await;
    assert_eq!(send().await.status, StatusCode::TOO_MANY_REQUESTS);
    h.clock.advance_ms(61_000);
    assert_eq!(send().await.status, StatusCode::NO_CONTENT);
    let mails = h.wait_mail(2).await;

    // The newest link wins; the older one was superseded.
    let (old, new) = (
        link_token(&mails[0], "/verify-email/"),
        link_token(&mails[1], "/verify-email/"),
    );
    let verify = |t: &str| {
        h.post("/api/v1/sr/auth/verify-email", json!({"token": t}))
            .send()
    };
    assert_eq!(verify(&old).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(verify(&new).await.status, StatusCode::NO_CONTENT);
    // Already verified: resend is a quiet no-op.
    assert_eq!(send().await.status, StatusCode::NO_CONTENT);
    assert_eq!(h.mailer.sent().len(), 2);
}

#[tokio::test]
async fn age_gate_records_the_answer() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let s = h.login("alice").await;
    assert_eq!(
        h.post("/api/v1/sr/age-gate", json!({"over_18": true}))
            .send()
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    let r = h
        .post("/api/v1/sr/age-gate", json!({"over_18": true}))
        .session(&s)
        .send()
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert_eq!(h.account(1).age_confirmed, Some(true));
    h.post("/api/v1/sr/age-gate", json!({"over_18": false}))
        .session(&s)
        .send()
        .await;
    assert_eq!(h.account(1).age_confirmed, Some(false));
}

#[tokio::test]
async fn age_confirmation_can_be_read_and_reset() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let s = h.login("alice").await;
    let get = || {
        h.get("/api/v1/users/@me/age-confirmation")
            .session(&s)
            .send()
    };
    let r = get().await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    assert_eq!(r.body["confirmed_at"], Value::Null);
    assert_eq!(r.body["method"], Value::Null);

    h.post("/api/v1/sr/age-gate", json!({"over_18": true}))
        .session(&s)
        .send()
        .await;
    let r = get().await;
    assert_eq!(r.body["method"], "self-declared");
    assert!(
        r.body["confirmed_at"]
            .as_str()
            .is_some_and(|t| t.ends_with('Z'))
    );

    // Signed-out callers are refused; resetting needs the password and then asks again.
    assert_eq!(
        h.get("/api/v1/users/@me/age-confirmation")
            .send()
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    let reset = |pw: &str| {
        h.call(Method::DELETE, "/api/v1/users/@me/age-confirmation")
            .json(json!({ "current_password": pw }))
            .session(&s)
            .send()
    };
    assert_ne!(reset("wrong password").await.status, StatusCode::NO_CONTENT);
    assert_eq!(h.account(1).age_confirmed, Some(true));
    assert_eq!(reset(PW).await.status, StatusCode::NO_CONTENT);
    assert_eq!(h.account(1).age_confirmed, None);
    assert_eq!(get().await.body["confirmed_at"], Value::Null);

    // A "younger" answer carries no time.
    h.post("/api/v1/sr/age-gate", json!({"over_18": false}))
        .session(&s)
        .send()
        .await;
    assert_eq!(get().await.body["confirmed_at"], Value::Null);
}

#[tokio::test]
async fn missing_service_extension_is_a_500_not_a_panic() {
    let state = AppState::builder().build();
    let app = build_router(state, super::router());
    let r = app
        .oneshot(
            Request::get(invite_uri("AAAAA-AAAAA"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

// ---- login link, trusted devices, Discord (open questions 43, 78, 144) ----

/// Store a one-time token for `purpose` and return its raw value.
async fn issue_token(h: &H, user: i64, purpose: sr_auth::email_token::EmailTokenPurpose) -> String {
    let t = sr_auth::email_token::EmailToken::generate(purpose).unwrap();
    h.store
        .email_token_put(user, purpose, &t.hash(), h.now() as i64 + 3600)
        .await
        .unwrap();
    t.expose().to_owned()
}

#[tokio::test]
async fn login_link_signs_in_once_and_only_with_its_own_purpose() {
    use sr_auth::email_token::EmailTokenPurpose::{LoginLink, ResetPassword};
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);

    // A reset token does not log anyone in.
    let reset = issue_token(&h, 1, ResetPassword).await;
    let wrong = h
        .post("/api/v1/sr/auth/login-link", json!({ "token": reset }))
        .send()
        .await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST);

    let link = issue_token(&h, 1, LoginLink).await;
    let ok = h
        .post("/api/v1/sr/auth/login-link", json!({ "token": link }))
        .send()
        .await;
    assert_eq!(ok.status, StatusCode::OK, "{:?}", ok.body);
    assert!(ok.cookie("sr_session").is_some());

    let reused = h
        .post("/api/v1/sr/auth/login-link", json!({ "token": link }))
        .send()
        .await;
    assert_eq!(reused.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn trusted_device_skips_the_second_factor_until_revoked() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let s = h.login("alice").await;
    let (secret, _) = enrol_totp(&h, &s).await;

    let mfa = login_expecting_mfa(&h).await;
    h.clock.advance_ms(60_000);
    let code = totp::current_code(&secret, "SIDE ROOM", "alice", h.now()).unwrap();
    let ok = h
        .post(
            "/api/v1/sr/auth/2fa",
            json!({"method": "totp", "code": code, "trust_device": true}),
        )
        .cookie("sr_mfa", &mfa)
        .ua(UA_CHROME)
        .send()
        .await;
    assert_eq!(ok.status, StatusCode::OK, "{:?}", ok.body);
    let trusted = ok.cookie("sr_trusted").expect("trusted-device cookie");

    // The trusted browser signs in with the password alone.
    let login = |cookie: Option<&str>| {
        let call = h.post(
            "/api/v1/sr/auth/login",
            json!({"login": "alice", "password": PW}),
        );
        match cookie {
            Some(c) => call.cookie("sr_trusted", c),
            None => call,
        }
    };
    let skip = login(Some(&trusted)).send().await;
    assert_eq!(skip.body["status"], "ok", "{:?}", skip.body);
    let plain = login(None).send().await;
    assert_eq!(plain.body["status"], "mfa_required");

    // Listed under Devices, and revocable.
    let list = h
        .get("/api/v1/sr/auth/trusted-devices")
        .session(&s)
        .send()
        .await;
    assert_eq!(list.status, StatusCode::OK);
    let id = list.body[0]["id"].as_str().unwrap().to_owned();
    let del = h
        .call(
            Method::DELETE,
            &format!("/api/v1/sr/auth/trusted-devices/{id}"),
        )
        .session(&s)
        .send()
        .await;
    assert_eq!(del.status, StatusCode::NO_CONTENT);
    let after = login(Some(&trusted)).send().await;
    assert_eq!(after.body["status"], "mfa_required");
}

/// The harness with Discord configured (no network: only the local checks run).
fn discord_harness() -> H {
    let sessions = Arc::new(MemorySessions::new());
    let clock = Arc::new(ManualClock::new(NOW_MS));
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let store = Arc::new(MemoryAuthStore::new(sessions.clone()));
    let mailer = Arc::new(MemoryMailer::new());
    let next = Arc::new(AtomicI64::new(10_000));
    let ids: super::IdSource = Arc::new(move || next.fetch_add(1, Ordering::SeqCst));
    let svc = AuthService::new(store.clone(), mailer.clone(), ids)
        .with_hasher(cheap_hasher())
        .with_discord(&super::DiscordSettings {
            client_id: "1234".into(),
            client_secret: "secret".into(),
            redirect_uri: "https://chat.example.org/api/v1/sr/auth/discord/callback".into(),
        })
        .unwrap();
    let state = AppState::builder()
        .sessions(sessions)
        .clock(clock.clone())
        .settings(settings)
        .extension(Arc::new(svc))
        .build();
    let app = build_router(state.clone(), super::router());
    H {
        app,
        state,
        store,
        mailer,
        clock,
        hasher: cheap_hasher(),
        bus: Arc::new(sr_events::InProcessBus::new()),
    }
}

#[tokio::test]
async fn discord_start_redirects_with_a_state_cookie() {
    let h = discord_harness();
    let r = h
        .get("/api/v1/sr/auth/discord/start?intent=login&next=/lounge")
        .send()
        .await;
    assert_eq!(r.status, StatusCode::FOUND);
    let location = r.headers[header::LOCATION].to_str().unwrap().to_owned();
    assert!(location.starts_with("https://discord.com/oauth2/authorize?"));
    let state = r.cookie("sr_oauth_state").expect("state cookie");
    assert!(location.contains(&format!("state={state}")));
    assert!(location.contains("scope=identify"));

    let bad = h
        .get("/api/v1/sr/auth/discord/start?intent=steal")
        .send()
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn discord_callback_refuses_a_mismatched_state() {
    let h = discord_harness();
    let start = h
        .get("/api/v1/sr/auth/discord/start?intent=login")
        .send()
        .await;
    let state = start.cookie("sr_oauth_state").unwrap();
    let r = h
        .get("/api/v1/sr/auth/discord/callback?code=abc&state=forged")
        .cookie("sr_oauth_state", &state)
        .send()
        .await;
    assert_eq!(r.status, StatusCode::FOUND);
    assert_eq!(
        r.headers[header::LOCATION].to_str().unwrap(),
        "/login?error=discord_failed"
    );
}

#[tokio::test]
async fn discord_pending_link_is_empty_without_a_cookie() {
    let h = discord_harness();
    let r = h.get("/api/v1/sr/auth/discord/pending").send().await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body["pending"], false);
}

#[tokio::test]
async fn discord_routes_are_404_when_not_configured() {
    let h = harness();
    let r = h
        .get("/api/v1/sr/auth/discord/start?intent=login")
        .send()
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn users_me_is_a_discord_shaped_user_for_sessions() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let s = h.login("alice").await;
    let me = h.get("/api/v1/users/@me").session(&s).send().await;
    assert_eq!(me.status, StatusCode::OK);
    let b = &me.body;
    assert_eq!(b["id"], "1");
    assert_eq!(b["auth"], "session");
    assert_eq!(b["username"], "alice");
    assert_eq!(b["discriminator"], "0");
    assert!(b["avatar"].is_null());
    assert_eq!(b["bot"], false);
    assert_eq!(b["email"], "alice@x.test");
    assert_eq!(b["verified"], true);
    assert_eq!(b["mfa_enabled"], false);
    assert_eq!(b["flags"], 0);
    assert_eq!(b["locale"], "en-US");
    // No display name set: the fields are omitted rather than null.
    assert!(b.get("global_name").is_none());
    assert!(b.get("display_name").is_none());
}

// ---- signed-in identity edits -------------------------------------------------

#[tokio::test]
async fn change_password_checks_current_and_revokes_other_sessions() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let a = h.login("alice").await;
    let b = h.login("alice").await;
    let url = "/api/v1/sr/auth/password";

    // Wrong current password: refused, nothing changes.
    let bad = h
        .post(
            url,
            json!({"current_password": "nope nope nope", "new_password": NEW_PW}),
        )
        .session(&a)
        .send()
        .await;
    assert_eq!(bad.status, StatusCode::FORBIDDEN, "{:?}", bad.body);
    // Weak new password.
    let weak = h
        .post(
            url,
            json!({"current_password": PW, "new_password": "alice1234"}),
        )
        .session(&a)
        .send()
        .await;
    assert_eq!(weak.status, StatusCode::BAD_REQUEST);
    // Same password again.
    let same = h
        .post(url, json!({"current_password": PW, "new_password": PW}))
        .session(&a)
        .send()
        .await;
    assert_eq!(same.status, StatusCode::BAD_REQUEST);

    let ok = h
        .post(url, json!({"current_password": PW, "new_password": NEW_PW}))
        .session(&a)
        .send()
        .await;
    assert_eq!(ok.status, StatusCode::NO_CONTENT, "{:?}", ok.body);
    // The calling session survives, the other one is gone.
    let sessions = h.get("/api/v1/sr/auth/sessions").session(&a).send().await;
    assert_eq!(sessions.status, StatusCode::OK);
    assert_eq!(sessions.body.as_array().unwrap().len(), 1);
    let other = h.get("/api/v1/sr/auth/sessions").session(&b).send().await;
    assert_eq!(other.status, StatusCode::UNAUTHORIZED);
    // The new password logs in; the old one no longer does.
    let login = |pw: &str| {
        h.post(
            "/api/v1/sr/auth/login",
            json!({"login": "alice", "password": pw}),
        )
        .send()
    };
    assert_eq!(login(NEW_PW).await.status, StatusCode::OK);
    assert_ne!(login(PW).await.status, StatusCode::OK);
    // A notice mail went out and the change is audited.
    let mails = h.wait_mail(1).await;
    assert!(mails[0].subject.contains("password"));
    assert!(
        h.store
            .audit_rows()
            .iter()
            .any(|(actor, action, _)| *actor == 1 && action == "account.password_change")
    );
}

#[tokio::test]
async fn change_password_can_keep_other_sessions() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let a = h.login("alice").await;
    let b = h.login("alice").await;
    let ok = h
        .post(
            "/api/v1/sr/auth/password",
            json!({"current_password": PW, "new_password": NEW_PW, "revoke_other_sessions": false}),
        )
        .session(&a)
        .send()
        .await;
    assert_eq!(ok.status, StatusCode::NO_CONTENT);
    let other = h.get("/api/v1/sr/auth/sessions").session(&b).send().await;
    assert_eq!(other.status, StatusCode::OK);
}

#[tokio::test]
async fn change_password_needs_a_session_and_csrf() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let a = h.login("alice").await;
    let body = json!({"current_password": PW, "new_password": NEW_PW});
    let anon = h
        .post("/api/v1/sr/auth/password", body.clone())
        .send()
        .await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);
    let no_csrf = h
        .post("/api/v1/sr/auth/password", body)
        .session_no_csrf(&a)
        .send()
        .await;
    assert_eq!(no_csrf.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn change_username_validates_reauths_and_rate_limits() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    h.add_account(2, "bobby", "bobby@x.test", AccountState::Active);
    let s = h.login("alice").await;
    let put = |u: &str, pw: &str| {
        h.call(Method::PUT, "/api/v1/sr/auth/username")
            .json(json!({"username": u, "password": pw}))
            .session(&s)
            .send()
    };
    assert_eq!(put("Bad Name!", PW).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(put("admin", PW).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        put("alice2", "wrong password").await.status,
        StatusCode::FORBIDDEN
    );
    let taken = put("bobby", PW).await;
    assert_eq!(taken.status, StatusCode::BAD_REQUEST);

    let ok = put("Alice.New", PW).await;
    assert_eq!(ok.status, StatusCode::OK, "{:?}", ok.body);
    assert_eq!(ok.body["username"], "alice.new");
    assert_eq!(h.account(1).username, "alice.new");
    // Same name is a no-op and does not use up a change.
    assert_eq!(put("alice.new", PW).await.status, StatusCode::OK);
    assert_eq!(put("alice.v3", PW).await.status, StatusCode::OK);
    // A third real change within a day is throttled.
    assert_eq!(
        put("alice.v4", PW).await.status,
        StatusCode::TOO_MANY_REQUESTS
    );
    h.clock.advance_ms(86_401_000);
    assert_eq!(put("alice.v4", PW).await.status, StatusCode::OK);
    assert!(
        h.store
            .audit_rows()
            .iter()
            .any(|(_, action, _)| action == "account.username_change")
    );
}

#[tokio::test]
async fn delete_account_needs_password_and_typed_username_and_ends_the_account() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let s = h.login("alice").await;
    let del = |username: &str, pw: &str| {
        h.post(
            "/api/v1/sr/auth/account/delete",
            json!({"username": username, "password": pw}),
        )
        .session(&s)
        .send()
    };
    // Wrong typed username, wrong password: nothing happens.
    assert_eq!(del("someone", PW).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        del("alice", "wrong password").await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(h.account(1).state, AccountState::Active);
    // The typed username is case-insensitive.
    let ok = del("ALICE", PW).await;
    assert_eq!(ok.status, StatusCode::NO_CONTENT, "{:?}", ok.body);
    let a = h.account(1);
    assert_eq!(a.state, AccountState::Deleted);
    assert_eq!(a.display_name.as_deref(), Some("Deleted User"));
    assert!(
        h.store
            .audit_rows()
            .iter()
            .any(|(_, action, _)| action == "account.delete")
    );
    // The session is gone and the account can never sign in again.
    let again = h.get("/api/v1/sr/auth/sessions").session(&s).send().await;
    assert_eq!(again.status, StatusCode::UNAUTHORIZED);
    let login = h
        .post(
            "/api/v1/sr/auth/login",
            json!({"login": "alice", "password": PW}),
        )
        .send()
        .await;
    assert_ne!(login.status, StatusCode::OK);
}

#[tokio::test]
async fn delete_account_needs_a_session() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let anon = h
        .post(
            "/api/v1/sr/auth/account/delete",
            json!({"username": "alice", "password": PW}),
        )
        .send()
        .await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);
    assert_eq!(h.account(1).state, AccountState::Active);
}

#[tokio::test]
async fn change_email_confirms_through_the_verify_link() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    h.add_account(2, "bobby", "bobby@x.test", AccountState::Active);
    let s = h.login("alice").await;
    let start = |e: &str, pw: &str| {
        h.post(
            "/api/v1/sr/auth/email/change",
            json!({"new_email": e, "password": pw}),
        )
        .session(&s)
        .send()
    };
    assert_eq!(
        start("not-an-email", PW).await.status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        start("new@x.test", "wrong password").await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        start("alice@x.test", PW).await.status,
        StatusCode::BAD_REQUEST
    );
    // Someone else's address: 204 and no mail (no oracle).
    assert_eq!(
        start("bobby@x.test", PW).await.status,
        StatusCode::NO_CONTENT
    );
    assert!(h.mailer.sent().is_empty());
    h.clock.advance_ms(61_000);

    assert_eq!(start("New@X.test", PW).await.status, StatusCode::NO_CONTENT);
    let mails = h.wait_mail(2).await;
    assert_eq!(mails[0].to, "new@x.test");
    // The old address is told right away, with a cancel link (D-167).
    assert_eq!(mails[1].to, "alice@x.test");
    assert!(mails[1].text.contains("/email-change/cancel?token="));
    // Nothing changes until the link is used.
    assert_eq!(h.account(1).email, "alice@x.test");
    let token = link_token(&mails[0], "/verify-email/");
    let confirm = h
        .post("/api/v1/sr/auth/verify-email", json!({"token": token}))
        .send()
        .await;
    assert_eq!(confirm.status, StatusCode::NO_CONTENT, "{:?}", confirm.body);
    let acct = h.account(1);
    assert_eq!(acct.email, "new@x.test");
    assert!(acct.email_verified);
    // The old address is told, the link is single use, and the change is audited.
    let all = h.wait_mail(3).await;
    assert_eq!(all[2].to, "alice@x.test");
    let again = h
        .post("/api/v1/sr/auth/verify-email", json!({"token": token}))
        .send()
        .await;
    assert_eq!(again.status, StatusCode::BAD_REQUEST);
    assert!(
        h.store
            .audit_rows()
            .iter()
            .any(|(_, action, _)| action == "account.email_change")
    );
    // Logging in with the new address works.
    let login = h
        .post(
            "/api/v1/sr/auth/login",
            json!({"login": "new@x.test", "password": PW}),
        )
        .send()
        .await;
    assert_eq!(login.status, StatusCode::OK);
}

#[tokio::test]
async fn change_email_link_expires_and_loses_races() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    h.add_account(2, "bobby", "bobby@x.test", AccountState::Active);
    let s = h.login("alice").await;
    let start = |e: &str| {
        h.post(
            "/api/v1/sr/auth/email/change",
            json!({"new_email": e, "password": PW}),
        )
        .session(&s)
        .send()
    };
    assert_eq!(start("race@x.test").await.status, StatusCode::NO_CONTENT);
    let mails = h.wait_mail(1).await;
    let token = link_token(&mails[0], "/verify-email/");
    // Another account takes the address first.
    h.store.set_email(2, "race@x.test").await.unwrap();
    let r = h
        .post("/api/v1/sr/auth/verify-email", json!({"token": token}))
        .send()
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(h.account(1).email, "alice@x.test");

    // An expired link is refused.
    h.clock.advance_ms(61_000);
    assert_eq!(start("later@x.test").await.status, StatusCode::NO_CONTENT);
    // Mails: confirm + notice for each start.
    let mails = h.wait_mail(4).await;
    let token = link_token(&mails[2], "/verify-email/");
    h.clock.advance_ms(25 * 3_600_000);
    let r = h
        .post("/api/v1/sr/auth/verify-email", json!({"token": token}))
        .send()
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
}

// ---- email change: second factor and "This wasn't me" (D-167, F-03) ---------

/// The token of the cancel link in a notice mail.
fn cancel_token(m: &OutgoingMail) -> String {
    m.text
        .lines()
        .find(|l| l.contains("/email-change/cancel?token="))
        .unwrap()
        .rsplit("token=")
        .next()
        .unwrap()
        .to_owned()
}

async fn change_to(h: &H, session: &str, email: &str, mfa: Option<Value>) -> Resp {
    let mut body = json!({"new_email": email, "password": PW});
    if let Some(m) = mfa {
        body["mfa"] = m;
    }
    h.post("/api/v1/sr/auth/email/change", body)
        .session(session)
        .send()
        .await
}

#[tokio::test]
async fn email_change_needs_a_second_factor_when_two_factor_is_on() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let s = h.login("alice").await;
    let (secret, recovery) = enrol_totp(&h, &s).await;
    h.clock.advance_ms(61_000);

    // No proof, a wrong code and an unknown method are refused; no mail goes out.
    let none = change_to(&h, &s, "new@x.test", None).await;
    assert_eq!(none.status, StatusCode::BAD_REQUEST);
    assert_eq!(none.body["errors"]["mfa"]["_errors"][0]["code"], "required");
    let wrong = change_to(
        &h,
        &s,
        "new@x.test",
        Some(json!({"method": "totp", "code": "000000"})),
    )
    .await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST);
    assert_eq!(wrong.body["errors"]["mfa"]["_errors"][0]["code"], "invalid");
    assert!(h.mailer.sent().is_empty());

    // A valid code starts the change.
    let code = totp::current_code(&secret, "SIDE ROOM", "alice", h.now()).unwrap();
    let ok = change_to(
        &h,
        &s,
        "new@x.test",
        Some(json!({"method": "totp", "code": code})),
    )
    .await;
    assert_eq!(ok.status, StatusCode::NO_CONTENT, "{:?}", ok.body);
    assert_eq!(h.wait_mail(2).await.len(), 2);

    // The check counts for 15 minutes on this session: no code needed again.
    h.clock.advance_ms(61_000);
    assert_eq!(
        change_to(&h, &s, "newer@x.test", None).await.status,
        StatusCode::NO_CONTENT
    );
    // After that it is stale and a recovery code is accepted instead.
    h.clock.advance_ms(16 * 60_000);
    assert_eq!(
        change_to(&h, &s, "latest@x.test", None).await.status,
        StatusCode::BAD_REQUEST
    );
    let rec = change_to(
        &h,
        &s,
        "latest@x.test",
        Some(json!({"method": "recovery", "code": recovery[0]})),
    )
    .await;
    assert_eq!(rec.status, StatusCode::NO_CONTENT, "{:?}", rec.body);
}

#[tokio::test]
async fn a_login_with_two_factor_counts_as_a_fresh_check() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let s = h.login("alice").await;
    let (secret, _) = enrol_totp(&h, &s).await;
    h.clock.advance_ms(61_000);
    let mfa = login_expecting_mfa(&h).await;
    let code = totp::current_code(&secret, "SIDE ROOM", "alice", h.now()).unwrap();
    let r = h
        .post("/api/v1/sr/auth/2fa", totp_body(&code))
        .cookie("sr_mfa", &mfa)
        .send()
        .await;
    let fresh = r.cookie("sr_session").unwrap();
    assert_eq!(
        change_to(&h, &fresh, "new@x.test", None).await.status,
        StatusCode::NO_CONTENT
    );
    // The older session, which never passed a check, still has to.
    h.clock.advance_ms(61_000);
    assert_eq!(
        change_to(&h, &s, "other@x.test", None).await.status,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn email_change_without_two_factor_needs_no_proof() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let s = h.login("alice").await;
    assert_eq!(
        change_to(&h, &s, "new@x.test", None).await.status,
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
async fn this_wasnt_me_cancels_a_pending_change() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let s = h.login("alice").await;
    assert_eq!(
        change_to(&h, &s, "new@x.test", None).await.status,
        StatusCode::NO_CONTENT
    );
    let mails = h.wait_mail(2).await;
    let confirm = link_token(&mails[0], "/verify-email/");
    let cancel = cancel_token(&mails[1]);
    let r = h
        .post("/api/v1/sr/auth/email/cancel", json!({"token": cancel}))
        .send()
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{:?}", r.body);
    // The confirmation link no longer works and the address is unchanged.
    let late = h
        .post("/api/v1/sr/auth/verify-email", json!({"token": confirm}))
        .send()
        .await;
    assert_eq!(late.status, StatusCode::BAD_REQUEST);
    assert_eq!(h.account(1).email, "alice@x.test");
    // The session survives (nothing had completed) and the link is single use.
    assert_eq!(
        h.get("/api/v1/sr/auth/sessions")
            .session(&s)
            .send()
            .await
            .status,
        StatusCode::OK
    );
    let again = h
        .post("/api/v1/sr/auth/email/cancel", json!({"token": cancel}))
        .send()
        .await;
    assert_eq!(again.status, StatusCode::BAD_REQUEST);
    let junk = h
        .post("/api/v1/sr/auth/email/cancel", json!({"token": "nope"}))
        .send()
        .await;
    assert_eq!(junk.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn this_wasnt_me_after_completion_revokes_every_session() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let s = h.login("alice").await;
    let other = h.login("alice").await;
    assert_eq!(
        change_to(&h, &s, "new@x.test", None).await.status,
        StatusCode::NO_CONTENT
    );
    let mails = h.wait_mail(2).await;
    let confirm = link_token(&mails[0], "/verify-email/");
    let old_cancel = cancel_token(&mails[1]);
    let done = h
        .post("/api/v1/sr/auth/verify-email", json!({"token": confirm}))
        .send()
        .await;
    assert_eq!(done.status, StatusCode::NO_CONTENT);
    let mails = h.wait_mail(3).await;
    assert_eq!(mails[2].to, "alice@x.test");
    // The notice sent with the request was superseded by the one sent on completion.
    let stale = h
        .post("/api/v1/sr/auth/email/cancel", json!({"token": old_cancel}))
        .send()
        .await;
    assert_eq!(stale.status, StatusCode::BAD_REQUEST);
    let r = h
        .post(
            "/api/v1/sr/auth/email/cancel",
            json!({"token": cancel_token(&mails[2])}),
        )
        .send()
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{:?}", r.body);
    for session in [&s, &other] {
        assert_eq!(
            h.get("/api/v1/sr/auth/sessions")
                .session(session)
                .send()
                .await
                .status,
            StatusCode::UNAUTHORIZED
        );
    }
    assert!(
        h.store
            .audit_rows()
            .iter()
            .any(|(_, action, _)| action == "account.email_change_revoked")
    );
}

mod passkeys;

#[tokio::test]
async fn discord_start_uses_pkce() {
    let h = discord_harness();
    let r = h
        .get("/api/v1/sr/auth/discord/start?intent=login")
        .send()
        .await;
    let location = r.headers[header::LOCATION].to_str().unwrap();
    assert!(location.contains("code_challenge="));
    assert!(location.contains("code_challenge_method=S256"));
}

#[tokio::test]
async fn discord_link_status_start_and_unlink() {
    use sr_auth::store::{AuthStore, DiscordLink};
    let h = discord_harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    h.add_account(2, "bob", "bob@x.test", AccountState::Active);
    let s = h.login("alice").await;

    // Needs a session.
    let anon = h.get("/api/v1/sr/auth/discord").send().await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);

    let st = h.get("/api/v1/sr/auth/discord").session(&s).send().await;
    assert_eq!(st.status, StatusCode::OK);
    assert_eq!(st.body, json!({"enabled": true, "linked": false}));

    // Starting answers the authorize URL and sets the state cookie.
    let start = h
        .post("/api/v1/sr/auth/discord/link", json!({}))
        .session(&s)
        .send()
        .await;
    assert_eq!(start.status, StatusCode::OK);
    let url = start.body["url"].as_str().unwrap();
    let state = start.cookie("sr_oauth_state").expect("state cookie");
    assert!(url.starts_with("https://discord.com/oauth2/authorize?"));
    assert!(url.contains(&format!("state={state}")));
    assert!(url.contains("code_challenge_method=S256"));

    // Unlinking with nothing linked is refused; a wrong password is refused.
    let none = h
        .post("/api/v1/sr/auth/discord/unlink", json!({"password": PW}))
        .session(&s)
        .send()
        .await;
    assert_eq!(none.status, StatusCode::BAD_REQUEST);

    // Link through the store (the callback does the same after Discord answers).
    assert_eq!(
        h.store.link_discord(1, 77, "999").await.unwrap(),
        DiscordLink::Linked
    );
    assert_eq!(
        h.store.link_discord(2, 78, "999").await.unwrap(),
        DiscordLink::TakenByOther
    );
    assert_eq!(
        h.store.link_discord(1, 79, "888").await.unwrap(),
        DiscordLink::AlreadyLinked
    );
    let st = h.get("/api/v1/sr/auth/discord").session(&s).send().await;
    assert_eq!(st.body["linked"], true);
    // Linking twice through the API is refused up front.
    let again = h
        .post("/api/v1/sr/auth/discord/link", json!({}))
        .session(&s)
        .send()
        .await;
    assert_eq!(again.status, StatusCode::BAD_REQUEST);

    let wrong = h
        .post(
            "/api/v1/sr/auth/discord/unlink",
            json!({"password": "nope-nope-nope"}),
        )
        .session(&s)
        .send()
        .await;
    assert!(wrong.status.is_client_error());
    assert_eq!(
        h.get("/api/v1/sr/auth/discord")
            .session(&s)
            .send()
            .await
            .body["linked"],
        true
    );

    let ok = h
        .post("/api/v1/sr/auth/discord/unlink", json!({"password": PW}))
        .session(&s)
        .send()
        .await;
    assert_eq!(ok.status, StatusCode::NO_CONTENT);
    assert_eq!(
        h.get("/api/v1/sr/auth/discord")
            .session(&s)
            .send()
            .await
            .body["linked"],
        false
    );
    // Sign-in with the unlinked identity no longer finds the account, and nobody else can take it.
    assert!(h.store.find_by_discord("999").await.unwrap().is_none());
    assert_eq!(
        h.store.link_discord(2, 80, "999").await.unwrap(),
        DiscordLink::TakenByOther
    );
    // The owner can link it again.
    assert_eq!(
        h.store.link_discord(1, 81, "999").await.unwrap(),
        DiscordLink::Linked
    );
    assert!(h.store.find_by_discord("999").await.unwrap().is_some());
}

#[tokio::test]
async fn discord_link_routes_are_404_when_not_configured() {
    let h = harness();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let s = h.login("alice").await;
    let st = h.get("/api/v1/sr/auth/discord").session(&s).send().await;
    assert_eq!(st.body, json!({"enabled": false, "linked": false}));
    let start = h
        .post("/api/v1/sr/auth/discord/link", json!({}))
        .session(&s)
        .send()
        .await;
    assert_eq!(start.status, StatusCode::NOT_FOUND);
}

// ---- N-05: member bulk self-delete (D-324) ----------------------------------------------

struct MemSelfDelete {
    runs: std::sync::Mutex<Vec<super::SelfDeleteRun>>,
    visible: i64,
}

#[async_trait::async_trait]
impl super::SelfDeleteStore for MemSelfDelete {
    async fn count(
        &self,
        _u: i64,
        s: super::SelfDeleteScope,
    ) -> Result<super::SelfDeleteCounts, String> {
        Ok(super::SelfDeleteCounts {
            messages: if s.servers && s.dms {
                self.visible
            } else {
                self.visible - 1
            },
            reactions: 4,
            poll_votes: 2,
        })
    }
    async fn latest(&self, _u: i64) -> Result<Option<super::SelfDeleteRun>, String> {
        Ok(self.runs.lock().unwrap().last().cloned())
    }
    async fn request(
        &self,
        _u: i64,
        scope: super::SelfDeleteScope,
    ) -> Result<Result<super::SelfDeleteRun, super::SelfDeleteRefusal>, String> {
        let mut runs = self.runs.lock().unwrap();
        if let Some(last) = runs.last() {
            return Ok(Err(if last.state == "queued" {
                super::SelfDeleteRefusal::InProgress
            } else {
                super::SelfDeleteRefusal::TooSoon {
                    retry_after_secs: 7200,
                }
            }));
        }
        let run = super::SelfDeleteRun {
            id: 77,
            state: "queued".into(),
            scope,
            requested_ms: NOW_MS,
            finished_ms: None,
            hidden_count: None,
        };
        runs.push(run.clone());
        Ok(Ok(run))
    }
}

fn harness_sd() -> (H, Arc<MemSelfDelete>) {
    let sd = Arc::new(MemSelfDelete {
        runs: std::sync::Mutex::new(Vec::new()),
        visible: 12,
    });
    let sessions = Arc::new(MemorySessions::new());
    let clock = Arc::new(ManualClock::new(NOW_MS));
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let store = Arc::new(MemoryAuthStore::new(sessions.clone()));
    let mailer = Arc::new(MemoryMailer::new());
    let next = Arc::new(AtomicI64::new(10_000));
    let ids: super::IdSource = Arc::new(move || next.fetch_add(1, Ordering::SeqCst));
    let bus = Arc::new(sr_events::InProcessBus::new());
    let svc = Arc::new(
        AuthService::new(store.clone(), mailer.clone(), ids)
            .with_bus(bus.clone())
            .with_hasher(cheap_hasher())
            .with_config(AuthConfig::default()),
    );
    let state = AppState::builder()
        .sessions(sessions)
        .clock(clock.clone())
        .settings(settings)
        .extension(svc)
        .extension(Arc::new(super::SelfDeleteDeps { store: sd.clone() }))
        .build();
    let app = build_router(state.clone(), super::router());
    (
        H {
            app,
            state,
            store,
            mailer,
            clock,
            hasher: cheap_hasher(),
            bus,
        },
        sd,
    )
}

#[tokio::test]
async fn message_self_delete_counts_confirms_and_limits_runs() {
    let (h, sd) = harness_sd();
    h.add_account(1, "alice", "alice@x.test", AccountState::Active);
    let s = h.login("alice").await;
    let path = "/api/v1/sr/users/@me/messages/delete-all";
    // Anonymous: 401.
    let anon = h.get(path).send().await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);
    // The count for the confirm dialog, narrower with one place off.
    let all = h.get(path).session(&s).send().await;
    assert_eq!(all.status, StatusCode::OK, "{:?}", all.body);
    assert_eq!(all.body["count"], 12);
    assert_eq!(all.body["reactions"], 4);
    assert_eq!(all.body["poll_votes"], 2);
    let part = h.get(&format!("{path}?dms=false")).session(&s).send().await;
    assert_eq!(part.body["count"], 11);
    let body = |user: &str, pw: &str| json!({"confirm_username": user, "password": pw, "servers": true, "dms": true});
    // Typed username, password and at least one place are required; nothing is queued by a refusal.
    assert_eq!(
        h.post(path, body("someone", PW))
            .session(&s)
            .send()
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        h.post(path, body("alice", "wrong password"))
            .session(&s)
            .send()
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    let none = h
        .post(
            path,
            json!({"confirm_username": "alice", "password": PW, "servers": false, "dms": false}),
        )
        .session(&s)
        .send()
        .await;
    assert_eq!(none.status, StatusCode::BAD_REQUEST);
    assert!(sd.runs.lock().unwrap().is_empty());
    // The run is accepted; a second one while it is open, and one after it ran, are 409.
    let ok = h.post(path, body("ALICE", PW)).session(&s).send().await;
    assert_eq!(ok.status, StatusCode::ACCEPTED, "{:?}", ok.body);
    assert_eq!(ok.body["state"], "queued");
    let open = h.post(path, body("alice", PW)).session(&s).send().await;
    assert_eq!(open.status, StatusCode::CONFLICT);
    sd.runs.lock().unwrap()[0].state = "done".into();
    let soon = h.post(path, body("alice", PW)).session(&s).send().await;
    assert_eq!(soon.status, StatusCode::CONFLICT);
    assert!(soon.body["message"].as_str().unwrap().contains("24 hours"));
    let last = h.get(path).session(&s).send().await;
    assert_eq!(last.body["last"]["id"], "77");
}
