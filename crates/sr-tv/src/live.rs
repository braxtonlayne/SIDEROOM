//! Virtual-linear HLS (TV-HLS-001, 002): live-style playlists computed from the wall clock out of the
//! stored VOD renditions. No process runs per channel; every request is one cheap computation.
//!
//! How a window is cut. A block puts its asset on the air so that wall time `block.start_ms` shows asset
//! position `block.offset_ms`; segment `k` of the asset (starting `p_k` into it) therefore starts on the wall
//! clock at `block.start_ms - block.offset_ms + p_k`. The playlist at time `now` lists the last
//! [`WINDOW_SEGMENTS`] segments whose wall start is not after `now`, the newest being the one on air. A
//! viewer joining 17 minutes into a programme gets a window around that position and everyone gets the
//! same window (so two clients are within a segment of each other).
//!
//! Numbering. Players follow a live playlist by segment sequence number, so a segment must keep its number
//! from one reload to the next, whatever else is in the window. Each block therefore stores `seq_base` (the
//! number of its first on-air segment) and `disc_base` (its discontinuity counter), chained from the block
//! before it (`seq_base + segments on air`, `disc_base + 1`) and seeded from the wall clock in seconds when
//! there is no contiguous predecessor (seconds only grow, and a segment lasts at least a second, so numbers
//! never go backwards). Segment `k` of a block is `seq_base + (k - first on-air k)`.
//!
//! Programme boundaries carry `#EXT-X-DISCONTINUITY`, a new `#EXT-X-MAP` and a `#EXT-X-PROGRAM-DATE-TIME`.
//! The window never runs past the end of a programme into a slate: when nothing with media is on air the
//! caller answers off air.

use sr_media::playback::PlaybackAsset;
use sr_media::playlist::{
    AudioVariant, MediaSet, PlaylistConfig, Segment, UrlSigner, VideoVariant, render_master,
};

use crate::model::BlockRecord;
use crate::tz::{MS_DAY, civil_from_days};

/// Segments in a media playlist (spec section 5: "e.g. 6").
pub const WINDOW_SEGMENTS: usize = 6;
/// Signed URLs in a playlist stay valid this long (as the VOD playlists).
pub const URL_TTL_SECS: u64 = 6 * 3600;

/// A block with the asset it puts on the air.
#[derive(Debug, Clone)]
pub struct LiveBlock {
    pub block: BlockRecord,
    pub asset: PlaybackAsset,
}

/// Which playlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Track {
    /// The rung closest to this height.
    Video(u32),
    /// The default audio rendition.
    Audio,
}

/// The rung of `asset` closest to `height` (the lower one on a tie).
pub fn pick_rung(asset: &PlaybackAsset, height: u32) -> Option<&VideoVariant> {
    asset
        .media
        .videos
        .iter()
        .min_by_key(|v| (v.height.abs_diff(height), v.height))
}

/// The default audio rendition, else the first.
pub fn default_audio(asset: &PlaybackAsset) -> Option<&AudioVariant> {
    asset
        .media
        .audios
        .iter()
        .find(|a| a.is_default)
        .or_else(|| asset.media.audios.first())
}

fn track_of(asset: &PlaybackAsset, track: Track) -> Option<(&str, &[Segment])> {
    match track {
        Track::Video(h) => {
            pick_rung(asset, h).map(|v| (v.init_path.as_str(), v.segments.as_slice()))
        }
        Track::Audio => default_audio(asset).map(|a| (a.init_path.as_str(), a.segments.as_slice())),
    }
}

/// One on-air segment of a block: its index, wall-clock start and end (ms) and duration.
#[derive(Debug, Clone, PartialEq)]
struct OnAir {
    k: usize,
    wall_start: i64,
    wall_end: i64,
    dur: f64,
}

fn on_air(block: &BlockRecord, segs: &[Segment]) -> Vec<OnAir> {
    let mut pos = 0.0_f64;
    let mut out = Vec::new();
    for (k, s) in segs.iter().enumerate() {
        let a = (pos * 1000.0).round() as i64;
        pos += s.duration_secs;
        let b = (pos * 1000.0).round() as i64;
        let wall_start = block.start_ms - block.offset_ms + a;
        let wall_end = block.start_ms - block.offset_ms + b;
        if wall_end > block.start_ms && wall_start < block.end_ms {
            out.push(OnAir {
                k,
                wall_start,
                wall_end,
                dur: s.duration_secs,
            });
        }
    }
    out
}

