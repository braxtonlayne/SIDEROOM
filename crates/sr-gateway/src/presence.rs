//! Pure presence rules (LNG-PRF-013/015): activity validation, the client platform of a
//! session, and merging what every attached session reports into the one presence other
//! users see.
//!
//! * Each session reports through op 3 / IDENTIFY: a status, an `afk` flag (the client
//!   sets it after 10 minutes without input and clears it on activity) and activities.
//! * `dnd` and `invisible` are user choices (open question 119): from any session, or from
//!   `PATCH /users/@me/presence`, they apply to all sessions. `idle` is per session: the
//!   user shows `idle` only when every attached session is idle.
//! * Activities come from the most recently updated session that has any; the custom status
//!   (set over REST) always comes along.
//! * `client_status` maps each platform (`web`, `mobile`, `desktop`) to the status its
//!   sessions show.

use crate::protocol::PresenceUpdate;
use serde_json::{Map, Value, json};
use sr_chat::social::presence::{Status, merge_sessions};

/// Most activities a user can show at once.
pub const MAX_ACTIVITIES: usize = 6;
const MAX_TEXT: usize = 128;
const MAX_KEY: usize = 256;
const MAX_URL: usize = 512;

/// Discord activity types plus SIDE ROOM's own `6` (Reading).
/// 0 Playing, 1 Streaming, 2 Listening, 3 Watching, 4 Custom, 5 Competing, 6 Reading.
const MAX_TYPE: i64 = 6;

fn text(v: &Value, max: usize) -> Option<String> {
    let s = v.as_str()?.trim();
    (!s.is_empty() && s.chars().count() <= max && !s.chars().any(char::is_control))
        .then(|| s.to_owned())
}

fn snowflake(v: &Value) -> Option<String> {
    let s = v.as_str()?;
    (!s.is_empty() && s.len() <= 20 && s.bytes().all(|b| b.is_ascii_digit())).then(|| s.to_owned())
}

fn millis(v: &Value) -> Option<i64> {
    v.as_i64().filter(|n| (0..=i64::MAX / 2).contains(n))
}

/// Keep only well-formed activities, rebuilt from a whitelist of fields. Clients are not
/// trusted: unknown fields, over-long text and bad types are dropped, and at most
/// [`MAX_ACTIVITIES`] survive.
pub fn sanitize_activities(raw: &[Value]) -> Vec<Value> {
    raw.iter()
        .filter_map(sanitize_one)
        .take(MAX_ACTIVITIES)
        .collect()
}

fn sanitize_one(a: &Value) -> Option<Value> {
    let o = a.as_object()?;
    let kind = o
        .get("type")?
        .as_i64()
        .filter(|t| (0..=MAX_TYPE).contains(t))?;
    let mut out = Map::new();
    out.insert("type".into(), json!(kind));
    out.insert("name".into(), json!(text(o.get("name")?, MAX_TEXT)?));
    for key in ["details", "state"] {
        if let Some(t) = o.get(key).and_then(|v| text(v, MAX_TEXT)) {
            out.insert(key.into(), json!(t));
        }
    }
    if let Some(url) = o.get("url").and_then(|v| text(v, MAX_URL))
        && url.starts_with("https://")
    {
        out.insert("url".into(), json!(url));
    }
    if let Some(id) = o.get("application_id").and_then(snowflake) {
        out.insert("application_id".into(), json!(id));
    }
    if let Some(ts) = o.get("timestamps").and_then(Value::as_object) {
        let mut t = Map::new();
        for key in ["start", "end"] {
            if let Some(n) = ts.get(key).and_then(millis) {
                t.insert(key.into(), json!(n));
            }
        }
        if !t.is_empty() {
            out.insert("timestamps".into(), Value::Object(t));
        }
    }
    if let Some(assets) = o.get("assets").and_then(Value::as_object) {
        let mut t = Map::new();
        for key in ["large_image", "large_text", "small_image", "small_text"] {
            if let Some(s) = assets.get(key).and_then(|v| text(v, MAX_KEY)) {
                t.insert(key.into(), json!(s));
            }
        }
        if !t.is_empty() {
            out.insert("assets".into(), Value::Object(t));
        }
    }
    if let Some(e) = o.get("emoji").and_then(Value::as_object) {
        let name = e.get("name").and_then(|v| text(v, 64));
        if let Some(name) = name {
            out.insert(
                "emoji".into(),
                json!({ "name": name, "id": e.get("id").and_then(snowflake) }),
            );
        }
    }
    Some(Value::Object(out))
}

/// `web`, `mobile` or `desktop`, from the IDENTIFY `properties` (Discord's `os`, `browser`,
/// `device`, with or without the `$` prefix).
pub fn platform_of(props: &Value) -> &'static str {
    let get = |k: &str| {
        props
            .get(k)
            .or_else(|| props.get(format!("${k}")))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_ascii_lowercase()
    };
    let (os, browser, device) = (get("os"), get("browser"), get("device"));
    let hay = format!("{os} {browser} {device}");
    if ["android", "ios", "iphone", "ipad", "mobile"]
        .iter()
        .any(|k| hay.contains(k))
    {
        "mobile"
    } else if ["desktop", "electron", "tauri"]
        .iter()
        .any(|k| browser.contains(k))
    {
        "desktop"
    } else {
        "web"
    }
}

