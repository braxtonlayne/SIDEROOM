use super::*;
use axum::{
    body::Body,
    http::{Request, Response, StatusCode},
};
use bytes::Bytes;
use http_body_util::BodyExt;
use sr_storage::ObjectStoreStorage;
use tower::ServiceExt;

const SECRET: &[u8] = b"0123456789abcdef0123456789abcdef";

struct Fx {
    app: Router,
    signer: CdnSigner,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

async fn fx(objects: &[(&str, Vec<u8>)]) -> Fx {
    let storage = Arc::new(ObjectStoreStorage::in_memory());
    for (k, v) in objects {
        storage.put(k, Bytes::from(v.clone())).await.unwrap();
    }
    let signer = CdnSigner::new(SECRET);
    let state = AppState::builder()
        .extension(Arc::new(CdnDeps::new(storage, signer.clone())))
        .build();
    Fx {
        app: layered(state),
        signer,
    }
}

impl Fx {
    fn url(&self, path: &str) -> String {
        self.signer.signed_url(path, now() + 3600)
    }
    async fn send(&self, method: &str, uri: &str, hdrs: &[(&str, &str)]) -> Response<Body> {
        let mut r = Request::builder().method(method).uri(uri);
        for (k, v) in hdrs {
            r = r.header(*k, *v);
        }
        self.app
            .clone()
            .oneshot(r.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }
    async fn get(&self, uri: &str) -> Response<Body> {
        self.send("GET", uri, &[]).await
    }
}

async fn bytes(r: Response<Body>) -> Bytes {
    r.into_body().collect().await.unwrap().to_bytes()
}

fn h<'a>(r: &'a Response<Body>, n: &str) -> &'a str {
    r.headers()
        .get(n)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

fn png() -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    v.extend((0..100u8).collect::<Vec<_>>());
    v
}

const P: &str = "/cdn/attachments/1/2/a.png";

#[tokio::test]
async fn valid_get_serves_with_hardened_headers() {
    let f = fx(&[("chat/1/2/a.png", png())]).await;
    let r = f.get(&f.url(P)).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(h(&r, "content-type"), "image/png");
    assert_eq!(h(&r, "content-length"), "108");
    assert_eq!(h(&r, "accept-ranges"), "bytes");
    assert_eq!(h(&r, "x-content-type-options"), "nosniff");
    assert_eq!(
        h(&r, "content-security-policy"),
        "default-src 'none'; sandbox"
    );
    assert!(h(&r, "content-disposition").starts_with("inline;"));
    assert!(h(&r, "etag").starts_with('"'));
    let cc = h(&r, "cache-control").to_owned();
    assert!(cc.starts_with("private, max-age="), "{cc}");
    let age: u64 = cc.rsplit('=').next().unwrap().parse().unwrap();
    assert!((3590..=3600).contains(&age));
    assert!(!h(&r, "x-request-id").is_empty());
    assert_eq!(bytes(r).await.as_ref(), png().as_slice());
}

#[tokio::test]
async fn cache_age_is_capped_at_one_day() {
    let f = fx(&[("chat/1/2/a.png", png())]).await;
    let url = f.signer.signed_url(P, now() + 30 * 86_400);
    let r = f.get(&url).await;
    assert_eq!(h(&r, "cache-control"), "private, max-age=86400");
}

#[tokio::test]
async fn head_has_headers_and_no_body() {
    let f = fx(&[("chat/1/2/a.png", png())]).await;
    let r = f.send("HEAD", &f.url(P), &[]).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(h(&r, "content-length"), "108");
    assert_eq!(h(&r, "content-type"), "image/png");
    assert!(bytes(r).await.is_empty());
}

