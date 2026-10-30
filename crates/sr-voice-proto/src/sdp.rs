//! Two Opus payload types, one per kind of audio line (MUS-JB-001).
//!
//! The microphone stays mono Opus on payload type [`MONO_OPUS_PT`] (no `stereo` in its fmtp). The music track is
//! stereo Opus on [`STEREO_OPUS_PT`] (`stereo=1; sprop-stereo=1`). The SFU configures both payload types, and so
//! does a bot; what keeps them apart on the wire is the SDP: each audio m-line lists only the payload type of
//! what it carries. [`restrict_opus_lines`] rewrites an SDP that way (the SFU applies it to its offers, a bot to its
//! offer; browsers do the same to their own offer in `apps/web/src/voice/protocol.ts`). Pure text work: the media
//! is never touched (D-201).

use std::collections::HashSet;

/// Payload type of the mono Opus used by microphones and screen audio.
pub const MONO_OPUS_PT: u8 = 111;
/// Payload type of the stereo Opus used by the music track.
pub const STEREO_OPUS_PT: u8 = 112;

fn pt_of_attribute(line: &str) -> Option<u32> {
    let rest = line.strip_prefix("a=")?;
    let (name, tail) = rest.split_once(':')?;
    if !matches!(name, "rtpmap" | "fmtp" | "rtcp-fb") {
        return None;
    }
    let pt = tail.split([' ', '/']).next()?;
    pt.parse().ok()
}

/// Rewrite every audio m-line of `sdp` so that it lists one payload type: [`STEREO_OPUS_PT`] when its `a=mid` is in
/// `music_mids`, [`MONO_OPUS_PT`] otherwise. The payload types' `a=rtpmap`, `a=fmtp` and `a=rtcp-fb` lines of the
/// other payload types are dropped from that section. Sections without the wanted payload type, and every
/// non-audio section, are left alone, so the function is safe on any SDP. Line endings are kept (CRLF).
pub fn restrict_opus_lines(sdp: &str, music_mids: &HashSet<String>) -> String {
    let crlf = sdp.contains("\r\n");
    let eol = if crlf { "\r\n" } else { "\n" };
    let mut sections: Vec<Vec<&str>> = Vec::new();
    for line in sdp.split(eol) {
        if line.starts_with("m=") || sections.is_empty() {
            sections.push(Vec::new());
        }
        if let Some(s) = sections.last_mut() {
            s.push(line);
        }
    }
    let mut out: Vec<String> = Vec::new();
    for section in sections {
        let is_audio = section.first().is_some_and(|l| l.starts_with("m=audio"));
        if !is_audio {
            out.extend(section.iter().map(|l| (*l).to_owned()));
            continue;
        }
        let mid = section
            .iter()
            .find_map(|l| l.strip_prefix("a=mid:"))
            .map(str::trim);
        let want = if mid.is_some_and(|m| music_mids.contains(m)) {
            u32::from(STEREO_OPUS_PT)
        } else {
            u32::from(MONO_OPUS_PT)
        };
        let has_want = section
            .iter()
            .any(|l| l.starts_with("a=rtpmap:") && pt_of_attribute(l) == Some(want));
        if !has_want {
            out.extend(section.iter().map(|l| (*l).to_owned()));
            continue;
        }
        for line in section {
            if let Some(rest) = line.strip_prefix("m=audio ") {
                // m=audio <port> <proto> <pt>...
                let mut parts = rest.splitn(3, ' ');
                let (port, proto) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
                out.push(format!("m=audio {port} {proto} {want}"));
            } else if pt_of_attribute(line).is_some_and(|pt| pt != want) {
                // another payload type's attribute
            } else {
                out.push(line.to_owned());
            }
        }
    }
    out.join(eol)
}

/// The fmtp of each payload type, as the SFU and the bots configure them.
const MONO_FMTP: &str = "minptime=10;useinbandfec=1";
const STEREO_FMTP: &str = "minptime=10;useinbandfec=1;stereo=1;sprop-stereo=1";

