//! sr-loadtest: the load test suite (XC-PERF-003). Drives a running `sr-server` the way the product is used:
//! thousands of gateway sessions held open, a steady stream of chat messages that fans out to them, and
//! REST reads and writes on the side, then checks the run against the targets of doc 12 (2,000 concurrent
//! gateway sessions, 200 messages per second, REST p95 under 100 ms).
//!
//! ```text
//! # a throwaway database, migrated, with a server on it (rate limits are per user, so one box is fine)
//! sr-loadtest --base-url http://127.0.0.1:8080 --database-url postgres://.../sr_loadtest
//! sr-loadtest --sessions 100 --msg-rate 20 --duration 15      # a quick smoke run
//! ```
//!
//! It seeds its own accounts, sessions and guilds straight into the database, so it refuses a database whose
//! name does not look disposable (`--allow-any-database` overrides). Exit code 0 when every target is met.

mod gateway;
mod http;
mod seed;
mod stats;

use anyhow::{Context, Result, bail};
use clap::Parser;
use gateway::{Conn, Ready};
use http::Client;
use seed::{Participant, Seeded};
use stats::{Measured, Samples, Targets};
use std::{
    process::ExitCode,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{sync::watch, time::sleep};

#[derive(Debug, Parser)]
#[command(name = "sr-loadtest", version, about)]
struct Args {
    /// The server under test (plain http://).
    #[arg(
        long,
        env = "SR_LOADTEST_BASE_URL",
        default_value = "http://127.0.0.1:8080"
    )]
    base_url: String,
    /// Database of that server, to seed accounts and sessions into.
    #[arg(long, env = "SR_DATABASE__URL", hide_env_values = true)]
    database_url: String,
    /// `Origin` for the gateway handshake; defaults to the base URL.
    #[arg(long)]
    origin: Option<String>,
    /// Concurrent gateway sessions (one per account).
    #[arg(long, default_value_t = 2000)]
    sessions: usize,
    /// Guilds the accounts are spread over; a message reaches the sessions of one guild.
    #[arg(long, default_value_t = 100)]
    guilds: usize,
    /// Chat messages posted per second across all accounts.
    #[arg(long, default_value_t = 200.0)]
    msg_rate: f64,
    /// Other REST requests per second (profile and message history reads).
    #[arg(long, default_value_t = 300.0)]
    rest_rps: f64,
    /// Steady-state length in seconds, after every session is up.
    #[arg(long, default_value_t = 60)]
    duration: u64,
    /// Gateway sessions opened per second while ramping up.
    #[arg(long, default_value_t = 100.0)]
    ramp_rate: f64,
    /// Concurrent REST workers.
    #[arg(long, default_value_t = 32)]
    rest_workers: usize,
    /// Marks the seeded accounts (`lt<run-id>u<n>`); change it to run again on the same database.
    #[arg(long, default_value = "a")]
    run_id: String,
    /// Snowflake worker id for the seeded rows (0 to 31); keep it away from the server's.
    #[arg(long, default_value_t = 29)]
    worker_id: u8,
    /// Seed into a database whose name does not look disposable.
    #[arg(long)]
    allow_any_database: bool,
    /// Write the result as JSON to this file.
    #[arg(long)]
    report: Option<std::path::PathBuf>,
    /// REST p95 limit in milliseconds.
    #[arg(long, default_value_t = 100.0)]
    p95_ms: f64,
    /// Fan-out (POST to dispatch) p95 limit in milliseconds.
    #[arg(long, default_value_t = 250.0)]
    fanout_p95_ms: f64,
    /// Servers the first-paint probe account is a member of (K-22: 60); 0 turns the probe off.
    #[arg(long, default_value_t = 60)]
    first_paint_guilds: usize,
    /// First paint limit in milliseconds for that account.
    #[arg(long, default_value_t = 1500.0)]
    first_paint_ms: f64,
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = Args::parse();
    match run(args).await {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn ws_url(base: &str) -> Result<String> {
    let rest = base
        .strip_prefix("http://")
        .context("--base-url must start with http://")?;
    Ok(format!(
        "ws://{}/gateway?v=10&encoding=json",
        rest.trim_end_matches('/')
    ))
}

async fn run(args: Args) -> Result<bool> {
    if args.msg_rate <= 0.0 || args.rest_rps < 0.0 || args.ramp_rate <= 0.0 {
        bail!("--msg-rate and --ramp-rate must be positive, --rest-rps not negative");
    }
    if args.sessions == 0 || args.guilds == 0 || args.guilds > args.sessions {
        bail!("need --sessions >= --guilds >= 1");
    }
    let db_name =
        seed::database_of(&args.database_url).context("--database-url names no database")?;
    if !args.allow_any_database && !seed::is_disposable(db_name) {
        bail!(
            "database {db_name:?} does not look disposable (its name should contain load, test, scratch, smoke, e2e, dev or tmp); \
             this tool seeds accounts and sessions into it. Pass --allow-any-database if you mean it."
        );
    }
    let client = Arc::new(Client::new(&args.base_url)?);
    let probe = client
        .request("GET", "/healthz", &[], None)
        .await
        .with_context(|| format!("{} is not answering; start sr-server first", args.base_url))?;
    if probe.status != 200 {
        bail!("GET /healthz answered {}", probe.status);
    }

    eprintln!(
        "seeding {} accounts over {} guilds into {db_name} ...",
        args.sessions, args.guilds
    );
    let pool = sr_db::connect(&sr_db::DbConfig {
        max_connections: 4,
        ..sr_db::DbConfig::new(&args.database_url)
    })
    .await
    .context("connecting to the database")?;
    let t0 = Instant::now();
    let seeded = Arc::new(
        seed::seed(
            &pool,
            args.sessions,
            args.guilds,
            if args.guilds >= args.first_paint_guilds {
                args.first_paint_guilds
            } else {
                0
            },
            &args.run_id,
            args.worker_id,
        )
        .await?,
    );
    eprintln!("seeded in {:.1}s", t0.elapsed().as_secs_f64());

    let targets = Targets {
        max_rest_p95_ms: args.p95_ms,
        max_fanout_p95_ms: args.fanout_p95_ms,
        max_first_paint_ms: args.first_paint_ms,
        ..Targets::default()
    };
    let measured = scenario(&args, client, seeded).await?;
    let failures = measured.failures(&targets);
    print_report(&args, &measured, &failures);
    if let Some(path) = &args.report {
        std::fs::write(path, report_json(&args, &measured, &failures))
            .with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(failures.is_empty())
}

/// After the steady state, how long without a new delivery ends the wait for stragglers.
const DRAIN_QUIET: Duration = Duration::from_secs(3);
/// The longest the run waits for stragglers after the steady state.
const DRAIN_MAX: Duration = Duration::from_secs(30);

/// A small xorshift generator: the workers only need cheap, uncorrelated choices.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n.max(1) as u64).unwrap_or(0)
    }
}

