use super::external::{self, ExternalImageDeps};
use super::*;
use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, Response, StatusCode},
};
use bytes::Bytes;
use http_body_util::BodyExt;
use serde_json::json;
use sr_storage::ObjectStoreStorage;
use sr_unfurl::fetch::{FetchError, HopRequest, HopResponse, HttpTransport, Resolver};
use sr_unfurl::{Policy, SafeFetcher};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use tower::ServiceExt;

const SECRET: &[u8] = b"0123456789abcdef0123456789abcdef";

#[derive(Default)]
struct Dns(HashMap<String, Vec<IpAddr>>);

#[async_trait]
impl Resolver for Dns {
    async fn resolve(&self, host: &str, _port: u16) -> Result<Vec<IpAddr>, String> {
        self.0
            .get(host)
            .cloned()
            .ok_or_else(|| format!("no such host {host}"))
    }
}

#[derive(Default)]
struct Origin {
    routes: Mutex<HashMap<String, HopResponse>>,
    seen: Mutex<Vec<HopRequest>>,
}

#[async_trait]
impl HttpTransport for Origin {
    async fn get(&self, req: &HopRequest) -> Result<HopResponse, FetchError> {
        self.seen.lock().unwrap().push(req.clone());
        self.routes
            .lock()
            .unwrap()
            .get(req.url.as_str())
            .cloned()
            .ok_or_else(|| FetchError::Http("no route".into()))
    }
}

struct Fx {
    app: Router,
    signer: CdnSigner,
    origin: Arc<Origin>,
    storage: Arc<ObjectStoreStorage>,
    clock: Arc<std::sync::atomic::AtomicI64>,
}

fn dns() -> Dns {
    let mut d = Dns::default();
    for (h, a) in [
        ("img.example", "93.184.216.34"),
        ("inner.example", "10.1.2.3"),
        ("meta.example", "169.254.169.254"),
        ("mapped.example", "::ffff:127.0.0.1"),
    ] {
        d.0.insert(h.to_owned(), vec![a.parse().unwrap()]);
    }
    d
}

fn pic(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_fn(w, h, |x, y| {
        image::Rgba([(x * 7 % 256) as u8, (y * 5 % 256) as u8, 90, 255])
    });
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// `png` with a `tEXt` chunk (metadata) inserted before `IDAT`.
fn with_text_chunk(png: &[u8], text: &str) -> Vec<u8> {
    let mut data = b"Comment\0".to_vec();
    data.extend_from_slice(text.as_bytes());
    let mut chunk = Vec::new();
    chunk.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = b"tEXt".to_vec();
    body.extend_from_slice(&data);
    chunk.extend_from_slice(&body);
    chunk.extend_from_slice(&crc32(&body).to_be_bytes());
    let at = png.windows(4).position(|w| w == b"IDAT").unwrap() - 4;
    let mut out = png[..at].to_vec();
    out.extend_from_slice(&chunk);
    out.extend_from_slice(&png[at..]);
    out
}

fn ok(content_type: &str, body: Vec<u8>) -> HopResponse {
    HopResponse {
        status: 200,
        content_type: Some(content_type.into()),
        body,
        ..HopResponse::default()
    }
}

fn redirect(to: &str) -> HopResponse {
    HopResponse {
        status: 302,
        location: Some(to.into()),
        ..HopResponse::default()
    }
}

async fn fx_with(settings: crate::ApiSettings) -> Fx {
    let storage = Arc::new(ObjectStoreStorage::in_memory());
    let origin = Arc::new(Origin::default());
    let signer = CdnSigner::new(SECRET);
    let fetcher = SafeFetcher::new(Policy::default(), Arc::new(dns()), origin.clone());
    let clock = Arc::new(std::sync::atomic::AtomicI64::new(1_000_000));
    let c = clock.clone();
    let deps = ExternalImageDeps::new(fetcher, storage.clone(), signer.clone()).with_clock(
        Arc::new(move || c.load(std::sync::atomic::Ordering::SeqCst)),
    );
    let state = AppState::builder()
        .settings(settings)
        .extension(Arc::new(CdnDeps::new(storage.clone(), signer.clone())))
        .extension(Arc::new(deps))
        .build();
    Fx {
        app: layered(state),
        signer,
        origin,
        storage,
        clock,
    }
}

async fn fx() -> Fx {
    fx_with(crate::ApiSettings::default()).await
}

impl Fx {
    fn route(&self, url: &str, resp: HopResponse) {
        self.origin
            .routes
            .lock()
            .unwrap()
            .insert(url.to_owned(), resp);
    }
    fn proxied(&self, url: &str) -> String {
        self.signer.external_path(url).unwrap()
    }
    fn fetches(&self) -> usize {
        self.origin.seen.lock().unwrap().len()
    }
    async fn get(&self, uri: &str) -> Response<Body> {
        self.send(uri, &[]).await
    }
    async fn send(&self, uri: &str, hdrs: &[(&str, &str)]) -> Response<Body> {
        let mut r = Request::builder().method("GET").uri(uri);
        for (k, v) in hdrs {
            r = r.header(*k, *v);
        }
        self.app
            .clone()
            .oneshot(r.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }
}

async fn body(r: Response<Body>) -> Bytes {
    r.into_body().collect().await.unwrap().to_bytes()
}

fn h<'a>(r: &'a Response<Body>, n: &str) -> &'a str {
    r.headers()
        .get(n)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

#[tokio::test]
async fn serves_a_cleaned_image_with_strict_headers_and_caches_it() {
    let f = fx().await;
    let secret = "GPS-SECRET-LOCATION";
    f.route(
        "https://img.example/cat.png",
        ok("image/png", with_text_chunk(&pic(40, 30), secret)),
    );
    let uri = f.proxied("https://img.example/cat.png");
    let r = f.get(&uri).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(h(&r, "content-type"), "image/png");
    assert_eq!(h(&r, "x-content-type-options"), "nosniff");
    assert_eq!(
        h(&r, "content-security-policy"),
        "default-src 'none'; sandbox"
    );
    assert_eq!(h(&r, "cache-control"), "public, max-age=86400");
    assert_eq!(h(&r, "referrer-policy"), "no-referrer");
    assert!(h(&r, "content-disposition").starts_with("inline;"));
    assert!(h(&r, "etag").starts_with('"'));
    let etag = h(&r, "etag").to_owned();
    let served = body(r).await;
    assert!(
        !served.windows(secret.len()).any(|w| w == secret.as_bytes()),
        "metadata must not survive re-encoding"
    );
    let decoded = image::load_from_memory(&served).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (40, 30));

    // Served again from the cache: no second fetch, same bytes, 304 on revalidation.
    let again = f.get(&uri).await;
    assert_eq!(again.status(), StatusCode::OK);
    assert_eq!(body(again).await, served);
    assert_eq!(f.fetches(), 1);
    let nm = f.send(&uri, &[("if-none-match", &etag)]).await;
    assert_eq!(nm.status(), StatusCode::NOT_MODIFIED);
    // The cache object lives under external/ and starts with the expiry header.
    let listed = f.storage.list("external/").await.unwrap();
    assert_eq!(listed.len(), 1);
}

