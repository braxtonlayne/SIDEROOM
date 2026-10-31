//! sr-api: axum router composition, tower layers, auth extractors, rate
//! limiting, OpenAPI and operational endpoints. See docs/02-architecture.md
//! section 3 and docs/05-api-and-gateway.md.
//!
//! Entry points: build an [`AppState`] with [`AppState::builder`], then call
//! [`build_router`] (public listener) and [`internal_router`] (loopback-only
//! listener serving `/internal/metrics`). Feature crates contribute routes by
//! passing a `Router<AppState>` to [`build_router`]; they are mounted under
//! both `/api/v1` and the `/api/v10` compatibility alias.

pub mod auth;
pub mod error;
pub mod health;
pub mod module_gate;
pub mod observability;
pub mod openapi;
pub mod rate_limit;
pub mod router;
pub mod routes;
pub mod security_headers;
pub mod sensitive;
pub mod state;

pub use auth::{AuthKind, Principal};
pub use error::HttpError;
pub use router::{build_router, internal_router};
pub use state::{ApiSettings, AppState, AppStateBuilder};
