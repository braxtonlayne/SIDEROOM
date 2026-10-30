//! The currency rate job against real Postgres (UTL-CALC-001; run with `SR_TEST_DATABASE_URL`, `--ignored`,
//! `--test-threads=1`). No network: the fetcher is a fixture.

use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use sr_api::routes::tools::fx::render;
use sr_db::testing::TestDb;

use crate::fx_jobs::{
    FxFetch, Outcome, enabled, fixture_for_tests, last_success_ms, run_once, snapshot,
};

struct Canned {
    body: Result<Vec<u8>, String>,
    calls: AtomicUsize,
}

impl Canned {
    fn ok(date: &str, usd: &str) -> Self {
        Self::raw(Ok(fixture_for_tests(date, usd).into_bytes()))
    }
    fn err(msg: &str) -> Self {
        Self::raw(Err(msg.to_owned()))
    }
    fn raw(body: Result<Vec<u8>, String>) -> Self {
        Self {
            body,
            calls: AtomicUsize::new(0),
        }
    }
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl FxFetch for Canned {
    async fn fetch(&self) -> Result<Vec<u8>, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.body.clone()
    }
}

const HOUR_MS: i64 = 3_600_000;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap())
        .unwrap()
}

async fn age_last_success(pool: &sqlx::PgPool, interval: &str) {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE fx_fetch_state SET last_success_at = now() - interval '{interval}'"
    )))
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_first_run_stores_the_set_and_the_snapshot_serves_it() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    let f = Canned::ok("2026-10-29", "1.0845");
    let out = run_once(pool, &f, now_ms()).await.unwrap();
    assert_eq!(
        out,
        Outcome::Stored {
            currencies: 14,
            as_of: "2026-10-29".into(),
            replaced: true
        }
    );
    let s = snapshot(pool).await.unwrap();
    assert!(s.enabled);
    assert_eq!(s.as_of.as_deref(), Some("2026-10-29"));
    assert_eq!(s.rates.len(), 14);
    assert_eq!(s.rates["USD"], 1.0845);
    assert_eq!(s.stale_after_hours, 96);
    let o = render(s, now_ms() / 1000);
    assert!(o.enabled && !o.stale);
    assert!(last_success_ms(pool).await.unwrap().is_some());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_second_run_within_the_window_makes_no_request() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    let f = Canned::ok("2026-10-29", "1.0845");
    run_once(pool, &f, now_ms()).await.unwrap();
    assert_eq!(f.calls(), 1);
    assert_eq!(
        run_once(pool, &f, now_ms() + HOUR_MS).await.unwrap(),
        Outcome::NotDue
    );
    assert_eq!(f.calls(), 1);
    // 21 hours later it is due again.
    let again = run_once(pool, &f, now_ms() + 21 * HOUR_MS).await.unwrap();
    assert!(matches!(again, Outcome::Stored { .. }));
    assert_eq!(f.calls(), 2);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_setting_off_makes_no_request_and_hides_the_rates() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    assert!(enabled(pool).await.unwrap(), "on by default");
    sqlx::query(
        "INSERT INTO platform_settings (key, value) VALUES ('tools.currency_rates', 'false'::jsonb)",
    )
    .execute(pool)
    .await
    .unwrap();
    assert!(!enabled(pool).await.unwrap());
    let f = Canned::ok("2026-10-29", "1.08");
    assert_eq!(
        run_once(pool, &f, now_ms()).await.unwrap(),
        Outcome::Disabled
    );
    assert_eq!(
        f.calls(),
        0,
        "the server must not call out while the tool is off"
    );
    let s = snapshot(pool).await.unwrap();
    assert!(!s.enabled && s.rates.is_empty());
    assert!(!render(s, 0).enabled);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn switching_it_back_on_serves_the_rates_that_were_kept() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    run_once(pool, &Canned::ok("2026-10-29", "1.08"), now_ms())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO platform_settings (key, value) VALUES ('tools.currency_rates', 'false'::jsonb)",
    )
    .execute(pool)
    .await
    .unwrap();
    assert!(snapshot(pool).await.unwrap().rates.is_empty());
    sqlx::query(
        "UPDATE platform_settings SET value = 'true'::jsonb WHERE key = 'tools.currency_rates'",
    )
    .execute(pool)
    .await
    .unwrap();
    assert_eq!(snapshot(pool).await.unwrap().rates.len(), 14);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_failed_fetch_is_recorded_and_the_old_rates_go_stale() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    // Never fetched: empty and stale.
    let s = snapshot(pool).await.unwrap();
    assert!(s.rates.is_empty());
    assert!(render(s, now_ms() / 1000).stale);

    run_once(pool, &Canned::ok("2026-10-23", "1.05"), now_ms())
        .await
        .unwrap();
    // The success is five days old (the stored timestamps move back).
    sqlx::query("UPDATE fx_rates SET fetched_at = now() - interval '5 days'")
        .execute(pool)
        .await
        .unwrap();
    age_last_success(pool, "5 days").await;

    let bad = Canned::err("request failed: connection refused");
    let out = run_once(pool, &bad, now_ms()).await.unwrap();
    assert_eq!(
        out,
        Outcome::Failed("request failed: connection refused".into())
    );
    let (failures, err): (i32, String) =
        sqlx::query_as("SELECT failures, last_error FROM fx_fetch_state WHERE id = 1")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(failures, 1);
    assert!(err.contains("connection refused"));

    // The old rates are still served, flagged stale (96 h by default, 5 days old).
    let s = snapshot(pool).await.unwrap();
    assert_eq!(s.rates.len(), 14);
    let o = render(s, now_ms() / 1000);
    assert!(o.enabled && o.stale);
    assert_eq!(o.as_of.as_deref(), Some("2026-10-23"));

    // Raising the threshold to 10 days clears the flag.
    sqlx::query(
        "INSERT INTO platform_settings (key, value) VALUES ('tools.currency_stale_hours', '240'::jsonb)",
    )
    .execute(pool)
    .await
    .unwrap();
    let o = render(snapshot(pool).await.unwrap(), now_ms() / 1000);
    assert!(!o.stale);

    // Another failure counts up; a success resets the counter.
    run_once(pool, &bad, now_ms()).await.unwrap();
    let failures: i32 = sqlx::query_scalar("SELECT failures FROM fx_fetch_state WHERE id = 1")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(failures, 2);
    let good = Canned::ok("2026-10-29", "1.09");
    assert!(matches!(
        run_once(pool, &good, now_ms()).await.unwrap(),
        Outcome::Stored { .. }
    ));
    let (failures, err): (i32, String) =
        sqlx::query_as("SELECT failures, last_error FROM fx_fetch_state WHERE id = 1")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!((failures, err.as_str()), (0, ""));
    assert!(!render(snapshot(pool).await.unwrap(), now_ms() / 1000).stale);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_garbled_feed_is_a_recorded_failure_and_changes_nothing() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    run_once(pool, &Canned::ok("2026-10-29", "1.08"), now_ms())
        .await
        .unwrap();
    age_last_success(pool, "2 days").await;
    let junk = Canned::raw(Ok(b"<html>captive portal</html>".to_vec()));
    let out = run_once(pool, &junk, now_ms()).await.unwrap();
    assert!(matches!(out, Outcome::Failed(_)), "{out:?}");
    let s = snapshot(pool).await.unwrap();
    assert_eq!(s.rates["USD"], 1.08);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_older_set_never_replaces_a_newer_one_and_dropped_currencies_go() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    run_once(pool, &Canned::ok("2026-10-29", "1.10"), now_ms())
        .await
        .unwrap();
    age_last_success(pool, "2 days").await;
    let out = run_once(pool, &Canned::ok("2026-10-28", "1.00"), now_ms())
        .await
        .unwrap();
    assert_eq!(
        out,
        Outcome::Stored {
            currencies: 14,
            as_of: "2026-10-28".into(),
            replaced: false
        }
    );
    let s = snapshot(pool).await.unwrap();
    assert_eq!(s.rates["USD"], 1.10, "the newer set stays");
    assert_eq!(s.as_of.as_deref(), Some("2026-10-29"));

    // A newer set that no longer lists a currency removes it.
    age_last_success(pool, "2 days").await;
    let doc =
        fixture_for_tests("2026-10-30", "1.20").replace("<Cube currency='AUD' rate='1.6420'/>", "");
    run_once(pool, &Canned::raw(Ok(doc.into_bytes())), now_ms())
        .await
        .unwrap();
    let s = snapshot(pool).await.unwrap();
    assert_eq!(s.rates.len(), 13);
    assert!(!s.rates.contains_key("AUD"));
    assert_eq!(s.rates["USD"], 1.20);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_tables_reject_nonsense() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    for (code, rate) in [
        ("usd", 1.0),
        ("EUR", 1.0),
        ("USDX", 1.0),
        ("USD", 0.0),
        ("USD", -2.0),
    ] {
        let r = sqlx::query(
            "INSERT INTO fx_rates (currency, rate, as_of) VALUES ($1, $2, '2026-10-29')",
        )
        .bind(code)
        .bind(rate)
        .execute(pool)
        .await;
        assert!(r.is_err(), "{code} {rate}");
    }
    // The state row is a singleton.
    assert!(
        sqlx::query("INSERT INTO fx_fetch_state (id) VALUES (2)")
            .execute(pool)
            .await
            .is_err()
    );
}
