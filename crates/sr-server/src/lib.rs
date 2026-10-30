//! sr-server: wires config, Postgres, storage, the event bus, the job runner,
//! the gateway and the HTTP API into one process. See docs/02-architecture.md
//! section 3 and docs/dev/server.md.

pub mod activities;
pub mod activities_jobs;
pub mod admin_jobs;
pub mod app;
pub mod apps_jobs;
pub mod backends;
pub mod cinema_catalog;
pub mod cinema_jobs;
pub mod csp_reports;
pub mod endpoint_client;
pub mod events_jobs;
pub mod external_jobs;
pub mod feature_actions;
pub mod features_store;
pub mod gateway_pg;
pub mod healthcheck;
pub mod inbox_pg;
pub mod instance_mount;
pub mod integration_policy;
pub mod jobs;
pub mod jukebox;
pub mod jukebox_jobs;
pub mod lag_bus;
pub mod library_jobs;
pub mod lookup_wiring;
pub mod media_jobs;
pub mod moderation_actions;
pub mod moderation_age;
pub mod moderation_audit;
pub mod moderation_index;
pub mod party_cards;
pub mod party_rooms;
pub mod payments_jobs;
pub mod preflight;
pub mod push_notify;
pub mod push_pg;
pub mod ready;
pub mod review_jobs;
pub mod rich_jobs;
pub mod rings_jobs;
pub mod search_jobs;
pub mod settings_pg;
pub mod staff_feed;
pub mod storage_jobs;
pub mod suspicion_jobs;
pub mod takedown_jobs;
pub mod tv_catalog;
pub mod upload_jobs;
pub mod upload_limits;
pub mod userfeat;
pub mod userset_jobs;
pub mod voice;
pub mod watchparty;
pub mod web;

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

#[cfg(test)]
mod voice_pg_tests;

#[cfg(test)]
mod cinema_wiring_pg_tests;

#[cfg(test)]
mod bot_pg_tests;
#[cfg(test)]
mod instance_mount_pg_tests;
#[cfg(test)]
mod jobs_scale_pg_tests;
#[cfg(test)]
mod jukebox_pg_tests;
#[cfg(test)]
mod party_cards_pg_tests;
#[cfg(test)]
mod rings_pg_tests;
#[cfg(test)]
mod staff_feed_pg_tests;
#[cfg(test)]
mod storage_index_pg_tests;
#[cfg(test)]
mod storage_measure_pg_tests;
#[cfg(test)]
mod tv_wiring_pg_tests;
#[cfg(test)]
mod userfeat_pg_tests;
#[cfg(test)]
mod userset_pg_tests;
#[cfg(test)]
mod watchparty_pg_tests;

#[cfg(test)]
mod feature_actions_pg_tests;

#[cfg(test)]
mod moderation_index_pg_tests;
