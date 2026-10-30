//! The gate in front of the built web app (SEC-ENT-003, D-385; design: docs/dev/entrance-isolation.md section 3).
//!
//! With `[web] root` set, `sr-server` serves the app build (`app.html` and `app/*` of `apps/web/dist`) only to a
//! request with a valid session, and answers everything else so that a visitor learns nothing from the status
//! codes (no existence oracle):
//!
//! | Request | No valid session | Valid session |
//! |---|---|---|
//! | `GET /app`, `/app/`, a navigation to any path that is not a public surface or file | `302 /login?next=<path>` | `200` `app.html`, `private, no-store` |
//! | `GET /app/*` | `401`, empty body (present or absent file alike) | the file, `private`; `404` if absent |
//! | `GET /api/...` unknown path | `401` | `404` |
//! | other methods on the gated paths | `405` | `405` |
//!
//! The session is checked before any path is resolved. Bot and bearer credentials are not sessions. The gate has
//! its own per-IP bucket for `401`s. With `serve_public` the module also serves the public surfaces, the public
//! files and the error pages; `/house-notes` (the legal page) is rendered here either way.

use std::{
    collections::HashMap,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    handler::Handler,
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use sr_api::{AppState, rate_limit::client_ip};
use sr_auth::session::{SessionPolicy, SessionToken};
use sr_core::config::Config;

/// The public shells' policy (docs/dev/entrance-isolation.md section 3): no `wasm-unsafe-eval`, no `blob:`, no
/// `frame-src`, no voice or activity origins. `apps/web/src/security/csp.ts` is the other copy; `csp.test.ts`
/// pins them together.
pub const PUBLIC_CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self'; style-src-attr 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; manifest-src 'self'; worker-src 'self'; object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'; report-uri /internal/csp-report; report-to csp-endpoint";

/// The legal and error pages: plain HTML, no script, no fetch.
pub const NO_SCRIPT_CSP: &str = "default-src 'none'; style-src 'self'; img-src 'self'; font-src 'self'; manifest-src 'self'; object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'; report-uri /internal/csp-report; report-to csp-endpoint";

const REPORTING_ENDPOINTS: &str = "csp-endpoint=\"/internal/csp-report\"";
const CLEAR_SITE_DATA: &str = "\"cache\", \"storage\"";
/// Longest `next` the gate writes (the web app's `safeNext` uses the same figure).
pub const MAX_NEXT_LEN: usize = 2048;
/// `401`s one address may draw from the gate in a minute before it gets `429`.
const BUCKET_LIMIT: u32 = 240;
const BUCKET_WINDOW: Duration = Duration::from_secs(60);

/// The two operator values the legal page shows (`{{operator_country}}`, `{{operator_contact}}`).
#[async_trait]
pub trait OperatorInfo: Send + Sync {
    /// `(country, contact)` from the compliance settings; `None` when not set.
    async fn operator(&self, api: &AppState) -> (Option<String>, Option<String>);
}

/// No operator values (tests, installs that never set them).
pub struct NoOperator;

#[async_trait]
impl OperatorInfo for NoOperator {
    async fn operator(&self, _api: &AppState) -> (Option<String>, Option<String>) {
        (None, None)
    }
}

/// The operator values from the platform compliance settings (the same ones `GET /sr/compliance/public` returns).
pub struct ComplianceOperator;

#[async_trait]
impl OperatorInfo for ComplianceOperator {
    async fn operator(&self, api: &AppState) -> (Option<String>, Option<String>) {
        let Some(gf) = api.extension::<sr_api::routes::settings::features::GuildFeatures>() else {
            return (None, None);
        };
        let s = sr_api::routes::settings::features::compliance_settings(&gf).await;
        (s.operator_country, s.operator_contact)
    }
}

/// One row of `surfaces.json` (the copy of `apps/web/public-surfaces.json` the build puts in the web root).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct Surface {
    kind: String,
    shell: Option<String>,
    paths: Vec<String>,
    files: Vec<String>,
    reserved: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct SurfaceTable {
    surfaces: Vec<Surface>,
    #[serde(rename = "publicFiles")]
    public_files: Vec<String>,
}

/// A Caddy-style path pattern: exact, or a prefix when it ends in `*`.
fn pattern_matches(pattern: &str, path: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => path.starts_with(prefix),
        None => pattern == path,
    }
}

