//! The shared SSRF-safe HTTP client.

use crate::policy::{Blocked, Policy};
use crate::resolve::{Resolver, SystemResolver, resolve_validated};
use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use url::{Host, Url};

/// Most redirects followed; each hop is validated again.
pub const MAX_REDIRECTS: usize = 3;
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub const TOTAL_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NetError {
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
    #[error("response is too large")]
    TooLarge,
}

/// Whether `status` is a redirect the client follows.
pub fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

/// A reqwest builder that is safe for one hop: no redirects, no environment proxies, no
/// idle pooling, and the host resolved only to the validated `addrs`. Every transport in
/// the workspace that talks to an untrusted URL must start from this.
pub fn pinned_builder(user_agent: &str, url: &Url, addrs: &[SocketAddr]) -> reqwest::ClientBuilder {
    let mut builder = reqwest::Client::builder()
        .user_agent(user_agent)
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(TOTAL_TIMEOUT)
        .pool_max_idle_per_host(0);
    if let Some(Host::Domain(d)) = url.host() {
        builder = builder.resolve_to_addrs(d.trim_end_matches('.'), addrs);
    }
    builder
}

/// One outgoing request. `addrs` are validated; the transport must not resolve the host.
#[derive(Debug, Clone)]
pub struct HopRequest {
    pub url: Url,
    pub addrs: Vec<SocketAddr>,
    pub accept: &'static str,
    /// Hard cap on the body read.
    pub max_bytes: usize,
}

#[derive(Debug, Clone, Default)]
pub struct HopResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub location: Option<String>,
    pub body: Vec<u8>,
}

/// One outgoing POST. `addrs` are validated; the transport must not resolve the host.
#[derive(Debug, Clone)]
pub struct PostRequest {
    pub url: Url,
    pub addrs: Vec<SocketAddr>,
    /// Extra headers (name, value); `Content-Type` is `application/json`.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Hard cap on the response body read.
    pub max_bytes: usize,
}

/// The answer to a POST, whatever its status (callers need to tell a 401 from a 200).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PostResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Methods the Activity proxy forwards to an application's own server (J-17, D-248).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForwardMethod {
    Post,
    Put,
    Patch,
    Delete,
}

impl ForwardMethod {
    fn as_str(self) -> &'static str {
        match self {
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
}

/// One forwarded write. `addrs` are validated; the transport must not resolve the host. Nothing but the
/// body and `Content-Type` is sent.
#[derive(Debug, Clone)]
pub struct ForwardRequest {
    pub method: ForwardMethod,
    pub url: Url,
    pub addrs: Vec<SocketAddr>,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
    /// Hard cap on the response body read.
    pub max_bytes: usize,
}

/// The answer to a forwarded write, whatever its status.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ForwardResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

/// Performs a single request without following redirects.
#[async_trait]
pub trait HttpTransport: Send + Sync {
    async fn get(&self, req: &HopRequest) -> Result<HopResponse, NetError>;

    /// One API request (GET or form POST, with credentials) to validated addresses, never
    /// following a redirect. Transports that only fetch public pages need not implement it.
    async fn send(&self, req: &ApiHop) -> Result<HopResponse, NetError> {
        let _ = req;
        Err(NetError::Http("this transport only supports GET".into()))
    }

    /// Forward a write once. Transports that cannot keep this default.
    async fn forward(&self, _req: &ForwardRequest) -> Result<ForwardResponse, NetError> {
        Err(NetError::Http(
            "forwarding writes is not supported by this transport".into(),
        ))
    }

    /// POST a JSON body. Transports that cannot POST keep this default.
    async fn post(&self, _req: &PostRequest) -> Result<PostResponse, NetError> {
        Err(NetError::Http(
            "POST is not supported by this transport".into(),
        ))
    }
}

/// How an [`ApiCall`] sends its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiMethod {
    Get,
    /// `application/x-www-form-urlencoded` POST.
    PostForm(Vec<(String, String)>),
}

