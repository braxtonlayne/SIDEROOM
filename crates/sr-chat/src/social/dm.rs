//! DM and group DM lifecycle rules: access, creation, recipients, ownership.
//! Pure functions over loaded state.

use super::relationships::{PairState, RelationshipKind, is_blocked_either_way};
use super::{SocialError, SocialLimits, SocialResult, clean_text};
use sr_core::{ChannelId, MessageId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmKind {
    Dm,
    Group,
}

/// A DM or group DM with recipients in join order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmChannel {
    pub id: ChannelId,
    pub kind: DmKind,
    pub owner: Option<UserId>,
    pub name: Option<String>,
    pub icon_key: Option<String>,
    pub recipients: Vec<UserId>,
    pub last_message_id: Option<MessageId>,
}

impl DmChannel {
    pub fn has(&self, user: UserId) -> bool {
        self.recipients.contains(&user)
    }
}

/// How long a group DM invite lives (Discord: 24 hours). Invites never run out of uses and
/// are always in-platform (G-47; D-022 does not apply).
pub const GROUP_INVITE_TTL_MS: i64 = 24 * 60 * 60 * 1000;

/// An invite link to a group DM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupInvite {
    pub code: String,
    pub channel_id: ChannelId,
    pub inviter_id: UserId,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
    pub revoked: bool,
}

impl GroupInvite {
    pub fn new(code: String, channel_id: ChannelId, inviter_id: UserId, now_ms: i64) -> Self {
        Self {
            code,
            channel_id,
            inviter_id,
            created_at_ms: now_ms,
            expires_at_ms: now_ms + GROUP_INVITE_TTL_MS,
            revoked: false,
        }
    }

    /// Neither revoked nor expired.
    pub fn usable(&self, now_ms: i64) -> bool {
        !self.revoked && now_ms < self.expires_at_ms
    }
}

/// Only the owner of a group DM makes or revokes invites for it.
pub fn plan_group_invite_manage(actor: UserId, dm: &DmChannel) -> SocialResult<()> {
    if dm.kind != DmKind::Group {
        return Err(SocialError::NotGroupDm);
    }
    if !dm.has(actor) {
        return Err(SocialError::NotParticipant);
    }
    if dm.owner != Some(actor) {
        return Err(SocialError::NotOwner);
    }
    Ok(())
}

/// What accepting a group invite does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteJoinPlan {
    /// Already in the group: nothing changes.
    AlreadyMember,
    Join,
}

/// Accepting a group invite: it must be usable and the group not full; the newcomer's own
/// group DM cap applies as for any other way of joining a group.
pub fn plan_group_invite_join(
    user: UserId,
    dm: &DmChannel,
    invite: &GroupInvite,
    now_ms: i64,
    user_group_count: usize,
    limits: &SocialLimits,
) -> SocialResult<InviteJoinPlan> {
    if dm.kind != DmKind::Group || !invite.usable(now_ms) {
        return Err(SocialError::UnknownInvite);
    }
    if dm.has(user) {
        return Ok(InviteJoinPlan::AlreadyMember);
    }
    if dm.recipients.len() >= limits.max_group_size {
        return Err(SocialError::MaxGroupSize);
    }
    if user_group_count >= limits.max_group_dms_per_user {
        return Err(SocialError::MaxGroupDms);
    }
    Ok(InviteJoinPlan::Join)
}

/// A user's DM privacy preferences (docs/inventory LNG-USET-013).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmPrivacy {
    /// Allow DMs from members of servers we share (default on).
    pub allow_server_member_dms: bool,
    /// Route DMs from non-friends to Message Requests (default on).
    pub message_requests: bool,
}

impl Default for DmPrivacy {
    fn default() -> Self {
        Self {
            allow_server_member_dms: true,
            message_requests: true,
        }
    }
}

/// A recipient's Message Request state for a conversation (open question 112).
/// `Pending` is set only by the server when a request lands; the recipient may
/// then move it to `Accepted`, `Ignored` or `Spam`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestState {
    Accepted,
    Pending,
    Ignored,
    Spam,
}

impl RequestState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Pending => "pending",
            Self::Ignored => "ignored",
            Self::Spam => "spam",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "accepted" => Self::Accepted,
            "pending" => Self::Pending,
            "ignored" => Self::Ignored,
            "spam" => Self::Spam,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmAccess {
    Allowed,
    /// Delivered to the recipient's Message Requests until accepted.
    Request,
    Denied,
}

