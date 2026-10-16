//! Interaction rules (DEV-BOT-004): which callbacks fit which interaction, callback data
//! validation, and checking what a client submits against the registered option schema.
//! Pure functions over JSON.

use crate::{commands::option_type::*, error::FieldProblem, model::Command};
use serde_json::{Map, Value, json};

/// Time the application has for its first response.
pub const ACK_WINDOW_MS: i64 = 3_000;
/// Time the interaction token stays valid for follow-ups and edits.
pub const TOKEN_LIFETIME_MS: i64 = 15 * 60 * 1000;

/// Interaction types.
pub mod itype {
    /// Ping (HTTP interactions only; never created here).
    pub const PING: i16 = 1;
    /// Slash, user or message command.
    pub const APPLICATION_COMMAND: i16 = 2;
    /// Button or select menu.
    pub const MESSAGE_COMPONENT: i16 = 3;
    /// Autocomplete.
    pub const AUTOCOMPLETE: i16 = 4;
    /// Modal submit.
    pub const MODAL_SUBMIT: i16 = 5;
}

/// Interaction callback types.
pub mod ctype {
    /// Pong.
    pub const PONG: i16 = 1;
    /// Message with source.
    pub const CHANNEL_MESSAGE: i16 = 4;
    /// Deferred message.
    pub const DEFERRED_CHANNEL_MESSAGE: i16 = 5;
    /// Deferred update of the component's message.
    pub const DEFERRED_UPDATE_MESSAGE: i16 = 6;
    /// Update the component's message.
    pub const UPDATE_MESSAGE: i16 = 7;
    /// Autocomplete choices.
    pub const AUTOCOMPLETE_RESULT: i16 = 8;
    /// Modal.
    pub const MODAL: i16 = 9;
}

/// Message flag: only the invoker sees it.
pub const EPHEMERAL: u32 = 1 << 6;
/// Message flag: no embeds.
pub const SUPPRESS_EMBEDS: u32 = 1 << 2;

/// Whether `callback` is a legal first response to an interaction of `kind`.
pub fn callback_allowed(kind: i16, callback: i16) -> bool {
    use ctype::*;
    match kind {
        itype::PING => callback == PONG,
        itype::APPLICATION_COMMAND => {
            matches!(callback, CHANNEL_MESSAGE | DEFERRED_CHANNEL_MESSAGE | MODAL)
        }
        itype::MESSAGE_COMPONENT => matches!(
            callback,
            CHANNEL_MESSAGE
                | DEFERRED_CHANNEL_MESSAGE
                | DEFERRED_UPDATE_MESSAGE
                | UPDATE_MESSAGE
                | MODAL
        ),
        itype::AUTOCOMPLETE => callback == AUTOCOMPLETE_RESULT,
        itype::MODAL_SUBMIT => matches!(
            callback,
            CHANNEL_MESSAGE | DEFERRED_CHANNEL_MESSAGE | DEFERRED_UPDATE_MESSAGE | UPDATE_MESSAGE
        ),
        _ => false,
    }
}

/// Message fields of a callback or follow-up.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MessageData {
    /// Text.
    pub content: Option<String>,
    /// Embeds as submitted.
    pub embeds: Vec<Value>,
    /// Components as submitted.
    pub components: Vec<Value>,
    /// Honoured flags (`EPHEMERAL`, `SUPPRESS_EMBEDS`).
    pub flags: u32,
    /// Text-to-speech.
    pub tts: bool,
}

impl MessageData {
    /// Ephemeral?
    pub fn ephemeral(&self) -> bool {
        self.flags & EPHEMERAL != 0
    }

    /// Nothing to show.
    pub fn is_empty(&self) -> bool {
        self.content.as_deref().is_none_or(|c| c.trim().is_empty())
            && self.embeds.is_empty()
            && self.components.is_empty()
    }
}

type Problems = Vec<FieldProblem>;

