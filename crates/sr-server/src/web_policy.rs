//! The server completes the app policy (SEC-AUD-210, D-682).
//!
//! The release web asset is built once, without the operator's deployment settings, so its `app/csp.txt` is the plain
//! app policy. At start the server adds the origins its own configuration implies, by the rule the web build uses
//! (`cspOptionsFromConfig` in `apps/web/src/security/csp.ts`), plus the operator's extra origins
//! (`SR_CSP_CONNECT`, `SR_CSP_FRAME`, `SR_CSP_MEDIA`). A build made from source with the `VITE_*` values already
//! holds them: nothing is added twice.
//!
//! Pure functions, no I/O. The origins come from the operator's configuration only, never from a request. The merge
//! only adds: no source of the file is removed, reordered or loosened, and the public policies are not touched here.
//! `apps/web/src/security/csp.fixtures.json` holds cases (configuration in, policy out) that the web build's own
//! function and this module both have to agree on.

use std::collections::BTreeMap;

use url::Url;

use crate::web::WebError;

pub const CSP_CONNECT_ENV: &str = "SR_CSP_CONNECT";
pub const CSP_FRAME_ENV: &str = "SR_CSP_FRAME";
pub const CSP_MEDIA_ENV: &str = "SR_CSP_MEDIA";

/// What the origins are computed from: the server's configuration and the operator's variables, as read at start.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PolicyInputs {
    /// `server.public_url`.
    pub public_url: String,
    /// `activities.enabled`.
    pub activities_enabled: bool,
    /// `activities.origin_mode = "same_host_sandboxed"`: the games are framed from this host, no Activities origin.
    pub same_host_activities: bool,
    /// `voice.endpoints` (the SFU's public WebSocket URLs); empty means the site's own host.
    pub voice_endpoints: Vec<String>,
    /// `SR_PEERTUBE_HOSTS`, as written.
    pub peertube_hosts: String,
    /// `SR_MEDIA_HOSTS`, as written.
    pub media_hosts: String,
    /// `SR_CSP_CONNECT`, `SR_CSP_FRAME`, `SR_CSP_MEDIA`, as written (comma separated).
    pub csp_connect: String,
    pub csp_frame: String,
    pub csp_media: String,
}

impl PolicyInputs {
    /// The inputs from a configuration and the process environment.
    pub fn from_env(cfg: &sr_core::config::Config) -> Self {
        let var = |k: &str| std::env::var(k).unwrap_or_default();
        Self {
            public_url: cfg.server.public_url.clone(),
            activities_enabled: cfg.activities.enabled,
            same_host_activities: cfg.activities.same_host(),
            voice_endpoints: cfg.voice.endpoints.clone(),
            peertube_hosts: var(sr_unfurl::embed::PEERTUBE_HOSTS_ENV),
            media_hosts: var(sr_unfurl::embed::MEDIA_HOSTS_ENV),
            csp_connect: var(CSP_CONNECT_ENV),
            csp_frame: var(CSP_FRAME_ENV),
            csp_media: var(CSP_MEDIA_ENV),
        }
    }
}

/// The origins to add, per directive of the app policy. `media` goes to `img-src` and `media-src`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PolicyOrigins {
    pub connect: Vec<String>,
    pub frame: Vec<String>,
    pub media: Vec<String>,
}

impl PolicyOrigins {
    pub fn is_empty(&self) -> bool {
        self.connect.is_empty() && self.frame.is_empty() && self.media.is_empty()
    }

