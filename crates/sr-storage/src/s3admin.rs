//! Bucket administration calls that `object_store` does not offer, signed with
//! AWS Signature Version 4 (F-08, D-175): `PutBucketCors` and `GetBucketCors`.
//!
//! Chat uploads are presigned PUTs from the browser straight to the bucket, so
//! the bucket must answer the CORS preflight for the app origin. `sr-cli storage
//! ensure-cors` calls [`S3Admin::put_bucket_cors`]. `PutBucketCors` replaces the
//! whole configuration, so repeating the call with the same origins is
//! idempotent.
//!
//! The signer is hand written over `hmac` and `sha2` (both already used for CDN
//! links); the HTTP exchange uses `reqwest` with rustls, never OpenSSL.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};

use crate::error::{Result, StorageError};

type HmacSha256 = Hmac<Sha256>;

/// Methods the app origin may use against the bucket (D-175).
pub const CORS_METHODS: [&str; 3] = ["PUT", "GET", "HEAD"];
/// Request headers the browser may send on a presigned upload. Lowercase: browsers send preflight header names
/// lowercased (Fetch spec), and with Garage `Content-Type` refused a ZIP's `application/zip`
/// PUT (the p24 library gate); the composer never noticed because its part PUTs carry no content type.
pub const CORS_ALLOWED_HEADERS: [&str; 1] = ["content-type"];
/// Response headers the page may read.
pub const CORS_EXPOSE_HEADERS: [&str; 4] =
    ["ETag", "Content-Range", "Accept-Ranges", "Content-Length"];
const CORS_MAX_AGE_SECONDS: u32 = 3600;

pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

pub(crate) fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

pub(crate) fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    match HmacSha256::new_from_slice(key) {
        Ok(mut m) => {
            m.update(data);
            m.finalize().into_bytes().to_vec()
        }
        // HMAC accepts any key length; unreachable.
        Err(_) => Vec::new(),
    }
}

/// One request to sign. Header names must be lower case and sorted.
pub(crate) struct SigningInput<'a> {
    pub(crate) method: &'a str,
    pub(crate) canonical_uri: &'a str,
    pub(crate) canonical_query: &'a str,
    pub(crate) headers: &'a [(String, String)],
    pub(crate) payload_hash: &'a str,
    pub(crate) access_key: &'a str,
    pub(crate) secret_key: &'a str,
    pub(crate) region: &'a str,
    pub(crate) service: &'a str,
    /// `YYYYMMDDTHHMMSSZ`.
    pub(crate) amz_date: &'a str,
}

/// The `Authorization` header value for `input`.
pub(crate) fn authorization(input: &SigningInput<'_>) -> String {
    let date = &input.amz_date[..8];
    let canonical_headers: String = input
        .headers
        .iter()
        .map(|(k, v)| format!("{k}:{}\n", v.trim()))
        .collect();
    let signed_headers = input
        .headers
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");
    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        input.method,
        input.canonical_uri,
        input.canonical_query,
        canonical_headers,
        signed_headers,
        input.payload_hash
    );
    let scope = format!("{date}/{}/{}/aws4_request", input.region, input.service);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{scope}\n{}",
        input.amz_date,
        sha256_hex(canonical_request.as_bytes())
    );
    let k_date = hmac(
        format!("AWS4{}", input.secret_key).as_bytes(),
        date.as_bytes(),
    );
    let k_region = hmac(&k_date, input.region.as_bytes());
    let k_service = hmac(&k_region, input.service.as_bytes());
    let k_signing = hmac(&k_service, b"aws4_request");
    let signature = hex(&hmac(&k_signing, string_to_sign.as_bytes()));
    format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
        input.access_key
    )
}