/// How many segments of the asset a block puts on the air. The numbering chain counts the lowest video
/// rung (all rungs of an asset are cut at the same places).
pub fn on_air_count(block: &BlockRecord, asset: &PlaybackAsset) -> i64 {
    asset
        .media
        .videos
        .iter()
        .min_by_key(|v| v.height)
        .map_or(0, |v| on_air(block, &v.segments).len() as i64)
}

/// `YYYY-MM-DDTHH:MM:SS.mmmZ`.
pub fn iso(ms: i64) -> String {
    let (y, m, d) = civil_from_days(ms.div_euclid(MS_DAY));
    let rem = ms.rem_euclid(MS_DAY);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3_600_000,
        rem / 60_000 % 60,
        rem / 1000 % 60,
        rem % 1000
    )
}

fn signed(signer: &dyn UrlSigner, asset: i64, rel: &str, expires: u64) -> String {
    signer.signed_url(&format!("/cdn/hls/{asset}/{rel}"), expires)
}

/// The media playlist of a track at `now_ms`, or `None` when no block in `blocks` is on air with that
/// track. `blocks` are one channel's blocks around `now_ms`, ordered by start, each with `seq_base` and
/// `disc_base` assigned; only blocks directly joined to the one on air are used before it.
pub fn render_media(
    blocks: &[LiveBlock],
    track: Track,
    now_ms: i64,
    signer: &dyn UrlSigner,
    now_secs: u64,
) -> Option<String> {
    let cur = blocks.iter().position(|b| b.block.contains(now_ms))?;
    // Walk back over blocks that touch each other.
    let mut first = cur;
    while first > 0
        && blocks[first - 1].block.end_ms == blocks[first].block.start_ms
        && cur - first < 1
    {
        first -= 1;
    }
    struct Entry<'a> {
        block: usize,
        seg: OnAir,
        path: &'a str,
        first_k: usize,
    }
    let mut entries: Vec<Entry<'_>> = Vec::new();
    for (bi, lb) in blocks.iter().enumerate().take(cur + 1).skip(first) {
        let Some((_, segs)) = track_of(&lb.asset, track) else {
            if bi == cur {
                return None;
            }
            continue;
        };
        let air = on_air(&lb.block, segs);
        let first_k = air.first().map_or(0, |a| a.k);
        for a in air {
            if bi == cur && a.wall_start > now_ms {
                break;
            }
            let path = segs[a.k].path.as_str();
            entries.push(Entry {
                block: bi,
                seg: a,
                path,
                first_k,
            });
        }
    }
    if entries.is_empty() || entries.last().is_none_or(|e| e.block != cur) {
        return None;
    }
    let start = entries.len().saturating_sub(WINDOW_SEGMENTS);
    let window = &entries[start..];
    let head = &window[0];
    let head_block = &blocks[head.block].block;
    let media_seq = head_block.seq_base? + (head.seg.k - head.first_k) as i64;
    let disc_seq = head_block.disc_base?;
    let target = window
        .iter()
        .map(|e| e.seg.dur)
        .fold(1.0_f64, f64::max)
        .ceil() as u64;
    let expires = now_secs + URL_TTL_SECS;
    let mut out = format!(
        "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:{target}\n#EXT-X-MEDIA-SEQUENCE:{media_seq}\n\
         #EXT-X-DISCONTINUITY-SEQUENCE:{disc_seq}\n#EXT-X-INDEPENDENT-SEGMENTS\n"
    );
    let mut group: Option<usize> = None;
    for e in window {
        if group != Some(e.block) {
            let lb = &blocks[e.block];
            if group.is_some() {
                out.push_str("#EXT-X-DISCONTINUITY\n");
            }
            if let Some((init, _)) = track_of(&lb.asset, track) {
                out.push_str(&format!(
                    "#EXT-X-MAP:URI=\"{}\"\n",
                    signed(signer, lb.asset.id, init, expires)
                ));
            }
            out.push_str(&format!(
                "#EXT-X-PROGRAM-DATE-TIME:{}\n",
                iso(e.seg.wall_start)
            ));
            group = Some(e.block);
        }
        out.push_str(&format!(
            "#EXTINF:{:.6},\n{}\n",
            e.seg.dur,
            signed(signer, blocks[e.block].asset.id, e.path, expires)
        ));
    }
    Some(out)
}

/// Turn "relative path" into itself: the live master points at sibling endpoints, not at the CDN.
struct Relative;

