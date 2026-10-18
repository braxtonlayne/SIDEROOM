//! The schedule engine (TV-ENG-003..006, 008): a pure, deterministic function from rules, a catalog
//! snapshot, a time window and a [`Cursor`] to schedule blocks.
//!
//! * Same inputs, same blocks: the shuffle uses a seeded generator whose state lives in the cursor, nothing
//!   reads a clock or the environment.
//! * Rolling: the result carries the cursor after the last block. Generating `[a, b)` and then continuing
//!   from that cursor to `c` gives the same blocks as generating `[a, c)` in one go, so a schedule can be
//!   extended day by day without drift (sequential series marathons keep counting, shuffles keep their
//!   no-repeat window).
//! * Times of day in the rules are channel time ([`Zone`]), including across daylight-saving changes.
//! * Programmes start on the grid (`grid_minutes`) in channel time; the gap after a programme that ends
//!   off the grid is filled by the filler policy (bumpers clipped to the grid, a slate, colour bars) or,
//!   for `next_program_early`, closed by starting the next programme at once.

use crate::catalog::CatalogItem;
use crate::model::parse_hm;
use crate::model::{BlockKind, ChannelRules, Cursor, Daypart, FillerPolicy, Fit, PickMode};
use crate::tz::{MS_DAY, Zone};

/// Gaps shorter than this are not worth aligning for.
const MIN_ALIGN_GAP_MS: i64 = 15_000;
/// Bumpers fill gaps up to this long; longer gaps get one slate block.
const MAX_BUMPER_GAP_MS: i64 = 30 * 60_000;
const RECENT_CAP: usize = 64;

pub const SLATE_TITLE: &str = "We'll be right back";
pub const OFF_AIR_TITLE: &str = "Off air";

/// The part of a channel the engine needs.
pub struct EngineChannel<'a> {
    pub zone: Zone,
    pub rules: &'a ChannelRules,
    pub filler: FillerPolicy,
    pub seed: u64,
}

/// What the catalog offers, index-aligned with `rules.dayparts`.
#[derive(Default)]
pub struct Pools {
    pub dayparts: Vec<Vec<CatalogItem>>,
    pub bumpers: Vec<CatalogItem>,
}

/// A generated block, before it gets an id.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub start_ms: i64,
    pub end_ms: i64,
    pub kind: BlockKind,
    /// The programme or bumper; `None` for slates and colour bars.
    pub item: Option<CatalogItem>,
    pub cursor_after: Cursor,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Generated {
    pub blocks: Vec<Draft>,
    pub cursor: Cursor,
    pub warnings: Vec<String>,
}

struct Part {
    start: i64,
    end: i64,
    wraps: bool,
    days: [bool; 7],
}

impl Part {
    fn new(d: &Daypart) -> Self {
        let start = parse_hm(&d.start).unwrap_or(0);
        let end = parse_hm(&d.end).unwrap_or(0);
        let mut days = [d.days.is_empty(); 7];
        for x in &d.days {
            if let Some(slot) = days.get_mut(usize::from(*x)) {
                *slot = true;
            }
        }
        Self {
            start,
            end,
            wraps: end <= start,
            days,
        }
    }
}

struct Run<'a> {
    ch: &'a EngineChannel<'a>,
    pools: &'a Pools,
    parts: Vec<Part>,
    grid_ms: i64,
    cursor: Cursor,
    blocks: Vec<Draft>,
    warnings: Vec<String>,
}

fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Run<'_> {
    fn zone(&self) -> &Zone {
        &self.ch.zone
    }

    fn align_up(&self, t: i64) -> i64 {
        if self.grid_ms == 0 {
            return t;
        }
        // Instant arithmetic, so the repeated hour of a clock change is not skipped; the loop settles a
        // change of offset between `t` and the grid line.
        let mut a = t;
        for _ in 0..4 {
            let rem = self.zone().to_local(a).rem_euclid(self.grid_ms);
            if rem == 0 {
                break;
            }
            a += self.grid_ms - rem;
        }
        a
    }

    /// The daypart in force at `t` and the instant it ends.
    fn active(&self, t: i64) -> Option<(usize, i64)> {
        let (day, dow, minute) = self.zone().local_day(t);
        for (i, p) in self.parts.iter().enumerate() {
            let end_local =
                if p.days[usize::from(dow)] && minute >= p.start && (p.wraps || minute < p.end) {
                    Some(day * MS_DAY + (if p.wraps { p.end + 1440 } else { p.end }) * 60_000)
                } else if p.wraps && minute < p.end && p.days[usize::from((dow + 6) % 7)] {
                    Some(day * MS_DAY + p.end * 60_000)
                } else {
                    None
                };
            if let Some(local) = end_local {
                let end = self.zone().from_local(local);
                if end > t {
                    return Some((i, end));
                }
            }
        }
        None
    }

    /// The first daypart start strictly after `t`, looking a week ahead.
    fn next_start(&self, t: i64) -> Option<i64> {
        let (day0, _, _) = self.zone().local_day(t);
        let mut best: Option<i64> = None;
        for d in 0..=8 {
            let day = day0 + d;
            let dow = usize::from(crate::tz::weekday(day));
            for p in &self.parts {
                if p.days[dow] {
                    let s = self.zone().from_local(day * MS_DAY + p.start * 60_000);
                    if s > t && best.is_none_or(|b| s < b) {
                        best = Some(s);
                    }
                }
            }
        }
        best
    }

    fn push(&mut self, start: i64, end: i64, kind: BlockKind, item: Option<CatalogItem>) {
        self.cursor.at_ms = end;
        self.blocks.push(Draft {
            start_ms: start,
            end_ms: end,
            kind,
            item,
            cursor_after: self.cursor.clone(),
        });
    }

    /// Fill `[t0, t1)` according to the filler policy.
    fn fill(&mut self, t0: i64, t1: i64) {
        if t1 <= t0 {
            return;
        }
        match self.ch.filler {
            FillerPolicy::OffAir => self.push(t0, t1, BlockKind::OffAir, None),
            FillerPolicy::Bumpers
                if !self.pools.bumpers.is_empty() && t1 - t0 <= MAX_BUMPER_GAP_MS =>
            {
                let mut t = t0;
                while t < t1 {
                    let n = self.pools.bumpers.len() as u64;
                    let b = self.pools.bumpers[(self.cursor.bumper % n) as usize].clone();
                    self.cursor.bumper = self.cursor.bumper.wrapping_add(1);
                    let end = (t + b.duration_ms).min(t1);
                    self.push(t, end, BlockKind::Filler, Some(b));
                    t = end;
                }
            }
            _ => self.push(t0, t1, BlockKind::Slate, None),
        }
    }

    fn pick(&mut self, idx: usize, remaining: i64) -> Option<CatalogItem> {
        let dp = &self.ch.rules.dayparts[idx];
        let items = &self.pools.dayparts[idx];
        let fits = |i: &CatalogItem| dp.fit == Fit::Overrun || i.duration_ms <= remaining;
        let chosen = match dp.mode {
            PickMode::Sequential => {
                let n = items.len() as u64;
                let at = self.cursor.seq.get(&dp.name).copied().unwrap_or(0);
                let item = &items[(at % n) as usize];
                if !fits(item) {
                    return None;
                }
                self.cursor.seq.insert(dp.name.clone(), at.wrapping_add(1));
                item.clone()
            }
            PickMode::Shuffle => {
                let window = (self.ch.rules.no_repeat_window as usize).min(items.len() - 1);
                let tail_from = self.cursor.recent.len().saturating_sub(window);
                let recent = &self.cursor.recent[tail_from..];
                let mut cands: Vec<usize> = (0..items.len())
                    .filter(|n| fits(&items[*n]) && !recent.contains(&items[*n].key()))
                    .collect();
                if cands.is_empty() {
                    cands = (0..items.len()).filter(|n| fits(&items[*n])).collect();
                }
                if cands.is_empty() {
                    return None;
                }
                let r = splitmix(&mut self.cursor.rng);
                items[cands[(r % cands.len() as u64) as usize]].clone()
            }
        };
        self.cursor.recent.push(chosen.key());
        if self.cursor.recent.len() > RECENT_CAP {
            self.cursor.recent.remove(0);
        }
        Some(chosen)
    }
}