/// Parse message fields. `require_body` demands content, an embed or a component.
pub fn parse_message_data(
    data: &Value,
    path: &str,
    require_body: bool,
) -> Result<MessageData, Problems> {
    let at = |k: &str| {
        if path.is_empty() {
            k.to_owned()
        } else {
            format!("{path}.{k}")
        }
    };
    let mut p = Problems::new();
    let Some(o) = data.as_object() else {
        return Err(vec![FieldProblem::new(
            path,
            "MODEL_TYPE_CONVERT",
            "Must be an object.",
        )]);
    };
    let content = match o.get("content") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.chars().count() <= 2000 => Some(s.clone()),
        _ => {
            p.push(FieldProblem::new(
                at("content"),
                "BASE_TYPE_MAX_LENGTH",
                "Must be 2000 or fewer in length.",
            ));
            None
        }
    };
    let list = |key: &str, max: usize, p: &mut Problems| -> Vec<Value> {
        match o.get(key) {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(a)) if a.len() <= max => a.clone(),
            _ => {
                p.push(FieldProblem::new(
                    at(key),
                    "BASE_TYPE_MAX_LENGTH",
                    format!("Must be {max} or fewer in length."),
                ));
                Vec::new()
            }
        }
    };
    let embeds = list("embeds", 10, &mut p);
    let components = list("components", 5, &mut p);
    let flags = match o.get("flags") {
        None | Some(Value::Null) => 0,
        Some(v) => match v.as_u64() {
            Some(f) => (f as u32) & (EPHEMERAL | SUPPRESS_EMBEDS),
            None => {
                p.push(FieldProblem::new(
                    at("flags"),
                    "NUMBER_TYPE_COERCE",
                    "Not a number.",
                ));
                0
            }
        },
    };
    let tts = o.get("tts").and_then(Value::as_bool).unwrap_or(false);
    let out = MessageData {
        content,
        embeds,
        components,
        flags,
        tts,
    };
    if require_body && out.is_empty() && p.is_empty() {
        p.push(FieldProblem::new(
            at("content"),
            "BASE_TYPE_REQUIRED",
            "Cannot send an empty message.",
        ));
    }
    if p.is_empty() { Ok(out) } else { Err(p) }
}

/// Validate an autocomplete result: up to 25 choices with names and values.
pub fn parse_choices(data: &Value) -> Result<Vec<Value>, Problems> {
    let arr = data.get("choices").and_then(Value::as_array);
    let Some(arr) = arr else {
        return Err(vec![FieldProblem::new(
            "data.choices",
            "BASE_TYPE_REQUIRED",
            "This field is required",
        )]);
    };
    if arr.len() > 25 {
        return Err(vec![FieldProblem::new(
            "data.choices",
            "BASE_TYPE_MAX_LENGTH",
            "Must be 25 or fewer in length.",
        )]);
    }
    let mut out = Vec::new();
    for (i, c) in arr.iter().enumerate() {
        let name = c.get("name").and_then(Value::as_str).unwrap_or("");
        let value_ok = matches!(c.get("value"), Some(Value::String(s)) if s.chars().count() <= 100)
            || c.get("value").is_some_and(Value::is_number);
        if name.is_empty() || name.chars().count() > 100 || !value_ok {
            return Err(vec![FieldProblem::new(
                format!("data.choices.{i}"),
                "BASE_TYPE_BAD_LENGTH",
                "Invalid choice.",
            )]);
        }
        out.push(json!({"name": name, "value": c["value"]}));
    }
    Ok(out)
}

