//! Service tests over the in-memory repository with a manual clock.

use super::model::{TagInput, channel_flags};
use super::repo::ArchivedFilter;
use super::rules::{ForumPost, StartThread, ThreadPatch};
use super::service::ThreadService;
use crate::channel_rules::NewChannel;
use crate::domain::{ChannelType, Member, MessageType};
use crate::error::ChatError;
use crate::limits::ChatLimits;
use crate::memory::MemoryRepo;
use crate::service::{ChatService, SendMessage};
use sr_core::permissions::{Overwrite, OverwriteKind, Permissions};
use sr_core::{ChannelId, Clock, GuildId, ManualClock, Snowflake, SnowflakeGenerator, UserId};
use std::sync::Arc;

const T0: i64 = 1_800_000_000_000;
const MIN: i64 = 60_000;
const OWNER: UserId = UserId::new(1);
const BOB: UserId = UserId::new(2);
const CAROL: UserId = UserId::new(3);

struct Fx {
    chat: ChatService<MemoryRepo>,
    threads: ThreadService<MemoryRepo>,
    repo: MemoryRepo,
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
        avatar_key: None,
        banner_key: None,
        bio: None,
        extras: Default::default(),
    }
}

async fn fx() -> Fx {
    let clock = Arc::new(ManualClock::new(T0));
    let dyn_clock: Arc<dyn Clock> = clock.clone();
    let repo = MemoryRepo::default();
    let chat = ChatService::new(
        repo.clone(),
        dyn_clock.clone(),
        SnowflakeGenerator::new(dyn_clock.clone(), 1, 1).unwrap(),
        ChatLimits::default(),
    );
    let threads = ThreadService::new(
        repo.clone(),
        dyn_clock.clone(),
        SnowflakeGenerator::new(dyn_clock, 1, 2).unwrap(),
    );
    let bp = chat.create_guild(OWNER, "Club").await.unwrap();
    let guild = bp.guild.id;
    {
        let mut s = repo.store.lock().unwrap();
        s.members.push(member(guild, BOB));
        s.members.push(member(guild, CAROL));
    }
    Fx {
        general: bp.channels[2].id,
        chat,
        threads,
        repo,
        clock,
        guild,
    }
}

fn say(s: &str) -> SendMessage {
    SendMessage {
        content: s.into(),
        ..Default::default()
    }
}

fn start(name: &str) -> StartThread {
    StartThread {
        name: name.into(),
        ..StartThread::default()
    }
}

async fn thread_from_message(f: &Fx, author: UserId, starter: UserId) -> ChannelId {
    let m = f
        .chat
        .send_message(author, f.general, say("kickoff"))
        .await
        .unwrap();
    f.threads
        .start_from_message(starter, f.general, m.id, start("Planning"))
        .await
        .unwrap()
        .view
        .channel
        .id
}

#[tokio::test]
async fn starting_from_a_message_uses_its_id_and_adds_system_messages() {
    let f = fx().await;
    let m = f
        .chat
        .send_message(BOB, f.general, say("lets plan"))
        .await
        .unwrap();
    let out = f
        .threads
        .start_from_message(CAROL, f.general, m.id, start("Planning"))
        .await
        .unwrap();
    let t = &out.view.channel;
    assert_eq!(t.id.get(), m.id.get());
    assert_eq!(t.kind, ChannelType::PublicThread);
    assert_eq!(t.owner_id, Some(CAROL));
    assert_eq!(t.thread.as_ref().unwrap().member_count, 1);
    assert!(out.view.member.is_some());
    let kinds: Vec<_> = out
        .messages
        .iter()
        .map(|m| (m.kind, m.channel_id))
        .collect();
    assert!(kinds.contains(&(MessageType::ThreadStarterMessage, t.id)));
    assert!(kinds.contains(&(MessageType::ThreadCreated, f.general)));
    // One thread per message.
    let again = f
        .threads
        .start_from_message(BOB, f.general, m.id, start("Again"))
        .await;
    assert!(matches!(again, Err(ChatError::ThreadAlreadyCreated)));
    // The thread does not show up as a guild channel.
    let listed = f.chat.list_channels(OWNER, f.guild).await.unwrap();
    assert!(listed.iter().all(|c| !c.channel.kind.is_thread()));
}

