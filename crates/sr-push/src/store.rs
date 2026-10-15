//! Persistence seam for push subscriptions and the VAPID key, plus the sealing helper for
//! the key at rest. `sr-server` implements [`PushStore`] over Postgres; the in-memory
//! version serves tests.

use std::sync::Mutex;

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use async_trait::async_trait;

use crate::{PushError, Subscription, VapidKey, ece};

/// Most devices a user can register; the oldest is replaced beyond that.
pub const MAX_SUBSCRIPTIONS_PER_USER: usize = 10;

/// A storage failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("push store: {0}")]
pub struct StoreError(pub String);

/// A subscription as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSubscription {
    pub id: u64,
    pub user_id: u64,
    pub sub: Subscription,
    pub device_name: String,
}

#[async_trait]
pub trait PushStore: Send + Sync {
    /// The persisted (sealed) VAPID secret, if one was generated before.
    async fn vapid_secret(&self) -> Result<Option<Vec<u8>>, StoreError>;
    /// Persist the sealed secret unless one exists. Returns `false` when another node
    /// won the race; the caller then re-reads.
    async fn put_vapid_secret(&self, sealed: &[u8]) -> Result<bool, StoreError>;
    /// Add or replace (same endpoint) a subscription, then trim the user to `max` rows,
    /// dropping the oldest.
    async fn upsert(
        &self,
        id: u64,
        user_id: u64,
        sub: &Subscription,
        device_name: &str,
        max: usize,
    ) -> Result<(), StoreError>;
    /// Remove the user's subscription for `endpoint`; true when one existed.
    async fn delete(&self, user_id: u64, endpoint: &str) -> Result<bool, StoreError>;
    /// Remove a subscription the push service reported gone.
    async fn delete_endpoint(&self, endpoint: &str) -> Result<(), StoreError>;
    /// Every subscription of the given users.
    async fn for_users(&self, users: &[u64]) -> Result<Vec<StoredSubscription>, StoreError>;
    /// Users (among `candidates`) that have at least one subscription.
    async fn users_with_subscriptions(&self, candidates: &[u64]) -> Result<Vec<u64>, StoreError>;
}

/// Load the VAPID key, generating and persisting it on first use. `seal_key` is the
/// 32-byte server master key.
pub async fn load_or_create_vapid(
    store: &dyn PushStore,
    seal_key: &[u8; 32],
) -> Result<VapidKey, PushError> {
    let err = |e: StoreError| PushError::Crypto(e.to_string());
    for _ in 0..3 {
        if let Some(sealed) = store.vapid_secret().await.map_err(err)? {
            let secret = open(seal_key, b"vapid", &sealed)?;
            return VapidKey::from_secret_bytes(&secret);
        }
        let fresh = VapidKey::generate()?;
        let sealed = seal(seal_key, b"vapid", &fresh.secret_bytes())?;
        if store.put_vapid_secret(&sealed).await.map_err(err)? {
            return Ok(fresh);
        }
    }
    Err(PushError::Crypto("could not settle the VAPID key".into()))
}

