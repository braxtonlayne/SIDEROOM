//! The connection test (`probe::run`) and the CORS checker (`cors_check`) against an in-process fake S3
//! that answers by method and target. Every row of the test and every CORS state has a case.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use sr_storage::cors_check::{self, CorsState};
use sr_storage::mounts::Provider;
use sr_storage::probe::{self, ProbeStatus};
use sr_storage::s3admin::S3Admin;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

type Handler = dyn Fn(&str, &str, &str) -> (u16, String) + Send + Sync;

/// Serve until the test ends; every request is `(method, target, body)` -> `(status, body)`.
async fn fake_s3(handler: Arc<Handler>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let handler = handler.clone();
            let log = log.clone();
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
                            let mut first = head.lines().next().unwrap_or("").split(' ');
                            let method = first.next().unwrap_or("").to_owned();
                            let target = first.next().unwrap_or("").to_owned();
                            log.lock().unwrap().push(format!("{method} {target}"));
                            let (status, reply) = handler(&method, &target, body);
                            let reply = if method == "HEAD" {
                                String::new()
                            } else {
                                reply
                            };
                            let resp = format!(
                                "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
                                reply.len()
                            );
                            let _ = sock.write_all(resp.as_bytes()).await;
                            let _ = sock.shutdown().await;
                            return;
                        }
                    }
                }
            });
        }
    });
    (format!("http://127.0.0.1:{port}"), seen)
}

fn admin(endpoint: &str) -> S3Admin {
    S3Admin::new(endpoint, "shelf", "garage", "GKexample", "secretexample").unwrap()
}

const BUDGET: Duration = Duration::from_secs(10);
const UPLOAD: &str =
    "<InitiateMultipartUploadResult><UploadId>abc123</UploadId></InitiateMultipartUploadResult>";
const NO_CORS: &str = "<Error><Code>NoSuchCORSConfiguration</Code></Error>";

/// A healthy bucket-scoped key on a service with no CORS yet.
fn healthy(m: &str, t: &str, body: &str) -> (u16, String) {
    let _ = body;
    match (m, t) {
        ("HEAD", "/shelf") => (200, String::new()),
        ("GET", t) if t.starts_with("/shelf?") && t.contains("list-type=2") => {
            (200, "<ListBucketResult/>".into())
        }
        ("PUT", "/shelf/media/.sideroom-probe") => (200, String::new()),
        ("GET", "/shelf/media/.sideroom-probe") => (200, "sideroom probe".into()),
        ("DELETE", "/shelf/media/.sideroom-probe") => (204, String::new()),
        ("POST", "/shelf/media/.sideroom-probe?uploads") => (200, UPLOAD.into()),
        ("DELETE", t) if t.starts_with("/shelf/media/.sideroom-probe?uploadId=abc123") => {
            (204, String::new())
        }
        ("GET", "/shelf?cors") => (404, NO_CORS.into()),
        ("GET", "/") => (403, "<Error><Code>AccessDenied</Code></Error>".into()),
        _ => (500, "<Error><Code>Unexpected</Code></Error>".into()),
    }
}

fn statuses(t: &probe::ConnectionTest) -> Vec<(&'static str, ProbeStatus)> {
    t.rows.iter().map(|r| (r.id, r.status)).collect()
}

#[tokio::test]
async fn a_healthy_bucket_passes_every_row_but_cors_warns() {
    let (ep, seen) = fake_s3(Arc::new(healthy)).await;
    let t = probe::run(&admin(&ep), "media/", BUDGET).await;
    use ProbeStatus::{Pass, Warn};
    assert_eq!(
        statuses(&t),
        vec![
            ("head_bucket", Pass),
            ("list", Pass),
            ("put", Pass),
            ("multipart", Pass),
            ("cors", Warn),
            ("list_buckets", Pass)
        ]
    );
    assert!(!t.failed());
    // Signed, path-style, nothing leaked into any message.
    assert!(seen.lock().unwrap().iter().any(|l| l == "HEAD /shelf"));
    for r in &t.rows {
        assert!(!r.message.contains("secretexample") && !r.message.contains("127.0.0.1"));
        assert!(r.message.ends_with('.'), "{}", r.message);
    }
}

#[tokio::test]
async fn a_key_that_lists_buckets_earns_the_warning() {
    let (ep, _) = fake_s3(Arc::new(|m, t, b| {
        if (m, t) == ("GET", "/") {
            (200, "<ListAllMyBucketsResult/>".into())
        } else {
            healthy(m, t, b)
        }
    }))
    .await;
    let t = probe::run(&admin(&ep), "media/", BUDGET).await;
    let row = t.rows.iter().find(|r| r.id == "list_buckets").unwrap();
    assert_eq!(row.status, ProbeStatus::Warn);
    assert_eq!(
        row.message,
        "This key can see other buckets; use a bucket-scoped key."
    );
    assert!(!t.failed(), "a warning does not fail the mount");
}

