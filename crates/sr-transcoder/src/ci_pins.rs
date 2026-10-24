//! Pins for the media pipeline test in CI (`.github/workflows/media.yml`, RDY-MED-003): the exact
//! plan and ffmpeg argument lists for the source the smoke driver generates with `lavfi`
//! (`deploy/scripts/media-smoke.mjs`), and the playlist text ffmpeg's HLS muxer writes. When a real
//! ffmpeg disagrees with one of these, the fix and its test land together; no ffmpeg is needed here.
use crate::args::tests::caps;
use crate::args::{BuildCtx, StepKind, ladder_steps, proxy_steps, remux_steps};
use crate::encoders::Encoder;
use sr_media::plan::{TranscodeSettings, plan_ladder_job, plan_proxy_job};
use sr_media::probe::parse_ffprobe_json;
use std::path::PathBuf;

/// What `ffprobe -show_streams -show_format -of json` reports for the generated source: Matroska,
/// 1280x720 at 25 fps H.264, two stereo AAC tracks (eng, spa) and one SubRip subtitle (eng).
pub const CI_SOURCE: &str = r#"{"streams":[
  {"index":0,"codec_name":"h264","codec_type":"video","width":1280,"height":720,
   "pix_fmt":"yuv420p","field_order":"progressive","r_frame_rate":"25/1","avg_frame_rate":"25/1",
   "disposition":{"default":1}},
  {"index":1,"codec_name":"aac","codec_type":"audio","channels":2,"sample_rate":"48000",
   "disposition":{"default":1},"tags":{"language":"eng","title":"English"}},
  {"index":2,"codec_name":"aac","codec_type":"audio","channels":2,"sample_rate":"48000",
   "disposition":{"default":0},"tags":{"language":"spa","title":"Spanish"}},
  {"index":3,"codec_name":"subrip","codec_type":"subtitle",
   "disposition":{"default":0},"tags":{"language":"eng","title":"English"}}
],"format":{"format_name":"matroska,webm","duration":"20.000000"}}"#;

fn probe() -> sr_media::probe::ProbeResult {
    parse_ffprobe_json(CI_SOURCE.as_bytes()).expect("the CI source probe parses")
}

#[test]
fn ci_source_ladder_plan_is_pinned() {
    let plan = plan_ladder_job(&probe(), &TranscodeSettings::default()).unwrap();
    let rungs: Vec<_> = plan
        .rungs
        .iter()
        .map(|r| (r.name.as_str(), r.width, r.height))
        .collect();
    // 1080p is not offered for a 720p source (never upscaled).
    assert_eq!(rungs, vec![("720p", 1280, 720), ("480p", 854, 480)]);
    assert_eq!((plan.video.fps_num, plan.video.fps_den), (25, 1));
    assert_eq!(plan.video.gop_frames, 50);
    assert_eq!(plan.hls.segment_secs, 6);
    let audio: Vec<_> = plan.audio.iter().map(|a| a.dir.as_str()).collect();
    assert_eq!(audio, vec!["a/00-eng-stereo", "a/01-spa-stereo"]);
    assert_eq!(plan.subtitles.len(), 1);
    assert_eq!(plan.subtitles[0].vtt_path, "subs/00-eng.vtt");
    assert_eq!(plan.sprite.sheets, 1);
    assert_eq!(plan.sprite.thumbs_total, 2);
    assert_eq!(plan.sprite.vtt_path, "sprites/sprites.vtt");
    assert!(plan.fonts.is_empty());
    assert!(plan.warnings.is_empty(), "{:?}", plan.warnings);
}

