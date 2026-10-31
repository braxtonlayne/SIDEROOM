//! Config types and figment loader (FND-CORE-007). Native only (feature `config`).
//!
//! Layering, lowest to highest precedence: built-in defaults, TOML file,
//! `SR_*` environment variables. Nested keys use a double underscore, e.g.
//! `SR_DATABASE__URL` sets `database.url`.

use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid configuration: {0}")]
    Invalid(Box<figment::Error>),
}

impl From<figment::Error> for ConfigError {
    fn from(e: figment::Error) -> Self {
        Self::Invalid(Box::new(e))
    }
}

/// A string that never prints its value in `Debug` output.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(v: impl Into<String>) -> Self {
        Self(v.into())
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub bind: String,
    /// Public base URL used in links and cookies.
    pub public_url: String,
    /// Snowflake worker id (0-31); unique per node.
    pub worker_id: u8,
    /// Snowflake process id (0-31).
    pub process_id: u8,
    /// Loopback listener for `/internal/metrics`.
    pub metrics_bind: String,
    /// 64 hex chars (32 bytes): encrypts TOTP secrets at rest. Empty means an
    /// ephemeral random key is used (development only).
    pub secret_key: Secret,
    /// Seconds to wait for in-flight requests and jobs on shutdown.
    pub shutdown_grace_secs: u64,
    /// Previous keys, still accepted for decrypting TOTP secrets during
    /// rotation (64 hex chars each). Open question 40.
    pub secret_key_previous: Vec<Secret>,
    /// Take the client IP from the last `X-Forwarded-For` entry. Enable only
    /// behind a reverse proxy that appends to the header (Caddy).
    pub trust_forwarded_for: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8080".into(),
            public_url: "http://localhost:8080".into(),
            worker_id: 0,
            process_id: 0,
            metrics_bind: "127.0.0.1:9100".into(),
            secret_key: Secret::default(),
            shutdown_grace_secs: 25,
            secret_key_previous: Vec::new(),
            trust_forwarded_for: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    pub url: Secret,
    pub max_connections: u32,
    /// Run migrations before binding the listener. For single-node installs;
    /// multi-node deployments run `sr-cli migrate` instead. Open question 106.
    pub auto_migrate: bool,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: Secret::new("postgres://sideroom:sideroom@localhost:5432/sideroom"),
            max_connections: 20,
            auto_migrate: false,
        }
    }
}

/// Transport security for the SMTP relay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SmtpTls {
    /// Upgrade a plain connection with STARTTLS (port 587).
    #[default]
    Starttls,
    /// Implicit TLS from the first byte (port 465).
    Tls,
    /// Plaintext; only for a local relay.
    None,
}

/// `[smtp]`: outgoing mail. An empty `host` means the log-only mailer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Secret,
    pub tls: SmtpTls,
    pub from: String,
}

impl Default for SmtpConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 587,
            username: String::new(),
            password: Secret::default(),
            tls: SmtpTls::default(),
            from: String::new(),
        }
    }
}

/// `[gateway]`: WebSocket limits. Open question 107.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GatewayLimits {
    /// Connections accepted at once; further upgrades get 503.
    pub max_connections: usize,
}

impl Default for GatewayLimits {
    fn default() -> Self {
        Self {
            max_connections: 10_000,
        }
    }
}

