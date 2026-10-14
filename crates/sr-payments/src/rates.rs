//! BTC/USD price: operator-configured sources, a median, a short cache, and a stale-rate guard.
//!
//! * Every [`RateSource`] answers USD cents per BTC. The [`RateService`] asks all of them, takes the
//!   median, and refuses answers that disagree by more than `max_spread_bps` (a broken source must
//!   not set the price).
//! * Answers are cached for `cache_secs`. When every source fails the last good quote is used for
//!   at most `max_age_secs` and is flagged `stale`; beyond that no invoice can be created.
//! * Invoices lock the quote they were created with (see `invoice::sats_for_cents`).

use crate::{
    chain::HttpGet,
    error::{PayError, PayResult},
};
use async_trait::async_trait;
use std::sync::{Arc, Mutex, MutexGuard};

/// A source of the BTC price in USD cents.
#[async_trait]
pub trait RateSource: Send + Sync {
    fn name(&self) -> &str;
    /// USD cents for one BTC.
    async fn fetch(&self) -> Result<u64, String>;
}

/// Plausibility window for a quote: $1,000 to $10,000,000 per BTC.
pub const MIN_CENTS_PER_BTC: u64 = 100_000;
pub const MAX_CENTS_PER_BTC: u64 = 1_000_000_000;

/// A JSON endpoint: `url` returns a document, `pointer` (RFC 6901) selects the USD price
/// (a number or a numeric string). Configured by the operator, e.g. `/bitcoin/usd`.
pub struct HttpJsonRate {
    name: String,
    url: String,
    pointer: String,
    http: Arc<dyn HttpGet>,
}

impl HttpJsonRate {
    pub fn new(name: &str, url: &str, pointer: &str, http: Arc<dyn HttpGet>) -> Self {
        Self {
            name: name.to_owned(),
            url: url.to_owned(),
            pointer: pointer.to_owned(),
            http,
        }
    }
}

/// Reads USD cents out of a JSON document.
pub fn parse_usd_price(json: &str, pointer: &str) -> Result<u64, String> {
    let doc: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("bad JSON: {e}"))?;
    let v = doc
        .pointer(pointer)
        .ok_or("price not found at the pointer")?;
    let usd = match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
    .filter(|x| x.is_finite() && *x > 0.0)
    .ok_or("price is not a positive number")?;
    let cents = (usd * 100.0).round();
    if cents < MIN_CENTS_PER_BTC as f64 || cents > MAX_CENTS_PER_BTC as f64 {
        return Err("price outside the plausible range".into());
    }
    Ok(cents as u64)
}

#[async_trait]
impl RateSource for HttpJsonRate {
    fn name(&self) -> &str {
        &self.name
    }

    async fn fetch(&self) -> Result<u64, String> {
        let body = self.http.get(&self.url).await?;
        parse_usd_price(&body, &self.pointer)
    }
}

/// A scripted source (tests, local development).
pub struct FakeRate {
    name: String,
    answer: Mutex<Result<u64, String>>,
}

impl FakeRate {
    pub fn new(name: &str, cents: u64) -> Self {
        Self {
            name: name.to_owned(),
            answer: Mutex::new(Ok(cents)),
        }
    }

    pub fn set(&self, answer: Result<u64, String>) {
        *self.answer.lock().unwrap_or_else(|e| e.into_inner()) = answer;
    }
}

#[async_trait]
impl RateSource for FakeRate {
    fn name(&self) -> &str {
        &self.name
    }

