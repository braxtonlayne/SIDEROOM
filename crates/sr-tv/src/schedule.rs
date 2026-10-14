//! Pure edits of a channel's list of blocks: placing a programme over what is there, and fitting
//! generated blocks around blocks staff placed. Nothing here touches storage; the service turns a
//! [`Change`] into repository calls.

use crate::model::BlockRecord;

/// Rows to delete and rows to insert or replace (by id).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Change {
    pub delete: Vec<i64>,
    pub upsert: Vec<BlockRecord>,
}

/// Cut the part of `e` before `at` (keeps the id).
fn keep_head(e: &BlockRecord, at: i64) -> BlockRecord {
    let mut head = e.clone();
    head.end_ms = at;
    // The generator state belongs to the end of the original block.
    head.cursor_after = None;
    head
}

/// Cut the part of `e` from `at` on (new id `id`, or `e`'s own when `keep_id`).
fn keep_tail(e: &BlockRecord, at: i64, id: i64) -> BlockRecord {
    let mut tail = e.clone();
    tail.id = id;
    tail.start_ms = at;
    tail.offset_ms = e.offset_ms + (at - e.start_ms);
    tail.seq_base = None;
    tail.disc_base = None;
    tail
}

/// Place `new` over `existing` (the channel's blocks, non-overlapping). Blocks it covers are deleted,
/// blocks it clips are shortened (a head cut moves the start and the offset into the asset), and a block
/// that contains it is split in two. `new_id` supplies ids for the second half of a split.
pub fn place(
    existing: &[BlockRecord],
    new: BlockRecord,
    new_id: &mut dyn FnMut() -> i64,
) -> Change {
    let mut change = Change::default();
    for e in existing
        .iter()
        .filter(|e| e.channel_id == new.channel_id && e.id != new.id)
    {
        if e.end_ms <= new.start_ms || e.start_ms >= new.end_ms {
            continue;
        }
        let before = e.start_ms < new.start_ms;
        let after = e.end_ms > new.end_ms;
        match (before, after) {
            (false, false) => change.delete.push(e.id),
            (true, false) => change.upsert.push(keep_head(e, new.start_ms)),
            (false, true) => {
                // The tail keeps the id: the row's start moves forward.
                change.upsert.push(keep_tail(e, new.end_ms, e.id));
            }
            (true, true) => {
                change.upsert.push(keep_head(e, new.start_ms));
                change.upsert.push(keep_tail(e, new.end_ms, new_id()));
            }
        }
    }
    change.upsert.push(new);
    change
}

/// Cut `rec` around busy intervals (sorted by start). Returns the free pieces: the first keeps the id,
/// later ones get ids from `new_id`; only the last piece (when it still ends where `rec` ended) keeps
/// the generator state.
pub fn fit_around(
    rec: BlockRecord,
    busy: &[(i64, i64)],
    new_id: &mut dyn FnMut() -> i64,
) -> Vec<BlockRecord> {
    let mut pieces: Vec<(i64, i64)> = vec![(rec.start_ms, rec.end_ms)];
    for &(bs, be) in busy {
        pieces = pieces
            .into_iter()
            .flat_map(|(s, e)| {
                if be <= s || bs >= e {
                    vec![(s, e)]
                } else {
                    let mut out = Vec::new();
                    if bs > s {
                        out.push((s, bs));
                    }
                    if be < e {
                        out.push((be, e));
                    }
                    out
                }
            })
            .collect();
    }
    let mut out = Vec::new();
    for (n, (s, e)) in pieces.into_iter().enumerate() {
        let mut p = rec.clone();
        p.id = if n == 0 { rec.id } else { new_id() };
        p.start_ms = s;
        p.end_ms = e;
        p.offset_ms = rec.offset_ms + (s - rec.start_ms);
        if e != rec.end_ms {
            p.cursor_after = None;
        }
        out.push(p);
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::model::{BlockKind, BlockSource};

    pub fn rec(id: i64, start: i64, end: i64) -> BlockRecord {
        BlockRecord {
            id,
            channel_id: 1,
            start_ms: start,
            end_ms: end,
            kind: BlockKind::Program,
            title_id: Some(id),
            episode_id: None,
            asset_id: Some(id * 10),
            title: format!("T{id}"),
            episode_title: None,
            media_kind: None,
            season_number: None,
            episode_number: None,
            year: None,
            age_restricted: false,
            offset_ms: 0,
            source: BlockSource::Generated,
            cursor_after: Some(crate::model::Cursor::default()),
            seq_base: Some(5),
            disc_base: Some(1),
        }
    }

    fn ids() -> impl FnMut() -> i64 {
        let mut n = 100;
        move || {
            n += 1;
            n
        }
    }

    #[test]
    fn placing_deletes_trims_and_splits() {
        let existing = vec![rec(1, 0, 100), rec(2, 100, 200), rec(3, 200, 300)];
        // Covers block 2 entirely and clips both neighbours.
        let c = place(&existing, rec(9, 50, 250), &mut ids());
        assert_eq!(c.delete, vec![2]);
        let head = c.upsert.iter().find(|b| b.id == 1).unwrap();
        assert_eq!((head.start_ms, head.end_ms), (0, 50));
        assert!(head.cursor_after.is_none());
        let tail = c.upsert.iter().find(|b| b.id == 3).unwrap();
        assert_eq!((tail.start_ms, tail.end_ms, tail.offset_ms), (250, 300, 50));
        assert!(tail.seq_base.is_none(), "a moved start needs new HLS bases");
        assert_eq!(c.upsert.last().unwrap().id, 9);

        // Inside one block: split in two.
        let c = place(&existing, rec(9, 120, 140), &mut ids());
        assert!(c.delete.is_empty());
        let left = c.upsert.iter().find(|b| b.id == 2).unwrap();
        let right = c.upsert.iter().find(|b| b.id == 101).unwrap();
        assert_eq!((left.start_ms, left.end_ms), (100, 120));
        assert_eq!(
            (right.start_ms, right.end_ms, right.offset_ms),
            (140, 200, 40)
        );
        assert!(
            right.cursor_after.is_some(),
            "the right half still ends the original block"
        );

        // Nothing overlapped: just the new block.
        let c = place(&existing, rec(9, 300, 400), &mut ids());
        assert_eq!(c.upsert.len(), 1);
        assert!(c.delete.is_empty());
    }

    #[test]
    fn generated_blocks_fit_around_busy_time() {
        let g = rec(1, 0, 100);
        let pieces = fit_around(g.clone(), &[(20, 30), (60, 200)], &mut ids());
        let spans: Vec<_> = pieces
            .iter()
            .map(|p| (p.id, p.start_ms, p.end_ms, p.offset_ms))
            .collect();
        assert_eq!(spans, [(1, 0, 20, 0), (101, 30, 60, 30)]);
        assert!(pieces.iter().all(|p| p.cursor_after.is_none()));
        // Untouched.
        let same = fit_around(g.clone(), &[(100, 120)], &mut ids());
        assert_eq!(same, vec![g.clone()]);
        // Fully covered: nothing left.
        assert!(fit_around(g, &[(0, 100)], &mut ids()).is_empty());
    }
}
