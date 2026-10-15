//! Birthdays (FTR-BDAY-001), leveling (FTR-LVL-001), RSS feeds (FTR-RSS-001), poll templates (FTR-POLLT-001) and
//! suggestions (FTR-SUGG-001).

use super::{UserFeatDeps, body_id, me, raw, row_id, uerr};
use crate::routes::Ext;
use crate::routes::messages::MessagesDeps;
use crate::routes::messages::error::{ApiJson, ApiQuery, MsgResult, invalid, parse_id};
use crate::routes::settings::deps::{GuildAccess, SettingsDeps};
use crate::{Principal, openapi::ErrorBody};
use axum::{Json, extract::Path, http::StatusCode};
use serde::Deserialize;
use sr_chat::ChannelType;
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, ErrorCode, GuildId, UserId};
use sr_userfeat::birthdays::{Birthday, BirthdayInput, UpcomingBirthday};
use sr_userfeat::features::{LEVELING_ID, SUGGESTIONS_ID};
use sr_userfeat::leveling::{LeaderboardEntry, RankCard};
use sr_userfeat::poll_templates::{PollTemplate, PollTemplateInput};
use sr_userfeat::rss::{RssFeed, RssFeedInput, RssFeedPatch, check_feed_url};
use sr_userfeat::suggestions::{Suggestion, SuggestionInput, SuggestionResponse};
use utoipa::ToSchema;

// ------------------------------------------------------------------ shared

/// The caller as a member of the server.
async fn member(
    s: &SettingsDeps,
    p: &Principal,
    guild: &str,
) -> MsgResult<(UserId, i64, GuildAccess)> {
    let user = me(p)?;
    let gid: GuildId = parse_id(guild, ErrorCode::UnknownGuild)?;
    let access = s.guild_access(user, gid).await?;
    Ok((user, i64::try_from(gid.get()).unwrap_or(0), access))
}

/// The caller as a member who may manage the server.
async fn manager(
    s: &SettingsDeps,
    p: &Principal,
    guild: &str,
) -> MsgResult<(UserId, i64, GuildAccess)> {
    let (user, gid, access) = member(s, p, guild).await?;
    if !access.perms.has(Permissions::MANAGE_GUILD) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    Ok((user, gid, access))
}

// ------------------------------------------------------------------ birthdays

/// The caller's birthday.
#[utoipa::path(
    get,
    path = "/users/@me/birthday",
    responses((status = 200, body = Birthday), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_birthday(p: Principal, Ext(d): Ext<UserFeatDeps>) -> MsgResult<Json<Birthday>> {
    let user = me(&p)?;
    Ok(Json(d.uf.get_birthday(raw(user)).await.map_err(uerr)?))
}

/// Set the caller's birthday. `visibility: hidden` keeps it private (nothing is announced).
#[utoipa::path(
    put,
    path = "/users/@me/birthday",
    request_body = BirthdayInput,
    responses((status = 200, body = Birthday), (status = 400, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn set_birthday(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    ApiJson(body): ApiJson<BirthdayInput>,
) -> MsgResult<Json<Birthday>> {
    let user = me(&p)?;
    Ok(Json(
        d.uf.set_birthday(raw(user), &body).await.map_err(uerr)?,
    ))
}

/// Forget the caller's birthday.
#[utoipa::path(
    delete,
    path = "/users/@me/birthday",
    responses((status = 204, description = "Removed")),
    security(("authorization" = []), ("session" = []))
)]
pub async fn clear_birthday(p: Principal, Ext(d): Ext<UserFeatDeps>) -> MsgResult<StatusCode> {
    let user = me(&p)?;
    d.uf.clear_birthday(raw(user)).await.map_err(uerr)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Default, Deserialize)]
