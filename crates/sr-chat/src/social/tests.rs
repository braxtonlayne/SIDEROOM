use super::dm::{DmAccess, DmKind, DmPrivacy, GroupEdit, dm_access};
use super::memory::MemoryRepo;
use super::notifications::{
    ChannelOverride, MentionKind, MuteDuration, NotificationLevel, ScopeSettings, effective_level,
    should_notify,
};
use super::presence::{CustomStatus, PresencePrefs, Status, merge_sessions};
use super::profile::{Connection, Profile, ProfileEdit};
use super::read_state::{self, ReadState};
use super::relationships::{PairState, RelationshipKind as K};
use super::repo::{Privacy, ProfileRepository, RelationshipRepository};
use super::service::{ScopeUpdate, SocialService};
use super::{SocialError, SocialLimits};
use crate::social::relationships::FriendRequestSources;
use sr_core::{ChannelId, Clock, ManualClock, MessageId, SnowflakeGenerator, UserId};
use std::sync::Arc;

fn u(n: u64) -> UserId {
    UserId::new(n)
}

struct Fx {
    svc: SocialService<MemoryRepo>,
    clock: Arc<ManualClock>,
}

fn fx_with(limits: SocialLimits) -> Fx {
    let clock = Arc::new(ManualClock::new(1_700_000_000_000));
    let dynclock: Arc<dyn Clock> = clock.clone();
    let ids = SnowflakeGenerator::new(dynclock.clone(), 1, 1).expect("ids");
    let repo = MemoryRepo::default();
    for n in 1..=15 {
        repo.add_user(u(n));
    }
    Fx {
        svc: SocialService::new(repo, dynclock, ids, limits),
        clock,
    }
}

fn fx() -> Fx {
    fx_with(SocialLimits::default())
}

async fn befriend(f: &Fx, a: u64, b: u64) {
    f.svc.send_friend_request(u(a), u(b)).await.expect("send");
    f.svc
        .accept_friend_request(u(b), u(a))
        .await
        .expect("accept");
}

// ---- relationships ----

#[tokio::test]
async fn request_accept_remove_cycle() {
    let f = fx();
    let p = f.svc.send_friend_request(u(1), u(2)).await.unwrap();
    assert_eq!(p.result, Some(K::PendingOutgoing));
    let rels = f.svc.list_relationships(u(2)).await.unwrap();
    assert_eq!(rels[0].kind, K::PendingIncoming);
    // Repeat is idempotent.
    assert!(
        f.svc
            .send_friend_request(u(1), u(2))
            .await
            .unwrap()
            .ops
            .is_empty()
    );
    let p = f.svc.accept_friend_request(u(2), u(1)).await.unwrap();
    assert!(p.became_friends);
    assert_eq!(
        f.svc.list_relationships(u(1)).await.unwrap()[0].kind,
        K::Friend
    );
    f.svc.remove_relationship(u(1), u(2)).await.unwrap();
    assert!(f.svc.list_relationships(u(1)).await.unwrap().is_empty());
    assert!(f.svc.list_relationships(u(2)).await.unwrap().is_empty());
}

#[tokio::test]
async fn crossing_requests_auto_accept() {
    let f = fx();
    f.svc.send_friend_request(u(1), u(2)).await.unwrap();
    let p = f.svc.send_friend_request(u(2), u(1)).await.unwrap();
    assert!(p.became_friends);
}

#[tokio::test]
async fn decline_and_cancel_remove_both_sides() {
    let f = fx();
    f.svc.send_friend_request(u(1), u(2)).await.unwrap();
    f.svc.remove_relationship(u(2), u(1)).await.unwrap(); // decline
    assert!(f.svc.list_relationships(u(1)).await.unwrap().is_empty());
    f.svc.send_friend_request(u(1), u(2)).await.unwrap();
    f.svc.remove_relationship(u(1), u(2)).await.unwrap(); // cancel
    assert!(f.svc.list_relationships(u(2)).await.unwrap().is_empty());
    assert!(matches!(
        f.svc.remove_relationship(u(1), u(2)).await,
        Err(SocialError::NoPendingRequest)
    ));
}

#[tokio::test]
async fn self_and_unknown_targets_rejected() {
    let f = fx();
    assert!(matches!(
        f.svc.send_friend_request(u(1), u(1)).await,
        Err(SocialError::SelfTarget)
    ));
    assert!(matches!(
        f.svc.send_friend_request(u(1), u(999)).await,
        Err(SocialError::UnknownUser)
    ));
    assert!(matches!(
        f.svc.accept_friend_request(u(1), u(2)).await,
        Err(SocialError::NoPendingRequest)
    ));
}

#[tokio::test]
async fn block_breaks_friendship_and_prevents_requests() {
    let f = fx();
    befriend(&f, 1, 2).await;
    f.svc.block(u(1), u(2)).await.unwrap();
    let mine = f.svc.list_relationships(u(1)).await.unwrap();
    assert_eq!(mine[0].kind, K::Blocked);
    assert!(f.svc.list_relationships(u(2)).await.unwrap().is_empty());
    assert!(matches!(
        f.svc.send_friend_request(u(2), u(1)).await,
        Err(SocialError::BlockedByThem)
    ));
    assert!(matches!(
        f.svc.send_friend_request(u(1), u(2)).await,
        Err(SocialError::BlockedByYou)
    ));
    f.svc.unblock(u(1), u(2)).await.unwrap();
    assert!(f.svc.list_relationships(u(1)).await.unwrap().is_empty());
}

#[tokio::test]
async fn mutual_blocks_survive_one_unblock() {
    let f = fx();
    f.svc.block(u(1), u(2)).await.unwrap();
    f.svc.block(u(2), u(1)).await.unwrap();
    f.svc.unblock(u(1), u(2)).await.unwrap();
    assert_eq!(
        f.svc.list_relationships(u(2)).await.unwrap()[0].kind,
        K::Blocked
    );
}

#[tokio::test]
async fn ignored_sender_request_stays_hidden_from_target() {
    let f = fx();
    f.svc.ignore(u(2), u(1)).await.unwrap();
    f.svc.send_friend_request(u(1), u(2)).await.unwrap();
    // Sender sees the pending request; the ignoring target sees nothing.
    assert_eq!(
        f.svc.list_relationships(u(1)).await.unwrap()[0].kind,
        K::PendingOutgoing
    );
    assert!(f.svc.list_relationships(u(2)).await.unwrap().is_empty());
    assert_eq!(f.svc.list_ignores(u(2)).await.unwrap(), vec![u(1)]);
    f.svc.unignore(u(2), u(1)).await.unwrap();
    assert!(f.svc.list_ignores(u(2)).await.unwrap().is_empty());
}

#[tokio::test]
async fn ignore_is_independent_of_friendship() {
    let f = fx();
    befriend(&f, 1, 2).await;
    f.svc.ignore(u(1), u(2)).await.unwrap();
    // Friend rows are untouched on both sides.
    assert_eq!(
        f.svc.list_relationships(u(1)).await.unwrap()[0].kind,
        K::Friend
    );
    assert_eq!(
        f.svc.list_relationships(u(2)).await.unwrap()[0].kind,
        K::Friend
    );
    assert!(f.svc.is_ignoring(u(1), u(2)).await.unwrap());
    assert!(!f.svc.is_ignoring(u(2), u(1)).await.unwrap());
    // Idempotent, and an ignored friend can still be messaged (friendship decides).
    f.svc.ignore(u(1), u(2)).await.unwrap();
    assert_eq!(f.svc.list_ignores(u(1)).await.unwrap(), vec![u(2)]);
    assert!(matches!(
        f.svc.open_dm(u(2), u(1)).await.unwrap().access,
        DmAccess::Allowed
    ));
}

