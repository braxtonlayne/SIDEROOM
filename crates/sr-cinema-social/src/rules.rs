//! Validation and pure logic: progress, text, cursors, presence payloads and recommendation ranking.

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::catalog::{CatalogEpisode, CatalogTitle, TitleKind};
use crate::error::{SocialError, SocialResult};
use crate::model::{ProgressInput, ProgressWrite, Recommendation};

/// A progress of at least this many percent counts as watched.
pub const COMPLETED_PERCENT: f32 = 90.0;
/// Longest media the player may report (100 hours).
pub const MAX_DURATION_SECS: i32 = 360_000;
/// How long a "watching" record lives without a heartbeat.
pub const PRESENCE_TTL_SECS: i64 = 45;
/// People notified at most per presence change through the shared-server audience.
pub const MAX_PRESENCE_AUDIENCE: i64 = 1000;
pub const MAX_LIMIT: u32 = 100;
pub const DEFAULT_LIMIT: u32 = 24;
pub const MAX_LISTS: i64 = 100;
pub const MAX_LIST_ITEMS: i64 = 1000;
pub const MAX_REVIEW: usize = 8000;
pub const MAX_COMMENT: usize = 2000;
pub const MAX_NAME: usize = 100;
pub const MAX_DESCRIPTION: usize = 500;
pub const MAX_NOTE: usize = 500;
/// Fewest viewers a title needs before the global trending shows it.
pub const GLOBAL_MIN_VIEWERS: i64 = 2;
/// Fewest other viewers behind a co-watch recommendation.
pub const COWATCH_MIN: i64 = 2;

pub fn limit(raw: Option<u32>) -> u32 {
    raw.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

/// Who may see a person's "watching" activity: the `cinema.share_watching` setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Share {
    Off,
    Friends,
    Everyone,
}

impl Share {
    /// From `cinema_share_level()`; anything unknown is the default, friends.
    pub fn from_level(level: i16) -> Self {
        match level {
            0 => Self::Off,
            2 => Self::Everyone,
            _ => Self::Friends,
        }
    }

    /// From the setting's text; absent or unknown means friends.
    pub fn from_setting(raw: Option<&str>) -> Self {
        match raw {
            Some("off") => Self::Off,
            Some("everyone") => Self::Everyone,
            _ => Self::Friends,
        }
    }
}

/// Validate a progress save. The percent follows from the position.
pub fn progress_write(input: &ProgressInput) -> SocialResult<ProgressWrite> {
    if input.duration_secs <= 0 || input.duration_secs > MAX_DURATION_SECS {
        return Err(SocialError::Invalid(
            "duration_secs must be between 1 and 360000".into(),
        ));
    }
    if input.position_secs < 0 {
        return Err(SocialError::Invalid(
            "position_secs must not be negative".into(),
        ));
    }
    if input.base_updated_at_ms.is_some_and(|b| b < 0) {
        return Err(SocialError::Invalid(
            "base_updated_at_ms must not be negative".into(),
        ));
    }
    // Players report a little past the end; more than a minute is a broken client.
    if input.position_secs > input.duration_secs.saturating_add(60) {
        return Err(SocialError::Invalid(
            "position_secs is past the end of the media".into(),
        ));
    }
    let position = input.position_secs.min(input.duration_secs);
    let percent = (position as f32 / input.duration_secs as f32 * 100.0).clamp(0.0, 100.0);
    Ok(ProgressWrite {
        position_secs: position,
        duration_secs: input.duration_secs,
        percent,
        completed: input.completed,
        playing: input.playing.unwrap_or(true),
        base_updated_at_ms: input.base_updated_at_ms,
        force: input.force.unwrap_or(false),
    })
}

/// Whether a stored row counts as watched after a write: an explicit flag wins; otherwise 90% or
/// more, and a finished item stays finished.
pub fn completed_after(explicit: Option<bool>, percent: f32, was_completed: bool) -> bool {
    explicit.unwrap_or(percent >= COMPLETED_PERCENT || was_completed)
}

