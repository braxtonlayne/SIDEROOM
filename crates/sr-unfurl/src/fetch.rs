//! The hardened fetcher: validate, resolve, pin, connect, re-validate on redirects.
//!
//! [`SafeFetcher`] owns the policy. DNS ([`Resolver`]) and the HTTP exchange of a
//! single hop ([`HttpTransport`]) are injected so the SSRF rules can be tested
//! without a network. The real transport ([`crate::transport::ReqwestTransport`])
//! connects to the exact addresses that passed validation, never to a name it
//! would resolve again (no DNS-rebinding window).

use crate::policy::{Blocked, Policy};
use async_trait::async_trait;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use url::{Host, Url};

/// Value of the `User-Agent` header on every outgoing request.
pub const USER_AGENT: &str = "SIDEROOM-Unfurl";
/// Redirects followed before giving up.
pub const MAX_REDIRECTS: usize = 3;
/// TCP/TLS connect timeout.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Whole-fetch timeout (resolution, redirects and body).
pub const TOTAL_TIMEOUT: Duration = Duration::from_secs(10);
/// Largest HTML document read.
pub const MAX_HTML_BYTES: usize = 2 * 1024 * 1024;
/// Images larger than this (by `Content-Length`) are refused.
pub const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
/// How much of an image is read to learn its dimensions.
pub const IMAGE_HEAD_BYTES: usize = 256 * 1024;
/// Largest oEmbed JSON document read.
pub const MAX_JSON_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FetchError {
    #[error(transparent)]
    Blocked(#[from] Blocked),
    #[error("invalid URL: {0}")]
    BadUrl(String),
    #[error("DNS resolution failed: {0}")]
    Resolve(String),
    #[error("request failed: {0}")]
    Http(String),
    #[error("timed out")]
    Timeout,
    #[error("too many redirects")]
    TooManyRedirects,
    #[error("redirect without a usable Location")]
    BadRedirect,
    #[error("unexpected status {0}")]
    BadStatus(u16),
    #[error("content type is not allowed")]
    UnsupportedType,
    #[error("response is too large")]
    TooLarge,
}

/// What kind of body a response carries; decides the size cap and the parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyClass {
    Html,
    Image,
    Video,
    Json,
}

/// Content-type allow-list. `None` means the type is refused. SVG is refused: it
/// can carry script and is not needed for previews.
pub fn classify(content_type: Option<&str>) -> Option<BodyClass> {
    let ct = content_type?.split(';').next()?.trim().to_ascii_lowercase();
    match ct.as_str() {
        "text/html" | "application/xhtml+xml" => Some(BodyClass::Html),
        "image/png" | "image/jpeg" | "image/jpg" | "image/gif" | "image/webp" | "image/avif"
        | "image/bmp" => Some(BodyClass::Image),
        "video/mp4" | "video/webm" | "video/ogg" | "video/quicktime" => Some(BodyClass::Video),
        "application/json" | "text/json" | "application/json+oembed" | "text/xml+oembed" => {
            Some(BodyClass::Json)
        }
        s if s.ends_with("+json") => Some(BodyClass::Json),
        _ => None,
    }
}

/// Resolves a host name to addresses.
#[async_trait]
pub trait Resolver: Send + Sync {
    async fn resolve(&self, host: &str, port: u16) -> Result<Vec<IpAddr>, String>;
}

/// The operating system resolver.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemResolver;

#[async_trait]
impl Resolver for SystemResolver {
    async fn resolve(&self, host: &str, port: u16) -> Result<Vec<IpAddr>, String> {
        let addrs = tokio::net::lookup_host((host, port))
            .await
            .map_err(|e| e.to_string())?;
        Ok(addrs.map(|a| a.ip()).collect())
    }
}

/// One outgoing request. `addrs` are the validated addresses to connect to; the
/// transport must not resolve `url`'s host itself.
#[derive(Debug, Clone)]
pub struct HopRequest {
    pub url: Url,
    pub addrs: Vec<SocketAddr>,
    pub accept: &'static str,
}

/// One response, with the body already capped by the transport.
#[derive(Debug, Clone, Default)]
pub struct HopResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub location: Option<String>,
    pub body: Vec<u8>,
}

