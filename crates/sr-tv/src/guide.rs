//! Guide import (TV-ADM-004): parse an uploaded CSV or JSON weekly guide, match its show names to library
//! titles and build the mapping report staff confirm. Pure functions; the service stores the result.
//!
//! Columns (header row required for CSV, keys for JSON): `weekday` (`Mon`..`Sun`, or ISO `1`..`7`),
//! `start_time` (`20:00` or `8:00 PM`), `duration` (minutes, or `H:MM`), `show`, and the optional `season`,
//! `episode`, `note`. Times are in the channel time zone.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::catalog::TitleRef;
use crate::model::{Slot, parse_hm};

/// Largest guide accepted (rows).
pub const MAX_ROWS: usize = 1000;
/// Candidates shown for an ambiguous or missing row.
const MAX_CANDIDATES: usize = 5;
/// A single best match at or above this score, clearly ahead of the runner-up, is taken as matched.
const CONFIDENT: f64 = 0.92;
/// Below this a title is not offered at all.
const OFFER: f64 = 0.55;
/// The best match must lead the second by this much to count as unambiguous.
const MARGIN: f64 = 0.05;

pub const CSV_TEMPLATE: &str = "weekday,start_time,duration,show,season,episode,note\n\
Mon,20:00,30,Cowboy Bebop,1,1,Opening night\n\
Mon,20:30,30,Samurai Champloo,,,\n\
Tue,22:00,60,The Big Movie (1999),,,Feature\n";

pub const JSON_TEMPLATE: &str = r#"[
  {"weekday": "Mon", "start_time": "20:00", "duration": 30, "show": "Cowboy Bebop", "season": 1, "episode": 1, "note": "Opening night"},
  {"weekday": "Mon", "start_time": "20:30", "duration": 30, "show": "Samurai Champloo"},
  {"weekday": "Tue", "start_time": "22:00", "duration": 60, "show": "The Big Movie (1999)", "note": "Feature"}
]
"#;

/// One parsed row, before matching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuideRow {
    /// 1-based line (CSV) or entry number (JSON).
    pub line: usize,
    pub weekday: u8,
    pub start_minute: u16,
    pub duration_min: u16,
    pub show: String,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub note: Option<String>,
}

/// A row that could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = TvGuideProblem)]
pub struct GuideProblem {
    pub line: usize,
    pub message: String,
}

type Cells = BTreeMap<String, String>;

/// Parse an upload. `format` is `csv` or `json`. Rows that cannot be read are returned as problems, never
/// as an error; only an unreadable file is an `Err`.
pub fn parse(format: &str, content: &str) -> Result<(Vec<GuideRow>, Vec<GuideProblem>), String> {
    let raw = match format {
        "csv" => csv_rows(content)?,
        "json" => json_rows(content)?,
        _ => return Err("format must be csv or json".into()),
    };
    if raw.len() > MAX_ROWS {
        return Err(format!("A guide has at most {MAX_ROWS} rows"));
    }
    let mut rows = Vec::new();
    let mut problems = Vec::new();
    for (line, cells) in raw {
        match row_of(line, &cells) {
            Ok(r) => rows.push(r),
            Err(message) => problems.push(GuideProblem { line, message }),
        }
    }
    Ok((rows, problems))
}

fn canon_key(k: &str) -> String {
    let lowered = k.trim().to_lowercase().replace([' ', '-'], "_");
    match lowered.as_str() {
        "weekday" | "day" | "day_of_week" => "weekday".to_owned(),
        "start_time" | "start" | "time" | "starts" => "start_time".to_owned(),
        "duration" | "duration_min" | "duration_minutes" | "length" | "minutes" => {
            "duration".to_owned()
        }
        "show" | "title" | "program" | "programme" | "name" => "show".to_owned(),
        "season" | "s" => "season".to_owned(),
        "episode" | "ep" | "e" => "episode".to_owned(),
        "note" | "notes" | "comment" => "note".to_owned(),
        _ => lowered,
    }
}

fn csv_rows(content: &str) -> Result<Vec<(usize, Cells)>, String> {
    let content = content.trim_start_matches('\u{feff}');
    let records = csv_records(content)?;
    let mut it = records.into_iter();
    let Some((_, header)) = it.next() else {
        return Err("The file is empty".into());
    };
    let keys: Vec<String> = header.iter().map(|h| canon_key(h)).collect();
    for need in ["weekday", "start_time", "duration", "show"] {
        if !keys.iter().any(|k| k == need) {
            return Err(format!("The header row needs a {need} column"));
        }
    }
    let mut out = Vec::new();
    for (line, cells) in it {
        if cells.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let mut m = Cells::new();
        for (k, v) in keys.iter().zip(cells) {
            m.insert(k.clone(), v);
        }
        out.push((line, m));
    }
    Ok(out)
}

