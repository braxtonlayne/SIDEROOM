//! The real reqwest transport against a local HTTP server: the image proxy reads
//! the whole body up to its cap and refuses anything bigger, whether the size is
//! declared or only discovered while streaming.

use sr_unfurl::fetch::{FetchError, MAX_PROXY_IMAGE_BYTES, SafeFetcher, SystemResolver};
use sr_unfurl::policy::Policy;
use sr_unfurl::transport::ReqwestTransport;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

/// Serve `/sized` (Content-Length), `/chunked` (no length, closes at the end) and
/// `/html`, each carrying `len` bytes of body where it applies.
async fn serve(listener: TcpListener, len: usize) {
    loop {
        let Ok((mut sock, _)) = listener.accept().await else {
            return;
        };
        tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let path = req.split_whitespace().nth(1).unwrap_or("/").to_owned();
            let body = vec![0x89u8; len];
            let head = match path.as_str() {
                "/sized" => format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: image/png\r\ncontent-length: {len}\r\nconnection: close\r\n\r\n"
                ),
                "/chunked" => {
                    "HTTP/1.1 200 OK\r\ncontent-type: image/png\r\nconnection: close\r\n\r\n"
                        .to_owned()
                }
                "/html" => "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: 5\r\nconnection: close\r\n\r\nhello".to_owned(),
                _ => "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                    .to_owned(),
            };
            let _ = sock.write_all(head.as_bytes()).await;
            if matches!(path.as_str(), "/sized" | "/chunked") {
                let _ = sock.write_all(&body).await;
            }
            let _ = sock.shutdown().await;
        });
    }
}

async fn fetcher_for(len: usize) -> (SafeFetcher, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(serve(listener, len));
    let policy = Policy {
        allow_private_for_tests: true,
        ..Policy::default()
    };
    (
        SafeFetcher::new(policy, Arc::new(SystemResolver), Arc::new(ReqwestTransport)),
        port,
    )
}

fn url(port: u16, path: &str) -> Url {
    Url::parse(&format!("http://127.0.0.1:{port}{path}")).unwrap()
}

#[tokio::test]
async fn reads_a_large_body_completely() {
    let (f, port) = fetcher_for(3 * 1024 * 1024).await;
    let got = f
        .fetch_image(&url(port, "/sized"), MAX_PROXY_IMAGE_BYTES)
        .await
        .unwrap();
    assert_eq!(got.body.len(), 3 * 1024 * 1024);
    let got = f
        .fetch_image(&url(port, "/chunked"), MAX_PROXY_IMAGE_BYTES)
        .await
        .unwrap();
    assert_eq!(got.body.len(), 3 * 1024 * 1024);
}

#[tokio::test]
async fn refuses_a_body_over_the_cap() {
    let (f, port) = fetcher_for(300 * 1024).await;
    // Declared by Content-Length.
    assert_eq!(
        f.fetch_image(&url(port, "/sized"), 100 * 1024)
            .await
            .unwrap_err(),
        FetchError::TooLarge
    );
    // Only discovered while streaming.
    assert_eq!(
        f.fetch_image(&url(port, "/chunked"), 100 * 1024)
            .await
            .unwrap_err(),
        FetchError::TooLarge
    );
    // Exactly at the cap is fine.
    let at = f
        .fetch_image(&url(port, "/chunked"), 300 * 1024)
        .await
        .unwrap();
    assert_eq!(at.body.len(), 300 * 1024);
}

#[tokio::test]
async fn refuses_non_images_and_errors() {
    let (f, port) = fetcher_for(10).await;
    assert_eq!(
        f.fetch_image(&url(port, "/html"), 1024).await.unwrap_err(),
        FetchError::UnsupportedType
    );
    assert_eq!(
        f.fetch_image(&url(port, "/missing"), 1024)
            .await
            .unwrap_err(),
        FetchError::BadStatus(404)
    );
}

#[tokio::test]
async fn the_default_policy_refuses_the_loopback_server() {
    let (_, port) = fetcher_for(10).await;
    let f = SafeFetcher::new(
        Policy::default(),
        Arc::new(SystemResolver),
        Arc::new(ReqwestTransport),
    );
    let err = f.fetch_image(&url(port, "/sized"), 1024).await.unwrap_err();
    assert!(matches!(err, FetchError::Blocked(_)), "{err:?}");
}