/// Whether `sender` may DM `recipient`. `pair.mine` is the sender's row,
/// `pair.theirs` the recipient's row. Non-friends need a shared server.
pub fn dm_access(pair: PairState, recipient_privacy: &DmPrivacy, shares_guild: bool) -> DmAccess {
    if is_blocked_either_way(pair) {
        return DmAccess::Denied;
    }
    if pair.mine == Some(RelationshipKind::Friend) && pair.theirs == Some(RelationshipKind::Friend)
    {
        return DmAccess::Allowed;
    }
    if !shares_guild || !recipient_privacy.allow_server_member_dms {
        return DmAccess::Denied;
    }
    // Recipient ignoring the sender hides the conversation in requests.
    if recipient_privacy.message_requests || pair.they_ignore_me {
        DmAccess::Request
    } else {
        DmAccess::Allowed
    }
}

/// The Server Protection settings of the servers two accounts share (D-224).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SharedProtection {
    /// Number of servers both accounts are members of.
    pub shared: usize,
    /// At least one server is shared and every shared server has `dm_spam_filter` on.
    pub spam_filter_all: bool,
    /// Any shared server has `explicit_media_filter_dms` on.
    pub explicit_media_any: bool,
    /// The largest `warn_new_account_days` among the shared servers (0 = off).
    pub warn_new_account_days: u32,
    /// At least one server is shared and every shared server has its DM pause running
    /// (`dms_disabled_until` in the future, D-243). A pause blocks; it does not file into Message
    /// Requests.
    pub dms_paused_all: bool,
}

/// Why a conversation sits in Message Requests because of a server's Protection settings
/// (`sr_request_reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestReason {
    /// Every shared server filters DMs from non-friends.
    ServerFilter,
    /// The sender's account is younger than a shared server's threshold.
    NewAccount,
}

impl RequestReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ServerFilter => "server_filter",
            Self::NewAccount => "new_account",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "server_filter" => Self::ServerFilter,
            "new_account" => Self::NewAccount,
            _ => return None,
        })
    }
}

/// [`dm_access`] with the Server Protection settings of the shared servers applied (D-224).
/// They only tighten: a conversation the recipient's own settings already send to Message
/// Requests stays there, and one they allow moves there when every shared server filters
/// DMs from non-friends or the sender's account is younger than the largest shared
/// threshold. Friends are never affected. The reason is reported whenever a setting applies.
pub fn dm_access_protected(
    pair: PairState,
    recipient_privacy: &DmPrivacy,
    shares_guild: bool,
    protection: &SharedProtection,
    sender_age_days: i64,
) -> (DmAccess, Option<RequestReason>) {
    let base = dm_access(pair, recipient_privacy, shares_guild);
    let friends = pair.mine == Some(RelationshipKind::Friend)
        && pair.theirs == Some(RelationshipKind::Friend);
    if friends || base == DmAccess::Denied {
        return (base, None);
    }
    // D-243: while every shared server has paused DMs, non-friends are refused outright.
    if protection.dms_paused_all {
        return (DmAccess::Denied, None);
    }
    let reason = if protection.warn_new_account_days > 0
        && sender_age_days < i64::from(protection.warn_new_account_days)
    {
        Some(RequestReason::NewAccount)
    } else if protection.spam_filter_all {
        Some(RequestReason::ServerFilter)
    } else {
        None
    };
    match reason {
        Some(_) => (DmAccess::Request, reason),
        None => (base, None),
    }
}

/// Ordered pair for the one-DM-per-pair uniqueness key (`user_a < user_b`).
pub fn dm_pair(a: UserId, b: UserId) -> (UserId, UserId) {
    if a < b { (a, b) } else { (b, a) }
}