/// RFC 4180 records with their starting line. Quotes may hold commas, quotes (doubled) and newlines.
fn csv_records(s: &str) -> Result<Vec<(usize, Vec<String>)>, String> {
    let mut out = Vec::new();
    let mut rec: Vec<String> = Vec::new();
    let mut cell = String::new();
    let (mut quoted, mut line, mut rec_line) = (false, 1usize, 1usize);
    let mut chars = s.chars().peekable();
    let mut started = false;
    while let Some(c) = chars.next() {
        started = true;
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    cell.push('"');
                }
                '"' => quoted = false,
                '\n' => {
                    line += 1;
                    cell.push('\n');
                }
                _ => cell.push(c),
            }
            continue;
        }
        match c {
            '"' if cell.is_empty() => quoted = true,
            ',' => rec.push(std::mem::take(&mut cell)),
            '\r' => {}
            '\n' => {
                rec.push(std::mem::take(&mut cell));
                out.push((rec_line, std::mem::take(&mut rec)));
                line += 1;
                rec_line = line;
                started = false;
            }
            _ => cell.push(c),
        }
    }
    if quoted {
        return Err("A quoted value is not closed".into());
    }
    if started {
        rec.push(cell);
        out.push((rec_line, rec));
    }
    Ok(out)
}

fn json_rows(content: &str) -> Result<Vec<(usize, Cells)>, String> {
    let v: serde_json::Value =
        serde_json::from_str(content).map_err(|e| format!("Not valid JSON: {e}"))?;
    let list = match v {
        serde_json::Value::Array(a) => a,
        serde_json::Value::Object(mut o) => match o.remove("slots") {
            Some(serde_json::Value::Array(a)) => a,
            _ => return Err("Expected an array of slots".into()),
        },
        _ => return Err("Expected an array of slots".into()),
    };
    let mut out = Vec::new();
    for (i, e) in list.into_iter().enumerate() {
        let serde_json::Value::Object(o) = e else {
            out.push((i + 1, Cells::new()));
            continue;
        };
        let mut m = Cells::new();
        for (k, v) in o {
            let text = match v {
                serde_json::Value::String(s) => s,
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Null => String::new(),
                other => other.to_string(),
            };
            m.insert(canon_key(&k), text);
        }
        out.push((i + 1, m));
    }
    Ok(out)
}

fn weekday_of(s: &str) -> Option<u8> {
    let t = s.trim().to_lowercase();
    let t = t.trim_end_matches('.');
    const NAMES: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
    if t.chars().all(|c| c.is_ascii_alphabetic())
        && let Some(i) = NAMES.iter().position(|n| t.starts_with(n))
    {
        return u8::try_from(i).ok();
    }
    match t.parse::<u8>() {
        Ok(n @ 1..=7) => Some(n - 1),
        _ => None,
    }
}

fn time_of(s: &str) -> Option<u16> {
    let t = s.trim().to_lowercase();
    let (body, pm) = if let Some(b) = t.strip_suffix("pm") {
        (b.trim(), Some(true))
    } else if let Some(b) = t.strip_suffix("am") {
        (b.trim(), Some(false))
    } else {
        (t.as_str(), None)
    };
    let (h, m) = body.split_once(':').unwrap_or((body, "00"));
    let (mut h, m): (u16, u16) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
    if m > 59 {
        return None;
    }
    match pm {
        Some(pm) => {
            if !(1..=12).contains(&h) {
                return None;
            }
            h %= 12;
            if pm {
                h += 12;
            }
        }
        None if h > 23 => return None,
        None => {}
    }
    Some(h * 60 + m)
}

fn minutes_of(s: &str) -> Option<u16> {
    let t = s.trim();
    let v: u32 = if let Some((h, m)) = t.split_once(':') {
        let (h, m): (u32, u32) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
        if m > 59 {
            return None;
        }
        h * 60 + m
    } else {
        t.trim_end_matches("min").trim().parse().ok()?
    };
    u16::try_from(v).ok().filter(|v| (1..=1440).contains(v))
}

