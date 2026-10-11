//! End-to-end service tests over the in-memory repository, with a manual clock.

use crate::channel_rules::NewChannel;
use crate::domain::{ChannelType, Member, Visibility};
use crate::error::ChatError;
use crate::limits::{ChatLimits, DAY_MS};
use crate::memory::MemoryRepo;
use crate::message_rules::EditMessageInput;
use crate::role_rules::NewRole;
use crate::service::{ChatService, SendMessage};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, Clock, GuildId, ManualClock, SnowflakeGenerator, UserId};
use std::sync::Arc;

const T0: i64 = 1_800_000_000_000;
const OWNER: UserId = UserId::new(1);
const BOB: UserId = UserId::new(2);

struct Fixture {
    svc: ChatService<MemoryRepo>,
    clock: Arc<ManualClock>,
    guild: GuildId,
    general: ChannelId,
}

fn member(guild: GuildId, user: UserId) -> Member {
    Member {
        guild_id: guild,
        user_id: user,
        nick: None,
        role_ids: vec![],
        joined_at_ms: T0,
        communication_disabled_until_ms: None,
        pending: false,
    }
}

async fn fixture() -> Fixture {
    let clock = Arc::new(ManualClock::new(T0));
    let dyn_clock: Arc<dyn Clock> = clock.clone();
    let ids = SnowflakeGenerator::new(dyn_clock.clone(), 1, 1).unwrap();
    let svc = ChatService::new(MemoryRepo::default(), dyn_clock, ids, ChatLimits::default());
    let bp = svc.create_guild(OWNER, "Club House").await.unwrap();
    let general = bp.channels[2].id;
    let guild = bp.guild.id;
    svc.repo_for_tests()
        .store
        .lock()
        .unwrap()
        .members
        .push(member(guild, BOB));
    Fixture {
        svc,
        clock,
        guild,
        general,
    }
}

fn say(s: &str) -> SendMessage {
    SendMessage {
        content: s.into(),
        ..Default::default()
    }
}

#[tokio::test]
async fn guild_creation_persists_defaults() {
    let f = fixture().await;
    let s = f.svc.repo_for_tests().store.lock().unwrap();
    assert_eq!(s.channels.len(), 4);
    assert_eq!(s.roles.len(), 1);
    assert_eq!(s.roles[0].id.get(), f.guild.get());
}