    fn directives(&self) -> [(&'static str, Vec<&String>); 4] {
        [
            ("img-src", self.media.iter().collect()),
            ("media-src", self.media.iter().collect()),
            ("connect-src", self.connect.iter().collect()),
            ("frame-src", self.frame.iter().collect()),
        ]
    }
}

fn push_unique(list: &mut Vec<String>, origin: String) {
    if !list.iter().any(|o| o.eq_ignore_ascii_case(&origin)) {
        list.push(origin);
    }
}

fn is_loopback(host: &str) -> bool {
    let h = host.trim_start_matches("*.");
    h == "localhost" || h.ends_with(".localhost") || h == "127.0.0.1" || h == "[::1]"
}

/// `host[:port]` of a URL as the browser's `URL.host` writes it (lower case, default port left out).
fn host_of(u: &Url) -> Option<String> {
    let h = u.host_str()?;
    Some(match u.port() {
        Some(p) => format!("{h}:{p}"),
        None => h.to_owned(),
    })
}

/// The origins the configuration implies: the Rust twin of `cspOptionsFromConfig` (`csp.ts`). Unusable values add
/// nothing. Same-host Activities mode adds no Activities origin (D-450).
pub fn origins_from_config(inp: &PolicyInputs) -> PolicyOrigins {
    let mut out = PolicyOrigins::default();
    let site = Url::parse(&inp.public_url).ok();
    if inp.activities_enabled
        && !inp.same_host_activities
        && let Some(host) = site
            .as_ref()
            .filter(|s| s.scheme() == "https")
            .and_then(host_of)
    {
        push_unique(&mut out.frame, format!("https://*.activities.{host}"));
    }
    let peertube = sr_unfurl::embed::parse_hosts(&inp.peertube_hosts);
    let media = sr_unfurl::embed::parse_hosts(&inp.media_hosts);
    for h in &peertube {
        push_unique(&mut out.frame, format!("https://{h}"));
    }
    // D-517: hls.js fetches from the listed PeerTube instances and media hosts, exact https origins only.
    for h in peertube.iter().chain(media.iter()) {
        push_unique(&mut out.connect, format!("https://{h}"));
    }
    let site_host = site.as_ref().and_then(host_of);
    for endpoint in &inp.voice_endpoints {
        let Ok(v) = Url::parse(endpoint.trim()) else {
            continue;
        };
        if !matches!(v.scheme(), "http" | "https" | "ws" | "wss") {
            continue;
        }
        let Some(host) = host_of(&v) else { continue };
        if Some(&host) == site_host.as_ref() {
            continue;
        }
        let secure = matches!(v.scheme(), "https" | "wss");
        let (http, ws) = if secure {
            ("https", "wss")
        } else {
            ("http", "ws")
        };
        push_unique(&mut out.connect, format!("{http}://{host}"));
        push_unique(&mut out.connect, format!("{ws}://{host}"));
    }
    out
}

/// One entry of `SR_CSP_*`: `scheme://host[:port]`, scheme `https` or `wss` (`http` or `ws` for a loopback host),
/// an optional leading `*.` label. Anything else is an error message.
fn check_extra_entry(entry: &str) -> Result<(), &'static str> {
    if entry
        .bytes()
        .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
    {
        return Err("it holds a space");
    }
    if entry.contains(['\'', '"', ';', '`']) {
        return Err("it holds a quote or a semicolon");
    }
    let Some((scheme, rest)) = entry.split_once("://") else {
        return Err("it is not scheme://host[:port]");
    };
    let scheme = scheme.to_ascii_lowercase();
    if !matches!(scheme.as_str(), "https" | "wss" | "http" | "ws") {
        return Err(
            "the scheme must be https or wss (http or ws only for a loopback host such as localhost)",
        );
    }
    if rest.contains(['/', '?', '#', '@']) {
        return Err("it must be an origin without a path");
    }
    let (host, port) = match rest.rsplit_once(':') {
        Some((h, p)) if !h.ends_with(':') && !p.contains(']') => (h, Some(p)),
        _ => (rest, None),
    };
    if let Some(p) = port
        && (p.is_empty() || p.len() > 5 || !p.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err("the port is not a number");
    }
    let host = host.to_ascii_lowercase();
    let bare = host.strip_prefix("*.").unwrap_or(&host);
    let host_ok = bare == "[::1]"
        || (!bare.is_empty()
            && bare.len() <= 253
            && bare.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            }));
    if !host_ok {
        return Err("the host is not a host name");
    }
    if matches!(scheme.as_str(), "http" | "ws") && !is_loopback(&host) {
        return Err(
            "the scheme must be https or wss (http or ws only for a loopback host such as localhost)",
        );
    }
    Ok(())
}

/// The entries of one `SR_CSP_*` value. A bad entry is an error that names the variable and the entry.
pub fn parse_extra_origins(var: &str, raw: &str) -> Result<Vec<String>, WebError> {
    let mut out = Vec::new();
    for part in raw.split(',') {
        let entry = part.trim();
        if entry.is_empty() {
            continue;
        }
        check_extra_entry(entry).map_err(|why| WebError::BadCspOrigin {
            var: var.to_owned(),
            entry: entry.to_owned(),
            why: why.to_owned(),
        })?;
        push_unique(&mut out, entry.to_owned());
    }
    Ok(out)
}

