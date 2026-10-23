//! What the checks need to know, as plain data (built from the loaded configuration).

use sr_core::config::{Config, SECRET_FILE_KEYS, SmtpTls, secret_file_var};
use sr_voice_proto::turn::TurnConfig;

/// The mail relay to look at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmtpTarget {
    pub host: String,
    pub port: u16,
    pub tls: SmtpTls,
}

/// Plain-data view of the configuration for the checks. No secret value is kept except the TURN shared
/// secret, which only feeds the credential derivation.
#[derive(Debug, Clone)]
pub struct Settings {
    pub public_url: String,
    /// Accept the certificate of a private CA (the smoke stack's Caddy); never on by default.
    pub insecure_tls: bool,
    /// `None`: no relay configured (mail goes to the log).
    pub smtp: Option<SmtpTarget>,
    /// `storage.endpoint` is `memory:`.
    pub storage_memory: bool,
    pub turn: TurnConfig,
    pub voice_enabled: bool,
    /// Voice endpoints handed to clients; empty means the embedded SFU behind `public_url`.
    pub voice_endpoints: Vec<String>,
    /// `SR_PAYMENTS_ESPLORA_URL`, when payments are on.
    pub esplora_url: Option<String>,
    /// `SR_PAYMENTS_ALLOW_PRIVATE_HOSTS` is `1` or `true`: `sr-payments` then accepts a private esplora address
    /// (the self-hosted electrs), so the esplora row does too (D-358).
    pub esplora_allow_private: bool,
    /// Folders whose free space matters: the data folder, the search index, every ingest root.
    pub disk_paths: Vec<String>,
    /// `(environment variable, path)` of every `SR_*_FILE` secret that is set.
    pub secret_files: Vec<(String, String)>,
    /// The `config.toml` in use, when there is one.
    pub config_file: Option<String>,
}

impl Settings {
    /// Build from the configuration. `env` reads the environment (injected for tests).
    pub fn from_config(
        cfg: &Config,
        config_file: Option<&str>,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Self {
        let smtp = (!cfg.smtp.host.trim().is_empty()).then(|| SmtpTarget {
            host: cfg.smtp.host.clone(),
            port: cfg.smtp.port,
            tls: cfg.smtp.tls,
        });
        let mut disk_paths = vec![".".to_owned()];
        if !cfg.search.dir.is_empty() {
            disk_paths.push(cfg.search.dir.clone());
        }
        disk_paths.extend(cfg.ingest.roots.iter().cloned());
        let secret_files = SECRET_FILE_KEYS
            .iter()
            .chain(["server.secret_key_previous"].iter())
            .filter_map(|k| {
                let var = secret_file_var(k);
                env(&var).filter(|p| !p.is_empty()).map(|path| (var, path))
            })
            .collect();
        Self {
            public_url: cfg.server.public_url.clone(),
            insecure_tls: false,
            smtp,
            storage_memory: cfg.storage.endpoint == "memory:",
            turn: TurnConfig {
                urls: cfg.voice.turn.urls.clone(),
                secret: cfg.voice.turn.secret.expose().to_owned(),
                ttl_secs: cfg.voice.turn.ttl_secs,
            },
            voice_enabled: cfg.voice.enabled,
            voice_endpoints: cfg.voice.endpoints.clone(),
            esplora_url: env("SR_PAYMENTS_ESPLORA_URL")
                .map(|u| u.trim().to_owned())
                .filter(|u| !u.is_empty()),
            esplora_allow_private: env("SR_PAYMENTS_ALLOW_PRIVATE_HOSTS")
                .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true")),
            disk_paths,
            secret_files,
            config_file: config_file.map(str::to_owned),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_from_defaults_and_environment() {
        let cfg = Config::default();
        let env = |k: &str| match k {
            "SR_PAYMENTS_ESPLORA_URL" => Some(" https://esplora.test/api ".to_owned()),
            "SR_PAYMENTS_ALLOW_PRIVATE_HOSTS" => Some("TRUE".to_owned()),
            "SR_DATABASE__URL_FILE" => Some("/run/secrets/db".to_owned()),
            _ => None,
        };
        let s = Settings::from_config(&cfg, Some("/etc/sideroom/config.toml"), &env);
        assert_eq!(s.esplora_url.as_deref(), Some("https://esplora.test/api"));
        assert!(s.esplora_allow_private);
        assert!(!Settings::from_config(&cfg, None, &|_| None).esplora_allow_private);
        assert_eq!(
            s.secret_files,
            vec![(
                "SR_DATABASE__URL_FILE".to_owned(),
                "/run/secrets/db".to_owned()
            )]
        );
        assert!(s.disk_paths.contains(&".".to_owned()));
        assert_eq!(s.config_file.as_deref(), Some("/etc/sideroom/config.toml"));
        assert!(s.smtp.is_none());
    }
}
