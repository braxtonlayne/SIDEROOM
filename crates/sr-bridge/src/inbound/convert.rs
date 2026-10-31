//! Discord dispatch payloads to the plain values the bridge works with: no I/O, so every rule here is a unit test.
//!
//! Ids are decimal strings on the wire and `i64` in the database. A payload that lacks what the bridge needs is
//! `None` (it is dropped, never guessed at).

use serde_json::Value;

/// Longest message the bridge brings in (the server's own message limit); the rest is cut with an ellipsis.
pub const MAX_CONTENT_CHARS: usize = 4000;
/// How long a Discord attachment link is assumed to live when it carries no `ex` (about a day, note 8).
pub const DEFAULT_LINK_LIFE_MS: i64 = 24 * 3600 * 1000;

/// An attachment as a link. Nothing is downloaded or re-hosted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InLink {
    pub filename: String,
    pub url: String,
    /// When Discord's CDN stops serving the link (its `ex` parameter), Unix ms.
    pub expires_at_ms: i64,
}

/// A `MESSAGE_CREATE` from a mapped channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InMessage {
    pub id: i64,
    pub channel_id: i64,
    pub author_id: i64,
    pub author_name: String,
    pub author_avatar: Option<String>,
    pub author_is_bot: bool,
    /// Set when a webhook posted it (including the bridge's own webhooks: the loop guard reads this).
    pub webhook_id: Option<i64>,
    pub content: String,
    /// The Discord message this one answers.
    pub reply_to: Option<i64>,
    pub links: Vec<InLink>,
}

/// A `MESSAGE_UPDATE`. A partial update (an embed resolved, no `content`) has `content: None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InEdit {
    pub id: i64,
    pub channel_id: i64,
    pub content: Option<String>,
    pub links: Option<Vec<InLink>>,
}

/// A `MESSAGE_DELETE` or `MESSAGE_DELETE_BULK`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InDelete {
    pub ids: Vec<i64>,
    pub channel_id: i64,
}

/// A `MESSAGE_REACTION_ADD` or `MESSAGE_REACTION_REMOVE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InReaction {
    pub channel_id: i64,
    pub message_id: i64,
    pub user_id: i64,
    /// A Unicode emoji; `None` for a custom emoji (which is shown as text only, never mirrored).
    pub unicode: Option<String>,
    pub add: bool,
}

pub fn id_of(v: &Value) -> Option<i64> {
    v.as_str()
        .and_then(|s| s.parse::<i64>().ok())
        .or_else(|| v.as_i64())
        .filter(|n| *n > 0)
}

fn text(v: &Value) -> Option<&str> {
    v.as_str().filter(|s| !s.is_empty())
}

/// Message types worth mirroring: default (0), reply (19). Joins, pins, boosts and so on stay on Discord.
const MIRRORED_TYPES: [i64; 2] = [0, 19];

pub fn parse_message(d: &Value) -> Option<InMessage> {
    let kind = d.get("type").and_then(Value::as_i64).unwrap_or(0);
    if !MIRRORED_TYPES.contains(&kind) {
        return None;
    }
    let author = d.get("author")?;
    let author_id = id_of(author.get("id")?)?;
    let name = d
        .pointer("/member/nick")
        .and_then(text)
        .or_else(|| author.get("global_name").and_then(text))
        .or_else(|| author.get("username").and_then(text))
        .unwrap_or("Discord user");
    let mentions = mention_names(d);
    Some(InMessage {
        id: id_of(d.get("id")?)?,
        channel_id: id_of(d.get("channel_id")?)?,
        author_id,
        author_name: clean_name(name),
        author_avatar: avatar_url(author, author_id),
        author_is_bot: author.get("bot").and_then(Value::as_bool).unwrap_or(false),
        webhook_id: d.get("webhook_id").and_then(id_of),
        content: clean_content(
            d.get("content").and_then(Value::as_str).unwrap_or(""),
            &mentions,
        ),
        reply_to: d
            .pointer("/message_reference/message_id")
            .and_then(id_of)
            // A forward carries `message_reference.type = 1`; only a real reply points at a message here.
            .filter(|_| {
                d.pointer("/message_reference/type")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    == 0
            }),
        links: parse_links(d, now_ms()),
    })
}

