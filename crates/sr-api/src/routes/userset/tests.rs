//! Endpoint tests for the data export and the connections (LNG-USET-020/024): one-shot
//! requests with a session cookie over in-memory repositories and a scripted provider.

use super::connections::ConnectionsDeps;
use super::data_export::{DataExportDeps, DataExportStore, ExportRecord, Refusal};
use super::router;
use crate::{
    ApiSettings, AppState,
    auth::{csrf, memory::MemorySessions},
    build_router,
    routes::social::{DynRepo, SocialDeps},
};
use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Method, Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_chat::memory::MemoryBackend;
use sr_chat::social::SocialLimits;
use sr_chat::social::memory::MemoryRepo;
use sr_chat::social::profile::Profile;
use sr_chat::social::repo::SocialRepository;
use sr_core::{Clock, ManualClock, UserId, config::ConnectionsConfig, config::Secret};
use sr_events::InProcessBus;
use sr_net::{
    ApiHop, ApiMethod, HopRequest, HopResponse, HttpTransport, NetError, Policy, Resolver,
    SafeClient,
};
use sr_storage::{ObjectStorage, ObjectStoreStorage};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

const T0: i64 = 1_800_000_000_000;
const ALICE: i64 = 1;
const BOB: i64 = 2;

// ---- fakes ------------------------------------------------------------------------------

/// Every host resolves to a public address unless listed otherwise.
struct Dns(Mutex<HashMap<String, IpAddr>>);
#[async_trait]
impl Resolver for Dns {
    async fn resolve(&self, host: &str, _port: u16) -> Result<Vec<IpAddr>, String> {
        let m = self.0.lock().unwrap();
        Ok(vec![
            m.get(host)
                .copied()
                .unwrap_or_else(|| "93.184.216.34".parse().unwrap()),
        ])
    }
}

/// Answers API calls from a table keyed by URL without the query; records what was sent.
#[derive(Default)]
struct Provider {
    routes: Mutex<HashMap<String, (u16, String)>>,
    seen: Mutex<Vec<ApiHop>>,
}

impl Provider {
    fn on(&self, url: &str, status: u16, body: &str) {
        self.routes
            .lock()
            .unwrap()
            .insert(url.to_owned(), (status, body.to_owned()));
    }
    fn calls(&self, url: &str) -> Vec<ApiHop> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|h| strip(h.call.url.as_str()) == url)
            .cloned()
            .collect()
    }
}

fn strip(url: &str) -> &str {
    url.split(['?', '#']).next().unwrap_or(url)
}

#[async_trait]
impl HttpTransport for Provider {
    async fn get(&self, _req: &HopRequest) -> Result<HopResponse, NetError> {
        Err(NetError::Http("get not scripted".into()))
    }
    async fn send(&self, req: &ApiHop) -> Result<HopResponse, NetError> {
        self.seen.lock().unwrap().push(req.clone());
        let (status, body) = self
            .routes
            .lock()
            .unwrap()
            .get(strip(req.call.url.as_str()))
            .cloned()
            .ok_or_else(|| NetError::Http("no route".into()))?;
        Ok(HopResponse {
            status,
            body: body.into_bytes(),
            ..HopResponse::default()
        })
    }
}

/// An in-memory export store with the same rules as the Postgres one.
#[derive(Default)]
struct Exports {
    rows: Mutex<Vec<(i64, ExportRecord)>>,
    next: Mutex<i64>,
    now_ms: Mutex<i64>,
}

#[async_trait]
impl DataExportStore for Exports {
    async fn latest(&self, user: i64) -> Result<Option<ExportRecord>, String> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(u, _)| *u == user)
            .map(|(_, r)| r.clone()))
    }
    async fn get(&self, user: i64, id: i64) -> Result<Option<ExportRecord>, String> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|(u, r)| *u == user && r.id == id)
            .map(|(_, r)| r.clone()))
    }
    async fn request(&self, user: i64) -> Result<Result<ExportRecord, Refusal>, String> {
        let now = *self.now_ms.lock().unwrap();
        let mut rows = self.rows.lock().unwrap();
        if let Some((_, r)) = rows
            .iter()
            .rev()
            .find(|(u, r)| *u == user && matches!(r.state.as_str(), "pending" | "building"))
        {
            return Ok(Err(Refusal::InProgress(r.clone())));
        }
        if let Some((_, r)) = rows
            .iter()
            .rev()
            .find(|(u, r)| *u == user && r.state != "failed")
        {
            let wait = r.requested_ms + 24 * 3_600_000 - now;
            if wait > 0 {
                return Ok(Err(Refusal::TooSoon {
                    retry_after_secs: wait / 1000,
                }));
            }
        }
        let mut next = self.next.lock().unwrap();
        *next += 1;
        let rec = ExportRecord {
            id: 700 + *next,
            state: "pending".into(),
            requested_ms: now,
            ready_ms: None,
            expires_ms: None,
            storage_key: None,
            size_bytes: None,
        };
        rows.push((user, rec.clone()));
        Ok(Ok(rec))
    }
}

