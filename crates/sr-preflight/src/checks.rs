//! The checks. Each is a function over probe traits and returns one [`Check`] row.

use crate::{
    report::{Check, Report, Status},
    settings::{Settings, SmtpTarget},
    turn,
};
use async_trait::async_trait;
use bytes::Bytes;
use sr_core::{UserId, config::SmtpTls};
use sr_storage::{ObjectStorage, storage::PresignMethod};
use std::time::Duration;

const GIB: u64 = 1 << 30;

/// How the migrations stand.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrationState {
    pub applied: usize,
    pub pending: usize,
    pub failed: Vec<i64>,
    pub unknown: Vec<i64>,
}

/// The database side of the checks.
#[async_trait]
pub trait DbProbe: Send + Sync {
    async fn ping(&self) -> Result<(), String>;
    async fn migrations(&self) -> Result<MigrationState, String>;
    /// The database clock in Unix milliseconds.
    async fn now_ms(&self) -> Result<i64, String>;
    /// Whether the Web Push VAPID key row exists.
    async fn vapid_present(&self) -> Result<bool, String>;
}

/// An HTTP answer, reduced to what the checks read.
#[derive(Debug, Clone, Default)]
pub struct HttpReply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    /// The first bytes of the body, lossy.
    pub body: String,
}

impl HttpReply {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Network access from the box.
#[async_trait]
pub trait NetProbe: Send + Sync {
    /// Connect over TCP; with `banner`, also return the first line the server sends.
    async fn tcp(&self, host: &str, port: u16, banner: bool) -> Result<Option<String>, String>;
    async fn http_get(&self, url: &str, insecure: bool) -> Result<HttpReply, String>;
    /// A TURN allocation over UDP, released again.
    async fn turn_allocate(
        &self,
        host: &str,
        port: u16,
        username: &str,
        password: &str,
    ) -> Result<(), String>;
}

/// File mode of a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeResult {
    Missing,
    /// Permission bits (`0o600`).
    Mode(u32),
    /// This operating system has no Unix modes.
    Unsupported,
}

/// The local machine.
pub trait HostProbe: Send + Sync {
    /// `(total, free)` bytes of the file system holding `path`.
    fn disk(&self, path: &str) -> Option<(u64, u64)>;
    fn mode(&self, path: &str) -> ModeResult;
    fn now_ms(&self) -> i64;
}

/// Everything a run needs.
pub struct Ctx<'a> {
    pub db: &'a dyn DbProbe,
    pub storage: &'a dyn ObjectStorage,
    pub net: &'a dyn NetProbe,
    pub host: &'a dyn HostProbe,
    pub settings: &'a Settings,
}

/// Run every check (concurrently) and collect the table.
pub async fn run(ctx: &Ctx<'_>) -> Report {
    let (db, mig, clock, storage, smtp, url, turn, vapid, disk, secrets, esplora, voice) = tokio::join!(
        database(ctx.db),
        migrations(ctx.db),
        clock_skew(ctx.db, ctx.host),
        storage(ctx.storage, ctx.host, ctx.settings.storage_memory),
        smtp(ctx.net, ctx.settings),
        public_url(ctx.net, ctx.settings),
        turn_check(ctx.net, ctx.host, ctx.settings),
        vapid(ctx.db),
        async { disks(ctx.host, ctx.settings) },
        async { secret_files(ctx.host, ctx.settings) },
        esplora(ctx.net, ctx.settings),
        voice(ctx.net, ctx.settings),
    );
    Report::new(
        vec![
            db, mig, clock, storage, smtp, url, turn, vapid, disk, secrets, esplora, voice,
        ],
        ctx.host.now_ms(),
    )
}

/// The four database-backed rows (database, migrations, clock, Web Push key); `run` uses the same functions.
pub async fn database_checks(db: &dyn DbProbe, host: &dyn HostProbe) -> Vec<Check> {
    vec![
        database(db).await,
        migrations(db).await,
        clock_skew(db, host).await,
        vapid(db).await,
    ]
}

pub(crate) async fn database(db: &dyn DbProbe) -> Check {
    match db.ping().await {
        Ok(()) => Check::new("database", "Database", Status::Pass, "reachable"),
        Err(e) => Check::new(
            "database",
            "Database",
            Status::Fail,
            format!("cannot query the database: {e}. Check database.url and that Postgres is up."),
        ),
    }
}