/// Validate a modal: `custom_id` (<=100), `title` (<=45), 1 to 5 action rows of components.
pub fn parse_modal(data: &Value) -> Result<Value, Problems> {
    let mut p = Problems::new();
    let custom_id = data.get("custom_id").and_then(Value::as_str).unwrap_or("");
    if custom_id.is_empty() || custom_id.chars().count() > 100 {
        p.push(FieldProblem::new(
            "data.custom_id",
            "BASE_TYPE_BAD_LENGTH",
            "Must be between 1 and 100 in length.",
        ));
    }
    let title = data.get("title").and_then(Value::as_str).unwrap_or("");
    if title.is_empty() || title.chars().count() > 45 {
        p.push(FieldProblem::new(
            "data.title",
            "BASE_TYPE_BAD_LENGTH",
            "Must be between 1 and 45 in length.",
        ));
    }
    let comps = data.get("components").and_then(Value::as_array);
    match comps {
        Some(c)
            if (1..=5).contains(&c.len()) && c.iter().all(|r| r.get("type") == Some(&json!(1))) => {
        }
        _ => p.push(FieldProblem::new(
            "data.components",
            "BASE_TYPE_BAD_LENGTH",
            "Must hold 1 to 5 action rows.",
        )),
    }
    if !p.is_empty() {
        return Err(p);
    }
    Ok(json!({"custom_id": custom_id, "title": title, "components": comps}))
}

/// Ids referenced by option values, to be resolved into `data.resolved`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Referenced {
    /// `USER` option ids (and mentionables that are users).
    pub users: Vec<i64>,
    /// `ROLE` option ids.
    pub roles: Vec<i64>,
    /// `CHANNEL` option ids.
    pub channels: Vec<i64>,
    /// `MENTIONABLE` ids (user or role; resolved by lookup).
    pub mentionables: Vec<i64>,
    /// `ATTACHMENT` option ids: upload slots the invoker staged in the channel (DEV-BOT-008).
    pub attachments: Vec<i64>,
}

/// What a client submitted, checked against the registered schema.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckedOptions {
    /// Normalised `data.options`.
    pub options: Vec<Value>,
    /// Ids to resolve.
    pub referenced: Referenced,
    /// Path of the focused option (autocomplete).
    pub focused: Option<String>,
}

/// Check `submitted` (the client's `data.options`) against the command's schema.
/// `autocomplete` relaxes the "required options present" rule and requires one focused
/// option.
pub fn check_options(
    command: &Command,
    submitted: &Value,
    autocomplete: bool,
) -> Result<CheckedOptions, Problems> {
    let mut p = Problems::new();
    let mut refs = Referenced::default();
    let mut focused = None;
    let schema = command.options.as_array().cloned().unwrap_or_default();
    let given = submitted.as_array().cloned().unwrap_or_default();
    if !submitted.is_null() && !submitted.is_array() {
        p.push(FieldProblem::new(
            "data.options",
            "MODEL_TYPE_CONVERT",
            "Must be an array.",
        ));
    }
    let options = walk(
        &schema,
        &given,
        "data.options",
        autocomplete,
        &mut p,
        &mut refs,
        &mut focused,
    );
    if autocomplete && focused.is_none() && p.is_empty() {
        p.push(FieldProblem::new(
            "data.options",
            "BASE_TYPE_REQUIRED",
            "An autocomplete interaction needs a focused option.",
        ));
    }
    if p.is_empty() {
        Ok(CheckedOptions {
            options,
            referenced: refs,
            focused,
        })
    } else {
        Err(p)
    }
}