// ---- environment ------------------------------------------------------------------------

struct Env {
    app: Router,
    sessions: Arc<MemorySessions>,
    clock: Arc<ManualClock>,
    provider: Arc<Provider>,
    dns: Arc<Dns>,
    exports: Arc<Exports>,
    storage: Arc<ObjectStoreStorage>,
    social: Arc<SocialDeps>,
    repo: Arc<MemoryRepo>,
}

fn secret(id: &str) -> sr_core::config::ConnectionProviderConfig {
    sr_core::config::ConnectionProviderConfig {
        client_id: id.into(),
        client_secret: Secret::new(format!("{id}-secret")),
        enabled: None,
    }
}

async fn env() -> Env {
    let clock = Arc::new(ManualClock::new(T0));
    let dyn_clock: Arc<dyn Clock> = clock.clone();
    let sessions = Arc::new(MemorySessions::new());
    let repo = Arc::new(MemoryRepo::default());
    for uid in [ALICE, BOB] {
        let u = UserId::new(uid as u64);
        repo.add_user(u);
        repo.store.lock().unwrap().profiles.insert(
            u,
            Profile {
                user: u,
                ..Profile::default()
            },
        );
    }
    let backend = MemoryBackend::new();
    let bus = Arc::new(InProcessBus::new());
    let social = Arc::new(
        SocialDeps::new(
            DynRepo(repo.clone() as Arc<dyn SocialRepository>),
            Arc::new(backend),
            bus,
            dyn_clock.clone(),
            SocialLimits::default(),
            1,
            3,
        )
        .unwrap(),
    );

    let provider = Arc::new(Provider::default());
    let dns = Arc::new(Dns(Mutex::default()));
    let client = SafeClient::new(Policy::default(), dns.clone(), provider.clone());
    let cfg = ConnectionsConfig {
        github: secret("gh"),
        spotify: secret("sp"),
        steam: sr_core::config::ConnectionProviderConfig::default(),
        ..ConnectionsConfig::default()
    };
    let connections = Arc::new(ConnectionsDeps::new(
        "https://room.test/",
        &cfg,
        ("", ""),
        client,
        dyn_clock.clone(),
    ));

    let exports = Arc::new(Exports::default());
    *exports.now_ms.lock().unwrap() = T0;
    let storage = Arc::new(ObjectStoreStorage::in_memory());
    let export_deps = Arc::new(DataExportDeps {
        store: exports.clone(),
        storage: storage.clone(),
        clock: dyn_clock.clone(),
    });

    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .sessions(sessions.clone())
        .clock(dyn_clock)
        .settings(settings)
        .extension(social.clone())
        .extension(connections)
        .extension(export_deps)
        .build();
    Env {
        app: build_router(state, router()),
        sessions,
        clock,
        provider,
        dns,
        exports,
        storage,
        social,
        repo,
    }
}

struct Resp {
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
    raw: Vec<u8>,
}