#[tokio::test]
async fn ignore_rules_self_blocked_and_unknown() {
    let f = fx();
    assert!(matches!(
        f.svc.ignore(u(1), u(1)).await,
        Err(SocialError::SelfTarget)
    ));
    assert!(matches!(
        f.svc.ignore(u(1), u(99)).await,
        Err(SocialError::UnknownUser)
    ));
    f.svc.block(u(1), u(2)).await.unwrap();
    assert!(matches!(
        f.svc.ignore(u(1), u(2)).await,
        Err(SocialError::BlockedByYou)
    ));
}

#[tokio::test]
async fn private_notes_limit_and_clear() {
    let f = fx();
    f.svc
        .set_note(u(1), u(2), Some("  met at con  "))
        .await
        .unwrap();
    assert_eq!(
        f.svc.get_note(u(1), u(2)).await.unwrap().as_deref(),
        Some("met at con")
    );
    // Notes are private to the author.
    assert_eq!(f.svc.get_note(u(2), u(1)).await.unwrap(), None);
    assert!(matches!(
        f.svc.set_note(u(1), u(2), Some(&"x".repeat(257))).await,
        Err(SocialError::Invalid { .. })
    ));
    assert!(matches!(
        f.svc.set_note(u(1), u(1), Some("me")).await,
        Err(SocialError::SelfTarget)
    ));
    f.svc.set_note(u(1), u(2), Some("   ")).await.unwrap();
    assert_eq!(f.svc.get_note(u(1), u(2)).await.unwrap(), None);
}

#[tokio::test]
async fn message_request_pending_then_responded() {
    use super::dm::RequestState;
    let f = fx();
    f.svc.repo().share_guild(u(1), u(2));
    let opened = f.svc.open_dm(u(1), u(2)).await.unwrap();
    assert_eq!(opened.access, DmAccess::Request);
    let id = opened.channel.id;
    assert_eq!(
        f.svc.request_state(u(2), id).await.unwrap(),
        Some(RequestState::Pending)
    );
    // The sender's own row is not a request.
    assert_eq!(
        f.svc.request_state(u(1), id).await.unwrap(),
        Some(RequestState::Accepted)
    );
    assert!(matches!(
        f.svc
            .respond_to_request(u(2), id, RequestState::Pending)
            .await,
        Err(SocialError::Invalid { .. })
    ));
    f.svc
        .respond_to_request(u(2), id, RequestState::Spam)
        .await
        .unwrap();
    assert_eq!(
        f.svc.request_state(u(2), id).await.unwrap(),
        Some(RequestState::Spam)
    );
}

#[tokio::test]
async fn friends_dm_has_no_request() {
    use super::dm::RequestState;
    let f = fx();
    befriend(&f, 1, 2).await;
    let id = f.svc.open_dm(u(1), u(2)).await.unwrap().channel.id;
    assert_eq!(
        f.svc.request_state(u(2), id).await.unwrap(),
        Some(RequestState::Accepted)
    );
}

#[tokio::test]
async fn connections_verified_only_via_oauth() {
    let f = fx();
    f.svc
        .repo()
        .put_profile(&Profile {
            user: u(1),
            ..Profile::default()
        })
        .await
        .unwrap();
    let conn = |verified| Connection {
        user: u(1),
        provider: "github".into(),
        external_id: "octo".into(),
        display_name: "octo".into(),
        verified,
        visible: true,
        created_ms: 0,
        url: None,
        show_activity: true,
        stats: serde_json::Value::Null,
    };
    f.svc.upsert_connection(conn(true)).await.unwrap();
    assert!(!f.svc.get_profile(u(1), u(1)).await.unwrap().connections[0].verified);
    f.svc.upsert_verified_connection(conn(false)).await.unwrap();
    assert!(f.svc.get_profile(u(1), u(1)).await.unwrap().connections[0].verified);
    // A plain re-link keeps the proven flag.
    f.svc.upsert_connection(conn(false)).await.unwrap();
    assert!(f.svc.get_profile(u(1), u(1)).await.unwrap().connections[0].verified);
}

#[tokio::test]
async fn mentions_counted_only_for_eligible_users() {
    let f = fx();
    let ch = ChannelId::new(7_000);
    f.svc
        .record_eligible_mentions(u(1), &[u(1), u(2), u(3), u(3)], ch, |user| user != u(3))
        .await
        .unwrap();
    let states = f.svc.read_states(u(2)).await.unwrap();
    assert_eq!(states.len(), 1);
    assert_eq!(states[0].mention_count, 1);
    assert!(f.svc.read_states(u(3)).await.unwrap().is_empty());
    assert!(f.svc.read_states(u(1)).await.unwrap().is_empty());
}

#[tokio::test]
async fn nicknames_only_for_friends_and_trimmed() {
    let f = fx();
    assert!(matches!(
        f.svc.set_friend_nickname(u(1), u(2), Some("x")).await,
        Err(SocialError::NotFriends)
    ));
    befriend(&f, 1, 2).await;
    f.svc
        .set_friend_nickname(u(1), u(2), Some("  Bestie "))
        .await
        .unwrap();
    let r = f.svc.list_relationships(u(1)).await.unwrap();
    assert_eq!(r[0].nickname.as_deref(), Some("Bestie"));
    // The other side does not see it.
    assert_eq!(
        f.svc.list_relationships(u(2)).await.unwrap()[0].nickname,
        None
    );
    assert!(
        f.svc
            .set_friend_nickname(u(1), u(2), Some(&"n".repeat(33)))
            .await
            .is_err()
    );
    f.svc
        .set_friend_nickname(u(1), u(2), Some("  "))
        .await
        .unwrap();
    assert_eq!(
        f.svc.list_relationships(u(1)).await.unwrap()[0].nickname,
        None
    );
}

#[tokio::test]
async fn friend_limits_enforced_for_both_sides() {
    let f = fx_with(SocialLimits {
        max_friends: 1,
        max_pending_outgoing: 1,
        ..SocialLimits::default()
    });
    befriend(&f, 1, 2).await;
    f.svc.send_friend_request(u(3), u(1)).await.unwrap();
    assert!(matches!(
        f.svc.accept_friend_request(u(1), u(3)).await,
        Err(SocialError::MaxFriends)
    ));
    f.svc.send_friend_request(u(4), u(5)).await.unwrap();
    assert!(matches!(
        f.svc.send_friend_request(u(4), u(6)).await,
        Err(SocialError::MaxPending)
    ));
}

#[tokio::test]
async fn friend_request_sources_enforced() {
    let f = fx();
    let privacy = Privacy {
        friend_requests: FriendRequestSources {
            everyone: false,
            friends_of_friends: true,
            server_members: false,
        },
        ..Privacy::default()
    };
    f.svc_repo().put_privacy(u(2), &privacy).await.unwrap();
    assert!(matches!(
        f.svc.send_friend_request(u(1), u(2)).await,
        Err(SocialError::RequestsNotAccepted)
    ));
    befriend(&f, 1, 3).await;
    befriend(&f, 2, 3).await;
    f.svc.send_friend_request(u(1), u(2)).await.unwrap();
}

