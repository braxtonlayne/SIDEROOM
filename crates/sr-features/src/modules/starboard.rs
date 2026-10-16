//! FTR-STAR-001 Starboard: messages that collect enough of one reaction are
//! reposted to a showcase channel, and the count follows the reactions.
//!
//! * The reaction counts distinct users; the author's own reaction counts only when
//!   `self_star` is on.
//! * A post disappears again when the count falls below the threshold or every such
//!   reaction is cleared, and when the source message is deleted. If a moderator
//!   deletes the starboard post by hand it is not reposted.
//! * Messages in NSFW channels follow `nsfw`: `skip`, `nsfw_target` (only when the
//!   starboard channel is itself age-restricted, the default) or `allow`.

use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::json;
use sr_events::{ChannelId, DomainEvent, GuildId};

use crate::actions::{ActionError, OutMessage};
use crate::feature::{Feature, FeatureCtx, FeatureError, FeatureMeta, Scope};
use crate::schema::{EnumOption, Field, FieldKind, SettingsSchema};
use crate::state::StateEntry;
use crate::template::clip;

use super::{emoji_display, event_emoji_key, normalize_emoji, snowflake, snowflakes};

pub const STARBOARD_ID: &str = "lounge.starboard";

const STAR_COLOR: u32 = 0xFF_AC_33;

#[derive(Default)]
pub struct Starboard;

#[derive(Debug, Deserialize)]
#[serde(default)]
struct Config {
    channel: Option<String>,
    emoji: String,
    threshold: u32,
    self_star: bool,
    nsfw: String,
    ignored_channels: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            channel: None,
            emoji: "\u{2b50}".into(),
            threshold: 3,
            self_star: false,
            nsfw: "nsfw_target".into(),
            ignored_channels: Vec::new(),
        }
    }
}

fn link_key(message: u64) -> String {
    format!("link:{message}")
}

impl Starboard {
    async fn handle(&self, ev: &DomainEvent, ctx: &FeatureCtx) -> Result<(), FeatureError> {
        match ev {
            DomainEvent::ReactionAdded {
                guild: Some(g),
                channel,
                message_id,
                emoji,
                ..
            }
            | DomainEvent::ReactionRemoved {
                guild: Some(g),
                channel,
                message_id,
                emoji,
                ..
            } => {
                let Some(key) = event_emoji_key(emoji) else {
                    return Ok(());
                };
                self.refresh(ctx, *g, *channel, *message_id, &key).await
            }
            DomainEvent::ReactionsCleared {
                guild: Some(g),
                channel,
                message_id,
            } => self.refresh_all(ctx, *g, *channel, *message_id).await,
            DomainEvent::ReactionEmojiCleared {
                guild: Some(g),
                channel,
                message_id,
                emoji,
            } => {
                let Some(key) = event_emoji_key(emoji) else {
                    return Ok(());
                };
                let cfg: Config = ctx.guild_config(STARBOARD_ID, *g).await?;
                if normalize_emoji(&cfg.emoji) == key {
                    self.refresh_all(ctx, *g, *channel, *message_id).await?;
                }
                Ok(())
            }
            DomainEvent::MessageDeleted {
                guild: Some(g),
                message_id,
                ..
            } => self.drop_post(ctx, *g, *message_id).await,
            _ => Ok(()),
        }
    }

    /// All reactions of a message are gone: the count is zero.
    async fn refresh_all(
        &self,
        ctx: &FeatureCtx,
        guild: GuildId,
        _channel: ChannelId,
        message: u64,
    ) -> Result<(), FeatureError> {
        self.drop_post(ctx, guild, message).await
    }

    async fn drop_post(
        &self,
        ctx: &FeatureCtx,
        guild: GuildId,
        message: u64,
    ) -> Result<(), FeatureError> {
        let Some(link) = ctx
            .state
            .get(STARBOARD_ID, guild, &link_key(message))
            .await?
        else {
            return Ok(());
        };
        if let (Some(ch), Some(post)) = (
            link.value.get("channel").and_then(|v| v.as_u64()),
            link.value.get("post").and_then(|v| v.as_u64()),
        ) {
            match ctx.actions.delete_message(ChannelId::new(ch), post).await {
                Ok(()) | Err(ActionError::NotFound) => {}
                Err(e) => return Err(e.into()),
            }
        }
        ctx.state
            .delete(STARBOARD_ID, guild, &link_key(message))
            .await?;
        Ok(())
    }

