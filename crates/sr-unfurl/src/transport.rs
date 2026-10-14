//! The real [`HttpTransport`]: reqwest over rustls, pinned to validated addresses.

use crate::fetch::{
    BodyClass, FetchError, HopRequest, HopResponse, HttpTransport, IMAGE_HEAD_BYTES,
    MAX_HTML_BYTES, MAX_IMAGE_BYTES, MAX_JSON_BYTES, USER_AGENT, classify,
};
use async_trait::async_trait;
use reqwest::header::{ACCEPT, CONTENT_LENGTH, CONTENT_TYPE, LOCATION};

/// One short-lived client per hop. The client never follows redirects, never reads
/// proxy settings from the environment, and resolves the target host only to the
/// addresses the caller validated.
#[derive(Debug, Default, Clone, Copy)]
pub struct ReqwestTransport;

fn http_err(e: &reqwest::Error) -> FetchError {
    if e.is_timeout() {
        FetchError::Timeout
    } else {
        // `without_url` keeps query strings (which may carry tokens) out of logs.
        FetchError::Http(e.to_string())
    }
}

#[async_trait]
impl HttpTransport for ReqwestTransport {
    async fn get(&self, req: &HopRequest) -> Result<HopResponse, FetchError> {
        let client = sr_net::pinned_builder(USER_AGENT, &req.url, &req.addrs)
            .build()
            .map_err(|e| FetchError::Http(e.to_string()))?;
        let mut resp = client
            .get(req.url.clone())
            .header(ACCEPT, req.accept)
            .send()
            .await
            .map_err(|e| http_err(&e.without_url()))?;

        let status = resp.status().as_u16();
        let header = |name| {
            resp.headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        let content_type = header(CONTENT_TYPE);
        let location = header(LOCATION);
        let content_length = header(CONTENT_LENGTH).and_then(|v| v.parse::<u64>().ok());
        let mut out = HopResponse {
            status,
            content_type,
            location,
            body: Vec::new(),
        };
        if !(200..300).contains(&status) {
            return Ok(out);
        }
        let Some(class) = classify(out.content_type.as_deref()) else {
            return Err(FetchError::UnsupportedType);
        };
        // Refuse by declared size first, then enforce the cap while streaming.
        let (declared_cap, read_cap) = match class {
            BodyClass::Html => (MAX_HTML_BYTES as u64, MAX_HTML_BYTES),
            BodyClass::Json => (MAX_JSON_BYTES as u64, MAX_JSON_BYTES),
            BodyClass::Image => match req.full_image_cap {
                Some(n) => (n as u64, n),
                None => (MAX_IMAGE_BYTES, IMAGE_HEAD_BYTES),
            },
            BodyClass::Video => return Ok(out),
        };
        if class == BodyClass::Json && content_length.is_some_and(|n| n > declared_cap) {
            return Err(FetchError::TooLarge);
        }
        if class == BodyClass::Image && content_length.is_some_and(|n| n > declared_cap) {
            return Err(FetchError::TooLarge);
        }
        let full = class == BodyClass::Image && req.full_image_cap.is_some();
        while out.body.len() < read_cap {
            match resp.chunk().await.map_err(|e| http_err(&e.without_url()))? {
                Some(chunk) => {
                    let room = read_cap - out.body.len();
                    out.body.extend_from_slice(&chunk[..chunk.len().min(room)]);
                    // A full read that fills the cap and still has bytes left is too large.
                    if full && chunk.len() > room {
                        return Err(FetchError::TooLarge);
                    }
                }
                None => break,
            }
        }
        if full
            && out.body.len() == read_cap
            && resp
                .chunk()
                .await
                .map_err(|e| http_err(&e.without_url()))?
                .is_some()
        {
            return Err(FetchError::TooLarge);
        }
        Ok(out)
    }
}