pub fn parse_edit(d: &Value) -> Option<InEdit> {
    // An edit of a message type the bridge never mirrored simply finds no map row.
    Some(InEdit {
        id: id_of(d.get("id")?)?,
        channel_id: id_of(d.get("channel_id")?)?,
        content: d
            .get("content")
            .and_then(Value::as_str)
            .map(|c| clean_content(c, &mention_names(d))),
        links: d
            .get("attachments")
            .is_some()
            .then(|| parse_links(d, now_ms())),
    })
}

pub fn parse_delete(name: &str, d: &Value) -> Option<InDelete> {
    let channel_id = id_of(d.get("channel_id")?)?;
    let ids = if name == "MESSAGE_DELETE_BULK" {
        d.get("ids")?.as_array()?.iter().filter_map(id_of).collect()
    } else {
        vec![id_of(d.get("id")?)?]
    };
    Some(InDelete { ids, channel_id })
}

pub fn parse_reaction(name: &str, d: &Value) -> Option<InReaction> {
    let emoji = d.get("emoji")?;
    let unicode = match emoji.get("id") {
        Some(Value::Null) | None => emoji.get("name").and_then(text).map(str::to_owned),
        Some(_) => None,
    };
    Some(InReaction {
        channel_id: id_of(d.get("channel_id")?)?,
        message_id: id_of(d.get("message_id")?)?,
        user_id: id_of(d.get("user_id")?)?,
        unicode,
        add: name == "MESSAGE_REACTION_ADD",
    })
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// `(id, name)` of everyone the message mentions, for turning `<@id>` into text.
fn mention_names(d: &Value) -> Vec<(i64, String)> {
    d.get("mentions")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|u| {
                    let id = id_of(u.get("id")?)?;
                    let name = u
                        .pointer("/member/nick")
                        .and_then(text)
                        .or_else(|| u.get("global_name").and_then(text))
                        .or_else(|| u.get("username").and_then(text))?;
                    Some((id, clean_name(name)))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A display name with control characters removed and a sane length (the name is shown as an author).
pub fn clean_name(name: &str) -> String {
    let n: String = name
        .chars()
        .filter(|c| !c.is_control())
        .take(80)
        .collect::<String>()
        .trim()
        .to_owned();
    if n.is_empty() {
        "Discord user".to_owned()
    } else {
        n
    }
}

fn avatar_url(author: &Value, id: i64) -> Option<String> {
    let hash = author.get("avatar").and_then(text)?;
    if !hash.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some(format!(
        "https://cdn.discordapp.com/avatars/{id}/{hash}.png?size=128"
    ))
}

fn parse_links(d: &Value, now: i64) -> Vec<InLink> {
    d.get("attachments")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    let url = text(x.get("url")?)?;
                    // Only Discord's own CDN is linked; anything else in an attachment slot is dropped.
                    if !is_discord_cdn(url) {
                        return None;
                    }
                    Some(InLink {
                        filename: clean_name(
                            x.get("filename").and_then(Value::as_str).unwrap_or("file"),
                        ),
                        url: url.to_owned(),
                        expires_at_ms: link_expiry_ms(url, now),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Discord serves attachments from `cdn.discordapp.com` and `media.discordapp.net` (and the `*.discordapp.com`
/// proxies). A fake Discord in tests uses `127.0.0.1`/`localhost` over plain http, which is allowed too.
pub fn is_discord_cdn(url: &str) -> bool {
    let Some(host) = sr_sdk::gateway::url_host(url) else {
        return false;
    };
    let secure_discord = url.starts_with("https://")
        && (host == "discordapp.com"
            || host.ends_with(".discordapp.com")
            || host.ends_with(".discordapp.net"));
    let local_fake = url.starts_with("http://") && (host == "127.0.0.1" || host == "localhost");
    secure_discord || local_fake
}

/// When a Discord CDN link expires: its `ex` query parameter (hex Unix seconds), else a day after `now_ms`.
pub fn link_expiry_ms(url: &str, now_ms: i64) -> i64 {
    let ex = url
        .split_once('?')
        .map(|(_, q)| q)
        .into_iter()
        .flat_map(|q| q.split('&'))
        .find_map(|kv| kv.strip_prefix("ex="))
        .and_then(|h| i64::from_str_radix(h, 16).ok())
        .filter(|s| *s > 0);
    match ex {
        Some(secs) => secs.saturating_mul(1000),
        None => now_ms + DEFAULT_LINK_LIFE_MS,
    }
}

/// Discord markup to what reads right here, and never a ping: user, role and channel mentions become plain text
/// (a name, not a `<@id>` token the server would resolve), `@everyone` and `@here` get a zero-width space, custom
/// emoji become `:name:` (the emoji does not exist here, note 8), and the result is cut to [`MAX_CONTENT_CHARS`].
pub fn clean_content(raw: &str, mentions: &[(i64, String)]) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        if let Some(end) = tail.find('>').filter(|e| *e <= 64) {
            let token = &tail[1..end];
            if let Some(repl) = rewrite_token(token, mentions) {
                out.push_str(&repl);
                rest = &tail[end + 1..];
                continue;
            }
        }
        out.push('<');
        rest = &tail[1..];
    }
    out.push_str(rest);
    let out = out
        .replace("@everyone", "@\u{200b}everyone")
        .replace("@here", "@\u{200b}here");
    if out.chars().count() > MAX_CONTENT_CHARS {
        let mut cut: String = out.chars().take(MAX_CONTENT_CHARS - 1).collect();
        cut.push('\u{2026}');
        cut
    } else {
        out
    }
}

fn rewrite_token(token: &str, mentions: &[(i64, String)]) -> Option<String> {
    if let Some(id) = token
        .strip_prefix("@!")
        .or_else(|| token.strip_prefix('@').filter(|r| !r.starts_with('&')))
    {
        let id: i64 = id.parse().ok()?;
        let name = mentions
            .iter()
            .find(|(m, _)| *m == id)
            .map_or("unknown-user", |(_, n)| n.as_str());
        return Some(format!("@\u{200b}{name}"));
    }
    if token
        .strip_prefix("@&")
        .is_some_and(|r| r.parse::<i64>().is_ok())
    {
        return Some("@\u{200b}role".to_owned());
    }
    if token
        .strip_prefix('#')
        .is_some_and(|r| r.parse::<i64>().is_ok())
    {
        return Some("#channel".to_owned());
    }
    // <:name:id> and <a:name:id>
    let body = token
        .strip_prefix("a:")
        .or_else(|| token.strip_prefix(':'))?;
    let (name, id) = body.split_once(':')?;
    (id.parse::<i64>().is_ok()
        && !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
    .then(|| format!(":{name}:"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_plain_message_parses_with_the_nick_first_and_a_cdn_avatar() {
        let m = parse_message(&json!({
            "id": "100", "channel_id": "200", "type": 0, "content": "hi",
            "author": {"id": "7", "username": "bob", "global_name": "Bob", "avatar": "abc123"},
            "member": {"nick": "Bobby"}
        }))
        .expect("parses");
        assert_eq!((m.id, m.channel_id, m.author_id), (100, 200, 7));
        assert_eq!(m.author_name, "Bobby");
        assert_eq!(
            m.author_avatar.as_deref(),
            Some("https://cdn.discordapp.com/avatars/7/abc123.png?size=128")
        );
        assert!(!m.author_is_bot && m.webhook_id.is_none() && m.reply_to.is_none());
    }

    #[test]
    fn system_messages_and_payloads_without_an_author_are_dropped() {
        let base = |t: i64| {
            json!({"id":"1","channel_id":"2","type":t,"content":"x","author":{"id":"3","username":"u"}})
        };
        assert!(parse_message(&base(0)).is_some());
        assert!(parse_message(&base(19)).is_some());
        assert!(parse_message(&base(7)).is_none(), "a member joined");
        assert!(parse_message(&base(6)).is_none(), "a pin notice");
        assert!(parse_message(&json!({"id":"1","channel_id":"2"})).is_none());
    }

    #[test]
    fn a_reply_points_at_its_target_but_a_forward_does_not() {
        let mut d = json!({"id":"1","channel_id":"2","type":19,"content":"x","author":{"id":"3","username":"u"},
            "message_reference":{"message_id":"55","channel_id":"2"}});
        assert_eq!(parse_message(&d).unwrap().reply_to, Some(55));
        d["message_reference"]["type"] = json!(1);
        assert_eq!(parse_message(&d).unwrap().reply_to, None);
    }

    #[test]
    fn the_expiry_is_the_ex_parameter_or_a_day_from_now() {
        assert_eq!(
            link_expiry_ms(
                "https://cdn.discordapp.com/a/1/f.png?ex=65f00000&is=1&hm=2",
                5
            ),
            0x65f0_0000_i64 * 1000
        );
        assert_eq!(
            link_expiry_ms("https://cdn.discordapp.com/a/1/f.png", 5),
            5 + DEFAULT_LINK_LIFE_MS
        );
        assert_eq!(
            link_expiry_ms("https://cdn.discordapp.com/a/1/f.png?ex=zz", 5),
            5 + DEFAULT_LINK_LIFE_MS
        );
    }

    #[test]
    fn only_discord_cdn_links_are_kept() {
        let d = json!({"id":"1","channel_id":"2","content":"","author":{"id":"3","username":"u"},
          "attachments":[
            {"filename":"a.png","url":"https://cdn.discordapp.com/attachments/1/2/a.png?ex=65f00000"},
            {"filename":"b.png","url":"https://media.discordapp.net/attachments/1/2/b.png"},
            {"filename":"c.png","url":"https://evil.example/c.png"},
            {"filename":"d.png","url":"https://discordapp.com.evil.example/d.png"}]});
        let links = parse_message(&d).unwrap().links;
        let names: Vec<_> = links.iter().map(|l| l.filename.as_str()).collect();
        assert_eq!(names, ["a.png", "b.png"]);
    }

    #[test]
    fn mentions_become_text_and_never_a_ping() {
        let m = vec![(9, "Ann".to_owned())];
        assert_eq!(
            clean_content("hi <@9> and <@!9>", &m),
            "hi @\u{200b}Ann and @\u{200b}Ann"
        );
        assert_eq!(clean_content("<@12>", &m), "@\u{200b}unknown-user");
        assert_eq!(clean_content("<@&77> <#5>", &m), "@\u{200b}role #channel");
        assert_eq!(
            clean_content("@everyone @here", &m),
            "@\u{200b}everyone @\u{200b}here"
        );
        assert!(!clean_content("<@9> <@&1> @everyone", &m).contains("<@"));
    }

    #[test]
    fn custom_emoji_are_text_and_unrelated_angle_brackets_survive() {
        assert_eq!(
            clean_content("<:pog:123> <a:dance_x:456>", &[]),
            ":pog: :dance_x:"
        );
        assert_eq!(clean_content("1 < 2 > 0 <b>", &[]), "1 < 2 > 0 <b>");
        assert_eq!(
            clean_content("<https://example.com>", &[]),
            "<https://example.com>"
        );
        assert_eq!(clean_content("unterminated <@9", &[]), "unterminated <@9");
    }

    #[test]
    fn long_content_is_cut_with_an_ellipsis() {
        let out = clean_content(&"a".repeat(MAX_CONTENT_CHARS + 50), &[]);
        assert_eq!(out.chars().count(), MAX_CONTENT_CHARS);
        assert!(out.ends_with('\u{2026}'));
    }

    #[test]
    fn edits_deletes_and_reactions_parse() {
        let e = parse_edit(&json!({"id":"1","channel_id":"2","content":"new"})).unwrap();
        assert_eq!(e.content.as_deref(), Some("new"));
        assert!(e.links.is_none(), "no attachments key: leave the links alone");
        let partial = parse_edit(&json!({"id":"1","channel_id":"2","embeds":[]})).unwrap();
        assert!(partial.content.is_none());
        let d = parse_delete("MESSAGE_DELETE", &json!({"id":"1","channel_id":"2"})).unwrap();
        assert_eq!(d.ids, [1]);
        let b = parse_delete(
            "MESSAGE_DELETE_BULK",
            &json!({"ids":["1","3"],"channel_id":"2"}),
        )
        .unwrap();
        assert_eq!(b.ids, [1, 3]);
        let r = parse_reaction(
            "MESSAGE_REACTION_ADD",
            &json!({"channel_id":"2","message_id":"1","user_id":"3","emoji":{"id":null,"name":"\u{1F44D}"}}),
        )
        .unwrap();
        assert!(r.add && r.unicode.as_deref() == Some("\u{1F44D}"));
        let c = parse_reaction(
            "MESSAGE_REACTION_REMOVE",
            &json!({"channel_id":"2","message_id":"1","user_id":"3","emoji":{"id":"9","name":"pog"}}),
        )
        .unwrap();
        assert!(!c.add && c.unicode.is_none(), "a custom emoji is never mirrored");
    }
}
