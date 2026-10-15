//! FTR-TEMPR-001 Temporary roles: grant a role for a limited time; the tick removes
//! it when the time is up.
//!
//! Grants are rows in the module's state (`grant:{user}:{role}`, due at the expiry),
//! created through [`TempRoles::grant`]. Granting again replaces the expiry. A grant
//! disappears when the member leaves or the role is deleted. An expiry that passed
//! while the server was down is honoured on the first tick afterwards.

use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::json;
use sr_events::{DomainEvent, GuildId, UserId};

use crate::feature::{Feature, FeatureCtx, FeatureError, FeatureMeta, Scope};
use crate::schema::{Field, FieldKind, SettingsSchema};
use crate::state::StateEntry;

use super::snowflakes;

pub const TEMP_ROLES_ID: &str = "moderation.temp_roles";

/// Most grants one tick removes, so a backlog cannot stall the others.
const TICK_BATCH: usize = 200;

#[derive(Default)]
pub struct TempRoles;

#[derive(Debug, Deserialize)]
#[serde(default)]
struct Config {
    max_duration: u64,
    allowed_roles: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_duration: 90 * 86_400,
            allowed_roles: Vec::new(),
        }
    }
}

/// Why a grant was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum GrantError {
    /// The role is not in the guild's allow list.
    RoleNotAllowed,
    /// Zero, or longer than the guild's maximum.
    BadDuration { max_secs: u64 },
}

impl std::fmt::Display for GrantError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RoleNotAllowed => write!(f, "that role cannot be granted temporarily"),
            Self::BadDuration { max_secs } => {
                write!(f, "duration must be between 1 and {max_secs} seconds")
            }
        }
    }
}

fn grant_key(user: u64, role: u64) -> String {
    format!("grant:{user}:{role}")
}

impl TempRoles {
    /// Give `user` the role now and schedule its removal `duration_secs` from now.
    /// Returns the expiry in unix ms. The caller has already checked that the acting
    /// member may manage the role.
    pub async fn grant(
        ctx: &FeatureCtx,
        guild: GuildId,
        user: UserId,
        role: u64,
        duration_secs: u64,
    ) -> Result<Result<i64, GrantError>, FeatureError> {
        let cfg: Config = ctx.guild_config(TEMP_ROLES_ID, guild).await?;
        let allowed = snowflakes(&cfg.allowed_roles);
        if !allowed.is_empty() && !allowed.contains(&role) {
            return Ok(Err(GrantError::RoleNotAllowed));
        }
        if duration_secs == 0 || duration_secs > cfg.max_duration {
            return Ok(Err(GrantError::BadDuration {
                max_secs: cfg.max_duration,
            }));
        }
        let expires = ctx.now_ms() + (duration_secs as i64) * 1000;
        ctx.actions.add_role(guild, user, role).await?;
        ctx.state
            .put(
                StateEntry::new(
                    TEMP_ROLES_ID,
                    guild,
                    grant_key(user.get(), role),
                    json!({"user": user.get(), "role": role, "expires_ms": expires}),
                )
                .due(expires),
            )
            .await?;
        Ok(Ok(expires))
    }

    /// Active grants of a guild as `(user, role, expires_ms)`.
    pub async fn list(
        ctx: &FeatureCtx,
        guild: GuildId,
    ) -> Result<Vec<(UserId, u64, i64)>, FeatureError> {
        Ok(ctx
            .state
            .list(TEMP_ROLES_ID, guild, "grant:")
            .await?
            .into_iter()
            .filter_map(|e| {
                Some((
                    UserId::new(e.value.get("user")?.as_u64()?),
                    e.value.get("role")?.as_u64()?,
                    e.due_ms?,
                ))
            })
            .collect())
    }

    /// Cancel a grant early and take the role away.
    pub async fn revoke(
        ctx: &FeatureCtx,
        guild: GuildId,
        user: UserId,
        role: u64,
    ) -> Result<bool, FeatureError> {
        let key = grant_key(user.get(), role);
        if ctx.state.get(TEMP_ROLES_ID, guild, &key).await?.is_none() {
            return Ok(false);
        }
        ctx.actions.remove_role(guild, user, role).await?;
        ctx.state.delete(TEMP_ROLES_ID, guild, &key).await?;
        Ok(true)
    }
}

