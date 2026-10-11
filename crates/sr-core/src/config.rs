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
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8080".into(),
            public_url: "http://localhost:8080".into(),
            worker_id: 0,
            process_id: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    pub url: Secret,
    pub max_connections: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: Secret::new("postgres://sideroom:sideroom@localhost:5432/sideroom"),
            max_connections: 20,
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
        if self.database.max_connections == 0 {
            return Err(bad("database.max_connections must be > 0"));
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
