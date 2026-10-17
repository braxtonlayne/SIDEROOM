//! Tests of the imported-guide resolver (`engine::resolve_slots`, TV-ENG-003).

use crate::catalog::{CatalogItem, episode, item};
use crate::engine::{EngineChannel, Generated, SlotPlan, resolve_slots};
use crate::model::{BlockKind, ChannelRules, Cursor, FillerPolicy};
use crate::tz::{MS_DAY, Zone, days_from_civil};

const H: i64 = 3_600_000;
const M: i64 = 60_000;

/// 2025-03-03 was a Monday.
fn t(day: i64, h: i64, min: i64) -> i64 {
    days_from_civil(2025, 3, 3 + day) * MS_DAY + h * H + min * M
}

fn plan(key: i64, weekday: u8, start: &str, minutes: i64, items: Vec<CatalogItem>) -> SlotPlan {
    let (h, m) = start.split_once(':').unwrap();
    SlotPlan {
        key,
        weekday,
        start_minute: h.parse::<i64>().unwrap() * 60 + m.parse::<i64>().unwrap(),
        duration_min: minutes,
        items,
    }
}

fn run(
    zone: &str,
    filler: FillerPolicy,
    bumpers: &[CatalogItem],
    plans: &[SlotPlan],
    from: i64,
    to: i64,
    cursor: Cursor,
) -> Generated {
    let rules = ChannelRules::default();
    let ch = EngineChannel {
        zone: Zone::parse(zone).unwrap(),
        rules: &rules,
        filler,
        seed: 1,
    };
    resolve_slots(&ch, bumpers, plans, from, to, cursor)
}

fn shape(g: &Generated) -> Vec<(i64, i64, BlockKind, Option<i64>)> {
    g.blocks
        .iter()
        .map(|b| {
            (
                b.start_ms,
                b.end_ms,
                b.kind,
                b.item.as_ref().map(|i| i.key()),
            )
        })
        .collect()
}

#[test]
fn slots_become_blocks_and_gaps_are_filled() {
    let plans = [
        plan(1, 0, "20:00", 30, vec![item(1, "A", 30)]),
        plan(2, 0, "20:30", 30, vec![item(2, "B", 30)]),
    ];
    let g = run(
        "UTC",
        FillerPolicy::Slate,
        &[],
        &plans,
        t(0, 19, 0),
        t(0, 21, 0),
        Cursor::default(),
    );
    // No cursor and nothing on air at 19:00: the hour before the first slot is filler.
    assert_eq!(
        shape(&g),
        [
            (t(0, 19, 0), t(0, 20, 0), BlockKind::Slate, None),
            (t(0, 20, 0), t(0, 20, 30), BlockKind::Program, Some(1)),
            (t(0, 20, 30), t(0, 21, 0), BlockKind::Program, Some(2)),
        ]
    );
}

#[test]
fn a_gap_before_a_later_slot_is_filler_when_continuing() {
    let plans = [plan(1, 0, "20:00", 30, vec![item(1, "A", 30)])];
    let cursor = Cursor {
        at_ms: t(0, 19, 0),
        ..Cursor::default()
    };
    let g = run(
        "UTC",
        FillerPolicy::Slate,
        &[],
        &plans,
        t(0, 19, 0),
        t(0, 21, 0),
        cursor,
    );
    let s = shape(&g);
    assert_eq!(s[0], (t(0, 19, 0), t(0, 20, 0), BlockKind::Slate, None));
    assert_eq!(s[1].2, BlockKind::Program);
    // After the last slot the rest of the window is filler.
    assert_eq!(s[2], (t(0, 20, 30), t(0, 21, 0), BlockKind::Slate, None));
    for w in g.blocks.windows(2) {
        assert_eq!(w[0].end_ms, w[1].start_ms);
    }
}

#[test]
fn a_missing_show_is_skipped_and_the_gap_is_filled() {
    let plans = [
        plan(1, 0, "20:00", 30, vec![item(1, "A", 30)]),
        plan(2, 0, "20:30", 30, vec![]),
        plan(3, 0, "21:00", 30, vec![item(3, "C", 30)]),
    ];
    let g = run(
        "UTC",
        FillerPolicy::OffAir,
        &[],
        &plans,
        t(0, 20, 0),
        t(0, 21, 30),
        Cursor::default(),
    );
    assert_eq!(
        shape(&g),
        [
            (t(0, 20, 0), t(0, 20, 30), BlockKind::Program, Some(1)),
            (t(0, 20, 30), t(0, 21, 0), BlockKind::OffAir, None),
            (t(0, 21, 0), t(0, 21, 30), BlockKind::Program, Some(3)),
        ]
    );
}

#[test]
fn bumpers_fill_a_short_programme() {
    let mut bump = item(90, "Bump", 3);
    bump.tags = vec!["filler".into()];
    let plans = [
        plan(1, 0, "20:00", 30, vec![item(1, "A", 22)]),
        plan(2, 0, "20:30", 30, vec![item(2, "B", 30)]),
    ];
    let g = run(
        "UTC",
        FillerPolicy::Bumpers,
        &[bump],
        &plans,
        t(0, 20, 0),
        t(0, 21, 0),
        Cursor::default(),
    );
    let kinds: Vec<_> = g.blocks.iter().map(|b| b.kind).collect();
    assert_eq!(kinds[0], BlockKind::Program);
    assert!(
        kinds[1..kinds.len() - 1]
            .iter()
            .all(|k| *k == BlockKind::Filler)
    );
    // Bumpers are clipped at the next slot start.
    assert_eq!(g.blocks[g.blocks.len() - 2].end_ms, t(0, 20, 30));
    assert_eq!(g.blocks.last().unwrap().start_ms, t(0, 20, 30));
}

