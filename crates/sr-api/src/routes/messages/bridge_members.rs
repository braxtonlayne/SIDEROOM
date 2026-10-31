//! `GET /guilds/{guild_id}/bridge/members` (BRG-GW-001): the Discord members bridged into the server, for the
//! member list. They are not accounts: no permissions here, never in authz. Any member of the server may read the
//! list (no `MANAGE_GUILD`); a server without a bridge answers an empty list.

use super::deps::MessagesDeps;
use super::error::{MsgResult, parse_id};
use super::render::actor;
use crate::routes::Ext;
use crate::{Principal, openapi::ErrorBody};
use axum::{Json, extract::Path};
use serde::Serialize;
use sr_core::{ErrorCode, GuildId};
use utoipa::ToSchema;

/// A Discord member shown in the list. `id` is the Discord user id as a decimal string: the same as
/// `sr_bridge.member_id` on their messages, the `user_id` of a `TYPING_START` they cause, and the id inside the
/// standard `<@id>` mention token both ways.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BridgedMemberObject {
    pub id: String,
    pub display_name: String,
    /// An image-proxy path (`/cdn/external/...`), never the Discord URL; null without an avatar.
    pub avatar: Option<String>,
    /// The member's top role on Discord.
    pub role_name: Option<String>,
    /// `0xRRGGBB` as a number; null for a role without a colour.
    pub role_color: Option<i32>,
    pub bot: bool,
    /// `online`, `idle`, `dnd` or `offline`; null unless the owner granted the presence intent.
    pub presence: Option<String>,
    /// Whole-server bridges only: the SIDE ROOM role ids the member's Discord roles map to, for display (grouping,
    /// colour). Never an authorization input; absent for a selective bridge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_ids: Option<Vec<String>>,
}

/// The Discord members bridged into this server.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/bridge/members",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<BridgedMemberObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_bridge_members(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    Path(guild): Path<String>,
) -> MsgResult<Json<Vec<BridgedMemberObject>>> {
    let user = actor(&p);
    let guild_id: GuildId = parse_id(&guild, ErrorCode::UnknownGuild)?;
    // Being a member of the server is the whole gate.
    if deps.repo.get_member(guild_id, user).await?.is_none() {
        return Err(ErrorCode::MissingAccess.into());
    }
    let rows = match &deps.rich.store {
        Some(store) => store.bridged_members(guild_id).await?,
        None => Vec::new(),
    };
    Ok(Json(
        rows.into_iter()
            .map(|m| BridgedMemberObject {
                id: m.discord_user_id,
                display_name: m.display_name,
                avatar: m
                    .avatar_url
                    .as_deref()
                    .and_then(|u| deps.proxy_external_image(u)),
                role_name: m.role_name,
                role_color: m.role_color.filter(|c| *c != 0),
                bot: m.bot,
                presence: m.presence,
                role_ids: m.role_ids,
            })
            .collect(),
    ))
}
