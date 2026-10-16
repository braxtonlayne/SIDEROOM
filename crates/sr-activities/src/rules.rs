//! Pure rules of Activities: entry URL and URL-mapping validation, the proxy's mapping of a request path to
//! the application's own server, and the activity host name. No I/O.

use url::Url;

use crate::model::UrlMapping;

/// Most URL mappings an application may declare.
pub const MAX_MAPPINGS: usize = 10;
/// Longest URL, entry or mapping target.
pub const MAX_URL_LEN: usize = 512;
/// Longest mapping prefix.
pub const MAX_PREFIX_LEN: usize = 64;

/// Why `u` cannot be an https URL of an application's server, or `None`: `https`, a host, at most
/// [`MAX_URL_LEN`] characters, no credentials, no fragment, no spaces or control characters.
pub fn https_url_problem(u: &str) -> Option<&'static str> {
    let Some(rest) = u.strip_prefix("https://") else {
        return Some("Must be an https URL.");
    };
    if u.len() > MAX_URL_LEN {
        return Some("Must be 512 characters or fewer.");
    }
    if u.chars().any(|c| c.is_control() || c == ' ' || c == '\\') || u.contains('#') {
        return Some("Must not contain spaces, control characters or a fragment.");
    }
    let authority = rest.split(['/', '?']).next().unwrap_or_default();
    if authority.is_empty() || authority.starts_with(':') {
        return Some("Must name a host.");
    }
    if authority.contains('@') {
        return Some("Must not contain credentials.");
    }
    if Url::parse(u).is_err() {
        return Some("Must be a valid URL.");
    }
    None
}

/// Why `prefix` cannot be a mapping prefix, or `None`: `/name` segments of letters, digits, `-`, `_`, `.`
/// (never `.` or `..` alone), no trailing slash, and not `/` itself (the root is the entry URL).
pub fn prefix_problem(prefix: &str) -> Option<&'static str> {
    if prefix.len() > MAX_PREFIX_LEN {
        return Some("Prefix must be 64 characters or fewer.");
    }
    if prefix == "/" || !prefix.starts_with('/') || prefix.ends_with('/') {
        return Some("Prefix must start with / and name a path, like /api.");
    }
    for seg in prefix[1..].split('/') {
        let ok = !seg.is_empty()
            && seg != "."
            && seg != ".."
            && seg
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
        if !ok {
            return Some("Prefix segments may use letters, digits, - _ . only.");
        }
    }
    None
}

/// The first problem in a mapping list, as text for the owner.
pub fn validate_mappings(m: &[UrlMapping]) -> Result<(), String> {
    if m.len() > MAX_MAPPINGS {
        return Err(format!("At most {MAX_MAPPINGS} URL mappings."));
    }
    for (i, a) in m.iter().enumerate() {
        if let Some(p) = prefix_problem(&a.prefix) {
            return Err(format!("{}: {p}", a.prefix));
        }
        if let Some(p) = https_url_problem(&a.target) {
            return Err(format!("{}: target {p}", a.prefix));
        }
        if a.target.contains('?') {
            return Err(format!("{}: target must not contain a query.", a.prefix));
        }
        if m[..i]
            .iter()
            .any(|b| b.prefix.eq_ignore_ascii_case(&a.prefix))
        {
            return Err(format!("{}: listed twice.", a.prefix));
        }
    }
    Ok(())
}

/// Why the proxy refuses a request path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// A path that tries to leave its mapping (`..`, encoded slashes, control characters).
    BadPath,
    /// The application has no usable entry URL.
    NoEntry,
}

/// Whether the path (without query) is safe to forward.
fn path_is_clean(path: &str) -> bool {
    if !path.starts_with('/') || path.chars().any(|c| c.is_control() || c == '\\') {
        return false;
    }
    let lower = path.to_ascii_lowercase();
    if lower.contains("%2e")
        || lower.contains("%2f")
        || lower.contains("%5c")
        || lower.contains("%00")
    {
        return false;
    }
    !path.split('/').any(|s| s == ".." || s == ".")
}