    async fn refresh(
        &self,
        ctx: &FeatureCtx,
        guild: GuildId,
        channel: ChannelId,
        message: u64,
        key: &str,
    ) -> Result<(), FeatureError> {
        let cfg: Config = ctx.guild_config(STARBOARD_ID, guild).await?;
        let Some(target) = cfg.channel.as_deref().and_then(snowflake) else {
            return Ok(());
        };
        let want = normalize_emoji(&cfg.emoji);
        if want.is_empty() || want != key {
            return Ok(());
        }
        if channel.get() == target || snowflakes(&cfg.ignored_channels).contains(&channel.get()) {
            return Ok(());
        }
        let target = ChannelId::new(target);
        let Some(msg) = ctx.actions.get_message(channel, message).await? else {
            return Ok(());
        };
        if msg.author == ctx.actions.self_user() {
            return Ok(());
        }
        let link = ctx
            .state
            .get(STARBOARD_ID, guild, &link_key(message))
            .await?;
        if link
            .as_ref()
            .is_some_and(|l| l.value.get("removed").and_then(|v| v.as_bool()) == Some(true))
        {
            return Ok(());
        }

        let exclude = (!cfg.self_star).then_some(msg.author);
        let count = ctx
            .actions
            .reaction_count(channel, message, &want, exclude)
            .await?;
        let emoji = emoji_display(&want);

        if count < cfg.threshold.max(1) {
            if link.is_some() {
                self.drop_post(ctx, guild, message).await?;
            }
            return Ok(());
        }

        let content = format!("{emoji} **{count}** | <#{}>", channel.get());
        if let Some(link) = link {
            let post = link.value.get("post").and_then(|v| v.as_u64());
            if let Some(post) = post {
                match ctx.actions.edit_message(target, post, content).await {
                    Ok(()) => {}
                    Err(ActionError::NotFound) => {
                        // A moderator removed the post: leave it removed.
                        ctx.state
                            .put(StateEntry::new(
                                STARBOARD_ID,
                                guild,
                                link_key(message),
                                json!({"removed": true}),
                            ))
                            .await?;
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            return Ok(());
        }

        // First time over the threshold: respect the NSFW policy, then post.
        let source = ctx.actions.get_channel(channel).await?;
        let src_nsfw = source.as_ref().is_some_and(|c| c.nsfw);
        if src_nsfw {
            let target_nsfw = ctx
                .actions
                .get_channel(target)
                .await?
                .is_some_and(|c| c.nsfw);
            let allowed = match cfg.nsfw.as_str() {
                "allow" => true,
                "skip" => false,
                _ => target_nsfw,
            };
            if !allowed {
                return Ok(());
            }
        }
        let author = ctx
            .actions
            .get_user(msg.author)
            .await?
            .map(|u| u.name)
            .unwrap_or_else(|| "Unknown".into());
        let jump = format!(
            "{}/channels/{}/{}/{}",
            ctx.actions.base_url(),
            guild.get(),
            channel.get(),
            message
        );
        let mut embed = json!({
            "type": "rich",
            "color": STAR_COLOR,
            "author": {"name": crate::template::defuse(&author)},
            "description": clip(&msg.content, 2000),
            "fields": [{"name": "Source", "value": format!("[Jump to message]({jump})")}],
        });
        if let Some(img) = msg.attachments.iter().find(|a| {
            a.content_type
                .as_deref()
                .is_some_and(|t| t.starts_with("image/"))
        }) {
            embed["image"] = json!({"url": img.url});
        }
        let post = ctx
            .actions
            .post_message(
                target,
                OutMessage {
                    content,
                    embed: Some(embed),
                    mention_users: Vec::new(),
                },
            )
            .await?;
        ctx.state
            .put(StateEntry::new(
                STARBOARD_ID,
                guild,
                link_key(message),
                json!({"channel": target.get(), "post": post}),
            ))
            .await?;
        Ok(())
    }
}

impl Feature for Starboard {
    fn id(&self) -> &'static str {
        STARBOARD_ID
    }

    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Starboard".into(),
            description: "Repost messages that collect enough of one reaction into a showcase \
                          channel, with a live count."
                .into(),
            section: "Lounge".into(),
            phase: 11,
            default_enabled: false,
        }
    }