fn row_of(line: usize, c: &Cells) -> Result<GuideRow, String> {
    let get = |k: &str| c.get(k).map_or("", |s| s.trim());
    let weekday = weekday_of(get("weekday"))
        .ok_or_else(|| format!("Unknown weekday {:?}", get("weekday")))?;
    let start_minute = time_of(get("start_time"))
        .ok_or_else(|| format!("Unreadable start time {:?}", get("start_time")))?;
    let duration_min = minutes_of(get("duration"))
        .ok_or_else(|| format!("Unreadable duration {:?}", get("duration")))?;
    let show = get("show").to_owned();
    if show.is_empty() || show.chars().count() > 200 {
        return Err("The show needs 1 to 200 characters".into());
    }
    let num = |k: &str| -> Result<Option<i32>, String> {
        match get(k) {
            "" => Ok(None),
            v => v
                .parse::<i32>()
                .ok()
                .filter(|n| (0..=9999).contains(n))
                .map(Some)
                .ok_or_else(|| format!("{k} must be a number")),
        }
    };
    let note = Some(get("note").to_owned()).filter(|n| !n.is_empty());
    if note.as_ref().is_some_and(|n| n.chars().count() > 200) {
        return Err("The note is at most 200 characters".into());
    }
    Ok(GuideRow {
        line,
        weekday,
        start_minute,
        duration_min,
        show,
        season: num("season")?,
        episode: num("episode")?,
        note,
    })
}

// ---- matching -----------------------------------------------------------------------------------

/// Lower case, `&` as "and", punctuation as spaces, a leading article dropped.
pub fn normalise(s: &str) -> String {
    let lowered = s.to_lowercase().replace('&', " and ");
    let words: Vec<&str> = lowered
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let words: &[&str] = match words.as_slice() {
        ["the" | "a" | "an", rest @ ..] if !rest.is_empty() => rest,
        all => all,
    };
    words.join(" ")
}

fn bigrams(s: &str) -> Vec<(char, char)> {
    let c: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
    c.windows(2).map(|w| (w[0], w[1])).collect()
}

/// 0.0 to 1.0: 1.0 for equal normalised names, else the better of a bigram Dice coefficient and a
/// whole-word containment score (so "Cowboy Bebop" finds "Cowboy Bebop: The Movie" only as a candidate).
pub fn similarity(a: &str, b: &str) -> f64 {
    let (a, b) = (normalise(a), normalise(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    let (ga, gb_all) = (bigrams(&a), bigrams(&b));
    let dice = if ga.is_empty() || gb_all.is_empty() {
        0.0
    } else {
        let mut gb = gb_all.clone();
        let mut hit = 0usize;
        for g in &ga {
            if let Some(i) = gb.iter().position(|x| x == g) {
                gb.swap_remove(i);
                hit += 1;
            }
        }
        2.0 * hit as f64 / (ga.len() + gb_all.len()) as f64
    };
    let (short, long) = if a.len() <= b.len() {
        (&a, &b)
    } else {
        (&b, &a)
    };
    let padded = format!(" {long} ");
    let contained = if padded.contains(&format!(" {short} ")) {
        0.6 + 0.3 * short.len() as f64 / long.len() as f64
    } else {
        0.0
    };
    dice.max(contained).min(0.99)
}

/// Split a trailing `(1999)` off a show name.
fn split_year(show: &str) -> (&str, Option<i16>) {
    let t = show.trim_end();
    if let Some(rest) = t.strip_suffix(')')
        && let Some((name, year)) = rest.rsplit_once('(')
        && let Ok(y) = year.trim().parse::<i16>()
        && (1800..=2200).contains(&y)
    {
        return (name.trim_end(), Some(y));
    }
    (t, None)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = TvGuideCandidate)]
pub struct Candidate {
    #[serde(with = "crate::model::id")]
    #[schema(value_type = String)]
    pub title_id: i64,
    pub title: String,
    pub year: Option<i16>,
    pub score: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = TvGuideStatus)]
pub enum Status {
    Matched,
    Ambiguous,
    Missing,
    Invalid,
}

/// One row of the mapping report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = TvGuideReportRow)]
pub struct ReportRow {
    pub line: usize,
    pub weekday: u8,
    /// `HH:MM`.
    pub start: String,
    pub duration_min: u16,
    pub show: String,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub note: Option<String>,
    pub status: Status,
    /// The chosen title when matched.
    #[serde(default, with = "crate::model::opt_id")]
    #[schema(value_type = Option<String>)]
    pub title_id: Option<i64>,
    /// Near matches, best first (ambiguous and missing rows).
    pub candidates: Vec<Candidate>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = TvGuideReport)]
