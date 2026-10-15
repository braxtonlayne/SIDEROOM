//! FTR-LOG-001 Mod log: a channel that records what happened in the server.
//!
//! Logged on request: message edits and deletes, joins, leaves, bans and unbans, role
//! changes (created, renamed, deleted), member updates (nickname, roles) and channel
//! creation and deletion.
//!
//! Privacy: the log never carries message text unless `include_content` is on, and even
//! then only for channels every member can read, never for private channels. Deleted
//! messages are logged by id only (the text is not kept). Channels can be excluded,
//! and the log channel itself is never logged, so it cannot loop.

use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::{Value, json};
use sr_events::{ChannelId, DomainEvent, GuildId, UserId};

use crate::actions::OutMessage;
use crate::feature::{Feature, FeatureCtx, FeatureError, FeatureMeta, Scope};
use crate::schema::{EnumOption, Field, FieldKind, SettingsSchema};
use crate::state::StateEntry;
use crate::template::{clip, defuse};

use super::{snowflake, snowflakes};

pub const MOD_LOG_ID: &str = "moderation.mod_log";

const KINDS: [(&str, &str); 8] = [
    ("message_edits", "Message edits"),
    ("message_deletes", "Message deletes"),
    ("joins", "Members joining"),
    ("leaves", "Members leaving"),
    ("bans", "Bans and unbans"),
    ("roles", "Role changes"),
    ("member_updates", "Nickname and role changes of members"),
    ("channels", "Channels created or deleted"),
];

const GREEN: u32 = 0x2ecc71;
const RED: u32 = 0xe74c3c;
const BLUE: u32 = 0x3498db;
const AMBER: u32 = 0xf1c40f;

#[derive(Default)]
pub struct ModLog;

#[derive(Debug, Deserialize)]
#[serde(default)]
struct Config {
    channel: Option<String>,
    events: Vec<String>,
    include_content: bool,
    ignored_channels: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            channel: None,
            events: KINDS.iter().map(|(k, _)| (*k).to_owned()).collect(),
            include_content: false,
            ignored_channels: Vec::new(),
        }
    }
}

struct Entry {
    title: String,
    description: String,
    color: u32,
}

fn s<'a>(v: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut cur = v;
    for p in path {
        cur = cur.get(*p)?;
    }
    cur.as_str()
}