/// Generate blocks from `max(from_ms, cursor.at_ms)` until a block starts at or after `to_ms`. The last
/// block may end after `to_ms`.
pub fn generate(
    ch: &EngineChannel<'_>,
    pools: &Pools,
    from_ms: i64,
    to_ms: i64,
    mut cursor: Cursor,
) -> Generated {
    if cursor.rng == 0 {
        cursor.rng = ch.seed ^ 0xA076_1D64_78BD_642F;
    }
    let mut run = Run {
        ch,
        pools,
        parts: ch.rules.dayparts.iter().map(Part::new).collect(),
        grid_ms: i64::from(ch.rules.grid_minutes) * 60_000,
        cursor,
        blocks: Vec::new(),
        warnings: Vec::new(),
    };
    let mut warned = vec![false; ch.rules.dayparts.len()];
    let mut t = from_ms.max(run.cursor.at_ms);
    while t < to_ms {
        if ch.filler != FillerPolicy::NextProgramEarly {
            let aligned = run.align_up(t);
            if aligned - t >= MIN_ALIGN_GAP_MS {
                run.fill(t, aligned);
                t = aligned;
                if t >= to_ms {
                    break;
                }
            }
        }
        let Some((idx, dp_end)) = run.active(t) else {
            let end = run.next_start(t).unwrap_or(to_ms.max(t + 1));
            let (t0, t1) = (t, end.max(t + 1));
            // A long gap is one block, whatever the policy says about bumpers.
            let policy = run.ch.filler;
            if policy == FillerPolicy::OffAir {
                run.push(t0, t1, BlockKind::OffAir, None);
            } else {
                run.push(t0, t1, BlockKind::Slate, None);
            }
            t = t1;
            continue;
        };
        if pools.dayparts.get(idx).is_none_or(Vec::is_empty) {
            if !warned[idx] {
                warned[idx] = true;
                run.warnings.push(format!(
                    "Daypart {:?} has nothing to air: its pool is empty",
                    ch.rules.dayparts[idx].name
                ));
            }
            run.fill(t, dp_end);
            t = dp_end;
            continue;
        }
        match run.pick(idx, dp_end - t) {
            Some(item) => {
                let end = t + item.duration_ms.max(1000);
                run.push(t, end, BlockKind::Program, Some(item));
                t = end;
            }
            None => {
                run.fill(t, dp_end);
                t = dp_end;
            }
        }
    }
    Generated {
        cursor: run.cursor,
        blocks: run.blocks,
        warnings: run.warnings,
    }
}

// ---- imported guides (TV-ENG-003) ----------------------------------------------------------------

/// One weekly slot of an `imported_guide` channel, with what the catalog offers for it.
#[derive(Debug, Clone)]
pub struct SlotPlan {
    /// The slot id; keys the rotation of a series slot in the cursor.
    pub key: i64,
    /// 0 = Monday, channel time.
    pub weekday: u8,
    pub start_minute: i64,
    pub duration_min: i64,
    /// What can air in the slot: one item for a film or a fixed episode, the episodes in order for a series
    /// (they rotate week by week), nothing when the title is not in the library (the slot is skipped).
    pub items: Vec<CatalogItem>,
}