#[tokio::test]
async fn thread_messages_use_the_ordinary_send_path_and_bump_counters() {
    let f = fx().await;
    let tid = thread_from_message(&f, BOB, CAROL).await;
    f.clock.advance_ms(MIN);
    let sent = f
        .chat
        .send_message(BOB, tid, say("in thread"))
        .await
        .unwrap();
    assert_eq!(sent.channel_id, tid);
    let out = f.threads.record_message(&sent).await.unwrap().unwrap();
    assert!(out.joined.is_some(), "posting joins the thread");
    let info = out.thread.thread.unwrap();
    assert_eq!(info.message_count, 1);
    assert_eq!(info.member_count, 2);
    assert_eq!(info.last_activity_ms, T0 + MIN);
    // Non-threads are not touched.
    let plain = f.chat.send_message(BOB, f.general, say("x")).await.unwrap();
    assert!(f.threads.record_message(&plain).await.unwrap().is_none());
}

#[tokio::test]
async fn deleting_the_starter_message_keeps_the_thread() {
    let f = fx().await;
    let m = f
        .chat
        .send_message(BOB, f.general, say("topic"))
        .await
        .unwrap();
    f.threads
        .start_from_message(BOB, f.general, m.id, start("T"))
        .await
        .unwrap();
    f.chat.delete_message(BOB, f.general, m.id).await.unwrap();
    let v = f
        .threads
        .view(BOB, ChannelId::new(m.id.get()))
        .await
        .unwrap();
    assert_eq!(v.channel.name.as_deref(), Some("T"));
}

#[tokio::test]
async fn deleting_a_message_lowers_message_count() {
    let f = fx().await;
    let tid = thread_from_message(&f, BOB, BOB).await;
    let a = f.chat.send_message(BOB, tid, say("a")).await.unwrap();
    f.threads.record_message(&a).await.unwrap();
    let b = f.chat.send_message(BOB, tid, say("b")).await.unwrap();
    f.threads.record_message(&b).await.unwrap();
    f.chat.delete_message(BOB, tid, a.id).await.unwrap();
    let v = f.threads.view(BOB, tid).await.unwrap();
    let info = v.channel.thread.unwrap();
    assert_eq!(info.message_count, 1);
    assert_eq!(info.total_message_sent, 2);
}

#[tokio::test]
async fn threads_inherit_view_permission_from_the_parent() {
    let f = fx().await;
    let tid = thread_from_message(&f, OWNER, OWNER).await;
    assert!(f.threads.view(BOB, tid).await.is_ok());
    // @everyone loses VIEW_CHANNEL on the parent: the thread disappears for Bob.
    f.chat
        .set_overwrite(
            OWNER,
            f.general,
            Overwrite {
                id: Snowflake::new(f.guild.get()),
                kind: OverwriteKind::Role,
                allow: Permissions::empty(),
                deny: Permissions::VIEW_CHANNEL,
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        f.threads.view(BOB, tid).await,
        Err(ChatError::MissingAccess)
    ));
    assert!(matches!(
        f.chat.send_message(BOB, tid, say("hi")).await,
        Err(ChatError::MissingAccess)
    ));
    assert!(f.threads.view(OWNER, tid).await.is_ok());
    let active = f.threads.list_active(BOB, f.guild).await.unwrap();
    assert!(active.threads.is_empty());
}