#[test]
fn an_overrun_shifts_the_next_programme_or_skips_its_slot() {
    // A runs 40 minutes in a 30 minute slot: B starts at 20:40 instead of 20:30.
    let plans = [
        plan(1, 0, "20:00", 30, vec![item(1, "A", 40)]),
        plan(2, 0, "20:30", 30, vec![item(2, "B", 20)]),
        plan(3, 0, "21:00", 30, vec![item(3, "C", 30)]),
    ];
    let g = run(
        "UTC",
        FillerPolicy::Slate,
        &[],
        &plans,
        t(0, 20, 0),
        t(0, 21, 30),
        Cursor::default(),
    );
    let s = shape(&g);
    assert_eq!(
        s[1],
        (t(0, 20, 40), t(0, 21, 0), BlockKind::Program, Some(2))
    );
    assert_eq!(
        s[2],
        (t(0, 21, 0), t(0, 21, 30), BlockKind::Program, Some(3))
    );
    // A film of 70 minutes swallows the 20:30 slot whole: B is skipped, C starts on time.
    let plans = [
        plan(1, 0, "20:00", 30, vec![item(1, "Film", 70)]),
        plan(2, 0, "20:30", 30, vec![item(2, "B", 20)]),
        plan(3, 0, "21:30", 30, vec![item(3, "C", 30)]),
    ];
    let g = run(
        "UTC",
        FillerPolicy::Slate,
        &[],
        &plans,
        t(0, 20, 0),
        t(0, 22, 0),
        Cursor::default(),
    );
    let titles: Vec<_> = g
        .blocks
        .iter()
        .filter_map(|b| b.item.as_ref().map(|i| i.title.clone()))
        .collect();
    assert_eq!(titles, ["Film", "C"]);
    assert!(g.warnings.iter().any(|w| w.contains("overran")));
}

#[test]
fn tuning_in_mid_slot_lands_inside_the_programme() {
    let plans = [plan(1, 0, "20:00", 60, vec![item(1, "A", 60)])];
    let g = run(
        "UTC",
        FillerPolicy::Slate,
        &[],
        &plans,
        t(0, 20, 17),
        t(0, 22, 0),
        Cursor::default(),
    );
    assert_eq!(g.blocks[0].start_ms, t(0, 20, 0));
    assert!(g.blocks[0].start_ms <= t(0, 20, 17) && t(0, 20, 17) < g.blocks[0].end_ms);
}

#[test]
fn a_series_slot_rotates_its_episodes_week_by_week() {
    let eps = vec![
        episode(7, "Show", 1, 1, 30),
        episode(7, "Show", 1, 2, 30),
        episode(7, "Show", 1, 3, 30),
    ];
    let plans = [plan(5, 0, "20:00", 30, eps.clone())];
    let g = run(
        "UTC",
        FillerPolicy::Slate,
        &[],
        &plans,
        t(0, 20, 0),
        t(22, 0, 0),
        Cursor::default(),
    );
    let aired: Vec<_> = g
        .blocks
        .iter()
        .filter_map(|b| b.item.as_ref().and_then(|i| i.episode_number))
        .collect();
    assert_eq!(aired, [1, 2, 3, 1]);
}

#[test]
fn extending_in_steps_equals_one_pass() {
    let plans = [
        plan(1, 0, "20:00", 30, vec![item(1, "A", 30)]),
        plan(
            2,
            2,
            "21:00",
            60,
            vec![episode(7, "S", 1, 1, 60), episode(7, "S", 1, 2, 60)],
        ),
        plan(3, 6, "23:30", 90, vec![item(3, "Late", 80)]),
    ];
    let whole = run(
        "UTC",
        FillerPolicy::Slate,
        &[],
        &plans,
        t(0, 0, 0),
        t(14, 0, 0),
        Cursor::default(),
    );
    let first = run(
        "UTC",
        FillerPolicy::Slate,
        &[],
        &plans,
        t(0, 0, 0),
        t(5, 0, 0),
        Cursor::default(),
    );
    let second = run(
        "UTC",
        FillerPolicy::Slate,
        &[],
        &plans,
        t(5, 0, 0),
        t(14, 0, 0),
        first.cursor.clone(),
    );
    let programs = |g: &Generated| -> Vec<(i64, i64)> {
        g.blocks
            .iter()
            .filter(|b| b.kind == BlockKind::Program)
            .map(|b| (b.start_ms, b.end_ms))
            .collect()
    };
    let mut joined = programs(&first);
    joined.extend(programs(&second));
    assert_eq!(joined, programs(&whole));
}

#[test]
fn slot_times_are_channel_time_across_a_clock_change() {
    // New York moved to summer time on 2025-03-09: 20:00 local is 01:00 UTC before and 00:00 UTC after.
    let plans = [plan(1, 6, "20:00", 30, vec![item(1, "A", 30)])];
    let g = run(
        "America/New_York",
        FillerPolicy::Slate,
        &[],
        &plans,
        t(0, 0, 0),
        t(15, 0, 0),
        Cursor::default(),
    );
    let starts: Vec<i64> = g
        .blocks
        .iter()
        .filter(|b| b.kind == BlockKind::Program)
        .map(|b| b.start_ms)
        .collect();
    assert_eq!(starts, [t(0, 1, 0), t(7, 0, 0), t(14, 0, 0)]);
}
