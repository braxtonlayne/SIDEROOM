//! Tests build every file byte by byte; nothing is downloaded.

use super::*;

const JPEG: &[u8] = &[0xff, 0xd8, 0xff, 0xe0, 1, 2, 3, 4, 0xff, 0xd9];

fn syncsafe(n: u32) -> [u8; 4] {
    [
        ((n >> 21) & 0x7f) as u8,
        ((n >> 14) & 0x7f) as u8,
        ((n >> 7) & 0x7f) as u8,
        (n & 0x7f) as u8,
    ]
}

fn utf16(s: &str, bom: bool) -> Vec<u8> {
    let mut v = if bom { vec![0xff, 0xfe] } else { vec![] };
    for u in s.encode_utf16() {
        v.extend_from_slice(&u.to_le_bytes());
    }
    v
}

fn text_body(enc: u8, s: &str) -> Vec<u8> {
    let mut b = vec![enc];
    match enc {
        0 => b.extend(s.chars().map(|c| c as u8)),
        1 => b.extend(utf16(s, true)),
        2 => {
            for u in s.encode_utf16() {
                b.extend_from_slice(&u.to_be_bytes());
            }
        }
        _ => b.extend_from_slice(s.as_bytes()),
    }
    b
}

fn frame(major: u8, id: &str, body: &[u8], flags: u16) -> Vec<u8> {
    let mut f = id.as_bytes().to_vec();
    if major == 4 {
        f.extend_from_slice(&syncsafe(body.len() as u32));
    } else {
        f.extend_from_slice(&(body.len() as u32).to_be_bytes());
    }
    f.extend_from_slice(&flags.to_be_bytes());
    f.extend_from_slice(body);
    f
}

fn apic(kind: u8, mime: &str, data: &[u8]) -> Vec<u8> {
    let mut b = vec![0];
    b.extend_from_slice(mime.as_bytes());
    b.push(0);
    b.push(kind);
    b.extend_from_slice(b"cover\0");
    b.extend_from_slice(data);
    b
}

fn id3(major: u8, flags: u8, frames: &[Vec<u8>]) -> Vec<u8> {
    let body: Vec<u8> = frames.concat();
    let mut f = b"ID3".to_vec();
    f.extend_from_slice(&[major, 0, flags]);
    f.extend_from_slice(&syncsafe(body.len() as u32));
    f.extend_from_slice(&body);
    f
}

#[test]
fn id3v24_text_frames_encodings_and_cover() {
    let bytes = id3(
        4,
        0,
        &[
            frame(4, "TIT2", &text_body(3, "Café Del Mar"), 0),
            frame(4, "TPE1", &text_body(1, "Енергия"), 0),
            frame(4, "TALB", &text_body(0, "Caf\u{e9} Album"), 0),
            frame(4, "TPE2", &text_body(2, "Various"), 0),
            frame(4, "TRCK", &text_body(0, "3/12"), 0),
            frame(4, "TPOS", &text_body(0, "2/2"), 0),
            frame(4, "TDRC", &text_body(3, "2019-03-04"), 0),
            frame(4, "TCON", &text_body(0, "(17)"), 0),
            frame(4, "TLEN", &text_body(0, "215000"), 0),
            frame(4, "APIC", &apic(0, "image/png", &[0x89, b'P', 1]), 0),
            frame(4, "APIC", &apic(3, "image/jpg", JPEG), 0),
            vec![0; 40],
        ],
    );
    let t = read_tags(&bytes).unwrap();
    assert_eq!(t.title.as_deref(), Some("Café Del Mar"));
    assert_eq!(t.artist.as_deref(), Some("Енергия"));
    assert_eq!(t.album.as_deref(), Some("Café Album"));
    assert_eq!(t.album_artist.as_deref(), Some("Various"));
    assert_eq!((t.track, t.disc, t.year), (Some(3), Some(2), Some(2019)));
    assert_eq!(t.genre.as_deref(), Some("Rock"));
    assert_eq!(t.duration_hint, Some(215_000));
    let c = t.cover.unwrap();
    assert_eq!(c.mime, "image/jpeg"); // the front cover wins and image/jpg is repaired
    assert_eq!(c.bytes, JPEG);
}

