//! The currency converter's rates (UTL-CALC-001): the `tools.fx_refresh` job and the Postgres side of
//! `GET /tools/fx`.
//!
//! * The server, never a browser, fetches the European Central Bank daily reference rates
//!   (`ECB_URL`, EUR base, no key, a few kilobytes) through `sr_net::SafeClient`, the same SSRF-safe path as the
//!   RSS feeds: scheme, deny and allow lists of the instance's outbound policy, every resolved address and every
//!   redirect hop are checked, and the body is capped. An instance that blocks `www.ecb.europa.eu` gets a
//!   recorded failure and a converter that says its rates are out of date, nothing else.
//! * The platform setting `tools.currency_rates` (default on) switches the fetch off: the job then makes no
//!   request and the route answers `enabled: false`.
//! * An hourly singleton (id 1620) decides whether a fetch is due: never more than one per 20 hours after a
//!   success, and one per hour while failing. The feed is parsed with `quick-xml`; DTDs are refused (no entity
//!   expansion), the document and the number of events are bounded, and a set with fewer than 10 currencies is
//!   rejected as not the ECB's.
//! * The newest set replaces the old one in `fx_rates` (migration 1620); a set older than the stored one is
//!   ignored.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use quick_xml::{
    Reader,
    events::{BytesStart, Event},
};
use serde_json::json;
use sqlx::Row;
use sr_api::routes::tools::fx::{FxDeps, FxSnapshot, FxSource};
use sr_db::jobs::{self, Job};
use sr_db::runner::{HandlerError, JobRunner};
use sr_db::{DbResult, PgPool};
use sr_net::SafeClient;
use url::Url;

/// Reserved singleton id of the refresh job; unique among the `*_JOB_ID` constants.
pub const FX_JOB_ID: i64 = 1620;
pub const FX_KIND: &str = "tools.fx_refresh";
/// The ECB daily reference rates (XML).
pub const ECB_URL: &str = "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml";
const MAX_BYTES: usize = 256 * 1024;
const MAX_EVENTS: usize = 5_000;
const MIN_CURRENCIES: usize = 10;
const MAX_CURRENCIES: usize = 100;
/// How often the job looks at the clock.
const TICK: Duration = Duration::from_secs(3600);
/// Least time between two fetches after a success.
const REFRESH_AFTER_MS: i64 = 20 * 3600 * 1000;

const SETTING_ON: &str = "tools.currency_rates";
const SETTING_STALE_HOURS: &str = "tools.currency_stale_hours";
const DEFAULT_STALE_HOURS: i64 = 96;

// ------------------------------------------------------------------ the feed

/// One parsed ECB set.
#[derive(Debug, Clone, PartialEq)]
pub struct EcbSet {
    /// `YYYY-MM-DD`.
    pub as_of: String,
    /// Units per euro; the euro itself is not listed.
    pub rates: BTreeMap<String, f64>,
}

fn bad(msg: impl Into<String>) -> String {
    msg.into()
}

fn attr(e: &BytesStart<'_>, name: &str) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref().eq_ignore_ascii_case(name))
        .map(|a| a.value.as_ref().to_owned())
}

fn valid_date(d: &str) -> bool {
    let b = d.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let num = |r: std::ops::Range<usize>| d[r].parse::<u32>().ok();
    match (num(0..4), num(5..7), num(8..10)) {
        (Some(y), Some(m), Some(day)) => {
            (2000..=2200).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&day)
        }
        _ => false,
    }
}

