//! Router composition and tower layers (FND-API-001).

use crate::{health, observability, openapi, rate_limit::rate_limit, state::AppState};
use axum::{
    Router,
    http::{HeaderName, HeaderValue, Method, Request, StatusCode, header},
    middleware::{from_fn, from_fn_with_state},
    routing::get,
};
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
pub fn build_router(state: AppState, extra_v1: Router<AppState>) -> Router {
    let v1 = Router::new()
        .route("/openapi.json", get(openapi::openapi_json))
        .route("/users/@me", get(openapi::current_user))
        .route("/sr/csrf", get(openapi::csrf_token))
        .merge(extra_v1)
        .route_layer(from_fn_with_state(state.clone(), rate_limit));

    let app = Router::new()
        .route("/healthz", get(health::healthz))
        .route("/readyz", get(health::readyz))
        .nest("/api/v1", v1.clone())
        .nest("/api/v10", v1)
        .fallback(not_found);
    with_common_layers(app, &state).with_state(state)
}

/// Router for the loopback-only operations listener: `/internal/metrics`.
/// Never expose this on the public listener.
pub fn internal_router(state: AppState) -> Router {
    Router::new()
        .route("/internal/metrics", get(health::metrics))
        .with_state(state)
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
                            path = %req.uri().path(),
                            request_id,
                        )
                    })
                    .on_response(DefaultOnResponse::new().level(Level::INFO)),
            )
            .layer(from_fn(observability::http_metrics))
            .layer(PropagateHeaderLayer::new(id))
            .layer(CompressionLayer::new())
            .layer(cors(state))
            .layer(TimeoutLayer::with_status_code(
                StatusCode::REQUEST_TIMEOUT,
                state.settings().request_timeout,
            )),
    )
}