#[test]
fn id3v23_whole_tag_unsynchronisation_and_multiple_artists() {
    // The JPEG-like data contains FF D8 FF; unsynchronised, FF followed by a byte >= E0 gets 00.
    let plain = frame(
        3,
        "APIC",
        &apic(3, "image/jpeg", &[0xff, 0xe0, 0xff, 0x00, 5]),
        0,
    );
    let mut body = Vec::new();
    let all: Vec<u8> = [
        frame(3, "TIT2", &text_body(0, "Song"), 0),
        frame(3, "TPE1", &[3, b'A', 0, b'B'].to_vec(), 0),
        plain,
    ]
    .concat();
    for (i, b) in all.iter().enumerate() {
        body.push(*b);
        if *b == 0xff && all.get(i + 1).is_some_and(|n| *n >= 0xe0 || *n == 0) {
            body.push(0);
        }
    }
    let mut file = b"ID3".to_vec();
    file.extend_from_slice(&[3, 0, 0x80]);
    file.extend_from_slice(&syncsafe(body.len() as u32));
    file.extend_from_slice(&body);
    let t = read_tags(&file).unwrap();
    assert_eq!(t.title.as_deref(), Some("Song"));
    assert_eq!(t.artist.as_deref(), Some("A, B"));
    assert_eq!(t.cover.unwrap().bytes, vec![0xff, 0xe0, 0xff, 0x00, 5]);
}

#[test]
fn id3v24_frame_unsync_flag_and_data_length_indicator() {
    let data = [3, 0xff, 0x00, 0xe0, b'x']; // utf-8 text "\xff?" is invalid; use latin1 enc 0
    let _ = data;
    let raw = text_body(0, "A\u{ff}\u{e0}"); // bytes 00 41 FF E0
    let mut enc = Vec::new();
    for (i, b) in raw.iter().enumerate() {
        enc.push(*b);
        if *b == 0xff && raw.get(i + 1).is_some_and(|n| *n >= 0xe0) {
            enc.push(0);
        }
    }
    let mut with_len = syncsafe(raw.len() as u32).to_vec();
    with_len.extend_from_slice(&enc);
    let f = frame(4, "TIT2", &with_len, 0x0003);
    let t = read_tags(&id3(4, 0, &[f])).unwrap();
    assert_eq!(t.title.as_deref(), Some("A\u{ff}\u{e0}"));
}

#[test]
fn id3v22_frames() {
    let mut f = Vec::new();
    for (id, body) in [
        ("TT2", text_body(0, "Old")),
        ("TP1", text_body(0, "Timer")),
        ("TRK", text_body(0, "7")),
        ("TYE", text_body(0, "1999")),
    ] {
        f.extend_from_slice(id.as_bytes());
        f.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
        f.extend_from_slice(&body);
    }
    let mut pic = vec![0];
    pic.extend_from_slice(b"JPG");
    pic.push(3);
    pic.push(0);
    pic.extend_from_slice(JPEG);
    f.extend_from_slice(b"PIC");
    f.extend_from_slice(&(pic.len() as u32).to_be_bytes()[1..]);
    f.extend_from_slice(&pic);
    let mut file = b"ID3".to_vec();
    file.extend_from_slice(&[2, 0, 0]);
    file.extend_from_slice(&syncsafe(f.len() as u32));
    file.extend_from_slice(&f);
    let t = read_tags(&file).unwrap();
    assert_eq!(t.title.as_deref(), Some("Old"));
    assert_eq!(t.artist.as_deref(), Some("Timer"));
    assert_eq!((t.track, t.year), (Some(7), Some(1999)));
    assert_eq!(t.cover.unwrap().mime, "image/jpeg");
}