/// Parse the ECB `eurofxref-daily.xml` document.
pub fn parse_ecb(bytes: &[u8]) -> Result<EcbSet, String> {
    if bytes.len() > MAX_BYTES {
        return Err(bad("document is too large"));
    }
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let src = std::str::from_utf8(bytes).map_err(|_| bad("not UTF-8"))?;
    let mut reader = Reader::from_str(src);
    reader.config_mut().check_end_names = true;
    let mut as_of: Option<String> = None;
    let mut rates = BTreeMap::new();
    let mut events = 0usize;
    loop {
        events += 1;
        if events > MAX_EVENTS {
            return Err(bad("too many elements"));
        }
        match reader.read_event().map_err(|e| bad(e.to_string()))? {
            Event::DocType(_) => return Err(bad("DTDs are not allowed")),
            Event::Start(e) | Event::Empty(e) => {
                if e.name().as_ref() != "Cube" {
                    continue;
                }
                if let Some(t) = attr(&e, "time") {
                    if !valid_date(&t) {
                        return Err(bad("bad reference date"));
                    }
                    if as_of.as_ref().is_some_and(|d| *d != t) {
                        return Err(bad("more than one reference date"));
                    }
                    as_of = Some(t);
                }
                if let (Some(c), Some(r)) = (attr(&e, "currency"), attr(&e, "rate")) {
                    let ok = c.len() == 3 && c.bytes().all(|b| b.is_ascii_uppercase());
                    if !ok {
                        return Err(bad("bad currency code"));
                    }
                    let rate: f64 = r.trim().parse().map_err(|_| bad("bad rate"))?;
                    if !rate.is_finite() || rate <= 0.0 || rate >= 1e9 {
                        return Err(bad("rate out of range"));
                    }
                    if c != "EUR" && rates.insert(c, rate).is_some() {
                        return Err(bad("duplicate currency"));
                    }
                    if rates.len() > MAX_CURRENCIES {
                        return Err(bad("too many currencies"));
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let as_of = as_of.ok_or_else(|| bad("no reference date"))?;
    if rates.len() < MIN_CURRENCIES {
        return Err(bad("too few rates"));
    }
    Ok(EcbSet { as_of, rates })
}

// ------------------------------------------------------------------ fetching

#[async_trait]
pub trait FxFetch: Send + Sync {
    async fn fetch(&self) -> Result<Vec<u8>, String>;
}

/// The production fetcher: the SSRF-safe client on the ECB URL.
pub struct EcbFetcher {
    client: Arc<SafeClient>,
    url: Url,
}

impl EcbFetcher {
    pub fn new(client: Arc<SafeClient>) -> Self {
        Self {
            client,
            url: Url::parse(ECB_URL).expect("the ECB URL is valid"),
        }
    }
}

#[async_trait]
impl FxFetch for EcbFetcher {
    async fn fetch(&self) -> Result<Vec<u8>, String> {
        self.client
            .get(&self.url, "application/xml, text/xml;q=0.9", MAX_BYTES)
            .await
            .map(|r| r.body)
            .map_err(|e| e.to_string())
    }
}

// ------------------------------------------------------------------ storage

/// `tools.currency_rates`; a missing row means the default, on.
pub async fn enabled(pool: &PgPool) -> DbResult<bool> {
    let v: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT value FROM platform_settings WHERE key = $1")
            .bind(SETTING_ON)
            .fetch_optional(pool)
            .await?;
    Ok(match v {
        None => true,
        Some(v) => v
            .as_bool()
            .or_else(|| v.as_i64().map(|n| n != 0))
            .unwrap_or(true),
    })
}

/// `tools.currency_stale_hours`.
pub async fn stale_hours(pool: &PgPool) -> DbResult<i64> {
    let v: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT value FROM platform_settings WHERE key = $1")
            .bind(SETTING_STALE_HOURS)
            .fetch_optional(pool)
            .await?;
    Ok(v.and_then(|v| v.as_i64()).unwrap_or(DEFAULT_STALE_HOURS))
}

/// Unix milliseconds of the last successful fetch.
pub async fn last_success_ms(pool: &PgPool) -> DbResult<Option<i64>> {
    let v: Option<f64> = sqlx::query_scalar(
        "SELECT (EXTRACT(EPOCH FROM last_success_at) * 1000)::float8 FROM fx_fetch_state WHERE id = 1",
    )
    .fetch_one(pool)
    .await?;
    Ok(v.map(|ms| ms as i64))
}

/// Store a set. A set dated before the one already stored is ignored (returns `false`).
pub async fn save(pool: &PgPool, set: &EcbSet) -> DbResult<bool> {
    let mut tx = pool.begin().await?;
    let newer: bool =
        sqlx::query_scalar("SELECT NOT EXISTS (SELECT 1 FROM fx_rates WHERE as_of > $1::date)")
            .bind(&set.as_of)
            .fetch_one(&mut *tx)
            .await?;
    if newer {
        let codes: Vec<String> = set.rates.keys().cloned().collect();
        let values: Vec<f64> = set.rates.values().copied().collect();
        sqlx::query(
            "INSERT INTO fx_rates (currency, rate, as_of, fetched_at) \
             SELECT c, r, $3::date, now() FROM unnest($1::text[], $2::float8[]) AS t(c, r) \
             ON CONFLICT (currency) DO UPDATE SET \
                 rate = EXCLUDED.rate, as_of = EXCLUDED.as_of, fetched_at = EXCLUDED.fetched_at",
        )
        .bind(&codes)
        .bind(&values)
        .bind(&set.as_of)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM fx_rates WHERE currency <> ALL($1::text[])")
            .bind(&codes)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(
        "UPDATE fx_fetch_state SET last_attempt_at = now(), last_success_at = now(), last_error = '', \
             failures = 0 WHERE id = 1",
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(newer)
}

/// Remember a failed attempt (the message is clipped; nothing secret is in it).
pub async fn record_failure(pool: &PgPool, message: &str) -> DbResult<()> {
    let msg: String = message.chars().take(500).collect();
    sqlx::query(
        "UPDATE fx_fetch_state SET last_attempt_at = now(), last_error = $1, failures = failures + 1 \
         WHERE id = 1",
    )
    .bind(msg)
    .execute(pool)
    .await?;
    Ok(())
}

/// What `GET /tools/fx` serves.
pub async fn snapshot(pool: &PgPool) -> DbResult<FxSnapshot> {
    let on = enabled(pool).await?;
    let stale_after_hours = stale_hours(pool).await?;
    if !on {
        return Ok(FxSnapshot {
            enabled: false,
            stale_after_hours,
            ..FxSnapshot::default()
        });
    }
    let rows = sqlx::query(
        "SELECT currency, rate, as_of::text AS as_of, EXTRACT(EPOCH FROM fetched_at)::bigint AS fetched \
         FROM fx_rates ORDER BY currency",
    )
    .fetch_all(pool)
    .await?;
    let mut rates = BTreeMap::new();
    let (mut as_of, mut fetched_at): (Option<String>, Option<i64>) = (None, None);
    for r in rows {
        rates.insert(r.get::<String, _>("currency"), r.get::<f64, _>("rate"));
        let d: String = r.get("as_of");
        let f: i64 = r.get("fetched");
        if as_of.as_ref().is_none_or(|x| *x < d) {
            as_of = Some(d);
        }
        if fetched_at.is_none_or(|x| x < f) {
            fetched_at = Some(f);
        }
    }
    Ok(FxSnapshot {
        enabled: true,
        as_of,
        fetched_at,
        stale_after_hours,
        rates,
    })
}

/// The route's source of rates.
pub struct PgFxSource(pub PgPool);

#[async_trait]
impl FxSource for PgFxSource {
    async fn snapshot(&self) -> Result<FxSnapshot, String> {
        snapshot(&self.0).await.map_err(|e| e.to_string())
    }
}

/// The extension the route reads.
pub fn deps(pool: &PgPool) -> Arc<FxDeps> {
    Arc::new(FxDeps {
        source: Arc::new(PgFxSource(pool.clone())),
    })
}

// ------------------------------------------------------------------ the job

/// Whether a fetch is due: never fetched, or the last success is old enough.
pub fn is_due(last_success_ms: Option<i64>, now_ms: i64) -> bool {
    last_success_ms.is_none_or(|at| now_ms.saturating_sub(at) >= REFRESH_AFTER_MS)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// `tools.currency_rates` is off: nothing was requested.
    Disabled,
    /// A fetch succeeded recently.
    NotDue,
    /// The set was stored (or ignored because a newer one is stored).
    Stored {
        currencies: usize,
        as_of: String,
        replaced: bool,
    },
    /// The attempt failed; the message is also recorded.
    Failed(String),
}

/// One tick of the job.
pub async fn run_once(pool: &PgPool, fetcher: &dyn FxFetch, now_ms: i64) -> DbResult<Outcome> {
    if !enabled(pool).await? {
        return Ok(Outcome::Disabled);
    }
    if !is_due(last_success_ms(pool).await?, now_ms) {
        return Ok(Outcome::NotDue);
    }
    let parsed = match fetcher.fetch().await {
        Ok(body) => parse_ecb(&body),
        Err(e) => Err(e),
    };
    match parsed {
        Ok(set) => {
            let replaced = save(pool, &set).await?;
            Ok(Outcome::Stored {
                currencies: set.rates.len(),
                as_of: set.as_of,
                replaced,
            })
        }
        Err(e) => {
            record_failure(pool, &e).await?;
            Ok(Outcome::Failed(e))
        }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Register the handler on `runner`. Pair with [`schedule`].
pub fn register(runner: JobRunner, pool: &PgPool, fetcher: Arc<dyn FxFetch>) -> JobRunner {
    let pool = pool.clone();
    runner.register(FX_KIND, 1, move |job: Job| {
        let pool = pool.clone();
        let fetcher = fetcher.clone();
        Box::pin(async move {
            match run_once(&pool, fetcher.as_ref(), now_ms()).await {
                Ok(Outcome::Stored {
                    currencies, as_of, ..
                }) => {
                    tracing::info!(currencies, as_of, "currency rates refreshed");
                }
                Ok(Outcome::Failed(e)) => {
                    tracing::warn!(error = %e, "currency rates refresh failed")
                }
                Ok(_) => {}
                // A database hiccup is retried at the next tick, not dead-lettered.
                Err(e) => tracing::warn!(error = %e, "currency rates job failed"),
            }
            jobs::reschedule_running(&pool, job.id, TICK)
                .await
                .map_err(HandlerError::from)?;
            Ok(())
        })
    })
}

/// Create or re-arm the singleton. Call only when [`register`] is on the runner.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(
        pool,
        FX_JOB_ID,
        FX_KIND,
        &json!({}),
        Duration::from_secs(30),
    )
    .await?;
    Ok(())
}

/// Currency codes in a set, for the logs and tests.
pub fn codes(set: &EcbSet) -> BTreeSet<&str> {
    set.rates.keys().map(String::as_str).collect()
}

/// The shape of the real document, with a fixed handful of currencies (no network in tests).
#[cfg(test)]
pub(crate) fn fixture_for_tests(date: &str, usd: &str) -> String {
    let mut cubes = String::new();
    for (c, r) in [
        ("USD", usd),
        ("JPY", "163.12"),
        ("BGN", "1.9558"),
        ("CZK", "24.351"),
        ("DKK", "7.4602"),
        ("GBP", "0.83390"),
        ("HUF", "395.20"),
        ("PLN", "4.2790"),
        ("RON", "4.9770"),
        ("SEK", "11.5030"),
        ("CHF", "0.9410"),
        ("NOK", "11.7620"),
        ("CAD", "1.5120"),
        ("AUD", "1.6420"),
    ] {
        cubes.push_str(&format!("<Cube currency='{c}' rate='{r}'/>"));
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <gesmes:Envelope xmlns:gesmes=\"http://www.gesmes.org/xml/2002-08-01\" \
         xmlns=\"http://www.ecb.int/vocabulary/2002-08-01/eurofxref\">\
         <gesmes:subject>Reference rates</gesmes:subject>\
         <gesmes:Sender><gesmes:name>European Central Bank</gesmes:name></gesmes:Sender>\
         <Cube><Cube time='{date}'>{cubes}</Cube></Cube></gesmes:Envelope>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(date: &str, usd: &str) -> String {
        fixture_for_tests(date, usd)
    }

    #[test]
    fn parses_the_ecb_document() {
        let set = parse_ecb(fixture("2026-10-29", "1.0845").as_bytes()).unwrap();
        assert_eq!(set.as_of, "2026-10-29");
        assert_eq!(set.rates.len(), 14);
        assert_eq!(set.rates["USD"], 1.0845);
        assert_eq!(set.rates["GBP"], 0.8339);
        assert!(!set.rates.contains_key("EUR"));
        assert!(codes(&set).contains("JPY"));
    }

    #[test]
    fn refuses_hostile_or_wrong_documents() {
        let ok = fixture("2026-10-29", "1.08");
        let cases: Vec<(String, &str)> = vec![
            (
                format!("<!DOCTYPE x [<!ENTITY a \"b\">]>{ok}"),
                "DTDs are not allowed",
            ),
            (ok.replace("2026-10-29", "29/10/2026"), "bad reference date"),
            (ok.replace("rate='1.08'", "rate='abc'"), "bad rate"),
            (ok.replace("rate='1.08'", "rate='-1'"), "rate out of range"),
            (ok.replace("rate='1.08'", "rate='0'"), "rate out of range"),
            (
                ok.replace("rate='1.08'", "rate='1e999'"),
                "rate out of range",
            ),
            (
                ok.replace("currency='USD'", "currency='usd'"),
                "bad currency code",
            ),
            (
                ok.replace("currency='JPY'", "currency='USD'"),
                "duplicate currency",
            ),
            ("<Cube/>".to_owned(), "no reference date"),
            (
                "<Cube><Cube time='2026-10-29'><Cube currency='USD' rate='1'/></Cube></Cube>"
                    .to_owned(),
                "too few rates",
            ),
            ("not xml at all <".to_owned(), ""),
            ("x".repeat(MAX_BYTES + 1), "document is too large"),
        ];
        for (doc, want) in cases {
            let got = parse_ecb(doc.as_bytes());
            let e = got.expect_err(&doc[..doc.len().min(60)]);
            assert!(e.contains(want), "{e:?} should mention {want:?}");
        }
        // Two dates in one document are not the daily file.
        let two = ok.replace(
            "</Cube></Cube>",
            "</Cube><Cube time='2026-10-28'></Cube></Cube>",
        );
        assert!(parse_ecb(two.as_bytes()).is_err());
        // Binary rubbish and a BOM.
        assert!(parse_ecb(&[0xff, 0xfe, 0x00]).is_err());
        let with_bom = [b"\xEF\xBB\xBF".as_slice(), ok.as_bytes()].concat();
        assert!(parse_ecb(&with_bom).is_ok());
    }

    #[test]
    fn a_fetch_is_due_once_the_last_success_is_old_enough() {
        let now = 100 * 3600 * 1000;
        assert!(is_due(None, now));
        assert!(!is_due(Some(now - 3600 * 1000), now));
        assert!(!is_due(Some(now - REFRESH_AFTER_MS + 1), now));
        assert!(is_due(Some(now - REFRESH_AFTER_MS), now));
        // A clock that ran backwards does not trigger a fetch storm.
        assert!(!is_due(Some(now + 5), now));
    }

    #[test]
    fn job_constants_are_sane() {
        assert!(ECB_URL.starts_with("https://www.ecb.europa.eu/"));
        assert!(Url::parse(ECB_URL).is_ok());
        assert_eq!(FX_JOB_ID, 1620);
    }

    // ---- the fetcher respects the instance's outbound policy (no network: stub resolver and transport)

    use sr_net::{HopRequest, HopResponse, HttpTransport, NetError, Policy, Resolver};
    use std::net::IpAddr;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FixedDns(&'static str);

    #[async_trait]
    impl Resolver for FixedDns {
        async fn resolve(&self, _host: &str, _port: u16) -> Result<Vec<IpAddr>, String> {
            Ok(vec![self.0.parse().unwrap()])
        }
    }

    struct Serve {
        status: u16,
        body: Vec<u8>,
        calls: AtomicUsize,
        last: std::sync::Mutex<Option<String>>,
    }

    impl Serve {
        fn new(status: u16, body: Vec<u8>) -> Arc<Self> {
            Arc::new(Self {
                status,
                body,
                calls: AtomicUsize::new(0),
                last: std::sync::Mutex::new(None),
            })
        }
    }

    #[async_trait]
    impl HttpTransport for Serve {
        async fn get(&self, req: &HopRequest) -> Result<HopResponse, NetError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            *self.last.lock().unwrap() = Some(req.url.to_string());
            Ok(HopResponse {
                status: self.status,
                content_type: Some("text/xml".into()),
                location: None,
                body: self.body.clone(),
            })
        }
    }

    fn fetcher(policy: Policy, dns: &'static str, t: Arc<Serve>) -> EcbFetcher {
        EcbFetcher::new(Arc::new(SafeClient::new(
            policy,
            Arc::new(FixedDns(dns)),
            t,
        )))
    }

    #[tokio::test]
    async fn the_fetcher_gets_the_ecb_url_through_the_safe_client() {
        let t = Serve::new(200, fixture("2026-10-29", "1.08").into_bytes());
        let f = fetcher(Policy::default(), "193.0.0.1", t.clone());
        let body = f.fetch().await.unwrap();
        assert!(parse_ecb(&body).is_ok());
        assert_eq!(t.calls.load(Ordering::SeqCst), 1);
        assert_eq!(t.last.lock().unwrap().as_deref(), Some(ECB_URL));
    }

    #[tokio::test]
    async fn a_deny_list_entry_stops_the_request_before_it_is_made() {
        for deny in ["www.ecb.europa.eu", "ecb.europa.eu", "europa.eu"] {
            let t = Serve::new(200, fixture("2026-10-29", "1.08").into_bytes());
            let policy = Policy {
                deny_hosts: vec![deny.to_owned()],
                ..Policy::default()
            };
            let err = fetcher(policy, "193.0.0.1", t.clone())
                .fetch()
                .await
                .unwrap_err();
            assert!(!err.is_empty());
            assert_eq!(t.calls.load(Ordering::SeqCst), 0, "{deny}: {err}");
        }
    }

    #[tokio::test]
    async fn an_allow_list_that_omits_the_ecb_stops_it_too() {
        let t = Serve::new(200, fixture("2026-10-29", "1.08").into_bytes());
        let policy = Policy {
            allow_hosts: vec!["example.org".to_owned()],
            ..Policy::default()
        };
        assert!(
            fetcher(policy, "193.0.0.1", t.clone())
                .fetch()
                .await
                .is_err()
        );
        assert_eq!(t.calls.load(Ordering::SeqCst), 0);
        // Listing the host (or its parent domain) lets it through.
        for allow in ["www.ecb.europa.eu", "europa.eu"] {
            let t = Serve::new(200, fixture("2026-10-29", "1.08").into_bytes());
            let policy = Policy {
                allow_hosts: vec![allow.to_owned()],
                ..Policy::default()
            };
            assert!(
                fetcher(policy, "193.0.0.1", t.clone())
                    .fetch()
                    .await
                    .is_ok(),
                "{allow}"
            );
        }
    }

    #[tokio::test]
    async fn a_name_that_resolves_to_a_private_address_is_refused() {
        for ip in [
            "10.0.0.5",
            "127.0.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "::1",
        ] {
            let t = Serve::new(200, fixture("2026-10-29", "1.08").into_bytes());
            let err = fetcher(Policy::default(), ip, t.clone())
                .fetch()
                .await
                .unwrap_err();
            assert!(!err.is_empty());
            assert_eq!(t.calls.load(Ordering::SeqCst), 0, "{ip}");
        }
    }

    #[tokio::test]
    async fn an_error_status_is_a_failure_not_a_document() {
        let t = Serve::new(503, b"busy".to_vec());
        let err = fetcher(Policy::default(), "193.0.0.1", t)
            .fetch()
            .await
            .unwrap_err();
        assert!(err.contains("503"), "{err}");
    }
}
