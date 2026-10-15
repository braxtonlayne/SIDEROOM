//! FTR-WEL-001 Welcome and goodbye messages.
//!
//! Templates take `{user}` (a mention, welcome only pings it), `{user.name}`,
//! `{server}` and `{member_count}`. A message can be plain text or an embed, and the
//! welcome can also be sent to the new member by DM.

use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::{Value, json};
use sr_events::{ChannelId, DomainEvent, GuildId, UserId};

use crate::actions::OutMessage;
use crate::feature::{Feature, FeatureCtx, FeatureError, FeatureMeta, Scope};
use crate::schema::{Field, FieldKind, SettingsSchema};
use crate::template::{clip, defuse, render};

use super::{payload_user_name, snowflake};

pub const WELCOME_ID: &str = "lounge.welcome";

const DEFAULT_COLOR: &str = "#5865f2";

#[derive(Default)]
pub struct Welcome;

#[derive(Debug, Deserialize)]
#[serde(default)]
struct Side {
    enabled: bool,
    channel: Option<String>,
    template: String,
    embed: bool,
    color: String,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct Config {
    welcome: Side,
    goodbye: Side,
    dm_enabled: bool,
    dm_template: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            welcome: Side {
                template: "Welcome to {server}, {user}! You are member #{member_count}.".into(),
                ..Side::default()
            },
            goodbye: Side {
                template: "{user.name} has left {server}.".into(),
                ..Side::default()
            },
            dm_enabled: false,
            dm_template: "Welcome to {server}! Take a look at the rules and say hi.".into(),
        }
    }
}

impl Default for Side {
    fn default() -> Self {
        Self {
            enabled: false,
            channel: None,
            template: String::new(),
            embed: false,
            color: DEFAULT_COLOR.into(),
        }
    }
}

fn parse_color(hex: &str) -> u32 {
    u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0x5865f2)
}

fn build(side: &Side, text: String, user: UserId, ping: bool) -> OutMessage {
    let text = clip(&text, 2000);
    if side.embed {
        OutMessage {
            content: String::new(),
            embed: Some(json!({
                "type": "rich",
                "description": text,
                "color": parse_color(&side.color),
            })),
            mention_users: Vec::new(),
        }
    } else {
        OutMessage {
            content: text,
            embed: None,
            mention_users: if ping { vec![user] } else { Vec::new() },
        }
    }
}

impl Welcome {
    async fn vars(
        &self,
        ctx: &FeatureCtx,
        guild: GuildId,
        user: UserId,
        payload: Option<&Value>,
        member_count_delta: i64,
    ) -> Result<Vec<(&'static str, String)>, FeatureError> {
        let name = match payload.and_then(payload_user_name) {
            Some(n) => n,
            None => ctx
                .actions
                .get_user(user)
                .await?
                .map(|u| u.name)
                .unwrap_or_else(|| "Someone".into()),
        };
        let g = ctx.actions.get_guild(guild).await?;
        let (server, count) = g
            .map(|g| (g.name, g.member_count as i64))
            .unwrap_or_else(|| ("this server".into(), 0));
        Ok(vec![
            ("user", format!("<@{}>", user.get())),
            ("user.name", defuse(&name)),
            ("server", defuse(&server)),
            (
                "member_count",
                (count + member_count_delta).max(0).to_string(),
            ),
        ])
    }

    async fn handle(&self, ev: &DomainEvent, ctx: &FeatureCtx) -> Result<(), FeatureError> {
        match ev {
            DomainEvent::MemberJoined {
                guild,
                user,
                payload,
            } => {
                if *user == ctx.actions.self_user() {
                    return Ok(());
                }
                let cfg: Config = ctx.guild_config(WELCOME_ID, *guild).await?;
                let vars = self.vars(ctx, *guild, *user, Some(payload), 0).await?;
                if cfg.welcome.enabled
                    && let Some(ch) = cfg.welcome.channel.as_deref().and_then(snowflake)
                {
                    let text = render(&cfg.welcome.template, &vars);
                    ctx.actions
                        .post_message(ChannelId::new(ch), build(&cfg.welcome, text, *user, true))
                        .await?;
                }
                if cfg.dm_enabled {
                    let text = clip(&render(&cfg.dm_template, &vars), 2000);
                    // A member with closed DMs is not an error worth stopping for.
                    if let Err(e) = ctx.actions.send_dm(*user, OutMessage::text(text)).await {
                        tracing::debug!(error = %e, "welcome DM not delivered");
                    }
                }
            }
            DomainEvent::MemberLeft { guild, user } => {
                let cfg: Config = ctx.guild_config(WELCOME_ID, *guild).await?;
                if !cfg.goodbye.enabled {
                    return Ok(());
                }
                let Some(ch) = cfg.goodbye.channel.as_deref().and_then(snowflake) else {
                    return Ok(());
                };
                // The guild's member count already excludes the leaver.
                let vars = self.vars(ctx, *guild, *user, None, 0).await?;
                let text = render(&cfg.goodbye.template, &vars);
                ctx.actions
                    .post_message(ChannelId::new(ch), build(&cfg.goodbye, text, *user, false))
                    .await?;
            }
            _ => {}
        }
        Ok(())
    }
}

