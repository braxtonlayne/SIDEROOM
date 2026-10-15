//! The inbox's Mentions tab (LNG-NTF-006): `GET /users/@me/mentions`, Discord's
//! recent-mentions endpoint. The source finds candidate messages of the last seven days;
//! this module drops everything the caller can no longer see and renders the rest.

use crate::routes::Ext;
use crate::routes::messages::MessagesDeps;
use crate::routes::messages::model::MessageObject;
use crate::routes::messages::render::render_many;
use crate::routes::social::error::{MsgResult, invalid};
use crate::routes::social::util::actor;
use crate::{Principal, openapi::ErrorBody, state::AppState};
use async_trait::async_trait;
use axum::{Json, Router, extract::Query, routing::get};
use serde::Deserialize;
use sr_core::{ChannelId, GuildId, MessageId, UserId};
use std::collections::HashMap;
use std::sync::Arc;
use utoipa::{IntoParams, OpenApi};

/// Most mentions per page.
pub const MAX_LIMIT: u32 = 50;

/// What to look for.
#[derive(Debug, Clone, Copy)]
pub struct MentionQuery {
    pub user: UserId,
    pub limit: u32,
    /// Only messages with an id below this.
    pub before: Option<MessageId>,
    pub include_roles: bool,
    pub include_everyone: bool,
    pub guild: Option<GuildId>,
}

/// Finds mention candidates; the Postgres implementation lives in `sr-server`.
#[async_trait]
pub trait MentionSource: Send + Sync {
    /// `(channel, message)` pairs, newest first, from the last seven days.
    async fn recent(&self, q: MentionQuery) -> Result<Vec<(ChannelId, MessageId)>, String>;
}

pub struct InboxDeps {
    source: Arc<dyn MentionSource>,
}

impl InboxDeps {
    pub fn new(source: Arc<dyn MentionSource>) -> Self {
        Self { source }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct MentionsParams {
    /// 1 to 50, default 25.
    pub limit: Option<u32>,
    /// Message snowflake: return mentions older than this.
    pub before: Option<String>,
    /// Include messages that mention one of your roles (default true).
    pub roles: Option<bool>,
    /// Include `@everyone` and `@here` (default true).
    pub everyone: Option<bool>,
    /// Restrict to one server.
    pub guild_id: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/users/@me/mentions", get(recent_mentions))
}

/// Messages from the last seven days that mention the caller, newest first.
#[utoipa::path(
    get,
    path = "/users/@me/mentions",
    params(MentionsParams),
    responses((status = 200, body = Vec<MessageObject>), (status = 400, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn recent_mentions(
    p: Principal,
    Ext(deps): Ext<InboxDeps>,
    Ext(messages): Ext<MessagesDeps>,
    Query(q): Query<MentionsParams>,
) -> MsgResult<Json<Vec<MessageObject>>> {
    let me = actor(&p)?;
    let parse = |field: &str, raw: &Option<String>| -> MsgResult<Option<u64>> {
        match raw {
            None => Ok(None),
            Some(s) => s
                .parse::<u64>()
                .map(Some)
                .map_err(|_| invalid(field, "SNOWFLAKE_INVALID", "Not a valid ID.")),
        }
    };
    let query = MentionQuery {
        user: me,
        limit: q.limit.unwrap_or(25).clamp(1, MAX_LIMIT),
        before: parse("before", &q.before)?.map(MessageId::new),
        include_roles: q.roles.unwrap_or(true),
        include_everyone: q.everyone.unwrap_or(true),
        guild: parse("guild_id", &q.guild_id)?.map(GuildId::new),
    };
    let found = deps.source.recent(query).await.map_err(|e| {
        tracing::error!(error = %e, "listing mentions failed");
        sr_core::ErrorCode::SrInternal
    })?;
    let mut by_channel: HashMap<ChannelId, Vec<MessageId>> = HashMap::new();
    for (c, m) in &found {
        by_channel.entry(*c).or_default().push(*m);
    }
    let mut msgs = Vec::new();
    for (channel, ids) in by_channel {
        // Anything the caller can no longer see (left the server, lost the channel) is dropped.
        let Ok(access) = messages.access(me, channel).await else {
            continue;
        };
        if access.require_view().is_err() {
            continue;
        }
        if let Ok(list) = messages.repo.get_messages(channel, &ids).await {
            msgs.extend(list.into_iter().filter(sr_chat::Message::is_visible));
        }
    }
    msgs.sort_by_key(|m| std::cmp::Reverse(m.id));
    let rendered = render_many(&messages, me, &msgs).await?;
    Ok(Json(rendered))
}

#[derive(OpenApi)]
#[openapi(paths(recent_mentions), components(schemas(ErrorBody, MessageObject)))]
pub struct InboxApi;

/// The OpenAPI fragment for the inbox endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    InboxApi::openapi()
}
