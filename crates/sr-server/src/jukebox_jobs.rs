//! 24/7 Listening Room channels (MUS-JB-003): the cursor job `jukebox.rooms`.
//!
//! A listening room is a voice channel with a row in `listening_rooms` (migration 1580) and a server playlist.
//! **The server never streams audio** (D-201): the room is a cursor, "item N, this far in, as of this moment".
//! Every listener plays the file locally from the server shelf and seeks to the cursor.
//!
//! * [`cursor`] is the arithmetic, pure functions with their tests: advance by elapsed time across track
//!   boundaries and loops, skip tracks without a known duration, list the next tracks.
//! * [`tick`] reads the due rooms (`FOR UPDATE SKIP LOCKED`, at most [`MAX_ROOMS_PER_TICK`]), advances each,
//!   stores the new cursor and returns the events to publish: on a track change at once, otherwise every 15 s.
//! * [`register`] and [`schedule`] put the singleton job `jukebox.rooms` (id 1580) on the runner; it reschedules
//!   itself every [`TICK_INTERVAL`] and a room that is not due costs one index probe.
//!
//! A room plays only tracks of the server's own shelf (a mount owned by that server) or the instance bucket:
//! a 24/7 room must not depend on a member's personal bucket (it vanishes when the member leaves, and the
//! titles would disclose it to the server). A track with no duration (`duration_ms` null or 0) is skipped,
//! because a cursor cannot know when it ends; the card says how many were skipped.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use sr_core::{ChannelId, GuildId};
use sr_db::DbResult;
use sr_db::jobs::{self, Job};
use sr_db::runner::{HandlerError, JobRunner};
use sr_events::{DomainEvent, EventBus};

/// Kind of the cursor job.
pub const ROOMS_KIND: &str = "jukebox.rooms";
/// Reserved singleton id (below any Snowflake; unique, checked by `admin_jobs` tests).
pub const ROOMS_JOB_ID: i64 = 1580;
/// How often the job looks for due rooms.
pub const TICK_INTERVAL: Duration = Duration::from_secs(2);
/// A playing room publishes its cursor at least this often.
pub const PUBLISH_EVERY_MS: i64 = 15_000;
/// Rooms handled by one tick; the rest are picked up by the next one (they stay due).
pub const MAX_ROOMS_PER_TICK: i64 = 200;
/// How many upcoming tracks the card shows.
pub const NEXT_TRACKS: usize = 3;

