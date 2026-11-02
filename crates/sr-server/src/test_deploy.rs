//! OPS-TDEP-001 (D-450 to D-452, threat model T49): the startup warnings of a test deployment.
//!
//! `[activities] origin_mode = "same_host_sandboxed"` serves every Activity from `/_activity/<id>/` on the main host,
//! for a short test behind one host name (a cloudflared quick tunnel, docs/ops/test-deploy-quick-tunnel.md). It is
//! set from the config file or `SR_ACTIVITIES__ORIGIN_MODE` only. The server says so loudly at startup, and warns
//! when it trusts `X-Forwarded-For` while listening beyond loopback (then any client could pick its own address for
//! the rate limits).

use sr_core::config::Config;

/// Whether `bind` (`host:port`) listens on loopback only.
fn loopback_only(bind: &str) -> bool {
    if let Ok(a) = bind.parse::<std::net::SocketAddr>() {
        return a.ip().is_loopback();
    }
    bind.rsplit_once(':')
        .is_some_and(|(h, _)| h.eq_ignore_ascii_case("localhost"))
}

/// The warnings to log at startup, one line each (empty for a normal deployment).
pub fn startup_warnings(cfg: &Config) -> Vec<String> {
    let mut out = Vec::new();
    if cfg.activities.same_host() {
        out.push(
            "TEST DEPLOYMENT: activities.origin_mode = same_host_sandboxed. Activities are served from /_activity/<id>/ \
             on the main host (sandboxed, opaque origin) instead of their own origins. Games share this host's origin \
             namespace; use this only for a short test, never in production (OPS-TDEP-001, D-450, threat model T49)."
                .to_owned(),
        );
        if !cfg.server.public_url.starts_with("https://") {
            out.push(format!(
                "TEST DEPLOYMENT: server.public_url is {:?}; behind a tunnel set SR_SERVER__PUBLIC_URL to the tunnel's \
                 https address, or sign-in (CSRF origin check), cookies and the activity frame policy will not match.",
                cfg.server.public_url
            ));
        }
    }
    if cfg.server.trust_forwarded_for && !loopback_only(&cfg.server.bind) {
        out.push(format!(
            "server.trust_forwarded_for is on while listening on {}: any client that reaches this address can choose \
             its own X-Forwarded-For. Bind to 127.0.0.1 behind a tunnel or proxy.",
            cfg.server.bind
        ));
    }
    out
}

/// Log [`startup_warnings`] at WARN.
pub fn log_startup_warnings(cfg: &Config) {
    for w in startup_warnings(cfg) {
        tracing::warn!("{w}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_core::config::ActivityOriginMode;

    #[test]
    fn a_normal_deployment_has_no_warning() {
        assert!(startup_warnings(&Config::default()).is_empty());
    }

    #[test]
    fn the_same_host_mode_warns_and_names_the_tunnel_url() {
        let mut cfg = Config::default();
        cfg.activities.origin_mode = ActivityOriginMode::SameHostSandboxed;
        cfg.server.public_url = "http://127.0.0.1:8080".into();
        let w = startup_warnings(&cfg);
        assert_eq!(w.len(), 2, "{w:?}");
        assert!(w[0].starts_with("TEST DEPLOYMENT"));
        assert!(w[0].contains("same_host_sandboxed"));
        assert!(w[1].contains("SR_SERVER__PUBLIC_URL"));
        cfg.server.public_url = "https://abc.trycloudflare.com".into();
        assert_eq!(startup_warnings(&cfg).len(), 1);
    }

    #[test]
    fn trusting_forwarded_headers_beyond_loopback_warns() {
        let mut cfg = Config::default();
        cfg.server.trust_forwarded_for = true;
        for bind in ["127.0.0.1:8080", "[::1]:8080", "localhost:8080"] {
            cfg.server.bind = bind.into();
            assert!(startup_warnings(&cfg).is_empty(), "{bind}");
        }
        for bind in ["0.0.0.0:8080", "192.168.1.5:8080", "[::]:8080"] {
            cfg.server.bind = bind.into();
            assert_eq!(startup_warnings(&cfg).len(), 1, "{bind}");
        }
    }
}
