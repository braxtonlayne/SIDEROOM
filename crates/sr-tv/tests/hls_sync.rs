//! TV-HLS-005: two viewers of one channel stay within one segment of each other, and a programme boundary
//! carries a discontinuity (the browser-level check of Safari is the e2e agent's; this is the playlist side).
//! No browser, no server: the real window code on a fixed clock.

use sr_media::playback::{PlaybackAsset, sample_asset};
use sr_media::playlist::{Segment, UrlSigner};
use sr_tv::live::{LiveBlock, Track, WINDOW_SEGMENTS, on_air_count, render_media};
use sr_tv::model::{BlockKind, BlockRecord, BlockSource};

struct Sig;
impl UrlSigner for Sig {
    fn signed_url(&self, path: &str, expires_at: u64) -> String {
        format!("{path}?ex={expires_at}&sig=S")
    }
}

const SEG_SECS: f64 = 6.0;

fn asset(id: i64, n: usize) -> PlaybackAsset {
    let mut a = sample_asset(id);
    let segs = |dir: &str| {
        (0..n)
            .map(|k| Segment {
                path: format!("{dir}/seg_{k:05}.m4s"),
                duration_secs: SEG_SECS,
            })
            .collect::<Vec<_>>()
    };
    for v in &mut a.media.videos {
        v.segments = segs(&format!("v/{}", v.name));
    }
    for au in &mut a.media.audios {
        let dir = au.playlist_path.rsplit_once('/').unwrap().0.to_owned();
        au.segments = segs(&dir);
    }
    a.duration_ms = Some(n as i64 * 6000);
    a
}

fn block(id: i64, start: i64, end: i64, seq: i64, disc: i64, n: usize) -> LiveBlock {
    LiveBlock {
        block: BlockRecord {
            id,
            channel_id: 1,
            start_ms: start,
            end_ms: end,
            kind: BlockKind::Program,
            title_id: Some(id),
            episode_id: None,
            asset_id: Some(id * 100),
            title: format!("T{id}"),
            episode_title: None,
            media_kind: None,
            season_number: None,
            episode_number: None,
            year: None,
            age_restricted: false,
            offset_ms: 0,
            source: BlockSource::Generated,
            cursor_after: None,
            seq_base: Some(seq),
            disc_base: Some(disc),
        },
        asset: asset(id * 100, n),
    }
}

/// (media sequence, segment paths) of a playlist.
fn parse(text: &str) -> (i64, Vec<String>) {
    let seq = text
        .lines()
        .find_map(|l| l.strip_prefix("#EXT-X-MEDIA-SEQUENCE:"))
        .unwrap()
        .parse()
        .unwrap();
    let segs = text
        .lines()
        .filter(|l| l.starts_with("/cdn/hls/"))
        .map(|l| l.split('?').next().unwrap().to_owned())
        .collect();
    (seq, segs)
}

/// The sequence number of the newest segment on air.
fn live_edge(text: &str) -> i64 {
    let (seq, segs) = parse(text);
    seq + segs.len() as i64 - 1
}

fn fetch(blocks: &[LiveBlock], track: Track, now_ms: i64) -> String {
    render_media(blocks, track, now_ms, &Sig, (now_ms / 1000) as u64).unwrap()
}