/// The cursor arithmetic. No clock, no database.
pub mod cursor {
    /// One playlist item as the cursor sees it.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Slot {
        /// `music_playlist_items.id`.
        pub item_id: i64,
        /// `music_tracks.id`.
        pub track_id: i64,
        /// Known length; `None` or 0 means the item cannot be scheduled.
        pub duration_ms: Option<i64>,
    }

    impl Slot {
        /// The length when the item can be scheduled.
        pub fn playable_ms(&self) -> Option<i64> {
            self.duration_ms.filter(|d| *d > 0)
        }
    }

    /// Where the cursor is after an advance.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Position {
        /// Index into the slots (not into the playable ones).
        pub index: usize,
        /// Milliseconds into that item, always below its duration.
        pub position_ms: i64,
        /// Milliseconds until the item ends.
        pub remaining_ms: i64,
        /// True when the cursor left the item it was on (next track, a loop, or the old item is gone).
        pub changed: bool,
    }

    /// Items that cannot be scheduled.
    pub fn skipped(slots: &[Slot]) -> usize {
        slots.iter().filter(|s| s.playable_ms().is_none()).count()
    }

    /// Length of one lap of the playlist (playable items only).
    pub fn lap_ms(slots: &[Slot]) -> i64 {
        slots.iter().filter_map(Slot::playable_ms).sum()
    }

    /// Move the cursor `elapsed_ms` forward. `current` is the item the cursor was on and `position_ms` how far
    /// into it; a `current` that is `None`, no longer on the list or no longer playable restarts the lap at its
    /// first playable item (and counts as a change). The playlist loops, so any elapsed time lands somewhere;
    /// `None` only when nothing on the list can be scheduled.
    pub fn advance(
        slots: &[Slot],
        current: Option<i64>,
        position_ms: i64,
        elapsed_ms: i64,
    ) -> Option<Position> {
        let total = lap_ms(slots);
        if total <= 0 {
            return None;
        }
        let elapsed = elapsed_ms.max(0);
        // Offset of the cursor from the start of the lap, and whether we know where we were.
        let mut before = 0i64;
        let mut start: Option<(i64, i64)> = None; // (offset, duration of the current item)
        for s in slots {
            let Some(d) = s.playable_ms() else { continue };
            if Some(s.item_id) == current {
                start = Some((before + position_ms.clamp(0, d), d));
                break;
            }
            before += d;
        }
        let (offset, left_in_current) = match start {
            Some((off, d)) => {
                let pos_in = position_ms.clamp(0, d);
                (off, d - pos_in)
            }
            None => (0, 0),
        };
        // `elapsed % total` keeps the sum far from overflow however long the job was down.
        let landed = (offset + elapsed % total) % total;
        let changed = start.is_none() || elapsed >= left_in_current;
        let mut acc = 0i64;
        for (index, s) in slots.iter().enumerate() {
            let Some(d) = s.playable_ms() else { continue };
            if landed < acc + d {
                let position_ms = landed - acc;
                return Some(Position {
                    index,
                    position_ms,
                    remaining_ms: d - position_ms,
                    changed,
                });
            }
            acc += d;
        }
        None
    }

    /// The tracks after `index`, in play order, wrapping round the lap, at most `n` and never more than the
    /// playable items there are (a one-track room lists that track once).
    pub fn upcoming(slots: &[Slot], index: usize, n: usize) -> Vec<&Slot> {
        let playable: Vec<&Slot> = slots.iter().filter(|s| s.playable_ms().is_some()).collect();
        let Some(here) = slots.get(index).map(|s| s.item_id) else {
            return Vec::new();
        };
        let Some(at) = playable.iter().position(|s| s.item_id == here) else {
            return Vec::new();
        };
        (1..=playable.len().min(n))
            .map(|k| playable[(at + k) % playable.len()])
            .collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn slot(id: i64, d: Option<i64>) -> Slot {
            Slot {
                item_id: id,
                track_id: id * 10,
                duration_ms: d,
            }
        }

        fn list() -> Vec<Slot> {
            vec![
                slot(1, Some(1000)),
                slot(2, Some(2000)),
                slot(3, Some(3000)),
            ]
        }

        #[test]
        fn inside_a_track_only_the_position_moves() {
            let p = advance(&list(), Some(2), 500, 700).unwrap();
            assert_eq!((p.index, p.position_ms, p.remaining_ms), (1, 1200, 800));
            assert!(!p.changed);
        }

        #[test]
        fn crossing_a_boundary_lands_in_the_next_track() {
            let p = advance(&list(), Some(1), 900, 250).unwrap();
            assert_eq!((p.index, p.position_ms), (1, 150));
            assert!(p.changed);
        }

        #[test]
        fn the_exact_end_of_a_track_starts_the_next_one() {
            let p = advance(&list(), Some(1), 0, 1000).unwrap();
            assert_eq!((p.index, p.position_ms), (1, 0));
            assert!(p.changed);
        }

        #[test]
        fn the_lap_loops_from_the_last_track_to_the_first() {
            let p = advance(&list(), Some(3), 2500, 700).unwrap();
            assert_eq!((p.index, p.position_ms), (0, 200));
            assert!(p.changed);
        }

        #[test]
        fn a_long_outage_lands_where_the_lap_says() {
            // 6000 ms per lap; 10 laps + 4500 ms after the start of item 1: inside item 3 at 1500.
            let p = advance(&list(), Some(1), 0, 60_000 + 4_500).unwrap();
            assert_eq!((p.index, p.position_ms), (2, 1500));
            assert!(p.changed);
            // The largest elapsed time does not overflow.
            assert!(advance(&list(), Some(2), 100, i64::MAX).is_some());
        }

        #[test]
        fn a_single_track_playlist_loops_on_itself() {
            let one = vec![slot(7, Some(1000))];
            let p = advance(&one, Some(7), 900, 200).unwrap();
            assert_eq!((p.index, p.position_ms), (0, 100));
            assert!(p.changed, "a restart of the same track is a change (seq)");
            let q = advance(&one, Some(7), 100, 200).unwrap();
            assert!(!q.changed);
        }

        #[test]
        fn a_missing_duration_is_skipped_not_guessed() {
            let l = vec![
                slot(1, Some(1000)),
                slot(2, None),
                slot(3, Some(0)),
                slot(4, Some(1000)),
            ];
            assert_eq!(skipped(&l), 2);
            assert_eq!(lap_ms(&l), 2000);
            let p = advance(&l, Some(1), 900, 200).unwrap();
            assert_eq!((p.index, p.position_ms), (3, 100));
            let wrap = advance(&l, Some(4), 900, 200).unwrap();
            assert_eq!((wrap.index, wrap.position_ms), (0, 100));
        }

        #[test]
        fn nothing_playable_gives_no_cursor() {
            assert_eq!(advance(&[], None, 0, 1000), None);
            assert_eq!(
                advance(&[slot(1, None), slot(2, Some(0))], None, 0, 5),
                None
            );
        }

        #[test]
        fn a_cursor_on_a_vanished_item_restarts_the_lap() {
            let p = advance(&list(), Some(99), 400, 300).unwrap();
            assert_eq!((p.index, p.position_ms), (0, 300));
            assert!(p.changed);
            let first = advance(&list(), None, 0, 0).unwrap();
            assert_eq!((first.index, first.position_ms), (0, 0));
            assert!(first.changed);
            // The item went from playable to unknown length: same rule.
            let l = vec![slot(1, None), slot(2, Some(1000))];
            let p = advance(&l, Some(1), 500, 100).unwrap();
            assert_eq!((p.index, p.position_ms), (1, 100));
        }

        #[test]
        fn a_position_past_the_end_is_clamped() {
            let p = advance(&list(), Some(1), 99_999, 10).unwrap();
            assert_eq!((p.index, p.position_ms), (1, 10));
        }

        #[test]
        fn negative_elapsed_time_does_not_move_the_cursor() {
            let p = advance(&list(), Some(2), 500, -5000).unwrap();
            assert_eq!((p.index, p.position_ms), (1, 500));
        }

        #[test]
        fn upcoming_wraps_and_never_repeats_more_than_the_list_has() {
            let l = list();
            let ids = |v: Vec<&Slot>| v.iter().map(|s| s.item_id).collect::<Vec<_>>();
            assert_eq!(ids(upcoming(&l, 0, 3)), vec![2, 3, 1]);
            assert_eq!(ids(upcoming(&l, 2, 2)), vec![1, 2]);
            let two = vec![slot(1, Some(10)), slot(2, Some(10))];
            assert_eq!(ids(upcoming(&two, 0, 3)), vec![2, 1]);
            let one = vec![slot(1, Some(10))];
            assert_eq!(ids(upcoming(&one, 0, 3)), vec![1]);
            let with_gap = vec![slot(1, Some(10)), slot(2, None), slot(3, Some(10))];
            assert_eq!(ids(upcoming(&with_gap, 0, 3)), vec![3, 1]);
            assert!(upcoming(&l, 9, 3).is_empty());
        }
    }
}

