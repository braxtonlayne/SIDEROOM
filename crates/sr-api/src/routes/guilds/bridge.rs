//! The Discord bridge routes (BRG-COR-001, module `bridge.enabled`, migration 1720; D-381).
//!
//! Two classes of caller, both session-only (a bot never configures the bridge):
//!
//! * **`guild_owner`** (the server owner alone, `MANAGE_GUILD` and `ADMINISTRATOR` are 403, with the same
//!   second-factor step as the other owner-only server actions): `PUT|DELETE /guilds/{id}/bridge/token`, the
//!   list of Discord webhooks a teardown could not delete, and (through the one feature-toggle handler, on the
//!   module's `owner_only` property) switching the module on or off.
//! * **`bridge_admin`** (`MANAGE_GUILD`, once the owner switched the module on): the status, the Discord
//!   channels the bot sees, the channel maps and the test. They answer 404 while the module is off.
//!
//! The operator's switch `platform.bridge` off refuses every route here with 403 `SR_FEATURE_DISABLED`. No
//! route returns, accepts in a query, or logs a token: the bot token arrives once in the body of the `PUT`.

use crate::auth::SessionOnly;
use crate::error::ApiResult;
use crate::openapi::ErrorBody;
use crate::routes::Ext;
use crate::routes::guilds::safety::MfaGate;
use crate::routes::messages::error::ApiJson;
use crate::routes::settings::deps::{GuildAccess, SettingsDeps, actor, id, invalid};
use crate::{HttpError, state::AppState};
use axum::{
    Json, Router,
    extract::Path,
    http::StatusCode,
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};
use sr_bridge::{BridgeError, BridgeService, Direction, TeardownReason};
use sr_core::permissions::Permissions;
use sr_core::{ApiError, ChannelId, ErrorCode, GuildId, UserId};
use std::sync::Arc;
use utoipa::ToSchema;

/// The permissions the invite link asks for: view channels, send messages, read history, add reactions and
/// manage webhooks (the bridge's own webhooks, nothing else).
const INVITE_PERMISSIONS: u64 = 536_939_584;

/// The routes, relative to the API prefix.
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/guilds/{guild_id}/bridge/token",
            put(put_token).delete(delete_token),
        )
        .route("/guilds/{guild_id}/bridge/leftovers", get(leftovers))
        .route("/guilds/{guild_id}/bridge/status", get(status))
        .route(
            "/guilds/{guild_id}/bridge/discord-channels",
            get(discord_channels),
        )
        .route(
            "/guilds/{guild_id}/bridge/maps",
            get(list_maps).post(create_map),
        )
        .route(
            "/guilds/{guild_id}/bridge/maps/{map_id}",
            axum::routing::patch(patch_map).delete(delete_map),
        )
        .route(
            "/guilds/{guild_id}/bridge/maps/{map_id}/test",
            post(test_map),
        )
}

// ---- wire types ------------------------------------------------------------------------------------

/// The connected bot and the Discord server it works in. The token is never part of it.
#[derive(Debug, Serialize, ToSchema)]
pub struct BridgeObject {
    /// The Discord server's snowflake.
    pub discord_guild_id: String,
    /// The Discord server's name.
    pub discord_guild_name: String,
    /// The bot's Discord user id (also its application id).
    pub bot_user_id: String,
    /// The bot's name.
    pub bot_name: String,
    /// `connecting`, `connected`, `disconnected` or `token_invalid`.
    pub status: String,
    /// The gateway intents Discord granted (the privileged `MESSAGE_CONTENT` intent is bit 15).
    pub intents_granted: i64,
    /// Who connected it.
    pub created_by: String,
    /// When, unix ms.
    pub created_at: i64,
    /// A link that adds the bot to a Discord server with the permissions the bridge needs.
    pub invite_url: String,
}