impl Feature for TempRoles {
    fn id(&self) -> &'static str {
        TEMP_ROLES_ID
    }

    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Temporary roles".into(),
            description: "Grant a role for a limited time; it is removed automatically.".into(),
            section: "Moderation".into(),
            phase: 11,
            default_enabled: false,
        }
    }

    fn settings_schema(&self) -> SettingsSchema {
        SettingsSchema::new(vec![
            Field::new(
                "max_duration",
                "Longest grant",
                FieldKind::Duration {
                    min_secs: Some(60),
                    max_secs: Some(365 * 86_400),
                },
            )
            .default_value(json!(90 * 86_400)),
            Field::new(
                "allowed_roles",
                "Roles that can be granted temporarily",
                FieldKind::RolePicker { multi: true },
            )
            .help("Leave empty to allow any role you can manage."),
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
                DomainEvent::MemberLeft { guild, user } => {
                    let prefix = format!("grant:{}:", user.get());
                    for e in ctx.state.list(TEMP_ROLES_ID, *guild, &prefix).await? {
                        ctx.state.delete(TEMP_ROLES_ID, *guild, &e.key).await?;
                    }
                }
                DomainEvent::RoleDeleted { guild, role, .. } => {
                    let suffix = format!(":{}", role.get());
                    for e in ctx.state.list(TEMP_ROLES_ID, *guild, "grant:").await? {
                        if e.key.ends_with(&suffix) {
                            ctx.state.delete(TEMP_ROLES_ID, *guild, &e.key).await?;
                        }
                    }
                }
                _ => {}
            }
            Ok(())
        })
    }

    fn on_tick<'a>(&'a self, ctx: &'a FeatureCtx) -> BoxFuture<'a, Result<(), FeatureError>> {
        Box::pin(async move {
            // Expiries are honoured even if the module was switched off since: a
            // role that was promised to go away must go away.
            let due = ctx
                .state
                .due(TEMP_ROLES_ID, ctx.now_ms(), TICK_BATCH)
                .await?;
            for e in due {
                let (Some(user), Some(role)) = (
                    e.value.get("user").and_then(|v| v.as_u64()),
                    e.value.get("role").and_then(|v| v.as_u64()),
                ) else {
                    ctx.state.delete(TEMP_ROLES_ID, e.guild, &e.key).await?;
                    continue;
                };
                match ctx
                    .actions
                    .remove_role(e.guild, UserId::new(user), role)
                    .await
                {
                    Ok(()) | Err(crate::actions::ActionError::NotFound) => {
                        ctx.state.delete(TEMP_ROLES_ID, e.guild, &e.key).await?;
                    }
                    // Keep the row and retry on the next tick.
                    Err(err) => {
                        tracing::warn!(error = %err, "removing an expired temporary role failed");
                    }
                }
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use sr_events::RoleId;

    use super::*;
    use crate::FeatureRegistry;
    use crate::testing::Harness;

    const G: GuildId = GuildId::new(1);
    const U: UserId = UserId::new(5);

    async fn harness() -> Harness {
        let mut reg = FeatureRegistry::new();
        reg.register(TempRoles).unwrap();
        Harness::new(reg).await
    }

    #[tokio::test]
    async fn grant_then_expire() {
        let h = harness().await;
        let expires = TempRoles::grant(&h.ctx, G, U, 100, 3600)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(h.actions.roles_of(1, 5), vec![100]);
        assert_eq!(expires, h.now() + 3_600_000);

        h.clock.advance_ms(3_599_000);
        TempRoles.on_tick(&h.ctx).await.unwrap();
        assert_eq!(h.actions.roles_of(1, 5), vec![100], "not yet");

        h.clock.advance_ms(2_000);
        TempRoles.on_tick(&h.ctx).await.unwrap();
        assert!(h.actions.roles_of(1, 5).is_empty());
        assert!(TempRoles::list(&h.ctx, G).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn regranting_replaces_the_expiry() {
        let h = harness().await;
        TempRoles::grant(&h.ctx, G, U, 100, 60)
            .await
            .unwrap()
            .unwrap();
        TempRoles::grant(&h.ctx, G, U, 100, 7200)
            .await
            .unwrap()
            .unwrap();
        h.clock.advance_ms(61_000);
        TempRoles.on_tick(&h.ctx).await.unwrap();
        assert_eq!(h.actions.roles_of(1, 5), vec![100]);
        assert_eq!(TempRoles::list(&h.ctx, G).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn limits_are_enforced() {
        let h = harness().await;
        h.configure(
            TEMP_ROLES_ID,
            json!({"max_duration": 600, "allowed_roles": ["100"]}),
        )
        .await;
        assert_eq!(
            TempRoles::grant(&h.ctx, G, U, 200, 60).await.unwrap(),
            Err(GrantError::RoleNotAllowed)
        );
        assert_eq!(
            TempRoles::grant(&h.ctx, G, U, 100, 601).await.unwrap(),
            Err(GrantError::BadDuration { max_secs: 600 })
        );
        assert_eq!(
            TempRoles::grant(&h.ctx, G, U, 100, 0).await.unwrap(),
            Err(GrantError::BadDuration { max_secs: 600 })
        );
        assert!(h.actions.calls().is_empty());
    }

    #[tokio::test]
    async fn leaving_or_role_deletion_drops_grants_and_revoke_works() {
        let h = harness().await;
        TempRoles::grant(&h.ctx, G, U, 100, 60)
            .await
            .unwrap()
            .unwrap();
        TempRoles::grant(&h.ctx, G, UserId::new(6), 100, 60)
            .await
            .unwrap()
            .unwrap();
        TempRoles::grant(&h.ctx, G, UserId::new(6), 300, 60)
            .await
            .unwrap()
            .unwrap();
        TempRoles
            .on_event(&DomainEvent::MemberLeft { guild: G, user: U }, &h.ctx)
            .await
            .unwrap();
        TempRoles
            .on_event(
                &DomainEvent::RoleDeleted {
                    guild: G,
                    role: RoleId::new(300),
                    payload: json!({}),
                },
                &h.ctx,
            )
            .await
            .unwrap();
        let left = TempRoles::list(&h.ctx, G).await.unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!((left[0].0, left[0].1), (UserId::new(6), 100));
        assert!(
            TempRoles::revoke(&h.ctx, G, UserId::new(6), 100)
                .await
                .unwrap()
        );
        assert!(
            !TempRoles::revoke(&h.ctx, G, UserId::new(6), 100)
                .await
                .unwrap()
        );
        assert!(!h.actions.roles_of(1, 6).contains(&100));
    }

    #[test]
    fn schema_defaults_validate() {
        let s = TempRoles.settings_schema();
        s.validate(&s.defaults(Scope::Guild)).unwrap();
    }
}