impl Env {
    /// A session cookie value for `user`.
    async fn login(&self, user: i64) -> String {
        let t = SessionToken::generate().unwrap();
        let now = self.clock.now_unix_ms() / 1000;
        self.sessions
            .create(SessionRecord {
                id: 5_000 + user + now % 1000,
                user_id: user,
                token_hash: t.hash(),
                created_at: now,
                last_seen_at: now,
                expires_at: now + 86_400,
                device_name: None,
            })
            .await
            .unwrap();
        t.expose().to_owned()
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        session: Option<&str>,
        body: Option<Value>,
    ) -> Resp {
        let mut b = Request::builder()
            .method(method)
            .uri(format!("/api/v1{path}"));
        if let Some(s) = session {
            b = b
                .header(header::COOKIE, format!("sr_session={s}"))
                .header("x-sr-csrf", csrf::token_for(s));
        }
        let req = match body {
            Some(v) => b
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(v.to_string()))
                .unwrap(),
            None => b.body(Body::empty()).unwrap(),
        };
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let (parts, body) = resp.into_parts();
        let raw = body.collect().await.unwrap().to_bytes().to_vec();
        Resp {
            status: parts.status,
            headers: parts.headers,
            body: serde_json::from_slice(&raw).unwrap_or(Value::Null),
            raw,
        }
    }

    async fn get(&self, path: &str, s: &str) -> Resp {
        self.call(Method::GET, path, Some(s), None).await
    }
    async fn post(&self, path: &str, s: &str, body: Value) -> Resp {
        self.call(Method::POST, path, Some(s), Some(body)).await
    }
    async fn patch(&self, path: &str, s: &str, body: Value) -> Resp {
        self.call(Method::PATCH, path, Some(s), Some(body)).await
    }
    async fn delete(&self, path: &str, s: &str) -> Resp {
        self.call(Method::DELETE, path, Some(s), None).await
    }

    /// Start a flow and return the `state` parameter of the provider URL.
    async fn start(&self, provider: &str, s: &str) -> (String, url::Url) {
        let r = self
            .post(
                &format!("/users/@me/connections/{provider}/authorize"),
                s,
                json!({}),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
        let u = url::Url::parse(r.body["url"].as_str().unwrap()).unwrap();
        let state = u
            .query_pairs()
            .find(|(k, _)| k == "state")
            .map(|(_, v)| v.into_owned())
            .or_else(|| {
                // Steam carries the state inside return_to.
                let rt = u
                    .query_pairs()
                    .find(|(k, _)| k == "openid.return_to")?
                    .1
                    .into_owned();
                url::Url::parse(&rt)
                    .ok()?
                    .query_pairs()
                    .find(|(k, _)| k == "state")
                    .map(|(_, v)| v.into_owned())
            })
            .expect("state in the authorize URL");
        (state, u)
    }

    /// The callback, with or without a session; returns the redirect target.
    async fn callback(&self, provider: &str, query: &str, s: Option<&str>) -> String {
        let mut b = Request::builder()
            .method(Method::GET)
            .uri(format!("/api/v1/connections/{provider}/callback?{query}"));
        if let Some(s) = s {
            b = b.header(header::COOKIE, format!("sr_session={s}"));
        }
        let resp = self
            .app
            .clone()
            .oneshot(b.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FOUND);
        resp.headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned()
    }
}

const GH_TOKEN: &str = "https://github.com/login/oauth/access_token";
const GH_USER: &str = "https://api.github.com/user";

fn github_ok(env: &Env) {
    env.provider.on(
        GH_TOKEN,
        200,
        r#"{"access_token":"tok123","token_type":"bearer"}"#,
    );
    env.provider.on(
        GH_USER,
        200,
        r#"{"id":583231,"login":"octocat","public_repos":8,"followers":3938,"following":9}"#,
    );
}

// ---- connections --------------------------------------------------------------------------

