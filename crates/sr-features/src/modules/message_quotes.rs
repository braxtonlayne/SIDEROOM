//! FTR-QUOTE-001 Message link quotes: a link to a message of this server gets an
//! embed preview of that message under it.
//!
//! Safety first: the preview is a public post, so it is only made when everyone who
//! can read the new message could already read the quoted one. That means the quoted
//! channel is open to the whole server (the `@everyone` role can read it) or is the
//! very channel the link was posted in; a quote never carries content from an
//! age-restricted channel into an ordinary one, and links to other servers are
//! ignored. The module's own posts and replies to nothing are not previewed again.

use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::{Value, json};
use sr_events::{ChannelId, DomainEvent, GuildId};

use crate::actions::OutMessage;
use crate::feature::{Feature, FeatureCtx, FeatureError, FeatureMeta, Scope};
use crate::schema::{Field, FieldKind, SettingsSchema};
use crate::template::{clip, defuse};

pub const QUOTES_ID: &str = "lounge.message_quotes";

const QUOTE_COLOR: u32 = 0x95a5a6;

#[derive(Default)]
pub struct MessageQuotes;

#[derive(Debug, Deserialize)]
#[serde(default)]
struct Config {
    max_quotes: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self { max_quotes: 2 }
    }
}

/// `(channel, message)` pairs of links to messages of `guild` in `text`, in order,
/// without repeats. A link looks like `.../channels/{guild}/{channel}/{message}`.
pub fn message_links(text: &str, guild: GuildId) -> Vec<(u64, u64)> {
    let mut out: Vec<(u64, u64)> = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("/channels/") {
        rest = &rest[at + "/channels/".len()..];
        let mut parts = rest.splitn(4, '/');
        let digits = |p: Option<&str>| -> Option<u64> {
            let p = p?;
            let end = p.find(|c: char| !c.is_ascii_digit()).unwrap_or(p.len());
            if end == 0 || end > 20 {
                return None;
            }
            p[..end].parse().ok().filter(|n| *n > 0)
        };
        // The first two segments must be entirely digits, the third may be followed by
        // punctuation, a query or a closing bracket.
        let g = parts.next().and_then(|p| {
            p.bytes()
                .all(|b| b.is_ascii_digit())
                .then(|| digits(Some(p)))
                .flatten()
        });
        let c = parts.next().and_then(|p| {
            p.bytes()
                .all(|b| b.is_ascii_digit())
                .then(|| digits(Some(p)))
                .flatten()
        });
        let m = digits(parts.next());
        if let (Some(g), Some(c), Some(m)) = (g, c, m)
            && g == guild.get()
            && !out.contains(&(c, m))
        {
            out.push((c, m));
        }
    }
    out
}