#[tokio::test]
async fn large_images_are_bounded_to_2048_pixels() {
    let f = fx().await;
    f.route(
        "https://img.example/big.png",
        ok("image/png", pic(2600, 40)),
    );
    let r = f.get(&f.proxied("https://img.example/big.png")).await;
    assert_eq!(r.status(), StatusCode::OK);
    let img = image::load_from_memory(&body(r).await).unwrap();
    assert_eq!(img.width(), 2048);
}

#[tokio::test]
async fn an_invalid_signature_fetches_nothing() {
    let f = fx().await;
    f.route("https://img.example/a.png", ok("image/png", pic(8, 8)));
    let good = f.proxied("https://img.example/a.png");
    let (sig, b64) = good
        .strip_prefix("/cdn/external/")
        .unwrap()
        .split_once('/')
        .unwrap();
    // Another URL under the signature of the first.
    let other = f
        .proxied("https://img.example/b.png")
        .rsplit('/')
        .next()
        .unwrap()
        .to_owned();
    for uri in [
        format!("/cdn/external/{sig}/{other}"),
        format!("/cdn/external/AAAA{}/{b64}", &sig[4..]),
        format!("/cdn/external/{b64}/{sig}"),
    ] {
        let r = f.get(&uri).await;
        assert!(
            matches!(r.status(), StatusCode::FORBIDDEN | StatusCode::BAD_REQUEST),
            "{uri}: {}",
            r.status()
        );
    }
    let other_key = CdnSigner::new(b"another-secret-another-secret-12");
    let forged = other_key
        .external_path("https://img.example/a.png")
        .unwrap();
    assert_eq!(f.get(&forged).await.status(), StatusCode::FORBIDDEN);
    assert_eq!(f.fetches(), 0);
    // And the genuine link still works.
    assert_eq!(f.get(&good).await.status(), StatusCode::OK);
}

