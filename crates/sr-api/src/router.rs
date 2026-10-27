//! Router composition and tower layers (FND-API-001).

use crate::{
    auth::{SESSION_COOKIE, csrf, parse_cookie},
    health, observability, openapi,
    rate_limit::rate_limit,
    security_headers::{SecurityHeaderConfig, security_headers},
    state::AppState,
};
use axum::{
    Router,
    extract::State,
    http::{HeaderName, HeaderValue, Method, Request, StatusCode, header},
    middleware::{Next, from_fn, from_fn_with_state},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use sr_core::ErrorCode;
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    cors::{AllowOrigin, CorsLayer},
    propagate_header::PropagateHeaderLayer,
    request_id::{MakeRequestUuid, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::{DefaultOnResponse, TraceLayer},
};
use tracing::Level;

const REQUEST_ID: &str = "x-request-id";

/// The public router: `/healthz`, `/readyz`, and the REST API under both
/// `/api/v1` and the `/api/v10` alias. `extra_v1` carries feature routes
/// (relative to the version prefix). Auth is per-handler via the
/// [`crate::Principal`] extractor; rate limiting applies to every API route.
/// Panics if two routers register the same method and path.
pub fn build_router(state: AppState, extra_v1: Router<AppState>) -> Router {
    let v1 = Router::new()
        .route("/openapi.json", get(openapi::openapi_json))
        .route("/users/@me", get(openapi::current_user))
        .route("/sr/csrf", get(openapi::csrf_token))
        .merge(extra_v1)
        .route_layer(from_fn_with_state(
            state.clone(),
            crate::routes::storage::flags::byos_gate,
        ))
        .route_layer(from_fn_with_state(state.clone(), rate_limit))
        .route_layer(from_fn_with_state(
            state.clone(),
            crate::sensitive::reauth_gate,
        ))
        .route_layer(from_fn_with_state(state.clone(), origin_check));

    let app = Router::new()
        .route("/healthz", get(health::healthz))
        .route("/readyz", get(health::readyz))
        .route("/developers/docs/api", get(api_docs))
        .route("/share", axum::routing::post(share_fallback))
        .route(
            "/internal/csp-report",
            axum::routing::post(crate::routes::internal::csp_report),
        )
        .nest("/api/v1", v1.clone())
        .nest("/api/v10", v1)
        .fallback(not_found);
    with_common_layers(app, &state).with_state(state)
}

/// Request-id layers for routes outside [`build_router`] (the gateway socket).
pub fn with_request_id<S: Clone + Send + Sync + 'static>(app: Router<S>) -> Router<S> {
    let id = HeaderName::from_static(REQUEST_ID);
    app.layer(
        ServiceBuilder::new()
            .layer(SetRequestIdLayer::new(id.clone(), MakeRequestUuid))
            .layer(PropagateHeaderLayer::new(id)),
    )
}

/// Defence in depth for `SameSite=Lax` cookies (open question 55): a
/// cookie-authenticated non-safe request whose `Origin` (or, without one,
/// `Referer`) is outside the CORS allow-list is refused before any handler runs.
/// Requests without the session cookie or with a bearer/bot header are not
/// cookie-authenticated and pass through.
async fn origin_check(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let cookie_authed = !csrf::is_safe_method(req.method())
        && req.headers().get(header::AUTHORIZATION).is_none()
        && parse_cookie(req.headers(), SESSION_COOKIE).is_some();
    if cookie_authed && !csrf::origin_allowed(req.headers(), &state.settings().allowed_origins) {
        return crate::HttpError::from(ErrorCode::MissingAccess).into_response();
    }
    next.run(req).await
}

/// The API reference viewer (doc 05 section 4), a single static page that
/// embeds the OpenAPI document.
async fn api_docs(State(state): State<AppState>) -> Html<String> {
    Html(utoipa_scalar::Scalar::new(state.openapi().clone()).to_html())
}

/// Router for the loopback-only operations listener: `/internal/metrics`.
/// Never expose this on the public listener.
pub fn internal_router(state: AppState) -> Router {
    Router::new()
        .route("/internal/metrics", get(health::metrics))
        .with_state(state)
}

/// A Web Share Target POST that reached the server because no service worker took it (G-77):
/// the body is never read, and the browser is sent to the share page with a GET, which says to
/// open SIDE ROOM once and share again.
async fn share_fallback() -> axum::response::Redirect {
    axum::response::Redirect::to("/share")
}

async fn not_found() -> (StatusCode, axum::Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        axum::Json(serde_json::json!({ "code": 0, "message": "404: Not Found" })),
    )
}

