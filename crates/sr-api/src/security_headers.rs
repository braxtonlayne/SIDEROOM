//! Baseline security response headers for the API listener (XC-SEC-001).
//!
//! The single-page app's own document headers (CSP with fonts, `/cdn` images,
//! the voice WebSocket and WASM) are set by the reverse proxy (see
//! `deploy/compose/caddy/Caddyfile.prod`) and by the Vite dev/preview servers;
//! the same policy lives in `apps/web/src/security/csp.ts`. This layer covers
//! everything `sr-server` itself answers: JSON API, gateway upgrade, health
//! endpoints. Handlers that set a header themselves (the `/cdn` routes send a
//! sandboxing CSP) are never overridden.

use axum::{
    extract::{Request, State},
    http::{HeaderName, HeaderValue, header},
    middleware::Next,
    response::Response,
};

/// Policy for responses that are data, not documents: nothing may load, and
/// nobody may frame them.
const API_CSP: &str =
    "default-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";
/// One year, subdomains included. Only sent when the public URL is https.
const HSTS: &str = "max-age=31536000; includeSubDomains";
/// Features the app never uses on the API origin; `camera`/`microphone`/
/// `display-capture` are allowed for the page itself via the proxy policy.
const PERMISSIONS: &str = "accelerometer=(), geolocation=(), gyroscope=(), magnetometer=(), payment=(), usb=(), interest-cohort=()";

/// Settings for [`security_headers`].
#[derive(Debug, Clone, Copy)]
pub struct SecurityHeaderConfig {
    /// Send `Strict-Transport-Security` (the public URL is https).
    pub hsts: bool,
}

/// Adds the baseline headers to every response that does not already carry them.
pub async fn security_headers(
    State(cfg): State<SecurityHeaderConfig>,
    req: Request,
    next: Next,
) -> Response {
    // The API reference page is a script-bearing document; give it no CSP of ours.
    let is_docs = req.uri().path().starts_with("/developers/");
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    let mut set = |name: HeaderName, value: &'static str| {
        if !h.contains_key(&name) {
            h.insert(name, HeaderValue::from_static(value));
        }
    };
    set(header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    // OPS-IDX-001 (D-384): only the landing page's HTML is indexable and that is served by the proxy or the static
    // host, never by sr-server: everything this layer covers (API, /cdn, /developers, health) is noindex.
    set(HeaderName::from_static("x-robots-tag"), "noindex, nofollow");
    set(header::REFERRER_POLICY, "no-referrer");
    set(HeaderName::from_static("permissions-policy"), PERMISSIONS);
    set(
        HeaderName::from_static("cross-origin-opener-policy"),
        "same-origin",
    );
    set(
        HeaderName::from_static("cross-origin-resource-policy"),
        "same-site",
    );
    if !is_docs {
        set(header::CONTENT_SECURITY_POLICY, API_CSP);
    }
    if cfg.hsts {
        set(header::STRICT_TRANSPORT_SECURITY, HSTS);
    }
    res
}

#[cfg(test)]
mod tests {
    use super::API_CSP;

    /// The app policy is built by `apps/web/src/security/csp.ts` and sent by `sr-server` from `dist/app/csp.txt`
    /// (wave 30b). PRF-UNF-001 adds exactly two framed origins and `media-src https:` there, nowhere else: this pins
    /// the hosts to the ones `sr-unfurl` produces embeds for, so the offer and the policy cannot drift apart.
    const CSP_TS: &str = include_str!("../../../apps/web/src/security/csp.ts");
    const WEB_RS: &str = include_str!("../../sr-server/src/web.rs");

    fn const_list(src: &str, name: &str) -> Vec<String> {
        let start = src.find(&format!("export const {name} = [")).expect(name);
        let rest = &src[start..];
        let body = &rest[rest.find('[').unwrap() + 1..rest.find(']').unwrap()];
        body.split(',')
            .map(|s| s.trim().trim_matches('\''))
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn the_app_policy_frames_exactly_the_hosts_unfurl_makes_embeds_for() {
        let origins = const_list(CSP_TS, "EMBED_FRAME_ORIGINS");
        assert_eq!(
            origins,
            vec![
                format!("https://{}", sr_unfurl::embed::YOUTUBE_EMBED_HOST),
                format!("https://{}", sr_unfurl::embed::VIMEO_EMBED_HOST),
            ]
        );
        assert_eq!(origins[0], "https://www.youtube-nocookie.com");
        assert_eq!(origins[1], "https://player.vimeo.com");
        // The directive is built from that list, in front of the optional extras.
        assert!(CSP_TS.contains(
            "['frame-src', join([\"'self'\", ...EMBED_FRAME_ORIGINS], opts.extraFrame)]"
        ));
        assert!(
            CSP_TS
                .contains("['media-src', join([\"'self'\", 'blob:', 'https:'], opts.extraMedia)]")
        );
    }

    #[test]
    fn no_other_policy_frames_or_loads_media() {
        // The API answers data, not documents.
        assert!(!API_CSP.contains("frame-src") && !API_CSP.contains("media-src"));
        // The public and no-script policies (the Rust copies pinned to csp.ts by csp.test.ts) carry neither.
        for name in ["PUBLIC_CSP", "NO_SCRIPT_CSP"] {
            let at = WEB_RS
                .find(&format!("pub const {name}: &str = \""))
                .expect(name);
            let line = WEB_RS[at..].lines().next().unwrap();
            for banned in ["frame-src", "media-src", "youtube", "vimeo", "https:"] {
                assert!(!line.contains(banned), "{name} contains {banned}");
            }
        }
    }
}