/// Remember the first failed request (status and the start of its body) for the report.
fn note_failure(slot: &Mutex<Option<String>>, what: &str, r: Result<http::Response>) {
    let mut g = slot.lock().unwrap_or_else(|p| p.into_inner());
    if g.is_none() {
        *g = Some(match r {
            Ok(resp) => format!(
                "{what} -> {} {}",
                resp.status,
                String::from_utf8_lossy(&resp.body[..resp.body.len().min(200)])
            ),
            Err(e) => format!("{what} -> {e:#}"),
        });
    }
}

fn headers<'a>(p: &'a Participant, cookie: &'a str) -> [(&'a str, &'a str); 3] {
    [
        ("Cookie", cookie),
        ("X-SR-CSRF", p.csrf.as_str()),
        ("Content-Type", "application/json"),
    ]
}

async fn scenario(args: &Args, client: Arc<Client>, seeded: Arc<Seeded>) -> Result<Measured> {
    let conn = Conn {
        url: ws_url(&args.base_url)?,
        origin: args.origin.clone().unwrap_or_else(|| args.base_url.clone()),
    };
    let ready = Arc::new(Ready::new(seeded.guilds.len()));
    let (stop_tx, stop_rx) = watch::channel(false);

    // ---- gateway sessions, ramped up -------------------------------------------------------------
    eprintln!(
        "opening {} gateway sessions at {}/s ...",
        args.sessions, args.ramp_rate
    );
    let mut sessions = Vec::with_capacity(args.sessions);
    let pace = Duration::from_secs_f64(1.0 / args.ramp_rate);
    let ramp_start = Instant::now();
    for (i, p) in seeded.people.iter().enumerate() {
        let (conn, token, guild) = (conn.clone(), p.token.clone(), p.guild);
        let (ready, rx) = (ready.clone(), stop_rx.clone());
        sessions.push(tokio::spawn(async move {
            gateway::run(&conn, &token, guild, &ready, rx).await
        }));
        let due = ramp_start + pace * u32::try_from(i + 1).unwrap_or(u32::MAX);
        sleep(due.saturating_duration_since(Instant::now())).await;
    }
    // Give the last ones time to finish their handshake.
    let settle_until = Instant::now() + Duration::from_secs(30);
    while ready.total.load(Ordering::Relaxed) < args.sessions && Instant::now() < settle_until {
        sleep(Duration::from_millis(100)).await;
    }
    let up = ready.total.load(Ordering::Relaxed);
    eprintln!(
        "{up}/{} sessions ready after {:.1}s",
        args.sessions,
        ramp_start.elapsed().as_secs_f64()
    );

    // ---- steady state: messages and REST traffic ----------------------------------------------------
    let running = Arc::new(AtomicBool::new(true));
    let sent_ok = Arc::new(AtomicU64::new(0));
    let sent_per_guild: Arc<Vec<AtomicU64>> = Arc::new(
        (0..seeded.guilds.len())
            .map(|_| AtomicU64::new(0))
            .collect(),
    );
    let rest = Arc::new(Mutex::new(Samples::default()));
    let in_flight = Arc::new(tokio::sync::Semaphore::new(4096));
    let first_failure: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let steady = Instant::now();

    let sender = {
        let (client, seeded, running) = (client.clone(), seeded.clone(), running.clone());
        let (sent_ok, per_guild, rest) = (sent_ok.clone(), sent_per_guild.clone(), rest.clone());
        let (in_flight, first_failure) = (in_flight.clone(), first_failure.clone());
        let interval = Duration::from_secs_f64(1.0 / args.msg_rate);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(interval);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);
            let mut seq = 0u64;
            while running.load(Ordering::Relaxed) {
                tick.tick().await;
                let Ok(permit) = in_flight.clone().try_acquire_owned() else {
                    rest.lock().unwrap_or_else(|p| p.into_inner()).error();
                    continue;
                };
                let p =
                    seeded.people[usize::try_from(seq).unwrap_or(0) % seeded.people.len()].clone();
                let ch = seeded.guilds[p.guild].channel;
                seq += 1;
                let (client, rest, sent_ok, per_guild, first_failure) = (
                    client.clone(),
                    rest.clone(),
                    sent_ok.clone(),
                    per_guild.clone(),
                    first_failure.clone(),
                );
                tokio::spawn(async move {
                    let cookie = format!("sr_session={}", p.token);
                    let body =
                        serde_json::json!({ "content": gateway::marker(gateway::now_us(), seq) })
                            .to_string();
                    let t = Instant::now();
                    let r = client
                        .request(
                            "POST",
                            &format!("/api/v1/channels/{ch}/messages"),
                            &headers(&p, &cookie),
                            Some(body.as_bytes()),
                        )
                        .await;
                    let mut s = rest.lock().unwrap_or_else(|p| p.into_inner());
                    match r {
                        Ok(resp) if resp.status < 300 => {
                            s.record(t.elapsed());
                            sent_ok.fetch_add(1, Ordering::Relaxed);
                            per_guild[p.guild].fetch_add(1, Ordering::Relaxed);
                        }
                        other => {
                            s.error();
                            note_failure(&first_failure, "POST message", other);
                        }
                    }
                    drop(permit);
                });
            }
        })
    };

    let workers = args.rest_workers.max(1);
    let per_worker = if args.rest_rps > 0.0 {
        Some(Duration::from_secs_f64(workers as f64 / args.rest_rps))
    } else {
        None
    };
    let mut readers = Vec::new();
    if let Some(pause) = per_worker {
        for w in 0..workers {
            let (client, seeded, running, rest, first_failure) = (
                client.clone(),
                seeded.clone(),
                running.clone(),
                rest.clone(),
                first_failure.clone(),
            );
            readers.push(tokio::spawn(async move {
                let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ ((w as u64 + 1) * 0x1000_0001));
                let mut local = Samples::default();
                while running.load(Ordering::Relaxed) {
                    let p = &seeded.people[rng.below(seeded.people.len())];
                    let ch = seeded.guilds[p.guild].channel;
                    let cookie = format!("sr_session={}", p.token);
                    let path = match rng.below(3) {
                        0 => "/api/v1/users/@me".to_owned(),
                        1 => format!("/api/v1/channels/{ch}/messages?limit=50"),
                        _ => "/api/v1/users/@me/guilds".to_owned(),
                    };
                    let t = Instant::now();
                    match client
                        .request("GET", &path, &headers(p, &cookie), None)
                        .await
                    {
                        Ok(r) if r.status < 300 => local.record(t.elapsed()),
                        other => {
                            local.error();
                            note_failure(&first_failure, &path, other);
                        }
                    }
                    sleep(pause).await;
                }
                rest.lock().unwrap_or_else(|p| p.into_inner()).merge(local);
            }));
        }
    }

    eprintln!(
        "steady state for {}s ({} msg/s, {} REST/s) ...",
        args.duration, args.msg_rate, args.rest_rps
    );
    // The first-paint probe loads the Lounge once, halfway through the steady state, under the full load.
    sleep(Duration::from_secs(args.duration / 2)).await;
    let first_paint = match &seeded.probe {
        Some((p, guilds)) => Some(first_paint(&conn, &client, p, *guilds).await),
        None => None,
    };
    sleep(Duration::from_secs(args.duration - args.duration / 2)).await;
    // The rate is what completed inside the window; stragglers still count for delivery below.
    let steady_secs = steady.elapsed().as_secs_f64();
    let sent_in_window = sent_ok.load(Ordering::Relaxed);
    running.store(false, Ordering::Relaxed);
    let _ = sender.await;
    for r in readers {
        let _ = r.await;
    }
    // Let the last dispatches arrive before the sessions are closed: until every expected one is in, or none
    // arrived for DRAIN_QUIET, at most DRAIN_MAX. A fixed 3 s here counted the backlog of a slow run as lost
    // (run 40226523570: fan-out p50 19 s, "76% delivered"); late ones still count against the fan-out p95.
    let expected_now = || -> u64 {
        sent_per_guild
            .iter()
            .enumerate()
            .map(|(g, n)| {
                n.load(Ordering::Relaxed) * ready.per_guild[g].load(Ordering::Relaxed) as u64
            })
            .sum()
    };
    let drain_started = Instant::now();
    let (mut last_seen, mut last_change) =
        (ready.delivered.load(Ordering::Relaxed), Instant::now());
    while ready.delivered.load(Ordering::Relaxed) < expected_now()
        && last_change.elapsed() < DRAIN_QUIET
        && drain_started.elapsed() < DRAIN_MAX
    {
        sleep(Duration::from_millis(100)).await;
        let now = ready.delivered.load(Ordering::Relaxed);
        if now != last_seen {
            (last_seen, last_change) = (now, Instant::now());
        }
    }
    eprintln!(
        "drained for {:.1}s after the steady state",
        drain_started.elapsed().as_secs_f64()
    );
    let _ = stop_tx.send(true);

    // ---- collect --------------------------------------------------------------------------------------
    let mut fanout = Samples::default();
    let mut received = 0u64;
    let mut ready_count = 0usize;
    let mut first_error: Option<String> = None;
    for s in sessions {
        let Ok(rep) = s.await else { continue };
        received += rep.received;
        fanout.merge(rep.fanout);
        if rep.ready {
            ready_count += 1;
        } else if first_error.is_none() {
            first_error = rep.error;
        }
    }
    if let Some(e) = &first_error {
        eprintln!("first failed session: {e}");
    }
    if let Some(f) = first_failure
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_ref()
    {
        eprintln!("first failed request: {f}");
    }
    let expected = expected_now();
    let rest = Arc::try_unwrap(rest).map_or_else(
        |a| a.lock().unwrap_or_else(|p| p.into_inner()).clone(),
        |m| m.into_inner().unwrap_or_else(|p| p.into_inner()),
    );
    Ok(Measured {
        sessions_requested: args.sessions,
        sessions_ready: ready_count,
        rest,
        send_rate_target: args.msg_rate,
        send_rate_achieved: sent_in_window as f64 / steady_secs,
        deliveries_expected: expected,
        deliveries_received: received,
        fanout,
        first_paint,
    })
}

