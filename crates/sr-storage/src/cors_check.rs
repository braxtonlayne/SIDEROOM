//! The CORS checker and applier of a storage mount (BYOS-MNT-003).
//!
//! The required rule is the one of [`crate::s3admin::cors_xml`] for one origin (`server.public_url`, never a
//! wildcard): methods `GET`, `HEAD`, `PUT`; allowed header `content-type`; exposed headers `ETag`,
//! `Content-Range`, `Accept-Ranges`, `Content-Length`. A bucket passes when one of its rules names exactly
//! that origin and covers all of those. Everything else is `different` (with the rules it has) or `missing`.
//! Providers whose S3 CORS API is not implemented (B2 in some regions answers 501) get `manual` with the
//! provider's own steps.

use crate::mounts::Provider;
use crate::s3admin::{
    CORS_ALLOWED_HEADERS, CORS_EXPOSE_HEADERS, CORS_METHODS, S3Admin, S3Reply, normalize_origin,
};

/// Result of a check or an apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorsState {
    /// The bucket allows the origin as required.
    Ok,
    /// The bucket has no CORS rules.
    Missing,
    /// It has rules, but none covers the origin as required.
    Different,
    /// The service does not offer the CORS API: set the rule in its console.
    Manual,
}

impl CorsState {
    /// The wire value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Missing => "missing",
            Self::Different => "different",
            Self::Manual => "manual",
        }
    }
}

/// What a check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorsReport {
    /// The verdict.
    pub state: CorsState,
    /// The required rule, as lines of text.
    pub required: Vec<String>,
    /// The rules the bucket has, as lines of text (empty when none or unreadable).
    pub found: Vec<String>,
    /// Provider steps for [`CorsState::Manual`].
    pub manual_steps: Option<String>,
}

/// One `CORSRule`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rule {
    /// `AllowedOrigin` values.
    pub origins: Vec<String>,
    /// `AllowedMethod` values.
    pub methods: Vec<String>,
    /// `AllowedHeader` values.
    pub headers: Vec<String>,
    /// `ExposeHeader` values.
    pub expose: Vec<String>,
}

impl Rule {
    /// One line for the UI.
    pub fn describe(&self) -> String {
        format!(
            "origin {} | methods {} | headers {} | exposes {}",
            self.origins.join(", "),
            self.methods.join(", "),
            self.headers.join(", "),
            self.expose.join(", ")
        )
    }
}

fn all_tags(xml: &str, tag: &str) -> Vec<String> {
    let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find(&open) {
        let after = &rest[i + open.len()..];
        let Some(j) = after.find(&close) else { break };
        out.push(unescape(&after[..j]));
        rest = &after[j + close.len()..];
    }
    out
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The rules of a `CORSConfiguration` document.
pub fn parse_rules(xml: &str) -> Vec<Rule> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<CORSRule>") {
        let after = &rest[i + "<CORSRule>".len()..];
        let Some(j) = after.find("</CORSRule>") else {
            break;
        };
        let body = &after[..j];
        out.push(Rule {
            origins: all_tags(body, "AllowedOrigin"),
            methods: all_tags(body, "AllowedMethod"),
            headers: all_tags(body, "AllowedHeader"),
            expose: all_tags(body, "ExposeHeader"),
        });
        rest = &after[j + "</CORSRule>".len()..];
    }
    out
}

/// The rule a mount's bucket must have for `origin`.
pub fn required_rule(origin: &str) -> Rule {
    Rule {
        origins: vec![origin.to_owned()],
        methods: CORS_METHODS.iter().map(|s| (*s).to_owned()).collect(),
        headers: CORS_ALLOWED_HEADERS
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
        expose: CORS_EXPOSE_HEADERS
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
    }
}

fn has_all(have: &[String], want: &[String]) -> bool {
    want.iter()
        .all(|w| have.iter().any(|h| h.eq_ignore_ascii_case(w)))
}