/// Performs a single HTTP GET without following redirects.
#[async_trait]
pub trait HttpTransport: Send + Sync {
    async fn get(&self, req: &HopRequest) -> Result<HopResponse, FetchError>;
}

/// A successful, validated response.
#[derive(Debug, Clone)]
pub struct Fetched {
    pub final_url: Url,
    pub content_type: Option<String>,
    pub class: BodyClass,
    pub body: Vec<u8>,
}

/// Fetches URLs under the SSRF policy.
#[derive(Clone)]
pub struct SafeFetcher {
    policy: Policy,
    resolver: Arc<dyn Resolver>,
    transport: Arc<dyn HttpTransport>,
}

impl SafeFetcher {
    pub fn new(
        policy: Policy,
        resolver: Arc<dyn Resolver>,
        transport: Arc<dyn HttpTransport>,
    ) -> Self {
        Self {
            policy,
            resolver,
            transport,
        }
    }

    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    /// The validated socket addresses for `url`, or why it is refused.
    async fn validated_addrs(&self, url: &Url) -> Result<Vec<SocketAddr>, FetchError> {
        self.policy.check_url(url)?;
        let port = url
            .port_or_known_default()
            .ok_or_else(|| FetchError::BadUrl("no port".into()))?;
        let ips = match url.host() {
            Some(Host::Ipv4(ip)) => vec![IpAddr::V4(ip)],
            Some(Host::Ipv6(ip)) => vec![IpAddr::V6(ip)],
            Some(Host::Domain(d)) => {
                let d = d.trim_end_matches('.');
                tokio::time::timeout(CONNECT_TIMEOUT, self.resolver.resolve(d, port))
                    .await
                    .map_err(|_| FetchError::Timeout)?
                    .map_err(FetchError::Resolve)?
            }
            None => return Err(Blocked::NoHost.into()),
        };
        self.policy.check_all(&ips)?;
        Ok(ips
            .into_iter()
            .map(|ip| SocketAddr::new(ip, port))
            .collect())
    }

    /// GET `url`, following at most [`MAX_REDIRECTS`] redirects and validating every
    /// hop. The whole call is bounded by [`TOTAL_TIMEOUT`].
    pub async fn fetch(&self, url: &Url, accept: &'static str) -> Result<Fetched, FetchError> {
        tokio::time::timeout(TOTAL_TIMEOUT, self.fetch_inner(url, accept))
            .await
            .map_err(|_| FetchError::Timeout)?
    }

