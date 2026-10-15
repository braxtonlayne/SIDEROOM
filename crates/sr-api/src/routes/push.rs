//! Web Push subscription management (LNG-NTF-003): the VAPID public key and the
//! caller's own browser subscriptions. Delivery lives in `sr-server` (`push_notify`).

use crate::routes::social::error::{ApiJson, MsgResult, invalid};
use crate::routes::social::util::actor;
use crate::state::AppState;
use crate::{Principal, openapi::ErrorBody, routes::Ext};
use axum::{
    Json, Router,
    http::StatusCode,
    routing::{delete, get},
};
use serde::{Deserialize, Serialize};
use sr_push::{PushStore, Subscription, validate_subscription};
use std::sync::Arc;
use utoipa::{OpenApi, ToSchema};

/// Collaborators of the push endpoints; registered on the composition root.
pub struct PushDeps {
    store: Arc<dyn PushStore>,
    public_key: String,
    next_id: Arc<dyn Fn() -> u64 + Send + Sync>,
}

impl PushDeps {
    pub fn new(
        store: Arc<dyn PushStore>,
        public_key: String,
        next_id: Arc<dyn Fn() -> u64 + Send + Sync>,
    ) -> Self {
        Self {
            store,
            public_key,
            next_id,
        }
    }
}

/// The server's VAPID application-server key.
#[derive(Debug, Serialize, ToSchema)]
pub struct VapidKeyObject {
    /// Base64url, uncompressed P-256 point; pass it to `pushManager.subscribe`.
    pub public_key: String,
}

/// The `keys` of a browser `PushSubscription`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PushKeys {
    pub p256dh: String,
    pub auth: String,
}

/// A browser push subscription to register for the caller.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PushSubscriptionBody {
    /// The push service URL (https).
    pub endpoint: String,
    pub keys: PushKeys,
    /// Free-text device label (at most 100 characters).
    #[serde(default)]
    pub device_name: String,
}

/// Which subscription to remove.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PushUnsubscribeBody {
    pub endpoint: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/push/vapid-key", get(vapid_key))
        .route(
            "/users/@me/push-subscriptions",
            delete(unsubscribe).post(subscribe),
        )
}

/// The VAPID public key browsers subscribe against.
#[utoipa::path(
    get,
    path = "/push/vapid-key",
    responses((status = 200, body = VapidKeyObject), (status = 403, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn vapid_key(p: Principal, Ext(deps): Ext<PushDeps>) -> MsgResult<Json<VapidKeyObject>> {
    actor(&p)?;
    Ok(Json(VapidKeyObject {
        public_key: deps.public_key.clone(),
    }))
}

/// Register (or refresh) this browser's push subscription. Re-registering the same
/// endpoint replaces it; a user keeps at most 10 devices, the oldest is dropped.
#[utoipa::path(
    post,
    path = "/users/@me/push-subscriptions",
    request_body = PushSubscriptionBody,
    responses((status = 204, description = "Saved"), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn subscribe(
    p: Principal,
    Ext(deps): Ext<PushDeps>,
    ApiJson(body): ApiJson<PushSubscriptionBody>,
) -> MsgResult<StatusCode> {
    let me = actor(&p)?;
    let sub = Subscription {
        endpoint: body.endpoint,
        p256dh: body.keys.p256dh,
        auth: body.keys.auth,
    };
    validate_subscription(&sub).map_err(|_| {
        invalid(
            "endpoint",
            "PUSH_SUBSCRIPTION_INVALID",
            "Invalid push subscription.",
        )
    })?;
    let device: String = body.device_name.trim().chars().take(100).collect();
    deps.store
        .upsert(
            (deps.next_id)(),
            me.get(),
            &sub,
            &device,
            sr_push::MAX_SUBSCRIPTIONS_PER_USER,
        )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "saving a push subscription failed");
            sr_core::ErrorCode::SrInternal
        })?;
    Ok(StatusCode::NO_CONTENT)
}

