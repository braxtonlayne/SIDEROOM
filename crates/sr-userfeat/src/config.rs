//! Typed reads of a module's effective settings (a JSON object laid out by its schema).

use serde_json::{Map, Value};

/// Effective settings of one module at one server.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Settings(pub Map<String, Value>);

impl Settings {
    pub fn int(&self, key: &str, default: i64) -> i64 {
        self.0.get(key).and_then(Value::as_i64).unwrap_or(default)
    }

    pub fn flag(&self, key: &str, default: bool) -> bool {
        self.0.get(key).and_then(Value::as_bool).unwrap_or(default)
    }

    pub fn text(&self, key: &str) -> Option<&str> {
        self.0
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    }

    /// A channel or role picker value.
    pub fn id(&self, key: &str) -> Option<i64> {
        self.0
            .get(key)
            .and_then(Value::as_str)
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|v| *v > 0)
    }

    /// A multi picker value.
    pub fn ids(&self, key: &str) -> Vec<i64> {
        self.0
            .get(key)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .filter_map(|s| s.parse::<i64>().ok())
                    .filter(|v| *v > 0)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A list of strings.
    pub fn strings(&self, key: &str) -> Vec<String> {
        self.0
            .get(key)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn list(&self, key: &str) -> Vec<&Value> {
        self.0
            .get(key)
            .and_then(Value::as_array)
            .map(|a| a.iter().collect())
            .unwrap_or_default()
    }
}

/// Replace `{name}` placeholders in `template`. Unknown placeholders stay as written.
pub fn render_template(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) => {
                let name = &after[..end];
                match vars.iter().find(|(k, _)| *k == name) {
                    Some((_, v)) => out.push_str(v),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn s(v: Value) -> Settings {
        Settings(v.as_object().cloned().unwrap_or_default())
    }

    #[test]
    fn typed_reads_fall_back() {
        let c = s(json!({"n": 5, "on": true, "t": "hi", "ch": "42", "roles": ["1", "x", "3"]}));
        assert_eq!(c.int("n", 1), 5);
        assert_eq!(c.int("missing", 7), 7);
        assert!(c.flag("on", false));
        assert_eq!(c.text("t"), Some("hi"));
        assert_eq!(c.id("ch"), Some(42));
        assert_eq!(c.id("t"), None);
        assert_eq!(c.ids("roles"), vec![1, 3]);
    }

    #[test]
    fn templates_fill_known_names_only() {
        let out = render_template("Happy birthday {user}! {nope} {", &[("user", "<@1>")]);
        assert_eq!(out, "Happy birthday <@1>! {nope} {");
        assert_eq!(render_template("{a}{a}", &[("a", "x")]), "xx");
    }
}
