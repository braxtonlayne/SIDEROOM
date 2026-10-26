//! Live checks of the BYOS mount calls against a real S3-compatible server (Garage in CI): the connection test,
//! the CORS apply and check, and a live preflight. Ignored by default; same environment as `s3_live.rs`:
//!
//! ```text
//! SR_TEST_S3_ENDPOINT=http://127.0.0.1:3900 SR_TEST_S3_BUCKET=... SR_TEST_S3_ACCESS_KEY=... \
//! SR_TEST_S3_SECRET_KEY=... cargo test -p sr-storage --test s3_mounts_live -- --ignored --test-threads=1
//! ```
//!
//! Without `SR_TEST_S3_ENDPOINT` each test skips. The CORS test replaces the bucket's CORS rules with the one for
//! `https://chat.example.test`, which is the shared dev bucket's state anyway (`sr-cli storage ensure-cors` runs
//! later in the e2e jobs and writes its own).

use std::time::Duration;

use sr_storage::cors_check::{self, CorsState};
use sr_storage::mounts::Provider;
use sr_storage::probe::{self, ProbeStatus};
use sr_storage::s3admin::S3Admin;

macro_rules! require_s3 {
    () => {
        if std::env::var("SR_TEST_S3_ENDPOINT").is_err() {
            eprintln!("skipping: SR_TEST_S3_ENDPOINT is not set");
            return;
        }
    };
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set"))
}

fn admin() -> S3Admin {
    S3Admin::new(
        &env("SR_TEST_S3_ENDPOINT"),
        &env("SR_TEST_S3_BUCKET"),
        &std::env::var("SR_TEST_S3_REGION").unwrap_or_else(|_| "garage".into()),
        &env("SR_TEST_S3_ACCESS_KEY"),
        &env("SR_TEST_S3_SECRET_KEY"),
    )
    .expect("admin client")
}

const ORIGIN: &str = "https://chat.example.test";

#[tokio::test]
#[ignore = "needs a live S3 (SR_TEST_S3_ENDPOINT)"]
async fn the_connection_test_passes_on_garage() {
    require_s3!();
    let t = probe::run(&admin(), "sr-live-mounts/", Duration::from_secs(20)).await;
    for r in &t.rows {
        eprintln!("{} {:?}: {}", r.id, r.status, r.message);
    }
    assert!(!t.failed(), "no row may fail on the dev Garage: {t:?}");
    for id in ["head_bucket", "list", "put", "multipart"] {
        let row = t.rows.iter().find(|r| r.id == id).unwrap();
        assert_eq!(row.status, ProbeStatus::Pass, "{id}: {}", row.message);
    }
}

#[tokio::test]
#[ignore = "needs a live S3 (SR_TEST_S3_ENDPOINT)"]
async fn apply_then_check_is_ok_and_a_preflight_agrees() {
    require_s3!();
    let a = admin();
    let applied = cors_check::apply(&a, Provider::Garage, ORIGIN)
        .await
        .expect("apply");
    assert_eq!(applied.state, CorsState::Ok, "{applied:?}");
    let checked = cors_check::check(&a, Provider::Garage, ORIGIN)
        .await
        .expect("check");
    assert_eq!(checked.state, CorsState::Ok, "{checked:?}");
    assert!(
        a.preflight_allows(probe::PROBE_NAME, ORIGIN, Duration::from_secs(10))
            .await
            .expect("preflight"),
        "Garage must answer the preflight for the allowed origin"
    );
    // Another origin is not allowed by that rule.
    assert!(
        !a.preflight_allows(
            probe::PROBE_NAME,
            "https://evil.example.test",
            Duration::from_secs(10)
        )
        .await
        .expect("preflight")
    );
    // A different rule set is reported as such, and applying repairs it.
    a.put_bucket_cors(&["https://other.example.test".to_owned()])
        .await
        .expect("put other");
    let r = cors_check::check(&a, Provider::Garage, ORIGIN)
        .await
        .expect("check");
    assert_eq!(r.state, CorsState::Different, "{r:?}");
    assert_eq!(
        cors_check::apply(&a, Provider::Garage, ORIGIN)
            .await
            .expect("apply")
            .state,
        CorsState::Ok
    );
}