/// Trim text, refuse control characters (newlines are fine) and enforce the length (in characters).
pub fn clean_text(raw: &str, max: usize, field: &str, allow_empty: bool) -> SocialResult<String> {
    let t = raw.trim();
    if t.is_empty() && !allow_empty {
        return Err(SocialError::Invalid(format!("{field} must not be empty")));
    }
    if t.chars().count() > max {
        return Err(SocialError::Invalid(format!(
            "{field} must be at most {max} characters"
        )));
    }
    if t.chars().any(|c| c.is_control() && c != '\n') {
        return Err(SocialError::Invalid(format!(
            "{field} contains control characters"
        )));
    }
    Ok(t.to_owned())
}

pub fn score(raw: i32) -> SocialResult<i16> {
    if (1..=10).contains(&raw) {
        Ok(raw as i16)
    } else {
        Err(SocialError::Invalid(
            "score must be between 1 and 10".into(),
        ))
    }
}

/// Encode a keyset cursor of integers.
pub fn encode_cursor(parts: &[i64]) -> String {
    parts
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(".")
}

/// Decode a cursor of exactly `n` integers.
pub fn decode_cursor(raw: &str, n: usize) -> SocialResult<Vec<i64>> {
    let parts: Result<Vec<i64>, _> = raw.split('.').map(str::parse::<i64>).collect();
    match parts {
        Ok(p) if p.len() == n => Ok(p),
        _ => Err(SocialError::Invalid("before is not a valid cursor".into())),
    }
}

// ---- presence --------------------------------------------------------------

const ACTIVITY_NAME: &str = "SIDE ROOM Cinema";
const MASKED: &str = "An age-restricted title";

/// "S1 E3" style marker.
pub fn episode_marker(e: &CatalogEpisode) -> String {
    format!("S{} E{}", e.season, e.number)
}

/// The Discord-shaped "Watching" activity (type 3). Age-restricted titles are masked: no title, no ids.
pub fn watching_activity(
    title: &CatalogTitle,
    episode: Option<&CatalogEpisode>,
    started_ms: i64,
) -> Value {
    if title.age_restricted {
        return json!({
            "type": 3, "name": ACTIVITY_NAME, "details": MASKED,
            "timestamps": { "start": started_ms }, "application_id": null,
        });
    }
    let state = episode.map(|e| match &e.title {
        Some(t) => format!("{} · {t}", episode_marker(e)),
        None => episode_marker(e),
    });
    json!({
        "type": 3, "name": ACTIVITY_NAME, "details": title.title, "state": state,
        "timestamps": { "start": started_ms }, "application_id": null,
    })
}

/// The `SR_ACTIVITY_UPDATE` payload for a start or refresh.
pub fn watching_event(
    user: i64,
    title: &CatalogTitle,
    episode: Option<&CatalogEpisode>,
    started_ms: i64,
    expires_ms: i64,
) -> Value {
    let mut v = json!({
        "type": "watching",
        "user_id": user.to_string(),
        "activity": watching_activity(title, episode, started_ms),
        "expires_at_ms": expires_ms,
    });
    if !title.age_restricted {
        v["title_id"] = json!(title.id.to_string());
        v["episode_id"] = json!(episode.map_or(0, |e| e.id).to_string());
    }
    v
}

pub fn stopped_event(user: i64) -> Value {
    json!({ "type": "stopped", "user_id": user.to_string() })
}

// ---- recommendations ---------------------------------------------------------

/// Overlap of two tag sets, 0 to 1 (Jaccard).
pub fn tag_similarity(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let inter = a.iter().filter(|t| b.contains(t)).count();
    let union = a.len() + b.len() - inter;
    inter as f64 / union as f64
}