#[tokio::test]
async fn private_threads_are_for_members_and_moderators() {
    let f = fx().await;
    // @everyone may create private threads here.
    let role = f.guild.get();
    f.repo.store.lock().unwrap().roles[0].permissions |= Permissions::CREATE_PRIVATE_THREADS;
    let _ = role;
    let mut input = start("secret");
    input.kind = Some(ChannelType::PrivateThread);
    let out = f
        .threads
        .start_standalone(BOB, f.general, input)
        .await
        .unwrap();
    let tid = out.view.channel.id;
    assert_eq!(out.view.channel.kind, ChannelType::PrivateThread);
    // No "thread created" notice for private threads.
    assert!(
        out.messages
            .iter()
            .all(|m| m.kind != MessageType::ThreadCreated)
    );
    assert!(matches!(
        f.threads.view(CAROL, tid).await,
        Err(ChatError::MissingAccess)
    ));
    assert!(matches!(
        f.chat.send_message(CAROL, tid, say("hi")).await,
        Err(ChatError::MissingAccess)
    ));
    // Not even by joining herself.
    assert!(f.threads.add_member(CAROL, tid, CAROL).await.is_err());
    assert!(
        f.threads
            .list_active(CAROL, f.guild)
            .await
            .unwrap()
            .threads
            .is_empty()
    );
    // The owner of the guild (administrator) sees it.
    assert!(f.threads.view(OWNER, tid).await.is_ok());
    // Bob invites Carol.
    let ch = f.threads.add_member(BOB, tid, CAROL).await.unwrap();
    assert!(ch.changed);
    assert_eq!(ch.thread.thread.unwrap().member_count, 2);
    assert!(f.threads.view(CAROL, tid).await.is_ok());
    assert!(f.chat.send_message(CAROL, tid, say("hi")).await.is_ok());
    assert_eq!(
        f.threads
            .list_active(CAROL, f.guild)
            .await
            .unwrap()
            .threads
            .len(),
        1
    );
    // Removing her takes the access away again.
    f.threads.remove_member(BOB, tid, CAROL).await.unwrap();
    assert!(f.threads.view(CAROL, tid).await.is_err());
}

#[tokio::test]
async fn join_leave_and_member_listing() {
    let f = fx().await;
    let tid = thread_from_message(&f, OWNER, OWNER).await;
    let joined = f.threads.add_member(BOB, tid, BOB).await.unwrap();
    assert!(joined.changed);
    assert_eq!(joined.thread.thread.as_ref().unwrap().member_count, 2);
    let again = f.threads.add_member(BOB, tid, BOB).await.unwrap();
    assert!(!again.changed);
    let list = f.threads.list_members(CAROL, tid, None, 100).await.unwrap();
    assert_eq!(list.len(), 2);
    let page = f
        .threads
        .list_members(CAROL, tid, Some(OWNER), 100)
        .await
        .unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].user_id, BOB);
    // Others can only be removed by moderators.
    assert!(matches!(
        f.threads.remove_member(CAROL, tid, BOB).await,
        Err(ChatError::MissingPermissions)
    ));
    let left = f.threads.remove_member(BOB, tid, BOB).await.unwrap();
    assert!(left.changed);
    assert!(matches!(
        f.threads.get_member(CAROL, tid, BOB).await,
        Err(ChatError::UnknownMember)
    ));
    // The owner of the guild may remove anyone.
    f.threads.add_member(CAROL, tid, CAROL).await.unwrap();
    assert!(
        f.threads
            .remove_member(OWNER, tid, CAROL)
            .await
            .unwrap()
            .changed
    );
    // Strangers to the guild cannot be added.
    let missing = f.threads.add_member(OWNER, tid, UserId::new(99)).await;
    assert!(matches!(missing, Err(ChatError::UnknownMember)));
}