fn covers(rule: &Rule, required: &Rule) -> bool {
    // The origin must be named; a wildcard origin does not count (D-175).
    rule.origins.iter().any(|o| {
        o.trim_end_matches('/')
            .eq_ignore_ascii_case(&required.origins[0])
    }) && has_all(&rule.methods, &required.methods)
        && (rule.headers.iter().any(|h| h == "*") || has_all(&rule.headers, &required.headers))
        && has_all(&rule.expose, &required.expose)
}

/// Compare a bucket's CORS document (`None`: no configuration) with the required rule for `origin`.
pub fn compare(origin: &str, found_xml: Option<&str>) -> CorsReport {
    let required = required_rule(origin);
    let rules = found_xml.map(parse_rules).unwrap_or_default();
    let state = if rules.is_empty() {
        CorsState::Missing
    } else if rules.iter().any(|r| covers(r, &required)) {
        CorsState::Ok
    } else {
        CorsState::Different
    };
    CorsReport {
        state,
        required: vec![required.describe()],
        found: rules.iter().map(Rule::describe).collect(),
        manual_steps: None,
    }
}

/// The provider's own steps for setting the rule by hand.
pub fn manual_steps(provider: Provider, origin: &str) -> String {
    let rule = format!(
        "Allow origin {origin}; methods GET, HEAD, PUT; allowed header content-type; exposed headers ETag, Content-Range, Accept-Ranges, Content-Length."
    );
    let how = match provider {
        Provider::B2 => {
            "In the Backblaze B2 console open Buckets, your bucket, Bucket Settings, CORS Rules, and choose to share with a specific origin, or use the b2 command line tool to set the rule."
        }
        Provider::R2 => {
            "In the Cloudflare dashboard open R2, your bucket, Settings, CORS Policy and add a rule."
        }
        Provider::Wasabi => {
            "In the Wasabi console open the bucket, Settings, CORS Configuration and add a rule."
        }
        Provider::S3 => {
            "In the AWS console open the bucket, Permissions, Cross-origin resource sharing (CORS) and add a rule."
        }
        Provider::Minio | Provider::Garage => {
            "Set the rule with your server's own tooling (for example the S3 API of your deployment)."
        }
    };
    format!("{how} {rule}")
}

fn unsupported(r: &S3Reply) -> bool {
    matches!(r.status, 501 | 405) || r.code().as_deref() == Some("NotImplemented")
}

/// Read the bucket's CORS rules and compare them with the required rule for `origin`.
pub async fn check(admin: &S3Admin, provider: Provider, origin: &str) -> crate::Result<CorsReport> {
    let origin = normalize_origin(origin)?;
    let reply = admin.get_bucket_cors_raw().await?;
    if reply.ok() {
        return Ok(compare(&origin, Some(&reply.body)));
    }
    if reply.status == 404 && reply.body.contains("NoSuchCORSConfiguration") {
        return Ok(compare(&origin, None));
    }
    if unsupported(&reply) {
        return Ok(manual_or_verified(admin, provider, &origin).await);
    }
    Err(crate::StorageError::Backend(format!(
        "reading the bucket's CORS rules failed: {}",
        reply.reason()
    )))
}

/// For a service without the CORS API: if a live preflight already succeeds, the rule is in place (the owner
/// set it by hand); otherwise the provider's steps.
async fn manual_or_verified(admin: &S3Admin, provider: Provider, origin: &str) -> CorsReport {
    let allowed = admin
        .preflight_allows(
            crate::probe::PROBE_NAME,
            origin,
            std::time::Duration::from_secs(5),
        )
        .await
        .unwrap_or(false);
    if allowed {
        return CorsReport {
            state: CorsState::Ok,
            required: vec![required_rule(origin).describe()],
            found: vec![
                "Verified with a live preflight request; this service has no CORS API.".to_owned(),
            ],
            manual_steps: None,
        };
    }
    manual_report(provider, origin)
}

