//! FTR-SLOW-001 Slowmode schedules: raise a channel's slowmode during recurring
//! time windows (a nightly event, a weekend raid hour) and put it back afterwards.
//!
//! Rules are evaluated on every tick. While a rule is active its channels are held at
//! the rule's value; the value the channel had before the first rule touched it is
//! remembered and restored when no rule is active any more. If several rules cover a
//! channel at once, the later rule in the list wins.

use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::json;
use sr_events::{ChannelId, GuildId};

use crate::feature::{Feature, FeatureCtx, FeatureError, FeatureMeta, Scope};
use crate::schema::{EnumOption, Field, FieldKind, SettingsSchema};
use crate::state::StateEntry;

use super::snowflakes;

pub const SLOWMODE_ID: &str = "lounge.slowmode_plus";

/// Discord's maximum slowmode.
const MAX_SECONDS: u32 = 21_600;

#[derive(Default)]
pub struct SlowmodePlus;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Config {
    rules: Vec<Rule>,
    presets: Vec<Preset>,
}

/// A named slowmode value that staff apply on demand (G-74).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(default)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub seconds: u32,
}

impl Default for Preset {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            seconds: 10,
        }
    }
}

/// Presets offered while a server has configured none.
pub fn default_presets() -> Vec<Preset> {
    [
        ("light", "Light (5 seconds)", 5),
        ("medium", "Medium (30 seconds)", 30),
        ("strict", "Strict (2 minutes)", 120),
        ("lockdown", "Lockdown (10 minutes)", 600),
    ]
    .into_iter()
    .map(|(id, name, seconds)| Preset {
        id: id.into(),
        name: name.into(),
        seconds,
    })
    .collect()
}

/// What one on-demand application did to a channel.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Applied {
    pub channel: ChannelId,
    /// The slowmode the channel had before (what a revert restores).
    pub before: u32,
    pub seconds: u32,
    /// When the slowmode goes back, if a duration was given.
    pub revert_at_ms: Option<i64>,
}

/// Longest on-demand duration: one week.
pub const MAX_APPLY_MINUTES: u32 = 7 * 24 * 60;