impl UrlSigner for Relative {
    fn signed_url(&self, path: &str, _expires_at: u64) -> String {
        path.trim_start_matches('/').to_owned()
    }
}

/// The heights every asset in `assets` has.
pub fn common_heights(assets: &[&PlaybackAsset]) -> Vec<u32> {
    let mut heights: Vec<u32> = assets
        .first()
        .map(|a| a.media.videos.iter().map(|v| v.height).collect())
        .unwrap_or_default();
    for a in assets.iter().skip(1) {
        heights.retain(|h| a.media.videos.iter().any(|v| v.height == *h));
    }
    heights.sort_unstable();
    heights.dedup();
    heights
}

/// The master playlist: one variant per height that every upcoming programme has (`assets`, the one on air
/// first); when there is no common height the heights of the one on air. Variants point at
/// `v/{height}.m3u8` and the audio at `audio.m3u8`, relative to the master.
pub fn render_live_master(assets: &[&PlaybackAsset], now_secs: u64) -> Option<String> {
    let current = assets.first()?;
    let mut heights = common_heights(assets);
    if heights.is_empty() {
        heights = common_heights(&[current]);
    }
    let videos = heights
        .iter()
        .filter_map(|h| {
            let mut v = current
                .media
                .videos
                .iter()
                .find(|v| v.height == *h)?
                .clone();
            v.playlist_path = format!("v/{h}.m3u8");
            v.segments.clear();
            Some(v)
        })
        .collect::<Vec<_>>();
    if videos.is_empty() {
        return None;
    }
    let audios = default_audio(current)
        .map(|a| {
            let mut a = a.clone();
            a.name = "Main".into();
            a.is_default = true;
            a.playlist_path = "audio.m3u8".into();
            a.segments.clear();
            vec![a]
        })
        .unwrap_or_default();
    let set = MediaSet {
        videos,
        audios,
        subtitles: Vec::new(),
    };
    Some(render_master(
        &set,
        &PlaylistConfig::new(""),
        &Relative,
        now_secs,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BlockKind, BlockSource};
    use sr_media::playback::sample_asset;

    struct Sig;
    impl UrlSigner for Sig {
        fn signed_url(&self, path: &str, expires_at: u64) -> String {
            format!("{path}?ex={expires_at}&sig=S")
        }
    }

    /// An asset with `n` six-second segments per track.
    fn asset(id: i64, n: usize) -> PlaybackAsset {
        let mut a = sample_asset(id);
        for v in &mut a.media.videos {
            let dir = format!("v/{}", v.name);
            v.segments = (0..n)
                .map(|k| Segment {
                    path: format!("{dir}/seg_{k:05}.m4s"),
                    duration_secs: 6.0,
                })
                .collect();
        }
        for au in &mut a.media.audios {
            let dir = au.playlist_path.rsplit_once('/').unwrap().0.to_owned();
            au.segments = (0..n)
                .map(|k| Segment {
                    path: format!("{dir}/seg_{k:05}.m4s"),
                    duration_secs: 6.0,
                })
                .collect();
        }
        a.duration_ms = Some(n as i64 * 6000);
        a
    }

    fn block(id: i64, start: i64, end: i64, offset: i64, seq: i64, disc: i64) -> BlockRecord {
        BlockRecord {
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
            offset_ms: offset,
            source: BlockSource::Generated,
            cursor_after: None,
            seq_base: Some(seq),
            disc_base: Some(disc),
        }
    }

    fn lb(id: i64, start: i64, end: i64, offset: i64, seq: i64, disc: i64, n: usize) -> LiveBlock {
        LiveBlock {
            block: block(id, start, end, offset, seq, disc),
            asset: asset(id * 100, n),
        }
    }

    fn lines(s: &str) -> Vec<&str> {
        s.lines().collect()
    }

    fn seqs(text: &str) -> (i64, Vec<String>) {
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

    #[test]
    fn joining_17_minutes_in_starts_17_minutes_in() {
        // A 100-segment (10 minute) asset would not reach; use 200 segments (20 minutes), block from t=0.
        let b = lb(1, 0, 1_200_000, 0, 1000, 7, 200);
        // 17 minutes = 1020 s = segment 170 starts exactly there.
        let t = 17 * 60_000;
        let text = render_media(&[b], Track::Video(720), t, &Sig, t as u64 / 1000).unwrap();
        let (seq, segs) = seqs(&text);
        assert_eq!(segs.len(), WINDOW_SEGMENTS);
        assert_eq!(segs.last().unwrap(), "/cdn/hls/100/v/720p/seg_00170.m4s");
        assert_eq!(segs[0], "/cdn/hls/100/v/720p/seg_00165.m4s");
        assert_eq!(seq, 1000 + 165);
        assert!(text.contains("#EXT-X-DISCONTINUITY-SEQUENCE:7"));
        assert!(text.contains("#EXT-X-TARGETDURATION:6"));
        assert!(!text.contains("ENDLIST"), "a live playlist never ends");
        assert!(text.contains("#EXT-X-PROGRAM-DATE-TIME:1970-01-01T00:16:30.000Z"));
        assert!(text.contains("#EXT-X-MAP:URI=\"/cdn/hls/100/v/720p/init.mp4?ex="));
    }

    #[test]
    fn the_newest_segment_is_the_one_on_air_and_the_window_slides_with_a_stable_sequence() {
        let b = lb(1, 0, 1_200_000, 0, 1000, 7, 200);
        let at = |t: i64| {
            let text =
                render_media(std::slice::from_ref(&b), Track::Video(720), t, &Sig, 0).unwrap();
            seqs(&text)
        };
        // 1000 s in: segment 166 (996 s to 1002 s) is on air.
        let (s1, a) = at(1_000_000);
        assert_eq!(a.last().unwrap(), "/cdn/hls/100/v/720p/seg_00166.m4s");
        assert_eq!(s1, 1000 + 161);
        // Six seconds later the window moved by exactly one: the same segment keeps its number.
        let (s2, c) = at(1_006_000);
        assert_eq!(s2, s1 + 1);
        assert_eq!(c[..5], a[1..]);
        // Two clients within the same segment get the same playlist.
        assert_eq!(at(1_000_000), at(1_001_999));
    }

    #[test]
    fn an_offset_start_and_the_first_segments_of_a_block() {
        // The block starts 40 s into the asset: the segment holding 40 s (index 6, 36 s to 42 s) is first.
        let b = lb(1, 600_000, 1_200_000, 40_000, 50, 3, 200);
        let text = render_media(&[b], Track::Video(720), 600_000, &Sig, 0).unwrap();
        let (seq, segs) = seqs(&text);
        assert_eq!(segs, ["/cdn/hls/100/v/720p/seg_00006.m4s"]);
        assert_eq!(seq, 50, "the first on-air segment carries seq_base");
        // Wall start 4 s before the block begins.
        assert!(
            text.contains("PROGRAM-DATE-TIME:1970-01-01T00:09:56.000Z"),
            "{text}"
        );
    }

    #[test]
    fn a_programme_boundary_carries_a_discontinuity_a_new_map_and_a_continuous_sequence() {
        // Block 1: 60 s (10 segments), seq 100..109, then block 2 chained: seq_base 110, disc 5.
        let b1 = lb(1, 0, 60_000, 0, 100, 4, 10);
        let b2 = lb(2, 60_000, 180_000, 0, 110, 5, 20);
        assert_eq!(on_air_count(&b1.block, &b1.asset), 10);
        // 14 s into block 2: segments 0, 1, 2 are on air (0 s, 6 s, 12 s); 3 from block 1's tail.
        let now = 74_000;
        let text =
            render_media(&[b1.clone(), b2.clone()], Track::Video(720), now, &Sig, 0).unwrap();
        let (seq, segs) = seqs(&text);
        assert_eq!(segs.len(), 6);
        assert_eq!(segs[0], "/cdn/hls/100/v/720p/seg_00007.m4s");
        assert_eq!(segs[2], "/cdn/hls/100/v/720p/seg_00009.m4s");
        assert_eq!(segs[3], "/cdn/hls/200/v/720p/seg_00000.m4s");
        assert_eq!(seq, 107);
        assert!(text.contains("#EXT-X-DISCONTINUITY-SEQUENCE:4"));
        let l = lines(&text);
        let d = l.iter().position(|l| *l == "#EXT-X-DISCONTINUITY").unwrap();
        assert!(l[d + 1].starts_with("#EXT-X-MAP:URI=\"/cdn/hls/200/v/720p/init.mp4"));
        assert!(l[d + 2].starts_with("#EXT-X-PROGRAM-DATE-TIME:1970-01-01T00:01:00.000Z"));
        // The next reload (6 s later) keeps every number: one new segment, window one further.
        let later = render_media(
            &[b1.clone(), b2.clone()],
            Track::Video(720),
            now + 6_000,
            &Sig,
            0,
        )
        .unwrap();
        assert_eq!(seqs(&later).0, 108);
        // Once block 1 has left the window the numbers carry on from seq_base 110 and the sequence of
        // discontinuities is 5 (one boundary has gone by).
        let far = render_media(&[b1, b2], Track::Video(720), 95_000, &Sig, 0).unwrap();
        let (fseq, fsegs) = seqs(&far);
        assert_eq!(fsegs[0], "/cdn/hls/200/v/720p/seg_00000.m4s");
        assert_eq!(fseq, 110);
        assert!(far.contains("#EXT-X-DISCONTINUITY-SEQUENCE:5"));
        assert!(!far.contains("#EXT-X-DISCONTINUITY\n"));
    }

    #[test]
    fn a_cut_block_stops_at_its_end_and_a_gap_does_not_join() {
        // The asset has 20 segments (120 s) but the block is cut at 30 s: only 5 segments are on air.
        let cut = lb(1, 0, 30_000, 0, 10, 1, 20);
        assert_eq!(on_air_count(&cut.block, &cut.asset), 5);
        let text = render_media(
            std::slice::from_ref(&cut),
            Track::Video(720),
            29_000,
            &Sig,
            0,
        )
        .unwrap();
        assert_eq!(seqs(&text).1.len(), 5);
        // Nothing is on air after the block.
        assert!(
            render_media(
                std::slice::from_ref(&cut),
                Track::Video(720),
                31_000,
                &Sig,
                0
            )
            .is_none()
        );
        // A programme after a gap does not pull the earlier one into its window.
        let after = lb(2, 40_000, 100_000, 0, 77, 2, 10);
        let text = render_media(&[cut, after], Track::Video(720), 52_000, &Sig, 0).unwrap();
        let (seq, segs) = seqs(&text);
        assert_eq!(seq, 77);
        assert!(segs.iter().all(|s| s.contains("/cdn/hls/200/")));
        assert!(!text.contains("#EXT-X-DISCONTINUITY\n"));
    }

    #[test]
    fn audio_uses_the_default_track_and_nearest_rung_serves_missing_heights() {
        let b = lb(1, 0, 120_000, 0, 1, 1, 20);
        let text = render_media(std::slice::from_ref(&b), Track::Audio, 30_000, &Sig, 0).unwrap();
        let (_, segs) = seqs(&text);
        assert!(
            segs[0].starts_with("/cdn/hls/100/a/00-en-stereo/seg_"),
            "{segs:?}"
        );
        assert!(text.contains("#EXT-X-MAP:URI=\"/cdn/hls/100/a/00-en-stereo/init.mp4"));
        // 1000 is closest to 1080; 600 to 720.
        let a = &b.asset;
        assert_eq!(pick_rung(a, 1000).unwrap().height, 1080);
        assert_eq!(pick_rung(a, 600).unwrap().height, 720);
        assert_eq!(
            pick_rung(a, 900).unwrap().height,
            720,
            "ties go to the lower rung"
        );
    }

    #[test]
    fn the_master_lists_the_common_heights_relative_to_itself() {
        let mut a = asset(100, 3);
        let mut b = asset(200, 3);
        let full = render_live_master(&[&a, &b], 0).unwrap();
        assert!(
            full.contains("\nv/720.m3u8\n") && full.contains("\nv/1080.m3u8\n"),
            "{full}"
        );
        assert!(full.contains("URI=\"audio.m3u8\""));
        assert!(full.contains("AUDIO=\"aud\"") && !full.contains("SUBTITLES"));
        // The next programme lacks 1080: only 720 is offered.
        b.media.videos.retain(|v| v.height == 720);
        let common = render_live_master(&[&a, &b], 0).unwrap();
        assert!(common.contains("v/720.m3u8") && !common.contains("v/1080.m3u8"));
        // No common height at all: fall back to the one on air.
        a.media.videos.retain(|v| v.height == 1080);
        let fallback = render_live_master(&[&a, &b], 0).unwrap();
        assert!(fallback.contains("v/1080.m3u8") && !fallback.contains("v/720.m3u8"));
        // No audio rendition: no audio group.
        a.media.audios.clear();
        let silent = render_live_master(&[&a], 0).unwrap();
        assert!(!silent.contains("TYPE=AUDIO") && !silent.contains("AUDIO=\"aud\""));
    }

    #[test]
    fn iso_formats_milliseconds() {
        assert_eq!(iso(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso(1_700_000_123_456), "2023-11-14T22:15:23.456Z");
    }
}
