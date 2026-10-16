//! A small keep-alive HTTP/1.1 client, enough to drive the server's JSON API without a general HTTP client
//! (the workspace keeps `reqwest` out of everything that is not a vetted outbound path, see `deny.toml`).
//! Plain `http://` only: a load test runs against a node or a private address, never through TLS.

use anyhow::{Context, Result, bail};
use std::sync::Mutex;
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::timeout,
};

/// One response.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Parsed response head.
#[derive(Debug, PartialEq, Eq)]
pub struct Head {
    pub status: u16,
    pub content_length: Option<usize>,
    pub chunked: bool,
    pub close: bool,
    /// Bytes of the head including the blank line.
    pub len: usize,
}

/// Parse `HTTP/1.1 200 OK\r\n...headers...\r\n\r\n` from the start of `buf`, if it is complete.
pub fn parse_head(buf: &[u8]) -> Result<Option<Head>> {
    let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
        return Ok(None);
    };
    let text = std::str::from_utf8(&buf[..end]).context("the response head is not UTF-8")?;
    let mut lines = text.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .with_context(|| format!("not an HTTP status line: {status_line:?}"))?;
    let mut head = Head {
        status,
        content_length: None,
        chunked: false,
        close: status_line.starts_with("HTTP/1.0"),
        len: end + 4,
    };
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.to_ascii_lowercase().as_str() {
            "content-length" => head.content_length = value.parse().ok(),
            "transfer-encoding" => head.chunked = value.to_ascii_lowercase().contains("chunked"),
            "connection" => {
                if value.eq_ignore_ascii_case("close") {
                    head.close = true;
                } else if value.eq_ignore_ascii_case("keep-alive") {
                    head.close = false;
                }
            }
            _ => {}
        }
    }
    Ok(Some(head))
}

/// Decode a complete chunked body; `None` while it is still incomplete.
pub fn decode_chunked(buf: &[u8]) -> Result<Option<(Vec<u8>, usize)>> {
    let mut out = Vec::new();
    let mut at = 0;
    loop {
        let Some(nl) = buf[at..].windows(2).position(|w| w == b"\r\n") else {
            return Ok(None);
        };
        let size_text = std::str::from_utf8(&buf[at..at + nl]).context("chunk size")?;
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16)
            .with_context(|| format!("bad chunk size {size_text:?}"))?;
        at += nl + 2;
        if size == 0 {
            // Trailers end with a blank line.
            return Ok(buf[at..]
                .windows(2)
                .position(|w| w == b"\r\n")
                .map(|p| (out, at + p + 2)));
        }
        if buf.len() < at + size + 2 {
            return Ok(None);
        }
        out.extend_from_slice(&buf[at..at + size]);
        at += size + 2;
    }
}

pub struct Client {
    addr: String,
    host: String,
    idle: Mutex<Vec<TcpStream>>,
    pub request_timeout: Duration,
}

impl Client {
    /// `base` is `http://host:port` (a path part is ignored).
    pub fn new(base: &str) -> Result<Self> {
        let rest = base
            .strip_prefix("http://")
            .context("--base-url must start with http:// (a load test does not use TLS)")?;
        let host = rest.split('/').next().unwrap_or("").to_owned();
        if host.is_empty() {
            bail!("--base-url has no host");
        }
        let addr = if host.contains(':') {
            host.clone()
        } else {
            format!("{host}:80")
        };
        Ok(Self {
            addr,
            host,
            idle: Mutex::new(Vec::new()),
            request_timeout: Duration::from_secs(30),
        })
    }

    fn take_idle(&self) -> Option<TcpStream> {
        self.idle.lock().unwrap_or_else(|p| p.into_inner()).pop()
    }

    fn put_idle(&self, s: TcpStream) {
        let mut g = self.idle.lock().unwrap_or_else(|p| p.into_inner());
        if g.len() < 256 {
            g.push(s);
        }
    }

