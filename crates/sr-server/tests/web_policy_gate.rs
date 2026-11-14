//! SEC-AUD-210 (D-682): the server completes the app policy. A temporary web root holds the plain policy a release
//! asset carries; the gate is opened with a configuration and the header on `/app` is read back.
//!
//! Needs no database: the sessions are in memory. (`web_gate.rs` shows the same fixture in full.)

use std::{path::Path, sync::Arc};

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use sr_api::{AppState, auth::memory::MemorySessions, build_router};
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_core::{Clock, ManualClock};
use sr_server::{
    web::{NoOperator, PUBLIC_CSP, WebError, WebGate},
    web_policy::{PolicyInputs, host_warnings, merge_origins, origins_for, sources_by_directive},
};
use tower::ServiceExt;

const NOW_MS: i64 = 1_800_000_000_000;
const PLAIN: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; img-src 'self' data: blob:; media-src 'self' blob: https:; connect-src 'self'; frame-src 'self' https://www.youtube-nocookie.com https://player.vimeo.com; frame-ancestors 'none'";
const SURFACES: &str = r#"{ "version": 1, "appShell": "app.html", "appPrefix": "/app",
  "surfaces": [ { "surface": "landing", "kind": "bundle", "shell": "index.html", "paths": ["/", "/join/*"] },
                { "surface": "auth", "kind": "bundle", "shell": "auth.html", "paths": ["/login"] } ],
  "publicFiles": ["/entrance/*"] }"#;

fn write(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

fn web_root() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    write(r, "app.html", "<html>APP-SHELL</html>");
    write(r, "app/csp.txt", &format!("{PLAIN}\n"));
    write(r, "index.html", "<html>LANDING</html>");
    write(r, "auth.html", "<html>AUTH</html>");
    write(r, "surfaces.json", SURFACES);
    d
}

fn inputs() -> PolicyInputs {
    PolicyInputs {
        public_url: "https://chat.example.org".into(),
        activities_enabled: true,
        ..PolicyInputs::default()
    }
}

struct Fx {
    app: Router,
    cookie: String,
    _root: tempfile::TempDir,
}

/// A gate over a fresh web root, opened through `from_config_with` as the server does.
async fn fixture(inp: &PolicyInputs) -> Result<Fx, WebError> {
    let root = web_root();
    let mut cfg = sr_core::config::Config::default();
    cfg.web.root = Some(root.path().to_string_lossy().into_owned());
    cfg.web.serve_public = true;
    let gate = WebGate::from_config_with(&cfg, inp, Arc::new(NoOperator))?.unwrap();
    let sessions = Arc::new(MemorySessions::new());
    let clock = Arc::new(ManualClock::new(NOW_MS));
    let state = AppState::builder()
        .sessions(sessions.clone())
        .clock(clock.clone())
        .build();
    let t = SessionToken::generate().unwrap();
    sessions
        .create(SessionRecord {
            id: 100,
            user_id: 5,
            token_hash: t.hash(),
            created_at: 1_799_999_000,
            last_seen_at: 1_799_999_000,
            expires_at: clock.now_unix_ms() / 1000 + 3600,
            device_name: None,
        })
        .await
        .unwrap();
    let app = gate.install(build_router(state.clone(), Router::new()), state);
    Ok(Fx {
        app,
        cookie: format!("sr_session={}", t.expose()),
        _root: root,
    })
}