pub(crate) async fn migrations(db: &dyn DbProbe) -> Check {
    let t = "Migrations";
    match db.migrations().await {
        Err(e) => Check::new(
            "migrations",
            t,
            Status::Fail,
            format!("cannot read them: {e}"),
        ),
        Ok(m) if !m.failed.is_empty() => Check::new(
            "migrations",
            t,
            Status::Fail,
            format!(
                "{} recorded as failed ({:?}): needs a human, see sr-cli upgrade --dry-run",
                m.failed.len(),
                m.failed
            ),
        ),
        Ok(m) if !m.unknown.is_empty() => Check::new(
            "migrations",
            t,
            Status::Fail,
            format!(
                "the database has {} migration(s) this program does not know: a downgrade? Install the newer release.",
                m.unknown.len()
            ),
        ),
        Ok(m) if m.pending > 0 => Check::new(
            "migrations",
            t,
            Status::Fail,
            format!("{} pending: run sr-cli migrate", m.pending),
        ),
        Ok(m) => Check::new(
            "migrations",
            t,
            Status::Pass,
            format!("current ({} applied)", m.applied),
        ),
    }
}

pub(crate) async fn clock_skew(db: &dyn DbProbe, host: &dyn HostProbe) -> Check {
    let t = "Clock";
    let before = host.now_ms();
    let theirs = match db.now_ms().await {
        Ok(v) => v,
        Err(e) => {
            return Check::new(
                "clock",
                t,
                Status::Fail,
                format!("cannot read the database clock: {e}"),
            );
        }
    };
    let after = host.now_ms();
    let skew = ((before + after) / 2 - theirs).abs();
    let detail = format!("{skew} ms apart from the database clock");
    if skew >= 30_000 {
        Check::new(
            "clock",
            t,
            Status::Fail,
            format!("{detail}: fix NTP, sign-in codes and tokens will fail"),
        )
    } else if skew >= 2_000 {
        Check::new("clock", t, Status::Warn, format!("{detail}: check NTP"))
    } else {
        Check::new("clock", t, Status::Pass, detail)
    }
}

pub(crate) async fn storage(s: &dyn ObjectStorage, host: &dyn HostProbe, memory: bool) -> Check {
    let t = "Object storage";
    let key = format!("preflight/{}-probe.txt", host.now_ms());
    let body = Bytes::from_static(b"sideroom preflight");
    if let Err(e) = s.put(&key, body.clone()).await {
        return Check::new(
            "storage",
            t,
            Status::Fail,
            format!("put failed: {e}. Check the storage section and bucket."),
        );
    }
    let got = s.get(&key).await;
    let result = match got {
        Err(e) => Err(format!("get failed: {e}")),
        Ok(b) if b != body => Err("get returned different bytes than were put".to_owned()),
        Ok(_) => match s
            .presign(PresignMethod::Get, &key, Duration::from_secs(60))
            .await
        {
            Err(e) => Err(format!("presign failed: {e}")),
            Ok(u) if u.is_empty() => Err("presign returned an empty URL".to_owned()),
            Ok(_) => match s.multipart_create(&format!("{key}.multipart")).await {
                Err(e) => Err(format!("multipart create failed: {e}")),
                Ok(mut m) => m
                    .abort()
                    .await
                    .map_err(|e| format!("multipart abort failed: {e}")),
            },
        },
    };
    let _ = s.delete(&key).await;
    match result {
        Err(e) => Check::new("storage", t, Status::Fail, e),
        Ok(()) if memory => Check::new(
            "storage",
            t,
            Status::Warn,
            "works, but storage.endpoint is memory: nothing survives a restart",
        ),
        Ok(()) => Check::new(
            "storage",
            t,
            Status::Pass,
            "put, get, presign and multipart create/abort work",
        ),
    }
}

pub(crate) async fn smtp(net: &dyn NetProbe, s: &Settings) -> Check {
    let t = "Mail relay";
    let Some(SmtpTarget { host, port, tls }) = &s.smtp else {
        return Check::new(
            "smtp",
            t,
            Status::Warn,
            "smtp.host is empty: mail goes to the log only, so invites and resets cannot reach anyone",
        );
    };
    let banner = !matches!(tls, SmtpTls::Tls);
    match net.tcp(host, *port, banner).await {
        Err(e) => Check::new(
            "smtp",
            t,
            Status::Fail,
            format!("cannot connect to {host}:{port}: {e}"),
        ),
        Ok(Some(line)) if banner && !line.starts_with("220") => Check::new(
            "smtp",
            t,
            Status::Warn,
            format!(
                "{host}:{port} answered, but not with a 220 greeting (is smtp.tls right for this port?)"
            ),
        ),
        Ok(_) => Check::new(
            "smtp",
            t,
            Status::Pass,
            format!("{host}:{port} accepts connections (nothing sent)"),
        ),
    }
}

fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let auth = rest.split(['/', '?', '#']).next().unwrap_or("");
    if let Some(r) = auth.strip_prefix('[') {
        return r.split(']').next().unwrap_or("");
    }
    auth.rsplit_once(':').map_or(auth, |(h, _)| h)
}

fn is_local(host: &str) -> bool {
    host == "localhost" || host == "::1" || host.starts_with("127.")
}

pub(crate) async fn public_url(net: &dyn NetProbe, s: &Settings) -> Check {
    let t = "Public URL";
    let url = s.public_url.trim_end_matches('/');
    let https = url.starts_with("https://");
    let reply = match net.http_get(&format!("{url}/"), s.insecure_tls).await {
        Ok(r) => r,
        Err(e) => {
            return Check::new(
                "public-url",
                t,
                Status::Fail,
                format!("{url} is not reachable from this box: {e}"),
            );
        }
    };
    if reply.status >= 500 {
        return Check::new(
            "public-url",
            t,
            Status::Fail,
            format!("{url} answered {}", reply.status),
        );
    }
    let mut missing = Vec::new();
    if https && reply.header("strict-transport-security").is_none() {
        missing.push("Strict-Transport-Security");
    }
    match reply.header("content-security-policy") {
        None => missing.push("Content-Security-Policy"),
        Some(csp) if !csp.contains("frame-ancestors") => missing.push("CSP frame-ancestors"),
        Some(_) => {}
    }
    if !missing.is_empty() {
        return Check::new(
            "public-url",
            t,
            Status::Fail,
            format!(
                "{url} answered {} but lacks: {}. The reverse proxy or the app is not set up as deploy/compose/caddy/Caddyfile.prod.",
                reply.status,
                missing.join(", ")
            ),
        );
    }
    if !https {
        let status = if is_local(host_of(url)) {
            Status::Warn
        } else {
            Status::Fail
        };
        return Check::new(
            "public-url",
            t,
            status,
            format!(
                "{url} is plain http: HSTS could not be checked; use an https server.public_url"
            ),
        );
    }
    if reply.status >= 400 {
        return Check::new(
            "public-url",
            t,
            Status::Warn,
            format!("{url} answered {}", reply.status),
        );
    }
    Check::new(
        "public-url",
        t,
        Status::Pass,
        format!(
            "{url} answered {} with HSTS, CSP and frame-ancestors",
            reply.status
        ),
    )
}

pub(crate) async fn turn_check(net: &dyn NetProbe, host: &dyn HostProbe, s: &Settings) -> Check {
    let t = "TURN";
    let now = u64::try_from(host.now_ms() / 1000).unwrap_or(0);
    let Some(c) = s.turn.credentials(UserId::new(1), now) else {
        return Check::new(
            "turn",
            t,
            Status::Warn,
            "no TURN configured (voice.turn.urls and voice.turn.secret): calls behind strict NATs will not connect",
        );
    };
    let parsed: Vec<_> = c.uris.iter().filter_map(|u| turn::parse_url(u)).collect();
    let udp = parsed
        .iter()
        .find(|(scheme, _, _, tr)| scheme == "turn" && tr.as_deref().is_none_or(|x| x == "udp"));
    if let Some((_, h, p, _)) = udp {
        return match net.turn_allocate(h, *p, &c.username, &c.password).await {
            Ok(()) => Check::new(
                "turn",
                t,
                Status::Pass,
                format!("credentials derive and {h}:{p} granted an allocation"),
            ),
            Err(e) => Check::new(
                "turn",
                t,
                Status::Fail,
                format!("allocation at {h}:{p} failed: {e}"),
            ),
        };
    }
    match parsed.first() {
        None => Check::new(
            "turn",
            t,
            Status::Fail,
            "voice.turn.urls has no valid turn: or turns: URL",
        ),
        Some((_, h, p, _)) => match net.tcp(h, *p, false).await {
            Ok(_) => Check::new(
                "turn",
                t,
                Status::Pass,
                format!(
                    "credentials derive and {h}:{p} accepts TCP (allocation is only tried over UDP)"
                ),
            ),
            Err(e) => Check::new(
                "turn",
                t,
                Status::Fail,
                format!("cannot connect to {h}:{p}: {e}"),
            ),
        },
    }
}

pub(crate) async fn vapid(db: &dyn DbProbe) -> Check {
    let t = "Web Push key";
    match db.vapid_present().await {
        Ok(true) => Check::new("vapid", t, Status::Pass, "the VAPID key is stored"),
        Ok(false) => Check::new(
            "vapid",
            t,
            Status::Warn,
            "no VAPID key yet: sr-server creates it on its first start, then push works",
        ),
        Err(e) => Check::new("vapid", t, Status::Fail, format!("cannot read it: {e}")),
    }
}

