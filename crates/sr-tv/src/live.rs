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
//! Numbering (one domain, D-267). Players follow a live playlist by segment sequence number, so a segment
//! must keep its number from one reload to the next, whatever else is in the window. Every segment the
//! channel ever airs, programme or slate, is numbered in one chain:
//!
//! - A block stores `seq_base` (the number of its first on-air segment) and `disc_base` (its discontinuity
//!   counter). Chained from the block right before it (`seq_base + segments on air`, `disc_base + 1`).
//! - With no contiguous predecessor and a slate configured, a block chains from the slate segment on air at
//!   its start (that segment's number + 1, discontinuity + 1). The slate, in turn, counts on from the last
//!   programme with media that ended before it ([`SlateChain`]): the slate segment on air at that end is
//!   last + 1 with discontinuity sequence + 1, each later segment adds one, each wrap of the loop one
//!   discontinuity.
//! - Only where there is nothing to chain from (no slate, or no earlier programme on the channel) the number
//!   is seeded from the wall clock in seconds: seconds only grow and a segment lasts at least a second, so
//!   a chained number never exceeds a later seed and numbers never go backwards.
//!
//! Programme boundaries, on either side of a slate too, carry `#EXT-X-DISCONTINUITY`, a new `#EXT-X-MAP` and
//! a `#EXT-X-PROGRAM-DATE-TIME`; the window lists both sides of a boundary while it is young
//! ([`render_media_with_lead`], [`render_slate_media`]). A wrap of the slate loop carries the same tags.
//! The window never runs past the end of a programme into nothing: when no programme and no slate is on air
//! the caller answers off air.

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

/// One segment of a window, whichever side of a programme/slate boundary it is on.
struct Entry<'a> {
    /// What the segment belongs to: a programme block (`false`, block id) or one pass of the slate loop
    /// (`true`, loop number). A change of group is a discontinuity.
    group: (bool, i64),
    asset_id: i64,
    init: &'a str,
    path: &'a str,
    dur: f64,
    /// Wall-clock start (ms) for `#EXT-X-PROGRAM-DATE-TIME` at the head of a group.
    pdt: i64,
    seq: i64,
    disc: i64,
}

/// The programme-side entries of `lb` that are on air no later than `upto_ms` (all of them when `None`).
/// `None` when the block's asset has no such track or its numbering bases are not assigned.
fn block_entries(lb: &LiveBlock, track: Track, upto_ms: Option<i64>) -> Option<Vec<Entry<'_>>> {
    let (init, segs) = track_of(&lb.asset, track)?;
    let air = on_air(&lb.block, segs);
    let first_k = air.first().map_or(0, |a| a.k);
    let (seq0, disc) = (lb.block.seq_base?, lb.block.disc_base?);
    Some(
        air.into_iter()
            .take_while(|a| upto_ms.is_none_or(|t| a.wall_start <= t))
            .map(|a| Entry {
                group: (false, lb.block.id),
                asset_id: lb.asset.id,
                init,
                path: segs[a.k].path.as_str(),
                dur: a.dur,
                pdt: a.wall_start,
                seq: seq0 + (a.k - first_k) as i64,
                disc,
            })
            .collect(),
    )
}

