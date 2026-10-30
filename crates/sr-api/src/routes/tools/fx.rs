//! Exchange rates for the currency converter (UTL-CALC-001).
//!
//! The server fetches the European Central Bank daily reference rates (EUR base, no key) once a day with the
//! SSRF-safe client, keeps them in `fx_rates` and serves them here; the browser does the arithmetic and never
//! calls a third party. `tools.currency_rates` (a platform setting, default on) switches the fetch and the
//! converter off: the answer is then `enabled: false` with no rates.

use crate::routes::Ext;
use crate::{HttpError, Principal, openapi::ErrorBody};
use async_trait::async_trait;
use axum::Json;
use serde::Serialize;
use sr_core::ErrorCode;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use utoipa::ToSchema;

/// What the store holds (the server wiring reads Postgres; tests use a stub).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FxSnapshot {
    /// `tools.currency_rates`.
    pub enabled: bool,
    /// The ECB reference date of the rates, `YYYY-MM-DD`.
    pub as_of: Option<String>,
    /// When the server last stored them, unix seconds.
    pub fetched_at: Option<i64>,
    /// `tools.currency_stale_hours`.
    pub stale_after_hours: i64,
    /// Currency code to units per one euro. The euro itself is not listed.
    pub rates: BTreeMap<String, f64>,
}

#[async_trait]
pub trait FxSource: Send + Sync {
    async fn snapshot(&self) -> Result<FxSnapshot, String>;
}

/// The collaborator the route reads (registered on the composition root).
pub struct FxDeps {
    pub source: Arc<dyn FxSource>,
}

/// True when there are no rates yet or the last fetch is older than `after_hours`.
pub fn is_stale(fetched_at: Option<i64>, now: i64, after_hours: i64) -> bool {
    match fetched_at {
        None => true,
        Some(at) => now.saturating_sub(at) > after_hours.max(1).saturating_mul(3600),
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FxRatesObject {
    /// False when the instance switched the converter off (`tools.currency_rates`); nothing else is set then.
    pub enabled: bool,
    /// The currency every rate is quoted against; always `EUR`.
    pub base: String,
    /// The ECB reference date, `YYYY-MM-DD`; absent until the first fetch succeeded.
    pub as_of: Option<String>,
    /// When the server stored the rates, unix seconds; absent until the first fetch succeeded.
    pub fetched_at: Option<i64>,
    /// True when there are no rates yet or the newest fetch is older than `tools.currency_stale_hours`. The
    /// client still converts with the old rates and says so.
    pub stale: bool,
    /// Units of the currency per one euro, for example `{"USD": 1.0845}`.
    pub rates: BTreeMap<String, f64>,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

pub fn render(s: FxSnapshot, now: i64) -> FxRatesObject {
    if !s.enabled {
        return FxRatesObject {
            enabled: false,
            base: "EUR".into(),
            as_of: None,
            fetched_at: None,
            stale: false,
            rates: BTreeMap::new(),
        };
    }
    FxRatesObject {
        enabled: true,
        base: "EUR".into(),
        stale: is_stale(s.fetched_at, now, s.stale_after_hours),
        as_of: s.as_of,
        fetched_at: s.fetched_at,
        rates: s.rates,
    }
}

/// The cached ECB reference rates (EUR base). Any signed-in account or bot may read them.
#[utoipa::path(
    get,
    path = "/tools/fx",
    responses((status = 200, body = FxRatesObject), (status = 500, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn rates(_p: Principal, Ext(d): Ext<FxDeps>) -> Result<Json<FxRatesObject>, HttpError> {
    let snap = d.source.snapshot().await.map_err(|e| {
        tracing::error!(error = %e, "fx rates unreadable");
        HttpError::from(ErrorCode::SrInternal)
    })?;
    Ok(Json(render(snap, now_secs())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(enabled: bool, fetched_at: Option<i64>) -> FxSnapshot {
        FxSnapshot {
            enabled,
            as_of: fetched_at.map(|_| "2026-10-29".into()),
            fetched_at,
            stale_after_hours: 96,
            rates: BTreeMap::from([("USD".to_owned(), 1.08)]),
        }
    }

    #[test]
    fn staleness_is_measured_from_the_last_fetch() {
        let now = 1_000_000;
        assert!(is_stale(None, now, 96));
        assert!(!is_stale(Some(now - 3600), now, 96));
        assert!(!is_stale(Some(now - 96 * 3600), now, 96));
        assert!(is_stale(Some(now - 96 * 3600 - 1), now, 96));
        // A clock that ran backwards is not stale.
        assert!(!is_stale(Some(now + 10), now, 96));
    }

    #[test]
    fn off_means_no_rates_and_not_stale() {
        let o = render(snap(false, Some(5)), 1_000_000);
        assert!(!o.enabled);
        assert!(o.rates.is_empty());
        assert!(!o.stale);
        assert_eq!(o.as_of, None);
    }

    #[test]
    fn on_with_fresh_rates() {
        let o = render(snap(true, Some(999_000)), 1_000_000);
        assert!(o.enabled && !o.stale);
        assert_eq!(o.base, "EUR");
        assert_eq!(o.rates["USD"], 1.08);
        assert_eq!(o.as_of.as_deref(), Some("2026-10-29"));
    }

    #[test]
    fn on_before_the_first_fetch_is_stale_and_empty() {
        let mut s = snap(true, None);
        s.rates.clear();
        let o = render(s, 1_000_000);
        assert!(o.enabled && o.stale);
        assert!(o.rates.is_empty());
    }
}
