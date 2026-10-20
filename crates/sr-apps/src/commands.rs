//! Application command validation (DEV-BOT-003): names, descriptions, options schema,
//! localizations, limits. Pure functions over JSON; the Discord rules are copied from the
//! public API reference.

use crate::{
    error::FieldProblem,
    model::{Command, NewCommand},
};
use serde_json::{Map, Value, json};

/// Chat input (slash) commands per application and scope.
pub const MAX_CHAT_INPUT: usize = 100;
/// User commands per application and scope.
pub const MAX_USER: usize = 15;
/// Message commands per application and scope.
pub const MAX_MESSAGE: usize = 15;
/// Total characters across names, descriptions and option text (Discord: 8000).
pub const MAX_TOTAL_CHARS: usize = 8000;
/// Options per level.
pub const MAX_OPTIONS: usize = 25;
/// Choices per option.
pub const MAX_CHOICES: usize = 25;

/// Option types.
pub mod option_type {
    /// Sub-command.
    pub const SUB_COMMAND: i64 = 1;
    /// Sub-command group.
    pub const SUB_COMMAND_GROUP: i64 = 2;
    /// String.
    pub const STRING: i64 = 3;
    /// Integer.
    pub const INTEGER: i64 = 4;
    /// Boolean.
    pub const BOOLEAN: i64 = 5;
    /// User.
    pub const USER: i64 = 6;
    /// Channel.
    pub const CHANNEL: i64 = 7;
    /// Role.
    pub const ROLE: i64 = 8;
    /// User or role.
    pub const MENTIONABLE: i64 = 9;
    /// Floating point.
    pub const NUMBER: i64 = 10;
    /// Attachment.
    pub const ATTACHMENT: i64 = 11;
}

type Problems = Vec<FieldProblem>;

fn push(p: &mut Problems, path: &str, code: &str, msg: impl Into<String>) {
    p.push(FieldProblem::new(path, code, msg));
}

fn chars(s: &str) -> usize {
    s.chars().count()
}

/// Lowercase letters/digits/`-`/`_` (any script) in 1 to 32 characters.
fn slash_name_ok(name: &str) -> bool {
    let n = chars(name);
    (1..=32).contains(&n)
        && name.chars().all(|c| {
            (c.is_alphanumeric() || c == '-' || c == '_')
                && c.to_lowercase().collect::<String>() == c.to_string()
        })
}

fn context_name_ok(name: &str) -> bool {
    let n = chars(name);
    (1..=32).contains(&n) && name.trim() == name && !name.chars().any(char::is_control)
}

fn text_ok(s: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&chars(s)) && !s.chars().any(|c| c.is_control() && c != '\n')
}

fn localizations(
    v: Option<&Value>,
    path: &str,
    slash_name: bool,
    min: usize,
    max: usize,
    out: &mut Problems,
    total: &mut usize,
) -> Option<Value> {
    let v = v?;
    if v.is_null() {
        return None;
    }
    let Some(map) = v.as_object() else {
        push(
            out,
            path,
            "MODEL_TYPE_CONVERT",
            "Must be an object of locale to text.",
        );
        return None;
    };
    if map.len() > 40 {
        push(out, path, "BASE_TYPE_MAX_LENGTH", "Too many locales.");
        return None;
    }
    let mut clean = Map::new();
    for (locale, text) in map {
        let ok_locale = (2..=8).contains(&locale.len())
            && locale
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-');
        let Some(t) = text.as_str() else {
            push(
                out,
                &format!("{path}.{locale}"),
                "BASE_TYPE_REQUIRED",
                "Must be a string.",
            );
            continue;
        };
        let valid = if slash_name {
            slash_name_ok(t)
        } else {
            text_ok(t, min, max)
        };
        if !ok_locale || !valid {
            push(
                out,
                &format!("{path}.{locale}"),
                "APPLICATION_COMMAND_INVALID_NAME",
                "Invalid locale or text.",
            );
            continue;
        }
        *total += chars(t);
        clean.insert(locale.clone(), json!(t));
    }
    Some(Value::Object(clean))
}