impl Fx {
    async fn csp(&self, path: &str, signed_in: bool) -> String {
        let mut b = Request::builder()
            .method(Method::GET)
            .uri(path)
            .header("sec-fetch-mode", "navigate")
            .header(header::ACCEPT, "text/html");
        if signed_in {
            b = b.header(header::COOKIE, &self.cookie);
        }
        let r = self
            .app
            .clone()
            .oneshot(b.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_ne!(r.status(), StatusCode::INTERNAL_SERVER_ERROR);
        r.headers()
            .get("content-security-policy")
            .map(|v| v.to_str().unwrap().to_owned())
            .unwrap_or_default()
    }
}

#[tokio::test]
async fn a_release_build_with_activities_on_gets_the_frame_origin() {
    let f = fixture(&inputs()).await.unwrap();
    let got = f.csp("/app", true).await;
    let frame = sources_by_directive(&got)["frame-src"].clone();
    assert!(
        frame.contains(&"https://*.activities.chat.example.org".to_owned()),
        "{got}"
    );
    // Everything the file had is still there, in the same order.
    assert!(got.starts_with("default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src"));
    assert_eq!(
        sources_by_directive(&got)["script-src"],
        sources_by_directive(PLAIN)["script-src"]
    );
    // Without a configuration the file is sent as written.
    let plain = fixture(&PolicyInputs::default()).await.unwrap();
    assert_eq!(plain.csp("/app", true).await, PLAIN);
}

#[tokio::test]
async fn the_emulator_policy_follows() {
    let f = fixture(&PolicyInputs {
        csp_connect: "https://api.example.net".into(),
        ..inputs()
    })
    .await
    .unwrap();
    let app = f.csp("/app", true).await;
    let emu = f.csp("/games", true).await;
    assert!(
        emu.contains("https://*.activities.chat.example.org"),
        "{emu}"
    );
    assert!(
        emu.contains("connect-src 'self' https://api.example.net"),
        "{emu}"
    );
    assert!(
        emu.contains("script-src 'self' 'wasm-unsafe-eval' blob:"),
        "{emu}"
    );
    // The same policy, `blob:` in `script-src` apart.
    assert_eq!(
        emu.replace("'wasm-unsafe-eval' blob:", "'wasm-unsafe-eval'"),
        app
    );
}

#[tokio::test]
async fn a_bad_extra_origin_stops_the_start_with_its_name() {
    for (var, bad) in [
        ("SR_CSP_CONNECT", "https://a.example;"),
        ("SR_CSP_FRAME", "https://a.example https://b.example"),
        ("SR_CSP_MEDIA", "'unsafe-inline'"),
        ("SR_CSP_CONNECT", "'self'"),
        ("SR_CSP_FRAME", "*"),
        ("SR_CSP_MEDIA", "https://a.example/path"),
        ("SR_CSP_CONNECT", "https://a.example\""),
    ] {
        let mut inp = inputs();
        match var {
            "SR_CSP_CONNECT" => inp.csp_connect = format!("https://ok.example,{bad}"),
            "SR_CSP_FRAME" => inp.csp_frame = format!("https://ok.example,{bad}"),
            _ => inp.csp_media = format!("https://ok.example,{bad}"),
        }
        let err = match fixture(&inp).await {
            Ok(_) => panic!("{var}={bad} must stop the start"),
            Err(e) => e,
        };
        assert!(matches!(err, WebError::BadCspOrigin { .. }), "{err}");
        let msg = err.to_string();
        assert!(msg.contains(var) && msg.contains(bad), "{msg}");
    }
}

#[tokio::test]
async fn a_bad_embed_host_is_warned_about_and_the_server_starts() {
    // D-697: the opposite of a bad `SR_CSP_*` entry: the start goes on, the bad entries are dropped and named.
    let inp = PolicyInputs {
        peertube_hosts: "tube.example.org, http://bad.example, nodot".into(),
        media_hosts: "cdn.example.org".into(),
        ..inputs()
    };
    let fx = fixture(&inp)
        .await
        .unwrap_or_else(|e| panic!("a bad embed host must not stop the start: {e}"));
    let warnings = host_warnings(&inp);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("SR_PEERTUBE_HOSTS")
            && warnings[0].contains("\"http://bad.example\"")
            && warnings[0].contains("\"nodot\"")
            && !warnings[0].contains("tube.example.org"),
        "{warnings:?}"
    );
    // The policy holds the good host only.
    let origins = origins_for(&inp).unwrap();
    assert!(
        origins
            .frame
            .contains(&"https://tube.example.org".to_owned())
    );
    assert!(
        !origins
            .connect
            .iter()
            .any(|o| o.contains("bad.example") || o.contains("nodot"))
    );
    drop(fx);
}

#[tokio::test]
async fn the_public_policy_is_unchanged() {
    let everything = PolicyInputs {
        peertube_hosts: "tube.example.org".into(),
        media_hosts: "cdn.example.org".into(),
        voice_endpoints: vec!["wss://voice.example.net/voice/ws".into()],
        csp_connect: "https://api.example.net".into(),
        csp_frame: "https://frames.example.net".into(),
        csp_media: "https://img.example.net".into(),
        ..inputs()
    };
    let with = fixture(&everything).await.unwrap();
    let without = fixture(&PolicyInputs::default()).await.unwrap();
    for path in ["/", "/login", "/join/abc"] {
        let a = with.csp(path, false).await;
        let b = without.csp(path, false).await;
        assert_eq!(a, PUBLIC_CSP, "{path}");
        assert_eq!(a, b, "{path}");
    }
    // The app policy did change.
    assert_ne!(
        with.csp("/app", true).await,
        without.csp("/app", true).await
    );
}

#[test]
fn nothing_is_added_twice_when_the_build_already_holds_the_origins() {
    let o = origins_for(&inputs()).unwrap();
    let once = merge_origins(PLAIN, &o).policy;
    let twice = merge_origins(&once, &o);
    assert_eq!(twice.policy, once);
    assert!(twice.added.is_empty());
}