/// AES-256-GCM with a random nonce prefix; `aad` binds the purpose.
pub fn seal(key: &[u8; 32], aad: &[u8], plain: &[u8]) -> Result<Vec<u8>, PushError> {
    let nonce: [u8; 12] = ece::random()?;
    let ct = Aes256Gcm::new(key.into())
        .encrypt(&Nonce::from(nonce), Payload { msg: plain, aad })
        .map_err(|_| PushError::Crypto("seal failed".into()))?;
    let mut out = nonce.to_vec();
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Inverse of [`seal`].
pub fn open(key: &[u8; 32], aad: &[u8], blob: &[u8]) -> Result<Vec<u8>, PushError> {
    let (nonce, ct) = blob
        .split_at_checked(12)
        .ok_or_else(|| PushError::Crypto("sealed blob too short".into()))?;
    let nonce: [u8; 12] = nonce
        .try_into()
        .map_err(|_| PushError::Crypto("sealed blob too short".into()))?;
    Aes256Gcm::new(key.into())
        .decrypt(&Nonce::from(nonce), Payload { msg: ct, aad })
        .map_err(|_| PushError::Crypto("sealed blob does not open".into()))
}

/// In-memory [`PushStore`] for tests.
#[derive(Default)]
pub struct MemoryPushStore {
    inner: Mutex<MemoryInner>,
}

#[derive(Default)]
struct MemoryInner {
    vapid: Option<Vec<u8>>,
    subs: Vec<StoredSubscription>,
}

#[async_trait]
impl PushStore for MemoryPushStore {
    async fn vapid_secret(&self) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self.inner.lock().unwrap().vapid.clone())
    }

    async fn put_vapid_secret(&self, sealed: &[u8]) -> Result<bool, StoreError> {
        let mut g = self.inner.lock().unwrap();
        if g.vapid.is_some() {
            return Ok(false);
        }
        g.vapid = Some(sealed.to_vec());
        Ok(true)
    }

    async fn upsert(
        &self,
        id: u64,
        user_id: u64,
        sub: &Subscription,
        device_name: &str,
        max: usize,
    ) -> Result<(), StoreError> {
        let mut g = self.inner.lock().unwrap();
        g.subs.retain(|s| s.sub.endpoint != sub.endpoint);
        g.subs.push(StoredSubscription {
            id,
            user_id,
            sub: sub.clone(),
            device_name: device_name.to_owned(),
        });
        let mine: Vec<u64> = g
            .subs
            .iter()
            .filter(|s| s.user_id == user_id)
            .map(|s| s.id)
            .collect();
        if mine.len() > max {
            let drop: Vec<u64> = mine[..mine.len() - max].to_vec();
            g.subs.retain(|s| !drop.contains(&s.id));
        }
        Ok(())
    }

    async fn delete(&self, user_id: u64, endpoint: &str) -> Result<bool, StoreError> {
        let mut g = self.inner.lock().unwrap();
        let before = g.subs.len();
        g.subs
            .retain(|s| !(s.user_id == user_id && s.sub.endpoint == endpoint));
        Ok(g.subs.len() != before)
    }

    async fn delete_endpoint(&self, endpoint: &str) -> Result<(), StoreError> {
        self.inner
            .lock()
            .unwrap()
            .subs
            .retain(|s| s.sub.endpoint != endpoint);
        Ok(())
    }

    async fn for_users(&self, users: &[u64]) -> Result<Vec<StoredSubscription>, StoreError> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .subs
            .iter()
            .filter(|s| users.contains(&s.user_id))
            .cloned()
            .collect())
    }

    async fn users_with_subscriptions(&self, candidates: &[u64]) -> Result<Vec<u64>, StoreError> {
        let g = self.inner.lock().unwrap();
        let mut out: Vec<u64> = candidates
            .iter()
            .copied()
            .filter(|u| g.subs.iter().any(|s| s.user_id == *u))
            .collect();
        out.dedup();
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(n: u32) -> Subscription {
        Subscription {
            endpoint: format!("https://push.example.test/{n}"),
            p256dh: "k".into(),
            auth: "a".into(),
        }
    }

    #[test]
    fn sealing_round_trips_and_binds_the_purpose() {
        let key = [7_u8; 32];
        let blob = seal(&key, b"vapid", b"secret").unwrap();
        assert_eq!(open(&key, b"vapid", &blob).unwrap(), b"secret");
        assert!(open(&key, b"other", &blob).is_err());
        assert!(open(&[8_u8; 32], b"vapid", &blob).is_err());
        assert!(open(&key, b"vapid", &blob[..5]).is_err());
    }

    #[tokio::test]
    async fn the_vapid_key_is_created_once_and_reloaded() {
        let store = MemoryPushStore::default();
        let key = [3_u8; 32];
        let a = load_or_create_vapid(&store, &key).await.unwrap();
        let b = load_or_create_vapid(&store, &key).await.unwrap();
        assert_eq!(a.public_key(), b.public_key());
        let wrong = load_or_create_vapid(&store, &[4_u8; 32]).await;
        assert!(wrong.is_err(), "a different master key cannot open it");
    }

    #[tokio::test]
    async fn upsert_replaces_by_endpoint_and_caps_per_user() {
        let store = MemoryPushStore::default();
        for n in 0..4_u32 {
            store
                .upsert(u64::from(n) + 1, 5, &sub(n), "d", 3)
                .await
                .unwrap();
        }
        let mine = store.for_users(&[5]).await.unwrap();
        assert_eq!(mine.len(), 3);
        assert!(mine.iter().all(|s| s.sub.endpoint != sub(0).endpoint));
        // Re-subscribing the same endpoint replaces it (even for another user).
        store.upsert(99, 6, &sub(3), "phone", 3).await.unwrap();
        assert_eq!(store.for_users(&[5]).await.unwrap().len(), 2);
        assert_eq!(store.for_users(&[6]).await.unwrap().len(), 1);
        assert!(store.delete(6, &sub(3).endpoint).await.unwrap());
        assert!(!store.delete(6, &sub(3).endpoint).await.unwrap());
        assert_eq!(
            store.users_with_subscriptions(&[5, 6, 7]).await.unwrap(),
            vec![5]
        );
    }
}