/// An authenticated API call (OAuth token exchange, user-info lookup). The URL is checked
/// against the SSRF policy exactly like a fetch; redirects are not followed at all.
#[derive(Debug, Clone)]
pub struct ApiCall {
    pub url: Url,
    pub method: ApiMethod,
    /// Extra headers such as `Authorization: Bearer ...` or `Client-Id`.
    pub headers: Vec<(String, String)>,
    /// HTTP basic credentials (client id and secret), when the provider wants them there.
    pub basic: Option<(String, String)>,
    pub accept: &'static str,
    /// Hard cap on the body read.
    pub max_bytes: usize,
}

/// An [`ApiCall`] bound to validated addresses.
#[derive(Debug, Clone)]
pub struct ApiHop {
    pub call: ApiCall,
    pub addrs: Vec<SocketAddr>,
}

/// reqwest over rustls, pinned via [`pinned_builder`].
#[derive(Debug, Clone)]
pub struct ReqwestTransport {
    user_agent: String,
}

impl ReqwestTransport {
    pub fn new(user_agent: &str) -> Self {
        Self {
            user_agent: user_agent.to_owned(),
        }
    }
}

fn http_err(e: &reqwest::Error) -> NetError {
    if e.is_timeout() {
        NetError::Timeout
    } else {
        NetError::Http(e.to_string())
    }
}