#[test]
fn ci_source_step_list_and_argument_shape() {
    let plan = plan_ladder_job(&probe(), &TranscodeSettings::default()).unwrap();
    let caps = caps();
    let out = PathBuf::from("/work/j/out");
    let ctx = BuildCtx {
        input: "/work/j/in/source.mkv",
        out_dir: &out,
        encoder: Encoder::Libx264,
        caps: &caps,
        duration_secs: 20.0,
    };
    let steps = ladder_steps(&plan, &ctx).unwrap();
    let kinds: Vec<_> = steps.iter().map(|s| s.kind).collect();
    assert_eq!(
        kinds,
        vec![
            StepKind::Video,
            StepKind::Audio,
            StepKind::Audio,
            StepKind::Subtitle,
            StepKind::Sprites,
            StepKind::Poster,
            StepKind::Poster,
            StepKind::Poster,
            StepKind::Poster,
            StepKind::Poster,
        ]
    );
    let video = steps[0].args.join(" ");
    assert!(
        video.contains(
            "-filter_complex [0:v:0]fps=25/1,split=2[s0][s1];\
             [s0]scale=1280:720:flags=lanczos,setsar=1,format=yuv420p[o0];\
             [s1]scale=854:480:flags=lanczos,setsar=1,format=yuv420p[o1] "
        ),
        "{video}"
    );
    assert!(
        video.ends_with(
            "-hls_segment_filename /work/j/out/v/480p/seg_%05d.m4s /work/j/out/v/480p/index.m3u8"
        ),
        "{video}"
    );
    // The second audio track maps the second audio stream of the source, not stream 2 of the file.
    let a1 = steps[2].args.join(" ");
    assert!(
        a1.contains("-map 0:a:1 -vn -sn -c:a aac -ar 48000 -ac 2"),
        "{a1}"
    );
    assert!(
        a1.ends_with("/work/j/out/a/01-spa-stereo/index.m3u8"),
        "{a1}"
    );
    // The subtitle is the first subtitle stream of the source, written as SubRip.
    let sub = steps[3].args.join(" ");
    assert!(
        sub.contains("-map 0:s:0 -vn -an -c:s srt -f srt /work/j/out/subs/00-eng.srt"),
        "{sub}"
    );
    // 20 s at one frame per 10 s, 10 x 10 tiles of 160 x 90 in one sheet.
    let sprite = steps[4].args.join(" ");
    assert!(
        sprite.contains("-vf fps=1/10,scale=160:90:flags=bilinear,tile=10x10"),
        "{sprite}"
    );
    assert!(
        sprite.ends_with("-f image2 /work/j/out/sprites/sheet-%03d.jpg"),
        "{sprite}"
    );
}

#[test]
fn ci_source_proxy_is_480p_with_the_default_stereo_track() {
    let plan = plan_proxy_job(&probe(), &TranscodeSettings::default()).unwrap();
    let caps = caps();
    let out = PathBuf::from("/work/j/out");
    let ctx = BuildCtx {
        input: "/work/j/in/source.mkv",
        out_dir: &out,
        encoder: Encoder::Libx264,
        caps: &caps,
        duration_secs: 20.0,
    };
    let args = proxy_steps(&plan, &ctx).unwrap().remove(0).args.join(" ");
    assert!(
        args.contains(
            "-map 0:v:0 -map 0:a:0 -vf fps=25,scale=854:480:flags=bilinear,setsar=1,format=yuv420p"
        ),
        "{args}"
    );
    assert!(
        args.ends_with("-movflags +faststart -f mp4 /work/j/out/proxy/proxy.mp4"),
        "{args}"
    );
}

/// What ffmpeg 6.x writes for `-hls_segment_type fmp4 -hls_playlist_type vod` (video and audio).
const FFMPEG_VOD_PLAYLIST: &str = "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:6\n\
#EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-PLAYLIST-TYPE:VOD\n#EXT-X-INDEPENDENT-SEGMENTS\n\
#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:6.000000,\nseg_00000.m4s\n#EXTINF:6.000000,\nseg_00001.m4s\n\
#EXTINF:6.000000,\nseg_00002.m4s\n#EXTINF:2.000000,\nseg_00003.m4s\n#EXT-X-ENDLIST\n";

