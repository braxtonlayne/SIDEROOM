//! `GET /channels/{id}/bridge-members` (BRG-GW-001): the Discord members bridged into a mapped channel, for the
//! member list. They are not accounts: they have no permissions here and never appear in authz. Anyone who can
//! see the channel sees them (no `MANAGE_GUILD`); a channel that is not bridged answers an empty list.

use super::deps::MessagesDeps;
use super::error::{MsgResult, parse_id};
use super::render::actor;
use crate::routes::Ext;
use crate::{Principal, openapi::ErrorBody};
use axum::{Json, extract::Path};
use serde::Serialize;
use sr_core::{ChannelId, ErrorCode};
use utoipa::ToSchema;

/// A Discord member shown in the list. `id` is the Discord snowflake, the same as `sr_bridge.author_id` on their
/// messages and the `user_id` of a `TYPING_START` they cause.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BridgedMemberObject {
    pub id: String,
    pub name: String,
    /// An image-proxy path, never the Discord URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    /// The member's top role on Discord.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<BridgedRoleObject>,
    pub bot: bool,
    /// `online`, `idle`, `dnd` or `offline`; always `offline` unless the owner granted the presence intent.
    pub presence: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BridgedRoleObject {
    pub name: String,
    /// `0xRRGGBB` as a number; absent for a role without a colour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<i32>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BridgedMembersPage {
    /// `discord`.
    pub platform: String,
    pub members: Vec<BridgedMemberObject>,
}

/// The Discord members bridged into this channel.
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/bridge-members",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 200, body = BridgedMembersPage), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_bridge_members(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    Path(channel): Path<String>,
) -> MsgResult<Json<BridgedMembersPage>> {
    let user = actor(&p);
    let channel_id: ChannelId = parse_id(&channel, ErrorCode::UnknownChannel)?;
    // Seeing the channel is the whole gate.
    deps.access(user, channel_id).await?;
    let rows = match &deps.rich.store {
        Some(store) => store.bridged_members(channel_id).await?,
        None => Vec::new(),
    };
    Ok(Json(BridgedMembersPage {
        platform: "discord".to_owned(),
        members: rows
            .into_iter()
            .map(|m| BridgedMemberObject {
                id: m.discord_user_id,
                name: m.display_name,
                avatar: m
                    .avatar_url
                    .as_deref()
                    .and_then(|u| deps.proxy_external_image(u)),
                role: m.role_name.map(|name| BridgedRoleObject {
                    name,
                    color: m.role_color.filter(|c| *c != 0),
                }),
                bot: m.bot,
                presence: m.presence,
            })
            .collect(),
    }))
}