/// Validate a full command body into a [`NewCommand`].
pub fn validate_command(body: &Value) -> Result<NewCommand, Problems> {
    let mut p = Problems::new();
    let Some(obj) = body.as_object() else {
        return Err(vec![FieldProblem::new(
            "",
            "MODEL_TYPE_CONVERT",
            "Request body must be an object.",
        )]);
    };
    let mut total = 0usize;
    let kind = match obj.get("type") {
        None | Some(Value::Null) => 1,
        Some(v) => match v.as_i64() {
            Some(k @ 1..=3) => k as i16,
            _ => {
                push(
                    &mut p,
                    "type",
                    "BASE_TYPE_CHOICES",
                    "Must be one of (1, 2, 3).",
                );
                1
            }
        },
    };
    let name = obj.get("name").and_then(Value::as_str).unwrap_or("");
    if obj.get("name").is_none() {
        push(
            &mut p,
            "name",
            "BASE_TYPE_REQUIRED",
            "This field is required",
        );
    } else if kind == 1 && !slash_name_ok(name) || kind != 1 && !context_name_ok(name) {
        push(
            &mut p,
            "name",
            "APPLICATION_COMMAND_INVALID_NAME",
            "Command name is invalid.",
        );
    }
    total += chars(name);

    let desc = obj.get("description").and_then(Value::as_str).unwrap_or("");
    if kind == 1 {
        if !text_ok(desc, 1, 100) {
            push(
                &mut p,
                "description",
                "BASE_TYPE_BAD_LENGTH",
                "Must be between 1 and 100 in length.",
            );
        }
    } else if !desc.is_empty() {
        push(
            &mut p,
            "description",
            "BASE_TYPE_BAD_LENGTH",
            "User and message commands have no description.",
        );
    }
    total += chars(desc);

    let name_loc = localizations(
        obj.get("name_localizations"),
        "name_localizations",
        kind == 1,
        1,
        32,
        &mut p,
        &mut total,
    );
    let desc_loc = localizations(
        obj.get("description_localizations"),
        "description_localizations",
        false,
        1,
        100,
        &mut p,
        &mut total,
    );

    let options = match obj.get("options") {
        None | Some(Value::Null) => json!([]),
        Some(Value::Array(a)) if a.is_empty() => json!([]),
        Some(Value::Array(a)) => {
            if kind != 1 {
                push(
                    &mut p,
                    "options",
                    "BASE_TYPE_BAD_LENGTH",
                    "User and message commands take no options.",
                );
                json!([])
            } else {
                Value::Array(validate_options(a, "options", 0, &mut p, &mut total))
            }
        }
        Some(_) => {
            push(&mut p, "options", "MODEL_TYPE_CONVERT", "Must be an array.");
            json!([])
        }
    };

    let default_member_permissions = match obj.get("default_member_permissions") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => match s.parse::<u64>() {
            Ok(v) => Some(v),
            Err(_) => {
                push(
                    &mut p,
                    "default_member_permissions",
                    "NUMBER_TYPE_COERCE",
                    "Not a bitfield.",
                );
                None
            }
        },
        Some(Value::Number(n)) => n.as_u64().or_else(|| {
            push(
                &mut p,
                "default_member_permissions",
                "NUMBER_TYPE_COERCE",
                "Not a bitfield.",
            );
            None
        }),
        Some(_) => {
            push(
                &mut p,
                "default_member_permissions",
                "NUMBER_TYPE_COERCE",
                "Not a bitfield.",
            );
            None
        }
    };
    let flag = |key: &str, default: bool, p: &mut Problems| match obj.get(key) {
        None | Some(Value::Null) => default,
        Some(Value::Bool(b)) => *b,
        Some(_) => {
            push(p, key, "BOOLEAN_TYPE_COERCE", "Must be a boolean.");
            default
        }
    };
    let dm_permission = flag("dm_permission", true, &mut p);
    let nsfw = flag("nsfw", false, &mut p);
    let int_list = |key: &str, p: &mut Problems| -> Option<Value> {
        match obj.get(key) {
            None | Some(Value::Null) => None,
            Some(Value::Array(a))
                if a.len() <= 8
                    && a.iter()
                        .all(|x| x.as_i64().is_some_and(|n| (0..=3).contains(&n))) =>
            {
                Some(Value::Array(a.clone()))
            }
            Some(_) => {
                push(p, key, "BASE_TYPE_CHOICES", "Must be an array of 0 to 3.");
                None
            }
        }
    };
    let contexts = int_list("contexts", &mut p);
    let integration_types = normalise_integration_types(int_list("integration_types", &mut p));

    if total > MAX_TOTAL_CHARS {
        push(
            &mut p,
            "",
            "APPLICATION_COMMAND_TOO_LARGE",
            "Command exceeds maximum size (8000).",
        );
    }
    if !p.is_empty() {
        return Err(p);
    }
    Ok(NewCommand {
        kind,
        name: name.to_owned(),
        description: desc.to_owned(),
        options,
        name_localizations: name_loc,
        description_localizations: desc_loc,
        default_member_permissions,
        dm_permission,
        nsfw,
        contexts,
        integration_types,
    })
}