/// `YYYYMMDDTHHMMSSZ` for a unix time in seconds.
pub(crate) fn amz_date(unix_secs: u64) -> String {
    let days = i64::try_from(unix_secs / 86_400).unwrap_or(0);
    let rem = unix_secs % 86_400;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}{m:02}{d:02}T{:02}{:02}{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

pub(crate) fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Normalise an origin: scheme, host and port only, no path, no trailing slash.
/// `Err` when it is not an `http(s)` origin (a wildcard is refused: D-175 names
/// the app origin).
pub fn normalize_origin(raw: &str) -> Result<String> {
    let bad = || StorageError::Backend(format!("not an http(s) origin: {raw}"));
    let trimmed = raw.trim().trim_end_matches('/');
    let (scheme, rest) = trimmed.split_once("://").ok_or_else(bad)?;
    if !matches!(scheme, "http" | "https")
        || rest.is_empty()
        || rest.contains(['/', '?', '#', '@', '*'])
    {
        return Err(bad());
    }
    Ok(format!("{scheme}://{}", rest.to_ascii_lowercase()))
}

/// The `CORSConfiguration` document for `origins` (D-175): one rule per origin. Garage answers a preflight with
/// the matching rule's `AllowedOrigin` values joined into one header, and a browser refuses an
/// `Access-Control-Allow-Origin` that lists more than one origin, so a rule must never carry two.
pub fn cors_xml(origins: &[String]) -> Result<String> {
    if origins.is_empty() {
        return Err(StorageError::Backend(
            "at least one origin is required".into(),
        ));
    }
    let mut normalized: Vec<String> = Vec::with_capacity(origins.len());
    for o in origins {
        let n = normalize_origin(o)?;
        if !normalized.contains(&n) {
            normalized.push(n);
        }
    }
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><CORSConfiguration xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">",
    );
    for o in &normalized {
        xml.push_str(&format!(
            "<CORSRule><AllowedOrigin>{}</AllowedOrigin>",
            xml_escape(o)
        ));
        for m in CORS_METHODS {
            xml.push_str(&format!("<AllowedMethod>{m}</AllowedMethod>"));
        }
        for h in CORS_ALLOWED_HEADERS {
            xml.push_str(&format!("<AllowedHeader>{h}</AllowedHeader>"));
        }
        for h in CORS_EXPOSE_HEADERS {
            xml.push_str(&format!("<ExposeHeader>{h}</ExposeHeader>"));
        }
        xml.push_str(&format!(
            "<MaxAgeSeconds>{CORS_MAX_AGE_SECONDS}</MaxAgeSeconds></CORSRule>"
        ));
    }
    xml.push_str("</CORSConfiguration>");
    Ok(xml)
}

/// Bucket-level calls against an S3-compatible service (path-style addressing).
#[derive(Clone)]
pub struct S3Admin {
    endpoint: String,
    bucket: String,
    region: String,
    access_key: String,
    secret_key: String,
    client: reqwest::Client,
}

impl std::fmt::Debug for S3Admin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Admin")
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .finish_non_exhaustive()
    }
}

/// An S3 answer: the status and the (text) body. Binary bodies are not needed by the admin calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct S3Reply {
    /// HTTP status.
    pub status: u16,
    /// Response body as text.
    pub body: String,
    /// Response headers, names lower case.
    pub headers: Vec<(String, String)>,
}

impl S3Reply {
    /// A 2xx answer.
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
    /// The S3 error `<Code>` of an XML error body.
    pub fn code(&self) -> Option<String> {
        xml_tag(&self.body, "Code")
    }
    /// A short English reason for a failed call, with no body text beyond the S3 error code.
    pub fn reason(&self) -> String {
        match (self.status, self.code()) {
            (_, Some(c)) => format!("{} ({c})", status_words(self.status)),
            (s, None) => status_words(s).to_owned(),
        }
    }
}

fn status_words(status: u16) -> &'static str {
    match status {
        301 | 307 | 308 => "the bucket is in another region or endpoint",
        400 => "the service rejected the request",
        401 | 403 => "access denied for this key",
        404 => "not found",
        405 | 501 => "the service does not support this call",
        409 => "conflict",
        500..=599 => "the service had an error",
        _ => "unexpected answer",
    }
}

/// The text of the first `<tag>..</tag>` in `xml`.
pub(crate) fn xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&format!("</{tag}>"))? + start;
    Some(xml[start..end].to_owned())
}

