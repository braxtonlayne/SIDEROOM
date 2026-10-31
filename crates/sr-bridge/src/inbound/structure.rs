//! BRG-SYN-003: structural dispatches (roles, channels, threads) as "look at object X" (design 11.1).
//!
//! A dispatch only tells the reconciler *which* object to look at; the three-way compare decides what to do. So
//! this module is a pure translation ([`events_for`]) plus the port the sync engine implements
//! ([`StructureSink`]). Echoes are dropped here: a `GUILD_AUDIT_LOG_ENTRY_CREATE` whose reason starts with
//! `sr-bridge:` is the bridge's own write coming back ([`is_echo`]); every other audit entry becomes a
//! [`StructureSink::attribute`] call that names the Discord actor of an object (for conflict rows).
//! Layer 2 of loop prevention (state comparison against `synced_state`) stays with the reconciler.

use async_trait::async_trait;
use serde_json::Value;

use super::convert::id_of;
use crate::discord_structure::{channel_type, is_bridge_reason};
use crate::model::Bridge;

/// What kind of Discord object changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructureKind {
    Role,
    /// A channel or category (anything that is not a thread).
    Channel,
    Thread,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Created,
    Updated,
    Deleted,
}

/// "Look at object `id`." `parent_id` is the channel a thread lives in, when the payload said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructureEvent {
    pub kind: StructureKind,
    pub id: i64,
    pub change: Change,
    pub parent_id: Option<i64>,
}

/// The reconciler's side. Implemented by sync-core over `bridge_sync_ops` (enqueue a Discord-to-SIDE ROOM op for
/// the object, debounced); tests implement it in memory.
#[async_trait]
pub trait StructureSink: Send + Sync {
    /// Look at these objects of the bridged Discord server.
    async fn look_at(&self, bridge: &Bridge, events: Vec<StructureEvent>);
    /// Discord's audit log says `user` (a Discord user id) did something to `target` (not a bridge write).
    async fn attribute(&self, bridge: &Bridge, target: i64, user: i64, action_type: i64);
}

/// Whether an audit-log entry payload carries the bridge's own reason.
pub fn is_echo(data: &Value) -> bool {
    is_bridge_reason(data.get("reason").and_then(Value::as_str))
}

fn thread_kind(v: &Value) -> bool {
    v.get("type")
        .and_then(Value::as_i64)
        .is_some_and(channel_type::is_thread)
}

/// The events one dispatch stands for. Empty for a dispatch that is not structural or not parseable.
pub fn events_for(name: &str, d: &Value) -> Vec<StructureEvent> {
    let channel_event = |change| {
        let kind = if thread_kind(d) {
            StructureKind::Thread
        } else {
            StructureKind::Channel
        };
        d.get("id")
            .and_then(id_of)
            .map(|id| StructureEvent {
                kind,
                id,
                change,
                parent_id: d.get("parent_id").and_then(id_of),
            })
            .into_iter()
            .collect::<Vec<_>>()
    };
    match name {
        "GUILD_ROLE_CREATE" | "GUILD_ROLE_UPDATE" => d
            .pointer("/role/id")
            .and_then(id_of)
            .map(|id| StructureEvent {
                kind: StructureKind::Role,
                id,
                change: if name.ends_with("CREATE") {
                    Change::Created
                } else {
                    Change::Updated
                },
                parent_id: None,
            })
            .into_iter()
            .collect(),
        "GUILD_ROLE_DELETE" => d
            .get("role_id")
            .and_then(id_of)
            .map(|id| StructureEvent {
                kind: StructureKind::Role,
                id,
                change: Change::Deleted,
                parent_id: None,
            })
            .into_iter()
            .collect(),
        "CHANNEL_CREATE" | "THREAD_CREATE" => channel_event(Change::Created),
        "CHANNEL_UPDATE" | "THREAD_UPDATE" => channel_event(Change::Updated),
        "CHANNEL_DELETE" | "THREAD_DELETE" => {
            let mut v = channel_event(Change::Deleted);
            if name == "THREAD_DELETE" {
                for e in &mut v {
                    e.kind = StructureKind::Thread;
                }
            }
            v
        }
        // The active threads of the server (after a join or a reconnect): each is worth a look.
        "THREAD_LIST_SYNC" => d
            .get("threads")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|t| {
                Some(StructureEvent {
                    kind: StructureKind::Thread,
                    id: id_of(t.get("id")?)?,
                    change: Change::Updated,
                    parent_id: t.get("parent_id").and_then(id_of),
                })
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Dispatch names this module understands (the handler routes on it).
pub fn is_structural(name: &str) -> bool {
    matches!(
        name,
        "GUILD_ROLE_CREATE"
            | "GUILD_ROLE_UPDATE"
            | "GUILD_ROLE_DELETE"
            | "CHANNEL_CREATE"
            | "CHANNEL_UPDATE"
            | "CHANNEL_DELETE"
            | "THREAD_CREATE"
            | "THREAD_UPDATE"
            | "THREAD_DELETE"
            | "THREAD_LIST_SYNC"
            | "GUILD_AUDIT_LOG_ENTRY_CREATE"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn roles_channels_and_threads_become_look_at_events() {
        let e = events_for(
            "GUILD_ROLE_UPDATE",
            &json!({"guild_id": "1", "role": {"id": "7"}}),
        );
        assert_eq!(e[0].kind, StructureKind::Role);
        assert_eq!(e[0].id, 7);
        assert_eq!(e[0].change, Change::Updated);
        let e = events_for("GUILD_ROLE_DELETE", &json!({"role_id": "7"}));
        assert_eq!(e[0].change, Change::Deleted);
        let e = events_for("CHANNEL_CREATE", &json!({"id": "9", "type": 4}));
        assert_eq!(
            (e[0].kind, e[0].change),
            (StructureKind::Channel, Change::Created)
        );
        // A thread arriving as CHANNEL_UPDATE is still a thread.
        let e = events_for(
            "CHANNEL_UPDATE",
            &json!({"id": "9", "type": 11, "parent_id": "3"}),
        );
        assert_eq!(
            (e[0].kind, e[0].parent_id),
            (StructureKind::Thread, Some(3))
        );
        let e = events_for("THREAD_DELETE", &json!({"id": "9", "parent_id": "3"}));
        assert_eq!(
            (e[0].kind, e[0].change),
            (StructureKind::Thread, Change::Deleted)
        );
    }

    #[test]
    fn a_thread_list_sync_is_one_event_per_thread_and_junk_is_nothing() {
        let e = events_for(
            "THREAD_LIST_SYNC",
            &json!({"threads": [{"id": "1", "parent_id": "5"}, {"id": "2"}, {"nope": 1}]}),
        );
        assert_eq!(e.len(), 2);
        assert!(events_for("CHANNEL_CREATE", &json!({"id": "zero"})).is_empty());
        assert!(events_for("MESSAGE_CREATE", &json!({})).is_empty());
    }

    #[test]
    fn the_bridges_own_audit_entries_are_echoes() {
        assert!(is_echo(&json!({"reason": "sr-bridge:99"})));
        assert!(!is_echo(&json!({"reason": "cleanup"})));
        assert!(!is_echo(&json!({})));
        assert!(is_structural("GUILD_AUDIT_LOG_ENTRY_CREATE"));
        assert!(!is_structural("MESSAGE_CREATE"));
    }
}
