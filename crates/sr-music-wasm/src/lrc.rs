//! LRC lyrics: `[mm:ss.xx]line`, several stamps per line, enhanced word stamps, `[offset:]`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LrcLine {
    pub ms: u64,
    pub line: String,
}

/// `mm:ss`, `mm:ss.f`, `mm:ss.ff`, `mm:ss.fff` (also `:` before the fraction) to milliseconds.
fn parse_stamp(s: &str) -> Option<u64> {
    let (mm, rest) = s.split_once(':')?;
    if mm.is_empty() || mm.len() > 4 || !mm.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (ss, frac) = match rest.split_once(['.', ':']) {
        Some((a, b)) => (a, Some(b)),
        None => (rest, None),
    };
    if ss.is_empty() || ss.len() > 2 || !ss.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut ms = (mm.parse::<u64>().ok()? * 60 + ss.parse::<u64>().ok()?) * 1000;
    if let Some(f) = frac {
        if f.is_empty() || !f.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let f3: String = f.chars().take(3).collect();
        let v: u64 = f3.parse().ok()?;
        ms += v * 10u64.pow(3 - f3.len() as u32);
    }
    Some(ms)
}

/// Drop `<mm:ss.xx>` word stamps from a lyric line.
fn strip_word_stamps(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        let after = &rest[i + 1..];
        match after.find('>') {
            Some(j) if parse_stamp(&after[..j]).is_some() => {
                out.push_str(&rest[..i]);
                rest = &after[j + 1..];
            }
            _ => {
                out.push_str(&rest[..=i]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Parse LRC text. Lines without a time stamp (metadata, plain text) are skipped; the result is
/// sorted by time (stable, so equal stamps keep their order). Plain unsynced lyrics give `[]`.
pub fn parse_lrc(text: &str) -> Vec<LrcLine> {
    let mut offset: i64 = 0;
    let mut out: Vec<LrcLine> = Vec::new();
    for raw in text.trim_start_matches('\u{feff}').lines() {
        let mut rest = raw.trim_start();
        let mut stamps: Vec<u64> = Vec::new();
        while let Some(inner) = rest.strip_prefix('[') {
            let Some(close) = inner.find(']') else {
                break;
            };
            let tag = &inner[..close];
            if let Some(ms) = parse_stamp(tag.trim()) {
                stamps.push(ms);
            } else if let Some((k, v)) = tag.split_once(':') {
                if k.trim().eq_ignore_ascii_case("offset")
                    && let Ok(o) = v.trim().parse::<i64>()
                {
                    offset = o;
                }
                // Other metadata tags ([ar:], [ti:] ...) carry no lyric line.
                if stamps.is_empty() {
                    rest = "";
                    break;
                }
            } else {
                break;
            }
            rest = inner[close + 1..].trim_start_matches([' ', '\t']);
        }
        if stamps.is_empty() {
            continue;
        }
        let line = strip_word_stamps(rest).trim().to_string();
        for ms in stamps {
            out.push(LrcLine {
                ms: ms.saturating_add_signed(-offset),
                line: line.clone(),
            });
        }
    }
    out.sort_by_key(|l| l.ms);
    out
}

/// The sidecar keys to look up for a track key, best first. A positive `[offset:]` is applied by
/// the parser, not here.
pub fn lrc_sidecar_names(track_key: &str) -> Vec<String> {
    let slash = track_key.rfind('/').map_or(0, |i| i + 1);
    let name = &track_key[slash..];
    let mut out = Vec::new();
    if let Some(dot) = name.rfind('.').filter(|d| *d > 0) {
        let stem = &track_key[..slash + dot];
        out.push(format!("{stem}.lrc"));
        out.push(format!("{stem}.LRC"));
    }
    out.push(format!("{track_key}.lrc"));
    out
}
