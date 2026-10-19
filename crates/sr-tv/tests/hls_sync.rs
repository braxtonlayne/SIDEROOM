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
