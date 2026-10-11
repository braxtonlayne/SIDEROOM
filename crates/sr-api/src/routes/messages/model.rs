//! Request and response shapes (Discord-compatible field names).

use super::store::{Attachment, ReactionSummary};
use serde::{Deserialize, Deserializer, Serialize};
use sr_chat::Message;
use utoipa::ToSchema;

/// Unix milliseconds as ISO 8601 (`2026-01-02T03:04:05.678000+00:00`).
pub fn iso(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let millis = ms.rem_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{millis:03}000+00:00",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Partial user object. Only the id is available until the users module lands.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UserRef {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AttachmentObject {
    pub id: String,
    pub filename: String,
    pub size: u64,
    /// Relative CDN path (`/cdn/attachments/...`); signing happens at the edge.
    pub url: String,
    pub content_type: Option<String>,
    pub description: Option<String>,
    /// Set when the attachment is a large file that will expire.
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PartialEmoji {
    pub id: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ReactionObject {
    pub emoji: PartialEmoji,
    pub count: u32,
    pub me: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MessageReferenceObject {
    pub message_id: String,
    pub channel_id: String,
    pub guild_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MessageObject {
    pub id: String,
    pub channel_id: String,
    pub guild_id: Option<String>,
    pub author: UserRef,
    pub content: String,
    pub timestamp: String,
    pub edited_timestamp: Option<String>,
    pub tts: bool,
    pub mention_everyone: bool,
    pub mentions: Vec<UserRef>,
    pub mention_roles: Vec<String>,
    pub attachments: Vec<AttachmentObject>,
    #[schema(value_type = Vec<Object>)]
    pub embeds: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reactions: Vec<ReactionObject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    pub pinned: bool,
    #[serde(rename = "type")]
    pub kind: i16,
    pub flags: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_reference: Option<MessageReferenceObject>,
}

impl MessageObject {
    pub fn build(
        m: &Message,
        attachments: &[Attachment],
        reactions: &[ReactionSummary],
        nonce: Option<String>,
    ) -> Self {
        let gid = m.guild_id.map(|g| g.to_string());
        Self {
            id: m.id.to_string(),
            channel_id: m.channel_id.to_string(),
            guild_id: gid.clone(),
            author: UserRef {
                id: m.author_id.to_string(),
            },
            content: m.content.clone(),
            timestamp: iso(m.id.snowflake().unix_ms()),
            edited_timestamp: m.edited_at_ms.map(iso),
            tts: false,
            mention_everyone: m.mention_everyone,
            mentions: m
                .mention_user_ids
                .iter()
                .map(|u| UserRef { id: u.to_string() })
                .collect(),
            mention_roles: m.mention_role_ids.iter().map(ToString::to_string).collect(),
            attachments: attachments.iter().map(attachment_object).collect(),
            // Embed payloads are not persisted yet (open question 98): only the count exists.
            embeds: Vec::new(),
            reactions: reactions
                .iter()
                .map(|r| ReactionObject {
                    emoji: PartialEmoji {
                        id: r.emoji.id.map(|i| i.to_string()),
                        name: r.emoji.name.clone(),
                    },
                    count: r.count,
                    me: r.me,
                })
                .collect(),
            nonce,
            pinned: m.pinned,
            kind: m.kind.code(),
            flags: m.flags,
            message_reference: m.reference.as_ref().map(|r| MessageReferenceObject {
                message_id: r.message_id.to_string(),
                channel_id: r.channel_id.to_string(),
                guild_id: gid,
            }),
        }
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

pub fn attachment_object(a: &Attachment) -> AttachmentObject {
    AttachmentObject {
        id: a.id.to_string(),
        filename: a.filename.clone(),
        size: a.size,
        url: format!("/cdn/attachments/{}/{}/{}", a.channel_id, a.id, a.filename),
        content_type: a.content_type.clone(),
        description: a.description.clone(),
        expires_at: a.expires_at_ms.map(iso),
    }
}

// ------------------------------------------------------------------ requests

/// Discord accepts the nonce as a string or an integer.
fn nonce_de<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum N {
        S(String),
        I(i64),
    }
    Ok(Option::<N>::deserialize(d)?.map(|n| match n {
        N::S(s) => s,
        N::I(i) => i.to_string(),
    }))
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct MessageReferenceIn {
    pub message_id: String,
    /// Default true: a missing target is an error. When false the message is
    /// sent without a reply.
    #[serde(default)]
    pub fail_if_not_exists: Option<bool>,
}

/// An uploaded file referenced by a new message.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct AttachmentRef {
    /// The `id` returned by the upload-slot call (client id echo or upload id).
    pub id: Option<String>,
    /// `upload_filename` returned by the upload-slot call.
    pub uploaded_filename: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
pub struct CreateMessageBody {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default, deserialize_with = "nonce_de")]
    #[schema(value_type = Option<String>)]
    pub nonce: Option<String>,
    #[serde(default)]
    pub tts: Option<bool>,
    /// Only the count is kept for now.
    #[serde(default)]
    #[schema(value_type = Option<Vec<Object>>)]
    pub embeds: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub message_reference: Option<MessageReferenceIn>,
    #[serde(default)]
    pub attachments: Option<Vec<AttachmentRef>>,
    #[serde(default)]
    pub flags: Option<u32>,
    /// Not supported yet (stickers/polls have no storage); a non-empty value is rejected.
    #[serde(default)]
    pub sticker_ids: Option<Vec<String>>,
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub poll: Option<serde_json::Value>,
    /// When true a nonce collision is an error instead of returning the existing message.
    #[serde(default)]
    pub enforce_nonce: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
pub struct EditMessageBody {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub flags: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct BulkDeleteBody {
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ListMessagesQuery {
    pub limit: Option<u32>,
    pub before: Option<String>,
    pub after: Option<String>,
    pub around: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ReactionUsersQuery {
    pub limit: Option<u32>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct FileSpec {
    /// Client-chosen id, echoed back.
    #[serde(default, deserialize_with = "nonce_de")]
    #[schema(value_type = Option<String>)]
    pub id: Option<String>,
    pub filename: String,
    pub file_size: u64,
    /// What the client believes the type is; verified by sniffing at completion.
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub spoiler: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UploadSlotsBody {
    pub files: Vec<FileSpec>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UploadSlot {
    /// Echo of the client id (or the upload id when none was sent).
    pub id: String,
    /// Server-side upload id; reference it from the message.
    pub upload_id: String,
    pub upload_url: String,
    /// Opaque name to pass as `uploaded_filename` when creating the message.
    pub upload_filename: String,
    pub upload_url_expires_in: u64,
    /// The maximum accepted size for this caller.
    pub max_size: u64,
    /// Large files expire after this many seconds; absent for normal files.
    pub expires_after: Option<u64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UploadSlotsResponse {
    pub attachments: Vec<UploadSlot>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UserListEntry {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PinEntry {
    pub pinned_at: String,
    pub message: MessageObject,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PinsPage {
    pub items: Vec<PinEntry>,
    pub has_more: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_formats_unix_ms() {
        assert_eq!(iso(0), "1970-01-01T00:00:00.000000+00:00");
        assert_eq!(iso(1_800_000_000_123), "2027-01-15T08:00:00.123000+00:00");
        assert_eq!(iso(951_782_400_000), "2000-02-29T00:00:00.000000+00:00");
    }

    #[test]
    fn nonce_accepts_int_and_string() {
        let a: CreateMessageBody = serde_json::from_str(r#"{"nonce": 42}"#).unwrap();
        assert_eq!(a.nonce.as_deref(), Some("42"));
        let b: CreateMessageBody = serde_json::from_str(r#"{"nonce": "x1"}"#).unwrap();
        assert_eq!(b.nonce.as_deref(), Some("x1"));
        let c: CreateMessageBody = serde_json::from_str("{}").unwrap();
        assert!(c.nonce.is_none());
    }
}