pub struct Report {
    pub rows: Vec<ReportRow>,
    pub matched: usize,
    pub ambiguous: usize,
    pub missing: usize,
    pub invalid: usize,
}

fn candidates_for(show: &str, hint_year: Option<i16>, titles: &[TitleRef]) -> Vec<Candidate> {
    let mut v: Vec<Candidate> = titles
        .iter()
        .map(|t| {
            let mut score = similarity(show, &t.title);
            if let (Some(h), Some(y)) = (hint_year, t.year) {
                if h == y {
                    score = (score + 0.05).min(1.0);
                } else {
                    score -= 0.2;
                }
            }
            Candidate {
                title_id: t.title_id,
                title: t.title.clone(),
                year: t.year,
                score: (score * 1000.0).round() / 1000.0,
            }
        })
        .filter(|c| c.score >= OFFER)
        .collect();
    v.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(a.title_id.cmp(&b.title_id))
    });
    v.truncate(MAX_CANDIDATES);
    v
}

/// Whether two weekly intervals (minutes since Monday 00:00, may run past the end of the week) overlap.
pub(crate) fn spans_overlap(a: (i32, i32), b: (i32, i32)) -> bool {
    const WEEK: i32 = 7 * 1440;
    [-WEEK, 0, WEEK]
        .iter()
        .any(|shift| a.0 < b.1 + shift && b.0 + shift < a.1)
}

/// Build the mapping report: parse problems become invalid rows, overlapping rows are invalid, and every
/// other row is matched, ambiguous or missing against `titles`.
pub fn build_report(rows: &[GuideRow], problems: &[GuideProblem], titles: &[TitleRef]) -> Report {
    let mut out: Vec<ReportRow> = Vec::new();
    let mut taken: Vec<(usize, (i32, i32))> = Vec::new();
    for r in rows {
        let start = i32::from(r.weekday) * 1440 + i32::from(r.start_minute);
        let span = (start, start + i32::from(r.duration_min));
        let clash = taken.iter().find(|(_, t)| spans_overlap(span, *t));
        let (name, year) = split_year(&r.show);
        let mut row = ReportRow {
            line: r.line,
            weekday: r.weekday,
            start: format!("{:02}:{:02}", r.start_minute / 60, r.start_minute % 60),
            duration_min: r.duration_min,
            show: r.show.clone(),
            season: r.season,
            episode: r.episode,
            note: r.note.clone(),
            status: Status::Missing,
            title_id: None,
            candidates: Vec::new(),
            message: None,
        };
        if let Some((line, _)) = clash {
            row.status = Status::Invalid;
            row.message = Some(format!("Overlaps line {line}"));
            out.push(row);
            continue;
        }
        taken.push((r.line, span));
        let cands = candidates_for(name, year, titles);
        match cands.as_slice() {
            [] => row.message = Some("Not in the library: this slot will be skipped".into()),
            [best, rest @ ..] => {
                let lead = rest.first().is_none_or(|n| best.score - n.score >= MARGIN);
                if best.score >= CONFIDENT && lead {
                    row.status = Status::Matched;
                    row.title_id = Some(best.title_id);
                    row.candidates = vec![best.clone()];
                } else {
                    row.status = Status::Ambiguous;
                    row.candidates = cands.clone();
                    row.message = Some("Pick the right title, or leave it to skip the slot".into());
                }
            }
        }
        out.push(row);
    }
    for p in problems {
        out.push(ReportRow {
            line: p.line,
            weekday: 0,
            start: String::new(),
            duration_min: 0,
            show: String::new(),
            season: None,
            episode: None,
            note: None,
            status: Status::Invalid,
            title_id: None,
            candidates: Vec::new(),
            message: Some(p.message.clone()),
        });
    }
    out.sort_by_key(|r| r.line);
    let count = |s: Status| out.iter().filter(|r| r.status == s).count();
    Report {
        matched: count(Status::Matched),
        ambiguous: count(Status::Ambiguous),
        missing: count(Status::Missing),
        invalid: count(Status::Invalid),
        rows: out,
    }
}

