//! FTR-COUNT-001 Stat channels: channels whose name shows a live number, such as
//! `Members: 1,204` on a voice channel nobody joins.
//!
//! Placeholders: `{members}` and `{server}`. Names are refreshed when someone joins or
//! leaves and on every tick, but a channel is renamed at most once every
//! [`MIN_INTERVAL_MS`] (Discord allows two renames per ten minutes), so a burst of
//! joins settles on the final number at the next tick.

use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::json;
use sr_events::{ChannelId, DomainEvent, GuildId};

use crate::feature::{Feature, FeatureCtx, FeatureError, FeatureMeta, Scope};
use crate::schema::{Field, FieldKind, SettingsSchema};
use crate::state::StateEntry;
use crate::template::{clip, render};

use super::snowflake;

pub const STAT_CHANNELS_ID: &str = "lounge.stat_channels";

/// Shortest time between renames of one channel.
pub const MIN_INTERVAL_MS: i64 = 5 * 60_000;

#[derive(Default)]
pub struct StatChannels;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Config {
    channels: Vec<Stat>,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct Stat {
    channel: String,
    template: String,
}

impl Default for Stat {
    fn default() -> Self {
        Self {
            channel: String::new(),
            template: "Members: {members}".into(),
        }
    }
}

impl StatChannels {
    async fn refresh(&self, ctx: &FeatureCtx, guild: GuildId) -> Result<(), FeatureError> {
        let cfg: Config = ctx.guild_config(STAT_CHANNELS_ID, guild).await?;
        if cfg.channels.is_empty() {
            return Ok(());
        }
        let Some(info) = ctx.actions.get_guild(guild).await? else {
            return Ok(());
        };
        let vars = [
            ("members", info.member_count.to_string()),
            ("server", info.name.clone()),
        ];
        let now = ctx.now_ms();
        for stat in &cfg.channels {
            let Some(ch) = snowflake(&stat.channel) else {
                continue;
            };
            let name = clip(render(&stat.template, &vars).trim(), 100);
            if name.is_empty() {
                continue;
            }
            let key = format!("chan:{ch}");
            let last = ctx.state.get(STAT_CHANNELS_ID, guild, &key).await?;
            if let Some(last) = &last {
                let same = last.value.get("name").and_then(|v| v.as_str()) == Some(name.as_str());
                let at = last
                    .value
                    .get("at_ms")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                if same || now - at < MIN_INTERVAL_MS {
                    continue;
                }
            }
            ctx.actions
                .set_channel_name(ChannelId::new(ch), &name)
                .await?;
            ctx.state
                .put(StateEntry::new(
                    STAT_CHANNELS_ID,
                    guild,
                    key,
                    json!({"name": name, "at_ms": now}),
                ))
                .await?;
        }
        Ok(())
    }
}

impl Feature for StatChannels {
    fn id(&self) -> &'static str {
        STAT_CHANNELS_ID
    }

    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Stat channels".into(),
            description: "Show the member count in the name of a channel.".into(),
            section: "Lounge".into(),
            phase: 11,
            default_enabled: false,
        }
    }

    fn settings_schema(&self) -> SettingsSchema {
        SettingsSchema::new(vec![Field::new(
            "channels",
            "Stat channels",
            FieldKind::List {
                max_items: Some(10),
                item: Box::new(FieldKind::Group {
                    fields: vec![
                        Field::new(
                            "channel",
                            "Channel",
                            FieldKind::ChannelPicker { multi: false },
                        )
                        .help("Usually a voice channel with the Connect permission removed."),
                        Field::new("template", "Name", FieldKind::String { max_len: Some(100) })
                            .help("Placeholders: {members} {server}")
                            .default_value(json!("Members: {members}")),
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
            match ev {
                DomainEvent::MemberJoined { guild, .. } | DomainEvent::MemberLeft { guild, .. } => {
                    self.refresh(ctx, *guild).await
                }
                _ => Ok(()),
            }
        })
    }

    fn on_tick<'a>(&'a self, ctx: &'a FeatureCtx) -> BoxFuture<'a, Result<(), FeatureError>> {
        Box::pin(async move {
            for guild in ctx.settings.enabled_guilds(STAT_CHANNELS_ID).await? {
                if let Err(e) = self.refresh(ctx, guild).await {
                    tracing::warn!(guild = guild.get(), error = %e, "stat channel refresh failed");
                }
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
    use crate::actions::ChannelInfo;
    use crate::testing::{Call, Harness};

    const G: GuildId = GuildId::new(1);
    const CH: ChannelId = ChannelId::new(10);

    async fn harness() -> Harness {
        let mut reg = FeatureRegistry::new();
        reg.register(StatChannels).unwrap();
        let h = Harness::new(reg).await;
        h.actions.put_guild(1, "Club", 100);
        h.actions.put_channel(ChannelInfo {
            id: CH,
            guild: Some(G),
            kind: 2,
            name: "old".into(),
            nsfw: false,
            parent: None,
            rate_limit_per_user: 0,
            everyone_can_view: true,
        });
        h.configure(
            STAT_CHANNELS_ID,
            json!({"channels": [{"channel": "10", "template": "{server}: {members}"}]}),
        )
        .await;
        h
    }

    fn joined() -> DomainEvent {
        DomainEvent::MemberJoined {
            guild: G,
            user: UserId::new(5),
            payload: json!({}),
        }
    }

    #[tokio::test]
    async fn renames_then_throttles_and_catches_up_on_the_tick() {
        let h = harness().await;
        StatChannels.on_event(&joined(), &h.ctx).await.unwrap();
        assert_eq!(
            h.actions.calls(),
            vec![Call::Rename(CH, "Club: 100".into())]
        );

        // More joins inside the throttle window do not rename again.
        h.actions.put_guild(1, "Club", 103);
        h.clock.advance_ms(60_000);
        StatChannels.on_event(&joined(), &h.ctx).await.unwrap();
        assert_eq!(h.actions.calls().len(), 1);

        // The tick after the window settles on the latest number.
        h.clock.advance_ms(MIN_INTERVAL_MS);
        StatChannels.on_tick(&h.ctx).await.unwrap();
        assert_eq!(
            h.actions.calls().last(),
            Some(&Call::Rename(CH, "Club: 103".into()))
        );

        // An unchanged number is never re-sent.
        h.clock.advance_ms(MIN_INTERVAL_MS * 2);
        StatChannels.on_tick(&h.ctx).await.unwrap();
        assert_eq!(h.actions.calls().len(), 2);
    }

    #[test]
    fn schema_defaults_validate() {
        let s = StatChannels.settings_schema();
        s.validate(&s.defaults(Scope::Guild)).unwrap();
    }
}
