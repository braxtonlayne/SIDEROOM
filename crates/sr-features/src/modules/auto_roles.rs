//! FTR-AUTOR-001 Auto-roles: roles given to members when they join.
//!
//! * `delay` waits that long after joining (a probation period); 0 gives the roles at
//!   once.
//! * `wait_for_rules` holds back members who still have to accept the server rules
//!   (membership screening, the member's `pending` flag) and grants when they have.
//! * Bots can get their own roles.
//!
//! Pending grants are rows in the module's state, keyed by member and due when the
//! delay is over, so they survive a restart. A member who leaves is dropped.

use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::{Value, json};
use sr_events::{DomainEvent, GuildId, UserId};

use crate::feature::{Feature, FeatureCtx, FeatureError, FeatureMeta, Scope};
use crate::schema::{Field, FieldKind, SettingsSchema};
use crate::state::StateEntry;

use super::snowflakes;

pub const AUTO_ROLES_ID: &str = "moderation.auto_roles";

const TICK_BATCH: usize = 200;

#[derive(Default)]
pub struct AutoRoles;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Config {
    roles: Vec<String>,
    bot_roles: Vec<String>,
    delay: u64,
    wait_for_rules: bool,
}

fn key(user: u64) -> String {
    format!("join:{user}")
}

impl AutoRoles {
    async fn give(
        &self,
        ctx: &FeatureCtx,
        guild: GuildId,
        user: UserId,
        bot: bool,
    ) -> Result<(), FeatureError> {
        let cfg: Config = ctx.guild_config(AUTO_ROLES_ID, guild).await?;
        let roles = if bot { &cfg.bot_roles } else { &cfg.roles };
        for role in snowflakes(roles) {
            if !ctx.actions.has_role(guild, user, role).await? {
                ctx.actions.add_role(guild, user, role).await?;
            }
        }
        Ok(())
    }

    /// Hand over now, or park the member until the delay is over.
    async fn schedule(
        &self,
        ctx: &FeatureCtx,
        guild: GuildId,
        user: UserId,
        bot: bool,
        pending: bool,
    ) -> Result<(), FeatureError> {
        let cfg: Config = ctx.guild_config(AUTO_ROLES_ID, guild).await?;
        if cfg.wait_for_rules && pending {
            // Wait for the member update that clears `pending`.
            ctx.state
                .put(StateEntry::new(
                    AUTO_ROLES_ID,
                    guild,
                    key(user.get()),
                    json!({"user": user.get(), "bot": bot, "waiting_for_rules": true}),
                ))
                .await?;
            return Ok(());
        }
        if cfg.delay == 0 {
            return self.give(ctx, guild, user, bot).await;
        }
        let due = ctx.now_ms() + (cfg.delay as i64) * 1000;
        ctx.state
            .put(
                StateEntry::new(
                    AUTO_ROLES_ID,
                    guild,
                    key(user.get()),
                    json!({"user": user.get(), "bot": bot}),
                )
                .due(due),
            )
            .await?;
        Ok(())
    }
}