/// The playlist text of a window of entries.
fn emit(window: &[Entry<'_>], signer: &dyn UrlSigner, now_secs: u64) -> String {
    let head = &window[0];
    let target = window.iter().map(|e| e.dur).fold(1.0_f64, f64::max).ceil() as u64;
    let expires = now_secs + URL_TTL_SECS;
    let mut out = format!(
        "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:{target}\n#EXT-X-MEDIA-SEQUENCE:{}\n\
         #EXT-X-DISCONTINUITY-SEQUENCE:{}\n#EXT-X-INDEPENDENT-SEGMENTS\n",
        head.seq, head.disc
    );
    let mut group: Option<(bool, i64)> = None;
    for e in window {
        if group != Some(e.group) {
            if group.is_some() {
                out.push_str("#EXT-X-DISCONTINUITY\n");
            }
            out.push_str(&format!(
                "#EXT-X-MAP:URI=\"{}\"\n#EXT-X-PROGRAM-DATE-TIME:{}\n",
                signed(signer, e.asset_id, e.init, expires),
                iso(e.pdt)
            ));
            group = Some(e.group);
        }
        out.push_str(&format!(
            "#EXTINF:{:.6},\n{}\n",
            e.dur,
            signed(signer, e.asset_id, e.path, expires)
        ));
    }
    out
}

/// The slate loop of an asset on one track, laid on the wall clock (anchored at the unix epoch, so every
/// client sees the same loop). Segment `k` of loop `c` starts at `c * cycle_ms + starts[k]`; the global
/// index of that segment is `c * n + k`.
struct SlateLoop<'a> {
    asset_id: i64,
    init: &'a str,
    segs: &'a [Segment],
    starts: Vec<i64>,
    cycle_ms: i64,
}

impl<'a> SlateLoop<'a> {
    fn of(asset_id: i64, init: &'a str, segs: &'a [Segment]) -> Option<Self> {
        if segs.is_empty() {
            return None;
        }
        let mut starts = Vec::with_capacity(segs.len());
        let mut acc = 0.0_f64;
        for s in segs {
            starts.push((acc * 1000.0).round() as i64);
            acc += s.duration_secs;
        }
        let cycle_ms = (acc * 1000.0).round() as i64;
        (cycle_ms > 0).then_some(Self {
            asset_id,
            init,
            segs,
            starts,
            cycle_ms,
        })
    }

    fn new(asset: &'a PlaybackAsset, track: Track) -> Option<Self> {
        let (init, segs) = track_of(asset, track)?;
        Self::of(asset.id, init, segs)
    }

    /// The loop of the lowest video rung, which the numbering chain counts (as `on_air_count` does).
    fn lowest(asset: &'a PlaybackAsset) -> Option<Self> {
        let v = asset.media.videos.iter().min_by_key(|v| v.height)?;
        Self::of(asset.id, &v.init_path, &v.segments)
    }

    fn n(&self) -> i64 {
        self.segs.len() as i64
    }

    /// The global index of the segment on air at `t_ms`.
    fn index_at(&self, t_ms: i64) -> i64 {
        let t = t_ms.max(0);
        let (cycle, pos) = (t / self.cycle_ms, t % self.cycle_ms);
        let k = self.starts.partition_point(|s| *s <= pos) as i64 - 1;
        cycle * self.n() + k
    }

    fn start_of(&self, g: i64) -> i64 {
        g.div_euclid(self.n()) * self.cycle_ms + self.starts[g.rem_euclid(self.n()) as usize]
    }

    /// (media sequence, discontinuity sequence) of segment `g`: counted on from the programme before the
    /// gap when there is one (D-267), else seeded from the wall clock in seconds (as a programme with no
    /// predecessor is, so numbers never go backwards: a segment lasts at least a second).
    fn numbering(&self, chain: Option<&SlateChain<'_>>, g: i64) -> (i64, i64) {
        let cycle = g.div_euclid(self.n());
        match chain {
            Some(c) => {
                let g0 = self.index_at(c.end_ms);
                (c.seq + (g - g0), c.disc + (cycle - g0.div_euclid(self.n())))
            }
            None => (self.start_of(g) / 1000, cycle * self.cycle_ms / 1000),
        }
    }

    fn entry(&self, g: i64, chain: Option<&SlateChain<'_>>) -> Entry<'a> {
        let (seq, disc) = self.numbering(chain, g);
        let k = g.rem_euclid(self.n()) as usize;
        Entry {
            group: (true, g.div_euclid(self.n())),
            asset_id: self.asset_id,
            init: self.init,
            path: self.segs[k].path.as_str(),
            dur: self.segs[k].duration_secs,
            pdt: self.start_of(g),
            seq,
            disc,
        }
    }
}

/// Where the slate's numbering starts (D-267): the last programme with media that ended before the gap.
/// The slate segment on air at that programme's end is numbered one above its last segment, with the
/// discontinuity sequence one above its own; every later slate segment counts on from there.
#[derive(Debug, Clone, Copy)]
pub struct SlateChain<'a> {
    /// When the programme ended (ms).
    end_ms: i64,
    seq: i64,
    disc: i64,
    /// The programme, so a window just after the boundary can list its tail.
    prev: &'a LiveBlock,
}