/// What opening the Lounge costs a member of `guilds` servers: the gateway READY and the REST calls of the shell
/// (own user, server list, all server settings in one request) at the same time. Failures are counted, not raised.
async fn first_paint(
    conn: &Conn,
    client: &Arc<Client>,
    p: &Participant,
    guilds: usize,
) -> stats::FirstPaint {
    let started = Instant::now();
    let ready = Arc::new(Ready::new(1));
    let (stop_tx, stop_rx) = watch::channel(false);
    let session = {
        let (conn, token, ready) = (conn.clone(), p.token.clone(), ready.clone());
        tokio::spawn(async move { gateway::run(&conn, &token, 0, &ready, stop_rx).await })
    };
    let cookie = format!("sr_session={}", p.token);
    let paths = [
        "/api/v1/users/@me",
        "/api/v1/users/@me/guilds",
        "/api/v1/users/@me/guild-settings",
    ];
    let calls = paths.map(|path| {
        let (client, cookie, csrf) = (client.clone(), cookie.clone(), p.csrf.clone());
        async move {
            let h = [("Cookie", cookie.as_str()), ("X-SR-CSRF", csrf.as_str())];
            client.request("GET", path, &h, None).await
        }
    });
    let answers = futures::future::join_all(calls).await;
    let mut out = stats::FirstPaint {
        guilds,
        requests: paths.len(),
        ..Default::default()
    };
    for a in &answers {
        match a {
            Ok(r) if r.status == 429 => out.rate_limited += 1,
            Ok(r) if r.status < 300 => {}
            _ => out.failed += 1,
        }
    }
    // READY: wait for the session the probe opened (up to 10 s).
    let deadline = Instant::now() + Duration::from_secs(10);
    while ready.total.load(Ordering::Relaxed) == 0 && Instant::now() < deadline {
        sleep(Duration::from_millis(5)).await;
    }
    out.ready_missing = ready.total.load(Ordering::Relaxed) == 0;
    out.millis = started.elapsed().as_secs_f64() * 1000.0;
    let _ = stop_tx.send(true);
    if let Ok(rep) = session.await {
        out.ready_bytes = rep.ready_bytes;
    }
    out
}

