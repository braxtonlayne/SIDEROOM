//! Subtitle conversion (CIN-ING-008): ASS/SSA and SRT to WebVTT, and the scrubber sprite map.
//!
//! ASS files are retained as uploaded for styled rendering (JASSUB); the WebVTT produced here
//! is the plain fallback every player can show. The converter keeps what WebVTT can express
//! (italic, bold, underline, line breaks, top/middle/left/right placement and `\pos`) and drops
//! the rest (fonts, colours, karaoke, clips, vector drawings). Input is untrusted: size and cue
//! counts are capped and all text is escaped.

use crate::plan::SpritePlan;
use thiserror::Error;

pub const MAX_SUBTITLE_BYTES: usize = 20 * 1024 * 1024;
pub const MAX_CUES: usize = 200_000;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SubtitleError {
    #[error("subtitle file is larger than {MAX_SUBTITLE_BYTES} bytes")]
    TooLarge,
    #[error("too many cues (limit {MAX_CUES})")]
    TooManyCues,
    #[error("no readable cues")]
    NoCues,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Cue {
    start_ms: u64,
    end_ms: u64,
    settings: String,
    text: String,
}

/// `HH:MM:SS.mmm`.
pub fn format_timestamp(ms: u64) -> String {
    let h = ms / 3_600_000;
    let m = (ms / 60_000) % 60;
    let s = (ms / 1000) % 60;
    format!("{h:02}:{m:02}:{s:02}.{:03}", ms % 1000)
}

/// Parse `[H+:]MM:SS[,.]f{1,3}` (SRT, ASS and VTT variants) into milliseconds.
fn parse_timestamp(s: &str) -> Option<u64> {
    let s = s.trim();
    let (whole, frac) = match s.split_once([',', '.']) {
        Some((w, f)) => (w, f),
        None => (s, ""),
    };
    if frac.len() > 9 || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut ms_digits: String = frac.chars().take(3).collect();
    while ms_digits.len() < 3 {
        ms_digits.push('0');
    }
    let ms: u64 = ms_digits.parse().ok()?;
    let parts: Vec<&str> = whole.split(':').collect();
    let nums: Option<Vec<u64>> = parts
        .iter()
        .map(|p| {
            (!p.is_empty() && p.len() <= 6 && p.bytes().all(|b| b.is_ascii_digit()))
                .then(|| p.parse().ok())
                .flatten()
        })
        .collect();
    let nums = nums?;
    let (h, m, sec) = match nums.as_slice() {
        [m, s] => (0, *m, *s),
        [h, m, s] => (*h, *m, *s),
        _ => return None,
    };
    if m > 59 || sec > 59 {
        // Tolerate sloppy files such as 00:00:75.000 by carrying.
        if m > 5999 || sec > 599 {
            return None;
        }
    }
    Some(((h * 60 + m) * 60 + sec) * 1000 + ms)
}

/// Decode bytes of unknown encoding: UTF-8 (BOM stripped) or UTF-16 with a BOM.
fn decode(input: &[u8]) -> Result<String, SubtitleError> {
    if input.len() > MAX_SUBTITLE_BYTES {
        return Err(SubtitleError::TooLarge);
    }
    let utf16 = |bytes: &[u8], le: bool| {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| {
                if le {
                    u16::from_le_bytes(*c)
                } else {
                    u16::from_be_bytes(*c)
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    let text = match input {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, true),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, false),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ => String::from_utf8_lossy(input).into_owned(),
    };
    Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
}

/// Emits `<b><i><u>` tags, keeping them properly nested however the source toggles them.
#[derive(Default)]
struct StyleEmitter {
    out: String,
    open: (bool, bool, bool),
}

impl StyleEmitter {
    fn set(&mut self, want: (bool, bool, bool)) {
        if want == self.open {
            return;
        }
        self.close_all();
        if want.0 {
            self.out.push_str("<b>");
        }
        if want.1 {
            self.out.push_str("<i>");
        }
        if want.2 {
            self.out.push_str("<u>");
        }
        self.open = want;
    }
    fn close_all(&mut self) {
        if self.open.2 {
            self.out.push_str("</u>");
        }
        if self.open.1 {
            self.out.push_str("</i>");
        }
        if self.open.0 {
            self.out.push_str("</b>");
        }
        self.open = (false, false, false);
    }
    fn text(&mut self, c: char) {
        match c {
            '&' => self.out.push_str("&amp;"),
            '<' => self.out.push_str("&lt;"),
            '>' => self.out.push_str("&gt;"),
            '\u{a0}' => self.out.push_str("&nbsp;"),
            c if c.is_control() && c != '\n' => {}
            c => self.out.push(c),
        }
    }
    /// Close tags and return the text, trimmed, with no blank lines (a blank line would end
    /// the cue).
    fn finish(mut self) -> String {
        self.close_all();
        self.out
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Numpad alignment (1..=9) to WebVTT cue settings.
fn alignment_settings(an: u8) -> String {
    let mut parts: Vec<&str> = Vec::new();
    match an {
        7..=9 => parts.push("line:0"),
        4..=6 => parts.push("line:50%"),
        _ => {}
    }
    match an {
        1 | 4 | 7 => parts.push("align:left"),
        3 | 6 | 9 => parts.push("align:right"),
        _ => {}
    }
    parts.join(" ")
}

fn render_vtt(cues: &[Cue]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for c in cues {
        out.push_str(&format_timestamp(c.start_ms));
        out.push_str(" --> ");
        out.push_str(&format_timestamp(c.end_ms));
        if !c.settings.is_empty() {
            out.push(' ');
            out.push_str(&c.settings);
        }
        out.push('\n');
        out.push_str(&c.text);
        out.push_str("\n\n");
    }
    out
}

fn finish(mut cues: Vec<Cue>) -> Result<String, SubtitleError> {
    cues.retain(|c| c.end_ms > c.start_ms && !c.text.is_empty());
    if cues.is_empty() {
        return Err(SubtitleError::NoCues);
    }
    // Stable: cues starting together keep their file order (layers).
    cues.sort_by_key(|c| c.start_ms);
    Ok(render_vtt(&cues))
}

// ---- SRT ----------------------------------------------------------------------------------

/// Convert SRT to WebVTT. Malformed blocks are skipped; at least one cue must survive.
pub fn srt_to_webvtt(input: &[u8]) -> Result<String, SubtitleError> {
    let text = decode(input)?;
    let mut cues = Vec::new();
    let mut block: Vec<&str> = Vec::new();
    let flush = |block: &mut Vec<&str>, cues: &mut Vec<Cue>| {
        if let Some(cue) = parse_srt_block(block) {
            cues.push(cue);
        }
        block.clear();
    };
    for line in text.split('\n') {
        if line.trim().is_empty() {
            flush(&mut block, &mut cues);
        } else {
            block.push(line);
        }
        if cues.len() > MAX_CUES {
            return Err(SubtitleError::TooManyCues);
        }
    }
    flush(&mut block, &mut cues);
    finish(cues)
}

fn parse_srt_block(lines: &[&str]) -> Option<Cue> {
    let idx = lines.iter().take(2).position(|l| l.contains("-->"))?;
    let (a, rest) = lines[idx].split_once("-->")?;
    let start_ms = parse_timestamp(a)?;
    // The end timestamp may be followed by `X1:.. Y1:..` coordinates, which are dropped.
    let end_ms = parse_timestamp(rest.split_whitespace().next()?)?;
    let mut em = StyleEmitter::default();
    let mut style = (false, false, false);
    let mut an: Option<u8> = None;
    for (n, line) in lines[idx + 1..].iter().enumerate() {
        if n > 0 {
            em.text('\n');
        }
        srt_line(line, &mut em, &mut style, &mut an);
    }
    Some(Cue {
        start_ms,
        end_ms,
        settings: an.map(alignment_settings).unwrap_or_default(),
        text: em.finish(),
    })
}

fn srt_line(
    line: &str,
    em: &mut StyleEmitter,
    style: &mut (bool, bool, bool),
    an: &mut Option<u8>,
) {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '<'
            && let Some(len) = chars[i + 1..].iter().take(64).position(|&x| x == '>')
        {
            let inner: String = chars[i + 1..i + 1 + len].iter().collect();
            let closing = inner.starts_with('/');
            let name = inner
                .trim_start_matches('/')
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            if name.chars().next().is_some_and(|f| f.is_ascii_alphabetic()) {
                match name.as_str() {
                    "b" => style.0 = !closing,
                    "i" => style.1 = !closing,
                    "u" => style.2 = !closing,
                    _ => {} // font, ruby and anything else: dropped, text kept
                }
                em.set(*style);
                i += len + 2;
                continue;
            }
        }
        if c == '{'
            && let Some(len) = chars[i + 1..].iter().position(|&x| x == '}')
        {
            let inner: String = chars[i + 1..i + 1 + len].iter().collect();
            if let Some(pos) = inner.find("\\an")
                && let Some(d) = inner[pos + 3..].chars().next().and_then(|d| d.to_digit(10))
                && (1..=9).contains(&d)
            {
                *an = u8::try_from(d).ok();
            }
            i += len + 2;
            continue;
        }
        if c == '&' {
            let rest: String = chars[i..].iter().take(8).collect();
            if let Some(end) = rest.find(';')
                && matches!(
                    &rest[..=end],
                    "&amp;" | "&lt;" | "&gt;" | "&nbsp;" | "&lrm;" | "&rlm;"
                )
            {
                em.out.push_str(&rest[..=end]);
                i += end + 1;
                continue;
            }
        }
        em.set(*style);
        em.text(c);
        i += 1;
    }
}

// ---- ASS / SSA ----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct StyleInfo {
    bold: bool,
    italic: bool,
    underline: bool,
    alignment: u8,
}

impl Default for StyleInfo {
    fn default() -> Self {
        Self {
            bold: false,
            italic: false,
            underline: false,
            alignment: 2,
        }
    }
}

struct AssDoc {
    play_res: (f64, f64),
    styles: Vec<(String, StyleInfo)>,
}

fn ass_truthy(v: &str) -> bool {
    matches!(v.trim(), "-1" | "1") || v.trim().parse::<i32>().is_ok_and(|n| n != 0)
}

/// SSA (v4) alignment codes to numpad.
fn ssa_alignment(a: u8) -> u8 {
    match a {
        1..=3 => a,
        5..=7 => a + 2,
        9..=11 => a - 5,
        _ => 2,
    }
}

fn split_fields(line: &str) -> Vec<&str> {
    line.split(',').map(str::trim).collect()
}

/// Convert ASS/SSA to WebVTT.
pub fn ass_to_webvtt(input: &[u8]) -> Result<String, SubtitleError> {
    let text = decode(input)?;
    let mut doc = AssDoc {
        play_res: (384.0, 288.0),
        styles: Vec::new(),
    };
    let mut section = String::new();
    let mut style_format: Vec<String> = Vec::new();
    let mut event_format: Vec<String> = Vec::new();
    let mut ssa_v4 = false;
    let mut cues: Vec<Cue> = Vec::new();
    for raw in text.split('\n') {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line.to_ascii_lowercase();
            ssa_v4 = section == "[v4 styles]";
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim_start();
        match section.as_str() {
            "[script info]" => {
                let n = value.trim().parse::<f64>().ok().filter(|n| *n > 0.0);
                match (key.as_str(), n) {
                    ("playresx", Some(n)) => doc.play_res.0 = n,
                    ("playresy", Some(n)) => doc.play_res.1 = n,
                    _ => {}
                }
            }
            "[v4+ styles]" | "[v4 styles]" => match key.as_str() {
                "format" => {
                    style_format = split_fields(value)
                        .iter()
                        .map(|s| s.to_ascii_lowercase())
                        .collect();
                }
                "style" => {
                    let fields = split_fields(value);
                    let get = |name: &str| {
                        style_format
                            .iter()
                            .position(|f| f == name)
                            .and_then(|i| fields.get(i).copied())
                    };
                    let Some(name) = get("name") else { continue };
                    let alignment = get("alignment")
                        .and_then(|a| a.parse::<u8>().ok())
                        .map(|a| if ssa_v4 { ssa_alignment(a) } else { a })
                        .filter(|a| (1..=9).contains(a))
                        .unwrap_or(2);
                    doc.styles.push((
                        name.to_string(),
                        StyleInfo {
                            bold: get("bold").is_some_and(ass_truthy),
                            italic: get("italic").is_some_and(ass_truthy),
                            underline: get("underline").is_some_and(ass_truthy),
                            alignment,
                        },
                    ));
                }
                _ => {}
            },
            "[events]" => match key.as_str() {
                "format" => {
                    event_format = split_fields(value)
                        .iter()
                        .map(|s| s.to_ascii_lowercase())
                        .collect();
                }
                "dialogue" => {
                    if event_format.is_empty() {
                        event_format = [
                            "layer", "start", "end", "style", "name", "marginl", "marginr",
                            "marginv", "effect", "text",
                        ]
                        .iter()
                        .map(|s| (*s).to_string())
                        .collect();
                    }
                    if let Some(cue) = ass_dialogue(value, &event_format, &doc) {
                        cues.push(cue);
                        if cues.len() > MAX_CUES {
                            return Err(SubtitleError::TooManyCues);
                        }
                    }
                }
                _ => {} // Comment:, Picture:, Sound:, Movie:, Command: are not subtitles
            },
            _ => {}
        }
    }
    finish(cues)
}

fn ass_dialogue(value: &str, format: &[String], doc: &AssDoc) -> Option<Cue> {
    // The Text field is last and may itself contain commas.
    let fields: Vec<&str> = value.splitn(format.len(), ',').collect();
    let get = |name: &str| {
        format
            .iter()
            .position(|f| f == name)
            .and_then(|i| fields.get(i).copied())
    };
    let start_ms = parse_timestamp(get("start")?)?;
    let end_ms = parse_timestamp(get("end")?)?;
    let style_name = get("style").unwrap_or("Default").trim_start_matches('*');
    let style = doc
        .styles
        .iter()
        .find(|(n, _)| n == style_name)
        .or_else(|| doc.styles.iter().find(|(n, _)| n == "Default"))
        .map(|(_, s)| *s)
        .unwrap_or_default();
    let body = get("text")?;
    let (text, an, pos) = ass_text(body, style, doc.play_res);
    let mut settings: Vec<String> = Vec::new();
    match pos {
        Some((x, y)) => {
            let an = an.unwrap_or(style.alignment);
            let line_align = match an {
                7..=9 => "start",
                4..=6 => "center",
                _ => "end",
            };
            let pct = |v: f64, max: f64| (v / max * 100.0).clamp(0.0, 100.0).round();
            settings.push(format!(
                "line:{}% position:{}% line-align:{line_align} align:center",
                pct(y, doc.play_res.1),
                pct(x, doc.play_res.0)
            ));
        }
        None => {
            let s = alignment_settings(an.unwrap_or(style.alignment));
            if !s.is_empty() {
                settings.push(s);
            }
        }
    }
    Some(Cue {
        start_ms,
        end_ms,
        settings: settings.join(" "),
        text,
    })
}

/// Split an override block into tags, honouring parentheses (`\t(0,500,\fs50)` is one tag).
fn override_tags(block: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let mut cur = String::new();
    let mut depth = 0u32;
    let mut in_tag = false;
    for c in block.chars() {
        match c {
            '\\' if depth == 0 => {
                if in_tag && !cur.is_empty() {
                    tags.push(std::mem::take(&mut cur));
                }
                in_tag = true;
                cur.clear();
            }
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                cur.push(c);
            }
            _ if in_tag => cur.push(c),
            _ => {} // text before the first backslash is a comment
        }
    }
    if in_tag && !cur.is_empty() {
        tags.push(cur);
    }
    tags
}

fn numbers_in_parens(tag: &str) -> Vec<f64> {
    let inner = tag
        .split_once('(')
        .map(|(_, r)| r.trim_end_matches(')'))
        .unwrap_or("");
    inner
        .split(',')
        .filter_map(|n| n.trim().parse::<f64>().ok())
        .collect()
}

type AssText = (String, Option<u8>, Option<(f64, f64)>);

/// Convert one dialogue text: (webvtt text, `\an` override, `\pos` override).
fn ass_text(body: &str, base: StyleInfo, _res: (f64, f64)) -> AssText {
    let mut em = StyleEmitter::default();
    let mut st = (base.bold, base.italic, base.underline);
    let mut an = None;
    let mut pos = None;
    let mut drawing = false;
    let chars: Vec<char> = body.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '{'
            && let Some(len) = chars[i + 1..].iter().position(|&x| x == '}')
        {
            let block: String = chars[i + 1..i + 1 + len].iter().collect();
            for tag in override_tags(&block) {
                let t = tag.as_str();
                let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
                if let Some(d) = t.strip_prefix("an").filter(|d| digits(d)) {
                    an = d.parse::<u8>().ok().filter(|a| (1..=9).contains(a));
                } else if t.starts_with("pos(") || t.starts_with("move(") {
                    let n = numbers_in_parens(t);
                    if n.len() >= 2 {
                        pos = Some((n[0], n[1]));
                    }
                } else if let Some(v) = t.strip_prefix('i').filter(|v| digits(v)) {
                    st.1 = v != "0";
                } else if let Some(v) = t.strip_prefix('u').filter(|v| digits(v)) {
                    st.2 = v != "0";
                } else if let Some(v) = t.strip_prefix('b').filter(|v| digits(v)) {
                    st.0 = v.parse::<u32>().is_ok_and(|w| w == 1 || w >= 600);
                } else if let Some(v) = t.strip_prefix('p').filter(|v| digits(v)) {
                    drawing = v != "0";
                } else if t == "r" || (t.starts_with('r') && !t.starts_with("rnd")) {
                    st = (base.bold, base.italic, base.underline);
                }
            }
            i += len + 2;
            continue;
        }
        if c == '\\'
            && let Some(&next) = chars.get(i + 1)
        {
            match next {
                'N' => {
                    if !drawing {
                        em.text('\n');
                    }
                    i += 2;
                    continue;
                }
                'n' => {
                    if !drawing {
                        em.text(' ');
                    }
                    i += 2;
                    continue;
                }
                'h' => {
                    if !drawing {
                        em.set(st);
                        em.text('\u{a0}');
                    }
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        if !drawing {
            em.set(st);
            em.text(c);
        }
        i += 1;
    }
    (em.finish(), an, pos)
}

// ---- sprite map ---------------------------------------------------------------------------

/// WebVTT map for the scrubber thumbnails: one cue per interval pointing into a sheet with a
/// `#xywh=` media fragment. `sheet_url(n)` gives the URL for sheet `n` (1-based).
pub fn sprite_vtt(
    sprite: &SpritePlan,
    duration_secs: f64,
    sheet_url: &dyn Fn(u32) -> String,
) -> String {
    let per_sheet = (sprite.columns * sprite.rows).max(1);
    let duration_ms = (duration_secs * 1000.0).round().max(0.0) as u64;
    let interval_ms = u64::from(sprite.interval_secs.max(1)) * 1000;
    let mut out = String::from("WEBVTT\n\n");
    for i in 0..sprite.thumbs_total {
        let start = u64::from(i) * interval_ms;
        if start >= duration_ms {
            break;
        }
        let end = (start + interval_ms).min(duration_ms);
        let sheet = i / per_sheet + 1;
        let slot = i % per_sheet;
        let (col, row) = (slot % sprite.columns, slot / sprite.columns);
        out.push_str(&format!(
            "{} --> {}\n{}#xywh={},{},{},{}\n\n",
            format_timestamp(start),
            format_timestamp(end),
            sheet_url(sheet),
            col * sprite.thumb_width,
            row * sprite.thumb_height,
            sprite.thumb_width,
            sprite.thumb_height
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ass(events: &str) -> String {
        format!(
            "[Script Info]\nPlayResX: 1920\nPlayResY: 1080\n\n[V4+ Styles]\n\
             Format: Name, Fontname, Fontsize, PrimaryColour, Bold, Italic, Underline, Alignment\n\
             Style: Default,Arial,48,&H00FFFFFF,0,0,0,2\n\
             Style: Sign,Arial,48,&H00FFFFFF,-1,-1,0,8\n\n\
             [Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n{events}\n"
        )
    }

    #[test]
    fn timestamps() {
        assert_eq!(format_timestamp(3_723_456), "01:02:03.456");
        assert_eq!(parse_timestamp("0:01:02.50"), Some(62_500));
        assert_eq!(parse_timestamp("00:00:01,5"), Some(1500));
        assert_eq!(parse_timestamp("01:02"), Some(62_000));
        assert_eq!(parse_timestamp("100:00:00.000"), Some(360_000_000));
        assert_eq!(parse_timestamp("abc"), None);
        assert_eq!(parse_timestamp("1:2:3:4"), None);
    }

    #[test]
    fn srt_basic_with_tags_and_variants() {
        let srt = "\u{feff}1\r\n00:00:01,000 --> 00:00:02,500\r\nHello <i>world</i>\r\nsecond & line\r\n\r\n\
                   2\r\n00:00:03.000 --> 00:00:04.000 X1:100 X2:200\r\n<font color=\"red\">Red</font> <b>bold <u>both</b> x</u>\r\n\r\n\
                   garbage block\r\n\r\n\
                   3\r\n00:00:05,000 --> 00:00:06,000\r\n{\\an8}Top <3 &amp; ok\r\n";
        let vtt = srt_to_webvtt(srt.as_bytes()).unwrap();
        assert!(vtt.starts_with("WEBVTT\n\n"));
        assert!(
            vtt.contains("00:00:01.000 --> 00:00:02.500\nHello <i>world</i>\nsecond &amp; line\n")
        );
        assert!(vtt.contains(
            "00:00:03.000 --> 00:00:04.000\nRed <b>bold </b><b><u>both</u></b><u> x</u>\n"
        ));
        assert!(vtt.contains("00:00:05.000 --> 00:00:06.000 line:0\nTop &lt;3 &amp; ok\n"));
        assert!(!vtt.contains("garbage"));
        assert!(!vtt.contains("<font"));
        assert!(!vtt.contains("\r"));
    }

    #[test]
    fn srt_skips_empty_and_backwards_cues_and_sorts() {
        let srt = "1\n00:00:05,000 --> 00:00:06,000\nB\n\n2\n00:00:01,000 --> 00:00:02,000\nA\n\n\
                   3\n00:00:03,000 --> 00:00:03,000\nzero\n\n4\n00:00:07,000 --> 00:00:08,000\n   \n";
        let vtt = srt_to_webvtt(srt.as_bytes()).unwrap();
        let a = vtt.find("\nA\n").unwrap();
        let b = vtt.find("\nB\n").unwrap();
        assert!(a < b);
        assert!(!vtt.contains("zero"));
        assert_eq!(vtt.matches("-->").count(), 2);
    }

    #[test]
    fn srt_errors() {
        assert_eq!(srt_to_webvtt(b"nothing here"), Err(SubtitleError::NoCues));
        assert_eq!(
            srt_to_webvtt(&vec![b'a'; MAX_SUBTITLE_BYTES + 1]),
            Err(SubtitleError::TooLarge)
        );
    }

    #[test]
    fn srt_utf16_bom() {
        let s = "1\n00:00:01,000 --> 00:00:02,000\nHi\n";
        let mut bytes = vec![0xFF, 0xFE];
        for u in s.encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        assert!(srt_to_webvtt(&bytes).unwrap().contains("\nHi\n"));
    }

    #[test]
    fn ass_basic_commas_breaks_and_escaping() {
        let src = ass(
            "Dialogue: 0,0:00:01.00,0:00:03.50,Default,,0,0,0,,Hello, world\\NSecond <line> & more\\hx\n\
             Comment: 0,0:00:04.00,0:00:05.00,Default,,0,0,0,,not shown\n\
             Dialogue: 0,0:00:06.00,0:00:07.00,Default,,0,0,0,,{\\an8}Up top",
        );
        let vtt = ass_to_webvtt(src.as_bytes()).unwrap();
        assert!(vtt.contains(
            "00:00:01.000 --> 00:00:03.500\nHello, world\nSecond &lt;line&gt; &amp; more&nbsp;x\n"
        ));
        assert!(!vtt.contains("not shown"));
        assert!(vtt.contains("00:00:06.000 --> 00:00:07.000 line:0\nUp top\n"));
    }

    #[test]
    fn ass_override_tags_styles_and_reset() {
        let src = ass(
            "Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,{\\i1}tilt{\\i0} plain {\\b1\\u1}both{\\r} base\n\
             Dialogue: 0,0:00:03.00,0:00:04.00,Sign,,0,0,0,,styled bold italic {\\fs60\\c&H0000FF&\\blur2}colour ignored\n\
             Dialogue: 0,0:00:05.00,0:00:06.00,Default,,0,0,0,,a{\\t(0,500,\\i1)}b{\\fad(100,100)}c",
        );
        let vtt = ass_to_webvtt(src.as_bytes()).unwrap();
        assert!(vtt.contains("<i>tilt</i> plain <b><u>both</u></b> base\n"));
        // Style "Sign" is bold+italic and top aligned.
        assert!(vtt.contains("00:00:03.000 --> 00:00:04.000 line:0\n<b><i>styled bold italic colour ignored</i></b>\n"));
        // \t animation tags must not toggle italics.
        assert!(vtt.contains("\nabc\n"));
    }

    #[test]
    fn ass_pos_and_vector_drawings() {
        let src = ass(
            "Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,{\\pos(960,108)}Placed\n\
             Dialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,{\\p1}m 0 0 l 100 0 100 100{\\p0}Visible\n\
             Dialogue: 0,0:00:05.00,0:00:06.00,Default,,0,0,0,,{\\p1}m 0 0 l 1 1",
        );
        let vtt = ass_to_webvtt(src.as_bytes()).unwrap();
        assert!(vtt.contains("line:10% position:50% line-align:end align:center\nPlaced"));
        assert!(vtt.contains("\nVisible\n"));
        assert!(!vtt.contains("m 0 0"));
        assert_eq!(vtt.matches("-->").count(), 2);
    }

    #[test]
    fn ass_alignment_variants_and_ssa() {
        // V4 (SSA) alignment 6 is top-centre, 9 is middle-left? (9..=11 -> 4..=6).
        assert_eq!(ssa_alignment(6), 8);
        assert_eq!(ssa_alignment(9), 4);
        assert_eq!(ssa_alignment(2), 2);
        assert_eq!(alignment_settings(1), "align:left");
        assert_eq!(alignment_settings(9), "line:0 align:right");
        assert_eq!(alignment_settings(5), "line:50%");
        assert_eq!(alignment_settings(2), "");
        let src = "[Script Info]\n[V4 Styles]\nFormat: Name, Alignment\nStyle: Default,6\n\
                   [Events]\nFormat: Marked, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n\
                   Dialogue: Marked=0,0:00:01.00,0:00:02.00,Default,,0,0,0,,Top\n";
        let vtt = ass_to_webvtt(src.as_bytes()).unwrap();
        assert!(vtt.contains("00:00:01.000 --> 00:00:02.000 line:0\nTop"));
    }

    #[test]
    fn ass_timing_hours_sorting_and_dropping() {
        let src = ass("Dialogue: 0,1:02:03.45,1:02:04.00,Default,,0,0,0,,Late\n\
             Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,Early\n\
             Dialogue: 0,0:00:03.00,0:00:02.00,Default,,0,0,0,,Backwards\n\
             Dialogue: 0,0:00:04.00,0:00:05.00,Default,,0,0,0,,{\\an8}\n\
             Dialogue: broken line");
        let vtt = ass_to_webvtt(src.as_bytes()).unwrap();
        assert!(vtt.contains("01:02:03.450 --> 01:02:04.000\nLate"));
        assert!(vtt.find("Early").unwrap() < vtt.find("Late").unwrap());
        assert!(!vtt.contains("Backwards"));
        assert_eq!(vtt.matches("-->").count(), 2);
    }

    #[test]
    fn ass_unclosed_brace_is_literal_and_no_cues_errors() {
        let src = ass("Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,open { brace");
        assert!(
            ass_to_webvtt(src.as_bytes())
                .unwrap()
                .contains("open { brace")
        );
        assert_eq!(
            ass_to_webvtt(ass("").as_bytes()),
            Err(SubtitleError::NoCues)
        );
    }

    #[test]
    fn ass_output_never_has_blank_lines_inside_a_cue() {
        let src = ass("Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,a\\N\\N\\Nb");
        let vtt = ass_to_webvtt(src.as_bytes()).unwrap();
        assert!(vtt.contains("\na\nb\n\n"));
        assert!(!vtt.contains("-->\n\n"));
    }

    #[test]
    fn sprite_map() {
        let sprite = SpritePlan {
            interval_secs: 10,
            thumb_width: 160,
            thumb_height: 90,
            columns: 10,
            rows: 10,
            thumbs_total: 105,
            sheets: 2,
            sheet_pattern: "sprites/sheet-%03d.jpg".into(),
            vtt_path: "sprites/sprites.vtt".into(),
        };
        let vtt = sprite_vtt(&sprite, 1043.0, &|n| format!("sheet-{n:03}.jpg"));
        assert!(vtt.starts_with(
            "WEBVTT\n\n00:00:00.000 --> 00:00:10.000\nsheet-001.jpg#xywh=0,0,160,90\n"
        ));
        assert!(vtt.contains("00:00:10.000 --> 00:00:20.000\nsheet-001.jpg#xywh=160,0,160,90\n"));
        assert!(
            vtt.contains("00:16:30.000 --> 00:16:40.000\nsheet-001.jpg#xywh=1440,810,160,90\n")
        );
        // Cue 100 (index 100) starts sheet 2.
        assert!(vtt.contains("00:16:40.000 --> 00:16:50.000\nsheet-002.jpg#xywh=0,0,160,90\n"));
        // The last cue is clamped to the duration.
        assert!(vtt.contains("00:17:20.000 --> 00:17:23.000\nsheet-002.jpg#xywh=640,0,160,90\n"));
        assert_eq!(vtt.matches("-->").count(), 105);
    }
}