/// `[voice]`: the SFU embedded in the server and the gateway's voice wiring (docs/specs/
/// lounge-voice-video.md section 4). Off by default: enabling it binds the UDP range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceConfig {
    pub enabled: bool,
    /// Public WebSocket URLs of the SFU nodes, handed to clients in `VOICE_SERVER_UPDATE`.
    /// Empty: derived from `server.public_url` as `ws(s)://<host>/voice/ws` (the embedded SFU).
    pub endpoints: Vec<String>,
    /// Interface the media sockets bind to.
    pub bind_ip: String,
    /// UDP media port range (inclusive); at most 256 sockets are bound from it.
    pub udp_port_min: u16,
    pub udp_port_max: u16,
    /// ICE-TCP passive port; 0 disables it.
    pub tcp_port: u16,
    /// Addresses advertised as ICE host candidates: the node's public (or LAN) IPs.
    pub announce_ips: Vec<String>,
    pub max_participants: usize,
    pub max_room_size: usize,
    /// Secret shared by the gateway and the SFU for join tokens. Empty: derived from
    /// `server.secret_key`.
    pub token_secret: Secret,
    /// Seconds a voice state survives its gateway session losing the socket.
    pub session_grace_secs: u64,
    /// Egress budget of this node in Mbit/s for the egress governor (doc 08 section 3):
    /// above it, simulcast layers are capped for new and existing subscribers, Level 0
    /// first and Patrons last. `0` turns the governor off. A good value is 70 % of the
    /// measured uplink.
    pub egress_budget_mbps: u32,
    /// Per-subscriber TWCC bandwidth estimation for video layer selection. Off by default:
    /// it makes the SFU pad every sending connection to 50 kbit/s, audio-only ones too. With
    /// it off, layer selection falls back to loss reports.
    pub video_bwe: bool,
    /// Each member's activity data-channel budget in kbit/s (`voice.datachannel_kbps`, ACT-CH-001): what a
    /// member may send through the SFU to the other participants of an activity instance; above it messages
    /// are dropped. `0` turns activity data channels off.
    pub datachannel_kbps: u32,
    /// Media actors the SFU shards its rooms over (one per channel id hash, each with its own
    /// UDP sockets). Defaults to `1` (single actor) until the AX42 run is recorded; `0` picks half
    /// the available cores, at most 8; it never exceeds the number
    /// of bound media sockets.
    pub shards: usize,
    /// The removed `voice.e2ee` switch (OPS-IMM-001). End-to-end encryption of calls is the only voice
    /// mode (D-053, D-249, D-263): the gateway is the MLS delivery service over op 101 E2EE_MLS, a join
    /// that does not declare E2EE is refused, and the SFU forwards a sender only after its key package.
    /// The key is still read so that an old `e2ee = false` is an error instead of being ignored;
    /// `e2ee = true` is accepted and does nothing. Never serialized.
    #[serde(rename = "e2ee", skip_serializing_if = "Option::is_none")]
    removed_e2ee: Option<bool>,
    pub turn: TurnSection,
}

/// `[voice.turn]`: STUN/TURN handed to clients (coturn REST credentials, ADR-0004).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TurnSection {
    /// `stun:host:3478`, `turn:host:3478?transport=udp`, `turns:host:443?transport=tcp`.
    pub urls: Vec<String>,
    /// coturn `static-auth-secret`. Empty disables TURN credentials (STUN URLs still work).
    pub secret: Secret,
    /// Credential lifetime in seconds.
    pub ttl_secs: u64,
}

impl Default for TurnSection {
    fn default() -> Self {
        Self {
            urls: Vec::new(),
            secret: Secret::default(),
            ttl_secs: 3600,
        }
    }
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoints: Vec::new(),
            bind_ip: "0.0.0.0".into(),
            udp_port_min: 50_000,
            udp_port_max: 50_099,
            tcp_port: 0,
            announce_ips: vec!["127.0.0.1".into()],
            max_participants: 2_000,
            max_room_size: 99,
            token_secret: Secret::default(),
            session_grace_secs: 20,
            egress_budget_mbps: 0,
            video_bwe: false,
            datachannel_kbps: 256,
            removed_e2ee: None,
            shards: 1,
            turn: TurnSection::default(),
        }
    }
}

/// `[unfurl]`: server-side link previews (LNG-MSG-014). The feature itself is the
/// `unfurl` feature module (default on); this section holds operator policy. The hosts and the rate
/// are the bootstrap values: once an administrator saves them on the Email & Integrations page
/// (`platform.integrations`, D-254) the database row wins, and applies live (D-300).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UnfurlConfig {
    /// Hosts (and their subdomains) that are never fetched.
    pub deny_hosts: Vec<String>,
    /// When not empty, link previews and the image proxy fetch only these hosts (and
    /// their subdomains). The deny list still wins.
    pub allow_hosts: Vec<String>,
    /// Outgoing requests allowed per destination host per minute.
    pub per_host_per_minute: u32,
    /// TEST ONLY, DANGEROUS: disables the private/loopback address check of the
    /// SSRF policy so a local test page can be previewed. Anyone who can post a link
    /// can then make this server fetch internal URLs. Never enable it in production.
    pub allow_private_for_tests: bool,
}

impl Default for UnfurlConfig {
    fn default() -> Self {
        Self {
            deny_hosts: Vec::new(),
            allow_hosts: Vec::new(),
            per_host_per_minute: 20,
            allow_private_for_tests: false,
        }
    }
}

/// `[gif]`: the GIF picker's provider (LNG-EXP-005). Empty `provider` switches the
/// feature off; the picker then says no provider is connected. The server proxies every
/// request and every preview, so members' browsers never contact the provider. `country` and
/// `locale` are bootstrap values; a saved `platform.integrations` row wins (D-254). The key stays here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GifConfig {
    /// `"tenor"` or empty (off).
    pub provider: String,
    /// The provider API key. Treat as a secret (`SR_GIF__API_KEY_FILE` works).
    pub api_key: Secret,
    /// Tenor `client_key`: names this installation in the provider's dashboard.
    pub client_key: String,
    /// ISO 3166-1 alpha-2 region for results, e.g. `US`.
    pub country: String,
    /// Locale for results, e.g. `en_US`.
    pub locale: String,
}