#[tokio::test]
async fn archive_lock_and_unarchive_by_message() {
    let f = fx().await;
    let tid = thread_from_message(&f, BOB, BOB).await;
    // Only the owner or a moderator may close it.
    let archive = ThreadPatch {
        archived: Some(true),
        ..ThreadPatch::default()
    };
    assert!(f.threads.modify(CAROL, tid, archive.clone()).await.is_err());
    let v = f.threads.modify(BOB, tid, archive).await.unwrap();
    assert!(v.channel.thread.unwrap().archived);
    // Active listing hides it; the archived listing shows it.
    assert!(
        f.threads
            .list_active(BOB, f.guild)
            .await
            .unwrap()
            .threads
            .is_empty()
    );
    let arch = f
        .threads
        .list_archived(BOB, f.general, ArchivedFilter::Public, None, 50)
        .await
        .unwrap();
    assert_eq!(arch.threads.len(), 1);
    assert_eq!(arch.members.len(), 1);
    assert!(!arch.has_more);
    // Cannot join while archived.
    assert!(matches!(
        f.threads.add_member(CAROL, tid, CAROL).await,
        Err(ChatError::ThreadArchived)
    ));
    // A message from an allowed member reopens it.
    f.clock.advance_ms(MIN);
    let m = f
        .chat
        .send_message(CAROL, tid, say("revive"))
        .await
        .unwrap();
    let out = f.threads.record_message(&m).await.unwrap().unwrap();
    assert!(out.unarchived);
    assert!(!out.thread.thread.unwrap().archived);
    // Locking needs MANAGE_THREADS and blocks plain members from sending.
    let lock = ThreadPatch {
        locked: Some(true),
        ..ThreadPatch::default()
    };
    assert!(f.threads.modify(BOB, tid, lock.clone()).await.is_err());
    f.threads.modify(OWNER, tid, lock).await.unwrap();
    assert!(matches!(
        f.chat.send_message(CAROL, tid, say("nope")).await,
        Err(ChatError::ThreadLocked)
    ));
    assert!(f.chat.send_message(OWNER, tid, say("mod")).await.is_ok());
}

