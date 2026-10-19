//! Service tests over the in-memory repository, catalog and playback source, with a manual clock.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use sr_core::{ManualClock, PlatformPermissions as P};
use sr_media::playback::MemoryPlaybackSource;
use sr_media::playlist::UrlSigner;

use crate::catalog::{MemoryCatalog, episode, item};
use crate::error::TvError;
use crate::live::{Track, tests::asset};
use crate::model::*;
use crate::repo::{MemoryTvRepo, TvRepository};
use crate::service::{Actor, LiveDeps, PlaceInput, SetNowInput, TuneState, TvService, Viewer};
use crate::tz::{MS_DAY, days_from_civil};

const MIN: i64 = 60_000;

fn utc(y: i64, m: i64, d: i64, h: i64, min: i64) -> i64 {
    days_from_civil(y, m, d) * MS_DAY + h * 3_600_000 + min * MIN
}

struct Sig;
impl UrlSigner for Sig {
    fn signed_url(&self, path: &str, expires_at: u64) -> String {
        format!("{path}?ex={expires_at}&sig=S")
    }
}

struct Env {
    svc: TvService,
    clock: Arc<ManualClock>,
    repo: Arc<MemoryTvRepo>,
    catalog: MemoryCatalog,
    staff: Actor,
}

const MEMBER: Viewer = Viewer {
    user_id: 5,
    adult: false,
};
const ADULT: Viewer = Viewer {
    user_id: 6,
    adult: true,
};

/// Monday 2025-06-02 20:00 UTC; a series of 40 half-hour episodes and an asset for each.
fn env() -> Env {
    let clock = Arc::new(ManualClock::new(utc(2025, 6, 2, 20, 0)));
    let repo = Arc::new(MemoryTvRepo::new());
    let catalog = MemoryCatalog::new();
    let source = MemoryPlaybackSource::new();
    for n in 1..=40 {
        let e = episode(7, "Cowboy Bebop", 1, n, 30);
        source.insert(asset(e.asset_id, 300));
        catalog.add(e);
    }
    let next = Arc::new(AtomicI64::new(1000));
    let svc = TvService::new(
        repo.clone(),
        Arc::new(catalog.clone()),
        clock.clone(),
        Arc::new(move || next.fetch_add(1, Ordering::SeqCst)),
    )
    .with_live(LiveDeps {
        source: Arc::new(source),
        signer: Arc::new(Sig),
    });
    Env {
        svc,
        clock,
        repo,
        catalog,
        staff: Actor {
            user_id: 1,
            permissions: P::MANAGE_TV,
        },
    }
}

fn daypart(name: &str, mode: PickMode, pool: PoolFilter) -> Daypart {
    Daypart {
        name: name.into(),
        start: "00:00".into(),
        end: "24:00".into(),
        days: vec![],
        mode,
        fit: Fit::Overrun,
        pool,
    }
}

fn input(number: i32, rules: ChannelRules) -> ChannelInput {
    ChannelInput {
        number,
        name: format!("Channel {number}"),
        call_sign: Some("[adult swim]".into()),
        era_label: None,
        theme: None,
        logo_key: Some("tv/logo.png".into()),
        enabled: true,
        kind: ChannelKind::RuleBased,
        timezone: "UTC".into(),
        age_restricted: false,
        filler_policy: FillerPolicy::Slate,
        rules,
    }
}

fn marathon() -> ChannelRules {
    ChannelRules {
        grid_minutes: 30,
        no_repeat_window: 0,
        dayparts: vec![daypart(
            "All day",
            PickMode::Sequential,
            PoolFilter {
                title_ids: vec![7],
                ..PoolFilter::default()
            },
        )],
    }
}

async fn channel(e: &Env, number: i32, rules: ChannelRules) -> i64 {
    e.svc
        .create_channel(e.staff, input(number, rules))
        .await
        .unwrap()
        .id
}

#[tokio::test]
async fn staff_need_manage_tv_and_numbers_are_unique() {
    let e = env();
    let nobody = Actor {
        user_id: 2,
        permissions: P::PUBLISH_MEDIA,
    };
    assert_eq!(
        e.svc.create_channel(nobody, input(1, marathon())).await,
        Err(TvError::MissingPermission("MANAGE_TV"))
    );
    assert!(e.svc.admin_channels(nobody).await.is_err());
    let id = channel(&e, 101, marathon()).await;
    let dup = e.svc.create_channel(e.staff, input(101, marathon())).await;
    assert!(matches!(dup, Err(TvError::Conflict(_))), "{dup:?}");
    let c = e.svc.admin_channel(e.staff, id).await.unwrap();
    assert_eq!(c.rules.as_ref().unwrap().grid_minutes, 30);
    assert!(c.seed.is_some());
    assert!(
        c.logo_url.unwrap().starts_with("/cdn/tv/logo.png?ex="),
        "a signed logo link"
    );
    let mut upd = input(102, marathon());
    upd.name = "Renamed".into();
    assert_eq!(
        e.svc.update_channel(e.staff, id, upd).await.unwrap().number,
        102
    );
    assert_eq!(
        e.svc.delete_channel(e.staff, id).await.unwrap(),
        (102, "Renamed".into())
    );
    assert_eq!(e.svc.channel(id).await, Err(TvError::NotFound("channel")));
}