/// What one attached session last reported.
#[derive(Debug, Clone)]
pub struct Report<'a> {
    pub platform: &'static str,
    /// `None` when the session never sent a presence.
    pub update: Option<&'a PresenceUpdate>,
    /// Larger is newer.
    pub order: u64,
}

fn parse(s: &str) -> Option<Status> {
    Some(match s {
        "online" => Status::Online,
        "idle" => Status::Idle,
        "dnd" => Status::Dnd,
        "invisible" => Status::Invisible,
        _ => return None,
    })
}

/// The status one session contributes to the merge: `idle` when it reports idle or `afk`,
/// otherwise active. `dnd` and `invisible` choices are lifted out by the caller.
fn contribution(u: Option<&PresenceUpdate>) -> Status {
    match u.and_then(|u| parse(&u.status).map(|s| (s, u.afk))) {
        Some((Status::Idle, _)) | Some((Status::Online, true)) => Status::Idle,
        _ => Status::Online,
    }
}

/// Merge the chosen presence (`base`: REST preference and custom status) with the attached
/// sessions. `sessions` must not be empty.
pub fn merge(base: Option<&PresenceUpdate>, sessions: &[Report<'_>]) -> PresenceUpdate {
    let base_status = base
        .and_then(|b| parse(&b.status))
        .unwrap_or(Status::Online);
    // A deliberate dnd / invisible from any session applies to the whole user; the REST
    // choice wins over it.
    let chosen = if base_status != Status::Online {
        base_status
    } else {
        let reported = |s: Status| {
            sessions
                .iter()
                .any(|r| r.update.and_then(|u| parse(&u.status)) == Some(s))
        };
        if reported(Status::Invisible) {
            Status::Invisible
        } else if reported(Status::Dnd) {
            Status::Dnd
        } else {
            Status::Online
        }
    };
    let all: Vec<Status> = sessions.iter().map(|r| contribution(r.update)).collect();
    let merged = merge_sessions(chosen, &all);
    // `merge_sessions` reports offline when nothing contributes; an all-invisible user keeps
    // the `invisible` label so that other users see offline while the user sees themself.
    let status = if chosen == Status::Invisible {
        Status::Invisible
    } else {
        merged
    };

    let mut client_status = Map::new();
    for platform in ["desktop", "web", "mobile"] {
        let of: Vec<Status> = sessions
            .iter()
            .filter(|r| r.platform == platform)
            .map(|r| contribution(r.update))
            .collect();
        if of.is_empty() {
            continue;
        }
        let s = merge_sessions(chosen, &of);
        if !matches!(s, Status::Offline | Status::Invisible) {
            client_status.insert(platform.into(), json!(s.as_str()));
        }
    }

    // Custom status first (from REST), then the newest session's activities.
    let mut activities: Vec<Value> = base.map(|b| b.activities.clone()).unwrap_or_default();
    let has_custom = activities.iter().any(|a| a["type"] == 4);
    let newest = sessions
        .iter()
        .filter(|r| r.update.is_some_and(|u| !u.activities.is_empty()))
        .max_by_key(|r| r.order);
    if let Some(u) = newest.and_then(|r| r.update) {
        for a in &u.activities {
            if a["type"] == 4 && has_custom {
                continue;
            }
            if !activities.contains(a) {
                activities.push(a.clone());
            }
        }
    }
    activities.truncate(MAX_ACTIVITIES);
    // A custom status (type 4) is listed first, like Discord.
    activities.sort_by_key(|a| a["type"] != 4);

    let since = if status == Status::Idle {
        sessions
            .iter()
            .filter_map(|r| r.update.and_then(|u| u.since))
            .min()
    } else {
        None
    };
    PresenceUpdate {
        status: status.as_str().to_owned(),
        activities,
        since,
        afk: status == Status::Idle && sessions.iter().all(|r| r.update.is_some_and(|u| u.afk)),
        client_status: Value::Object(client_status),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upd(status: &str, afk: bool, activities: Vec<Value>) -> PresenceUpdate {
        PresenceUpdate {
            status: status.into(),
            afk,
            activities,
            ..PresenceUpdate::default()
        }
    }

    fn rep<'a>(platform: &'static str, u: Option<&'a PresenceUpdate>, order: u64) -> Report<'a> {
        Report {
            platform,
            update: u,
            order,
        }
    }

    #[test]
    fn activities_are_rebuilt_from_a_whitelist() {
        let clean = sanitize_activities(&[
            json!({"type": 0, "name": "  Chess ", "details": "Round 2", "state": "winning",
                   "evil": {"x": 1}, "url": "http://insecure.test",
                   "timestamps": {"start": 1700000000000_i64, "end": "soon"},
                   "assets": {"large_image": "chess", "junk": 1}, "application_id": "123"}),
            json!({"type": 3, "name": "A Movie", "url": "https://cinema.test/watch/1"}),
            json!({"type": 6, "name": "Some Manga"}),
            json!({"type": 4, "name": "Custom Status", "state": "brb", "emoji": {"name": "coffee", "id": "55"}}),
        ]);
        assert_eq!(clean.len(), 4);
        assert_eq!(clean[0]["name"], "Chess");
        assert_eq!(clean[0]["details"], "Round 2");
        assert!(clean[0].get("evil").is_none() && clean[0].get("url").is_none());
        assert_eq!(clean[0]["timestamps"], json!({"start": 1700000000000_i64}));
        assert_eq!(clean[0]["assets"], json!({"large_image": "chess"}));
        assert_eq!(clean[1]["url"], "https://cinema.test/watch/1");
        assert_eq!(clean[2]["type"], 6, "reading is a SIDE ROOM activity type");
        assert_eq!(clean[3]["emoji"], json!({"name": "coffee", "id": "55"}));
    }

    #[test]
    fn bad_activities_are_dropped_and_the_list_is_capped() {
        let long = "x".repeat(129);
        let mut raw = vec![
            json!("not an object"),
            json!({"type": 9, "name": "bad type"}),
            json!({"type": 0}),
            json!({"type": 0, "name": ""}),
            json!({"type": 0, "name": long}),
            json!({"type": 0, "name": "ctrl\u{7}char"}),
            json!({"type": "0", "name": "stringly typed"}),
        ];
        assert!(sanitize_activities(&raw).is_empty());
        raw.extend((0..10).map(|i| json!({"type": 0, "name": format!("g{i}")})));
        assert_eq!(sanitize_activities(&raw).len(), MAX_ACTIVITIES);
    }

    #[test]
    fn platform_comes_from_the_identify_properties() {
        assert_eq!(
            platform_of(&json!({"os": "Android", "browser": "Chrome"})),
            "mobile"
        );
        assert_eq!(
            platform_of(&json!({"$os": "ios", "$device": "iPhone"})),
            "mobile"
        );
        assert_eq!(
            platform_of(&json!({"os": "Windows", "browser": "SideRoom Desktop"})),
            "desktop"
        );
        assert_eq!(
            platform_of(&json!({"os": "Linux", "browser": "Firefox"})),
            "web"
        );
        assert_eq!(platform_of(&json!(null)), "web");
    }

    #[test]
    fn idle_needs_every_session_idle() {
        let active = upd("online", false, vec![]);
        let away = upd("online", true, vec![]);
        let m = merge(
            None,
            &[rep("web", Some(&away), 1), rep("mobile", Some(&active), 2)],
        );
        assert_eq!(
            m.status, "online",
            "one active session keeps the user online"
        );
        assert_eq!(m.client_status, json!({"web": "idle", "mobile": "online"}));

        let m = merge(
            None,
            &[rep("web", Some(&away), 1), rep("desktop", Some(&away), 2)],
        );
        assert_eq!(m.status, "idle");
        assert!(m.afk);
        assert_eq!(m.client_status, json!({"desktop": "idle", "web": "idle"}));

        // A session that never reported counts as active.
        let m = merge(None, &[rep("web", Some(&away), 1), rep("web", None, 2)]);
        assert_eq!(m.status, "online");
    }

    #[test]
    fn dnd_and_invisible_are_user_wide() {
        let dnd = upd("dnd", false, vec![]);
        let idle = upd("idle", false, vec![]);
        let m = merge(
            None,
            &[rep("web", Some(&dnd), 1), rep("mobile", Some(&idle), 2)],
        );
        assert_eq!(m.status, "dnd");

        let inv = upd("invisible", false, vec![]);
        let m = merge(None, &[rep("web", Some(&inv), 1), rep("mobile", None, 2)]);
        assert_eq!(m.status, "invisible");
        assert_eq!(m.client_status, json!({}));

        // The REST choice beats a session report.
        let rest = upd("dnd", false, vec![]);
        let active = upd("online", false, vec![]);
        let m = merge(Some(&rest), &[rep("web", Some(&active), 1)]);
        assert_eq!(m.status, "dnd");
        assert_eq!(m.client_status, json!({"web": "dnd"}));
    }

    #[test]
    fn activities_follow_the_newest_session_and_keep_the_custom_status() {
        let custom = json!({"type": 4, "name": "Custom Status", "state": "brb"});
        let base = upd("online", false, vec![custom.clone()]);
        let game = json!({"type": 0, "name": "Chess"});
        let movie = json!({"type": 3, "name": "A Movie"});
        let old = upd("online", false, vec![game.clone()]);
        let new = upd("online", false, vec![movie.clone()]);
        let m = merge(
            Some(&base),
            &[rep("web", Some(&old), 1), rep("desktop", Some(&new), 2)],
        );
        assert_eq!(m.activities, vec![custom.clone(), movie.clone()]);
        // A session's own custom status does not double up the REST one.
        let dup = upd(
            "online",
            false,
            vec![
                json!({"type": 4, "name": "Custom Status", "state": "other"}),
                game.clone(),
            ],
        );
        let m = merge(Some(&base), &[rep("web", Some(&dup), 5)]);
        assert_eq!(m.activities, vec![custom, game]);
    }
}