fn walk(
    schema: &[Value],
    given: &[Value],
    path: &str,
    autocomplete: bool,
    p: &mut Problems,
    refs: &mut Referenced,
    focused: &mut Option<String>,
) -> Vec<Value> {
    let mut out = Vec::new();
    let is_sub_level = schema
        .iter()
        .any(|s| matches!(s["type"].as_i64(), Some(SUB_COMMAND | SUB_COMMAND_GROUP)));
    if is_sub_level && given.len() != 1 {
        p.push(FieldProblem::new(
            path,
            "BASE_TYPE_REQUIRED",
            "Choose exactly one sub-command.",
        ));
        return out;
    }
    for (i, g) in given.iter().enumerate() {
        let at = format!("{path}.{i}");
        let name = g.get("name").and_then(Value::as_str).unwrap_or("");
        let Some(def) = schema.iter().find(|s| s["name"] == name) else {
            p.push(FieldProblem::new(
                format!("{at}.name"),
                "APPLICATION_COMMAND_UNKNOWN_OPTION",
                "Unknown option.",
            ));
            continue;
        };
        let ty = def["type"].as_i64().unwrap_or(0);
        if g.get("type").and_then(Value::as_i64) != Some(ty) {
            p.push(FieldProblem::new(
                format!("{at}.type"),
                "BASE_TYPE_CHOICES",
                "Option type does not match the command.",
            ));
            continue;
        }
        let mut o = Map::new();
        o.insert("name".into(), json!(name));
        o.insert("type".into(), json!(ty));
        match ty {
            SUB_COMMAND | SUB_COMMAND_GROUP => {
                let inner_schema = def["options"].as_array().cloned().unwrap_or_default();
                let inner_given = g["options"].as_array().cloned().unwrap_or_default();
                let nested = walk(
                    &inner_schema,
                    &inner_given,
                    &format!("{at}.options"),
                    autocomplete,
                    p,
                    refs,
                    focused,
                );
                o.insert("options".into(), Value::Array(nested));
            }
            _ => {
                let v = g.get("value").cloned().unwrap_or(Value::Null);
                let is_focused = g.get("focused").and_then(Value::as_bool).unwrap_or(false);
                if is_focused {
                    if !autocomplete {
                        p.push(FieldProblem::new(
                            format!("{at}.focused"),
                            "BASE_TYPE_CHOICES",
                            "Only autocomplete interactions have a focused option.",
                        ));
                    }
                    *focused = Some(name.to_owned());
                    o.insert("focused".into(), json!(true));
                }
                // The focused option of an autocomplete is partial text: only its type is checked.
                if let Some(msg) = value_problem(def, ty, &v, is_focused && autocomplete, refs) {
                    p.push(FieldProblem::new(
                        format!("{at}.value"),
                        "BASE_TYPE_CHOICES",
                        msg,
                    ));
                } else {
                    o.insert("value".into(), v);
                }
            }
        }
        out.push(Value::Object(o));
    }
    if !autocomplete && !is_sub_level {
        for def in schema {
            if def["required"] == true && !given.iter().any(|g| g["name"] == def["name"]) {
                p.push(FieldProblem::new(
                    format!("{path}.{}", def["name"].as_str().unwrap_or("")),
                    "BASE_TYPE_REQUIRED",
                    "This field is required",
                ));
            }
        }
    }
    out
}

fn snowflake(v: &Value) -> Option<i64> {
    v.as_str()?.parse::<i64>().ok().filter(|n| *n > 0)
}

/// Returns a message when `v` is not acceptable for the option definition.
fn value_problem(
    def: &Value,
    ty: i64,
    v: &Value,
    partial: bool,
    refs: &mut Referenced,
) -> Option<&'static str> {
    match ty {
        STRING => {
            let Some(s) = v.as_str() else {
                return Some("Must be a string.");
            };
            if let Some(min) = def["min_length"].as_i64()
                && (s.chars().count() as i64) < min
                && !partial
            {
                return Some("Value is shorter than min_length.");
            }
            if let Some(max) = def["max_length"].as_i64()
                && s.chars().count() as i64 > max
            {
                return Some("Value is longer than max_length.");
            }
            if !partial
                && let Some(choices) = def["choices"].as_array()
                && !choices.is_empty()
                && !choices.iter().any(|c| c["value"] == *v)
            {
                return Some("Value is not one of the choices.");
            }
            None
        }
        INTEGER => {
            let Some(n) = v.as_i64() else {
                return if v.is_string() && partial {
                    None
                } else {
                    Some("Must be an integer.")
                };
            };
            number_problem(def, n as f64, v, partial)
        }
        NUMBER => {
            let Some(n) = v.as_f64() else {
                return if v.is_string() && partial {
                    None
                } else {
                    Some("Must be a number.")
                };
            };
            number_problem(def, n, v, partial)
        }
        BOOLEAN => (!v.is_boolean()).then_some("Must be a boolean."),
        USER => snowflake(v).map_or(Some("Must be a user id."), |id| {
            refs.users.push(id);
            None
        }),
        ROLE => snowflake(v).map_or(Some("Must be a role id."), |id| {
            refs.roles.push(id);
            None
        }),
        CHANNEL => snowflake(v).map_or(Some("Must be a channel id."), |id| {
            refs.channels.push(id);
            None
        }),
        MENTIONABLE => snowflake(v).map_or(Some("Must be a user or role id."), |id| {
            refs.mentionables.push(id);
            None
        }),
        ATTACHMENT => snowflake(v).map_or(Some("Must be an attachment id."), |id| {
            refs.attachments.push(id);
            None
        }),
        _ => Some("Not a value option."),
    }
}