#[tokio::test]
async fn now_and_next_follow_the_wall_clock_and_joining_17_minutes_in_is_17_minutes_in() {
    let e = env();
    let id = channel(&e, 101, marathon()).await;
    let first = e.svc.now(id).await.unwrap();
    let cur = first.current.unwrap();
    assert_eq!(
        (cur.start_ms, cur.end_ms),
        (utc(2025, 6, 2, 20, 0), utc(2025, 6, 2, 20, 30))
    );
    assert_eq!(cur.episode_number, Some(1));
    assert_eq!(cur.source, BlockSource::Generated);
    assert_eq!(first.next.unwrap().episode_number, Some(2));
    assert_eq!(first.offset_ms, Some(0));

    e.clock.advance_ms(17 * MIN);
    let at17 = e.svc.now(id).await.unwrap();
    assert_eq!(at17.offset_ms, Some(17 * MIN));
    assert_eq!(at17.current.unwrap().episode_number, Some(1));
    e.clock.advance_ms(14 * MIN);
    let after = e.svc.now(id).await.unwrap();
    assert_eq!(after.current.unwrap().episode_number, Some(2));
    assert_eq!(after.offset_ms, Some(MIN));

    // The list shows the same, and a disabled channel disappears for members.
    let list = e.svc.channels().await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].now.as_ref().unwrap().episode_number, Some(2));
    let mut off = input(101, marathon());
    off.enabled = false;
    e.svc.update_channel(e.staff, id, off).await.unwrap();
    assert!(e.svc.channels().await.unwrap().is_empty());
    assert_eq!(e.svc.now(id).await, Err(TvError::NotFound("channel")));
}

#[tokio::test]
async fn the_schedule_is_extended_a_day_ahead_and_stays_contiguous_across_calls() {
    let e = env();
    let id = channel(&e, 101, marathon()).await;
    e.svc.now(id).await.unwrap();
    let blocks = e.repo.blocks_overlapping(&[id], 0, i64::MAX).await.unwrap();
    assert!(blocks.last().unwrap().end_ms >= utc(2025, 6, 3, 20, 0));
    // Time passes; the next read extends from where the schedule ended, with the next episodes.
    e.clock.advance_ms(20 * 3_600_000);
    e.svc.now(id).await.unwrap();
    let all = e.repo.blocks_overlapping(&[id], 0, i64::MAX).await.unwrap();
    for w in all.windows(2) {
        assert_eq!(w[0].end_ms, w[1].start_ms);
    }
    let eps: Vec<_> = all.iter().map(|b| b.episode_number.unwrap()).collect();
    for w in eps.windows(2) {
        assert_eq!(w[1], w[0] % 40 + 1, "episodes follow each other: {eps:?}");
    }
    assert!(all.last().unwrap().end_ms >= utc(2025, 6, 4, 16, 0));
}

#[tokio::test]
async fn the_guide_returns_blocks_per_channel_and_validates_the_window() {
    let e = env();
    let a = channel(&e, 101, marathon()).await;
    let b = channel(&e, 102, marathon()).await;
    let g = e.svc.guide(None, None, &[]).await.unwrap();
    assert_eq!(g.from_ms, utc(2025, 6, 2, 20, 0));
    assert_eq!(g.to_ms, g.from_ms + 4 * 3_600_000);
    assert_eq!(g.channels.len(), 2);
    assert_eq!(g.channels[0].blocks.len(), 8);
    assert!(
        g.channels[0].channel.now.is_none(),
        "the guide carries no now/next"
    );
    let only = e.svc.guide(None, None, &[b]).await.unwrap();
    assert_eq!(only.channels.len(), 1);
    assert_eq!(only.channels[0].channel.id, b);
    assert!(e.svc.guide(Some(10), Some(5), &[]).await.is_err());
    assert!(
        e.svc
            .guide(Some(0), Some(49 * 3_600_000), &[a])
            .await
            .is_err()
    );
}

