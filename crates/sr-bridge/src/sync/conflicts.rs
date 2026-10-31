//! The conflict log (BRG-SYN-002; migration 1754; design sections 5.3, 11.4 and 13).
//!
//! When Discord differs from SIDE ROOM in a SIDE ROOM-authoritative field, the bridge puts SIDE ROOM's value back
//! and records a row here. The row doubles as the proposal: "Apply on SIDE ROOM" opens the editor prefilled with
//! what Discord attempted, and only an ordinary SIDE ROOM edit by the person who clicks ever makes it a SIDE ROOM
//! permission. Nothing in this table is read by authorization code.
//!
//! Coalescing happens before the write: one open row per (kind, object, field); a repeat inside
//! [`COALESCE_SECS`] bumps `count` and `last_at`. The audit row (222 for permission and overwrite reverts, 223
//! for the other kinds) is written once, when the row is created; the audit table is append-only and is never
//! edited, so the running count lives here.

use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{Row, postgres::PgRow};

use crate::error::{BridgeError, BridgeResult};
use crate::service::{BridgeService, SYSTEM_USER_ID};

/// Window inside which a repeat of the same conflict only bumps the open row.
pub const COALESCE_SECS: i64 = 600;
/// Closed rows are pruned after this many days.
pub const RETENTION_DAYS: i64 = 90;

/// Audit action numbers of the bridge (`sr_chat::settings::audit::AuditAction`, equality pinned by a test in
/// `sr-api`).
pub const AUDIT_MODE_UPDATE: i16 = 220;
pub const AUDIT_SYNC_RUN: i16 = 221;
pub const AUDIT_PERMISSION_REVERTED: i16 = 222;
pub const AUDIT_CONFLICT: i16 = 223;
pub const AUDIT_OBJECT_REMOVED: i16 = 224;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    PermissionReverted,
    OverwriteTrimmed,
    StructureConflict,
    RoleDeletionRefused,
    DeletionHeld,
    Frozen,
    DeadOp,
    Cap,
}

impl ConflictKind {
    pub const ALL: [ConflictKind; 8] = [
        Self::PermissionReverted,
        Self::OverwriteTrimmed,
        Self::StructureConflict,
        Self::RoleDeletionRefused,
        Self::DeletionHeld,
        Self::Frozen,
        Self::DeadOp,
        Self::Cap,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PermissionReverted => "permission_reverted",
            Self::OverwriteTrimmed => "overwrite_trimmed",
            Self::StructureConflict => "structure_conflict",
            Self::RoleDeletionRefused => "role_deletion_refused",
            Self::DeletionHeld => "deletion_held",
            Self::Frozen => "frozen",
            Self::DeadOp => "dead_op",
            Self::Cap => "cap",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }

    /// The audit action a new row of this kind writes.
    pub const fn audit_action(self) -> i16 {
        match self {
            Self::PermissionReverted | Self::OverwriteTrimmed => AUDIT_PERMISSION_REVERTED,
            _ => AUDIT_CONFLICT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    Role,
    Channel,
    Category,
    Thread,
    Overwrite,
    Op,
    Guild,
}

impl ObjectKind {
    pub const ALL: [ObjectKind; 7] = [
        Self::Role,
        Self::Channel,
        Self::Category,
        Self::Thread,
        Self::Overwrite,
        Self::Op,
        Self::Guild,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Role => "role",
            Self::Channel => "channel",
            Self::Category => "category",
            Self::Thread => "thread",
            Self::Overwrite => "overwrite",
            Self::Op => "op",
            Self::Guild => "guild",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

/// How a person closes a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    AppliedOnSideRoom,
    TookDiscord,
    Restored,
    DeletedNow,
    Unfrozen,
    Retried,
    Dismissed,
}

impl Resolution {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AppliedOnSideRoom => "applied_on_sideroom",
            Self::TookDiscord => "took_discord",
            Self::Restored => "restored",
            Self::DeletedNow => "deleted_now",
            Self::Unfrozen => "unfrozen",
            Self::Retried => "retried",
            Self::Dismissed => "dismissed",
        }
    }
}

/// A conflict to record.
#[derive(Debug, Clone)]
pub struct NewConflict {
    pub guild: i64,
    pub kind: ConflictKind,
    pub object_kind: ObjectKind,
    pub sr_object_id: Option<i64>,
    pub discord_object_id: Option<i64>,
    pub target_id: Option<i64>,
    pub field: String,
    pub discord_value: Option<Value>,
    pub sr_value: Option<Value>,
    pub discord_actor_id: Option<i64>,
    pub discord_actor_name: Option<String>,
    pub op_id: Option<i64>,
}

impl NewConflict {
    pub fn new(guild: i64, kind: ConflictKind, object_kind: ObjectKind, field: &str) -> Self {
        Self {
            guild,
            kind,
            object_kind,
            sr_object_id: None,
            discord_object_id: None,
            target_id: None,
            field: field.to_owned(),
            discord_value: None,
            sr_value: None,
            discord_actor_id: None,
            discord_actor_name: None,
            op_id: None,
        }
    }
}

/// One row of the log.
#[derive(Debug, Clone, Serialize)]
pub struct Conflict {
    pub id: i64,
    pub guild_id: i64,
    pub kind: String,
    pub object_kind: String,
    pub sr_object_id: Option<i64>,
    pub discord_object_id: Option<i64>,
    pub target_id: Option<i64>,
    pub field: String,
    pub discord_value: Option<Value>,
    pub sr_value: Option<Value>,
    pub discord_actor_id: Option<i64>,
    pub discord_actor_name: Option<String>,
    pub op_id: Option<i64>,
    pub count: i32,
    pub status: String,
    pub resolution: Option<String>,
    pub resolved_by: Option<i64>,
    pub resolved_at_ms: Option<i64>,
    pub first_at_ms: i64,
    pub last_at_ms: i64,
}

const COLS: &str = "id, guild_id, kind, object_kind, sr_object_id, discord_object_id, target_id, field, \
     discord_value, sr_value, discord_actor_id, discord_actor_name, op_id, count, status, resolution, \
     resolved_by, (extract(epoch from resolved_at) * 1000)::bigint AS resolved_ms, \
     (extract(epoch from first_at) * 1000)::bigint AS first_ms, \
     (extract(epoch from last_at) * 1000)::bigint AS last_ms";

fn from_row(r: &PgRow) -> Conflict {
    Conflict {
        id: r.get("id"),
        guild_id: r.get("guild_id"),
        kind: r.get("kind"),
        object_kind: r.get("object_kind"),
        sr_object_id: r.get("sr_object_id"),
        discord_object_id: r.get("discord_object_id"),
        target_id: r.get("target_id"),
        field: r.get("field"),
        discord_value: r.get("discord_value"),
        sr_value: r.get("sr_value"),
        discord_actor_id: r.get("discord_actor_id"),
        discord_actor_name: r.get("discord_actor_name"),
        op_id: r.get("op_id"),
        count: r.get("count"),
        status: r.get("status"),
        resolution: r.get("resolution"),
        resolved_by: r.get("resolved_by"),
        resolved_at_ms: r.get("resolved_ms"),
        first_at_ms: r.get("first_ms"),
        last_at_ms: r.get("last_ms"),
    }
}

/// What recording did.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub conflict: Conflict,
    /// A new row (and a new audit row); false when an open row was bumped.
    pub created: bool,
}

/// Filters of the list.
#[derive(Debug, Clone, Default)]
pub struct ConflictFilter {
    pub kind: Option<ConflictKind>,
    /// `open`, `resolved`, `dismissed`, `superseded`; `None` lists every status.
    pub status: Option<String>,
    pub object_kind: Option<ObjectKind>,
    pub sr_object_id: Option<i64>,
    /// Page: rows with an id below this.
    pub before: Option<i64>,
    pub limit: i64,
}

/// One bridge audit row (the append-only `audit_log_entries`), for the callers that write 220, 221 and 224.
pub struct BridgeAudit<'a> {
    pub id: i64,
    pub guild: i64,
    pub actor: i64,
    pub action: i16,
    pub target: Option<i64>,
    pub changes: Value,
    pub options: Value,
    pub reason: Option<&'a str>,
}

