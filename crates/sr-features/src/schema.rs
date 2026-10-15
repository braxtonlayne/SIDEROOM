//! Typed, serializable settings descriptions that the web app auto-renders.
//! Values are plain JSON: bool, integer, string, duration as whole seconds,
//! color as `#rrggbb`, pickers as snowflake strings (arrays when `multi`),
//! lists as arrays, groups as objects.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

use crate::feature::Scope;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("invalid setting `{path}`: {reason}")]
pub struct ValidationError {
    pub path: String,
    pub reason: String,
}

impl ValidationError {
    fn new(path: &str, reason: impl Into<String>) -> Self {
        Self {
            path: path.to_owned(),
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FieldKind {
    Bool,
    Int {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<i64>,
    },
    Enum {
        options: Vec<EnumOption>,
    },
    String {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_len: Option<usize>,
    },
    /// Whole seconds.
    Duration {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_secs: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_secs: Option<u64>,
    },
    /// `#rrggbb`.
    Color,
    RolePicker {
        #[serde(default)]
        multi: bool,
    },
    ChannelPicker {
        #[serde(default)]
        multi: bool,
    },
    EmojiPicker {
        #[serde(default)]
        multi: bool,
    },
    List {
        item: Box<FieldKind>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_items: Option<usize>,
    },
    Group {
        fields: Vec<Field>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub key: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(flatten)]
    pub kind: FieldKind,
    /// Default used when no scope overrides it. `None` means unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    /// Per-scope default overrides.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub scope_defaults: BTreeMap<Scope, Value>,
}

impl Field {
    pub fn new(key: &str, label: &str, kind: FieldKind) -> Self {
        Self {
            key: key.to_owned(),
            label: label.to_owned(),
            help: None,
            kind,
            default: None,
            scope_defaults: BTreeMap::new(),
        }
    }

    pub fn help(mut self, help: &str) -> Self {
        self.help = Some(help.to_owned());
        self
    }

    pub fn default_value(mut self, v: Value) -> Self {
        self.default = Some(v);
        self
    }

    pub fn scope_default(mut self, scope: Scope, v: Value) -> Self {
        self.scope_defaults.insert(scope, v);
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SettingsSchema {
    pub fields: Vec<Field>,
}

impl SettingsSchema {
    pub fn new(fields: Vec<Field>) -> Self {
        Self { fields }
    }

    /// Defaults for `scope`: field default, overridden by the scope default;
    /// groups yield nested objects. Fields without a default are omitted.
    pub fn defaults(&self, scope: Scope) -> Map<String, Value> {
        defaults_of(&self.fields, scope)
    }

    /// Validate a (possibly partial) settings object. Unknown keys are rejected.
    pub fn validate(&self, values: &Map<String, Value>) -> Result<(), ValidationError> {
        validate_object(&self.fields, values, "")
    }

    /// Every channel and role id the values name through pickers, walking groups and
    /// lists. Used to check that a server only points at its own channels and roles.
    pub fn picked_ids(&self, values: &Map<String, Value>) -> PickedIds {
        let mut out = PickedIds::default();
        collect_picked(&self.fields, values, &mut out);
        out
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

/// Channel and role ids named by picker fields (see [`SettingsSchema::picked_ids`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PickedIds {
    pub channels: Vec<u64>,
    pub roles: Vec<u64>,
}

fn push_ids(v: &Value, into: &mut Vec<u64>) {
    match v {
        Value::String(s) => into.extend(s.parse::<u64>()),
        Value::Array(a) => a.iter().for_each(|x| push_ids(x, into)),
        _ => {}
    }
}

fn collect_kind(kind: &FieldKind, v: &Value, out: &mut PickedIds) {
    match kind {
        FieldKind::ChannelPicker { .. } => push_ids(v, &mut out.channels),
        FieldKind::RolePicker { .. } => push_ids(v, &mut out.roles),
        FieldKind::List { item, .. } => {
            if let Value::Array(a) = v {
                a.iter().for_each(|x| collect_kind(item, x, out));
            }
        }
        FieldKind::Group { fields } => {
            if let Value::Object(o) = v {
                collect_picked(fields, o, out);
            }
        }
        _ => {}
    }
}

fn collect_picked(fields: &[Field], values: &Map<String, Value>, out: &mut PickedIds) {
    for f in fields {
        if let Some(v) = values.get(&f.key) {
            collect_kind(&f.kind, v, out);
        }
    }
}
fn defaults_of(fields: &[Field], scope: Scope) -> Map<String, Value> {
    let mut out = Map::new();
    for f in fields {
        if let FieldKind::Group { fields: sub } = &f.kind {
            let nested = defaults_of(sub, scope);
            if !nested.is_empty() {
                out.insert(f.key.clone(), Value::Object(nested));
            }
        } else if let Some(v) = f.scope_defaults.get(&scope).or(f.default.as_ref()) {
            out.insert(f.key.clone(), v.clone());
        }
    }
    out
}

/// Recursively overlay `top` on `base` (objects merge, everything else replaces).
pub fn merge(base: &mut Map<String, Value>, top: &Map<String, Value>) {
    for (k, v) in top {
        match (base.get_mut(k), v) {
            (Some(Value::Object(b)), Value::Object(t)) => merge(b, t),
            _ => {
                base.insert(k.clone(), v.clone());
            }
        }
    }
}

fn validate_object(
    fields: &[Field],
    values: &Map<String, Value>,
    prefix: &str,
) -> Result<(), ValidationError> {
    for (k, v) in values {
        let path = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}.{k}")
        };
        let field = fields
            .iter()
            .find(|f| &f.key == k)
            .ok_or_else(|| ValidationError::new(&path, "unknown setting"))?;
        validate_value(&field.kind, v, &path)?;
    }
    Ok(())
}

fn is_snowflake(v: &Value) -> bool {
    matches!(v, Value::String(s) if !s.is_empty() && s.len() <= 20 && s.bytes().all(|b| b.is_ascii_digit()))
}

fn picker(multi: bool, v: &Value, path: &str, what: &str) -> Result<(), ValidationError> {
    let ok = if multi {
        matches!(v, Value::Array(a) if a.iter().all(is_snowflake))
    } else {
        is_snowflake(v)
    };
    if ok {
        Ok(())
    } else {
        Err(ValidationError::new(
            path,
            format!(
                "expected {}{what} id(s) as string(s)",
                if multi { "a list of " } else { "one " }
            ),
        ))
    }
}

fn validate_value(kind: &FieldKind, v: &Value, path: &str) -> Result<(), ValidationError> {
    let err = |m: &str| Err(ValidationError::new(path, m));
    match kind {
        FieldKind::Bool => {
            if v.is_boolean() {
                Ok(())
            } else {
                err("expected a boolean")
            }
        }
        FieldKind::Int { min, max } => {
            let Some(n) = v.as_i64() else {
                return err("expected an integer");
            };
            if min.is_some_and(|m| n < m) || max.is_some_and(|m| n > m) {
                return err("out of range");
            }
            Ok(())
        }
        FieldKind::Enum { options } => match v.as_str() {
            Some(s) if options.iter().any(|o| o.value == s) => Ok(()),
            _ => err("not one of the allowed options"),
        },
        FieldKind::String { max_len } => match v.as_str() {
            Some(s) if max_len.is_none_or(|m| s.chars().count() <= m) => Ok(()),
            Some(_) => err("too long"),
            None => err("expected a string"),
        },
        FieldKind::Duration { min_secs, max_secs } => {
            let Some(n) = v.as_u64() else {
                return err("expected whole seconds");
            };
            if min_secs.is_some_and(|m| n < m) || max_secs.is_some_and(|m| n > m) {
                return err("duration out of range");
            }
            Ok(())
        }
        FieldKind::Color => match v.as_str() {
            Some(s)
                if s.len() == 7
                    && s.starts_with('#')
                    && s[1..].bytes().all(|b| b.is_ascii_hexdigit()) =>
            {
                Ok(())
            }
            _ => err("expected #rrggbb"),
        },
        FieldKind::RolePicker { multi } => picker(*multi, v, path, "role"),
        FieldKind::ChannelPicker { multi } => picker(*multi, v, path, "channel"),
        FieldKind::EmojiPicker { multi } => picker(*multi, v, path, "emoji"),
        FieldKind::List { item, max_items } => {
            let Some(a) = v.as_array() else {
                return err("expected a list");
            };
            if max_items.is_some_and(|m| a.len() > m) {
                return err("too many items");
            }
            for (i, x) in a.iter().enumerate() {
                validate_value(item, x, &format!("{path}[{i}]"))?;
            }
            Ok(())
        }
        FieldKind::Group { fields } => match v.as_object() {
            Some(o) => validate_object(fields, o, path),
            None => err("expected an object"),
        },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn schema() -> SettingsSchema {
        SettingsSchema::new(vec![
            Field::new("on", "On", FieldKind::Bool).default_value(json!(true)),
            Field::new(
                "n",
                "N",
                FieldKind::Int {
                    min: Some(1),
                    max: Some(10),
                },
            )
            .default_value(json!(5))
            .scope_default(Scope::Channel, json!(2)),
            Field::new(
                "mode",
                "Mode",
                FieldKind::Enum {
                    options: vec![EnumOption {
                        value: "a".into(),
                        label: "A".into(),
                    }],
                },
            ),
            Field::new("s", "S", FieldKind::String { max_len: Some(3) }),
            Field::new(
                "d",
                "D",
                FieldKind::Duration {
                    min_secs: Some(1),
                    max_secs: Some(60),
                },
            ),
            Field::new("c", "C", FieldKind::Color),
            Field::new("role", "Role", FieldKind::RolePicker { multi: false }),
            Field::new("chs", "Chs", FieldKind::ChannelPicker { multi: true }),
            Field::new("e", "E", FieldKind::EmojiPicker { multi: false }),
            Field::new(
                "l",
                "L",
                FieldKind::List {
                    item: Box::new(FieldKind::Bool),
                    max_items: Some(2),
                },
            ),
            Field::new(
                "g",
                "G",
                FieldKind::Group {
                    fields: vec![Field::new("x", "X", FieldKind::Bool).default_value(json!(false))],
                },
            ),
        ])
    }

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().cloned().unwrap()
    }

    #[test]
    fn json_roundtrip() {
        let s = schema();
        assert_eq!(SettingsSchema::from_json(&s.to_json().unwrap()).unwrap(), s);
    }

    #[test]
    fn json_shape_is_flat_and_tagged() {
        let f = Field::new("on", "On", FieldKind::Bool);
        let v = serde_json::to_value(&f).unwrap();
        assert_eq!(v, json!({"key": "on", "label": "On", "type": "bool"}));
    }

    #[test]
    fn defaults_resolve_per_scope() {
        let s = schema();
        assert_eq!(s.defaults(Scope::Guild)["n"], json!(5));
        assert_eq!(s.defaults(Scope::Channel)["n"], json!(2));
        assert_eq!(s.defaults(Scope::Guild)["g"], json!({"x": false}));
        assert!(!s.defaults(Scope::Guild).contains_key("mode"));
    }

    #[test]
    fn accepts_valid() {
        let ok = obj(json!({
            "on": false, "n": 3, "mode": "a", "s": "abc", "d": 30, "c": "#a1B2c3",
            "role": "123", "chs": ["1", "2"], "e": "9", "l": [true], "g": {"x": true}
        }));
        schema().validate(&ok).unwrap();
    }

    #[test]
    fn rejects_invalid() {
        for bad in [
            json!({"nope": 1}),
            json!({"on": "yes"}),
            json!({"n": 11}),
            json!({"n": 1.5}),
            json!({"mode": "b"}),
            json!({"s": "abcd"}),
            json!({"d": 0}),
            json!({"d": -1}),
            json!({"c": "red"}),
            json!({"role": 5}),
            json!({"chs": "1"}),
            json!({"l": [true, true, true]}),
            json!({"l": [1]}),
            json!({"g": {"y": 1}}),
        ] {
            assert!(schema().validate(&obj(bad.clone())).is_err(), "{bad}");
        }
        let e = schema().validate(&obj(json!({"g": {"x": 1}}))).unwrap_err();
        assert_eq!(e.path, "g.x");
    }
}

#[cfg(test)]
mod picked_tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn collects_channel_and_role_ids_through_groups_and_lists() {
        let s = SettingsSchema::new(vec![
            Field::new("ch", "Ch", FieldKind::ChannelPicker { multi: false }),
            Field::new("many", "Many", FieldKind::RolePicker { multi: true }),
            Field::new("emoji", "E", FieldKind::EmojiPicker { multi: false }),
            Field::new(
                "rows",
                "Rows",
                FieldKind::List {
                    max_items: None,
                    item: Box::new(FieldKind::Group {
                        fields: vec![
                            Field::new("c", "C", FieldKind::ChannelPicker { multi: false }),
                            Field::new("r", "R", FieldKind::RolePicker { multi: false }),
                            Field::new("n", "N", FieldKind::String { max_len: None }),
                        ],
                    }),
                },
            ),
        ]);
        let v = json!({
            "ch": "1", "many": ["2", "3"], "emoji": "99",
            "rows": [{"c": "4", "r": "5", "n": "6"}, {"c": "7"}]
        });
        let got = s.picked_ids(v.as_object().unwrap());
        assert_eq!(got.channels, vec![1, 4, 7]);
        assert_eq!(got.roles, vec![2, 3, 5]);
    }
}
