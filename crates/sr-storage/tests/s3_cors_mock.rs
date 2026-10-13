//! `S3Admin::put_bucket_cors` against an in-process fake S3 endpoint: the request
//! is path-style, signed, carries the CORS document and a checksum header, and a
//! repeated call sends the identical document (idempotent).

use sr_storage::s3admin::S3Admin;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Accept `n` connections, answer each with `status_line`, return the raw requests.
async fn fake_s3(listener: TcpListener, n: usize, status_line: &'static str) -> Vec<String> {
    let mut seen = Vec::new();
    for _ in 0..n {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let read = sock.read(&mut chunk).await.unwrap();
            if read == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..read]);
            let text = String::from_utf8_lossy(&buf).to_string();
            if let Some((head, body)) = text.split_once("\r\n\r\n") {
                let want = head
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if body.len() >= want {
                    break;
                }
            }
        }
        seen.push(String::from_utf8_lossy(&buf).to_string());
        let resp = format!("{status_line}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
        sock.write_all(resp.as_bytes()).await.unwrap();
        let _ = sock.shutdown().await;
    }
    seen
}

#[tokio::test]
async fn put_bucket_cors_is_signed_path_style_and_repeatable() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(fake_s3(listener, 2, "HTTP/1.1 200 OK"));
    let admin = S3Admin::new(
        &format!("http://127.0.0.1:{port}"),
        "sideroom",
        "garage",
        "GKexample",
        "secretexample",
    )
    .unwrap();
    let origins = vec!["https://chat.example.org/".to_owned()];
    admin.put_bucket_cors(&origins).await.unwrap();
    admin.put_bucket_cors(&origins).await.unwrap();
    let seen = server.await.unwrap();
    assert_eq!(seen.len(), 2);
    for req in &seen {
        let lower = req.to_ascii_lowercase();
        assert!(req.starts_with("PUT /sideroom?cors HTTP/1.1"), "{req}");
        assert!(lower.contains("authorization: aws4-hmac-sha256 credential=gkexample/"));
        assert!(lower.contains("/garage/s3/aws4_request"));
        assert!(lower.contains("x-amz-checksum-sha256:"));
        assert!(lower.contains(&format!("host: 127.0.0.1:{port}")));
        assert!(req.contains("<AllowedOrigin>https://chat.example.org</AllowedOrigin>"));
        assert!(!req.contains("secretexample"));
    }
    let body = |r: &str| r.split_once("\r\n\r\n").unwrap().1.to_owned();
    assert_eq!(body(&seen[0]), body(&seen[1]));
}

#[tokio::test]
async fn a_refusal_surfaces_as_an_error_without_the_secret() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(fake_s3(listener, 1, "HTTP/1.1 403 Forbidden"));
    let admin = S3Admin::new(
        &format!("http://127.0.0.1:{port}"),
        "b",
        "garage",
        "k",
        "topsecret",
    )
    .unwrap();
    let err = admin
        .put_bucket_cors(&["https://chat.example.org".to_owned()])
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("403"), "{err}");
    assert!(!err.contains("topsecret"));
    let _ = server.await;
}

#[tokio::test]
async fn a_bad_origin_is_refused_before_any_request() {
    let admin = S3Admin::new("http://127.0.0.1:9", "b", "garage", "k", "s").unwrap();
    assert!(admin.put_bucket_cors(&["*".to_owned()]).await.is_err());
    assert!(admin.put_bucket_cors(&[]).await.is_err());
}
