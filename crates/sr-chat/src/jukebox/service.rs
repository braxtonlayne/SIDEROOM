//! The service the routes call: a locked unit of work, the clock, then the rules.

use super::model::{Doc, JukeboxError, Mode, NewTrack, VoteOutcome};
use super::rules;
use super::store::JukeboxStore;
use sr_core::Clock;
use std::sync::Arc;

/// The caller's standing, decided by the routes from channel permissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Actor {
    /// The member.
    pub user: i64,
    /// `MANAGE_CHANNELS` or administrator.
    pub manage: bool,
}

/// A settled copy of a channel's state with the numbers clients need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// The rows after settling.
    pub doc: Doc,
    /// The server clock the numbers below were read at, unix ms.
    pub now_ms: i64,
    /// Position in the current track at `now_ms`.
    pub position_ms: i64,
    /// Skip votes standing from live listeners.
    pub votes: usize,
    /// Skip votes needed.
    pub needed: usize,
    /// Live listeners.
    pub listeners: usize,
}

/// The result of a change: the value, the new state, and whether other clients must be told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done<T> {
    /// What the rule returned.
    pub value: T,
    /// The state after.
    pub snapshot: Snapshot,
    /// Clients should refetch (the version moved).
    pub changed: bool,
}

/// Store, clock, ids and the vote threshold.
#[derive(Clone)]
pub struct JukeboxService {
    store: Arc<dyn JukeboxStore>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn Fn() -> i64 + Send + Sync>,
}

impl JukeboxService {
    /// Wire the service.
    pub fn new(
        store: Arc<dyn JukeboxStore>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn Fn() -> i64 + Send + Sync>,
    ) -> Self {
        Self { store, clock, ids }
    }

    fn snapshot(doc: Doc, now: i64, percent: i64) -> Snapshot {
        let (votes, needed) = rules::tally(&doc, now, percent);
        Snapshot {
            position_ms: rules::position_at(&doc, now),
            listeners: rules::live_listeners(&doc, now).len(),
            votes,
            needed,
            now_ms: now,
            doc,
        }
    }

    /// The state of a channel, settled to now (a finished track moves on and is saved). `None` when the
    /// channel never used the jukebox. The bool says the settle changed something.
    pub async fn state(
        &self,
        channel: i64,
        percent: i64,
    ) -> Result<Option<(Snapshot, bool)>, JukeboxError> {
        let now = self.clock.now_unix_ms();
        let Some(doc) = self.store.peek(channel).await? else {
            return Ok(None);
        };
        if rules::needs_settle(&doc, now) {
            let guild = doc.session.guild_id;
            let d = self
                .mutate(channel, guild, percent, |_, _| Ok::<(), JukeboxError>(()))
                .await?;
            return Ok(Some((d.snapshot, d.changed)));
        }
        Ok(Some((Self::snapshot(doc, now, percent), false)))
    }

    /// The clock the service reads, unix ms.
    pub fn now_ms(&self) -> i64 {
        self.clock.now_unix_ms()
    }

    /// Run `f` on the settled doc under the channel lock and save the result.
    async fn mutate<R>(
        &self,
        channel: i64,
        guild: i64,
        percent: i64,
        f: impl FnOnce(&mut Doc, i64) -> Result<R, JukeboxError>,
    ) -> Result<Done<R>, JukeboxError> {
        let now = self.clock.now_unix_ms();
        let mut txn = self.store.begin(channel, guild).await?;
        let mut doc = txn.load().await?;
        let v0 = doc.session.version;
        rules::settle(&mut doc, now);
        let value = f(&mut doc, now)?;
        let changed = doc.session.version != v0;
        txn.save(&doc).await?;
        txn.commit().await?;
        Ok(Done {
            value,
            snapshot: Self::snapshot(doc, now, percent),
            changed,
        })
    }

    /// Queue tracks (the routes checked that the caller may play each from its mount).
    pub async fn enqueue(
        &self,
        channel: i64,
        guild: i64,
        user: i64,
        tracks: &[NewTrack],
        percent: i64,
    ) -> Result<Done<Vec<i64>>, JukeboxError> {
        let ids = self.ids.clone();
        self.mutate(channel, guild, percent, |doc, now| {
            rules::enqueue(doc, user, tracks, &mut || ids(), now)
        })
        .await
    }

    /// Remove a queue row (its adder, the DJ or a moderator).
    pub async fn remove(
        &self,
        channel: i64,
        guild: i64,
        actor: Actor,
        item: i64,
        percent: i64,
    ) -> Result<Done<()>, JukeboxError> {
        self.mutate(channel, guild, percent, |doc, now| {
            let privileged = actor.manage || doc.session.dj == Some(actor.user);
            rules::remove(doc, item, actor.user, privileged, now)
        })
        .await
    }

