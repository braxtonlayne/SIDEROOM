//! Persistence surface for threads. `PgChatRepo` (sr-db) and `MemoryRepo` implement it.

use super::model::ThreadMember;
use crate::domain::Channel;
use crate::repo::RepoResult;
use async_trait::async_trait;
use sr_core::{ChannelId, GuildId, UserId};

/// Which archived threads of a parent channel to list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchivedFilter {
    /// Public and announcement threads (and forum posts).
    Public,
    /// Every private thread (needs MANAGE_THREADS).
    Private,
    /// Private threads the user joined.
    JoinedPrivate(UserId),
}

/// What recording a message in a thread changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityOutcome {
    /// The thread after the update.
    pub thread: Channel,
    /// The message reopened an archived thread.
    pub unarchived: bool,
    /// The author joined the thread by posting.
    pub joined: Option<ThreadMember>,
}

#[async_trait]
pub trait ThreadRepository: Send + Sync {
    /// Insert the thread channel, its metadata and its creator's membership atomically.
    async fn insert_thread(&self, thread: &Channel, creator: UserId, now_ms: i64)
    -> RepoResult<()>;
    /// Persist name, slowmode, flags and all thread metadata of `thread`.
    async fn update_thread(&self, thread: &Channel) -> RepoResult<()>;

    /// Returns true when the user was not a member yet.
    async fn add_thread_member(
        &self,
        thread: ChannelId,
        user: UserId,
        now_ms: i64,
    ) -> RepoResult<bool>;
    /// Returns true when the user was a member.
    async fn remove_thread_member(&self, thread: ChannelId, user: UserId) -> RepoResult<bool>;
    async fn get_thread_member(
        &self,
        thread: ChannelId,
        user: UserId,
    ) -> RepoResult<Option<ThreadMember>>;
    /// Members ordered by user id, strictly after `after`.
    async fn list_thread_members(
        &self,
        thread: ChannelId,
        after: Option<UserId>,
        limit: usize,
    ) -> RepoResult<Vec<ThreadMember>>;
    /// `user`'s membership rows among `threads`.
    async fn thread_memberships(
        &self,
        user: UserId,
        threads: &[ChannelId],
    ) -> RepoResult<Vec<ThreadMember>>;

    /// Unarchived threads of the guild, newest first.
    async fn list_active_threads(&self, guild: GuildId) -> RepoResult<Vec<Channel>>;
    /// Archived threads of `parent` whose archive time is before `before_ms`, newest
    /// archive first.
    async fn list_archived_threads(
        &self,
        parent: ChannelId,
        filter: ArchivedFilter,
        before_ms: Option<i64>,
        limit: usize,
    ) -> RepoResult<Vec<Channel>>;

    /// Bookkeeping for a message stored in `thread`: bump the counters and the
    /// activity clock, unarchive, and make `author` a member. `None` when
    /// `thread` is not a thread.
    async fn record_thread_activity(
        &self,
        thread: ChannelId,
        author: UserId,
        now_ms: i64,
    ) -> RepoResult<Option<ActivityOutcome>>;
    /// Archive up to `limit` threads whose auto-archive timer ran out (pinned posts
    /// are exempt). Returns the archived ids.
    async fn archive_idle_threads(&self, now_ms: i64, limit: usize) -> RepoResult<Vec<ChannelId>>;
}
