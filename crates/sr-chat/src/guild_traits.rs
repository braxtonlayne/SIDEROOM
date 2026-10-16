//! Server traits (J-07, LNG-SSET-007): up to five short labels, each with an optional emoji,
//! shown on the server profile and the directory card.

use crate::error::{ChatError, ChatResult};
use serde::{Deserialize, Serialize};

/// Most traits a server holds.
pub const TRAITS_MAX: usize = 5;
/// Longest trait label, in characters.
pub const TRAIT_LABEL_MAX: usize = 24;
/// Longest unicode emoji name (a ZWJ sequence is several characters).
const EMOJI_NAME_MAX: usize = 32;
/// Characters that start a mention or a markdown construct; a label has none of them.
const FORBIDDEN: &[char] = &['@', '<', '>', '*', '_', '~', '`', '|', '\\', '#', '[', ']'];

/// One trait: a label and an emoji, which is either a custom emoji of the server
/// (`emoji_id`, with its name) or a unicode emoji (`emoji_name` only).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GuildTrait {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji_name: Option<String>,
    pub label: String,
}

fn bad(field: &str, code: &'static str, msg: &str) -> ChatError {
    ChatError::invalid(field, code, msg)
}

/// Check and normalise a trait list: at most [`TRAITS_MAX`] entries, labels of 1 to
/// [`TRAIT_LABEL_MAX`] characters without mentions or markdown, emoji ids that are snowflakes.
pub fn validate_traits(list: Vec<GuildTrait>) -> ChatResult<Vec<GuildTrait>> {
    if list.len() > TRAITS_MAX {
        return Err(bad(
            "traits",
            "BASE_TYPE_MAX_LENGTH",
            "A server has at most 5 traits.",
        ));
    }
    let mut out = Vec::with_capacity(list.len());
    for (n, t) in list.into_iter().enumerate() {
        let field = format!("traits.{n}");
        let label = t.label.trim().to_owned();
        let len = label.chars().count();
        if len == 0 || len > TRAIT_LABEL_MAX {
            return Err(bad(
                &format!("{field}.label"),
                "BASE_TYPE_BAD_LENGTH",
                "Must be between 1 and 24 in length.",
            ));
        }
        if label.chars().any(|c| FORBIDDEN.contains(&c)) {
            return Err(bad(
                &format!("{field}.label"),
                "TRAIT_LABEL_INVALID",
                "A label cannot contain mentions or markdown.",
            ));
        }
        let emoji_id = t
            .emoji_id
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());
        if let Some(id) = &emoji_id
            && (id.len() > 20 || !id.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err(bad(
                &format!("{field}.emoji_id"),
                "SNOWFLAKE_TYPE",
                "Not a valid snowflake.",
            ));
        }
        let emoji_name = t
            .emoji_name
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());
        if let Some(n) = &emoji_name
            && (n.chars().count() > EMOJI_NAME_MAX || n.chars().any(char::is_control))
        {
            return Err(bad(
                &format!("{field}.emoji_name"),
                "INVALID_EMOJI",
                "Must be a single emoji or a custom emoji name.",
            ));
        }
        out.push(GuildTrait {
            emoji_id,
            emoji_name,
            label,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(label: &str) -> GuildTrait {
        GuildTrait {
            label: label.into(),
            ..GuildTrait::default()
        }
    }

    #[test]
    fn accepts_up_to_five_and_trims() {
        let v = validate_traits(vec![
            t("  Chill "),
            t("Art"),
            t("Games"),
            t("Music"),
            t("Dev"),
        ])
        .unwrap();
        assert_eq!(v[0].label, "Chill");
        assert!(validate_traits(vec![]).unwrap().is_empty());
        assert!(validate_traits((0..6).map(|n| t(&format!("t{n}"))).collect()).is_err());
    }

    #[test]
    fn label_length_is_one_to_twenty_four() {
        assert!(validate_traits(vec![t("")]).is_err());
        assert!(validate_traits(vec![t("   ")]).is_err());
        assert!(validate_traits(vec![t(&"a".repeat(24))]).is_ok());
        assert!(validate_traits(vec![t(&"a".repeat(25))]).is_err());
        // Characters, not bytes.
        assert!(validate_traits(vec![t(&"é".repeat(24))]).is_ok());
    }

    #[test]
    fn labels_carry_no_mentions_or_markdown() {
        for bad in [
            "@everyone",
            "<@123>",
            "**bold**",
            "a_b",
            "#general",
            "[x](y)",
            "`c`",
        ] {
            assert!(validate_traits(vec![t(bad)]).is_err(), "{bad}");
        }
    }

    #[test]
    fn emoji_fields_are_checked() {
        let ok = GuildTrait {
            emoji_id: Some("123456789".into()),
            emoji_name: Some("blob".into()),
            label: "Cute".into(),
        };
        assert!(validate_traits(vec![ok.clone()]).is_ok());
        let mut bad_id = ok.clone();
        bad_id.emoji_id = Some("abc".into());
        assert!(validate_traits(vec![bad_id]).is_err());
        let mut uni = ok;
        uni.emoji_id = None;
        uni.emoji_name = Some("🎮".into());
        assert!(validate_traits(vec![uni]).is_ok());
    }
}