#[tokio::test]
async fn ssrf_targets_are_refused_after_dns_and_on_redirects() {
    let f = fx().await;
    f.route(
        "https://img.example/to-private",
        redirect("http://127.0.0.1:9000/secret.png"),
    );
    f.route(
        "https://img.example/to-meta",
        redirect("http://meta.example/latest.png"),
    );
    f.route(
        "https://img.example/to-dec",
        redirect("http://2130706433/secret.png"),
    );
    f.route(
        "https://img.example/to-mapped",
        redirect("http://[::ffff:7f00:1]/secret.png"),
    );
    for url in [
        "http://127.0.0.1/a.png",
        "http://[::1]/a.png",
        "http://2130706433/a.png",
        "http://0x7f.0.0.1/a.png",
        "http://[::ffff:10.0.0.1]/a.png",
        "http://169.254.169.254/latest/meta-data/",
        "http://inner.example/a.png",
        "http://meta.example/a.png",
        "http://mapped.example/a.png",
        "https://img.example/to-private",
        "https://img.example/to-meta",
        "https://img.example/to-dec",
        "https://img.example/to-mapped",
    ] {
        let r = f.get(&f.proxied(url)).await;
        assert_eq!(r.status(), StatusCode::FORBIDDEN, "{url}");
        let text = body(r).await;
        // The answer never says what the name resolved to.
        let text = String::from_utf8_lossy(&text).to_string();
        assert!(
            !text.contains("10.1.2.3") && !text.contains("127.0.0.1"),
            "{text}"
        );
    }
    for req in f.origin.seen.lock().unwrap().iter() {
        for a in &req.addrs {
            assert!(!sr_unfurl::is_forbidden_ip(a.ip()), "connected to {a}");
        }
    }
}

#[tokio::test]
async fn only_real_images_are_served() {
    let f = fx().await;
    f.route(
        "https://img.example/page",
        ok("text/html", b"<html>".to_vec()),
    );
    f.route(
        "https://img.example/lies.png",
        ok(
            "image/png",
            b"<html><script>alert(1)</script></html>".to_vec(),
        ),
    );
    f.route(
        "https://img.example/x.svg",
        ok(
            "image/svg+xml",
            b"<svg xmlns='http://www.w3.org/2000/svg'><script>alert(1)</script></svg>".to_vec(),
        ),
    );
    let mut truncated = pic(64, 64);
    truncated.truncate(80);
    f.route("https://img.example/cut.png", ok("image/png", truncated));
    f.route(
        "https://img.example/avif",
        ok("image/avif", b"\0\0\0\x1cftypavif\0\0\0\0".to_vec()),
    );
    f.route(
        "https://img.example/gone",
        HopResponse {
            status: 404,
            ..HopResponse::default()
        },
    );
    for (path, want) in [
        ("page", StatusCode::UNSUPPORTED_MEDIA_TYPE),
        ("lies.png", StatusCode::UNSUPPORTED_MEDIA_TYPE),
        ("x.svg", StatusCode::UNSUPPORTED_MEDIA_TYPE),
        ("cut.png", StatusCode::UNSUPPORTED_MEDIA_TYPE),
        ("avif", StatusCode::UNSUPPORTED_MEDIA_TYPE),
        ("gone", StatusCode::BAD_GATEWAY),
    ] {
        let r = f
            .get(&f.proxied(&format!("https://img.example/{path}")))
            .await;
        assert_eq!(r.status(), want, "{path}");
        assert_eq!(h(&r, "cache-control"), "no-store");
    }
    assert!(f.storage.list("external/").await.unwrap().is_empty());
}