impl Feature for MessageQuotes {
    fn id(&self) -> &'static str {
        QUOTES_ID
    }

    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Message link quotes".into(),
            description: "Show a preview of the message behind a message link, when everyone \
                          here could already read it."
                .into(),
            section: "Lounge".into(),
            phase: 11,
            default_enabled: false,
        }
    }

    fn settings_schema(&self) -> SettingsSchema {
        SettingsSchema::new(vec![
            Field::new(
                "max_quotes",
                "Previews per message",
                FieldKind::Int {
                    min: Some(1),
                    max: Some(5),
                },
            )
            .default_value(json!(2)),
        ])
    }

    fn scopes(&self) -> &[Scope] {
        &[Scope::Platform, Scope::Guild]
    }

    fn on_event<'a>(
        &'a self,
        ev: &'a DomainEvent,
        ctx: &'a FeatureCtx,
    ) -> BoxFuture<'a, Result<(), FeatureError>> {
        Box::pin(async move {
            let DomainEvent::MessageSent {
                guild: Some(g),
                channel,
                author,
                data,
                ..
            } = ev
            else {
                return Ok(());
            };
            if *author == ctx.actions.self_user() {
                return Ok(());
            }
            let Some(content) = data.get("content").and_then(Value::as_str) else {
                return Ok(());
            };
            let links = message_links(content, *g);
            if links.is_empty() {
                return Ok(());
            }
            let cfg: Config = ctx.guild_config(QUOTES_ID, *g).await?;
            let here = ctx.actions.get_channel(*channel).await?;
            let here_nsfw = here.as_ref().is_some_and(|c| c.nsfw);
            let mut posted = 0;
            for (src_channel, src_message) in links {
                if posted >= cfg.max_quotes.max(1) {
                    break;
                }
                let src_id = ChannelId::new(src_channel);
                if src_id != *channel {
                    let Some(src) = ctx.actions.get_channel(src_id).await? else {
                        continue;
                    };
                    if !src.everyone_can_view || src.guild != Some(*g) || (src.nsfw && !here_nsfw) {
                        continue;
                    }
                }
                let Some(msg) = ctx.actions.get_message(src_id, src_message).await? else {
                    continue;
                };
                let name = ctx
                    .actions
                    .get_user(msg.author)
                    .await?
                    .map(|u| u.name)
                    .unwrap_or_else(|| "Unknown".into());
                let jump = format!(
                    "{}/channels/{}/{}/{}",
                    ctx.actions.base_url(),
                    g.get(),
                    src_channel,
                    src_message
                );
                let text = if msg.content.is_empty() && !msg.attachments.is_empty() {
                    "(attachment)".to_owned()
                } else {
                    clip(&msg.content, 500)
                };
                let mut embed = json!({
                    "type": "rich",
                    "color": QUOTE_COLOR,
                    "author": {"name": defuse(&name)},
                    "description": text,
                    "fields": [{
                        "name": "Quoted from",
                        "value": format!("<#{src_channel}> - [Jump to message]({jump})"),
                    }],
                });
                if let Some(img) = msg.attachments.iter().find(|a| {
                    a.content_type
                        .as_deref()
                        .is_some_and(|t| t.starts_with("image/"))
                }) {
                    embed["image"] = json!({"url": img.url});
                }
                ctx.actions
                    .post_message(
                        *channel,
                        OutMessage {
                            content: String::new(),
                            embed: Some(embed),
                            mention_users: Vec::new(),
                        },
                    )
                    .await?;
                posted += 1;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use sr_events::UserId;

    use super::*;
    use crate::FeatureRegistry;
    use crate::actions::{ChannelInfo, MessageInfo};
    use crate::testing::Harness;

    const G: GuildId = GuildId::new(1);
    const HERE: ChannelId = ChannelId::new(10);

    #[test]
    fn extracts_links_of_this_guild_only() {
        let t = "see https://sr.test/channels/1/20/300 and (https://sr.test/channels/1/20/300) \
                 plus https://sr.test/channels/2/20/301 and /channels/1/21/302?x=1 \
                 and /channels/1/@me/5 and /channels/1/20/";
        assert_eq!(message_links(t, G), vec![(20, 300), (21, 302)]);
        assert!(message_links("nothing here", G).is_empty());
    }

    fn channel(id: u64, public: bool, nsfw: bool) -> ChannelInfo {
        ChannelInfo {
            id: ChannelId::new(id),
            guild: Some(G),
            kind: 0,
            name: format!("c{id}"),
            nsfw,
            parent: None,
            rate_limit_per_user: 0,
            everyone_can_view: public,
        }
    }

    async fn harness() -> Harness {
        let mut reg = FeatureRegistry::new();
        reg.register(MessageQuotes).unwrap();
        let h = Harness::new(reg).await;
        h.actions.put_user(7, "Ada");
        h.actions.put_channel(channel(10, true, false));
        h.actions.put_channel(channel(20, true, false));
        h.actions.put_channel(channel(21, false, false));
        h.actions.put_channel(channel(22, true, true));
        for (ch, id) in [(20, 300), (21, 301), (22, 302), (10, 303)] {
            h.actions.put_message(MessageInfo {
                id,
                channel: ChannelId::new(ch),
                guild: Some(G),
                author: UserId::new(7),
                content: format!("body {id}"),
                embeds: vec![],
                attachments: vec![],
            });
        }
        h
    }

    fn said(content: &str) -> DomainEvent {
        DomainEvent::MessageSent {
            guild: Some(G),
            channel: HERE,
            message_id: 1,
            author: UserId::new(5),
            data: json!({"content": content, "type": 0}),
        }
    }

    #[tokio::test]
    async fn quotes_public_channels() {
        let h = harness().await;
        MessageQuotes
            .on_event(&said("look /channels/1/20/300"), &h.ctx)
            .await
            .unwrap();
        let posts = h.actions.posts();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].0, HERE);
        let e = posts[0].1.embed.clone().unwrap();
        assert_eq!(e["description"], "body 300");
        assert_eq!(e["author"]["name"], "Ada");
    }

    #[tokio::test]
    async fn never_leaks_private_or_age_restricted_channels() {
        let h = harness().await;
        MessageQuotes
            .on_event(&said("/channels/1/21/301 and /channels/1/22/302"), &h.ctx)
            .await
            .unwrap();
        assert!(h.actions.posts().is_empty());
    }

    #[tokio::test]
    async fn quoting_within_a_private_channel_is_fine() {
        let h = harness().await;
        h.actions.put_channel(channel(10, false, false));
        MessageQuotes
            .on_event(&said("/channels/1/10/303"), &h.ctx)
            .await
            .unwrap();
        assert_eq!(h.actions.posts().len(), 1);
    }

    #[tokio::test]
    async fn caps_the_number_of_previews_and_ignores_itself() {
        let h = harness().await;
        MessageQuotes
            .on_event(
                &said("/channels/1/20/300 /channels/1/10/303 /channels/1/22/302"),
                &h.ctx,
            )
            .await
            .unwrap();
        assert_eq!(h.actions.posts().len(), 2);

        h.actions.clear_calls();
        let own = DomainEvent::MessageSent {
            guild: Some(G),
            channel: HERE,
            message_id: 2,
            author: UserId::new(1),
            data: json!({"content": "/channels/1/20/300"}),
        };
        MessageQuotes.on_event(&own, &h.ctx).await.unwrap();
        assert!(h.actions.posts().is_empty());
    }

    #[test]
    fn schema_defaults_validate() {
        let s = MessageQuotes.settings_schema();
        s.validate(&s.defaults(Scope::Guild)).unwrap();
    }
}