impl Default for GifConfig {
    fn default() -> Self {
        Self {
            provider: String::new(),
            api_key: Secret::default(),
            client_key: "sideroom".to_owned(),
            country: "US".to_owned(),
            locale: "en_US".to_owned(),
        }
    }
}

impl GifConfig {
    /// Whether the picker has a usable provider.
    pub fn enabled(&self) -> bool {
        self.provider.eq_ignore_ascii_case("tenor") && !self.api_key.expose().is_empty()
    }
}

/// `[ingest]`: folders the server may read for watched folders and imports (REV-UP-005). A staff
/// member can only point an ingest source at a directory under one of these roots; with none
/// listed, local folders are off (storage prefixes below `imports/` still work).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct IngestConfig {
    /// Absolute directories, for example a mounted Storage Box. From the environment this may be a
    /// single path or a comma-separated list (`SR_INGEST__ROOTS=/srv/sideroom/ingest`); empty means none.
    #[serde(deserialize_with = "roots_from_list_or_csv")]
    pub roots: Vec<String>,
}

/// Accept a TOML array of paths, or one string with comma-separated paths (what a compose
/// environment line can carry). Blank entries are dropped.
fn roots_from_list_or_csv<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Roots {
        List(Vec<String>),
        Csv(String),
    }
    let all = match Roots::deserialize(d)? {
        Roots::List(l) => l,
        Roots::Csv(s) => s.split(',').map(str::to_owned).collect(),
    };
    Ok(all
        .into_iter()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect())
}

/// `[lookup]`: metadata lookup providers behind the submission editor's "Fetch details"
/// (REV-ED-005). Everything is off by default: a provider is used only when the operator turns it
/// on here (and gave its key, where it needs one) AND the platform setting
/// `review.lookup.enabled` is on. Requests go out from this server under the SSRF-safe client; the
/// member's browser never contacts a provider. The providers offered are the bootstrap value; a saved
/// `platform.integrations` list wins (D-254). API keys stay here, never in the database.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LookupConfig {
    /// AniList (manga and anime), no key needed.
    pub anilist: bool,
    /// MangaDex, no key needed.
    pub mangadex: bool,
    /// MangaUpdates, no key needed.
    pub mangaupdates: bool,
    /// TMDB (films and series): a v3 API key or a v4 read access token. Empty switches TMDB off.
    /// Treat as a secret (`SR_LOOKUP__TMDB_API_KEY_FILE` works).
    pub tmdb_api_key: Secret,
    /// ComicVine (comics): the API key. Empty switches ComicVine off.
    /// Treat as a secret (`SR_LOOKUP__COMICVINE_API_KEY_FILE` works).
    pub comicvine_api_key: Secret,
}

/// `[search]`: the message search index (open questions 131-138).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchConfig {
    /// Directory holding the Tantivy index. Relative paths resolve against the
    /// working directory. The legacy `SR_SEARCH_DIR` variable still overrides the
    /// default.
    pub dir: String,
}

impl SearchConfig {
    /// The built-in value of [`Self::dir`].
    pub const DEFAULT_DIR: &'static str = "data/search";
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            dir: Self::DEFAULT_DIR.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key: Secret,
    pub secret_key: Secret,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:3900".into(),
            region: "garage".into(),
            bucket: "sideroom".into(),
            access_key: Secret::default(),
            secret_key: Secret::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LogConfig {
    /// `tracing` env-filter directive.
    pub filter: String,
    pub json: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            filter: "info".into(),
            json: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub storage: StorageConfig,
    pub log: LogConfig,
    pub smtp: SmtpConfig,
    pub discord: DiscordConfig,
    pub connections: ConnectionsConfig,
    pub gateway: GatewayLimits,
    pub search: SearchConfig,
    pub unfurl: UnfurlConfig,
    pub gif: GifConfig,
    pub lookup: LookupConfig,
    pub ingest: IngestConfig,
    pub voice: VoiceConfig,
    pub activities: ActivitiesConfig,
    pub web: WebConfig,
    pub bridge: BridgeConfig,
}

/// `[web]`: the built web app (`apps/web/dist`) and the gate in front of it (SEC-ENT-003, D-385,
/// docs/dev/entrance-isolation.md section 3). With `root` unset the server serves no documents (the API
/// only). With it set, `sr-server` serves the app (`/app`, `/app/*` and every in-app path) only to a request
/// with a valid session, and refuses to start when `<root>/app/csp.txt` is missing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WebConfig {
    /// The built `apps/web/dist` (`SR_WEB__ROOT`). Empty or unset: no documents are served.
    pub root: Option<String>,
    /// Serve the public surfaces, the legal page and the error pages too (single-binary installs without a
    /// proxy, CI). Default false: Caddy serves them and proxies the rest here.
    pub serve_public: bool,
}