fn revert_key(channel: u64) -> String {
    format!("revert:{channel}")
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Rule {
    name: String,
    channels: Vec<String>,
    seconds: u32,
    /// Three-letter weekdays (`mon`..`sun`); empty means every day.
    days: Vec<String>,
    start_hour: u32,
    start_minute: u32,
    end_hour: u32,
    end_minute: u32,
    /// Minutes east of UTC, so the window is in the community's own time.
    utc_offset_minutes: i32,
}

impl Default for Rule {
    fn default() -> Self {
        Self {
            name: String::new(),
            channels: Vec::new(),
            seconds: 10,
            days: Vec::new(),
            start_hour: 20,
            start_minute: 0,
            end_hour: 22,
            end_minute: 0,
            utc_offset_minutes: 0,
        }
    }
}

const DAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

impl Rule {
    /// Whether the window is open at `now_ms`. A window whose end is before its start
    /// crosses midnight and belongs to the weekday it starts on. Equal start and end
    /// means the whole day.
    pub fn active_at(&self, now_ms: i64) -> bool {
        let local = now_ms + i64::from(self.utc_offset_minutes) * 60_000;
        let day = local.div_euclid(86_400_000);
        let minute = (local.rem_euclid(86_400_000) / 60_000) as u32;
        // 1970-01-01 was a Thursday (index 3 with Monday first).
        let weekday = |d: i64| (d + 3).rem_euclid(7) as usize;
        let on_day =
            |d: i64| self.days.is_empty() || self.days.iter().any(|x| x == DAYS[weekday(d)]);
        let start = self.start_hour * 60 + self.start_minute;
        let end = self.end_hour * 60 + self.end_minute;
        if start == end {
            on_day(day)
        } else if start < end {
            minute >= start && minute < end && on_day(day)
        } else if minute >= start {
            on_day(day)
        } else if minute < end {
            on_day(day - 1)
        } else {
            false
        }
    }
}

fn applied_key(channel: u64) -> String {
    format!("applied:{channel}")
}

impl SlowmodePlus {
    /// The presets of a server: its own list, or the built-in ones when it set none.
    pub async fn presets(ctx: &FeatureCtx, guild: GuildId) -> Result<Vec<Preset>, FeatureError> {
        let cfg: Config = ctx.guild_config(SLOWMODE_ID, guild).await?;
        let own: Vec<Preset> = cfg
            .presets
            .into_iter()
            .filter(|p| !p.id.trim().is_empty())
            .collect();
        Ok(if own.is_empty() {
            default_presets()
        } else {
            own
        })
    }

    /// Apply preset `preset_id` to `channels` now (G-74). With `minutes` the previous value is
    /// put back by the module's tick once the time is up, unless someone changed the channel
    /// meanwhile. The caller has checked the permission to manage every channel. Writes go through
    /// the guild backend as this module (D-187), so the audit log reads like a manual change.
    pub async fn apply_preset(
        ctx: &FeatureCtx,
        guild: GuildId,
        preset_id: &str,
        channels: &[ChannelId],
        minutes: Option<u32>,
    ) -> Result<Vec<Applied>, FeatureError> {
        let presets = Self::presets(ctx, guild).await?;
        let preset = presets.iter().find(|p| p.id == preset_id).ok_or_else(|| {
            FeatureError::Validation(crate::schema::ValidationError {
                path: "preset_id".into(),
                reason: "unknown preset".into(),
            })
        })?;
        let seconds = preset.seconds.min(MAX_SECONDS);
        let now = ctx.now_ms();
        let revert_at = minutes.map(|m| now + i64::from(m.min(MAX_APPLY_MINUTES)) * 60_000);
        let mut out = Vec::new();
        for channel in channels {
            let Some(info) = ctx.actions.get_channel(*channel).await? else {
                continue;
            };
            let key = revert_key(channel.get());
            // A channel that is already on a timed preset keeps the value from before the first one.
            let before = match ctx.state.get(SLOWMODE_ID, guild, &key).await? {
                Some(e) => e
                    .value
                    .get("before")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|v| u32::try_from(v).ok())
                    .unwrap_or(info.rate_limit_per_user),
                None => info.rate_limit_per_user,
            };
            if info.rate_limit_per_user != seconds {
                ctx.actions.set_slowmode(*channel, seconds).await?;
            }
            match revert_at {
                Some(at) => {
                    ctx.state
                        .put(
                            StateEntry::new(
                                SLOWMODE_ID,
                                guild,
                                key,
                                json!({"before": before, "seconds": seconds}),
                            )
                            .due(at),
                        )
                        .await?;
                }
                None => ctx.state.delete(SLOWMODE_ID, guild, &key).await?,
            }
            out.push(Applied {
                channel: *channel,
                before,
                seconds,
                revert_at_ms: revert_at,
            });
        }
        Ok(out)
    }

    /// Put back the channels whose timed preset ran out.
    async fn run_reverts(&self, ctx: &FeatureCtx) -> Result<(), FeatureError> {
        for entry in ctx.state.due(SLOWMODE_ID, ctx.now_ms(), 100).await? {
            let Some(ch) = entry
                .key
                .strip_prefix("revert:")
                .and_then(|s| s.parse::<u64>().ok())
            else {
                continue;
            };
            let channel = ChannelId::new(ch);
            let read = |name: &str| {
                entry
                    .value
                    .get(name)
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|v| u32::try_from(v).ok())
            };
            let before = read("before").unwrap_or(0);
            let applied = read("seconds");
            if let Some(info) = ctx.actions.get_channel(channel).await?
                && Some(info.rate_limit_per_user) == applied
                && info.rate_limit_per_user != before
            {
                // A schedule that holds the channel right now wins; its own restore runs later.
                let held = ctx
                    .state
                    .get(SLOWMODE_ID, entry.guild, &applied_key(ch))
                    .await?
                    .is_some();
                if !held {
                    ctx.actions.set_slowmode(channel, before).await?;
                }
            }
            ctx.state
                .delete(SLOWMODE_ID, entry.guild, &entry.key)
                .await?;
        }
        Ok(())
    }

    async fn sweep(&self, ctx: &FeatureCtx, guild: GuildId) -> Result<(), FeatureError> {
        let cfg: Config = ctx.guild_config(SLOWMODE_ID, guild).await?;
        let now = ctx.now_ms();

        // Desired value per channel from the currently active rules (later wins).
        let mut desired: Vec<(u64, u32)> = Vec::new();
        for rule in cfg.rules.iter().filter(|r| r.active_at(now)) {
            for ch in snowflakes(&rule.channels) {
                let secs = rule.seconds.min(MAX_SECONDS);
                match desired.iter_mut().find(|(c, _)| *c == ch) {
                    Some(slot) => slot.1 = secs,
                    None => desired.push((ch, secs)),
                }
            }
        }

        for (ch, secs) in &desired {
            let channel = ChannelId::new(*ch);
            let Some(info) = ctx.actions.get_channel(channel).await? else {
                continue;
            };
            let key = applied_key(*ch);
            if ctx.state.get(SLOWMODE_ID, guild, &key).await?.is_none() {
                ctx.state
                    .put(StateEntry::new(
                        SLOWMODE_ID,
                        guild,
                        key,
                        json!({"before": info.rate_limit_per_user}),
                    ))
                    .await?;
            }
            if info.rate_limit_per_user != *secs {
                ctx.actions.set_slowmode(channel, *secs).await?;
            }
        }

        // Restore channels whose rules are no longer active.
        for entry in ctx.state.list(SLOWMODE_ID, guild, "applied:").await? {
            let Some(ch) = entry
                .key
                .strip_prefix("applied:")
                .and_then(|s| s.parse::<u64>().ok())
            else {
                continue;
            };
            if desired.iter().any(|(c, _)| *c == ch) {
                continue;
            }
            let before = entry
                .value
                .get("before")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let channel = ChannelId::new(ch);
            if let Some(info) = ctx.actions.get_channel(channel).await?
                && info.rate_limit_per_user != before
            {
                ctx.actions.set_slowmode(channel, before).await?;
            }
            ctx.state.delete(SLOWMODE_ID, guild, &entry.key).await?;
        }
        Ok(())
    }
}

