//! Message payload types that Discord clients send and receive: embeds, polls and
//! message components (open question 98). They are stored as JSON in the
//! `messages.embeds`, `messages.poll` and `messages.components` columns, so the
//! field names mirror Discord's JSON.

use serde::{Deserialize, Serialize};

/// Discord's embed limits: up to 10 embeds per message, 25 fields per embed.
pub const MAX_EMBEDS: usize = 10;
pub const MAX_EMBED_FIELDS: usize = 25;

fn is_zero(v: &u32) -> bool {
    *v == 0
}

/// A rich embed (Discord `embed` object). Unknown keys are dropped on parse.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Embed {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub color: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footer: Option<EmbedFooter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<EmbedMedia>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<EmbedMedia>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video: Option<EmbedMedia>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<EmbedAuthor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<EmbedField>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbedFooter {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbedMedia {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbedAuthor {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbedField {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub inline: bool,
}

/// A poll (Discord `poll` object). Creating polls is not available yet (LNG-MSG-010),
/// so only stored polls can carry one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Poll {
    pub question: PollMedia,
    pub answers: Vec<PollAnswer>,
    #[serde(default)]
    pub allow_multiselect: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_hours: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_type: Option<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollMedia {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollAnswer {
    pub answer_id: u32,
    pub poll_media: PollMedia,
}

/// A message component (Discord `component` object): action rows (type 1) hold
/// buttons (type 2), select menus (3 to 8) and text inputs (4).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Component {
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<Component>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embed_round_trips_discord_field_names() {
        let e: Embed = serde_json::from_value(serde_json::json!({
            "title": "Hi", "type": "rich", "color": 16711680,
            "fields": [{"name": "a", "value": "b", "inline": true}],
            "unknown": 1
        }))
        .unwrap();
        assert_eq!(e.kind.as_deref(), Some("rich"));
        assert_eq!(e.color, 0xff0000);
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["type"], "rich");
        assert!(v.get("unknown").is_none());
        assert!(v.get("footer").is_none());
    }

    #[test]
    fn component_nests() {
        let c: Component = serde_json::from_value(serde_json::json!({
            "type": 1, "components": [{"type": 2, "style": 1, "label": "ok", "custom_id": "x"}]
        }))
        .unwrap();
        assert_eq!(c.components[0].kind, 2);
    }
}