/// The warnings for the entries of `SR_PEERTUBE_HOSTS` and `SR_MEDIA_HOSTS` that are not plain host names (D-697):
/// one line per variable that has any, none for a clean or empty one. The entries are dropped by the policy and the
/// unfurler already; this only says so. Never an error: the start goes on.
pub fn host_warnings(inp: &PolicyInputs) -> Vec<String> {
    [
        (sr_unfurl::embed::PEERTUBE_HOSTS_ENV, &inp.peertube_hosts),
        (sr_unfurl::embed::MEDIA_HOSTS_ENV, &inp.media_hosts),
    ]
    .into_iter()
    .filter_map(|(var, raw)| sr_unfurl::embed::host_warning(var, raw))
    .collect()
}

/// Logs [`host_warnings`] once at start and returns them.
pub fn warn_about_hosts(inp: &PolicyInputs) -> Vec<String> {
    let warnings = host_warnings(inp);
    for w in &warnings {
        tracing::warn!("{w}");
    }
    warnings
}

/// Everything the server adds: the configuration's origins, then the operator's extra ones, validated.
pub fn origins_for(inp: &PolicyInputs) -> Result<PolicyOrigins, WebError> {
    let mut o = origins_from_config(inp);
    for e in parse_extra_origins(CSP_CONNECT_ENV, &inp.csp_connect)? {
        push_unique(&mut o.connect, e);
    }
    for e in parse_extra_origins(CSP_FRAME_ENV, &inp.csp_frame)? {
        push_unique(&mut o.frame, e);
    }
    for e in parse_extra_origins(CSP_MEDIA_ENV, &inp.csp_media)? {
        push_unique(&mut o.media, e);
    }
    Ok(o)
}

/// The result of a merge: the policy line and what was added, by directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merged {
    pub policy: String,
    pub added: BTreeMap<String, Vec<String>>,
}

/// Adds each origin to its directive when it is not a source there yet. Directive order and every existing source
/// stay as they are; when nothing is added the policy comes back byte for byte. A directive the file lacks is created
/// with `'self'` and the origins (the web build always writes all four: the fixture decides). Nothing is removed.
pub fn merge_origins(policy: &str, origins: &PolicyOrigins) -> Merged {
    let mut directives: Vec<String> = policy
        .split(';')
        .map(|d| d.trim().to_owned())
        .filter(|d| !d.is_empty())
        .collect();
    let mut added: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, list) in origins.directives() {
        if list.is_empty() {
            continue;
        }
        let at = directives
            .iter()
            .position(|d| d.split_whitespace().next() == Some(name));
        let mut new: Vec<String> = Vec::new();
        for origin in list {
            let present = at.is_some_and(|i| {
                directives[i]
                    .split_whitespace()
                    .skip(1)
                    .any(|s| s.eq_ignore_ascii_case(origin))
            });
            if !present && !new.iter().any(|n| n.eq_ignore_ascii_case(origin)) {
                new.push(origin.clone());
            }
        }
        if new.is_empty() {
            continue;
        }
        match at {
            Some(i) => {
                for o in &new {
                    directives[i].push(' ');
                    directives[i].push_str(o);
                }
            }
            None => directives.push(format!("{name} 'self' {}", new.join(" "))),
        }
        added.insert(name.to_owned(), new);
    }
    if added.is_empty() {
        return Merged {
            policy: policy.to_owned(),
            added,
        };
    }
    Merged {
        policy: directives.join("; "),
        added,
    }
}

/// The sources of each directive, lower-cased and sorted: what two policy lines have to agree on.
pub fn sources_by_directive(policy: &str) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    for d in policy.split(';') {
        let mut it = d.split_whitespace();
        let Some(name) = it.next() else { continue };
        let mut src: Vec<String> = it.map(str::to_ascii_lowercase).collect();
        src.sort();
        src.dedup();
        out.insert(name.to_ascii_lowercase(), src);
    }
    out
}

