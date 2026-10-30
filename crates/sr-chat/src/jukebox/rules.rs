//! The jukebox rules, pure functions over a [`Doc`]. Callers hold the session lock; nothing here does I/O.
//!
//! Playback is a function of the server clock: the current track was at `position_ms` at `started_ms`, so
//! its position at `now` is `position_ms + (now - started_ms)` while it plays and `position_ms` while it is
//! paused. When a track's known length is reached the next queued track starts at the exact end instant (not
//! at `now`), so a late reader derives the same schedule as an early one.

use super::model::{
    Doc, END_SLACK_MS, Item, ItemState, JukeboxError, KEEP_PLAYED, LISTENER_TTL_MS, MAX_QUEUED,
    MAX_QUEUED_PER_MEMBER, Mode, NewTrack, VoteOutcome,
};

/// Position in the current track at `now` (0 when nothing plays).
pub fn position_at(doc: &Doc, now: i64) -> i64 {
    let s = &doc.session;
    if s.current.is_none() {
        return 0;
    }
    match (s.paused, s.started_ms) {
        (false, Some(started)) => s.position_ms.saturating_add((now - started).max(0)),
        _ => s.position_ms,
    }
}

/// Whether `seen_ms` still counts as listening at `now`.
fn live(seen_ms: i64, now: i64) -> bool {
    now - seen_ms <= LISTENER_TTL_MS
}

/// Listeners seen within [`LISTENER_TTL_MS`].
pub fn live_listeners(doc: &Doc, now: i64) -> Vec<i64> {
    doc.listeners
        .iter()
        .filter(|(_, seen)| live(*seen, now))
        .map(|(u, _)| *u)
        .collect()
}

/// Whether `user` is a live listener.
pub fn is_listening(doc: &Doc, user: i64, now: i64) -> bool {
    doc.listeners
        .iter()
        .any(|(u, seen)| *u == user && live(*seen, now))
}

/// Record that `user` is listening now (join and heartbeat).
pub fn touch_listener(doc: &mut Doc, user: i64, now: i64) {
    match doc.listeners.iter_mut().find(|(u, _)| *u == user) {
        Some(l) => l.1 = now,
        None => doc.listeners.push((user, now)),
    }
}

/// Remove `user` from the listeners; their votes go with them. Returns whether anything changed.
pub fn drop_listener(doc: &mut Doc, user: i64) -> bool {
    let before = (doc.listeners.len(), doc.votes.len());
    doc.listeners.retain(|(u, _)| *u != user);
    doc.votes.retain(|(_, u)| *u != user);
    if doc.session.dj == Some(user) {
        doc.session.dj = None;
        doc.session.version += 1;
        return true;
    }
    before != (doc.listeners.len(), doc.votes.len())
}

fn bump(doc: &mut Doc) {
    doc.session.version += 1;
}

/// End the current item as `state` and clear the playing fields.
fn finish_current(doc: &mut Doc, state: ItemState) {
    if let Some(id) = doc.session.current.take()
        && let Some(it) = doc.items.iter_mut().find(|i| i.id == id)
    {
        it.state = state;
    }
    doc.session.started_ms = None;
    doc.session.position_ms = 0;
    doc.session.paused = false;
    let keep = doc.session.current;
    doc.votes.retain(|(item, _)| Some(*item) == keep);
}

/// Start the first queued item at `at_ms`. Returns whether one started.
fn start_next(doc: &mut Doc, at_ms: i64) -> bool {
    let Some(next) = doc
        .items
        .iter_mut()
        .filter(|i| i.state == ItemState::Queued)
        .min_by_key(|i| i.seq)
    else {
        return false;
    };
    next.state = ItemState::Playing;
    doc.session.current = Some(next.id);
    doc.session.started_ms = Some(at_ms);
    doc.session.position_ms = 0;
    doc.session.paused = false;
    true
}

fn prune_played(doc: &mut Doc) {
    let done: Vec<(i64, i64)> = doc
        .items
        .iter()
        .filter(|i| matches!(i.state, ItemState::Played | ItemState::Skipped))
        .map(|i| (i.seq, i.id))
        .collect();
    if done.len() <= KEEP_PLAYED {
        return;
    }
    let mut ordered = done;
    ordered.sort_unstable();
    let drop: Vec<i64> = ordered[..ordered.len() - KEEP_PLAYED]
        .iter()
        .map(|(_, id)| *id)
        .collect();
    doc.items.retain(|i| !drop.contains(&i.id));
}

