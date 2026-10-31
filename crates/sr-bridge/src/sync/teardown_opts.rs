//! The whole_guild teardown options (design section 12): remove what the bridge created on Discord, and restore
//! Discord's pre-bridge permissions from the snapshot. The bot never deletes a Discord object it did not create
//! (map `origin = 'sr'` is the only thing removed); there is no option for it.

use serde::Serialize;
use sqlx::Row;

use crate::discord::CallError;
use crate::discord_structure::{Overwrite, RoleParams, reason_for_op};
use crate::error::BridgeResult;
use crate::service::BridgeService;
use crate::teardown::{FailedObject, RemovalReport, TeardownOptions};

/// What the dialog shows before the owner chooses (counts, and the channels with native Discord messages).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TeardownPreview {
    pub whole_guild: bool,
    /// Roles, categories, channels and threads the bridge created on Discord (removed by "also remove").
    pub created_roles: i64,
    pub created_categories: i64,
    pub created_channels: i64,
    pub created_threads: i64,
    /// Of the created channels, how many hold messages posted natively by Discord members (they are lost on
    /// Discord when removed).
    pub created_with_discord_messages: i64,
    /// Roles and channels whose Discord permissions "restore" would reset.
    pub restorable_roles: i64,
    pub restorable_channels: i64,
}

impl BridgeService {
    pub async fn teardown_preview(&self, guild: i64) -> BridgeResult<TeardownPreview> {
        let whole: bool = sqlx::query_scalar(
            "SELECT COALESCE((SELECT mode = 'whole_guild' FROM bridges WHERE guild_id = $1), false)",
        )
        .bind(guild)
        .fetch_one(&self.pool)
        .await?;
        if !whole {
            return Ok(TeardownPreview::default());
        }
        let r = sqlx::query(
            "SELECT \
               (SELECT count(*) FROM bridge_role_maps WHERE guild_id = $1 AND origin = 'sr' AND NOT is_everyone) AS roles, \
               (SELECT count(*) FROM bridge_channel_maps WHERE guild_id = $1 AND origin = 'sr' AND kind = 'category') AS cats, \
               (SELECT count(*) FROM bridge_channel_maps WHERE guild_id = $1 AND origin = 'sr' AND kind = 'channel') AS chans, \
               (SELECT count(*) FROM bridge_channel_maps WHERE guild_id = $1 AND origin = 'sr' AND kind = 'thread') AS threads, \
               (SELECT count(DISTINCT m.id) FROM bridge_channel_maps m JOIN bridge_message_map mm ON mm.map_id = m.id \
                  WHERE m.guild_id = $1 AND m.origin = 'sr' AND mm.direction = 'in') AS native, \
               (SELECT count(*) FROM bridge_role_maps WHERE guild_id = $1 AND origin IN ('matched', 'discord')) AS rroles, \
               (SELECT count(*) FROM bridge_channel_maps WHERE guild_id = $1 AND origin IN ('matched', 'discord') AND kind <> 'thread') AS rchans",
        )
        .bind(guild)
        .fetch_one(&self.pool)
        .await?;
        Ok(TeardownPreview {
            whole_guild: true,
            created_roles: r.try_get("roles")?,
            created_categories: r.try_get("cats")?,
            created_channels: r.try_get("chans")?,
            created_threads: r.try_get("threads")?,
            created_with_discord_messages: r.try_get("native")?,
            restorable_roles: r.try_get("rroles")?,
            restorable_channels: r.try_get("rchans")?,
        })
    }

