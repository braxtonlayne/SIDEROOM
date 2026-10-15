//! User profiles and the caller's own profile edit (LNG-PRF-*, Q118).

use super::deps::SocialDeps;
use super::error::invalid;
use super::error::{ApiJson, MsgResult};
use super::events::SocialEvent;
use super::model::{
    ConnectionObject, CustomStatusObject, EditSelfBody, GuildMemberProfileObject,
    MutualGuildObject, ProfileObject, SelfObject, UserObject, UserProfileFields,
};
use super::util::{actor, custom_status, user_id};
use crate::routes::MaybeExt;
use crate::routes::media::MediaDeps;
use crate::routes::messages::error::MsgError;
use crate::routes::messages::model::iso;
use crate::{Principal, openapi::ErrorBody, routes::Ext};
use axum::{
    Json,
    extract::{Path, Query},
};
use serde::Deserialize;
use serde_json::Value;
use sr_chat::social::profile::ProfileEdit;
use sr_core::ErrorCode;
use sr_core::UserId;
use sr_storage::media::MediaKind;

/// A user's profile: bio, pronouns, banner, accent colour and the connections they
/// made visible.
#[utoipa::path(
    get,
    path = "/users/{user_id}/profile",
    params(
        ("user_id" = String, Path, description = "User snowflake"),
        ("with_mutual_guilds" = Option<bool>, Query, description = "Include mutual guilds (default true)"),
        ("guild_id" = Option<String>, Query, description = "Also return the user's profile in this server (viewer must be a member)")
    ),
    responses((status = 200, body = ProfileObject), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_profile(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    Path(target): Path<String>,
    Query(q): Query<ProfileQuery>,
) -> MsgResult<Json<ProfileObject>> {
    let me = actor(&p)?;
    let guild = q
        .guild_id
        .as_deref()
        .map(|g| g.parse::<u64>().map(sr_core::GuildId::new))
        .transpose()
        .map_err(|_| {
            invalid(
                "guild_id",
                "SNOWFLAKE_TYPE_INVALID",
                "Not a valid snowflake.",
            )
        })?;
    render_profile(
        &deps,
        me,
        user_id(&target)?,
        q.with_mutual_guilds.unwrap_or(true),
        guild,
    )
    .await
}

/// Query of `GET /users/{id}/profile`.
#[derive(Debug, Default, Deserialize)]
pub struct ProfileQuery {
    pub with_mutual_guilds: Option<bool>,
    pub guild_id: Option<String>,
}

/// The caller's own profile.
#[utoipa::path(
    get,
    path = "/users/@me/profile",
    responses((status = 200, body = ProfileObject)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_own_profile(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
) -> MsgResult<Json<ProfileObject>> {
    let me = actor(&p)?;
    render_profile(&deps, me, me, false, None).await
}

async fn render_profile(
    deps: &SocialDeps,
    viewer: UserId,
    user: UserId,
    with_mutual_guilds: bool,
    guild: Option<sr_core::GuildId>,
) -> MsgResult<Json<ProfileObject>> {
    let profile = deps.service.get_profile(viewer, user).await?;
    let mut guild_member_profile = None;
    let (mutual_friends, mutual_friends_count, mutual_guilds, friends_since) = if viewer == user {
        if let Some(g) = guild {
            // Own profile in a server: the viewer's own membership.
            guild_member_profile = deps
                .service
                .own_guild_memberships(viewer)
                .await?
                .into_iter()
                .find(|m| m.guild == g)
                .map(|m| guild_profile_object(&m));
        }
        (None, None, None, None)
    } else {
        let extras = deps.service.profile_extras(viewer, user).await?;
        let users = deps
            .summaries(extras.mutual_friends.iter().copied())
            .await?;
        let friends: Vec<UserObject> = extras
            .mutual_friends
            .iter()
            .map(|f| UserObject::new(*f, &users))
            .collect();
        let count = friends.len();
        // Only a server both share exposes the member's server profile.
        if let Some(g) = guild {
            guild_member_profile = extras
                .mutual_guilds
                .iter()
                .find(|m| m.guild == g)
                .map(guild_profile_object);
        }
        let guilds = with_mutual_guilds.then(|| {
            extras
                .mutual_guilds
                .iter()
                .map(|g| MutualGuildObject {
                    id: g.guild.to_string(),
                    nick: g.nick.clone(),
                    member_since: iso(g.joined_at_ms),
                })
                .collect::<Vec<_>>()
        });
        (
            Some(friends),
            Some(count),
            guilds,
            extras.friends_since_ms.map(iso),
        )
    };
    Ok(Json(ProfileObject {
        badges: profile.badges.clone(),
        member_since: iso(profile.created_ms),
        guild_member_profile,
        mutual_friends,
        mutual_friends_count,
        mutual_guilds,
        friends_since,
        user: UserObject {
            avatar: profile.avatar_key.clone(),
            banner: profile.banner_key.clone(),
            ..deps.user_object(user).await?
        },
        user_profile: UserProfileFields::from(&profile),
        connected_accounts: profile
            .connections
            .iter()
            .map(ConnectionObject::from)
            .collect(),
    }))
}

fn guild_profile_object(m: &sr_chat::social::repo::GuildMembership) -> GuildMemberProfileObject {
    GuildMemberProfileObject {
        guild_id: m.guild.to_string(),
        nick: m.nick.clone(),
        avatar: m.avatar_key.clone(),
        banner: m.banner_key.clone(),
        bio: m.bio.clone(),
        joined_at: iso(m.joined_at_ms),
    }
}

/// Edit the caller's profile: `avatar` and `banner` (image data URIs, stored by the
/// media pipeline), `bio` (190), `pronouns` (40), `accent_color` and
/// `custom_status`. Absent fields stay, `null` clears (and deletes the stored
/// image). Publishes `USER_UPDATE`.
#[utoipa::path(
    patch,
    path = "/users/@me",
    request_body = EditSelfBody,
    responses((status = 200, body = SelfObject), (status = 400, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn edit_self(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    MaybeExt(media): MaybeExt<MediaDeps>,
    ApiJson(body): ApiJson<EditSelfBody>,
) -> MsgResult<Json<SelfObject>> {
    let me = actor(&p)?;
    // Convert before writing so a bad custom status cannot leave a half-applied edit.
    let custom = match &body.custom_status {
        Some(Some(c)) => Some(custom_status(c)?),
        Some(None) => Some(Default::default()),
        None => None,
    };

    // Images first: a bad upload fails the whole edit before anything is written.
    let wants_images = body.avatar.is_some() || body.banner.is_some();
    let media = match (wants_images, media) {
        (true, None) => {
            tracing::error!("profile image edit needs MediaDeps registered on AppState");
            return Err(ErrorCode::SrInternal.into());
        }
        (_, m) => m,
    };
    let before = if wants_images {
        Some(deps.service.get_profile(me, me).await?)
    } else {
        None
    };
    let mut stored: Vec<(MediaKind, String)> = Vec::new();
    let resolve = |kind: MediaKind, field: &'static str, v: Option<Option<String>>| {
        let media = media.clone();
        async move {
            Ok::<_, MsgError>(match (v, media) {
                (None, _) => None,
                (Some(None), _) => Some((None, None)),
                (Some(Some(uri)), Some(m)) => {
                    let h = m.store(kind, me.get(), field, uri).await?;
                    Some((Some(h.clone()), Some((kind, h))))
                }
                (Some(Some(_)), None) => None,
            })
        }
    };
    let avatar = resolve(MediaKind::UserAvatar, "avatar", body.avatar).await;
    let banner = match avatar {
        Ok(_) => resolve(MediaKind::UserBanner, "banner", body.banner).await,
        Err(_) => Ok(None),
    };
    let (avatar_key, banner_key) = match (avatar, banner) {
        (Ok(a), Ok(b)) => {
            let mut keys = (None, None);
            if let Some((k, s)) = a {
                keys.0 = Some(k);
                stored.extend(s);
            }
            if let Some((k, s)) = b {
                keys.1 = Some(k);
                stored.extend(s);
            }
            keys
        }
        (a, b) => {
            // One of the two failed: drop whatever the other one stored.
            if let (Some(m), Ok(Some((_, Some((kind, h)))))) = (&media, &a) {
                m.remove(*kind, me.get(), h).await;
            }
            if let (Some(m), Ok(Some((_, Some((kind, h)))))) = (&media, &b) {
                m.remove(*kind, me.get(), h).await;
            }
            return Err(a
                .err()
                .or(b.err())
                .unwrap_or_else(|| ErrorCode::SrInternal.into()));
        }
    };

    let edit = ProfileEdit {
        avatar_key,
        banner_key,
        accent_color: body.accent_color,
        bio: body.bio,
        pronouns: body.pronouns,
        display_name: body.global_name,
        avatar_decoration: body.avatar_decoration,
        profile_effect: body.profile_effect,
        theme_colors: body.theme_colors,
    };
    let profile = match deps.service.edit_profile(me, &edit).await {
        Ok(p) => p,
        Err(e) => {
            if let Some(m) = &media {
                for (kind, h) in &stored {
                    m.remove(*kind, me.get(), h).await;
                }
            }
            return Err(e.into());
        }
    };
    // The edit is committed: delete the images it replaced or cleared.
    if let (Some(m), Some(before)) = (&media, &before) {
        for (kind, old, new) in [
            (
                MediaKind::UserAvatar,
                &before.avatar_key,
                &profile.avatar_key,
            ),
            (
                MediaKind::UserBanner,
                &before.banner_key,
                &profile.banner_key,
            ),
        ] {
            if let Some(old) = old
                && Some(old) != new.as_ref()
            {
                m.remove(kind, me.get(), old).await;
            }
        }
    }

    let prefs = match custom {
        Some(c) => deps.service.set_custom_status(me, c).await?,
        None => deps.service.presence(me).await?,
    };
    let out = self_object(&deps, me, &profile, &prefs).await?;
    publish_self_update(&deps, me, &out).await;
    if body.custom_status.is_some() {
        deps.announce_presence(&prefs).await;
    }
    Ok(Json(out))
}

/// The caller's `SelfObject` from already loaded profile and presence preferences.
async fn self_object(
    deps: &SocialDeps,
    me: UserId,
    profile: &sr_chat::social::profile::Profile,
    prefs: &sr_chat::social::presence::PresencePrefs,
) -> MsgResult<SelfObject> {
    Ok(SelfObject {
        user: UserObject {
            avatar: profile.avatar_key.clone(),
            banner: profile.banner_key.clone(),
            ..deps.user_object(me).await?
        },
        bio: profile.bio.clone(),
        pronouns: profile.pronouns.clone(),
        banner: profile.banner_key.clone(),
        accent_color: profile.accent_color,
        avatar_decoration: profile.avatar_decoration.clone(),
        profile_effect: profile.profile_effect.clone(),
        theme_colors: profile.theme_colors,
        custom_status: CustomStatusObject::new(&prefs.custom),
    })
}

/// `USER_UPDATE` to the user's own sessions, then to friends and shared servers
/// (the private note and relationship data are never part of this payload).
async fn publish_self_update(deps: &SocialDeps, me: UserId, out: &SelfObject) {
    let payload = serde_json::to_value(out).unwrap_or(Value::Null);
    deps.emit(SocialEvent::new("USER_UPDATE", vec![me], payload.clone()))
        .await;
    deps.fan_out_user_update(me, payload).await;
}

/// Re-read the caller's profile and publish `USER_UPDATE` (used after an account
/// change such as a new username). Best effort: the change is already stored.
pub async fn broadcast_self_update(deps: &SocialDeps, user_id: i64) {
    let Ok(raw) = u64::try_from(user_id) else {
        return;
    };
    let me = UserId::new(raw);
    let result = async {
        let profile = deps.service.get_profile(me, me).await?;
        let prefs = deps.service.presence(me).await?;
        let out = self_object(deps, me, &profile, &prefs).await?;
        publish_self_update(deps, me, &out).await;
        Ok::<_, MsgError>(())
    }
    .await;
    if result.is_err() {
        tracing::warn!("could not broadcast USER_UPDATE after an account change");
    }
}
