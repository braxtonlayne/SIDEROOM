//! SIDE ROOM message content to what Discord should show (BRG-COR-001).
//!
//! SIDE ROOM speaks Discord's markdown, so most content passes unchanged. What must change is everything that
//! would *mean something different* on the other side:
//!
//! * `<@id>`, `<@!id>`, `<@&id>`, `<#id>` are ids of this server; on Discord they would resolve to unrelated
//!   people and channels (or ping them). They become plain `@name`, `@role` and `#channel`.
//! * The one exception is a **bridged member** (a Discord member of the mapped server, `bridge_members`): the
//!   bridged set of the server is looked up first, so `<@id>` of a bridged member goes out unchanged and is the only
//!   id the webhook's `allowed_mentions.users` lets ping. An id that is both is treated as the Discord member.
//! * Custom emoji `<:name:id>` are ours; they become `:name:`.
//! * `@everyone` and `@here` get a zero-width space so they cannot ping, and every post also sends
//!   `allowed_mentions: {parse: []}`.
//! * Embeds are dropped (only text is mirrored) and attachments are appended as links to the signed `/cdn`
//!   URL, never uploaded.
//!
//! Pure functions, no I/O: the queue resolves the names first and hands them in.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Discord's limit for a message is 2000 characters; the rest is left for the attachment links.
pub const MAX_CONTENT_CHARS: usize = 2000;
/// A webhook username is 1 to 80 characters.
pub const MAX_USERNAME_CHARS: usize = 80;
/// Lifetime of the signed links that leave the instance: the author's avatar and attachment links. One day, the
/// one place a signed CDN URL leaves the instance (documented in the handoff and the lawyer brief).
pub const SIGNED_LINK_TTL_SECS: u64 = 24 * 60 * 60;

/// Names the content refers to by id, resolved by the caller.
#[derive(Debug, Default, Clone)]
pub struct Names {
    pub users: HashMap<String, String>,
    pub roles: HashMap<String, String>,
    pub channels: HashMap<String, String>,
    /// Ids (decimal) of the server's bridged Discord members that the content mentions: kept as `<@id>`.
    pub bridged: HashSet<String>,
    /// BRG-SYN-003 (whole-guild mirror, design 7.2): SIDE ROOM role id to the Discord role id it is mapped to.
    /// A mapped `<@&id>` goes out as the Discord role mention (it renders as the role there and never pings:
    /// every post sends `allowed_mentions: {parse: []}`). Empty in selective mode.
    pub role_ids: HashMap<String, String>,
    /// SIDE ROOM channel id to the Discord channel id it is mapped to, so `<#id>` is a working channel link.
    pub channel_ids: HashMap<String, String>,
}

/// An attachment as the queue keeps it until the send (the link is signed then, so a retry gets a fresh one).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentLink {
    pub filename: String,
    /// The relative `/cdn/attachments/...` path of the message object.
    pub path: String,
}