#[test]
fn two_clients_with_skewed_clocks_are_within_one_segment() {
    // A 30 minute programme, 300 segments; client B joins later and its clock runs ahead and behind.
    let blocks = [block(1, 0, 1_800_000, 5000, 1, 300)];
    let seg_ms = (SEG_SECS * 1000.0) as i64;
    for track in [Track::Video(720), Track::Video(1080), Track::Audio] {
        // Every 700 ms for ten minutes, A polls on time and B up to a second off in either direction.
        let mut t = 120_000;
        while t < 720_000 {
            for skew in [-1000_i64, -250, 0, 250, 1000] {
                let a = live_edge(&fetch(&blocks, track, t));
                let b = live_edge(&fetch(&blocks, track, t + skew));
                assert!(
                    (a - b).abs() <= 1,
                    "{track:?} at {t} ms skew {skew}: edges {a} vs {b}"
                );
            }
            t += 700;
        }
    }
    // Inside one segment the two clients get byte-identical playlists apart from the signing expiry.
    let strip = |s: String| {
        s.lines()
            .map(|l| l.split("?ex=").next().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    let t = 300_000 + 100;
    assert_eq!(
        strip(fetch(&blocks, Track::Video(720), t)),
        strip(fetch(&blocks, Track::Video(720), t + seg_ms - 200))
    );
}

#[test]
fn a_client_that_reloads_slowly_never_sees_the_sequence_go_backwards() {
    let blocks = [block(1, 0, 1_800_000, 5000, 1, 300)];
    let mut last_edge = i64::MIN;
    let mut last_head = i64::MIN;
    // A reload every 4.5 s, i.e. faster than the segment length, plus a few long stalls.
    for t in (60_000..400_000).step_by(4500).chain([410_000, 480_000]) {
        let text = fetch(&blocks, Track::Video(720), t);
        let (head, segs) = parse(&text);
        assert_eq!(segs.len(), WINDOW_SEGMENTS);
        assert!(head >= last_head, "the window never moves backwards");
        assert!(live_edge(&text) >= last_edge);
        // The same segment keeps its number: path index = sequence - base.
        assert!(segs[0].ends_with(&format!("seg_{:05}.m4s", head - 5000)));
        last_head = head;
        last_edge = live_edge(&text);
    }
}

#[test]
fn both_clients_see_the_discontinuity_at_the_programme_boundary() {
    let b1 = block(1, 0, 120_000, 100, 4, 20);
    let b2 = block(
        2,
        120_000,
        600_000,
        100 + on_air_count(&b1.block, &b1.asset),
        5,
        80,
    );
    let blocks = [b1, b2];
    let sees = |t: i64| {
        let text = fetch(&blocks, Track::Video(720), t);
        (
            text.contains("#EXT-X-DISCONTINUITY\n"),
            text.contains("#EXT-X-DISCONTINUITY-SEQUENCE:4"),
            text.contains("#EXT-X-DISCONTINUITY-SEQUENCE:5"),
            live_edge(&text),
        )
    };
    // 2 s into the new programme: the previous one still fills the window; both clients (one a second
    // earlier, still in the old programme's last segment) agree to within one segment.
    let (d_late, ds4_late, _, edge_late) = sees(122_000);
    let (d_early, ds4_early, _, edge_early) = sees(119_000);
    assert!(
        d_late && ds4_late,
        "the new programme follows a discontinuity"
    );
    assert!(!d_early && ds4_early, "the old programme has none yet");
    assert_eq!(
        edge_late - edge_early,
        1,
        "the numbering is continuous across the boundary"
    );
    // Once the window has moved fully into the new programme the counter has advanced.
    let (d, _, ds5, _) = sees(120_000 + 7 * 6000);
    assert!(
        !d && ds5,
        "past the boundary the old programme left the window"
    );
    // The map changes with the programme.
    let text = fetch(&blocks, Track::Video(720), 122_000);
    let maps: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("#EXT-X-MAP"))
        .collect();
    assert_eq!(maps.len(), 2);
    assert_ne!(maps[0], maps[1]);
}

// ---- D-267: one numbering domain across programmes and the slate ---------------------------------------

use sr_tv::live::{
    SlateChain, SlateLead, bases_after_slate, render_media_with_lead, render_slate_media,
    slate_position, slate_window_ms,
};
use sr_tv::model::ChainAnchor;
use std::collections::HashMap;

/// xorshift64, so the schedules are reproducible without a dependency.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
}

/// An asset whose segments (and those of its audio) last `seg_secs` each.
fn asset_with(id: i64, n: usize, seg_secs: f64) -> PlaybackAsset {
    let mut a = asset(id, n);
    for v in &mut a.media.videos {
        for s in &mut v.segments {
            s.duration_secs = seg_secs;
        }
    }
    for au in &mut a.media.audios {
        for s in &mut au.segments {
            s.duration_secs = seg_secs;
        }
    }
    a.duration_ms = Some((n as f64 * seg_secs * 1000.0) as i64);
    a
}

/// The asset's audio renditions end in a 21 ms sliver segment the video does not have, as ffmpeg cuts an AAC
/// track that runs past the video by its encoder priming.
fn with_audio_sliver(a: &mut PlaybackAsset) {
    for au in &mut a.media.audios {
        let dir = au.playlist_path.rsplit_once('/').unwrap().0.to_owned();
        let k = au.segments.len();
        au.segments.push(Segment {
            path: format!("{dir}/seg_{k:05}.m4s"),
            duration_secs: 0.021,
        });
    }
}

