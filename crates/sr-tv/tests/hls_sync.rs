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
};
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
    let slate_id = 9999;
    for case in 0..80_u64 {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ (case + 1).wrapping_mul(0xD1B5_4A32_D192_ED03));
        for _ in 0..4 {
            rng.next();
        }
        // The slate: 2..=5 segments of 1, 2, 2.5 or 4 s, whatever the programmes use.
        let slate_secs = [1.0, 2.0, 2.5, 4.0][rng.range(0, 3) as usize];
        let slate = asset_with(slate_id, rng.range(2, 5) as usize, slate_secs);
        let seg_ms = (slate_secs * 1000.0) as i64;
        let n_slate = slate.media.videos[0].segments.len() as i64;
        let cycle_ms = seg_ms * n_slate;
        // Programmes with gaps (some none: contiguous), cut and offset at random, from a start that is
        // not aligned to anything.
        let t0 = 1_700_000_000_000 + rng.range(0, 999_999);
        let mut cursor = t0;
        let mut live: Vec<LiveBlock> = Vec::new();
        for id in 1..=rng.range(3, 6) {
            cursor += [0, 0, rng.range(1, 90_000)][rng.range(0, 2) as usize];
            let seg = [2.0, 4.0, 6.0][rng.range(0, 2) as usize];
            let len = rng.range(20, 240) * 1000;
            let offset = rng.range(0, 40) * 1000;
            let n = ((len + offset) as f64 / (seg * 1000.0)).ceil() as usize + 2;
            let mut b = block(id, cursor, cursor + len, 0, 0, n);
            b.asset = asset_with(id * 100, n, seg);
            b.block.offset_ms = offset;
            b.block.seq_base = None;
            b.block.disc_base = None;
            live.push(b);
            cursor += len;
        }
        // Bases as the service assigns them: from the contiguous programme before, else from the slate
        // segment on air at the start (itself counting on from the last programme before the gap).
        for i in 0..live.len() {
            let (seq, disc) = match i.checked_sub(1).map(|p| &live[p]) {
                Some(p) if p.block.end_ms == live[i].block.start_ms => (
                    p.block.seq_base.unwrap() + on_air_count(&p.block, &p.asset),
                    p.block.disc_base.unwrap() + 1,
                ),
                prev => {
                    let chain = prev.and_then(SlateChain::after);
                    bases_after_slate(&slate, chain.as_ref(), live[i].block.start_ms).unwrap()
                }
            };
            live[i].block.seq_base = Some(seq);
            live[i].block.disc_base = Some(disc);
        }
        let t_end = cursor + 60_000;
        for track in [Track::Video(720), Track::Video(480), Track::Audio] {
            // Every number maps to one segment and one discontinuity counter, whoever asks and when.
            let mut seen: HashMap<i64, (i64, String, String)> = HashMap::new();
            let (mut last_head, mut last_disc, mut last_edge) = (i64::MIN, i64::MIN, i64::MIN);
            let mut t = t0 - 5_000;
            while t < t_end {
                let cur = live.iter().position(|b| b.block.contains(t));
                let (text, expect): (String, (String, String)) = match cur {
                    Some(c) => {
                        let mut blocks = Vec::new();
                        if c > 0 && live[c - 1].block.end_ms == live[c].block.start_ms {
                            blocks.push(live[c - 1].clone());
                        }
                        blocks.push(live[c].clone());
                        let last = c
                            .checked_sub(1)
                            .map(|p| &live[p])
                            .filter(|p| p.block.end_ms <= live[c].block.start_ms);
                        let lead = SlateLead {
                            asset: &slate,
                            chain: last.and_then(SlateChain::after),
                        };
                        let text = render_media_with_lead(&blocks, track, t, Some(&lead), &Sig, 0)
                            .unwrap();
                        let b = &live[c];
                        let bseg =
                            (b.asset.media.videos[0].segments[0].duration_secs * 1000.0) as i64;
                        let k = (t - b.block.start_ms + b.block.offset_ms) / bseg;
                        (text, (b.asset.id.to_string(), format!("seg_{k:05}.m4s")))
                    }
                    None => {
                        let last = live.iter().rfind(|b| b.block.end_ms <= t);
                        let chain = last.and_then(SlateChain::after);
                        let text =
                            render_slate_media(&slate, track, t, chain.as_ref(), &Sig, 0).unwrap();
                        let g = (t % cycle_ms) / seg_ms;
                        assert!(g < n_slate);
                        (text, (slate_id.to_string(), format!("seg_{g:05}.m4s")))
                    }
                };
                let es = entries(&text);
                let ctx = format!("case {case} {track:?} t0+{} ms", t - t0);
                // The window always holds the segment on air, last.
                let tail = es.last().unwrap();
                assert_eq!((tail.2.clone(), tail.3.clone()), expect, "{ctx}\n{text}");
                // Neither counter nor the live edge ever decreases between two reloads.
                let (head, disc, edge) = (es[0].0, es[0].1, es.last().unwrap().0);
                assert!(
                    head >= last_head,
                    "{ctx}: media sequence {head} < {last_head}\n{text}"
                );
                assert!(
                    disc >= last_disc,
                    "{ctx}: discontinuity sequence {disc} < {last_disc}"
                );
                assert!(edge >= last_edge, "{ctx}: live edge {edge} < {last_edge}");
                // A number is one segment with one counter, for every client.
                for (s, d, a, f) in es {
                    let prev = seen.entry(s).or_insert((d, a.clone(), f.clone()));
                    assert_eq!(
                        *prev,
                        (d, a, f),
                        "{ctx}: sequence {s} changed meaning\n{text}"
                    );
                }
                (last_head, last_disc, last_edge) = (head, disc, edge);
                t += rng.range(400, 3_500);
            }
        }
    }
}