    async fn fetch_inner(&self, url: &Url, accept: &'static str) -> Result<Fetched, FetchError> {
        let mut current = url.clone();
        let mut redirects = 0;
        loop {
            let addrs = self.validated_addrs(&current).await?;
            let resp = self
                .transport
                .get(&HopRequest {
                    url: current.clone(),
                    addrs,
                    accept,
                })
                .await?;
            if matches!(resp.status, 301 | 302 | 303 | 307 | 308) {
                let loc = resp.location.ok_or(FetchError::BadRedirect)?;
                if redirects == MAX_REDIRECTS {
                    return Err(FetchError::TooManyRedirects);
                }
                redirects += 1;
                current = current.join(&loc).map_err(|_| FetchError::BadRedirect)?;
                continue;
            }
            if !(200..300).contains(&resp.status) {
                return Err(FetchError::BadStatus(resp.status));
            }
            let class =
                classify(resp.content_type.as_deref()).ok_or(FetchError::UnsupportedType)?;
            let cap = match class {
                BodyClass::Html => MAX_HTML_BYTES,
                BodyClass::Json => MAX_JSON_BYTES,
                BodyClass::Image => IMAGE_HEAD_BYTES,
                BodyClass::Video => 0,
            };
            let mut body = resp.body;
            // A transport must already cap the body; this is the second line of defence.
            body.truncate(cap);
            return Ok(Fetched {
                final_url: current,
                content_type: resp.content_type,
                class,
                body,
            });
        }
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Resolver that answers from a table; unknown hosts fail.
    #[derive(Default)]
    pub struct TableResolver(pub HashMap<String, Vec<IpAddr>>);

    #[async_trait]
    impl Resolver for TableResolver {
        async fn resolve(&self, host: &str, _port: u16) -> Result<Vec<IpAddr>, String> {
            self.0
                .get(host)
                .cloned()
                .ok_or_else(|| format!("no such host {host}"))
        }
    }

    /// Transport that serves canned responses by URL and records every request.
    #[derive(Default)]
    pub struct ScriptedTransport {
        pub routes: HashMap<String, HopResponse>,
        pub seen: Mutex<Vec<HopRequest>>,
    }

    #[async_trait]
    impl HttpTransport for ScriptedTransport {
        async fn get(&self, req: &HopRequest) -> Result<HopResponse, FetchError> {
            self.seen.lock().unwrap().push(req.clone());
            self.routes
                .get(req.url.as_str())
                .cloned()
                .ok_or_else(|| FetchError::Http(format!("no route for {}", req.url)))
        }
    }

    pub fn html(body: &str) -> HopResponse {
        HopResponse {
            status: 200,
            content_type: Some("text/html; charset=utf-8".into()),
            location: None,
            body: body.as_bytes().to_vec(),
        }
    }

    pub fn redirect(to: &str) -> HopResponse {
        HopResponse {
            status: 302,
            location: Some(to.into()),
            ..HopResponse::default()
        }
    }

    pub fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    fn fetcher(
        hosts: &[(&str, &str)],
        routes: Vec<(&str, HopResponse)>,
    ) -> (SafeFetcher, Arc<ScriptedTransport>) {
        let mut r = TableResolver::default();
        for (h, a) in hosts {
            r.0.entry((*h).to_owned()).or_default().push(ip(a));
        }
        let t = Arc::new(ScriptedTransport {
            routes: routes.into_iter().map(|(u, r)| (u.to_owned(), r)).collect(),
            ..ScriptedTransport::default()
        });
        (
            SafeFetcher::new(Policy::default(), Arc::new(r), t.clone()),
            t,
        )
    }

    async fn get(f: &SafeFetcher, u: &str) -> Result<Fetched, FetchError> {
        f.fetch(&Url::parse(u).unwrap(), "text/html").await
    }

    #[tokio::test]
    async fn ssrf_rejection_table() {
        let (f, t) = fetcher(
            &[
                ("public.test", "93.184.216.34"),
                ("rebind.test", "10.1.2.3"),
                ("meta.test", "169.254.169.254"),
                ("v6.test", "fd00::1"),
                ("redir.test", "93.184.216.35"),
            ],
            vec![
                // A public page that redirects into the private network.
                ("http://redir.test/go", redirect("http://127.0.0.1/admin")),
                (
                    "http://redir.test/meta",
                    redirect("http://meta.test/latest"),
                ),
                ("http://redir.test/scheme", redirect("file:///etc/passwd")),
                ("http://redir.test/ok", redirect("http://public.test/page")),
                ("http://public.test/page", html("<title>x</title>")),
            ],
        );
        let blocked = |e: FetchError| matches!(e, FetchError::Blocked(_));
        for url in [
            "http://127.0.0.1/",
            "http://[::1]/",
            "http://10.0.0.1/",
            "http://169.254.169.254/latest/meta-data/",
            "http://localhost.invalid/",
            "http://rebind.test/",
            "http://meta.test/",
            "http://v6.test/",
            "ftp://public.test/",
            "http://redir.test/go",
            "http://redir.test/meta",
            "http://redir.test/scheme",
        ] {
            let err = get(&f, url).await.expect_err(url);
            // `localhost.invalid` has no DNS entry: a resolution failure is also a refusal.
            assert!(
                blocked(err.clone()) || matches!(err, FetchError::Resolve(_)),
                "{url}: {err:?}"
            );
        }
        // Nothing private was ever contacted: only the two public hops reached the transport.
        for req in t.seen.lock().unwrap().iter() {
            for a in &req.addrs {
                assert!(!crate::policy::is_forbidden_ip(a.ip()), "connected to {a}");
            }
        }
        // A redirect to a public page is followed and is connected by validated IP.
        let ok = get(&f, "http://redir.test/ok").await.unwrap();
        assert_eq!(ok.final_url.as_str(), "http://public.test/page");
        let seen = t.seen.lock().unwrap();
        let last = seen.last().unwrap();
        assert_eq!(last.addrs, vec![SocketAddr::new(ip("93.184.216.34"), 80)]);
    }

    #[tokio::test]
    async fn one_private_answer_refuses_the_host() {
        let mut r = TableResolver::default();
        r.0.insert(
            "mixed.test".into(),
            vec![ip("93.184.216.34"), ip("192.168.0.9")],
        );
        let t = Arc::new(ScriptedTransport::default());
        let f = SafeFetcher::new(Policy::default(), Arc::new(r), t.clone());
        let err = get(&f, "http://mixed.test/").await.unwrap_err();
        assert_eq!(
            err,
            FetchError::Blocked(Blocked::PrivateAddress(ip("192.168.0.9")))
        );
        assert!(t.seen.lock().unwrap().is_empty(), "no connection attempt");
    }

    #[tokio::test]
    async fn at_most_three_redirects() {
        let hosts = [("a.test", "93.184.216.34")];
        let (f, _) = fetcher(
            &hosts,
            vec![
                ("http://a.test/0", redirect("/1")),
                ("http://a.test/1", redirect("/2")),
                ("http://a.test/2", redirect("/3")),
                ("http://a.test/3", html("<title>end</title>")),
                ("http://a.test/4", redirect("/5")),
            ],
        );
        // Three hops reach /3.
        assert!(get(&f, "http://a.test/0").await.is_ok());
        let (f, _) = fetcher(
            &hosts,
            vec![
                ("http://a.test/0", redirect("/1")),
                ("http://a.test/1", redirect("/2")),
                ("http://a.test/2", redirect("/3")),
                ("http://a.test/3", redirect("/4")),
                ("http://a.test/4", html("x")),
            ],
        );
        assert_eq!(
            get(&f, "http://a.test/0").await.unwrap_err(),
            FetchError::TooManyRedirects
        );
    }

    #[tokio::test]
    async fn content_type_allow_list() {
        let hosts = [("a.test", "93.184.216.34")];
        let resp = |ct: &str| HopResponse {
            status: 200,
            content_type: Some(ct.into()),
            ..HopResponse::default()
        };
        let (f, _) = fetcher(
            &hosts,
            vec![
                ("http://a.test/bin", resp("application/octet-stream")),
                ("http://a.test/svg", resp("image/svg+xml")),
                ("http://a.test/png", resp("image/png")),
                (
                    "http://a.test/none",
                    HopResponse {
                        status: 200,
                        ..HopResponse::default()
                    },
                ),
                (
                    "http://a.test/404",
                    HopResponse {
                        status: 404,
                        ..HopResponse::default()
                    },
                ),
            ],
        );
        assert_eq!(
            get(&f, "http://a.test/bin").await.unwrap_err(),
            FetchError::UnsupportedType
        );
        assert_eq!(
            get(&f, "http://a.test/svg").await.unwrap_err(),
            FetchError::UnsupportedType
        );
        assert_eq!(
            get(&f, "http://a.test/none").await.unwrap_err(),
            FetchError::UnsupportedType
        );
        assert_eq!(
            get(&f, "http://a.test/404").await.unwrap_err(),
            FetchError::BadStatus(404)
        );
        assert_eq!(
            get(&f, "http://a.test/png").await.unwrap().class,
            BodyClass::Image
        );
    }

    #[test]
    fn classify_table() {
        assert_eq!(
            classify(Some("text/html; charset=UTF-8")),
            Some(BodyClass::Html)
        );
        assert_eq!(classify(Some("IMAGE/PNG")), Some(BodyClass::Image));
        assert_eq!(
            classify(Some("application/vnd.api+json")),
            Some(BodyClass::Json)
        );
        assert_eq!(classify(Some("video/mp4")), Some(BodyClass::Video));
        assert_eq!(classify(Some("text/javascript")), None);
        assert_eq!(classify(Some("image/svg+xml")), None);
        assert_eq!(classify(None), None);
    }
}