fn is_bot(payload: &Value) -> bool {
    payload
        .get("user")
        .and_then(|u| u.get("bot"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn is_pending(payload: &Value) -> bool {
    payload
        .get("pending")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

impl Feature for AutoRoles {
    fn id(&self) -> &'static str {
        AUTO_ROLES_ID
    }

    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Auto-roles".into(),
            description: "Give new members roles when they join, optionally after a delay or \
                          once they accept the rules."
                .into(),
            section: "Moderation".into(),
            phase: 11,
            default_enabled: false,
        }
    }

    fn settings_schema(&self) -> SettingsSchema {
        SettingsSchema::new(vec![
            Field::new(
                "roles",
                "Roles for members",
                FieldKind::RolePicker { multi: true },
            ),
            Field::new(
                "bot_roles",
                "Roles for bots",
                FieldKind::RolePicker { multi: true },
            ),
            Field::new(
                "delay",
                "Wait after joining",
                FieldKind::Duration {
                    min_secs: Some(0),
                    max_secs: Some(30 * 86_400),
                },
            )
            .help("0 gives the roles immediately.")
            .default_value(json!(0)),
            Field::new(
                "wait_for_rules",
                "Wait until the rules are accepted",
                FieldKind::Bool,
            )
            .default_value(json!(false)),
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
            match ev {
                DomainEvent::MemberJoined {
                    guild,
                    user,
                    payload,
                } => {
                    self.schedule(ctx, *guild, *user, is_bot(payload), is_pending(payload))
                        .await?;
                }
                DomainEvent::MemberUpdated {
                    guild,
                    user,
                    payload,
                } if !is_pending(payload) => {
                    // The member passed screening: release a parked grant.
                    let Some(entry) = ctx
                        .state
                        .get(AUTO_ROLES_ID, *guild, &key(user.get()))
                        .await?
                    else {
                        return Ok(());
                    };
                    if entry
                        .value
                        .get("waiting_for_rules")
                        .and_then(Value::as_bool)
                        != Some(true)
                    {
                        return Ok(());
                    }
                    ctx.state
                        .delete(AUTO_ROLES_ID, *guild, &key(user.get()))
                        .await?;
                    let bot = entry.value.get("bot").and_then(Value::as_bool) == Some(true);
                    let cfg: Config = ctx.guild_config(AUTO_ROLES_ID, *guild).await?;
                    if cfg.delay == 0 {
                        self.give(ctx, *guild, *user, bot).await?;
                    } else {
                        self.schedule(ctx, *guild, *user, bot, false).await?;
                    }
                }
                DomainEvent::MemberLeft { guild, user } => {
                    ctx.state
                        .delete(AUTO_ROLES_ID, *guild, &key(user.get()))
                        .await?;
                }
                _ => {}
            }
            Ok(())
        })
    }

    fn on_tick<'a>(&'a self, ctx: &'a FeatureCtx) -> BoxFuture<'a, Result<(), FeatureError>> {
        Box::pin(async move {
            for e in ctx
                .state
                .due(AUTO_ROLES_ID, ctx.now_ms(), TICK_BATCH)
                .await?
            {
                let Some(user) = e.value.get("user").and_then(Value::as_u64) else {
                    ctx.state.delete(AUTO_ROLES_ID, e.guild, &e.key).await?;
                    continue;
                };
                let bot = e.value.get("bot").and_then(Value::as_bool) == Some(true);
                let enabled = ctx
                    .settings
                    .is_enabled(
                        AUTO_ROLES_ID,
                        &crate::feature::ScopeRef::Guild { guild: e.guild },
                    )
                    .await?;
                if enabled {
                    match self.give(ctx, e.guild, UserId::new(user), bot).await {
                        Ok(()) => {}
                        Err(FeatureError::Action(crate::actions::ActionError::NotFound)) => {}
                        Err(err) => {
                            tracing::warn!(error = %err, "delayed auto-role failed; will retry");
                            continue;
                        }
                    }
                }
                ctx.state.delete(AUTO_ROLES_ID, e.guild, &e.key).await?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FeatureRegistry;
    use crate::testing::Harness;

    const G: GuildId = GuildId::new(1);

    async fn harness(cfg: Value) -> Harness {
        let mut reg = FeatureRegistry::new();
        reg.register(AutoRoles).unwrap();
        let h = Harness::new(reg).await;
        h.configure(AUTO_ROLES_ID, cfg).await;
        h
    }

    fn joined(user: u64, bot: bool, pending: bool) -> DomainEvent {
        DomainEvent::MemberJoined {
            guild: G,
            user: UserId::new(user),
            payload: json!({"user": {"id": user.to_string(), "bot": bot}, "pending": pending}),
        }
    }

    #[tokio::test]
    async fn immediate_roles_split_by_bot() {
        let h = harness(json!({"roles": ["100", "101"], "bot_roles": ["200"]})).await;
        AutoRoles
            .on_event(&joined(5, false, false), &h.ctx)
            .await
            .unwrap();
        AutoRoles
            .on_event(&joined(6, true, false), &h.ctx)
            .await
            .unwrap();
        assert_eq!(h.actions.roles_of(1, 5), vec![100, 101]);
        assert_eq!(h.actions.roles_of(1, 6), vec![200]);
    }

    #[tokio::test]
    async fn delay_parks_the_member_until_the_tick() {
        let h = harness(json!({"roles": ["100"], "delay": 600})).await;
        AutoRoles
            .on_event(&joined(5, false, false), &h.ctx)
            .await
            .unwrap();
        assert!(h.actions.roles_of(1, 5).is_empty());
        h.clock.advance_ms(599_000);
        AutoRoles.on_tick(&h.ctx).await.unwrap();
        assert!(h.actions.roles_of(1, 5).is_empty());
        h.clock.advance_ms(2_000);
        AutoRoles.on_tick(&h.ctx).await.unwrap();
        assert_eq!(h.actions.roles_of(1, 5), vec![100]);
        // Done: a later tick does nothing more.
        h.actions.clear_calls();
        AutoRoles.on_tick(&h.ctx).await.unwrap();
        assert!(h.actions.calls().is_empty());
    }

    #[tokio::test]
    async fn waits_for_rules_acceptance() {
        let h = harness(json!({"roles": ["100"], "wait_for_rules": true})).await;
        AutoRoles
            .on_event(&joined(5, false, true), &h.ctx)
            .await
            .unwrap();
        assert!(h.actions.roles_of(1, 5).is_empty());
        // A member update while still pending changes nothing.
        let still = DomainEvent::MemberUpdated {
            guild: G,
            user: UserId::new(5),
            payload: json!({"pending": true}),
        };
        AutoRoles.on_event(&still, &h.ctx).await.unwrap();
        assert!(h.actions.roles_of(1, 5).is_empty());
        let accepted = DomainEvent::MemberUpdated {
            guild: G,
            user: UserId::new(5),
            payload: json!({"pending": false}),
        };
        AutoRoles.on_event(&accepted, &h.ctx).await.unwrap();
        assert_eq!(h.actions.roles_of(1, 5), vec![100]);
    }

    #[tokio::test]
    async fn leaving_cancels_the_grant() {
        let h = harness(json!({"roles": ["100"], "delay": 60})).await;
        AutoRoles
            .on_event(&joined(5, false, false), &h.ctx)
            .await
            .unwrap();
        AutoRoles
            .on_event(
                &DomainEvent::MemberLeft {
                    guild: G,
                    user: UserId::new(5),
                },
                &h.ctx,
            )
            .await
            .unwrap();
        h.clock.advance_ms(120_000);
        AutoRoles.on_tick(&h.ctx).await.unwrap();
        assert!(h.actions.calls().is_empty());
    }

    #[test]
    fn schema_defaults_validate() {
        let s = AutoRoles.settings_schema();
        s.validate(&s.defaults(Scope::Guild)).unwrap();
    }
}