/// Normalises `integration_types` (D-305): a list containing `0` (guild install) becomes
/// `[0]`; anything else (for example `[1]`, user install) is stored as given, de-duplicated.
/// User installs are not supported, so such a command is never offered in a guild (see
/// [`integration_types_guild_offered`]), but registering it succeeds so libraries keep working.
pub fn normalise_integration_types(v: Option<Value>) -> Option<Value> {
    let Some(Value::Array(a)) = v else {
        return v;
    };
    if a.iter().any(|x| x.as_i64() == Some(0)) {
        return Some(json!([0]));
    }
    let mut out: Vec<Value> = Vec::new();
    for x in a {
        if !out.contains(&x) {
            out.push(x);
        }
    }
    Some(Value::Array(out))
}

/// Is a command with these `integration_types` offered in guilds? Absent or empty means the
/// default (guild install); a list without `0` (for example `[1]`) is user-install only and is
/// never offered, listed, autocompleted or dispatched (D-305).
pub fn integration_types_guild_offered(v: &Option<Value>) -> bool {
    match v {
        Some(Value::Array(a)) if !a.is_empty() => a.iter().any(|x| x.as_i64() == Some(0)),
        _ => true,
    }
}

fn validate_options(
    list: &[Value],
    path: &str,
    depth: u8,
    p: &mut Problems,
    total: &mut usize,
) -> Vec<Value> {
    use option_type::*;
    let mut out = Vec::new();
    if list.len() > MAX_OPTIONS {
        push(
            p,
            path,
            "BASE_TYPE_MAX_LENGTH",
            "Must be 25 or fewer in length.",
        );
        return out;
    }
    let mut seen: Vec<String> = Vec::new();
    let mut had_optional = false;
    let (mut subs, mut plain) = (0, 0);
    for (i, raw) in list.iter().enumerate() {
        let at = format!("{path}.{i}");
        let Some(o) = raw.as_object() else {
            push(p, &at, "MODEL_TYPE_CONVERT", "Must be an object.");
            continue;
        };
        let ty = o.get("type").and_then(Value::as_i64).unwrap_or(0);
        if !(1..=11).contains(&ty) {
            push(
                p,
                &format!("{at}.type"),
                "BASE_TYPE_CHOICES",
                "Must be one of (1 ... 11).",
            );
            continue;
        }
        if ty <= SUB_COMMAND_GROUP {
            subs += 1;
        } else {
            plain += 1;
        }
        let name = o.get("name").and_then(Value::as_str).unwrap_or("");
        if !slash_name_ok(name) {
            push(
                p,
                &format!("{at}.name"),
                "APPLICATION_COMMAND_INVALID_NAME",
                "Option name is invalid.",
            );
        } else if seen.iter().any(|s| s == name) {
            push(
                p,
                &format!("{at}.name"),
                "APPLICATION_COMMAND_OPTIONS_NAMES_UNIQUE",
                "Option names must be unique.",
            );
        } else {
            seen.push(name.to_owned());
        }
        let desc = o.get("description").and_then(Value::as_str).unwrap_or("");
        if !text_ok(desc, 1, 100) {
            push(
                p,
                &format!("{at}.description"),
                "BASE_TYPE_BAD_LENGTH",
                "Must be between 1 and 100 in length.",
            );
        }
        *total += chars(name) + chars(desc);
        let mut clean = Map::new();
        clean.insert("type".into(), json!(ty));
        clean.insert("name".into(), json!(name));
        clean.insert("description".into(), json!(desc));
        if let Some(v) = localizations(
            o.get("name_localizations"),
            &format!("{at}.name_localizations"),
            true,
            1,
            32,
            p,
            total,
        ) {
            clean.insert("name_localizations".into(), v);
        }
        if let Some(v) = localizations(
            o.get("description_localizations"),
            &format!("{at}.description_localizations"),
            false,
            1,
            100,
            p,
            total,
        ) {
            clean.insert("description_localizations".into(), v);
        }
        let required = o.get("required").and_then(Value::as_bool).unwrap_or(false);
        if ty > SUB_COMMAND_GROUP {
            if required && had_optional {
                push(
                    p,
                    &format!("{at}.required"),
                    "APPLICATION_COMMAND_OPTIONS_REQUIRED_FIRST",
                    "Required options must be placed before non-required options",
                );
            }
            had_optional |= !required;
            if required {
                clean.insert("required".into(), json!(true));
            }
        }
        match ty {
            SUB_COMMAND | SUB_COMMAND_GROUP => {
                let inner = match o.get("options") {
                    Some(Value::Array(a)) => a.as_slice(),
                    _ => &[],
                };
                let max_depth = if ty == SUB_COMMAND_GROUP { 0 } else { 1 };
                if depth > max_depth || (depth == 1 && ty == SUB_COMMAND_GROUP) {
                    push(
                        p,
                        &at,
                        "APPLICATION_COMMAND_TOO_DEEP",
                        "Sub-commands nest at most two levels.",
                    );
                } else {
                    let nested =
                        validate_options(inner, &format!("{at}.options"), depth + 1, p, total);
                    let all_sub = nested.iter().all(|n| n["type"] == SUB_COMMAND);
                    let none_sub = nested
                        .iter()
                        .all(|n| n["type"].as_i64().is_some_and(|t| t > SUB_COMMAND_GROUP));
                    if ty == SUB_COMMAND_GROUP && (!all_sub || nested.is_empty()) {
                        push(
                            p,
                            &format!("{at}.options"),
                            "APPLICATION_COMMAND_GROUP_NEEDS_SUBCOMMANDS",
                            "A sub-command group holds sub-commands only.",
                        );
                    }
                    if ty == SUB_COMMAND && !none_sub {
                        push(
                            p,
                            &format!("{at}.options"),
                            "APPLICATION_COMMAND_SUBCOMMAND_NEEDS_PLAIN_OPTIONS",
                            "A sub-command holds plain options only.",
                        );
                    }
                    if !nested.is_empty() {
                        clean.insert("options".into(), Value::Array(nested));
                    }
                }
            }
            _ => {
                if o.get("options").is_some_and(|v| !v.is_null()) {
                    push(
                        p,
                        &format!("{at}.options"),
                        "BASE_TYPE_BAD_LENGTH",
                        "Only sub-commands take options.",
                    );
                }
                plain_option(o, ty, &at, &mut clean, p, total);
            }
        }
        out.push(Value::Object(clean));
    }
    if subs > 0 && plain > 0 {
        push(
            p,
            path,
            "APPLICATION_COMMAND_MIXED_OPTIONS",
            "Sub-commands cannot be mixed with plain options.",
        );
    }
    out
}