impl WebConfig {
    /// The web root when one is configured (an empty string counts as unset).
    pub fn root_path(&self) -> Option<&str> {
        self.root
            .as_deref()
            .map(str::trim)
            .filter(|r| !r.is_empty())
    }
}

/// `[bridge]`: where the Discord bridge (BRG-COR-001, BRG-GW-001) talks to Discord. Both unset: Discord itself
/// (`https://discord.com/api/v10` and its gateway). Tests and the end-to-end suite point them at a fake Discord
/// (`e2e/support/fake-discord.mjs`), so real Discord is never contacted. The environment variables
/// `SR_BRIDGE_DISCORD_API` and `SR_BRIDGE_DISCORD_GATEWAY` are read too when these are unset.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BridgeConfig {
    /// The REST root, for example `http://127.0.0.1:8090/api/v10` (`SR_BRIDGE__DISCORD_API`).
    pub discord_api: Option<String>,
    /// The gateway WebSocket, for example `ws://127.0.0.1:8090/gateway` (`SR_BRIDGE__DISCORD_GATEWAY`).
    pub discord_gateway: Option<String>,
}

/// `[activities]`: Activities (DEV-ACT, docs/11 section 3). Off by default: when on, the server answers on
/// `https://<application-id>.activities.<host>` (the wildcard host of `deploy/compose/caddy/activities.snippet`,
/// `<host>` taken from `server.public_url`), the voice call bar shows the Activities button and launching
/// works. The web build's `VITE_ACTIVITIES_ENABLED` and the Caddy CSP are switched on with it (OQ-XC-3).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ActivitiesConfig {
    pub enabled: bool,
}

/// `[discord]`: OAuth sign-in and registration (open question 79). An empty
/// `client_id` disables the Discord routes. The redirect URI registered with
/// Discord is `<public_url>/api/v1/sr/auth/discord/callback`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscordConfig {
    pub client_id: String,
    pub client_secret: Secret,
}

/// One provider of `[connections.<provider>]` (LNG-USET-024): the OAuth application the
/// operator registered with it. The redirect URI to register is
/// `<public_url>/api/v1/connections/<provider>/callback`. A provider without a `client_id`
/// is off (Steam, which needs no secret, is on unless `enabled = false`); `enabled = false`
/// switches a configured provider off.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConnectionProviderConfig {
    pub client_id: String,
    pub client_secret: Secret,
    pub enabled: Option<bool>,
}

/// `[connections]`: account connections shown on profiles. Each provider is optional; the
/// operator keeps the client secrets (`SR_CONNECTIONS__GITHUB__CLIENT_SECRET` or
/// `SR_CONNECTIONS__GITHUB__CLIENT_SECRET_FILE`). For Steam, `client_secret` is an optional
/// Steam Web API key (it adds the display name and profile stats).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConnectionsConfig {
    pub github: ConnectionProviderConfig,
    pub discord: ConnectionProviderConfig,
    pub spotify: ConnectionProviderConfig,
    pub twitch: ConnectionProviderConfig,
    pub youtube: ConnectionProviderConfig,
    pub reddit: ConnectionProviderConfig,
    pub steam: ConnectionProviderConfig,
}

/// Host part of an absolute URL (`[::1]` keeps its brackets).
fn url_host(url: &str) -> Option<&str> {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit_once('@').map_or(authority, |(_, a)| a);
    if authority.starts_with('[') {
        let end = authority.find(']')?;
        return Some(&authority[..=end]);
    }
    authority.split(':').next()
}

/// Whether the public URL points at a development host (D-357): `localhost`, a `127.*` loopback literal,
/// `[::1]`, or a name ending in `.localhost` (RFC 6761 reserves those for loopback). The empty secret key and
/// `SR_PREFLIGHT_INSECURE_TLS` are tolerated only there.
pub fn is_development_host(public_url: &str) -> bool {
    let Some(host) = url_host(public_url) else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    let host = host.trim_end_matches('.');
    host == "localhost"
        || host == "[::1]"
        || host.ends_with(".localhost")
        || (host.starts_with("127.")
            && host
                .parse::<std::net::Ipv4Addr>()
                .is_ok_and(|ip| ip.is_loopback()))
}