/// One channel map with the numbers beside it.
#[derive(Debug, Serialize, ToSchema)]
pub struct MapObject {
    pub id: String,
    /// The SIDE ROOM channel.
    pub channel_id: String,
    /// The Discord channel.
    pub discord_channel_id: String,
    pub discord_channel_name: String,
    /// `both`, `to_discord` or `from_discord`.
    pub direction: String,
    /// The bridge holds a Discord webhook for posting here.
    pub has_webhook: bool,
    /// Posts waiting for their turn under Discord's rate limits.
    pub queued: i64,
    /// Posts that failed for good.
    pub dead: i64,
    pub created_at: i64,
}

/// A Discord webhook that could not be removed from Discord.
#[derive(Debug, Serialize, ToSchema)]
pub struct LeftoverObject {
    /// The webhook's id: remove it from Discord's Integrations page by hand.
    pub webhook_id: String,
    pub discord_channel_id: String,
}

/// What the owner's screen shows.
#[derive(Debug, Serialize, ToSchema)]
pub struct BridgeStatusObject {
    /// The operator's switch.
    pub platform_on: bool,
    /// The server's module.
    pub module_on: bool,
    /// A bot token is held.
    pub connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bridge: Option<BridgeObject>,
    pub maps: Vec<MapObject>,
    /// Posts waiting.
    pub queued: i64,
    /// Posts that failed for good (the dead-letter counter).
    pub dead_letters: i64,
    pub leftovers: Vec<LeftoverObject>,
    /// The viewer is the server owner (may paste and revoke the token and switch the module off).
    pub is_owner: bool,
}

/// Body of `PUT /guilds/{id}/bridge/token`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct TokenBody {
    /// The bot token of the owner's own Discord application. Sealed on arrival, shown nowhere.
    pub token: String,
    /// The Discord server to bridge, when the bot is in several.
    #[serde(default)]
    pub discord_guild_id: Option<String>,
}

/// What a teardown did.
#[derive(Debug, Serialize, ToSchema)]
pub struct TeardownObject {
    pub maps_removed: i64,
    pub webhooks_deleted: i64,
    /// Webhooks still on Discord, to remove by hand.
    pub leftovers: Vec<LeftoverObject>,
}

/// The Discord webhooks a teardown could not delete.
#[derive(Debug, Serialize, ToSchema)]
pub struct LeftoverList {
    pub leftovers: Vec<LeftoverObject>,
}

/// A Discord channel the bot sees.
#[derive(Debug, Serialize, ToSchema)]
pub struct DiscordChannelObject {
    pub id: String,
    pub name: String,
}

/// Body of `POST /guilds/{id}/bridge/maps`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateMapBody {
    /// The SIDE ROOM channel (a text or announcement channel of this server).
    pub channel_id: String,
    /// The Discord channel.
    pub discord_channel_id: String,
    /// `both` (default), `to_discord` or `from_discord`.
    #[serde(default)]
    pub direction: Option<String>,
}

/// Body of `PATCH /guilds/{id}/bridge/maps/{map}`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PatchMapBody {
    /// `both`, `to_discord` or `from_discord`.
    pub direction: String,
}

// ---- helpers ---------------------------------------------------------------------------------------

fn s(v: i64) -> String {
    v.to_string()
}

fn map_err(e: BridgeError) -> HttpError {
    match e {
        BridgeError::NotFound(what) => {
            ApiError::with_message(ErrorCode::SrNotFound, format!("Unknown {what}")).into()
        }
        BridgeError::Invalid(m) => {
            let (field, msg) = m.split_once(": ").unwrap_or(("bridge", m.as_str()));
            invalid(field, "BRIDGE_INVALID", msg)
        }
        BridgeError::BadToken(m) => invalid("token", "BRIDGE_BAD_TOKEN", &m),
        BridgeError::Conflict(m) => ApiError::with_message(ErrorCode::SrConflict, m).into(),
        BridgeError::Discord(m) => {
            // Discord's answer is the owner's business, but never a token: the client already redacts.
            tracing::warn!(error = %m, "discord bridge: Discord refused or failed");
            ApiError::with_message(
                ErrorCode::SrConflict,
                "Discord did not accept the request. Check the bot's permissions in the Discord server and try again.",
            )
            .into()
        }
        BridgeError::Seal | BridgeError::Storage(_) => {
            tracing::error!(error = %e, "discord bridge failure");
            ErrorCode::SrInternal.into()
        }
    }
}