    async fn fetch(&self) -> Result<u64, String> {
        self.answer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

#[derive(Debug, Clone)]
pub struct RateSettings {
    pub cache_secs: u64,
    pub max_age_secs: u64,
    /// Largest accepted spread between the highest and lowest answer, in basis points of the median.
    pub max_spread_bps: u64,
    pub min_sources: usize,
}

impl Default for RateSettings {
    fn default() -> Self {
        Self {
            cache_secs: 60,
            max_age_secs: 900,
            max_spread_bps: 1000,
            min_sources: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateQuote {
    pub cents_per_btc: u64,
    /// `coinbase` or `median(a,b)`.
    pub source: String,
    pub at_ms: i64,
    /// Served from the last good answer because the sources currently fail.
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceStatus {
    pub name: String,
    pub last_cents: Option<u64>,
    pub last_ok_ms: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Default)]
struct State {
    last: Option<RateQuote>,
    statuses: Vec<SourceStatus>,
}

pub struct RateService {
    sources: Vec<Arc<dyn RateSource>>,
    settings: RateSettings,
    state: Mutex<State>,
}

/// Median of a non-empty list (mean of the two middle values for an even count).
pub fn median(values: &mut [u64]) -> u64 {
    values.sort_unstable();
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2
    }
}

impl RateService {
    pub fn new(sources: Vec<Arc<dyn RateSource>>, settings: RateSettings) -> Self {
        let statuses = sources
            .iter()
            .map(|s| SourceStatus {
                name: s.name().to_owned(),
                last_cents: None,
                last_ok_ms: None,
                last_error: None,
            })
            .collect();
        Self {
            sources,
            settings,
            state: Mutex::new(State {
                last: None,
                statuses,
            }),
        }
    }

    fn st(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn has_sources(&self) -> bool {
        !self.sources.is_empty()
    }

    /// The current quote (cached, refreshed, or stale-but-allowed).
    pub async fn quote(&self, now_ms: i64) -> PayResult<RateQuote> {
        if let Some(q) = self.st().last.clone()
            && now_ms - q.at_ms < i64::try_from(self.settings.cache_secs * 1000).unwrap_or(i64::MAX)
            && !q.stale
        {
            return Ok(q);
        }
        let answers = futures::future::join_all(self.sources.iter().map(|s| s.fetch())).await;
        let mut ok: Vec<(usize, u64)> = Vec::new();
        {
            let mut st = self.st();
            for (i, a) in answers.into_iter().enumerate() {
                let s = &mut st.statuses[i];
                match a {
                    Ok(c) if (MIN_CENTS_PER_BTC..=MAX_CENTS_PER_BTC).contains(&c) => {
                        s.last_cents = Some(c);
                        s.last_ok_ms = Some(now_ms);
                        s.last_error = None;
                        ok.push((i, c));
                    }
                    Ok(_) => s.last_error = Some("price outside the plausible range".into()),
                    Err(e) => s.last_error = Some(e),
                }
            }
        }
        let fresh = self.combine(&ok, now_ms);
        let mut st = self.st();
        match fresh {
            Some(q) => {
                st.last = Some(q.clone());
                Ok(q)
            }
            None => match st.last.clone() {
                Some(mut q)
                    if now_ms - q.at_ms
                        <= i64::try_from(self.settings.max_age_secs * 1000).unwrap_or(i64::MAX) =>
                {
                    q.stale = true;
                    Ok(q)
                }
                _ => Err(PayError::RateUnavailable),
            },
        }
    }

    fn combine(&self, ok: &[(usize, u64)], now_ms: i64) -> Option<RateQuote> {
        if ok.is_empty() || ok.len() < self.settings.min_sources {
            return None;
        }
        let mut values: Vec<u64> = ok.iter().map(|(_, c)| *c).collect();
        let med = median(&mut values);
        let (lo, hi) = (values[0], values[values.len() - 1]);
        if ok.len() > 1 && (hi - lo) * 10_000 / med.max(1) > self.settings.max_spread_bps {
            let mut st = self.st();
            for (i, _) in ok {
                st.statuses[*i].last_error = Some("sources disagree; quote refused".into());
            }
            return None;
        }
        let st = self.st();
        let names: Vec<&str> = ok
            .iter()
            .map(|(i, _)| st.statuses[*i].name.as_str())
            .collect();
        let source = if names.len() == 1 {
            names[0].to_owned()
        } else {
            format!("median({})", names.join(","))
        };
        Some(RateQuote {
            cents_per_btc: med,
            source,
            at_ms: now_ms,
            stale: false,
        })
    }

    pub fn statuses(&self) -> Vec<SourceStatus> {
        self.st().statuses.clone()
    }

    /// The last quote without fetching (for display).
    pub fn last(&self) -> Option<RateQuote> {
        self.st().last.clone()
    }
}

/// Satoshis for `cents` USD at `cents_per_btc`, rounded up (the payer never underpays by rounding).
pub fn sats_for_cents(cents: u64, cents_per_btc: u64) -> u64 {
    let n = u128::from(cents) * 100_000_000;
    let d = u128::from(cents_per_btc.max(1));
    u64::try_from(n.div_ceil(d)).unwrap_or(u64::MAX)
}

/// USD cents for `sats` at `cents_per_btc`, rounded down.
pub fn cents_for_sats(sats: u64, cents_per_btc: u64) -> u64 {
    u64::try_from(u128::from(sats) * u128::from(cents_per_btc) / 100_000_000).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svc(vals: &[(&str, u64)]) -> (RateService, Vec<Arc<FakeRate>>) {
        let fakes: Vec<Arc<FakeRate>> = vals
            .iter()
            .map(|(n, c)| Arc::new(FakeRate::new(n, *c)))
            .collect();
        let sources = fakes
            .iter()
            .map(|f| f.clone() as Arc<dyn RateSource>)
            .collect();
        (RateService::new(sources, RateSettings::default()), fakes)
    }

    #[test]
    fn conversion_rounds_up_and_down() {
        // $5.00 at $65,000/BTC = 7692.3 sats, rounded up.
        assert_eq!(sats_for_cents(500, 6_500_000), 7_693);
        assert_eq!(sats_for_cents(6_500_000, 6_500_000), 100_000_000);
        assert_eq!(cents_for_sats(7_693, 6_500_000), 500);
        assert_eq!(sats_for_cents(0, 6_500_000), 0);
    }

    #[test]
    fn median_odd_even() {
        assert_eq!(median(&mut [5, 1, 9]), 5);
        assert_eq!(median(&mut [10, 20]), 15);
    }

    #[test]
    fn price_parsing() {
        assert_eq!(
            parse_usd_price(r#"{"bitcoin":{"usd":65000.5}}"#, "/bitcoin/usd").unwrap(),
            6_500_050
        );
        assert_eq!(
            parse_usd_price(r#"{"data":{"amount":"64999.99"}}"#, "/data/amount").unwrap(),
            6_499_999
        );
        assert!(parse_usd_price(r#"{"a":1}"#, "/b").is_err());
        assert!(parse_usd_price(r#"{"a":"x"}"#, "/a").is_err());
        assert!(parse_usd_price(r#"{"a":0.5}"#, "/a").is_err());
        assert!(parse_usd_price(r#"{"a":-5}"#, "/a").is_err());
    }

    #[tokio::test]
    async fn median_of_sources_and_cache() {
        let (s, f) = svc(&[("a", 6_000_000), ("b", 6_200_000), ("c", 6_100_000)]);
        let q = s.quote(1_000).await.unwrap();
        assert_eq!(q.cents_per_btc, 6_100_000);
        assert_eq!(q.source, "median(a,b,c)");
        assert!(!q.stale);
        // Cached: a changed source is not asked again within the cache window.
        f[0].set(Ok(1_000_000));
        assert_eq!(s.quote(30_000).await.unwrap().cents_per_btc, 6_100_000);
        // After the window the new answer is read (and the outlier is rejected for spread).
        assert!(s.quote(100_000).await.unwrap().stale);
    }

    #[tokio::test]
    async fn stale_rate_is_allowed_briefly_then_refused() {
        let (s, f) = svc(&[("a", 6_000_000)]);
        assert!(!s.quote(0).await.unwrap().stale);
        f[0].set(Err("down".into()));
        let q = s.quote(120_000).await.unwrap();
        assert!(q.stale);
        assert_eq!(q.cents_per_btc, 6_000_000);
        // Past max_age (900 s) there is no safe price.
        assert_eq!(
            s.quote(2_000_000).await.unwrap_err(),
            PayError::RateUnavailable
        );
        let st = s.statuses();
        assert_eq!(st[0].last_error.as_deref(), Some("down"));
        // Recovery.
        f[0].set(Ok(6_100_000));
        assert!(!s.quote(2_100_000).await.unwrap().stale);
    }

    #[tokio::test]
    async fn one_failing_source_does_not_block_and_no_source_means_unavailable() {
        let (s, f) = svc(&[("a", 6_000_000), ("b", 6_000_000)]);
        f[1].set(Err("timeout".into()));
        let q = s.quote(0).await.unwrap();
        assert_eq!(q.source, "a");
        let empty = RateService::new(vec![], RateSettings::default());
        assert_eq!(empty.quote(0).await.unwrap_err(), PayError::RateUnavailable);
        assert!(!empty.has_sources());
    }

    #[tokio::test]
    async fn implausible_or_disagreeing_answers_are_refused() {
        let (s, _) = svc(&[("a", 6_000_000), ("b", 9_000_000)]);
        assert_eq!(s.quote(0).await.unwrap_err(), PayError::RateUnavailable);
        let (s, _) = svc(&[("a", 5)]);
        assert_eq!(s.quote(0).await.unwrap_err(), PayError::RateUnavailable);
    }
}