/// The failure to start the gate.
#[derive(Debug, thiserror::Error)]
pub enum WebError {
    #[error("web root {0} is not a directory")]
    NoRoot(String),
    #[error(
        "{0} is missing: the app build writes it (apps/web: pnpm build). The server refuses to serve the app without its policy"
    )]
    MissingCsp(String),
    #[error("{0} is not readable: {1}")]
    Unreadable(String, String),
    #[error("surfaces.json is needed to serve the public surfaces: {0}")]
    Surfaces(String),
}

/// The gate: the web root, the app policy, the surface table and the `401` bucket.
pub struct WebGate {
    root: PathBuf,
    serve_public: bool,
    app_csp: HeaderValue,
    table: SurfaceTable,
    operator: Arc<dyn OperatorInfo>,
    trust_forwarded_for: bool,
    bucket: Mutex<HashMap<IpAddr, (Instant, u32)>>,
}

#[derive(Clone)]
struct GateState {
    gate: Arc<WebGate>,
    api: AppState,
}

impl WebGate {
    /// `Ok(None)` when `[web] root` is unset (the API only). Refuses (`Err`) when the root is set and
    /// `app/csp.txt` is missing, or when `serve_public` is on and `surfaces.json` is missing.
    pub fn from_config(
        cfg: &Config,
        operator: Arc<dyn OperatorInfo>,
    ) -> Result<Option<Arc<Self>>, WebError> {
        let Some(root) = cfg.web.root_path() else {
            return Ok(None);
        };
        Self::open(
            Path::new(root),
            cfg.web.serve_public,
            cfg.server.trust_forwarded_for,
            operator,
        )
        .map(Some)
    }

