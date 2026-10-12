//! Per-(user, guild) viewable-channel cache and NSFW gating (open question 131).
//!
//! The caller computes the channel allow-list ([`super::Access`]) from VIEW_CHANNEL
//! and READ_MESSAGE_HISTORY. That is costly for large guilds, so the result is
//! cached for [`ACCESS_TTL`]. Role, channel and member changes invalidate entries
//! explicitly through [`ViewableChannelCache::invalidate_guild`] and
//! [`ViewableChannelCache::invalidate_member`].

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use moka::sync::Cache;
use sr_core::{ChannelId, GuildId, UserId};

/// Cache lifetime for a (user, guild) allow-list.
pub const ACCESS_TTL: Duration = Duration::from_secs(60);
const MAX_ENTRIES: u64 = 100_000;

#[derive(Clone)]
struct Entry {
    /// Guild generation the entry was computed under; stale once it moves on.
    generation: u64,
    channels: Arc<Vec<ChannelId>>,
}

/// Cache of the channels a user may search in each guild.
///
/// Guild-wide invalidation bumps a per-guild generation instead of scanning the
/// cache, so it is O(1) and takes effect on the next read.
#[derive(Clone)]
pub struct ViewableChannelCache {
    inner: Cache<(UserId, GuildId), Entry>,
    generations: Arc<Mutex<HashMap<GuildId, u64>>>,
}

impl Default for ViewableChannelCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewableChannelCache {
    pub fn new() -> Self {
        Self::with_ttl(ACCESS_TTL)
    }

    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            inner: Cache::builder()
                .time_to_live(ttl)
                .max_capacity(MAX_ENTRIES)
                .build(),
            generations: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn generations(&self) -> MutexGuard<'_, HashMap<GuildId, u64>> {
        // The map holds plain counters, so a poisoned lock is still consistent.
        self.generations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn current_generation(&self, guild: GuildId) -> u64 {
        self.generations().get(&guild).copied().unwrap_or(0)
    }

    /// Cached allow-list, if present, not expired and not invalidated.
    pub fn get(&self, user: UserId, guild: GuildId) -> Option<Arc<Vec<ChannelId>>> {
        let entry = self.inner.get(&(user, guild))?;
        (entry.generation == self.current_generation(guild)).then_some(entry.channels)
    }

    /// Return the cached allow-list, or compute, store and return it.
    pub fn get_or_compute<E>(
        &self,
        user: UserId,
        guild: GuildId,
        compute: impl FnOnce() -> Result<Vec<ChannelId>, E>,
    ) -> Result<Arc<Vec<ChannelId>>, E> {
        if let Some(hit) = self.get(user, guild) {
            return Ok(hit);
        }
        let generation = self.current_generation(guild);
        let channels = Arc::new(compute()?);
        self.inner.insert(
            (user, guild),
            Entry {
                generation,
                channels: Arc::clone(&channels),
            },
        );
        Ok(channels)
    }

    /// `MemberUpdated` (roles or nickname changed for one member).
    pub fn invalidate_member(&self, user: UserId, guild: GuildId) {
        self.inner.invalidate(&(user, guild));
    }

    /// `RoleUpdated` / `ChannelUpdated` / guild-wide overwrite changes.
    pub fn invalidate_guild(&self, guild: GuildId) {
        *self.generations().entry(guild).or_insert(0) += 1;
    }
}

/// NSFW gating: NSFW channels are removed from the allow-list unless the account
/// has confirmed its age (`accounts.age_confirmed_at` is set).
pub fn apply_nsfw_gate(
    allowed: &[ChannelId],
    nsfw_channels: &HashSet<ChannelId>,
    age_confirmed: bool,
) -> Vec<ChannelId> {
    if age_confirmed {
        return allowed.to_vec();
    }
    allowed
        .iter()
        .copied()
        .filter(|c| !nsfw_channels.contains(c))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(n: u64) -> ChannelId {
        ChannelId::new(n)
    }

    #[test]
    fn caches_until_invalidated() {
        let cache = ViewableChannelCache::new();
        let (u, g) = (UserId::new(1), GuildId::new(10));
        let first: Result<_, ()> = cache.get_or_compute(u, g, || Ok(vec![ch(5)]));
        assert_eq!(*first.unwrap(), vec![ch(5)]);
        // Second call must not recompute.
        let second: Result<_, ()> = cache.get_or_compute(u, g, || Err(()));
        assert_eq!(*second.unwrap(), vec![ch(5)]);

        cache.invalidate_member(u, g);
        assert!(cache.get(u, g).is_none());
    }

    #[test]
    fn guild_invalidation_only_touches_that_guild() {
        let cache = ViewableChannelCache::new();
        let u = UserId::new(1);
        let _: Result<_, ()> = cache.get_or_compute(u, GuildId::new(10), || Ok(vec![ch(1)]));
        let _: Result<_, ()> = cache.get_or_compute(u, GuildId::new(11), || Ok(vec![ch(2)]));
        cache.invalidate_guild(GuildId::new(10));
        assert!(cache.get(u, GuildId::new(10)).is_none());
        assert!(cache.get(u, GuildId::new(11)).is_some());
        // Recomputed under the new generation.
        let fresh: Result<_, ()> = cache.get_or_compute(u, GuildId::new(10), || Ok(vec![ch(3)]));
        assert_eq!(*fresh.unwrap(), vec![ch(3)]);
    }

    #[test]
    fn nsfw_gate_requires_age_confirmation() {
        let allowed = [ch(1), ch(2), ch(3)];
        let nsfw: HashSet<_> = [ch(2)].into_iter().collect();
        assert_eq!(apply_nsfw_gate(&allowed, &nsfw, true), allowed.to_vec());
        assert_eq!(apply_nsfw_gate(&allowed, &nsfw, false), vec![ch(1), ch(3)]);
    }
}
