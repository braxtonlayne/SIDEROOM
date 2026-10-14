//! S3 multipart upload with part-level presigning (CIN-ING-004, spec 4.4).
//!
//! `object_store` can presign whole-object requests only. The transcode workers upload large
//! outputs straight to the bucket with one presigned `UploadPart` URL per part, so the server
//! needs three calls that library does not offer:
//!
//! * `CreateMultipartUpload` (`POST /<bucket>/<key>?uploads`), signed with an `Authorization`
//!   header, returns the upload id;
//! * [`S3Multipart::presign_part`], a query-string SigV4 URL (`UploadPart`, `UNSIGNED-PAYLOAD`)
//!   that the worker `PUT`s the part to; the part's `ETag` is in the response headers;
//! * `CompleteMultipartUpload` (`POST ...?uploadId=`), signed, with the part list.
//!
//! The signer is the hand-written SigV4 of [`crate::s3admin`] (only `hmac` and `sha2`); the HTTP
//! exchange uses `reqwest` with rustls. Path-style addressing, like the rest of the storage layer.
//! Nothing here ever logs or returns the secret key.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::{Result, StorageError};
use crate::s3admin::{SigningInput, amz_date, authorization, sha256_hex, xml_escape};

/// Largest part number S3 accepts.
pub const MAX_PART_NUMBER: u32 = 10_000;
/// Longest a presigned URL may live (S3 allows 7 days).
pub const MAX_PRESIGN_SECS: u64 = 7 * 24 * 3600;