#[tokio::test]
async fn member_can_chat_and_edit_but_not_edit_others() {
    let f = fixture().await;
    let m = f.svc.send_message(BOB, f.general, say("hi")).await.unwrap();
    assert_eq!(m.content, "hi");
    f.clock.advance_ms(1000);
    let plan = f
        .svc
        .edit_message(
            BOB,
            f.general,
            m.id,
            EditMessageInput {
                content: Some("hello".into()),
                flags: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(plan.history_previous_content.as_deref(), Some("hi"));
    let err = f
        .svc
        .edit_message(
            OWNER,
            f.general,
            m.id,
            EditMessageInput {
                content: Some("x".into()),
                flags: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ChatError::NotAuthor));
    let s = f.svc.repo_for_tests().store.lock().unwrap();
    assert_eq!(s.history, vec![(m.id, "hi".to_string())]);
    assert_eq!(s.messages[0].content, "hello");
}

#[tokio::test]
async fn non_member_cannot_read_or_write() {
    let f = fixture().await;
    let err = f
        .svc
        .send_message(UserId::new(99), f.general, say("hi"))
        .await
        .unwrap_err();
    assert!(matches!(err, ChatError::MissingAccess));
    let err = f
        .svc
        .send_message(BOB, ChannelId::new(12345), say("hi"))
        .await
        .unwrap_err();
    assert!(matches!(err, ChatError::UnknownChannel));
}

#[tokio::test]
async fn role_gated_moderation_flow() {
    let f = fixture().await;
    // Bob has no MANAGE_ROLES yet.
    assert!(matches!(
        f.svc.create_role(BOB, f.guild, NewRole::default()).await,
        Err(ChatError::MissingPermissions)
    ));
    // The owner makes a moderator role and gives it to Bob.
    let role = f
        .svc
        .create_role(
            OWNER,
            f.guild,
            NewRole {
                name: Some("Mods".into()),
                permissions: Some(
                    Permissions::MANAGE_MESSAGES
                        | Permissions::PIN_MESSAGES
                        | Permissions::MANAGE_CHANNELS,
                ),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    f.svc
        .repo_for_tests()
        .store
        .lock()
        .unwrap()
        .members
        .iter_mut()
        .find(|m| m.user_id == BOB)
        .unwrap()
        .role_ids
        .push(role.id);

    let m = f
        .svc
        .send_message(OWNER, f.general, say("rules"))
        .await
        .unwrap();
    f.svc.pin_message(BOB, f.general, m.id).await.unwrap();
    assert!(f.svc.repo_for_tests().store.lock().unwrap().messages[0].pinned);
    f.svc.unpin_message(BOB, f.general, m.id).await.unwrap();

    // Bob (a mod) can delete the owner's message; deletion is soft with a 30-day purge.
    let plan = f.svc.delete_message(BOB, f.general, m.id).await.unwrap();
    assert_eq!(plan.visibility, Visibility::UserDeleted);
    assert_eq!(plan.purge_after_ms, Some(T0 + 30 * DAY_MS));
    {
        let s = f.svc.repo_for_tests().store.lock().unwrap();
        assert_eq!(s.messages[0].visibility, Visibility::UserDeleted);
    }
    // Deleting again reports the message as gone.
    assert!(matches!(
        f.svc.delete_message(BOB, f.general, m.id).await,
        Err(ChatError::UnknownMessage)
    ));

    // Bob can create channels but cannot delete the role above... he is below none; the
    // Mods role is his own top role, so he cannot manage it.
    let ch = f
        .svc
        .create_channel(
            BOB,
            f.guild,
            NewChannel::new(ChannelType::Text, "Off Topic"),
        )
        .await
        .unwrap();
    assert_eq!(ch.name.as_deref(), Some("off-topic"));
    assert!(matches!(
        f.svc.delete_role(BOB, f.guild, role.id).await,
        Err(ChatError::MissingPermissions)
    ));
    f.svc.delete_role(OWNER, f.guild, role.id).await.unwrap();
    let s = f.svc.repo_for_tests().store.lock().unwrap();
    assert!(s.members.iter().all(|m| m.role_ids.is_empty()));
}

#[tokio::test]
async fn slowmode_uses_injected_clock() {
    let f = fixture().await;
    {
        let mut s = f.svc.repo_for_tests().store.lock().unwrap();
        s.channels.get_mut(&f.general).unwrap().rate_limit_per_user = 10;
    }
    f.svc
        .send_message(BOB, f.general, say("one"))
        .await
        .unwrap();
    f.clock.advance_ms(4_000);
    match f.svc.send_message(BOB, f.general, say("two")).await {
        Err(ChatError::Slowmode { retry_after_ms }) => assert_eq!(retry_after_ms, 6_000),
        other => panic!("expected slowmode, got {other:?}"),
    }
    f.clock.advance_ms(6_000);
    f.svc
        .send_message(BOB, f.general, say("two"))
        .await
        .unwrap();
    // The owner bypasses slowmode.
    f.svc
        .send_message(OWNER, f.general, say("a"))
        .await
        .unwrap();
    f.svc
        .send_message(OWNER, f.general, say("b"))
        .await
        .unwrap();
}

#[tokio::test]
async fn bulk_delete_respects_age_via_clock() {
    let f = fixture().await;
    let a = f
        .svc
        .send_message(OWNER, f.general, say("a"))
        .await
        .unwrap();
    let b = f
        .svc
        .send_message(OWNER, f.general, say("b"))
        .await
        .unwrap();
    f.clock.advance_ms(13 * DAY_MS);
    let plans = f
        .svc
        .bulk_delete(OWNER, f.general, &[a.id, b.id])
        .await
        .unwrap();
    assert_eq!(plans.len(), 2);

    let c = f
        .svc
        .send_message(OWNER, f.general, say("c"))
        .await
        .unwrap();
    let d = f
        .svc
        .send_message(OWNER, f.general, say("d"))
        .await
        .unwrap();
    f.clock.advance_ms(15 * DAY_MS);
    assert!(matches!(
        f.svc.bulk_delete(OWNER, f.general, &[c.id, d.id]).await,
        Err(ChatError::Invalid {
            code: "MESSAGE_TOO_OLD",
            ..
        })
    ));
}

#[tokio::test]
async fn dm_requires_recipient_and_has_no_everyone_pings() {
    let f = fixture().await;
    let dm = ChannelId::new(777);
    {
        let mut s = f.svc.repo_for_tests().store.lock().unwrap();
        s.channels
            .insert(dm, crate::domain::Channel::new(dm, None, ChannelType::Dm));
        s.recipients.insert((dm, OWNER));
        s.recipients.insert((dm, BOB));
    }
    let m = f
        .svc
        .send_message(BOB, dm, say("@everyone psst"))
        .await
        .unwrap();
    assert!(!m.mention_everyone);
    assert_eq!(m.guild_id, None);
    assert!(matches!(
        f.svc.send_message(UserId::new(50), dm, say("x")).await,
        Err(ChatError::MissingAccess)
    ));
    // Non-managers cannot bulk delete in a DM.
    assert!(f.svc.bulk_delete(BOB, dm, &[m.id, m.id]).await.is_err());
}

#[tokio::test]
async fn ids_are_unique_and_increasing() {
    let f = fixture().await;
    let a = f.svc.send_message(BOB, f.general, say("a")).await.unwrap();
    let b = f.svc.send_message(BOB, f.general, say("b")).await.unwrap();
    assert!(b.id > a.id);
}
