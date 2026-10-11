//! Conversion from stored chat messages to index events (open question 132).
//!
//! `impl From<&Message> for MessageEvent` is what the `search.index_message` job
//! uses. Plain text comes from `sr_markdown::to_plain_text` (LNG-MD-004) and the
//! `has:` flags are computed here. A [`Message`] only carries attachment ids, so
//! the attachment flags are added by the caller with
//! [`IndexedMessage::with_attachments`] once it has loaded the attachment rows.

use std::collections::BTreeSet;

use sr_chat::{Message, Visibility};
use sr_markdown::{extract_mentions, parse, to_plain_text};

use crate::messages::{AuthorType, HasKind, IndexedMessage, MessageEvent};

/// Whether `content` carries a URL (`has:link`).
pub fn has_link(content: &str) -> bool {
    content.split_whitespace().any(|w| {
        let w = w.trim_start_matches(['<', '(']);
        let lower = w.to_ascii_lowercase();
        lower.starts_with("http://") || lower.starts_with("https://")
    })
}

/// `has:` kinds for one attachment: always `file`, plus `image`, `video` or
/// `sound` from the content type.
pub fn attachment_kinds(content_type: &str) -> BTreeSet<HasKind> {
    let mut kinds = BTreeSet::from([HasKind::File]);
    let main = content_type
        .split('/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match main.as_str() {
        "image" => {
            kinds.insert(HasKind::Image);
        }
        "video" => {
            kinds.insert(HasKind::Video);
        }
        "audio" => {
            kinds.insert(HasKind::Sound);
        }
        _ => {}
    }
    kinds
}

impl IndexedMessage {
    /// Add the `has:` flags of the message's attachments (content types).
    #[must_use]
    pub fn with_attachments<'a>(
        mut self,
        content_types: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        for ct in content_types {
            self.has.extend(attachment_kinds(ct));
        }
        self
    }
}

impl From<&Message> for IndexedMessage {
    fn from(m: &Message) -> Self {
        let mut has = BTreeSet::new();
        if has_link(&m.content) || m.embeds.iter().any(|e| e.url.is_some()) {
            has.insert(HasKind::Link);
        }
        if !m.embeds.is_empty() {
            has.insert(HasKind::Embed);
        }
        if !m.sticker_ids.is_empty() {
            has.insert(HasKind::Sticker);
        }
        if m.poll.is_some() {
            has.insert(HasKind::Poll);
        }
        let ast = parse(&m.content);
        let mut mentions = m.mention_user_ids.clone();
        for id in extract_mentions(&ast).users {
            let id = sr_core::UserId::new(id);
            if !mentions.contains(&id) {
                mentions.push(id);
            }
        }
        Self {
            id: m.id,
            channel_id: m.channel_id,
            guild_id: m.guild_id,
            author_id: m.author_id,
            // `webhook_id`/`application_id` are not stored on messages yet, so
            // `authorType:` only distinguishes users until they are.
            author_type: AuthorType::User,
            content: to_plain_text(&ast),
            has,
            timestamp_ms: m.id.snowflake().unix_ms(),
            pinned: m.pinned,
            mention_user_ids: mentions,
        }
    }
}

/// Created and edited messages become upserts; anything not visible (user
/// deleted, staff removed) becomes a removal, so it is never searchable.
impl From<&Message> for MessageEvent {
    fn from(m: &Message) -> Self {
        if m.visibility != Visibility::Visible || m.deleted_at_ms.is_some() {
            MessageEvent::Deleted(m.id)
        } else if m.edited_at_ms.is_some() {
            MessageEvent::Updated(m.into())
        } else {
            MessageEvent::Created(m.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_chat::{MessageType, message::Embed};
    use sr_core::{ChannelId, GuildId, MessageId, UserId};

    fn msg(content: &str) -> Message {
        Message {
            // A snowflake whose timestamp part is non-zero.
            id: MessageId::new((1_800_000_000_000u64 - 1_420_070_400_000) << 22),
            channel_id: ChannelId::new(2),
            guild_id: Some(GuildId::new(3)),
            author_id: UserId::new(4),
            kind: MessageType::Default,
            content: content.to_owned(),
            flags: 0,
            edited_at_ms: None,
            reference: None,
            attachment_ids: vec![],
            embeds: vec![],
            components: vec![],
            sticker_ids: vec![],
            poll: None,
            mention_everyone: false,
            mention_user_ids: vec![],
            mention_role_ids: vec![],
            mention_channel_ids: vec![],
            pinned: false,
            visibility: Visibility::Visible,
            deleted_at_ms: None,
            purge_after_ms: None,
            hold_id: None,
        }
    }

    #[test]
    fn plain_text_flags_and_timestamp() {
        let mut m = msg("**bold** see https://example.com/x and <@9>");
        m.pinned = true;
        m.embeds = vec![Embed::default()];
        m.sticker_ids = vec![1];
        let MessageEvent::Created(i) = MessageEvent::from(&m) else {
            panic!("expected Created");
        };
        assert!(i.content.starts_with("bold see https://example.com/x"));
        assert!(!i.content.contains("**"));
        assert!(i.has.contains(&HasKind::Link));
        assert!(i.has.contains(&HasKind::Embed));
        assert!(i.has.contains(&HasKind::Sticker));
        assert!(!i.has.contains(&HasKind::Poll));
        assert!(i.pinned);
        assert_eq!(i.author_type, AuthorType::User);
        assert_eq!(i.guild_id, Some(GuildId::new(3)));
        // Mentions parsed from the text are indexed even if the row lacks them.
        assert_eq!(i.mention_user_ids, vec![UserId::new(9)]);
        assert_eq!(i.timestamp_ms, m.id.snowflake().unix_ms());
        assert!(i.timestamp_ms > 1_700_000_000_000);
    }

    #[test]
    fn edits_upsert_and_hidden_messages_are_removed() {
        let mut m = msg("hello");
        m.edited_at_ms = Some(5);
        assert!(matches!(MessageEvent::from(&m), MessageEvent::Updated(_)));
        m.visibility = Visibility::UserDeleted;
        assert_eq!(MessageEvent::from(&m), MessageEvent::Deleted(m.id));
        let mut m = msg("hello");
        m.deleted_at_ms = Some(1);
        assert_eq!(MessageEvent::from(&m), MessageEvent::Deleted(m.id));
    }

    #[test]
    fn attachment_flags_come_from_content_types() {
        let i = IndexedMessage::from(&msg("pic")).with_attachments(["image/png", "audio/ogg"]);
        for k in [HasKind::File, HasKind::Image, HasKind::Sound] {
            assert!(i.has.contains(&k), "{k:?}");
        }
        assert!(!i.has.contains(&HasKind::Video));
        let doc = IndexedMessage::from(&msg("pdf")).with_attachments(["application/pdf"]);
        assert_eq!(doc.has, BTreeSet::from([HasKind::File]));
    }

    #[test]
    fn link_detection() {
        assert!(has_link("go to <https://a.b/c>"));
        assert!(has_link("HTTP://A.B"));
        assert!(!has_link("no link here, just example.com"));
    }
}
