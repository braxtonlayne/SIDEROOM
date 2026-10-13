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
    /// Set only on link previews the server fetched (`messages.unfurl`); client-sent
    /// embeds never keep it, so its presence marks an embed as server-generated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<EmbedProvider>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbedProvider {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl Embed {
    /// Whether the server generated this embed (a link preview) rather than a
    /// client or bot. Client embeds are normalised to type `rich` without a provider.
    pub fn is_unfurled(&self) -> bool {
        self.provider.is_some() || self.kind.as_deref().is_some_and(|k| k != "rich")
    }
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

/// A poll (Discord `poll` object) as stored in `messages.poll`. Votes live in
/// `poll_votes`; `expiry_ms` and `finalized` are server state (see `poll_rules`).
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
    /// Unix ms at which voting closes. Early expiry moves it to "now".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiry_ms: Option<i64>,
    /// Set once the poll has been finalised (results are final, votes refused).
    #[serde(default)]
    pub finalized: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollMedia {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji: Option<PollEmoji>,
}

/// Partial emoji of a poll answer: a custom emoji `id` or a unicode `name`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollEmoji {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollAnswer {
    pub answer_id: u32,
    pub poll_media: PollMedia,
}

/// Copy of a message taken when it is forwarded (`message_reference.type = 1`,
/// LNG-MSG-007). The forward keeps showing this even when the source is edited or
/// deleted. Attachments are referenced, not copied: they live as long as the source
/// message's objects do.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForwardSnapshot {
    #[serde(default)]
    pub source_guild_id: Option<u64>,
    /// The source message type code.
    #[serde(default)]
    pub kind: i16,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub embeds: Vec<Embed>,
    #[serde(default)]
    pub attachments: Vec<SnapshotAttachment>,
    #[serde(default)]
    pub sticker_ids: Vec<u64>,
    #[serde(default)]
    pub flags: u32,
    /// Creation time of the source message (Unix ms).
    #[serde(default)]
    pub timestamp_ms: i64,
    #[serde(default)]
    pub edited_timestamp_ms: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotAttachment {
    pub id: u64,
    pub channel_id: u64,
    pub filename: String,
    pub size: u64,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    /// Voice message length in milliseconds.
    pub duration_ms: Option<u32>,
    #[serde(default)]
    pub waveform: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub blurhash: Option<String>,
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