#[tokio::test]
async fn ranges() {
    let data: Vec<u8> = (0..=255u8).collect();
    let f = fx(&[("chat/1/2/v.mp4", data.clone())]).await;
    let url = f.url("/cdn/attachments/1/2/v.mp4");
    let range = |v: &'static str| {
        let (f, url) = (&f, url.clone());
        async move { f.send("GET", &url, &[("range", v)]).await }
    };

    let r = range("bytes=10-19").await;
    assert_eq!(r.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(h(&r, "content-range"), "bytes 10-19/256");
    assert_eq!(h(&r, "content-length"), "10");
    assert_eq!(bytes(r).await.as_ref(), &data[10..20]);

    let r = range("bytes=250-").await;
    assert_eq!(r.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(h(&r, "content-range"), "bytes 250-255/256");
    assert_eq!(bytes(r).await.as_ref(), &data[250..]);

    let r = range("bytes=-4").await;
    assert_eq!(r.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(h(&r, "content-range"), "bytes 252-255/256");
    assert_eq!(bytes(r).await.as_ref(), &data[252..]);

    // End past the object is clamped.
    let r = range("bytes=200-9999").await;
    assert_eq!(h(&r, "content-range"), "bytes 200-255/256");

    let r = range("bytes=256-").await;
    assert_eq!(r.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(h(&r, "content-range"), "bytes */256");

    let r = range("bytes=-0").await;
    assert_eq!(r.status(), StatusCode::RANGE_NOT_SATISFIABLE);

    // Malformed and multi-range headers are ignored: full 200.
    for bad in [
        "bytes=abc",
        "items=0-1",
        "bytes=5-2",
        "bytes=0-1,4-5",
        "bytes=-",
    ] {
        let r = range(bad).await;
        assert_eq!(r.status(), StatusCode::OK, "{bad}");
        assert_eq!(bytes(r).await.len(), 256, "{bad}");
    }
}

#[tokio::test]
async fn if_none_match_gives_304() {
    let f = fx(&[("chat/1/2/a.png", png())]).await;
    let url = f.url(P);
    let tag = h(&f.get(&url).await, "etag").to_owned();
    for inm in [
        tag.clone(),
        format!("W/{tag}"),
        format!("\"x\", {tag}"),
        "*".into(),
    ] {
        let r = f.send("GET", &url, &[("if-none-match", &inm)]).await;
        assert_eq!(r.status(), StatusCode::NOT_MODIFIED, "{inm}");
        assert_eq!(h(&r, "etag"), tag);
        assert!(bytes(r).await.is_empty());
    }
    let r = f.send("GET", &url, &[("if-none-match", "\"other\"")]).await;
    assert_eq!(r.status(), StatusCode::OK);
    // A stale If-Range serves the whole object.
    let r = f
        .send(
            "GET",
            &url,
            &[("range", "bytes=0-3"), ("if-range", "\"stale\"")],
        )
        .await;
    assert_eq!(r.status(), StatusCode::OK);
    let r = f
        .send("GET", &url, &[("range", "bytes=0-3"), ("if-range", &tag)])
        .await;
    assert_eq!(r.status(), StatusCode::PARTIAL_CONTENT);
}

#[tokio::test]
async fn expired_is_410_but_only_with_a_valid_signature() {
    let f = fx(&[("chat/1/2/a.png", png())]).await;
    let r = f.get(&f.signer.signed_url(P, now() - 10)).await;
    assert_eq!(r.status(), StatusCode::GONE);
    // Forged expiry in the past is 403, not 410: nothing leaks without a signature.
    let r = f.get(&format!("{P}?ex={}&sig=AAAA", now() - 10)).await;
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn bad_signatures_are_403() {
    let f = fx(&[("chat/1/2/a.png", png())]).await;
    let url = f.url(P);
    let (_, q) = url.split_once('?').unwrap();
    // missing sig / ex / both
    assert_eq!(f.get(P).await.status(), StatusCode::FORBIDDEN);
    let ex = q.split('&').next().unwrap();
    assert_eq!(
        f.get(&format!("{P}?{ex}")).await.status(),
        StatusCode::FORBIDDEN
    );
    let sig = q.split('&').nth(1).unwrap();
    assert_eq!(
        f.get(&format!("{P}?{sig}")).await.status(),
        StatusCode::FORBIDDEN
    );
    // tampered path
    let other = url.replace("a.png", "b.png");
    assert_eq!(f.get(&other).await.status(), StatusCode::FORBIDDEN);
    let other = url.replace("/1/2/", "/1/3/");
    assert_eq!(f.get(&other).await.status(), StatusCode::FORBIDDEN);
    // tampered ex
    let n: u64 = ex.strip_prefix("ex=").unwrap().parse().unwrap();
    let tampered = format!("{P}?ex={}&{sig}", n + 1000);
    assert_eq!(f.get(&tampered).await.status(), StatusCode::FORBIDDEN);
    // truncated and garbage sig
    let short = &sig[..sig.len() - 3];
    assert_eq!(
        f.get(&format!("{P}?{ex}&{short}")).await.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        f.get(&format!("{P}?{ex}&sig=!!!")).await.status(),
        StatusCode::FORBIDDEN
    );
    // signed by a different key
    let other_key = CdnSigner::new(b"another-secret-another-secret-12");
    let r = f.get(&other_key.signed_url(P, now() + 60)).await;
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn traversal_and_odd_paths_are_refused() {
    let f = fx(&[
        ("chat/1/2/a.png", png()),
        ("secret/key.png", png()),
        ("other.png", png()),
    ])
    .await;
    // Even with a signature minted over the hostile path, the path is refused.
    for p in [
        "/cdn/attachments/../secret/key.png",
        "/cdn/attachments/1/../../secret/key.png",
        "/cdn/attachments/%2e%2e/secret/key.png",
        "/cdn/attachments/%2E%2E/secret/key.png",
        "/cdn/attachments/1/..%2f..%2fsecret/key.png",
        "/cdn/attachments/1/%2e%2e%2fsecret",
        "/cdn/attachments/1%5C..%5Cx/y.png",
        "/cdn/attachments/1\\2\\a.png",
        "/cdn/attachments//1/a.png",
        "/cdn/attachments/1/2/",
        "/cdn/attachments/1/./2/a.png",
        "/cdn/attachments/%00/a.png",
        "/cdn//attachments/1/2/a.png",
    ] {
        let signed = f.url(p);
        let r = f.get(&signed).await;
        assert!(
            matches!(r.status(), StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND),
            "{p} -> {}",
            r.status()
        );
        assert_ne!(r.status(), StatusCode::OK, "{p}");
    }
    // Absolute-ish and prefix-less paths never reach the handler or are 404.
    for p in ["/cdn/", "/cdn", "/cdn/attachments", "/cdn/attachments/"] {
        let r = f.get(&f.url(p)).await;
        assert!(r.status().is_client_error(), "{p}");
    }
}

#[tokio::test]
async fn wrong_prefix_is_404_even_with_a_valid_signature() {
    let f = fx(&[("secret/key.png", png()), ("chat/1/2/a.png", png())]).await;
    for p in [
        "/cdn/secret/key.png",
        "/cdn/chat/1/2/a.png",
        "/cdn/exports/1/a.png",
    ] {
        assert_eq!(
            f.get(&f.url(p)).await.status(),
            StatusCode::NOT_FOUND,
            "{p}"
        );
    }
}

#[tokio::test]
async fn unknown_object_is_404_only_with_a_valid_signature() {
    let f = fx(&[]).await;
    let p = "/cdn/emoji/9/1.png";
    assert_eq!(f.get(&f.url(p)).await.status(), StatusCode::NOT_FOUND);
    assert_eq!(f.get(p).await.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        f.get(&format!("{p}?ex=1&sig=x")).await.status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn encoded_filenames_verify_against_the_canonical_path() {
    let f = fx(&[("chat/1/2/my fé #1.png", png())]).await;
    let canonical = cdn_path("attachments", &["1", "2", "my fé #1.png"]);
    assert_eq!(canonical, "/cdn/attachments/1/2/my%20f%C3%A9%20%231.png");
    let url = f.url(&canonical);
    assert_eq!(f.get(&url).await.status(), StatusCode::OK);
    // Lower-case hex escapes are the same resource.
    let lower = url.replace("%C3%A9", "%c3%a9");
    assert_eq!(f.get(&lower).await.status(), StatusCode::OK);
}

#[tokio::test]
async fn html_upload_is_never_served_as_html() {
    let html = b"<html><script>alert(1)</script></html>".to_vec();
    let f = fx(&[
        ("chat/1/2/evil.html", html.clone()),
        ("chat/1/2/evil.svg", b"<svg onload=alert(1)>".to_vec()),
        ("chat/1/2/noext", html.clone()),
        ("chat/1/2/doc.pdf", b"%PDF-1.7".to_vec()),
    ])
    .await;
    for name in ["evil.html", "evil.svg", "noext", "doc.pdf"] {
        let r = f.get(&f.url(&format!("/cdn/attachments/1/2/{name}"))).await;
        assert_eq!(r.status(), StatusCode::OK, "{name}");
        assert_eq!(h(&r, "content-type"), "application/octet-stream", "{name}");
        assert!(
            h(&r, "content-disposition").starts_with("attachment;"),
            "{name}"
        );
        assert_eq!(h(&r, "x-content-type-options"), "nosniff");
        assert!(h(&r, "content-security-policy").contains("sandbox"));
    }
}

#[tokio::test]
async fn unknown_extension_is_sniffed_but_html_stays_a_download() {
    let f = fx(&[("chat/1/2/blob", png())]).await;
    let r = f.get(&f.url("/cdn/attachments/1/2/blob")).await;
    assert_eq!(h(&r, "content-type"), "image/png");
    assert!(h(&r, "content-disposition").starts_with("inline;"));
}

#[tokio::test]
async fn big_object_streams_in_chunks() {
    use http_body_util::BodyExt as _;
    const LEN: usize = 20 * 1024 * 1024;
    let data: Vec<u8> = (0..LEN).map(|i| (i % 251) as u8).collect();
    let f = fx(&[("chat/1/2/big.mp4", data.clone())]).await;
    let url = f.url("/cdn/attachments/1/2/big.mp4");

    let r = f.get(&url).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(h(&r, "content-length"), LEN.to_string());
    // Consume frame by frame, never collecting the whole body.
    let mut body = r.into_body();
    let (mut total, mut frames) = (0usize, 0usize);
    while let Some(frame) = body.frame().await {
        let frame = frame.unwrap();
        if let Some(d) = frame.data_ref() {
            assert_eq!(d.as_ref(), &data[total..total + d.len()]);
            total += d.len();
            frames += 1;
        }
    }
    assert_eq!(total, LEN);
    assert!(frames >= 1);

    // A ranged read in the middle of the big object.
    let r = f
        .send("GET", &url, &[("range", "bytes=10485760-10485779")])
        .await;
    assert_eq!(r.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(bytes(r).await.as_ref(), &data[10_485_760..10_485_780]);
}

#[tokio::test]
async fn empty_object_serves_zero_bytes() {
    let f = fx(&[("chat/1/2/e.png", vec![])]).await;
    let r = f.get(&f.url("/cdn/attachments/1/2/e.png")).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(h(&r, "content-length"), "0");
    assert!(bytes(r).await.is_empty());
}

#[tokio::test]
async fn cors_allows_only_the_public_origin() {
    let f = fx(&[("chat/1/2/a.png", png())]).await;
    let url = f.url(P);
    let bad = f
        .send("GET", &url, &[("origin", "https://evil.example")])
        .await;
    let allowed = AppState::builder()
        .build()
        .settings()
        .allowed_origins
        .clone();
    if let Some(o) = allowed.first() {
        let r = f.send("GET", &url, &[("origin", o)]).await;
        assert_eq!(h(&r, "access-control-allow-origin"), o);
        assert_eq!(h(&r, "access-control-allow-credentials"), "");
    }
    assert_eq!(h(&bad, "access-control-allow-origin"), "");
}

#[tokio::test]
async fn non_get_methods_are_rejected() {
    let f = fx(&[("chat/1/2/a.png", png())]).await;
    let r = f.send("POST", &f.url(P), &[]).await;
    assert_eq!(r.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn missing_deps_is_500_not_a_panic() {
    let state = AppState::builder().build();
    let app = layered(state);
    let r = app
        .oneshot(Request::get(P).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn the_cdn_router_does_not_overlap_the_api_router() {
    let state = AppState::builder().build();
    let _ = crate::router::build_router(state.clone(), crate::routes::all()).merge(layered(state));
}

/// Asset 9 keeps its files where the worker wrote them (`transcode/9/1`); nothing is rendered here.
struct Renderer;

#[async_trait::async_trait]
impl PlaylistRenderer for Renderer {
    async fn render(&self, _asset_id: i64, _rel: &str, _now: u64) -> Option<Rendered> {
        None
    }

    async fn storage_key(&self, asset_id: i64, rel: &str) -> Option<String> {
        (asset_id == 9).then(|| format!("transcode/9/1/{rel}"))
    }
}

#[tokio::test]
async fn originals_and_transcode_outputs_are_signed_only_and_hls_assets_map_to_their_prefix() {
    let storage = Arc::new(ObjectStoreStorage::in_memory());
    storage
        .put("originals/12/source.mkv", Bytes::from_static(b"mkvdata"))
        .await
        .unwrap();
    storage
        .put(
            "transcode/9/1/remux/video.mp4",
            Bytes::from_static(b"mp4data"),
        )
        .await
        .unwrap();
    storage
        .put(
            "transcode/9/1/v/480p/seg_00000.m4s",
            Bytes::from_static(b"seg"),
        )
        .await
        .unwrap();
    storage
        .put("transcode/9/1/master.m3u8", Bytes::from_static(b"stored"))
        .await
        .unwrap();
    let signer = CdnSigner::new(SECRET);
    let state = AppState::builder()
        .extension(Arc::new(
            CdnDeps::new(storage, signer.clone()).with_playlists(Arc::new(Renderer)),
        ))
        .build();
    let f = Fx {
        app: layered(state),
        signer,
    };
    // No signature, no object: originals are never public.
    let r = f.get("/cdn/originals/12/source.mkv").await;
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
    let r = f.get(&f.url("/cdn/originals/12/source.mkv")).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert!(h(&r, "content-disposition").starts_with("attachment;"));
    assert_eq!(bytes(r).await.as_ref(), b"mkvdata");
    // A signature for another path does not carry over.
    let other = f
        .url("/cdn/originals/13/source.mkv")
        .replace("/13/", "/12/");
    assert_eq!(f.get(&other).await.status(), StatusCode::FORBIDDEN);
    // Segments are stored objects served as media; `dl=1` turns a video into a download.
    let r = f
        .get(&f.url("/cdn/transcode/9/1/v/480p/seg_00000.m4s"))
        .await;
    assert_eq!(h(&r, "content-type"), "video/iso.segment");
    let r = f.get(&f.url("/cdn/transcode/9/1/remux/video.mp4")).await;
    assert!(h(&r, "content-disposition").starts_with("inline;"));
    let url = format!("{}&dl=1", f.url("/cdn/transcode/9/1/remux/video.mp4"));
    let r = f.get(&url).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert!(h(&r, "content-disposition").starts_with("attachment;"));
    assert_eq!(h(&r, "content-type"), "video/mp4");
    // By asset id, the stored files are found under the asset's own storage prefix.
    let r = f.get(&f.url("/cdn/hls/9/v/480p/seg_00000.m4s")).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(bytes(r).await.as_ref(), b"seg");
    let r = f.get(&f.url("/cdn/hls/9/master.m3u8")).await;
    assert_eq!(h(&r, "content-type"), "application/vnd.apple.mpegurl");
    assert_eq!(bytes(r).await.as_ref(), b"stored");
    // Another asset has no mapping: its default prefix holds nothing.
    assert_eq!(
        f.get(&f.url("/cdn/hls/10/master.m3u8")).await.status(),
        StatusCode::NOT_FOUND
    );
}

/// A takedown blocks the stored object on the media path (MOD-TKD-004): a valid link answers 451, a bad
/// signature still 403 (nothing is revealed to a stranger), and other objects are untouched.
#[tokio::test]
async fn blocked_objects_answer_451_after_the_signature_check() {
    use sr_moderation::blocklist::{BlockSet, MediaBlocklist};
    let storage = Arc::new(ObjectStoreStorage::in_memory());
    storage
        .put("chat/1/2/a.png", Bytes::from(png()))
        .await
        .unwrap();
    storage
        .put("chat/1/2/b.png", Bytes::from(png()))
        .await
        .unwrap();
    storage
        .put("library/7/9/001.webp", Bytes::from(png()))
        .await
        .unwrap();
    let signer = CdnSigner::new(SECRET);
    let list = BlockSet::from_rows([
        ("key".to_owned(), "chat/1/2/a.png".to_owned()),
        ("prefix".to_owned(), "library/7/".to_owned()),
    ]);
    let state = AppState::builder()
        .extension(Arc::new(
            CdnDeps::new(storage, signer.clone())
                .with_blocklist(Arc::new(MediaBlocklist::fixed(list))),
        ))
        .build();
    let f = Fx {
        app: layered(state),
        signer,
    };
    let blocked = f.get(&f.url("/cdn/attachments/1/2/a.png")).await;
    assert_eq!(blocked.status(), StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS);
    let by_prefix = f.get(&f.url("/cdn/library/7/9/001.webp")).await;
    assert_eq!(
        by_prefix.status(),
        StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS
    );
    let other = f.get(&f.url("/cdn/attachments/1/2/b.png")).await;
    assert_eq!(other.status(), StatusCode::OK);
    let unsigned = f.get("/cdn/attachments/1/2/a.png?ex=1&sig=bad").await;
    assert_eq!(unsigned.status(), StatusCode::FORBIDDEN);
}
