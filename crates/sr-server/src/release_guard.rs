//! SEC-AUD-028: settings meant for tests and development that a release build refuses to start with.
//!
//! A debug build (`cargo build`, what CI's e2e and test jobs and the local test stacks run) only warns about them, as
//! before; a release build (`cargo build --release`, the release binaries and the server image) stops with a message
//! that names each key. The checks:
//!
//! - an empty `server.secret_key`: every boot would pick a new random key, so stored second factors are unreadable
//!   after a restart;
//! - `unfurl.allow_private_for_tests`: link previews, the image proxy and storage mounts could reach loopback and
//!   private addresses;
//! - `storage.endpoint = "memory:"`: every upload lives in the process and is gone on restart;
//! - `activities.origin_mode = "same_host_sandboxed"`: the short tunnel test mode (D-450), never for production;
//! - `SR_PAYMENTS_ALLOW_PRIVATE_HOSTS` unless the Esplora endpoint is a private service name or address (the
//!   self-hosted `http://electrs:3000` of the `payments` profile, D-358); on a public host name the opt-in would let
//!   DNS point the payment fetches at the internal network;
//! - a non-loopback `server.metrics_bind` without `server.metrics_token`: an unauthenticated `/internal/metrics`.

use sr_core::config::Config;

/// The reasons a release build refuses `cfg`, one line each (empty when it may start). `env` reads the process
/// environment (injected for tests).
pub fn release_refusals(cfg: &Config, env: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    let mut out = Vec::new();
    if cfg.server.secret_key.expose().is_empty() {
        out.push(
            "server.secret_key (SR_SERVER__SECRET_KEY) is empty: set a fixed 64-hex-character key \
             (openssl rand -hex 32), or every stored second factor is unreadable after a restart"
                .to_owned(),
        );
    }
    if cfg.unfurl.allow_private_for_tests {
        out.push(
            "unfurl.allow_private_for_tests (SR_UNFURL__ALLOW_PRIVATE_FOR_TESTS) is on: it lets link previews and \
             storage mounts reach private and loopback addresses; it is for tests only"
                .to_owned(),
        );
    }
    if cfg.storage.endpoint.trim() == "memory:" {
        out.push(
            "storage.endpoint (SR_STORAGE__ENDPOINT) is \"memory:\": every upload would be lost on restart; \
             point it at an S3 bucket"
                .to_owned(),
        );
    }
    if cfg.activities.same_host() {
        out.push(
            "activities.origin_mode (SR_ACTIVITIES__ORIGIN_MODE) is same_host_sandboxed: the test deployment mode \
             (D-450) runs only from a debug build; use the default subdomain mode"
                .to_owned(),
        );
    }
    let on = |k: &str| {
        env(k).is_some_and(|v| {
            let v = v.trim();
            v == "1" || v.eq_ignore_ascii_case("true")
        })
    };
    if on("SR_PAYMENTS_ALLOW_PRIVATE_HOSTS") {
        let esplora = env("SR_PAYMENTS_ESPLORA_URL").unwrap_or_default();
        if !is_private_service_url(&esplora) {
            out.push(
                "SR_PAYMENTS_ALLOW_PRIVATE_HOSTS is set, but SR_PAYMENTS_ESPLORA_URL is not a private service \
                 (a single-label name such as http://electrs:3000, or a private IP address): unset it"
                    .to_owned(),
            );
        }
    }
    if !crate::test_deploy::loopback_only(&cfg.server.metrics_bind)
        && cfg.server.metrics_token.expose().is_empty()
    {
        out.push(format!(
            "server.metrics_bind (SR_SERVER__METRICS_BIND) is {} without server.metrics_token \
             (SR_SERVER__METRICS_TOKEN): /internal/metrics would answer anyone who reaches it; bind it to \
             127.0.0.1 or set a token",
            cfg.server.metrics_bind
        ));
    }
    out
}

/// Whether `url` names a private service: a host without a dot (a Compose service name) or a private, loopback or
/// link-local IP literal.
fn is_private_service_url(url: &str) -> bool {
    let Ok(u) = url::Url::parse(url.trim()) else {
        return false;
    };
    match u.host() {
        Some(url::Host::Domain(d)) => !d.contains('.') && !d.is_empty(),
        Some(url::Host::Ipv4(ip)) => ip.is_private() || ip.is_loopback() || ip.is_link_local(),
        Some(url::Host::Ipv6(ip)) => {
            ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local()
        }
        None => false,
    }
}

/// [`release_refusals`] as an error.
pub fn check(cfg: &Config, env: &dyn Fn(&str) -> Option<String>) -> anyhow::Result<()> {
    let r = release_refusals(cfg, env);
    if r.is_empty() {
        return Ok(());
    }
    anyhow::bail!(
        "this release build refuses to start with test-only settings:\n  - {}",
        r.join("\n  - ")
    )
}

