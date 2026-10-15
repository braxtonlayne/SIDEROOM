//! Built-in feature modules (docs/inventory/09-feature-modules.md).
//!
//! Each module is a [`crate::Feature`]: an id, a settings schema the web app
//! renders generically, and handlers for platform events and ticks. All of them
//! are off by default and enabled per guild. [`register_all`] adds them to a
//! registry at startup.

use serde_json::Value;

use crate::feature::{FeatureError, FeatureRegistry};

pub mod auto_roles;
pub mod auto_thread;
pub mod join_to_create;
pub mod message_quotes;
pub mod mod_log;
pub mod reaction_roles;
pub mod slowmode_plus;
pub mod starboard;
pub mod stat_channels;
pub mod sticky;
pub mod temp_roles;
pub mod welcome;
// new-feature:mod

/// Register every built-in module.
pub fn register_all(registry: &mut FeatureRegistry) -> Result<(), FeatureError> {
    crate::register_features!(
        registry,
        starboard::Starboard,
        reaction_roles::ReactionRoles,
        welcome::Welcome,
        sticky::Sticky,
        auto_thread::AutoThread,
        slowmode_plus::SlowmodePlus,
        temp_roles::TempRoles,
        auto_roles::AutoRoles,
        mod_log::ModLog,
        stat_channels::StatChannels,
        message_quotes::MessageQuotes,
        join_to_create::JoinToCreate,
        // new-feature:register
    )
}

/// A snowflake written as a decimal string (how pickers store ids).
pub(crate) fn snowflake(s: &str) -> Option<u64> {
    s.trim().parse::<u64>().ok().filter(|n| *n > 0)
}

pub(crate) fn snowflakes(list: &[String]) -> Vec<u64> {
    list.iter().filter_map(|s| snowflake(s)).collect()
}

/// Canonical key of an emoji written by an admin: `<:name:id>`, `:name:id` and
/// `name:id` all become `name:id`; unicode emoji lose the variation selector.
pub(crate) fn normalize_emoji(raw: &str) -> String {
    let t = raw.trim();
    let t = t
        .strip_prefix("<a:")
        .or_else(|| t.strip_prefix("<:"))
        .map(|r| r.trim_end_matches('>'))
        .unwrap_or(t);
    let t = t.strip_prefix(':').unwrap_or(t);
    t.replace('\u{fe0f}', "")
}

/// The emoji key of a reaction event's `{id, name}` object.
pub(crate) fn event_emoji_key(emoji: &Value) -> Option<String> {
    let name = emoji.get("name")?.as_str()?;
    let id = match emoji.get("id") {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    };
    Some(match id {
        Some(id) => format!("{name}:{id}"),
        None => name.replace('\u{fe0f}', ""),
    })
}

/// How an emoji key reads in message text.
pub(crate) fn emoji_display(key: &str) -> String {
    match key.rsplit_once(':') {
        Some((name, id)) if id.bytes().all(|b| b.is_ascii_digit()) && !id.is_empty() => {
            format!("<:{name}:{id}>")
        }
        _ => key.to_owned(),
    }
}

/// UTC calendar date `YYYY-MM-DD` of a unix-ms timestamp.
pub(crate) fn utc_date(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Display name from a member object (`user.global_name`, else `user.username`).
pub(crate) fn payload_user_name(payload: &Value) -> Option<String> {
    let u = payload.get("user")?;
    u.get("global_name")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .or_else(|| u.get("username").and_then(Value::as_str))
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn emoji_normalisation() {
        assert_eq!(normalize_emoji("<:pog:123>"), "pog:123");
        assert_eq!(normalize_emoji(":pog:123"), "pog:123");
        assert_eq!(normalize_emoji("\u{2b50}\u{fe0f}"), "\u{2b50}");
        assert_eq!(
            event_emoji_key(&json!({"id": "123", "name": "pog"})).as_deref(),
            Some("pog:123")
        );
        assert_eq!(
            event_emoji_key(&json!({"id": null, "name": "\u{2b50}"})).as_deref(),
            Some("\u{2b50}")
        );
        assert_eq!(emoji_display("pog:123"), "<:pog:123>");
        assert_eq!(emoji_display("\u{2b50}"), "\u{2b50}");
    }

    #[test]
    fn civil_dates() {
        assert_eq!(utc_date(0), "1970-01-01");
        assert_eq!(utc_date(1_782_000_000_000), "2026-06-21");
        assert_eq!(utc_date(-86_400_000), "1969-12-31");
    }

    /// `just new-feature` fills these placeholders and edits these markers; if either
    /// side drifts the generator breaks silently, so pin them.
    #[test]
    fn generator_template_and_markers_are_in_sync() {
        let tmpl = include_str!("../../templates/feature.rs.tmpl");
        for p in ["__ID__", "__TYPE__", "__CONST__", "__NAME__"] {
            assert!(tmpl.contains(p), "template lost {p}");
        }
        let src = include_str!("mod.rs");
        assert!(src.contains("// new-feature:mod"));
        assert!(src.contains("// new-feature:register"));
    }

    #[test]
    fn registered_modules_have_valid_ids_default_off_and_valid_default_settings() {
        let mut reg = FeatureRegistry::new();
        register_all(&mut reg).unwrap();
        assert!(reg.len() >= 11);
        for f in reg.iter() {
            assert!(
                f.id()
                    .split_once('.')
                    .is_some_and(|(a, b)| !a.is_empty() && !b.is_empty()),
                "{} is not section.name",
                f.id()
            );
            assert!(!f.meta().default_enabled, "{} must start off", f.id());
            let schema = f.settings_schema();
            for scope in f.scopes() {
                schema
                    .validate(&schema.defaults(*scope))
                    .unwrap_or_else(|e| panic!("{}: {e}", f.id()));
            }
        }
    }

    #[test]
    fn snowflakes_parse() {
        assert_eq!(snowflake(" 42 "), Some(42));
        assert_eq!(snowflake("0"), None);
        assert_eq!(snowflake("x"), None);
    }
}