impl Feature for SlowmodePlus {
    fn id(&self) -> &'static str {
        SLOWMODE_ID
    }

    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Slowmode schedules".into(),
            description: "Raise slowmode in chosen channels during recurring time windows and \
                          restore it afterwards."
                .into(),
            section: "Moderation".into(),
            phase: 11,
            default_enabled: false,
        }
    }

    fn settings_schema(&self) -> SettingsSchema {
        let day = |d: &str, l: &str| EnumOption {
            value: d.into(),
            label: l.into(),
        };
        let hour = |key: &str, label: &str, default: u32| {
            Field::new(
                key,
                label,
                FieldKind::Int {
                    min: Some(0),
                    max: Some(23),
                },
            )
            .default_value(json!(default))
        };
        let minute = |key: &str, label: &str| {
            Field::new(
                key,
                label,
                FieldKind::Int {
                    min: Some(0),
                    max: Some(59),
                },
            )
            .default_value(json!(0))
        };
        let presets = Field::new(
            "presets",
            "Presets",
            FieldKind::List {
                max_items: Some(12),
                item: Box::new(FieldKind::Group {
                    fields: vec![
                        Field::new("id", "Id", FieldKind::String { max_len: Some(32) }),
                        Field::new("name", "Name", FieldKind::String { max_len: Some(60) }),
                        Field::new(
                            "seconds",
                            "Slowmode (seconds)",
                            FieldKind::Int {
                                min: Some(0),
                                max: Some(i64::from(MAX_SECONDS)),
                            },
                        )
                        .default_value(json!(10)),
                    ],
                }),
            },
        )
        .help(
            "Named values staff apply on demand from the channel menu. Leave empty for the built-in four.",
        );
        SettingsSchema::new(vec![
            presets,
            Field::new(
                "rules",
                "Schedules",
                FieldKind::List {
                    max_items: Some(25),
                    item: Box::new(FieldKind::Group {
                        fields: vec![
                            Field::new("name", "Name", FieldKind::String { max_len: Some(60) }),
                            Field::new(
                                "channels",
                                "Channels",
                                FieldKind::ChannelPicker { multi: true },
                            ),
                            Field::new(
                                "seconds",
                                "Slowmode (seconds)",
                                FieldKind::Int {
                                    min: Some(0),
                                    max: Some(i64::from(MAX_SECONDS)),
                                },
                            )
                            .default_value(json!(10)),
                            Field::new(
                                "days",
                                "Days",
                                FieldKind::List {
                                    max_items: Some(7),
                                    item: Box::new(FieldKind::Enum {
                                        options: vec![
                                            day("mon", "Monday"),
                                            day("tue", "Tuesday"),
                                            day("wed", "Wednesday"),
                                            day("thu", "Thursday"),
                                            day("fri", "Friday"),
                                            day("sat", "Saturday"),
                                            day("sun", "Sunday"),
                                        ],
                                    }),
                                },
                            )
                            .help("Leave empty for every day."),
                            hour("start_hour", "Start hour", 20),
                            minute("start_minute", "Start minute"),
                            hour("end_hour", "End hour", 22),
                            minute("end_minute", "End minute"),
                            Field::new(
                                "utc_offset_minutes",
                                "Time zone (minutes from UTC)",
                                FieldKind::Int {
                                    min: Some(-720),
                                    max: Some(840),
                                },
                            )
                            .default_value(json!(0)),
                        ],
                    }),
                },
            ),
        ])
    }

    fn scopes(&self) -> &[Scope] {
        &[Scope::Platform, Scope::Guild]
    }

    fn on_tick<'a>(&'a self, ctx: &'a FeatureCtx) -> BoxFuture<'a, Result<(), FeatureError>> {
        Box::pin(async move {
            if let Err(e) = self.run_reverts(ctx).await {
                tracing::warn!(error = %e, "slowmode revert failed");
            }
            for guild in ctx.settings.enabled_guilds(SLOWMODE_ID).await? {
                if let Err(e) = self.sweep(ctx, guild).await {
                    tracing::warn!(guild = guild.get(), error = %e, "slowmode sweep failed");
                }
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FeatureRegistry;
    use crate::actions::ChannelInfo;
    use crate::testing::{Call, Harness};

    const MIN: i64 = 60_000;
    const DAY: i64 = 86_400_000;
    // 1970-01-05 is a Monday.
    const MONDAY: i64 = 4 * DAY;

    fn rule(json: serde_json::Value) -> Rule {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn window_within_a_day() {
        let r = rule(json!({"start_hour": 20, "end_hour": 22}));
        assert!(!r.active_at(MONDAY + 19 * 60 * MIN + 59 * MIN));
        assert!(r.active_at(MONDAY + 20 * 60 * MIN));
        assert!(r.active_at(MONDAY + 21 * 60 * MIN + 59 * MIN));
        assert!(!r.active_at(MONDAY + 22 * 60 * MIN));
    }

    #[test]
    fn window_crossing_midnight_belongs_to_the_start_day() {
        let r = rule(json!({"start_hour": 22, "end_hour": 2, "days": ["fri"]}));
        let friday = MONDAY + 4 * DAY;
        assert!(r.active_at(friday + 23 * 60 * MIN));
        assert!(
            r.active_at(friday + DAY + 60 * MIN),
            "Saturday 01:00 still Friday's"
        );
        assert!(
            !r.active_at(friday + 60 * MIN),
            "Friday 01:00 is Thursday night's"
        );
        assert!(!r.active_at(MONDAY + 23 * 60 * MIN));
    }

    #[test]
    fn weekday_and_offset() {
        let r = rule(json!({"days": ["mon"], "start_hour": 0, "end_hour": 0}));
        assert!(r.active_at(MONDAY + 5 * 60 * MIN));
        assert!(!r.active_at(MONDAY + DAY + MIN));
        // UTC+2: Sunday 23:00 UTC is already Monday 01:00 local.
        let r = rule(
            json!({"days": ["mon"], "start_hour": 0, "end_hour": 0, "utc_offset_minutes": 120}),
        );
        assert!(r.active_at(MONDAY - 60 * MIN));
    }

    fn chan(id: u64, slow: u32) -> ChannelInfo {
        ChannelInfo {
            id: ChannelId::new(id),
            guild: Some(GuildId::new(1)),
            kind: 0,
            name: "general".into(),
            nsfw: false,
            parent: None,
            rate_limit_per_user: slow,
            everyone_can_view: true,
        }
    }

    #[tokio::test]
    async fn applies_during_the_window_and_restores_after() {
        let mut reg = FeatureRegistry::new();
        reg.register(SlowmodePlus).unwrap();
        let h = Harness::new(reg).await;
        h.actions.put_channel(chan(10, 5));
        h.configure(
            SLOWMODE_ID,
            json!({"rules": [{"channels": ["10"], "seconds": 30,
                "start_hour": 20, "end_hour": 22}]}),
        )
        .await;

        h.clock.set(MONDAY + 19 * 60 * MIN);
        SlowmodePlus.on_tick(&h.ctx).await.unwrap();
        assert!(h.actions.calls().is_empty(), "before the window");

        h.clock.set(MONDAY + 20 * 60 * MIN + MIN);
        SlowmodePlus.on_tick(&h.ctx).await.unwrap();
        SlowmodePlus.on_tick(&h.ctx).await.unwrap();
        assert_eq!(
            h.actions.calls(),
            vec![Call::Slowmode(ChannelId::new(10), 30)],
            "set once, not on every tick"
        );

        h.clock.set(MONDAY + 22 * 60 * MIN + MIN);
        SlowmodePlus.on_tick(&h.ctx).await.unwrap();
        assert_eq!(
            h.actions.calls().last(),
            Some(&Call::Slowmode(ChannelId::new(10), 5)),
            "back to what it was"
        );
        h.actions.clear_calls();
        SlowmodePlus.on_tick(&h.ctx).await.unwrap();
        assert!(h.actions.calls().is_empty());
    }

    #[tokio::test]
    async fn a_preset_applies_now_and_reverts_after_its_time() {
        let mut reg = FeatureRegistry::new();
        reg.register(SlowmodePlus).unwrap();
        let h = Harness::new(reg).await;
        h.actions.put_channel(chan(10, 5));
        h.actions.put_channel(chan(11, 0));
        let guild = GuildId::new(1);
        h.clock.set(MONDAY);

        // Built-in presets while the server set none; unknown ids are refused.
        assert!(
            SlowmodePlus::apply_preset(&h.ctx, guild, "nope", &[ChannelId::new(10)], None)
                .await
                .is_err()
        );
        let done = SlowmodePlus::apply_preset(
            &h.ctx,
            guild,
            "medium",
            &[ChannelId::new(10), ChannelId::new(11)],
            Some(10),
        )
        .await
        .unwrap();
        assert_eq!(done.len(), 2);
        assert_eq!((done[0].before, done[0].seconds), (5, 30));
        assert_eq!(done[0].revert_at_ms, Some(MONDAY + 10 * MIN));
        assert_eq!(h.actions.calls().len(), 2);

        // Not yet due.
        h.clock.set(MONDAY + 9 * MIN);
        SlowmodePlus.on_tick(&h.ctx).await.unwrap();
        assert_eq!(h.actions.calls().len(), 2);

        // Due: both go back, and a channel someone changed in between is left alone.
        h.actions.put_channel(chan(11, 60));
        h.clock.set(MONDAY + 11 * MIN);
        SlowmodePlus.on_tick(&h.ctx).await.unwrap();
        assert!(
            h.actions
                .calls()
                .contains(&Call::Slowmode(ChannelId::new(10), 5))
        );
        assert!(
            !h.actions
                .calls()
                .contains(&Call::Slowmode(ChannelId::new(11), 0))
        );
        let before = h.actions.calls().len();
        SlowmodePlus.on_tick(&h.ctx).await.unwrap();
        assert_eq!(h.actions.calls().len(), before, "the revert ran once");
    }

    #[tokio::test]
    async fn a_server_can_define_its_own_presets() {
        let mut reg = FeatureRegistry::new();
        reg.register(SlowmodePlus).unwrap();
        let h = Harness::new(reg).await;
        h.actions.put_channel(chan(10, 0));
        h.configure(
            SLOWMODE_ID,
            json!({"presets": [{"id": "raid", "name": "Raid", "seconds": 90}]}),
        )
        .await;
        let guild = GuildId::new(1);
        assert!(
            SlowmodePlus::apply_preset(&h.ctx, guild, "medium", &[ChannelId::new(10)], None)
                .await
                .is_err(),
            "own presets replace the built-in ones"
        );
        let done = SlowmodePlus::apply_preset(&h.ctx, guild, "raid", &[ChannelId::new(10)], None)
            .await
            .unwrap();
        assert_eq!(done[0].seconds, 90);
        assert_eq!(done[0].revert_at_ms, None);
    }

    #[test]
    fn schema_defaults_validate_and_a_rule_roundtrips() {
        let s = SlowmodePlus.settings_schema();
        s.validate(&s.defaults(Scope::Guild)).unwrap();
        let v = json!({"rules": [{"name": "Events", "channels": ["1"], "seconds": 60,
            "days": ["fri", "sat"], "start_hour": 18, "start_minute": 30,
            "end_hour": 23, "end_minute": 0, "utc_offset_minutes": -300}]});
        s.validate(v.as_object().unwrap()).unwrap();
        let bad = json!({"rules": [{"days": ["funday"]}]});
        assert!(s.validate(bad.as_object().unwrap()).is_err());
    }
}
