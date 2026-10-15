//! FTR-ATH-001 Auto-thread: every message posted in the chosen channels gets its own
//! thread, named from a template.
//!
//! Placeholders: `{author}`, `{content}` (the first line, cut to 50 characters),
//! `{date}` (UTC, `YYYY-MM-DD`). Replies, system messages, the module's own posts and
//! (by default) bot posts do not start threads.

use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::{Value, json};
use sr_events::{ChannelId, DomainEvent};

use crate::feature::{Feature, FeatureCtx, FeatureError, FeatureMeta, Scope};
use crate::schema::{Field, FieldKind, SettingsSchema};
use crate::template::{clip, render};

use super::{snowflakes, utc_date};

pub const AUTO_THREAD_ID: &str = "lounge.auto_thread";

/// Discord limit for thread names.
const NAME_MAX: usize = 100;

#[derive(Default)]
pub struct AutoThread;

#[derive(Debug, Deserialize)]
#[serde(default)]
struct Config {
    channels: Vec<String>,
    name_template: String,
    ignore_bots: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            channels: Vec::new(),
            name_template: "{author}: {content}".into(),
            ignore_bots: true,
        }
    }
}

/// Thread name for a message, never empty and at most 100 characters.
pub fn thread_name(template: &str, author: &str, content: &str, now_ms: i64) -> String {
    let first_line = content.lines().next().unwrap_or("").trim();
    let vars = [
        ("author", author.to_owned()),
        ("content", clip(first_line, 50)),
        ("date", utc_date(now_ms)),
    ];
    let rendered = render(template, &vars);
    // Thread names are one line; collapse whitespace and trim leftovers like ": ".
    let one_line = rendered.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = one_line.trim_matches(|c: char| c == ':' || c == '-' || c.is_whitespace());
    let name = clip(trimmed, NAME_MAX);
    if name.is_empty() {
        "Thread".into()
    } else {
        name
    }
}

impl Feature for AutoThread {
    fn id(&self) -> &'static str {
        AUTO_THREAD_ID
    }

    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Auto-thread".into(),
            description: "Start a thread on every message posted in chosen channels.".into(),
            section: "Lounge".into(),
            phase: 11,
            default_enabled: false,
        }
    }

    fn settings_schema(&self) -> SettingsSchema {
        SettingsSchema::new(vec![
            Field::new(
                "channels",
                "Channels",
                FieldKind::ChannelPicker { multi: true },
            ),
            Field::new(
                "name_template",
                "Thread name",
                FieldKind::String { max_len: Some(100) },
            )
            .help("Placeholders: {author} {content} {date}")
            .default_value(json!("{author}: {content}")),
            Field::new("ignore_bots", "Skip bot messages", FieldKind::Bool)
                .default_value(json!(true)),
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
                message_id,
                author,
                data,
            } = ev
            else {
                return Ok(());
            };
            if *author == ctx.actions.self_user() {
                return Ok(());
            }
            // Only plain messages (type 0) start threads; replies carry type 19.
            if data.get("type").and_then(Value::as_u64).unwrap_or(0) != 0 {
                return Ok(());
            }
            let cfg: Config = ctx.guild_config(AUTO_THREAD_ID, *g).await?;
            if !snowflakes(&cfg.channels).contains(&channel.get()) {
                return Ok(());
            }
            let user = ctx.actions.get_user(*author).await?;
            if cfg.ignore_bots && user.as_ref().is_some_and(|u| u.bot) {
                return Ok(());
            }
            let author_name = user.map(|u| u.name).unwrap_or_else(|| "Someone".into());
            let content = data.get("content").and_then(Value::as_str).unwrap_or("");
            let name = thread_name(&cfg.name_template, &author_name, content, ctx.now_ms());
            ctx.actions
                .create_thread(ChannelId::new(channel.get()), *message_id, &name)
                .await?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use sr_events::{GuildId, UserId};

    use super::*;
    use crate::FeatureRegistry;
    use crate::testing::{Call, Harness};

    const G: GuildId = GuildId::new(1);

    async fn harness() -> Harness {
        let mut reg = FeatureRegistry::new();
        reg.register(AutoThread).unwrap();
        let h = Harness::new(reg).await;
        h.actions.put_user(5, "Eve");
        h.configure(AUTO_THREAD_ID, json!({"channels": ["10"]}))
            .await;
        h
    }

    fn said(channel: u64, author: u64, data: Value) -> DomainEvent {
        DomainEvent::MessageSent {
            guild: Some(G),
            channel: ChannelId::new(channel),
            message_id: 42,
            author: UserId::new(author),
            data,
        }
    }

    #[test]
    fn names_are_one_line_bounded_and_never_empty() {
        assert_eq!(
            thread_name("{author}: {content}", "Eve", "hello\nsecond line", 0),
            "Eve: hello"
        );
        assert_eq!(thread_name("{date}", "x", "", 0), "1970-01-01");
        assert_eq!(thread_name("{content}", "x", "   ", 0), "Thread");
        let long = "a".repeat(300);
        assert_eq!(thread_name("{author}", &long, "", 0).chars().count(), 100);
        assert_eq!(thread_name("{author}: {content}", "Eve", "", 0), "Eve");
    }

    #[tokio::test]
    async fn starts_a_thread_in_configured_channels_only() {
        let h = harness().await;
        AutoThread
            .on_event(
                &said(10, 5, json!({"type": 0, "content": "any plans?"})),
                &h.ctx,
            )
            .await
            .unwrap();
        AutoThread
            .on_event(&said(11, 5, json!({"type": 0, "content": "other"})), &h.ctx)
            .await
            .unwrap();
        assert_eq!(
            h.actions.calls(),
            vec![Call::Thread(
                ChannelId::new(10),
                42,
                "Eve: any plans?".into()
            )]
        );
    }

    #[tokio::test]
    async fn skips_replies_system_messages_and_itself() {
        let h = harness().await;
        AutoThread
            .on_event(&said(10, 5, json!({"type": 19, "content": "re"})), &h.ctx)
            .await
            .unwrap();
        AutoThread
            .on_event(&said(10, 1, json!({"type": 0, "content": "me"})), &h.ctx)
            .await
            .unwrap();
        assert!(h.actions.calls().is_empty());
    }

    #[test]
    fn schema_defaults_validate() {
        let s = AutoThread.settings_schema();
        s.validate(&s.defaults(Scope::Guild)).unwrap();
    }
}