/// Remove this browser's push subscription (idempotent).
#[utoipa::path(
    delete,
    path = "/users/@me/push-subscriptions",
    request_body = PushUnsubscribeBody,
    responses((status = 204, description = "Removed"), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn unsubscribe(
    p: Principal,
    Ext(deps): Ext<PushDeps>,
    ApiJson(body): ApiJson<PushUnsubscribeBody>,
) -> MsgResult<StatusCode> {
    let me = actor(&p)?;
    deps.store
        .delete(me.get(), &body.endpoint)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "removing a push subscription failed");
            sr_core::ErrorCode::SrInternal
        })?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(OpenApi)]
#[openapi(
    paths(vapid_key, subscribe, unsubscribe),
    components(schemas(
        ErrorBody,
        VapidKeyObject,
        PushKeys,
        PushSubscriptionBody,
        PushUnsubscribeBody
    ))
)]
pub struct PushApi;

/// The OpenAPI fragment for the push endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    PushApi::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthKind;

    use sr_push::MemoryPushStore;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn deps() -> (Arc<PushDeps>, Arc<MemoryPushStore>) {
        let store = Arc::new(MemoryPushStore::default());
        let n = Arc::new(AtomicU64::new(1));
        let d = PushDeps::new(
            store.clone(),
            "PUBKEY".into(),
            Arc::new(move || n.fetch_add(1, Ordering::Relaxed)),
        );
        (Arc::new(d), store)
    }

    fn me(id: i64) -> Principal {
        Principal {
            user_id: id,
            kind: AuthKind::Session { session_id: 1 },
        }
    }

    fn key() -> (String, String) {
        // Any valid P-256 point and a 16-byte secret.
        let secret = sr_push::VapidKey::generate().unwrap();
        (
            secret.public_key(),
            data_encoding::BASE64URL_NOPAD.encode(&[9_u8; 16]),
        )
    }

    fn body(endpoint: &str) -> PushSubscriptionBody {
        let (p256dh, auth) = key();
        PushSubscriptionBody {
            endpoint: endpoint.into(),
            keys: PushKeys { p256dh, auth },
            device_name: "  Pixel  ".into(),
        }
    }

    #[tokio::test]
    async fn serves_the_vapid_key() {
        let (d, _) = deps();
        let Json(k) = vapid_key(me(1), Ext(d)).await.unwrap();
        assert_eq!(k.public_key, "PUBKEY");
    }

    #[tokio::test]
    async fn subscribe_stores_for_the_caller_and_unsubscribe_removes() {
        let (d, store) = deps();
        let ep = "https://push.example.test/abc";
        let r = subscribe(me(7), Ext(d.clone()), ApiJson(body(ep)))
            .await
            .unwrap();
        assert_eq!(r, StatusCode::NO_CONTENT);
        let mine = store.for_users(&[7]).await.unwrap();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].device_name, "Pixel");
        // Another user cannot remove it.
        unsubscribe(
            me(8),
            Ext(d.clone()),
            ApiJson(PushUnsubscribeBody {
                endpoint: ep.into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(store.for_users(&[7]).await.unwrap().len(), 1);
        unsubscribe(
            me(7),
            Ext(d),
            ApiJson(PushUnsubscribeBody {
                endpoint: ep.into(),
            }),
        )
        .await
        .unwrap();
        assert!(store.for_users(&[7]).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn rejects_insecure_or_malformed_subscriptions() {
        let (d, store) = deps();
        for ep in [
            "http://push.example.test/x",
            "not a url",
            "https://u:p@h.test/x",
        ] {
            assert!(
                subscribe(me(7), Ext(d.clone()), ApiJson(body(ep)))
                    .await
                    .is_err(),
                "{ep}"
            );
        }
        let mut bad = body("https://push.example.test/x");
        bad.keys.auth = "short".into();
        assert!(subscribe(me(7), Ext(d), ApiJson(bad)).await.is_err());
        assert!(store.for_users(&[7]).await.unwrap().is_empty());
    }
}