#[tokio::test]
async fn the_list_offers_configured_providers_and_every_link_only_one() {
    let e = env().await;
    let s = e.login(ALICE).await;
    let r = e.get("/users/@me/connections", &s).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body["connections"], json!([]));
    let p = |id: &str| {
        r.body["providers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == id)
            .cloned()
            .unwrap()
    };
    assert_eq!(p("github")["available"], true);
    assert_eq!(p("github")["verified"], true);
    assert_eq!(p("spotify")["available"], true);
    assert_eq!(
        p("twitch")["available"],
        false,
        "no OAuth application configured"
    );
    assert_eq!(p("youtube")["available"], false);
    assert_eq!(p("steam")["available"], true, "Steam needs no secret");
    assert_eq!(p("roblox")["verified"], false);
    assert_eq!(p("roblox")["available"], true);
    assert_eq!(p("website")["requires_url"], true);
    // Needs a session.
    assert_eq!(
        e.call(Method::GET, "/users/@me/connections", None, None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn authorize_builds_the_provider_url_for_configured_providers_only() {
    let e = env().await;
    let s = e.login(ALICE).await;
    let (state, u) = e.start("github", &s).await;
    assert_eq!(u.host_str(), Some("github.com"));
    let q: HashMap<String, String> = u.query_pairs().map(|(k, v)| (k.into(), v.into())).collect();
    assert_eq!(q["client_id"], "gh");
    assert_eq!(q["response_type"], "code");
    assert_eq!(
        q["redirect_uri"],
        "https://room.test/api/v1/connections/github/callback"
    );
    assert_eq!(q["scope"], "read:user");
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(q["state"], state);
    assert!(state.len() >= 43, "256 bits of state");
    assert!(
        !q.values().any(|v| v.contains("gh-secret")),
        "the secret never leaves the server"
    );

    for p in ["twitch", "nope", "roblox"] {
        let r = e
            .post(
                &format!("/users/@me/connections/{p}/authorize"),
                &s,
                json!({}),
            )
            .await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "{p}");
    }
    // Starting is bounded per account.
    for _ in 0..4 {
        e.start("github", &s).await;
    }
    let r = e
        .post("/users/@me/connections/github/authorize", &s, json!({}))
        .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn a_github_link_end_to_end() {
    let e = env().await;
    github_ok(&e);
    let s = e.login(ALICE).await;
    let (state, u) = e.start("github", &s).await;
    let challenge = u
        .query_pairs()
        .find(|(k, _)| k == "code_challenge")
        .unwrap()
        .1
        .into_owned();

    let to = e
        .callback("github", &format!("code=abc&state={state}"), Some(&s))
        .await;
    assert_eq!(to, "/settings/user/connections?connected=github");

    // The code went to the token endpoint with the verifier and the secret, and the token
    // went to the API as a bearer.
    let tok = e.provider.calls(GH_TOKEN);
    assert_eq!(tok.len(), 1);
    let ApiMethod::PostForm(form) = &tok[0].call.method else {
        panic!("token exchange must be a form POST");
    };
    let f: HashMap<&str, &str> = form.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    assert_eq!(f["code"], "abc");
    assert_eq!(f["grant_type"], "authorization_code");
    assert_eq!(f["client_secret"], "gh-secret");
    assert_eq!(
        f["redirect_uri"],
        "https://room.test/api/v1/connections/github/callback"
    );
    use sha2::{Digest, Sha256};
    assert_eq!(
        data_encoding::BASE64URL_NOPAD.encode(&Sha256::digest(f["code_verifier"])),
        challenge,
        "the verifier matches the challenge that was sent"
    );
    let api = e.provider.calls(GH_USER);
    assert!(
        api[0]
            .call
            .headers
            .contains(&("Authorization".into(), "Bearer tok123".into()))
    );

    let list = e.get("/users/@me/connections", &s).await;
    let c = &list.body["connections"][0];
    assert_eq!(c["type"], "github");
    assert_eq!(c["id"], "583231");
    assert_eq!(c["name"], "octocat");
    assert_eq!(c["verified"], true);
    assert_eq!(c["url"], "https://github.com/octocat");
    assert_eq!(
        c["stats"],
        json!({"public_repos": 8, "followers": 3938, "following": 9})
    );
    assert!(
        !String::from_utf8_lossy(&list.raw).contains("tok123"),
        "no token is kept or shown"
    );
}

#[tokio::test]
async fn the_state_is_single_use_and_bound_to_the_account() {
    let e = env().await;
    github_ok(&e);
    let (a, b) = (e.login(ALICE).await, e.login(BOB).await);

    // Bob cannot finish Alice's flow, and the attempt burns the state.
    let (state, _) = e.start("github", &a).await;
    let to = e
        .callback("github", &format!("code=abc&state={state}"), Some(&b))
        .await;
    assert_eq!(to, "/settings/user/connections?error=state&provider=github");
    let to = e
        .callback("github", &format!("code=abc&state={state}"), Some(&a))
        .await;
    assert!(to.contains("error=state"), "{to}");

    // A state replayed after success is refused.
    let (state, _) = e.start("github", &a).await;
    assert!(
        e.callback("github", &format!("code=abc&state={state}"), Some(&a))
            .await
            .contains("connected=github")
    );
    assert!(
        e.callback("github", &format!("code=abc&state={state}"), Some(&a))
            .await
            .contains("error=state")
    );

    // Another provider's callback does not accept it, and an unknown or missing state fails.
    let (state, _) = e.start("github", &a).await;
    assert!(
        e.callback("spotify", &format!("code=abc&state={state}"), Some(&a))
            .await
            .contains("error=state")
    );
    assert!(
        e.callback("github", "code=abc&state=guess", Some(&a))
            .await
            .contains("error=state")
    );
    assert!(
        e.callback("github", "code=abc", Some(&a))
            .await
            .contains("error=state")
    );

    // No session at all.
    let (state, _) = e.start("github", &a).await;
    assert!(
        e.callback("github", &format!("code=abc&state={state}"), None)
            .await
            .contains("error=signed_out")
    );

    // Expired.
    let (state, _) = e.start("github", &a).await;
    e.clock.advance_ms(11 * 60 * 1000);
    let session = e.login(ALICE).await;
    assert!(
        e.callback("github", &format!("code=abc&state={state}"), Some(&session))
            .await
            .contains("error=state")
    );
}

#[tokio::test]
async fn provider_failures_come_back_as_error_codes() {
    let e = env().await;
    let s = e.login(ALICE).await;

    let (state, _) = e.start("github", &s).await;
    let to = e
        .callback(
            "github",
            &format!("error=access_denied&state={state}"),
            Some(&s),
        )
        .await;
    assert!(to.contains("error=denied"), "{to}");

    // The token endpoint refuses the code (GitHub answers 200 with an error document).
    e.provider
        .on(GH_TOKEN, 200, r#"{"error":"bad_verification_code"}"#);
    let (state, _) = e.start("github", &s).await;
    assert!(
        e.callback("github", &format!("code=x&state={state}"), Some(&s))
            .await
            .contains("error=exchange")
    );
    e.provider.on(GH_TOKEN, 400, r#"{"error":"invalid_grant"}"#);
    let (state, _) = e.start("github", &s).await;
    assert!(
        e.callback("github", &format!("code=x&state={state}"), Some(&s))
            .await
            .contains("error=exchange")
    );
    let (state, _) = e.start("github", &s).await;
    assert!(
        e.callback("github", &format!("state={state}"), Some(&s))
            .await
            .contains("error=exchange"),
        "no code"
    );

    // A token but an unusable profile.
    e.provider.on(GH_TOKEN, 200, r#"{"access_token":"t"}"#);
    e.provider.on(GH_USER, 200, r#"{"message":"nope"}"#);
    let (state, _) = e.start("github", &s).await;
    assert!(
        e.callback("github", &format!("code=x&state={state}"), Some(&s))
            .await
            .contains("error=profile")
    );
    assert_eq!(
        e.get("/users/@me/connections", &s).await.body["connections"],
        json!([])
    );
}

#[tokio::test]
async fn a_provider_identity_belongs_to_one_account() {
    let e = env().await;
    github_ok(&e);
    let (a, b) = (e.login(ALICE).await, e.login(BOB).await);
    let (state, _) = e.start("github", &a).await;
    assert!(
        e.callback("github", &format!("code=x&state={state}"), Some(&a))
            .await
            .contains("connected=github")
    );
    let (state, _) = e.start("github", &b).await;
    let to = e
        .callback("github", &format!("code=x&state={state}"), Some(&b))
        .await;
    assert!(to.contains("error=taken"), "{to}");
    assert_eq!(
        e.get("/users/@me/connections", &b).await.body["connections"],
        json!([])
    );

    // Linking again keeps the member's choices on the card.
    e.call(
        Method::PATCH,
        "/users/@me/connections/github/583231",
        Some(&a),
        Some(json!({"visible": false, "show_activity": false})),
    )
    .await;
    let (state, _) = e.start("github", &a).await;
    e.callback("github", &format!("code=x&state={state}"), Some(&a))
        .await;
    let c = &e.get("/users/@me/connections", &a).await.body["connections"][0];
    assert_eq!(c["visible"], false);
    assert_eq!(c["show_activity"], false);
}

#[tokio::test]
async fn spotify_uses_basic_credentials_without_a_secret_in_the_form() {
    let e = env().await;
    e.provider.on(
        "https://accounts.spotify.com/api/token",
        200,
        r#"{"access_token":"sp-token"}"#,
    );
    e.provider.on(
        "https://api.spotify.com/v1/me",
        200,
        r#"{"id":"wizzler","display_name":"Wiz","followers":{"total":12}}"#,
    );
    let s = e.login(ALICE).await;
    let (state, _) = e.start("spotify", &s).await;
    assert!(
        e.callback("spotify", &format!("code=x&state={state}"), Some(&s))
            .await
            .contains("connected=spotify")
    );
    let tok = &e.provider.calls("https://accounts.spotify.com/api/token")[0];
    assert_eq!(tok.call.basic, Some(("sp".into(), "sp-secret".into())));
    let ApiMethod::PostForm(form) = &tok.call.method else {
        panic!()
    };
    assert!(!form.iter().any(|(k, _)| k == "client_secret"));
    let c = &e.get("/users/@me/connections", &s).await.body["connections"][0];
    assert_eq!(c["name"], "Wiz");
    assert_eq!(c["stats"], json!({"followers": 12}));
}

#[tokio::test]
async fn a_provider_host_that_resolves_privately_is_never_contacted() {
    let e = env().await;
    github_ok(&e);
    e.dns
        .0
        .lock()
        .unwrap()
        .insert("github.com".into(), "127.0.0.1".parse().unwrap());
    let s = e.login(ALICE).await;
    let (state, _) = e.start("github", &s).await;
    let to = e
        .callback("github", &format!("code=x&state={state}"), Some(&s))
        .await;
    assert!(to.contains("error=exchange"), "{to}");
    assert!(
        e.provider.calls(GH_TOKEN).is_empty(),
        "the pinned-address check ran first"
    );
}

#[tokio::test]
async fn steam_is_verified_with_steam_before_it_is_believed() {
    let e = env().await;
    let s = e.login(ALICE).await;
    let (state, u) = e.start("steam", &s).await;
    assert_eq!(u.host_str(), Some("steamcommunity.com"));
    let q: HashMap<String, String> = u.query_pairs().map(|(k, v)| (k.into(), v.into())).collect();
    assert_eq!(q["openid.mode"], "checkid_setup");
    assert_eq!(q["openid.realm"], "https://room.test");
    let return_to = format!("https://room.test/api/v1/connections/steam/callback?state={state}");
    assert_eq!(q["openid.return_to"], return_to);

    let claimed = "https://steamcommunity.com/openid/id/76561197960435530";
    let answer = |state: &str, rt: &str| {
        let enc = |v: &str| url::form_urlencoded::byte_serialize(v.as_bytes()).collect::<String>();
        format!(
            "state={state}&openid.ns={}&openid.mode=id_res&openid.op_endpoint={}&openid.claimed_id={}&openid.identity={}&openid.return_to={}&openid.response_nonce=n&openid.assoc_handle=h&openid.signed=signed%2Cop_endpoint%2Cclaimed_id%2Cidentity%2Creturn_to%2Cresponse_nonce%2Cassoc_handle&openid.sig=c2ln",
            enc("http://specs.openid.net/auth/2.0"),
            enc("https://steamcommunity.com/openid/login"),
            enc(claimed),
            enc(claimed),
            enc(rt),
        )
    };
    let steam = "https://steamcommunity.com/openid/login";

    // Steam says the signature is not valid: refused.
    e.provider.on(
        steam,
        200,
        "ns:http://specs.openid.net/auth/2.0\nis_valid:false\n",
    );
    assert!(
        e.callback("steam", &answer(&state, &return_to), Some(&s))
            .await
            .contains("error=exchange")
    );

    // A return_to for another site is refused before Steam is even asked.
    let (state2, _) = e.start("steam", &s).await;
    let before = e.provider.calls(steam).len();
    let evil = format!("https://evil.test/cb?state={state2}");
    assert!(
        e.callback("steam", &answer(&state2, &evil), Some(&s))
            .await
            .contains("error=exchange")
    );
    assert_eq!(e.provider.calls(steam).len(), before);

    // A valid answer links the Steam id.
    e.provider.on(
        steam,
        200,
        "ns:http://specs.openid.net/auth/2.0\nis_valid:true\n",
    );
    let (state3, _) = e.start("steam", &s).await;
    let rt3 = format!("https://room.test/api/v1/connections/steam/callback?state={state3}");
    let to = e.callback("steam", &answer(&state3, &rt3), Some(&s)).await;
    assert!(to.contains("connected=steam"), "{to}");
    let sent = e.provider.calls(steam).last().cloned().unwrap();
    let ApiMethod::PostForm(form) = &sent.call.method else {
        panic!()
    };
    assert!(form.contains(&("openid.mode".into(), "check_authentication".into())));
    let c = &e.get("/users/@me/connections", &s).await.body["connections"][0];
    assert_eq!(c["id"], "76561197960435530");
    assert_eq!(
        c["url"],
        "https://steamcommunity.com/profiles/76561197960435530"
    );
    assert_eq!(c["verified"], true);
}

#[tokio::test]
async fn unverified_links_are_validated_edited_and_removed() {
    let e = env().await;
    let s = e.login(ALICE).await;
    let put = |body: Value| {
        e.call(
            Method::PUT,
            "/users/@me/connections/link",
            Some(&s),
            Some(body),
        )
    };

    assert_eq!(
        put(json!({"provider": "github", "name": "x"})).await.status,
        StatusCode::BAD_REQUEST,
        "verified providers cannot be typed in"
    );
    assert_eq!(
        put(json!({"provider": "nope", "name": "x"})).await.status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        put(json!({"provider": "website"})).await.status,
        StatusCode::BAD_REQUEST,
        "a website needs a URL"
    );
    for bad in [
        "http://plain.test",
        "javascript:alert(1)",
        "https://user:pw@x.test/",
        "not a url",
    ] {
        assert_eq!(
            put(json!({"provider": "website", "url": bad})).await.status,
            StatusCode::BAD_REQUEST,
            "{bad}"
        );
    }
    assert_eq!(
        put(json!({"provider": "roblox"})).await.status,
        StatusCode::BAD_REQUEST,
        "needs a name"
    );

    let w = put(json!({"provider": "website", "url": "https://example.com/me"})).await;
    assert_eq!(w.status, StatusCode::OK, "{:?}", w.body);
    assert_eq!(w.body["name"], "example.com");
    assert_eq!(w.body["verified"], false);
    let r = put(json!({"provider": "roblox", "name": "Builder99"})).await;
    assert_eq!(r.body["id"], "Builder99");

    // Rename and hide the link; verified names cannot be edited.
    let patch = |path: &'static str, body: Value| e.patch(path, &s, body);
    let p = patch(
        "/users/@me/connections/roblox/Builder99",
        json!({"name": "Builder100", "visible": false}),
    )
    .await;
    assert_eq!(p.status, StatusCode::OK, "{:?}", p.body);
    assert_eq!(p.body["name"], "Builder100");
    assert_eq!(p.body["visible"], false);
    assert_eq!(
        patch(
            "/users/@me/connections/roblox/nobody",
            json!({"visible": true})
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        patch(
            "/users/@me/connections/roblox/Builder99",
            json!({"name": ""})
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );

    github_ok(&e);
    let (state, _) = e.start("github", &s).await;
    e.callback("github", &format!("code=x&state={state}"), Some(&s))
        .await;
    assert_eq!(
        patch(
            "/users/@me/connections/github/583231",
            json!({"name": "renamed"})
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );

    // Removal.
    let del = |path: &'static str| e.delete(path, &s);
    assert_eq!(
        del("/users/@me/connections/roblox/Builder99").await.status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        del("/users/@me/connections/roblox/Builder99").await.status,
        StatusCode::NOT_FOUND
    );
    let left = e.get("/users/@me/connections", &s).await.body["connections"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(left, 2);
}

#[tokio::test]
async fn others_see_only_visible_connections_and_stats_only_when_shown() {
    let e = env().await;
    github_ok(&e);
    let s = e.login(ALICE).await;
    let (state, _) = e.start("github", &s).await;
    e.callback("github", &format!("code=x&state={state}"), Some(&s))
        .await;
    let alice = UserId::new(ALICE as u64);
    let bob = UserId::new(BOB as u64);

    let seen = e.social.service.get_profile(bob, alice).await.unwrap();
    let o = crate::routes::social::model::ConnectionObject::from(&seen.connections[0]);
    assert_eq!(o.stats["followers"], 3938);

    e.call(
        Method::PATCH,
        "/users/@me/connections/github/583231",
        Some(&s),
        Some(json!({"show_activity": false})),
    )
    .await;
    let seen = e.social.service.get_profile(bob, alice).await.unwrap();
    let o = crate::routes::social::model::ConnectionObject::from(&seen.connections[0]);
    assert_eq!(o.stats, json!({}), "the stats line is hidden from others");
    // The owner still sees the numbers.
    let mine = &e.get("/users/@me/connections", &s).await.body["connections"][0];
    assert_eq!(mine["stats"]["followers"], 3938);

    e.call(
        Method::PATCH,
        "/users/@me/connections/github/583231",
        Some(&s),
        Some(json!({"visible": false})),
    )
    .await;
    assert!(
        e.social
            .service
            .get_profile(bob, alice)
            .await
            .unwrap()
            .connections
            .is_empty()
    );
    let _ = &e.repo;
}

#[tokio::test]
async fn stats_refresh_is_token_free_limited_and_github_reddit_only() {
    let e = env().await;
    github_ok(&e);
    let s = e.login(ALICE).await;
    let (state, _) = e.start("github", &s).await;
    e.callback("github", &format!("code=x&state={state}"), Some(&s))
        .await;

    e.provider.on(
        "https://api.github.com/user/583231",
        200,
        r#"{"id":583231,"login":"octocat","public_repos":9,"followers":4000,"following":9}"#,
    );
    let r = e
        .post(
            "/users/@me/connections/github/583231/refresh",
            &s,
            json!({}),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    assert_eq!(r.body["stats"]["followers"], 4000);
    assert!(
        e.provider.calls("https://api.github.com/user/583231")[0]
            .call
            .headers
            .is_empty(),
        "no credentials are sent for a public refresh"
    );
    let r = e
        .post(
            "/users/@me/connections/github/583231/refresh",
            &s,
            json!({}),
        )
        .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(r.headers.contains_key(header::RETRY_AFTER));
    e.clock.advance_ms(61_000);
    assert_eq!(
        e.post(
            "/users/@me/connections/github/583231/refresh",
            &s,
            json!({})
        )
        .await
        .status,
        StatusCode::OK
    );
    // A typed link cannot be refreshed.
    e.call(
        Method::PUT,
        "/users/@me/connections/link",
        Some(&s),
        Some(json!({"provider": "roblox", "name": "b"})),
    )
    .await;
    e.clock.advance_ms(61_000);
    assert_eq!(
        e.post("/users/@me/connections/roblox/b/refresh", &s, json!({}))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
}

// ---- data export ----------------------------------------------------------------------------

#[tokio::test]
async fn an_export_is_requested_once_built_downloaded_and_expires() {
    let e = env().await;
    let (a, b) = (e.login(ALICE).await, e.login(BOB).await);

    let none = e.get("/users/@me/data-export", &a).await;
    assert_eq!(none.body["state"], "none");

    let r = e.post("/users/@me/data-export", &a, json!({})).await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{:?}", r.body);
    assert_eq!(r.body["state"], "pending");
    assert!(r.body["download_url"].is_null());
    let id: i64 = r.body["id"].as_str().unwrap().parse().unwrap();
    // One at a time.
    let again = e.post("/users/@me/data-export", &a, json!({})).await;
    assert_eq!(again.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        again.body["errors"]["export"]["_errors"][0]["code"],
        "in_progress"
    );
    // Not downloadable before it is built.
    let path = format!("/users/@me/data-export/{id}/download");
    assert_eq!(e.get(&path, &a).await.status, StatusCode::NOT_FOUND);

    // The job finishes: the zip is in storage and the record is ready for 7 days.
    e.storage
        .put(
            "exports/1/zip",
            bytes::Bytes::from_static(b"PK\x03\x04fake"),
        )
        .await
        .unwrap();
    {
        let mut rows = e.exports.rows.lock().unwrap();
        let rec = &mut rows[0].1;
        rec.state = "ready".into();
        rec.ready_ms = Some(T0);
        rec.expires_ms = Some(T0 + 7 * 86_400_000);
        rec.storage_key = Some("exports/1/zip".into());
        rec.size_bytes = Some(8);
    }
    let st = e.get("/users/@me/data-export", &a).await;
    assert_eq!(st.body["state"], "ready");
    assert_eq!(st.body["download_url"], path.as_str());
    assert_eq!(st.body["size_bytes"], 8);

    let dl = e.get(&path, &a).await;
    assert_eq!(dl.status, StatusCode::OK);
    assert_eq!(dl.raw, b"PK\x03\x04fake");
    assert_eq!(dl.headers[header::CONTENT_TYPE], "application/zip");
    assert!(
        dl.headers[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .starts_with("attachment")
    );
    assert_eq!(dl.headers[header::CACHE_CONTROL], "private, no-store");
    // Nobody else can fetch it.
    assert_eq!(e.get(&path, &b).await.status, StatusCode::NOT_FOUND);
    assert_eq!(
        e.call(Method::GET, &path, None, None).await.status,
        StatusCode::UNAUTHORIZED
    );

    // Once a day.
    let soon = e.post("/users/@me/data-export", &a, json!({})).await;
    assert_eq!(soon.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(soon.headers.contains_key(header::RETRY_AFTER));

    // After seven days the link is dead even before the sweep deletes the file.
    e.clock.advance_ms(7 * 86_400_000 + 1);
    *e.exports.now_ms.lock().unwrap() = T0 + 7 * 86_400_000 + 1;
    let a2 = e.login(ALICE).await;
    assert_eq!(e.get(&path, &a2).await.status, StatusCode::NOT_FOUND);
    let st = e.get("/users/@me/data-export", &a2).await;
    assert_eq!(st.body["state"], "expired");
    assert!(st.body["download_url"].is_null());
    // And a new package can be requested.
    assert_eq!(
        e.post("/users/@me/data-export", &a2, json!({}))
            .await
            .status,
        StatusCode::ACCEPTED
    );
}