/// Rank candidates for one seed title: co-watchers weigh 1 each (capped at 10, so one huge cohort
/// cannot bury tag matches) and tag overlap up to 3. Candidates with neither signal are dropped.
pub fn rank_candidates(
    seed_tags: &[String],
    cowatch: &HashMap<i64, i64>,
    candidates: &[CatalogTitle],
    limit: usize,
) -> Vec<(i64, f64, Vec<String>)> {
    let mut out: Vec<(i64, f64, Vec<String>)> = candidates
        .iter()
        .filter_map(|c| {
            let co = cowatch.get(&c.id).copied().unwrap_or(0);
            let sim = tag_similarity(seed_tags, &c.tags);
            let mut reasons = Vec::new();
            let mut score = 0.0;
            if co >= COWATCH_MIN {
                score += co.min(10) as f64;
                reasons.push("cowatch".to_owned());
            }
            if sim > 0.0 {
                score += sim * 3.0;
                reasons.push("tags".to_owned());
            }
            (!reasons.is_empty()).then_some((c.id, score, reasons))
        })
        .collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    out.truncate(limit);
    out
}

/// Convert ranked tuples to wire items, resolving titles from `by_id`.
pub fn to_recommendations(
    ranked: Vec<(i64, f64, Vec<String>)>,
    by_id: &HashMap<i64, CatalogTitle>,
) -> Vec<Recommendation> {
    ranked
        .into_iter()
        .filter_map(|(id, score, reasons)| {
            by_id.get(&id).map(|t| Recommendation {
                title: t.into(),
                score,
                reasons,
            })
        })
        .collect()
}

