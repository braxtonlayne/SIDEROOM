//! Loot tables (UTL-DEV-001): weighted rows to roll on, per account or per server.
//!
//! A member manages their own tables. A server's tables are readable by every member and written with
//! `MANAGE_GUILD`. Rolling happens in the browser (`sr-tools-wasm`); the server only stores the tables.

use crate::routes::Ext;
use crate::routes::messages::error::{ApiJson, MsgResult, parse_id};
use crate::routes::settings::deps::SettingsDeps;
use crate::routes::userfeat::{UserFeatDeps, me, raw, row_id, uerr};
use crate::{Principal, openapi::ErrorBody};
use axum::{Json, extract::Path, http::StatusCode};
use sr_core::permissions::Permissions;
use sr_core::{ErrorCode, GuildId};
use sr_userfeat::tools::{LootOwner, LootTable, LootTableInput};

/// A server the caller belongs to, with whether they may manage it.
async fn guild_of(
    s: &SettingsDeps,
    p: &Principal,
    guild: &str,
    write: bool,
) -> MsgResult<(i64, i64)> {
    let user = me(p)?;
    let gid: GuildId = parse_id(guild, ErrorCode::UnknownGuild)?;
    let access = s.guild_access(user, gid).await?;
    if write && !access.perms.has(Permissions::MANAGE_GUILD) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    Ok((raw(user), i64::try_from(gid.get()).unwrap_or(0)))
}

// ------------------------------------------------------------------ the caller's own tables

/// The caller's own loot tables.
#[utoipa::path(
    get,
    path = "/users/@me/tools/loot-tables",
    responses((status = 200, body = Vec<LootTable>), (status = 403, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_mine(p: Principal, Ext(d): Ext<UserFeatDeps>) -> MsgResult<Json<Vec<LootTable>>> {
    let user = raw(me(&p)?);
    Ok(Json(
        d.uf.list_loot_tables(LootOwner::User(user))
            .await
            .map_err(uerr)?,
    ))
}

/// Save a loot table to the caller's account (up to 50, names unique ignoring case).
#[utoipa::path(
    post,
    path = "/users/@me/tools/loot-tables",
    request_body = LootTableInput,
    responses(
        (status = 200, body = LootTable),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 409, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create_mine(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    ApiJson(body): ApiJson<LootTableInput>,
) -> MsgResult<Json<LootTable>> {
    let user = raw(me(&p)?);
    Ok(Json(
        d.uf.create_loot_table(LootOwner::User(user), user, &body)
            .await
            .map_err(uerr)?,
    ))
}

/// Replace one of the caller's loot tables.
#[utoipa::path(
    patch,
    path = "/users/@me/tools/loot-tables/{table_id}",
    params(("table_id" = String, Path, description = "Loot table snowflake")),
    request_body = LootTableInput,
    responses(
        (status = 200, body = LootTable),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn update_mine(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Path(table): Path<String>,
    ApiJson(body): ApiJson<LootTableInput>,
) -> MsgResult<Json<LootTable>> {
    let user = raw(me(&p)?);
    let id = row_id(&table, "loot table")?;
    Ok(Json(
        d.uf.update_loot_table(LootOwner::User(user), id, &body)
            .await
            .map_err(uerr)?,
    ))
}

/// Delete one of the caller's loot tables.
#[utoipa::path(
    delete,
    path = "/users/@me/tools/loot-tables/{table_id}",
    params(("table_id" = String, Path, description = "Loot table snowflake")),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_mine(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Path(table): Path<String>,
) -> MsgResult<StatusCode> {
    let user = raw(me(&p)?);
    let id = row_id(&table, "loot table")?;
    d.uf.delete_loot_table(LootOwner::User(user), id)
        .await
        .map_err(uerr)?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------------ a server's tables

/// A server's loot tables. Members only.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/tools/loot-tables",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<LootTable>), (status = 403, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_guild(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path(guild): Path<String>,
) -> MsgResult<Json<Vec<LootTable>>> {
    let (_, gid) = guild_of(&s, &p, &guild, false).await?;
    Ok(Json(
        d.uf.list_loot_tables(LootOwner::Guild(gid))
            .await
            .map_err(uerr)?,
    ))
}

/// Save a loot table for the server (`MANAGE_GUILD`; up to 50).
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/tools/loot-tables",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = LootTableInput,
    responses(
        (status = 200, body = LootTable),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 409, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create_guild(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path(guild): Path<String>,
    ApiJson(body): ApiJson<LootTableInput>,
) -> MsgResult<Json<LootTable>> {
    let (user, gid) = guild_of(&s, &p, &guild, true).await?;
    Ok(Json(
        d.uf.create_loot_table(LootOwner::Guild(gid), user, &body)
            .await
            .map_err(uerr)?,
    ))
}

/// Replace a server's loot table (`MANAGE_GUILD`).
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/tools/loot-tables/{table_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("table_id" = String, Path, description = "Loot table snowflake"),
    ),
    request_body = LootTableInput,
    responses(
        (status = 200, body = LootTable),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn update_guild(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path((guild, table)): Path<(String, String)>,
    ApiJson(body): ApiJson<LootTableInput>,
) -> MsgResult<Json<LootTable>> {
    let (_, gid) = guild_of(&s, &p, &guild, true).await?;
    let id = row_id(&table, "loot table")?;
    Ok(Json(
        d.uf.update_loot_table(LootOwner::Guild(gid), id, &body)
            .await
            .map_err(uerr)?,
    ))
}

/// Delete a server's loot table (`MANAGE_GUILD`).
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/tools/loot-tables/{table_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("table_id" = String, Path, description = "Loot table snowflake"),
    ),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_guild(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path((guild, table)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    let (_, gid) = guild_of(&s, &p, &guild, true).await?;
    let id = row_id(&table, "loot table")?;
    d.uf.delete_loot_table(LootOwner::Guild(gid), id)
        .await
        .map_err(uerr)?;
    Ok(StatusCode::NO_CONTENT)
}
