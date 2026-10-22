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
//! * `/cdn/external/{hmac}/{url_b64}` is the third-party image proxy
//!   ([`external`]); it has its own buckets and no storage object of its own
//!   beyond the cache.

pub mod content;
pub mod external;
pub mod path;
mod serve;
mod variant;

#[cfg(test)]
mod variant_tests;

#[cfg(test)]
mod external_tests;

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

/// A playlist or map of the `hls` kind that is rendered on request (its URLs carry fresh signatures)
/// instead of being stored. `rel` is the path under the asset root, for example `master.m3u8` or
/// `v/1080p/index.m3u8`.
#[async_trait::async_trait]
pub trait PlaylistRenderer: Send + Sync {
    /// `None`: not a rendered file (the object is read from storage) or unknown.
    async fn render(&self, asset_id: i64, rel: &str, now: u64) -> Option<Rendered>;

    /// Storage key of a stored (not rendered) file of the asset, for assets whose outputs do not live under
    /// the default `video/<asset>/` prefix (transcode outputs live under `transcode/<job>/<attempt>/`).
    /// `None`: use the default key.
    async fn storage_key(&self, _asset_id: i64, _rel: &str) -> Option<String> {
        None
    }
}

/// A rendered playlist.
pub struct Rendered {
    pub content_type: &'static str,
    pub body: String,
}

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
    /// Budget for resizing (bytes, pixels, frames).
    pub limits: sr_storage::img::Limits,
    /// Caps concurrent resize jobs (CPU and memory).
    pub render_permits: Arc<tokio::sync::Semaphore>,
    /// Renders the playlists of the `hls` kind; without it they are read from storage.
    pub playlists: Option<Arc<dyn PlaylistRenderer>>,
    /// Objects a takedown blocked (MOD-TKD-004): answered with 451 instead of being served.
    pub blocklist: Option<Arc<sr_moderation::blocklist::MediaBlocklist>>,
}

/// Resize jobs allowed at once.
pub const RENDER_CONCURRENCY: usize = 4;

impl CdnDeps {
    /// Bundle storage and signer, with the default resize budget.
    pub fn new(storage: Arc<dyn ObjectStorage>, signer: CdnSigner) -> Self {
        Self {
            storage,
            signer,
            limits: sr_storage::img::Limits::default(),
            render_permits: Arc::new(tokio::sync::Semaphore::new(RENDER_CONCURRENCY)),
            playlists: None,
            blocklist: None,
        }
    }

    /// Render the `hls` playlists on request.
    pub fn with_playlists(mut self, playlists: Arc<dyn PlaylistRenderer>) -> Self {
        self.playlists = Some(playlists);
        self
    }

    /// Refuse the objects the takedown register blocked.
    pub fn with_blocklist(mut self, b: Arc<sr_moderation::blocklist::MediaBlocklist>) -> Self {
        self.blocklist = Some(b);
        self
    }

    /// Replace the resize budget.
    pub fn with_limits(mut self, limits: sr_storage::img::Limits) -> Self {
        self.limits = limits;
        self
    }
}

/// The bare route (`/cdn/{*path}`), GET and HEAD. Axum answers HEAD from GET
/// routes, but the handler distinguishes it to avoid reading the body.
pub fn router() -> Router<AppState> {
    Router::new()
        // The static prefix wins over the catch-all below.
        .route("/cdn/external/{hmac}/{url_b64}", get(external::serve))
        .route("/cdn/{*path}", get(serve::serve))
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