/// One entry of a parsed playlist: (sequence, discontinuity counter, asset id, segment file).
type Seg = (i64, i64, String, String);

fn entries(text: &str) -> Vec<Seg> {
    let (head, _) = parse(text);
    let mut disc: i64 = text
        .lines()
        .find_map(|l| l.strip_prefix("#EXT-X-DISCONTINUITY-SEQUENCE:"))
        .unwrap()
        .parse()
        .unwrap();
    let mut out = Vec::new();
    for l in text.lines() {
        if l == "#EXT-X-DISCONTINUITY" {
            disc += 1;
        } else if let Some(p) = l.strip_prefix("/cdn/hls/") {
            let p = p.split('?').next().unwrap();
            let (asset, file) = (p.split('/').next().unwrap(), p.rsplit('/').next().unwrap());
            out.push((
                head + out.len() as i64,
                disc,
                asset.to_owned(),
                file.to_owned(),
            ));
        }
    }
    out
}

#[test]
fn random_schedules_with_a_slate_never_move_the_sequence_backwards() {
    const HOUR: i64 = 3_600_000;
    const DAY: i64 = 24 * HOUR;
    let slate_id = 9999;
    for case in 0..150_u64 {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ (case + 1).wrapping_mul(0xD1B5_4A32_D192_ED03));
        for _ in 0..4 {
            rng.next();
        }
        // The slate: 2..=5 segments of 1, 2, 2.5 or 4 s, whatever the programmes use.
        let slate_secs = [1.0, 2.0, 2.5, 4.0][rng.range(0, 3) as usize];
        let mut slate = asset_with(slate_id, rng.range(2, 5) as usize, slate_secs);
        // As ffmpeg cuts a real slate: the AAC track runs 21 ms past the video (encoder priming), so its
        // rendition ends in a sliver segment the video does not have (CI run of f1e89b77).
        if rng.range(0, 1) == 0 {
            with_audio_sliver(&mut slate);
        }
        let seg_ms = (slate_secs * 1000.0) as i64;
        let n_slate = slate.media.videos[0].segments.len() as i64;
        let cycle_ms = seg_ms * n_slate;
        // Programmes with gaps (some none: contiguous; some longer than 14 days), cut and offset at random,
        // from a start that is not aligned to anything. Some are never viewed (nobody samples inside them) and
        // some lose their asset while (or before) they air.
        let t0 = 1_700_000_000_000 + rng.range(0, 999_999);
        let mut cursor = t0;
        let mut proto: Vec<LiveBlock> = Vec::new();
        let mut unviewed: Vec<bool> = Vec::new();
        let mut gone_at: Vec<Option<i64>> = Vec::new();
        for id in 1..=rng.range(4, 8) {
            let gap = match rng.range(0, 5) {
                0 | 1 => 0,
                2 | 3 => rng.range(1, 90_000),
                4 => rng.range(2 * HOUR, 3 * DAY),
                _ => rng.range(15 * DAY, 40 * DAY),
            };
            // A block after one that loses its asset is not contiguous with it (the service would not play it).
            let after_gone = gone_at.last().is_some_and(Option::is_some);
            cursor += if after_gone { gap.max(1_000) } else { gap };
            let seg = [2.0, 4.0, 6.0][rng.range(0, 2) as usize];
            let len = rng.range(20, 240) * 1000;
            let offset = rng.range(0, 40) * 1000;
            let n = ((len + offset) as f64 / (seg * 1000.0)).ceil() as usize + 2;
            let mut b = block(id, cursor, cursor + len, 0, 0, n);
            b.asset = asset_with(id * 100, n, seg);
            if rng.range(0, 2) == 0 {
                with_audio_sliver(&mut b.asset);
            }
            b.block.offset_ms = offset;
            b.block.seq_base = None;
            b.block.disc_base = None;
            proto.push(b);
            unviewed.push(rng.range(0, 4) == 0);
            gone_at.push(match rng.range(0, 6) {
                0 => Some(cursor - rng.range(1, 3 * DAY)),
                1 => Some(cursor + rng.range(0, len)),
                _ => None,
            });
            cursor += len;
        }
        let t_end = cursor + 60_000;
        // Every track is asked at the same moments, so the tracks can be compared with each other.
        const TRACKS: [Track; 3] = [Track::Video(720), Track::Video(480), Track::Audio];
        {
            // The channel as the service keeps it: bases per block, assigned when first viewed (and the block
            // before a viewed one, when contiguous), and the chain anchor.
            let mut live = proto.clone();
            let mut anchor: Option<ChainAnchor> = None;
            let gone = |i: usize, t: i64| gone_at[i].is_some_and(|g| g <= t);
            // Give block `i` its bases. Returns false when the block cannot be played.
            let assign =
                |live: &mut Vec<LiveBlock>, anchor: &mut Option<ChainAnchor>, i: usize, t: i64| {
                    fn go(
                        live: &mut Vec<LiveBlock>,
                        anchor: &mut Option<ChainAnchor>,
                        slate: &PlaybackAsset,
                        i: usize,
                        gone: &dyn Fn(usize, i64) -> bool,
                        t: i64,
                    ) {
                        if live[i].block.seq_base.is_some() {
                            return;
                        }
                        let start = live[i].block.start_ms;
                        let contiguous_pred =
                            i > 0 && live[i - 1].block.end_ms == start && !gone(i - 1, t);
                        if contiguous_pred {
                            go(live, anchor, slate, i - 1, gone, t);
                        }
                        let chained = contiguous_pred
                            .then(|| {
                                let p = &live[i - 1];
                                Some((
                                    p.block.seq_base? + on_air_count(&p.block, &p.asset),
                                    p.block.disc_base? + 1,
                                ))
                            })
                            .flatten();
                        let (seq, disc) = chained.unwrap_or_else(|| {
                            let chain = anchor
                                .filter(|a| a.end_ms <= start)
                                .map(|a| SlateChain::from_anchor(&a, None));
                            bases_after_slate(slate, chain.as_ref(), start).unwrap()
                        });
                        live[i].block.seq_base = Some(seq);
                        live[i].block.disc_base = Some(disc);
                        let end = live[i].block.end_ms;
                        if anchor.is_none_or(|a| a.end_ms < end) {
                            *anchor = Some(ChainAnchor {
                                end_ms: end,
                                seq: seq + on_air_count(&live[i].block, &live[i].asset),
                                disc: disc + 1,
                                block_id: live[i].block.id,
                            });
                        }
                    }
                    go(live, anchor, &slate, i, &gone, t);
                };
            // Every number maps to one segment and one discontinuity counter, whoever asks and when (per track).
            let mut seen: [HashMap<i64, (i64, String, String)>; 3] = Default::default();
            let mut last = [(i64::MIN, i64::MIN, i64::MIN); 3];
            let mut last_text: [String; 3] = Default::default();
            // Across the tracks: a number names the same segment of the same asset on the video and the audio
            // playlists (the player keeps them in step by number), whoever asks and when.
            let mut across: HashMap<i64, (i64, String, String)> = HashMap::new();
            let mut t = t0 - 5_000;
            while t < t_end {
                // Long gaps: look at their start and their end, not at the middle.
                if let Some(next) = live.iter().map(|b| b.block.start_ms).find(|s| *s > t)
                    && next - t > 200_000
                    && live.iter().all(|b| !b.block.contains(t))
                    && t - live
                        .iter()
                        .map(|b| b.block.end_ms)
                        .filter(|e| *e <= t)
                        .max()
                        .unwrap_or(t0)
                        > 60_000
                {
                    t = next - 60_000;
                }
                let cur = live.iter().position(|b| b.block.contains(t));
                let playable = cur.filter(|c| !gone(*c, t));
                if let Some(c) = cur
                    && playable.is_some()
                    && unviewed[c]
                {
                    t += rng.range(400, 3_500);
                    continue;
                }
                let rendered: Vec<(String, (String, String))> = match playable {
                    Some(c) => {
                        assign(&mut live, &mut anchor, c, t);
                        let mut blocks = Vec::new();
                        if c > 0 && live[c - 1].block.end_ms == live[c].block.start_ms {
                            blocks.push(live[c - 1].clone());
                        }
                        blocks.push(live[c].clone());
                        // The slate before a young programme, as the service finds it.
                        let cb = &live[c];
                        let (seq, disc) = (cb.block.seq_base.unwrap(), cb.block.disc_base.unwrap());
                        let start = cb.block.start_ms;
                        let found = live
                            .iter()
                            .enumerate()
                            .filter(|(i, b)| {
                                *i != c && b.block.end_ms <= start && b.block.end_ms > start - HOUR
                            })
                            .max_by_key(|(_, b)| b.block.end_ms)
                            .map(|(i, b)| (i, b.clone()));
                        let bound = found.as_ref().map(|(_, b)| b.block.end_ms);
                        let prev = found
                            .filter(|(i, b)| !gone(*i, t) && b.block.seq_base.is_some())
                            .map(|(_, b)| b)
                            .filter(|b| {
                                SlateChain::after(b).is_some_and(|ch| {
                                    slate_position(&slate, Some(&ch), start)
                                        == Some((seq - 1, disc - 1))
                                })
                            });
                        let chain = match &prev {
                            Some(p) => SlateChain::after(p).unwrap(),
                            None => SlateChain::before_programme(start, seq, disc, bound),
                        };
                        let lead = SlateLead {
                            asset: &slate,
                            chain: Some(chain),
                        };
                        let b = &live[c];
                        let bseg =
                            (b.asset.media.videos[0].segments[0].duration_secs * 1000.0) as i64;
                        let k = (t - b.block.start_ms + b.block.offset_ms) / bseg;
                        TRACKS
                            .iter()
                            .map(|track| {
                                let text = render_media_with_lead(
                                    &blocks,
                                    *track,
                                    t,
                                    Some(&lead),
                                    &Sig,
                                    0,
                                )
                                .unwrap();
                                (text, (b.asset.id.to_string(), format!("seg_{k:05}.m4s")))
                            })
                            .collect()
                    }
                    None => {
                        // The slate counts on from the stored anchor; one in the future (its programme was cut
                        // short or lost its asset while on air) is brought to now with the same numbers.
                        if let Some(a) = anchor
                            && a.end_ms > t
                        {
                            let pad = slate_window_ms(&slate);
                            anchor = Some(ChainAnchor {
                                end_ms: t - pad,
                                ..a
                            });
                        }
                        let tail = anchor.and_then(|a| {
                            let i = live.iter().position(|b| b.block.id == a.block_id)?;
                            (!gone(i, t)
                                && live[i].block.end_ms == a.end_ms
                                && t - a.end_ms < 120_000)
                                .then(|| live[i].clone())
                        });
                        let chain = anchor.map(|a| SlateChain::from_anchor(&a, tail.as_ref()));
                        let g = (t % cycle_ms) / seg_ms;
                        assert!(g < n_slate);
                        TRACKS
                            .iter()
                            .map(|track| {
                                let text =
                                    render_slate_media(&slate, *track, t, chain.as_ref(), &Sig, 0)
                                        .unwrap();
                                (text, (slate_id.to_string(), format!("seg_{g:05}.m4s")))
                            })
                            .collect()
                    }
                };
                let mut edges = Vec::new();
                for (ti, (track, (text, expect))) in TRACKS.iter().zip(rendered).enumerate() {
                    let (last_head, last_disc, last_edge) = last[ti];
                    let seen = &mut seen[ti];
                    let prev_text = last_text[ti].clone();
                    let es = entries(&text);
                    let ctx = format!("case {case} {track:?} t0+{} ms", t - t0);
                    // The window always holds the segment on air, last.
                    let tail = es.last().unwrap();
                    assert_eq!((tail.2.clone(), tail.3.clone()), expect, "{ctx}\n{text}");
                    // Neither counter nor the live edge ever decreases between two reloads.
                    let (head, disc, edge) = (es[0].0, es[0].1, es.last().unwrap().0);
                    assert!(
                        head >= last_head,
                        "{ctx}: media sequence {head} < {last_head}
{text}
PREVIOUS:
{prev_text}
ANCHOR {anchor:?}
BLOCKS {:?}
GONE {gone_at:?} UNVIEWED {unviewed:?}",
                        live.iter()
                            .map(|b| (
                                b.block.id,
                                b.block.start_ms - t0,
                                b.block.end_ms - t0,
                                b.block.seq_base,
                                b.block.disc_base
                            ))
                            .collect::<Vec<_>>()
                    );
                    assert!(
                        disc >= last_disc,
                        "{ctx}: discontinuity sequence {disc} < {last_disc}"
                    );
                    assert!(edge >= last_edge, "{ctx}: live edge {edge} < {last_edge}");
                    // A number is one segment with one counter, for every client, and the same on every track.
                    for (s, d, a, f) in es {
                        let prev = seen.entry(s).or_insert((d, a.clone(), f.clone()));
                        assert_eq!(
                            *prev,
                            (d, a.clone(), f.clone()),
                            "{ctx}: sequence {s} changed meaning\n{text}"
                        );
                        let other = across.entry(s).or_insert((d, a.clone(), f.clone()));
                        assert_eq!(
                            *other,
                            (d, a, f),
                            "{ctx}: sequence {s} names another segment on another track\n{text}"
                        );
                    }
                    last[ti] = (head, disc, edge);
                    last_text[ti] = text;
                    edges.push(edge);
                }
                // Audio and video agree on the number on air.
                let (lo, hi) = (edges.iter().min().unwrap(), edges.iter().max().unwrap());
                assert!(
                    hi - lo <= 1,
                    "case {case} t0+{} ms: live edges {edges:?} of {TRACKS:?}",
                    t - t0
                );
                t += rng.range(400, 3_500);
            }
        }
    }
}