/// Replace every `<...>` token of `s` that `f` knows how to rewrite.
fn rewrite_tokens(s: &str, f: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        match tail.find('>') {
            Some(j) if j <= 64 => {
                match f(&tail[1..j]) {
                    Some(r) => out.push_str(&r),
                    None => out.push_str(&tail[..=j]),
                }
                rest = &tail[j + 1..];
            }
            _ => {
                out.push('<');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// Channel ids the content mentions (`<#id>`), for the caller to resolve.
pub fn channel_mention_ids(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = content;
    while let Some(i) = rest.find("<#") {
        let tail = &rest[i + 2..];
        if let Some(j) = tail.find('>')
            && digits(&tail[..j])
            && j <= 24
        {
            out.push(tail[..j].to_owned());
            rest = &tail[j + 1..];
        } else {
            rest = tail;
        }
    }
    out
}

/// User ids the content mentions (`<@id>` and `<@!id>`), for the caller to look up among the bridged members.
pub fn user_mention_ids(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = content;
    while let Some(i) = rest.find("<@") {
        let tail = &rest[i + 2..];
        let tail = tail.strip_prefix('!').unwrap_or(tail);
        if let Some(j) = tail.find('>')
            && j <= 24
            && digits(&tail[..j])
        {
            if !out.iter().any(|v: &String| v == &tail[..j]) {
                out.push(tail[..j].to_owned());
            }
            rest = &tail[j + 1..];
        } else {
            rest = tail;
        }
    }
    out
}

/// The bridged members a rendered post may ping on Discord (Discord takes at most 100).
pub fn bridged_pings(content: &str, names: &Names) -> Vec<String> {
    user_mention_ids(content)
        .into_iter()
        .filter(|id| names.bridged.contains(id))
        .take(100)
        .collect()
}

/// Make `content` safe and meaningful for Discord.
pub fn content_for_discord(content: &str, names: &Names) -> String {
    let s = rewrite_tokens(content, &|t| {
        if let Some(id) = t.strip_prefix("@&") {
            if let Some(d) = names.role_ids.get(id) {
                return Some(format!("<@&{d}>"));
            }
            return digits(id)
                .then(|| format!("@{}", names.roles.get(id).map_or("role", String::as_str)));
        }
        if let Some(id) = t.strip_prefix("@!").or_else(|| t.strip_prefix('@')) {
            if names.bridged.contains(id) {
                return Some(format!("<@{id}>"));
            }
            return digits(id)
                .then(|| format!("@{}", names.users.get(id).map_or("user", String::as_str)));
        }
        if let Some(id) = t.strip_prefix('#') {
            if let Some(d) = names.channel_ids.get(id) {
                return Some(format!("<#{d}>"));
            }
            return digits(id).then(|| {
                format!(
                    "#{}",
                    names.channels.get(id).map_or("channel", String::as_str)
                )
            });
        }
        // <:name:id> and <a:name:id>
        let e = t.strip_prefix("a:").or_else(|| t.strip_prefix(':'))?;
        let (name, id) = e.split_once(':')?;
        (digits(id) && !name.is_empty()).then(|| format!(":{name}:"))
    });
    neutralise_broadcast(&s)
}

/// `@everyone` and `@here` that survive as text cannot ping: a zero-width space follows the `@`.
fn neutralise_broadcast(s: &str) -> String {
    s.replace("@everyone", "@\u{200b}everyone")
        .replace("@here", "@\u{200b}here")
}

/// A webhook username Discord accepts: 1 to 80 characters, no `@`, `#`, `:` or code fences, and none of the
/// words Discord reserves for itself (`discord`, `clyde`), which are broken up with a zero-width space.
pub fn username_for_discord(name: &str) -> String {
    let mut s: String = name
        .chars()
        .filter(|c| !matches!(c, '@' | '#' | ':' | '`') && !c.is_control())
        .collect();
    s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    for banned in ["discord", "clyde", "everyone", "here"] {
        let lower = s.to_lowercase();
        if let Some(at) = lower.find(banned) {
            // Same byte offsets only when lowercasing kept lengths; otherwise leave it to the cut below.
            if lower.len() == s.len() && s.is_char_boundary(at + 1) {
                s.insert(at + 1, '\u{200b}');
            }
        }
    }
    let mut s: String = s.chars().take(MAX_USERNAME_CHARS).collect();
    if s.chars().count() < 2 {
        s = format!("{s} \u{b7}");
        s = s.trim_start().to_owned();
        if s.chars().count() < 2 {
            s = "member".into();
        }
    }
    s
}

/// Cut `s` to `max` characters, ending with an ellipsis when something was cut.
pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

/// The attachments of a message object, for the queue.
pub fn attachment_links(data: &Value) -> Vec<AttachmentLink> {
    data.get("attachments")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|a| {
                    let path = a.get("url")?.as_str()?.trim();
                    // The sensitive-media filter clears `url`; such an attachment is not mirrored.
                    if path.is_empty() || !path.starts_with("/cdn/") {
                        return None;
                    }
                    Some(AttachmentLink {
                        filename: a
                            .get("filename")
                            .and_then(Value::as_str)
                            .unwrap_or("file")
                            .to_owned(),
                        path: path.to_owned(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The text of a post: the rendered content, then one line per attachment (`name: link`), within Discord's
/// limit. The content gives way to the links, never the reverse.
pub fn compose(content: &str, links: &[(String, String)]) -> String {
    let tail: String = links
        .iter()
        .map(|(name, url)| format!("\n{}: {url}", username_safe_name(name)))
        .collect();
    let room = MAX_CONTENT_CHARS.saturating_sub(tail.chars().count());
    if room < 20 {
        // Too many links for a text of any use: keep the text, drop what does not fit.
        return truncate_chars(&format!("{content}{tail}"), MAX_CONTENT_CHARS);
    }
    let body = truncate_chars(content, room);
    format!("{body}{tail}").trim_start_matches('\n').to_owned()
}

fn username_safe_name(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_control() && !matches!(c, '[' | ']' | '(' | ')' | '`' | '<' | '>'))
        .take(60)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Names {
        let mut n = Names::default();
        n.users.insert("42".into(), "ann".into());
        n.roles.insert("7".into(), "mods".into());
        n.channels.insert("9".into(), "general".into());
        n
    }

    #[test]
    fn ids_of_this_server_become_plain_names() {
        let out = content_for_discord(
            "hi <@42> and <@!42> in <#9>, ping <@&7> and <@43>",
            &names(),
        );
        assert_eq!(out, "hi @ann and @ann in #general, ping @mods and @user");
    }

    #[test]
    fn a_bridged_member_keeps_the_mention_and_is_the_only_ping() {
        let mut n = names();
        // 42 is also an account here: the bridged set is looked at first, so it is the Discord member.
        n.bridged.insert("42".into());
        n.bridged.insert("77".into());
        let content = "hi <@42> <@!77> <@43> <@&7>";
        assert_eq!(
            content_for_discord(content, &n),
            "hi <@42> <@77> @user @mods"
        );
        assert_eq!(bridged_pings(content, &n), vec!["42", "77"]);
        assert_eq!(
            user_mention_ids("<@1> <@!2> <@&3> <@x> <@1>"),
            vec!["1", "2"]
        );
        // Nothing bridged: nobody to ping.
        assert!(bridged_pings(content, &names()).is_empty());
    }

    #[test]
    fn mapped_roles_and_channels_keep_their_mention_on_discord() {
        let mut n = names();
        n.role_ids.insert("7".into(), "7007".into());
        n.channel_ids.insert("9".into(), "9009".into());
        assert_eq!(
            content_for_discord("<@&7> in <#9>, <@&8> <#10>", &n),
            "<@&7007> in <#9009>, @role #channel"
        );
        // Still never a ping.
        assert!(bridged_pings("<@&7>", &n).is_empty());
    }

    #[test]
    fn custom_emoji_become_text_and_unicode_is_untouched() {
        assert_eq!(
            content_for_discord("<:wave:123> <a:party:456> \u{1f600} <b>bold</b>", &names()),
            ":wave: :party: \u{1f600} <b>bold</b>"
        );
    }

    #[test]
    fn broadcast_mentions_cannot_ping() {
        let out = content_for_discord("@everyone and @here", &names());
        assert!(!out.contains("@everyone") && !out.contains("@here"));
        assert!(out.contains('\u{200b}'));
    }

    #[test]
    fn a_lone_angle_bracket_is_left_alone() {
        assert_eq!(
            content_for_discord("1 < 2 and 3 > 2", &names()),
            "1 < 2 and 3 > 2"
        );
        assert_eq!(content_for_discord("<", &names()), "<");
    }

    #[test]
    fn channel_mentions_are_found() {
        assert_eq!(
            channel_mention_ids("a <#1> b <#22> <#x> <@3>"),
            vec!["1", "22"]
        );
    }

    #[test]
    fn usernames_are_made_acceptable() {
        assert_eq!(username_for_discord("  Ann   Lee "), "Ann Lee");
        assert_eq!(
            username_for_discord("@everyone#1:`x`"),
            "e\u{200b}veryone1x"
        );
        assert!(
            !username_for_discord("Discord Fan")
                .to_lowercase()
                .contains("discord")
        );
        assert!(username_for_discord("x").chars().count() >= 2);
        assert!(username_for_discord("").chars().count() >= 2);
        assert_eq!(username_for_discord(&"a".repeat(200)).chars().count(), 80);
    }

    #[test]
    fn compose_appends_links_and_stays_in_the_limit() {
        let out = compose(
            "hello",
            &[("a.png".into(), "https://sr.test/cdn/x?ex=1".into())],
        );
        assert_eq!(out, "hello\na.png: https://sr.test/cdn/x?ex=1");
        let long = "x".repeat(5000);
        let out = compose(&long, &[("a.png".into(), "https://sr.test/cdn/x".into())]);
        assert!(out.chars().count() <= MAX_CONTENT_CHARS);
        assert!(out.ends_with("a.png: https://sr.test/cdn/x"));
        // Attachments only: no leading blank line.
        assert_eq!(compose("", &[("a".into(), "u".into())]), "a: u");
    }

    #[test]
    fn attachment_links_skip_filtered_and_foreign_urls() {
        let data = serde_json::json!({"attachments": [
            {"filename": "a.png", "url": "/cdn/attachments/1/2/a.png"},
            {"filename": "b.png", "url": ""},
            {"filename": "c.png", "url": "https://elsewhere.test/c.png"}
        ]});
        let links = attachment_links(&data);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].filename, "a.png");
    }
}