/// Whether [`settle`] would change `doc` at `now` (so a reader knows to take the lock).
pub fn needs_settle(doc: &Doc, now: i64) -> bool {
    let mut probe = doc.clone();
    settle(&mut probe, now)
}

/// Bring the doc to `now`: finish tracks whose known end has passed, start the next at the end instant,
/// start a waiting track on an idle deck, and drop a DJ who stopped listening. Returns whether it changed.
pub fn settle(doc: &mut Doc, now: i64) -> bool {
    let mut changed = false;
    // Bounded: every pass consumes a queued item.
    for _ in 0..=(MAX_QUEUED + 1) {
        let Some(cur) = doc.current() else {
            if doc.session.current.is_some() {
                // The current row vanished (pruned or removed): reset.
                finish_current(doc, ItemState::Skipped);
                changed = true;
            }
            if doc.queued().next().is_some() && start_next(doc, now) {
                changed = true;
                continue;
            }
            break;
        };
        if doc.session.paused {
            break;
        }
        let (Some(len), Some(started)) = (cur.known_len(), doc.session.started_ms) else {
            break;
        };
        let end = started + (len - doc.session.position_ms).max(0);
        if now < end {
            break;
        }
        finish_current(doc, ItemState::Played);
        start_next(doc, end);
        changed = true;
    }
    if let Some(dj) = doc.session.dj
        && !is_listening(doc, dj, now)
    {
        doc.session.dj = None;
        changed = true;
    }
    let before = doc.listeners.len();
    doc.listeners
        .retain(|(_, seen)| now - seen <= LISTENER_TTL_MS * 10);
    if doc.listeners.len() != before {
        changed = true;
    }
    let items = doc.items.len();
    prune_played(doc);
    if doc.items.len() != items {
        changed = true;
    }
    if changed {
        bump(doc);
    }
    changed
}

/// Queue `tracks` for `user` (who holds `SPEAK`, checked by the caller). Returns the new row ids. Starts the
/// first one at once when the deck is idle.
pub fn enqueue(
    doc: &mut Doc,
    user: i64,
    tracks: &[NewTrack],
    ids: &mut dyn FnMut() -> i64,
    now: i64,
) -> Result<Vec<i64>, JukeboxError> {
    let waiting = doc.queued().count();
    if waiting + tracks.len() > MAX_QUEUED {
        return Err(JukeboxError::QueueFull);
    }
    let mine = doc.queued().filter(|i| i.added_by == Some(user)).count();
    if mine + tracks.len() > MAX_QUEUED_PER_MEMBER {
        return Err(JukeboxError::MemberQueueFull);
    }
    let first = doc.items.iter().map(|i| i.seq).max().unwrap_or(0) + 1;
    let mut out = Vec::with_capacity(tracks.len());
    for (seq, t) in (first..).zip(tracks) {
        let id = ids();
        doc.items.push(Item {
            id,
            seq,
            track_id: Some(t.track_id),
            mount_id: t.mount_id,
            key: t.key.clone(),
            title: t.title.clone(),
            artist: t.artist.clone(),
            duration_ms: t.duration_ms,
            added_by: Some(user),
            state: ItemState::Queued,
            added_ms: now,
        });
        out.push(id);
    }
    touch_listener(doc, user, now);
    if doc.session.current.is_none() {
        start_next(doc, now);
    }
    bump(doc);
    Ok(out)
}

/// Remove a queue row. A waiting row may be removed by whoever queued it, the DJ and a moderator
/// (`privileged`); removing the playing row is a skip and needs `privileged` or being its adder.
pub fn remove(
    doc: &mut Doc,
    item_id: i64,
    user: i64,
    privileged: bool,
    now: i64,
) -> Result<(), JukeboxError> {
    let it = doc
        .items
        .iter()
        .find(|i| i.id == item_id)
        .ok_or(JukeboxError::NotFound)?;
    let own = it.added_by == Some(user);
    if !(own || privileged) {
        return Err(JukeboxError::NotAllowed);
    }
    match it.state {
        ItemState::Queued => {
            doc.items.retain(|i| i.id != item_id);
            doc.votes.retain(|(i, _)| *i != item_id);
        }
        ItemState::Playing => {
            finish_current(doc, ItemState::Skipped);
            start_next(doc, now);
        }
        ItemState::Played | ItemState::Skipped => return Err(JukeboxError::NotFound),
    }
    bump(doc);
    Ok(())
}

