//! `/cdn/mount/<id>/<key>`: signed links to media in a member's bucket are answered with a redirect to a short
//! presigned URL, never with bytes (BYOS).

use super::*;
use axum::body::Body;
use axum::http::{Request, Response, StatusCode, header};
use bytes::Bytes;
use sr_storage::mount_storage::MountStorages;
use sr_storage::mounts::{
    MemoryMountRepo, MountError, MountInput, MountOwner, MountRepo, NewMount, Provider, Sealer,
    Secret,
};
use sr_storage::{ObjectStorage, ObjectStoreStorage, cdn::mount_path};
use std::sync::Arc;
use tower::ServiceExt;

const SECRET: &[u8] = b"cdn-mount-secret";

/// Not secure: tags the blob with the AAD.
struct TagSealer;
impl Sealer for TagSealer {
    fn seal(&self, aad: &[u8], plain: &[u8]) -> Result<Vec<u8>, MountError> {
        let mut v = aad.to_vec();
        v.push(b'|');
        v.extend_from_slice(plain);
        Ok(v)
    }
    fn open(&self, aad: &[u8], blob: &[u8]) -> Result<Vec<u8>, MountError> {
        let mut head = aad.to_vec();
        head.push(b'|');
        blob.strip_prefix(head.as_slice())
            .map(<[u8]>::to_vec)
            .ok_or(MountError::Unseal)
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

struct Fx {
    app: axum::Router,
    signer: sr_storage::cdn::CdnSigner,
}

async fn fx(with_mounts: bool) -> Fx {
    let repo = Arc::new(MemoryMountRepo::new());
    let new = NewMount::seal(
        &TagSealer,
        77,
        MountInput {
            owner: MountOwner::User(9),
            label: "shelf".into(),
            provider: Provider::Garage,
            endpoint: "http://127.0.0.1:1".into(),
            region: "garage".into(),
            bucket: "b".into(),
            prefix: "lib".into(),
            path_style: true,
            access_key_id: "AKIAEXAMPLE".into(),
        },
        &Secret::new("s3cret"),
    )
    .unwrap();
    repo.create(new, None).await.unwrap();
    let instance = Arc::new(ObjectStoreStorage::in_memory());
    instance
        .put("cinema/1/poster.webp", Bytes::from_static(b"instance"))
        .await
        .unwrap();
    let mounts = MountStorages::new(instance.clone(), repo, Arc::new(TagSealer));
    let signer = sr_storage::cdn::CdnSigner::new(SECRET);
    let mut deps = CdnDeps::new(instance, signer.clone());
    if with_mounts {
        deps = deps.with_mounts(Arc::new(mounts));
    }
    let state = AppState::builder().extension(Arc::new(deps)).build();
    Fx {
        app: layered(state),
        signer,
    }
}

impl Fx {
    async fn send(&self, method: &str, uri: &str) -> Response<Body> {
        self.app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }
    fn signed(&self, mount: i64, key: &str, left: u64) -> String {
        self.signer
            .signed_url(&mount_path(mount, key), now() + left)
    }
}

fn location(r: &Response<Body>) -> String {
    r.headers()
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned()
}

#[tokio::test]
async fn a_signed_mount_link_redirects_to_a_presigned_url_of_the_bucket() {
    let f = fx(true).await;
    let r = f
        .send("GET", &f.signed(77, "Movies/Alien (1979)/poster.jpg", 3600))
        .await;
    assert_eq!(r.status(), StatusCode::FOUND);
    let loc = location(&r);
    assert!(
        loc.starts_with("http://127.0.0.1:1/b/lib/Movies/Alien%20%281979%29/poster.jpg?"),
        "{loc}"
    );
    assert!(loc.contains("X-Amz-Signature="), "{loc}");
    assert!(loc.contains("X-Amz-Expires=900"), "{loc}");
    assert_eq!(r.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(r.headers()[header::REFERRER_POLICY], "no-referrer");
    let h = f.send("HEAD", &f.signed(77, "Movies/a.mp4", 3600)).await;
    assert_eq!(h.status(), StatusCode::FOUND);
}

#[tokio::test]
async fn the_redirect_never_outlives_the_signed_link_and_never_drops_below_a_minute() {
    let f = fx(true).await;
    let short = f.send("GET", &f.signed(77, "a.jpg", 120)).await;
    let loc = location(&short);
    let ex: u64 = loc
        .split("X-Amz-Expires=")
        .nth(1)
        .and_then(|s| s.split('&').next())
        .and_then(|s| s.parse().ok())
        .unwrap();
    assert!((60..=121).contains(&ex), "{loc}");
    let tiny = f.send("GET", &f.signed(77, "a.jpg", 5)).await;
    assert!(location(&tiny).contains("X-Amz-Expires=60"));
}

#[tokio::test]
async fn the_signature_is_the_credential() {
    let f = fx(true).await;
    assert_eq!(
        f.send("GET", "/cdn/mount/77/Movies/a.jpg").await.status(),
        StatusCode::FORBIDDEN
    );
    let other = f.signed(77, "Movies/b.jpg", 3600);
    let forged = other.replace("Movies/b.jpg", "Movies/a.jpg");
    assert_eq!(f.send("GET", &forged).await.status(), StatusCode::FORBIDDEN);
    let expired = f
        .signer
        .signed_url(&mount_path(77, "Movies/a.jpg"), now().saturating_sub(10));
    assert_eq!(f.send("GET", &expired).await.status(), StatusCode::GONE);
    let moved = f.signed(77, "a.jpg", 3600).replace("/77/", "/78/");
    assert_eq!(f.send("GET", &moved).await.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn unknown_mounts_the_instance_bucket_and_bad_keys_are_refused() {
    let f = fx(true).await;
    assert_eq!(
        f.send("GET", &f.signed(78, "a.jpg", 3600)).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        f.send("GET", &f.signed(1, "cinema/1/poster.webp", 3600))
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    for bad in [
        "/cdn/mount/77",
        "/cdn/mount/77/",
        "/cdn/mount/abc/x.jpg",
        "/cdn/mount/77/../x.jpg",
        "/cdn/mount/77/a/%2e%2e/b",
        "/cdn/mount/77//b",
        "/cdn/mount/77/a%5Cb",
    ] {
        assert_eq!(
            f.send("GET", bad).await.status(),
            StatusCode::BAD_REQUEST,
            "{bad}"
        );
    }
}

#[tokio::test]
async fn without_mounts_configured_the_link_is_a_404_and_the_instance_still_serves() {
    let f = fx(false).await;
    assert_eq!(
        f.send("GET", &f.signed(77, "a.jpg", 3600)).await.status(),
        StatusCode::NOT_FOUND
    );
    let ok = f
        .send(
            "GET",
            &f.signer
                .signed_url("/cdn/cinema/1/poster.webp", now() + 3600),
        )
        .await;
    assert_eq!(ok.status(), StatusCode::OK);
}