/// The CI run of f1e89b77: a 12 s slate made by `sr-cli tv slate` has video segments of 6 + 6 s, and its AAC
/// track is cut 6 + 6 + 0.021 s. The audio loop used its own segment count and cycle, so with no programme
/// before it the audio playlist was numbered 447024435 while the video said 298811000.
#[test]
fn a_slate_whose_audio_ends_in_a_sliver_has_the_numbers_of_its_video() {
    let mut slate = asset_with(9999, 2, 6.0);
    with_audio_sliver(&mut slate);
    let t0 = 1_792_887_000_000_i64;
    for chain in [
        None,
        Some(SlateChain::after(&block(1, 0, t0 - 30_000, 70, 3, 1000)).unwrap()),
    ] {
        let mut t = t0;
        while t < t0 + 120_000 {
            let v =
                render_slate_media(&slate, Track::Video(720), t, chain.as_ref(), &Sig, 0).unwrap();
            let a = render_slate_media(&slate, Track::Audio, t, chain.as_ref(), &Sig, 0).unwrap();
            let (ev, ea) = (entries(&v), entries(&a));
            assert_eq!(live_edge(&v), live_edge(&a), "at {t}\n{v}\n{a}");
            // Same numbers, same counters, same segment files; the sliver is never listed.
            let key = |e: &Vec<Seg>| {
                e.iter()
                    .map(|s| (s.0, s.1, s.3.clone()))
                    .collect::<Vec<_>>()
            };
            assert_eq!(key(&ev), key(&ea), "at {t}\n{v}\n{a}");
            assert!(!a.contains("seg_00002.m4s"), "{a}");
            t += 700;
        }
    }
}