fn plain_option(
    o: &Map<String, Value>,
    ty: i64,
    at: &str,
    clean: &mut Map<String, Value>,
    p: &mut Problems,
    total: &mut usize,
) {
    use option_type::*;
    let choices = o.get("choices").filter(|v| !v.is_null());
    let autocomplete = o
        .get("autocomplete")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if let Some(c) = choices {
        if !matches!(ty, STRING | INTEGER | NUMBER) {
            push(
                p,
                &format!("{at}.choices"),
                "BASE_TYPE_BAD_LENGTH",
                "Choices need a string, integer or number option.",
            );
        } else if let Some(arr) = c.as_array() {
            if autocomplete && !arr.is_empty() {
                push(
                    p,
                    &format!("{at}.autocomplete"),
                    "APPLICATION_COMMAND_OPTIONS_AUTOCOMPLETE_CHOICES",
                    "Autocomplete and choices are mutually exclusive.",
                );
            }
            if arr.len() > MAX_CHOICES {
                push(
                    p,
                    &format!("{at}.choices"),
                    "BASE_TYPE_MAX_LENGTH",
                    "Must be 25 or fewer in length.",
                );
            }
            let mut clean_choices = Vec::new();
            let mut names: Vec<&str> = Vec::new();
            for (j, ch) in arr.iter().enumerate() {
                let cat = format!("{at}.choices.{j}");
                let cname = ch.get("name").and_then(Value::as_str).unwrap_or("");
                if !text_ok(cname, 1, 100) {
                    push(
                        p,
                        &format!("{cat}.name"),
                        "BASE_TYPE_BAD_LENGTH",
                        "Must be between 1 and 100 in length.",
                    );
                } else if names.contains(&cname) {
                    push(
                        p,
                        &format!("{cat}.name"),
                        "APPLICATION_COMMAND_OPTIONS_CHOICES_NAMES_UNIQUE",
                        "Choice names must be unique.",
                    );
                }
                names.push(cname);
                *total += chars(cname);
                let value = ch.get("value");
                let ok = match (ty, value) {
                    (STRING, Some(Value::String(s))) => {
                        *total += chars(s);
                        chars(s) <= 100
                    }
                    (INTEGER, Some(v)) => {
                        v.is_i64()
                            || v.as_f64()
                                .is_some_and(|f| f.fract() == 0.0 && f.abs() < 9.007e15)
                    }
                    (NUMBER, Some(v)) => v.is_number(),
                    _ => false,
                };
                if !ok {
                    push(
                        p,
                        &format!("{cat}.value"),
                        "BASE_TYPE_CHOICES",
                        "Choice value does not match the option type.",
                    );
                }
                let mut cc = Map::new();
                cc.insert("name".into(), json!(cname));
                if let Some(v) = value {
                    cc.insert("value".into(), v.clone());
                }
                if let Some(v) = localizations(
                    ch.get("name_localizations"),
                    &format!("{cat}.name_localizations"),
                    false,
                    1,
                    100,
                    p,
                    total,
                ) {
                    cc.insert("name_localizations".into(), v);
                }
                clean_choices.push(Value::Object(cc));
            }
            if !clean_choices.is_empty() {
                clean.insert("choices".into(), Value::Array(clean_choices));
            }
        } else {
            push(
                p,
                &format!("{at}.choices"),
                "MODEL_TYPE_CONVERT",
                "Must be an array.",
            );
        }
    }
    if autocomplete {
        if !matches!(ty, STRING | INTEGER | NUMBER) {
            push(
                p,
                &format!("{at}.autocomplete"),
                "BASE_TYPE_BAD_LENGTH",
                "Autocomplete needs a string, integer or number option.",
            );
        } else {
            clean.insert("autocomplete".into(), json!(true));
        }
    }
    if let Some(ct) = o.get("channel_types").filter(|v| !v.is_null()) {
        let ok = ty == CHANNEL
            && ct.as_array().is_some_and(|a| {
                a.len() <= 16
                    && a.iter()
                        .all(|x| x.as_i64().is_some_and(|n| (0..=16).contains(&n)))
            });
        if ok {
            clean.insert("channel_types".into(), ct.clone());
        } else {
            push(
                p,
                &format!("{at}.channel_types"),
                "BASE_TYPE_CHOICES",
                "Only channel options take channel types.",
            );
        }
    }
    for key in ["min_value", "max_value"] {
        if let Some(v) = o.get(key).filter(|v| !v.is_null()) {
            let ok = match ty {
                INTEGER => v.is_i64() || v.as_f64().is_some_and(|f| f.fract() == 0.0),
                NUMBER => v.is_number(),
                _ => false,
            };
            if ok {
                clean.insert(key.into(), v.clone());
            } else {
                push(
                    p,
                    &format!("{at}.{key}"),
                    "NUMBER_TYPE_COERCE",
                    "Only integer and number options take bounds.",
                );
            }
        }
    }
    if let (Some(lo), Some(hi)) = (
        clean.get("min_value").and_then(Value::as_f64),
        clean.get("max_value").and_then(Value::as_f64),
    ) && lo > hi
    {
        push(
            p,
            &format!("{at}.min_value"),
            "NUMBER_TYPE_MAX",
            "min_value cannot exceed max_value.",
        );
    }
    for (key, lo, hi) in [("min_length", 0, 6000), ("max_length", 1, 6000)] {
        if let Some(v) = o.get(key).filter(|v| !v.is_null()) {
            match v.as_i64() {
                Some(n) if ty == STRING && (lo..=hi).contains(&n) => {
                    clean.insert(key.into(), json!(n));
                }
                _ => push(
                    p,
                    &format!("{at}.{key}"),
                    "BASE_TYPE_BAD_LENGTH",
                    "Invalid string length bound.",
                ),
            }
        }
    }
    if let (Some(lo), Some(hi)) = (
        clean.get("min_length").and_then(Value::as_i64),
        clean.get("max_length").and_then(Value::as_i64),
    ) && lo > hi
    {
        push(
            p,
            &format!("{at}.min_length"),
            "BASE_TYPE_BAD_LENGTH",
            "min_length cannot exceed max_length.",
        );
    }
}

