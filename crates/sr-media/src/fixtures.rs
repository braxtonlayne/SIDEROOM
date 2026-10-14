//! Canned `ffprobe -print_format json` documents, shaped like real ffprobe 6/7 output.

/// 1080p H.264 High SDR in Matroska, one 5.1 English AC-3 track.
pub const H264_SDR: &str = r#"{
  "streams": [
    {"index":0,"codec_name":"h264","codec_long_name":"H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10",
     "profile":"High","codec_type":"video","width":1920,"height":1080,"coded_width":1920,
     "coded_height":1088,"sample_aspect_ratio":"1:1","display_aspect_ratio":"16:9",
     "pix_fmt":"yuv420p","level":41,"color_range":"tv","color_space":"bt709",
     "color_transfer":"bt709","color_primaries":"bt709","field_order":"progressive",
     "r_frame_rate":"24000/1001","avg_frame_rate":"24000/1001","bits_per_raw_sample":"8",
     "disposition":{"default":1,"forced":0,"attached_pic":0},
     "tags":{"language":"eng","BPS":"6500000"}},
    {"index":1,"codec_name":"ac3","profile":"unknown","codec_type":"audio","sample_fmt":"fltp",
     "sample_rate":"48000","channels":6,"channel_layout":"5.1(side)","bit_rate":"640000",
     "disposition":{"default":1,"forced":0,"comment":0},
     "tags":{"language":"eng","title":"Surround 5.1"}}
  ],
  "chapters": [],
  "format": {"filename":"in.mkv","nb_streams":2,"format_name":"matroska,webm",
     "format_long_name":"Matroska / WebM","start_time":"0.000000","duration":"5432.123000",
     "size":"4100000000","bit_rate":"6038000","tags":{"title":"Some Film"}}
}"#;

/// 4K HEVC Main 10, PQ / BT.2020, with mastering display metadata.
pub const HEVC_HDR10: &str = r#"{
  "streams": [
    {"index":0,"codec_name":"hevc","profile":"Main 10","codec_type":"video","width":3840,
     "height":2160,"sample_aspect_ratio":"1:1","pix_fmt":"yuv420p10le","level":153,
     "color_range":"tv","color_space":"bt2020nc","color_transfer":"smpte2084",
     "color_primaries":"bt2020","field_order":"progressive","r_frame_rate":"24000/1001",
     "avg_frame_rate":"24000/1001","bits_per_raw_sample":"10",
     "disposition":{"default":1},
     "side_data_list":[{"side_data_type":"Mastering display metadata"},
                       {"side_data_type":"Content light level metadata","max_content":1000,
                        "max_average":400}],
     "tags":{"language":"eng"}},
    {"index":1,"codec_name":"eac3","codec_type":"audio","sample_rate":"48000","channels":6,
     "channel_layout":"5.1(side)","disposition":{"default":1},"tags":{"language":"eng"}}
  ],
  "format":{"format_name":"matroska,webm","duration":"7200.5","size":"30000000000",
     "bit_rate":"33000000"}
}"#;

/// Dolby Vision profile 8.1 (HDR10 compatible base layer).
pub const DV_P81: &str = r#"{
  "streams": [
    {"index":0,"codec_name":"hevc","profile":"Main 10","codec_type":"video","width":3840,
     "height":2160,"pix_fmt":"yuv420p10le","color_transfer":"smpte2084",
     "color_primaries":"bt2020","color_space":"bt2020nc","field_order":"progressive",
     "r_frame_rate":"24/1","avg_frame_rate":"24/1","disposition":{"default":1},
     "side_data_list":[{"side_data_type":"DOVI configuration record","dv_version_major":1,
        "dv_version_minor":0,"dv_profile":8,"dv_level":6,"rpu_present_flag":1,
        "el_present_flag":0,"bl_present_flag":1,"dv_bl_signal_compatibility_id":1}]}
  ],
  "format":{"format_name":"mov,mp4,m4a,3gp,3g2,mj2","duration":"3600.0"}
}"#;

/// Dolby Vision profile 5 (no fallback layer).
pub const DV_P5: &str = r#"{
  "streams": [
    {"index":0,"codec_name":"hevc","profile":"Main 10","codec_type":"video","width":3840,
     "height":2160,"pix_fmt":"yuv420p10le","color_space":"unknown","field_order":"progressive",
     "r_frame_rate":"24/1","avg_frame_rate":"24/1","disposition":{"default":1},
     "side_data_list":[{"side_data_type":"DOVI configuration record","dv_profile":5,
        "dv_level":6,"bl_present_flag":1,"dv_bl_signal_compatibility_id":0}]}
  ],
  "format":{"format_name":"mov,mp4,m4a,3gp,3g2,mj2","duration":"1800.0"}
}"#;

/// Broadcast HLG capture.
pub const HLG: &str = r#"{
  "streams": [
    {"index":0,"codec_name":"hevc","profile":"Main 10","codec_type":"video","width":1920,
     "height":1080,"pix_fmt":"yuv420p10le","color_transfer":"arib-std-b67",
     "color_primaries":"bt2020","color_space":"bt2020nc","field_order":"progressive",
     "r_frame_rate":"50/1","avg_frame_rate":"50/1"}
  ],
  "format":{"format_name":"mpegts","duration":"600.0"}
}"#;

