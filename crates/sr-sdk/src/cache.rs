//! In-memory cache of what the gateway has told the bot: itself, guilds, channels, roles.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde_json::Value;

use crate::model::{Channel, Guild, Id, Role, User};

#[derive(Debug, Default)]
struct Inner {
    user: Option<User>,
    application_id: Option<Id>,
    guilds: HashMap<Id, Guild>,
}

/// Cheap to clone; clones share the same data.
#[derive(Debug, Clone, Default)]
pub struct Cache {
    inner: Arc<RwLock<Inner>>,
}

impl Cache {
    pub fn new() -> Self {
        Self::default()
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, Inner> {
        self.inner.read().unwrap_or_else(|e| e.into_inner())
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, Inner> {
        self.inner.write().unwrap_or_else(|e| e.into_inner())
    }

    /// The bot's own user, once READY arrived.
    pub fn current_user(&self) -> Option<User> {
        self.read().user.clone()
    }

    pub fn application_id(&self) -> Option<Id> {
        self.read().application_id.clone()
    }

    pub fn guild(&self, id: &str) -> Option<Guild> {
        self.read().guilds.get(id).cloned()
    }

    pub fn guild_ids(&self) -> Vec<Id> {
        let mut v: Vec<Id> = self.read().guilds.keys().cloned().collect();
        v.sort();
        v
    }

    pub fn channel(&self, id: &str) -> Option<Channel> {
        self.read()
            .guilds
            .values()
            .flat_map(|g| g.channels.iter())
            .find(|c| c.id == id)
            .cloned()
    }

    pub fn role(&self, guild_id: &str, role_id: &str) -> Option<Role> {
        self.read()
            .guilds
            .get(guild_id)?
            .roles
            .iter()
            .find(|r| r.id == role_id)
            .cloned()
    }

    /// Fold one dispatch into the cache. Unknown events and malformed payloads are ignored.
    pub fn apply(&self, name: &str, data: &Value) {
        let mut w = self.write();
        match name {
            "READY" => {
                w.user = data.get("user").and_then(parse);
                w.application_id = data
                    .pointer("/application/id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                w.guilds.clear();
                if let Some(gs) = data.get("guilds").and_then(Value::as_array) {
                    for g in gs.iter().filter_map(parse::<Guild>) {
                        w.guilds.insert(g.id.clone(), g);
                    }
                }
            }
            "GUILD_CREATE" => {
                if let Some(g) = parse::<Guild>(data) {
                    w.guilds.insert(g.id.clone(), g);
                }
            }
            "GUILD_DELETE" => {
                if let Some(id) = data.get("id").and_then(Value::as_str) {
                    w.guilds.remove(id);
                }
            }
            "CHANNEL_CREATE" | "CHANNEL_UPDATE" => {
                if let Some(c) = parse::<Channel>(data)
                    && let Some(g) = c.guild_id.as_ref().and_then(|g| w.guilds.get_mut(g))
                {
                    upsert(&mut g.channels, c, |c| &c.id);
                }
            }
            "CHANNEL_DELETE" => {
                if let (Some(gid), Some(id)) = (
                    data.get("guild_id").and_then(Value::as_str),
                    data.get("id").and_then(Value::as_str),
                ) && let Some(g) = w.guilds.get_mut(gid)
                {
                    g.channels.retain(|c| c.id != id);
                }
            }
            "GUILD_ROLE_CREATE" | "GUILD_ROLE_UPDATE" => {
                if let (Some(gid), Some(role)) = (
                    data.get("guild_id").and_then(Value::as_str),
                    data.get("role").and_then(parse::<Role>),
                ) && let Some(g) = w.guilds.get_mut(gid)
                {
                    upsert(&mut g.roles, role, |r| &r.id);
                }
            }
            "GUILD_ROLE_DELETE" => {
                if let (Some(gid), Some(rid)) = (
                    data.get("guild_id").and_then(Value::as_str),
                    data.get("role_id").and_then(Value::as_str),
                ) && let Some(g) = w.guilds.get_mut(gid)
                {
                    g.roles.retain(|r| r.id != rid);
                }
            }
            _ => {}
        }
    }
}

fn parse<T: serde::de::DeserializeOwned>(v: &Value) -> Option<T> {
    serde_json::from_value(v.clone()).ok()
}

fn upsert<T>(list: &mut Vec<T>, item: T, id: impl Fn(&T) -> &String) {
    match list.iter().position(|x| id(x) == id(&item)) {
        Some(i) => list[i] = item,
        None => list.push(item),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn ready_then_guild_events_build_and_change_the_cache() {
        let c = Cache::new();
        c.apply(
            "READY",
            &json!({"user": {"id": "9", "bot": true}, "application": {"id": "5"}, "guilds": [{"id": "1", "unavailable": true}]}),
        );
        assert_eq!(c.current_user().expect("user").id, "9");
        assert_eq!(c.application_id().as_deref(), Some("5"));
        assert!(c.guild("1").expect("stub").unavailable);

        c.apply(
            "GUILD_CREATE",
            &json!({"id": "1", "name": "Home", "channels": [{"id": "10", "name": "general", "type": 0, "guild_id": "1"}],
                    "roles": [{"id": "20", "name": "Mod"}]}),
        );
        assert_eq!(c.guild("1").expect("full").name, "Home");
        assert_eq!(
            c.channel("10").expect("channel").name.as_deref(),
            Some("general")
        );

        c.apply(
            "CHANNEL_UPDATE",
            &json!({"id": "10", "name": "talk", "type": 0, "guild_id": "1"}),
        );
        c.apply(
            "CHANNEL_CREATE",
            &json!({"id": "11", "name": "new", "type": 0, "guild_id": "1"}),
        );
        assert_eq!(
            c.channel("10").expect("renamed").name.as_deref(),
            Some("talk")
        );
        assert_eq!(c.guild("1").expect("g").channels.len(), 2);
        c.apply("CHANNEL_DELETE", &json!({"id": "11", "guild_id": "1"}));
        assert!(c.channel("11").is_none());

        c.apply(
            "GUILD_ROLE_UPDATE",
            &json!({"guild_id": "1", "role": {"id": "20", "name": "Admin"}}),
        );
        assert_eq!(c.role("1", "20").expect("role").name, "Admin");
        c.apply(
            "GUILD_ROLE_DELETE",
            &json!({"guild_id": "1", "role_id": "20"}),
        );
        assert!(c.role("1", "20").is_none());

        c.apply("GUILD_DELETE", &json!({"id": "1"}));
        assert!(c.guild_ids().is_empty());
    }

    #[test]
    fn malformed_payloads_are_ignored() {
        let c = Cache::new();
        c.apply("GUILD_CREATE", &json!("nonsense"));
        c.apply("CHANNEL_DELETE", &json!({}));
        assert!(c.guild_ids().is_empty());
    }
}