fn bridge_object(b: &sr_bridge::Bridge) -> BridgeObject {
    BridgeObject {
        discord_guild_id: s(b.discord_guild_id),
        discord_guild_name: b.discord_guild_name.clone(),
        bot_user_id: s(b.bot_user_id),
        bot_name: b.bot_name.clone(),
        status: b.status.as_str().to_owned(),
        intents_granted: b.intents_granted,
        created_by: s(b.created_by),
        created_at: b.created_at_ms,
        invite_url: format!(
            "https://discord.com/oauth2/authorize?client_id={}&scope=bot&permissions={INVITE_PERMISSIONS}",
            b.bot_user_id
        ),
    }
}

fn map_object(m: &sr_bridge::MapView) -> MapObject {
    MapObject {
        id: s(m.map.id),
        channel_id: s(m.map.channel_id),
        discord_channel_id: s(m.map.discord_channel_id),
        discord_channel_name: m.map.discord_channel_name.clone(),
        direction: m.map.direction.as_str().to_owned(),
        has_webhook: m.map.discord_webhook_id.is_some(),
        queued: m.queued,
        dead: m.dead,
        created_at: m.map.created_at_ms,
    }
}

fn leftover_objects(l: &[sr_bridge::Leftover]) -> Vec<LeftoverObject> {
    l.iter()
        .map(|x| LeftoverObject {
            webhook_id: s(x.webhook_id),
            discord_channel_id: s(x.discord_channel_id),
        })
        .collect()
}

fn direction(raw: &str) -> ApiResult<Direction> {
    Direction::parse(raw).ok_or_else(|| {
        invalid(
            "direction",
            "ENUM_TYPE_INVALID",
            "Direction must be both, to_discord or from_discord.",
        )
    })
}

/// Who may call and in which state of the switches.
struct Caller {
    gid: i64,
    user: i64,
    acc: GuildAccess,
}