pub struct LimitQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// The next birthdays of members who share theirs (months and days, never the year). Members only.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/birthdays",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("limit" = Option<i64>, Query, description = "1 to 100, default 25"),
    ),
    responses((status = 200, body = Vec<UpcomingBirthday>), (status = 403, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn birthdays(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path(guild): Path<String>,
    ApiQuery(q): ApiQuery<LimitQuery>,
) -> MsgResult<Json<Vec<UpcomingBirthday>>> {
    let (_, gid, _) = member(&s, &p, &guild).await?;
    Ok(Json(
        d.uf.upcoming_birthdays(gid, q.limit.unwrap_or(25))
            .await
            .map_err(uerr)?,
    ))
}

// ------------------------------------------------------------------ leveling

/// A member's rank card: XP, level, progress and position. `user_id` may be `@me`.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/leveling/rank/{user_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("user_id" = String, Path, description = "User snowflake or `@me`"),
    ),
    responses((status = 200, body = RankCard), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn rank(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path((guild, who)): Path<(String, String)>,
) -> MsgResult<Json<RankCard>> {
    let (user, gid, _) = member(&s, &p, &guild).await?;
    let target = if who == "@me" {
        raw(user)
    } else {
        row_id(&who, "member")?
    };
    Ok(Json(d.uf.rank_card(gid, target).await.map_err(uerr)?))
}

/// The server's leaderboard, highest XP first.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/leveling/leaderboard",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("limit" = Option<i64>, Query, description = "1 to 100, default 25"),
        ("offset" = Option<i64>, Query, description = "Rows to skip"),
    ),
    responses((status = 200, body = Vec<LeaderboardEntry>), (status = 403, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn leaderboard(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path(guild): Path<String>,
    ApiQuery(q): ApiQuery<LimitQuery>,
) -> MsgResult<Json<Vec<LeaderboardEntry>>> {
    let (_, gid, _) = member(&s, &p, &guild).await?;
    Ok(Json(
        d.uf.leaderboard(gid, q.limit.unwrap_or(25), q.offset.unwrap_or(0))
            .await
            .map_err(uerr)?,
    ))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LevelingOptBody {
    /// `false` opts the caller out of XP in this server and deletes their XP.
    pub enabled: bool,
}

/// Opt in or out of leveling in a server.
#[utoipa::path(
    put,
    path = "/users/@me/guilds/{guild_id}/leveling",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = LevelingOptBody,
    responses((status = 204, description = "Saved"), (status = 403, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn set_leveling(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path(guild): Path<String>,
    ApiJson(body): ApiJson<LevelingOptBody>,
) -> MsgResult<StatusCode> {
    let (user, gid, _) = member(&s, &p, &guild).await?;
    d.uf.require(LEVELING_ID, Some(gid)).await.map_err(uerr)?;
    d.uf.set_xp_opt_out(gid, raw(user), !body.enabled)
        .await
        .map_err(uerr)?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------------ RSS feeds

/// The server's RSS and Atom feeds (`MANAGE_GUILD`).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/rss-feeds",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<RssFeed>), (status = 403, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_feeds(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path(guild): Path<String>,
) -> MsgResult<Json<Vec<RssFeed>>> {
    let (_, gid, _) = manager(&s, &p, &guild).await?;
    Ok(Json(d.uf.list_feeds(gid).await.map_err(uerr)?))
}

/// A text or announcement channel of the server, as a posting target.
async fn feed_channel(
    s: &SettingsDeps,
    user: UserId,
    guild: i64,
    raw_channel: &str,
) -> MsgResult<i64> {
    let id = body_id("channel_id", raw_channel)?;
    let access = s.channel_access(user, ChannelId::new(id as u64)).await?;
    let in_guild = access
        .channel
        .guild_id
        .is_some_and(|g| i64::try_from(g.get()).ok() == Some(guild));
    let textual = matches!(
        access.channel.kind,
        ChannelType::Text | ChannelType::Announcement
    );
    if !in_guild || !textual {
        return Err(invalid(
            "channel_id",
            "CHANNEL_TYPE_INVALID",
            "Pick a text channel of this server.",
        ));
    }
    Ok(id)
}

/// Follow a feed into a channel (`MANAGE_GUILD`). The address is checked against the SSRF policy; the first
/// fetch only records what is already in the feed.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/rss-feeds",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = RssFeedInput,
    responses(
        (status = 200, body = RssFeed),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 409, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn add_feed(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path(guild): Path<String>,
    ApiJson(body): ApiJson<RssFeedInput>,
) -> MsgResult<Json<RssFeed>> {
    let (user, gid, _) = manager(&s, &p, &guild).await?;
    d.uf.require(sr_userfeat::features::RSS_ID, Some(gid))
        .await
        .map_err(uerr)?;
    let channel = feed_channel(&s, user, gid, &body.channel_id).await?;
    let url = check_feed_url(&d.net_policy, &body.url).map_err(uerr)?;
    Ok(Json(
        d.uf.add_feed(gid, channel, raw(user), &url, &body)
            .await
            .map_err(uerr)?,
    ))
}

/// Change a feed's channel, filters, interval or on/off state (`MANAGE_GUILD`).
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/rss-feeds/{feed_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("feed_id" = String, Path, description = "Feed snowflake"),
    ),
    request_body = RssFeedPatch,
    responses((status = 200, body = RssFeed), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn patch_feed(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path((guild, feed)): Path<(String, String)>,
    ApiJson(body): ApiJson<RssFeedPatch>,
) -> MsgResult<Json<RssFeed>> {
    let (user, gid, _) = manager(&s, &p, &guild).await?;
    let id = row_id(&feed, "feed")?;
    let channel = match &body.channel_id {
        Some(c) => Some(feed_channel(&s, user, gid, c).await?),
        None => None,
    };
    Ok(Json(
        d.uf.patch_feed(gid, id, channel, &body)
            .await
            .map_err(uerr)?,
    ))
}

/// Stop following a feed (`MANAGE_GUILD`).
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/rss-feeds/{feed_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("feed_id" = String, Path, description = "Feed snowflake"),
    ),
    responses((status = 204, description = "Removed"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_feed(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path((guild, feed)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    let (_, gid, _) = manager(&s, &p, &guild).await?;
    d.uf.delete_feed(gid, row_id(&feed, "feed")?)
        .await
        .map_err(uerr)?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------------ poll templates

/// The server's poll templates. Members only.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/poll-templates",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<PollTemplate>), (status = 403, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_poll_templates(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path(guild): Path<String>,
) -> MsgResult<Json<Vec<PollTemplate>>> {
    let (_, gid, _) = member(&s, &p, &guild).await?;
    Ok(Json(d.uf.list_poll_templates(gid).await.map_err(uerr)?))
}

/// Save a poll setup (`MANAGE_GUILD`).
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/poll-templates",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = PollTemplateInput,
    responses((status = 200, body = PollTemplate), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create_poll_template(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path(guild): Path<String>,
    ApiJson(body): ApiJson<PollTemplateInput>,
) -> MsgResult<Json<PollTemplate>> {
    let (user, gid, _) = manager(&s, &p, &guild).await?;
    Ok(Json(
        d.uf.create_poll_template(gid, raw(user), &body)
            .await
            .map_err(uerr)?,
    ))
}

/// Replace a template (`MANAGE_GUILD`).
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/poll-templates/{template_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("template_id" = String, Path, description = "Template snowflake"),
    ),
    request_body = PollTemplateInput,
    responses((status = 200, body = PollTemplate), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn update_poll_template(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path((guild, template)): Path<(String, String)>,
    ApiJson(body): ApiJson<PollTemplateInput>,
) -> MsgResult<Json<PollTemplate>> {
    let (_, gid, _) = manager(&s, &p, &guild).await?;
    Ok(Json(
        d.uf.update_poll_template(gid, row_id(&template, "poll template")?, &body)
            .await
            .map_err(uerr)?,
    ))
}

/// Delete a template (`MANAGE_GUILD`).
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/poll-templates/{template_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("template_id" = String, Path, description = "Template snowflake"),
    ),
    responses((status = 204, description = "Deleted"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_poll_template(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path((guild, template)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    let (_, gid, _) = manager(&s, &p, &guild).await?;
    d.uf.delete_poll_template(gid, row_id(&template, "poll template")?)
        .await
        .map_err(uerr)?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------------ suggestions

#[derive(Debug, Default, Deserialize)]
pub struct SuggestionQuery {
    pub status: Option<String>,
    pub before: Option<String>,
    pub limit: Option<i64>,
}

/// Whether the caller may answer suggestions: they manage the server or hold its staff role.
async fn can_respond(d: &UserFeatDeps, guild: i64, access: &GuildAccess) -> MsgResult<bool> {
    if access.perms.has(Permissions::MANAGE_GUILD) {
        return Ok(true);
    }
    let cfg =
        d.uf.settings(SUGGESTIONS_ID, Some(guild))
            .await
            .map_err(uerr)?;
    Ok(cfg.id("staff_role").is_some_and(|r| {
        access
            .member
            .role_ids
            .iter()
            .any(|m| i64::try_from(m.get()).ok() == Some(r))
    }))
}

/// Post a suggestion to the server's suggestions channel.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/suggestions",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = SuggestionInput,
    responses((status = 200, body = Suggestion), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn submit_suggestion(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Ext(m): Ext<MessagesDeps>,
    Path(guild): Path<String>,
    ApiJson(body): ApiJson<SuggestionInput>,
) -> MsgResult<Json<Suggestion>> {
    let (user, gid, _) = member(&s, &p, &guild).await?;
    d.uf.require(SUGGESTIONS_ID, Some(gid))
        .await
        .map_err(uerr)?;
    // The member must be able to see the channel their suggestion lands in.
    if let Some(ch) =
        d.uf.settings(SUGGESTIONS_ID, Some(gid))
            .await
            .map_err(uerr)?
            .id("channel")
    {
        m.access(user, ChannelId::new(ch as u64))
            .await?
            .require_view()?;
    }
    Ok(Json(
        d.uf.submit_suggestion(&*d.poster, gid, raw(user), &body.content)
            .await
            .map_err(uerr)?,
    ))
}

/// The server's suggestions, newest first. Members only.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/suggestions",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("status" = Option<String>, Query, description = "open, considering, approved, denied or implemented"),
        ("before" = Option<String>, Query, description = "Suggestion id to page from"),
        ("limit" = Option<i64>, Query, description = "1 to 100, default 50"),
    ),
    responses((status = 200, body = Vec<Suggestion>), (status = 403, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_suggestions(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path(guild): Path<String>,
    ApiQuery(q): ApiQuery<SuggestionQuery>,
) -> MsgResult<Json<Vec<Suggestion>>> {
    let (_, gid, _) = member(&s, &p, &guild).await?;
    let before = q
        .before
        .as_deref()
        .map(|b| body_id("before", b))
        .transpose()?;
    Ok(Json(
        d.uf.list_suggestions(gid, q.status.as_deref(), before, q.limit.unwrap_or(50))
            .await
            .map_err(uerr)?,
    ))
}

/// Set a suggestion's status and staff response (`MANAGE_GUILD` or the staff role). The card is edited to match.
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/suggestions/{suggestion_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("suggestion_id" = String, Path, description = "Suggestion snowflake"),
    ),
    request_body = SuggestionResponse,
    responses((status = 200, body = Suggestion), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn respond_suggestion(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path((guild, suggestion)): Path<(String, String)>,
    ApiJson(body): ApiJson<SuggestionResponse>,
) -> MsgResult<Json<Suggestion>> {
    let (user, gid, access) = member(&s, &p, &guild).await?;
    if !can_respond(&d, gid, &access).await? {
        return Err(ErrorCode::MissingPermissions.into());
    }
    Ok(Json(
        d.uf.respond_to_suggestion(
            &*d.poster,
            gid,
            row_id(&suggestion, "suggestion")?,
            raw(user),
            &body,
        )
        .await
        .map_err(uerr)?,
    ))
}

/// Delete a suggestion (its author, or someone who may answer them). The card stays in the channel; moderators
/// delete it there.
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/suggestions/{suggestion_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("suggestion_id" = String, Path, description = "Suggestion snowflake"),
    ),
    responses((status = 204, description = "Deleted"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_suggestion(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path((guild, suggestion)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    let (user, gid, access) = member(&s, &p, &guild).await?;
    let id = row_id(&suggestion, "suggestion")?;
    let row = d.uf.get_suggestion(gid, id).await.map_err(uerr)?;
    let own = row.author_id == raw(user).to_string();
    if !own && !can_respond(&d, gid, &access).await? {
        return Err(ErrorCode::MissingPermissions.into());
    }
    d.uf.delete_suggestion(gid, id).await.map_err(uerr)?;
    Ok(StatusCode::NO_CONTENT)
}