    /// Vote to skip the current track.
    pub async fn vote_skip(
        &self,
        channel: i64,
        guild: i64,
        user: i64,
        percent: i64,
    ) -> Result<Done<VoteOutcome>, JukeboxError> {
        self.mutate(channel, guild, percent, |doc, now| {
            rules::vote_skip(doc, user, now, percent)
        })
        .await
    }

    /// Take a skip vote back.
    pub async fn withdraw_vote(
        &self,
        channel: i64,
        guild: i64,
        user: i64,
        percent: i64,
    ) -> Result<Done<bool>, JukeboxError> {
        self.mutate(channel, guild, percent, |doc, _| {
            Ok(rules::withdraw_vote(doc, user))
        })
        .await
    }

    /// Skip outright: the DJ, a moderator, or whoever queued the current track.
    pub async fn force_skip(
        &self,
        channel: i64,
        guild: i64,
        actor: Actor,
        percent: i64,
    ) -> Result<Done<()>, JukeboxError> {
        self.mutate(channel, guild, percent, |doc, now| {
            let own = doc.current().and_then(|i| i.added_by) == Some(actor.user);
            if !(actor.manage || doc.session.dj == Some(actor.user) || own) {
                return Err(JukeboxError::NotAllowed);
            }
            rules::skip_now(doc, now)
        })
        .await
    }

    /// A client reports that a track reached its end.
    pub async fn ended(
        &self,
        channel: i64,
        guild: i64,
        item: i64,
        percent: i64,
    ) -> Result<Done<bool>, JukeboxError> {
        self.mutate(channel, guild, percent, |doc, now| {
            Ok(rules::ended(doc, item, now))
        })
        .await
    }

    /// Pause or resume (the DJ, or a moderator).
    pub async fn set_paused(
        &self,
        channel: i64,
        guild: i64,
        actor: Actor,
        paused: bool,
        percent: i64,
    ) -> Result<Done<()>, JukeboxError> {
        self.mutate(channel, guild, percent, |doc, now| {
            if !(actor.manage || doc.session.dj == Some(actor.user)) {
                return Err(JukeboxError::NotAllowed);
            }
            if paused {
                rules::pause(doc, now)
            } else {
                rules::resume(doc, now)
            }
        })
        .await
    }

    /// Switch stream or synced: the DJ, a moderator, or any speaker while nobody holds the decks.
    pub async fn set_mode(
        &self,
        channel: i64,
        guild: i64,
        actor: Actor,
        mode: Mode,
        percent: i64,
    ) -> Result<Done<()>, JukeboxError> {
        self.mutate(channel, guild, percent, |doc, now| {
            let open = doc.session.dj.is_none();
            if !(actor.manage || open || doc.session.dj == Some(actor.user)) {
                return Err(JukeboxError::NotAllowed);
            }
            rules::touch_listener(doc, actor.user, now);
            rules::set_mode(doc, mode);
            Ok(())
        })
        .await
    }

    /// Claim, hand over or release the decks. Claiming (`target` is the caller) is open while nobody holds
    /// them; anything else needs the current DJ or a moderator.
    pub async fn set_dj(
        &self,
        channel: i64,
        guild: i64,
        actor: Actor,
        target: Option<i64>,
        percent: i64,
    ) -> Result<Done<()>, JukeboxError> {
        self.mutate(channel, guild, percent, |doc, now| {
            let dj = doc.session.dj;
            let claim = dj.is_none() && target == Some(actor.user);
            if !(actor.manage || claim || dj == Some(actor.user)) {
                return Err(JukeboxError::NotAllowed);
            }
            rules::touch_listener(doc, actor.user, now);
            rules::set_dj(doc, target, now)
        })
        .await
    }

    /// The member is listening (join and heartbeat). Only a change in who is listening is announced.
    pub async fn listen(
        &self,
        channel: i64,
        guild: i64,
        user: i64,
        percent: i64,
    ) -> Result<Done<()>, JukeboxError> {
        self.mutate(channel, guild, percent, |doc, now| {
            let was = rules::is_listening(doc, user, now);
            rules::touch_listener(doc, user, now);
            if !was {
                doc.session.version += 1;
            }
            Ok(())
        })
        .await
    }

