//! The storage port of Live TV and an in-memory implementation for tests. [`crate::pg::PgTvRepo`]
//! implements it over Postgres (migrations 0460 and 0461).

use std::sync::Mutex;

use async_trait::async_trait;

use crate::error::{TvError, TvResult};
use crate::model::{
    BlockRecord, BlockSource, ChannelInput, ChannelRecord, ImportRecord, ReminderRecord, SlotRecord,
};
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
    /// The member's pending reminders, soonest first.
    async fn list_reminders(&self, account_id: i64) -> TvResult<Vec<ReminderRecord>>;
    /// The member's `tv.reminder_lead_minutes` setting (user settings blob) when it is a number.
    async fn reminder_lead_setting(&self, account_id: i64) -> TvResult<Option<i64>>;

    // ---- weekly slots, guide imports, favorites (migration 1000) ----------------------------------

    /// The weekly slots of a channel by weekday and start.
    async fn list_slots(&self, channel: i64) -> TvResult<Vec<SlotRecord>>;
    /// Replace every slot of the channel in one transaction.
    async fn replace_slots(&self, channel: i64, slots: &[SlotRecord]) -> TvResult<()>;
    /// Imported-guide channels' ids.
    async fn imported_channels(&self) -> TvResult<Vec<i64>>;
    /// Store a pending import.
    async fn put_import(&self, rec: &ImportRecord) -> TvResult<()>;
    async fn get_import(&self, id: i64) -> TvResult<Option<ImportRecord>>;
    /// The channel's imports, newest first, without rows.
    async fn list_imports(&self, channel: i64, limit: i64) -> TvResult<Vec<ImportRecord>>;
    /// Mark an import confirmed. `false` when it was confirmed already (or is gone).
    async fn confirm_import(&self, id: i64, now_ms: i64) -> TvResult<bool>;

    /// The member's starred channels.
    async fn favorites(&self, account_id: i64) -> TvResult<Vec<i64>>;
    /// Star or unstar a channel (idempotent).
    async fn set_favorite(&self, account_id: i64, channel: i64, on: bool) -> TvResult<()>;
}

#[derive(Default)]
struct State {
    slots: Vec<SlotRecord>,
    imports: Vec<ImportRecord>,
    favorites: Vec<(i64, i64)>,
    channels: Vec<ChannelRecord>,
    blocks: Vec<BlockRecord>,
    reminders: Vec<ReminderRecord>,
    leads: Vec<(i64, i64)>,
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

    /// Test helper: the member's `tv.reminder_lead_minutes` setting.
    pub fn set_lead_setting(&self, account_id: i64, minutes: i64) {
        let mut s = self.lock();
        s.leads.retain(|(a, _)| *a != account_id);
        s.leads.push((account_id, minutes));
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
        s.slots.retain(|x| x.channel_id != id);
        s.imports.retain(|x| x.channel_id != id);
        s.favorites.retain(|(_, c)| *c != id);
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

    async fn list_reminders(&self, account_id: i64) -> TvResult<Vec<ReminderRecord>> {
        let mut v: Vec<ReminderRecord> = self
            .lock()
            .reminders
            .iter()
            .filter(|r| r.account_id == account_id)
            .cloned()
            .collect();
        v.sort_by_key(|r| (r.start_ms, r.block_id));
        Ok(v)
    }

    async fn reminder_lead_setting(&self, account_id: i64) -> TvResult<Option<i64>> {
        Ok(self
            .lock()
            .leads
            .iter()
            .find(|(a, _)| *a == account_id)
            .map(|(_, m)| *m))
    }

    async fn list_slots(&self, channel: i64) -> TvResult<Vec<SlotRecord>> {
        let mut v: Vec<SlotRecord> = self
            .lock()
            .slots
            .iter()
            .filter(|s| s.channel_id == channel)
            .cloned()
            .collect();
        v.sort_by_key(|s| (s.weekday, s.start_minute, s.id));
        Ok(v)
    }

    async fn replace_slots(&self, channel: i64, slots: &[SlotRecord]) -> TvResult<()> {
        let mut s = self.lock();
        s.slots.retain(|x| x.channel_id != channel);
        s.slots.extend(slots.iter().cloned());
        Ok(())
    }

    async fn imported_channels(&self) -> TvResult<Vec<i64>> {
        Ok(self
            .lock()
            .channels
            .iter()
            .filter(|c| c.input.kind == crate::model::ChannelKind::ImportedGuide)
            .map(|c| c.id)
            .collect())
    }

    async fn put_import(&self, rec: &ImportRecord) -> TvResult<()> {
        let mut s = self.lock();
        s.imports.retain(|x| x.id != rec.id);
        s.imports.push(rec.clone());
        Ok(())
    }

    async fn get_import(&self, id: i64) -> TvResult<Option<ImportRecord>> {
        Ok(self.lock().imports.iter().find(|x| x.id == id).cloned())
    }

    async fn list_imports(&self, channel: i64, limit: i64) -> TvResult<Vec<ImportRecord>> {
        let mut v: Vec<ImportRecord> = self
            .lock()
            .imports
            .iter()
            .filter(|x| x.channel_id == channel)
            .map(|x| ImportRecord {
                rows: serde_json::Value::Null,
                ..x.clone()
            })
            .collect();
        v.sort_by_key(|x| std::cmp::Reverse((x.created_at_ms, x.id)));
        v.truncate(usize::try_from(limit).unwrap_or(0));
        Ok(v)
    }

    async fn confirm_import(&self, id: i64, _now_ms: i64) -> TvResult<bool> {
        let mut s = self.lock();
        Ok(s.imports
            .iter_mut()
            .find(|x| x.id == id && !x.confirmed)
            .map(|x| x.confirmed = true)
            .is_some())
    }

    async fn favorites(&self, account_id: i64) -> TvResult<Vec<i64>> {
        let mut v: Vec<i64> = self
            .lock()
            .favorites
            .iter()
            .filter(|(a, _)| *a == account_id)
            .map(|(_, c)| *c)
            .collect();
        v.sort_unstable();
        Ok(v)
    }

    async fn set_favorite(&self, account_id: i64, channel: i64, on: bool) -> TvResult<()> {
        let mut s = self.lock();
        s.favorites
            .retain(|(a, c)| !(*a == account_id && *c == channel));
        if on {
            s.favorites.push((account_id, channel));
        }
        Ok(())
    }
}