#[async_trait]
impl HttpTransport for ReqwestTransport {
    async fn send(&self, req: &ApiHop) -> Result<HopResponse, NetError> {
        let call = &req.call;
        let client = pinned_builder(&self.user_agent, &call.url, &req.addrs)
            .build()
            .map_err(|e| NetError::Http(e.to_string()))?;
        let mut builder = match &call.method {
            ApiMethod::Get => client.get(call.url.clone()),
            ApiMethod::PostForm(pairs) => {
                let body = url::form_urlencoded::Serializer::new(String::new())
                    .extend_pairs(pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
                    .finish();
                client
                    .post(call.url.clone())
                    .header(
                        reqwest::header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .body(body)
            }
        };
        builder = builder.header(reqwest::header::ACCEPT, call.accept);
        for (k, v) in &call.headers {
            builder = builder.header(k.as_str(), v.as_str());
        }
        if let Some((user, pass)) = &call.basic {
            builder = builder.basic_auth(user, Some(pass));
        }
        let resp = builder
            .send()
            .await
            // `without_url` keeps query strings (which may carry codes) out of logs.
            .map_err(|e| http_err(&e.without_url()))?;
        read_body(resp, call.max_bytes, true).await
    }

    async fn get(&self, req: &HopRequest) -> Result<HopResponse, NetError> {
        let client = pinned_builder(&self.user_agent, &req.url, &req.addrs)
            .build()
            .map_err(|e| NetError::Http(e.to_string()))?;
        let mut resp = client
            .get(req.url.clone())
            .header(reqwest::header::ACCEPT, req.accept)
            .send()
            .await
            // `without_url` keeps query strings (which may carry tokens) out of logs.
            .map_err(|e| http_err(&e.without_url()))?;
        let header = |name| {
            resp.headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        let mut out = HopResponse {
            status: resp.status().as_u16(),
            content_type: header(reqwest::header::CONTENT_TYPE),
            location: header(reqwest::header::LOCATION),
            body: Vec::new(),
        };
        if !(200..300).contains(&out.status) {
            return Ok(out);
        }
        while let Some(chunk) = resp.chunk().await.map_err(|e| http_err(&e.without_url()))? {
            if out.body.len() + chunk.len() > req.max_bytes {
                return Err(NetError::TooLarge);
            }
            out.body.extend_from_slice(&chunk);
        }
        Ok(out)
    }

    async fn forward(&self, req: &ForwardRequest) -> Result<ForwardResponse, NetError> {
        let client = pinned_builder(&self.user_agent, &req.url, &req.addrs)
            .build()
            .map_err(|e| NetError::Http(e.to_string()))?;
        let method = reqwest::Method::from_bytes(req.method.as_str().as_bytes())
            .map_err(|e| NetError::Http(e.to_string()))?;
        let mut builder = client.request(method, req.url.clone()).body(req.body.clone());
        if let Some(ct) = &req.content_type {
            builder = builder.header(reqwest::header::CONTENT_TYPE, ct.as_str());
        }
        let mut resp = builder
            .send()
            .await
            .map_err(|e| http_err(&e.without_url()))?;
        let mut out = ForwardResponse {
            status: resp.status().as_u16(),
            content_type: resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned),
            body: Vec::new(),
        };
        while let Some(chunk) = resp.chunk().await.map_err(|e| http_err(&e.without_url()))? {
            if out.body.len() + chunk.len() > req.max_bytes {
                return Err(NetError::TooLarge);
            }
            out.body.extend_from_slice(&chunk);
        }
        Ok(out)
    }

    async fn post(&self, req: &PostRequest) -> Result<PostResponse, NetError> {
        let client = pinned_builder(&self.user_agent, &req.url, &req.addrs)
            .build()
            .map_err(|e| NetError::Http(e.to_string()))?;
        let mut builder = client
            .post(req.url.clone())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::ACCEPT, "application/json")
            .body(req.body.clone());
        for (k, v) in &req.headers {
            builder = builder.header(k.as_str(), v.as_str());
        }
        let mut resp = builder
            .send()
            .await
            .map_err(|e| http_err(&e.without_url()))?;
        let mut out = PostResponse {
            status: resp.status().as_u16(),
            body: Vec::new(),
        };
        while let Some(chunk) = resp.chunk().await.map_err(|e| http_err(&e.without_url()))? {
            if out.body.len() + chunk.len() > req.max_bytes {
                return Err(NetError::TooLarge);
            }
            out.body.extend_from_slice(&chunk);
        }
        Ok(out)
    }
}

async fn read_body(
    mut resp: reqwest::Response,
    max_bytes: usize,
    keep_error_body: bool,
) -> Result<HopResponse, NetError> {
    let header = |resp: &reqwest::Response, name| {
        resp.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    let mut out = HopResponse {
        status: resp.status().as_u16(),
        content_type: header(&resp, reqwest::header::CONTENT_TYPE),
        location: header(&resp, reqwest::header::LOCATION),
        body: Vec::new(),
    };
    if !keep_error_body && !(200..300).contains(&out.status) {
        return Ok(out);
    }
    while let Some(chunk) = resp.chunk().await.map_err(|e| http_err(&e.without_url()))? {
        if out.body.len() + chunk.len() > max_bytes {
            return Err(NetError::TooLarge);
        }
        out.body.extend_from_slice(&chunk);
    }
    Ok(out)
}

/// A successful response.
#[derive(Debug, Clone)]
pub struct SafeResponse {
    pub final_url: Url,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

impl SafeResponse {
    /// The body as lossy UTF-8.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// A capped GET under the SSRF policy.
#[derive(Clone)]
pub struct SafeClient {
    policy: Policy,
    resolver: Arc<dyn Resolver>,
    transport: Arc<dyn HttpTransport>,
}

impl SafeClient {
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

    /// The system resolver and the reqwest transport.
    pub fn production(policy: Policy, user_agent: &str) -> Self {
        Self::new(
            policy,
            Arc::new(SystemResolver),
            Arc::new(ReqwestTransport::new(user_agent)),
        )
    }

    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    /// One API call under the SSRF policy: the host is resolved and every address checked,
    /// the connection is pinned to them, and a redirect is an error (credentials never
    /// follow one). Returns the status and the capped body of *any* status, so callers can
    /// read an OAuth error document; transport failures and over-long bodies are errors.
    pub async fn call(&self, call: ApiCall) -> Result<(u16, Vec<u8>), NetError> {
        tokio::time::timeout(TOTAL_TIMEOUT, async {
            let addrs = resolve_validated(&self.policy, &*self.resolver, &call.url).await?;
            let max = call.max_bytes;
            let resp = self.transport.send(&ApiHop { call, addrs }).await?;
            if is_redirect(resp.status) {
                return Err(NetError::BadRedirect);
            }
            if resp.body.len() > max {
                return Err(NetError::TooLarge);
            }
            Ok((resp.status, resp.body))
        })
        .await
        .map_err(|_| NetError::Timeout)?
    }

    /// GET `url`, following at most [`MAX_REDIRECTS`] redirects and validating every hop.
    /// The whole call is bounded by [`TOTAL_TIMEOUT`]; the body by `max_bytes`.
    pub async fn get(
        &self,
        url: &Url,
        accept: &'static str,
        max_bytes: usize,
    ) -> Result<SafeResponse, NetError> {
        tokio::time::timeout(TOTAL_TIMEOUT, self.get_inner(url, accept, max_bytes))
            .await
            .map_err(|_| NetError::Timeout)?
    }

    /// POST `body` (JSON) once under the SSRF policy. Redirects are never followed (a 3xx
    /// comes back as a plain status) and any status is returned; the whole call is bounded
    /// by `timeout` (at most [`TOTAL_TIMEOUT`]).
    pub async fn post_json(
        &self,
        url: &Url,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
        max_bytes: usize,
        timeout: Duration,
    ) -> Result<PostResponse, NetError> {
        let work = async {
            let addrs = resolve_validated(&self.policy, &*self.resolver, url).await?;
            self.transport
                .post(&PostRequest {
                    url: url.clone(),
                    addrs,
                    headers,
                    body,
                    max_bytes,
                })
                .await
        };
        tokio::time::timeout(timeout.min(TOTAL_TIMEOUT), work)
            .await
            .map_err(|_| NetError::Timeout)?
    }

    /// Forward a write (`POST`, `PUT`, `PATCH`, `DELETE`) once under the SSRF policy. Redirects are never
    /// followed (a 3xx comes back as a plain status) and any status is returned. Only the body and the
    /// content type are sent. Bounded by [`TOTAL_TIMEOUT`].
    pub async fn forward(
        &self,
        method: ForwardMethod,
        url: &Url,
        content_type: Option<String>,
        body: Vec<u8>,
        max_bytes: usize,
    ) -> Result<ForwardResponse, NetError> {
        let work = async {
            let addrs = resolve_validated(&self.policy, &*self.resolver, url).await?;
            self.transport
                .forward(&ForwardRequest {
                    method,
                    url: url.clone(),
                    addrs,
                    content_type,
                    body,
                    max_bytes,
                })
                .await
        };
        tokio::time::timeout(TOTAL_TIMEOUT, work)
            .await
            .map_err(|_| NetError::Timeout)?
    }

    async fn get_inner(
        &self,
        url: &Url,
        accept: &'static str,
        max_bytes: usize,
    ) -> Result<SafeResponse, NetError> {
        let mut current = url.clone();
        let mut redirects = 0;
        loop {
            let addrs = resolve_validated(&self.policy, &*self.resolver, &current).await?;
            let resp = self
                .transport
                .get(&HopRequest {
                    url: current.clone(),
                    addrs,
                    accept,
                    max_bytes,
                })
                .await?;
            if is_redirect(resp.status) {
                let loc = resp.location.ok_or(NetError::BadRedirect)?;
                if redirects == MAX_REDIRECTS {
                    return Err(NetError::TooManyRedirects);
                }
                redirects += 1;
                current = current.join(&loc).map_err(|_| NetError::BadRedirect)?;
                continue;
            }
            if !(200..300).contains(&resp.status) {
                return Err(NetError::BadStatus(resp.status));
            }
            if resp.body.len() > max_bytes {
                return Err(NetError::TooLarge);
            }
            return Ok(SafeResponse {
                final_url: current,
                content_type: resp.content_type,
                body: resp.body,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::net::IpAddr;
    use std::sync::Mutex;

    /// Scripted DNS: each lookup pops the next answer for the host (the last repeats).
    struct Table(Mutex<HashMap<String, Vec<Vec<IpAddr>>>>);
    #[async_trait]
    impl Resolver for Table {
        async fn resolve(&self, host: &str, _p: u16) -> Result<Vec<IpAddr>, String> {
            let mut m = self.0.lock().unwrap();
            match m.get_mut(host) {
                Some(v) if v.len() > 1 => Ok(v.remove(0)),
                Some(v) => Ok(v[0].clone()),
                None => Err("nxdomain".into()),
            }
        }
    }
    struct Script {
        routes: HashMap<String, HopResponse>,
        seen: Mutex<Vec<HopRequest>>,
    }
    #[async_trait]
    impl HttpTransport for Script {
        async fn get(&self, req: &HopRequest) -> Result<HopResponse, NetError> {
            self.seen.lock().unwrap().push(req.clone());
            self.routes
                .get(req.url.as_str())
                .cloned()
                .ok_or_else(|| NetError::Http("no route".into()))
        }
    }

    fn ips(s: &[&str]) -> Vec<IpAddr> {
        s.iter().map(|x| x.parse().unwrap()).collect()
    }
    fn ok(body: &str) -> HopResponse {
        HopResponse {
            status: 200,
            body: body.as_bytes().to_vec(),
            ..HopResponse::default()
        }
    }
    fn redir(to: &str) -> HopResponse {
        HopResponse {
            status: 302,
            location: Some(to.into()),
            ..HopResponse::default()
        }
    }
    type Dns<'a> = &'a [(&'a str, Vec<Vec<&'a str>>)];
    fn client(dns: Dns, routes: &[(&str, HopResponse)]) -> (SafeClient, Arc<Script>) {
        let table = dns
            .iter()
            .map(|(h, a)| ((*h).to_owned(), a.iter().map(|x| ips(x)).collect()))
            .collect();
        let script = Arc::new(Script {
            routes: routes
                .iter()
                .map(|(u, r)| ((*u).to_owned(), r.clone()))
                .collect(),
            seen: Mutex::default(),
        });
        let c = SafeClient::new(
            Policy::default(),
            Arc::new(Table(Mutex::new(table))),
            script.clone(),
        );
        (c, script)
    }
    async fn get(c: &SafeClient, u: &str) -> Result<SafeResponse, NetError> {
        c.get(&Url::parse(u).unwrap(), "*/*", 1024).await
    }
    fn is_private(e: &NetError) -> bool {
        matches!(e, NetError::Blocked(Blocked::PrivateAddress(_)))
    }

    #[tokio::test]
    async fn public_host_is_fetched_and_pinned() {
        let (c, s) = client(
            &[("example.com", vec![vec!["93.184.216.34"]])],
            &[("https://example.com/a", ok("hi"))],
        );
        assert_eq!(get(&c, "https://example.com/a").await.unwrap().text(), "hi");
        let seen = s.seen.lock().unwrap();
        assert_eq!(seen[0].addrs[0].ip().to_string(), "93.184.216.34");
        assert_eq!(seen[0].addrs[0].port(), 443);
    }

    #[tokio::test]
    async fn private_loopback_and_link_local_are_refused_before_any_request() {
        for lit in [
            "http://127.0.0.1/",
            "http://[::1]/",
            "http://10.1.2.3/",
            "http://192.168.0.1/",
            "http://169.254.169.254/latest/meta-data/",
            "http://[fe80::1]/",
            "http://2130706433/",
        ] {
            let (c, s) = client(&[], &[]);
            let err = get(&c, lit).await.unwrap_err();
            assert!(is_private(&err), "{lit}: {err:?}");
            assert!(s.seen.lock().unwrap().is_empty(), "{lit} must not connect");
        }
    }

    #[tokio::test]
    async fn names_that_resolve_private_are_refused_even_if_mixed() {
        for answers in [
            vec!["127.0.0.1"],
            vec!["8.8.8.8", "10.0.0.1"],
            vec!["::ffff:169.254.169.254"],
        ] {
            let (c, s) = client(&[("evil.test", vec![answers])], &[]);
            let err = get(&c, "http://evil.test/").await.unwrap_err();
            assert!(is_private(&err), "{err:?}");
            assert!(s.seen.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn redirect_to_private_is_refused() {
        for target in [
            "http://127.0.0.1/admin",
            "http://169.254.169.254/",
            "http://internal.test/",
        ] {
            let (c, s) = client(
                &[
                    ("good.test", vec![vec!["8.8.8.8"]]),
                    ("internal.test", vec![vec!["10.0.0.9"]]),
                ],
                &[("http://good.test/", redir(target))],
            );
            let err = get(&c, "http://good.test/").await.unwrap_err();
            assert!(is_private(&err), "{target}: {err:?}");
            assert_eq!(
                s.seen.lock().unwrap().len(),
                1,
                "only the first hop is requested"
            );
        }
    }

    #[tokio::test]
    async fn dns_rebinding_flip_is_caught_on_the_next_lookup() {
        // The name is public on the first lookup and loopback on the second. Each hop
        // resolves once and connects to exactly that answer, so the second hop is refused
        // instead of reaching loopback.
        let (c, s) = client(
            &[
                ("a.test", vec![vec!["8.8.8.8"]]),
                ("rebind.test", vec![vec!["1.1.1.1"], vec!["127.0.0.1"]]),
            ],
            &[
                ("http://a.test/", redir("http://rebind.test/x")),
                ("http://rebind.test/x", redir("http://rebind.test/y")),
            ],
        );
        let err = get(&c, "http://a.test/").await.unwrap_err();
        assert!(is_private(&err), "{err:?}");
        let seen = s.seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[1].addrs[0].ip().to_string(), "1.1.1.1");
    }

    #[tokio::test]
    async fn redirect_cap_status_and_size() {
        let (c, _) = client(
            &[("r.test", vec![vec!["8.8.8.8"]])],
            &[
                ("http://r.test/0", redir("/1")),
                ("http://r.test/1", redir("/2")),
                ("http://r.test/2", redir("/3")),
                ("http://r.test/3", redir("/4")),
                ("http://r.test/ok", ok("0123456789")),
                (
                    "http://r.test/500",
                    HopResponse {
                        status: 500,
                        ..HopResponse::default()
                    },
                ),
            ],
        );
        assert_eq!(
            get(&c, "http://r.test/0").await.unwrap_err(),
            NetError::TooManyRedirects
        );
        assert_eq!(
            get(&c, "http://r.test/500").await.unwrap_err(),
            NetError::BadStatus(500)
        );
        let small = c
            .get(&Url::parse("http://r.test/ok").unwrap(), "*/*", 4)
            .await;
        assert_eq!(small.unwrap_err(), NetError::TooLarge);
    }

    #[tokio::test]
    async fn non_http_schemes_and_credentials_are_refused() {
        let (c, _) = client(&[], &[]);
        for u in [
            "file:///etc/passwd",
            "ftp://example.com/",
            "http://u:p@example.com/",
        ] {
            assert!(
                matches!(get(&c, u).await.unwrap_err(), NetError::Blocked(_)),
                "{u}"
            );
        }
    }

    #[tokio::test]
    async fn post_is_validated_pinned_and_returns_any_status() {
        struct Poster(Mutex<Vec<PostRequest>>);
        #[async_trait]
        impl HttpTransport for Poster {
            async fn get(&self, _: &HopRequest) -> Result<HopResponse, NetError> {
                Err(NetError::Http("no".into()))
            }
            async fn post(&self, req: &PostRequest) -> Result<PostResponse, NetError> {
                self.0.lock().unwrap().push(req.clone());
                Ok(PostResponse {
                    status: 401,
                    body: b"no".to_vec(),
                })
            }
        }
        let poster = Arc::new(Poster(Mutex::default()));
        let table = HashMap::from([
            ("good.test".to_owned(), vec![ips(&["8.8.8.8"])]),
            ("bad.test".to_owned(), vec![ips(&["10.0.0.1"])]),
        ]);
        let c = SafeClient::new(
            Policy::default(),
            Arc::new(Table(Mutex::new(table))),
            poster.clone(),
        );
        let t = Duration::from_secs(2);
        let r = c
            .post_json(
                &Url::parse("https://good.test/i").unwrap(),
                vec![("X-A".into(), "b".into())],
                b"{}".to_vec(),
                64,
                t,
            )
            .await
            .unwrap();
        assert_eq!(r.status, 401);
        assert_eq!(
            poster.0.lock().unwrap()[0].addrs[0].ip().to_string(),
            "8.8.8.8"
        );
        let err = c
            .post_json(
                &Url::parse("https://bad.test/i").unwrap(),
                vec![],
                vec![],
                64,
                t,
            )
            .await
            .unwrap_err();
        assert!(is_private(&err), "{err:?}");
        assert_eq!(
            poster.0.lock().unwrap().len(),
            1,
            "no request to a private host"
        );
        // A transport without POST support fails closed.
        let (g, _) = client(&[("good.test", vec![vec!["8.8.8.8"]])], &[]);
        assert!(
            g.post_json(
                &Url::parse("https://good.test/").unwrap(),
                vec![],
                vec![],
                8,
                t
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn operator_opt_in_allows_private_hosts() {
        let script = Arc::new(Script {
            routes: HashMap::from([("http://127.0.0.1:3002/x".to_owned(), ok("local"))]),
            seen: Mutex::default(),
        });
        let c = SafeClient::new(
            Policy {
                allow_private_hosts: true,
                ..Policy::default()
            },
            Arc::new(Table(Mutex::new(HashMap::new()))),
            script,
        );
        assert_eq!(
            get(&c, "http://127.0.0.1:3002/x").await.unwrap().text(),
            "local"
        );
    }

    /// Records API calls and answers from a table keyed by URL.
    struct ApiScript {
        routes: HashMap<String, HopResponse>,
        seen: Mutex<Vec<ApiHop>>,
    }
    #[async_trait]
    impl HttpTransport for ApiScript {
        async fn get(&self, _req: &HopRequest) -> Result<HopResponse, NetError> {
            Err(NetError::Http("get not scripted".into()))
        }
        async fn send(&self, req: &ApiHop) -> Result<HopResponse, NetError> {
            self.seen.lock().unwrap().push(req.clone());
            self.routes
                .get(req.call.url.as_str())
                .cloned()
                .ok_or_else(|| NetError::Http("no route".into()))
        }
    }

    fn api(url: &str) -> ApiCall {
        ApiCall {
            url: Url::parse(url).unwrap(),
            method: ApiMethod::PostForm(vec![("code".into(), "abc".into())]),
            headers: vec![("Authorization".into(), "Bearer t".into())],
            basic: None,
            accept: "application/json",
            max_bytes: 1024,
        }
    }

    fn api_client(routes: &[(&str, HopResponse)]) -> (SafeClient, Arc<ApiScript>) {
        let script = Arc::new(ApiScript {
            routes: routes
                .iter()
                .map(|(u, r)| ((*u).to_owned(), r.clone()))
                .collect(),
            seen: Mutex::default(),
        });
        let table = HashMap::from([("api.example.com".to_owned(), vec![ips(&["93.184.216.34"])])]);
        let c = SafeClient::new(
            Policy::default(),
            Arc::new(Table(Mutex::new(table))),
            script.clone(),
        );
        (c, script)
    }

    #[tokio::test]
    async fn api_calls_are_pinned_and_return_error_documents() {
        let mut bad = ok("{\"error\":\"invalid_grant\"}");
        bad.status = 400;
        let (c, s) = api_client(&[("https://api.example.com/token", bad)]);
        let (status, body) = c.call(api("https://api.example.com/token")).await.unwrap();
        assert_eq!(
            status, 400,
            "the caller reads the provider's error document"
        );
        assert!(String::from_utf8_lossy(&body).contains("invalid_grant"));
        let seen = s.seen.lock().unwrap();
        assert_eq!(
            seen[0].addrs[0].ip().to_string(),
            "93.184.216.34",
            "pinned address"
        );
        assert_eq!(seen[0].call.headers[0].1, "Bearer t");
    }

    #[tokio::test]
    async fn api_calls_never_follow_redirects_or_reach_private_hosts() {
        let (c, s) = api_client(&[(
            "https://api.example.com/token",
            redir("http://169.254.169.254/latest"),
        )]);
        assert_eq!(
            c.call(api("https://api.example.com/token"))
                .await
                .unwrap_err(),
            NetError::BadRedirect
        );
        assert_eq!(
            s.seen.lock().unwrap().len(),
            1,
            "the redirect target is not contacted"
        );
        for u in [
            "http://127.0.0.1/token",
            "https://10.0.0.1/token",
            "ftp://api.example.com/x",
        ] {
            assert!(
                matches!(c.call(api(u)).await.unwrap_err(), NetError::Blocked(_)),
                "{u}"
            );
        }
        // The default transport cannot send API calls.
        let get_only = SafeClient::new(
            Policy::default(),
            Arc::new(Table(Mutex::new(HashMap::from([(
                "api.example.com".to_owned(),
                vec![ips(&["93.184.216.34"])],
            )])))),
            Arc::new(Script {
                routes: HashMap::new(),
                seen: Mutex::default(),
            }),
        );
        assert!(
            get_only
                .call(api("https://api.example.com/token"))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn api_bodies_are_capped() {
        let big = HopResponse {
            status: 200,
            body: vec![b'x'; 2048],
            ..HopResponse::default()
        };
        let (c, _) = api_client(&[("https://api.example.com/big", big)]);
        assert_eq!(
            c.call(api("https://api.example.com/big"))
                .await
                .unwrap_err(),
            NetError::TooLarge
        );
    }

    /// Records forwarded writes.
    struct FwScript(Mutex<Vec<ForwardRequest>>);
    #[async_trait]
    impl HttpTransport for FwScript {
        async fn get(&self, _req: &HopRequest) -> Result<HopResponse, NetError> {
            Err(NetError::Http("get not scripted".into()))
        }
        async fn forward(&self, req: &ForwardRequest) -> Result<ForwardResponse, NetError> {
            self.0.lock().unwrap().push(req.clone());
            Ok(ForwardResponse {
                status: 201,
                content_type: Some("application/json".into()),
                body: b"{}".to_vec(),
            })
        }
    }

    #[tokio::test]
    async fn forwarded_writes_are_pinned_checked_and_carry_only_the_body() {
        let script = Arc::new(FwScript(Mutex::default()));
        let table = HashMap::from([
            ("api.example.com".to_owned(), vec![ips(&["93.184.216.34"])]),
            ("evil.example.com".to_owned(), vec![ips(&["10.0.0.5"])]),
        ]);
        let c = SafeClient::new(
            Policy::default(),
            Arc::new(Table(Mutex::new(table))),
            script.clone(),
        );
        let url = Url::parse("https://api.example.com/v1/rooms").unwrap();
        let r = c
            .forward(
                ForwardMethod::Patch,
                &url,
                Some("application/json".into()),
                b"{\"a\":1}".to_vec(),
                1024,
            )
            .await
            .unwrap();
        assert_eq!(r.status, 201);
        {
            let seen = script.0.lock().unwrap();
            assert_eq!(seen[0].method, ForwardMethod::Patch);
            assert_eq!(seen[0].addrs[0].ip().to_string(), "93.184.216.34");
            assert_eq!(seen[0].body, b"{\"a\":1}");
        }
        // A host that resolves to a private address is refused before anything is sent.
        let bad = Url::parse("https://evil.example.com/x").unwrap();
        assert!(
            c.forward(ForwardMethod::Post, &bad, None, vec![], 1024)
                .await
                .is_err()
        );
        assert_eq!(script.0.lock().unwrap().len(), 1);
    }
}