    /// The member stopped listening: their votes go and they let go of the decks.
    pub async fn leave(
        &self,
        channel: i64,
        guild: i64,
        user: i64,
        percent: i64,
    ) -> Result<Done<()>, JukeboxError> {
        self.mutate(channel, guild, percent, |doc, _| {
            if rules::drop_listener(doc, user) {
                doc.session.version += 1;
            }
            Ok(())
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::super::memory::MemoryJukebox;
    use super::*;
    use std::sync::atomic::{AtomicI64, Ordering};

    struct TestClock(AtomicI64);
    impl Clock for TestClock {
        fn now_unix_ms(&self) -> i64 {
            self.0.load(Ordering::SeqCst)
        }
    }

    fn svc() -> (JukeboxService, Arc<TestClock>) {
        let clock = Arc::new(TestClock(AtomicI64::new(1_000_000)));
        let n = Arc::new(AtomicI64::new(0));
        let ids: Arc<dyn Fn() -> i64 + Send + Sync> =
            Arc::new(move || n.fetch_add(1, Ordering::SeqCst) + 1);
        (
            JukeboxService::new(Arc::new(MemoryJukebox::new()), clock.clone(), ids),
            clock,
        )
    }

    fn t(n: i64, len: i64) -> NewTrack {
        NewTrack {
            track_id: n,
            mount_id: 3,
            key: format!("m/{n}.mp3"),
            title: format!("T{n}"),
            artist: String::new(),
            duration_ms: Some(len),
        }
    }

    #[tokio::test]
    async fn queue_play_and_auto_advance_on_read() {
        let (s, clock) = svc();
        assert!(s.state(1, 50).await.unwrap().is_none());
        let d = s
            .enqueue(1, 9, 10, &[t(1, 5_000), t(2, 5_000)], 50)
            .await
            .unwrap();
        assert!(d.changed);
        assert_eq!(d.snapshot.position_ms, 0);
        clock.0.store(1_007_000, Ordering::SeqCst);
        let (snap, changed) = s.state(1, 50).await.unwrap().unwrap();
        assert!(changed, "the settle moved the queue on");
        assert_eq!(snap.position_ms, 2_000);
        assert_eq!(snap.doc.current().map(|i| i.track_id), Some(Some(2)));
        let (_, changed) = s.state(1, 50).await.unwrap().unwrap();
        assert!(!changed, "a second read changes nothing");
    }

    #[tokio::test]
    async fn only_the_privileged_pause_and_the_open_decks_can_be_claimed() {
        let (s, _) = svc();
        s.enqueue(1, 9, 10, &[t(1, 60_000)], 50).await.unwrap();
        let me = Actor {
            user: 10,
            manage: false,
        };
        let other = Actor {
            user: 11,
            manage: false,
        };
        let boss = Actor {
            user: 12,
            manage: true,
        };
        assert_eq!(
            s.set_paused(1, 9, me, true, 50).await.unwrap_err(),
            JukeboxError::NotAllowed
        );
        s.set_dj(1, 9, me, Some(10), 50).await.unwrap();
        s.set_paused(1, 9, me, true, 50).await.unwrap();
        assert_eq!(
            s.set_paused(1, 9, other, false, 50).await.unwrap_err(),
            JukeboxError::NotAllowed
        );
        assert_eq!(
            s.set_dj(1, 9, other, Some(11), 50).await.unwrap_err(),
            JukeboxError::NotAllowed
        );
        s.listen(1, 9, 11, 50).await.unwrap();
        s.set_dj(1, 9, me, Some(11), 50).await.unwrap();
        s.set_paused(1, 9, boss, false, 50).await.unwrap();
        let (snap, _) = s.state(1, 50).await.unwrap().unwrap();
        assert_eq!(snap.doc.session.dj, Some(11));
        assert!(!snap.doc.session.paused);
    }

    #[tokio::test]
    async fn a_heartbeat_is_not_announced_but_a_new_listener_is() {
        let (s, _) = svc();
        assert!(s.listen(1, 9, 10, 50).await.unwrap().changed);
        assert!(!s.listen(1, 9, 10, 50).await.unwrap().changed);
        assert!(s.listen(1, 9, 11, 50).await.unwrap().changed);
        assert_eq!(s.listen(1, 9, 11, 50).await.unwrap().snapshot.listeners, 2);
    }

    #[tokio::test]
    async fn vote_skip_runs_through_the_service() {
        let (s, _) = svc();
        s.enqueue(1, 9, 10, &[t(1, 600_000), t(2, 1_000)], 50)
            .await
            .unwrap();
        s.listen(1, 9, 11, 50).await.unwrap();
        let v = s.vote_skip(1, 9, 11, 50).await.unwrap();
        assert!(v.value.skipped, "1 of 2 live listeners is the ratio");
        assert_eq!(v.snapshot.doc.current().map(|i| i.track_id), Some(Some(2)));
        assert!(v.changed);
    }
}