/// The slots a confirmed report becomes. `choices` maps a line to the title staff picked for an ambiguous
/// row (`None` keeps it skipped). Matched rows use their match; others stay without a title.
pub fn slots_from_report(
    report: &Report,
    choices: &BTreeMap<usize, Option<i64>>,
    titles: &[TitleRef],
) -> Result<Vec<Slot>, String> {
    let mut out = Vec::new();
    for r in &report.rows {
        if r.status == Status::Invalid {
            continue;
        }
        let title_id = match (r.status, choices.get(&r.line)) {
            (_, Some(Some(id))) => {
                if !titles.iter().any(|t| t.title_id == *id) {
                    return Err(format!("Line {}: title {id} is not in the library", r.line));
                }
                Some(*id)
            }
            (_, Some(None)) => None,
            (Status::Matched, None) => r.title_id,
            _ => None,
        };
        if parse_hm(&r.start).is_none() {
            return Err(format!("Line {}: invalid start time", r.line));
        }
        out.push(Slot {
            id: None,
            weekday: r.weekday,
            start: r.start.clone(),
            duration_min: r.duration_min,
            show_name: r.show.clone(),
            title_id,
            episode_id: None,
            season_number: r.season,
            episode_number: r.episode,
            note: r.note.clone(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MediaKind;

    fn t(id: i64, title: &str, year: i16) -> TitleRef {
        TitleRef {
            title_id: id,
            title: title.into(),
            year: Some(year),
            media_kind: MediaKind::Series,
        }
    }

    fn library() -> Vec<TitleRef> {
        vec![
            t(1, "Cowboy Bebop", 1998),
            t(2, "Samurai Champloo", 2004),
            t(3, "The Big Movie", 1999),
            t(4, "The Big Movie", 2015),
            t(5, "Dragon Ball", 1986),
            t(6, "Dragon Ball Z", 1989),
        ]
    }

    fn row(line: usize, wd: u8, start: u16, dur: u16, show: &str) -> GuideRow {
        GuideRow {
            line,
            weekday: wd,
            start_minute: start,
            duration_min: dur,
            show: show.into(),
            season: None,
            episode: None,
            note: None,
        }
    }

    #[test]
    fn csv_parses_quotes_times_and_synonyms() {
        let csv = "Day,Start,Length,Title,Notes\nMon,8:00 PM,0:30,\"Cowboy, Bebop\",\"say \"\"hi\"\"\"\n\ntue,22:00,60,Samurai Champloo,\n";
        let (rows, problems) = parse("csv", csv).unwrap();
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(rows.len(), 2);
        assert_eq!(
            (rows[0].weekday, rows[0].start_minute, rows[0].duration_min),
            (0, 20 * 60, 30)
        );
        assert_eq!(rows[0].show, "Cowboy, Bebop");
        assert_eq!(rows[0].note.as_deref(), Some("say \"hi\""));
        assert_eq!(rows[1].line, 4);
        assert_eq!(rows[1].weekday, 1);
    }

    #[test]
    fn bad_rows_are_problems_and_a_bad_header_is_an_error() {
        let (rows, problems) = parse(
            "csv",
            "weekday,start_time,duration,show\nFunday,20:00,30,X\nMon,25:00,30,X\nMon,20:00,0,X\nMon,20:00,30,\nSun,23:30,60,Ok\n",
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(problems.len(), 4);
        assert!(problems[0].message.contains("weekday"));
        assert!(
            parse("csv", "weekday,show\nMon,X\n")
                .unwrap_err()
                .contains("start_time")
        );
        assert!(parse("csv", "").is_err());
        assert!(parse("csv", "a,b\n\"open").is_err());
        assert!(parse("xml", "x").is_err());
    }

    #[test]
    fn json_accepts_an_array_or_a_slots_object() {
        let a = r#"[{"weekday":1,"start_time":"20:00","duration":30,"show":"Cowboy Bebop","season":1}]"#;
        let (rows, _) = parse("json", a).unwrap();
        assert_eq!((rows[0].weekday, rows[0].season), (0, Some(1)));
        let b = format!("{{\"slots\":{a}}}");
        assert_eq!(parse("json", &b).unwrap().0.len(), 1);
        assert!(parse("json", "{\"x\":1}").is_err());
        assert!(parse("json", "nope").is_err());
        let (rows, problems) = parse("json", "[3]").unwrap();
        assert!(rows.is_empty() && problems.len() == 1);
    }

    #[test]
    fn templates_parse_and_match() {
        for (f, body) in [("csv", CSV_TEMPLATE), ("json", JSON_TEMPLATE)] {
            let (rows, problems) = parse(f, body).unwrap();
            assert!(problems.is_empty(), "{f}: {problems:?}");
            assert_eq!(rows.len(), 3);
            let rep = build_report(&rows, &problems, &library());
            assert_eq!(rep.matched, 3, "{f}: {rep:?}");
        }
    }

    #[test]
    fn matching_separates_matched_ambiguous_and_missing() {
        let rows: Vec<GuideRow> = [
            "Cowboy Bebop",
            "the cowboy bebop!",
            "The Big Movie",
            "The Big Movie (2015)",
            "Dragon",
            "Totally Unknown Show",
        ]
        .iter()
        .enumerate()
        .map(|(i, s)| row(i + 2, 0, u16::try_from(i * 60).unwrap(), 30, s))
        .collect();
        let rep = build_report(&rows, &[], &library());
        let st: Vec<_> = rep.rows.iter().map(|r| r.status).collect();
        assert_eq!(
            st,
            [
                Status::Matched,
                Status::Matched,
                Status::Ambiguous,
                Status::Matched,
                Status::Ambiguous,
                Status::Missing
            ]
        );
        assert_eq!(rep.rows[3].title_id, Some(4), "the year picks the remake");
        assert_eq!(
            rep.rows[2].candidates.len(),
            2,
            "two titles of the same name"
        );
        assert!(rep.rows[4].candidates.iter().any(|c| c.title_id == 5));
        assert_eq!(
            (rep.matched, rep.ambiguous, rep.missing, rep.invalid),
            (3, 2, 1, 0)
        );
    }

    #[test]
    fn overlapping_rows_and_parse_problems_are_invalid() {
        // Line 3 starts inside line 2; line 4 wraps from Sunday night into Monday morning, over line 5.
        let rows = vec![
            row(2, 0, 600, 60, "Cowboy Bebop"),
            row(3, 0, 630, 30, "Cowboy Bebop"),
            row(4, 6, 1400, 120, "Cowboy Bebop"),
            row(5, 0, 30, 30, "Cowboy Bebop"),
        ];
        let problems = [GuideProblem {
            line: 6,
            message: "bad".into(),
        }];
        let rep = build_report(&rows, &problems, &library());
        let st: Vec<_> = rep.rows.iter().map(|r| r.status).collect();
        assert_eq!(
            st,
            [
                Status::Matched,
                Status::Invalid,
                Status::Matched,
                Status::Invalid,
                Status::Invalid
            ]
        );
        assert_eq!(rep.rows[1].message.as_deref(), Some("Overlaps line 2"));
        assert_eq!(rep.invalid, 3);
    }

    #[test]
    fn confirmation_applies_choices_and_skips_the_rest() {
        let mut first = row(2, 0, 0, 30, "Cowboy Bebop");
        first.season = Some(1);
        first.episode = Some(2);
        let rows = vec![
            first,
            row(3, 0, 60, 30, "The Big Movie"),
            row(4, 0, 120, 30, "Nope"),
        ];
        let rep = build_report(&rows, &[], &library());
        let mut choices = BTreeMap::new();
        let slots = slots_from_report(&rep, &choices, &library()).unwrap();
        assert_eq!(
            slots.iter().map(|s| s.title_id).collect::<Vec<_>>(),
            [Some(1), None, None]
        );
        assert_eq!(
            (slots[0].season_number, slots[0].episode_number),
            (Some(1), Some(2))
        );
        choices.insert(3, Some(4));
        choices.insert(2, None);
        let slots = slots_from_report(&rep, &choices, &library()).unwrap();
        assert_eq!(
            slots.iter().map(|s| s.title_id).collect::<Vec<_>>(),
            [None, Some(4), None]
        );
        choices.insert(4, Some(999));
        assert!(slots_from_report(&rep, &choices, &library()).is_err());
    }

    #[test]
    fn similarity_is_forgiving_but_not_loose() {
        assert_eq!(similarity("Cowboy Bebop", "cowboy  bebop"), 1.0);
        assert!(similarity("Cowboy Bebop", "Cowboy Bebop: The Movie") < CONFIDENT);
        assert!(similarity("Samurai Champlo", "Samurai Champloo") > CONFIDENT);
        assert!(similarity("Naruto", "Bleach") < OFFER);
        assert_eq!(similarity("A & B", "A and B"), 1.0);
    }
}