use cursor::{Slot, advance, skipped, upcoming};

/// A track as the card shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackInfo {
    pub item_id: i64,
    pub track_id: i64,
    pub title: String,
    pub artist: String,
    pub duration_ms: i64,
}

/// A room's playlist, the schedulable and the skipped items, with the titles for the card.
#[derive(Debug, Clone, Default)]
pub struct Lap {
    pub slots: Vec<Slot>,
    pub titles: Vec<(String, String)>,
}

impl Lap {
    fn info(&self, index: usize) -> Option<TrackInfo> {
        let s = self.slots.get(index)?;
        let (title, artist) = self.titles.get(index)?.clone();
        Some(TrackInfo {
            item_id: s.item_id,
            track_id: s.track_id,
            title,
            artist,
            duration_ms: s.playable_ms()?,
        })
    }
}

fn track_json(t: &TrackInfo) -> Value {
    json!({
        "item_id": t.item_id.to_string(),
        "track_id": t.track_id.to_string(),
        "title": t.title,
        "artist": t.artist,
        "duration_ms": t.duration_ms,
    })
}

/// The `LISTENING_ROOM_UPDATE` payload. `at_ms` is the server's unix time the position is valid at; a client
/// plays from `position_ms + (its now - at_ms)` and corrects itself with the next event.
pub fn room_payload(
    channel: i64,
    guild: i64,
    mode: &str,
    seq: i64,
    at_ms: i64,
    pos: Option<(&cursor::Position, &Lap)>,
) -> Value {
    let mut v = json!({
        "channel_id": channel.to_string(),
        "guild_id": guild.to_string(),
        "mode": mode,
        "seq": seq,
        "at_ms": at_ms,
        "position_ms": 0,
        "now_playing": Value::Null,
        "next": [],
        "skipped": 0,
    });
    if let Some((p, lap)) = pos {
        v["position_ms"] = json!(p.position_ms);
        v["now_playing"] = lap.info(p.index).as_ref().map_or(Value::Null, track_json);
        v["next"] = Value::Array(
            upcoming(&lap.slots, p.index, NEXT_TRACKS)
                .iter()
                .filter_map(|s| {
                    lap.slots
                        .iter()
                        .position(|x| x.item_id == s.item_id)
                        .and_then(|i| lap.info(i))
                })
                .map(|t| track_json(&t))
                .collect(),
        );
    }
    if let Some((_, lap)) = pos {
        v["skipped"] = json!(skipped(&lap.slots));
    }
    v
}

