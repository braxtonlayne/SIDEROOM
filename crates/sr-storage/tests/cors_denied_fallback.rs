//! D-1032: when the key may not read the bucket's CORS configuration (403 / `AccessDenied`), the checker falls back to
//! a live preflight. Only a preflight that answers success with the allow-origin header counts. All values are made up.

use sr_storage::cors_check::{self, CorsState};
use sr_storage::mounts::Provider;
use sr_storage::s3admin::S3Admin;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const ORIGIN: &str = "https://chat.example.org";

/// A fake bucket: `config` answers `GET /shelf?cors`; `preflight` is the status of an `OPTIONS` and whether it carries
/// `access-control-allow-origin`.
async fn fake(config: (u16, &'static str), preflight: (u16, bool)) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let n = sock.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    let Some((head, _)) = text.split_once("\r\n\r\n") else {
                        continue;
                    };
                    let mut first = head.lines().next().unwrap_or("").split(' ');
                    let (m, t) = (first.next().unwrap_or(""), first.next().unwrap_or(""));
                    let (status, extra, body) = match (m, t) {
                        ("GET", "/shelf?cors") => (config.0, String::new(), config.1),
                        ("OPTIONS", _) => (
                            preflight.0,
                            if preflight.1 {
                                format!("access-control-allow-origin: {ORIGIN}\r\n")
                            } else {
                                String::new()
                            },
                            "",
                        ),
                        _ => (500, String::new(), ""),
                    };
                    let resp = format!(
                        "HTTP/1.1 {status} X\r\n{extra}content-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                    let _ = sock.shutdown().await;
                    return;
                }
            });
        }
    });
    format!("http://127.0.0.1:{port}")
}

fn admin(ep: &str) -> S3Admin {
    S3Admin::new(
        ep,
        "shelf",
        "auto",
        "TESTKEYIDFORMOUNT0001",
        "test-secret-not-real",
    )
    .unwrap()
}

const DENIED: (u16, &str) = (403, "<Error><Code>AccessDenied</Code></Error>");

#[tokio::test]
async fn config_read_403_and_a_passing_preflight_is_ok() {
    let ep = fake(DENIED, (200, true)).await;
    let r = cors_check::check(&admin(&ep), Provider::R2, ORIGIN)
        .await
        .unwrap();
    assert_eq!(r.state, CorsState::Ok);
    assert!(r.found[0].contains("live preflight"));
}

#[tokio::test]
async fn access_denied_code_with_another_status_also_falls_back() {
    let ep = fake(
        (400, "<Error><Code>AccessDeniedException</Code></Error>"),
        (200, true),
    )
    .await;
    let r = cors_check::check(&admin(&ep), Provider::R2, ORIGIN)
        .await
        .unwrap();
    assert_eq!(r.state, CorsState::Ok);
}

#[tokio::test]
async fn config_read_403_and_a_preflight_without_the_allow_header_is_not_ready() {
    let ep = fake(DENIED, (200, false)).await;
    let e = cors_check::check(&admin(&ep), Provider::R2, ORIGIN)
        .await
        .unwrap_err()
        .to_string();
    assert!(e.contains("reading the bucket's CORS rules failed"), "{e}");
}

#[tokio::test]
async fn a_403_on_the_preflight_itself_is_never_ok() {
    // Wrong key: both the config read and the preflight are refused.
    let ep = fake(DENIED, (403, true)).await;
    assert!(
        cors_check::check(&admin(&ep), Provider::R2, ORIGIN)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn config_read_501_is_still_manual_and_a_good_config_needs_no_preflight() {
    let ep = fake(
        (501, "<Error><Code>NotImplemented</Code></Error>"),
        (200, false),
    )
    .await;
    let r = cors_check::check(&admin(&ep), Provider::B2, ORIGIN)
        .await
        .unwrap();
    assert_eq!(r.state, CorsState::Manual);

    // A readable, correct config passes although the preflight would fail: no preflight is made.
    let xml = sr_storage::s3admin::cors_xml(&[ORIGIN.to_owned()]).unwrap();
    let xml: &'static str = Box::leak(xml.into_boxed_str());
    let ep = fake((200, xml), (403, false)).await;
    let r = cors_check::check(&admin(&ep), Provider::R2, ORIGIN)
        .await
        .unwrap();
    assert_eq!(r.state, CorsState::Ok);
    assert!(r.found[0].starts_with("origin "), "{:?}", r.found);
}