/// Validate a prospective group creation. `candidates` pairs each requested
/// recipient with the creator's relationship to them.
pub fn plan_group_create(
    owner: UserId,
    candidates: &[(UserId, Option<RelationshipKind>)],
    existing_group_count: usize,
    limits: &SocialLimits,
) -> SocialResult<Vec<UserId>> {
    if existing_group_count >= limits.max_group_dms_per_user {
        return Err(SocialError::MaxGroupDms);
    }
    let mut members = vec![owner];
    for (user, rel) in candidates {
        if members.contains(user) {
            continue;
        }
        if *rel != Some(RelationshipKind::Friend) {
            return Err(if *rel == Some(RelationshipKind::Blocked) {
                SocialError::BlockedByYou
            } else {
                SocialError::NotFriends
            });
        }
        members.push(*user);
    }
    if members.len() < 3 {
        return Err(SocialError::invalid(
            "recipients",
            "BASE_TYPE_MIN_LENGTH",
            "a group DM needs at least two other recipients",
        ));
    }
    if members.len() > limits.max_group_size {
        return Err(SocialError::MaxGroupSize);
    }
    Ok(members)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddPlan {
    /// Nobody new to add.
    Nothing,
    /// Add to the existing group.
    ToGroup(Vec<UserId>),
    /// Adding to a 1:1 DM spawns a new group of everyone (the DM stays).
    NewGroup { members: Vec<UserId> },
}

/// Add recipients to a DM or group DM. Any current member may add friends.
pub fn plan_add_recipients(
    actor: UserId,
    dm: &DmChannel,
    candidates: &[(UserId, Option<RelationshipKind>)],
    actor_group_count: usize,
    limits: &SocialLimits,
) -> SocialResult<AddPlan> {
    if !dm.has(actor) {
        return Err(SocialError::NotParticipant);
    }
    let mut new = Vec::new();
    for (user, rel) in candidates {
        if dm.has(*user) || new.contains(user) {
            continue;
        }
        if *rel != Some(RelationshipKind::Friend) {
            return Err(if *rel == Some(RelationshipKind::Blocked) {
                SocialError::BlockedByYou
            } else {
                SocialError::NotFriends
            });
        }
        new.push(*user);
    }
    if new.is_empty() {
        return Ok(AddPlan::Nothing);
    }
    if dm.recipients.len() + new.len() > limits.max_group_size {
        return Err(SocialError::MaxGroupSize);
    }
    match dm.kind {
        DmKind::Group => Ok(AddPlan::ToGroup(new)),
        DmKind::Dm => {
            if actor_group_count >= limits.max_group_dms_per_user {
                return Err(SocialError::MaxGroupDms);
            }
            let mut members = vec![actor];
            members.extend(dm.recipients.iter().copied().filter(|u| *u != actor));
            members.extend(new);
            Ok(AddPlan::NewGroup { members })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovePlan {
    pub removed: UserId,
    /// Set when the departing member was the owner and others remain.
    pub new_owner: Option<UserId>,
    /// The last member left: delete the channel.
    pub delete_channel: bool,
}

/// Remove a recipient (owner kicks) or leave (`target == actor`). The owner
/// leaving hands ownership to the longest-standing remaining member.
pub fn plan_remove_recipient(
    actor: UserId,
    dm: &DmChannel,
    target: UserId,
) -> SocialResult<RemovePlan> {
    if dm.kind != DmKind::Group {
        return Err(SocialError::NotGroupDm);
    }
    if !dm.has(actor) {
        return Err(SocialError::NotParticipant);
    }
    if !dm.has(target) {
        return Err(SocialError::UnknownUser);
    }
    if actor != target && dm.owner != Some(actor) {
        return Err(SocialError::NotOwner);
    }
    let remaining: Vec<UserId> = dm
        .recipients
        .iter()
        .copied()
        .filter(|u| *u != target)
        .collect();
    let new_owner = if dm.owner == Some(target) {
        remaining.first().copied()
    } else {
        None
    };
    Ok(RemovePlan {
        removed: target,
        new_owner,
        delete_channel: remaining.is_empty(),
    })
}

pub fn plan_transfer_owner(actor: UserId, dm: &DmChannel, new_owner: UserId) -> SocialResult<()> {
    if dm.kind != DmKind::Group {
        return Err(SocialError::NotGroupDm);
    }
    if dm.owner != Some(actor) {
        return Err(SocialError::NotOwner);
    }
    if !dm.has(new_owner) {
        return Err(SocialError::UnknownUser);
    }
    if new_owner == actor {
        return Err(SocialError::SelfTarget);
    }
    Ok(())
}

/// Group edit input: `None` leaves a field alone, `Some(None-ish)` clears it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupEdit {
    pub name: Option<String>,
    pub icon_key: Option<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupUpdate {
    pub name: Option<String>,
    pub icon_key: Option<String>,
}

/// Validate a group rename/icon change (any member may edit, as on Discord).
pub fn plan_group_edit(
    actor: UserId,
    dm: &DmChannel,
    edit: &GroupEdit,
    limits: &SocialLimits,
) -> SocialResult<GroupUpdate> {
    if dm.kind != DmKind::Group {
        return Err(SocialError::NotGroupDm);
    }
    if !dm.has(actor) {
        return Err(SocialError::NotParticipant);
    }
    let name = match &edit.name {
        Some(n) => clean_text("name", Some(n), limits.max_group_name_chars)?,
        None => dm.name.clone(),
    };
    let icon_key = match &edit.icon_key {
        Some(k) => k.clone(),
        None => dm.icon_key.clone(),
    };
    Ok(GroupUpdate { name, icon_key })
}