    fn settings_schema(&self) -> SettingsSchema {
        let opt = |v: &str, l: &str| EnumOption {
            value: v.into(),
            label: l.into(),
        };
        SettingsSchema::new(vec![
            Field::new(
                "channel",
                "Starboard channel",
                FieldKind::ChannelPicker { multi: false },
            )
            .help("Where starred messages are posted. Nothing happens until this is set."),
            Field::new("emoji", "Emoji", FieldKind::String { max_len: Some(64) })
                .help("A unicode emoji, or name:id for a custom emoji.")
                .default_value(json!("\u{2b50}")),
            Field::new(
                "threshold",
                "Reactions needed",
                FieldKind::Int {
                    min: Some(1),
                    max: Some(100),
                },
            )
            .default_value(json!(3)),
            Field::new(
                "self_star",
                "Count the author's own reaction",
                FieldKind::Bool,
            )
            .default_value(json!(false)),
            Field::new(
                "nsfw",
                "Age-restricted channels",
                FieldKind::Enum {
                    options: vec![
                        opt("skip", "Never post them"),
                        opt(
                            "nsfw_target",
                            "Only if the starboard channel is age-restricted",
                        ),
                        opt("allow", "Always post them"),
                    ],
                },
            )
            .default_value(json!("nsfw_target")),
            Field::new(
                "ignored_channels",
                "Ignored channels",
                FieldKind::ChannelPicker { multi: true },
            ),
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
    use serde_json::json;
    use sr_events::UserId;

    use super::*;
    use crate::FeatureRegistry;
    use crate::actions::{AttachmentInfo, ChannelInfo, FeatureActions, MessageInfo};
    use crate::testing::{Call, Harness};

    const G: GuildId = GuildId::new(1);
    const SRC: ChannelId = ChannelId::new(10);
    const BOARD: ChannelId = ChannelId::new(20);
    const MSG: u64 = 500;
    const AUTHOR: UserId = UserId::new(7);

    fn channel(id: u64, nsfw: bool) -> ChannelInfo {
        ChannelInfo {
            id: ChannelId::new(id),
            guild: Some(G),
            kind: 0,
            name: format!("c{id}"),
            nsfw,
            parent: None,
            rate_limit_per_user: 0,
            everyone_can_view: true,
        }
    }

    async fn harness() -> Harness {
        let mut reg = FeatureRegistry::new();
        reg.register(Starboard).unwrap();
        let h = Harness::new(reg).await;
        h.configure(
            STARBOARD_ID,
            json!({"channel": "20", "threshold": 2, "emoji": "\u{2b50}"}),
        )
        .await;
        h.actions.put_user(7, "Ada");
        h.actions.put_channel(channel(10, false));
        h.actions.put_channel(channel(20, false));
        h.actions.put_message(MessageInfo {
            id: MSG,
            channel: SRC,
            guild: Some(G),
            author: AUTHOR,
            content: "hello world".into(),
            embeds: vec![],
            attachments: vec![AttachmentInfo {
                url: "https://x/y.png".into(),
                filename: "y.png".into(),
                content_type: Some("image/png".into()),
            }],
        });
        h
    }

    fn added(user: u64) -> DomainEvent {
        DomainEvent::ReactionAdded {
            guild: Some(G),
            channel: SRC,
            message_id: MSG,
            user: UserId::new(user),
            emoji: json!({"id": null, "name": "\u{2b50}"}),
        }
    }

    fn removed(user: u64) -> DomainEvent {
        DomainEvent::ReactionRemoved {
            guild: Some(G),
            channel: SRC,
            message_id: MSG,
            user: UserId::new(user),
            emoji: json!({"id": null, "name": "\u{2b50}"}),
        }
    }

    async fn fire(h: &Harness, ev: DomainEvent) {
        Starboard.on_event(&ev, &h.ctx).await.unwrap();
    }

    #[tokio::test]
    async fn posts_at_threshold_then_updates_the_count() {
        let h = harness().await;
        h.actions.react(MSG, "\u{2b50}", 1);
        fire(&h, added(1)).await;
        assert!(h.actions.posts().is_empty(), "below threshold");

        h.actions.react(MSG, "\u{2b50}", 2);
        fire(&h, added(2)).await;
        let posts = h.actions.posts();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].0, BOARD);
        assert!(posts[0].1.content.contains("**2**"));
        let embed = posts[0].1.embed.clone().unwrap();
        assert_eq!(embed["description"], "hello world");
        assert_eq!(embed["image"]["url"], "https://x/y.png");

        h.actions.react(MSG, "\u{2b50}", 3);
        fire(&h, added(3)).await;
        assert_eq!(h.actions.posts().len(), 1, "edited, not reposted");
        let edit = h
            .actions
            .calls()
            .into_iter()
            .find_map(|c| match c {
                Call::Edit(_, _, text) => Some(text),
                _ => None,
            })
            .unwrap();
        assert!(edit.contains("**3**"));
    }

    #[tokio::test]
    async fn post_is_removed_when_the_count_drops() {
        let h = harness().await;
        h.actions.react(MSG, "\u{2b50}", 1);
        h.actions.react(MSG, "\u{2b50}", 2);
        fire(&h, added(2)).await;
        assert_eq!(h.actions.posts().len(), 1);
        h.actions.unreact(MSG, "\u{2b50}", 2);
        fire(&h, removed(2)).await;
        assert!(
            h.actions
                .calls()
                .iter()
                .any(|c| matches!(c, Call::Delete(ch, _) if *ch == BOARD))
        );
        // Back over the threshold posts again.
        h.actions.react(MSG, "\u{2b50}", 2);
        fire(&h, added(2)).await;
        assert_eq!(h.actions.posts().len(), 2);
    }

    #[tokio::test]
    async fn authors_own_star_is_ignored_unless_allowed() {
        let h = harness().await;
        h.actions.react(MSG, "\u{2b50}", AUTHOR.get());
        h.actions.react(MSG, "\u{2b50}", 2);
        fire(&h, added(2)).await;
        assert!(h.actions.posts().is_empty(), "only one non-author star");

        h.configure(
            STARBOARD_ID,
            json!({"channel": "20", "threshold": 2, "self_star": true}),
        )
        .await;
        fire(&h, added(2)).await;
        assert_eq!(h.actions.posts().len(), 1);
    }

    #[tokio::test]
    async fn other_emoji_and_the_board_channel_are_ignored() {
        let h = harness().await;
        h.actions.react(MSG, "\u{2b50}", 1);
        h.actions.react(MSG, "\u{2b50}", 2);
        fire(
            &h,
            DomainEvent::ReactionAdded {
                guild: Some(G),
                channel: SRC,
                message_id: MSG,
                user: UserId::new(2),
                emoji: json!({"id": null, "name": "\u{1f44d}"}),
            },
        )
        .await;
        assert!(h.actions.posts().is_empty());
        fire(
            &h,
            DomainEvent::ReactionAdded {
                guild: Some(G),
                channel: BOARD,
                message_id: MSG,
                user: UserId::new(2),
                emoji: json!({"id": null, "name": "\u{2b50}"}),
            },
        )
        .await;
        assert!(h.actions.posts().is_empty());
    }

    #[tokio::test]
    async fn nsfw_sources_follow_the_policy() {
        let h = harness().await;
        h.actions.put_channel(channel(10, true));
        h.actions.react(MSG, "\u{2b50}", 1);
        h.actions.react(MSG, "\u{2b50}", 2);
        fire(&h, added(2)).await;
        assert!(h.actions.posts().is_empty(), "sfw starboard skips nsfw");

        h.actions.put_channel(channel(20, true));
        fire(&h, added(2)).await;
        assert_eq!(h.actions.posts().len(), 1, "nsfw starboard accepts it");
    }

    #[tokio::test]
    async fn a_moderator_deleted_post_is_not_reposted() {
        let h = harness().await;
        h.actions.react(MSG, "\u{2b50}", 1);
        h.actions.react(MSG, "\u{2b50}", 2);
        fire(&h, added(2)).await;
        let post = h
            .ctx
            .state
            .get(STARBOARD_ID, G, &link_key(MSG))
            .await
            .unwrap()
            .unwrap()
            .value["post"]
            .as_u64()
            .unwrap();
        // The moderator deletes it; the next reaction finds it gone.
        h.actions.delete_message(BOARD, post).await.unwrap();
        h.actions.react(MSG, "\u{2b50}", 3);
        fire(&h, added(3)).await;
        h.actions.react(MSG, "\u{2b50}", 4);
        fire(&h, added(4)).await;
        assert_eq!(h.actions.posts().len(), 1);
    }

    #[tokio::test]
    async fn deleting_the_source_deletes_the_post() {
        let h = harness().await;
        h.actions.react(MSG, "\u{2b50}", 1);
        h.actions.react(MSG, "\u{2b50}", 2);
        fire(&h, added(2)).await;
        fire(
            &h,
            DomainEvent::MessageDeleted {
                guild: Some(G),
                channel: SRC,
                message_id: MSG,
            },
        )
        .await;
        assert!(
            h.actions
                .calls()
                .iter()
                .any(|c| matches!(c, Call::Delete(ch, _) if *ch == BOARD))
        );
    }

    #[tokio::test]
    async fn without_a_channel_nothing_happens() {
        let mut reg = FeatureRegistry::new();
        reg.register(Starboard).unwrap();
        let h = Harness::new(reg).await;
        h.actions.react(MSG, "\u{2b50}", 1);
        fire(&h, added(1)).await;
        assert!(h.actions.calls().is_empty());
    }

    #[test]
    fn schema_validates_defaults() {
        let s = Starboard.settings_schema();
        s.validate(&s.defaults(Scope::Guild)).unwrap();
    }
}