fn print_report(args: &Args, m: &Measured, failures: &[String]) {
    println!("sr-loadtest against {}", args.base_url);
    println!(
        "  gateway   {}/{} sessions ready ({:.1}%)",
        m.sessions_ready,
        m.sessions_requested,
        m.ready_ratio() * 100.0
    );
    println!(
        "  messages  {:.0}/s sent of {:.0}/s target, {}/{} dispatches delivered ({:.2}%)",
        m.send_rate_achieved,
        m.send_rate_target,
        m.deliveries_received,
        m.deliveries_expected,
        m.delivery_ratio() * 100.0
    );
    println!(
        "  fan-out   p50 {:.1} ms  p95 {:.1} ms  p99 {:.1} ms  max {:.1} ms",
        m.fanout.percentile_ms(50.0),
        m.fanout.percentile_ms(95.0),
        m.fanout.percentile_ms(99.0),
        m.fanout.max_ms()
    );
    println!(
        "  REST      {} ok, {} failed  p50 {:.1} ms  p95 {:.1} ms  p99 {:.1} ms  max {:.1} ms",
        m.rest.count(),
        m.rest.errors(),
        m.rest.percentile_ms(50.0),
        m.rest.percentile_ms(95.0),
        m.rest.percentile_ms(99.0),
        m.rest.max_ms()
    );
    if let Some(p) = &m.first_paint {
        println!(
            "  first paint  {} servers: {:.0} ms, READY frame {} bytes{}, {} of {} requests failed ({} were 429)",
            p.guilds,
            p.millis,
            p.ready_bytes,
            if p.ready_missing { " (no READY)" } else { "" },
            p.rate_limited + p.failed,
            p.requests,
            p.rate_limited
        );
    }
    if failures.is_empty() {
        println!("PASS");
    } else {
        for f in failures {
            println!("FAIL: {f}");
        }
    }
}