/// The longest mapping whose prefix covers `path` (whole segments only).
fn mapping_of<'a>(mappings: &'a [UrlMapping], path: &str) -> Option<&'a UrlMapping> {
    mappings
        .iter()
        .filter(|m| path == m.prefix || path.starts_with(&format!("{}/", m.prefix)))
        .max_by_key(|m| m.prefix.len())
}

/// Where a request under a URL mapping goes: `Ok(Some(target))` when `path_and_query` is under a mapping
/// prefix, `Ok(None)` when it is not (the bundle answers), `Err` for an unsafe path.
pub fn mapping_target(
    mappings: &[UrlMapping],
    path_and_query: &str,
) -> Result<Option<Url>, Refusal> {
    let (path, query) = match path_and_query.split_once('?') {
        Some((p, q)) => (p, Some(q).filter(|q| !q.is_empty())),
        None => (path_and_query, None),
    };
    if !path_is_clean(path) {
        return Err(Refusal::BadPath);
    }
    let Some(m) = mapping_of(mappings, path) else {
        return Ok(None);
    };
    let t = Url::parse(&m.target).map_err(|_| Refusal::NoEntry)?;
    let base = t.path().trim_end_matches('/').to_owned();
    let mut u = t;
    u.set_path(&format!("{base}{}", &path[m.prefix.len()..]));
    u.set_query(query);
    u.set_fragment(None);
    Ok(Some(u))
}

/// Where the development proxy fetches `path_and_query` from. `/` is the entry URL itself; a path under a mapping prefix
/// goes to the mapping's target (longest prefix wins); anything else is looked up on the entry URL's origin.
pub fn target_for(
    entry_url: &str,
    mappings: &[UrlMapping],
    path_and_query: &str,
) -> Result<Url, Refusal> {
    let entry = Url::parse(entry_url).map_err(|_| Refusal::NoEntry)?;
    let (path, query) = match path_and_query.split_once('?') {
        Some((p, q)) => (p, Some(q).filter(|q| !q.is_empty())),
        None => (path_and_query, None),
    };
    if !path_is_clean(path) {
        return Err(Refusal::BadPath);
    }
    if path == "/" {
        // The entry document is exactly the URL the owner set.
        return Ok(entry);
    }
    let best = mapping_of(mappings, path);
    let mut u = match best {
        Some(m) => {
            let rest = &path[m.prefix.len()..];
            let t = Url::parse(&m.target).map_err(|_| Refusal::NoEntry)?;
            let base = t.path().trim_end_matches('/').to_owned();
            let mut u = t;
            u.set_path(&format!("{base}{rest}"));
            u
        }
        None => {
            let mut u = entry;
            u.set_path(path);
            u
        }
    };
    u.set_query(query);
    u.set_fragment(None);
    Ok(u)
}

/// The application id of an activity host `<id>.activities.<base>`, where `base` is the public host
/// (with its port, if any). `None` for any other host.
pub fn app_of_host(host: &str, base: &str) -> Option<i64> {
    let host = host.to_ascii_lowercase();
    let suffix = format!(".activities.{}", base.to_ascii_lowercase());
    let id = host.strip_suffix(&suffix)?;
    if id.is_empty() || id.len() > 19 || !id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    id.parse::<i64>().ok().filter(|v| *v > 0)
}

