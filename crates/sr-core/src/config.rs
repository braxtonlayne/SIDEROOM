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

/// `[unfurl]`: server-side link previews (LNG-MSG-014). The feature itself is the
/// `unfurl` feature module (default on); this section holds operator policy.
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
    pub gateway: GatewayLimits,
    pub search: SearchConfig,
    pub unfurl: UnfurlConfig,
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

/// Whether the public URL points at a loopback development host.
fn is_local_host(public_url: &str) -> bool {
    matches!(
        url_host(public_url),
        Some("localhost" | "127.0.0.1" | "[::1]")
    )
}

fn is_hex_key(key: &str) -> bool {
    key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit())
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
        Self::from_figment(&Self::figment(toml_path))
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
        if key.is_empty() && !is_local_host(&self.server.public_url) {
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
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