#[test]
fn id3v2_extended_header_is_skipped() {
    let mut body = vec![0, 0, 0, 6, 0, 0, 0, 0, 0, 0]; // v2.3: size 6 (+4), flags, padding
    body.extend_from_slice(&frame(3, "TIT2", &text_body(0, "Ext"), 0));
    let mut file = b"ID3".to_vec();
    file.extend_from_slice(&[3, 0, 0x40]);
    file.extend_from_slice(&syncsafe(body.len() as u32));
    file.extend_from_slice(&body);
    assert_eq!(read_tags(&file).unwrap().title.as_deref(), Some("Ext"));
}

#[test]
fn id3v2_tag_cut_off_inside_the_cover_keeps_the_text() {
    let full = id3(
        4,
        0,
        &[
            frame(4, "TIT2", &text_body(3, "Head"), 0),
            frame(4, "APIC", &apic(3, "image/jpeg", &vec![0xff; 5000]), 0),
        ],
    );
    let cut = &full[..full.len() - 100];
    let t = read_tags(cut).unwrap();
    assert_eq!(t.title.as_deref(), Some("Head"));
    assert!(t.cover.is_none());
}

#[test]
fn id3v1_tail_with_track() {
    let mut tag = vec![0u8; 128];
    tag[..3].copy_from_slice(b"TAG");
    tag[3..8].copy_from_slice(b"Title");
    tag[33..39].copy_from_slice(b"Artist");
    tag[63..68].copy_from_slice(b"Album");
    tag[93..97].copy_from_slice(b"2001");
    tag[126] = 9; // v1.1 track
    tag[127] = 13; // Pop
    let t = read_tags_tail(&tag).unwrap();
    assert_eq!(t.title.as_deref(), Some("Title"));
    assert_eq!(t.artist.as_deref(), Some("Artist"));
    assert_eq!(t.album.as_deref(), Some("Album"));
    assert_eq!((t.year, t.track), (Some(2001), Some(9)));
    assert_eq!(t.genre.as_deref(), Some("Pop"));
    // A small whole file ending in the tag is read by read_tags too.
    let mut whole = vec![0xff, 0xfb, 0x90, 0x00];
    whole.extend_from_slice(&tag);
    assert_eq!(read_tags(&whole).unwrap().title.as_deref(), Some("Title"));
}

fn vorbis_comments(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&3u32.to_le_bytes());
    b.extend_from_slice(b"sr!");
    b.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for (k, v) in entries {
        let e = format!("{k}={v}");
        b.extend_from_slice(&(e.len() as u32).to_le_bytes());
        b.extend_from_slice(e.as_bytes());
    }
    b
}

fn flac_picture(mime: &str, data: &[u8]) -> Vec<u8> {
    let mut b = 3u32.to_be_bytes().to_vec();
    b.extend_from_slice(&(mime.len() as u32).to_be_bytes());
    b.extend_from_slice(mime.as_bytes());
    b.extend_from_slice(&0u32.to_be_bytes()); // description
    b.extend_from_slice(&[0; 16]);
    b.extend_from_slice(&(data.len() as u32).to_be_bytes());
    b.extend_from_slice(data);
    b
}

fn flac_block(kind: u8, last: bool, body: &[u8]) -> Vec<u8> {
    let mut b = vec![kind | if last { 0x80 } else { 0 }];
    b.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
    b.extend_from_slice(body);
    b
}

fn streaminfo(rate: u32, total: u64) -> Vec<u8> {
    let mut s = vec![0u8; 34];
    s[10] = (rate >> 12) as u8;
    s[11] = (rate >> 4) as u8;
    s[12] = ((rate & 0x0f) << 4) as u8;
    s[13] |= ((total >> 32) & 0x0f) as u8;
    s[14..18].copy_from_slice(&(total as u32).to_be_bytes());
    s
}