fn cors(state: &AppState) -> CorsLayer {
    let origins: Vec<HeaderValue> = state
        .settings()
        .allowed_origins
        .iter()
        .filter_map(|o| HeaderValue::from_str(o).ok())
        .collect();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            HeaderName::from_static("x-sr-csrf"),
            HeaderName::from_static("x-audit-log-reason"),
        ])
        .expose_headers([
            HeaderName::from_static("x-request-id"),
            HeaderName::from_static("x-ratelimit-limit"),
            HeaderName::from_static("x-ratelimit-remaining"),
            HeaderName::from_static("x-ratelimit-reset"),
            HeaderName::from_static("x-ratelimit-reset-after"),
            HeaderName::from_static("x-ratelimit-bucket"),
            HeaderName::from_static("x-ratelimit-scope"),
            HeaderName::from_static("x-ratelimit-global"),
            header::RETRY_AFTER,
        ])
        .max_age(Duration::from_secs(3600))
}

/// Outermost first: request id, trace span, metrics, id propagation,
/// compression, CORS, timeout.
fn with_common_layers(app: Router<AppState>, state: &AppState) -> Router<AppState> {
    let id = HeaderName::from_static(REQUEST_ID);
    app.layer(
        ServiceBuilder::new()
            .layer(SetRequestIdLayer::new(id.clone(), MakeRequestUuid))
            .layer(
                TraceLayer::new_for_http()
                    .make_span_with(|req: &Request<_>| {
                        let request_id = req
                            .headers()
                            .get(REQUEST_ID)
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("-");
                        tracing::info_span!(
                            "request",
                            method = %req.method(),
                            path = %redact_path(req.uri().path()),
                            request_id,
                        )
                    })
                    .on_response(DefaultOnResponse::new().level(Level::INFO)),
            )
            .layer(from_fn(observability::http_metrics))
            .layer(from_fn(audit_reason_scope))
            .layer(from_fn_with_state(
                SecurityHeaderConfig {
                    hsts: state.settings().secure_cookies,
                },
                security_headers,
            ))
            .layer(PropagateHeaderLayer::new(id))
            .layer(CompressionLayer::new())
            .layer(cors(state))
            .layer(TimeoutLayer::with_status_code(
                StatusCode::REQUEST_TIMEOUT,
                state.settings().request_timeout,
            )),
    )
}

/// The request path for logs, with credentials carried in the path masked: the token of
/// `/interactions/{id}/{token}/callback` and of `/webhooks/{id}/{token}[/...]`.
fn redact_path(path: &str) -> String {
    let mut segs: Vec<&str> = path.split('/').collect();
    for i in 0..segs.len() {
        if matches!(segs[i], "interactions" | "webhooks") && i + 2 < segs.len() {
            segs[i + 2] = ":redacted:";
        }
    }
    segs.join("/")
}

#[cfg(test)]
mod redact_tests {
    use super::redact_path;

    #[test]
    fn tokens_in_paths_are_masked() {
        assert_eq!(
            redact_path("/api/v1/interactions/123/SECRET/callback"),
            "/api/v1/interactions/123/:redacted:/callback"
        );
        assert_eq!(
            redact_path("/api/v10/webhooks/5/SECRET/messages/@original"),
            "/api/v10/webhooks/5/:redacted:/messages/@original"
        );
        assert_eq!(redact_path("/api/v1/webhooks/5"), "/api/v1/webhooks/5");
        assert_eq!(
            redact_path("/api/v1/channels/1/messages"),
            "/api/v1/channels/1/messages"
        );
        assert_eq!(redact_path("/api/v1/interactions"), "/api/v1/interactions");
    }
}

/// Scope the request's `X-Audit-Log-Reason` for audit writes made while serving it
/// (LNG-SSET-056): guild mutations record the reason without each route passing it on.
async fn audit_reason_scope(
    req: Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    match crate::routes::settings::deps::audit_reason(req.headers()) {
        Some(reason) => sr_chat::settings::audit::with_ambient_reason(reason, next.run(req)).await,
        None => next.run(req).await,
    }
}

#[cfg(test)]
mod audit_reason_tests {
    use super::audit_reason_scope;
    use axum::{Router, body::Body, http::Request, middleware::from_fn, routing::get};
    use tower::ServiceExt;

    #[tokio::test]
    async fn the_header_reaches_audit_writes_percent_decoded() {
        let app: Router = Router::new()
            .route(
                "/",
                get(|| async { sr_chat::settings::audit::ambient_reason().unwrap_or_default() }),
            )
            .layer(from_fn(audit_reason_scope));
        let call = |reason: Option<&'static str>| {
            let app = app.clone();
            async move {
                let mut req = Request::builder().uri("/");
                if let Some(r) = reason {
                    req = req.header("x-audit-log-reason", r);
                }
                let res = app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
                let bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
                String::from_utf8(bytes.to_vec()).unwrap()
            }
        };
        assert_eq!(call(Some("raid%20cleanup")).await, "raid cleanup");
        assert_eq!(call(None).await, "");
    }
}