/// What one tick did to one room: the new cursor row and whether to tell the server.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub item_id: Option<i64>,
    pub position_ms: i64,
    pub seq: i64,
    /// Milliseconds until the room must be looked at again.
    pub next_due_ms: i64,
    pub publish: bool,
}

/// The decision for one due room, without a database.
pub fn step(
    lap: &Lap,
    current: Option<i64>,
    position_ms: i64,
    elapsed_ms: i64,
    seq: i64,
    publish_due: bool,
) -> (Step, Option<cursor::Position>) {
    match advance(&lap.slots, current, position_ms, elapsed_ms) {
        Some(p) => {
            let item = lap.slots.get(p.index).map(|s| s.item_id);
            let seq = seq + i64::from(p.changed);
            let step = Step {
                item_id: item,
                position_ms: p.position_ms,
                seq,
                next_due_ms: p.remaining_ms.min(PUBLISH_EVERY_MS).max(1),
                publish: p.changed || publish_due,
            };
            (step, Some(p))
        }
        // Nothing can be scheduled: tell the room once that the cursor is gone, then look every 15 s
        // (a tag read may have filled a duration in).
        None => (
            Step {
                item_id: None,
                position_ms: 0,
                seq: seq + i64::from(current.is_some()),
                next_due_ms: PUBLISH_EVERY_MS,
                publish: current.is_some(),
            },
            None,
        ),
    }
}

const DUE_SQL: &str = "\
SELECT channel_id, guild_id, playlist_id, item_id, position_ms, seq, \
       (EXTRACT(EPOCH FROM (now() - cursor_at)) * 1000)::BIGINT AS elapsed_ms, \
       (EXTRACT(EPOCH FROM now()) * 1000)::BIGINT AS now_ms, \
       (last_published_at IS NULL OR last_published_at <= now() - make_interval(secs => $2)) AS publish_due \
FROM listening_rooms \
WHERE mode = 'cursor' AND next_due_at <= now() \
ORDER BY next_due_at \
LIMIT $1 \
FOR UPDATE SKIP LOCKED";

const SLOTS_SQL: &str = "\
SELECT i.id AS item_id, i.track_id, t.duration_ms, t.title, t.artist \
FROM music_playlist_items i \
JOIN music_tracks t ON t.id = i.track_id AND t.shelf_state = 'listed' \
JOIN storage_mounts m ON m.id = t.mount_id AND m.deleted_at IS NULL AND m.status <> 'disabled' \
     AND ((m.owner_kind = 'guild' AND m.owner_id = $2) OR m.owner_kind = 'platform') \
WHERE i.playlist_id = $1 \
ORDER BY i.position, i.id";