/// Resolve the session, the server and its access; no switch is checked yet.
async fn who(
    p: &crate::Principal,
    deps: &SettingsDeps,
    guild_id: &str,
) -> ApiResult<(GuildId, UserId, GuildAccess)> {
    let user = actor(p)?;
    let gid: GuildId = id("guild_id", guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    Ok((gid, user, acc))
}

fn raw(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

/// The operator's switch and the server's module, in that order: 403 `SR_FEATURE_DISABLED`, then 404.
async fn switches(bridge: &BridgeService, gid: i64) -> ApiResult<()> {
    if !bridge.platform_on().await.map_err(map_err)? {
        return Err(ApiError::with_message(
            ErrorCode::SrFeatureDisabled,
            "The Discord bridge is switched off on this installation.",
        )
        .into());
    }
    if !bridge.module_on(gid).await.map_err(map_err)? {
        return Err(ApiError::with_message(
            ErrorCode::SrNotFound,
            "The Discord bridge is not switched on for this server.",
        )
        .into());
    }
    Ok(())
}

/// A `MANAGE_GUILD` manager of a server whose bridge is on (class `bridge_admin`).
async fn bridge_admin(
    p: &crate::Principal,
    deps: &SettingsDeps,
    bridge: &BridgeService,
    guild_id: &str,
) -> ApiResult<Caller> {
    let (gid, user, acc) = who(p, deps, guild_id).await?;
    if !acc.perms.has(Permissions::MANAGE_GUILD) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    let (g, u) = (raw(gid.get()), raw(user.get()));
    switches(bridge, g).await?;
    Ok(Caller {
        gid: g,
        user: u,
        acc,
    })
}

/// The server owner alone, with the second-factor step of the server (class `guild_owner`). `ADMINISTRATOR` and
/// `MANAGE_GUILD` do not make an owner.
async fn guild_owner(
    p: &crate::Principal,
    gate: &MfaGate,
    deps: &SettingsDeps,
    guild_id: &str,
) -> ApiResult<Caller> {
    let (gid, user, acc) = who(p, deps, guild_id).await?;
    if acc.guild.owner_id != user {
        return Err(ErrorCode::MissingPermissions.into());
    }
    gate.check_level(user, acc.guild.mfa_level).await?;
    Ok(Caller {
        gid: raw(gid.get()),
        user: raw(user.get()),
        acc,
    })
}

// ---- owner: the token ------------------------------------------------------------------------------

/// Connect the owner's own Discord bot (`PUT`, owner only). The token is validated as a bot token with Discord,
/// sealed, and never shown again; pasting another token of the same bot replaces it.
#[utoipa::path(
    put,
    path = "/guilds/{guild_id}/bridge/token",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = TokenBody,
    responses((status = 200, body = BridgeObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn put_token(
    SessionOnly(p): SessionOnly,
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<TokenBody>,
) -> ApiResult<Json<BridgeObject>> {
    let c = guild_owner(&p, &gate, &deps, &guild_id).await?;
    switches(&bridge, c.gid).await?;
    let pick = match body.discord_guild_id.as_deref() {
        Some(raw) => Some(id::<i64>("discord_guild_id", raw)?),
        None => None,
    };
    let b = bridge
        .connect(c.gid, c.user, &body.token, pick)
        .await
        .map_err(map_err)?;
    Ok(Json(bridge_object(&b)))
}

/// Revoke the token (`DELETE`, owner only): the bridge is torn down (the webhooks it created are deleted on
/// Discord, the maps and the token go) and the module stays on, ready for a new token.
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/bridge/token",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = TeardownObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_token(
    SessionOnly(p): SessionOnly,
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<TeardownObject>> {
    let c = guild_owner(&p, &gate, &deps, &guild_id).await?;
    switches(&bridge, c.gid).await?;
    let r = bridge
        .teardown(c.gid, c.user, TeardownReason::Owner)
        .await
        .map_err(map_err)?;
    Ok(Json(TeardownObject {
        maps_removed: r.maps_removed,
        webhooks_deleted: r.webhooks_deleted,
        leftovers: leftover_objects(&r.leftovers),
    }))
}

/// The Discord webhooks a teardown could not delete (owner only; readable after the module was switched off, so
/// the dialog can name them).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/bridge/leftovers",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = LeftoverList), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn leftovers(
    SessionOnly(p): SessionOnly,
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<LeftoverList>> {
    let c = guild_owner(&p, &gate, &deps, &guild_id).await?;
    if !bridge.platform_on().await.map_err(map_err)? {
        return Err(ApiError::with_message(
            ErrorCode::SrFeatureDisabled,
            "The Discord bridge is switched off on this installation.",
        )
        .into());
    }
    let l = bridge.leftovers(c.gid).await.map_err(map_err)?;
    Ok(Json(LeftoverList {
        leftovers: leftover_objects(&l),
    }))
}

// ---- managers: status and maps ---------------------------------------------------------------------

/// The bridge of this server (`MANAGE_GUILD`; 404 while the module is off).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/bridge/status",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = BridgeStatusObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn status(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<BridgeStatusObject>> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    let r = bridge.status(c.gid).await.map_err(map_err)?;
    Ok(Json(BridgeStatusObject {
        platform_on: r.platform_on,
        module_on: r.module_on,
        connected: r.connected,
        bridge: r.bridge.as_ref().map(bridge_object),
        maps: r.maps.iter().map(map_object).collect(),
        queued: r.queued,
        dead_letters: r.dead_letters,
        leftovers: leftover_objects(&r.leftovers),
        is_owner: c.acc.guild.owner_id == UserId::new(u64::try_from(c.user).unwrap_or(0)),
    }))
}

/// The text channels the bot sees in the connected Discord server (`MANAGE_GUILD`).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/bridge/discord-channels",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<DiscordChannelObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn discord_channels(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<Vec<DiscordChannelObject>>> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    let list = bridge.discord_channels(c.gid).await.map_err(map_err)?;
    Ok(Json(
        list.into_iter()
            .map(|d| DiscordChannelObject {
                id: s(d.id),
                name: d.name,
            })
            .collect(),
    ))
}

/// The channel maps (`MANAGE_GUILD`).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/bridge/maps",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<MapObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list_maps(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<Vec<MapObject>>> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    let r = bridge.status(c.gid).await.map_err(map_err)?;
    Ok(Json(r.maps.iter().map(map_object).collect()))
}

/// Map a SIDE ROOM channel to a Discord channel (`MANAGE_GUILD`). Direct messages and anything that is not a
/// text or announcement channel are refused: end-to-end encrypted conversations are never bridged.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/bridge/maps",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = CreateMapBody,
    responses((status = 201, body = MapObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create_map(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<CreateMapBody>,
) -> ApiResult<(StatusCode, Json<MapObject>)> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    let channel: i64 = id("channel_id", &body.channel_id)?;
    let discord_channel: i64 = id("discord_channel_id", &body.discord_channel_id)?;
    let dir = match body.direction.as_deref() {
        Some(d) => direction(d)?,
        None => Direction::Both,
    };
    let m = bridge
        .create_map(c.gid, c.user, channel, discord_channel, dir)
        .await
        .map_err(map_err)?;
    Ok((
        StatusCode::CREATED,
        Json(map_object(&sr_bridge::MapView {
            map: m,
            queued: 0,
            dead: 0,
        })),
    ))
}

/// Change the direction of a map (`MANAGE_GUILD`).
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/bridge/maps/{map_id}",
    params(("guild_id" = String, Path, description = "Guild snowflake"), ("map_id" = String, Path, description = "Map snowflake")),
    request_body = PatchMapBody,
    responses((status = 200, body = MapObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn patch_map(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Path((guild_id, map_id)): Path<(String, String)>,
    ApiJson(body): ApiJson<PatchMapBody>,
) -> ApiResult<Json<MapObject>> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    let map_id: i64 = id("map_id", &map_id)?;
    let dir = direction(&body.direction)?;
    let m = bridge
        .update_map(c.gid, map_id, dir)
        .await
        .map_err(map_err)?;
    let view = bridge
        .status(c.gid)
        .await
        .map_err(map_err)?
        .maps
        .into_iter()
        .find(|v| v.map.id == m.id)
        .ok_or_else(|| HttpError::from(ErrorCode::SrNotFound))?;
    Ok(Json(map_object(&view)))
}

/// Remove a map (`MANAGE_GUILD`): its webhook is deleted on Discord and the channel is told.
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/bridge/maps/{map_id}",
    params(("guild_id" = String, Path, description = "Guild snowflake"), ("map_id" = String, Path, description = "Map snowflake")),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_map(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Path((guild_id, map_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    let map_id: i64 = id("map_id", &map_id)?;
    bridge.delete_map(c.gid, map_id).await.map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Post a hello through a map's webhook (`MANAGE_GUILD`), to see that the bridge works.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/bridge/maps/{map_id}/test",
    params(("guild_id" = String, Path, description = "Guild snowflake"), ("map_id" = String, Path, description = "Map snowflake")),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn test_map(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Path((guild_id, map_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    let map_id: i64 = id("map_id", &map_id)?;
    bridge.test_post(c.gid, map_id).await.map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// OpenAPI fragment of the bridge routes.
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        put_token,
        delete_token,
        leftovers,
        status,
        discord_channels,
        list_maps,
        create_map,
        patch_map,
        delete_map,
        test_map,
    ),
    components(schemas(
        ErrorBody,
        BridgeObject,
        MapObject,
        LeftoverObject,
        BridgeStatusObject,
        TokenBody,
        TeardownObject,
        LeftoverList,
        DiscordChannelObject,
        CreateMapBody,
        PatchMapBody,
    ))
)]
pub struct BridgeApiDoc;

/// The OpenAPI fragment of the bridge routes.
pub fn openapi() -> utoipa::openapi::OpenApi {
    <BridgeApiDoc as utoipa::OpenApi>::openapi()
}