/// Write a bridge audit row on `conn`. `reason` is clipped to the column's 512 characters.
pub async fn write_audit(conn: &mut sqlx::PgConnection, a: BridgeAudit<'_>) -> BridgeResult<()> {
    let reason: Option<String> = a.reason.map(|r| r.chars().take(512).collect());
    sqlx::query(
        "INSERT INTO audit_log_entries (id, guild_id, actor_id, target_id, action, changes, options, reason) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(a.id)
    .bind(a.guild)
    .bind(a.actor)
    .bind(a.target)
    .bind(a.action)
    .bind(a.changes)
    .bind(a.options)
    .bind(reason)
    .execute(conn)
    .await?;
    Ok(())
}

impl BridgeService {
    /// Record a conflict, coalescing with an open row of the same key younger than [`COALESCE_SECS`]. An older
    /// open row is marked `superseded` (it stays in the log) and a new one opens.
    pub async fn record_conflict(&self, c: NewConflict) -> BridgeResult<Recorded> {
        let mut last = None;
        for _ in 0..3 {
            match self.record_once(&c).await {
                Ok(r) => return Ok(r),
                // Two writers opened the same key at once: the unique index refused one; look again.
                Err(BridgeError::Storage(m)) if m.contains("bridge_conflicts_open_key") => {
                    last = Some(BridgeError::Storage(m));
                }
                Err(e) => return Err(e),
            }
        }
        Err(last.unwrap_or(BridgeError::Storage("conflict not recorded".into())))
    }

    async fn record_once(&self, c: &NewConflict) -> BridgeResult<Recorded> {
        let mut tx = self.pool.begin().await?;
        let key = "guild_id = $1 AND kind = $2 AND object_kind = $3 AND status = 'open' AND field = $4 \
                   AND COALESCE(sr_object_id, 0) = COALESCE($5, 0) \
                   AND COALESCE(discord_object_id, 0) = COALESCE($6, 0) \
                   AND COALESCE(target_id, 0) = COALESCE($7, 0)";
        let open: Option<(i64, bool)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT id, last_at > now() - make_interval(secs => $8) FROM bridge_conflicts WHERE {key} FOR UPDATE"
        )))
        .bind(c.guild)
        .bind(c.kind.as_str())
        .bind(c.object_kind.as_str())
        .bind(&c.field)
        .bind(c.sr_object_id)
        .bind(c.discord_object_id)
        .bind(c.target_id)
        .bind(COALESCE_SECS as f64)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((id, fresh)) = open {
            if fresh {
                let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                    "UPDATE bridge_conflicts SET count = count + 1, last_at = now(), \
                     discord_value = COALESCE($2, discord_value), sr_value = COALESCE($3, sr_value), \
                     discord_actor_id = COALESCE($4, discord_actor_id), \
                     discord_actor_name = COALESCE($5, discord_actor_name), op_id = COALESCE($6, op_id) \
                     WHERE id = $1 RETURNING {COLS}"
                )))
                .bind(id)
                .bind(&c.discord_value)
                .bind(&c.sr_value)
                .bind(c.discord_actor_id)
                .bind(&c.discord_actor_name)
                .bind(c.op_id)
                .fetch_one(&mut *tx)
                .await?;
                tx.commit().await?;
                return Ok(Recorded {
                    conflict: from_row(&row),
                    created: false,
                });
            }
            sqlx::query(
                "UPDATE bridge_conflicts SET status = 'superseded', resolution = 'superseded', \
                 resolved_at = now() WHERE id = $1",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }
        let id = self.next_id();
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO bridge_conflicts (id, guild_id, kind, object_kind, sr_object_id, discord_object_id, \
             target_id, field, discord_value, sr_value, discord_actor_id, discord_actor_name, op_id) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) RETURNING {COLS}"
        )))
        .bind(id)
        .bind(c.guild)
        .bind(c.kind.as_str())
        .bind(c.object_kind.as_str())
        .bind(c.sr_object_id)
        .bind(c.discord_object_id)
        .bind(c.target_id)
        .bind(&c.field)
        .bind(&c.discord_value)
        .bind(&c.sr_value)
        .bind(c.discord_actor_id)
        .bind(c.discord_actor_name.as_deref().map(|n| n.chars().take(100).collect::<String>()))
        .bind(c.op_id)
        .fetch_one(&mut *tx)
        .await?;
        let who = c.discord_actor_name.as_deref().unwrap_or("unknown");
        write_audit(
            &mut tx,
            BridgeAudit {
                id: self.next_id(),
                guild: c.guild,
                actor: SYSTEM_USER_ID,
                action: c.kind.audit_action(),
                target: c.sr_object_id,
                changes: json!([{
                    "key": c.field,
                    "old_value": c.sr_value,
                    "new_value": c.discord_value,
                }]),
                options: json!({
                    "sr_bridge": {
                        "origin": "discord",
                        "discord_user_id": c.discord_actor_id.map(|v| v.to_string()),
                        "op_id": c.op_id.map(|v| v.to_string()),
                    },
                    "conflict_id": id.to_string(),
                    "kind": c.kind.as_str(),
                    "object_kind": c.object_kind.as_str(),
                    "count": 1,
                }),
                reason: Some(&format!("Discord bridge: {who}")),
            },
        )
        .await?;
        tx.commit().await?;
        Ok(Recorded {
            conflict: from_row(&row),
            created: true,
        })
    }

    pub async fn get_conflict(&self, guild: i64, id: i64) -> BridgeResult<Conflict> {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {COLS} FROM bridge_conflicts WHERE guild_id = $1 AND id = $2"
        )))
        .bind(guild)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(BridgeError::NotFound("conflict"))?;
        Ok(from_row(&row))
    }

    /// Newest first.
    pub async fn list_conflicts(
        &self,
        guild: i64,
        f: &ConflictFilter,
    ) -> BridgeResult<Vec<Conflict>> {
        let limit = if f.limit <= 0 { 50 } else { f.limit.min(100) };
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {COLS} FROM bridge_conflicts WHERE guild_id = $1 \
             AND ($2::text IS NULL OR kind = $2) AND ($3::text IS NULL OR status = $3) \
             AND ($4::text IS NULL OR object_kind = $4) AND ($5::bigint IS NULL OR sr_object_id = $5) \
             AND ($6::bigint IS NULL OR id < $6) ORDER BY id DESC LIMIT $7"
        )))
        .bind(guild)
        .bind(f.kind.map(ConflictKind::as_str))
        .bind(f.status.as_deref())
        .bind(f.object_kind.map(ObjectKind::as_str))
        .bind(f.sr_object_id)
        .bind(f.before)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(from_row).collect())
    }

    pub async fn open_conflict_count(&self, guild: i64) -> BridgeResult<i64> {
        Ok(sqlx::query_scalar(
            "SELECT count(*) FROM bridge_conflicts WHERE guild_id = $1 AND status = 'open'",
        )
        .bind(guild)
        .fetch_one(&self.pool)
        .await?)
    }

    /// Close an open row. `by` is the person who clicked (their SIDE ROOM edit, when there is one, already passed
    /// its own checks). A row that is not open is a conflict (409) so a double click does nothing twice.
    pub async fn resolve_conflict(
        &self,
        guild: i64,
        id: i64,
        by: i64,
        how: Resolution,
    ) -> BridgeResult<Conflict> {
        let status = if how == Resolution::Dismissed {
            "dismissed"
        } else {
            "resolved"
        };
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE bridge_conflicts SET status = $3, resolution = $4, resolved_by = $5, resolved_at = now() \
             WHERE guild_id = $1 AND id = $2 AND status = 'open' RETURNING {COLS}"
        )))
        .bind(guild)
        .bind(id)
        .bind(status)
        .bind(how.as_str())
        .bind(by)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(r) => Ok(from_row(&r)),
            None => {
                // Distinguish unknown from already closed.
                self.get_conflict(guild, id).await?;
                Err(BridgeError::Conflict(
                    "This conflict is already closed.".into(),
                ))
            }
        }
    }

    /// Delete closed rows older than [`RETENTION_DAYS`] (the retention job calls it). Returns the count.
    pub async fn prune_closed_conflicts(&self) -> BridgeResult<u64> {
        Ok(sqlx::query(
            "DELETE FROM bridge_conflicts WHERE status <> 'open' \
             AND resolved_at < now() - make_interval(days => $1)",
        )
        .bind(RETENTION_DAYS as i32)
        .execute(&self.pool)
        .await?
        .rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_round_trip_and_pick_their_audit_action() {
        for k in ConflictKind::ALL {
            assert_eq!(ConflictKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(ConflictKind::PermissionReverted.audit_action(), 222);
        assert_eq!(ConflictKind::OverwriteTrimmed.audit_action(), 222);
        assert_eq!(ConflictKind::Frozen.audit_action(), 223);
        for o in ObjectKind::ALL {
            assert_eq!(ObjectKind::parse(o.as_str()), Some(o));
        }
    }
}
