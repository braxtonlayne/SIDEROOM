//! CDN delivery route (FND-STOR-002): `GET|HEAD /cdn/{*path}?ex=<unix>&sig=<b64url>`.
//!
//! Mounted OUTSIDE `/api/v1`, on the public listener, with no cookie auth and
//! no CSRF: the HMAC signature in the URL is the credential (a bearer link).
//! The contract is documented in `docs/05-api-and-gateway.md` section 5.2.
//!
//! Wiring, in the composition root:
//!
//! ```ignore
//! let state = AppState::builder()
//!     .extension(Arc::new(CdnDeps::new(storage, signer)))
//!     .build();
//! let app = build_router(state.clone(), api).merge(routes::cdn::layered(state));
//! ```
//!
//! Decisions:
//! * bad or missing signature: 403; expired (signature valid): 410. The
//!   signature is checked before the object is touched, so 404 is only ever
//!   returned to a holder of a valid signature.
//! * A malformed path (traversal, empty segment, backslash, bad escape) is 400
//!   and an unknown kind is 404, both decided before storage is consulted and
//!   independent of whether any object exists.
//! * `Range`: one `bytes` range gives 206; unsatisfiable gives 416 with
//!   `Content-Range: bytes */len`; a multi-range or syntactically invalid
//!   header is ignored and the full object is served with 200 (RFC 9110 14.2
//!   permits ignoring Range). `If-Range` with a non-matching validator also
//!   serves 200.
//! * Rate limit: the process-wide limiter, bucket `GET /cdn/{*path}` per client IP.

pub mod content;
pub mod path;
mod serve;

#[cfg(test)]
mod tests;

pub use path::{cdn_path, encode_segment};

use crate::state::AppState;
use axum::{
    Router,
    http::{HeaderName, HeaderValue, Method, Request, StatusCode, header},
    routing::get,
};
use sr_storage::{ObjectStorage, cdn::CdnSigner};
use std::{sync::Arc, time::Duration};
use tower::ServiceBuilder;
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    propagate_header::PropagateHeaderLayer,
    request_id::{MakeRequestUuid, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::{DefaultOnResponse, TraceLayer},
};

/// Longest `Cache-Control: max-age` handed out (the signed URL's remaining
/// lifetime is used when shorter).
pub const MAX_CACHE: Duration = Duration::from_secs(24 * 60 * 60);

/// Services the CDN route needs; register with `AppStateBuilder::extension`.
#[derive(Clone)]
pub struct CdnDeps {
    /// Where objects live.
    pub storage: Arc<dyn ObjectStorage>,
    /// The same signer that minted the URLs (`MessagesDeps::with_cdn_signer`).
    pub signer: CdnSigner,
}

impl CdnDeps {
    /// Bundle storage and signer.
    pub fn new(storage: Arc<dyn ObjectStorage>, signer: CdnSigner) -> Self {
        Self { storage, signer }
    }
}

/// The bare route (`/cdn/{*path}`), GET and HEAD. Axum answers HEAD from GET
/// routes, but the handler distinguishes it to avoid reading the body.
pub fn router() -> Router<AppState> {
    Router::new().route("/cdn/{*path}", get(serve::serve))
}

/// [`router`] with state applied and the layers the CDN needs: request id,
/// trace, id propagation, CORS for the configured public origins only (no
/// credentials), a timeout for non-streaming phases, and the rate limiter.
/// No compression layer (media is already compressed and ranges must be exact).
pub fn layered(state: AppState) -> Router {
    let id = HeaderName::from_static("x-request-id");
    let origins: Vec<HeaderValue> = state
        .settings()
        .allowed_origins
        .iter()
        .filter_map(|o| HeaderValue::from_str(o).ok())
        .collect();
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([Method::GET, Method::HEAD, Method::OPTIONS])
        .allow_headers([
            header::RANGE,
            header::IF_NONE_MATCH,
            header::IF_RANGE,
            header::CONTENT_TYPE,
        ])
        .expose_headers([
            header::CONTENT_LENGTH,
            header::CONTENT_RANGE,
            header::ACCEPT_RANGES,
            header::ETAG,
            header::RETRY_AFTER,
            id.clone(),
        ])
        .max_age(Duration::from_secs(3600));
    let timeout = state.settings().request_timeout;
    router()
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            serve::rate_limit,
        ))
        .layer(
            ServiceBuilder::new()
                .layer(SetRequestIdLayer::new(id.clone(), MakeRequestUuid))
                .layer(
                    TraceLayer::new_for_http()
                        .make_span_with(|req: &Request<_>| {
                            // Never log the query: it carries the signature.
                            tracing::info_span!(
                                "request",
                                method = %req.method(),
                                path = %req.uri().path(),
                            )
                        })
                        .on_response(DefaultOnResponse::new().level(tracing::Level::INFO)),
                )
                .layer(PropagateHeaderLayer::new(id))
                .layer(cors)
                .layer(TimeoutLayer::with_status_code(
                    StatusCode::REQUEST_TIMEOUT,
                    timeout,
                )),
        )
        .with_state(state)
}