/// Percent-encode like SigV4 wants: everything but `A-Z a-z 0-9 - _ . ~`; `/` kept when
/// `keep_slash`.
pub(crate) fn uri_encode(s: &str, keep_slash: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else if b == b'/' && keep_slash {
            out.push('/');
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Query string of a presigned request: the given parameters plus the `X-Amz-*` set and the
/// signature, sorted as SigV4 requires. Exposed to the tests with the AWS documentation vector.
#[allow(clippy::too_many_arguments)]
pub(crate) fn presigned_query(
    method: &str,
    host: &str,
    canonical_uri: &str,
    params: &[(&str, &str)],
    access_key: &str,
    secret_key: &str,
    region: &str,
    amz_date: &str,
    expires_secs: u64,
) -> String {
    let date = &amz_date[..8];
    let credential = format!("{access_key}/{date}/{region}/s3/aws4_request");
    let expires = expires_secs.to_string();
    let mut all: Vec<(String, String)> = params
        .iter()
        .map(|(k, v)| (uri_encode(k, false), uri_encode(v, false)))
        .collect();
    for (k, v) in [
        ("X-Amz-Algorithm", "AWS4-HMAC-SHA256"),
        ("X-Amz-Credential", credential.as_str()),
        ("X-Amz-Date", amz_date),
        ("X-Amz-Expires", expires.as_str()),
        ("X-Amz-SignedHeaders", "host"),
    ] {
        all.push((uri_encode(k, false), uri_encode(v, false)));
    }
    all.sort();
    let canonical_query = all
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    let headers = vec![("host".to_owned(), host.to_owned())];
    let auth = authorization(&SigningInput {
        method,
        canonical_uri,
        canonical_query: &canonical_query,
        headers: &headers,
        payload_hash: "UNSIGNED-PAYLOAD",
        access_key,
        secret_key,
        region,
        service: "s3",
        amz_date,
    });
    let signature = auth.rsplit("Signature=").next().unwrap_or_default();
    format!("{canonical_query}&X-Amz-Signature={signature}")
}

/// Multipart calls against an S3-compatible service.
#[derive(Clone)]
pub struct S3Multipart {
    endpoint: String,
    bucket: String,
    region: String,
    access_key: String,
    secret_key: String,
    client: reqwest::Client,
}

impl std::fmt::Debug for S3Multipart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Multipart")
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .finish_non_exhaustive()
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Object keys are produced by the server, but a bad one must never reach a signed request.
fn check_key(key: &str) -> Result<()> {
    let bad = key.is_empty()
        || key.len() > 1024
        || key.starts_with('/')
        || key
            .split('/')
            .any(|c| c.is_empty() || c == "." || c == "..")
        || key.chars().any(char::is_control);
    if bad {
        return Err(StorageError::Backend("invalid object key".into()));
    }
    Ok(())
}

fn check_upload_id(id: &str) -> Result<()> {
    if id.is_empty() || id.len() > 1024 || id.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(StorageError::Backend("invalid upload id".into()));
    }
    Ok(())
}

impl S3Multipart {
    /// `endpoint` like `http://127.0.0.1:3900`, path-style.
    pub fn new(
        endpoint: &str,
        bucket: &str,
        region: &str,
        access_key: &str,
        secret_key: &str,
    ) -> Result<Self> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(60))
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

    fn uri(&self, key: &str) -> String {
        format!(
            "/{}/{}",
            uri_encode(&self.bucket, false),
            uri_encode(key, true)
        )
    }

    /// A presigned `PUT` URL for one part. The worker sends the bytes with `PUT` and reads the
    /// `ETag` response header. `ttl` is clamped to what S3 accepts.
    pub fn presign_part(
        &self,
        key: &str,
        upload_id: &str,
        part_number: u32,
        ttl: Duration,
    ) -> Result<String> {
        self.presign_part_at(key, upload_id, part_number, ttl, now_secs())
    }

    /// [`Self::presign_part`] at an explicit time (tests).
    pub fn presign_part_at(
        &self,
        key: &str,
        upload_id: &str,
        part_number: u32,
        ttl: Duration,
        unix_secs: u64,
    ) -> Result<String> {
        check_key(key)?;
        check_upload_id(upload_id)?;
        if part_number == 0 || part_number > MAX_PART_NUMBER {
            return Err(StorageError::Backend("part number out of range".into()));
        }
        let expires = ttl.as_secs().clamp(1, MAX_PRESIGN_SECS);
        let part = part_number.to_string();
        let uri = self.uri(key);
        let query = presigned_query(
            "PUT",
            &self.host()?,
            &uri,
            &[("partNumber", part.as_str()), ("uploadId", upload_id)],
            &self.access_key,
            &self.secret_key,
            &self.region,
            &amz_date(unix_secs),
            expires,
        );
        Ok(format!("{}{uri}?{query}", self.endpoint))
    }

    /// Start a multipart upload and return its id.
    pub async fn create(&self, key: &str, content_type: &str) -> Result<String> {
        check_key(key)?;
        let body = self
            .send(
                "POST",
                key,
                "uploads=",
                "uploads",
                Some(content_type),
                Vec::new(),
            )
            .await?;
        let id = between(&body, "<UploadId>", "</UploadId>")
            .ok_or_else(|| StorageError::Backend("no UploadId in the response".into()))?;
        let id = xml_unescape(id);
        check_upload_id(&id)?;
        Ok(id)
    }

    /// Finish the upload with the part ETags (as the part responses returned them).
    pub async fn complete(
        &self,
        key: &str,
        upload_id: &str,
        parts: &[(u32, String)],
    ) -> Result<()> {
        check_key(key)?;
        check_upload_id(upload_id)?;
        if parts.is_empty() {
            return Err(StorageError::Backend("no parts".into()));
        }
        let mut sorted = parts.to_vec();
        sorted.sort_by_key(|p| p.0);
        let mut xml = String::from("<CompleteMultipartUpload>");
        for (n, etag) in &sorted {
            if *n == 0 || *n > MAX_PART_NUMBER || etag.is_empty() || etag.len() > 200 {
                return Err(StorageError::Backend("invalid part".into()));
            }
            xml.push_str(&format!(
                "<Part><PartNumber>{n}</PartNumber><ETag>{}</ETag></Part>",
                xml_escape(etag)
            ));
        }
        xml.push_str("</CompleteMultipartUpload>");
        let q = format!("uploadId={}", uri_encode(upload_id, false));
        let body = self
            .send(
                "POST",
                key,
                &q,
                &q,
                Some("application/xml"),
                xml.into_bytes(),
            )
            .await?;
        // S3 may answer 200 with an <Error> document when the assembly fails.
        if body.contains("<Error>") {
            let snippet: String = body.chars().take(300).collect();
            return Err(StorageError::Backend(format!(
                "CompleteMultipartUpload failed: {snippet}"
            )));
        }
        Ok(())
    }

    /// Abandon an upload and free its parts.
    pub async fn abort(&self, key: &str, upload_id: &str) -> Result<()> {
        check_key(key)?;
        check_upload_id(upload_id)?;
        let q = format!("uploadId={}", uri_encode(upload_id, false));
        self.send("DELETE", key, &q, &q, None, Vec::new())
            .await
            .map(|_| ())
    }

    async fn send(
        &self,
        method: &str,
        key: &str,
        canonical_query: &str,
        url_query: &str,
        content_type: Option<&str>,
        body: Vec<u8>,
    ) -> Result<String> {
        let amz = amz_date(now_secs());
        let payload_hash = sha256_hex(&body);
        let mut headers: Vec<(String, String)> = vec![
            ("host".into(), self.host()?),
            ("x-amz-content-sha256".into(), payload_hash.clone()),
            ("x-amz-date".into(), amz.clone()),
        ];
        headers.sort();
        let uri = self.uri(key);
        let auth = authorization(&SigningInput {
            method,
            canonical_uri: &uri,
            canonical_query,
            headers: &headers,
            payload_hash: &payload_hash,
            access_key: &self.access_key,
            secret_key: &self.secret_key,
            region: &self.region,
            service: "s3",
            amz_date: &amz,
        });
        let url = format!("{}{uri}?{url_query}", self.endpoint);
        let mut req = match method {
            "POST" => self.client.post(&url),
            "DELETE" => self.client.delete(&url),
            _ => self.client.get(&url),
        };
        for (k, v) in &headers {
            if k != "host" {
                req = req.header(k.as_str(), v.as_str());
            }
        }
        req = req.header("authorization", auth);
        if let Some(ct) = content_type {
            req = req.header("content-type", ct);
        }
        if !body.is_empty() {
            req = req.body(body);
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
            return Ok(text);
        }
        let snippet: String = text.chars().take(300).collect();
        Err(StorageError::Backend(format!(
            "{method} multipart call failed with {status}: {snippet}"
        )))
    }
}

