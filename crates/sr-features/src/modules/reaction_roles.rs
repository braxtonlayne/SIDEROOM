//! FTR-RR-001 Reaction roles: reacting to a chosen message grants a role.
//!
//! Each *panel* is one message with its own mode:
//!
//! * `multiple` - any number of the panel's roles; removing a reaction removes the role.
//! * `unique`   - one role at a time; reacting with another emoji swaps (the old
//!   reaction is removed for the user and so is the old role).
//! * `verify`   - the role is granted once and removing the reaction keeps it, the
//!   usual shape of a rules-agreement or verification message.

use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::json;
use sr_events::{ChannelId, DomainEvent, GuildId, UserId};

use crate::feature::{Feature, FeatureCtx, FeatureError, FeatureMeta, Scope};
use crate::schema::{EnumOption, Field, FieldKind, SettingsSchema};

use super::{event_emoji_key, normalize_emoji, snowflake};

pub const REACTION_ROLES_ID: &str = "lounge.reaction_roles";

#[derive(Default)]
pub struct ReactionRoles;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Config {
    panels: Vec<Panel>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Panel {
    /// Message id, as a decimal string.
    message: String,
    mode: String,
    mappings: Vec<Mapping>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Mapping {
    emoji: String,
    role: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Multiple,
    Unique,
    Verify,
}

impl Panel {
    fn mode(&self) -> Mode {
        match self.mode.as_str() {
            "unique" => Mode::Unique,
            "verify" => Mode::Verify,
            _ => Mode::Multiple,
        }
    }

    fn role_for(&self, key: &str) -> Option<u64> {
        self.mappings
            .iter()
            .find(|m| normalize_emoji(&m.emoji) == key)
            .and_then(|m| snowflake(&m.role))
    }
}

impl ReactionRoles {
    async fn handle(&self, ev: &DomainEvent, ctx: &FeatureCtx) -> Result<(), FeatureError> {
        match ev {
            DomainEvent::ReactionAdded {
                guild: Some(g),
                channel,
                message_id,
                user,
                emoji,
                burst: false,
            } => {
                if let Some(key) = event_emoji_key(emoji) {
                    self.added(ctx, *g, *channel, *message_id, *user, &key)
                        .await?;
                }
            }
            DomainEvent::ReactionRemoved {
                guild: Some(g),
                message_id,
                user,
                emoji,
                burst: false,
                ..
            } => {
                if let Some(key) = event_emoji_key(emoji) {
                    self.removed(ctx, *g, *message_id, *user, &key).await?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn added(
        &self,
        ctx: &FeatureCtx,
        guild: GuildId,
        channel: ChannelId,
        message: u64,
        user: UserId,
        key: &str,
    ) -> Result<(), FeatureError> {
        if user == ctx.actions.self_user() {
            return Ok(());
        }
        let cfg: Config = ctx.guild_config(REACTION_ROLES_ID, guild).await?;
        let Some(panel) = cfg
            .panels
            .iter()
            .find(|p| snowflake(&p.message) == Some(message))
        else {
            return Ok(());
        };
        let Some(role) = panel.role_for(key) else {
            return Ok(());
        };
        if panel.mode() == Mode::Unique {
            for other in &panel.mappings {
                let okey = normalize_emoji(&other.emoji);
                let Some(orole) = snowflake(&other.role) else {
                    continue;
                };
                if okey == key || orole == role {
                    continue;
                }
                // Dropping the reaction also fires a removal event for it; doing
                // the role change here too keeps the result right if that is lost.
                if ctx.actions.has_role(guild, user, orole).await? {
                    ctx.actions.remove_role(guild, user, orole).await?;
                }
                ctx.actions
                    .remove_reaction(channel, message, &okey, user)
                    .await?;
            }
        }
        if !ctx.actions.has_role(guild, user, role).await? {
            ctx.actions.add_role(guild, user, role).await?;
        }
        Ok(())
    }

    async fn removed(
        &self,
        ctx: &FeatureCtx,
        guild: GuildId,
        message: u64,
        user: UserId,
        key: &str,
    ) -> Result<(), FeatureError> {
        let cfg: Config = ctx.guild_config(REACTION_ROLES_ID, guild).await?;
        let Some(panel) = cfg
            .panels
            .iter()
            .find(|p| snowflake(&p.message) == Some(message))
        else {
            return Ok(());
        };
        if panel.mode() == Mode::Verify {
            return Ok(());
        }
        let Some(role) = panel.role_for(key) else {
            return Ok(());
        };
        if ctx.actions.has_role(guild, user, role).await? {
            ctx.actions.remove_role(guild, user, role).await?;
        }
        Ok(())
    }
}

impl Feature for ReactionRoles {
    fn id(&self) -> &'static str {
        REACTION_ROLES_ID
    }

    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Reaction roles".into(),
            description: "Members pick roles by reacting to a message.".into(),
            section: "Lounge".into(),
            phase: 11,
            default_enabled: false,
        }
    }

    fn settings_schema(&self) -> SettingsSchema {
        let mode = |v: &str, l: &str| EnumOption {
            value: v.into(),
            label: l.into(),
        };
        SettingsSchema::new(vec![Field::new(
            "panels",
            "Reaction role messages",
            FieldKind::List {
                max_items: Some(50),
                item: Box::new(FieldKind::Group {
                    fields: vec![
                        Field::new(
                            "message",
                            "Message id",
                            FieldKind::String { max_len: Some(20) },
                        )
                        .help("Copy the message id from the message's menu."),
                        Field::new(
                            "mode",
                            "Mode",
                            FieldKind::Enum {
                                options: vec![
                                    mode("multiple", "Any number of roles"),
                                    mode("unique", "One role at a time"),
                                    mode("verify", "Grant once, keep when un-reacting"),
                                ],
                            },
                        )
                        .default_value(json!("multiple")),
                        Field::new(
                            "mappings",
                            "Emoji and roles",
                            FieldKind::List {
                                max_items: Some(20),
                                item: Box::new(FieldKind::Group {
                                    fields: vec![
                                        Field::new(
                                            "emoji",
                                            "Emoji",
                                            FieldKind::String { max_len: Some(64) },
                                        )
                                        .help("A unicode emoji, or name:id for a custom one."),
                                        Field::new(
                                            "role",
                                            "Role",
                                            FieldKind::RolePicker { multi: false },
                                        ),
                                    ],
                                }),
                            },
                        ),
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
        Box::pin(self.handle(ev, ctx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FeatureRegistry;
    use crate::testing::{Call, Harness};

    const G: GuildId = GuildId::new(1);
    const CH: ChannelId = ChannelId::new(10);
    const MSG: u64 = 77;

    async fn harness(mode: &str) -> Harness {
        let mut reg = FeatureRegistry::new();
        reg.register(ReactionRoles).unwrap();
        let h = Harness::new(reg).await;
        h.configure(
            REACTION_ROLES_ID,
            json!({"panels": [{
                "message": "77", "mode": mode,
                "mappings": [
                    {"emoji": "\u{1f34e}", "role": "100"},
                    {"emoji": "pog:5", "role": "200"},
                ]
            }]}),
        )
        .await;
        h
    }

    fn add(user: u64, name: &str, id: Option<&str>) -> DomainEvent {
        DomainEvent::ReactionAdded {
            guild: Some(G),
            channel: CH,
            message_id: MSG,
            user: UserId::new(user),
            emoji: json!({"id": id, "name": name}),
            burst: false,
        }
    }

    fn remove(user: u64, name: &str, id: Option<&str>) -> DomainEvent {
        DomainEvent::ReactionRemoved {
            guild: Some(G),
            channel: CH,
            message_id: MSG,
            user: UserId::new(user),
            emoji: json!({"id": id, "name": name}),
            burst: false,
        }
    }

    async fn fire(h: &Harness, ev: DomainEvent) {
        ReactionRoles.on_event(&ev, &h.ctx).await.unwrap();
    }

    #[tokio::test]
    async fn multiple_mode_grants_and_revokes() {
        let h = harness("multiple").await;
        fire(&h, add(5, "\u{1f34e}", None)).await;
        fire(&h, add(5, "pog", Some("5"))).await;
        assert_eq!(h.actions.roles_of(1, 5), vec![100, 200]);
        fire(&h, remove(5, "\u{1f34e}", None)).await;
        assert_eq!(h.actions.roles_of(1, 5), vec![200]);
    }

    #[tokio::test]
    async fn unique_mode_swaps_roles_and_drops_the_old_reaction() {
        let h = harness("unique").await;
        h.actions.react(MSG, "\u{1f34e}", 5);
        fire(&h, add(5, "\u{1f34e}", None)).await;
        h.actions.react(MSG, "pog:5", 5);
        fire(&h, add(5, "pog", Some("5"))).await;
        assert_eq!(h.actions.roles_of(1, 5), vec![200]);
        assert!(h.actions.calls().iter().any(
            |c| matches!(c, Call::RemoveReaction(_, MSG, k, u) if k == "\u{1f34e}" && u.get() == 5)
        ));
    }

    #[tokio::test]
    async fn verify_mode_keeps_the_role() {
        let h = harness("verify").await;
        fire(&h, add(5, "\u{1f34e}", None)).await;
        fire(&h, remove(5, "\u{1f34e}", None)).await;
        assert_eq!(h.actions.roles_of(1, 5), vec![100]);
    }

    #[tokio::test]
    async fn unrelated_messages_emoji_and_the_bot_are_ignored() {
        let h = harness("multiple").await;
        fire(&h, add(5, "\u{1f525}", None)).await;
        fire(
            &h,
            DomainEvent::ReactionAdded {
                guild: Some(G),
                channel: CH,
                message_id: 999,
                user: UserId::new(5),
                emoji: json!({"id": null, "name": "\u{1f34e}"}),
                burst: false,
            },
        )
        .await;
        fire(&h, add(1, "\u{1f34e}", None)).await;
        assert!(h.actions.calls().is_empty());
    }

    #[test]
    fn schema_accepts_a_panel_and_rejects_junk() {
        let s = ReactionRoles.settings_schema();
        let ok = json!({"panels": [{"message": "1", "mode": "unique",
            "mappings": [{"emoji": "x", "role": "2"}]}]});
        s.validate(ok.as_object().unwrap()).unwrap();
        let bad = json!({"panels": [{"mode": "sideways"}]});
        assert!(s.validate(bad.as_object().unwrap()).is_err());
    }
}