/// Whether `host` is under `.activities.<base>` at all (an unknown application, or activities off).
pub fn is_activity_host(host: &str, base: &str) -> bool {
    host.to_ascii_lowercase()
        .ends_with(&format!(".activities.{}", base.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(p: &str, t: &str) -> UrlMapping {
        UrlMapping {
            prefix: p.into(),
            target: t.into(),
        }
    }

    #[test]
    fn urls() {
        assert_eq!(
            https_url_problem("https://app.example.com/index.html"),
            None
        );
        assert!(https_url_problem("http://a.example").is_some());
        assert!(https_url_problem("https://u:p@a.example").is_some());
        assert!(https_url_problem("https://a.example/#x").is_some());
        assert!(https_url_problem("https://").is_some());
        assert!(https_url_problem(&format!("https://a.example/{}", "x".repeat(600))).is_some());
    }

    #[test]
    fn prefixes_and_lists() {
        assert_eq!(prefix_problem("/api"), None);
        assert_eq!(prefix_problem("/a/b-c_d.e"), None);
        for bad in ["/", "api", "/api/", "/..", "/a//b", "/a b", "/a%2fb"] {
            assert!(prefix_problem(bad).is_some(), "{bad}");
        }
        assert!(validate_mappings(&[m("/api", "https://api.example.com")]).is_ok());
        assert!(
            validate_mappings(&[
                m("/api", "https://a.example"),
                m("/API", "https://b.example")
            ])
            .is_err()
        );
        assert!(validate_mappings(&[m("/api", "https://a.example/x?y=1")]).is_err());
        let many: Vec<_> = (0..11)
            .map(|i| m(&format!("/p{i}"), "https://a.example"))
            .collect();
        assert!(validate_mappings(&many).is_err());
    }

    #[test]
    fn targets() {
        let maps = [
            m("/api", "https://api.example.com/v1/"),
            m("/api/long", "https://long.example.com"),
        ];
        let e = "https://app.example.com/game/index.html";
        let t = |p| target_for(e, &maps, p).map(|u| u.to_string());
        assert_eq!(t("/").unwrap(), e);
        assert_eq!(t("/main.js").unwrap(), "https://app.example.com/main.js");
        assert_eq!(
            t("/api/users?x=1").unwrap(),
            "https://api.example.com/v1/users?x=1"
        );
        assert_eq!(t("/api").unwrap(), "https://api.example.com/v1");
        assert_eq!(t("/api/long/a").unwrap(), "https://long.example.com/a");
        // A prefix only matches whole segments.
        assert_eq!(t("/apix").unwrap(), "https://app.example.com/apix");
        for bad in ["/api/../x", "/a/%2e%2e/b", "/a%2Fb", "/a\\b", "/./x"] {
            assert_eq!(t(bad), Err(Refusal::BadPath), "{bad}");
        }
    }

    #[test]
    fn mapping_targets_only_under_a_prefix() {
        let maps = [m("/api", "https://api.example.com/v1/")];
        let t = |p| mapping_target(&maps, p).map(|u| u.map(|u| u.to_string()));
        assert_eq!(t("/main.js").unwrap(), None);
        assert_eq!(t("/").unwrap(), None);
        assert_eq!(t("/apix").unwrap(), None);
        assert_eq!(
            t("/api/rooms?x=1").unwrap().as_deref(),
            Some("https://api.example.com/v1/rooms?x=1")
        );
        assert_eq!(t("/api").unwrap().as_deref(), Some("https://api.example.com/v1"));
        for bad in ["/api/../x", "/api/%2e%2e/x", "/api%2Fx", "/api\\x", "/../api"] {
            assert_eq!(t(bad), Err(Refusal::BadPath), "{bad}");
        }
    }

    #[test]
    fn hosts() {
        let base = "chat.example.org";
        assert_eq!(
            app_of_host("123.activities.chat.example.org", base),
            Some(123)
        );
        assert_eq!(
            app_of_host("123.ACTIVITIES.Chat.example.org", base),
            Some(123)
        );
        assert_eq!(app_of_host("12a.activities.chat.example.org", base), None);
        assert_eq!(app_of_host("chat.example.org", base), None);
        assert_eq!(app_of_host("a.b.activities.chat.example.org", base), None);
        assert_eq!(app_of_host("0.activities.chat.example.org", base), None);
        assert!(is_activity_host("x.activities.chat.example.org", base));
        assert!(!is_activity_host("chat.example.org", base));
    }
}