/// One line for the log: what was added where, or that nothing was.
pub fn describe_added(added: &BTreeMap<String, Vec<String>>) -> String {
    if added.is_empty() {
        return "no origins added".to_owned();
    }
    added
        .iter()
        .map(|(d, o)| format!("{d}: {}", o.join(" ")))
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize, Default)]
    #[serde(rename_all = "camelCase", default)]
    struct CaseConfig {
        public_url: String,
        activities_enabled: bool,
        voice_public_url: String,
        peertube_hosts: String,
        media_hosts: String,
    }

    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct CaseExtra {
        connect: Vec<String>,
        frame: Vec<String>,
        media: Vec<String>,
    }

    #[derive(Deserialize)]
    struct Case {
        name: String,
        #[serde(default)]
        config: CaseConfig,
        #[serde(default)]
        extra: CaseExtra,
        /// `plain` (default): merge into the plain policy. `itself`: merge into `policy` (a build that already
        /// holds the origins), which must come back unchanged.
        #[serde(default)]
        from: String,
        policy: String,
    }

    fn cases() -> Vec<Case> {
        let raw = include_str!("../../../apps/web/src/security/csp.fixtures.json");
        serde_json::from_str(raw).expect("csp.fixtures.json parses")
    }

    fn inputs(c: &Case) -> PolicyInputs {
        PolicyInputs {
            public_url: c.config.public_url.clone(),
            activities_enabled: c.config.activities_enabled,
            same_host_activities: false,
            voice_endpoints: if c.config.voice_public_url.is_empty() {
                vec![]
            } else {
                vec![c.config.voice_public_url.clone()]
            },
            peertube_hosts: c.config.peertube_hosts.clone(),
            media_hosts: c.config.media_hosts.clone(),
            csp_connect: c.extra.connect.join(","),
            csp_frame: c.extra.frame.join(","),
            csp_media: c.extra.media.join(","),
        }
    }

    fn plain() -> String {
        cases()
            .into_iter()
            .find(|c| c.name == "nothing set")
            .expect("the fixture has the plain case")
            .policy
    }

    #[test]
    fn the_merge_gives_the_policy_the_web_build_gives_for_every_fixture_case() {
        let plain = plain();
        let all = cases();
        assert!(all.len() >= 14, "the fixture lost cases");
        for c in &all {
            let base = if c.from == "itself" {
                &c.policy
            } else {
                &plain
            };
            let got = merge_origins(base, &origins_for(&inputs(c)).unwrap());
            assert_eq!(
                sources_by_directive(&got.policy),
                sources_by_directive(&c.policy),
                "case {}: {}",
                c.name,
                got.policy
            );
            if c.from == "itself" {
                assert_eq!(got.policy, c.policy, "case {}: nothing may change", c.name);
                assert!(got.added.is_empty(), "case {}", c.name);
            }
        }
    }

    #[test]
    fn nothing_is_added_twice() {
        let plain = plain();
        let all = cases();
        let everything = all
            .iter()
            .find(|c| c.name == "everything at once")
            .expect("case");
        let o = origins_for(&inputs(everything)).unwrap();
        let once = merge_origins(&plain, &o);
        assert!(!once.added.is_empty());
        let twice = merge_origins(&once.policy, &o);
        assert_eq!(twice.policy, once.policy);
        assert!(twice.added.is_empty());
        // And no directive lists a source twice.
        for (d, src) in once.policy.split(';').filter_map(|d| {
            let mut it = d.split_whitespace();
            it.next().map(|n| (n.to_owned(), it.collect::<Vec<_>>()))
        }) {
            let mut seen = std::collections::HashSet::new();
            for s in src {
                assert!(seen.insert(s.to_ascii_lowercase()), "{d} repeats {s}");
            }
        }
        // The same origin written twice by the operator and by the configuration lands once.
        let inp = PolicyInputs {
            peertube_hosts: "tube.example".into(),
            csp_connect: "https://tube.example, https://TUBE.example".into(),
            csp_frame: "https://tube.example".into(),
            ..PolicyInputs::default()
        };
        let m = merge_origins(&plain, &origins_for(&inp).unwrap());
        assert_eq!(m.policy.matches("https://tube.example").count(), 2);
    }

    #[test]
    fn a_merge_that_adds_nothing_is_the_file_byte_for_byte() {
        let odd = "default-src  'self' ;connect-src 'self';  frame-src 'self'";
        let m = merge_origins(odd, &PolicyOrigins::default());
        assert_eq!(m.policy, odd);
    }

    #[test]
    fn existing_sources_and_the_order_of_directives_stay() {
        let plain = plain();
        let o = PolicyOrigins {
            connect: vec!["https://api.example".into()],
            frame: vec!["https://f.example".into()],
            media: vec!["https://cdn.example".into()],
        };
        let m = merge_origins(&plain, &o);
        let names = |p: &str| {
            p.split(';')
                .map(|d| d.split_whitespace().next().unwrap().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&m.policy), names(&plain));
        let before = sources_by_directive(&plain);
        let after = sources_by_directive(&m.policy);
        for (d, src) in before {
            for s in &src {
                assert!(after[&d].contains(s), "{d} lost {s}");
            }
        }
        // Only the four directives changed.
        let changed: Vec<_> = after
            .iter()
            .filter(|(d, s)| sources_by_directive(&plain)[*d] != **s)
            .map(|(d, _)| d.as_str())
            .collect();
        assert_eq!(
            changed,
            ["connect-src", "frame-src", "img-src", "media-src"]
        );
        // `'self'` stays first in each.
        assert!(m.policy.contains("connect-src 'self' https://api.example"));
    }

    #[test]
    fn a_missing_directive_is_created_with_self() {
        let m = merge_origins(
            "default-src 'self'",
            &PolicyOrigins {
                frame: vec!["https://f.example".into()],
                ..PolicyOrigins::default()
            },
        );
        assert_eq!(
            m.policy,
            "default-src 'self'; frame-src 'self' https://f.example"
        );
    }

    #[test]
    fn same_host_activities_adds_no_activities_origin() {
        let inp = PolicyInputs {
            public_url: "https://chat.example.org".into(),
            activities_enabled: true,
            same_host_activities: true,
            ..PolicyInputs::default()
        };
        assert!(origins_from_config(&inp).is_empty());
        let on = PolicyInputs {
            same_host_activities: false,
            ..inp
        };
        assert_eq!(
            origins_from_config(&on).frame,
            ["https://*.activities.chat.example.org"]
        );
    }

    #[test]
    fn a_bad_extra_origin_is_an_error_naming_the_variable_and_the_entry() {
        for bad in [
            "https://a.example;",
            "https://a.example https://b.example",
            "'self'",
            "https://a.example'",
            "https://a.example/path",
            "https://a.example/",
            "'unsafe-inline'",
            "*",
            "https://*",
            "*.example.com",
            "a.example",
            "http://a.example",
            "ftp://a.example",
            "https://a.example:abc",
            "https://user@a.example",
            "https://",
        ] {
            for var in [CSP_CONNECT_ENV, CSP_FRAME_ENV, CSP_MEDIA_ENV] {
                let err = parse_extra_origins(var, &format!("https://ok.example,{bad}"))
                    .expect_err(bad)
                    .to_string();
                assert!(err.contains(var), "{err}");
                assert!(err.contains(bad), "{err}");
            }
        }
        for ok in [
            "https://a.example",
            "https://*.a.example",
            "wss://voice.example:7443",
            "http://localhost:5173",
            "ws://127.0.0.1:9",
            "http://[::1]:3000",
            "https://A.Example",
        ] {
            assert_eq!(
                parse_extra_origins(CSP_CONNECT_ENV, ok).unwrap(),
                [ok],
                "{ok}"
            );
        }
        assert!(
            parse_extra_origins(CSP_FRAME_ENV, " , ,")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_lan_address_over_http_is_refused_with_the_loopback_message() {
        // D-698: no LAN exception; the message says why.
        for var in [CSP_CONNECT_ENV, CSP_FRAME_ENV, CSP_MEDIA_ENV] {
            let msg = parse_extra_origins(var, "http://192.168.1.10")
                .unwrap_err()
                .to_string();
            assert!(
                msg.contains(var) && msg.contains("http://192.168.1.10"),
                "{msg}"
            );
            assert!(msg.contains("loopback"), "{msg}");
        }
        assert!(parse_extra_origins(CSP_CONNECT_ENV, "ws://192.168.1.10:9").is_err());
    }

    #[test]
    fn host_warnings_name_the_variable_and_each_dropped_entry() {
        let inp = PolicyInputs {
            peertube_hosts: "tube.example.org, http://bad.example, nodot".into(),
            media_hosts: "cdn.example.org,,".into(),
            ..PolicyInputs::default()
        };
        assert_eq!(
            host_warnings(&inp),
            vec![
                "SR_PEERTUBE_HOSTS: \"http://bad.example\", \"nodot\" ignored: not a plain host name (write `video.example.org`, no scheme, no path)"
            ]
        );
        assert!(host_warnings(&PolicyInputs::default()).is_empty());
        let both = PolicyInputs {
            peertube_hosts: "x".into(),
            media_hosts: "y z.example".into(),
            ..PolicyInputs::default()
        };
        let w = host_warnings(&both);
        assert_eq!(w.len(), 2);
        assert!(w[1].starts_with("SR_MEDIA_HOSTS: \"y z.example\" ignored"));
    }

    #[test]
    fn the_start_error_names_the_variable() {
        let inp = PolicyInputs {
            csp_frame: "https://a.example;".into(),
            ..PolicyInputs::default()
        };
        let msg = origins_for(&inp).unwrap_err().to_string();
        assert!(
            msg.contains("SR_CSP_FRAME") && msg.contains("https://a.example;"),
            "{msg}"
        );
    }
}