fn gib(b: u64) -> String {
    format!("{:.1} GiB", b as f64 / GIB as f64)
}

pub(crate) fn disks(host: &dyn HostProbe, s: &Settings) -> Check {
    let t = "Disk space";
    let mut status = Status::Pass;
    let mut parts = Vec::new();
    for path in &s.disk_paths {
        match host.disk(path) {
            None => {
                status = status.max_with(Status::Warn);
                parts.push(format!("{path}: unreadable"));
            }
            Some((total, free)) => {
                let pct = if total == 0 {
                    0.0
                } else {
                    free as f64 * 100.0 / total as f64
                };
                let this = if free < GIB || pct < 3.0 {
                    Status::Fail
                } else if free < 5 * GIB || pct < 10.0 {
                    Status::Warn
                } else {
                    Status::Pass
                };
                status = status.max_with(this);
                parts.push(format!("{path}: {} free ({pct:.0}%)", gib(free)));
            }
        }
    }
    let tail = match status {
        Status::Pass => "",
        Status::Warn => " (getting low)",
        Status::Fail => " (almost full: free space before the first invite)",
    };
    Check::new("disk", t, status, format!("{}{tail}", parts.join("; ")))
}

pub(crate) fn secret_files(host: &dyn HostProbe, s: &Settings) -> Check {
    let t = "Secret files";
    let mut fail = Vec::new();
    let mut warn = Vec::new();
    let mut unsupported = false;
    for (var, path) in &s.secret_files {
        match host.mode(path) {
            ModeResult::Missing => fail.push(format!("{var}: {path} does not exist")),
            ModeResult::Unsupported => unsupported = true,
            ModeResult::Mode(m) if m != 0o600 && m != 0o400 => {
                fail.push(format!("{var}: {path} has mode {m:o}, expected 600 or 400"));
            }
            ModeResult::Mode(_) => {}
        }
    }
    if let Some(path) = &s.config_file {
        match host.mode(path) {
            ModeResult::Missing => fail.push(format!("config file {path} does not exist")),
            ModeResult::Unsupported => unsupported = true,
            ModeResult::Mode(m) if m & 0o077 != 0 => {
                warn.push(format!(
                    "config file {path} is readable by others (mode {m:o}); it may hold secrets"
                ));
            }
            ModeResult::Mode(_) => {}
        }
    }
    if !fail.is_empty() {
        return Check::new("secrets", t, Status::Fail, fail.join("; "));
    }
    if !warn.is_empty() {
        return Check::new("secrets", t, Status::Warn, warn.join("; "));
    }
    if unsupported {
        return Check::new(
            "secrets",
            t,
            Status::Warn,
            "file modes cannot be checked on this operating system",
        );
    }
    if s.secret_files.is_empty() {
        return Check::new(
            "secrets",
            t,
            Status::Pass,
            "no SR_*_FILE secrets are set; secrets come from the environment",
        );
    }
    Check::new(
        "secrets",
        t,
        Status::Pass,
        format!(
            "{} secret file(s) exist with mode 600 or 400",
            s.secret_files.len()
        ),
    )
}

pub(crate) async fn esplora(net: &dyn NetProbe, s: &Settings) -> Check {
    let t = "Esplora (payments)";
    let Some(base) = &s.esplora_url else {
        return Check::new(
            "esplora",
            t,
            Status::Pass,
            "payments are off (SR_PAYMENTS_ESPLORA_URL is not set)",
        );
    };
    let url = format!("{}/blocks/tip/height", base.trim_end_matches('/'));
    match net.http_get(&url, false).await {
        Err(e) => Check::new(
            "esplora",
            t,
            Status::Fail,
            format!("{base} is not reachable: {e}"),
        ),
        Ok(r) if r.status == 200 && r.body.trim().parse::<u64>().is_ok() => Check::new(
            "esplora",
            t,
            Status::Pass,
            format!("{base} answers, chain tip {}", r.body.trim()),
        ),
        Ok(r) => Check::new(
            "esplora",
            t,
            Status::Fail,
            format!("{base} answered {} without a block height", r.status),
        ),
    }
}

