//! The storage port of Live TV and an in-memory implementation for tests. [`crate::pg::PgTvRepo`]
//! implements it over Postgres (migrations 0460 and 0461).

use std::sync::Mutex;

use async_trait::async_trait;

use crate::error::{TvError, TvResult};
use crate::model::{BlockRecord, BlockSource, ChannelInput, ChannelRecord, ReminderRecord};
use crate::schedule::Change;

#[async_trait]
pub trait TvRepository: Send + Sync {
    /// Every channel by number.
    async fn list_channels(&self) -> TvResult<Vec<ChannelRecord>>;
    async fn get_channel(&self, id: i64) -> TvResult<Option<ChannelRecord>>;
    /// A taken number is [`TvError::Conflict`].
    async fn create_channel(
        &self,
        id: i64,
        input: &ChannelInput,
        seed: i64,
        now_ms: i64,
    ) -> TvResult<ChannelRecord>;
    async fn update_channel(
        &self,
        id: i64,
        input: &ChannelInput,
        now_ms: i64,
    ) -> TvResult<Option<ChannelRecord>>;
    /// Delete a channel and its blocks.
    async fn delete_channel(&self, id: i64) -> TvResult<bool>;

    /// Blocks of the channels that overlap `[from_ms, to_ms)`, by channel and start.
    async fn blocks_overlapping(
        &self,
        channels: &[i64],
        from_ms: i64,
        to_ms: i64,
    ) -> TvResult<Vec<BlockRecord>>;
    async fn get_block(&self, id: i64) -> TvResult<Option<BlockRecord>>;
    /// The block that ends last.
    async fn last_block(&self, channel: i64) -> TvResult<Option<BlockRecord>>;
    /// The generated block with a generator state that ends last, not after `end_ms_max`.
    async fn cursor_block(&self, channel: i64, end_ms_max: i64) -> TvResult<Option<BlockRecord>>;
    /// Apply a change in one transaction. `ignore_conflicts` skips rows whose id or `(channel, start)`
    /// exists already (two requests extending the same schedule). Returns the rows written.
    async fn apply(&self, channel: i64, change: &Change, ignore_conflicts: bool)
    -> TvResult<usize>;
    /// Record the HLS bases of a block unless it has them. Returns the stored pair.
    async fn set_bases(&self, id: i64, seq_base: i64, disc_base: i64) -> TvResult<(i64, i64)>;

    /// Store a reminder; setting it again replaces the earlier one.
    async fn put_reminder(&self, r: &ReminderRecord) -> TvResult<()>;
    /// Remove a reminder. `false` when there was none.
    async fn delete_reminder(&self, account_id: i64, block_id: i64) -> TvResult<bool>;
    /// Remove and return up to `limit` reminders due at `now_ms`, oldest first. Each is handed out once
    /// (concurrent callers never get the same row).
    async fn take_due_reminders(&self, now_ms: i64, limit: i64) -> TvResult<Vec<ReminderRecord>>;
}

#[derive(Default)]
struct State {
    channels: Vec<ChannelRecord>,
    blocks: Vec<BlockRecord>,
    reminders: Vec<ReminderRecord>,
}

/// In-memory repository.
#[derive(Default)]
pub struct MemoryTvRepo {
    state: Mutex<State>,
}

impl MemoryTvRepo {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }
}

#[async_trait]
impl TvRepository for MemoryTvRepo {
    async fn list_channels(&self) -> TvResult<Vec<ChannelRecord>> {
        let mut v = self.lock().channels.clone();
        v.sort_by_key(|c| (c.input.number, c.id));
        Ok(v)
    }

    async fn get_channel(&self, id: i64) -> TvResult<Option<ChannelRecord>> {
        Ok(self.lock().channels.iter().find(|c| c.id == id).cloned())
    }

    async fn create_channel(
        &self,
        id: i64,
        input: &ChannelInput,
        seed: i64,
        now_ms: i64,
    ) -> TvResult<ChannelRecord> {
        let mut s = self.lock();
        if s.channels.iter().any(|c| c.input.number == input.number) {
            return Err(TvError::Conflict(format!(
                "Channel number {} is taken",
                input.number
            )));
        }
        let rec = ChannelRecord {
            id,
            input: input.clone(),
            seed,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
        };
        s.channels.push(rec.clone());
        Ok(rec)
    }

    async fn update_channel(
        &self,
        id: i64,
        input: &ChannelInput,
        now_ms: i64,
    ) -> TvResult<Option<ChannelRecord>> {
        let mut s = self.lock();
        if s.channels
            .iter()
            .any(|c| c.id != id && c.input.number == input.number)
        {
            return Err(TvError::Conflict(format!(
                "Channel number {} is taken",
                input.number
            )));
        }
        Ok(s.channels.iter_mut().find(|c| c.id == id).map(|c| {
            c.input = input.clone();
            c.updated_at_ms = now_ms;
            c.clone()
        }))
    }