fn is_hex_key(key: &str) -> bool {
    key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Config keys that can be supplied from a file through `SR_<KEY>_FILE` (Docker and
/// Kubernetes secrets, systemd credentials). `server.secret_key_previous` is a list.
pub const SECRET_FILE_KEYS: &[&str] = &[
    "server.secret_key",
    "database.url",
    "smtp.password",
    "voice.token_secret",
    "voice.turn.secret",
    "storage.access_key",
    "storage.secret_key",
    "discord.client_secret",
    "connections.github.client_secret",
    "connections.discord.client_secret",
    "connections.spotify.client_secret",
    "connections.twitch.client_secret",
    "connections.youtube.client_secret",
    "connections.reddit.client_secret",
    "connections.steam.client_secret",
    "gif.api_key",
    "lookup.tmdb_api_key",
    "lookup.comicvine_api_key",
];

/// The list-valued key that reads one entry per line from `SR_<KEY>_FILE`.
pub const SECRET_FILE_LIST_KEY: &str = "server.secret_key_previous";

/// The environment variable that names the file for `key` (`database.url` becomes
/// `SR_DATABASE__URL_FILE`).
pub fn secret_file_var(key: &str) -> String {
    format!("SR_{}_FILE", key.to_ascii_uppercase().replace('.', "__"))
}

/// Read a secret file: one value, trailing newline trimmed, never empty.
fn read_secret_file(var: &str, path: &str) -> Result<String, ConfigError> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| invalid(&format!("{var}: cannot read {path}: {e}")))?;
    let v = raw.trim_end_matches(['\r', '\n']).to_owned();
    if v.is_empty() {
        return Err(invalid(&format!("{var}: {path} is empty")));
    }
    Ok(v)
}

fn invalid(msg: &str) -> ConfigError {
    ConfigError::Invalid(Box::new(figment::Error::from(msg.to_owned())))
}