fn side_fields(prefix_help: &str, template: &str, with_enabled_default: bool) -> Vec<Field> {
    vec![
        Field::new("enabled", "Enabled", FieldKind::Bool)
            .default_value(json!(with_enabled_default)),
        Field::new(
            "channel",
            "Channel",
            FieldKind::ChannelPicker { multi: false },
        )
        .help(prefix_help),
        Field::new(
            "template",
            "Message",
            FieldKind::String {
                max_len: Some(2000),
            },
        )
        .help("Placeholders: {user} {user.name} {server} {member_count}")
        .default_value(json!(template)),
        Field::new("embed", "Send as an embed", FieldKind::Bool).default_value(json!(false)),
        Field::new("color", "Embed color", FieldKind::Color).default_value(json!(DEFAULT_COLOR)),
    ]
}

impl Feature for Welcome {
    fn id(&self) -> &'static str {
        WELCOME_ID
    }

    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Welcome and goodbye".into(),
            description: "Greet new members and note departures in a channel, with an optional \
                          welcome DM."
                .into(),
            section: "Lounge".into(),
            phase: 11,
            default_enabled: false,
        }
    }

    fn settings_schema(&self) -> SettingsSchema {
        let d = Config::default();
        SettingsSchema::new(vec![
            Field::new(
                "welcome",
                "Welcome message",
                FieldKind::Group {
                    fields: side_fields(
                        "Where new members are greeted.",
                        &d.welcome.template,
                        false,
                    ),
                },
            ),
            Field::new(
                "goodbye",
                "Goodbye message",
                FieldKind::Group {
                    fields: side_fields("Where departures are noted.", &d.goodbye.template, false),
                },
            ),
            Field::new("dm_enabled", "Also DM new members", FieldKind::Bool)
                .default_value(json!(false)),
            Field::new(
                "dm_template",
                "Welcome DM",
                FieldKind::String {
                    max_len: Some(2000),
                },
            )
            .default_value(json!(d.dm_template)),
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
        Box::pin(self.handle(ev, ctx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FeatureRegistry;
    use crate::testing::{Call, Harness};

    const G: GuildId = GuildId::new(1);

    async fn harness(cfg: Value) -> Harness {
        let mut reg = FeatureRegistry::new();
        reg.register(Welcome).unwrap();
        let h = Harness::new(reg).await;
        h.actions.put_guild(1, "Club", 42);
        h.actions.put_user(9, "Bob");
        h.configure(WELCOME_ID, cfg).await;
        h
    }

    fn joined(user: u64, name: &str) -> DomainEvent {
        DomainEvent::MemberJoined {
            guild: G,
            user: UserId::new(user),
            payload: json!({"user": {"id": user.to_string(), "username": name}}),
        }
    }

    #[tokio::test]
    async fn welcomes_with_placeholders_and_pings_the_member() {
        let h = harness(json!({
            "welcome": {"enabled": true, "channel": "30",
                "template": "Hi {user} ({user.name}) to {server} #{member_count} {x}"}
        }))
        .await;
        Welcome.on_event(&joined(9, "Bob"), &h.ctx).await.unwrap();
        let posts = h.actions.posts();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].0, ChannelId::new(30));
        assert_eq!(posts[0].1.content, "Hi <@9> (Bob) to Club #42 {x}");
        assert_eq!(posts[0].1.mention_users, vec![UserId::new(9)]);
    }

    #[tokio::test]
    async fn names_cannot_inject_mentions() {
        let h = harness(json!({
            "welcome": {"enabled": true, "channel": "30", "template": "{user.name}"}
        }))
        .await;
        Welcome
            .on_event(&joined(9, "@everyone"), &h.ctx)
            .await
            .unwrap();
        assert!(!h.actions.posts()[0].1.content.contains("@everyone"));
    }

    #[tokio::test]
    async fn embed_style_and_dm() {
        let h = harness(json!({
            "welcome": {"enabled": true, "channel": "30", "embed": true, "color": "#ff0000",
                "template": "Hello {user.name}"},
            "dm_enabled": true, "dm_template": "DM for {user.name} at {server}"
        }))
        .await;
        Welcome.on_event(&joined(9, "Bob"), &h.ctx).await.unwrap();
        let post = &h.actions.posts()[0].1;
        let embed = post.embed.clone().unwrap();
        assert_eq!(embed["description"], "Hello Bob");
        assert_eq!(embed["color"], 0xff0000);
        assert!(post.content.is_empty());
        assert!(h.actions.calls().iter().any(|c| matches!(
            c,
            Call::Dm(u, m) if u.get() == 9 && m.content == "DM for Bob at Club"
        )));
    }

    #[tokio::test]
    async fn goodbye_uses_the_user_lookup() {
        let h = harness(json!({
            "goodbye": {"enabled": true, "channel": "31"}
        }))
        .await;
        Welcome
            .on_event(
                &DomainEvent::MemberLeft {
                    guild: G,
                    user: UserId::new(9),
                },
                &h.ctx,
            )
            .await
            .unwrap();
        let posts = h.actions.posts();
        assert_eq!(posts[0].0, ChannelId::new(31));
        assert_eq!(posts[0].1.content, "Bob has left Club.");
        assert!(posts[0].1.mention_users.is_empty());
    }

    #[tokio::test]
    async fn disabled_or_channelless_sides_stay_quiet() {
        let h = harness(json!({"welcome": {"enabled": true}})).await;
        Welcome.on_event(&joined(9, "Bob"), &h.ctx).await.unwrap();
        Welcome
            .on_event(
                &DomainEvent::MemberLeft {
                    guild: G,
                    user: UserId::new(9),
                },
                &h.ctx,
            )
            .await
            .unwrap();
        assert!(h.actions.calls().is_empty());
    }

    #[test]
    fn schema_defaults_validate() {
        let s = Welcome.settings_schema();
        s.validate(&s.defaults(Scope::Guild)).unwrap();
    }
}
