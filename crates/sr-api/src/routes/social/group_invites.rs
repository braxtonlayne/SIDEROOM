//! Invite links to group DMs (G-47, P11-SHL-2). The routes are the guild invite routes
//! (`POST|GET /channels/{id}/invites`, `GET|POST|DELETE /invites/{code}`); when the channel
//! or the code belongs to a group DM they are answered here.
//!
//! Group invites are owner-made, expire after 24 hours, have unlimited uses and are always
//! in-platform (D-022 does not apply). Accepting adds the recipient: the other members get
//! `CHANNEL_RECIPIENT_ADD` and a `CHANNEL_UPDATE`, the newcomer a `CHANNEL_CREATE`.

use super::deps::SocialDeps;
use super::dms::announce;
use super::events::SocialEvent;
use crate::HttpError;
use crate::error::ApiResult;
use crate::routes::guilds::dto::{UserRef, iso8601};
use crate::routes::settings::invites::{InviteChannel, InviteObject};
use serde_json::json;
use sr_chat::social::SocialError;
use sr_chat::social::dm::{DmChannel, GroupInvite};
use sr_chat::social::repo::DmRepository;
use sr_core::{ChannelId, UserId};

fn se(e: SocialError) -> HttpError {
    HttpError(e.to_api_error())
}

fn re(e: sr_chat::RepoError) -> HttpError {
    tracing::error!(error = %e, "group invite repository failure");
    HttpError(sr_core::ApiError::new(sr_core::ErrorCode::SrInternal))
}

async fn object(deps: &SocialDeps, inv: &GroupInvite, dm: &DmChannel) -> ApiResult<InviteObject> {
    let users = deps
        .summaries([inv.inviter_id])
        .await
        .map_err(|_| HttpError(sr_core::ApiError::new(sr_core::ErrorCode::SrInternal)))?;
    Ok(InviteObject {
        kind: 1,
        code: inv.code.clone(),
        guild: None,
        channel: InviteChannel {
            id: dm.id.to_string(),
            name: dm.name.clone(),
            kind: 3,
            icon: dm.icon_key.clone(),
        },
        inviter: UserRef::of(inv.inviter_id, &users),
        uses: 0,
        max_uses: 0,
        max_age: u32::try_from((inv.expires_at_ms - inv.created_at_ms) / 1000).unwrap_or(0),
        temporary: false,
        created_at: iso8601(inv.created_at_ms),
        expires_at: Some(iso8601(inv.expires_at_ms)),
        approximate_member_count: Some(dm.recipients.len() as u64),
        approximate_presence_count: None,
    })
}

/// `Some` when `channel` is a group DM: the owner's new invite. `None` when it is no DM
/// channel (the caller continues with the guild-channel path).
pub(crate) async fn create(
    deps: &SocialDeps,
    user: UserId,
    channel: ChannelId,
    code: impl FnOnce() -> String,
) -> ApiResult<Option<InviteObject>> {
    let Some(dm) = deps.service.find_group(user, channel).await.map_err(se)? else {
        return Ok(None);
    };
    let inv = deps
        .service
        .create_group_invite(user, dm.id, code())
        .await
        .map_err(se)?;
    Ok(Some(object(deps, &inv, &dm).await?))
}

/// Usable invites of a group DM (owner only); `None` when `channel` is no DM channel.
pub(crate) async fn list(
    deps: &SocialDeps,
    user: UserId,
    channel: ChannelId,
) -> ApiResult<Option<Vec<InviteObject>>> {
    let Some(dm) = deps.service.find_group(user, channel).await.map_err(se)? else {
        return Ok(None);
    };
    let invites = deps
        .service
        .list_group_invites(user, dm.id)
        .await
        .map_err(se)?;
    let mut out = Vec::new();
    for inv in &invites {
        out.push(object(deps, inv, &dm).await?);
    }
    Ok(Some(out))
}

/// `Some` when `code` is a usable group invite.
pub(crate) async fn resolve(deps: &SocialDeps, code: &str) -> ApiResult<Option<InviteObject>> {
    match deps.service.group_invite(code).await {
        Ok((inv, dm)) => Ok(Some(object(deps, &inv, &dm).await?)),
        Err(SocialError::UnknownInvite) => Ok(None),
        Err(e) => Err(se(e)),
    }
}

/// Join the group behind `code`; `None` when `code` is no group invite.
pub(crate) async fn accept(
    deps: &SocialDeps,
    user: UserId,
    code: &str,
) -> ApiResult<Option<InviteObject>> {
    let (inv, _) = match deps.service.group_invite(code).await {
        Ok(v) => v,
        Err(SocialError::UnknownInvite) => return Ok(None),
        Err(e) => return Err(se(e)),
    };
    let (dm, joined) = deps
        .service
        .join_group_invite(user, code)
        .await
        .map_err(se)?;
    if joined {
        let existing: Vec<UserId> = dm
            .recipients
            .iter()
            .copied()
            .filter(|r| *r != user)
            .collect();
        let who = deps
            .user_object(user)
            .await
            .map_err(|_| HttpError(sr_core::ApiError::new(sr_core::ErrorCode::SrInternal)))?;
        deps.emit(
            SocialEvent::new(
                "CHANNEL_RECIPIENT_ADD",
                existing.clone(),
                json!({ "channel_id": dm.id.to_string(), "user": who }),
            )
            .for_channel(dm.id),
        )
        .await;
        let _ = announce(deps, "CHANNEL_UPDATE", &dm, &existing).await;
        let _ = announce(deps, "CHANNEL_CREATE", &dm, &[user]).await;
    }
    Ok(Some(object(deps, &inv, &dm).await?))
}

/// Revoke `code` (group owner); `None` when `code` is no group invite.
pub(crate) async fn revoke(
    deps: &SocialDeps,
    user: UserId,
    code: &str,
) -> ApiResult<Option<InviteObject>> {
    let Some(inv) = deps
        .service
        .repo()
        .get_group_invite(code)
        .await
        .map_err(re)?
    else {
        return Ok(None);
    };
    let dm = deps
        .service
        .revoke_group_invite(user, code)
        .await
        .map_err(se)?;
    Ok(Some(object(deps, &inv, &dm).await?))
}
