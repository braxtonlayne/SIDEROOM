//! Whole-mount delisting (BYOS-LEG-001, note 4). Staff with MANAGE_TAKEDOWNS may delist a member's bucket: nothing of
//! it is served to anyone (`sr_storage::scopes::access` answers `Delisted`), the bucket and its objects stay the
//! member's, and the owner is told why. Relisting puts the mount back to the status it had. A platform ban or a
//! suspension needs no call here: `ScopeFacts::owner_active` already refuses every mount of an inactive account,
//! and lifting the ban restores them. Single objects go through the takedown register (item kind `storage_object`).

use serde_json::json;
use sqlx::Row;
use sr_admin::AccessKind;
use sr_core::PlatformPermissions as P;
use sr_db::storage_legal::{self, MountLegal};

use crate::error::{ModError, ModResult};
use crate::model::Who;
use crate::rules;
use crate::service::{Moderation, log_staff, require_staff};

impl Moderation {
    /// The account to tell about a mount: its owner, or the owner of the guild that owns it.
    async fn mount_owner_account(&self, m: &MountLegal) -> ModResult<Option<i64>> {
        Ok(match m.owner_kind.as_str() {
            "user" => Some(m.owner_id),
            "guild" => self.guild_owner_account(m.owner_id).await?,
            _ => None,
        })
    }

    async fn guild_owner_account(&self, guild_id: i64) -> ModResult<Option<i64>> {
        Ok(sqlx::query("SELECT owner_id FROM guilds WHERE id = $1")
            .bind(guild_id)
            .fetch_optional(&self.pool)
            .await?
            .map(|r| r.try_get::<i64, _>("owner_id"))
            .transpose()?)
    }

    /// The contact of a server's bucket: the owner of the guild that owns mount `mount_id`; `None` for a member's
    /// own bucket (its owner is already the subject) or an unknown mount. P28-LEG-1 tells this account about a
    /// takedown in the bucket when the strike went to the uploader.
    pub(crate) async fn guild_mount_contact(&self, mount_id: i64) -> ModResult<Option<i64>> {
        let guild: Option<i64> = sqlx::query_scalar(
            "SELECT owner_id FROM storage_mounts WHERE id = $1 AND owner_kind = 'guild'",
        )
        .bind(mount_id)
        .fetch_optional(&self.pool)
        .await?;
        match guild {
            Some(g) => self.guild_owner_account(g).await,
            None => Ok(None),
        }
    }

    /// Delist a whole mount. The reason is required, goes in the access log and is told to the owner.
    pub async fn delist_mount(
        &self,
        who: &Who<'_>,
        mount_id: i64,
        reason: Option<&str>,
    ) -> ModResult<MountLegal> {
        require_staff(who, P::MANAGE_TAKEDOWNS, "MANAGE_TAKEDOWNS")?;
        let reason = rules::reason(reason)?;
        let mut tx = self.pool.begin().await?;
        let m = storage_legal::delist_mount(&mut tx, mount_id, who.user_id(), &reason)
            .await
            .map_err(|e| ModError::Storage(e.to_string()))?
            .ok_or(ModError::NotFound("storage mount"))?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "storage.mount.delist",
            ("storage_mount", mount_id),
            &reason,
            json!({ "owner_kind": m.owner_kind, "owner_id": m.owner_id.to_string() }),
        )
        .await?;
        tx.commit().await?;
        if let Some(to) = self.mount_owner_account(&m).await? {
            let _ = self
                .dm_subject(
                    to,
                    &format!(
                        "Your storage connection \"{}\" was delisted by the staff, so nothing in it is shown in SIDE ROOM for now.\n\nWhy: {reason}\n\nYour bucket and everything in it are untouched; we cannot and do not delete them. Reply to the operator if you think this is a mistake.",
                        m.label
                    ),
                )
                .await;
        }
        Ok(m)
    }

    /// Relist a delisted mount (it returns to the status it had).
    pub async fn relist_mount(
        &self,
        who: &Who<'_>,
        mount_id: i64,
        reason: Option<&str>,
    ) -> ModResult<MountLegal> {
        require_staff(who, P::MANAGE_TAKEDOWNS, "MANAGE_TAKEDOWNS")?;
        let reason = rules::reason(reason)?;
        let mut tx = self.pool.begin().await?;
        let m = storage_legal::relist_mount(&mut tx, mount_id)
            .await
            .map_err(|e| ModError::Storage(e.to_string()))?
            .ok_or_else(|| ModError::Conflict("That storage connection is not delisted".into()))?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "storage.mount.relist",
            ("storage_mount", mount_id),
            &reason,
            json!({ "owner_kind": m.owner_kind, "owner_id": m.owner_id.to_string() }),
        )
        .await?;
        tx.commit().await?;
        if let Some(to) = self.mount_owner_account(&m).await? {
            let _ = self
                .dm_subject(
                    to,
                    &format!("Your storage connection \"{}\" is listed again.", m.label),
                )
                .await;
        }
        Ok(m)
    }

    /// Admin > Storage: every live mount (see [`storage_legal::admin_list`]).
    pub async fn list_mounts(
        &self,
        who: &Who<'_>,
        owner: Option<(&str, i64)>,
        status: Option<&str>,
        limit: i64,
        before: Option<i64>,
    ) -> ModResult<Vec<storage_legal::AdminMountRow>> {
        require_staff(who, P::MANAGE_TAKEDOWNS, "MANAGE_TAKEDOWNS")?;
        storage_legal::admin_list(&self.pool, owner, status, limit, before)
            .await
            .map_err(|e| ModError::Storage(e.to_string()))
    }
}