#[tokio::test]
async fn an_oversized_body_is_refused() {
    let f = fx().await;
    let mut huge = pic(8, 8);
    huge.resize(10 * 1024 * 1024 + 1, 0);
    f.route("https://img.example/huge.png", ok("image/png", huge));
    let r = f.get(&f.proxied("https://img.example/huge.png")).await;
    assert_eq!(r.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn failures_are_remembered_for_a_while() {
    let f = fx().await;
    f.route(
        "https://img.example/page",
        ok("text/html", b"<html>".to_vec()),
    );
    let uri = f.proxied("https://img.example/page");
    assert_eq!(
        f.get(&uri).await.status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    assert_eq!(
        f.get(&uri).await.status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    assert_eq!(f.fetches(), 1, "the second request did not fetch again");
}

#[tokio::test]
async fn an_expired_copy_is_fetched_again() {
    let f = fx().await;
    f.route("https://img.example/a.png", ok("image/png", pic(8, 8)));
    let uri = f.proxied("https://img.example/a.png");
    assert_eq!(f.get(&uri).await.status(), StatusCode::OK);
    assert_eq!(f.fetches(), 1);
    // 29 days later it is still cached; after 31 days it is fetched again.
    f.clock
        .fetch_add(29 * 86_400_000, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(f.get(&uri).await.status(), StatusCode::OK);
    assert_eq!(f.fetches(), 1);
    f.clock
        .fetch_add(2 * 86_400_000, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(f.get(&uri).await.status(), StatusCode::OK);
    assert_eq!(f.fetches(), 2);
}

#[tokio::test]
async fn gc_deletes_only_expired_objects() {
    let f = fx().await;
    f.route("https://img.example/a.png", ok("image/png", pic(8, 8)));
    f.route("https://img.example/b.png", ok("image/png", pic(9, 9)));
    f.get(&f.proxied("https://img.example/a.png")).await;
    f.clock
        .fetch_add(20 * 86_400_000, std::sync::atomic::Ordering::SeqCst);
    f.get(&f.proxied("https://img.example/b.png")).await;
    // Junk without the header is collected too.
    f.storage
        .put("external/junk", Bytes::from_static(b"xx"))
        .await
        .unwrap();
    let now = f.clock.load(std::sync::atomic::Ordering::SeqCst) + 15 * 86_400_000;
    // a.png expires at t0 + 30d (< now = t0 + 35d); b.png at t0 + 50d.
    let removed = external::gc_expired(f.storage.as_ref(), now).await.unwrap();
    assert_eq!(removed, 2);
    let left = f.storage.list("external/").await.unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(
        left[0].key,
        external::cache_key("https://img.example/b.png")
    );
}

#[tokio::test]
async fn cache_misses_are_rate_limited_but_hits_are_not() {
    let mut settings = crate::ApiSettings::default();
    settings.rate_limit.routes.insert(
        external::FETCH_KEY.to_owned(),
        crate::rate_limit::BucketSpec::new(2, 60),
    );
    let f = fx_with(settings).await;
    for i in 0..3 {
        f.route(
            &format!("https://img.example/{i}.png"),
            ok("image/png", pic(8 + i, 8)),
        );
    }
    let u = |i: u32| f.proxied(&format!("https://img.example/{i}.png"));
    assert_eq!(f.get(&u(0)).await.status(), StatusCode::OK);
    assert_eq!(f.get(&u(1)).await.status(), StatusCode::OK);
    let limited = f.get(&u(2)).await;
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(h(&limited, "retry-after").parse::<u64>().unwrap() >= 1);
    // Cached images keep working while the fetch budget is spent.
    assert_eq!(f.get(&u(0)).await.status(), StatusCode::OK);
    assert_eq!(f.fetches(), 2);
}

#[tokio::test]
async fn the_global_fetch_budget_applies_across_callers() {
    let mut settings = crate::ApiSettings::default();
    settings.rate_limit.routes.insert(
        external::GLOBAL_KEY.to_owned(),
        crate::rate_limit::BucketSpec::new(1, 60),
    );
    let f = fx_with(settings).await;
    f.route("https://img.example/a.png", ok("image/png", pic(8, 8)));
    f.route("https://img.example/b.png", ok("image/png", pic(9, 9)));
    assert_eq!(
        f.get(&f.proxied("https://img.example/a.png"))
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        f.get(&f.proxied("https://img.example/b.png"))
            .await
            .status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn the_normal_cdn_route_is_unaffected() {
    let f = fx().await;
    f.storage
        .put("chat/1/2/a.png", Bytes::from(pic(4, 4)))
        .await
        .unwrap();
    let url = f
        .signer
        .signed_url("/cdn/attachments/1/2/a.png", 4_000_000_000);
    assert_eq!(f.get(&url).await.status(), StatusCode::OK);
    // An unknown kind is still a 404 and the proxy path never reaches it.
    assert_eq!(
        f.get("/cdn/nothing/1/2/a.png").await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        f.get("/cdn/external/x").await.status(),
        StatusCode::NOT_FOUND
    );
}

#[test]
fn embeds_get_proxy_urls_for_third_party_images_only() {
    let s = CdnSigner::new(SECRET);
    let mut embeds = vec![json!({
        "type": "rich",
        "image": {"url": "https://img.example/a.png", "proxy_url": "https://evil.example/x"},
        "thumbnail": {"url": "/cdn/attachments/1/2/a.png"},
        "video": {"url": "https://img.example/v.mp4", "proxy_url": "https://evil.example/v"},
        "author": {"name": "n", "icon_url": "http://img.example/i.png"},
        "footer": {"text": "t", "icon_url": "javascript:alert(1)", "proxy_icon_url": "https://evil.example/f"},
    })];
    external::proxy_embed_images(&mut embeds, &s);
    let e = &embeds[0];
    assert_eq!(
        e["image"]["proxy_url"],
        json!(s.external_path("https://img.example/a.png").unwrap())
    );
    assert!(
        e["image"]["proxy_url"]
            .as_str()
            .unwrap()
            .starts_with("/cdn/external/")
    );
    assert!(
        e["thumbnail"].get("proxy_url").is_none(),
        "own CDN path stays direct"
    );
    assert!(e["video"].get("proxy_url").is_none());
    assert_eq!(
        e["author"]["proxy_icon_url"],
        json!(s.external_path("http://img.example/i.png").unwrap())
    );
    assert!(e["footer"].get("proxy_icon_url").is_none());
    // Idempotent: running it twice gives the same result.
    let once = embeds.clone();
    external::proxy_embed_images(&mut embeds, &s);
    assert_eq!(embeds, once);
}