impl<'a> SlateChain<'a> {
    /// The chain after `last`, or `None` when its numbering bases were never assigned.
    pub fn after(last: &'a LiveBlock) -> Option<Self> {
        Some(Self {
            end_ms: last.block.end_ms,
            seq: last.block.seq_base? + on_air_count(&last.block, &last.asset),
            disc: last.block.disc_base? + 1,
            prev: last,
        })
    }
}

/// What precedes a programme that has no programme right before it: the slate (`asset`) and the chain it
/// has been counting on from.
#[derive(Debug, Clone, Copy)]
pub struct SlateLead<'a> {
    pub asset: &'a PlaybackAsset,
    pub chain: Option<SlateChain<'a>>,
}

/// (media sequence, discontinuity sequence) of the slate segment on air at `t_ms`, `None` when the asset
/// has no video segments. The numbering is the same on every track.
pub fn slate_position(
    asset: &PlaybackAsset,
    chain: Option<&SlateChain<'_>>,
    t_ms: i64,
) -> Option<(i64, i64)> {
    let sl = SlateLoop::lowest(asset)?;
    Some(sl.numbering(chain, sl.index_at(t_ms)))
}

/// The numbering bases (`seq_base`, `disc_base`) of a programme starting at `start_ms` with no programme
/// right before it, chained from the slate segment on air at that moment (D-267): that segment's numbers + 1.
pub fn bases_after_slate(
    asset: &PlaybackAsset,
    chain: Option<&SlateChain<'_>>,
    start_ms: i64,
) -> Option<(i64, i64)> {
    let (s, d) = slate_position(asset, chain, start_ms)?;
    Some((s + 1, d + 1))
}

/// The slate-side entries from the programme end (or `WINDOW_SEGMENTS - 1` back) to segment `g`, preceded
/// by the programme's own tail when the chain's start is reached.
fn slate_entries<'a>(
    sl: &SlateLoop<'a>,
    chain: Option<&SlateChain<'a>>,
    track: Track,
    g: i64,
) -> Vec<Entry<'a>> {
    let floor = (g - (WINDOW_SEGMENTS as i64 - 1)).max(0);
    let g0 = chain.map(|c| sl.index_at(c.end_ms));
    let lo = g0.map_or(floor, |g0| g0.max(floor));
    let mut out = Vec::new();
    if let (Some(c), Some(g0)) = (chain, g0)
        && lo == g0
        && let Some(tail) = block_entries(c.prev, track, None)
    {
        out.extend(tail);
    }
    out.extend((lo..=g).map(|gi| sl.entry(gi, chain)));
    out
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
    render_media_with_lead(blocks, track, now_ms, None, signer, now_secs)
}

/// [`render_media`], and when the programme on air has no programme right before it and its window is not
/// yet full, the slate that was on air before it (`lead`) fills the window (D-267): both sides of the
/// boundary are listed with a discontinuity between them.
pub fn render_media_with_lead(
    blocks: &[LiveBlock],
    track: Track,
    now_ms: i64,
    lead: Option<&SlateLead<'_>>,
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
    let mut entries: Vec<Entry<'_>> = Vec::new();
    for (bi, lb) in blocks.iter().enumerate().take(cur + 1).skip(first) {
        let upto = (bi == cur).then_some(now_ms);
        match block_entries(lb, track, upto) {
            Some(e) => entries.extend(e),
            None if bi == cur => return None,
            None => {}
        }
    }
    if entries
        .last()
        .is_none_or(|e| e.group != (false, blocks[cur].block.id))
    {
        return None;
    }
    if first == cur
        && entries.len() < WINDOW_SEGMENTS
        && let Some(lead) = lead
        && let Some(sl) = SlateLoop::new(lead.asset, track)
    {
        let g = sl.index_at(blocks[cur].block.start_ms);
        let mut before = slate_entries(&sl, lead.chain.as_ref(), track, g);
        before.append(&mut entries);
        entries = before;
    }
    let start = entries.len().saturating_sub(WINDOW_SEGMENTS);
    Some(emit(&entries[start..], signer, now_secs))
}