fn report_json(args: &Args, m: &Measured, failures: &[String]) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "base_url": args.base_url,
        "sessions": { "requested": m.sessions_requested, "ready": m.sessions_ready },
        "messages": {
            "target_per_s": m.send_rate_target,
            "achieved_per_s": m.send_rate_achieved,
            "deliveries_expected": m.deliveries_expected,
            "deliveries_received": m.deliveries_received,
        },
        "fanout_ms": { "p50": m.fanout.percentile_ms(50.0), "p95": m.fanout.percentile_ms(95.0), "p99": m.fanout.percentile_ms(99.0), "max": m.fanout.max_ms() },
        "rest": {
            "ok": m.rest.count(), "failed": m.rest.errors(),
            "p50_ms": m.rest.percentile_ms(50.0), "p95_ms": m.rest.percentile_ms(95.0),
            "p99_ms": m.rest.percentile_ms(99.0), "max_ms": m.rest.max_ms(),
        },
        "first_paint": m.first_paint.as_ref().map(|p| serde_json::json!({
            "guilds": p.guilds, "ms": p.millis, "ready_bytes": p.ready_bytes,
            "requests": p.requests, "rate_limited": p.rate_limited, "failed": p.failed,
            "ready_missing": p.ready_missing,
        })),
        "passed": failures.is_empty(),
        "failures": failures,
    }))
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn websocket_url_follows_the_base_url() {
        assert_eq!(
            ws_url("http://127.0.0.1:8080").unwrap(),
            "ws://127.0.0.1:8080/gateway?v=10&encoding=json"
        );
        assert_eq!(
            ws_url("http://host/").unwrap(),
            "ws://host/gateway?v=10&encoding=json"
        );
        assert!(ws_url("https://host").is_err());
    }

    #[test]
    fn the_generator_covers_the_range() {
        let mut r = Rng(1);
        let mut seen = [false; 5];
        for _ in 0..200 {
            seen[r.below(5)] = true;
        }
        assert!(seen.iter().all(|s| *s));
    }

    #[test]
    fn arguments_have_the_inventory_defaults() {
        let a = Args::parse_from(["sr-loadtest", "--database-url", "postgres://u@h/load"]);
        assert_eq!(
            (a.sessions, a.msg_rate as u32, a.p95_ms as u32),
            (2000, 200, 100)
        );
        // clap falls back to SR_DATABASE__URL, so "required" is only observable without it (a shell that
        // runs the Postgres suites usually exports it).
        if std::env::var_os("SR_DATABASE__URL").is_none() {
            assert!(
                Args::try_parse_from(["sr-loadtest"]).is_err(),
                "the database URL is required"
            );
        }
    }

    #[test]
    fn the_csrf_token_matches_the_servers() {
        // sr_api is a dev-dependency: a change on either side fails here, not as a wall of 403s.
        for raw in ["a", "0123456789abcdef", "tok/en+with=chars"] {
            assert_eq!(seed::csrf_for(raw), sr_api::auth::csrf::token_for(raw));
        }
    }
}