fn flac_file() -> Vec<u8> {
    let mut f = b"fLaC".to_vec();
    f.extend(flac_block(0, false, &streaminfo(44_100, 44_100 * 200)));
    f.extend(flac_block(
        4,
        false,
        &vorbis_comments(&[
            ("title", "Flac Song"),
            ("ARTIST", "Band"),
            ("ALBUM", "LP"),
            ("ALBUMARTIST", "Band"),
            ("TRACKNUMBER", "04/10"),
            ("DISCNUMBER", "1"),
            ("DATE", "2012-05"),
            ("GENRE", "Jazz"),
        ]),
    ));
    f.extend(flac_block(6, true, &flac_picture("image/jpeg", JPEG)));
    f.extend_from_slice(&[0xff, 0xf8, 0, 0]); // audio
    f
}

#[test]
fn flac_comments_picture_and_duration() {
    let t = read_tags(&flac_file()).unwrap();
    assert_eq!(t.title.as_deref(), Some("Flac Song"));
    assert_eq!(t.artist.as_deref(), Some("Band"));
    assert_eq!(t.album_artist.as_deref(), Some("Band"));
    assert_eq!((t.track, t.disc, t.year), (Some(4), Some(1), Some(2012)));
    assert_eq!(t.genre.as_deref(), Some("Jazz"));
    assert_eq!(t.duration_hint, Some(200_000));
    assert_eq!(t.cover.unwrap().bytes, JPEG);
}

fn ogg_page(seq: u32, header: u8, packets: &[&[u8]], continued_last: bool) -> Vec<u8> {
    let mut table = Vec::new();
    let mut data = Vec::new();
    for (i, p) in packets.iter().enumerate() {
        let mut left = p.len();
        loop {
            let seg = left.min(255);
            table.push(seg as u8);
            left -= seg;
            if seg < 255 {
                break;
            }
            if left == 0 && i + 1 == packets.len() && continued_last {
                break;
            }
        }
        data.extend_from_slice(p);
    }
    let mut pg = b"OggS".to_vec();
    pg.push(0);
    pg.push(header);
    pg.extend_from_slice(&[0; 8]);
    pg.extend_from_slice(&7u32.to_le_bytes());
    pg.extend_from_slice(&seq.to_le_bytes());
    pg.extend_from_slice(&[0; 4]);
    pg.push(table.len() as u8);
    pg.extend_from_slice(&table);
    pg.extend_from_slice(&data);
    pg
}

#[test]
fn ogg_vorbis_and_opus_comments() {
    let c = vorbis_comments(&[("TITLE", "Ogg"), ("ARTIST", "Vorbis")]);
    let mut vorbis = b"\x03vorbis".to_vec();
    vorbis.extend_from_slice(&c);
    vorbis.push(1);
    let mut f = ogg_page(0, 2, &[b"\x01vorbis-ident"], false);
    f.extend(ogg_page(1, 0, &[&vorbis], false));
    let t = read_tags(&f).unwrap();
    assert_eq!(
        (t.title.as_deref(), t.artist.as_deref()),
        (Some("Ogg"), Some("Vorbis"))
    );

    // Opus, with a picture in METADATA_BLOCK_PICTURE (base64) that spans several 255 segments.
    let pic = flac_picture("image/jpeg", &vec![0xd8; 700]);
    let b64 = base64_encode(&pic);
    let c = vorbis_comments(&[("title", "Opus"), ("METADATA_BLOCK_PICTURE", &b64)]);
    let mut tags = b"OpusTags".to_vec();
    tags.extend_from_slice(&c);
    let mut f = ogg_page(0, 2, &[b"OpusHead\x01\x02"], false);
    f.extend(ogg_page(1, 0, &[&tags], false));
    let t = read_tags(&f).unwrap();
    assert_eq!(t.title.as_deref(), Some("Opus"));
    assert_eq!(t.cover.unwrap().bytes.len(), 700);
}