/// Merge a PATCH body onto an existing command and validate the result.
pub fn validate_patch(existing: &Command, patch: &Value) -> Result<NewCommand, Problems> {
    let mut merged = command_json(existing, true);
    if let (Some(m), Some(patch)) = (merged.as_object_mut(), patch.as_object()) {
        for (k, v) in patch {
            // The type of a command cannot change.
            if k != "type" {
                m.insert(k.clone(), v.clone());
            }
        }
    }
    validate_command(&merged)
}

/// The Discord command object. `storage` form keeps `default_member_permissions` numeric
/// input-compatible (used for merging); the API form renders it as a string.
pub fn command_json(c: &Command, storage: bool) -> Value {
    let mut o = Map::new();
    o.insert("id".into(), json!(c.id.to_string()));
    o.insert("application_id".into(), json!(c.application_id.to_string()));
    o.insert("version".into(), json!(c.version.to_string()));
    o.insert(
        "default_member_permissions".into(),
        c.default_member_permissions
            .map_or(Value::Null, |v| json!(v.to_string())),
    );
    o.insert("type".into(), json!(c.kind));
    o.insert("name".into(), json!(c.name));
    o.insert(
        "name_localizations".into(),
        c.name_localizations.clone().unwrap_or(Value::Null),
    );
    o.insert("description".into(), json!(c.description));
    o.insert(
        "description_localizations".into(),
        c.description_localizations.clone().unwrap_or(Value::Null),
    );
    if let Some(g) = c.guild_id {
        o.insert("guild_id".into(), json!(g.to_string()));
    }
    o.insert("dm_permission".into(), json!(c.dm_permission));
    if c.options.as_array().is_some_and(|a| !a.is_empty()) {
        o.insert("options".into(), c.options.clone());
    }
    o.insert("nsfw".into(), json!(c.nsfw));
    o.insert("contexts".into(), c.contexts.clone().unwrap_or(Value::Null));
    // Discord answers `[0]` (guild install) for a command registered without the field, and typed
    // libraries rely on a list (serenity 0.12's `Command::integration_types` is a `Vec`; null fails to
    // decode). Only server installs exist (D-305). The merge input keeps "absent".
    o.insert(
        "integration_types".into(),
        match (&c.integration_types, storage) {
            (Some(v), _) => v.clone(),
            (None, true) => Value::Null,
            (None, false) => json!([0]),
        },
    );
    if storage {
        // Merge input: ids and server fields are ignored by validation.
        o.remove("id");
        o.remove("application_id");
        o.remove("version");
        o.remove("guild_id");
    }
    Value::Object(o)
}