/// `host`, `port` of a `ws(s)://` or `http(s)://` URL.
fn host_port(url: &str) -> Option<(String, u16)> {
    let (scheme, rest) = url.split_once("://")?;
    let default = if matches!(scheme, "wss" | "https") {
        443
    } else {
        80
    };
    let auth = rest.split(['/', '?', '#']).next()?;
    let (h, p) = if let Some(r) = auth.strip_prefix('[') {
        let (h, tail) = r.split_once(']')?;
        (h, tail.strip_prefix(':').and_then(|p| p.parse().ok()))
    } else if let Some((h, p)) = auth.rsplit_once(':') {
        (h, p.parse().ok())
    } else {
        (auth, None)
    };
    (!h.is_empty()).then(|| (h.to_owned(), p.unwrap_or(default)))
}

pub(crate) async fn voice(net: &dyn NetProbe, s: &Settings) -> Check {
    let t = "Voice node";
    if !s.voice_enabled {
        return Check::new("voice", t, Status::Pass, "voice is off");
    }
    let targets: Vec<String> = if s.voice_endpoints.is_empty() {
        vec![s.public_url.clone()]
    } else {
        s.voice_endpoints.clone()
    };
    let mut bad = Vec::new();
    for u in &targets {
        match host_port(u) {
            None => bad.push(format!("{u}: not a URL")),
            Some((h, p)) => {
                if let Err(e) = net.tcp(&h, p, false).await {
                    bad.push(format!("{h}:{p}: {e}"));
                }
            }
        }
    }
    if bad.is_empty() {
        Check::new(
            "voice",
            t,
            Status::Pass,
            format!("{} voice endpoint(s) accept connections", targets.len()),
        )
    } else {
        Check::new(
            "voice",
            t,
            Status::Fail,
            format!("cannot reach {}", bad.join("; ")),
        )
    }
}

