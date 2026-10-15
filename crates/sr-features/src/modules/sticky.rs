//! FTR-STK-001 Sticky messages: a message that stays at the bottom of a channel.
//!
//! Each sticky is re-posted (the old copy is deleted) once `every_messages` new
//! messages have arrived or `every_seconds` have passed since it was posted,
//! whichever comes first. Time is checked when a message arrives and on every tick,
//! so a quiet channel is not re-posted needlessly: the timer only matters once there
//! has been new chatter. Re-posts are at least [`MIN_GAP_MS`] apart so a fast channel
//! with `every_messages = 1` cannot make the bot spam.

use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::json;
use sr_events::{ChannelId, DomainEvent, GuildId};

use crate::actions::{ActionError, OutMessage};
use crate::feature::{Feature, FeatureCtx, FeatureError, FeatureMeta, Scope};
use crate::schema::{Field, FieldKind, SettingsSchema};
use crate::state::StateEntry;

use super::snowflake;

pub const STICKY_ID: &str = "lounge.sticky";

/// Shortest time between two re-posts in one channel.
pub const MIN_GAP_MS: i64 = 3_000;

#[derive(Default)]
pub struct Sticky;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Config {
    stickies: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct Entry {
    channel: String,
    content: String,
    every_messages: u32,
    every_seconds: u64,
}

impl Default for Entry {
    fn default() -> Self {
        Self {
            channel: String::new(),
            content: String::new(),
            every_messages: 5,
            every_seconds: 0,
        }
    }
}

#[derive(Debug, Default, Clone, Deserialize, serde::Serialize)]
struct Runtime {
    /// Messages seen since the sticky was posted.
    count: u32,
    /// Id of the live sticky post, 0 if none.
    post: u64,
    posted_ms: i64,
    /// The content that was posted, to notice edits to the setting.
    content: String,
}

fn key(channel: u64) -> String {
    format!("ch:{channel}")
}

impl Sticky {
    async fn load(
        &self,
        ctx: &FeatureCtx,
        guild: GuildId,
        channel: u64,
    ) -> Result<Runtime, FeatureError> {
        Ok(ctx
            .state
            .get(STICKY_ID, guild, &key(channel))
            .await?
            .and_then(|e| serde_json::from_value(e.value).ok())
            .unwrap_or_default())
    }

    async fn save(
        &self,
        ctx: &FeatureCtx,
        guild: GuildId,
        channel: u64,
        rt: &Runtime,
    ) -> Result<(), FeatureError> {
        ctx.state
            .put(StateEntry::new(STICKY_ID, guild, key(channel), json!(rt)))
            .await?;
        Ok(())
    }

    /// Whether `entry` is due, given what happened since it was posted.
    fn due(entry: &Entry, rt: &Runtime, now: i64) -> bool {
        if rt.post == 0 || rt.content != entry.content {
            return true;
        }
        if rt.count == 0 {
            return false;
        }
        if now - rt.posted_ms < MIN_GAP_MS {
            return false;
        }
        (entry.every_messages > 0 && rt.count >= entry.every_messages)
            || (entry.every_seconds > 0
                && now - rt.posted_ms >= (entry.every_seconds as i64) * 1000)
    }

    async fn repost(
        &self,
        ctx: &FeatureCtx,
        guild: GuildId,
        entry: &Entry,
        channel: ChannelId,
        mut rt: Runtime,
    ) -> Result<(), FeatureError> {
        if rt.post != 0 {
            match ctx.actions.delete_message(channel, rt.post).await {
                Ok(()) | Err(ActionError::NotFound) => {}
                Err(e) => return Err(e.into()),
            }
        }
        let post = ctx
            .actions
            .post_message(channel, OutMessage::text(entry.content.clone()))
            .await?;
        rt = Runtime {
            count: 0,
            post,
            posted_ms: ctx.now_ms(),
            content: entry.content.clone(),
        };
        self.save(ctx, guild, channel.get(), &rt).await
    }