/// The media playlist of the slate (TV-HLS-003): the short pre-encoded loop `asset` repeated forever on
/// the wall clock, so an off-air channel answers a playable live playlist instead of 404. Every client
/// sees the same window (the loop is anchored at the unix epoch); each wrap of the loop carries a
/// discontinuity, a new map and a program date time, as a programme boundary does. `chain` (D-267) is the
/// programme that aired before the gap: the slate counts on from it and, while the window is young, lists
/// that programme's tail before the first slate segment.
pub fn render_slate_media(
    asset: &PlaybackAsset,
    track: Track,
    now_ms: i64,
    chain: Option<&SlateChain<'_>>,
    signer: &dyn UrlSigner,
    now_secs: u64,
) -> Option<String> {
    let sl = SlateLoop::new(asset, track)?;
    let g = sl.index_at(now_ms);
    let entries = slate_entries(&sl, chain, track, g);
    let start = entries.len().saturating_sub(WINDOW_SEGMENTS);
    Some(emit(&entries[start..], signer, now_secs))
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
    master_of(current, &heights, now_secs)
}

/// Heights of the standard transcode ladder (480p, 720p, 1080p; `sr_media::plan`), which the slate is encoded in.
pub const STANDARD_HEIGHTS: [u32; 3] = [480, 720, 1080];

/// The standard rungs `slate` lacks (empty for a slate made with `sr-cli tv slate`).
pub fn slate_missing_heights(slate: &PlaybackAsset) -> Vec<u32> {
    let own = common_heights(&[slate]);
    STANDARD_HEIGHTS
        .into_iter()
        .filter(|h| !own.contains(h))
        .collect()
}

/// The master of the slate when a channel is off air (D-267): the standard rung set, so a player switching
/// between programme and slate keeps its rung; when the slate lacks a rung only the rungs it has are offered
/// (the caller logs that once), and when it has none of the standard ones its own.
pub fn render_slate_master(slate: &PlaybackAsset, now_secs: u64) -> Option<String> {
    let own = common_heights(&[slate]);
    let standard: Vec<u32> = own
        .iter()
        .copied()
        .filter(|h| STANDARD_HEIGHTS.contains(h))
        .collect();
    master_of(
        slate,
        if standard.is_empty() { &own } else { &standard },
        now_secs,
    )
}

