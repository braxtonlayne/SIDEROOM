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
    if recipient_privacy.message_requests || pair.theirs == Some(RelationshipKind::Ignored) {
        DmAccess::Request
    } else {
        DmAccess::Allowed
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
