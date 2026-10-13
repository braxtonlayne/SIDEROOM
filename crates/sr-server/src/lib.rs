//! sr-server: wires config, Postgres, storage, the event bus, the job runner,
//! the gateway and the HTTP API into one process. See docs/02-architecture.md
//! section 3 and docs/dev/server.md.

pub mod admin_jobs;
pub mod app;
pub mod backends;
pub mod external_jobs;
pub mod features_store;
pub mod gateway_pg;
pub mod jobs;
pub mod lag_bus;
pub mod library_jobs;
pub mod ready;
pub mod rich_jobs;
pub mod search_jobs;
pub mod settings_pg;

/// The OpenAPI document served at `/api/v1/openapi.json` and printed by
/// `sr-server openapi`.
pub fn openapi_json() -> serde_json::Result<String> {
    serde_json::to_string_pretty(&sr_api::routes::openapi())
}

#[cfg(test)]
mod tests {
    #[test]
    fn openapi_is_deterministic_and_has_core_paths() {
        let a = super::openapi_json().unwrap();
        assert_eq!(a, super::openapi_json().unwrap());
        let v: serde_json::Value = serde_json::from_str(&a).unwrap();
        assert!(v["paths"]["/users/@me"].is_object());
        assert!(v["paths"]["/sr/csrf"].is_object());
    }
}
