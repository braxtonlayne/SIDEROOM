//! `GET /guilds/{id}/audit-logs` (LNG-SSET-056/057). Entries are written by the
//! mutating endpoints through `SettingsService::record_audit`.

use super::deps::{SettingsDeps, actor, id, invalid, se};
use crate::routes::Ext;
use crate::routes::guilds::dto::UserRef;
use crate::{Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{Json, Router, extract::Path, routing::get};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sr_chat::settings::AuditObserver;
use sr_chat::settings::audit::{AuditAction, AuditEntry, AuditQuery, MAX_PAGE};
use sr_core::{GuildId, UserId};
use utoipa::ToSchema;

pub fn routes() -> Router<AppState> {
    Router::new().route("/guilds/{guild_id}/audit-logs", get(list_audit_log))
}

#[derive(Debug, Deserialize)]
pub struct AuditLogQuery {
    /// Only entries by this actor.
    pub user_id: Option<String>,
    /// Discord audit-log event number.
    pub action_type: Option<i16>,
    /// Only entries about this user (the target of the action).
    pub target_id: Option<String>,
    /// Entries older than this entry id.
    pub before: Option<String>,
    /// 1 to 100, default 50.
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AuditChangeObject {
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_value: Option<Value>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AuditEntryObject {
    pub id: String,
    pub user_id: String,
    pub target_id: Option<String>,
    pub action_type: i16,
    pub changes: Vec<AuditChangeObject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl AuditEntryObject {
    pub fn of(e: &AuditEntry) -> Self {
        Self {
            id: e.id.to_string(),
            user_id: e.actor_id.to_string(),
            target_id: e.target_id.map(|t| t.to_string()),
            action_type: e.action.code(),
            changes: e
                .changes
                .iter()
                .map(|c| AuditChangeObject {
                    key: c.key.clone(),
                    old_value: c.old_value.clone(),
                    new_value: c.new_value.clone(),
                })
                .collect(),
            options: e.options.clone(),
            reason: e.reason.clone(),
        }
    }
}

/// The audit-log envelope. The lists SIDE ROOM does not populate yet are empty
/// arrays so Discord clients can parse the shape.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AuditLogObject {
    pub audit_log_entries: Vec<AuditEntryObject>,
    pub users: Vec<UserRef>,
    #[schema(value_type = Vec<Object>)]
    pub webhooks: Vec<Value>,
    #[schema(value_type = Vec<Object>)]
    pub integrations: Vec<Value>,
    #[schema(value_type = Vec<Object>)]
    pub threads: Vec<Value>,
    #[schema(value_type = Vec<Object>)]
    pub guild_scheduled_events: Vec<Value>,
    #[schema(value_type = Vec<Object>)]
    pub auto_moderation_rules: Vec<Value>,
}

/// List audit-log entries, newest first (`VIEW_AUDIT_LOG`). Filter by `user_id` (actor),
/// `target_id` (who or what the entry is about) and `action_type`; page with `before`.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/audit-logs",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("user_id" = Option<String>, Query, description = "Actor filter"),
        ("action_type" = Option<i16>, Query, description = "Event number filter"),
        ("target_id" = Option<String>, Query, description = "Target filter: entries about this user or object"),
        ("before" = Option<String>, Query, description = "Entry id cursor"),
        ("limit" = Option<usize>, Query, description = "1 to 100, default 50"),
    ),
    responses((status = 200, body = AuditLogObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_audit_log(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
    crate::routes::messages::error::ApiQuery(q): crate::routes::messages::error::ApiQuery<
        AuditLogQuery,
    >,
) -> ApiResult<Json<AuditLogObject>> {
    let user = actor(&p)?;
    let guild_id: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, guild_id).await?;
    let mut query = AuditQuery::new(guild_id);
    if let Some(u) = &q.user_id {
        query.actor_id = Some(id::<UserId>("user_id", u)?);
    }
    if let Some(code) = q.action_type {
        query.action = Some(
            AuditAction::from_code(code)
                .ok_or_else(|| invalid("action_type", "ENUM_TYPE_BAD_VALUE", "Unknown action."))?,
        );
    }
    if let Some(t) = &q.target_id {
        query.target_id = Some(id::<u64>("target_id", t)?);
    }
    if let Some(b) = &q.before {
        query.before = Some(id::<u64>("before", b)?);
    }
    if let Some(l) = q.limit {
        if !(1..=MAX_PAGE).contains(&l) {
            return Err(invalid(
                "limit",
                "NUMBER_TYPE_MAX",
                "Must be between 1 and 100.",
            ));
        }
        query.limit = l;
    }
    let entries = deps
        .service
        .list_audit(acc.perms, &query)
        .await
        .map_err(se)?;
    let mut ids: Vec<UserId> = entries.iter().map(|e| e.actor_id).collect();
    ids.extend(
        entries
            .iter()
            .filter(|e| is_user_target(e.action))
            .filter_map(|e| e.target_id.map(UserId::new)),
    );
    let users = deps.users(ids.iter().copied()).await?;
    let mut shown: Vec<UserId> = ids.into_iter().filter(|u| users.contains_key(u)).collect();
    shown.sort();
    shown.dedup();
    Ok(Json(AuditLogObject {
        audit_log_entries: entries.iter().map(AuditEntryObject::of).collect(),
        users: shown.into_iter().map(|u| UserRef::of(u, &users)).collect(),
        webhooks: vec![],
        integrations: vec![],
        threads: vec![],
        guild_scheduled_events: vec![],
        auto_moderation_rules: vec![],
    }))
}

/// Actions whose `target_id` is a user.
fn is_user_target(a: AuditAction) -> bool {
    matches!(
        a,
        AuditAction::MemberKick
            | AuditAction::MemberBanAdd
            | AuditAction::MemberBanRemove
            | AuditAction::MemberUpdate
            | AuditAction::MemberRoleUpdate
            | AuditAction::BotAdd
            | AuditAction::ApplicationDecision
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn entry_object_omits_empty_values_and_stringifies_ids() {
        let e = AuditEntry::new(
            9,
            GuildId::new(1),
            UserId::new(2),
            AuditAction::EmojiCreate,
            Some(5),
            Some("because"),
            0,
        )
        .unwrap()
        .with_changes(vec![sr_chat::settings::audit::AuditChange::new(
            "name",
            None,
            Some(json!("party")),
        )]);
        let v = serde_json::to_value(AuditEntryObject::of(&e)).unwrap();
        assert_eq!(v["id"], "9");
        assert_eq!(v["target_id"], "5");
        assert_eq!(v["action_type"], 60);
        assert_eq!(v["reason"], "because");
        assert_eq!(
            v["changes"][0],
            json!({"key": "name", "new_value": "party"})
        );
    }
}

/// An audit observer that announces each stored entry on the bus as
/// `GUILD_AUDIT_LOG_ENTRY_CREATE` (the gateway delivers it only to members with
/// `VIEW_AUDIT_LOG`). Install it on every `SettingsService` that records audit
/// entries. The publish runs on a spawned task so recording never waits on it.
pub fn bus_audit_observer(bus: std::sync::Arc<dyn sr_events::EventBus>) -> AuditObserver {
    std::sync::Arc::new(move |entry: &AuditEntry| {
        let guild = entry.guild_id;
        let mut payload = serde_json::to_value(AuditEntryObject::of(entry)).unwrap_or_default();
        if let Some(o) = payload.as_object_mut() {
            o.insert("guild_id".into(), Value::String(guild.to_string()));
        }
        let ev = sr_events::DomainEvent::AuditLogEntryCreated { guild, payload };
        let bus = bus.clone();
        tokio::spawn(async move {
            for topic in ev.topics() {
                if let Err(e) = bus.publish(topic, ev.clone()).await {
                    tracing::warn!(error = %e, "failed to publish audit log entry event");
                }
            }
        });
    })
}

#[cfg(test)]
mod observer_tests {
    use super::*;
    use futures::StreamExt;
    use sr_events::{DomainEvent, EventBus, InProcessBus, Topic};

    #[tokio::test]
    async fn stored_entries_are_published_with_the_guild_id() {
        let bus = std::sync::Arc::new(InProcessBus::new());
        let mut s = bus.subscribe(Topic::Guild(GuildId::new(1)));
        let observe = bus_audit_observer(bus.clone());
        let entry = AuditEntry::new(
            9,
            GuildId::new(1),
            UserId::new(2),
            AuditAction::RoleCreate,
            Some(5),
            Some("why"),
            1_000,
        )
        .unwrap();
        observe(&entry);
        let ev = tokio::time::timeout(std::time::Duration::from_secs(2), s.next())
            .await
            .unwrap()
            .unwrap();
        let DomainEvent::AuditLogEntryCreated { guild, payload } = ev else {
            panic!("wrong event");
        };
        assert_eq!(guild, GuildId::new(1));
        assert_eq!(payload["id"], "9");
        assert_eq!(payload["guild_id"], "1");
        assert_eq!(payload["user_id"], "2");
        assert_eq!(payload["reason"], "why");
    }
}
