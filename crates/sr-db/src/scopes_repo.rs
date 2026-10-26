//! Postgres side of `sr_storage::scopes` (BYOS-SCP-001, migration 1410): the [`ScopeFacts`] the decision
//! reads, the Friend Shelf share rows, the guild storage grants, the shelves summary and the unfriend hook.

use std::collections::BTreeMap;

use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Row, Transaction};
use sr_storage::mounts::MountOwner;
use sr_storage::scopes::{FactsError, Grant, GrantSet, GuildStanding, Kind, ScopeFacts, Share};

/// `ADMINISTRATOR` in `roles.permissions` (`sr_core::Permissions`).
const ADMINISTRATOR: i64 = 1 << 3;
/// `MANAGE_GUILD`.
const MANAGE_GUILD: i64 = 1 << 5;

fn fe(e: sqlx::Error) -> FactsError {
    FactsError(e.to_string())
}

/// Postgres-backed scope facts and share / grant rows.
#[derive(Clone)]
pub struct PgScopeFacts {
    pool: PgPool,
}

impl PgScopeFacts {
    /// Wrap a pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Set (insert or replace) the share of `mount_id` for `friend`. A share with every kind off is deleted.
    /// The caller has checked that the mount is the caller's and that the two are mutual friends.
    pub async fn set_share(
        &self,
        mount_id: i64,
        friend: i64,
        share: Share,
    ) -> Result<(), FactsError> {
        if !share.any() {
            return self.delete_share(mount_id, friend).await.map(|_| ());
        }
        sqlx::query(
            "INSERT INTO storage_mount_shares (mount_id, friend_id, cinema, library, music) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (mount_id, friend_id) DO UPDATE SET cinema = EXCLUDED.cinema, \
               library = EXCLUDED.library, music = EXCLUDED.music, updated_at = now()",
        )
        .bind(mount_id)
        .bind(friend)
        .bind(share.cinema)
        .bind(share.library)
        .bind(share.music)
        .execute(&self.pool)
        .await
        .map_err(fe)?;
        Ok(())
    }

    /// Remove a share; whether there was one.
    pub async fn delete_share(&self, mount_id: i64, friend: i64) -> Result<bool, FactsError> {
        let r =
            sqlx::query("DELETE FROM storage_mount_shares WHERE mount_id = $1 AND friend_id = $2")
                .bind(mount_id)
                .bind(friend)
                .execute(&self.pool)
                .await
                .map_err(fe)?;
        Ok(r.rows_affected() > 0)
    }

    /// The shares of one mount, by friend id.
    pub async fn shares_of_mount(&self, mount_id: i64) -> Result<Vec<(i64, Share)>, FactsError> {
        let rows = sqlx::query(
            "SELECT friend_id, cinema, library, music FROM storage_mount_shares \
             WHERE mount_id = $1 ORDER BY friend_id",
        )
        .bind(mount_id)
        .fetch_all(&self.pool)
        .await
        .map_err(fe)?;
        rows.iter()
            .map(|r| Ok((r.try_get("friend_id").map_err(fe)?, share_of(r)?)))
            .collect()
    }

    /// What friends share with `viewer`: live, active-owner mounts of mutual friends with a share row.
    pub async fn shares_to(&self, viewer: i64) -> Result<Vec<SharedToMe>, FactsError> {
        let rows = sqlx::query(
            "SELECT m.id AS mount_id, m.label, m.owner_id, s.cinema, s.library, s.music \
             FROM storage_mount_shares s \
             JOIN storage_mounts m ON m.id = s.mount_id AND m.deleted_at IS NULL \
                  AND m.owner_kind = 'user' AND m.status <> 'disabled' \
             JOIN accounts a ON a.id = m.owner_id AND a.state = 'active' \
             WHERE s.friend_id = $1 AND (s.cinema OR s.library OR s.music) \
               AND EXISTS (SELECT 1 FROM relationships r1 WHERE r1.user_id = m.owner_id \
                           AND r1.target_id = $1 AND r1.type = 1) \
               AND EXISTS (SELECT 1 FROM relationships r2 WHERE r2.user_id = $1 \
                           AND r2.target_id = m.owner_id AND r2.type = 1) \
             ORDER BY m.owner_id, m.id",
        )
        .bind(viewer)
        .fetch_all(&self.pool)
        .await
        .map_err(fe)?;
        rows.iter()
            .map(|r| {
                Ok(SharedToMe {
                    mount_id: r.try_get("mount_id").map_err(fe)?,
                    owner_id: r.try_get("owner_id").map_err(fe)?,
                    label: r.try_get("label").map_err(fe)?,
                    share: share_of(r)?,
                })
            })
            .collect()
    }

