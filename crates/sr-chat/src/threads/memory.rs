//! In-memory [`ThreadRepository`] over [`MemoryRepo`] for service and endpoint tests.

use super::model::{MAX_THREAD_MEMBERS, ThreadMember, channel_flags};
use super::repo::{ActivityOutcome, ArchivedFilter, ThreadRepository};
use crate::domain::{Channel, ChannelType};
use crate::memory::{MemoryRepo, Store};
use crate::repo::RepoResult;
use async_trait::async_trait;
use sr_core::{ChannelId, GuildId, UserId};

fn recount(s: &mut Store, thread: ChannelId) {
    let n = s
        .thread_members
        .iter()
        .filter(|m| m.thread_id == thread)
        .count();
    if let Some(t) = s.channels.get_mut(&thread).and_then(|c| c.thread.as_mut()) {
        t.member_count = u32::try_from(n).unwrap_or(u32::MAX);
    }
}

#[async_trait]
impl ThreadRepository for MemoryRepo {
    async fn insert_thread(
        &self,
        thread: &Channel,
        creator: UserId,
        now_ms: i64,
    ) -> RepoResult<()> {
        let mut s = self.lock()?;
        s.channels.insert(thread.id, thread.clone());
        s.thread_members.push(ThreadMember {
            thread_id: thread.id,
            user_id: creator,
            joined_at_ms: now_ms,
            flags: 0,
        });
        recount(&mut s, thread.id);
        Ok(())
    }

    async fn update_thread(&self, thread: &Channel) -> RepoResult<()> {
        let mut s = self.lock()?;
        if let Some(cur) = s.channels.get_mut(&thread.id) {
            let (messages, total, members) = cur.thread.as_ref().map_or((0, 0, 0), |t| {
                (t.message_count, t.total_message_sent, t.member_count)
            });
            *cur = thread.clone();
            if let Some(t) = cur.thread.as_mut() {
                t.message_count = messages;
                t.total_message_sent = total;
                t.member_count = members;
            }
        }
        Ok(())
    }

    async fn add_thread_member(
        &self,
        thread: ChannelId,
        user: UserId,
        now_ms: i64,
    ) -> RepoResult<bool> {
        let mut s = self.lock()?;
        if s.thread_members
            .iter()
            .any(|m| m.thread_id == thread && m.user_id == user)
        {
            return Ok(false);
        }
        s.thread_members.push(ThreadMember {
            thread_id: thread,
            user_id: user,
            joined_at_ms: now_ms,
            flags: 0,
        });
        recount(&mut s, thread);
        Ok(true)
    }

    async fn remove_thread_member(&self, thread: ChannelId, user: UserId) -> RepoResult<bool> {
        let mut s = self.lock()?;
        let before = s.thread_members.len();
        s.thread_members
            .retain(|m| !(m.thread_id == thread && m.user_id == user));
        let removed = s.thread_members.len() != before;
        recount(&mut s, thread);
        Ok(removed)
    }

    async fn get_thread_member(
        &self,
        thread: ChannelId,
        user: UserId,
    ) -> RepoResult<Option<ThreadMember>> {
        Ok(self
            .lock()?
            .thread_members
            .iter()
            .find(|m| m.thread_id == thread && m.user_id == user)
            .cloned())
    }

    async fn list_thread_members(
        &self,
        thread: ChannelId,
        after: Option<UserId>,
        limit: usize,
    ) -> RepoResult<Vec<ThreadMember>> {
        let mut v: Vec<ThreadMember> = self
            .lock()?
            .thread_members
            .iter()
            .filter(|m| m.thread_id == thread && after.is_none_or(|a| m.user_id > a))
            .cloned()
            .collect();
        v.sort_by_key(|m| m.user_id);
        v.truncate(limit);
        Ok(v)
    }