fn id_list(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn role_mentions(ids: &[String]) -> String {
    if ids.is_empty() {
        "none".into()
    } else {
        ids.iter()
            .map(|i| format!("<@&{i}>"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl ModLog {
    async fn entry(
        &self,
        ctx: &FeatureCtx,
        guild: GuildId,
        cfg: &Config,
        log_channel: u64,
        ev: &DomainEvent,
    ) -> Result<Option<Entry>, FeatureError> {
        let on = |k: &str| cfg.events.iter().any(|e| e == k);
        let ignored = snowflakes(&cfg.ignored_channels);
        let skip_channel = |c: &ChannelId| c.get() == log_channel || ignored.contains(&c.get());
        let self_user = ctx.actions.self_user();
        Ok(match ev {
            DomainEvent::MemberJoined { user, .. } if on("joins") => Some(Entry {
                title: "Member joined".into(),
                description: format!("<@{0}> ({0})", user.get()),
                color: GREEN,
            }),
            DomainEvent::MemberLeft { user, .. } if on("leaves") => Some(Entry {
                title: "Member left".into(),
                description: format!("<@{0}> ({0})", user.get()),
                color: AMBER,
            }),
            DomainEvent::MemberBanned { user, .. } if on("bans") => Some(Entry {
                title: "Member banned".into(),
                description: format!("<@{0}> ({0})", user.get()),
                color: RED,
            }),
            DomainEvent::MemberUnbanned { user, .. } if on("bans") => Some(Entry {
                title: "Member unbanned".into(),
                description: format!("<@{0}> ({0})", user.get()),
                color: BLUE,
            }),
            DomainEvent::MessageDeleted {
                channel,
                message_id,
                ..
            } if on("message_deletes") && !skip_channel(channel) => Some(Entry {
                title: "Message deleted".into(),
                description: format!("Message {message_id} in <#{}>", channel.get()),
                color: RED,
            }),
            DomainEvent::MessagesBulkDeleted {
                channel,
                message_ids,
                ..
            } if on("message_deletes") && !skip_channel(channel) => Some(Entry {
                title: "Messages deleted in bulk".into(),
                description: format!("{} messages in <#{}>", message_ids.len(), channel.get()),
                color: RED,
            }),
            DomainEvent::MessageEdited {
                channel,
                message_id,
                data,
                ..
            } if on("message_edits") && !skip_channel(channel) => {
                let author = s(data, &["author", "id"]).unwrap_or("0");
                if author == self_user.get().to_string() {
                    return Ok(None);
                }
                let mut description = format!(
                    "<@{author}> edited message {message_id} in <#{}>",
                    channel.get()
                );
                if cfg.include_content
                    && ctx
                        .actions
                        .get_channel(*channel)
                        .await?
                        .is_some_and(|c| c.everyone_can_view)
                    && let Some(text) = s(data, &["content"])
                {
                    description.push_str(&format!("\n\nNow reads: {}", defuse(&clip(text, 500))));
                }
                Some(Entry {
                    title: "Message edited".into(),
                    description,
                    color: BLUE,
                })
            }
            DomainEvent::RoleCreated { payload, .. } if on("roles") => Some(Entry {
                title: "Role created".into(),
                description: defuse(s(payload, &["role", "name"]).unwrap_or("(unnamed)")),
                color: GREEN,
            }),
            DomainEvent::RoleUpdated { payload, .. } if on("roles") => Some(Entry {
                title: "Role updated".into(),
                description: defuse(s(payload, &["role", "name"]).unwrap_or("(unnamed)")),
                color: BLUE,
            }),
            DomainEvent::RoleDeleted { role, .. } if on("roles") => Some(Entry {
                title: "Role deleted".into(),
                description: format!("Role {}", role.get()),
                color: RED,
            }),
            DomainEvent::ChannelCreated {
                channel, payload, ..
            } if on("channels") => Some(Entry {
                title: "Channel created".into(),
                description: format!(
                    "<#{}> ({})",
                    channel.get(),
                    defuse(s(payload, &["name"]).unwrap_or("?"))
                ),
                color: GREEN,
            }),
            DomainEvent::ChannelDeleted {
                channel, payload, ..
            } if on("channels") => Some(Entry {
                title: "Channel deleted".into(),
                description: format!(
                    "{} ({})",
                    defuse(s(payload, &["name"]).unwrap_or("?")),
                    channel.get()
                ),
                color: RED,
            }),
            DomainEvent::MemberUpdated { user, payload, .. } if on("member_updates") => {
                self.member_diff(ctx, guild, *user, payload).await?
            }
            _ => None,
        })
    }

    /// Compare a member update with what was last seen. The first sighting of a
    /// member only records the baseline.
    async fn member_diff(
        &self,
        ctx: &FeatureCtx,
        guild: GuildId,
        user: UserId,
        payload: &Value,
    ) -> Result<Option<Entry>, FeatureError> {
        let roles = id_list(payload.get("roles").unwrap_or(&Value::Null));
        let nick = s(payload, &["nick"]).map(str::to_owned);
        let key = format!("member:{}", user.get());
        let prev = ctx.state.get(MOD_LOG_ID, guild, &key).await?;
        ctx.state
            .put(StateEntry::new(
                MOD_LOG_ID,
                guild,
                key,
                json!({"roles": roles, "nick": nick}),
            ))
            .await?;
        let Some(prev) = prev else { return Ok(None) };
        let before_roles = id_list(prev.value.get("roles").unwrap_or(&Value::Null));
        let before_nick = prev
            .value
            .get("nick")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let added: Vec<String> = roles
            .iter()
            .filter(|r| !before_roles.contains(r))
            .cloned()
            .collect();
        let removed: Vec<String> = before_roles
            .iter()
            .filter(|r| !roles.contains(r))
            .cloned()
            .collect();
        let mut lines = Vec::new();
        if !added.is_empty() {
            lines.push(format!("Roles added: {}", role_mentions(&added)));
        }
        if !removed.is_empty() {
            lines.push(format!("Roles removed: {}", role_mentions(&removed)));
        }
        if nick != before_nick {
            lines.push(format!(
                "Nickname: {} -> {}",
                defuse(before_nick.as_deref().unwrap_or("none")),
                defuse(nick.as_deref().unwrap_or("none"))
            ));
        }
        if lines.is_empty() {
            return Ok(None);
        }
        Ok(Some(Entry {
            title: "Member updated".into(),
            description: format!("<@{}>\n{}", user.get(), lines.join("\n")),
            color: BLUE,
        }))
    }
}

impl Feature for ModLog {
    fn id(&self) -> &'static str {
        MOD_LOG_ID
    }

    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Mod log".into(),
            description: "Record joins, leaves, bans, edits, deletes and role changes in a log \
                          channel."
                .into(),
            section: "Moderation".into(),
            phase: 11,
            default_enabled: false,
        }
    }

    fn settings_schema(&self) -> SettingsSchema {
        let opts = KINDS
            .iter()
            .map(|(v, l)| EnumOption {
                value: (*v).into(),
                label: (*l).into(),
            })
            .collect();
        let all: Vec<&str> = KINDS.iter().map(|(k, _)| *k).collect();
        SettingsSchema::new(vec![
            Field::new(
                "channel",
                "Log channel",
                FieldKind::ChannelPicker { multi: false },
            )
            .help("Make this channel visible to moderators only."),
            Field::new(
                "events",
                "What to log",
                FieldKind::List {
                    max_items: Some(KINDS.len()),
                    item: Box::new(FieldKind::Enum { options: opts }),
                },
            )
            .default_value(json!(all)),
            Field::new(
                "include_content",
                "Include edited message text",
                FieldKind::Bool,
            )
            .help("Only for channels every member can read. Deleted text is never logged.")
            .default_value(json!(false)),
            Field::new(
                "ignored_channels",
                "Channels to leave out",
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
        Box::pin(async move {
            let Some(guild) = super::super::runtime::guild_of(ev) else {
                return Ok(());
            };
            let cfg: Config = ctx.guild_config(MOD_LOG_ID, guild).await?;
            let Some(log) = cfg.channel.as_deref().and_then(snowflake) else {
                return Ok(());
            };
            let Some(entry) = self.entry(ctx, guild, &cfg, log, ev).await? else {
                return Ok(());
            };
            ctx.actions
                .post_message(
                    ChannelId::new(log),
                    OutMessage {
                        content: String::new(),
                        embed: Some(json!({
                            "type": "rich",
                            "title": entry.title,
                            "description": clip(&entry.description, 2000),
                            "color": entry.color,
                        })),
                        mention_users: Vec::new(),
                    },
                )
                .await?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use sr_events::RoleId;

    use super::*;
    use crate::FeatureRegistry;
    use crate::actions::ChannelInfo;
    use crate::testing::Harness;

    const G: GuildId = GuildId::new(1);
    const LOG: ChannelId = ChannelId::new(50);

    async fn harness(cfg: Value) -> Harness {
        let mut reg = FeatureRegistry::new();
        reg.register(ModLog).unwrap();
        let h = Harness::new(reg).await;
        let mut cfg = cfg;
        cfg["channel"] = json!("50");
        h.configure(MOD_LOG_ID, cfg).await;
        h
    }

    fn channel(id: u64, public: bool) -> ChannelInfo {
        ChannelInfo {
            id: ChannelId::new(id),
            guild: Some(G),
            kind: 0,
            name: "c".into(),
            nsfw: false,
            parent: None,
            rate_limit_per_user: 0,
            everyone_can_view: public,
        }
    }

    fn edit(channel: u64) -> DomainEvent {
        DomainEvent::MessageEdited {
            guild: Some(G),
            channel: ChannelId::new(channel),
            message_id: 9,
            data: json!({"author": {"id": "5"}, "content": "new text @everyone"}),
        }
    }

    async fn fire(h: &Harness, ev: DomainEvent) {
        ModLog.on_event(&ev, &h.ctx).await.unwrap();
    }

    fn last_embed(h: &Harness) -> Value {
        h.actions.posts().last().unwrap().1.embed.clone().unwrap()
    }

    #[tokio::test]
    async fn logs_joins_leaves_and_bans() {
        let h = harness(json!({})).await;
        fire(
            &h,
            DomainEvent::MemberJoined {
                guild: G,
                user: UserId::new(5),
                payload: json!({}),
            },
        )
        .await;
        assert_eq!(last_embed(&h)["title"], "Member joined");
        fire(
            &h,
            DomainEvent::MemberLeft {
                guild: G,
                user: UserId::new(5),
            },
        )
        .await;
        assert_eq!(last_embed(&h)["title"], "Member left");
        fire(
            &h,
            DomainEvent::MemberBanned {
                guild: G,
                user: UserId::new(5),
                payload: json!({}),
            },
        )
        .await;
        assert_eq!(last_embed(&h)["title"], "Member banned");
        assert!(h.actions.posts().iter().all(|(c, _)| *c == LOG));
    }

    #[tokio::test]
    async fn only_selected_kinds_are_logged() {
        let h = harness(json!({"events": ["joins"]})).await;
        fire(
            &h,
            DomainEvent::MemberLeft {
                guild: G,
                user: UserId::new(5),
            },
        )
        .await;
        assert!(h.actions.posts().is_empty());
    }

    #[tokio::test]
    async fn edit_text_is_private_by_default() {
        let h = harness(json!({})).await;
        h.actions.put_channel(channel(10, true));
        fire(&h, edit(10)).await;
        assert!(
            !last_embed(&h)["description"]
                .as_str()
                .unwrap()
                .contains("new text")
        );
    }

    #[tokio::test]
    async fn edit_text_only_for_public_channels_when_enabled() {
        let h = harness(json!({"include_content": true})).await;
        h.actions.put_channel(channel(10, true));
        h.actions.put_channel(channel(11, false));
        fire(&h, edit(10)).await;
        let d = last_embed(&h)["description"].as_str().unwrap().to_owned();
        assert!(d.contains("new text"));
        assert!(!d.contains("@everyone"), "mentions are defused");
        fire(&h, edit(11)).await;
        assert!(
            !last_embed(&h)["description"]
                .as_str()
                .unwrap()
                .contains("new text")
        );
    }

    #[tokio::test]
    async fn log_channel_and_ignored_channels_are_skipped() {
        let h = harness(json!({"ignored_channels": ["12"]})).await;
        fire(
            &h,
            DomainEvent::MessageDeleted {
                guild: Some(G),
                channel: LOG,
                message_id: 1,
            },
        )
        .await;
        fire(
            &h,
            DomainEvent::MessageDeleted {
                guild: Some(G),
                channel: ChannelId::new(12),
                message_id: 1,
            },
        )
        .await;
        assert!(h.actions.posts().is_empty());
        fire(
            &h,
            DomainEvent::MessageDeleted {
                guild: Some(G),
                channel: ChannelId::new(13),
                message_id: 1,
            },
        )
        .await;
        assert_eq!(h.actions.posts().len(), 1);
    }

    #[tokio::test]
    async fn member_updates_are_diffed_against_the_last_sighting() {
        let h = harness(json!({})).await;
        let update = |roles: Value, nick: Value| DomainEvent::MemberUpdated {
            guild: G,
            user: UserId::new(5),
            payload: json!({"roles": roles, "nick": nick}),
        };
        fire(&h, update(json!(["1"]), Value::Null)).await;
        assert!(h.actions.posts().is_empty(), "baseline only");
        fire(&h, update(json!(["1", "2"]), json!("Neo"))).await;
        let d = last_embed(&h)["description"].as_str().unwrap().to_owned();
        assert!(d.contains("Roles added: <@&2>"));
        assert!(d.contains("none -> Neo"));
        fire(&h, update(json!(["1", "2"]), json!("Neo"))).await;
        assert_eq!(h.actions.posts().len(), 1, "no change, no entry");
    }

    #[tokio::test]
    async fn role_events_name_the_role() {
        let h = harness(json!({})).await;
        fire(
            &h,
            DomainEvent::RoleCreated {
                guild: G,
                role: RoleId::new(3),
                payload: json!({"role": {"name": "Mods"}}),
            },
        )
        .await;
        assert_eq!(last_embed(&h)["description"], "Mods");
    }

    #[test]
    fn schema_defaults_validate() {
        let s = ModLog.settings_schema();
        s.validate(&s.defaults(Scope::Guild)).unwrap();
    }
}
