//! `STORAGE_MOUNT_UPDATE` (BYOS-NAV-001): the gateway event the mount routes publish when what a viewer's
//! shelves reach may have changed, so the nav refetches `GET /users/@me/shelves/summary`.
//!
//! The payload names the mount and the change, never a secret: `{mount_id, owner_kind, guild_id?, change}`
//! with `change` one of `created`, `updated`, `deleted`, `attached`, `detached`, `shared`, `unshared`.
//!
//! * A member's own mount goes to that member's sessions (`UserScoped`), and a share change also to the friend.
//! * A server's mount (or the instance bucket attached to or detached from a server) goes to the server's
//!   sessions (`DomainEvent::StorageMountUpdated`), because every member's reach may change.
//!
//! A failed publish only logs: the next page load reads the summary anyway.

use std::sync::Arc;

use serde_json::{Value, json};
use sr_core::{GuildId, UserId};
use sr_events::{DomainEvent, EventBus, Topic};
use sr_storage::mounts::MountOwner;

/// The wire name of the event.
pub const EVENT_NAME: &str = "STORAGE_MOUNT_UPDATE";

/// Publishes mount changes on the bus. Registered as an extension; routes take it as `MaybeExt`.
pub struct StorageEvents {
    bus: Arc<dyn EventBus>,
}

/// The payload of one event.
pub fn payload(mount_id: i64, owner: MountOwner, change: &str) -> Value {
    let (kind, guild) = match owner {
        MountOwner::User(_) => ("user", None),
        MountOwner::Guild(g) => ("guild", Some(g)),
        MountOwner::Platform => ("platform", None),
    };
    let mut v = json!({
        "mount_id": mount_id.to_string(),
        "owner_kind": kind,
        "change": change,
    });
    if let Some(g) = guild {
        v["guild_id"] = Value::String(g.to_string());
    }
    v
}

impl StorageEvents {
    /// Publish on `bus`.
    pub fn new(bus: Arc<dyn EventBus>) -> Self {
        Self { bus }
    }

    /// A mount of `owner` changed.
    pub async fn mount_changed(&self, mount_id: i64, owner: MountOwner, change: &str) {
        match owner {
            MountOwner::User(u) => self.to_users(&[u], payload(mount_id, owner, change)).await,
            MountOwner::Guild(g) => self.to_guild(g, payload(mount_id, owner, change)).await,
            MountOwner::Platform => {}
        }
    }

    /// The instance bucket was attached to or detached from a server.
    pub async fn attachment_changed(&self, mount_id: i64, guild: i64, change: &str) {
        let mut p = payload(mount_id, MountOwner::Platform, change);
        p["guild_id"] = Value::String(guild.to_string());
        self.to_guild(guild, p).await;
    }

    /// A member shared a mount with a friend, or took the share back: both sessions refetch.
    pub async fn share_changed(&self, mount_id: i64, owner: i64, friend: i64, change: &str) {
        self.to_users(
            &[owner, friend],
            payload(mount_id, MountOwner::User(owner), change),
        )
        .await;
    }

    async fn to_users(&self, users: &[i64], payload: Value) {
        let ids: Vec<UserId> = users
            .iter()
            .filter_map(|u| u64::try_from(*u).ok().map(UserId::new))
            .collect();
        let ev = DomainEvent::UserScoped {
            users: ids.clone(),
            name: EVENT_NAME.to_owned(),
            payload,
        };
        for u in ids {
            if let Err(e) = self.bus.publish(Topic::User(u), ev.clone()).await {
                tracing::warn!(error = %e, event = EVENT_NAME, "publishing a storage event failed");
            }
        }
    }

    async fn to_guild(&self, guild: i64, payload: Value) {
        let Ok(g) = u64::try_from(guild) else { return };
        let ev = DomainEvent::StorageMountUpdated {
            guild: GuildId::new(g),
            payload,
        };
        for topic in ev.topics() {
            if let Err(e) = self.bus.publish(topic, ev.clone()).await {
                tracing::warn!(error = %e, event = EVENT_NAME, "publishing a storage event failed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[test]
    fn the_payload_names_the_mount_and_the_change_only() {
        let p = payload(7, MountOwner::Guild(3), "created");
        assert_eq!(
            p,
            json!({"mount_id": "7", "owner_kind": "guild", "guild_id": "3", "change": "created"})
        );
        let u = payload(8, MountOwner::User(2), "deleted");
        assert_eq!(
            u,
            json!({"mount_id": "8", "owner_kind": "user", "change": "deleted"})
        );
    }

    #[tokio::test]
    async fn a_member_mount_goes_to_the_member_and_a_server_mount_to_the_server() {
        let bus = Arc::new(sr_events::InProcessBus::default());
        let mut me = bus.subscribe(Topic::User(UserId::new(2)));
        let mut friend = bus.subscribe(Topic::User(UserId::new(5)));
        let mut guild = bus.subscribe(Topic::Guild(GuildId::new(3)));
        let ev = StorageEvents::new(bus.clone());
        ev.mount_changed(8, MountOwner::User(2), "updated").await;
        ev.share_changed(8, 2, 5, "shared").await;
        ev.mount_changed(9, MountOwner::Guild(3), "created").await;
        ev.attachment_changed(1, 3, "attached").await;

        let first = me.next().await.unwrap();
        assert!(matches!(&first, DomainEvent::UserScoped { name, .. } if name == EVENT_NAME));
        let second = me.next().await.unwrap();
        assert!(
            matches!(&second, DomainEvent::UserScoped { payload, .. } if payload["change"] == "shared")
        );
        let f = friend.next().await.unwrap();
        assert!(
            matches!(&f, DomainEvent::UserScoped { payload, .. } if payload["mount_id"] == "8")
        );
        let g1 = guild.next().await.unwrap();
        assert!(
            matches!(&g1, DomainEvent::StorageMountUpdated { payload, .. } if payload["change"] == "created")
        );
        let g2 = guild.next().await.unwrap();
        assert!(
            matches!(&g2, DomainEvent::StorageMountUpdated { payload, .. } if payload["owner_kind"] == "platform" && payload["guild_id"] == "3")
        );
    }
}