/// The kinds that are played by episode.
pub fn is_episodic(kind: TitleKind) -> bool {
    kind != TitleKind::Movie
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::MemoryCatalog;

    fn input(pos: i32, dur: i32) -> ProgressInput {
        ProgressInput {
            position_secs: pos,
            duration_secs: dur,
            completed: None,
            playing: None,
            base_updated_at_ms: None,
            force: None,
        }
    }

    #[test]
    fn progress_is_validated_and_the_percent_follows() {
        let w = progress_write(&input(900, 1000)).unwrap();
        assert_eq!(w.percent, 90.0);
        assert!(w.playing && !w.force);
        assert!(progress_write(&input(-1, 10)).is_err());
        assert!(progress_write(&input(0, 0)).is_err());
        assert!(progress_write(&input(0, MAX_DURATION_SECS + 1)).is_err());
        assert!(progress_write(&input(1200, 1000)).is_err());
        // A little past the end is clamped to the end.
        let end = progress_write(&input(1030, 1000)).unwrap();
        assert_eq!((end.position_secs, end.percent), (1000, 100.0));
        let mut i = input(1, 10);
        i.base_updated_at_ms = Some(-1);
        assert!(progress_write(&i).is_err());
    }

    #[test]
    fn ninety_percent_completes_and_completion_sticks() {
        assert!(!completed_after(None, 89.9, false));
        assert!(completed_after(None, 90.0, false));
        // A rewatch at 10% stays watched until the client says otherwise.
        assert!(completed_after(None, 10.0, true));
        assert!(!completed_after(Some(false), 95.0, true));
        assert!(completed_after(Some(true), 1.0, false));
    }

    #[test]
    fn text_is_trimmed_bounded_and_clean() {
        assert_eq!(clean_text("  hi ", 10, "body", false).unwrap(), "hi");
        assert!(clean_text("   ", 10, "body", false).is_err());
        assert_eq!(clean_text("   ", 10, "note", true).unwrap(), "");
        assert!(clean_text(&"x".repeat(11), 10, "body", false).is_err());
        assert!(clean_text("a\u{0}b", 10, "body", false).is_err());
        assert_eq!(clean_text("a\nb", 10, "body", false).unwrap(), "a\nb");
    }

    #[test]
    fn scores_are_one_to_ten() {
        assert!(score(0).is_err() && score(11).is_err());
        assert_eq!(score(1).unwrap(), 1);
        assert_eq!(score(10).unwrap(), 10);
    }

    #[test]
    fn cursors_round_trip_and_reject_garbage() {
        let c = encode_cursor(&[5, 6, 0]);
        assert_eq!(decode_cursor(&c, 3).unwrap(), vec![5, 6, 0]);
        assert!(decode_cursor(&c, 2).is_err());
        assert!(decode_cursor("x.y", 2).is_err());
        assert!(decode_cursor("", 1).is_err());
    }

    #[test]
    fn share_defaults_to_friends() {
        assert_eq!(Share::from_setting(None), Share::Friends);
        assert_eq!(Share::from_setting(Some("bogus")), Share::Friends);
        assert_eq!(Share::from_setting(Some("off")), Share::Off);
        assert_eq!(Share::from_setting(Some("everyone")), Share::Everyone);
        assert_eq!(Share::from_level(0), Share::Off);
        assert_eq!(Share::from_level(1), Share::Friends);
        assert_eq!(Share::from_level(2), Share::Everyone);
        assert_eq!(Share::from_level(9), Share::Friends);
    }

    #[test]
    fn restricted_titles_are_masked_in_the_activity() {
        let mut t = MemoryCatalog::title_with(7, TitleKind::Series, "Secret", &[]);
        let e = MemoryCatalog::episode_with(70, 7, 1, 3);
        let open = watching_event(1, &t, Some(&e), 100, 200);
        assert_eq!(open["activity"]["details"], "Secret");
        assert_eq!(open["activity"]["state"], "S1 E3 · Episode 3");
        assert_eq!(open["activity"]["type"], 3);
        assert_eq!(open["title_id"], "7");
        assert_eq!(open["episode_id"], "70");
        t.age_restricted = true;
        let masked = watching_event(1, &t, Some(&e), 100, 200);
        let text = masked.to_string();
        assert!(!text.contains("Secret") && !text.contains("\"70\"") && !text.contains("\"7\""));
        assert_eq!(masked["activity"]["details"], MASKED);
        assert!(masked.get("title_id").is_none());
    }

    #[test]
    fn a_movie_activity_has_no_state() {
        let t = MemoryCatalog::title_with(7, TitleKind::Movie, "Akira", &[]);
        let ev = watching_event(1, &t, None, 1, 2);
        assert!(ev["activity"]["state"].is_null());
        assert_eq!(ev["episode_id"], "0");
    }

    fn tags(t: &[&str]) -> Vec<String> {
        t.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn similarity_is_jaccard() {
        assert_eq!(tag_similarity(&tags(&["a", "b"]), &tags(&["a", "b"])), 1.0);
        assert_eq!(
            tag_similarity(&tags(&["a", "b"]), &tags(&["b", "c"])),
            1.0 / 3.0
        );
        assert_eq!(tag_similarity(&tags(&["a"]), &tags(&["b"])), 0.0);
        assert_eq!(tag_similarity(&[], &tags(&["b"])), 0.0);
    }

    #[test]
    fn ranking_combines_cowatch_and_tags_and_drops_noise() {
        let seed = tags(&["scifi", "anime"]);
        let c = |id, t: &[&str]| MemoryCatalog::title_with(id, TitleKind::Movie, "x", t);
        let candidates = vec![
            c(1, &["scifi", "anime"]), // tags only: 3.0
            c(2, &["drama"]),          // cowatch 4 only: 4.0
            c(3, &["scifi"]),          // tags 0.5*3 = 1.5 + cowatch 2 = 3.5
            c(4, &["drama"]),          // cowatch below the floor: dropped
            c(5, &["western"]),        // nothing: dropped
        ];
        let cowatch: HashMap<i64, i64> = [(2, 4), (3, 2), (4, 1)].into_iter().collect();
        let ranked = rank_candidates(&seed, &cowatch, &candidates, 10);
        let ids: Vec<i64> = ranked.iter().map(|r| r.0).collect();
        assert_eq!(ids, vec![2, 3, 1]);
        assert_eq!(ranked[1].2, vec!["cowatch".to_owned(), "tags".to_owned()]);
        assert_eq!(rank_candidates(&seed, &cowatch, &candidates, 1).len(), 1);
    }
}