    /// Open a web root.
    pub fn open(
        root: &Path,
        serve_public: bool,
        trust_forwarded_for: bool,
        operator: Arc<dyn OperatorInfo>,
    ) -> Result<Arc<Self>, WebError> {
        if !root.is_dir() {
            return Err(WebError::NoRoot(root.display().to_string()));
        }
        let csp_path = root.join("app").join("csp.txt");
        let csp = match std::fs::read_to_string(&csp_path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(WebError::MissingCsp(csp_path.display().to_string()));
            }
            Err(e) => {
                return Err(WebError::Unreadable(
                    csp_path.display().to_string(),
                    e.to_string(),
                ));
            }
        };
        let csp = csp.trim();
        let app_csp = HeaderValue::from_str(csp)
            .ok()
            .filter(|_| !csp.is_empty())
            .ok_or_else(|| {
                WebError::Unreadable(
                    csp_path.display().to_string(),
                    "not a single header line".into(),
                )
            })?;
        let table = match std::fs::read_to_string(root.join("surfaces.json")) {
            Ok(s) => serde_json::from_str::<SurfaceTable>(&s)
                .map_err(|e| WebError::Surfaces(e.to_string()))?,
            Err(e) if serve_public => return Err(WebError::Surfaces(e.to_string())),
            Err(_) => {
                tracing::warn!(
                    "web root has no surfaces.json; public paths that reach the server are answered 404"
                );
                SurfaceTable::default()
            }
        };
        Ok(Arc::new(Self {
            root: root.to_path_buf(),
            serve_public,
            app_csp,
            table,
            operator,
            trust_forwarded_for,
            bucket: Mutex::new(HashMap::new()),
        }))
    }

    /// Mount the gate on the finished router: its own routes, and the fallback that answers every other path.
    pub fn install(self: &Arc<Self>, app: Router, state: AppState) -> Router {
        let gs = GateState {
            gate: self.clone(),
            api: state,
        };
        let routes = Router::new()
            .route("/app", get(app_entry))
            .route("/app/", get(app_entry))
            .route("/app/{*path}", get(app_file))
            .route("/house-notes", get(legal))
            // `/share` is the Web Share Target page for a GET (the POST belongs to the API router); without
            // this route a GET would be a `405`, which tells a visitor the path exists.
            .route("/share", get(fallback_get))
            .with_state(gs.clone());
        app.merge(routes).fallback_service(fallback.with_state(gs))
    }

    // ---- the session -----------------------------------------------------------------------------------

    /// A valid `sr_session` cookie with the API's expiry policy. Bot and bearer credentials do not count.
    async fn has_session(&self, api: &AppState, headers: &HeaderMap) -> bool {
        let Some(raw) = sr_api::auth::parse_cookie(headers, sr_api::auth::SESSION_COOKIE) else {
            return false;
        };
        let Some(token) = SessionToken::parse(raw) else {
            return false;
        };
        match api.sessions().find(&token.hash()).await {
            Ok(Some(rec)) => SessionPolicy::is_valid(rec.expires_at, api.now_secs()),
            Ok(None) => false,
            Err(e) => {
                tracing::error!(error = %e, "web gate: session lookup failed");
                false
            }
        }
    }

    /// Count one `401` for `ip`; `false` once the address is over its minute.
    fn allow_401(&self, ip: Option<IpAddr>) -> bool {
        let key = ip.unwrap_or(IpAddr::from([0, 0, 0, 0]));
        let now = Instant::now();
        let mut b = self.bucket.lock().unwrap_or_else(|p| p.into_inner());
        if b.len() > 20_000 {
            b.retain(|_, (t, _)| now.duration_since(*t) < BUCKET_WINDOW);
        }
        let e = b.entry(key).or_insert((now, 0));
        if now.duration_since(e.0) >= BUCKET_WINDOW {
            *e = (now, 0);
        }
        e.1 += 1;
        e.1 <= BUCKET_LIMIT
    }

    fn unauthorized(&self, req_ip: Option<IpAddr>) -> Response {
        if !self.allow_401(req_ip) {
            let mut r = plain(StatusCode::TOO_MANY_REQUESTS);
            r.headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
            return r;
        }
        plain(StatusCode::UNAUTHORIZED)
    }

    // ---- surfaces --------------------------------------------------------------------------------------

    /// The shell of the public surface that owns `path`, if any (reserved surfaces have no shell yet).
    fn surface_shell(&self, path: &str) -> Option<&str> {
        self.table
            .surfaces
            .iter()
            .filter(|s| s.kind == "bundle" && !s.reserved)
            .find(|s| s.paths.iter().any(|p| pattern_matches(p, path)))
            .and_then(|s| s.shell.as_deref())
    }

    /// Whether `path` belongs to a public surface (a bundle, reserved or not, or a plain-HTML page).
    fn is_public_path(&self, path: &str) -> bool {
        self.table
            .surfaces
            .iter()
            .any(|s| s.paths.iter().any(|p| pattern_matches(p, path)))
            || self.is_public_file(path)
    }

    fn is_public_file(&self, path: &str) -> bool {
        self.table
            .public_files
            .iter()
            .any(|p| pattern_matches(p, path))
            || self
                .table
                .surfaces
                .iter()
                .flat_map(|s| s.files.iter())
                .any(|f| path.strip_prefix('/') == Some(f.as_str()))
    }

    fn redirect_to_login(&self, uri: &Uri) -> Response {
        let target = uri
            .path_and_query()
            .map(|pq| pq.as_str())
            .and_then(safe_next);
        let location = match target {
            Some(n) => format!("/login?next={}", encode_next(n)),
            None => "/login".to_owned(),
        };
        let mut r = plain(StatusCode::FOUND);
        if let Ok(v) = HeaderValue::from_str(&location) {
            r.headers_mut().insert(header::LOCATION, v);
        }
        r
    }

    async fn app_shell(&self, method: &Method) -> Response {
        let mut r = file_response(
            &self.root.join("app.html"),
            method,
            "text/html; charset=utf-8",
            "private, no-store",
        )
        .await;
        let h = r.headers_mut();
        h.insert(header::CONTENT_SECURITY_POLICY, self.app_csp.clone());
        h.insert(
            HeaderName::from_static("reporting-endpoints"),
            HeaderValue::from_static(REPORTING_ENDPOINTS),
        );
        h.insert(
            HeaderName::from_static("x-robots-tag"),
            HeaderValue::from_static("noindex, nofollow"),
        );
        h.insert(header::VARY, HeaderValue::from_static("Cookie"));
        r
    }
}

