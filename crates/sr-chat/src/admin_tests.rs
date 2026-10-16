//! Service tests for guild administration over the in-memory repository.

use crate::admin_repo::MemberFilter;
use crate::admin_rules::{ChannelMove, GuildPatch, MemberPatch};
use crate::channel_rules::ChannelPatch;
use crate::domain::Member;
use crate::error::ChatError;
use crate::limits::ChatLimits;
use crate::memory::MemoryRepo;
use crate::role_rules::{NewRole, RolePatch};
use crate::service::{ChatService, SendMessage};
use sr_core::permissions::{Overwrite, OverwriteKind, Permissions};
use sr_core::{ChannelId, Clock, GuildId, ManualClock, RoleId, SnowflakeGenerator, UserId};
use std::sync::Arc;

const T0: i64 = 1_800_000_000_000;
const OWNER: UserId = UserId::new(1);
const BOB: UserId = UserId::new(2);
const CAROL: UserId = UserId::new(3);

struct Fx {
    svc: ChatService<MemoryRepo>,
    guild: GuildId,
    general: ChannelId,
    text_cat: ChannelId,
    voice_cat: ChannelId,
}

async fn fx() -> Fx {
    let clock: Arc<dyn Clock> = Arc::new(ManualClock::new(T0));
    let ids = SnowflakeGenerator::new(clock.clone(), 1, 1).unwrap();
    let svc = ChatService::new(MemoryRepo::default(), clock, ids, ChatLimits::default());
    let bp = svc.create_guild(OWNER, "Club House").await.unwrap();
    for u in [BOB, CAROL] {
        svc.repo_for_tests()
            .store
            .lock()
            .unwrap()
            .members
            .push(Member {
                guild_id: bp.guild.id,
                user_id: u,
                nick: None,
                role_ids: vec![],
                joined_at_ms: T0,
                communication_disabled_until_ms: None,
                pending: false,
                avatar_key: None,
                banner_key: None,
                bio: None,
                extras: Default::default(),
            });
    }
    Fx {
        svc,
        guild: bp.guild.id,
        general: bp.channels[2].id,
        text_cat: bp.channels[0].id,
        voice_cat: bp.channels[1].id,
    }
}