impl Status {
    /// The worse of two statuses.
    pub(crate) fn max_with(self, other: Status) -> Status {
        use Status::{Fail, Pass, Warn};
        match (self, other) {
            (Fail, _) | (_, Fail) => Fail,
            (Warn, _) | (_, Warn) => Warn,
            _ => Pass,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_storage::{MultipartSession, ObjectStoreStorage, storage::ObjectInfo};
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeDb {
        ping: Option<String>,
        mig: Option<MigrationState>,
        now_offset: i64,
        vapid: Option<bool>,
    }

    #[async_trait]
    impl DbProbe for FakeDb {
        async fn ping(&self) -> Result<(), String> {
            self.ping.clone().map_or(Ok(()), Err)
        }
        async fn migrations(&self) -> Result<MigrationState, String> {
            Ok(self.mig.clone().unwrap_or(MigrationState {
                applied: 5,
                ..Default::default()
            }))
        }
        async fn now_ms(&self) -> Result<i64, String> {
            Ok(1_000_000 + self.now_offset)
        }
        async fn vapid_present(&self) -> Result<bool, String> {
            Ok(self.vapid.unwrap_or(true))
        }
    }

    struct FakeHost {
        disks: Vec<(String, Option<(u64, u64)>)>,
        modes: Vec<(String, ModeResult)>,
    }
    impl Default for FakeHost {
        fn default() -> Self {
            Self {
                disks: vec![],
                modes: vec![],
            }
        }
    }
    impl HostProbe for FakeHost {
        fn disk(&self, path: &str) -> Option<(u64, u64)> {
            self.disks
                .iter()
                .find(|(p, _)| p == path)
                .and_then(|(_, d)| *d)
        }
        fn mode(&self, path: &str) -> ModeResult {
            self.modes
                .iter()
                .find(|(p, _)| p == path)
                .map_or(ModeResult::Missing, |(_, m)| *m)
        }
        fn now_ms(&self) -> i64 {
            1_000_000
        }
    }

    #[derive(Default)]
    struct FakeNet {
        tcp: Mutex<Vec<(String, u16, bool)>>,
        tcp_err: Option<String>,
        banner: Option<String>,
        http: Option<Result<HttpReply, String>>,
        turn: Option<Result<(), String>>,
    }
    #[async_trait]
    impl NetProbe for FakeNet {
        async fn tcp(&self, host: &str, port: u16, banner: bool) -> Result<Option<String>, String> {
            self.tcp
                .lock()
                .unwrap()
                .push((host.to_owned(), port, banner));
            match &self.tcp_err {
                Some(e) => Err(e.clone()),
                None => Ok(self.banner.clone()),
            }
        }
        async fn http_get(&self, _: &str, _: bool) -> Result<HttpReply, String> {
            self.http.clone().unwrap_or(Err("no fake".into()))
        }
        async fn turn_allocate(&self, _: &str, _: u16, _: &str, _: &str) -> Result<(), String> {
            self.turn.clone().unwrap_or(Ok(()))
        }
    }

    fn settings() -> Settings {
        Settings {
            public_url: "https://sr.example.org".into(),
            insecure_tls: false,
            smtp: None,
            storage_memory: false,
            turn: Default::default(),
            voice_enabled: false,
            voice_endpoints: vec![],
            esplora_url: None,
            disk_paths: vec!["/data".into()],
            secret_files: vec![],
            config_file: None,
        }
    }

    fn reply(status: u16, headers: &[(&str, &str)]) -> HttpReply {
        HttpReply {
            status,
            headers: headers
                .iter()
                .map(|(k, v)| ((*k).into(), (*v).into()))
                .collect(),
            body: String::new(),
        }
    }

    #[tokio::test]
    async fn database_and_migrations() {
        let ok = FakeDb::default();
        assert_eq!(database(&ok).await.status, Status::Pass);
        let down = FakeDb {
            ping: Some("refused".into()),
            ..Default::default()
        };
        let c = database(&down).await;
        assert_eq!(c.status, Status::Fail);
        assert!(c.detail.contains("refused"));

        assert_eq!(migrations(&ok).await.status, Status::Pass);
        let pending = FakeDb {
            mig: Some(MigrationState {
                applied: 3,
                pending: 2,
                ..Default::default()
            }),
            ..Default::default()
        };
        let c = migrations(&pending).await;
        assert_eq!(c.status, Status::Fail);
        assert!(c.detail.contains("sr-cli migrate"));
        let failed = FakeDb {
            mig: Some(MigrationState {
                failed: vec![7],
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(migrations(&failed).await.status, Status::Fail);
        let newer = FakeDb {
            mig: Some(MigrationState {
                unknown: vec![9999],
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(migrations(&newer).await.status, Status::Fail);
    }

    #[tokio::test]
    async fn clock_skew_thresholds() {
        let host = FakeHost::default();
        for (off, want) in [
            (0, Status::Pass),
            (5_000, Status::Warn),
            (-60_000, Status::Fail),
        ] {
            let db = FakeDb {
                now_offset: off,
                ..Default::default()
            };
            assert_eq!(clock_skew(&db, &host).await.status, want, "offset {off}");
        }
    }

    struct BrokenPut(ObjectStoreStorage);
    #[async_trait]
    impl ObjectStorage for BrokenPut {
        async fn put(&self, _: &str, _: Bytes) -> sr_storage::Result<()> {
            Err(sr_storage::StorageError::Backend("bucket missing".into()))
        }
        async fn get(&self, k: &str) -> sr_storage::Result<Bytes> {
            self.0.get(k).await
        }
        async fn get_range(&self, k: &str, a: u64, b: u64) -> sr_storage::Result<Bytes> {
            self.0.get_range(k, a, b).await
        }
        async fn head(&self, k: &str) -> sr_storage::Result<ObjectInfo> {
            self.0.head(k).await
        }
        async fn delete(&self, k: &str) -> sr_storage::Result<()> {
            self.0.delete(k).await
        }
        async fn list(&self, p: &str) -> sr_storage::Result<Vec<ObjectInfo>> {
            self.0.list(p).await
        }
        async fn presign(
            &self,
            m: PresignMethod,
            k: &str,
            t: Duration,
        ) -> sr_storage::Result<String> {
            self.0.presign(m, k, t).await
        }
        async fn multipart_create(&self, k: &str) -> sr_storage::Result<Box<dyn MultipartSession>> {
            self.0.multipart_create(k).await
        }
    }

    #[tokio::test]
    async fn storage_roundtrip() {
        let host = FakeHost::default();
        let mem = ObjectStoreStorage::in_memory();
        assert_eq!(storage(&mem, &host, false).await.status, Status::Pass);
        assert_eq!(storage(&mem, &host, true).await.status, Status::Warn);
        // The probe object is cleaned up.
        assert!(mem.list("preflight/").await.unwrap().is_empty());
        let broken = BrokenPut(ObjectStoreStorage::in_memory());
        let c = storage(&broken, &host, false).await;
        assert_eq!(c.status, Status::Fail);
        assert!(c.detail.contains("put failed"));
    }

    #[tokio::test]
    async fn smtp_states() {
        let mut s = settings();
        let net = FakeNet::default();
        assert_eq!(smtp(&net, &s).await.status, Status::Warn);
        s.smtp = Some(SmtpTarget {
            host: "mail.test".into(),
            port: 587,
            tls: SmtpTls::Starttls,
        });
        let good = FakeNet {
            banner: Some("220 mail ready".into()),
            ..Default::default()
        };
        assert_eq!(smtp(&good, &s).await.status, Status::Pass);
        let odd = FakeNet {
            banner: Some("HTTP/1.1 400".into()),
            ..Default::default()
        };
        assert_eq!(smtp(&odd, &s).await.status, Status::Warn);
        let down = FakeNet {
            tcp_err: Some("refused".into()),
            ..Default::default()
        };
        assert_eq!(smtp(&down, &s).await.status, Status::Fail);
        // Implicit TLS: no banner is read.
        s.smtp = Some(SmtpTarget {
            host: "mail.test".into(),
            port: 465,
            tls: SmtpTls::Tls,
        });
        let plain = FakeNet::default();
        assert_eq!(smtp(&plain, &s).await.status, Status::Pass);
        assert!(!plain.tcp.lock().unwrap()[0].2);
    }

    #[tokio::test]
    async fn public_url_headers() {
        let s = settings();
        let full = [
            ("Strict-Transport-Security", "max-age=63072000"),
            (
                "Content-Security-Policy",
                "default-src 'self'; frame-ancestors 'none'",
            ),
        ];
        let net = FakeNet {
            http: Some(Ok(reply(200, &full))),
            ..Default::default()
        };
        assert_eq!(public_url(&net, &s).await.status, Status::Pass);
        let no_hsts = FakeNet {
            http: Some(Ok(reply(200, &full[1..]))),
            ..Default::default()
        };
        let c = public_url(&no_hsts, &s).await;
        assert_eq!(c.status, Status::Fail);
        assert!(c.detail.contains("Strict-Transport-Security"));
        let no_fa = FakeNet {
            http: Some(Ok(reply(
                200,
                &[full[0], ("content-security-policy", "default-src 'self'")],
            ))),
            ..Default::default()
        };
        assert!(
            public_url(&no_fa, &s)
                .await
                .detail
                .contains("frame-ancestors")
        );
        let err = FakeNet {
            http: Some(Err("dns".into())),
            ..Default::default()
        };
        assert_eq!(public_url(&err, &s).await.status, Status::Fail);
        let five = FakeNet {
            http: Some(Ok(reply(502, &full))),
            ..Default::default()
        };
        assert_eq!(public_url(&five, &s).await.status, Status::Fail);
        // Plain http: warn on localhost, fail elsewhere.
        let mut local = settings();
        local.public_url = "http://localhost:8080".into();
        let csp_only = FakeNet {
            http: Some(Ok(reply(200, &full[1..]))),
            ..Default::default()
        };
        assert_eq!(public_url(&csp_only, &local).await.status, Status::Warn);
        local.public_url = "http://sr.example.org".into();
        assert_eq!(public_url(&csp_only, &local).await.status, Status::Fail);
    }

    #[tokio::test]
    async fn turn_states() {
        let host = FakeHost::default();
        let mut s = settings();
        let net = FakeNet::default();
        assert_eq!(turn_check(&net, &host, &s).await.status, Status::Warn);
        s.turn = sr_voice_proto::turn::TurnConfig {
            urls: vec!["turn:turn.test:3478?transport=udp".into()],
            secret: "shared".into(),
            ttl_secs: 3600,
        };
        assert_eq!(turn_check(&net, &host, &s).await.status, Status::Pass);
        let refused = FakeNet {
            turn: Some(Err("credentials rejected".into())),
            ..Default::default()
        };
        let c = turn_check(&refused, &host, &s).await;
        assert_eq!(c.status, Status::Fail);
        assert!(c.detail.contains("rejected"));
        // TCP-only URLs fall back to a connect.
        s.turn.urls = vec!["turns:turn.test:5349?transport=tcp".into()];
        let tcp = FakeNet::default();
        assert_eq!(turn_check(&tcp, &host, &s).await.status, Status::Pass);
        assert_eq!(tcp.tcp.lock().unwrap()[0].1, 5349);
    }

    #[tokio::test]
    async fn vapid_states() {
        assert_eq!(vapid(&FakeDb::default()).await.status, Status::Pass);
        let none = FakeDb {
            vapid: Some(false),
            ..Default::default()
        };
        assert_eq!(vapid(&none).await.status, Status::Warn);
    }

    #[test]
    fn disk_thresholds() {
        let s = settings();
        let host = |free: u64| FakeHost {
            disks: vec![("/data".into(), Some((100 * GIB, free)))],
            ..Default::default()
        };
        assert_eq!(disks(&host(50 * GIB), &s).status, Status::Pass);
        assert_eq!(disks(&host(8 * GIB), &s).status, Status::Warn);
        assert_eq!(disks(&host(GIB / 2), &s).status, Status::Fail);
        assert_eq!(disks(&FakeHost::default(), &s).status, Status::Warn);
    }

    #[test]
    fn secret_file_modes() {
        let mut s = settings();
        assert_eq!(secret_files(&FakeHost::default(), &s).status, Status::Pass);
        s.secret_files = vec![("SR_DATABASE__URL_FILE".into(), "/run/db".into())];
        let host = |m: ModeResult| FakeHost {
            modes: vec![("/run/db".into(), m)],
            ..Default::default()
        };
        assert_eq!(
            secret_files(&host(ModeResult::Mode(0o600)), &s).status,
            Status::Pass
        );
        assert_eq!(
            secret_files(&host(ModeResult::Mode(0o400)), &s).status,
            Status::Pass
        );
        let c = secret_files(&host(ModeResult::Mode(0o644)), &s);
        assert_eq!(c.status, Status::Fail);
        assert!(c.detail.contains("644"));
        assert_eq!(secret_files(&FakeHost::default(), &s).status, Status::Fail);
        assert_eq!(
            secret_files(&host(ModeResult::Unsupported), &s).status,
            Status::Warn
        );
        s.secret_files.clear();
        s.config_file = Some("/etc/sr.toml".into());
        let cfg = |m: u32| FakeHost {
            modes: vec![("/etc/sr.toml".into(), ModeResult::Mode(m))],
            ..Default::default()
        };
        assert_eq!(secret_files(&cfg(0o640), &s).status, Status::Warn);
        assert_eq!(secret_files(&cfg(0o600), &s).status, Status::Pass);
    }

    #[tokio::test]
    async fn esplora_states() {
        let mut s = settings();
        let off = FakeNet::default();
        assert_eq!(esplora(&off, &s).await.status, Status::Pass);
        s.esplora_url = Some("https://esplora.test/api".into());
        let mut ok = reply(200, &[]);
        ok.body = "845123\n".into();
        let net = FakeNet {
            http: Some(Ok(ok)),
            ..Default::default()
        };
        assert_eq!(esplora(&net, &s).await.status, Status::Pass);
        let bad = FakeNet {
            http: Some(Ok(reply(200, &[]))),
            ..Default::default()
        };
        assert_eq!(esplora(&bad, &s).await.status, Status::Fail);
        let down = FakeNet {
            http: Some(Err("timeout".into())),
            ..Default::default()
        };
        assert_eq!(esplora(&down, &s).await.status, Status::Fail);
    }

    #[tokio::test]
    async fn voice_states() {
        let mut s = settings();
        let net = FakeNet::default();
        assert_eq!(voice(&net, &s).await.status, Status::Pass);
        s.voice_enabled = true;
        assert_eq!(voice(&net, &s).await.status, Status::Pass);
        assert_eq!(
            net.tcp.lock().unwrap()[0],
            ("sr.example.org".to_owned(), 443, false)
        );
        s.voice_endpoints = vec!["wss://sfu.test:7443/voice/ws".into()];
        let down = FakeNet {
            tcp_err: Some("refused".into()),
            ..Default::default()
        };
        let c = voice(&down, &s).await;
        assert_eq!(c.status, Status::Fail);
        assert!(c.detail.contains("sfu.test:7443"));
    }

    #[test]
    fn url_helpers() {
        assert_eq!(host_of("https://a.b:8443/x"), "a.b");
        assert_eq!(host_of("http://[::1]:80"), "::1");
        assert_eq!(host_port("ws://h/voice"), Some(("h".into(), 80)));
        assert_eq!(host_port("wss://[::1]:9/x"), Some(("::1".into(), 9)));
        assert_eq!(host_port("nonsense"), None);
    }

    #[tokio::test]
    async fn run_collects_every_row_in_order() {
        let db = FakeDb::default();
        let st = ObjectStoreStorage::in_memory();
        let net = FakeNet {
            http: Some(Ok(reply(
                200,
                &[
                    ("strict-transport-security", "max-age=1"),
                    ("content-security-policy", "frame-ancestors 'none'"),
                ],
            ))),
            ..Default::default()
        };
        let host = FakeHost {
            disks: vec![("/data".into(), Some((100 * GIB, 90 * GIB)))],
            ..Default::default()
        };
        let s = settings();
        let r = run(&Ctx {
            db: &db,
            storage: &st,
            net: &net,
            host: &host,
            settings: &s,
        })
        .await;
        let ids: Vec<_> = r.checks.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "database",
                "migrations",
                "clock",
                "storage",
                "smtp",
                "public-url",
                "turn",
                "vapid",
                "disk",
                "secrets",
                "esplora",
                "voice"
            ]
        );
        // smtp and turn are warnings, the rest passes.
        assert!(r.ok);
        assert_eq!((r.warned, r.failed), (2, 0));
    }
}