/// HDR10+ (dynamic metadata over a PQ stream).
pub const HDR10PLUS: &str = r#"{
  "streams": [
    {"index":0,"codec_name":"hevc","profile":"Main 10","codec_type":"video","width":3840,
     "height":2160,"pix_fmt":"yuv420p10le","color_transfer":"smpte2084",
     "color_primaries":"bt2020","field_order":"progressive","r_frame_rate":"60/1",
     "avg_frame_rate":"60/1",
     "side_data_list":[{"side_data_type":"HDR Dynamic Metadata SMPTE2094-40 (HDR10+)"}]}
  ],
  "format":{"format_name":"matroska,webm","duration":"900.0"}
}"#;

/// NTSC DVD rip: interlaced MPEG-2, anamorphic 32:27.
pub const INTERLACED_DVD: &str = r#"{
  "streams": [
    {"index":0,"codec_name":"mpeg2video","profile":"Main","codec_type":"video","width":720,
     "height":480,"sample_aspect_ratio":"32:27","display_aspect_ratio":"16:9",
     "pix_fmt":"yuv420p","color_space":"smpte170m","field_order":"tt",
     "r_frame_rate":"30000/1001","avg_frame_rate":"30000/1001"},
    {"index":1,"codec_name":"ac3","codec_type":"audio","sample_rate":"48000","channels":6,
     "tags":{"language":"eng"}}
  ],
  "format":{"format_name":"mpeg","duration":"5400.0","size":"4500000000"}
}"#;

/// Four audio tracks: English TrueHD 7.1, Japanese stereo, English commentary, untagged.
pub const MULTI_AUDIO: &str = r#"{
  "streams": [
    {"index":0,"codec_name":"h264","codec_type":"video","width":1920,"height":1080,
     "pix_fmt":"yuv420p","color_transfer":"bt709","field_order":"progressive",
     "r_frame_rate":"24/1","avg_frame_rate":"24/1","disposition":{"default":1}},
    {"index":1,"codec_name":"truehd","codec_type":"audio","sample_rate":"48000","channels":8,
     "channel_layout":"7.1","disposition":{"default":1},"tags":{"language":"eng"}},
    {"index":2,"codec_name":"aac","profile":"LC","codec_type":"audio","sample_rate":"48000",
     "channels":2,"channel_layout":"stereo","disposition":{"default":0},
     "tags":{"language":"jpn","title":"Japanese"}},
    {"index":3,"codec_name":"ac3","codec_type":"audio","sample_rate":"48000","channels":2,
     "disposition":{"default":0,"comment":1},
     "tags":{"language":"eng","title":"Director's Commentary"}},
    {"index":4,"codec_name":"aac","codec_type":"audio","sample_rate":"44100","channels":2,
     "tags":{"language":"und"}}
  ],
  "format":{"format_name":"matroska,webm","duration":"6000.0"}
}"#;

/// ASS, forced SRT, PGS, VobSub and mov_text subtitles plus embedded fonts and chapters.
pub const SUBS_AND_FONTS: &str = r#"{
  "streams": [
    {"index":0,"codec_name":"h264","codec_type":"video","width":1920,"height":1080,
     "pix_fmt":"yuv420p","field_order":"progressive","r_frame_rate":"24/1",
     "avg_frame_rate":"24/1"},
    {"index":1,"codec_name":"aac","codec_type":"audio","channels":2,
     "tags":{"language":"jpn"}},
    {"index":2,"codec_name":"ass","codec_type":"subtitle","disposition":{"default":1},
     "tags":{"language":"eng","title":"Signs & Songs"}},
    {"index":3,"codec_name":"subrip","codec_type":"subtitle","disposition":{"forced":1},
     "tags":{"language":"spa"}},
    {"index":4,"codec_name":"hdmv_pgs_subtitle","codec_type":"subtitle",
     "tags":{"language":"fre"}},
    {"index":5,"codec_name":"dvd_subtitle","codec_type":"subtitle","tags":{"language":"ger"}},
    {"index":6,"codec_name":"mov_text","codec_type":"subtitle",
     "disposition":{"hearing_impaired":1},"tags":{"language":"eng"}},
    {"index":7,"codec_name":"ttf","codec_type":"attachment",
     "tags":{"filename":"Arial-Bold.ttf","mimetype":"application/x-truetype-font"}},
    {"index":8,"codec_name":"none","codec_type":"attachment",
     "tags":{"filename":"../../etc/passwd","mimetype":"text/plain"}}
  ],
  "chapters":[
    {"id":1,"time_base":"1/1000","start":0,"start_time":"0.000000","end":90000,
     "end_time":"90.000000","tags":{"title":"Opening"}},
    {"id":2,"time_base":"1/1000","start":90000,"start_time":"90.000000","end":1500000,
     "end_time":"1500.000000","tags":{"title":"Part A"}}
  ],
  "format":{"format_name":"matroska,webm","duration":"1500.0"}
}"#;

/// An MP4 with cover art stored as a second video stream.
pub const COVER_ART: &str = r#"{
  "streams": [
    {"index":0,"codec_name":"h264","codec_type":"video","width":1280,"height":720,
     "pix_fmt":"yuv420p","field_order":"progressive","r_frame_rate":"25/1",
     "avg_frame_rate":"25/1","disposition":{"default":1,"attached_pic":0}},
    {"index":1,"codec_name":"mjpeg","codec_type":"video","width":3000,"height":3000,
     "pix_fmt":"yuvj444p","r_frame_rate":"90000/1","avg_frame_rate":"0/0",
     "disposition":{"default":0,"attached_pic":1}}
  ],
  "format":{"format_name":"mov,mp4,m4a,3gp,3g2,mj2","duration":"120.0"}
}"#;
