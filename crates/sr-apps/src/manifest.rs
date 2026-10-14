//! Release manifests (DEV-APP-005): the snapshot a version submits, the diff against the
//! last approved one, and the changes that force a re-review (doc 11 section 2.1).

use crate::model::{Application, Command, intents};
use serde_json::{Value, json};

/// Snapshot of what a version asks staff to approve.
pub fn build_manifest(app: &Application, global_commands: &[Command]) -> Value {
    let mut commands: Vec<Value> = global_commands
        .iter()
        .map(|c| json!({"name": c.name, "type": c.kind, "description": c.description}))
        .collect();
    commands.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    let mut uris = app.redirect_uris.clone();
    uris.sort();
    json!({
        "name": app.name,
        "description": app.description,
        "scopes": app.install_scopes,
        "permissions": app.install_permissions.to_string(),
        "redirect_uris": uris,
        "intents": intents::names(app.intents_requested & intents::PRIVILEGED),
        "bot_public": app.bot_public,
        "privacy_policy_url": app.privacy_url,
        "terms_of_service_url": app.terms_url,
        "commands": commands,
    })
}

/// Why a version needs staff review. The first release always does; later ones only when
/// scopes, redirect URIs or requested intents changed (doc 11: command or description
/// changes need re-review only if flagged by the diff, which is the staff's call).
pub fn review_reasons(previous: Option<&Value>, new: &Value) -> Vec<String> {
    let Some(prev) = previous else {
        return vec!["first_release".to_owned()];
    };
    let mut out = Vec::new();
    for (key, reason) in [
        ("scopes", "scopes"),
        ("redirect_uris", "redirect_uris"),
        ("intents", "intents"),
    ] {
        if prev.get(key) != new.get(key) {
            out.push(reason.to_owned());
        }
    }
    out
}

/// Every top-level key that differs: `[{key, old, new}]`.
pub fn diff(previous: Option<&Value>, new: &Value) -> Vec<Value> {
    let Some(obj) = new.as_object() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (k, v) in obj {
        let old = previous.and_then(|p| p.get(k));
        if old != Some(v) {
            out.push(json!({"key": k, "old": old.cloned().unwrap_or(Value::Null), "new": v}));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasons_and_diff() {
        let a = json!({"scopes": ["bot"], "redirect_uris": [], "intents": [], "name": "a"});
        assert_eq!(review_reasons(None, &a), vec!["first_release"]);
        assert!(review_reasons(Some(&a), &a).is_empty());
        let b = json!({"scopes": ["bot", "identify"], "redirect_uris": [], "intents": ["MESSAGE_CONTENT"], "name": "b"});
        assert_eq!(review_reasons(Some(&a), &b), vec!["scopes", "intents"]);
        let d = diff(Some(&a), &b);
        assert_eq!(d.len(), 3);
        assert_eq!(diff(None, &a).len(), 4);
        assert!(diff(Some(&a), &a).is_empty());
    }
}