/// Votes standing for the current track from live listeners, and how many are needed at `percent`.
pub fn tally(doc: &Doc, now: i64, percent: i64) -> (usize, usize) {
    let live_set = live_listeners(doc, now);
    let electorate = live_set.len().max(1);
    let pct = usize::try_from(percent.clamp(1, 100)).unwrap_or(50);
    let needed = (electorate * pct).div_ceil(100).max(1);
    let votes = match doc.session.current {
        Some(cur) => doc
            .votes
            .iter()
            .filter(|(i, u)| *i == cur && live_set.contains(u))
            .count(),
        None => 0,
    };
    (votes, needed)
}

/// `user` votes to skip the current track. Counts toward the ratio of live listeners; reaching it skips.
pub fn vote_skip(
    doc: &mut Doc,
    user: i64,
    now: i64,
    percent: i64,
) -> Result<VoteOutcome, JukeboxError> {
    let cur = doc.session.current.ok_or(JukeboxError::NothingPlaying)?;
    touch_listener(doc, user, now);
    if !doc.votes.contains(&(cur, user)) {
        doc.votes.push((cur, user));
    }
    let (votes, needed) = tally(doc, now, percent);
    if votes >= needed {
        finish_current(doc, ItemState::Skipped);
        start_next(doc, now);
        bump(doc);
        return Ok(VoteOutcome {
            votes,
            needed,
            skipped: true,
        });
    }
    bump(doc);
    Ok(VoteOutcome {
        votes,
        needed,
        skipped: false,
    })
}

/// Take back `user`'s vote on the current track. Returns whether one stood.
pub fn withdraw_vote(doc: &mut Doc, user: i64) -> bool {
    let before = doc.votes.len();
    doc.votes.retain(|(_, u)| *u != user);
    let changed = doc.votes.len() != before;
    if changed {
        bump(doc);
    }
    changed
}

/// Skip the current track outright (the DJ, a moderator, or its adder; the caller decides).
pub fn skip_now(doc: &mut Doc, now: i64) -> Result<(), JukeboxError> {
    if doc.session.current.is_none() {
        return Err(JukeboxError::NothingPlaying);
    }
    finish_current(doc, ItemState::Skipped);
    start_next(doc, now);
    bump(doc);
    Ok(())
}

/// A client reports that `item_id` reached its end. Honoured when it is the playing item and its known end is
/// within [`END_SLACK_MS`] (or its length is unknown); anything else is ignored. Returns whether it moved on.
pub fn ended(doc: &mut Doc, item_id: i64, now: i64) -> bool {
    if doc.session.current != Some(item_id) {
        return false;
    }
    if let Some(len) = doc.current().and_then(Item::known_len)
        && position_at(doc, now) + END_SLACK_MS < len
    {
        return false;
    }
    finish_current(doc, ItemState::Played);
    start_next(doc, now);
    bump(doc);
    true
}

/// Pause the current track (position freezes).
pub fn pause(doc: &mut Doc, now: i64) -> Result<(), JukeboxError> {
    if doc.session.current.is_none() {
        return Err(JukeboxError::NothingPlaying);
    }
    if !doc.session.paused {
        doc.session.position_ms = position_at(doc, now);
        doc.session.started_ms = None;
        doc.session.paused = true;
        bump(doc);
    }
    Ok(())
}

/// Resume the current track from the frozen position.
pub fn resume(doc: &mut Doc, now: i64) -> Result<(), JukeboxError> {
    if doc.session.current.is_none() {
        return Err(JukeboxError::NothingPlaying);
    }
    if doc.session.paused {
        doc.session.started_ms = Some(now);
        doc.session.paused = false;
        bump(doc);
    }
    Ok(())
}

/// Switch between stream and synced. The caller checks who may.
pub fn set_mode(doc: &mut Doc, mode: Mode) {
    if doc.session.mode != mode {
        doc.session.mode = mode;
        bump(doc);
    }
}