/// The inverse of [`restrict_opus_lines`], for text that goes **into** str0m. str0m keeps one payload-type table per
/// connection and matches it against the payload types of each m-line; with two Opus entries (mono and stereo) it
/// needs to see both on every audio line, otherwise both entries claim the one payload type listed. So every audio
/// m-line that lists only one of the two payload types gets the other added (with its rtpmap and fmtp). Returns the
/// SDP and the mids of the lines that listed the stereo type alone or first: those carry music.
pub fn expand_opus_lines(sdp: &str) -> (String, HashSet<String>) {
    let crlf = sdp.contains("\r\n");
    let eol = if crlf { "\r\n" } else { "\n" };
    let mut sections: Vec<Vec<String>> = Vec::new();
    for line in sdp.split(eol) {
        if line.starts_with("m=") || sections.is_empty() {
            sections.push(Vec::new());
        }
        if let Some(s) = sections.last_mut() {
            s.push(line.to_owned());
        }
    }
    let mut music = HashSet::new();
    let (mono, stereo) = (u32::from(MONO_OPUS_PT), u32::from(STEREO_OPUS_PT));
    for section in &mut sections {
        if !section.first().is_some_and(|l| l.starts_with("m=audio ")) {
            continue;
        }
        let has = |section: &[String], pt: u32| {
            section
                .iter()
                .any(|l| l.starts_with("a=rtpmap:") && pt_of_attribute(l) == Some(pt))
        };
        let (has_mono, has_stereo) = (has(section, mono), has(section, stereo));
        let mid = section
            .iter()
            .find_map(|l| l.strip_prefix("a=mid:"))
            .map(|m| m.trim().to_owned());
        // The payload type listed first on the m-line says what the line carries.
        let first: Option<u32> = section[0].split(' ').nth(3).and_then(|p| p.parse().ok());
        if has_stereo
            && (!has_mono || first == Some(stereo))
            && let Some(m) = &mid
        {
            music.insert(m.clone());
        }
        let (have, add, fmtp) = match (has_mono, has_stereo) {
            (true, false) => (mono, stereo, STEREO_FMTP),
            (false, true) => (stereo, mono, MONO_FMTP),
            _ => continue,
        };
        let last = section
            .iter()
            .rposition(|l| pt_of_attribute(l) == Some(have))
            .unwrap_or(0);
        let twcc = section
            .iter()
            .any(|l| l.starts_with(&format!("a=rtcp-fb:{have} transport-cc")));
        let mut extra = vec![format!("a=rtpmap:{add} opus/48000/2")];
        if twcc {
            extra.push(format!("a=rtcp-fb:{add} transport-cc"));
        }
        extra.push(format!("a=fmtp:{add} {fmtp}"));
        for (i, l) in extra.into_iter().enumerate() {
            section.insert(last + 1 + i, l);
        }
        section[0].push_str(&format!(" {add}"));
    }
    (
        sections.into_iter().flatten().collect::<Vec<_>>().join(eol),
        music,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SDP: &str = "v=0\r\n\
m=audio 9 UDP/TLS/RTP/SAVPF 111 112\r\n\
a=mid:0\r\n\
a=rtpmap:111 opus/48000/2\r\n\
a=fmtp:111 minptime=10;useinbandfec=1\r\n\
a=rtpmap:112 opus/48000/2\r\n\
a=fmtp:112 minptime=10;useinbandfec=1;stereo=1;sprop-stereo=1\r\n\
a=rtcp-fb:112 transport-cc\r\n\
m=audio 9 UDP/TLS/RTP/SAVPF 111 112\r\n\
a=mid:1\r\n\
a=rtpmap:111 opus/48000/2\r\n\
a=fmtp:111 minptime=10;useinbandfec=1\r\n\
a=rtpmap:112 opus/48000/2\r\n\
a=fmtp:112 minptime=10;useinbandfec=1;stereo=1;sprop-stereo=1\r\n\
m=video 9 UDP/TLS/RTP/SAVPF 96\r\n\
a=mid:2\r\n\
a=rtpmap:96 VP8/90000\r\n";

    #[test]
    fn the_mic_line_keeps_only_the_mono_type_and_the_music_line_only_the_stereo_one() {
        let music: HashSet<String> = ["1".to_owned()].into();
        let out = restrict_opus_lines(SDP, &music);
        let sections: Vec<&str> = out.split("\r\nm=").collect();
        let (mic, song, video) = (sections[1], sections[2], sections[3]);
        assert!(mic.contains("SAVPF 111\r\n"), "{mic}");
        assert!(mic.contains("a=fmtp:111"));
        assert!(!mic.contains("112") && !mic.contains("stereo"), "{mic}");
        assert!(song.contains("SAVPF 112\r\n"), "{song}");
        assert!(song.contains("stereo=1") && song.contains("sprop-stereo=1"));
        assert!(!song.contains("111"), "{song}");
        assert!(video.contains("VP8"), "video untouched");
    }

    #[test]
    fn expanding_restores_both_types_and_finds_the_music_line() {
        let music: HashSet<String> = ["1".to_owned()].into();
        let narrow = restrict_opus_lines(SDP, &music);
        let (wide, found) = expand_opus_lines(&narrow);
        assert_eq!(
            found, music,
            "the line that listed the stereo type is music"
        );
        let sections: Vec<&str> = wide
            .split(
                "
m=",
            )
            .collect();
        for s in &sections[1..3] {
            assert!(
                s.contains("a=rtpmap:111 opus/48000/2") && s.contains("a=rtpmap:112 opus/48000/2"),
                "{s}"
            );
        }
        assert!(
            sections[1].contains("a=fmtp:112 minptime=10;useinbandfec=1;stereo=1;sprop-stereo=1")
        );
        assert!(sections[2].contains("a=fmtp:111 minptime=10;useinbandfec=1"));
        // And narrowing it again gives the same SDP as narrowing the original.
        assert_eq!(restrict_opus_lines(&wide, &music), narrow);
    }

    #[test]
    fn an_sdp_without_the_wanted_payload_type_is_left_alone() {
        let one =
            "v=0\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\na=mid:0\r\na=rtpmap:111 opus/48000/2\r\n";
        let music: HashSet<String> = ["0".to_owned()].into();
        assert_eq!(restrict_opus_lines(one, &music), one);
        assert_eq!(restrict_opus_lines(one, &HashSet::new()), one);
    }
}