// ---- handlers -----------------------------------------------------------------------------------------

async fn app_entry(State(gs): State<GateState>, req: Request) -> Response {
    let ip = client_ip(&req, gs.gate.trust_forwarded_for);
    let _ = ip;
    if gs.gate.has_session(&gs.api, req.headers()).await {
        gs.gate.app_shell(req.method()).await
    } else {
        gs.gate.redirect_to_login(req.uri())
    }
}

async fn app_file(State(gs): State<GateState>, req: Request) -> Response {
    let ip = client_ip(&req, gs.gate.trust_forwarded_for);
    // The session comes before the path: an absent file and a present one are the same `401`.
    if !gs.gate.has_session(&gs.api, req.headers()).await {
        return gs.gate.unauthorized(ip);
    }
    let rel = req
        .uri()
        .path()
        .strip_prefix("/app/")
        .unwrap_or_default()
        .to_owned();
    let Some(rel) = clean_relative(&rel) else {
        return plain(StatusCode::NOT_FOUND);
    };
    let cache = if is_hashed(&rel) {
        "private, max-age=31536000, immutable"
    } else {
        "private, no-cache"
    };
    let path = gs.gate.root.join("app").join(&rel);
    let mut r = file_response(&path, req.method(), content_type(&rel), cache).await;
    r.headers_mut()
        .insert(header::VARY, HeaderValue::from_static("Cookie"));
    r.headers_mut().insert(
        HeaderName::from_static("x-robots-tag"),
        HeaderValue::from_static("noindex, nofollow"),
    );
    r
}

async fn fallback_get(State(gs): State<GateState>, req: Request) -> Response {
    fallback_inner(gs, req).await
}

async fn fallback(State(gs): State<GateState>, req: Request) -> Response {
    fallback_inner(gs, req).await
}

async fn fallback_inner(gs: GateState, req: Request) -> Response {
    let path = req.uri().path().to_owned();
    // The API keeps its own rule: `401` for a caller with no principal, `404` for one with.
    if path.starts_with("/api/") {
        return sr_api::router::not_found(State(gs.api.clone()), req).await;
    }
    let method = req.method().clone();
    if method != Method::GET && method != Method::HEAD {
        let mut r = plain(StatusCode::METHOD_NOT_ALLOWED);
        r.headers_mut()
            .insert(header::ALLOW, HeaderValue::from_static("GET, HEAD"));
        return r;
    }
    let gate = &gs.gate;
    if gate.is_public_path(&path) {
        return if gate.serve_public {
            serve_public_path(gate, &path, &method).await
        } else {
            // Caddy serves these; one that reaches the server is not ours to answer.
            plain(StatusCode::NOT_FOUND)
        };
    }
    let has_session = gate.has_session(&gs.api, req.headers()).await;
    if is_navigation(req.headers()) {
        return if has_session {
            gate.app_shell(&method).await
        } else {
            gate.redirect_to_login(req.uri())
        };
    }
    if has_session {
        // A signed-in caller may see an honest `404`.
        return sr_api::router::not_found(State(gs.api.clone()), req).await;
    }
    gate.unauthorized(client_ip(&req, gate.trust_forwarded_for))
}