/// Hand the decks to `target` (`None` releases them). `target` must be listening.
pub fn set_dj(doc: &mut Doc, target: Option<i64>, now: i64) -> Result<(), JukeboxError> {
    if let Some(t) = target
        && !is_listening(doc, t, now)
    {
        return Err(JukeboxError::NotListening);
    }
    if doc.session.dj != target {
        doc.session.dj = target;
        bump(doc);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(n: i64, len: Option<i64>) -> NewTrack {
        NewTrack {
            track_id: n,
            mount_id: 7,
            key: format!("music/{n}.flac"),
            title: format!("t{n}"),
            artist: "a".into(),
            duration_ms: len,
        }
    }

    fn ids() -> impl FnMut() -> i64 {
        let mut n = 100;
        move || {
            n += 1;
            n
        }
    }

    fn doc() -> Doc {
        Doc::new(1, 2)
    }

    #[test]
    fn the_first_track_starts_at_once_and_the_rest_wait() {
        let mut d = doc();
        let out = enqueue(
            &mut d,
            10,
            &[track(1, Some(1000)), track(2, Some(1000))],
            &mut ids(),
            5_000,
        )
        .unwrap();
        assert_eq!(out, vec![101, 102]);
        assert_eq!(d.session.current, Some(101));
        assert_eq!(d.session.started_ms, Some(5_000));
        assert_eq!(d.queued().count(), 1);
        assert_eq!(position_at(&d, 5_400), 400);
    }

    #[test]
    fn a_track_that_ended_starts_the_next_at_the_end_instant() {
        let mut d = doc();
        enqueue(
            &mut d,
            10,
            &[track(1, Some(1000)), track(2, Some(2000))],
            &mut ids(),
            0,
        )
        .unwrap();
        assert!(settle(&mut d, 1_500));
        assert_eq!(d.session.current, Some(102));
        assert_eq!(d.session.started_ms, Some(1_000));
        assert_eq!(position_at(&d, 1_500), 500);
        // Long idle: both finished, the deck is empty and no phantom position remains.
        assert!(settle(&mut d, 9_000));
        assert_eq!(d.session.current, None);
        assert_eq!(position_at(&d, 9_000), 0);
        assert!(!settle(&mut d, 9_001));
    }

    #[test]
    fn two_readers_derive_the_same_position() {
        let mut a = doc();
        enqueue(&mut a, 10, &[track(1, Some(10_000))], &mut ids(), 1_000).unwrap();
        let b = a.clone();
        assert_eq!(position_at(&a, 6_000), 5_000);
        assert_eq!(position_at(&b, 6_000), 5_000);
    }

    #[test]
    fn an_unknown_length_waits_for_the_ended_report_or_a_skip() {
        let mut d = doc();
        enqueue(&mut d, 10, &[track(1, None), track(2, None)], &mut ids(), 0).unwrap();
        settle(&mut d, 10_000_000);
        assert_eq!(d.session.current, Some(101));
        // A stale item id is ignored.
        assert!(!ended(&mut d, 102, 10));
        assert!(ended(&mut d, 101, 10));
        assert_eq!(d.session.current, Some(102));
    }

    #[test]
    fn ended_is_ignored_while_a_known_track_is_far_from_its_end() {
        let mut d = doc();
        enqueue(&mut d, 10, &[track(1, Some(60_000))], &mut ids(), 0).unwrap();
        assert!(!ended(&mut d, 101, 1_000));
        assert!(ended(&mut d, 101, 58_000));
        assert_eq!(d.session.current, None);
    }

    #[test]
    fn pause_freezes_the_position_and_resume_continues() {
        let mut d = doc();
        enqueue(&mut d, 10, &[track(1, Some(60_000))], &mut ids(), 0).unwrap();
        pause(&mut d, 4_000).unwrap();
        assert_eq!(position_at(&d, 30_000), 4_000);
        assert!(!settle(&mut d, 500_000), "a paused track never ends");
        resume(&mut d, 10_000).unwrap();
        assert_eq!(position_at(&d, 12_000), 6_000);
        assert_eq!(pause(&mut doc(), 0), Err(JukeboxError::NothingPlaying));
    }

    #[test]
    fn the_queue_caps_hold_per_member_and_in_total() {
        let mut d = doc();
        let many: Vec<NewTrack> = (0..26).map(|n| track(n, Some(1000))).collect();
        // 26 tracks: one plays at once, 25 wait: the per-member cap counts waiting rows.
        assert_eq!(
            enqueue(&mut d, 10, &many, &mut ids(), 0),
            Err(JukeboxError::MemberQueueFull)
        );
        let ok: Vec<NewTrack> = (0..25).map(|n| track(n, Some(1000))).collect();
        enqueue(&mut d, 10, &ok, &mut ids(), 0).unwrap();
        // The first of them plays, 24 wait: one more fits, the next does not.
        enqueue(&mut d, 10, &[track(98, None)], &mut ids(), 0).unwrap();
        assert_eq!(
            enqueue(&mut d, 10, &[track(99, None)], &mut ids(), 0),
            Err(JukeboxError::MemberQueueFull)
        );
        for u in 11..=17 {
            enqueue(&mut d, u, &ok, &mut ids(), 0).unwrap();
        }
        // 25 + 7 x 25 = 200 waiting: the queue is full for everyone.
        assert_eq!(
            enqueue(&mut d, 18, &[track(97, None)], &mut ids(), 0),
            Err(JukeboxError::QueueFull)
        );
    }

    #[test]
    fn vote_skip_needs_the_ratio_of_live_listeners() {
        let mut d = doc();
        enqueue(
            &mut d,
            10,
            &[track(1, Some(1e6 as i64)), track(2, Some(1000))],
            &mut ids(),
            0,
        )
        .unwrap();
        for u in [11, 12, 13] {
            touch_listener(&mut d, u, 0);
        }
        // Four live listeners at 50%: two votes skip.
        let v = vote_skip(&mut d, 11, 100, 50).unwrap();
        assert_eq!((v.votes, v.needed, v.skipped), (1, 2, false));
        // The same member voting again changes nothing.
        let v = vote_skip(&mut d, 11, 100, 50).unwrap();
        assert_eq!((v.votes, v.skipped), (1, false));
        let v = vote_skip(&mut d, 12, 100, 50).unwrap();
        assert!(v.skipped);
        assert_eq!(d.session.current, Some(102));
        assert!(d.votes.is_empty(), "votes belong to the skipped track");
    }

    #[test]
    fn a_vote_from_a_listener_who_left_does_not_count() {
        let mut d = doc();
        enqueue(&mut d, 10, &[track(1, Some(1e6 as i64))], &mut ids(), 0).unwrap();
        touch_listener(&mut d, 11, 0);
        vote_skip(&mut d, 11, 10, 100).unwrap();
        let (votes, needed) = tally(&d, 10 + LISTENER_TTL_MS + 1, 100);
        // Both listeners are stale now: the electorate falls to the floor of one and nobody's vote counts.
        assert_eq!((votes, needed), (0, 1));
        assert!(withdraw_vote(&mut d, 11));
        assert!(!withdraw_vote(&mut d, 11));
    }

    #[test]
    fn a_lone_listener_skips_alone_and_nothing_playing_refuses() {
        let mut d = doc();
        assert_eq!(
            vote_skip(&mut d, 10, 0, 50),
            Err(JukeboxError::NothingPlaying)
        );
        enqueue(&mut d, 10, &[track(1, Some(1e6 as i64))], &mut ids(), 0).unwrap();
        assert!(vote_skip(&mut d, 10, 1, 50).unwrap().skipped);
        assert_eq!(d.session.current, None);
    }

    #[test]
    fn removing_follows_who_queued_it() {
        let mut d = doc();
        let mut next = ids();
        enqueue(&mut d, 10, &[track(1, Some(1e6 as i64))], &mut next, 0).unwrap();
        enqueue(&mut d, 11, &[track(2, Some(1000))], &mut next, 0).unwrap();
        assert_eq!(
            remove(&mut d, 102, 12, false, 1),
            Err(JukeboxError::NotAllowed)
        );
        assert_eq!(
            remove(&mut d, 999, 12, true, 1),
            Err(JukeboxError::NotFound)
        );
        remove(&mut d, 102, 11, false, 1).unwrap();
        assert_eq!(d.queued().count(), 0);
        // Removing the playing row is a skip; its adder may do it.
        remove(&mut d, 101, 10, false, 2).unwrap();
        assert_eq!(d.session.current, None);
    }

    #[test]
    fn the_dj_must_be_listening_and_leaves_with_their_heartbeat() {
        let mut d = doc();
        assert_eq!(set_dj(&mut d, Some(10), 0), Err(JukeboxError::NotListening));
        touch_listener(&mut d, 10, 0);
        set_dj(&mut d, Some(10), 0).unwrap();
        assert_eq!(d.session.dj, Some(10));
        // Heartbeats stop: settle hands the decks back.
        assert!(settle(&mut d, LISTENER_TTL_MS + 1));
        assert_eq!(d.session.dj, None);
        touch_listener(&mut d, 10, 0);
        set_dj(&mut d, Some(10), 0).unwrap();
        assert!(drop_listener(&mut d, 10));
        assert_eq!(d.session.dj, None);
    }

    #[test]
    fn played_history_is_pruned_and_the_version_moves_on_change_only() {
        let mut d = doc();
        let many: Vec<NewTrack> = (0..25).map(|n| track(n, Some(10))).collect();
        enqueue(&mut d, 10, &many, &mut ids(), 0).unwrap();
        let v = d.session.version;
        assert!(settle(&mut d, 10_000));
        assert!(d.session.version > v);
        assert!(d.items.len() <= KEEP_PLAYED);
        let v = d.session.version;
        assert!(!settle(&mut d, 10_001));
        assert_eq!(d.session.version, v);
        assert!(!needs_settle(&d, 10_001));
    }
}