    async fn on_message(
        &self,
        ctx: &FeatureCtx,
        guild: GuildId,
        channel: ChannelId,
        author_is_self: bool,
    ) -> Result<(), FeatureError> {
        if author_is_self {
            return Ok(());
        }
        let cfg: Config = ctx.guild_config(STICKY_ID, guild).await?;
        let Some(entry) = cfg
            .stickies
            .iter()
            .find(|s| snowflake(&s.channel) == Some(channel.get()) && !s.content.is_empty())
        else {
            return Ok(());
        };
        let mut rt = self.load(ctx, guild, channel.get()).await?;
        rt.count = rt.count.saturating_add(1);
        if Self::due(entry, &rt, ctx.now_ms()) {
            self.repost(ctx, guild, entry, channel, rt).await
        } else {
            self.save(ctx, guild, channel.get(), &rt).await
        }
    }
}

impl Feature for Sticky {
    fn id(&self) -> &'static str {
        STICKY_ID
    }

    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Sticky messages".into(),
            description: "Keep a message at the bottom of a channel by re-posting it as the \
                          conversation moves on."
                .into(),
            section: "Lounge".into(),
            phase: 11,
            default_enabled: false,
        }
    }

    fn settings_schema(&self) -> SettingsSchema {
        SettingsSchema::new(vec![Field::new(
            "stickies",
            "Sticky messages",
            FieldKind::List {
                max_items: Some(25),
                item: Box::new(FieldKind::Group {
                    fields: vec![
                        Field::new(
                            "channel",
                            "Channel",
                            FieldKind::ChannelPicker { multi: false },
                        ),
                        Field::new(
                            "content",
                            "Message",
                            FieldKind::String {
                                max_len: Some(2000),
                            },
                        ),
                        Field::new(
                            "every_messages",
                            "Re-post after this many messages",
                            FieldKind::Int {
                                min: Some(0),
                                max: Some(1000),
                            },
                        )
                        .help("0 turns the message count off.")
                        .default_value(json!(5)),
                        Field::new(
                            "every_seconds",
                            "Re-post after this long",
                            FieldKind::Duration {
                                min_secs: Some(0),
                                max_secs: Some(86_400),
                            },
                        )
                        .help("0 turns the timer off. Only counts once someone has talked.")
                        .default_value(json!(0)),
                    ],
                }),
            },
        )])
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
            if let DomainEvent::MessageSent {
                guild: Some(g),
                channel,
                author,
                ..
            } = ev
            {
                self.on_message(ctx, *g, *channel, *author == ctx.actions.self_user())
                    .await?;
            }
            Ok(())
        })
    }

    fn on_tick<'a>(&'a self, ctx: &'a FeatureCtx) -> BoxFuture<'a, Result<(), FeatureError>> {
        Box::pin(async move {
            for guild in ctx.settings.enabled_guilds(STICKY_ID).await? {
                let cfg: Config = ctx.guild_config(STICKY_ID, guild).await?;
                for entry in &cfg.stickies {
                    let Some(ch) = snowflake(&entry.channel) else {
                        continue;
                    };
                    if entry.content.is_empty() {
                        continue;
                    }
                    let rt = self.load(ctx, guild, ch).await?;
                    // Never-posted stickies wait for the first message; the tick
                    // only serves the timer of ones that are live.
                    if rt.post != 0 && Self::due(entry, &rt, ctx.now_ms()) {
                        self.repost(ctx, guild, entry, ChannelId::new(ch), rt)
                            .await?;
                    }
                }
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use sr_events::UserId;

    use super::*;
    use crate::FeatureRegistry;
    use crate::testing::{Call, Harness};

    const G: GuildId = GuildId::new(1);
    const CH: ChannelId = ChannelId::new(10);

    async fn harness(entry: Value) -> Harness {
        let mut reg = FeatureRegistry::new();
        reg.register(Sticky).unwrap();
        let h = Harness::new(reg).await;
        h.configure(STICKY_ID, json!({"stickies": [entry]})).await;
        h
    }

    fn said(author: u64) -> DomainEvent {
        DomainEvent::MessageSent {
            guild: Some(G),
            channel: CH,
            message_id: 1,
            author: UserId::new(author),
            data: json!({}),
        }
    }

    async fn talk(h: &Harness, n: usize) {
        for _ in 0..n {
            Sticky.on_event(&said(5), &h.ctx).await.unwrap();
            h.clock.advance_ms(MIN_GAP_MS + 1);
        }
    }

    #[tokio::test]
    async fn posts_on_first_message_then_every_n() {
        let h = harness(json!({"channel": "10", "content": "Read the rules", "every_messages": 3}))
            .await;
        talk(&h, 1).await;
        assert_eq!(h.actions.posts().len(), 1);
        talk(&h, 2).await;
        assert_eq!(h.actions.posts().len(), 1, "two messages since the post");
        talk(&h, 1).await;
        assert_eq!(h.actions.posts().len(), 2);
        assert!(
            h.actions
                .calls()
                .iter()
                .any(|c| matches!(c, Call::Delete(ch, _) if *ch == CH)),
            "the old copy is removed"
        );
    }

    #[tokio::test]
    async fn ignores_its_own_posts_and_other_channels() {
        let h = harness(json!({"channel": "10", "content": "x", "every_messages": 1})).await;
        Sticky.on_event(&said(1), &h.ctx).await.unwrap();
        Sticky
            .on_event(
                &DomainEvent::MessageSent {
                    guild: Some(G),
                    channel: ChannelId::new(99),
                    message_id: 1,
                    author: UserId::new(5),
                    data: json!({}),
                },
                &h.ctx,
            )
            .await
            .unwrap();
        assert!(h.actions.calls().is_empty());
    }

    #[tokio::test]
    async fn min_gap_defers_a_fast_repost_to_the_tick() {
        let h = harness(json!({"channel": "10", "content": "x", "every_messages": 1})).await;
        talk(&h, 1).await; // first post, then the clock moves past the gap
        Sticky.on_event(&said(5), &h.ctx).await.unwrap(); // reposts (gap passed)
        assert_eq!(h.actions.posts().len(), 2);
        Sticky.on_event(&said(5), &h.ctx).await.unwrap(); // too soon
        assert_eq!(h.actions.posts().len(), 2);
        h.clock.advance_ms(MIN_GAP_MS);
        Sticky.on_tick(&h.ctx).await.unwrap();
        assert_eq!(h.actions.posts().len(), 3);
    }

    #[tokio::test]
    async fn timer_reposts_after_chatter() {
        let h = harness(json!({
            "channel": "10", "content": "x", "every_messages": 0, "every_seconds": 60
        }))
        .await;
        talk(&h, 1).await;
        Sticky.on_event(&said(5), &h.ctx).await.unwrap();
        Sticky.on_tick(&h.ctx).await.unwrap();
        assert_eq!(h.actions.posts().len(), 1, "not yet a minute");
        h.clock.advance_ms(61_000);
        Sticky.on_tick(&h.ctx).await.unwrap();
        assert_eq!(h.actions.posts().len(), 2);
        // Quiet channel: nothing to chase.
        h.clock.advance_ms(120_000);
        Sticky.on_tick(&h.ctx).await.unwrap();
        assert_eq!(h.actions.posts().len(), 2);
    }

    #[tokio::test]
    async fn editing_the_setting_replaces_the_live_post() {
        let h = harness(json!({"channel": "10", "content": "old", "every_messages": 50})).await;
        talk(&h, 1).await;
        h.configure(
            STICKY_ID,
            json!({"stickies": [{"channel": "10", "content": "new", "every_messages": 50}]}),
        )
        .await;
        talk(&h, 1).await;
        let posts = h.actions.posts();
        assert_eq!(posts.last().unwrap().1.content, "new");
    }

    #[test]
    fn schema_defaults_validate() {
        let s = Sticky.settings_schema();
        s.validate(&s.defaults(Scope::Guild)).unwrap();
    }
}