/// Percent-encode per SigV4 (unreserved characters stay; `/` stays when `keep_slash`).
fn uri_encode(s: &str, keep_slash: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            b'/' if keep_slash => out.push('/'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

impl S3Admin {
    /// Build a client for `endpoint` (e.g. `http://127.0.0.1:3900`) and `bucket`.
    pub fn new(
        endpoint: &str,
        bucket: &str,
        region: &str,
        access_key: &str,
        secret_key: &str,
    ) -> Result<Self> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        Ok(Self {
            endpoint: endpoint.trim_end_matches('/').to_owned(),
            bucket: bucket.to_owned(),
            region: region.to_owned(),
            access_key: access_key.to_owned(),
            secret_key: secret_key.to_owned(),
            client,
        })
    }

    /// The endpoint URL (for [`crate::guard::EndpointGuard`]).
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Connect only to `addrs` (resolved and checked once by an [`crate::guard::EndpointGuard`]) instead of
    /// looking the host name up again.
    pub fn pinned(mut self, addrs: &[std::net::SocketAddr]) -> Result<Self> {
        let mut b = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(30));
        if let Some(host) = crate::guard::pin_host(&self.endpoint) {
            b = b.resolve_to_addrs(&host, addrs);
        }
        self.client = b
            .build()
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        Ok(self)
    }

    fn host(&self) -> Result<String> {
        let rest = self
            .endpoint
            .split_once("://")
            .map_or(self.endpoint.as_str(), |(_, r)| r);
        let host = rest.split('/').next().unwrap_or(rest);
        if host.is_empty() {
            return Err(StorageError::Backend("empty S3 endpoint".into()));
        }
        Ok(host.to_owned())
    }

    /// Replace the bucket's CORS configuration with one rule allowing `origins`
    /// to `PUT`, `GET` and `HEAD` (`Content-Type` allowed, `ETag` and the range headers exposed).
    /// Repeating the call is harmless: S3 replaces the configuration.
    pub async fn put_bucket_cors(&self, origins: &[String]) -> Result<()> {
        let reply = self.put_bucket_cors_raw(origins).await?;
        if reply.ok() {
            Ok(())
        } else {
            Err(StorageError::Backend(format!(
                "PUT bucket cors failed with {}: {}",
                reply.status,
                reply.body.chars().take(300).collect::<String>()
            )))
        }
    }

    /// [`Self::put_bucket_cors`] returning the service's answer instead of failing on a refusal.
    pub async fn put_bucket_cors_raw(&self, origins: &[String]) -> Result<S3Reply> {
        let body = cors_xml(origins)?;
        let checksum = STANDARD.encode(Sha256::digest(body.as_bytes()));
        let extra = [
            ("x-amz-checksum-sha256", checksum),
            ("x-amz-sdk-checksum-algorithm", "SHA256".to_owned()),
            ("content-type", "application/xml".to_owned()),
        ];
        self.call(
            "PUT",
            Some(""),
            &[("cors", "")],
            Some(body.as_bytes()),
            &extra,
            None,
        )
        .await
    }

    /// The raw CORS configuration, or `None` when the bucket has none.
    pub async fn get_bucket_cors(&self) -> Result<Option<String>> {
        let reply = self.get_bucket_cors_raw().await?;
        if reply.ok() {
            return Ok(Some(reply.body));
        }
        if reply.status == 404 && reply.body.contains("NoSuchCORSConfiguration") {
            return Ok(None);
        }
        Err(StorageError::Backend(format!(
            "GET bucket cors failed with {}: {}",
            reply.status,
            reply.body.chars().take(300).collect::<String>()
        )))
    }

    /// [`Self::get_bucket_cors`] returning the service's answer as it is.
    pub async fn get_bucket_cors_raw(&self) -> Result<S3Reply> {
        self.call("GET", Some(""), &[("cors", "")], None, &[], None)
            .await
    }

    /// `HeadBucket`.
    pub async fn head_bucket(&self, timeout: Duration) -> Result<S3Reply> {
        self.call("HEAD", Some(""), &[], None, &[], Some(timeout))
            .await
    }

    /// `ListObjectsV2` with `max-keys=1` under `prefix`.
    pub async fn list_one(&self, prefix: &str, timeout: Duration) -> Result<S3Reply> {
        let q = [("list-type", "2"), ("max-keys", "1"), ("prefix", prefix)];
        self.call("GET", Some(""), &q, None, &[], Some(timeout))
            .await
    }

    /// `PutObject`.
    pub async fn put_object(&self, key: &str, body: &[u8], timeout: Duration) -> Result<S3Reply> {
        let extra = [("content-type", "application/octet-stream".to_owned())];
        self.call("PUT", Some(key), &[], Some(body), &extra, Some(timeout))
            .await
    }

    /// `GetObject` (the whole object; the admin calls only read tiny probe objects).
    pub async fn get_object(&self, key: &str, timeout: Duration) -> Result<S3Reply> {
        self.call("GET", Some(key), &[], None, &[], Some(timeout))
            .await
    }

    /// `DeleteObject`.
    pub async fn delete_object(&self, key: &str, timeout: Duration) -> Result<S3Reply> {
        self.call("DELETE", Some(key), &[], None, &[], Some(timeout))
            .await
    }

    /// `CreateMultipartUpload`; the reply body carries `<UploadId>`.
    pub async fn create_multipart(&self, key: &str, timeout: Duration) -> Result<S3Reply> {
        self.call(
            "POST",
            Some(key),
            &[("uploads", "")],
            None,
            &[],
            Some(timeout),
        )
        .await
    }

    /// `AbortMultipartUpload`.
    pub async fn abort_multipart(
        &self,
        key: &str,
        upload_id: &str,
        timeout: Duration,
    ) -> Result<S3Reply> {
        let q = [("uploadId", upload_id)];
        self.call("DELETE", Some(key), &q, None, &[], Some(timeout))
            .await
    }

    /// `ListBuckets` (a service-level call; a bucket-scoped key is expected to be refused).
    pub async fn list_buckets(&self, timeout: Duration) -> Result<S3Reply> {
        self.call("GET", None, &[], None, &[], Some(timeout)).await
    }

    /// One signed request. `key` is `None` for the service root, `Some("")` for the bucket itself.
    /// `query` values may be empty (a sub-resource such as `cors`). Transport failures are `Err`;
    /// any HTTP answer is `Ok`.
    async fn call(
        &self,
        method: &str,
        key: Option<&str>,
        query: &[(&str, &str)],
        body: Option<&[u8]>,
        extra: &[(&str, String)],
        timeout: Option<Duration>,
    ) -> Result<S3Reply> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let amz_date = amz_date(now);
        let host = self.host()?;
        let payload = body.unwrap_or(&[]);
        let payload_hash = sha256_hex(payload);
        let mut headers: Vec<(String, String)> = vec![
            ("host".into(), host),
            ("x-amz-content-sha256".into(), payload_hash.clone()),
            ("x-amz-date".into(), amz_date.clone()),
        ];
        for (k, v) in extra {
            // content-type is sent but not signed.
            if *k != "content-type" {
                headers.push(((*k).to_owned(), v.clone()));
            }
        }
        headers.sort();
        let uri = match key {
            None => "/".to_owned(),
            Some("") => format!("/{}", uri_encode(&self.bucket, false)),
            Some(k) => format!(
                "/{}/{}",
                uri_encode(&self.bucket, false),
                uri_encode(k.trim_start_matches('/'), true)
            ),
        };
        let mut pairs: Vec<(String, String)> = query
            .iter()
            .map(|(k, v)| (uri_encode(k, false), uri_encode(v, false)))
            .collect();
        pairs.sort();
        let canonical_query = pairs
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        let url_query = pairs
            .iter()
            .map(|(k, v)| {
                if v.is_empty() {
                    k.clone()
                } else {
                    format!("{k}={v}")
                }
            })
            .collect::<Vec<_>>()
            .join("&");
        let auth = authorization(&SigningInput {
            method,
            canonical_uri: &uri,
            canonical_query: &canonical_query,
            headers: &headers,
            payload_hash: &payload_hash,
            access_key: &self.access_key,
            secret_key: &self.secret_key,
            region: &self.region,
            service: "s3",
            amz_date: &amz_date,
        });
        let url = if url_query.is_empty() {
            format!("{}{uri}", self.endpoint)
        } else {
            format!("{}{uri}?{url_query}", self.endpoint)
        };
        let mut req = match method {
            "PUT" => self.client.put(&url),
            "POST" => self.client.post(&url),
            "DELETE" => self.client.delete(&url),
            "HEAD" => self.client.head(&url),
            _ => self.client.get(&url),
        };
        for (k, v) in &headers {
            if k != "host" {
                req = req.header(k.as_str(), v.as_str());
            }
        }
        for (k, v) in extra {
            if *k == "content-type" {
                req = req.header("content-type", v.as_str());
            }
        }
        req = req.header("authorization", auth);
        if let Some(b) = body {
            req = req.body(b.to_vec());
        }
        if let Some(t) = timeout {
            req = req.timeout(t);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| StorageError::Backend(e.without_url().to_string()))?;
        let status = resp.status().as_u16();
        let headers = resp
            .headers()
            .iter()
            .filter_map(|(k, v)| {
                Some((k.as_str().to_ascii_lowercase(), v.to_str().ok()?.to_owned()))
            })
            .collect();
        let body = resp
            .text()
            .await
            .map_err(|e| StorageError::Backend(e.without_url().to_string()))?;
        Ok(S3Reply {
            status,
            body,
            headers,
        })
    }

    /// A live CORS preflight (`OPTIONS`, unsigned, as a browser sends it) for a `GET` of the probe object
    /// from `origin`. `true` when the service answers with that origin (or `*`) in
    /// `Access-Control-Allow-Origin`. This proves the rule works even where the CORS API is not offered.
    pub async fn preflight_allows(
        &self,
        key: &str,
        origin: &str,
        timeout: Duration,
    ) -> Result<bool> {
        let url = format!(
            "{}/{}/{}",
            self.endpoint,
            uri_encode(&self.bucket, false),
            uri_encode(key.trim_start_matches('/'), true)
        );
        let resp = self
            .client
            .request(reqwest::Method::OPTIONS, &url)
            .header("origin", origin)
            .header("access-control-request-method", "GET")
            .timeout(timeout)
            .send()
            .await
            .map_err(|e| StorageError::Backend(e.without_url().to_string()))?;
        let allow = resp
            .headers()
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        Ok(resp.status().is_success()
            && matches!(allow.as_deref(), Some(a) if a == origin || a == "*"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The worked example from the AWS Signature Version 4 documentation
    /// (IAM `ListUsers`, 2015-08-30T12:36:00Z).
    #[test]
    fn matches_the_published_aws_example() {
        let headers = vec![
            (
                "content-type".to_owned(),
                "application/x-www-form-urlencoded; charset=utf-8".to_owned(),
            ),
            ("host".to_owned(), "iam.amazonaws.com".to_owned()),
            ("x-amz-date".to_owned(), "20150830T123600Z".to_owned()),
        ];
        let auth = authorization(&SigningInput {
            method: "GET",
            canonical_uri: "/",
            canonical_query: "Action=ListUsers&Version=2010-05-08",
            headers: &headers,
            payload_hash: &sha256_hex(b""),
            access_key: "AKIDEXAMPLE",
            secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            region: "us-east-1",
            service: "iam",
            amz_date: "20150830T123600Z",
        });
        assert_eq!(
            auth,
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/iam/aws4_request, \
             SignedHeaders=content-type;host;x-amz-date, \
             Signature=5d672d79c15b13162d9279b0855cfba6789a8edb4c82c400e06b5924a6f2b5d7"
        );
    }

    #[test]
    fn formats_utc_timestamps() {
        assert_eq!(amz_date(0), "19700101T000000Z");
        assert_eq!(amz_date(1_440_938_160), "20150830T123600Z");
        // A leap day.
        assert_eq!(amz_date(1_709_210_096), "20240229T123456Z");
    }

    #[test]
    fn origins_are_normalised_and_wildcards_refused() {
        assert_eq!(
            normalize_origin("https://Chat.Example.org/").unwrap(),
            "https://chat.example.org"
        );
        assert_eq!(
            normalize_origin("http://localhost:8080").unwrap(),
            "http://localhost:8080"
        );
        for bad in [
            "*",
            "https://*.example.org",
            "chat.example.org",
            "ftp://x.test",
            "https://x.test/path",
            "https://u@x.test",
            "",
        ] {
            assert!(normalize_origin(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn cors_document_matches_the_decision() {
        let xml = cors_xml(&["https://chat.example.org".into()]).unwrap();
        assert!(xml.contains("<AllowedOrigin>https://chat.example.org</AllowedOrigin>"));
        for m in ["PUT", "GET", "HEAD"] {
            assert!(xml.contains(&format!("<AllowedMethod>{m}</AllowedMethod>")));
        }
        assert!(xml.contains("<AllowedHeader>content-type</AllowedHeader>"));
        assert!(xml.contains("<ExposeHeader>ETag</ExposeHeader>"));
        assert!(!xml.contains("DELETE"));
        // Deterministic, so repeated calls send identical documents.
        assert_eq!(
            xml,
            cors_xml(&["https://chat.example.org/".into()]).unwrap()
        );
        assert!(cors_xml(&[]).is_err());
    }

    #[test]
    fn each_origin_gets_its_own_rule() {
        // Garage joins a rule's origins into one Access-Control-Allow-Origin header, which browsers refuse
        // (the p22 composer run): every origin must be alone in its rule, and duplicates collapse.
        let xml = cors_xml(&[
            "http://localhost:5173".into(),
            "http://localhost:8080".into(),
            "http://localhost:5173/".into(),
        ])
        .unwrap();
        assert_eq!(xml.matches("<CORSRule>").count(), 2);
        for rule in xml.split("<CORSRule>").skip(1) {
            assert_eq!(rule.matches("<AllowedOrigin>").count(), 1, "{rule}");
            assert!(rule.contains("<AllowedMethod>PUT</AllowedMethod>"));
            assert!(rule.contains("<ExposeHeader>ETag</ExposeHeader>"));
        }
    }

    #[test]
    fn endpoint_host_keeps_the_port() {
        let a = S3Admin::new("http://127.0.0.1:3900/", "b", "garage", "k", "s").unwrap();
        assert_eq!(a.host().unwrap(), "127.0.0.1:3900");
        let a = S3Admin::new("https://s3.example.org", "b", "r", "k", "s").unwrap();
        assert_eq!(a.host().unwrap(), "s3.example.org");
    }
}