impl Fx {
    fn svc_repo(&self) -> &MemoryRepo {
        self.svc.repo()
    }
}

// ---- DMs ----

#[test]
fn dm_access_matrix() {
    let p = DmPrivacy::default();
    let friends = PairState {
        mine: Some(K::Friend),
        theirs: Some(K::Friend),
        ..PairState::default()
    };
    assert_eq!(dm_access(friends, &p, false), DmAccess::Allowed);
    let strangers = PairState::default();
    assert_eq!(dm_access(strangers, &p, false), DmAccess::Denied);
    assert_eq!(dm_access(strangers, &p, true), DmAccess::Request);
    let open = DmPrivacy {
        message_requests: false,
        ..p
    };
    assert_eq!(dm_access(strangers, &open, true), DmAccess::Allowed);
    let closed = DmPrivacy {
        allow_server_member_dms: false,
        ..p
    };
    assert_eq!(dm_access(strangers, &closed, true), DmAccess::Denied);
    let blocked = PairState {
        mine: None,
        theirs: Some(K::Blocked),
        ..PairState::default()
    };
    let ignored_by_recipient = PairState {
        they_ignore_me: true,
        ..PairState::default()
    };
    assert_eq!(
        dm_access(ignored_by_recipient, &open, true),
        DmAccess::Request
    );
    assert_eq!(dm_access(blocked, &p, true), DmAccess::Denied);
}

#[tokio::test]
async fn open_dm_find_or_create() {
    let f = fx();
    befriend(&f, 1, 2).await;
    let a = f.svc.open_dm(u(1), u(2)).await.unwrap();
    assert!(a.created);
    assert_eq!(a.channel.kind, DmKind::Dm);
    let b = f.svc.open_dm(u(2), u(1)).await.unwrap();
    assert!(!b.created);
    assert_eq!(a.channel.id, b.channel.id);
}

#[tokio::test]
async fn open_dm_requires_friendship_or_shared_guild_and_respects_blocks() {
    let f = fx();
    assert!(matches!(
        f.svc.open_dm(u(1), u(2)).await,
        Err(SocialError::DmNotAllowed)
    ));
    f.svc_repo().share_guild(u(1), u(2));
    let o = f.svc.open_dm(u(1), u(2)).await.unwrap();
    assert_eq!(o.access, DmAccess::Request);
    f.svc.block(u(2), u(1)).await.unwrap();
    assert!(matches!(
        f.svc.open_dm(u(1), u(2)).await,
        Err(SocialError::DmNotAllowed)
    ));
    assert!(!f.svc.can_send_dm(u(1), u(2)).await.unwrap());
}

#[tokio::test]
async fn closed_dm_hidden_until_reopened() {
    let f = fx();
    befriend(&f, 1, 2).await;
    let ch = f.svc.open_dm(u(1), u(2)).await.unwrap().channel;
    f.svc.close_dm(u(1), ch.id).await.unwrap();
    assert!(f.svc.list_dms(u(1)).await.unwrap().is_empty());
    assert_eq!(f.svc.list_dms(u(2)).await.unwrap().len(), 1);
    assert_eq!(f.svc.reopen_for_message(ch.id).await.unwrap(), vec![u(1)]);
    assert_eq!(f.svc.list_dms(u(1)).await.unwrap().len(), 1);
    assert!(f.svc.reopen_for_message(ch.id).await.unwrap().is_empty());
    assert!(matches!(
        f.svc.close_dm(u(3), ch.id).await,
        Err(SocialError::NotParticipant)
    ));
}

async fn group_of(f: &Fx, owner: u64, others: &[u64]) -> ChannelId {
    for o in others {
        befriend(f, owner, *o).await;
    }
    let ids: Vec<_> = others.iter().map(|o| u(*o)).collect();
    f.svc
        .create_group_dm(u(owner), &ids, None)
        .await
        .unwrap()
        .id
}

#[tokio::test]
async fn group_create_rules() {
    let f = fx();
    befriend(&f, 1, 2).await;
    // One recipient is a plain DM, not a group.
    assert!(f.svc.create_group_dm(u(1), &[u(2)], None).await.is_err());
    // Non-friend refused.
    assert!(matches!(
        f.svc.create_group_dm(u(1), &[u(2), u(3)], None).await,
        Err(SocialError::NotFriends)
    ));
    befriend(&f, 1, 3).await;
    let g = f
        .svc
        .create_group_dm(u(1), &[u(2), u(3), u(2)], Some("  crew "))
        .await
        .unwrap();
    assert_eq!(g.owner, Some(u(1)));
    assert_eq!(g.recipients, vec![u(1), u(2), u(3)]);
    assert_eq!(g.name.as_deref(), Some("crew"));
}

#[tokio::test]
async fn group_size_limit_is_ten_including_owner() {
    let f = fx();
    let nine: Vec<u64> = (2..=10).collect();
    let id = group_of(&f, 1, &nine).await; // exactly 10 members
    befriend(&f, 1, 11).await;
    assert!(matches!(
        f.svc.add_recipients(u(1), id, &[u(11)]).await,
        Err(SocialError::MaxGroupSize)
    ));
    let too_many: Vec<UserId> = (2..=11).map(u).collect();
    assert!(matches!(
        f.svc.create_group_dm(u(1), &too_many, None).await,
        Err(SocialError::MaxGroupSize)
    ));
}

#[tokio::test]
async fn group_dm_count_limit() {
    let f = fx_with(SocialLimits {
        max_group_dms_per_user: 1,
        ..SocialLimits::default()
    });
    group_of(&f, 1, &[2, 3]).await;
    assert!(matches!(
        f.svc.create_group_dm(u(1), &[u(2), u(3)], None).await,
        Err(SocialError::MaxGroupDms)
    ));
}

#[tokio::test]
async fn add_to_group_and_to_one_to_one() {
    let f = fx();
    let g = group_of(&f, 1, &[2, 3]).await;
    befriend(&f, 2, 4).await;
    // A non-owner member can add their own friend.
    let after = f
        .svc
        .add_recipients(u(2), g, &[u(4)])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.recipients.len(), 4);
    // Adding an existing member is a no-op.
    assert!(
        f.svc
            .add_recipients(u(1), g, &[u(2)])
            .await
            .unwrap()
            .is_none()
    );
    // Outsiders cannot add.
    assert!(matches!(
        f.svc.add_recipients(u(9), g, &[u(4)]).await,
        Err(SocialError::NotParticipant)
    ));
    // Adding to a 1:1 spawns a new group, leaving the DM.
    befriend(&f, 5, 6).await;
    befriend(&f, 5, 7).await;
    let dm = f.svc.open_dm(u(5), u(6)).await.unwrap().channel;
    let grp = f
        .svc
        .add_recipients(u(5), dm.id, &[u(7)])
        .await
        .unwrap()
        .unwrap();
    assert_ne!(grp.id, dm.id);
    assert_eq!(grp.kind, DmKind::Group);
    assert_eq!(grp.owner, Some(u(5)));
    assert_eq!(grp.recipients.len(), 3);
    assert!(f.svc.open_dm(u(5), u(6)).await.unwrap().channel.id == dm.id);
}