const STORE_SQL: &str = "\
UPDATE listening_rooms SET item_id = $2, position_ms = $3, seq = $4, cursor_at = now(), \
       next_due_at = now() + make_interval(secs => $5), \
       last_published_at = CASE WHEN $6 THEN now() ELSE last_published_at END \
WHERE channel_id = $1";

/// Load a room's playlist with the tracks it may play.
pub async fn load_lap(pool: &PgPool, playlist: i64, guild: i64) -> Result<Lap, sqlx::Error> {
    let rows = sqlx::query(SLOTS_SQL)
        .bind(playlist)
        .bind(guild)
        .fetch_all(pool)
        .await?;
    let mut lap = Lap::default();
    for r in rows {
        let duration: Option<i32> = r.try_get("duration_ms")?;
        lap.slots.push(Slot {
            item_id: r.try_get("item_id")?,
            track_id: r.try_get("track_id")?,
            duration_ms: duration.map(i64::from),
        });
        lap.titles.push((r.try_get("title")?, r.try_get("artist")?));
    }
    Ok(lap)
}

/// One pass over the due rooms. Returns the events to publish (after the cursors are stored).
pub async fn tick(pool: &PgPool) -> Result<Vec<DomainEvent>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let due = sqlx::query(DUE_SQL)
        .bind(MAX_ROOMS_PER_TICK)
        .bind(PUBLISH_EVERY_MS as f64 / 1000.0)
        .fetch_all(&mut *tx)
        .await?;
    let mut events = Vec::new();
    for r in due {
        let channel: i64 = r.try_get("channel_id")?;
        let guild: i64 = r.try_get("guild_id")?;
        let playlist: i64 = r.try_get("playlist_id")?;
        let item: Option<i64> = r.try_get("item_id")?;
        let position: i64 = r.try_get("position_ms")?;
        let seq: i64 = r.try_get("seq")?;
        let elapsed: i64 = r.try_get("elapsed_ms")?;
        let now_ms: i64 = r.try_get("now_ms")?;
        let publish_due: bool = r.try_get("publish_due")?;
        let lap = load_lap(pool, playlist, guild).await?;
        let (st, pos) = step(&lap, item, position, elapsed, seq, publish_due);
        sqlx::query(STORE_SQL)
            .bind(channel)
            .bind(st.item_id)
            .bind(st.position_ms)
            .bind(st.seq)
            .bind(st.next_due_ms as f64 / 1000.0)
            .bind(st.publish)
            .execute(&mut *tx)
            .await?;
        if st.publish {
            events.push(DomainEvent::ListeningRoomUpdated {
                guild: GuildId::new(u64::try_from(guild).unwrap_or(0)),
                channel: ChannelId::new(u64::try_from(channel).unwrap_or(0)),
                payload: room_payload(
                    channel,
                    guild,
                    "cursor",
                    st.seq,
                    now_ms,
                    pos.as_ref().map(|p| (p, &lap)),
                ),
            });
        }
    }
    tx.commit().await?;
    Ok(events)
}

async fn rooms_handler(pool: PgPool, bus: Arc<dyn EventBus>, job: Job) -> Result<(), HandlerError> {
    match tick(&pool).await {
        Ok(events) => {
            for ev in events {
                for topic in ev.topics() {
                    if let Err(e) = bus.publish(topic, ev.clone()).await {
                        tracing::warn!(error = %e, "publishing LISTENING_ROOM_UPDATE failed");
                    }
                }
            }
        }
        // The next tick retries from the stored cursors; a failure must not stop the singleton.
        Err(e) => tracing::warn!(error = %e, "listening room tick failed"),
    }
    jobs::reschedule_running(&pool, job.id, TICK_INTERVAL).await?;
    Ok(())
}