fn manual_report(provider: Provider, origin: &str) -> CorsReport {
    CorsReport {
        state: CorsState::Manual,
        required: vec![required_rule(origin).describe()],
        found: Vec::new(),
        manual_steps: Some(manual_steps(provider, origin)),
    }
}

/// Write the required rule for `origin` and check again. A service that refuses the API answers `manual`.
pub async fn apply(admin: &S3Admin, provider: Provider, origin: &str) -> crate::Result<CorsReport> {
    let origin = normalize_origin(origin)?;
    let put = admin
        .put_bucket_cors_raw(std::slice::from_ref(&origin))
        .await?;
    if !put.ok() {
        if unsupported(&put) {
            return Ok(manual_or_verified(admin, provider, &origin).await);
        }
        return Err(crate::StorageError::Backend(format!(
            "writing the bucket's CORS rules failed: {}",
            put.reason()
        )));
    }
    check(admin, provider, &origin).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::s3admin::cors_xml;

    const ORIGIN: &str = "https://chat.example.org";

    #[test]
    fn the_document_we_write_passes_our_check() {
        let xml = cors_xml(&[ORIGIN.to_owned()]).unwrap();
        let rules = parse_rules(&xml);
        assert_eq!(rules.len(), 1);
        assert_eq!(compare(ORIGIN, Some(&xml)).state, CorsState::Ok);
    }

    #[test]
    fn none_is_missing_and_other_rules_are_different() {
        assert_eq!(compare(ORIGIN, None).state, CorsState::Missing);
        assert_eq!(
            compare(ORIGIN, Some("<CORSConfiguration/>")).state,
            CorsState::Missing
        );
        let other = cors_xml(&["https://other.example.org".to_owned()]).unwrap();
        let rep = compare(ORIGIN, Some(&other));
        assert_eq!(rep.state, CorsState::Different);
        assert_eq!(rep.found.len(), 1);
        assert!(rep.found[0].contains("other.example.org"));
        assert!(rep.required[0].contains("chat.example.org"));
    }

    #[test]
    fn a_wildcard_or_a_short_rule_is_different() {
        let wildcard = "<CORSConfiguration><CORSRule><AllowedOrigin>*</AllowedOrigin><AllowedMethod>GET</AllowedMethod></CORSRule></CORSConfiguration>";
        assert_eq!(compare(ORIGIN, Some(wildcard)).state, CorsState::Different);
        // Right origin but no PUT and no exposed range headers.
        let short = format!(
            "<CORSConfiguration><CORSRule><AllowedOrigin>{ORIGIN}</AllowedOrigin><AllowedMethod>GET</AllowedMethod><AllowedMethod>HEAD</AllowedMethod><AllowedHeader>content-type</AllowedHeader><ExposeHeader>ETag</ExposeHeader></CORSRule></CORSConfiguration>"
        );
        assert_eq!(compare(ORIGIN, Some(&short)).state, CorsState::Different);
    }

    #[test]
    fn header_case_and_a_wildcard_allowed_header_are_accepted() {
        let xml = format!(
            "<CORSConfiguration><CORSRule><AllowedOrigin>{ORIGIN}</AllowedOrigin>\
             <AllowedMethod>get</AllowedMethod><AllowedMethod>HEAD</AllowedMethod><AllowedMethod>PUT</AllowedMethod>\
             <AllowedHeader>*</AllowedHeader>\
             <ExposeHeader>etag</ExposeHeader><ExposeHeader>content-range</ExposeHeader>\
             <ExposeHeader>accept-ranges</ExposeHeader><ExposeHeader>content-length</ExposeHeader>\
             </CORSRule></CORSConfiguration>"
        );
        assert_eq!(compare(ORIGIN, Some(&xml)).state, CorsState::Ok);
    }

    #[test]
    fn manual_steps_name_the_provider() {
        assert!(manual_steps(Provider::B2, ORIGIN).contains("Backblaze"));
        assert!(manual_steps(Provider::R2, ORIGIN).contains("Cloudflare"));
        assert!(manual_steps(Provider::Garage, ORIGIN).contains(ORIGIN));
    }
}