/// Resolve weekly slots to concrete blocks from `max(from_ms, cursor.at_ms)` until a block starts at or
/// after `to_ms`, deterministically.
///
/// * A slot with nothing to air is skipped: the gap is filled by the filler policy (D-102).
/// * A programme starts at its slot start. If the previous programme overran, it starts when that one ends
///   (the schedule shifts, "overrun"); if that would be at or past the slot's end, the slot is skipped.
/// * A programme shorter than its slot leaves a gap that the filler policy fills (bumpers, a slate or bars;
///   `next_program_early` behaves as a slate here, because a wall-clock grid has fixed starts).
/// * Without a cursor, the slot already on air at `from_ms` is entered at its own start, so a viewer
///   tuning in 17 minutes into it lands 17 minutes in (D-100).
/// * A series slot rotates through its episodes, one per airing, the position kept in the cursor.
pub fn resolve_slots(
    ch: &EngineChannel<'_>,
    bumpers: &[CatalogItem],
    plans: &[SlotPlan],
    from_ms: i64,
    to_ms: i64,
    cursor: Cursor,
) -> Generated {
    let pools = Pools {
        dayparts: Vec::new(),
        bumpers: bumpers.to_vec(),
    };
    let mut run = Run {
        ch,
        pools: &pools,
        parts: Vec::new(),
        grid_ms: 0,
        cursor,
        blocks: Vec::new(),
        warnings: Vec::new(),
    };
    let fresh = run.cursor.at_ms == 0;
    let mut t = from_ms.max(run.cursor.at_ms);
    // Every airing of every slot that can matter: from the day before `t` (a slot past midnight) on.
    let (first_day, _, _) = ch.zone.local_day(t);
    let (last_day, _, _) = ch.zone.local_day(to_ms);
    struct Airing {
        start: i64,
        end: i64,
        plan: usize,
    }
    let mut airings: Vec<Airing> = Vec::new();
    for day in (first_day - 1)..=(last_day + 1) {
        let dow = i64::from(crate::tz::weekday(day));
        for (i, p) in plans.iter().enumerate() {
            if p.items.is_empty() || i64::from(p.weekday) != dow {
                continue;
            }
            let start = ch.zone.from_local(day * MS_DAY + p.start_minute * 60_000);
            airings.push(Airing {
                start,
                end: start + p.duration_min * 60_000,
                plan: i,
            });
        }
    }
    airings.sort_by_key(|a| (a.start, a.plan));
    if fresh && let Some(first) = airings.iter().find(|a| a.end > t) {
        // Enter the slot on air at its own start; nothing earlier is generated.
        t = t.min(first.start);
    }
    for a in &airings {
        if a.end <= t {
            // Swallowed whole by a programme that overran (not merely in the past).
            if run.blocks.last().is_some_and(|b| a.start >= b.start_ms) {
                run.warnings.push(format!(
                    "Slot at minute {} of weekday {} was skipped: the previous programme overran it",
                    plans[a.plan].start_minute, plans[a.plan].weekday
                ));
            }
            continue;
        }
        if a.start >= to_ms {
            break;
        }
        let start = a.start.max(t);
        if start >= a.end {
            run.warnings.push(format!(
                "Slot at minute {} of weekday {} was skipped: the previous programme overran it",
                plans[a.plan].start_minute, plans[a.plan].weekday
            ));
            continue;
        }
        let plan = &plans[a.plan];
        let item = if plan.items.len() == 1 {
            plan.items[0].clone()
        } else {
            let key = format!("slot:{}", plan.key);
            let at = run.cursor.seq.get(&key).copied().unwrap_or(0);
            run.cursor.seq.insert(key, at.wrapping_add(1));
            plan.items[(at % plan.items.len() as u64) as usize].clone()
        };
        if start > t {
            run.fill(t, start);
        }
        let end = start + item.duration_ms.max(1000);
        run.push(start, end, BlockKind::Program, Some(item));
        t = end;
    }
    if t < to_ms {
        // Nothing more in the window: the rest is filler, which a later extension continues from.
        let next = airings.iter().find(|a| a.start >= to_ms).map(|a| a.start);
        run.fill(t, next.unwrap_or(to_ms).max(to_ms));
    }
    Generated {
        cursor: run.cursor,
        blocks: run.blocks,
        warnings: run.warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{episode, item};
    use crate::model::PoolFilter;
    use crate::tz::days_from_civil;

    const H: i64 = 3_600_000;
    const M: i64 = 60_000;

    fn utc(y: i64, m: i64, d: i64, h: i64, min: i64) -> i64 {
        days_from_civil(y, m, d) * MS_DAY + h * H + min * M
    }

    fn part(name: &str, start: &str, end: &str, mode: PickMode, fit: Fit) -> Daypart {
        Daypart {
            name: name.into(),
            start: start.into(),
            end: end.into(),
            days: vec![],
            mode,
            fit,
            pool: PoolFilter::default(),
        }
    }

    fn rules(grid: u32, window: u32, dayparts: Vec<Daypart>) -> ChannelRules {
        ChannelRules {
            grid_minutes: grid,
            no_repeat_window: window,
            dayparts,
        }
    }

    fn run(
        zone: &str,
        rules: &ChannelRules,
        filler: FillerPolicy,
        pools: &Pools,
        from: i64,
        to: i64,
        cursor: Cursor,
    ) -> Generated {
        let ch = EngineChannel {
            zone: Zone::parse(zone).unwrap(),
            rules,
            filler,
            seed: 42,
        };
        generate(&ch, pools, from, to, cursor)
    }

    fn movies(n: i64, minutes: i64) -> Vec<CatalogItem> {
        (1..=n)
            .map(|i| item(i, &format!("Movie {i}"), minutes))
            .collect()
    }

    fn assert_contiguous(g: &Generated) {
        for w in g.blocks.windows(2) {
            assert_eq!(w[0].end_ms, w[1].start_ms, "gap or overlap: {w:?}");
        }
        for b in &g.blocks {
            assert!(b.end_ms > b.start_ms);
        }
    }

    #[test]
    fn the_same_inputs_give_the_same_schedule() {
        let r = rules(
            30,
            3,
            vec![part(
                "All",
                "00:00",
                "24:00",
                PickMode::Shuffle,
                Fit::Overrun,
            )],
        );
        let pools = Pools {
            dayparts: vec![movies(8, 95)],
            bumpers: vec![],
        };
        let a = run(
            "UTC",
            &r,
            FillerPolicy::Slate,
            &pools,
            0,
            3 * MS_DAY,
            Cursor::default(),
        );
        let b = run(
            "UTC",
            &r,
            FillerPolicy::Slate,
            &pools,
            0,
            3 * MS_DAY,
            Cursor::default(),
        );
        assert_eq!(a, b);
        assert!(a.blocks.len() > 30);
        assert_contiguous(&a);
        // A different seed shuffles differently.
        let ch = EngineChannel {
            zone: Zone::parse("UTC").unwrap(),
            rules: &r,
            filler: FillerPolicy::Slate,
            seed: 43,
        };
        let c = generate(&ch, &pools, 0, 3 * MS_DAY, Cursor::default());
        assert_ne!(a.blocks, c.blocks);
    }

    #[test]
    fn continuing_from_the_cursor_equals_one_long_run() {
        let r = rules(
            30,
            3,
            vec![
                part("Day", "06:00", "18:00", PickMode::Sequential, Fit::Overrun),
                part("Night", "18:00", "06:00", PickMode::Shuffle, Fit::Overrun),
            ],
        );
        let eps: Vec<_> = (1..=40).map(|n| episode(7, "Marathon", 1, n, 22)).collect();
        let pools = Pools {
            dayparts: vec![eps, movies(6, 100)],
            bumpers: vec![],
        };
        let whole = run(
            "UTC",
            &r,
            FillerPolicy::Slate,
            &pools,
            0,
            4 * MS_DAY,
            Cursor::default(),
        );
        let first = run(
            "UTC",
            &r,
            FillerPolicy::Slate,
            &pools,
            0,
            2 * MS_DAY,
            Cursor::default(),
        );
        let second = run(
            "UTC",
            &r,
            FillerPolicy::Slate,
            &pools,
            0,
            4 * MS_DAY,
            first.cursor.clone(),
        );
        let mut joined = first.blocks.clone();
        joined.extend(second.blocks);
        assert_eq!(joined, whole.blocks);
        assert_eq!(second.cursor, whole.cursor);
    }

    #[test]
    fn a_marathon_plays_the_episodes_in_order_and_keeps_counting() {
        let r = rules(
            0,
            0,
            vec![part(
                "Marathon",
                "00:00",
                "24:00",
                PickMode::Sequential,
                Fit::Overrun,
            )],
        );
        let eps: Vec<_> = (1..=5).map(|n| episode(7, "Show", 1, n, 30)).collect();
        let pools = Pools {
            dayparts: vec![eps],
            bumpers: vec![],
        };
        let g = run(
            "UTC",
            &r,
            FillerPolicy::NextProgramEarly,
            &pools,
            0,
            4 * H,
            Cursor::default(),
        );
        let numbers: Vec<_> = g
            .blocks
            .iter()
            .map(|b| b.item.as_ref().unwrap().episode_number.unwrap())
            .collect();
        assert_eq!(numbers, [1, 2, 3, 4, 5, 1, 2, 3]);
        assert_eq!(g.cursor.seq["Marathon"], 8);
    }

    #[test]
    fn shuffle_does_not_repeat_inside_the_window() {
        let r = rules(
            0,
            4,
            vec![part(
                "All",
                "00:00",
                "24:00",
                PickMode::Shuffle,
                Fit::Overrun,
            )],
        );
        let pools = Pools {
            dayparts: vec![movies(6, 60)],
            bumpers: vec![],
        };
        let g = run(
            "UTC",
            &r,
            FillerPolicy::NextProgramEarly,
            &pools,
            0,
            6 * MS_DAY,
            Cursor::default(),
        );
        let keys: Vec<i64> = g
            .blocks
            .iter()
            .map(|b| b.item.as_ref().unwrap().key())
            .collect();
        assert!(keys.len() >= 100);
        for w in keys.windows(5) {
            let mut s = w.to_vec();
            s.sort_unstable();
            s.dedup();
            assert_eq!(s.len(), 5, "a title repeated inside the window: {w:?}");
        }
        // A pool smaller than the window still works.
        let small = Pools {
            dayparts: vec![movies(2, 60)],
            bumpers: vec![],
        };
        let g = run(
            "UTC",
            &r,
            FillerPolicy::NextProgramEarly,
            &small,
            0,
            12 * H,
            Cursor::default(),
        );
        assert_eq!(g.blocks.len(), 12);
    }

    #[test]
    fn programmes_start_on_the_grid_and_bumpers_fill_the_gap() {
        let r = rules(
            30,
            0,
            vec![part(
                "All",
                "00:00",
                "24:00",
                PickMode::Sequential,
                Fit::Overrun,
            )],
        );
        let eps: Vec<_> = (1..=3).map(|n| episode(7, "Show", 1, n, 22)).collect();
        let mut bump = item(90, "Bump", 0);
        bump.duration_ms = 5 * M;
        bump.tags = vec!["filler".into()];
        let pools = Pools {
            dayparts: vec![eps],
            bumpers: vec![bump],
        };
        let g = run(
            "UTC",
            &r,
            FillerPolicy::Bumpers,
            &pools,
            0,
            2 * H,
            Cursor::default(),
        );
        assert_contiguous(&g);
        let progs: Vec<_> = g
            .blocks
            .iter()
            .filter(|b| b.kind == BlockKind::Program)
            .collect();
        for p in &progs {
            assert_eq!(p.start_ms % (30 * M), 0, "off the grid: {p:?}");
        }
        // 22 minutes of show, then 8 minutes of bumpers: one full (5 min) and one clipped to 3.
        let fillers: Vec<_> = g
            .blocks
            .iter()
            .take(3)
            .map(|b| b.end_ms - b.start_ms)
            .collect();
        assert_eq!(fillers, [22 * M, 5 * M, 3 * M]);
        assert_eq!(g.blocks[2].end_ms, 30 * M);
    }

    #[test]
    fn next_program_early_closes_the_gap_and_slate_and_off_air_fill_it() {
        let r = rules(
            30,
            0,
            vec![part(
                "All",
                "00:00",
                "24:00",
                PickMode::Sequential,
                Fit::Overrun,
            )],
        );
        let eps: Vec<_> = (1..=3).map(|n| episode(7, "Show", 1, n, 22)).collect();
        let pools = Pools {
            dayparts: vec![eps],
            bumpers: vec![],
        };
        let early = run(
            "UTC",
            &r,
            FillerPolicy::NextProgramEarly,
            &pools,
            0,
            H,
            Cursor::default(),
        );
        assert!(early.blocks.iter().all(|b| b.kind == BlockKind::Program));
        assert_eq!(early.blocks[1].start_ms, 22 * M);
        let slate = run(
            "UTC",
            &r,
            FillerPolicy::Slate,
            &pools,
            0,
            H,
            Cursor::default(),
        );
        assert_eq!(slate.blocks[1].kind, BlockKind::Slate);
        assert_eq!(slate.blocks[1].end_ms, 30 * M);
        let off = run(
            "UTC",
            &r,
            FillerPolicy::OffAir,
            &pools,
            0,
            H,
            Cursor::default(),
        );
        assert_eq!(off.blocks[1].kind, BlockKind::OffAir);
        // Bumpers wanted but none exist: a slate.
        let none = run(
            "UTC",
            &r,
            FillerPolicy::Bumpers,
            &pools,
            0,
            H,
            Cursor::default(),
        );
        assert_eq!(none.blocks[1].kind, BlockKind::Slate);
    }

    #[test]
    fn dayparts_wrap_midnight_follow_the_days_and_leave_gaps_off_air() {
        let mut late = part("Late", "22:00", "02:00", PickMode::Shuffle, Fit::Overrun);
        late.days = vec![5]; // Saturday evenings, running into Sunday morning
        let r = rules(60, 0, vec![late]);
        let pools = Pools {
            dayparts: vec![movies(3, 110)],
            bumpers: vec![],
        };
        // 2025-06-07 is a Saturday.
        let from = utc(2025, 6, 6, 12, 0);
        let g = run(
            "UTC",
            &r,
            FillerPolicy::Slate,
            &pools,
            from,
            utc(2025, 6, 8, 12, 0),
            Cursor::default(),
        );
        assert_contiguous(&g);
        let progs: Vec<_> = g
            .blocks
            .iter()
            .filter(|b| b.kind == BlockKind::Program)
            .collect();
        assert_eq!(
            progs.len(),
            2,
            "two 110 minute films fit 22:00 to 02:00 with an overrun"
        );
        assert_eq!(progs[0].start_ms, utc(2025, 6, 7, 22, 0));
        assert_eq!(progs[1].start_ms, utc(2025, 6, 8, 0, 0));
        // Friday and Sunday daytime are slate.
        assert_eq!(g.blocks[0].kind, BlockKind::Slate);
        assert_eq!(g.blocks[0].end_ms, utc(2025, 6, 7, 22, 0));
    }

    #[test]
    fn equal_start_and_end_is_the_whole_day_without_a_gap() {
        // G-11: an end at or before the start wraps past midnight; equal means 24 hours ("All day").
        for (start, end) in [("00:00", "00:00"), ("00:00", "24:00"), ("08:00", "08:00")] {
            let dp = part("All day", start, end, PickMode::Shuffle, Fit::Overrun);
            let r = rules(60, 0, vec![dp]);
            let pools = Pools {
                dayparts: vec![movies(4, 60)],
                bumpers: vec![],
            };
            let from = utc(2025, 6, 2, 9, 0);
            let g = run(
                "UTC",
                &r,
                FillerPolicy::Slate,
                &pools,
                from,
                utc(2025, 6, 4, 9, 0),
                Cursor::default(),
            );
            assert_contiguous(&g);
            assert!(
                g.blocks.iter().all(|b| b.kind == BlockKind::Program),
                "{start}-{end}: no slate or off-air block in 48 hours"
            );
            assert!(g.warnings.is_empty(), "{start}-{end}: {:?}", g.warnings);
            assert!(g.blocks.last().unwrap().end_ms >= utc(2025, 6, 4, 9, 0));
        }
    }

    #[test]
    fn fit_only_airs_what_ends_in_time() {
        let mut dp = part("Short", "20:00", "21:00", PickMode::Shuffle, Fit::Fit);
        dp.days = vec![];
        let r = rules(0, 0, vec![dp]);
        let pools = Pools {
            dayparts: vec![vec![item(1, "Long", 90), item(2, "Short", 25)]],
            bumpers: vec![],
        };
        let g = run(
            "UTC",
            &r,
            FillerPolicy::Slate,
            &pools,
            utc(2025, 1, 1, 20, 0),
            utc(2025, 1, 1, 21, 0),
            Cursor::default(),
        );
        let progs: Vec<_> = g
            .blocks
            .iter()
            .filter(|b| b.kind == BlockKind::Program)
            .collect();
        assert!(
            progs
                .iter()
                .all(|b| b.item.as_ref().unwrap().title == "Short")
        );
        assert_eq!(progs.len(), 2);
        // 50 minutes used, the last 10 are a slate up to 21:00.
        let last = g.blocks.last().unwrap();
        assert_eq!(
            (last.kind, last.end_ms),
            (BlockKind::Slate, utc(2025, 1, 1, 21, 0))
        );
    }

    #[test]
    fn an_empty_pool_warns_and_is_filled() {
        let r = rules(
            30,
            0,
            vec![part(
                "Empty",
                "00:00",
                "24:00",
                PickMode::Shuffle,
                Fit::Overrun,
            )],
        );
        let pools = Pools {
            dayparts: vec![vec![]],
            bumpers: vec![],
        };
        let g = run(
            "UTC",
            &r,
            FillerPolicy::OffAir,
            &pools,
            0,
            2 * MS_DAY,
            Cursor::default(),
        );
        assert_eq!(
            g.warnings.len(),
            1,
            "one warning per daypart: {:?}",
            g.warnings
        );
        assert!(g.blocks.iter().all(|b| b.kind == BlockKind::OffAir));
        assert_contiguous(&g);
        // No dayparts at all: the same.
        let none = run(
            "UTC",
            &rules(30, 0, vec![]),
            FillerPolicy::Slate,
            &Pools::default(),
            0,
            MS_DAY,
            Cursor::default(),
        );
        assert_eq!(none.blocks.len(), 1);
        assert_eq!(none.blocks[0].kind, BlockKind::Slate);
    }

    #[test]
    fn daylight_saving_days_are_23_and_25_hours_and_stay_on_the_local_grid() {
        let ny = Zone::parse("America/New_York").unwrap();
        let r = rules(
            30,
            0,
            vec![part(
                "All",
                "00:00",
                "24:00",
                PickMode::Sequential,
                Fit::Overrun,
            )],
        );
        let eps: Vec<_> = (1..=50).map(|n| episode(7, "Show", 1, n, 22)).collect();
        let pools = Pools {
            dayparts: vec![eps],
            bumpers: vec![],
        };
        // Local midnight to local midnight, 2025-03-09 (spring forward) and 2025-11-02 (fall back).
        for (day, hours) in [((2025, 3, 9), 23), ((2025, 11, 2), 25), ((2025, 6, 1), 24)] {
            let start = ny.from_local(days_from_civil(day.0, day.1, day.2) * MS_DAY);
            let end = ny.from_local(days_from_civil(day.0, day.1, day.2 + 1) * MS_DAY);
            assert_eq!((end - start) / H, hours, "{day:?}");
            let g = run(
                "America/New_York",
                &r,
                FillerPolicy::Slate,
                &pools,
                start,
                end,
                Cursor::default(),
            );
            assert_contiguous(&g);
            assert_eq!(g.blocks[0].start_ms, start);
            assert!(g.blocks.last().unwrap().end_ms >= end);
            for b in g.blocks.iter().filter(|b| b.kind == BlockKind::Program) {
                let local = ny.to_local(b.start_ms);
                assert_eq!(
                    local.rem_euclid(30 * M),
                    0,
                    "{day:?}: {b:?} is off the local grid"
                );
            }
            // The day boundary is covered without overlap or hole.
            let total: i64 = g.blocks.iter().map(|b| b.end_ms - b.start_ms).sum();
            assert_eq!(total, g.blocks.last().unwrap().end_ms - start);
        }
    }

    #[test]
    fn a_daypart_in_channel_time_moves_with_daylight_saving() {
        // 20:00 New York prime time starts at 01:00 UTC in winter... and at 00:00 UTC in summer.
        let r = rules(
            0,
            0,
            vec![part(
                "Prime",
                "20:00",
                "21:00",
                PickMode::Sequential,
                Fit::Overrun,
            )],
        );
        let pools = Pools {
            dayparts: vec![vec![item(1, "Show", 30)]],
            bumpers: vec![],
        };
        let first_program = |from: i64| {
            let g = run(
                "America/New_York",
                &r,
                FillerPolicy::Slate,
                &pools,
                from,
                from + MS_DAY,
                Cursor::default(),
            );
            g.blocks
                .iter()
                .find(|b| b.kind == BlockKind::Program)
                .unwrap()
                .start_ms
        };
        assert_eq!(
            first_program(utc(2025, 1, 15, 0, 0)),
            utc(2025, 1, 15, 1, 0)
        );
        assert_eq!(
            first_program(utc(2025, 7, 15, 0, 0)),
            utc(2025, 7, 15, 0, 0)
        );
    }

    #[test]
    fn a_start_inside_a_programme_slot_begins_with_the_alignment_filler() {
        let r = rules(
            30,
            0,
            vec![part(
                "All",
                "00:00",
                "24:00",
                PickMode::Sequential,
                Fit::Overrun,
            )],
        );
        let pools = Pools {
            dayparts: vec![vec![item(1, "Show", 30)]],
            bumpers: vec![],
        };
        let g = run(
            "UTC",
            &r,
            FillerPolicy::Slate,
            &pools,
            17 * M,
            2 * H,
            Cursor::default(),
        );
        assert_eq!(g.blocks[0].kind, BlockKind::Slate);
        assert_eq!((g.blocks[0].start_ms, g.blocks[0].end_ms), (17 * M, 30 * M));
        assert_eq!(g.blocks[1].start_ms, 30 * M);
    }
}