fn between<'a>(s: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = s.find(open)? + open.len();
    let end = s[start..].find(close)? + start;
    Some(&s[start..end])
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The "Presigned URL" example from the AWS Signature Version 4 documentation for S3
    /// (query string authentication): GET `examplebucket/test.txt`, 86400 seconds.
    #[test]
    fn matches_the_aws_presigned_url_example() {
        let q = presigned_query(
            "GET",
            "examplebucket.s3.amazonaws.com",
            "/test.txt",
            &[],
            "AKIAIOSFODNN7EXAMPLE",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            "us-east-1",
            "20130524T000000Z",
            86_400,
        );
        assert_eq!(
            q,
            "X-Amz-Algorithm=AWS4-HMAC-SHA256\
             &X-Amz-Credential=AKIAIOSFODNN7EXAMPLE%2F20130524%2Fus-east-1%2Fs3%2Faws4_request\
             &X-Amz-Date=20130524T000000Z&X-Amz-Expires=86400&X-Amz-SignedHeaders=host\
             &X-Amz-Signature=aeeed9bbccd4d02ee5c0109b86d86835f995330da4c265957d157751f604d404"
        );
    }

    /// The "GET Object" example (Range header) from the same documentation page checks the
    /// header-signed path used by `CreateMultipartUpload` / `CompleteMultipartUpload`.
    #[test]
    fn matches_the_aws_get_object_example() {
        let headers = vec![
            (
                "host".to_owned(),
                "examplebucket.s3.amazonaws.com".to_owned(),
            ),
            ("range".to_owned(), "bytes=0-9".to_owned()),
            (
                "x-amz-content-sha256".to_owned(),
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_owned(),
            ),
            ("x-amz-date".to_owned(), "20130524T000000Z".to_owned()),
        ];
        let auth = authorization(&SigningInput {
            method: "GET",
            canonical_uri: "/test.txt",
            canonical_query: "",
            headers: &headers,
            payload_hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            access_key: "AKIAIOSFODNN7EXAMPLE",
            secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            region: "us-east-1",
            service: "s3",
            amz_date: "20130524T000000Z",
        });
        assert_eq!(
            auth,
            "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, \
             SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, \
             Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
    }

    #[test]
    fn part_urls_are_path_style_and_carry_part_and_upload_id() {
        let m = S3Multipart::new(
            "http://127.0.0.1:3900/",
            "sideroom",
            "garage",
            "GKexample",
            "sec",
        )
        .unwrap();
        let url = m
            .presign_part_at(
                "transcode/12/1/v/1080p/seg 1.m4s",
                "up+id/==",
                3,
                Duration::from_secs(3600),
                1_440_938_160,
            )
            .unwrap();
        assert!(
            url.starts_with("http://127.0.0.1:3900/sideroom/transcode/12/1/v/1080p/seg%201.m4s?")
        );
        assert!(url.contains("partNumber=3"));
        assert!(url.contains("uploadId=up%2Bid%2F%3D%3D"));
        assert!(url.contains("X-Amz-Expires=3600"));
        assert!(url.contains("X-Amz-Date=20150830T123600Z"));
        assert!(url.contains("X-Amz-SignedHeaders=host"));
        assert!(url.contains("X-Amz-Credential=GKexample%2F20150830%2Fgarage%2Fs3%2Faws4_request"));
        assert!(!url.contains("sec&") && !url.ends_with("sec"));
        // Deterministic for the same inputs.
        let again = m
            .presign_part_at(
                "transcode/12/1/v/1080p/seg 1.m4s",
                "up+id/==",
                3,
                Duration::from_secs(3600),
                1_440_938_160,
            )
            .unwrap();
        assert_eq!(url, again);
    }

    #[test]
    fn bad_keys_parts_and_ids_are_refused() {
        let m = S3Multipart::new("http://x:1", "b", "r", "k", "s").unwrap();
        let t = Duration::from_secs(60);
        for key in ["", "/abs", "a//b", "a/../b", "a/./b", "a\nb"] {
            assert!(m.presign_part_at(key, "u", 1, t, 0).is_err(), "{key:?}");
        }
        assert!(m.presign_part_at("a", "u", 0, t, 0).is_err());
        assert!(m.presign_part_at("a", "u", 10_001, t, 0).is_err());
        assert!(m.presign_part_at("a", "", 1, t, 0).is_err());
        assert!(m.presign_part_at("a", "u v", 1, t, 0).is_err());
        // A long ttl is clamped to seven days.
        let url = m
            .presign_part_at("a", "u", 1, Duration::from_secs(99 * 86_400), 0)
            .unwrap();
        assert!(url.contains("X-Amz-Expires=604800"));
    }

    #[test]
    fn xml_helpers() {
        assert_eq!(
            between(
                "<a><UploadId>x&amp;y</UploadId></a>",
                "<UploadId>",
                "</UploadId>"
            ),
            Some("x&amp;y")
        );
        assert_eq!(xml_unescape("x&amp;y&lt;"), "x&y<");
        assert_eq!(uri_encode("a b/c~d", true), "a%20b/c~d");
        assert_eq!(uri_encode("a/b", false), "a%2Fb");
    }
}