#[test]
fn ffmpeg_vod_playlist_parses_into_the_segment_index() {
    let p = sr_media::playlist::parse_media_playlist(FFMPEG_VOD_PLAYLIST).unwrap();
    assert_eq!(p.init_name.as_deref(), Some("init.mp4"));
    let names: Vec<_> = p.segments.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        [
            "seg_00000.m4s",
            "seg_00001.m4s",
            "seg_00002.m4s",
            "seg_00003.m4s"
        ]
    );
    let total: f64 = p.segments.iter().map(|(_, d)| d).sum();
    assert!((total - 20.0).abs() < 1e-9);
    // The ffmpeg-written file names match the pattern the planner asks for.
    let plan = plan_ladder_job(&probe(), &TranscodeSettings::default()).unwrap();
    assert_eq!(plan.rungs[0].segment_pattern, "v/720p/seg_%05d.m4s");
    assert_eq!(plan.rungs[0].init_segment, "v/720p/init.mp4");
}

#[test]
fn remux_copies_streams_from_the_local_playlists() {
    use sr_media::remux::{RemuxFile, RemuxRequest, RemuxTrack, plan_remux};
    let track = |dir: &str| RemuxTrack {
        playlist: format!("{dir}/index.m3u8"),
        init: format!("{dir}/init.mp4"),
        segments: (0..4)
            .map(|i| RemuxFile {
                path: format!("{dir}/seg_{i:05}.m4s"),
                size_bytes: 1000,
            })
            .collect(),
        playlist_size: 300,
        init_size: 700,
    };
    let plan = plan_remux(&RemuxRequest {
        rendition: "720p".into(),
        cdn_base: "/cdn/transcode/1/1".into(),
        video: track("v/720p"),
        audio: vec![track("a/00-eng-stereo"), track("a/01-spa-stereo")],
        duration_secs: 20.0,
    })
    .unwrap();
    let steps = remux_steps(
        &plan,
        &PathBuf::from("/work/j/in"),
        &PathBuf::from("/work/j/out"),
    )
    .unwrap();
    assert_eq!(
        steps[0].args.join(" "),
        "-hide_banner -nostdin -y -loglevel warning -nostats -progress pipe:1 \
         -i /work/j/in/v/720p/index.m3u8 -i /work/j/in/a/00-eng-stereo/index.m3u8 \
         -i /work/j/in/a/01-spa-stereo/index.m3u8 -map 0:v:0 -map 1:a:0 -map 2:a:0 \
         -c copy -sn -dn -movflags +faststart -f mp4 /work/j/out/remux/video.mp4"
    );
}

/// Real ffprobe output for an x264 Matroska made by ffmpeg carries no `field_order` and tags the audio
/// with `BPS`/`DURATION` statistics; none of that changes the plan.
#[test]
fn ci_source_plan_does_not_depend_on_optional_ffprobe_fields() {
    let lean = CI_SOURCE.replace(r#""field_order":"progressive","#, "");
    let noisy = lean.replace(
        r#""tags":{"language":"eng","title":"English"}},
  {"index":2"#,
        r#""tags":{"language":"eng","title":"English","BPS":"96000","DURATION":"00:00:20.000000000","NUMBER_OF_FRAMES":"940"}},
  {"index":2"#,
    );
    assert_ne!(lean, CI_SOURCE);
    assert_ne!(noisy, lean);
    let reference = plan_ladder_job(&probe(), &TranscodeSettings::default()).unwrap();
    for json in [&lean, &noisy] {
        let p = parse_ffprobe_json(json.as_bytes()).unwrap();
        let plan = plan_ladder_job(&p, &TranscodeSettings::default()).unwrap();
        assert!(!plan.video.deinterlace);
        assert_eq!(plan.rungs, reference.rungs);
        assert_eq!(plan.audio, reference.audio);
        assert_eq!(plan.subtitles, reference.subtitles);
    }
}