#[tokio::test]
async fn add_blocked_or_stranger_rejected() {
    let f = fx();
    let g = group_of(&f, 1, &[2, 3]).await;
    assert!(matches!(
        f.svc.add_recipients(u(1), g, &[u(4)]).await,
        Err(SocialError::NotFriends)
    ));
    f.svc.block(u(1), u(4)).await.unwrap();
    assert!(matches!(
        f.svc.add_recipients(u(1), g, &[u(4)]).await,
        Err(SocialError::BlockedByYou)
    ));
}

#[tokio::test]
async fn remove_leave_and_owner_transfer() {
    let f = fx();
    let g = group_of(&f, 1, &[2, 3, 4]).await;
    // Non-owner cannot kick others but can leave.
    assert!(matches!(
        f.svc.remove_recipient(u(2), g, u(3)).await,
        Err(SocialError::NotOwner)
    ));
    let after = f
        .svc
        .remove_recipient(u(1), g, u(4))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.recipients, vec![u(1), u(2), u(3)]);
    // Owner leaves: longest-standing member inherits.
    let after = f
        .svc
        .remove_recipient(u(1), g, u(1))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.owner, Some(u(2)));
    // Explicit transfer.
    assert!(matches!(
        f.svc.transfer_owner(u(3), g, u(3)).await,
        Err(SocialError::NotOwner)
    ));
    let after = f.svc.transfer_owner(u(2), g, u(3)).await.unwrap();
    assert_eq!(after.owner, Some(u(3)));
    assert!(f.svc.transfer_owner(u(3), g, u(9)).await.is_err());
    // Last members leave: channel deleted.
    f.svc.remove_recipient(u(2), g, u(2)).await.unwrap();
    assert!(
        f.svc
            .remove_recipient(u(3), g, u(3))
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        f.svc.close_dm(u(3), g).await,
        Err(SocialError::UnknownChannel)
    ));
}

