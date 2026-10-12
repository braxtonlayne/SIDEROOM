//! Persistence the message endpoints need beyond `sr_chat::repo::ChatRepository`.
//! The trait and its types live in `sr_chat::repo` (open question 148); the
//! Postgres implementation is `sr_db::chat_repo::PgMessageStore`.

pub use sr_chat::repo::{
    Attachment, EmojiKey, MessageStore, Page, ReactionSummary, StoreError, StoreResult, UploadState,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_emoji_paths() {
        let u = EmojiKey::parse("\u{1F44D}").unwrap();
        assert_eq!((u.id, u.key().as_str()), (None, "\u{1F44D}"));
        let c = EmojiKey::parse("party_blob:123456").unwrap();
        assert_eq!(c.id, Some(123_456));
        assert_eq!(c.json()["id"], "123456");
        assert_eq!(EmojiKey::parse("a:wave:9").unwrap().name, "wave");
        assert!(EmojiKey::parse("smile").is_none());
        assert!(EmojiKey::parse("x:12").is_none());
        assert!(EmojiKey::parse("name:abc").is_none());
        assert!(EmojiKey::parse("").is_none());
    }
}
