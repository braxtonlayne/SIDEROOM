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
/// Request headers the browser may send on a presigned upload.
pub const CORS_ALLOWED_HEADERS: [&str; 1] = ["Content-Type"];
/// Response headers the page may read.
pub const CORS_EXPOSE_HEADERS: [&str; 1] = ["ETag"];
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

/// The `CORSConfiguration` document for `origins` (one rule, D-175).
pub fn cors_xml(origins: &[String]) -> Result<String> {
    if origins.is_empty() {
        return Err(StorageError::Backend(
            "at least one origin is required".into(),
        ));
    }
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><CORSConfiguration xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><CORSRule>",
    );
    for o in origins {
        xml.push_str(&format!(
            "<AllowedOrigin>{}</AllowedOrigin>",
            xml_escape(&normalize_origin(o)?)
        ));
    }
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
        "<MaxAgeSeconds>{CORS_MAX_AGE_SECONDS}</MaxAgeSeconds></CORSRule></CORSConfiguration>"
    ));
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
    /// to `PUT`, `GET` and `HEAD` (`Content-Type` allowed, `ETag` exposed).
    /// Repeating the call is harmless: S3 replaces the configuration.
    pub async fn put_bucket_cors(&self, origins: &[String]) -> Result<()> {
        let body = cors_xml(origins)?;
        let checksum = STANDARD.encode(Sha256::digest(body.as_bytes()));
        self.send("PUT", Some((body, checksum))).await.map(|_| ())
    }

    /// The raw CORS configuration, or `None` when the bucket has none.
    pub async fn get_bucket_cors(&self) -> Result<Option<String>> {
        self.send("GET", None).await
    }

    async fn send(&self, method: &str, body: Option<(String, String)>) -> Result<Option<String>> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let amz_date = amz_date(now);
        let host = self.host()?;
        let payload = body.as_ref().map_or("", |(b, _)| b.as_str());
        let payload_hash = sha256_hex(payload.as_bytes());
        let mut headers: Vec<(String, String)> = vec![
            ("host".into(), host),
            ("x-amz-content-sha256".into(), payload_hash.clone()),
            ("x-amz-date".into(), amz_date.clone()),
        ];
        if let Some((_, checksum)) = &body {
            // S3 requires an integrity header on PutBucketCors (Content-MD5 or a
            // flexible checksum); SHA-256 needs no extra crate.
            headers.push(("x-amz-checksum-sha256".into(), checksum.clone()));
            headers.push(("x-amz-sdk-checksum-algorithm".into(), "SHA256".into()));
        }
        headers.sort();
        let uri = format!("/{}", self.bucket);
        let auth = authorization(&SigningInput {
            method,
            canonical_uri: &uri,
            canonical_query: "cors=",
            headers: &headers,
            payload_hash: &payload_hash,
            access_key: &self.access_key,
            secret_key: &self.secret_key,
            region: &self.region,
            service: "s3",
            amz_date: &amz_date,
        });
        let url = format!("{}{uri}?cors", self.endpoint);
        let mut req = match method {
            "PUT" => self.client.put(&url),
            _ => self.client.get(&url),
        };
        for (k, v) in &headers {
            if k != "host" {
                req = req.header(k.as_str(), v.as_str());
            }
        }
        req = req.header("authorization", auth);
        if let Some((b, _)) = body {
            req = req.header("content-type", "application/xml").body(b);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| StorageError::Backend(e.without_url().to_string()))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| StorageError::Backend(e.without_url().to_string()))?;
        if status.is_success() {
            return Ok(Some(text));
        }
        if method == "GET" && status.as_u16() == 404 && text.contains("NoSuchCORSConfiguration") {
            return Ok(None);
        }
        // Do not echo the whole body: it can be large and is not ours.
        let snippet: String = text.chars().take(300).collect();
        Err(StorageError::Backend(format!(
            "{method} bucket cors failed with {status}: {snippet}"
        )))
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
        assert!(xml.contains("<AllowedHeader>Content-Type</AllowedHeader>"));
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
    fn endpoint_host_keeps_the_port() {
        let a = S3Admin::new("http://127.0.0.1:3900/", "b", "garage", "k", "s").unwrap();
        assert_eq!(a.host().unwrap(), "127.0.0.1:3900");
        let a = S3Admin::new("https://s3.example.org", "b", "r", "k", "s").unwrap();
        assert_eq!(a.host().unwrap(), "s3.example.org");
    }
}