fn number_problem(def: &Value, n: f64, v: &Value, partial: bool) -> Option<&'static str> {
    if partial {
        return None;
    }
    if let Some(min) = def["min_value"].as_f64()
        && n < min
    {
        return Some("Value is below min_value.");
    }
    if let Some(max) = def["max_value"].as_f64()
        && n > max
    {
        return Some("Value is above max_value.");
    }
    if let Some(choices) = def["choices"].as_array()
        && !choices.is_empty()
        && !choices.iter().any(|c| c["value"] == *v)
    {
        return Some("Value is not one of the choices.");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::validate_command;

    fn cmd(body: Value) -> Command {
        let n = validate_command(&body).unwrap();
        Command {
            id: 1,
            application_id: 2,
            guild_id: None,
            kind: n.kind,
            name: n.name,
            description: n.description,
            options: n.options,
            name_localizations: None,
            description_localizations: None,
            default_member_permissions: None,
            dm_permission: true,
            nsfw: false,
            contexts: None,
            integration_types: None,
            version: 1,
        }
    }

    fn ban() -> Command {
        cmd(json!({"name": "ban", "description": "d", "options": [
            {"type": 6, "name": "user", "description": "d", "required": true},
            {"type": 4, "name": "days", "description": "d", "min_value": 0, "max_value": 7},
            {"type": 3, "name": "why", "description": "d", "choices": [
                {"name": "spam", "value": "spam"}], "required": false},
        ]}))
    }

    #[test]
    fn valid_options_are_normalised_and_resolved() {
        let c = check_options(
            &ban(),
            &json!([{"name": "user", "type": 6, "value": "42", "junk": 1}, {"name": "days", "type": 4, "value": 3}]),
            false,
        )
        .unwrap();
        assert_eq!(c.referenced.users, vec![42]);
        assert_eq!(
            c.options[0],
            json!({"name": "user", "type": 6, "value": "42"})
        );
    }

    #[test]
    fn invalid_submissions_are_refused() {
        let b = ban();
        let bad = |v: Value| check_options(&b, &v, false).is_err();
        assert!(bad(json!([]))); // required missing
        assert!(bad(json!([{"name": "user", "type": 6, "value": "x"}])));
        assert!(bad(json!([{"name": "user", "type": 3, "value": "42"}]))); // type mismatch
        assert!(bad(json!([{"name": "nope", "type": 6, "value": "42"}])));
        assert!(bad(
            json!([{"name": "user", "type": 6, "value": "42"}, {"name": "days", "type": 4, "value": 9}])
        ));
        assert!(bad(
            json!([{"name": "user", "type": 6, "value": "42"}, {"name": "days", "type": 4, "value": "3"}])
        ));
        assert!(bad(
            json!([{"name": "user", "type": 6, "value": "42"}, {"name": "why", "type": 3, "value": "other"}])
        ));
        assert!(bad(
            json!([{"name": "user", "type": 6, "value": "42", "focused": true}])
        ));
    }

    #[test]
    fn subcommands_need_exactly_one_choice() {
        let c = cmd(json!({"name": "cfg", "description": "d", "options": [
            {"type": 1, "name": "get", "description": "d"},
            {"type": 1, "name": "set", "description": "d", "options": [
                {"type": 3, "name": "v", "description": "d", "required": true}]}]}));
        assert!(check_options(&c, &json!([]), false).is_err());
        assert!(
            check_options(
                &c,
                &json!([{"name": "get", "type": 1}, {"name": "set", "type": 1}]),
                false
            )
            .is_err()
        );
        assert!(
            check_options(
                &c,
                &json!([{"name": "set", "type": 1, "options": []}]),
                false
            )
            .is_err()
        );
        let ok = check_options(
            &c,
            &json!([{"name": "set", "type": 1, "options": [{"name": "v", "type": 3, "value": "x"}]}]),
            false,
        )
        .unwrap();
        assert_eq!(ok.options[0]["options"][0]["value"], "x");
    }

    #[test]
    fn autocomplete_needs_a_focused_option_and_allows_partial_text() {
        let c = cmd(json!({"name": "find", "description": "d", "options": [
            {"type": 3, "name": "q", "description": "d", "required": true, "autocomplete": true, "min_length": 3},
            {"type": 4, "name": "n", "description": "d", "required": true}]}));
        let r = check_options(
            &c,
            &json!([{"name": "q", "type": 3, "value": "ab", "focused": true}]),
            true,
        )
        .unwrap();
        assert_eq!(r.focused.as_deref(), Some("q"));
        assert!(
            check_options(&c, &json!([{"name": "q", "type": 3, "value": "abc"}]), true).is_err()
        );
        // a focused option in a non-autocomplete interaction is refused
        assert!(check_options(&c, &json!([{"name": "q", "type": 3, "value": "abc", "focused": true}, {"name": "n", "type": 4, "value": 1}]), false).is_err());
    }

    #[test]
    fn callbacks_fit_the_interaction_kind() {
        use ctype::*;
        assert!(callback_allowed(
            itype::APPLICATION_COMMAND,
            CHANNEL_MESSAGE
        ));
        assert!(callback_allowed(itype::APPLICATION_COMMAND, MODAL));
        assert!(!callback_allowed(
            itype::APPLICATION_COMMAND,
            UPDATE_MESSAGE
        ));
        assert!(!callback_allowed(
            itype::APPLICATION_COMMAND,
            DEFERRED_UPDATE_MESSAGE
        ));
        assert!(callback_allowed(itype::MESSAGE_COMPONENT, UPDATE_MESSAGE));
        assert!(!callback_allowed(itype::MODAL_SUBMIT, MODAL));
        assert!(callback_allowed(itype::AUTOCOMPLETE, AUTOCOMPLETE_RESULT));
        assert!(!callback_allowed(itype::AUTOCOMPLETE, CHANNEL_MESSAGE));
        assert!(!callback_allowed(itype::APPLICATION_COMMAND, 12));
    }

    #[test]
    fn message_data_limits_and_flags() {
        let d = parse_message_data(
            &json!({"content": "hi", "flags": 64 | 4 | 1 << 12}),
            "data",
            true,
        )
        .unwrap();
        assert!(d.ephemeral());
        assert_eq!(d.flags, EPHEMERAL | SUPPRESS_EMBEDS);
        assert!(parse_message_data(&json!({}), "data", true).is_err());
        assert!(parse_message_data(&json!({}), "data", false).is_ok());
        assert!(parse_message_data(&json!({"content": "x".repeat(2001)}), "data", true).is_err());
        assert!(
            parse_message_data(
                &json!({"content": "x", "embeds": vec![json!({}); 11]}),
                "data",
                true
            )
            .is_err()
        );
        assert!(parse_choices(&json!({"choices": [{"name": "a", "value": 1}]})).is_ok());
        assert!(
            parse_choices(&json!({"choices": vec![json!({"name": "a", "value": 1}); 26]})).is_err()
        );
        assert!(parse_choices(&json!({})).is_err());
        let m =
            json!({"custom_id": "m", "title": "T", "components": [{"type": 1, "components": []}]});
        assert!(parse_modal(&m).is_ok());
        assert!(
            parse_modal(
                &json!({"custom_id": "m", "title": "T".repeat(46), "components": [{"type": 1}]})
            )
            .is_err()
        );
        assert!(parse_modal(&json!({"custom_id": "m", "title": "T", "components": []})).is_err());
    }
}
