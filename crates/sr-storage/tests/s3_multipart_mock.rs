//! `S3Multipart` against an in-process fake S3 endpoint: `CreateMultipartUpload` and
//! `CompleteMultipartUpload` are signed, path-style and carry the right bodies; the presigned part
//! URL is accepted by a fake that checks its query like a real service would (it only checks the
//! shape: verifying the signature is covered by the AWS documentation vectors in the unit tests).

use sr_storage::s3multipart::S3Multipart;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Accept one connection per entry of `replies`, answer with `(status line, body)`, return the raw
/// requests.
async fn fake_s3(listener: TcpListener, replies: Vec<(&'static str, String)>) -> Vec<String> {
    let mut seen = Vec::new();
    for (status_line, body) in replies {
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
            if let Some((head, rest)) = text.split_once("\r\n\r\n") {
                let want = head
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if rest.len() >= want {
                    break;
                }
            }
        }
        seen.push(String::from_utf8_lossy(&buf).to_string());
        let resp = format!(
            "{status_line}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        sock.write_all(resp.as_bytes()).await.unwrap();
        let _ = sock.shutdown().await;
    }
    seen
}

async fn setup(
    replies: Vec<(&'static str, String)>,
) -> (S3Multipart, tokio::task::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(fake_s3(listener, replies));
    let m = S3Multipart::new(
        &format!("http://127.0.0.1:{port}"),
        "sideroom",
        "garage",
        "GKexample",
        "topsecretvalue",
    )
    .unwrap();
    (m, server)
}

#[tokio::test]
async fn create_part_complete_round_trip() {
    let created = "<?xml version=\"1.0\"?><InitiateMultipartUploadResult><Bucket>sideroom</Bucket>\
                   <Key>transcode/9/1/v/480p/seg_00001.m4s</Key><UploadId>abc123XYZ</UploadId>\
                   </InitiateMultipartUploadResult>"
        .to_owned();
    let (m, server) = setup(vec![
        ("HTTP/1.1 200 OK", created),
        ("HTTP/1.1 200 OK", "<CompleteMultipartUploadResult/>".into()),
    ])
    .await;
    let key = "transcode/9/1/v/480p/seg_00001.m4s";
    let id = m.create(key, "video/iso.segment").await.unwrap();
    assert_eq!(id, "abc123XYZ");
    let url = m
        .presign_part(key, &id, 1, Duration::from_secs(600))
        .unwrap();
    assert!(url.contains("/sideroom/transcode/9/1/v/480p/seg_00001.m4s?"));
    assert!(url.contains("partNumber=1&") && url.contains("uploadId=abc123XYZ"));
    m.complete(
        key,
        &id,
        &[(2, "\"etag-b\"".into()), (1, "\"etag-a\"".into())],
    )
    .await
    .unwrap();
    let seen = server.await.unwrap();
    let create = seen[0].to_ascii_lowercase();
    assert!(
        seen[0].starts_with("POST /sideroom/transcode/9/1/v/480p/seg_00001.m4s?uploads HTTP/1.1"),
        "{}",
        seen[0]
    );
    assert!(create.contains("authorization: aws4-hmac-sha256 credential=gkexample/"));
    assert!(create.contains("content-type: video/iso.segment"));
    assert!(create.contains("x-amz-content-sha256:"));
    let done = &seen[1];
    assert!(
        done.starts_with(
            "POST /sideroom/transcode/9/1/v/480p/seg_00001.m4s?uploadId=abc123XYZ HTTP/1.1"
        ),
        "{done}"
    );
    // Parts are sent in order with the ETags escaped as XML.
    let body = done.split_once("\r\n\r\n").unwrap().1;
    assert!(body.starts_with("<CompleteMultipartUpload><Part><PartNumber>1</PartNumber><ETag>&quot;etag-a&quot;</ETag></Part><Part><PartNumber>2</PartNumber>"));
    assert!(!done.contains("topsecretvalue"));
}

#[tokio::test]
async fn a_200_with_an_error_document_is_a_failure() {
    let (m, server) = setup(vec![(
        "HTTP/1.1 200 OK",
        "<Error><Code>InvalidPart</Code></Error>".into(),
    )])
    .await;
    let err = m
        .complete("k/o.bin", "up1", &[(1, "e".into())])
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("InvalidPart"), "{err}");
    let _ = server.await;
}

#[tokio::test]
async fn a_refusal_surfaces_without_the_secret() {
    let (m, server) = setup(vec![(
        "HTTP/1.1 403 Forbidden",
        "<Error>denied</Error>".into(),
    )])
    .await;
    let err = m
        .create("k/o.bin", "video/mp4")
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("403"), "{err}");
    assert!(!err.contains("topsecretvalue"));
    let _ = server.await;
}

#[tokio::test]
async fn abort_sends_a_signed_delete() {
    let (m, server) = setup(vec![("HTTP/1.1 204 No Content", String::new())]).await;
    m.abort("k/o.bin", "up1").await.unwrap();
    let seen = server.await.unwrap();
    assert!(
        seen[0].starts_with("DELETE /sideroom/k/o.bin?uploadId=up1 HTTP/1.1"),
        "{}",
        seen[0]
    );
    assert!(
        seen[0]
            .to_ascii_lowercase()
            .contains("authorization: aws4-hmac-sha256")
    );
}

#[tokio::test]
async fn list_parts_follows_the_marker_and_sorts() {
    let page1 = "<ListPartsResult><IsTruncated>true</IsTruncated><NextPartNumberMarker>2</NextPartNumberMarker>\
        <Part><PartNumber>2</PartNumber><ETag>&quot;b&quot;</ETag><Size>67108864</Size></Part>\
        <Part><PartNumber>1</PartNumber><ETag>&quot;a&quot;</ETag><Size>67108864</Size></Part></ListPartsResult>";
    let page2 = "<ListPartsResult><IsTruncated>false</IsTruncated>\
        <Part><PartNumber>3</PartNumber><ETag>&quot;c&quot;</ETag><Size>10</Size></Part></ListPartsResult>";
    let (m, server) = setup(vec![
        ("HTTP/1.1 200 OK", page1.into()),
        ("HTTP/1.1 200 OK", page2.into()),
    ])
    .await;
    let parts = m.list_parts("k/o.bin", "up1").await.unwrap();
    let seen = server.await.unwrap();
    assert_eq!(
        parts.iter().map(|p| p.number).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(parts[0].etag, "\"a\"");
    assert_eq!(parts[2].size, 10);
    assert!(
        seen[0].starts_with("GET /sideroom/k/o.bin?uploadId=up1 HTTP/1.1"),
        "{}",
        seen[0]
    );
    assert!(
        seen[1].starts_with("GET /sideroom/k/o.bin?part-number-marker=2&uploadId=up1 HTTP/1.1"),
        "{}",
        seen[1]
    );
}

#[tokio::test]
async fn list_parts_rejects_a_garbled_answer() {
    let (m, _server) = setup(vec![(
        "HTTP/1.1 200 OK",
        "<ListPartsResult><Part><PartNumber>x</PartNumber></Part></ListPartsResult>".into(),
    )])
    .await;
    assert!(m.list_parts("k/o.bin", "up1").await.is_err());
}