#[tokio::test]
async fn rename_and_slowmode() {
    let f = fx().await;
    let tid = thread_from_message(&f, BOB, BOB).await;
    let v = f
        .threads
        .modify(
            BOB,
            tid,
            ThreadPatch {
                name: Some("Renamed".into()),
                auto_archive_duration: Some(60),
                ..ThreadPatch::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(v.channel.name.as_deref(), Some("Renamed"));
    assert_eq!(v.channel.thread.unwrap().auto_archive_duration, 60);
    let slow = ThreadPatch {
        rate_limit_per_user: Some(30),
        ..ThreadPatch::default()
    };
    assert!(f.threads.modify(BOB, tid, slow.clone()).await.is_err());
    let v = f.threads.modify(OWNER, tid, slow).await.unwrap();
    assert_eq!(v.channel.rate_limit_per_user, 30);
    // The existing slowmode path applies to thread messages.
    f.chat.send_message(BOB, tid, say("one")).await.unwrap();
    assert!(matches!(
        f.chat.send_message(BOB, tid, say("two")).await,
        Err(ChatError::Slowmode { .. })
    ));
}

#[tokio::test]
async fn idle_threads_auto_archive_and_activity_postpones_it() {
    let f = fx().await;
    let a = thread_from_message(&f, BOB, BOB).await;
    // Second thread with the shortest timer.
    let m = f
        .chat
        .send_message(BOB, f.general, say("second"))
        .await
        .unwrap();
    let mut input = start("Short");
    input.auto_archive_duration = Some(60);
    let b = f
        .threads
        .start_from_message(BOB, f.general, m.id, input)
        .await
        .unwrap()
        .view
        .channel
        .id;
    // Default 1440 minutes for `a`.
    f.clock.advance_ms(59 * MIN);
    assert!(f.threads.archive_idle(10).await.unwrap().is_empty());
    // Activity in `b` restarts its clock.
    let msg = f
        .chat
        .send_message(CAROL, b, say("still here"))
        .await
        .unwrap();
    f.threads.record_message(&msg).await.unwrap();
    f.clock.advance_ms(2 * MIN);
    assert!(f.threads.archive_idle(10).await.unwrap().is_empty());
    f.clock.advance_ms(60 * MIN);
    let archived = f.threads.archive_idle(10).await.unwrap();
    assert_eq!(archived.iter().map(|c| c.id).collect::<Vec<_>>(), vec![b]);
    assert!(archived[0].thread.as_ref().unwrap().archived);
    // `a` runs out a day after its last activity.
    f.clock.advance_ms(24 * 60 * MIN);
    let archived = f.threads.archive_idle(10).await.unwrap();
    assert_eq!(archived.len(), 1);
    assert_eq!(archived[0].id, a);
    assert!(f.threads.archive_idle(10).await.unwrap().is_empty());
}

async fn forum(f: &Fx) -> ChannelId {
    let mut input = NewChannel::new(ChannelType::Forum, "help");
    input.topic = Some("Be nice".into());
    input.forum.available_tags = Some(vec![
        TagInput {
            id: None,
            name: "question".into(),
            moderated: false,
            emoji_id: None,
            emoji_name: Some("?".into()),
        },
        TagInput {
            id: None,
            name: "official".into(),
            moderated: true,
            emoji_id: None,
            emoji_name: None,
        },
    ]);
    input.forum.default_thread_rate_limit = Some(10);
    input.forum.flags = Some(channel_flags::REQUIRE_TAG);
    f.chat
        .create_channel(OWNER, f.guild, input)
        .await
        .unwrap()
        .id
}

#[tokio::test]
async fn forum_channels_store_their_settings() {
    let f = fx().await;
    let fid = forum(&f).await;
    let ch = f.repo.store.lock().unwrap().channels[&fid].clone();
    assert_eq!(ch.forum.available_tags.len(), 2);
    assert!(ch.forum.available_tags.iter().all(|t| t.id != 0));
    assert_eq!(
        ch.flags & channel_flags::REQUIRE_TAG,
        channel_flags::REQUIRE_TAG
    );
    assert_eq!(ch.forum.default_thread_rate_limit, Some(10));
    assert_eq!(ch.topic.as_deref(), Some("Be nice"));
}

#[tokio::test]
async fn forum_posts_need_a_tag_and_hold_the_starter_message() {
    let f = fx().await;
    let fid = forum(&f).await;
    let tags: Vec<u64> = f.repo.store.lock().unwrap().channels[&fid]
        .forum
        .available_tags
        .iter()
        .map(|t| t.id)
        .collect();
    let post = |tags: Vec<u64>| ForumPost {
        name: "How do I start?".into(),
        applied_tags: tags,
        ..ForumPost::default()
    };
    assert!(matches!(
        f.threads.start_forum_post(BOB, fid, post(vec![])).await,
        Err(ChatError::TagRequired)
    ));
    // Moderated tags are refused for members, allowed for moderators.
    assert!(matches!(
        f.threads
            .start_forum_post(BOB, fid, post(vec![tags[1]]))
            .await,
        Err(ChatError::MissingPermissions)
    ));
    let v = f
        .threads
        .start_forum_post(BOB, fid, post(vec![tags[0]]))
        .await
        .unwrap();
    assert_eq!(
        v.channel.thread.as_ref().unwrap().applied_tags,
        vec![tags[0]]
    );
    assert_eq!(v.channel.rate_limit_per_user, 10);
    // The starter message goes through the ordinary path.
    let msg = f
        .chat
        .send_message(BOB, v.channel.id, say("body"))
        .await
        .unwrap();
    assert_eq!(msg.channel_id, v.channel.id);
    // Plain messages in the forum channel itself are refused.
    assert!(f.chat.send_message(BOB, fid, say("direct")).await.is_err());
    // The post is listed in the archived/active views of the forum.
    let active = f.threads.list_active(CAROL, f.guild).await.unwrap();
    assert_eq!(active.threads.len(), 1);
    // The author may delete their post; others may not.
    assert!(f.threads.delete(CAROL, v.channel.id).await.is_err());
    let del = f.threads.delete(BOB, v.channel.id).await.unwrap();
    assert!(del.viewers.contains(&CAROL));
    assert!(f.threads.view(BOB, v.channel.id).await.is_err());
}

#[tokio::test]
async fn pinned_posts_do_not_auto_archive() {
    let f = fx().await;
    let fid = forum(&f).await;
    let tag = f.repo.store.lock().unwrap().channels[&fid]
        .forum
        .available_tags[0]
        .id;
    let v = f
        .threads
        .start_forum_post(
            BOB,
            fid,
            ForumPost {
                name: "pin me".into(),
                applied_tags: vec![tag],
                ..ForumPost::default()
            },
        )
        .await
        .unwrap();
    let pin = ThreadPatch {
        flags: Some(channel_flags::PINNED),
        ..ThreadPatch::default()
    };
    assert!(
        f.threads
            .modify(BOB, v.channel.id, pin.clone())
            .await
            .is_err()
    );
    let pinned = f.threads.modify(OWNER, v.channel.id, pin).await.unwrap();
    assert_eq!(
        pinned.channel.flags & channel_flags::PINNED,
        channel_flags::PINNED
    );
    f.clock.advance_ms(30 * 24 * 60 * MIN);
    assert!(f.threads.archive_idle(10).await.unwrap().is_empty());
}

#[tokio::test]
async fn deleting_the_parent_deletes_its_threads() {
    let f = fx().await;
    let tid = thread_from_message(&f, BOB, BOB).await;
    f.chat.delete_channel(OWNER, f.general).await.unwrap();
    assert!(!f.repo.store.lock().unwrap().channels.contains_key(&tid));
    assert!(f.repo.store.lock().unwrap().thread_members.is_empty());
}

#[tokio::test]
async fn archived_listing_paginates_newest_archive_first() {
    let f = fx().await;
    let mut ids = Vec::new();
    for n in 0..3 {
        let m = f
            .chat
            .send_message(BOB, f.general, say(&format!("m{n}")))
            .await
            .unwrap();
        let t = f
            .threads
            .start_from_message(BOB, f.general, m.id, start(&format!("t{n}")))
            .await
            .unwrap();
        f.clock.advance_ms(MIN);
        f.threads
            .modify(
                BOB,
                t.view.channel.id,
                ThreadPatch {
                    archived: Some(true),
                    ..ThreadPatch::default()
                },
            )
            .await
            .unwrap();
        ids.push(t.view.channel.id);
    }
    let first = f
        .threads
        .list_archived(BOB, f.general, ArchivedFilter::Public, None, 2)
        .await
        .unwrap();
    assert_eq!(first.threads.len(), 2);
    assert!(first.has_more);
    assert_eq!(first.threads[0].id, ids[2]);
    let before = first.threads[1]
        .thread
        .as_ref()
        .unwrap()
        .archive_timestamp_ms;
    let rest = f
        .threads
        .list_archived(BOB, f.general, ArchivedFilter::Public, Some(before), 2)
        .await
        .unwrap();
    assert_eq!(rest.threads.len(), 1);
    assert_eq!(rest.threads[0].id, ids[0]);
    assert!(!rest.has_more);
    // Private archives need MANAGE_THREADS.
    assert!(
        f.threads
            .list_archived(BOB, f.general, ArchivedFilter::Private, None, 5)
            .await
            .is_err()
    );
    assert!(
        f.threads
            .list_archived(OWNER, f.general, ArchivedFilter::Private, None, 5)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn mentions_in_private_threads_only_reach_members() {
    let f = fx().await;
    f.repo.store.lock().unwrap().roles[0].permissions |= Permissions::CREATE_PRIVATE_THREADS;
    let mut input = start("secret");
    input.kind = Some(ChannelType::PrivateThread);
    let tid = f
        .threads
        .start_standalone(BOB, f.general, input)
        .await
        .unwrap()
        .view
        .channel
        .id;
    let msg = f
        .chat
        .send_message(
            BOB,
            tid,
            say(&format!("hello <@{}> and <@{}>", OWNER.get(), CAROL.get())),
        )
        .await
        .unwrap();
    // Carol is not in the thread, so she is not pinged; the guild owner (admin) is.
    assert!(msg.mention_user_ids.contains(&OWNER));
    assert!(!msg.mention_user_ids.contains(&CAROL));
}