fn master_of(current: &PlaybackAsset, heights: &[u32], now_secs: u64) -> Option<String> {
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
pub(crate) mod tests {
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
    pub(crate) fn asset(id: i64, n: usize) -> PlaybackAsset {
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

    /// A slate of three 2 s segments (a 6 s loop).
    fn slate() -> PlaybackAsset {
        let mut a = asset(77, 3);
        for v in &mut a.media.videos {
            for s in &mut v.segments {
                s.duration_secs = 2.0;
            }
        }
        for au in &mut a.media.audios {
            for s in &mut au.segments {
                s.duration_secs = 2.0;
            }
        }
        a
    }

    fn header(text: &str, name: &str) -> i64 {
        text.lines()
            .find_map(|l| l.strip_prefix(name))
            .unwrap()
            .parse()
            .unwrap()
    }

    fn disc_tags(text: &str) -> usize {
        text.matches("#EXT-X-DISCONTINUITY\n").count()
    }

    #[test]
    fn the_slate_loops_on_the_wall_clock_with_a_discontinuity_at_each_wrap() {
        let mut a = slate();
        // 13 s: cycle 2 (12 s..18 s), segment 0 on air; global index 6. Window 1..=6.
        let text = render_slate_media(&a, Track::Video(720), 13_000, None, &Sig, 0).unwrap();
        let (seq, segs) = seqs(&text);
        // No programme before it: seeded from the wall clock in seconds (segment 1 starts at 2 s).
        assert_eq!(seq, 2);
        assert_eq!(segs.len(), WINDOW_SEGMENTS);
        assert_eq!(segs[0], "/cdn/hls/77/v/720p/seg_00001.m4s");
        assert_eq!(segs[5], "/cdn/hls/77/v/720p/seg_00000.m4s");
        assert_eq!(disc_tags(&text), 2);
        assert_eq!(text.matches("#EXT-X-MAP").count(), 3);
        assert_eq!(header(&text, "#EXT-X-DISCONTINUITY-SEQUENCE:"), 0);
        // Two clients a second apart see the same window.
        let b = render_slate_media(&a, Track::Video(720), 13_900, None, &Sig, 0).unwrap();
        assert_eq!(seqs(&b), (seq, segs));
        // Audio has a loop too; a track the asset lacks has none.
        assert!(render_slate_media(&a, Track::Audio, 13_000, None, &Sig, 0).is_some());
        a.media.audios.clear();
        assert!(render_slate_media(&a, Track::Audio, 13_000, None, &Sig, 0).is_none());
    }

    #[test]
    fn programme_to_slate_numbers_continue_with_one_discontinuity_and_both_sides_in_the_window() {
        // Ten segments, numbers 100..=109, discontinuity 4; ends at 60 s.
        let b = lb(1, 0, 60_000, 0, 100, 4, 10);
        let chain = SlateChain::after(&b).unwrap();
        let s = slate();
        // 64 s: slate segment 2 of loop 10 (global 32); the first slate segment (global 30) is 110.
        let text =
            render_slate_media(&s, Track::Video(720), 64_000, Some(&chain), &Sig, 0).unwrap();
        let (seq, segs) = seqs(&text);
        assert_eq!(seq, 107, "the tail of the programme is listed");
        assert_eq!(
            segs.iter().map(String::as_str).collect::<Vec<_>>(),
            [
                "/cdn/hls/100/v/720p/seg_00007.m4s",
                "/cdn/hls/100/v/720p/seg_00008.m4s",
                "/cdn/hls/100/v/720p/seg_00009.m4s",
                "/cdn/hls/77/v/720p/seg_00000.m4s",
                "/cdn/hls/77/v/720p/seg_00001.m4s",
                "/cdn/hls/77/v/720p/seg_00002.m4s",
            ]
        );
        assert_eq!(header(&text, "#EXT-X-DISCONTINUITY-SEQUENCE:"), 4);
        assert_eq!(disc_tags(&text), 1);
        let l = lines(&text);
        let d = l.iter().position(|l| *l == "#EXT-X-DISCONTINUITY").unwrap();
        assert!(l[d + 1].starts_with("#EXT-X-MAP:URI=\"/cdn/hls/77/v/720p/init.mp4"));
        assert!(l[d + 2].starts_with("#EXT-X-PROGRAM-DATE-TIME:1970-01-01T00:01:00.000Z"));
        // 70 s: slate only now, numbers 110..=115, discontinuity 5, one wrap inside.
        let t2 = render_slate_media(&s, Track::Video(720), 70_000, Some(&chain), &Sig, 0).unwrap();
        assert_eq!(seqs(&t2).0, 110);
        assert_eq!(header(&t2, "#EXT-X-DISCONTINUITY-SEQUENCE:"), 5);
        assert_eq!(disc_tags(&t2), 1);
        // 80 s: the head is segment 115 (loop 11), which carries discontinuity 6.
        let t3 = render_slate_media(&s, Track::Video(720), 80_000, Some(&chain), &Sig, 0).unwrap();
        assert_eq!(seqs(&t3).0, 115);
        assert_eq!(header(&t3, "#EXT-X-DISCONTINUITY-SEQUENCE:"), 6);
        // Audio has the same numbers.
        let au = render_slate_media(&s, Track::Audio, 70_000, Some(&chain), &Sig, 0).unwrap();
        assert_eq!(seqs(&au).0, 110);
    }

    #[test]
    fn slate_to_programme_numbers_continue_with_one_discontinuity_and_both_sides_in_the_window() {
        let b = lb(1, 0, 60_000, 0, 100, 4, 10);
        let chain = SlateChain::after(&b).unwrap();
        let s = slate();
        // A programme starts at 64 s, in the middle of slate segment 32 (112, discontinuity 5).
        assert_eq!(slate_position(&s, Some(&chain), 64_000), Some((112, 5)));
        let (seq, disc) = bases_after_slate(&s, Some(&chain), 64_000).unwrap();
        assert_eq!((seq, disc), (113, 6));
        let c = lb(2, 64_000, 130_000, 0, seq, disc, 11);
        let lead = SlateLead {
            asset: &s,
            chain: Some(chain),
        };
        let at = |t| {
            render_media_with_lead(
                std::slice::from_ref(&c),
                Track::Video(720),
                t,
                Some(&lead),
                &Sig,
                0,
            )
            .unwrap()
        };
        let first = at(65_000);
        let (s1, segs) = seqs(&first);
        assert_eq!(s1, 108);
        assert_eq!(segs.len(), WINDOW_SEGMENTS);
        assert_eq!(segs[5], "/cdn/hls/200/v/720p/seg_00000.m4s");
        assert_eq!(segs[4], "/cdn/hls/77/v/720p/seg_00002.m4s");
        assert_eq!(segs[1], "/cdn/hls/100/v/720p/seg_00009.m4s");
        // programme -> slate -> programme: two discontinuities, one per boundary.
        assert_eq!(disc_tags(&first), 2);
        assert_eq!(header(&first, "#EXT-X-DISCONTINUITY-SEQUENCE:"), 4);
        // Six seconds later the window moved by one (the new block segment 1 is 114).
        let later = at(71_000);
        assert_eq!(seqs(&later).0, 109);
        // Well into the programme the slate has left the window; numbers stay with the block.
        let much_later = at(100_000);
        assert_eq!(seqs(&much_later).0, 114);
        assert_eq!(disc_tags(&much_later), 0);
        assert_eq!(header(&much_later, "#EXT-X-DISCONTINUITY-SEQUENCE:"), 6);
    }

    #[test]
    fn without_a_slate_or_an_earlier_programme_the_seed_is_the_wall_clock() {
        let s = slate();
        // No programme before: slate numbers are seconds; a programme chains from the slate segment.
        assert_eq!(slate_position(&s, None, 64_000), Some((64, 60)));
        assert_eq!(bases_after_slate(&s, None, 64_000), Some((65, 61)));
        // A programme without a lead is numbered by its own bases as before.
        let c = lb(2, 64_000, 130_000, 0, 64, 64, 11);
        let text = render_media(&[c], Track::Video(720), 65_000, &Sig, 0).unwrap();
        assert_eq!(seqs(&text).0, 64);
        assert_eq!(disc_tags(&text), 0);
    }

    #[test]
    fn a_chain_needs_assigned_bases() {
        let mut b = lb(1, 0, 60_000, 0, 100, 4, 10);
        b.block.seq_base = None;
        assert!(SlateChain::after(&b).is_none());
    }

    #[test]
    fn the_off_air_master_offers_the_standard_rungs() {
        let mut s = slate();
        let full = render_slate_master(&s, 0).unwrap();
        assert!(full.contains("v/720.m3u8") && full.contains("v/1080.m3u8"));
        assert!(
            slate_missing_heights(&s).contains(&480),
            "sample has no 480"
        );
        // A slate with only a non-standard rung offers what it has.
        s.media.videos.retain(|v| v.height == 720);
        let one = render_slate_master(&s, 0).unwrap();
        assert!(one.contains("v/720.m3u8") && !one.contains("v/1080.m3u8"));
        assert!(slate_missing_heights(&s).contains(&1080));
    }

    #[test]
    fn iso_formats_milliseconds() {
        assert_eq!(iso(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso(1_700_000_123_456), "2023-11-14T22:15:23.456Z");
    }
}