/// A public shell or file, only with `serve_public`.
async fn serve_public_path(gate: &WebGate, path: &str, method: &Method) -> Response {
    let (file, ct, cache, is_doc) = if let Some(shell) = gate.surface_shell(path) {
        (
            gate.root.join(shell),
            "text/html; charset=utf-8".to_owned(),
            "no-cache",
            true,
        )
    } else if gate.is_public_file(path) {
        let Some(rel) = clean_relative(path.trim_start_matches('/')) else {
            return plain(StatusCode::NOT_FOUND);
        };
        let cache = if path.starts_with("/entrance/") || path.starts_with("/fonts/") {
            "public, max-age=31536000, immutable"
        } else if matches!(
            path,
            "/sw.js"
                | "/manifest.webmanifest"
                | "/offline.html"
                | "/offline.css"
                | "/404.html"
                | "/50x.html"
        ) {
            "no-cache"
        } else {
            ""
        };
        let doc = rel.ends_with(".html");
        (
            gate.root.join(&rel),
            content_type(&rel).to_owned(),
            cache,
            doc,
        )
    } else {
        return plain(StatusCode::NOT_FOUND);
    };
    let mut r = file_response(&file, method, &ct, cache).await;
    let h = r.headers_mut();
    if is_doc {
        let csp = if path.ends_with("404.html")
            || path.ends_with("50x.html")
            || path.ends_with("offline.html")
        {
            NO_SCRIPT_CSP
        } else {
            PUBLIC_CSP
        };
        h.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(csp),
        );
        h.insert(
            HeaderName::from_static("reporting-endpoints"),
            HeaderValue::from_static(REPORTING_ENDPOINTS),
        );
    }
    if path != "/" {
        h.insert(
            HeaderName::from_static("x-robots-tag"),
            HeaderValue::from_static("noindex, nofollow"),
        );
    }
    r
}