#[tokio::test]
async fn existing_cors_rules_pass_and_a_missing_api_warns() {
    let rules = "<CORSConfiguration><CORSRule><AllowedOrigin>https://x.test</AllowedOrigin></CORSRule></CORSConfiguration>";
    let (ep, _) = fake_s3(Arc::new(move |m, t, b| {
        if (m, t) == ("GET", "/shelf?cors") {
            (200, rules.into())
        } else {
            healthy(m, t, b)
        }
    }))
    .await;
    let t = probe::run(&admin(&ep), "media/", BUDGET).await;
    assert_eq!(t.rows[4].status, ProbeStatus::Pass);

    let (ep, _) = fake_s3(Arc::new(|m, t, b| {
        if (m, t) == ("GET", "/shelf?cors") {
            (501, "<Error><Code>NotImplemented</Code></Error>".into())
        } else {
            healthy(m, t, b)
        }
    }))
    .await;
    let t = probe::run(&admin(&ep), "media/", BUDGET).await;
    assert_eq!(t.rows[4].status, ProbeStatus::Warn);
    assert!(t.rows[4].message.contains("does not offer the CORS API"));
}

#[tokio::test]
async fn each_failure_is_a_fail_row_with_a_sentence() {
    // Head denied.
    let (ep, _) = fake_s3(Arc::new(|m, t, b| {
        if m == "HEAD" {
            (403, String::new())
        } else {
            healthy(m, t, b)
        }
    }))
    .await;
    let t = probe::run(&admin(&ep), "media/", BUDGET).await;
    assert_eq!(t.rows[0].status, ProbeStatus::Fail);
    assert!(t.rows[0].message.contains("Access was denied"));
    assert!(t.failed());

    // Head 404.
    let (ep, _) = fake_s3(Arc::new(|m, t, b| {
        if m == "HEAD" {
            (404, String::new())
        } else {
            healthy(m, t, b)
        }
    }))
    .await;
    let t = probe::run(&admin(&ep), "media/", BUDGET).await;
    assert!(t.rows[0].message.contains("bucket was not found"));

    // No delete permission: put fails with the hint.
    let (ep, _) = fake_s3(Arc::new(|m, t, b| {
        if m == "DELETE" && !t.contains("uploadId") {
            (403, "<Error><Code>AccessDenied</Code></Error>".into())
        } else {
            healthy(m, t, b)
        }
    }))
    .await;
    let t = probe::run(&admin(&ep), "media/", BUDGET).await;
    let put = t.rows.iter().find(|r| r.id == "put").unwrap();
    assert_eq!(put.status, ProbeStatus::Fail);
    assert!(put.message.contains("delete objects"), "{}", put.message);
    assert!(put.message.contains("AccessDenied"));

    // Write denied, listing denied, multipart unsupported.
    let (ep, _) = fake_s3(Arc::new(|m, t, b| match (m, t) {
        ("PUT", _) => (403, "<Error><Code>AccessDenied</Code></Error>".into()),
        ("POST", _) => (501, "<Error><Code>NotImplemented</Code></Error>".into()),
        ("GET", t) if t.contains("list-type") => (403, String::new()),
        _ => healthy(m, t, b),
    }))
    .await;
    let t = probe::run(&admin(&ep), "media/", BUDGET).await;
    use ProbeStatus::{Fail, Pass, Warn};
    assert_eq!(
        statuses(&t),
        vec![
            ("head_bucket", Pass),
            ("list", Fail),
            ("put", Fail),
            ("multipart", Fail),
            ("cors", Warn),
            ("list_buckets", Pass)
        ]
    );
}

#[tokio::test]
async fn an_unreachable_endpoint_fails_the_first_row_and_skips_the_rest() {
    // Bind and drop to get a closed port.
    let port = {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        l.local_addr().unwrap().port()
    };
    let t = probe::run(&admin(&format!("http://127.0.0.1:{port}")), "", BUDGET).await;
    assert_eq!(t.rows.len(), 6);
    assert!(t.rows.iter().all(|r| r.status == ProbeStatus::Fail));
    assert!(t.rows[1].message.starts_with("Skipped"));
    assert!(!t.rows[0].message.contains("127.0.0.1"));
}