#[tokio::test]
async fn edit_group_name_and_icon() {
    let f = fx();
    let g = group_of(&f, 1, &[2, 3]).await;
    let ch = f
        .svc
        .edit_group(
            u(2),
            g,
            &GroupEdit {
                name: Some(" Raid night ".into()),
                icon_key: Some(Some("icons/x".into())),
            },
        )
        .await
        .unwrap();
    assert_eq!(ch.name.as_deref(), Some("Raid night"));
    assert_eq!(ch.icon_key.as_deref(), Some("icons/x"));
    let ch = f
        .svc
        .edit_group(
            u(1),
            g,
            &GroupEdit {
                name: Some(String::new()),
                icon_key: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(ch.name, None);
    assert_eq!(ch.icon_key.as_deref(), Some("icons/x"));
    assert!(
        f.svc
            .edit_group(
                u(1),
                g,
                &GroupEdit {
                    name: Some("n".repeat(101)),
                    icon_key: None
                }
            )
            .await
            .is_err()
    );
    // 1:1 DMs have no group settings.
    let dm = f.svc.open_dm(u(1), u(2)).await.unwrap().channel;
    assert!(matches!(
        f.svc.edit_group(u(1), dm.id, &GroupEdit::default()).await,
        Err(SocialError::NotGroupDm)
    ));
}

// ---- read states ----

#[test]
fn read_state_rules() {
    let (user, ch) = (u(1), ChannelId::new(10));
    let s = read_state::add_mentions(None, user, ch, 3);
    assert_eq!(s.mention_count, 3);
    let s = read_state::ack(Some(s), user, ch, MessageId::new(50));
    assert_eq!(
        (s.last_read, s.mention_count),
        (Some(MessageId::new(50)), 0)
    );
    // Ack never moves backwards.
    let s = read_state::ack(Some(s), user, ch, MessageId::new(40));
    assert_eq!(s.last_read, Some(MessageId::new(50)));
    assert!(!s.has_unread(Some(MessageId::new(50))));
    assert!(s.has_unread(Some(MessageId::new(51))));
    // Mark unread moves it back explicitly.
    let s = read_state::mark_unread(Some(s), user, ch, Some(MessageId::new(30)), 2);
    assert_eq!(
        (s.last_read, s.mention_count),
        (Some(MessageId::new(30)), 2)
    );
    assert!(ReadState::new(user, ch).has_unread(Some(MessageId::new(1))));
    assert!(!ReadState::new(user, ch).has_unread(None));
    let s = read_state::add_mentions(None, user, ch, u32::MAX);
    assert_eq!(s.mention_count, read_state::MAX_MENTION_COUNT);
}

#[tokio::test]
async fn read_state_service_flow() {
    let f = fx();
    let ch = ChannelId::new(77);
    f.svc
        .record_mentions(u(1), &[u(1), u(2), u(2), u(3)], ch)
        .await
        .unwrap();
    assert!(f.svc.read_states(u(1)).await.unwrap().is_empty());
    assert_eq!(f.svc.read_states(u(2)).await.unwrap()[0].mention_count, 1);
    f.svc.record_mentions(u(1), &[u(2)], ch).await.unwrap();
    assert_eq!(f.svc.read_states(u(2)).await.unwrap()[0].mention_count, 2);
    let s = f.svc.ack(u(2), ch, MessageId::new(9)).await.unwrap();
    assert_eq!(s.mention_count, 0);
    let n = f
        .svc
        .ack_all(
            u(2),
            &[
                (ch, MessageId::new(12)),
                (ChannelId::new(78), MessageId::new(3)),
            ],
        )
        .await
        .unwrap();
    assert_eq!(n, 2);
    assert_eq!(f.svc.read_states(u(2)).await.unwrap().len(), 2);
    let s = f
        .svc
        .mark_unread(u(2), ch, Some(MessageId::new(5)), 4)
        .await
        .unwrap();
    assert_eq!(s.mention_count, 4);
    let s = f.svc.ack_pins(u(2), ch).await.unwrap();
    assert_eq!(s.last_pin_ms, Some(1_700_000_000_000));
}

// ---- notifications ----

#[test]
fn level_resolution_order() {
    let (user, ch, cat) = (u(1), ChannelId::new(1), ChannelId::new(2));
    let mut scope = ScopeSettings::new(user, None);
    let mut c_over = ChannelOverride::new(user, ch);
    let mut cat_over = ChannelOverride::new(user, cat);
    let def = NotificationLevel::OnlyMentions;
    assert_eq!(effective_level(def, None, None, None), def);
    scope.level = Some(NotificationLevel::All);
    assert_eq!(
        effective_level(def, Some(&scope), None, None),
        NotificationLevel::All
    );
    cat_over.level = Some(NotificationLevel::Nothing);
    assert_eq!(
        effective_level(def, Some(&scope), Some(&cat_over), None),
        NotificationLevel::Nothing
    );
    c_over.level = Some(NotificationLevel::All);
    assert_eq!(
        effective_level(def, Some(&scope), Some(&cat_over), Some(&c_over)),
        NotificationLevel::All
    );
}

#[test]
fn mute_expiry() {
    let m = super::notifications::Mute::new(MuteDuration::Hour1, 1000);
    assert!(m.is_active(1000 + 3_599_999));
    assert!(!m.is_active(1000 + 3_600_000));
    assert_eq!(
        m.normalized(10_000_000),
        super::notifications::Mute::default()
    );
    let forever = super::notifications::Mute::new(MuteDuration::UntilTurnedOff, 1000);
    assert!(forever.is_active(i64::MAX));
}

#[test]
fn notify_decisions() {
    let mut scope = ScopeSettings::new(u(1), None);
    let direct = MentionKind {
        direct: true,
        ..MentionKind::default()
    };
    let everyone = MentionKind {
        everyone: true,
        ..MentionKind::default()
    };
    let plain = MentionKind::default();
    assert!(should_notify(NotificationLevel::All, false, plain, None));
    assert!(!should_notify(NotificationLevel::All, true, direct, None));
    assert!(!should_notify(
        NotificationLevel::Nothing,
        false,
        direct,
        None
    ));
    assert!(should_notify(
        NotificationLevel::OnlyMentions,
        false,
        direct,
        None
    ));
    assert!(!should_notify(
        NotificationLevel::OnlyMentions,
        false,
        plain,
        None
    ));
    assert!(should_notify(
        NotificationLevel::OnlyMentions,
        false,
        everyone,
        Some(&scope)
    ));
    scope.suppress_everyone = true;
    assert!(!should_notify(
        NotificationLevel::OnlyMentions,
        false,
        everyone,
        Some(&scope)
    ));
}

#[tokio::test]
async fn notification_settings_service() {
    let f = fx();
    let g = sr_core::GuildId::new(5);
    let (ch, cat) = (ChannelId::new(1), ChannelId::new(2));
    let s = f
        .svc
        .update_scope(
            u(1),
            Some(g),
            &ScopeUpdate {
                level: Some(Some(NotificationLevel::OnlyMentions)),
                suppress_roles: Some(true),
                ..ScopeUpdate::default()
            },
        )
        .await
        .unwrap();
    assert!(s.suppress_roles && s.mobile_push);
    let r = f
        .svc
        .resolve_notifications(u(1), Some(g), ch, Some(cat), NotificationLevel::All)
        .await
        .unwrap();
    assert_eq!((r.level, r.muted), (NotificationLevel::OnlyMentions, false));
    // Category override then channel override.
    f.svc
        .update_override(u(1), cat, Some(Some(NotificationLevel::Nothing)), None)
        .await
        .unwrap();
    f.svc
        .update_override(u(1), ch, None, Some(Some(MuteDuration::Minutes15)))
        .await
        .unwrap();
    let r = f
        .svc
        .resolve_notifications(u(1), Some(g), ch, Some(cat), NotificationLevel::All)
        .await
        .unwrap();
    assert_eq!((r.level, r.muted), (NotificationLevel::Nothing, true));
    // Mute expires with the clock.
    f.clock.advance_ms(15 * 60_000);
    let r = f
        .svc
        .resolve_notifications(u(1), Some(g), ch, Some(cat), NotificationLevel::All)
        .await
        .unwrap();
    assert!(!r.muted);
    // Clearing an override that carries nothing deletes the row.
    assert!(
        f.svc
            .update_override(u(1), ch, Some(None), Some(None))
            .await
            .unwrap()
            .is_none()
    );
    // Guild-wide mute.
    f.svc
        .mute_scope(u(1), Some(g), Some(MuteDuration::UntilTurnedOff))
        .await
        .unwrap();
    assert!(
        f.svc
            .resolve_notifications(u(1), Some(g), ch, None, NotificationLevel::All)
            .await
            .unwrap()
            .muted
    );
    assert!(
        !f.svc
            .should_notify(
                u(1),
                Some(g),
                ch,
                None,
                NotificationLevel::All,
                MentionKind {
                    direct: true,
                    ..MentionKind::default()
                }
            )
            .await
            .unwrap()
    );
    f.svc.mute_scope(u(1), Some(g), None).await.unwrap();
    assert!(
        !f.svc
            .resolve_notifications(u(1), Some(g), ch, None, NotificationLevel::All)
            .await
            .unwrap()
            .muted
    );
}

// ---- profiles ----

#[tokio::test]
async fn profile_edit_and_connections() {
    let f = fx();
    f.svc_repo().store.lock().unwrap().profiles.insert(
        u(1),
        Profile {
            user: u(1),
            ..Profile::default()
        },
    );
    let p = f
        .svc
        .edit_profile(
            u(1),
            &ProfileEdit {
                bio: Some(Some("  hello **world**  ".into())),
                pronouns: Some(Some("they/them".into())),
                accent_color: Some(Some(0x00AB_CDEF)),
                banner_key: Some(Some("banners/1".into())),
                display_name: Some(Some("  Ada L.  ".into())),
                avatar_key: Some(Some("0123456789abcdef0123456789abcdef".into())),
                ..ProfileEdit::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(p.bio.as_deref(), Some("hello **world**"));
    assert_eq!(p.display_name.as_deref(), Some("Ada L."));
    assert_eq!(p.accent_color, Some(0x00AB_CDEF));
    assert_eq!(
        p.avatar_key.as_deref(),
        Some("0123456789abcdef0123456789abcdef")
    );
    // Bad values rejected without changing anything.
    for bad in [
        ProfileEdit {
            bio: Some(Some("x".repeat(401))),
            ..ProfileEdit::default()
        },
        ProfileEdit {
            pronouns: Some(Some("x".repeat(41))),
            ..ProfileEdit::default()
        },
        ProfileEdit {
            accent_color: Some(Some(0x0100_0000)),
            ..ProfileEdit::default()
        },
    ] {
        assert!(f.svc.edit_profile(u(1), &bad).await.is_err());
    }
    // Clearing.
    let p = f
        .svc
        .edit_profile(
            u(1),
            &ProfileEdit {
                bio: Some(None),
                ..ProfileEdit::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(p.bio, None);
    assert_eq!(p.pronouns.as_deref(), Some("they/them"));

    let conn = |visible, name: &str| Connection {
        user: u(1),
        provider: "github".into(),
        external_id: name.into(),
        display_name: name.into(),
        verified: false,
        visible,
        created_ms: 0,
        url: None,
        show_activity: true,
        stats: serde_json::Value::Null,
    };
    f.svc.upsert_connection(conn(true, "pub")).await.unwrap();
    f.svc.upsert_connection(conn(false, "priv")).await.unwrap();
    assert_eq!(
        f.svc
            .get_profile(u(2), u(1))
            .await
            .unwrap()
            .connections
            .len(),
        1
    );
    assert_eq!(
        f.svc
            .get_profile(u(1), u(1))
            .await
            .unwrap()
            .connections
            .len(),
        2
    );
    let mut bad = conn(true, "x");
    bad.provider = "Git Hub".into();
    assert!(f.svc.upsert_connection(bad).await.is_err());
    f.svc
        .remove_connection(u(1), "github", "priv")
        .await
        .unwrap();
    assert_eq!(
        f.svc
            .get_profile(u(1), u(1))
            .await
            .unwrap()
            .connections
            .len(),
        1
    );
    assert!(matches!(
        f.svc.get_profile(u(1), u(9)).await,
        Err(SocialError::UnknownUser)
    ));
}

// ---- presence ----

#[tokio::test]
async fn presence_status_and_custom_status() {
    let f = fx();
    assert_eq!(
        f.svc.presence(u(1)).await.unwrap(),
        PresencePrefs::new(u(1))
    );
    let p = f
        .svc
        .set_status(u(1), Status::Dnd, Some(3_600_000))
        .await
        .unwrap();
    assert_eq!(p.status, Status::Dnd);
    f.clock.advance_ms(3_600_000);
    assert_eq!(f.svc.presence(u(1)).await.unwrap().status, Status::Online);
    assert!(f.svc.set_status(u(1), Status::Offline, None).await.is_err());
    assert!(f.svc.set_status(u(1), Status::Idle, Some(0)).await.is_err());
    let p = f
        .svc
        .set_status(u(1), Status::Invisible, None)
        .await
        .unwrap();
    assert_eq!(p.status.visible(), Status::Offline);

    let now = 1_700_000_000_000 + 3_600_000;
    let p = f
        .svc
        .set_custom_status(
            u(1),
            CustomStatus {
                text: Some(" Glooping ".into()),
                emoji_name: Some("tada".into()),
                emoji_id: Some(5),
                expires_ms: Some(now + 1000),
            },
        )
        .await
        .unwrap();
    assert_eq!(p.custom.text.as_deref(), Some("Glooping"));
    // Status choice survives the custom status update.
    assert_eq!(p.status, Status::Invisible);
    f.clock.advance_ms(1000);
    assert!(f.svc.presence(u(1)).await.unwrap().custom.is_empty());
    assert!(
        f.svc
            .set_custom_status(
                u(1),
                CustomStatus {
                    text: Some("x".repeat(129)),
                    ..CustomStatus::default()
                }
            )
            .await
            .is_err()
    );
    // Empty clears; emoji id without name is dropped.
    let p = f
        .svc
        .set_custom_status(
            u(1),
            CustomStatus {
                emoji_id: Some(3),
                ..CustomStatus::default()
            },
        )
        .await
        .unwrap();
    assert!(p.custom.is_empty());
}

#[test]
fn session_merge_priority() {
    use Status::{Dnd, Idle, Invisible, Offline, Online};
    // No contributing session means offline, whatever was chosen.
    assert_eq!(merge_sessions(Online, &[]), Offline);
    assert_eq!(merge_sessions(Dnd, &[]), Offline);
    assert_eq!(merge_sessions(Online, &[Invisible]), Offline);
    // A chosen dnd / idle / invisible applies to every session.
    assert_eq!(merge_sessions(Dnd, &[Online, Idle]), Dnd);
    assert_eq!(merge_sessions(Idle, &[Online]), Idle);
    assert_eq!(merge_sessions(Invisible, &[Online]), Invisible);
    // Otherwise online if any session is active; idle only when all are idle.
    assert_eq!(merge_sessions(Online, &[Idle, Online]), Online);
    assert_eq!(merge_sessions(Online, &[Idle, Idle]), Idle);
    assert_eq!(merge_sessions(Online, &[Idle, Invisible]), Idle);
    assert_eq!(merge_sessions(Online, &[Idle, Dnd]), Dnd);
}

#[tokio::test]
async fn effective_status_uses_chosen_setting() {
    let f = fx();
    let sessions = [Status::Online, Status::Idle];
    assert_eq!(
        f.svc.effective_status(u(1), &sessions).await.unwrap(),
        Status::Online
    );
    f.svc
        .set_status(u(1), Status::Dnd, Some(3_600_000))
        .await
        .unwrap();
    assert_eq!(
        f.svc.effective_status(u(1), &sessions).await.unwrap(),
        Status::Dnd
    );
    f.clock.advance_ms(3_600_000);
    assert_eq!(
        f.svc.effective_status(u(1), &sessions).await.unwrap(),
        Status::Online
    );
}

#[test]
fn error_mapping() {
    assert_eq!(SocialError::UnknownUser.to_api_error().code.code(), 10013);
    assert_eq!(SocialError::DmNotAllowed.to_api_error().code.code(), 50007);
    assert_eq!(SocialError::NotOwner.to_api_error().status(), 403);
    assert_eq!(SocialError::MaxFriends.to_api_error().status(), 400);
}

// ---- user settings, privacy, global notification default ----

#[tokio::test]
async fn user_settings_blob_merges_versions_and_caps() {
    use super::user_settings::MAX_SETTINGS_BYTES;
    use serde_json::json;
    let f = fx();
    let empty = f.svc.user_settings(u(1)).await.unwrap();
    assert_eq!((empty.version, empty.data), (0, json!({})));

    let s = f
        .svc
        .patch_user_settings(u(1), &json!({"theme": "ash", "ui": {"compact": true}}))
        .await
        .unwrap();
    assert_eq!(s.version, 1);
    let s = f
        .svc
        .patch_user_settings(u(1), &json!({"ui": {"zoom": 110}, "theme": null}))
        .await
        .unwrap();
    assert_eq!(s.version, 2);
    assert_eq!(s.data, json!({"ui": {"compact": true, "zoom": 110}}));
    // Settings are per user.
    assert_eq!(f.svc.user_settings(u(2)).await.unwrap().version, 0);
    // Rejected patches change nothing and do not bump the version.
    let too_big = json!({"x": "y".repeat(MAX_SETTINGS_BYTES)});
    assert!(f.svc.patch_user_settings(u(1), &too_big).await.is_err());
    assert!(f.svc.patch_user_settings(u(1), &json!([1])).await.is_err());
    assert_eq!(f.svc.user_settings(u(1)).await.unwrap().version, 2);
}

#[tokio::test]
async fn privacy_updates_are_partial_and_drive_dm_access() {
    use super::user_settings::PrivacyUpdate;
    let f = fx();
    let p = f.svc.privacy(u(1)).await.unwrap();
    assert!(p.friend_requests.everyone && p.dms.message_requests);

    let p = f
        .svc
        .update_privacy(
            u(1),
            PrivacyUpdate {
                friend_everyone: Some(false),
                friend_of_friends: Some(false),
                allow_server_member_dms: Some(false),
                ..PrivacyUpdate::default()
            },
        )
        .await
        .unwrap();
    assert!(!p.friend_requests.everyone && !p.friend_requests.friends_of_friends);
    assert!(p.friend_requests.server_members, "untouched field stays");
    assert!(!p.dms.allow_server_member_dms && p.dms.message_requests);
    // Persisted, and it affects who may send a friend request.
    assert_eq!(f.svc.privacy(u(1)).await.unwrap(), p);
    f.svc
        .repo()
        .store
        .lock()
        .unwrap()
        .shared_guilds
        .insert((u(1), u(2)));
    assert!(f.svc.send_friend_request(u(2), u(1)).await.is_ok());
    assert!(matches!(
        f.svc.send_friend_request(u(3), u(1)).await,
        Err(SocialError::RequestsNotAccepted)
    ));
}

#[tokio::test]
async fn global_default_level_sits_between_guild_setting_and_guild_default() {
    let f = fx();
    let (g, ch) = (sr_core::GuildId::new(7), ChannelId::new(70));
    let resolve = |guild_default| {
        f.svc
            .resolve_notifications(u(1), Some(g), ch, None, guild_default)
    };
    assert_eq!(
        resolve(NotificationLevel::All).await.unwrap().level,
        NotificationLevel::All
    );
    f.svc
        .set_default_notification_level(u(1), Some(NotificationLevel::OnlyMentions))
        .await
        .unwrap();
    assert_eq!(
        resolve(NotificationLevel::All).await.unwrap().level,
        NotificationLevel::OnlyMentions
    );
    // An explicit per-guild choice wins over the global default.
    f.svc
        .update_scope(
            u(1),
            Some(g),
            &ScopeUpdate {
                level: Some(Some(NotificationLevel::Nothing)),
                ..ScopeUpdate::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        resolve(NotificationLevel::All).await.unwrap().level,
        NotificationLevel::Nothing
    );
    // The DM scope ignores the global default.
    assert_eq!(
        f.svc
            .resolve_notifications(u(1), None, ch, None, NotificationLevel::All)
            .await
            .unwrap()
            .level,
        NotificationLevel::All
    );
    f.svc
        .set_default_notification_level(u(1), None)
        .await
        .unwrap();
    assert_eq!(f.svc.default_notification_level(u(1)).await.unwrap(), None);
}

#[tokio::test]
async fn profile_decoration_effect_and_theme_are_validated() {
    let f = fx();
    f.svc_repo().store.lock().unwrap().profiles.insert(
        u(1),
        Profile {
            user: u(1),
            ..Profile::default()
        },
    );
    let p = f
        .svc
        .edit_profile(
            u(1),
            &ProfileEdit {
                profile_effect: Some(Some("aurora".into())),
                theme_colors: Some(Some([0x00AB_CDEF, 0x0012_3456])),
                ..ProfileEdit::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(p.profile_effect.as_deref(), Some("aurora"));
    assert_eq!(p.theme_colors, Some([0x00AB_CDEF, 0x0012_3456]));

    // The catalogue ships no decorations, and unknown effects are refused (G-48).
    for (field_deco, bad) in [(true, "neon-halo"), (false, "snow-2")] {
        let edit = if field_deco {
            ProfileEdit {
                avatar_decoration: Some(Some(bad.to_owned())),
                ..ProfileEdit::default()
            }
        } else {
            ProfileEdit {
                profile_effect: Some(Some(bad.to_owned())),
                ..ProfileEdit::default()
            }
        };
        assert!(f.svc.edit_profile(u(1), &edit).await.is_err(), "{bad}");
    }
    for bad in ["Neon Halo", "", "x/y", &"a".repeat(49)] {
        let err = f
            .svc
            .edit_profile(
                u(1),
                &ProfileEdit {
                    avatar_decoration: Some(Some(bad.to_owned())),
                    ..ProfileEdit::default()
                },
            )
            .await;
        assert!(err.is_err(), "{bad:?} must be refused");
    }
    // A colour wider than 24 bits is refused and the stored theme is unchanged.
    let err = f
        .svc
        .edit_profile(
            u(1),
            &ProfileEdit {
                theme_colors: Some(Some([0x0100_0000, 0])),
                ..ProfileEdit::default()
            },
        )
        .await;
    assert!(err.is_err());
    // `null` clears each of them.
    let cleared = f
        .svc
        .edit_profile(
            u(1),
            &ProfileEdit {
                avatar_decoration: Some(None),
                profile_effect: Some(None),
                theme_colors: Some(None),
                ..ProfileEdit::default()
            },
        )
        .await
        .unwrap();
    assert!(
        cleared.avatar_decoration.is_none()
            && cleared.profile_effect.is_none()
            && cleared.theme_colors.is_none()
    );
}

// ---- group DM invites (G-47) ----

#[tokio::test]
async fn group_invites_are_owner_only_expire_and_add_the_joiner() {
    let f = fx();
    befriend(&f, 1, 2).await;
    befriend(&f, 1, 3).await;
    let group = f
        .svc
        .create_group_dm(u(1), &[u(2), u(3)], Some("crew"))
        .await
        .unwrap();
    // A 1:1 DM has no invites; the owner makes one, members cannot.
    let dm = f.svc.open_dm(u(1), u(2)).await.unwrap().channel;
    assert!(matches!(
        f.svc.find_group(u(1), dm.id).await,
        Err(SocialError::NotGroupDm)
    ));
    assert!(
        f.svc
            .find_group(u(1), ChannelId::new(999_999))
            .await
            .unwrap()
            .is_none(),
        "not a DM channel at all"
    );
    assert!(matches!(
        f.svc
            .create_group_invite(u(2), group.id, "memberc0".into())
            .await,
        Err(SocialError::NotOwner)
    ));
    assert!(matches!(
        f.svc
            .create_group_invite(u(9), group.id, "outsider".into())
            .await,
        Err(SocialError::NotParticipant)
    ));
    let inv = f
        .svc
        .create_group_invite(u(1), group.id, "abcd1234".into())
        .await
        .unwrap();
    assert_eq!(inv.expires_at_ms - inv.created_at_ms, 24 * 60 * 60 * 1000);
    assert_eq!(
        f.svc
            .list_group_invites(u(1), group.id)
            .await
            .unwrap()
            .len(),
        1
    );

    // A stranger joins; joining twice changes nothing and burns nothing.
    let (joined, added) = f.svc.join_group_invite(u(8), "abcd1234").await.unwrap();
    assert!(added && joined.has(u(8)) && joined.recipients.len() == 4);
    let (_, again) = f.svc.join_group_invite(u(8), "abcd1234").await.unwrap();
    assert!(!again);
    assert!(f.svc.join_group_invite(u(9), "abcd1234").await.is_ok());

    // Revoking stops new joins; only the owner may.
    assert!(matches!(
        f.svc.revoke_group_invite(u(2), "abcd1234").await,
        Err(SocialError::NotOwner)
    ));
    f.svc.revoke_group_invite(u(1), "abcd1234").await.unwrap();
    assert!(matches!(
        f.svc.join_group_invite(u(10), "abcd1234").await,
        Err(SocialError::UnknownInvite)
    ));
    assert!(
        f.svc
            .list_group_invites(u(1), group.id)
            .await
            .unwrap()
            .is_empty()
    );

    // Expiry after 24 hours; an unknown code is just unknown.
    let inv = f
        .svc
        .create_group_invite(u(1), group.id, "expiring1".into())
        .await
        .unwrap();
    assert!(f.svc.group_invite(&inv.code).await.is_ok());
    f.clock.advance_ms(24 * 60 * 60 * 1000 + 1);
    assert!(matches!(
        f.svc.group_invite(&inv.code).await,
        Err(SocialError::UnknownInvite)
    ));
    assert!(matches!(
        f.svc.group_invite("nosuchcode").await,
        Err(SocialError::UnknownInvite)
    ));
}

#[tokio::test]
async fn group_invites_respect_the_group_size_cap() {
    let f = fx_with(SocialLimits {
        max_group_size: 4,
        ..SocialLimits::default()
    });
    befriend(&f, 1, 2).await;
    befriend(&f, 1, 3).await;
    let group = f
        .svc
        .create_group_dm(u(1), &[u(2), u(3)], None)
        .await
        .unwrap();
    f.svc
        .create_group_invite(u(1), group.id, "capcode1".into())
        .await
        .unwrap();
    assert!(f.svc.join_group_invite(u(5), "capcode1").await.is_ok());
    assert!(matches!(
        f.svc.join_group_invite(u(6), "capcode1").await,
        Err(SocialError::MaxGroupSize)
    ));
}

// ---- Server Protection (G-95, D-224) ----

#[test]
fn protection_only_tightens_and_names_the_reason() {
    use super::dm::{RequestReason, SharedProtection, dm_access_protected};
    let strangers = PairState::default();
    let friends = PairState {
        mine: Some(K::Friend),
        theirs: Some(K::Friend),
        ..PairState::default()
    };
    let open = DmPrivacy {
        message_requests: false,
        ..DmPrivacy::default()
    };
    let none = SharedProtection::default();
    let filter = SharedProtection {
        shared: 2,
        spam_filter_all: true,
        ..none
    };
    let young = SharedProtection {
        shared: 1,
        warn_new_account_days: 14,
        ..none
    };
    // No setting: the recipient's own choice stands.
    assert_eq!(
        dm_access_protected(strangers, &open, true, &none, 3),
        (DmAccess::Allowed, None)
    );
    // Every shared server filters: a stranger lands in requests even when the recipient is open.
    assert_eq!(
        dm_access_protected(strangers, &open, true, &filter, 400),
        (DmAccess::Request, Some(RequestReason::ServerFilter))
    );
    // A young sender is marked new_account, and wins over the filter.
    assert_eq!(
        dm_access_protected(strangers, &open, true, &young, 3),
        (DmAccess::Request, Some(RequestReason::NewAccount))
    );
    assert_eq!(
        dm_access_protected(
            strangers,
            &open,
            true,
            &SharedProtection {
                warn_new_account_days: 14,
                ..filter
            },
            3
        ),
        (DmAccess::Request, Some(RequestReason::NewAccount))
    );
    // An old enough account is not affected.
    assert_eq!(
        dm_access_protected(strangers, &open, true, &young, 14),
        (DmAccess::Allowed, None)
    );
    // Friends are never routed; settings never open a closed door.
    assert_eq!(
        dm_access_protected(friends, &open, true, &filter, 0),
        (DmAccess::Allowed, None)
    );
    assert_eq!(
        dm_access_protected(strangers, &open, false, &filter, 0),
        (DmAccess::Denied, None)
    );
}

#[tokio::test]
async fn open_dm_routes_by_protection_and_spoilers_follow_the_media_filter() {
    use super::dm::{RequestReason, SharedProtection};
    let f = fx();
    let repo = f.svc_repo();
    // 2 is open to DMs from members; 1 and 2 share a server that filters DMs.
    repo.store
        .lock()
        .unwrap()
        .shared_guilds
        .insert((u(1), u(2)));
    f.svc
        .update_privacy(
            u(2),
            super::user_settings::PrivacyUpdate {
                message_requests: Some(false),
                // D-230's default (`non_friends`) would route every stranger; this test is about the
                // server Protection settings alone.
                dm_spam_filter: Some(super::safety::SpamFilter::Off),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let plain = f.svc.open_dm(u(1), u(2)).await.unwrap();
    assert_eq!(plain.access, DmAccess::Allowed);
    assert!(
        !f.svc
            .spoiler_attachments(u(1), plain.channel.id)
            .await
            .unwrap()
    );
    repo.store.lock().unwrap().dms.clear();

    repo.store.lock().unwrap().protection.insert(
        (u(1), u(2)),
        SharedProtection {
            shared: 1,
            spam_filter_all: true,
            explicit_media_any: true,
            warn_new_account_days: 0,
        },
    );
    let opened = f.svc.open_dm(u(1), u(2)).await.unwrap();
    assert_eq!(opened.access, DmAccess::Request);
    assert_eq!(opened.reason, Some(RequestReason::ServerFilter));
    let id = opened.channel.id;
    assert_eq!(
        f.svc.request_state(u(2), id).await.unwrap(),
        Some(super::dm::RequestState::Pending)
    );
    assert_eq!(
        f.svc.request_reason(u(2), id).await.unwrap(),
        Some(RequestReason::ServerFilter)
    );
    // Attachments the stranger sends carry the spoiler flag while the request is pending...
    assert!(f.svc.spoiler_attachments(u(1), id).await.unwrap());
    // ...and no longer once the recipient accepted.
    f.svc
        .respond_to_request(u(2), id, super::dm::RequestState::Accepted)
        .await
        .unwrap();
    assert!(!f.svc.spoiler_attachments(u(1), id).await.unwrap());
}

#[tokio::test]
async fn safety_prefs_update_partially_and_overrides_gate_server_dms() {
    use super::dm::DmAccess;
    use super::safety::MediaFilter;
    use super::user_settings::PrivacyUpdate;
    use std::collections::BTreeMap;
    let f = fx();
    let defaults = f.svc.privacy(u(1)).await.unwrap();
    assert_eq!(defaults.safety.sensitive_media.others, MediaFilter::Blur);
    assert_eq!(
        defaults.safety.dm_spam_filter,
        super::safety::SpamFilter::NonFriends
    );

    let p = f
        .svc
        .update_privacy(
            u(1),
            PrivacyUpdate {
                media_friends: Some(MediaFilter::Block),
                dm_spam_filter: Some(super::safety::SpamFilter::All),
                server_dm_overrides: Some(BTreeMap::from([(70, Some(false)), (71, Some(true))])),
                ..PrivacyUpdate::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(p.safety.sensitive_media.friends, MediaFilter::Block);
    assert_eq!(
        p.safety.sensitive_media.others,
        MediaFilter::Blur,
        "untouched"
    );
    assert_eq!(p.safety.dm_spam_filter, super::safety::SpamFilter::All);
    assert_eq!(p.safety.server_dm_overrides.len(), 2);

    // null removes one override, the rest stays.
    let p = f
        .svc
        .update_privacy(
            u(1),
            PrivacyUpdate {
                server_dm_overrides: Some(BTreeMap::from([(71, None)])),
                ..PrivacyUpdate::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(p.safety.server_dm_overrides.get(&70), Some(&false));
    assert!(!p.safety.server_dm_overrides.contains_key(&71));

    // User 1 allows server DMs by default but not from server 70; user 2 shares only 70.
    let (g70, g71) = (sr_core::GuildId::new(70), sr_core::GuildId::new(71));
    f.svc.repo().add_member(g70, u(1), None, 0);
    f.svc.repo().add_member(g70, u(2), None, 0);
    f.svc.repo().add_member(g71, u(1), None, 0);
    f.svc.repo().add_member(g71, u(3), None, 0);
    assert!(matches!(
        f.svc.open_dm(u(2), u(1)).await,
        Err(SocialError::DmNotAllowed)
    ));
    let opened = f.svc.open_dm(u(3), u(1)).await.unwrap();
    assert_eq!(opened.access, DmAccess::Request);
}

#[tokio::test]
async fn too_many_server_dm_overrides_are_rejected() {
    use super::safety::MAX_SERVER_DM_OVERRIDES;
    use super::user_settings::PrivacyUpdate;
    let f = fx();
    let many = (1..=(MAX_SERVER_DM_OVERRIDES as u64 + 1))
        .map(|g| (g, Some(true)))
        .collect();
    let err = f
        .svc
        .update_privacy(
            u(1),
            PrivacyUpdate {
                server_dm_overrides: Some(many),
                ..PrivacyUpdate::default()
            },
        )
        .await;
    assert!(err.is_err());
    assert!(
        f.svc
            .privacy(u(1))
            .await
            .unwrap()
            .safety
            .server_dm_overrides
            .is_empty()
    );
}