/// Merge `SR_*_FILE` secrets into `f`. `lookup` reads the environment (injected for
/// tests). A file beats nothing else silently: setting both `SR_X` and `SR_X_FILE` is an
/// error, so a stale plain value can never shadow a rotated secret file.
pub fn with_secret_files(
    mut f: Figment,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<Figment, ConfigError> {
    let keys = SECRET_FILE_KEYS.iter().chain([&SECRET_FILE_LIST_KEY]);
    for key in keys {
        let var = secret_file_var(key);
        let Some(path) = lookup(&var).filter(|p| !p.is_empty()) else {
            continue;
        };
        let plain = var.trim_end_matches("_FILE");
        if lookup(plain).is_some_and(|v| !v.is_empty()) {
            return Err(invalid(&format!("set only one of {plain} and {var}")));
        }
        let value = if *key == SECRET_FILE_LIST_KEY {
            let raw = std::fs::read_to_string(&path)
                .map_err(|e| invalid(&format!("{var}: cannot read {path}: {e}")))?;
            let list: Vec<String> = raw
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect();
            figment::value::Value::serialize(list)?
        } else {
            figment::value::Value::serialize(read_secret_file(&var, &path)?)?
        };
        f = f.merge(Serialized::default(key, value));
    }
    Ok(f)
}

impl Config {
    /// Build the layered figment: defaults, optional TOML file, `SR_*` env.
    pub fn figment(toml_path: Option<&Path>) -> Figment {
        let mut f = Figment::from(Serialized::defaults(Config::default()));
        if let Some(p) = toml_path {
            f = f.merge(Toml::file(p));
        }
        f.merge(Env::prefixed("SR_").split("__"))
    }

    /// Load and validate configuration.
    pub fn load(toml_path: Option<&Path>) -> Result<Self, ConfigError> {
        let f = with_secret_files(Self::figment(toml_path), &|k| std::env::var(k).ok())?;
        Self::from_figment(&f)
    }

    pub fn from_figment(f: &Figment) -> Result<Self, ConfigError> {
        let cfg: Config = f.extract()?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        let bad = |msg: &str| ConfigError::Invalid(Box::new(figment::Error::from(msg.to_owned())));
        if self.server.worker_id >= 32 {
            return Err(bad("server.worker_id must be < 32"));
        }
        if self.server.process_id >= 32 {
            return Err(bad("server.process_id must be < 32"));
        }
        let key = self.server.secret_key.expose();
        if !key.is_empty() && !is_hex_key(key) {
            return Err(bad("server.secret_key must be 64 hex characters"));
        }
        // An ephemeral key loses every stored TOTP secret on restart, so it is
        // only tolerated on a loopback development host.
        if key.is_empty() && !is_development_host(&self.server.public_url) {
            return Err(bad(
                "server.secret_key is required unless server.public_url is a localhost address",
            ));
        }
        if self
            .server
            .secret_key_previous
            .iter()
            .any(|k| !is_hex_key(k.expose()))
        {
            return Err(bad(
                "server.secret_key_previous entries must be 64 hex characters",
            ));
        }
        if self.database.max_connections == 0 {
            return Err(bad("database.max_connections must be > 0"));
        }
        if self.search.dir.trim().is_empty() {
            return Err(bad("search.dir must not be empty"));
        }
        if self.gateway.max_connections == 0 {
            return Err(bad("gateway.max_connections must be > 0"));
        }
        if self.voice.removed_e2ee == Some(false) {
            return Err(bad(
                "voice.e2ee was removed: end-to-end encryption is the only voice mode, so delete the e2ee = false line (or SR_VOICE__E2EE=false)",
            ));
        }
        let v = &self.voice;
        if v.enabled {
            if v.udp_port_min > v.udp_port_max {
                return Err(bad("voice.udp_port_min must be <= voice.udp_port_max"));
            }
            if v.bind_ip.parse::<std::net::IpAddr>().is_err() {
                return Err(bad("voice.bind_ip must be an IP address"));
            }
            if v.announce_ips.is_empty()
                || v.announce_ips
                    .iter()
                    .any(|ip| ip.parse::<std::net::IpAddr>().is_err())
            {
                return Err(bad(
                    "voice.announce_ips must be a non-empty list of IP addresses",
                ));
            }
            if v.max_participants == 0 || v.max_room_size == 0 {
                return Err(bad(
                    "voice.max_participants and voice.max_room_size must be > 0",
                ));
            }
            if !v.token_secret.expose().is_empty() && v.token_secret.expose().len() < 16 {
                return Err(bad("voice.token_secret must be at least 16 characters"));
            }
            if !v.turn.secret.expose().is_empty() && v.turn.urls.is_empty() {
                return Err(bad(
                    "voice.turn.urls is required when voice.turn.secret is set",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingest_roots_accept_a_list_or_a_comma_separated_string() {
        let load = |toml: &str| {
            Config::from_figment(
                &Figment::from(Serialized::defaults(Config::default())).merge(Toml::string(toml)),
            )
            .unwrap()
            .ingest
            .roots
        };
        assert!(load("").is_empty());
        assert_eq!(load("[ingest]\nroots = [\"/a\", \"/b\"]"), ["/a", "/b"]);
        assert_eq!(
            load("[ingest]\nroots = \"/srv/sideroom/ingest\""),
            ["/srv/sideroom/ingest"]
        );
        assert_eq!(load("[ingest]\nroots = \"/a, /b,\""), ["/a", "/b"]);
        assert!(load("[ingest]\nroots = \"\"").is_empty());
    }
    #[test]
    fn voice_is_off_by_default_and_validated_when_on() {
        let c = Config::default();
        assert!(!c.voice.enabled);
        assert_eq!(
            (c.voice.udp_port_min, c.voice.udp_port_max),
            (50_000, 50_099)
        );
        assert_eq!(c.voice.turn.ttl_secs, 3600);
        // One media actor until the AX42 run is recorded (H-44); 0 keeps the auto rule.
        assert_eq!(c.voice.shards, 1);
        // OPS-IMM-001: there is no switch for call encryption.
        assert!(c.voice.removed_e2ee.is_none());
        let on = |toml: &str| {
            Config::from_figment(
                &Figment::from(Serialized::defaults(Config::default())).merge(Toml::string(
                    &format!(
                        "[voice]
enabled = true
{toml}"
                    ),
                )),
            )
        };
        assert!(on("").is_ok());
        // The removed switch: false is an error (never silently ignored), true is harmless.
        let err = on("e2ee = false").unwrap_err().to_string();
        assert!(err.contains("voice.e2ee was removed"), "{err}");
        assert!(on("e2ee = true").is_ok());
        assert_eq!(on("shards = 0").unwrap().voice.shards, 0);
        assert!(
            on("udp_port_min = 60000
udp_port_max = 50000")
            .is_err()
        );
        assert!(on("bind_ip = \"nope\"").is_err());
        assert!(on("announce_ips = []").is_err());
        assert!(on("announce_ips = [\"not an ip\"]").is_err());
        assert!(on("token_secret = \"short\"").is_err());
        assert!(
            on("[voice.turn]
secret = \"s\"")
            .is_err()
        );
        assert!(
            on("[voice.turn]
secret = \"s\"
urls = [\"turn:t:3478\"]")
            .is_ok()
        );
        // A disabled section is never validated.
        assert!(
            Config::from_figment(
                &Figment::from(Serialized::defaults(Config::default())).merge(Toml::string(
                    "[voice]
announce_ips = []
"
                ))
            )
            .is_ok()
        );
    }

    #[test]
    fn defaults_valid() {
        let f = Figment::from(Serialized::defaults(Config::default()));
        assert_eq!(Config::from_figment(&f).unwrap(), Config::default());
    }

    #[test]
    fn toml_overrides_defaults() {
        let f = Figment::from(Serialized::defaults(Config::default())).merge(Toml::string(
            "[server]\nbind = \"0.0.0.0:9000\"\nworker_id = 4\n[database]\nmax_connections = 5\n",
        ));
        let c = Config::from_figment(&f).unwrap();
        assert_eq!(c.server.bind, "0.0.0.0:9000");
        assert_eq!(c.server.worker_id, 4);
        assert_eq!(c.database.max_connections, 5);
        assert_eq!(c.log, LogConfig::default());
    }

    #[test]
    fn search_dir_defaults_and_is_configurable() {
        assert_eq!(Config::default().search.dir, "data/search");
        let f = Figment::from(Serialized::defaults(Config::default()))
            .merge(Toml::string("[search]\ndir = \"/var/lib/sr/search\"\n"));
        assert_eq!(
            Config::from_figment(&f).unwrap().search.dir,
            "/var/lib/sr/search"
        );
        let f = Figment::from(Serialized::defaults(Config::default()))
            .merge(Toml::string("[search]\ndir = \"\"\n"));
        assert!(Config::from_figment(&f).is_err());
    }

    #[test]
    fn rejects_bad_secret_key() {
        let f = Figment::from(Serialized::defaults(Config::default())).merge(Toml::string(
            "[server]
secret_key = \"abc\"
",
        ));
        assert!(Config::from_figment(&f).is_err());
    }

    #[test]
    fn empty_secret_key_only_allowed_on_loopback() {
        let remote = Figment::from(Serialized::defaults(Config::default())).merge(Toml::string(
            "[server]\npublic_url = \"https://chat.example.org\"\n",
        ));
        assert!(Config::from_figment(&remote).is_err());

        for url in [
            "http://localhost:8080",
            "http://127.0.0.1:8080/",
            "http://[::1]:8080",
        ] {
            let local = Figment::from(Serialized::defaults(Config::default()))
                .merge(Toml::string(&format!("[server]\npublic_url = \"{url}\"\n")));
            assert!(Config::from_figment(&local).is_ok(), "{url}");
        }

        let keyed =
            Figment::from(Serialized::defaults(Config::default())).merge(Toml::string(&format!(
                "[server]\npublic_url = \"https://chat.example.org\"\nsecret_key = \"{}\"\n",
                "ab".repeat(32)
            )));
        assert!(Config::from_figment(&keyed).is_ok());
    }

    #[test]
    fn previous_keys_and_smtp_parse() {
        let f = Figment::from(Serialized::defaults(Config::default()))
            .merge(Toml::string("[server]\nsecret_key_previous = [\"ab\"]\n"));
        assert!(Config::from_figment(&f).is_err());

        let f = Figment::from(Serialized::defaults(Config::default())).merge(Toml::string(
            "[smtp]\nhost = \"mail.example.org\"\ntls = \"tls\"\nfrom = \"SIDE ROOM <no-reply@example.org>\"\n[database]\nauto_migrate = true\n[gateway]\nmax_connections = 5\n",
        ));
        let c = Config::from_figment(&f).unwrap();
        assert_eq!(c.smtp.tls, SmtpTls::Tls);
        assert_eq!(c.smtp.port, 587);
        assert!(c.database.auto_migrate);
        assert_eq!(c.gateway.max_connections, 5);
        assert!(!c.server.trust_forwarded_for);
    }

    #[test]
    fn url_host_forms() {
        assert_eq!(url_host("https://a.b:8443/x"), Some("a.b"));
        assert_eq!(url_host("http://[::1]:80"), Some("[::1]"));
        assert_eq!(url_host("http://u:p@localhost"), Some("localhost"));
    }

    #[test]
    fn development_hosts() {
        for url in [
            "http://localhost:5173",
            "http://127.0.0.1:8080",
            "http://127.1.2.3",
            "http://[::1]:80",
            "https://smoke.localhost",
            "https://a.b.LOCALHOST/x",
        ] {
            assert!(is_development_host(url), "{url}");
        }
        for url in [
            "https://chat.example.org",
            "https://localhost.example.org",
            "https://evil-localhost",
            "http://127.0.0.1.example.org",
            "http://10.0.0.1",
            "",
        ] {
            assert!(!is_development_host(url), "{url}");
        }
    }

    #[test]
    fn rejects_bad_worker() {
        let f = Figment::from(Serialized::defaults(Config::default()))
            .merge(Toml::string("[server]\nworker_id = 40\n"));
        assert!(Config::from_figment(&f).is_err());
    }

    #[test]
    fn secrets_are_redacted() {
        let dbg = format!("{:?}", Config::default());
        assert!(!dbg.contains("sideroom:sideroom"));
    }

    fn secret_file(name: &str, body: &str) -> String {
        let p = std::env::temp_dir().join(format!("sr_cfg_{}_{name}", std::process::id()));
        std::fs::write(&p, body).unwrap();
        p.to_string_lossy().into_owned()
    }

    fn base() -> Figment {
        Figment::from(Serialized::defaults(Config::default()))
    }

    #[test]
    fn secret_files_fill_secrets_and_trim_the_newline() {
        let key = "ab".repeat(32);
        let sk = secret_file("sk", &format!("{key}\n"));
        let db = secret_file("db", "postgres://u:p@db/x\r\n");
        let prev = secret_file(
            "prev",
            &format!("{}\n\n{}\n", "cd".repeat(32), "ef".repeat(32)),
        );
        let env = move |k: &str| match k {
            "SR_SERVER__SECRET_KEY_FILE" => Some(sk.clone()),
            "SR_DATABASE__URL_FILE" => Some(db.clone()),
            "SR_SERVER__SECRET_KEY_PREVIOUS_FILE" => Some(prev.clone()),
            _ => None,
        };
        let cfg = Config::from_figment(&with_secret_files(base(), &env).unwrap()).unwrap();
        assert_eq!(cfg.server.secret_key.expose(), key);
        assert_eq!(cfg.database.url.expose(), "postgres://u:p@db/x");
        assert_eq!(cfg.server.secret_key_previous.len(), 2);
    }

    #[test]
    fn secret_file_errors_are_loud() {
        let empty = secret_file("empty", "\n");
        let good = secret_file("good", "s3cret-value-123456");
        let var = "SR_VOICE__TOKEN_SECRET_FILE";
        // Missing file.
        let missing = |k: &str| (k == var).then(|| "/nonexistent/sr/secret".to_owned());
        assert!(with_secret_files(base(), &missing).is_err());
        // Empty file.
        let e = empty.clone();
        assert!(with_secret_files(base(), &move |k| (k == var).then(|| e.clone())).is_err());
        // Both the plain variable and the file: refused.
        let g = good.clone();
        let both = move |k: &str| match k {
            "SR_VOICE__TOKEN_SECRET_FILE" => Some(g.clone()),
            "SR_VOICE__TOKEN_SECRET" => Some("plain".to_owned()),
            _ => None,
        };
        assert!(with_secret_files(base(), &both).is_err());
        // The file alone works.
        let g = good.clone();
        let f = with_secret_files(base(), &move |k| (k == var).then(|| g.clone())).unwrap();
        assert_eq!(
            Config::from_figment(&f)
                .unwrap()
                .voice
                .token_secret
                .expose(),
            "s3cret-value-123456"
        );
    }

    #[test]
    fn every_secret_field_has_a_file_variable() {
        // Every `Secret` in the config tree must be reachable through `_FILE`; adding a new
        // secret field without listing it here fails this test.
        let json = serde_json::to_value(Config::default()).unwrap();
        let mut secrets = Vec::new();
        fn walk(v: &serde_json::Value, path: &str, out: &mut Vec<String>) {
            if let serde_json::Value::Object(m) = v {
                for (k, v) in m {
                    let p = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    };
                    walk(v, &p, out);
                }
            } else if let Some(name) = path.rsplit('.').next()
                && [
                    "secret",
                    "password",
                    "token_secret",
                    "secret_key",
                    "access_key",
                    "url",
                ]
                .contains(&name)
                && !path.starts_with("unfurl")
            {
                out.push(path.to_owned());
            }
        }
        walk(&json, "", &mut secrets);
        for s in secrets {
            // `server.public_url`/`metrics` style URLs are not secrets; only `database.url` is.
            if s.ends_with("public_url") || s == "server.secret_key_previous" {
                continue;
            }
            assert!(
                SECRET_FILE_KEYS.contains(&s.as_str()),
                "{s} looks like a secret but is missing from SECRET_FILE_KEYS"
            );
        }
    }
}