    /// Send one request and read the whole response. A kept connection the server closed meanwhile is
    /// retried once on a fresh one.
    pub async fn request(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<&[u8]>,
    ) -> Result<Response> {
        let mut req = format!("{method} {path} HTTP/1.1\r\nHost: {}\r\n", self.host);
        for (k, v) in headers {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        match body {
            Some(b) => req.push_str(&format!("Content-Length: {}\r\n\r\n", b.len())),
            None => req.push_str("\r\n"),
        }
        let mut wire = req.into_bytes();
        if let Some(b) = body {
            wire.extend_from_slice(b);
        }

        if let Some(mut s) = self.take_idle() {
            // A stale kept connection falls through to a fresh one.
            if let Ok(Ok((resp, keep))) =
                timeout(self.request_timeout, exchange(&mut s, &wire)).await
            {
                if keep {
                    self.put_idle(s);
                }
                return Ok(resp);
            }
        }
        let mut s = timeout(self.request_timeout, TcpStream::connect(&self.addr))
            .await
            .context("connect timed out")?
            .with_context(|| format!("connecting to {}", self.addr))?;
        s.set_nodelay(true).ok();
        let (resp, keep) = timeout(self.request_timeout, exchange(&mut s, &wire))
            .await
            .context("request timed out")??;
        if keep {
            self.put_idle(s);
        }
        Ok(resp)
    }
}

/// Write the request and read one full response. The bool says whether the connection may be reused.
async fn exchange(s: &mut TcpStream, wire: &[u8]) -> Result<(Response, bool)> {
    s.write_all(wire).await?;
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut chunk = [0u8; 8192];
    let head = loop {
        if let Some(h) = parse_head(&buf)? {
            break h;
        }
        let n = s.read(&mut chunk).await?;
        if n == 0 {
            bail!("connection closed before the response head");
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let mut body_buf = buf.split_off(head.len);
    let body = if head.chunked {
        loop {
            if let Some((b, _)) = decode_chunked(&body_buf)? {
                break b;
            }
            let n = s.read(&mut chunk).await?;
            if n == 0 {
                bail!("connection closed inside a chunked body");
            }
            body_buf.extend_from_slice(&chunk[..n]);
        }
    } else if let Some(len) = head.content_length {
        while body_buf.len() < len {
            let n = s.read(&mut chunk).await?;
            if n == 0 {
                bail!("connection closed inside the body");
            }
            body_buf.extend_from_slice(&chunk[..n]);
        }
        body_buf.truncate(len);
        body_buf
    } else if matches!(head.status, 204 | 304) || head.status < 200 {
        Vec::new()
    } else {
        // No length: the body runs to the end of the stream.
        loop {
            let n = s.read(&mut chunk).await?;
            if n == 0 {
                break;
            }
            body_buf.extend_from_slice(&chunk[..n]);
        }
        body_buf
    };
    let keep = !head.close && (head.chunked || head.content_length.is_some() || head.status == 204);
    Ok((
        Response {
            status: head.status,
            body,
        },
        keep,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[test]
    fn parses_a_response_head() {
        let raw: &[u8] =
            b"HTTP/1.1 201 Created\r\nContent-Length: 12\r\nConnection: keep-alive\r\n\r\n";
        let h = parse_head(&[raw, b"rest"].concat()).unwrap().unwrap();
        assert_eq!(
            h,
            Head {
                status: 201,
                content_length: Some(12),
                chunked: false,
                close: false,
                len: raw.len(),
            }
        );
        assert!(
            parse_head(b"HTTP/1.1 200 OK\r\nContent-Le")
                .unwrap()
                .is_none()
        );
        assert!(parse_head(b"nonsense\r\n\r\n").is_err());
        let close = parse_head(b"HTTP/1.1 200 OK\r\nconnection: close\r\n\r\n")
            .unwrap()
            .unwrap();
        assert!(close.close);
        let chunked = parse_head(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
            .unwrap()
            .unwrap();
        assert!(chunked.chunked);
    }

    #[test]
    fn decodes_chunked_bodies() {
        let (body, used) = decode_chunked(b"4\r\nWiki\r\n5;ext=1\r\npedia\r\n0\r\n\r\ntail")
            .unwrap()
            .unwrap();
        assert_eq!(body, b"Wikipedia");
        assert_eq!(used, 30);
        assert!(decode_chunked(b"4\r\nWi").unwrap().is_none());
        assert!(decode_chunked(b"zz\r\n").is_err());
    }

    /// A server that answers every request on one connection, then lets it idle.
    async fn fake_server() -> (String, tokio::task::JoinHandle<usize>) {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = format!("http://{}", l.local_addr().unwrap());
        let h = tokio::spawn(async move {
            let mut connections = 0;
            while let Ok((mut c, _)) = l.accept().await {
                connections += 1;
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let mut n = 0;
                    loop {
                        let r = c.read(&mut buf[n..]).await.unwrap_or(0);
                        if r == 0 {
                            return;
                        }
                        n += r;
                        while let Some(p) = buf[..n].windows(4).position(|w| w == b"\r\n\r\n") {
                            let head = String::from_utf8_lossy(&buf[..p]).to_string();
                            let body = if head.starts_with("GET /chunked") {
                                "5\r\nhello\r\n0\r\n\r\n".to_owned()
                            } else {
                                "{\"ok\":true}".to_owned()
                            };
                            let resp = if head.starts_with("GET /chunked") {
                                format!(
                                    "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{body}"
                                )
                            } else if head.starts_with("GET /empty") {
                                "HTTP/1.1 204 No Content\r\n\r\n".to_owned()
                            } else {
                                format!(
                                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                                    body.len()
                                )
                            };
                            c.write_all(resp.as_bytes()).await.unwrap();
                            buf.copy_within(p + 4..n, 0);
                            n -= p + 4;
                        }
                    }
                });
            }
            connections
        });
        (addr, h)
    }

    #[tokio::test]
    async fn reuses_one_connection_and_reads_every_body_kind() {
        let (base, server) = fake_server().await;
        let c = Client::new(&base).unwrap();
        for _ in 0..3 {
            let r = c.request("GET", "/x", &[("X-A", "1")], None).await.unwrap();
            assert_eq!((r.status, r.body.as_slice()), (200, &b"{\"ok\":true}"[..]));
        }
        let ch = c.request("GET", "/chunked", &[], None).await.unwrap();
        assert_eq!(ch.body, b"hello");
        let none = c.request("GET", "/empty", &[], None).await.unwrap();
        assert_eq!((none.status, none.body.len()), (204, 0));
        let post = c.request("POST", "/x", &[], Some(b"{}")).await.unwrap();
        assert_eq!(post.status, 200);
        assert_eq!(
            c.idle.lock().unwrap().len(),
            1,
            "one connection served them all"
        );
        server.abort();
    }

    #[tokio::test]
    async fn refuses_non_http_urls_and_reports_a_dead_server() {
        assert!(Client::new("https://x").is_err());
        assert!(Client::new("http://").is_err());
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = format!("http://{}", l.local_addr().unwrap());
        drop(l);
        let c = Client::new(&addr).unwrap();
        assert!(c.request("GET", "/", &[], None).await.is_err());
    }
}