    /// Act on Discord for the chosen options. Best effort: a failure is a [`FailedObject`], never an abort.
    pub(crate) async fn teardown_discord_structure(
        &self,
        guild: i64,
        opts: &TeardownOptions,
    ) -> RemovalReport {
        let mut rep = RemovalReport::default();
        let Ok(token) = self.open_bot_token(guild).await else {
            return rep;
        };
        let reason = reason_for_op(self.next_id());
        let api = &self.discord;
        let Ok(dg) = sqlx::query_scalar::<_, i64>(
            "SELECT discord_guild_id FROM bridges WHERE guild_id = $1",
        )
        .bind(guild)
        .fetch_one(&self.pool)
        .await
        else {
            return rep;
        };

        // Restore first: it touches objects that survive.
        if opts.restore_permissions {
            let roles: Vec<i64> = sqlx::query_scalar(
                "SELECT discord_role_id FROM bridge_role_maps WHERE guild_id = $1 AND origin IN ('matched', 'discord')",
            )
            .bind(guild)
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();
            let snap_roles = super::holds::load_snapshot(&self.pool, guild, "role")
                .await
                .unwrap_or_default();
            for (id, data) in snap_roles.iter().filter(|(id, _)| roles.contains(id)) {
                let Some(word) = data
                    .get("permissions")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<u64>().ok())
                else {
                    continue;
                };
                match api
                    .edit_role(
                        &token,
                        dg,
                        *id,
                        &RoleParams {
                            permissions: Some(word),
                            ..RoleParams::default()
                        },
                        &reason,
                    )
                    .await
                {
                    Ok(_) => rep.restored_roles += 1,
                    Err(CallError::Gone(_)) => {}
                    Err(_) => rep.failed.push(FailedObject {
                        kind: "role".into(),
                        id: *id,
                    }),
                }
            }
            let chans: Vec<i64> = sqlx::query_scalar(
                "SELECT discord_channel_id FROM bridge_channel_maps WHERE guild_id = $1 \
                 AND origin IN ('matched', 'discord') AND kind <> 'thread'",
            )
            .bind(guild)
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();
            let snap_chans = super::holds::load_snapshot(&self.pool, guild, "channel")
                .await
                .unwrap_or_default();
            for (id, data) in snap_chans.iter().filter(|(id, _)| chans.contains(id)) {
                let ows: Vec<Overwrite> = data
                    .get("overwrites")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|o| {
                                Some(Overwrite {
                                    id: o.get("id")?.as_str()?.parse().ok()?,
                                    kind: o.get("type")?.as_i64()?,
                                    allow: o.get("allow")?.as_str()?.parse().ok()?,
                                    deny: o.get("deny")?.as_str()?.parse().ok()?,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                match api.set_overwrites(&token, *id, &ows, &reason).await {
                    Ok(_) => rep.restored_channels += 1,
                    Err(CallError::Gone(_)) => {}
                    Err(_) => rep.failed.push(FailedObject {
                        kind: "channel".into(),
                        id: *id,
                    }),
                }
            }
        }

        if opts.remove_created {
            // Children before parents: threads, channels, categories; then roles.
            let objs: Vec<(i64, String)> = sqlx::query_as(
                "SELECT discord_channel_id, kind FROM bridge_channel_maps WHERE guild_id = $1 AND origin = 'sr' \
                 ORDER BY CASE kind WHEN 'thread' THEN 0 WHEN 'channel' THEN 1 ELSE 2 END, id",
            )
            .bind(guild)
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();
            for (id, _kind) in objs {
                match api.delete_channel(&token, id, &reason).await {
                    Ok(()) => rep.channels += 1,
                    Err(CallError::Gone(_)) => {}
                    Err(_) => rep.failed.push(FailedObject {
                        kind: "channel".into(),
                        id,
                    }),
                }
            }
            let roles: Vec<i64> = sqlx::query_scalar(
                "SELECT discord_role_id FROM bridge_role_maps WHERE guild_id = $1 AND origin = 'sr' AND NOT is_everyone",
            )
            .bind(guild)
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();
            for id in roles {
                match api.delete_role(&token, dg, id, &reason).await {
                    Ok(()) => rep.roles += 1,
                    Err(CallError::Gone(_)) => {}
                    Err(_) => rep.failed.push(FailedObject {
                        kind: "role".into(),
                        id,
                    }),
                }
            }
        }
        rep
    }
}