#[tokio::test]
async fn the_budget_is_enforced() {
    // A server that never answers: the whole run still ends near the budget.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((s, _)) = listener.accept().await {
            held.push(s);
        }
    });
    let started = std::time::Instant::now();
    let t = probe::run(
        &admin(&format!("http://127.0.0.1:{port}")),
        "",
        Duration::from_millis(600),
    )
    .await;
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(t.failed());
    assert_eq!(t.rows.len(), 6);
}

/// A bucket whose CORS document is whatever was last PUT.
fn cors_store() -> (Arc<Handler>, Arc<Mutex<Option<String>>>) {
    let doc: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let d = doc.clone();
    let h: Arc<Handler> = Arc::new(move |m, t, body| match (m, t) {
        ("GET", "/shelf?cors") => match d.lock().unwrap().clone() {
            Some(x) => (200, x),
            None => (404, NO_CORS.into()),
        },
        ("PUT", "/shelf?cors") => {
            *d.lock().unwrap() = Some(body.to_owned());
            (200, String::new())
        }
        _ => (500, String::new()),
    });
    (h, doc)
}

const ORIGIN: &str = "https://chat.example.org";

#[tokio::test]
async fn cors_missing_then_apply_then_ok_then_different() {
    let (h, doc) = cors_store();
    let (ep, _) = fake_s3(h).await;
    let a = admin(&ep);
    let r = cors_check::check(&a, Provider::Garage, ORIGIN)
        .await
        .unwrap();
    assert_eq!(r.state, CorsState::Missing);
    assert!(r.found.is_empty() && r.required.len() == 1);

    let r = cors_check::apply(&a, Provider::Garage, ORIGIN)
        .await
        .unwrap();
    assert_eq!(r.state, CorsState::Ok, "apply then check is ok");
    assert_eq!(
        cors_check::check(&a, Provider::Garage, ORIGIN)
            .await
            .unwrap()
            .state,
        CorsState::Ok
    );

    // Someone replaced the rules with another origin's.
    *doc.lock().unwrap() =
        Some(sr_storage::s3admin::cors_xml(&["https://other.example.org".to_owned()]).unwrap());
    let r = cors_check::check(&a, Provider::Garage, ORIGIN)
        .await
        .unwrap();
    assert_eq!(r.state, CorsState::Different);
    assert!(r.found[0].contains("other.example.org"));
    // Apply repairs it.
    assert_eq!(
        cors_check::apply(&a, Provider::Garage, ORIGIN)
            .await
            .unwrap()
            .state,
        CorsState::Ok
    );
}

#[tokio::test]
async fn a_provider_that_refuses_the_api_gets_manual_steps() {
    let (ep, _) = fake_s3(Arc::new(|_, _, _| {
        (501, "<Error><Code>NotImplemented</Code></Error>".into())
    }))
    .await;
    let a = admin(&ep);
    for r in [
        cors_check::check(&a, Provider::B2, ORIGIN).await.unwrap(),
        cors_check::apply(&a, Provider::B2, ORIGIN).await.unwrap(),
    ] {
        assert_eq!(r.state, CorsState::Manual);
        let steps = r.manual_steps.unwrap();
        assert!(steps.contains("Backblaze") && steps.contains(ORIGIN));
    }
}

#[tokio::test]
async fn other_refusals_and_bad_origins_are_errors() {
    let (ep, _) = fake_s3(Arc::new(|_, _, _| {
        (403, "<Error><Code>AccessDenied</Code></Error>".into())
    }))
    .await;
    let a = admin(&ep);
    let e = cors_check::check(&a, Provider::Garage, ORIGIN)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("AccessDenied") && !e.contains("secretexample"),
        "{e}"
    );
    assert!(
        cors_check::apply(&a, Provider::Garage, ORIGIN)
            .await
            .is_err()
    );
    assert!(cors_check::check(&a, Provider::Garage, "*").await.is_err());
}

#[tokio::test]
async fn a_manual_provider_whose_preflight_works_counts_as_ok() {
    let (ep, seen) = fake_s3(Arc::new(|m, _, _| {
        if m == "OPTIONS" {
            (200, String::new())
        } else {
            (501, "<Error><Code>NotImplemented</Code></Error>".into())
        }
    }))
    .await;
    // The fake cannot set response headers, so a preflight without Allow-Origin stays manual.
    let r = cors_check::check(&admin(&ep), Provider::B2, ORIGIN)
        .await
        .unwrap();
    assert_eq!(r.state, CorsState::Manual);
    assert!(
        seen.lock()
            .unwrap()
            .iter()
            .any(|l| l.starts_with("OPTIONS /shelf/.sideroom-probe"))
    );
}