/// At startup: a release build stops on any refusal; a debug build starts as before (the settings still log their
/// own warnings).
pub fn enforce(cfg: &Config) -> anyhow::Result<()> {
    if cfg!(debug_assertions) {
        return Ok(());
    }
    check(cfg, &|k| std::env::var(k).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_core::config::{ActivityOriginMode, Secret};

    const KEY: &str = "1111111111111111111111111111111111111111111111111111111111111111";

    fn ok_cfg() -> Config {
        let mut c = Config::default();
        c.server.secret_key = Secret::new(KEY);
        c.storage.endpoint = "http://garage:3900".into();
        c
    }

    fn none(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn a_production_config_starts() {
        assert!(release_refusals(&ok_cfg(), &none).is_empty());
        assert!(check(&ok_cfg(), &none).is_ok());
    }

    #[test]
    fn each_test_only_setting_is_refused_by_name() {
        type Set = Box<dyn Fn(&mut Config)>;
        let cases: Vec<(&str, Set)> = vec![
            (
                "server.secret_key",
                Box::new(|c| c.server.secret_key = Secret::default()),
            ),
            (
                "unfurl.allow_private_for_tests",
                Box::new(|c| c.unfurl.allow_private_for_tests = true),
            ),
            (
                "storage.endpoint",
                Box::new(|c| c.storage.endpoint = "memory:".into()),
            ),
            (
                "activities.origin_mode",
                Box::new(|c| c.activities.origin_mode = ActivityOriginMode::SameHostSandboxed),
            ),
            (
                "server.metrics_bind",
                Box::new(|c| c.server.metrics_bind = "0.0.0.0:9100".into()),
            ),
        ];
        for (key, set) in cases {
            let mut c = ok_cfg();
            set(&mut c);
            let r = release_refusals(&c, &none);
            assert_eq!(r.len(), 1, "{key}: {r:?}");
            assert!(r[0].starts_with(key), "{key}: {r:?}");
            let e = check(&c, &none).unwrap_err().to_string();
            assert!(e.contains(key), "{e}");
        }
    }

    #[test]
    fn a_metrics_token_allows_a_wide_metrics_bind() {
        let mut c = ok_cfg();
        c.server.metrics_bind = "0.0.0.0:9100".into();
        c.server.metrics_token = Secret::new("t0ken");
        assert!(release_refusals(&c, &none).is_empty());
        for bind in ["127.0.0.1:9100", "[::1]:9100", "localhost:9100"] {
            let mut c = ok_cfg();
            c.server.metrics_bind = bind.into();
            assert!(release_refusals(&c, &none).is_empty(), "{bind}");
        }
    }

    #[test]
    fn the_payments_private_opt_in_needs_a_private_esplora() {
        let env_with = |esplora: Option<&'static str>, flag: &'static str| {
            move |k: &str| match k {
                "SR_PAYMENTS_ALLOW_PRIVATE_HOSTS" => Some(flag.to_owned()),
                "SR_PAYMENTS_ESPLORA_URL" => esplora.map(str::to_owned),
                _ => None,
            }
        };
        for esplora in [
            "http://electrs:3000",
            "http://10.0.0.5:3000",
            "http://192.168.1.2:3000/api",
            "http://[fd00::1]:3000",
        ] {
            let r = release_refusals(&ok_cfg(), &env_with(Some(esplora), "1"));
            assert!(r.is_empty(), "{esplora}: {r:?}");
        }
        for esplora in [
            None,
            Some(""),
            Some("https://mempool.space/api"),
            Some("http://electrs.example.org:3000"),
            Some("http://8.8.8.8/api"),
        ] {
            let r = release_refusals(&ok_cfg(), &env_with(esplora, "true"));
            assert_eq!(r.len(), 1, "{esplora:?}: {r:?}");
            assert!(r[0].starts_with("SR_PAYMENTS_ALLOW_PRIVATE_HOSTS"));
        }
        // Not set (or set to something other than 1/true): nothing to refuse.
        let r = release_refusals(&ok_cfg(), &env_with(None, "0"));
        assert!(r.is_empty());
    }

    #[test]
    fn this_debug_build_still_starts() {
        // Tests run in a debug build: the refusals are reported but never enforced, so the e2e stack's
        // `memory:` storage and empty key keep working.
        let mut c = Config::default();
        c.storage.endpoint = "memory:".into();
        assert!(!release_refusals(&c, &none).is_empty());
        assert!(enforce(&c).is_ok());
    }
}