    /// Replace a role's grants (the role payload's `sr_storage_grants`).
    pub async fn set_role_grants(
        &self,
        guild_id: i64,
        role_id: i64,
        grants: &GrantSet,
    ) -> Result<(), FactsError> {
        let mut tx = self.pool.begin().await.map_err(fe)?;
        sqlx::query("DELETE FROM guild_storage_grants WHERE guild_id = $1 AND role_id = $2")
            .bind(guild_id)
            .bind(role_id)
            .execute(&mut *tx)
            .await
            .map_err(fe)?;
        for g in Grant::ALL.into_iter().filter(|g| grants.has(*g)) {
            sqlx::query(
                "INSERT INTO guild_storage_grants (guild_id, role_id, grant_name) VALUES ($1, $2, $3)",
            )
            .bind(guild_id)
            .bind(role_id)
            .bind(g.as_str())
            .execute(&mut *tx)
            .await
            .map_err(fe)?;
        }
        tx.commit().await.map_err(fe)
    }

    /// Every role's grants in a guild (roles without grants are absent).
    pub async fn grants_of_guild(
        &self,
        guild_id: i64,
    ) -> Result<BTreeMap<i64, GrantSet>, FactsError> {
        let rows = sqlx::query(
            "SELECT role_id, grant_name FROM guild_storage_grants WHERE guild_id = $1 ORDER BY role_id",
        )
        .bind(guild_id)
        .fetch_all(&self.pool)
        .await
        .map_err(fe)?;
        let mut out: BTreeMap<i64, GrantSet> = BTreeMap::new();
        for r in &rows {
            let name: String = r.try_get("grant_name").map_err(fe)?;
            if let Some(g) = Grant::parse(&name) {
                out.entry(r.try_get("role_id").map_err(fe)?)
                    .or_default()
                    .insert(g);
            }
        }
        Ok(out)
    }

    /// How many shelves of each kind `viewer` reaches (the nav modes read this): their own live mounts count
    /// for every kind, plus friends' shares, plus guild shelves their grants open.
    pub async fn shelves_summary(&self, viewer: i64) -> Result<ShelvesSummary, FactsError> {
        let own: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM storage_mounts WHERE owner_kind = 'user' AND owner_id = $1 \
             AND deleted_at IS NULL AND status <> 'disabled'",
        )
        .bind(viewer)
        .fetch_one(&self.pool)
        .await
        .map_err(fe)?;
        let mut sum = ShelvesSummary {
            cinema: own,
            library: own,
            music: own,
        };
        for s in self.shares_to(viewer).await? {
            sum.cinema += i64::from(s.share.cinema);
            sum.library += i64::from(s.share.library);
            sum.music += i64::from(s.share.music);
        }
        let guilds = sqlx::query(
            "SELECT m.guild_id, count(x.id) AS shelves FROM guild_members m \
             JOIN guilds g ON g.id = m.guild_id \
             JOIN accounts a ON a.id = g.owner_id AND a.state = 'active' \
             JOIN storage_mounts x ON x.owner_kind = 'guild' AND x.owner_id = m.guild_id \
                  AND x.deleted_at IS NULL AND x.status <> 'disabled' \
             WHERE m.user_id = $1 AND NOT m.pending GROUP BY m.guild_id",
        )
        .bind(viewer)
        .fetch_all(&self.pool)
        .await
        .map_err(fe)?;
        for r in &guilds {
            let gid: i64 = r.try_get("guild_id").map_err(fe)?;
            let n: i64 = r.try_get("shelves").map_err(fe)?;
            let grants = sr_storage::scopes::guild_grants(self, gid, viewer).await?;
            sum.cinema += n * i64::from(grants.can_view(Some(Kind::Cinema)));
            sum.library += n * i64::from(grants.can_view(Some(Kind::Library)));
            sum.music += n * i64::from(grants.can_view(Some(Kind::Music)));
        }
        Ok(sum)
    }
}

fn share_of(r: &sqlx::postgres::PgRow) -> Result<Share, FactsError> {
    Ok(Share {
        cinema: r.try_get("cinema").map_err(fe)?,
        library: r.try_get("library").map_err(fe)?,
        music: r.try_get("music").map_err(fe)?,
    })
}

/// One row of `GET /users/@me/storage/shares`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedToMe {
    /// The shared mount.
    pub mount_id: i64,
    /// The friend who owns it.
    pub owner_id: i64,
    /// The mount's label.
    pub label: String,
    /// What is shared.
    pub share: Share,
}

/// Counts for the nav modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShelvesSummary {
    /// Shelves reachable for cinema.
    pub cinema: i64,
    /// For library.
    pub library: i64,
    /// For music.
    pub music: i64,
}