fn base64_encode(b: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for ch in b.chunks(3) {
        let n = (u32::from(ch[0]) << 16)
            | (u32::from(*ch.get(1).unwrap_or(&0)) << 8)
            | u32::from(*ch.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= ch.len() {
                s.push(A[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

fn mp4_box(kind: &[u8], body: &[u8]) -> Vec<u8> {
    let mut b = ((body.len() + 8) as u32).to_be_bytes().to_vec();
    b.extend_from_slice(kind);
    b.extend_from_slice(body);
    b
}

fn mp4_data(flags: u32, payload: &[u8]) -> Vec<u8> {
    let mut b = flags.to_be_bytes().to_vec();
    b.extend_from_slice(&[0; 4]);
    b.extend_from_slice(payload);
    mp4_box(b"data", &b)
}

fn m4a_file() -> Vec<u8> {
    let ilst = [
        mp4_box(b"\xa9nam", &mp4_data(1, "M4A Song".as_bytes())),
        mp4_box(b"\xa9ART", &mp4_data(1, b"Maker")),
        mp4_box(b"\xa9alb", &mp4_data(1, b"Record")),
        mp4_box(b"aART", &mp4_data(1, b"Maker AA")),
        mp4_box(b"\xa9day", &mp4_data(1, b"2020-01-02")),
        mp4_box(b"\xa9gen", &mp4_data(1, b"Pop")),
        mp4_box(b"trkn", &mp4_data(0, &[0, 0, 0, 5, 0, 12, 0, 0])),
        mp4_box(b"disk", &mp4_data(0, &[0, 0, 0, 2, 0, 3])),
        mp4_box(b"covr", &mp4_data(13, JPEG)),
    ]
    .concat();
    let mut meta_body = vec![0u8; 4];
    meta_body.extend(mp4_box(b"ilst", &ilst));
    let udta = mp4_box(b"udta", &mp4_box(b"meta", &meta_body));
    let mut mvhd = vec![0u8; 100];
    mvhd[12..16].copy_from_slice(&1000u32.to_be_bytes());
    mvhd[16..20].copy_from_slice(&183_000u32.to_be_bytes());
    let moov = mp4_box(b"moov", &[mp4_box(b"mvhd", &mvhd), udta].concat());
    [
        mp4_box(b"ftyp", b"M4A \0\0\0\0M4A mp42"),
        mp4_box(b"mdat", &[0; 64]),
        moov,
    ]
    .concat()
}

#[test]
fn mp4_ilst() {
    let t = read_tags(&m4a_file()).unwrap();
    assert_eq!(t.title.as_deref(), Some("M4A Song"));
    assert_eq!(t.artist.as_deref(), Some("Maker"));
    assert_eq!(t.album.as_deref(), Some("Record"));
    assert_eq!(t.album_artist.as_deref(), Some("Maker AA"));
    assert_eq!((t.track, t.disc, t.year), (Some(5), Some(2), Some(2020)));
    assert_eq!(t.genre.as_deref(), Some("Pop"));
    assert_eq!(t.duration_hint, Some(183_000));
    let c = t.cover.unwrap();
    assert_eq!((c.mime.as_str(), c.bytes.as_slice()), ("image/jpeg", JPEG));
}

#[test]
fn mp4_with_moov_past_the_head_asks_for_more() {
    let f = m4a_file();
    let ftyp_and_cut = &f[..40];
    assert_eq!(read_tags(ftyp_and_cut), Err(TagError::Truncated));
}

#[test]
fn json_shape() {
    let t = read_tags(&flac_file()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&t.to_json()).unwrap();
    assert_eq!(v["albumArtist"], "Band");
    assert_eq!(v["coverMime"], "image/jpeg");
    assert_eq!(v["durationHint"], 200_000);
    assert!(v["disc"].is_number());
    let none: serde_json::Value = serde_json::from_str(&Tags::default().to_json()).unwrap();
    assert!(none["title"].is_null() && none["coverMime"].is_null());
}

#[test]
fn garbage_and_truncation_never_panic() {
    assert_eq!(read_tags(b""), Err(TagError::Unsupported));
    assert_eq!(
        read_tags(b"hello world, not audio"),
        Err(TagError::Unsupported)
    );
    assert_eq!(read_tags(b"ID3"), Err(TagError::Truncated));
    assert_eq!(read_tags(b"fLaC"), Err(TagError::Truncated));
    assert!(read_tags(b"OggS").is_err());
    // Every prefix of every sample, and bit-flipped copies, must be handled without panicking.
    let samples = [
        id3(
            4,
            0,
            &[
                frame(4, "TIT2", &text_body(1, "x"), 0),
                frame(4, "APIC", &apic(3, "image/png", JPEG), 0),
            ],
        ),
        flac_file(),
        m4a_file(),
        {
            let mut tags = b"OpusTags".to_vec();
            tags.extend(vorbis_comments(&[("TITLE", "t")]));
            let mut f = ogg_page(0, 2, &[b"OpusHead\x01\x02"], false);
            f.extend(ogg_page(1, 0, &[&tags], false));
            f
        },
    ];
    for s in &samples {
        for n in 0..=s.len() {
            let _ = read_tags(&s[..n]);
            let _ = read_tags_tail(&s[..n]);
        }
        let mut seed = 0x1234_5678u32;
        for _ in 0..3000 {
            let mut c = s.clone();
            for _ in 0..3 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let i = (seed >> 8) as usize % c.len();
                c[i] = (seed >> 24) as u8;
            }
            let _ = read_tags(&c);
        }
    }
    // Hostile sizes.
    let mut huge = b"ID3\x04\x00\x00".to_vec();
    huge.extend_from_slice(&syncsafe(100));
    huge.extend_from_slice(b"TIT2\x7f\x7f\x7f\x7f\0\0\x03abc");
    let _ = read_tags(&huge);
    let _ = read_tags(b"\0\0\0\x01ftyp\xff\xff\xff\xff\xff\xff\xff\xff");
    let _ = read_tags(b"\0\0\0\x00ftyp");
}

#[test]
fn lrc_basic_multi_stamp_and_sorting() {
    let l = parse_lrc(
        "\u{feff}[ar:Someone]\r\n[ti:Song]\r\n[00:12.50]Second\r\n[00:01.5][01:00]Chorus\r\nplain line\n[00:05]  Third  \n",
    );
    let got: Vec<(u64, &str)> = l.iter().map(|x| (x.ms, x.line.as_str())).collect();
    assert_eq!(
        got,
        vec![
            (1500, "Chorus"),
            (5000, "Third"),
            (12_500, "Second"),
            (60_000, "Chorus")
        ]
    );
}

#[test]
fn lrc_fraction_widths_offset_enhanced_and_blank_lines() {
    let l = parse_lrc(
        "[offset:500]\n[00:01.5]a\n[00:02.25]b\n[00:03.125]c\n[00:04:50]d\n[00:00.1]early\n[00:06.00]\n[00:07.00]<00:07.00>wo<00:07.50>rd\n",
    );
    let got: Vec<(u64, &str)> = l.iter().map(|x| (x.ms, x.line.as_str())).collect();
    assert_eq!(
        got,
        vec![
            (0, "early"),
            (1000, "a"),
            (1750, "b"),
            (2625, "c"),
            (4000, "d"),
            (5500, ""),
            (6500, "word"),
        ]
    );
}

#[test]
fn lrc_rejects_garbage_without_panicking() {
    assert!(parse_lrc("").is_empty());
    assert!(parse_lrc("just some words\nno stamps").is_empty());
    assert!(parse_lrc("[xx:yy]nope\n[00:99999]bad\n[12:]x\n[").is_empty());
    assert!(parse_lrc("[99999999999999999999:00]x").is_empty());
    let _ = parse_lrc("[00:01.1][00:02.2][[[]]]<<>>[00:03]é\u{1f600}<");
}

#[test]
fn sidecar_names() {
    assert_eq!(
        lrc_sidecar_names("music/a/b.mp3"),
        ["music/a/b.lrc", "music/a/b.LRC", "music/a/b.mp3.lrc"]
    );
    assert_eq!(lrc_sidecar_names("x.d/track"), ["x.d/track.lrc"]);
    assert_eq!(lrc_sidecar_names(".hidden"), [".hidden.lrc"]);
    assert_eq!(lrc_sidecar_names("a.b.flac")[0], "a.b.lrc");
}