#[tokio::test]
async fn preview_stores_nothing_and_publish_replaces_unstarted_generated_blocks_only() {
    let e = env();
    let id = channel(&e, 101, marathon()).await;
    e.svc.now(id).await.unwrap();
    let before = e.repo.blocks_overlapping(&[id], 0, i64::MAX).await.unwrap();

    // A different rule set: shuffle movies. Preview shows it, storage is untouched.
    for n in 1..=3 {
        let mut m = item(50 + n, &format!("Film {n}"), 90);
        m.asset_id = 5000 + n;
        e.catalog.add(m);
    }
    let mut changed = input(
        101,
        ChannelRules {
            grid_minutes: 30,
            no_repeat_window: 2,
            dayparts: vec![daypart(
                "Films",
                PickMode::Shuffle,
                PoolFilter {
                    kinds: vec![MediaKind::Movie],
                    ..PoolFilter::default()
                },
            )],
        },
    );
    e.svc
        .update_channel(e.staff, id, changed.clone())
        .await
        .unwrap();
    let p = e
        .svc
        .preview(e.staff, id, None, Some(utc(2025, 6, 3, 8, 0)))
        .await
        .unwrap();
    assert!(p.blocks.iter().any(|b| b.title.starts_with("Film")));
    assert!(p.blocks.iter().all(|b| b.id == 0));
    assert_eq!(
        e.repo.blocks_overlapping(&[id], 0, i64::MAX).await.unwrap(),
        before
    );

    // An override placed tomorrow survives a publish, and the on-air block is kept.
    e.clock.advance_ms(10 * MIN);
    let placed = e
        .svc
        .place_block(
            e.staff,
            id,
            PlaceInput {
                title_id: 7,
                episode_id: Some(7000 + 105),
                start_ms: utc(2025, 6, 3, 1, 0),
                end_ms: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(placed.source, BlockSource::Override);
    changed.name = "Films".into();
    let done = e.svc.publish(e.staff, id, Some(48)).await.unwrap();
    assert_eq!(
        done.from_ms,
        utc(2025, 6, 2, 20, 30),
        "after the programme on air"
    );
    assert!(done.removed > 0 && done.written > 0, "{done:?}");
    let after = e.repo.blocks_overlapping(&[id], 0, i64::MAX).await.unwrap();
    assert_eq!(after[0].episode_number, Some(1), "the block on air stays");
    assert!(after[1..].iter().any(|b| b.title.starts_with("Film")));
    assert!(
        after
            .iter()
            .any(|b| b.id == placed.id && b.start_ms == placed.start_ms)
    );
    for w in after.windows(2) {
        assert!(w[0].end_ms <= w[1].start_ms, "overlap: {w:?}");
    }
    // Manual channels have nothing to generate.
    let mut manual = input(300, ChannelRules::default());
    manual.kind = ChannelKind::Manual;
    let m = e.svc.create_channel(e.staff, manual).await.unwrap().id;
    assert!(matches!(
        e.svc.publish(e.staff, m, None).await,
        Err(TvError::Invalid(_))
    ));
}

#[tokio::test]
async fn staff_place_programmes_cut_the_air_and_remove_blocks() {
    let e = env();
    let mut manual = input(300, ChannelRules::default());
    manual.kind = ChannelKind::Manual;
    let id = e.svc.create_channel(e.staff, manual).await.unwrap().id;
    // Nothing placed: nothing on.
    assert!(e.svc.now(id).await.unwrap().current.is_none());
    let t = utc(2025, 6, 2, 20, 0);
    let a = e
        .svc
        .place_block(
            e.staff,
            id,
            PlaceInput {
                title_id: 7,
                episode_id: Some(7101),
                start_ms: t,
                end_ms: None,
            },
        )
        .await
        .unwrap();
    assert_eq!((a.source, a.end_ms), (BlockSource::Manual, t + 30 * MIN));
    // A second programme laid over the middle of the first splits it.
    e.svc
        .place_block(
            e.staff,
            id,
            PlaceInput {
                title_id: 7,
                episode_id: Some(7102),
                start_ms: t + 10 * MIN,
                end_ms: Some(t + 20 * MIN),
            },
        )
        .await
        .unwrap();
    let spans: Vec<_> = e
        .repo
        .blocks_overlapping(&[id], 0, i64::MAX)
        .await
        .unwrap()
        .iter()
        .map(|b| {
            (
                b.start_ms - t,
                b.end_ms - t,
                b.offset_ms,
                b.episode_number.unwrap(),
            )
        })
        .collect();
    assert_eq!(
        spans,
        [
            (0, 10 * MIN, 0, 1),
            (10 * MIN, 20 * MIN, 0, 2),
            (20 * MIN, 30 * MIN, 20 * MIN, 1)
        ]
    );
    // Too long, unplayable, or inverted placements are refused.
    let bad = |start: i64, end: Option<i64>, title: i64| PlaceInput {
        title_id: title,
        episode_id: None,
        start_ms: start,
        end_ms: end,
    };
    assert!(
        e.svc
            .place_block(e.staff, id, bad(t, Some(t - 1), 7))
            .await
            .is_err()
    );
    assert!(
        e.svc
            .place_block(e.staff, id, bad(t, None, 999))
            .await
            .is_err()
    );

    // "On now": a film from 5 minutes in, cutting whatever is on.
    e.clock.advance_ms(12 * MIN);
    let mut film = item(60, "Film", 90);
    film.asset_id = 6000;
    e.catalog.add(film);
    let now = e
        .svc
        .set_now(
            e.staff,
            id,
            SetNowInput {
                title_id: 60,
                episode_id: None,
                offset_ms: Some(5 * MIN),
            },
        )
        .await
        .unwrap();
    assert_eq!(now.start_ms, t + 12 * MIN);
    assert_eq!(now.end_ms, t + 12 * MIN + 85 * MIN);
    let cur = e.svc.now(id).await.unwrap();
    assert_eq!(cur.current.as_ref().unwrap().title, "Film");
    assert_eq!(cur.offset_ms, Some(5 * MIN));
    // The simulate-now read looks at any moment.
    let past = e
        .svc
        .admin_now(e.staff, id, Some(t + 5 * MIN))
        .await
        .unwrap();
    assert_eq!(past.current.unwrap().episode_number, Some(1));

    // Removing a block leaves a hole.
    let removed = e.svc.remove_block(e.staff, now.id).await.unwrap();
    assert_eq!(removed.title, "Film");
    assert!(e.svc.now(id).await.unwrap().current.is_none());
    assert_eq!(
        e.svc.remove_block(e.staff, now.id).await,
        Err(TvError::NotFound("block"))
    );
}

#[tokio::test]
async fn the_report_lists_what_aired() {
    let e = env();
    let id = channel(&e, 101, marathon()).await;
    e.svc.now(id).await.unwrap();
    e.clock.advance_ms(95 * MIN);
    let r = e
        .svc
        .report(e.staff, Some(utc(2025, 6, 2, 20, 0)), None, None)
        .await
        .unwrap();
    assert_eq!(r.to_ms, utc(2025, 6, 2, 21, 35), "the future has not aired");
    assert_eq!(r.channels.len(), 1);
    assert_eq!(r.channels[0].aired_ms, 95 * MIN);
    assert_eq!(r.channels[0].programmes, 4);
    assert_eq!(r.channels[0].overrides, 0);
    assert_eq!(r.items.len(), 4);
    assert_eq!(
        r.items[3].aired_ms,
        5 * MIN,
        "the last one is clipped to now"
    );
    assert!(e.svc.report(e.staff, None, None, Some(1)).await.is_err());
}

#[tokio::test]
async fn restricted_programmes_are_listed_but_opening_them_needs_age_confirmation() {
    let e = env();
    let mut film = item(80, "Adult Film", 120);
    film.age_restricted = true;
    film.asset_id = 8000;
    e.catalog.add(film);
    let mut c = input(
        666,
        ChannelRules {
            grid_minutes: 0,
            no_repeat_window: 0,
            dayparts: vec![daypart(
                "Late",
                PickMode::Sequential,
                PoolFilter {
                    title_ids: vec![80],
                    restricted: Some(Restricted::Only),
                    ..PoolFilter::default()
                },
            )],
        },
    );
    c.age_restricted = true;
    let id = e.svc.create_channel(e.staff, c).await.unwrap().id;
    // Listed for everyone.
    let list = e.svc.channels().await.unwrap();
    assert!(list[0].now.as_ref().unwrap().age_restricted);
    // But the stream refuses an unconfirmed account (the source has no asset 8000 yet: the gate comes first).
    assert_eq!(
        e.svc.tune(MEMBER, id).await,
        Err(TvError::AgeConfirmationRequired)
    );
    assert_eq!(
        e.svc.live_media(MEMBER, id, Track::Video(720)).await,
        Err(TvError::AgeConfirmationRequired)
    );
    assert_eq!(
        e.svc.live_master(MEMBER, id).await,
        Err(TvError::AgeConfirmationRequired)
    );
    let t = e.svc.tune(ADULT, id).await.unwrap();
    assert_eq!(t.current.unwrap().title, "Adult Film");
    // A channel that is not restricted leaves the film out unless asked.
    let plain = e
        .catalog
        .pool(&PoolFilter::default(), false, 10)
        .await
        .unwrap();
    assert!(plain.iter().all(|i| i.title_id != 80));
}

use crate::catalog::TvCatalog;

#[tokio::test]
async fn tuning_and_the_live_playlists_end_to_end() {
    let e = env();
    let id = channel(&e, 101, marathon()).await;
    // Off air before the schedule exists is impossible (it is extended on demand); tune right away.
    let t = e.svc.tune(MEMBER, id).await.unwrap();
    assert_eq!(t.state, TuneState::Live);
    assert_eq!(
        t.master_url.unwrap(),
        format!("/api/v1/tv/channels/{id}/live/master.m3u8")
    );
    assert_eq!(t.viewers, 0, "tuning alone does not count");

    // Master: the rungs of the asset on air, relative links.
    let master = e.svc.live_master(MEMBER, id).await.unwrap();
    assert!(
        master.contains("\nv/720.m3u8\n") && master.contains("\nv/1080.m3u8\n"),
        "{master}"
    );
    assert!(master.contains("URI=\"audio.m3u8\""));

    // Media playlist 6 s into the second programme; the first one's tail is in the window.
    e.clock.advance_ms(30 * MIN + 7_000);
    let first = e
        .svc
        .live_media(MEMBER, id, Track::Video(720))
        .await
        .unwrap();
    assert!(first.contains("#EXT-X-DISCONTINUITY\n"), "{first}");
    assert!(first.contains("/cdn/hls/7101/v/720p/seg_00299.m4s"));
    assert!(first.contains("/cdn/hls/7102/v/720p/seg_00000.m4s"));
    let seq = |text: &str| -> i64 {
        text.lines()
            .find_map(|l| l.strip_prefix("#EXT-X-MEDIA-SEQUENCE:"))
            .unwrap()
            .parse()
            .unwrap()
    };
    // Six seconds later the window moved by exactly one segment.
    e.clock.advance_ms(6_000);
    let second = e
        .svc
        .live_media(ADULT, id, Track::Video(720))
        .await
        .unwrap();
    assert_eq!(seq(&second), seq(&first) + 1);
    // Audio uses the same numbering rules.
    let audio = e.svc.live_media(MEMBER, id, Track::Audio).await.unwrap();
    assert!(audio.contains("/cdn/hls/7102/a/00-en-stereo/seg_"));
    // Both viewers were counted.
    assert_eq!(e.svc.channel(id).await.unwrap().viewers, 2);
    // After the viewers stop for a minute they are gone.
    e.clock.advance_ms(60_000);
    assert_eq!(e.svc.channel(id).await.unwrap().viewers, 0);
}

#[tokio::test]
async fn channels_without_media_are_off_air() {
    let e = env();
    // No dayparts: nothing is ever scheduled.
    let id = channel(&e, 200, ChannelRules::default()).await;
    let t = e.svc.tune(MEMBER, id).await.unwrap();
    assert_eq!(t.state, TuneState::OffAir);
    assert!(t.master_url.is_none() && t.current.is_none());
    assert_eq!(
        e.svc.live_media(MEMBER, id, Track::Video(720)).await,
        Err(TvError::OffAir)
    );
    // A slate on air is off air for the stream too, but the guide shows it.
    let g = e.svc.guide(None, None, &[]).await.unwrap();
    assert!(g.channels[0].blocks.is_empty());
}

#[tokio::test]
async fn an_off_air_channel_shows_the_slate_instead_of_404() {
    let e = env();
    let id = channel(&e, 201, ChannelRules::default()).await;
    let source = MemoryPlaybackSource::new();
    let mut slate = asset(9999, 2);
    for v in &mut slate.media.videos {
        for s in &mut v.segments {
            s.duration_secs = 2.0;
        }
    }
    source.insert(slate);
    let next = Arc::new(AtomicI64::new(5000));
    let svc = TvService::new(
        e.repo.clone(),
        Arc::new(e.catalog.clone()),
        e.clock.clone(),
        Arc::new(move || next.fetch_add(1, Ordering::SeqCst)),
    )
    .with_live(LiveDeps {
        source: Arc::new(source),
        signer: Arc::new(Sig),
    })
    .with_slate(9999);
    let t = svc.tune(MEMBER, id).await.unwrap();
    assert_eq!(t.state, TuneState::OffAir, "the guide still says off air");
    assert!(t.master_url.is_some(), "but the stream answers");
    let master = svc.live_master(MEMBER, id).await.unwrap();
    assert!(master.contains("v/720.m3u8"));
    let media = svc.live_media(MEMBER, id, Track::Video(720)).await.unwrap();
    assert!(media.contains("/cdn/hls/9999/v/720p/seg_0000"), "{media}");
    assert!(!media.contains("ENDLIST"));
    assert!(svc.live_media(MEMBER, id, Track::Audio).await.is_ok());
    // Without a configured slate it is still off air.
    assert_eq!(
        e.svc.live_media(MEMBER, id, Track::Video(720)).await,
        Err(TvError::OffAir)
    );
}

/// The service with a slate (asset 9999: three 2 s segments) next to the programme assets of `env`.
fn with_slate(e: &Env) -> TvService {
    let source = MemoryPlaybackSource::new();
    for n in 1..=40 {
        source.insert(asset(7100 + n, 300));
    }
    let mut slate = asset(9999, 3);
    for v in &mut slate.media.videos {
        for s in &mut v.segments {
            s.duration_secs = 2.0;
        }
    }
    for au in &mut slate.media.audios {
        for s in &mut au.segments {
            s.duration_secs = 2.0;
        }
    }
    source.insert(slate);
    let next = Arc::new(AtomicI64::new(50_000));
    TvService::new(
        e.repo.clone(),
        Arc::new(e.catalog.clone()),
        e.clock.clone(),
        Arc::new(move || next.fetch_add(1, Ordering::SeqCst)),
    )
    .with_live(LiveDeps {
        source: Arc::new(source),
        signer: Arc::new(Sig),
    })
    .with_slate(9999)
}

fn header_of(text: &str, name: &str) -> i64 {
    text.lines()
        .find_map(|l| l.strip_prefix(name))
        .unwrap()
        .parse()
        .unwrap()
}

#[tokio::test]
async fn the_slate_chains_with_the_programmes_around_it() {
    let e = env();
    let svc = with_slate(&e);
    let mut manual = input(310, ChannelRules::default());
    manual.kind = ChannelKind::Manual;
    let id = e.svc.create_channel(e.staff, manual).await.unwrap().id;
    let t = utc(2025, 6, 2, 20, 0);
    // A (30 min) at 20:00, a ten minute gap, B at 20:40.
    for (ep, start) in [(7101, t), (7102, t + 40 * MIN)] {
        let input = PlaceInput {
            title_id: 7,
            episode_id: Some(ep),
            start_ms: start,
            end_ms: None,
        };
        e.svc.place_block(e.staff, id, input).await.unwrap();
    }
    let seq = |text: &str| header_of(text, "#EXT-X-MEDIA-SEQUENCE:");
    let disc_tags = |text: &str| text.matches("#EXT-X-DISCONTINUITY\n").count();

    // 20:01: A is on air; its segment 10 is the newest, so the head is segment 5.
    e.clock.advance_ms(MIN);
    let a = svc.live_media(MEMBER, id, Track::Video(720)).await.unwrap();
    let a_base = seq(&a) - 5;

    // 20:30:01: A has ended, the slate follows it. Its first segment is numbered one above A's last
    // (300 segments on air), the tail of A is still listed, with one discontinuity between.
    e.clock.advance_ms(29 * MIN + 1_000);
    let s1 = svc.live_media(MEMBER, id, Track::Video(720)).await.unwrap();
    assert!(s1.contains("/cdn/hls/7101/v/720p/seg_00299.m4s"), "{s1}");
    assert!(s1.contains("/cdn/hls/9999/v/720p/seg_"), "{s1}");
    assert_eq!(disc_tags(&s1), 1);
    let tune = svc.tune(MEMBER, id).await.unwrap();
    assert_eq!(
        tune.state,
        TuneState::OffAir,
        "the guide still says off air"
    );

    // Six seconds on: the window moved by three slate segments (2 s each), nothing renumbered.
    e.clock.advance_ms(6_000);
    let s2 = svc.live_media(MEMBER, id, Track::Video(720)).await.unwrap();
    assert_eq!(seq(&s2) - seq(&s1), 3);

    // 20:40:01: B starts mid-slate. It counts on from the slate segment on air at 20:40:00, which is
    // 300 slate segments after the first one: B's first segment is a_base + 300 + 300 + 1, and the
    // window lists the slate's last five segments before it with one discontinuity.
    e.clock.advance_ms(9 * MIN + 54_000);
    let b = svc.live_media(MEMBER, id, Track::Video(720)).await.unwrap();
    assert!(b.contains("/cdn/hls/7102/v/720p/seg_00000.m4s"), "{b}");
    // The last discontinuity is the one into B (the slate loop wraps inside its tail as well).
    let after_last = b
        .rsplit_once(
            "#EXT-X-DISCONTINUITY
",
        )
        .unwrap()
        .1;
    assert!(
        after_last.starts_with("#EXT-X-MAP:URI=\"/cdn/hls/7102/"),
        "{b}"
    );
    assert_eq!(seq(&b), a_base + 601 - 5, "{b}");
    // The audio playlist numbers alike.
    let audio = svc.live_media(MEMBER, id, Track::Audio).await.unwrap();
    assert_eq!(seq(&audio), seq(&b));

    // The same channel without the slate has nothing on between the programmes.
    e.clock.set(t + 35 * MIN);
    assert_eq!(
        e.svc.live_media(MEMBER, id, Track::Video(720)).await,
        Err(TvError::OffAir)
    );
}

// ---- reminders (G-10) ------------------------------------------------------------------------

#[derive(Default)]
struct Sink {
    seen: std::sync::Mutex<Vec<crate::service::ReminderNotice>>,
    fail: std::sync::atomic::AtomicBool,
}

#[async_trait::async_trait]
impl crate::service::ReminderSink for Sink {
    async fn notify(&self, n: &crate::service::ReminderNotice) -> Result<(), String> {
        if self.fail.load(Ordering::SeqCst) {
            return Err("offline".into());
        }
        self.seen.lock().unwrap().push(n.clone());
        Ok(())
    }
}

impl Sink {
    fn count(&self) -> usize {
        self.seen.lock().unwrap().len()
    }
}

/// The channel and its block that starts at 20:30 (the clock stands at 20:00).
async fn next_block(e: &Env) -> (i64, i64) {
    let id = channel(e, 101, marathon()).await;
    let next = e.svc.now(id).await.unwrap().next.unwrap();
    assert_eq!(next.start_ms, utc(2025, 6, 2, 20, 30));
    (id, next.id)
}

#[tokio::test]
async fn a_reminder_fires_once_five_minutes_before_the_programme() {
    let e = env();
    let (_, block) = next_block(&e).await;
    let sink = Sink::default();
    e.svc.set_reminder(MEMBER, block, None).await.unwrap();
    // Setting it again moves it, it does not duplicate it.
    e.svc.set_reminder(MEMBER, block, None).await.unwrap();

    e.clock.advance_ms(24 * MIN);
    assert_eq!(e.svc.fire_reminders(&sink).await.unwrap(), 0);
    e.clock.advance_ms(MIN);
    assert_eq!(e.svc.fire_reminders(&sink).await.unwrap(), 1);
    assert_eq!(e.svc.fire_reminders(&sink).await.unwrap(), 0, "once only");
    match &sink.seen.lock().unwrap()[0] {
        crate::service::ReminderNotice::Starting {
            account_id,
            channel_number,
            title,
            start_ms,
            ..
        } => {
            assert_eq!(*account_id, MEMBER.user_id);
            assert_eq!(*channel_number, 101);
            assert!(title.starts_with("Cowboy Bebop"));
            assert_eq!(*start_ms, utc(2025, 6, 2, 20, 30));
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn the_lead_setting_applies_when_no_lead_is_asked_and_the_list_follows_the_guide() {
    let e = env();
    let (_, block) = next_block(&e).await;
    e.repo.set_lead_setting(MEMBER.user_id, 20);
    e.repo.set_lead_setting(ADULT.user_id, 9_999);
    e.svc.set_reminder(MEMBER, block, None).await.unwrap();
    // An out-of-range blob value falls back to 5; an explicit lead beats the setting.
    e.svc.set_reminder(ADULT, block, None).await.unwrap();
    let mine = e.svc.list_reminders(MEMBER).await.unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].lead_minutes, 20);
    assert!(
        mine[0]
            .title
            .as_deref()
            .unwrap()
            .starts_with("Cowboy Bebop")
    );
    assert_eq!(
        e.svc.list_reminders(ADULT).await.unwrap()[0].lead_minutes,
        5
    );
    e.svc.set_reminder(MEMBER, block, Some(0)).await.unwrap();
    assert_eq!(
        e.svc.list_reminders(MEMBER).await.unwrap()[0].lead_minutes,
        0
    );
    // The reminder fires at the setting's lead: 20 minutes before 20:30 is 20:10.
    e.svc.set_reminder(MEMBER, block, None).await.unwrap();
    e.clock.advance_ms(9 * MIN);
    assert_eq!(e.svc.fire_reminders(&Sink::default()).await.unwrap(), 0);
    e.clock.advance_ms(MIN);
    assert_eq!(e.svc.fire_reminders(&Sink::default()).await.unwrap(), 1);
    assert!(e.svc.list_reminders(MEMBER).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_reminder_can_be_cancelled_and_a_lead_can_be_chosen() {
    let e = env();
    let (_, block) = next_block(&e).await;
    let sink = Sink::default();
    e.svc.set_reminder(MEMBER, block, Some(10)).await.unwrap();
    e.svc.set_reminder(ADULT, block, None).await.unwrap();
    e.svc.clear_reminder(ADULT, block).await.unwrap();
    e.svc.clear_reminder(ADULT, block).await.unwrap();
    e.clock.advance_ms(20 * MIN);
    assert_eq!(
        e.svc.fire_reminders(&sink).await.unwrap(),
        1,
        "10 minutes before 20:30"
    );
    assert!(matches!(
        sink.seen.lock().unwrap()[0],
        crate::service::ReminderNotice::Starting { account_id: 5, .. }
    ));
    assert!(matches!(
        e.svc.set_reminder(MEMBER, block, Some(500)).await,
        Err(TvError::Invalid(_))
    ));
    assert!(matches!(
        e.svc.set_reminder(MEMBER, block, Some(-1)).await,
        Err(TvError::Invalid(_))
    ));
}

#[tokio::test]
async fn reminders_need_an_upcoming_programme() {
    let e = env();
    let id = channel(&e, 101, marathon()).await;
    let now_block = e.svc.now(id).await.unwrap().current.unwrap();
    assert!(matches!(
        e.svc.set_reminder(MEMBER, now_block.id, None).await,
        Err(TvError::Invalid(_))
    ));
    assert_eq!(
        e.svc.set_reminder(MEMBER, 999_999, None).await,
        Err(TvError::NotFound("block"))
    );
}

#[tokio::test]
async fn a_reminder_for_a_block_that_was_regenerated_away_ends_with_a_notice() {
    let e = env();
    let (id, block) = next_block(&e).await;
    let sink = Sink::default();
    e.svc.set_reminder(MEMBER, block, None).await.unwrap();
    e.svc.remove_block(e.staff, block).await.unwrap();
    e.clock.advance_ms(26 * MIN);
    assert_eq!(e.svc.fire_reminders(&sink).await.unwrap(), 1);
    match &sink.seen.lock().unwrap()[0] {
        crate::service::ReminderNotice::Dropped {
            channel_id, title, ..
        } => {
            assert_eq!(*channel_id, id);
            assert!(title.starts_with("Cowboy Bebop"));
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_failed_notice_is_tried_again_until_the_programme_starts() {
    let e = env();
    let (_, block) = next_block(&e).await;
    let sink = Sink::default();
    e.svc.set_reminder(MEMBER, block, None).await.unwrap();
    sink.fail.store(true, Ordering::SeqCst);
    e.clock.advance_ms(26 * MIN);
    assert_eq!(e.svc.fire_reminders(&sink).await.unwrap(), 0);
    sink.fail.store(false, Ordering::SeqCst);
    e.clock.advance_ms(MIN);
    assert_eq!(e.svc.fire_reminders(&sink).await.unwrap(), 1);
    assert_eq!(sink.count(), 1);
}

// ---- the watch party gate for Live TV (G-16) ----------------------------------------------------

#[tokio::test]
async fn the_watch_party_gate_resolves_live_channels_only() {
    use sr_watchparty::{GateError, PlayGate};
    let e = env();
    let id = channel(&e, 101, marathon()).await;
    let r = e.svc.resolve_live(false, id).await.unwrap();
    assert!(r.live && r.duration_ms.is_none());
    assert_eq!((r.title_id, r.asset_id, r.episode_id), (id, id, 0));
    assert_eq!(r.title, "101 Channel 101");
    assert_eq!(r.episode_label.as_deref(), Some("Cowboy Bebop"));
    assert!(!r.age_restricted);
    // Not a Cinema gate, an unknown channel, a disabled one and one with nothing on air.
    assert_eq!(
        e.svc.resolve(true, 7, None, None).await,
        Err(GateError::NotFound)
    );
    assert_eq!(
        e.svc.resolve_live(true, 4242).await,
        Err(GateError::NotFound)
    );
    let empty = channel(&e, 200, ChannelRules::default()).await;
    assert_eq!(
        e.svc.resolve_live(true, empty).await,
        Err(GateError::NotFound)
    );
    let mut off = input(101, marathon());
    off.enabled = false;
    e.svc.update_channel(e.staff, id, off).await.unwrap();
    assert_eq!(e.svc.resolve_live(true, id).await, Err(GateError::NotFound));
}

#[tokio::test]
async fn a_restricted_programme_needs_age_confirmation_for_a_live_party_too() {
    use sr_watchparty::{GateError, PlayGate};
    let e = env();
    let mut film = item(80, "Adult Film", 120);
    film.age_restricted = true;
    film.asset_id = 8000;
    e.catalog.add(film);
    let mut c = input(
        666,
        ChannelRules {
            grid_minutes: 0,
            no_repeat_window: 0,
            dayparts: vec![daypart(
                "Late",
                PickMode::Sequential,
                PoolFilter {
                    title_ids: vec![80],
                    restricted: Some(Restricted::Only),
                    ..PoolFilter::default()
                },
            )],
        },
    );
    c.age_restricted = true;
    let id = e.svc.create_channel(e.staff, c).await.unwrap().id;
    assert_eq!(
        e.svc.resolve_live(false, id).await,
        Err(GateError::AgeConfirmationRequired)
    );
    let r = e.svc.resolve_live(true, id).await.unwrap();
    assert!(r.age_restricted);
}

// ---- imported guides (TV-ENG-003, TV-ENG-007, TV-ADM-003, TV-ADM-004) ------------------------------

async fn guide_channel(e: &Env) -> i64 {
    let mut i = input(120, ChannelRules::default());
    i.kind = ChannelKind::ImportedGuide;
    e.svc.create_channel(e.staff, i).await.unwrap().id
}

const GUIDE_CSV: &str = "weekday,start_time,duration,show,season,episode,note\n\
Mon,20:00,30,Cowboy Bebop,1,3,\n\
Mon,20:30,30,Nonexistent Show,,,\n\
Mon,21:00,30,The Big Movie,,,\n";

#[tokio::test]
async fn an_import_is_matched_confirmed_and_aired_with_the_missing_show_skipped() {
    let e = env();
    e.catalog.add(item(3, "The Big Movie", 25));
    e.catalog.add(item(4, "The Big Movie", 25));
    let id = guide_channel(&e).await;
    // Staff only.
    let nobody = Actor {
        user_id: 2,
        permissions: P::PUBLISH_MEDIA,
    };
    assert!(
        e.svc
            .import_guide(nobody, id, "csv", GUIDE_CSV)
            .await
            .is_err()
    );
    let imp = e
        .svc
        .import_guide(e.staff, id, "csv", GUIDE_CSV)
        .await
        .unwrap();
    let r = &imp.report;
    assert_eq!((r.matched, r.ambiguous, r.missing, r.invalid), (1, 1, 1, 0));
    // The upload changed nothing yet.
    assert!(e.svc.slots(e.staff, id).await.unwrap().is_empty());
    let ambiguous = r.rows.iter().find(|x| x.show == "The Big Movie").unwrap();
    assert_eq!(ambiguous.candidates.len(), 2);
    let done = e
        .svc
        .confirm_guide(
            e.staff,
            id,
            imp.id,
            crate::service::ConfirmInput {
                choices: vec![crate::service::GuideChoice {
                    line: ambiguous.line,
                    title_id: Some(4),
                }],
            },
        )
        .await
        .unwrap();
    assert_eq!((done.slots, done.skipped), (3, 1));
    // Twice is refused.
    assert!(matches!(
        e.svc
            .confirm_guide(e.staff, id, imp.id, Default::default())
            .await,
        Err(TvError::Conflict(_))
    ));
    // The clock is Monday 20:00: Cowboy Bebop S1E3 is on, the missing show leaves a gap, then the film.
    let ch = e.svc.channel(id).await.unwrap();
    let now = ch.now.unwrap();
    assert_eq!(
        (now.title.as_str(), now.episode_number),
        ("Cowboy Bebop", Some(3))
    );
    let blocks = e
        .svc
        .admin_schedule(
            e.staff,
            id,
            Some(utc(2025, 6, 2, 20, 0)),
            Some(utc(2025, 6, 2, 22, 0)),
        )
        .await
        .unwrap();
    let shape: Vec<_> = blocks
        .iter()
        .map(|b| (b.start_ms, b.title.as_str()))
        .collect();
    assert_eq!(shape[0], (utc(2025, 6, 2, 20, 0), "Cowboy Bebop"));
    assert_eq!(shape[1], (utc(2025, 6, 2, 20, 30), "We'll be right back"));
    assert_eq!(shape[2], (utc(2025, 6, 2, 21, 0), "The Big Movie"));
    assert_eq!(blocks[2].title_id, Some(4));
    // The report is kept and listed.
    assert!(
        e.svc
            .guide_import(e.staff, id, imp.id)
            .await
            .unwrap()
            .confirmed
    );
    assert_eq!(e.svc.guide_imports(e.staff, id).await.unwrap().len(), 1);
}

#[tokio::test]
async fn the_weekly_grid_is_saved_whole_and_overlaps_are_refused() {
    let e = env();
    let id = guide_channel(&e).await;
    let slot = |wd: u8, start: &str, dur: u16, show: &str, title: Option<i64>| Slot {
        id: None,
        weekday: wd,
        start: start.into(),
        duration_min: dur,
        show_name: show.into(),
        title_id: title,
        episode_id: None,
        season_number: None,
        episode_number: None,
        note: None,
    };
    let grid = vec![
        slot(0, "20:00", 30, "Cowboy Bebop", Some(7)),
        slot(2, "23:30", 90, "Late", None),
    ];
    let saved = e.svc.put_slots(e.staff, id, grid.clone()).await.unwrap();
    assert_eq!(saved.len(), 2);
    assert!(saved.iter().all(|s| s.id.is_some()));
    // A series slot without an episode rotates through the episodes: week one airs episode 1.
    let now = e.svc.channel(id).await.unwrap().now.unwrap();
    assert_eq!(
        (now.title.as_str(), now.episode_number),
        ("Cowboy Bebop", Some(1))
    );
    // Overlaps, also across midnight and the turn of the week, are refused.
    let mut bad = grid.clone();
    bad.push(slot(3, "00:30", 30, "Clash", None));
    assert!(matches!(
        e.svc.put_slots(e.staff, id, bad).await,
        Err(TvError::Invalid(m)) if m.contains("overlap")
    ));
    let mut bad = grid.clone();
    bad.push(slot(6, "23:00", 90, "Sunday night", None));
    bad.push(slot(0, "00:15", 10, "Monday dawn", None));
    assert!(e.svc.put_slots(e.staff, id, bad).await.is_err());
    assert!(
        e.svc
            .put_slots(e.staff, id, vec![slot(9, "20:00", 30, "X", None)])
            .await
            .is_err()
    );
    assert!(
        e.svc
            .put_slots(e.staff, id, vec![slot(1, "25:00", 30, "X", None)])
            .await
            .is_err()
    );
    // Rule-based channels have no grid.
    let rb = channel(&e, 130, marathon()).await;
    assert!(e.svc.slots(e.staff, rb).await.is_err());
    // The grid is unchanged by the refusals.
    assert_eq!(e.svc.slots(e.staff, id).await.unwrap().len(), 2);
}

#[tokio::test]
async fn revalidation_relinks_slots_and_regenerates_a_stale_schedule() {
    let e = env();
    let id = guide_channel(&e).await;
    let imp = e
        .svc
        .import_guide(
            e.staff,
            id,
            "json",
            r#"[{"weekday":"Mon","start_time":"21:00","duration":30,"show":"Late Arrival"}]"#,
        )
        .await
        .unwrap();
    e.svc
        .confirm_guide(e.staff, id, imp.id, Default::default())
        .await
        .unwrap();
    // Nothing to air: a slate, and a pass that finds nothing new changes nothing.
    e.svc.channel(id).await.unwrap();
    assert!(e.svc.revalidate_guides().await.unwrap().is_empty());
    // The library gains the show: the slot is linked and the stored schedule regenerated.
    e.catalog.add(item(9, "Late Arrival", 30));
    assert_eq!(e.svc.revalidate_guides().await.unwrap(), vec![id]);
    let slots = e.svc.slots(e.staff, id).await.unwrap();
    assert_eq!(slots[0].title_id, Some(9));
    let blocks = e
        .svc
        .admin_schedule(
            e.staff,
            id,
            Some(utc(2025, 6, 2, 21, 0)),
            Some(utc(2025, 6, 2, 21, 30)),
        )
        .await
        .unwrap();
    assert_eq!(blocks[0].title, "Late Arrival");
    // A second pass is quiet; a title removed from the library regenerates again.
    assert!(e.svc.revalidate_guides().await.unwrap().is_empty());
    e.catalog.clear();
    assert_eq!(e.svc.revalidate_guides().await.unwrap(), vec![id]);
}

#[tokio::test]
async fn favorites_are_per_member_and_only_for_enabled_channels() {
    let e = env();
    let a = channel(&e, 140, marathon()).await;
    let b = channel(&e, 141, marathon()).await;
    e.svc.set_favorite(MEMBER, a, true).await.unwrap();
    e.svc.set_favorite(MEMBER, b, true).await.unwrap();
    e.svc.set_favorite(MEMBER, b, true).await.unwrap();
    e.svc.set_favorite(ADULT, b, true).await.unwrap();
    assert_eq!(e.svc.favorites(MEMBER).await.unwrap(), vec![a, b]);
    e.svc.set_favorite(MEMBER, a, false).await.unwrap();
    e.svc.set_favorite(MEMBER, a, false).await.unwrap();
    assert_eq!(e.svc.favorites(MEMBER).await.unwrap(), vec![b]);
    assert_eq!(e.svc.favorites(ADULT).await.unwrap(), vec![b]);
    assert!(matches!(
        e.svc.set_favorite(MEMBER, 999, true).await,
        Err(TvError::NotFound(_))
    ));
}