/// A programme whose asset has the same sliver on its audio, aired to its last millisecond and followed by
/// the slate: the audio never lists a segment the chain did not count, so the slate's first number is not
/// given twice.
#[test]
fn a_programme_whose_audio_ends_in_a_sliver_hands_the_slate_the_same_numbers_on_every_track() {
    let mut b = block(1, 0, 12_021, 500, 2, 2);
    with_audio_sliver(&mut b.asset);
    let slate = asset_with(9999, 2, 6.0);
    let chain = SlateChain::after(&b).unwrap();
    let blocks = [b.clone()];
    let mut across: HashMap<i64, (i64, String, String)> = HashMap::new();
    let mut t = 0;
    while t < 60_000 {
        for track in [Track::Video(720), Track::Audio] {
            let text = if t < 12_021 {
                render_media(&blocks, track, t, &Sig, 0).unwrap()
            } else {
                render_slate_media(&slate, track, t, Some(&chain), &Sig, 0).unwrap()
            };
            for (s, d, a, f) in entries(&text) {
                let prev = across.entry(s).or_insert((d, a.clone(), f.clone()));
                assert_eq!(
                    *prev,
                    (d, a, f),
                    "{track:?} at {t}: {s} changed meaning\n{text}"
                );
            }
        }
        t += 7;
    }
}