/// Register `jukebox.rooms`. Pair with [`schedule`].
pub fn register(runner: JobRunner, pool: &PgPool, bus: Arc<dyn EventBus>) -> JobRunner {
    let pool = pool.clone();
    runner.register(ROOMS_KIND, 1, move |job| {
        Box::pin(rooms_handler(pool.clone(), bus.clone(), job))
    })
}

/// Create or re-arm the singleton. Call only when [`register`] is on the runner.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(pool, ROOMS_JOB_ID, ROOMS_KIND, &json!({}), TICK_INTERVAL).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::cursor::Slot;
    use super::*;

    fn lap() -> Lap {
        let mk = |id: i64, d: Option<i64>, t: &str| {
            (
                Slot {
                    item_id: id,
                    track_id: id * 10,
                    duration_ms: d,
                },
                (t.to_owned(), format!("artist {id}")),
            )
        };
        let rows = [
            mk(1, Some(20_000), "One"),
            mk(2, None, "Two"),
            mk(3, Some(30_000), "Three"),
            mk(4, Some(40_000), "Four"),
            mk(5, Some(50_000), "Five"),
        ];
        Lap {
            slots: rows.iter().map(|r| r.0.clone()).collect(),
            titles: rows.iter().map(|r| r.1.clone()).collect(),
        }
    }

    #[test]
    fn the_job_id_is_the_reserved_one() {
        assert_eq!(ROOMS_JOB_ID, 1580);
        assert_eq!(ROOMS_KIND, "jukebox.rooms");
    }

    #[test]
    fn a_quiet_tick_inside_a_track_publishes_only_when_due() {
        let l = lap();
        let (s, _) = step(&l, Some(1), 1000, 2000, 4, false);
        assert_eq!((s.item_id, s.position_ms, s.seq), (Some(1), 3000, 4));
        assert!(!s.publish);
        assert_eq!(
            s.next_due_ms, 15_000,
            "the next look is the 15 s publication"
        );
        let (s, _) = step(&l, Some(1), 1000, 2000, 4, true);
        assert!(s.publish);
    }

    #[test]
    fn a_track_change_publishes_at_once_and_bumps_seq() {
        let l = lap();
        let (s, p) = step(&l, Some(1), 19_000, 2000, 4, false);
        assert_eq!((s.item_id, s.position_ms, s.seq), (Some(3), 1000, 5));
        assert!(s.publish);
        assert_eq!(p.unwrap().remaining_ms, 29_000);
    }

    #[test]
    fn the_next_look_is_the_nearer_of_the_boundary_and_the_publication() {
        let l = lap();
        let (s, _) = step(&l, Some(1), 19_000, 500, 0, false);
        assert_eq!(s.next_due_ms, 500, "500 ms until the track ends");
    }

    #[test]
    fn an_empty_room_says_so_once_then_checks_every_fifteen_seconds() {
        let empty = Lap::default();
        let (s, p) = step(&empty, Some(3), 100, 50, 7, false);
        assert!(p.is_none() && s.publish);
        assert_eq!((s.item_id, s.seq, s.next_due_ms), (None, 8, 15_000));
        let (again, _) = step(&empty, None, 0, 15_000, 8, true);
        assert!(!again.publish, "silence is not re-announced");
        assert_eq!(again.seq, 8);
    }

    #[test]
    fn the_payload_has_the_card_fields() {
        let l = lap();
        let (s, p) = step(&l, Some(3), 5000, 0, 2, true);
        assert!(s.publish);
        let p = p.unwrap();
        let v = room_payload(77, 5, "cursor", s.seq, 1_700_000_000_000, Some((&p, &l)));
        assert_eq!(v["channel_id"], "77");
        assert_eq!(v["guild_id"], "5");
        assert_eq!(v["position_ms"], 5000);
        assert_eq!(v["now_playing"]["title"], "Three");
        assert_eq!(v["now_playing"]["duration_ms"], 30_000);
        let next: Vec<_> = v["next"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["title"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            next,
            vec!["Four", "Five", "One"],
            "the item without a duration is not listed"
        );
        assert_eq!(v["skipped"], 1);
        let idle = room_payload(77, 5, "cursor", 3, 1, None);
        assert!(idle["now_playing"].is_null() && idle["next"].as_array().unwrap().is_empty());
    }
}
