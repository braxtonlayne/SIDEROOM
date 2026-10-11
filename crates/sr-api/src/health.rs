//! `/healthz`, `/readyz` and `/internal/metrics` (FND-API-009).

use crate::state::AppState;
use async_trait::async_trait;
use axum::{
    Json,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
};
use serde_json::{Value, json};

/// A dependency probe for `/readyz` (database, object storage, ...).
#[async_trait]
pub trait ReadinessCheck: Send + Sync {
    /// Stable name shown in the response.
    fn name(&self) -> &'static str;
    /// `Err(reason)` when the dependency is unreachable.
    async fn check(&self) -> Result<(), String>;
}

/// Liveness: the process is up and serving.
pub async fn healthz() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// Readiness: every registered check passes within its timeout.
pub async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    let timeout = state.settings().readiness_timeout;
    let results = futures::future::join_all(state.readiness().iter().map(|c| async move {
        let r = match tokio::time::timeout(timeout, c.check()).await {
            Ok(r) => r,
            Err(_) => Err("timed out".to_owned()),
        };
        (c.name(), r)
    }))
    .await;
    let mut ok = true;
    let mut checks = serde_json::Map::new();
    for (name, r) in results {
        match r {
            Ok(()) => {
                checks.insert(name.to_owned(), json!("ok"));
            }
            Err(e) => {
                ok = false;
                tracing::warn!(check = name, error = %e, "readiness check failed");
                checks.insert(name.to_owned(), json!({ "error": e }));
            }
        }
    }
    let status = if ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(json!({ "status": if ok { "ok" } else { "unavailable" }, "checks": checks })),
    )
}

/// Prometheus text exposition.
pub async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        state.metrics().render(),
    )
}