/// The unfriend hook: delete every share between two accounts, in either direction. Call it inside the
/// transaction that ends (or downgrades) the friendship.
pub async fn revoke_shares_between(
    tx: &mut Transaction<'_, Postgres>,
    a: i64,
    b: i64,
) -> Result<u64, sqlx::Error> {
    let r = sqlx::query(
        "DELETE FROM storage_mount_shares s USING storage_mounts m \
         WHERE s.mount_id = m.id AND m.owner_kind = 'user' \
           AND ((m.owner_id = $1 AND s.friend_id = $2) OR (m.owner_id = $2 AND s.friend_id = $1))",
    )
    .bind(a)
    .bind(b)
    .execute(&mut **tx)
    .await?;
    Ok(r.rows_affected())
}

#[async_trait]
impl ScopeFacts for PgScopeFacts {
    async fn owner_active(&self, owner: MountOwner) -> Result<bool, FactsError> {
        let sql = match owner {
            MountOwner::Platform => return Ok(true),
            MountOwner::User(_) => {
                "SELECT EXISTS (SELECT 1 FROM accounts WHERE id = $1 AND state = 'active')"
            }
            MountOwner::Guild(_) => {
                "SELECT EXISTS (SELECT 1 FROM guilds g JOIN accounts a ON a.id = g.owner_id \
                 WHERE g.id = $1 AND a.state = 'active')"
            }
        };
        sqlx::query_scalar(sql)
            .bind(owner.id())
            .fetch_one(&self.pool)
            .await
            .map_err(fe)
    }

    async fn are_friends(&self, a: i64, b: i64) -> Result<bool, FactsError> {
        sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM relationships WHERE type = 1 \
               AND ((user_id = $1 AND target_id = $2) OR (user_id = $2 AND target_id = $1))) = 2",
        )
        .bind(a)
        .bind(b)
        .fetch_one(&self.pool)
        .await
        .map_err(fe)
    }

    async fn share(&self, mount_id: i64, friend: i64) -> Result<Option<Share>, FactsError> {
        let row = sqlx::query(
            "SELECT cinema, library, music FROM storage_mount_shares WHERE mount_id = $1 AND friend_id = $2",
        )
        .bind(mount_id)
        .bind(friend)
        .fetch_optional(&self.pool)
        .await
        .map_err(fe)?;
        row.as_ref().map(share_of).transpose()
    }

    async fn guild_standing(
        &self,
        guild_id: i64,
        user_id: i64,
    ) -> Result<Option<GuildStanding>, FactsError> {
        // The member row, the guild owner, and the union of the @everyone role (id = guild id) and the
        // member's roles.
        let member = sqlx::query(
            "SELECT g.owner_id = m.user_id AS is_owner FROM guild_members m \
             JOIN guilds g ON g.id = m.guild_id \
             WHERE m.guild_id = $1 AND m.user_id = $2 AND NOT m.pending",
        )
        .bind(guild_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(fe)?;
        let Some(member) = member else {
            return Ok(None);
        };
        let roles = sqlx::query(
            "SELECT r.id, r.permissions FROM roles r WHERE r.guild_id = $1 AND (r.id = $1 OR r.id IN \
             (SELECT role_id FROM member_roles WHERE guild_id = $1 AND user_id = $2))",
        )
        .bind(guild_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(fe)?;
        let mut perms = 0i64;
        let mut role_ids = Vec::with_capacity(roles.len());
        for r in &roles {
            perms |= r.try_get::<i64, _>("permissions").map_err(fe)?;
            role_ids.push(r.try_get::<i64, _>("id").map_err(fe)?);
        }
        Ok(Some(GuildStanding {
            is_owner: member.try_get("is_owner").map_err(fe)?,
            administrator: perms & ADMINISTRATOR != 0,
            manage_guild: perms & MANAGE_GUILD != 0,
            role_ids,
        }))
    }

    async fn role_grants(&self, guild_id: i64, role_ids: &[i64]) -> Result<GrantSet, FactsError> {
        let names: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT grant_name FROM guild_storage_grants WHERE guild_id = $1 AND role_id = ANY($2)",
        )
        .bind(guild_id)
        .bind(role_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(fe)?;
        Ok(GrantSet::of(names.iter().filter_map(|n| Grant::parse(n))))
    }

    async fn attached_guilds(&self, _mount_id: i64) -> Result<Vec<i64>, FactsError> {
        // The attach table arrives with BYOS-INS-001 (byos-instance); until then no guild has the instance
        // mount as a shelf.
        Ok(Vec::new())
    }

    async fn delisted(&self, _mount_id: i64, _keys: &[String]) -> Result<Vec<String>, FactsError> {
        // Delisting arrives with BYOS-LEG-001 (wave 28); until then nothing is delisted.
        Ok(Vec::new())
    }
}