    async fn delete_channel(&self, id: i64) -> TvResult<bool> {
        let mut s = self.lock();
        let before = s.channels.len();
        s.channels.retain(|c| c.id != id);
        s.blocks.retain(|b| b.channel_id != id);
        Ok(s.channels.len() != before)
    }

    async fn blocks_overlapping(
        &self,
        channels: &[i64],
        from_ms: i64,
        to_ms: i64,
    ) -> TvResult<Vec<BlockRecord>> {
        let mut v: Vec<_> = self
            .lock()
            .blocks
            .iter()
            .filter(|b| {
                channels.contains(&b.channel_id) && b.end_ms > from_ms && b.start_ms < to_ms
            })
            .cloned()
            .collect();
        v.sort_by_key(|b| (b.channel_id, b.start_ms));
        Ok(v)
    }

    async fn get_block(&self, id: i64) -> TvResult<Option<BlockRecord>> {
        Ok(self.lock().blocks.iter().find(|b| b.id == id).cloned())
    }

    async fn last_block(&self, channel: i64) -> TvResult<Option<BlockRecord>> {
        Ok(self
            .lock()
            .blocks
            .iter()
            .filter(|b| b.channel_id == channel)
            .max_by_key(|b| b.end_ms)
            .cloned())
    }

    async fn cursor_block(&self, channel: i64, end_ms_max: i64) -> TvResult<Option<BlockRecord>> {
        Ok(self
            .lock()
            .blocks
            .iter()
            .filter(|b| {
                b.channel_id == channel
                    && b.source == BlockSource::Generated
                    && b.cursor_after.is_some()
                    && b.end_ms <= end_ms_max
            })
            .max_by_key(|b| b.end_ms)
            .cloned())
    }

    async fn apply(
        &self,
        channel: i64,
        change: &Change,
        ignore_conflicts: bool,
    ) -> TvResult<usize> {
        let mut s = self.lock();
        let mut next = s.blocks.clone();
        next.retain(|b| !change.delete.contains(&b.id));
        let mut written = 0;
        for rec in &change.upsert {
            if let Some(slot) = next.iter_mut().find(|b| b.id == rec.id) {
                if ignore_conflicts {
                    continue;
                }
                *slot = rec.clone();
                written += 1;
                continue;
            }
            if next
                .iter()
                .any(|b| b.channel_id == channel && b.start_ms == rec.start_ms)
            {
                if ignore_conflicts {
                    continue;
                }
                return Err(TvError::Conflict("A block starts there already".into()));
            }
            next.push(rec.clone());
            written += 1;
        }
        // Mirror the database: the unique start per channel is checked on the final state.
        let mut starts: Vec<_> = next
            .iter()
            .filter(|b| b.channel_id == channel)
            .map(|b| b.start_ms)
            .collect();
        starts.sort_unstable();
        if starts.windows(2).any(|w| w[0] == w[1]) {
            return Err(TvError::Conflict("Two blocks start together".into()));
        }
        s.blocks = next;
        Ok(written)
    }

    async fn set_bases(&self, id: i64, seq_base: i64, disc_base: i64) -> TvResult<(i64, i64)> {
        let mut s = self.lock();
        let b = s
            .blocks
            .iter_mut()
            .find(|b| b.id == id)
            .ok_or(TvError::NotFound("block"))?;
        if let (Some(a), Some(d)) = (b.seq_base, b.disc_base) {
            return Ok((a, d));
        }
        b.seq_base = Some(seq_base);
        b.disc_base = Some(disc_base);
        Ok((seq_base, disc_base))
    }

    async fn put_reminder(&self, r: &ReminderRecord) -> TvResult<()> {
        let mut s = self.lock();
        s.reminders
            .retain(|x| !(x.account_id == r.account_id && x.block_id == r.block_id));
        s.reminders.push(r.clone());
        Ok(())
    }

    async fn delete_reminder(&self, account_id: i64, block_id: i64) -> TvResult<bool> {
        let mut s = self.lock();
        let before = s.reminders.len();
        s.reminders
            .retain(|x| !(x.account_id == account_id && x.block_id == block_id));
        Ok(s.reminders.len() != before)
    }

    async fn take_due_reminders(&self, now_ms: i64, limit: i64) -> TvResult<Vec<ReminderRecord>> {
        let mut s = self.lock();
        let mut due: Vec<ReminderRecord> = s
            .reminders
            .iter()
            .filter(|r| r.remind_at_ms <= now_ms)
            .cloned()
            .collect();
        due.sort_by_key(|r| (r.remind_at_ms, r.account_id, r.block_id));
        due.truncate(usize::try_from(limit).unwrap_or(0));
        s.reminders.retain(|x| !due.contains(x));
        Ok(due)
    }
}