async fn role_with(f: &Fx, perms: Permissions) -> RoleId {
    f.svc
        .create_role(
            OWNER,
            f.guild,
            NewRole {
                name: Some("staff".into()),
                permissions: Some(perms),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .id
}

#[tokio::test]
async fn guild_edit_delete_and_listing() {
    let f = fx().await;
    let patch = || GuildPatch {
        name: Some("Renamed".into()),
        ..Default::default()
    };
    assert!(matches!(
        f.svc.edit_guild(BOB, f.guild, patch()).await,
        Err(ChatError::MissingPermissions)
    ));
    let r = role_with(&f, Permissions::MANAGE_GUILD).await;
    f.svc
        .assign_role(OWNER, f.guild, BOB, r, true)
        .await
        .unwrap();
    assert_eq!(
        f.svc.edit_guild(BOB, f.guild, patch()).await.unwrap().name,
        "Renamed"
    );
    assert!(matches!(
        f.svc.get_guild(UserId::new(99), f.guild).await,
        Err(ChatError::MissingAccess)
    ));
    let mine = f.svc.list_my_guilds(BOB).await.unwrap();
    assert_eq!(mine.len(), 1);
    assert!(mine[0].1.contains(Permissions::MANAGE_GUILD));
    f.svc.leave_guild(CAROL, f.guild).await.unwrap();
    assert!(f.svc.leave_guild(OWNER, f.guild).await.is_err());
    assert!(f.svc.delete_guild(BOB, f.guild).await.is_err());
    f.svc.delete_guild(OWNER, f.guild).await.unwrap();
    assert!(matches!(
        f.svc.get_guild(OWNER, f.guild).await,
        Err(ChatError::UnknownGuild)
    ));
}

#[tokio::test]
async fn ownership_transfer() {
    let f = fx().await;
    let to_bob = GuildPatch {
        owner_id: Some(BOB),
        ..Default::default()
    };
    assert!(
        f.svc
            .edit_guild(BOB, f.guild, to_bob.clone())
            .await
            .is_err()
    );
    let g = f.svc.edit_guild(OWNER, f.guild, to_bob).await.unwrap();
    assert_eq!(g.owner_id, BOB);
    assert!(f.svc.delete_guild(OWNER, f.guild).await.is_err());
}

#[tokio::test]
async fn channel_edit_move_and_delete() {
    let f = fx().await;
    let a = f
        .svc
        .edit_channel(
            OWNER,
            f.general,
            ChannelPatch {
                name: Some("Big News".into()),
                ..Default::default()
            },
            ChannelMove {
                parent_id: Some(Some(f.voice_cat)),
                position: Some(5),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(a.channel.name.as_deref(), Some("big-news"));
    assert_eq!(a.channel.parent_id, Some(f.voice_cat));
    assert_eq!(a.channel.position, 5);
    assert!(matches!(
        f.svc
            .edit_channel(
                BOB,
                f.general,
                ChannelPatch::default(),
                ChannelMove::default()
            )
            .await,
        Err(ChatError::MissingPermissions)
    ));
    let mv = ChannelMove {
        position: Some(9),
        ..Default::default()
    };
    let moved = f
        .svc
        .reorder_channels(OWNER, f.guild, vec![(f.text_cat, mv)])
        .await
        .unwrap();
    assert_eq!(moved[0].channel.position, 9);
    f.svc.delete_channel(OWNER, f.voice_cat).await.unwrap();
    let after = f.svc.get_channel_view(OWNER, f.general).await.unwrap();
    assert_eq!(after.channel.parent_id, None);
}

#[tokio::test]
async fn overwrites_hide_channels_and_sync_with_category() {
    let f = fx().await;
    let deny = Overwrite {
        id: f.guild.snowflake(),
        kind: OverwriteKind::Role,
        allow: Permissions::empty(),
        deny: Permissions::VIEW_CHANNEL,
    };
    f.svc.set_overwrite(OWNER, f.text_cat, deny).await.unwrap();
    let lock = ChannelMove {
        parent_id: Some(Some(f.text_cat)),
        lock_permissions: true,
        ..Default::default()
    };
    f.svc
        .reorder_channels(OWNER, f.guild, vec![(f.general, lock)])
        .await
        .unwrap();
    let general = f.svc.get_channel_view(OWNER, f.general).await.unwrap();
    assert_eq!(general.overwrites, vec![deny]);
    assert!(matches!(
        f.svc.get_channel_view(BOB, f.general).await,
        Err(ChatError::MissingAccess)
    ));
    let visible = f.svc.list_channels(BOB, f.guild).await.unwrap();
    assert!(visible.iter().all(|c| c.channel.id != f.general));
    assert!(f.svc.set_overwrite(BOB, f.general, deny).await.is_err());
    f.svc
        .delete_overwrite(OWNER, f.general, f.guild.snowflake())
        .await
        .unwrap();
    assert!(f.svc.get_channel_view(BOB, f.general).await.is_ok());
    let both = Overwrite {
        allow: Permissions::SEND_MESSAGES,
        deny: Permissions::SEND_MESSAGES,
        ..deny
    };
    assert!(f.svc.set_overwrite(OWNER, f.general, both).await.is_err());
}

#[tokio::test]
async fn roles_edit_reorder_assign_and_hierarchy() {
    let f = fx().await;
    let low = role_with(&f, Permissions::empty()).await;
    let high = role_with(&f, Permissions::MANAGE_ROLES).await;
    let roles = f.svc.list_roles(OWNER, f.guild).await.unwrap();
    let pos = |id: RoleId| roles.iter().find(|r| r.id == id).unwrap().position;
    assert!(pos(high) < pos(low), "new roles land at position 1");
    f.svc
        .assign_role(OWNER, f.guild, BOB, low, true)
        .await
        .unwrap();
    f.svc
        .assign_role(OWNER, f.guild, BOB, high, true)
        .await
        .unwrap();
    // Bob holds the roles at positions 1 and 2, so his top is 2: he outranks only position 1.
    assert!(
        f.svc
            .assign_role(BOB, f.guild, CAROL, low, true)
            .await
            .is_err()
    );
    assert!(
        f.svc
            .assign_role(BOB, f.guild, CAROL, high, true)
            .await
            .is_ok()
    );
    let carol = f.svc.get_member_view(OWNER, f.guild, CAROL).await.unwrap();
    assert_eq!(carol.role_ids, vec![high]);
    f.svc
        .assign_role(BOB, f.guild, CAROL, high, false)
        .await
        .unwrap();
    let r = f
        .svc
        .edit_role(
            OWNER,
            f.guild,
            low,
            RolePatch {
                name: Some("renamed".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(r.name, "renamed");
    let changed = f
        .svc
        .reorder_roles(OWNER, f.guild, vec![(low, 1)])
        .await
        .unwrap();
    assert!(!changed.is_empty());
    let roles = f.svc.list_roles(OWNER, f.guild).await.unwrap();
    assert_eq!(roles.iter().find(|r| r.id == low).unwrap().position, 1);
    f.svc.delete_role(OWNER, f.guild, low).await.unwrap();
    assert!(matches!(
        f.svc.assign_role(OWNER, f.guild, BOB, low, true).await,
        Err(ChatError::UnknownRole)
    ));
}

#[tokio::test]
async fn member_edit_kick_and_ban() {
    let f = fx().await;
    let r = role_with(&f, Permissions::KICK_MEMBERS | Permissions::BAN_MEMBERS).await;
    f.svc
        .assign_role(OWNER, f.guild, BOB, r, true)
        .await
        .unwrap();

    let nick = |n: &str| MemberPatch {
        nick: Some(Some(n.into())),
        ..Default::default()
    };
    let m = f
        .svc
        .edit_member(CAROL, f.guild, CAROL, nick("Caz"))
        .await
        .unwrap();
    assert_eq!(m.nick.as_deref(), Some("Caz"));
    assert!(
        f.svc
            .edit_member(CAROL, f.guild, BOB, nick("x"))
            .await
            .is_err()
    );

    let listed = f
        .svc
        .list_members(OWNER, f.guild, MemberFilter::default(), None, 2)
        .await
        .unwrap();
    assert_eq!(listed.len(), 2);
    let next = f
        .svc
        .list_members(
            OWNER,
            f.guild,
            MemberFilter::default(),
            Some(listed[1].user_id),
            10,
        )
        .await
        .unwrap();
    assert_eq!(next.len(), 1);

    assert!(f.svc.kick_member(BOB, f.guild, OWNER).await.is_err());
    assert!(f.svc.kick_member(CAROL, f.guild, BOB).await.is_err());
    f.svc.kick_member(BOB, f.guild, CAROL).await.unwrap();
    assert!(matches!(
        f.svc.get_member_view(OWNER, f.guild, CAROL).await,
        Err(ChatError::UnknownMember)
    ));

    let stranger = UserId::new(50);
    let (ban, was_member) = f
        .svc
        .ban_member(BOB, f.guild, stranger, Some(" spam ".into()))
        .await
        .unwrap();
    assert!(!was_member);
    assert_eq!(ban.reason.as_deref(), Some("spam"));
    let bans = f.svc.list_bans(BOB, f.guild, None, None, 10).await.unwrap();
    assert_eq!(bans.len(), 1);
    assert_eq!(f.svc.get_ban(BOB, f.guild, stranger).await.unwrap(), ban);
    assert!(
        f.svc
            .list_bans(UserId::new(99), f.guild, None, None, 10)
            .await
            .is_err()
    );
    f.svc.unban(BOB, f.guild, stranger).await.unwrap();
    assert!(matches!(
        f.svc.unban(BOB, f.guild, stranger).await,
        Err(ChatError::UnknownBan)
    ));
    let (_, was_member) = f.svc.ban_member(OWNER, f.guild, BOB, None).await.unwrap();
    assert!(was_member);
    assert!(f.svc.get_guild(BOB, f.guild).await.is_err());
}

#[tokio::test]
async fn timeouts_follow_hierarchy() {
    let f = fx().await;
    let r = role_with(&f, Permissions::MODERATE_MEMBERS).await;
    f.svc
        .assign_role(OWNER, f.guild, BOB, r, true)
        .await
        .unwrap();
    let until = T0 + 60_000;
    let patch = MemberPatch {
        communication_disabled_until_ms: Some(Some(until)),
        ..Default::default()
    };
    let m = f
        .svc
        .edit_member(BOB, f.guild, CAROL, patch.clone())
        .await
        .unwrap();
    assert_eq!(m.communication_disabled_until_ms, Some(until));
    assert!(f.svc.edit_member(BOB, f.guild, OWNER, patch).await.is_err());
}

// ---- member search, role members, prune ------------------------------------------

const DAY: i64 = 86_400_000;
const DAVE: UserId = UserId::new(4);
const ERIN: UserId = UserId::new(5);

fn push_member(f: &Fx, user: UserId, nick: Option<&str>, joined: i64, roles: Vec<RoleId>) {
    f.svc
        .repo_for_tests()
        .store
        .lock()
        .unwrap()
        .members
        .push(Member {
            guild_id: f.guild,
            user_id: user,
            nick: nick.map(str::to_owned),
            role_ids: roles,
            joined_at_ms: joined,
            communication_disabled_until_ms: None,
            pending: false,
            avatar_key: None,
            banner_key: None,
            bio: None,
            extras: Default::default(),
        });
}

fn name(f: &Fx, user: UserId, username: &str, display: Option<&str>) {
    f.svc.repo_for_tests().store.lock().unwrap().users.insert(
        user,
        crate::repo::UserSummary {
            id: user,
            username: username.into(),
            display_name: display.map(str::to_owned),
            avatar_key: None,
            banner_key: None,
            bot: false,
            primary_guild: None,
            patron: false,
        },
    );
}

async fn ids(f: &Fx, filter: MemberFilter) -> Vec<u64> {
    f.svc
        .list_members(OWNER, f.guild, filter, None, 100)
        .await
        .unwrap()
        .iter()
        .map(|m| m.user_id.get())
        .collect()
}

#[tokio::test]
async fn member_search_matches_nick_username_and_display_name_and_filters_by_role() {
    let f = fx().await;
    name(&f, OWNER, "olivia", None);
    name(&f, BOB, "bobby_b", Some("Robert"));
    name(&f, CAROL, "carol", Some("Caroline 100%"));
    push_member(&f, DAVE, Some("The Dave"), T0, vec![]);
    name(&f, DAVE, "d4ve", None);
    let q = |s: &str| MemberFilter {
        query: Some(s.into()),
        role: None,
    };
    assert_eq!(ids(&f, MemberFilter::default()).await, [1, 2, 3, 4]);
    // Username, display name and nickname, case-insensitive, substring.
    assert_eq!(ids(&f, q("BOBBY")).await, [2]);
    assert_eq!(ids(&f, q("robert")).await, [2]);
    assert_eq!(ids(&f, q("the dave")).await, [4]);
    assert_eq!(ids(&f, q("o")).await, [1, 2, 3]);
    assert_eq!(ids(&f, q("zzz")).await, Vec::<u64>::new());
    // Blank searches are no filter at all.
    assert_eq!(ids(&f, q("   ")).await, [1, 2, 3, 4]);
    assert!(
        f.svc
            .list_members(OWNER, f.guild, q(&"x".repeat(65)), None, 10)
            .await
            .is_err()
    );
    // Paging still works under a filter.
    let first = f
        .svc
        .list_members(OWNER, f.guild, q("o"), None, 2)
        .await
        .unwrap();
    assert_eq!(first.len(), 2);
    let rest = f
        .svc
        .list_members(OWNER, f.guild, q("o"), Some(first[1].user_id), 10)
        .await
        .unwrap();
    assert_eq!(
        rest.iter().map(|m| m.user_id.get()).collect::<Vec<_>>(),
        [3]
    );

    // Role filter and role members.
    let staff = role_with(&f, Permissions::empty()).await;
    f.svc
        .assign_role(OWNER, f.guild, BOB, staff, true)
        .await
        .unwrap();
    let by_role = MemberFilter {
        query: None,
        role: Some(staff),
    };
    assert_eq!(ids(&f, by_role.clone()).await, [2]);
    let both = MemberFilter {
        query: Some("carol".into()),
        role: Some(staff),
    };
    assert_eq!(ids(&f, both).await, Vec::<u64>::new());
    let m = f
        .svc
        .list_role_members(CAROL, f.guild, staff, None, 10)
        .await
        .unwrap();
    assert_eq!(m.len(), 1);
    // @everyone matches all; an unknown role is an error; outsiders cannot list.
    let everyone = RoleId::new(f.guild.get());
    assert_eq!(
        ids(
            &f,
            MemberFilter {
                query: None,
                role: Some(everyone)
            }
        )
        .await
        .len(),
        4
    );
    assert!(matches!(
        f.svc
            .list_role_members(OWNER, f.guild, RoleId::new(424_242), None, 10)
            .await,
        Err(ChatError::UnknownRole)
    ));
    assert!(matches!(
        f.svc
            .list_role_members(UserId::new(99), f.guild, staff, None, 10)
            .await,
        Err(ChatError::MissingAccess)
    ));
}

#[tokio::test]
async fn prune_removes_only_inactive_role_free_members_below_the_actor() {
    let f = fx().await;
    let old = T0 - 40 * DAY;
    // fx() members joined at T0; make Bob and Carol old, add Dave (old, has a role),
    // Erin (new) and a moderator who prunes.
    for m in f
        .svc
        .repo_for_tests()
        .store
        .lock()
        .unwrap()
        .members
        .iter_mut()
    {
        m.joined_at_ms = old;
    }
    let vip = role_with(&f, Permissions::empty()).await;
    let kicker = role_with(&f, Permissions::KICK_MEMBERS).await;
    push_member(&f, DAVE, None, old, vec![vip]);
    push_member(&f, ERIN, None, T0 - DAY, vec![]);
    // Carol is active: she wrote a message.
    f.svc
        .send_message(CAROL, f.general, say("still here"))
        .await
        .unwrap();
    f.svc
        .assign_role(OWNER, f.guild, BOB, kicker, true)
        .await
        .unwrap();

    // Pruning needs KICK_MEMBERS.
    assert!(matches!(
        f.svc.prune_preview(CAROL, f.guild, 7, &[]).await,
        Err(ChatError::MissingPermissions)
    ));
    // Nobody qualifies: Bob holds a role, Carol is active, Dave has a role, Erin is new
    // and the owner is exempt.
    assert_eq!(
        f.svc.prune_preview(OWNER, f.guild, 7, &[]).await.unwrap(),
        0
    );
    // Including a role makes its holders candidates.
    assert_eq!(
        f.svc
            .prune_preview(OWNER, f.guild, 7, &[vip])
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        f.svc
            .prune_preview(OWNER, f.guild, 7, &[vip, kicker])
            .await
            .unwrap(),
        2
    );
    // Bad input.
    assert!(f.svc.prune_preview(OWNER, f.guild, 0, &[]).await.is_err());
    assert!(f.svc.prune_preview(OWNER, f.guild, 31, &[]).await.is_err());
    assert!(
        f.svc
            .prune_preview(OWNER, f.guild, 7, &[RoleId::new(999)])
            .await
            .is_err()
    );

    // A moderator cannot prune members at or above their own rank, even when included, and
    // never counts himself.
    let pos = |r: RoleId| {
        let roles = f.svc.repo_for_tests().store.lock().unwrap().roles.clone();
        roles.iter().find(|x| x.id == r).unwrap().position
    };
    let expect = usize::from(pos(kicker) > pos(vip));
    assert_eq!(
        f.svc
            .prune_preview(BOB, f.guild, 7, &[vip, kicker])
            .await
            .unwrap(),
        expect
    );

    // Execute: removes exactly the previewed members and nobody else.
    let removed = f.svc.prune(OWNER, f.guild, 7, &[vip]).await.unwrap();
    assert_eq!(removed, vec![DAVE]);
    assert_eq!(ids(&f, MemberFilter::default()).await, [1, 2, 3, 5]);
    assert_eq!(
        f.svc.prune(OWNER, f.guild, 7, &[vip]).await.unwrap(),
        vec![]
    );
}

fn say(s: &str) -> SendMessage {
    SendMessage {
        content: s.into(),
        ..Default::default()
    }
}

#[tokio::test]
async fn the_system_user_cannot_be_kicked_banned_or_given_roles() {
    // In this fixture the owner has the system user's id, so even they are refused (D-188).
    let f = fx().await;
    let role = role_with(&f, Permissions::empty()).await;
    let system = UserId::new(crate::domain::SYSTEM_USER_ID);
    assert!(matches!(
        f.svc.kick_member(BOB, f.guild, system).await,
        Err(ChatError::MissingPermissions)
    ));
    assert!(matches!(
        f.svc.ban_member(OWNER, f.guild, system, None).await,
        Err(ChatError::MissingPermissions)
    ));
    assert!(matches!(
        f.svc.assign_role(OWNER, f.guild, system, role, true).await,
        Err(ChatError::MissingPermissions)
    ));
}
