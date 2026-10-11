//! Friend requests, friends, blocks, ignores and friend nicknames.
//!
//! Each user owns one row per target: `(user, target, kind)`. A friendship is two
//! `Friend` rows; a pending request is `PendingOutgoing` on the sender and
//! `PendingIncoming` on the receiver. Planning functions are pure.

use super::{SocialError, SocialLimits, SocialResult, clean_text};
use sr_core::UserId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelationshipKind {
    Friend,
    Blocked,
    PendingIncoming,
    PendingOutgoing,
    Ignored,
}

impl RelationshipKind {
    /// Discord relationship type codes; `Ignored` is SIDE ROOM's own (6).
    pub const fn code(self) -> i16 {
        match self {
            Self::Friend => 1,
            Self::Blocked => 2,
            Self::PendingIncoming => 3,
            Self::PendingOutgoing => 4,
            Self::Ignored => 6,
        }
    }

    pub const fn from_code(code: i16) -> Option<Self> {
        Some(match code {
            1 => Self::Friend,
            2 => Self::Blocked,
            3 => Self::PendingIncoming,
            4 => Self::PendingOutgoing,
            6 => Self::Ignored,
            _ => return None,
        })
    }

    const fn is_social_link(self) -> bool {
        matches!(
            self,
            Self::Friend | Self::PendingIncoming | Self::PendingOutgoing
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relationship {
    pub user: UserId,
    pub target: UserId,
    pub kind: RelationshipKind,
    pub nickname: Option<String>,
    pub since_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RelationshipCounts {
    pub friends: usize,
    pub pending_outgoing: usize,
    pub blocked: usize,
}

/// One atomic write step; a plan is applied as a unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelOp {
    Upsert {
        user: UserId,
        target: UserId,
        kind: RelationshipKind,
        since_ms: i64,
    },
    Delete {
        user: UserId,
        target: UserId,
    },
    SetNickname {
        user: UserId,
        target: UserId,
        nickname: Option<String>,
    },
}

/// Who may send this user a friend request (Discord: three independent toggles).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FriendRequestSources {
    pub everyone: bool,
    pub friends_of_friends: bool,
    pub server_members: bool,
}

impl Default for FriendRequestSources {
    fn default() -> Self {
        Self {
            everyone: true,
            friends_of_friends: true,
            server_members: true,
        }
    }
}

impl FriendRequestSources {
    pub fn allows(&self, shares_guild: bool, mutual_friend: bool) -> bool {
        self.everyone
            || (self.friends_of_friends && mutual_friend)
            || (self.server_members && shares_guild)
    }
}

/// Both directions of a pair as seen by `me` and `them`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PairState {
    pub mine: Option<RelationshipKind>,
    pub theirs: Option<RelationshipKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub ops: Vec<RelOp>,
    /// Resulting kind of `me`'s row, `None` when it was removed or absent.
    pub result: Option<RelationshipKind>,
    pub became_friends: bool,
}

impl Plan {
    fn noop(result: Option<RelationshipKind>) -> Self {
        Self {
            ops: vec![],
            result,
            became_friends: false,
        }
    }
}

fn upsert(user: UserId, target: UserId, kind: RelationshipKind, now: i64) -> RelOp {
    RelOp::Upsert {
        user,
        target,
        kind,
        since_ms: now,
    }
}

fn del(user: UserId, target: UserId) -> RelOp {
    RelOp::Delete { user, target }
}

fn befriend(me: UserId, them: UserId, now: i64) -> Plan {
    Plan {
        ops: vec![
            upsert(me, them, RelationshipKind::Friend, now),
            upsert(them, me, RelationshipKind::Friend, now),
        ],
        result: Some(RelationshipKind::Friend),
        became_friends: true,
    }
}

fn check_befriend_limits(
    mine: &RelationshipCounts,
    theirs: &RelationshipCounts,
    limits: &SocialLimits,
) -> SocialResult<()> {
    if mine.friends >= limits.max_friends || theirs.friends >= limits.max_friends {
        return Err(SocialError::MaxFriends);
    }
    Ok(())
}

/// Counters for both parties of a pair.
#[derive(Debug, Clone, Copy, Default)]
pub struct PairCounts {
    pub mine: RelationshipCounts,
    pub theirs: RelationshipCounts,
}

/// Send a friend request, or accept one if the target already asked us.
pub fn plan_request(
    me: UserId,
    them: UserId,
    st: PairState,
    counts: &PairCounts,
    sources_allow: bool,
    limits: &SocialLimits,
    now: i64,
) -> SocialResult<Plan> {
    if me == them {
        return Err(SocialError::SelfTarget);
    }
    match st.mine {
        Some(RelationshipKind::Blocked) => return Err(SocialError::BlockedByYou),
        Some(RelationshipKind::Friend) => return Err(SocialError::AlreadyFriends),
        Some(RelationshipKind::PendingOutgoing) => {
            return Ok(Plan::noop(Some(RelationshipKind::PendingOutgoing)));
        }
        Some(RelationshipKind::PendingIncoming) => {
            check_befriend_limits(&counts.mine, &counts.theirs, limits)?;
            return Ok(befriend(me, them, now));
        }
        Some(RelationshipKind::Ignored) | None => {}
    }
    if st.theirs == Some(RelationshipKind::Blocked) {
        return Err(SocialError::BlockedByThem);
    }
    if !sources_allow {
        return Err(SocialError::RequestsNotAccepted);
    }
    if counts.mine.pending_outgoing >= limits.max_pending_outgoing {
        return Err(SocialError::MaxPending);
    }
    let mut ops = vec![upsert(me, them, RelationshipKind::PendingOutgoing, now)];
    // A target who ignores us never sees the request (it simply stays pending).
    if st.theirs != Some(RelationshipKind::Ignored) {
        ops.push(upsert(them, me, RelationshipKind::PendingIncoming, now));
    }
    Ok(Plan {
        ops,
        result: Some(RelationshipKind::PendingOutgoing),
        became_friends: false,
    })
}

/// Accept an incoming request.
pub fn plan_accept(
    me: UserId,
    them: UserId,
    st: PairState,
    counts: &PairCounts,
    limits: &SocialLimits,
    now: i64,
) -> SocialResult<Plan> {
    if me == them {
        return Err(SocialError::SelfTarget);
    }
    if st.mine != Some(RelationshipKind::PendingIncoming) {
        return Err(SocialError::NoPendingRequest);
    }
    check_befriend_limits(&counts.mine, &counts.theirs, limits)?;
    Ok(befriend(me, them, now))
}

/// Remove a friend, cancel an outgoing request or decline an incoming one.
pub fn plan_remove(me: UserId, them: UserId, st: PairState) -> SocialResult<Plan> {
    if me == them {
        return Err(SocialError::SelfTarget);
    }
    if !st.mine.is_some_and(RelationshipKind::is_social_link) {
        return Err(SocialError::NoPendingRequest);
    }
    let mut ops = vec![del(me, them)];
    if st.theirs.is_some_and(RelationshipKind::is_social_link) {
        ops.push(del(them, me));
    }
    Ok(Plan {
        ops,
        result: None,
        became_friends: false,
    })
}

/// Block: replaces our row, drops friendship/requests on their side, keeps their
/// own block of us (if any).
pub fn plan_block(
    me: UserId,
    them: UserId,
    st: PairState,
    counts: &PairCounts,
    limits: &SocialLimits,
    now: i64,
) -> SocialResult<Plan> {
    if me == them {
        return Err(SocialError::SelfTarget);
    }
    if st.mine == Some(RelationshipKind::Blocked) {
        return Ok(Plan::noop(Some(RelationshipKind::Blocked)));
    }
    if counts.mine.blocked >= limits.max_blocked {
        return Err(SocialError::MaxBlocked);
    }
    let mut ops = vec![upsert(me, them, RelationshipKind::Blocked, now)];
    if st.theirs.is_some_and(RelationshipKind::is_social_link) {
        ops.push(del(them, me));
    }
    Ok(Plan {
        ops,
        result: Some(RelationshipKind::Blocked),
        became_friends: false,
    })
}

/// Ignore: hides the user from us. Replaces any friendship or request on our side
/// and removes the other side's friend/pending row so state stays consistent.
pub fn plan_ignore(me: UserId, them: UserId, st: PairState, now: i64) -> SocialResult<Plan> {
    if me == them {
        return Err(SocialError::SelfTarget);
    }
    match st.mine {
        Some(RelationshipKind::Blocked) => return Err(SocialError::BlockedByYou),
        Some(RelationshipKind::Ignored) => {
            return Ok(Plan::noop(Some(RelationshipKind::Ignored)));
        }
        _ => {}
    }
    let mut ops = vec![upsert(me, them, RelationshipKind::Ignored, now)];
    if st.theirs.is_some_and(RelationshipKind::is_social_link) {
        ops.push(del(them, me));
    }
    Ok(Plan {
        ops,
        result: Some(RelationshipKind::Ignored),
        became_friends: false,
    })
}

/// Unblock or unignore (drops our row if it is of `kind`).
pub fn plan_clear(
    me: UserId,
    them: UserId,
    st: PairState,
    kind: RelationshipKind,
) -> SocialResult<Plan> {
    if me == them {
        return Err(SocialError::SelfTarget);
    }
    if st.mine == Some(kind) {
        Ok(Plan {
            ops: vec![del(me, them)],
            result: None,
            became_friends: false,
        })
    } else {
        Ok(Plan::noop(st.mine))
    }
}

/// Set (or clear, with empty text) a friend's private nickname.
pub fn plan_nickname(
    me: UserId,
    them: UserId,
    st: PairState,
    nickname: Option<&str>,
    limits: &SocialLimits,
) -> SocialResult<Plan> {
    if me == them {
        return Err(SocialError::SelfTarget);
    }
    if st.mine != Some(RelationshipKind::Friend) {
        return Err(SocialError::NotFriends);
    }
    let nickname = clean_text("nickname", nickname, limits.max_nickname_chars)?;
    Ok(Plan {
        ops: vec![RelOp::SetNickname {
            user: me,
            target: them,
            nickname,
        }],
        result: Some(RelationshipKind::Friend),
        became_friends: false,
    })
}

pub fn is_blocked_either_way(st: PairState) -> bool {
    st.mine == Some(RelationshipKind::Blocked) || st.theirs == Some(RelationshipKind::Blocked)
}