/// `GET /house-notes`: the legal page, plain HTML rendered from the per-locale template the build writes
/// (`legal/house-notes.<locale>.html`) with the two operator placeholders filled in.
async fn legal(State(gs): State<GateState>, req: Request) -> Response {
    let gate = &gs.gate;
    let lang = query_param(req.uri().query(), "lang");
    let accept = req
        .headers()
        .get(header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let dir = gate.root.join("legal");
    let mut template = None;
    for loc in locale_candidates(lang.as_deref(), accept) {
        if let Ok(s) = tokio::fs::read_to_string(dir.join(format!("house-notes.{loc}.html"))).await
        {
            template = Some(s);
            break;
        }
    }
    let Some(template) = template else {
        return plain(StatusCode::NOT_FOUND);
    };
    let (country, contact) = gate.operator.operator(&gs.api).await;
    let body = fill_placeholders(
        &template,
        country.as_deref().unwrap_or(""),
        contact.as_deref().unwrap_or(""),
    );
    let mut r = Response::new(if req.method() == Method::HEAD {
        Body::empty()
    } else {
        Body::from(body)
    });
    let h = r.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(NO_SCRIPT_CSP),
    );
    h.insert(
        HeaderName::from_static("reporting-endpoints"),
        HeaderValue::from_static(REPORTING_ENDPOINTS),
    );
    h.insert(header::VARY, HeaderValue::from_static("Accept-Language"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        HeaderName::from_static("x-robots-tag"),
        HeaderValue::from_static("noindex, nofollow"),
    );
    r
}

// ---- helpers ------------------------------------------------------------------------------------------

/// An empty response with the status and the headers every gate answer carries.
fn plain(status: StatusCode) -> Response {
    let mut r = status.into_response();
    let h = r.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    r
}

/// Whether the request is a browser navigation: `Sec-Fetch-Mode: navigate`, or an `Accept` that prefers HTML.
///
/// A page the service worker passes on (`fetch(event.request)`) is a navigation too, but the Fetch standard turns
/// its mode into `same-origin`, which is what Firefox sends for it (Chromium and WebKit keep `navigate`). So
/// `same-origin` with an HTML `Accept` counts, as does no `Sec-Fetch-Mode` at all; `cors` and `no-cors` never do.
/// Without this, Firefox with the worker installed got the empty `401`/`404` of a non-navigation for `/board` and
/// for signed-in app pages (`NS_ERROR_NET_EMPTY_RESPONSE` in the built-bundle e2e, wave 30b).
pub fn is_navigation(headers: &HeaderMap) -> bool {
    if let Some(mode) = headers.get("sec-fetch-mode").and_then(|v| v.to_str().ok()) {
        if mode.eq_ignore_ascii_case("navigate") {
            return true;
        }
        if !mode.eq_ignore_ascii_case("same-origin") {
            return false;
        }
    }
    let accept = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let first = accept.split(',').next().unwrap_or("").trim();
    first.starts_with("text/html") || first.starts_with("application/xhtml+xml")
}

/// The server's `next` rule (docs/dev/entrance-isolation.md section 3; `apps/web/src/lib/enterApp.ts` has the same
/// rule in TypeScript and the same test cases): one leading `/`, no `//`, no backslash, no control character, no
/// scheme, at most [`MAX_NEXT_LEN`] bytes. The rule holds for the value as written and once percent-decoded, so an
/// encoded `//`, backslash or control character (`/%2F%2Fevil.example`, `/%5Cevil`, `/%0D%0A`) is refused too:
/// whatever later decodes the path (a browser, a proxy, the app's router) never sees a host in it. Anything else is
/// `None` and the redirect carries no `next`.
pub fn safe_next(next: &str) -> Option<&str> {
    if !next_shape_ok(next) {
        return None;
    }
    if next.contains('%') && !next_shape_ok(&percent_decode_once(next)) {
        return None;
    }
    Some(next)
}

/// One pass of [`safe_next`]'s rule over a value, raw or decoded.
fn next_shape_ok(next: &str) -> bool {
    let bytes = next.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_NEXT_LEN || bytes[0] != b'/' {
        return false;
    }
    if bytes.get(1) == Some(&b'/') || next.contains('\\') {
        return false;
    }
    if next.chars().any(|c| c.is_control()) {
        return false;
    }
    // A scheme can only start after the first `/`, but a `/` followed by `:` host tricks (`/\`, `/@`) are out.
    bytes.get(1) != Some(&b'@')
}

/// Decode `%XX` escapes once; a `%` without two hex digits stays as it is. Invalid UTF-8 becomes U+FFFD.
fn percent_decode_once(s: &str) -> String {
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let escape = match (bytes[i], bytes.get(i + 1), bytes.get(i + 2)) {
            (b'%', Some(&h), Some(&l)) => hex(h).zip(hex(l)),
            _ => None,
        };
        match escape {
            Some((hi, lo)) => {
                out.push((hi << 4) | lo);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Percent-encode `next` for use as the value of a query parameter; `/` stays readable.
fn encode_next(next: &str) -> String {
    let mut out = String::with_capacity(next.len() + 8);
    for b in next.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// A path under the web root with no way out of it: no empty, `.`, `..` or dotted segment, no backslash,
/// colon or NUL.
fn clean_relative(rel: &str) -> Option<String> {
    if rel.is_empty() || rel.len() > 1024 {
        return None;
    }
    if rel.contains(['\\', ':', '\0']) {
        return None;
    }
    for seg in rel.split('/') {
        if seg.is_empty() || seg.starts_with('.') {
            return None;
        }
    }
    Some(rel.to_owned())
}

/// A name carrying a build hash (`index-1a2b3c4d.js`): safe to cache forever.
fn is_hashed(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    let stem = name.split('.').next().unwrap_or(name);
    stem.rsplit_once('-').is_some_and(|(_, h)| {
        h.len() >= 8
            && h.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    })
}

fn content_type(rel: &str) -> &'static str {
    let ext = rel.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "webmanifest" => "application/manifest+json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "wasm" => "application/wasm",
        "txt" | "md" => "text/plain; charset=utf-8",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "xml" => "application/xml",
        _ => "application/octet-stream",
    }
}

/// Stream a file, or answer `404` (plain) when it is not a regular file.
async fn file_response(path: &Path, method: &Method, ct: &str, cache: &str) -> Response {
    use tokio::io::AsyncReadExt;
    let Ok(file) = tokio::fs::File::open(path).await else {
        return plain(StatusCode::NOT_FOUND);
    };
    let Ok(meta) = file.metadata().await else {
        return plain(StatusCode::NOT_FOUND);
    };
    if !meta.is_file() {
        return plain(StatusCode::NOT_FOUND);
    }
    let body = if method == Method::HEAD {
        Body::empty()
    } else {
        let stream = futures::stream::unfold(Some(file), |state| async move {
            let mut file = state?;
            let mut buf = vec![0u8; 64 * 1024];
            match file.read(&mut buf).await {
                Ok(0) => None,
                Ok(n) => {
                    buf.truncate(n);
                    Some((Ok::<_, std::io::Error>(bytes::Bytes::from(buf)), Some(file)))
                }
                Err(e) => Some((Err(e), None)),
            }
        });
        Body::from_stream(stream)
    };
    let mut r = Response::new(body);
    let h = r.headers_mut();
    if let Ok(v) = HeaderValue::from_str(ct) {
        h.insert(header::CONTENT_TYPE, v);
    }
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(meta.len()));
    if !cache.is_empty()
        && let Ok(v) = HeaderValue::from_str(cache)
    {
        h.insert(header::CACHE_CONTROL, v);
    }
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    r
}

fn query_param(query: Option<&str>, key: &str) -> Option<String> {
    query?
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.to_owned())
}

/// The locale tags to try, best first: `?lang=`, then `Accept-Language` by weight; each tag as given and as its
/// primary subtag; always ending in `en`. Only `[A-Za-z0-9-]` tags survive, so a tag is never a path.
fn locale_candidates(lang: Option<&str>, accept: &str) -> Vec<String> {
    let valid = |t: &str| {
        !t.is_empty() && t.len() <= 16 && t.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    };
    let mut ranked: Vec<(f32, String)> = Vec::new();
    for part in accept.split(',') {
        let mut it = part.split(';');
        let tag = it.next().unwrap_or("").trim();
        let q = it
            .find_map(|p| {
                p.trim()
                    .strip_prefix("q=")
                    .and_then(|q| q.parse::<f32>().ok())
            })
            .unwrap_or(1.0);
        if valid(tag) && q > 0.0 {
            ranked.push((q, tag.to_owned()));
        }
    }
    ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut tags: Vec<String> = Vec::new();
    if let Some(l) = lang.filter(|l| valid(l)) {
        tags.push(l.to_owned());
    }
    tags.extend(ranked.into_iter().map(|(_, t)| t));
    let mut out: Vec<String> = Vec::new();
    for t in tags {
        let primary = t.split('-').next().unwrap_or(&t).to_owned();
        for c in [t, primary] {
            if c != "*" && !out.contains(&c) {
                out.push(c);
            }
        }
    }
    if !out.iter().any(|c| c == "en") {
        out.push("en".into());
    }
    out
}

/// Escape for HTML text and attribute values.
pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Replace `{{operator_country}}` and `{{operator_contact}}` in one pass, HTML-escaped, so a value that
/// contains a placeholder is never expanded again.
pub fn fill_placeholders(template: &str, country: &str, contact: &str) -> String {
    let country = escape_html(country);
    let contact = escape_html(contact);
    let mut out = String::with_capacity(template.len() + country.len() + contact.len());
    let mut rest = template;
    while let Some(i) = rest.find("{{") {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if let Some(t) = tail.strip_prefix("{{operator_country}}") {
            out.push_str(&country);
            rest = t;
        } else if let Some(t) = tail.strip_prefix("{{operator_contact}}") {
            out.push_str(&contact);
            rest = t;
        } else {
            out.push_str("{{");
            rest = &tail[2..];
        }
    }
    out.push_str(rest);
    out
}

/// The value of the response header every logout carries; exported so the tests and the docs name one string.
pub const LOGOUT_CLEAR_SITE_DATA: &str = CLEAR_SITE_DATA;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_rule() {
        for ok in [
            "/",
            "/channels",
            "/a/b?x=1&y=2",
            "/app",
            "/invite/abc?x=%2F",
            "/a%20b",
            "/100%",
            "/a%zz",
        ] {
            assert_eq!(safe_next(ok), Some(ok), "{ok}");
        }
        for bad in [
            "",
            "channels",
            "//evil",
            "//evil.example/x",
            "https://evil",
            "/\\evil",
            "/a\\b",
            "/a\nb",
            "/a\u{7f}b",
            "/@evil",
            "/%2F%2Fevil.example",
            "/%2f%2fevil",
            "/%5Cevil",
            "/a%5cb",
            "/%0D%0Aset-cookie:x",
            "/%40evil",
        ] {
            assert_eq!(safe_next(bad), None, "{bad:?}");
        }
        let long = format!("/{}", "a".repeat(MAX_NEXT_LEN));
        assert_eq!(safe_next(&long), None);
        let edge = format!("/{}", "a".repeat(MAX_NEXT_LEN - 1));
        assert!(safe_next(&edge).is_some());
    }

    #[test]
    fn next_is_encoded_for_a_query() {
        assert_eq!(encode_next("/a/b?x=1&y=2#z"), "/a/b%3Fx%3D1%26y%3D2%23z");
        assert_eq!(encode_next("/a b"), "/a%20b");
    }

    #[test]
    fn patterns() {
        assert!(pattern_matches("/", "/"));
        assert!(!pattern_matches("/", "/x"));
        assert!(pattern_matches("/join/*", "/join/abc"));
        assert!(!pattern_matches("/join/*", "/join"));
        assert!(pattern_matches("/~*", "/~handle"));
        assert!(pattern_matches("/login/*", "/login/2fa"));
        assert!(!pattern_matches("/login", "/login/2fa"));
    }

    #[test]
    fn paths_cannot_leave_the_root() {
        for bad in [
            "",
            "../x",
            "a/../x",
            "a//b",
            ".hidden",
            "a/.vite/m.json",
            "a\\b",
            "c:/x",
            "a\0b",
        ] {
            assert_eq!(clean_relative(bad), None, "{bad:?}");
        }
        assert_eq!(
            clean_relative("assets/a-1234abcd.js").unwrap(),
            "assets/a-1234abcd.js"
        );
    }

    #[test]
    fn hashed_names() {
        assert!(is_hashed("assets/index-1a2b3c4d.js"));
        assert!(is_hashed("x/Chunk-Ab_12xyz9.css"));
        assert!(!is_hashed("manifest.webmanifest"));
        assert!(!is_hashed("THIRD_PARTY_NOTICES.md"));
        assert!(!is_hashed("a-b.js"));
    }

    #[test]
    fn navigation_detection() {
        let mut h = HeaderMap::new();
        assert!(!is_navigation(&h));
        h.insert(
            header::ACCEPT,
            HeaderValue::from_static("text/html,application/xhtml+xml"),
        );
        assert!(is_navigation(&h));
        h.insert(header::ACCEPT, HeaderValue::from_static("*/*"));
        assert!(!is_navigation(&h));
        h.insert("sec-fetch-mode", HeaderValue::from_static("navigate"));
        assert!(is_navigation(&h));
        h.insert("sec-fetch-mode", HeaderValue::from_static("cors"));
        h.insert(header::ACCEPT, HeaderValue::from_static("text/html"));
        assert!(!is_navigation(&h));
        h.insert("sec-fetch-mode", HeaderValue::from_static("no-cors"));
        assert!(!is_navigation(&h));
        // A page the service worker passed on (Firefox): same-origin, HTML first.
        h.insert("sec-fetch-mode", HeaderValue::from_static("same-origin"));
        assert!(is_navigation(&h));
        h.insert(header::ACCEPT, HeaderValue::from_static("*/*"));
        assert!(!is_navigation(&h));
    }

    #[test]
    fn locales() {
        assert_eq!(locale_candidates(None, ""), ["en"]);
        assert_eq!(
            locale_candidates(Some("de"), "fr-CA,fr;q=0.8,en;q=0.5"),
            ["de", "fr-CA", "fr", "en"]
        );
        assert_eq!(
            locale_candidates(None, "pt-BR;q=0.4, es;q=0.9"),
            ["es", "pt-BR", "pt", "en"]
        );
        // A tag that could be a path is dropped.
        assert_eq!(locale_candidates(Some("../x"), "a/b"), ["en"]);
    }

    #[test]
    fn placeholders_are_escaped_and_single_pass() {
        let t = "<p>{{operator_country}} / {{operator_contact}} / {{other}}</p>";
        assert_eq!(
            fill_placeholders(t, "NL", "a@b.example"),
            "<p>NL / a@b.example / {{other}}</p>"
        );
        assert_eq!(
            fill_placeholders(t, "<script>", "\"x\" & 'y' {{operator_country}}"),
            "<p>&lt;script&gt; / &quot;x&quot; &amp; &#39;y&#39; {{operator_country}} / {{other}}</p>"
        );
    }
}