    async fn thread_memberships(
        &self,
        user: UserId,
        threads: &[ChannelId],
    ) -> RepoResult<Vec<ThreadMember>> {
        Ok(self
            .lock()?
            .thread_members
            .iter()
            .filter(|m| m.user_id == user && threads.contains(&m.thread_id))
            .cloned()
            .collect())
    }

    async fn list_active_threads(&self, guild: GuildId) -> RepoResult<Vec<Channel>> {
        let mut v: Vec<Channel> = self
            .lock()?
            .channels
            .values()
            .filter(|c| c.guild_id == Some(guild) && c.thread.as_ref().is_some_and(|t| !t.archived))
            .cloned()
            .collect();
        v.sort_by_key(|c| std::cmp::Reverse(c.id));
        Ok(v)
    }

    async fn list_archived_threads(
        &self,
        parent: ChannelId,
        filter: ArchivedFilter,
        before_ms: Option<i64>,
        limit: usize,
    ) -> RepoResult<Vec<Channel>> {
        let s = self.lock()?;
        let mut v: Vec<Channel> = s
            .channels
            .values()
            .filter(|c| c.parent_id == Some(parent))
            .filter(|c| {
                c.thread.as_ref().is_some_and(|t| {
                    t.archived && before_ms.is_none_or(|b| t.archive_timestamp_ms < b)
                })
            })
            .filter(|c| match filter {
                ArchivedFilter::Public => c.kind != ChannelType::PrivateThread,
                ArchivedFilter::Private => c.kind == ChannelType::PrivateThread,
                ArchivedFilter::JoinedPrivate(u) => {
                    c.kind == ChannelType::PrivateThread
                        && s.thread_members
                            .iter()
                            .any(|m| m.thread_id == c.id && m.user_id == u)
                }
            })
            .cloned()
            .collect();
        v.sort_by_key(|c| {
            std::cmp::Reverse((
                c.thread.as_ref().map_or(0, |t| t.archive_timestamp_ms),
                c.id,
            ))
        });
        v.truncate(limit);
        Ok(v)
    }

    async fn record_thread_activity(
        &self,
        thread: ChannelId,
        author: UserId,
        now_ms: i64,
    ) -> RepoResult<Option<ActivityOutcome>> {
        let mut s = self.lock()?;
        let Some(info) = s.channels.get_mut(&thread).and_then(|c| c.thread.as_mut()) else {
            return Ok(None);
        };
        let unarchived = info.archived;
        if unarchived {
            info.archived = false;
            info.archive_timestamp_ms = now_ms;
        }
        info.last_activity_ms = now_ms;
        info.message_count = info.message_count.saturating_add(1);
        info.total_message_sent = info.total_message_sent.saturating_add(1);
        let members = s
            .thread_members
            .iter()
            .filter(|m| m.thread_id == thread)
            .count();
        let mut joined = None;
        let already = s
            .thread_members
            .iter()
            .any(|m| m.thread_id == thread && m.user_id == author);
        if members < MAX_THREAD_MEMBERS && !already {
            let m = ThreadMember {
                thread_id: thread,
                user_id: author,
                joined_at_ms: now_ms,
                flags: 0,
            };
            s.thread_members.push(m.clone());
            joined = Some(m);
        }
        recount(&mut s, thread);
        let Some(channel) = s.channels.get(&thread).cloned() else {
            return Ok(None);
        };
        Ok(Some(ActivityOutcome {
            thread: channel,
            unarchived,
            joined,
        }))
    }

    async fn archive_idle_threads(&self, now_ms: i64, limit: usize) -> RepoResult<Vec<ChannelId>> {
        let mut s = self.lock()?;
        let mut out = Vec::new();
        for c in s.channels.values_mut() {
            if out.len() >= limit {
                break;
            }
            let pinned = c.flags & channel_flags::PINNED != 0;
            if let Some(t) = c.thread.as_mut()
                && !pinned
                && t.is_idle(now_ms)
            {
                t.archived = true;
                t.archive_timestamp_ms = now_ms;
                out.push(c.id);
            }
        }
        Ok(out)
    }
}