/// Per-scope counts check. `existing` holds the kinds already stored (excluding any command
/// about to be replaced); returns the offending kind's message.
pub fn check_limit(existing_kinds: &[i16], new_kind: i16) -> Result<(), &'static str> {
    let n = existing_kinds.iter().filter(|k| **k == new_kind).count();
    let (max, msg) = match new_kind {
        1 => (
            MAX_CHAT_INPUT,
            "Maximum number of chat input commands reached (100).",
        ),
        2 => (MAX_USER, "Maximum number of user commands reached (15)."),
        _ => (
            MAX_MESSAGE,
            "Maximum number of message commands reached (15).",
        ),
    };
    if n >= max { Err(msg) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(v: Value) -> NewCommand {
        validate_command(&v).unwrap_or_else(|e| panic!("{e:?}"))
    }

    fn bad(v: Value) -> Problems {
        validate_command(&v).unwrap_err()
    }

    #[test]
    fn minimal_slash_command() {
        let c = ok(json!({"name": "ping", "description": "Pong"}));
        assert_eq!(
            (c.kind, c.name.as_str(), c.options.clone()),
            (1, "ping", json!([]))
        );
        assert!(c.dm_permission && !c.nsfw);
    }

    #[test]
    fn names_follow_discord_rules() {
        for n in ["", "Ping", "has space", &"a".repeat(33), "emoji😀"] {
            assert!(
                !bad(json!({"name": n, "description": "x"})).is_empty(),
                "{n:?}"
            );
        }
        ok(json!({"name": "my-cmd_2", "description": "x"}));
        ok(json!({"name": "пинг", "description": "x"}));
        // context menu commands may use capitals and spaces
        ok(json!({"name": "Report Message", "type": 3}));
        assert!(!bad(json!({"name": "Report", "type": 3, "description": "no"})).is_empty());
        assert!(!bad(json!({"name": " Report", "type": 2})).is_empty());
        assert!(!bad(json!({"name": "x", "description": ""})).is_empty());
        assert!(!bad(json!({"name": "x", "description": "d".repeat(101)})).is_empty());
        assert!(!bad(json!({"name": "x", "description": "d", "type": 4})).is_empty());
        assert!(!bad(json!({"description": "d"})).is_empty());
    }

    #[test]
    fn options_schema() {
        let c = ok(json!({"name": "ban", "description": "d", "options": [
            {"type": 6, "name": "user", "description": "who", "required": true},
            {"type": 3, "name": "reason", "description": "why", "min_length": 1, "max_length": 100,
             "choices": [{"name": "spam", "value": "spam"}, {"name": "raid", "value": "raid"}]},
            {"type": 4, "name": "days", "description": "n", "min_value": 0, "max_value": 7},
            {"type": 7, "name": "where", "description": "c", "channel_types": [0, 2]},
        ]}));
        assert_eq!(c.options.as_array().unwrap().len(), 4);
        assert_eq!(c.options[0]["required"], true);
        // required after optional
        assert!(
            !bad(json!({"name": "x", "description": "d", "options": [
            {"type": 3, "name": "a", "description": "d"},
            {"type": 3, "name": "b", "description": "d", "required": true}]}))
            .is_empty()
        );
        // duplicate names
        assert!(
            !bad(json!({"name": "x", "description": "d", "options": [
            {"type": 3, "name": "a", "description": "d"},
            {"type": 3, "name": "a", "description": "d"}]}))
            .is_empty()
        );
        // choices and autocomplete exclusive; wrong choice value type
        assert!(
            !bad(json!({"name": "x", "description": "d", "options": [
            {"type": 3, "name": "a", "description": "d", "autocomplete": true,
             "choices": [{"name": "n", "value": "v"}]}]}))
            .is_empty()
        );
        assert!(!bad(json!({"name": "x", "description": "d", "options": [
            {"type": 4, "name": "a", "description": "d", "choices": [{"name": "n", "value": "v"}]}]})).is_empty());
        // bounds on the wrong type, inverted bounds, bad type number
        assert!(
            !bad(json!({"name": "x", "description": "d", "options": [
            {"type": 3, "name": "a", "description": "d", "min_value": 1}]}))
            .is_empty()
        );
        assert!(
            !bad(json!({"name": "x", "description": "d", "options": [
            {"type": 4, "name": "a", "description": "d", "min_value": 5, "max_value": 1}]}))
            .is_empty()
        );
        assert!(
            !bad(json!({"name": "x", "description": "d", "options": [
            {"type": 12, "name": "a", "description": "d"}]}))
            .is_empty()
        );
        // too many options / choices
        let many: Vec<Value> = (0..26)
            .map(|i| json!({"type": 5, "name": format!("o{i}"), "description": "d"}))
            .collect();
        assert!(!bad(json!({"name": "x", "description": "d", "options": many})).is_empty());
    }

    #[test]
    fn subcommands_nest_two_levels() {
        ok(json!({"name": "config", "description": "d", "options": [
            {"type": 2, "name": "user", "description": "d", "options": [
                {"type": 1, "name": "set", "description": "d", "options": [
                    {"type": 3, "name": "value", "description": "d", "required": true}]}]},
            {"type": 1, "name": "reset", "description": "d"}]}));
        // group inside a group
        assert!(
            !bad(json!({"name": "c", "description": "d", "options": [
            {"type": 2, "name": "g", "description": "d", "options": [
                {"type": 2, "name": "h", "description": "d", "options": [
                    {"type": 1, "name": "s", "description": "d"}]}]}]}))
            .is_empty()
        );
        // mixing sub-commands with plain options at one level
        assert!(
            !bad(json!({"name": "c", "description": "d", "options": [
            {"type": 1, "name": "s", "description": "d"},
            {"type": 3, "name": "p", "description": "d"}]}))
            .is_empty()
        );
        // a group must hold sub-commands
        assert!(
            !bad(json!({"name": "c", "description": "d", "options": [
            {"type": 2, "name": "g", "description": "d", "options": [
                {"type": 3, "name": "p", "description": "d"}]}]}))
            .is_empty()
        );
        // a sub-command cannot hold a sub-command
        assert!(
            !bad(json!({"name": "c", "description": "d", "options": [
            {"type": 1, "name": "s", "description": "d", "options": [
                {"type": 1, "name": "t", "description": "d"}]}]}))
            .is_empty()
        );
    }

    #[test]
    fn user_and_message_commands_take_no_options() {
        assert!(
            !bad(json!({"name": "Hi", "type": 2, "options": [
            {"type": 3, "name": "a", "description": "d"}]}))
            .is_empty()
        );
    }

    #[test]
    fn size_cap_and_flags() {
        let big: Vec<Value> = (0..25)
            .map(|i| {
                json!({"type": 3, "name": format!("o{i}"), "description": "d".repeat(100),
                "choices": (0..25).map(|j| json!({"name": format!("c{j}"), "value": "v".repeat(100)})).collect::<Vec<_>>()})
            })
            .collect();
        let e = bad(json!({"name": "x", "description": "d", "options": big}));
        assert!(e.iter().any(|p| p.code == "APPLICATION_COMMAND_TOO_LARGE"));
        let c = ok(
            json!({"name": "x", "description": "d", "default_member_permissions": "8",
            "dm_permission": false, "nsfw": true, "contexts": [0, 1]}),
        );
        assert_eq!(c.default_member_permissions, Some(8));
        assert!(!c.dm_permission && c.nsfw);
        assert!(
            !bad(json!({"name": "x", "description": "d", "default_member_permissions": "x"}))
                .is_empty()
        );
        assert!(!bad(json!({"name": "x", "description": "d", "nsfw": "yes"})).is_empty());
    }

    #[test]
    fn a_command_without_integration_types_answers_guild_install() {
        assert_eq!(
            ok(json!({"name": "x", "description": "d"})).integration_types,
            None
        );
        let c = Command {
            id: 2,
            application_id: 1,
            guild_id: None,
            kind: 1,
            name: "x".into(),
            description: "d".into(),
            options: json!([]),
            name_localizations: None,
            description_localizations: None,
            default_member_permissions: None,
            dm_permission: true,
            nsfw: false,
            contexts: None,
            integration_types: None,
            version: 3,
        };
        assert_eq!(command_json(&c, false)["integration_types"], json!([0]));
        assert_eq!(command_json(&c, true)["integration_types"], Value::Null);
        let u = Command {
            integration_types: Some(json!([1])),
            ..c
        };
        assert_eq!(command_json(&u, false)["integration_types"], json!([1]));
    }

    #[test]
    fn integration_types_are_normalised_and_filtered() {
        let c = ok(json!({"name": "x", "description": "d", "integration_types": [1, 0]}));
        assert_eq!(c.integration_types, Some(json!([0])));
        assert!(integration_types_guild_offered(&c.integration_types));
        let u = ok(json!({"name": "x", "description": "d", "integration_types": [1, 1]}));
        assert_eq!(u.integration_types, Some(json!([1])));
        assert!(!integration_types_guild_offered(&u.integration_types));
        assert!(integration_types_guild_offered(&None));
        assert!(integration_types_guild_offered(&Some(json!([]))));
    }

    #[test]
    fn localizations_are_validated() {
        let c = ok(json!({"name": "ping", "description": "d",
            "name_localizations": {"de": "ping", "fr": "sonner"},
            "description_localizations": {"de": "Pong"}}));
        assert!(c.name_localizations.is_some());
        assert!(
            !bad(json!({"name": "ping", "description": "d",
            "name_localizations": {"de": "Ping Pong"}}))
            .is_empty()
        );
        assert!(
            !bad(json!({"name": "ping", "description": "d",
            "name_localizations": {"toolongloc": "x"}}))
            .is_empty()
        );
    }

    #[test]
    fn limits_per_kind() {
        let full = vec![1i16; MAX_CHAT_INPUT];
        assert!(check_limit(&full, 1).is_err());
        assert!(check_limit(&full, 2).is_ok());
        let users = vec![2i16; MAX_USER];
        assert!(check_limit(&users, 2).is_err());
        assert!(check_limit(&[3; 14], 3).is_ok());
    }

    #[test]
    fn patch_merges_and_keeps_type() {
        let base = ok(json!({"name": "ping", "description": "d"}));
        let c = Command {
            id: 1,
            application_id: 2,
            guild_id: None,
            kind: base.kind,
            name: base.name,
            description: base.description,
            options: base.options,
            name_localizations: None,
            description_localizations: None,
            default_member_permissions: None,
            dm_permission: true,
            nsfw: false,
            contexts: None,
            integration_types: None,
            version: 9,
        };
        let n = validate_patch(&c, &json!({"description": "new", "type": 3})).unwrap();
        assert_eq!((n.kind, n.description.as_str()), (1, "new"));
        assert!(validate_patch(&c, &json!({"name": "BAD NAME"})).is_err());
    }
}
