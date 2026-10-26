//! Postgres implementation of [`sr_admin::AdminRepository`].
//!
//! Every state-changing method runs in one transaction that also appends the
//! action to `platform_audit_log` and to the hash-chained `staff_access_log`
//! (serialised by an advisory lock so the chain has no forks). Tests:
//! `tests/admin_repo.rs` (needs `SR_TEST_DATABASE_URL`).

use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;
use sqlx::{PgConnection, PgPool, Row, postgres::PgRow};
use sr_admin::{
    AccessKind, AccessLogEntry, AccessLogFilter, AccountDetail, AccountQuery, AccountRef,
    AccountState, AccountSummary, AdminError, AdminRepository, AdminResult, AuditEntry,
    BanCategory, BanFilter, BanOutcome, BanRecord, ChainReport, CreditLedger, Delegation,
    DeleteRequest, FormerStaff, InviteDescendants, InviteNode, InviteQuery, InviteRow, InviteStats,
    InviteTree, JobRow, LedgerEntry, LoginEvent, PersonDetail, Reset2faReport, StaffMember,
    StaffRole, Standing, StoredSetting, SuspectPerson, SuspicionChange, SystemCounts, UnbanOutcome,
    chain::{self, ChainVerifier},
};
use sr_core::PlatformPermissions as P;

use crate::{accounts, auth_repo::IdSource, jobs, lock};

/// Advisory lock that serialises appends to the access log.
pub const ACCESS_LOG_LOCK: i64 = lock::lock_key("sr.staff_access_log");

const MS: &str = "(extract(epoch FROM {c}) * 1000)::bigint";

pub(crate) fn ms(col: &str) -> String {
    MS.replace("{c}", col)
}

pub(crate) fn db(e: sqlx::Error) -> AdminError {
    AdminError::Storage(e.to_string())
}

fn db2(e: crate::DbError) -> AdminError {
    AdminError::Storage(e.to_string())
}

pub(crate) fn is_unique(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == "23505")
}

fn now_us() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_micros()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn state_of(s: &str) -> AccountState {
    AccountState::parse(s).unwrap_or(AccountState::Deactivated)
}

fn acct_cols() -> String {
    format!(
        "a.id, a.person_id, a.username::text AS username, a.display_name, a.email::text AS email, \
         (a.email_verified_at IS NOT NULL) AS email_verified, a.state::text AS state, a.flags, \
         {} AS created_ms, {} AS changed_ms",
        ms("a.created_at"),
        ms("a.state_changed_at")
    )
}

fn acct_from(r: &PgRow) -> Result<AccountSummary, sqlx::Error> {
    Ok(AccountSummary {
        id: r.try_get("id")?,
        person_id: r.try_get("person_id")?,
        username: r.try_get("username")?,
        display_name: r.try_get("display_name")?,
        email: r.try_get("email")?,
        email_verified: r.try_get("email_verified")?,
        state: state_of(&r.try_get::<String, _>("state")?),
        flags: r.try_get("flags")?,
        created_at_ms: r.try_get("created_ms")?,
        state_changed_at_ms: r.try_get("changed_ms")?,
    })
}

fn log_from(r: &PgRow) -> Result<AccessLogEntry, sqlx::Error> {
    let prev: Vec<u8> = r.try_get("prev_hash")?;
    let hash: Vec<u8> = r.try_get("hash")?;
    Ok(AccessLogEntry {
        seq: r.try_get("seq")?,
        at_us: r.try_get("at_us")?,
        actor_id: r.try_get("actor_id")?,
        kind: AccessKind::parse(&r.try_get::<String, _>("kind")?).unwrap_or(AccessKind::Read),
        action: r.try_get("action")?,
        target_kind: r.try_get("target_kind")?,
        target_id: r.try_get("target_id")?,
        reason: r.try_get("reason")?,
        ip: r.try_get("ip")?,
        detail: r.try_get("detail")?,
        prev_hash: chain::to_hex(&prev),
        hash: chain::to_hex(&hash),
    })
}

const LOG_COLS: &str = "seq, at_us, actor_id, kind, action, target_kind, target_id, reason, ip, detail, prev_hash, hash";

/// Append one chained row on `conn`, which must be inside a transaction.
pub async fn append_in(conn: &mut PgConnection, e: &AuditEntry) -> AdminResult<AccessLogEntry> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(ACCESS_LOG_LOCK)
        .execute(&mut *conn)
        .await
        .map_err(db)?;
    let last: Option<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT seq, hash FROM staff_access_log ORDER BY seq DESC LIMIT 1")
            .fetch_optional(&mut *conn)
            .await
            .map_err(db)?;
    let (prev, seq) = match last {
        Some((seq, h)) => (
            chain::from_hex(&chain::to_hex(&h)).unwrap_or(chain::GENESIS),
            seq + 1,
        ),
        None => (chain::GENESIS, 1),
    };
    let row = chain::seal(&prev, seq, now_us(), e);
    sqlx::query(
        "INSERT INTO staff_access_log (seq, at_us, actor_id, kind, action, target_kind, target_id, \
         reason, ip, detail, prev_hash, hash) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
    )
    .bind(row.seq)
    .bind(row.at_us)
    .bind(row.actor_id)
    .bind(row.kind.as_str())
    .bind(&row.action)
    .bind(&row.target_kind)
    .bind(row.target_id)
    .bind(&row.reason)
    .bind(&row.ip)
    .bind(&row.detail)
    .bind(prev.as_slice())
    .bind(
        chain::from_hex(&row.hash)
            .unwrap_or(chain::GENESIS)
            .as_slice(),
    )
    .execute(&mut *conn)
    .await
    .map_err(db)?;
    Ok(row)
}

/// Verify the whole chain on `conn`, page by page.
pub async fn verify_chain(conn: &mut PgConnection) -> AdminResult<ChainReport> {
    let mut v = ChainVerifier::new();
    let mut after = 0i64;
    loop {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {LOG_COLS} FROM staff_access_log WHERE seq > $1 ORDER BY seq LIMIT 1000"
        )))
        .bind(after)
        .fetch_all(&mut *conn)
        .await
        .map_err(db)?;
        if rows.is_empty() {
            break;
        }
        for r in &rows {
            let e = log_from(r).map_err(db)?;
            after = e.seq;
            v.feed(&e);
        }
        if v.failed() {
            break;
        }
    }
    Ok(v.finish())
}

#[derive(Clone)]
pub struct PgAdminRepo {
    pool: PgPool,
    ids: IdSource,
}

impl PgAdminRepo {
    pub fn new(pool: PgPool, ids: IdSource) -> Self {
        Self { pool, ids }
    }

    pub(crate) fn new_id(&self) -> i64 {
        (self.ids)()
    }

    pub(crate) fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub(crate) fn next_id(&self) -> i64 {
        (self.ids)()
    }

    pub(crate) async fn audit_in(
        &self,
        conn: &mut PgConnection,
        a: &AuditEntry,
    ) -> AdminResult<()> {
        self.audit(conn, a).await
    }

    /// Platform audit row plus chained access-log row, inside the caller's transaction.
    async fn audit(&self, conn: &mut PgConnection, a: &AuditEntry) -> AdminResult<()> {
        sqlx::query(
            "INSERT INTO platform_audit_log (id, actor_id, action, target_id, note) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind((self.ids)())
        .bind(a.actor_id)
        .bind(&a.action)
        .bind(a.target_id)
        .bind(&a.reason)
        .execute(&mut *conn)
        .await
        .map_err(db)?;
        append_in(conn, a).await.map(|_| ())
    }

    async fn staff_guild(conn: &mut PgConnection) -> AdminResult<(i64, i64)> {
        sqlx::query_as("SELECT id, owner_id FROM guilds WHERE is_staff_guild")
            .fetch_optional(conn)
            .await
            .map_err(db)?
            .ok_or_else(|| {
                AdminError::Conflict("There is no Staff Server (run `sr-cli bootstrap`)".into())
            })
    }

    async fn staff_members(
        conn: &mut PgConnection,
        only: Option<i64>,
    ) -> AdminResult<Vec<StaffMember>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT a.id, a.username::text AS username, a.display_name, (g.owner_id = a.id) AS is_owner, \
                    ap.appointed_by, COALESCE({}, 0) AS appointed_ms, COALESCE(ap.reason, '') AS reason, g.id AS guild_id \
             FROM guilds g \
             JOIN accounts a ON a.state = 'active' \
             LEFT JOIN staff_appointments ap ON ap.account_id = a.id AND ap.revoked_at IS NULL \
             WHERE g.is_staff_guild AND ($1::bigint IS NULL OR a.id = $1) \
               AND (g.owner_id = a.id OR (ap.id IS NOT NULL AND EXISTS( \
                      SELECT 1 FROM guild_members gm WHERE gm.guild_id = g.id AND gm.user_id = a.id))) \
             ORDER BY a.id",
            ms("ap.appointed_at")
        )))
        .bind(only)
        .fetch_all(&mut *conn)
        .await
        .map_err(db)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let id: i64 = r.try_get("id").map_err(db)?;
            let guild: i64 = r.try_get("guild_id").map_err(db)?;
            let role_rows = sqlx::query(
                "SELECT r.id, r.name, r.position, r.platform_permissions FROM member_roles mr \
                 JOIN roles r ON r.id = mr.role_id WHERE mr.guild_id = $1 AND mr.user_id = $2 \
                 ORDER BY r.position DESC",
            )
            .bind(guild)
            .bind(id)
            .fetch_all(&mut *conn)
            .await
            .map_err(db)?;
            let roles: Vec<StaffRole> = role_rows
                .iter()
                .map(role_from)
                .collect::<Result<_, _>>()
                .map_err(db)?;
            let is_owner: bool = r.try_get("is_owner").map_err(db)?;
            let mut bits = P::from_roles(roles.iter().map(StaffRole::permissions));
            if is_owner {
                bits |= P::OWNER;
            }
            out.push(StaffMember {
                account_id: id,
                username: r.try_get("username").map_err(db)?,
                display_name: r.try_get("display_name").map_err(db)?,
                is_owner,
                roles,
                platform_permissions: bits.bits().to_string(),
                appointed_by: r.try_get("appointed_by").map_err(db)?,
                appointed_at_ms: r.try_get("appointed_ms").map_err(db)?,
                reason: r.try_get("reason").map_err(db)?,
            });
        }
        Ok(out)
    }

    async fn check_roles(conn: &mut PgConnection, guild: i64, role_ids: &[i64]) -> AdminResult<()> {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM roles WHERE guild_id = $1 AND id <> $1 AND id = ANY($2)",
        )
        .bind(guild)
        .bind(role_ids)
        .fetch_one(conn)
        .await
        .map_err(db)?;
        if n as usize != role_ids.len() {
            return Err(AdminError::NotFound("staff role"));
        }
        Ok(())
    }

    async fn account_summary(conn: &mut PgConnection, id: i64) -> AdminResult<AccountSummary> {
        let r = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM accounts a WHERE a.id = $1",
            acct_cols()
        )))
        .bind(id)
        .fetch_optional(conn)
        .await
        .map_err(db)?
        .ok_or(AdminError::NotFound("account"))?;
        acct_from(&r).map_err(db)
    }

    async fn delegations_in(
        conn: &mut PgConnection,
        only: Option<i64>,
    ) -> AdminResult<Vec<Delegation>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT d.account_id, a.username::text AS username, d.max_permissions, d.granted_by, \
                    {} AS granted_ms, d.reason \
             FROM appointment_delegations d JOIN accounts a ON a.id = d.account_id \
             WHERE d.revoked_at IS NULL AND ($1::bigint IS NULL OR d.account_id = $1) \
             ORDER BY d.id",
            ms("d.granted_at")
        )))
        .bind(only)
        .fetch_all(conn)
        .await
        .map_err(db)?;
        rows.iter()
            .map(|r| {
                let bits: i64 = r.try_get("max_permissions")?;
                Ok(Delegation {
                    account_id: r.try_get("account_id")?,
                    username: r.try_get("username")?,
                    max_permissions: (bits as u64).to_string(),
                    granted_by: r.try_get("granted_by")?,
                    granted_at_ms: r.try_get("granted_ms")?,
                    reason: r.try_get("reason")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(db)
    }

    fn node_from(r: &PgRow) -> Result<InviteNode, sqlx::Error> {
        Ok(InviteNode {
            account_id: r.try_get("id")?,
            username: r.try_get("username")?,
            state: state_of(&r.try_get::<String, _>("state")?),
            person_id: r.try_get("person_id")?,
            suspicion_level: r.try_get("suspicion")?,
            parent_id: r.try_get("parent_id")?,
            depth: r.try_get("depth")?,
        })
    }
}

fn role_from(r: &PgRow) -> Result<StaffRole, sqlx::Error> {
    let bits: i64 = r.try_get("platform_permissions")?;
    Ok(StaffRole {
        id: r.try_get("id")?,
        name: r.try_get("name")?,
        position: r.try_get("position")?,
        // Bit 63 (OWNER) is the sign bit of the stored bigint.
        platform_permissions: (bits as u64).to_string(),
    })
}

fn invite_from(r: &PgRow) -> Result<InviteRow, sqlx::Error> {
    Ok(InviteRow {
        id: r.try_get("id")?,
        code: r.try_get("code")?,
        issuer_account_id: r.try_get("issuer_account_id")?,
        state: r.try_get("state")?,
        redeemed_by_account_id: r.try_get("redeemed_by_account_id")?,
        note: r.try_get("note")?,
        emergency: r.try_get("emergency")?,
        created_at_ms: r.try_get("created_ms")?,
    })
}

fn ban_from(r: &PgRow) -> Result<BanRecord, sqlx::Error> {
    Ok(BanRecord {
        id: r.try_get("id")?,
        person_id: r.try_get("person_id")?,
        category: BanCategory::parse(&r.try_get::<String, _>("category")?)
            .unwrap_or(BanCategory::Standard),
        note: r.try_get("note")?,
        banned_by: r.try_get("banned_by")?,
        created_at_ms: r.try_get("created_ms")?,
        lifted_at_ms: r.try_get("lifted_ms")?,
        reason_category: r.try_get("reason_category")?,
    })
}

fn ban_cols() -> String {
    format!(
        "id, person_id, category::text AS category, reason_category::text AS reason_category, note, banned_by, {} AS created_ms, {} AS lifted_ms",
        ms("created_at"),
        ms("lifted_at")
    )
}

fn job_from(r: &PgRow) -> Result<JobRow, sqlx::Error> {
    Ok(JobRow {
        id: r.try_get("id")?,
        kind: r.try_get("kind")?,
        state: r.try_get("state")?,
        attempts: r.try_get("attempts")?,
        max_attempts: r.try_get("max_attempts")?,
        last_error: r.try_get("last_error")?,
        run_at_ms: r.try_get("run_ms")?,
        finished_at_ms: r.try_get("finished_ms")?,
    })
}

#[async_trait]
impl AdminRepository for PgAdminRepo {
    // ---- staff ----------------------------------------------------------

    async fn standing(&self, user_id: i64) -> AdminResult<Option<Standing>> {
        let row: Option<(bool, bool, bool, i64, Option<i64>)> = sqlx::query_as(
            "SELECT (g.owner_id = $1), \
                    EXISTS(SELECT 1 FROM staff_appointments ap WHERE ap.account_id = $1 AND ap.revoked_at IS NULL), \
                    EXISTS(SELECT 1 FROM guild_members gm WHERE gm.guild_id = g.id AND gm.user_id = $1), \
                    COALESCE((SELECT bit_or(r.platform_permissions) FROM member_roles mr \
                              JOIN roles r ON r.id = mr.role_id \
                              WHERE mr.guild_id = g.id AND mr.user_id = $1 AND r.guild_id = g.id), 0)::bigint, \
                    (SELECT d.max_permissions FROM appointment_delegations d \
                      WHERE d.account_id = $1 AND d.revoked_at IS NULL) \
             FROM guilds g JOIN accounts a ON a.id = $1 AND a.state = 'active' \
             WHERE g.is_staff_guild",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        Ok(match row {
            Some((true, _, _, _, _)) => Some(Standing {
                permissions: P::OWNER,
                is_owner: true,
                grant_ceiling: None,
            }),
            Some((false, true, true, bits, delegation)) => {
                let mut permissions = P::from_bits_truncate(bits as u64);
                // A delegate may appoint, within the ceiling the Owner set (D-132).
                let grant_ceiling = delegation.map(|d| P::from_bits_truncate(d as u64));
                if grant_ceiling.is_some() {
                    permissions |= P::APPOINT_STAFF;
                }
                Some(Standing {
                    permissions,
                    is_owner: false,
                    grant_ceiling,
                })
            }
            _ => None,
        })
    }

    async fn staff_roles(&self) -> AdminResult<Vec<StaffRole>> {
        let rows = sqlx::query(
            "SELECT r.id, r.name, r.position, r.platform_permissions FROM roles r \
             JOIN guilds g ON g.id = r.guild_id AND g.is_staff_guild \
             WHERE r.id <> g.id ORDER BY r.position, r.id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(role_from)
            .collect::<Result<_, _>>()
            .map_err(db)
    }

    async fn list_staff(&self) -> AdminResult<Vec<StaffMember>> {
        let mut c = self.pool.acquire().await.map_err(db)?;
        Self::staff_members(&mut c, None).await
    }

    async fn former_staff(&self, limit: i64) -> AdminResult<Vec<FormerStaff>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT a.id, a.username::text AS username, ap.revoked_reason,                     COALESCE(ap.revoke_reason, '') AS note, {} AS revoked_ms              FROM staff_appointments ap JOIN accounts a ON a.id = ap.account_id              WHERE ap.revoked_at IS NOT NULL ORDER BY ap.revoked_at DESC LIMIT $1",
            ms("ap.revoked_at")
        )))
        .bind(limit.clamp(1, 500))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(|r| {
                Ok(FormerStaff {
                    account_id: r.try_get("id").map_err(db)?,
                    username: r.try_get("username").map_err(db)?,
                    revoked_reason: r
                        .try_get::<Option<String>, _>("revoked_reason")
                        .map_err(db)?
                        .unwrap_or_else(|| "revoked".into()),
                    revoked_at_ms: r.try_get("revoked_ms").map_err(db)?,
                    note: r.try_get("note").map_err(db)?,
                })
            })
            .collect()
    }

    async fn get_staff(&self, account_id: i64) -> AdminResult<Option<StaffMember>> {
        let mut c = self.pool.acquire().await.map_err(db)?;
        Ok(Self::staff_members(&mut c, Some(account_id)).await?.pop())
    }

    async fn appoint(
        &self,
        account_id: i64,
        role_ids: &[i64],
        by: i64,
        reason: &str,
        audit: &AuditEntry,
    ) -> AdminResult<StaffMember> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let (guild, _) = Self::staff_guild(&mut tx).await?;
        Self::check_roles(&mut tx, guild, role_ids).await?;
        sqlx::query(
            "INSERT INTO staff_appointments (id, account_id, appointed_by, reason) VALUES ($1,$2,$3,$4)",
        )
        .bind((self.ids)())
        .bind(account_id)
        .bind(by)
        .bind(reason)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if is_unique(&e) {
                AdminError::Conflict("The account is already staff".into())
            } else {
                db(e)
            }
        })?;
        sqlx::query(
            "INSERT INTO guild_members (guild_id, user_id, join_source) VALUES ($1, $2, 'staff') \
             ON CONFLICT (guild_id, user_id) DO NOTHING",
        )
        .bind(guild)
        .bind(account_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        // Replace whatever roles a stale membership carried: the appointment decides.
        sqlx::query("DELETE FROM member_roles WHERE guild_id = $1 AND user_id = $2")
            .bind(guild)
            .bind(account_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        for r in role_ids {
            sqlx::query("INSERT INTO member_roles (guild_id, user_id, role_id) VALUES ($1,$2,$3)")
                .bind(guild)
                .bind(account_id)
                .bind(r)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        self.audit(&mut tx, audit).await?;
        let m = Self::staff_members(&mut tx, Some(account_id)).await?.pop();
        tx.commit().await.map_err(db)?;
        m.ok_or_else(|| AdminError::Storage("appointment vanished".into()))
    }

    async fn set_staff_roles(
        &self,
        account_id: i64,
        role_ids: &[i64],
        audit: &AuditEntry,
    ) -> AdminResult<StaffMember> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let (guild, owner) = Self::staff_guild(&mut tx).await?;
        if owner == account_id {
            return Err(AdminError::Forbidden("The Owner cannot be changed".into()));
        }
        if Self::staff_members(&mut tx, Some(account_id))
            .await?
            .is_empty()
        {
            return Err(AdminError::NotFound("staff member"));
        }
        Self::check_roles(&mut tx, guild, role_ids).await?;
        sqlx::query("DELETE FROM member_roles WHERE guild_id = $1 AND user_id = $2")
            .bind(guild)
            .bind(account_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        for r in role_ids {
            sqlx::query("INSERT INTO member_roles (guild_id, user_id, role_id) VALUES ($1,$2,$3)")
                .bind(guild)
                .bind(account_id)
                .bind(r)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        self.audit(&mut tx, audit).await?;
        let m = Self::staff_members(&mut tx, Some(account_id)).await?.pop();
        tx.commit().await.map_err(db)?;
        m.ok_or(AdminError::NotFound("staff member"))
    }

    async fn revoke_staff(
        &self,
        account_id: i64,
        by: i64,
        reason: &str,
        audit: &AuditEntry,
    ) -> AdminResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let (guild, owner) = Self::staff_guild(&mut tx).await?;
        if owner == account_id {
            return Err(AdminError::Forbidden("The Owner cannot be revoked".into()));
        }
        let closed = sqlx::query(
            "UPDATE staff_appointments SET revoked_at = now(), revoked_by = $2, revoke_reason = $3, revoked_reason = 'revoked' \
             WHERE account_id = $1 AND revoked_at IS NULL",
        )
        .bind(account_id)
        .bind(by)
        .bind(reason)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        if closed == 0 {
            return Err(AdminError::NotFound("staff member"));
        }
        sqlx::query(
            "UPDATE appointment_delegations SET revoked_at = now(), revoked_by = $2 \
             WHERE account_id = $1 AND revoked_at IS NULL",
        )
        .bind(account_id)
        .bind(by)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        // Leaving the Staff Server removes the member's roles with it.
        sqlx::query("DELETE FROM guild_members WHERE guild_id = $1 AND user_id = $2")
            .bind(guild)
            .bind(account_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        self.audit(&mut tx, audit).await?;
        tx.commit().await.map_err(db)
    }

    async fn transfer_owner(&self, new_owner: i64, audit: &AuditEntry) -> AdminResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let (guild, old_owner) = Self::staff_guild(&mut tx).await?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM accounts WHERE id = $1)")
                .bind(new_owner)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
        if !exists {
            return Err(AdminError::NotFound("account"));
        }
        if old_owner == new_owner {
            return Err(AdminError::Conflict(
                "That account already owns the Staff Server".into(),
            ));
        }
        sqlx::query("UPDATE guilds SET owner_id = $2, updated_at = now() WHERE id = $1")
            .bind(guild)
            .bind(new_owner)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query(
            "INSERT INTO guild_members (guild_id, user_id, join_source) VALUES ($1, $2, 'staff') \
             ON CONFLICT (guild_id, user_id) DO NOTHING",
        )
        .bind(guild)
        .bind(new_owner)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        // Both accounts stay staff: the new Owner needs an appointment row to show in the
        // staff list, and the previous Owner keeps working as Senior Staff.
        for (acct, why) in [
            (new_owner, "ownership transfer"),
            (old_owner, "former Owner"),
        ] {
            sqlx::query(
                "INSERT INTO staff_appointments (id, account_id, appointed_by, reason) \
                 SELECT $1, $2, $3, $4 WHERE NOT EXISTS ( \
                    SELECT 1 FROM staff_appointments WHERE account_id = $2 AND revoked_at IS NULL)",
            )
            .bind((self.ids)())
            .bind(acct)
            .bind(audit.actor_id)
            .bind(why)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        let role = |name: &'static str| {
            sqlx::query_scalar::<_, i64>("SELECT id FROM roles WHERE guild_id = $1 AND name = $2")
                .bind(guild)
                .bind(name)
        };
        let owner_role = role("Owner").fetch_optional(&mut *tx).await.map_err(db)?;
        let senior_role = role("Senior Staff")
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
        if let Some(r) = owner_role {
            sqlx::query(
                "DELETE FROM member_roles WHERE guild_id = $1 AND user_id = $2 AND role_id = $3",
            )
            .bind(guild)
            .bind(old_owner)
            .bind(r)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
            sqlx::query(
                "INSERT INTO member_roles (guild_id, user_id, role_id) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING",
            )
            .bind(guild)
            .bind(new_owner)
            .bind(r)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        if let Some(r) = senior_role {
            sqlx::query(
                "INSERT INTO member_roles (guild_id, user_id, role_id) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING",
            )
            .bind(guild)
            .bind(old_owner)
            .bind(r)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        self.audit(&mut tx, audit).await?;
        tx.commit().await.map_err(db)
    }

    // ---- accounts -------------------------------------------------------

    async fn search_accounts(&self, q: &AccountQuery) -> AdminResult<Vec<AccountSummary>> {
        let like = q.q.as_ref().map(|s| {
            let esc = s
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            format!("%{esc}%")
        });
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM accounts a WHERE ($1::bigint IS NULL OR a.id > $1) \
               AND ($2::text IS NULL OR a.state::text = $2) AND ($3::bigint IS NULL OR a.person_id = $3) \
               AND ($4::text IS NULL OR a.username::text ILIKE $4 OR a.email::text ILIKE $4 \
                    OR a.display_name ILIKE $4 OR a.id::text = $5) \
               AND ($7::text IS NULL OR EXISTS (SELECT 1 FROM external_identities x \
                    WHERE x.account_id = a.id AND x.provider_user_id = $7 \
                    AND x.moved_to_account_id IS NULL)) \
               AND ($8::inet IS NULL OR EXISTS (SELECT 1 FROM login_events e \
                    WHERE e.account_id = a.id AND e.ip = $8::inet)) \
             ORDER BY a.id LIMIT $6",
            acct_cols()
        )))
        .bind(q.after)
        .bind(q.state.map(AccountState::as_str))
        .bind(q.person_id)
        .bind(&like)
        .bind(&q.q)
        .bind(q.limit)
        .bind(&q.discord_id)
        .bind(&q.ip)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(acct_from)
            .collect::<Result<_, _>>()
            .map_err(db)
    }

    async fn account_detail(&self, id: i64) -> AdminResult<Option<AccountDetail>> {
        let mut c = self.pool.acquire().await.map_err(db)?;
        let Some(r) = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {}, \
                (SELECT count(*) FROM sessions s WHERE s.account_id = a.id AND s.revoked_at IS NULL AND s.expires_at > now()) AS sessions, \
                EXISTS(SELECT 1 FROM mfa_totp m WHERE m.account_id = a.id AND m.confirmed_at IS NOT NULL) AS mfa, \
                COALESCE((SELECT SUM(delta) FROM invite_credits_ledger l WHERE l.account_id = a.id), 0)::bigint AS balance, \
                (SELECT count(*) FROM platform_invites i WHERE i.issuer_account_id = a.id) AS issued, \
                (SELECT count(*) FROM platform_invites i WHERE i.issuer_account_id = a.id AND i.state = 'redeemed') AS redeemed, \
                p.suspicion_level::int AS suspicion, \
                (EXISTS(SELECT 1 FROM person_bans b WHERE b.person_id = a.person_id AND b.lifted_at IS NULL) \
                 OR EXISTS(SELECT 1 FROM accounts o WHERE o.person_id = a.person_id AND o.state = 'platform_banned')) AS banned, \
                ia.id AS inviter_id, ia.username::text AS inviter_name, \
                a.invites_frozen_at IS NOT NULL AS frozen \
             FROM accounts a JOIN persons p ON p.id = a.person_id \
             LEFT JOIN platform_invites pi ON pi.id = a.invited_by_invite_id \
             LEFT JOIN accounts ia ON ia.id = pi.issuer_account_id \
             WHERE a.id = $1",
            acct_cols()
        )))
        .bind(id)
        .fetch_optional(&mut *c)
        .await
        .map_err(db)?
        else {
            return Ok(None);
        };
        let logins = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT ip::text AS ip, success, {} AS at_ms FROM login_events WHERE account_id = $1 \
             ORDER BY created_at DESC LIMIT 10",
            ms("created_at")
        )))
        .bind(id)
        .fetch_all(&mut *c)
        .await
        .map_err(db)?;
        let inviter: Option<i64> = r.try_get("inviter_id").map_err(db)?;
        Ok(Some(AccountDetail {
            account: acct_from(&r).map_err(db)?,
            invited_by: match inviter {
                Some(i) => Some(AccountRef {
                    id: i,
                    username: r.try_get("inviter_name").map_err(db)?,
                }),
                None => None,
            },
            sessions_active: r.try_get("sessions").map_err(db)?,
            mfa_enabled: r.try_get("mfa").map_err(db)?,
            credit_balance: r.try_get("balance").map_err(db)?,
            invites_issued: r.try_get("issued").map_err(db)?,
            invites_redeemed: r.try_get("redeemed").map_err(db)?,
            suspicion_level: r.try_get("suspicion").map_err(db)?,
            person_banned: r.try_get("banned").map_err(db)?,
            recent_logins: logins
                .iter()
                .map(|l| {
                    Ok(LoginEvent {
                        ip: l.try_get("ip")?,
                        success: l.try_get("success")?,
                        at_ms: l.try_get("at_ms")?,
                    })
                })
                .collect::<Result<_, sqlx::Error>>()
                .map_err(db)?,
            invites_frozen: r.try_get("frozen").map_err(db)?,
        }))
    }

    async fn set_account_state(
        &self,
        id: i64,
        to: AccountState,
        cap: i64,
        audit: &AuditEntry,
    ) -> AdminResult<AccountSummary> {
        let from = match to {
            AccountState::Deactivated => AccountState::Active,
            AccountState::Active => AccountState::Deactivated,
            _ => {
                return Err(AdminError::Conflict(
                    "That state change is not allowed".into(),
                ));
            }
        };
        let mut tx = self.pool.begin().await.map_err(db)?;
        if to == AccountState::Active {
            lock::xact_lock(&mut tx, lock::CAP_LOCK)
                .await
                .map_err(db2)?;
            let active: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM accounts WHERE state = 'active' AND NOT is_bot",
            )
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
            if active >= cap {
                return Err(AdminError::Conflict("The account cap is reached".into()));
            }
        }
        let changed = sqlx::query(
            "UPDATE accounts SET state = $2::account_state, state_changed_at = now(), updated_at = now() \
             WHERE id = $1 AND state = $3::account_state",
        )
        .bind(id)
        .bind(to.as_str())
        .bind(from.as_str())
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        if changed == 0 {
            return Err(AdminError::Conflict(format!(
                "The account is not {}",
                from.as_str()
            )));
        }
        if to == AccountState::Deactivated {
            sqlx::query(
                "UPDATE sessions SET revoked_at = now(), updated_at = now() \
                 WHERE account_id = $1 AND revoked_at IS NULL",
            )
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        self.audit(&mut tx, audit).await?;
        let out = Self::account_summary(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(out)
    }

    async fn revoke_sessions(&self, id: i64, audit: &AuditEntry) -> AdminResult<u64> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::account_summary(&mut tx, id).await?;
        let n = sqlx::query(
            "UPDATE sessions SET revoked_at = now(), updated_at = now() \
             WHERE account_id = $1 AND revoked_at IS NULL",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        self.audit(&mut tx, audit).await?;
        tx.commit().await.map_err(db)?;
        Ok(n)
    }

    async fn reset_2fa(&self, id: i64, audit: &AuditEntry) -> AdminResult<Reset2faReport> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::account_summary(&mut tx, id).await?;
        let r = accounts::reset_2fa_in(&mut tx, id).await.map_err(db2)?;
        self.audit(&mut tx, audit).await?;
        tx.commit().await.map_err(db)?;
        Ok(Reset2faReport {
            totp: r.totp,
            recovery_codes: r.recovery_codes,
            passkeys: r.passkeys,
            trusted_devices: r.trusted_devices,
            sessions_revoked: r.sessions_revoked,
        })
    }

    async fn create_delete_request(
        &self,
        id: i64,
        requested_by: i64,
        reason: &str,
        audit: &AuditEntry,
    ) -> AdminResult<DeleteRequest> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::account_summary(&mut tx, id).await?;
        let rid = (self.ids)();
        let created: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "INSERT INTO account_delete_requests (id, account_id, requested_by, reason) \
             VALUES ($1,$2,$3,$4) RETURNING {}",
            ms("created_at")
        )))
        .bind(rid)
        .bind(id)
        .bind(requested_by)
        .bind(reason)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            if is_unique(&e) {
                AdminError::Conflict("A deletion request is already pending".into())
            } else {
                db(e)
            }
        })?;
        self.audit(&mut tx, audit).await?;
        tx.commit().await.map_err(db)?;
        Ok(DeleteRequest {
            id: rid,
            account_id: id,
            state: "pending".into(),
            created_at_ms: created,
        })
    }

    // ---- persons and bans ----------------------------------------------

    async fn person_detail(&self, person_id: i64) -> AdminResult<Option<PersonDetail>> {
        let Some((notes, level)): Option<(String, i32)> =
            sqlx::query_as("SELECT notes, suspicion_level::int FROM persons WHERE id = $1")
                .bind(person_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db)?
        else {
            return Ok(None);
        };
        let accts = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM accounts a WHERE a.person_id = $1 ORDER BY a.id",
            acct_cols()
        )))
        .bind(person_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let ban = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM person_bans WHERE person_id = $1 ORDER BY id DESC LIMIT 1",
            ban_cols()
        )))
        .bind(person_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        let identities = sqlx::query(
            "SELECT e.id, e.account_id, e.provider::text AS provider, e.provider_user_id, \
                    (EXTRACT(EPOCH FROM e.linked_at) * 1000)::bigint AS linked_ms, \
                    (EXTRACT(EPOCH FROM e.unlinked_at) * 1000)::bigint AS unlinked_ms, \
                    e.moved_to_account_id \
             FROM external_identities e JOIN accounts a ON a.id = e.account_id \
             WHERE a.person_id = $1 ORDER BY e.id",
        )
        .bind(person_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?
        .iter()
        .map(|r| {
            Ok(sr_admin::IdentitySummary {
                id: r.try_get("id")?,
                account_id: r.try_get("account_id")?,
                provider: r.try_get("provider")?,
                provider_user_id: r.try_get("provider_user_id")?,
                linked_at_ms: r.try_get("linked_ms")?,
                unlinked_at_ms: r.try_get("unlinked_ms")?,
                moved_to_account_id: r.try_get("moved_to_account_id")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(db)?;
        Ok(Some(PersonDetail {
            id: person_id,
            notes,
            suspicion_level: level,
            identities,
            accounts: accts
                .iter()
                .map(acct_from)
                .collect::<Result<_, _>>()
                .map_err(db)?,
            ban: ban.as_ref().map(ban_from).transpose().map_err(db)?,
        }))
    }

    async fn ban_person(
        &self,
        person_id: i64,
        source_account: Option<i64>,
        category: BanCategory,
        note: &str,
        by: i64,
        freeze_at: i64,
        escalate_at: i64,
        audit: &AuditEntry,
    ) -> AdminResult<BanOutcome> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let found: Option<i64> =
            sqlx::query_scalar("SELECT id FROM persons WHERE id = $1 FOR UPDATE")
                .bind(person_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        if found.is_none() {
            return Err(AdminError::NotFound("person"));
        }
        let ban_id = (self.ids)();
        let created: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "INSERT INTO person_bans (id, person_id, category, note, banned_by) \
             VALUES ($1,$2,$3::ban_category,$4,$5) RETURNING {}",
            ms("created_at")
        )))
        .bind(ban_id)
        .bind(person_id)
        .bind(category.as_str())
        .bind(note)
        .bind(by)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            if is_unique(&e) {
                AdminError::Conflict("The Person is already banned".into())
            } else {
                db(e)
            }
        })?;
        let banned = sqlx::query(
            "UPDATE accounts SET state = 'platform_banned', state_changed_at = now(), updated_at = now() \
             WHERE person_id = $1 AND id <> $2 AND state IN ('active', 'deactivated')",
        )
        .bind(person_id)
        .bind(accounts::SYSTEM_USER_ID)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        let sessions = sqlx::query(
            "UPDATE sessions SET revoked_at = now(), updated_at = now() \
             WHERE revoked_at IS NULL AND account_id IN (SELECT id FROM accounts WHERE person_id = $1)",
        )
        .bind(person_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();

        let mut inviter_suspicion = None;
        if category == BanCategory::Serious
            && let Some(src) = source_account
        {
            let inviter: Option<(i64, i64)> = sqlx::query_as(
                "SELECT ia.id, ia.person_id FROM accounts a \
                 JOIN platform_invites pi ON pi.id = a.invited_by_invite_id \
                 JOIN accounts ia ON ia.id = pi.issuer_account_id WHERE a.id = $1",
            )
            .bind(src)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
            if let Some((inviter_id, inviter_person)) = inviter
                && inviter_person != person_id
                && inviter_id != accounts::SYSTEM_USER_ID
            {
                let level: i32 = sqlx::query_scalar(
                    "UPDATE persons SET suspicion_level = suspicion_level + 1, updated_at = now() \
                     WHERE id = $1 RETURNING suspicion_level::int",
                )
                .bind(inviter_person)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
                sqlx::query(
                    "INSERT INTO suspicion_events (id, person_id, delta, level_after, reason, source_account_id, actor_id) \
                     VALUES ($1,$2,1,$3,$4,$5,$6)",
                )
                .bind((self.ids)())
                .bind(inviter_person)
                .bind(level)
                .bind("serious ban of an invitee")
                .bind(src)
                .bind(by)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
                inviter_suspicion = Some(SuspicionChange {
                    person_id: inviter_person,
                    inviter_account_id: inviter_id,
                    level,
                    frozen: i64::from(level) >= freeze_at,
                    escalated: i64::from(level) >= escalate_at,
                    freeze_crossed: i64::from(level) >= freeze_at
                        && i64::from(level - 1) < freeze_at,
                    escalate_crossed: i64::from(level) >= escalate_at
                        && i64::from(level - 1) < escalate_at,
                    banned_person_id: person_id,
                });
            }
        }
        self.audit(&mut tx, audit).await?;
        tx.commit().await.map_err(db)?;
        Ok(BanOutcome {
            ban: BanRecord {
                id: ban_id,
                person_id,
                category,
                note: note.to_owned(),
                banned_by: by,
                created_at_ms: created,
                lifted_at_ms: None,
                reason_category: None,
            },
            accounts_banned: banned,
            sessions_revoked: sessions,
            inviter_suspicion,
        })
    }

    async fn unban_person(
        &self,
        person_id: i64,
        cap: i64,
        by: i64,
        note: &str,
        audit: &AuditEntry,
    ) -> AdminResult<UnbanOutcome> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock::xact_lock(&mut tx, lock::CAP_LOCK)
            .await
            .map_err(db2)?;
        let lifted = sqlx::query(
            "UPDATE person_bans SET lifted_at = now(), lifted_by = $2, lift_note = $3 \
             WHERE person_id = $1 AND lifted_at IS NULL",
        )
        .bind(person_id)
        .bind(by)
        .bind(note)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        if lifted == 0 {
            return Err(AdminError::Conflict("The Person is not banned".into()));
        }
        let mut active: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM accounts WHERE state = 'active' AND NOT is_bot",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM accounts WHERE person_id = $1 AND state = 'platform_banned' ORDER BY id",
        )
        .bind(person_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        let (mut ok, mut left) = (0u64, 0u64);
        for id in ids {
            let to = if active < cap {
                active += 1;
                ok += 1;
                "active"
            } else {
                left += 1;
                "deactivated"
            };
            sqlx::query(
                "UPDATE accounts SET state = $2::account_state, state_changed_at = now(), updated_at = now() WHERE id = $1",
            )
            .bind(id)
            .bind(to)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        self.audit(&mut tx, audit).await?;
        tx.commit().await.map_err(db)?;
        Ok(UnbanOutcome {
            accounts_reactivated: ok,
            accounts_left_deactivated: left,
        })
    }

    async fn list_bans(
        &self,
        f: &BanFilter,
        after: Option<i64>,
        limit: i64,
    ) -> AdminResult<Vec<BanRecord>> {
        // Every filter is a bound parameter. The age filters run on `created_at` (indexed) and the reason
        // filter on the partial `reason_category` index or, for `none`, on IS NULL.
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM person_bans WHERE ($1 = false OR lifted_at IS NULL) \
               AND ($2::bigint IS NULL OR id < $2) \
               AND ($4::text IS NULL OR category::text = $4) \
               AND ($5::text IS NULL OR ($5 = 'none' AND reason_category IS NULL) \
                    OR reason_category::text = $5) \
               AND ($6::int IS NULL OR created_at <= now() - make_interval(days => $6)) \
               AND ($7::int IS NULL OR created_at >= now() - make_interval(days => $7)) \
             ORDER BY id DESC LIMIT $3",
            ban_cols()
        )))
        .bind(f.active_only)
        .bind(after)
        .bind(limit)
        .bind(f.category.map(BanCategory::as_str))
        .bind(f.reason_category.as_deref())
        .bind(f.older_than_days)
        .bind(f.newer_than_days)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(ban_from)
            .collect::<Result<_, _>>()
            .map_err(db)
    }

    async fn list_suspects(&self, freeze_at: i64) -> AdminResult<Vec<SuspectPerson>> {
        let people: Vec<(i64, i32)> = sqlx::query_as(
            "SELECT id, suspicion_level::int FROM persons WHERE suspicion_level > 0 \
             ORDER BY suspicion_level DESC, id LIMIT 200",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let mut out = Vec::with_capacity(people.len());
        for (id, level) in people {
            let accts: Vec<(i64, String)> = sqlx::query_as(
                "SELECT id, username::text FROM accounts WHERE person_id = $1 ORDER BY id",
            )
            .bind(id)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
            out.push(SuspectPerson {
                person_id: id,
                suspicion_level: level,
                frozen: i64::from(level) >= freeze_at,
                accounts: accts
                    .into_iter()
                    .map(|(id, username)| AccountRef { id, username })
                    .collect(),
            });
        }
        Ok(out)
    }

    async fn clear_suspicion(
        &self,
        person_id: i64,
        by: i64,
        audit: &AuditEntry,
    ) -> AdminResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let old: Option<i32> =
            sqlx::query_scalar("SELECT suspicion_level::int FROM persons WHERE id = $1 FOR UPDATE")
                .bind(person_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        let Some(old) = old else {
            return Err(AdminError::NotFound("person"));
        };
        sqlx::query("UPDATE persons SET suspicion_level = 0, updated_at = now() WHERE id = $1")
            .bind(person_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        if old > 0 {
            sqlx::query(
                "INSERT INTO suspicion_events (id, person_id, delta, level_after, reason, actor_id) \
                 VALUES ($1,$2,$3,0,$5,$4)",
            )
            .bind((self.ids)())
            .bind(person_id)
            .bind(-(old.min(i32::from(i16::MAX)) as i16))
            .bind(by)
            .bind(format!("cleared by staff: {}", audit.reason))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        self.audit(&mut tx, audit).await?;
        tx.commit().await.map_err(db)
    }

    // ---- invite graph and credits --------------------------------------

    async fn invite_tree(&self, account_id: i64) -> AdminResult<Option<InviteTree>> {
        let rows = sqlx::query(
            "WITH RECURSIVE chain AS ( \
                SELECT a.id, a.username::text AS username, a.state::text AS state, a.person_id, \
                       pi.issuer_account_id AS parent_id, 0 AS depth \
                FROM accounts a LEFT JOIN platform_invites pi ON pi.id = a.invited_by_invite_id \
                WHERE a.id = $1 \
                UNION ALL \
                SELECT p.id, p.username::text, p.state::text, p.person_id, pi.issuer_account_id, c.depth - 1 \
                FROM chain c JOIN accounts p ON p.id = c.parent_id \
                LEFT JOIN platform_invites pi ON pi.id = p.invited_by_invite_id \
                WHERE c.depth > -1000) \
             SELECT c.id, c.username, c.state, c.person_id, c.parent_id, c.depth, ps.suspicion_level::int AS suspicion \
             FROM chain c JOIN persons ps ON ps.id = c.person_id ORDER BY c.depth",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        if rows.is_empty() {
            return Ok(None);
        }
        let mut nodes: Vec<InviteNode> = rows
            .iter()
            .map(Self::node_from)
            .collect::<Result<_, _>>()
            .map_err(db)?;
        // Ordered by depth ascending: the root (most negative) first, the account (0) last.
        let account = nodes.pop().ok_or(AdminError::NotFound("account"))?;
        let counts: Vec<(String, i64)> = sqlx::query_as(
            "SELECT state::text, count(*) FROM platform_invites WHERE issuer_account_id = $1 GROUP BY state",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let mut stats = InviteStats::default();
        for (state, n) in counts {
            stats.issued += n;
            match state.as_str() {
                "open" => stats.open = n,
                "redeemed" => stats.redeemed = n,
                _ => stats.revoked = n,
            }
        }
        let (d, b): (i64, i64) = sqlx::query_as(
            "WITH RECURSIVE d AS ( \
                SELECT pi.redeemed_by_account_id AS id, 1 AS depth FROM platform_invites pi \
                WHERE pi.issuer_account_id = $1 AND pi.state = 'redeemed' \
                UNION ALL \
                SELECT pi.redeemed_by_account_id, d.depth + 1 FROM d \
                JOIN platform_invites pi ON pi.issuer_account_id = d.id AND pi.state = 'redeemed' \
                WHERE d.depth < 1000) \
             SELECT count(*), count(*) FILTER (WHERE a.state = 'platform_banned') \
             FROM d JOIN accounts a ON a.id = d.id",
        )
        .bind(account_id)
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        stats.descendants = d;
        stats.banned_descendants = b;
        Ok(Some(InviteTree {
            account,
            ancestors: nodes,
            stats,
        }))
    }

    async fn invite_descendants(
        &self,
        account_id: i64,
        max_depth: i32,
        max_nodes: i64,
    ) -> AdminResult<Option<InviteDescendants>> {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM accounts WHERE id = $1)")
                .bind(account_id)
                .fetch_one(&self.pool)
                .await
                .map_err(db)?;
        if !exists {
            return Ok(None);
        }
        let rows = sqlx::query(
            "WITH RECURSIVE d AS ( \
                SELECT pi.redeemed_by_account_id AS id, pi.issuer_account_id AS parent_id, 1 AS depth \
                FROM platform_invites pi WHERE pi.issuer_account_id = $1 AND pi.state = 'redeemed' \
                UNION ALL \
                SELECT pi.redeemed_by_account_id, pi.issuer_account_id, d.depth + 1 FROM d \
                JOIN platform_invites pi ON pi.issuer_account_id = d.id AND pi.state = 'redeemed' \
                WHERE d.depth < $2) \
             SELECT a.id, a.username::text AS username, a.state::text AS state, a.person_id, \
                    d.parent_id, d.depth, ps.suspicion_level::int AS suspicion \
             FROM d JOIN accounts a ON a.id = d.id JOIN persons ps ON ps.id = a.person_id \
             ORDER BY d.depth, a.id LIMIT $3",
        )
        .bind(account_id)
        .bind(max_depth)
        .bind(max_nodes + 1)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let mut nodes: Vec<InviteNode> = rows
            .iter()
            .map(Self::node_from)
            .collect::<Result<_, _>>()
            .map_err(db)?;
        let truncated = nodes.len() as i64 > max_nodes;
        nodes.truncate(max_nodes as usize);
        Ok(Some(InviteDescendants { nodes, truncated }))
    }

    async fn credit_ledger(
        &self,
        account_id: i64,
        limit: i64,
    ) -> AdminResult<Option<CreditLedger>> {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM accounts WHERE id = $1)")
                .bind(account_id)
                .fetch_one(&self.pool)
                .await
                .map_err(db)?;
        if !exists {
            return Ok(None);
        }
        let balance: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(delta), 0)::bigint FROM invite_credits_ledger WHERE account_id = $1",
        )
        .bind(account_id)
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT id, delta, reason::text AS reason, ref_id, {} AS created_ms \
             FROM invite_credits_ledger WHERE account_id = $1 ORDER BY id DESC LIMIT $2",
            ms("created_at")
        )))
        .bind(account_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let entries = rows
            .iter()
            .map(|r| {
                Ok(LedgerEntry {
                    id: r.try_get("id")?,
                    delta: r.try_get("delta")?,
                    reason: r.try_get("reason")?,
                    ref_id: r.try_get("ref_id")?,
                    created_at_ms: r.try_get("created_ms")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(db)?;
        Ok(Some(CreditLedger { balance, entries }))
    }

    async fn adjust_credits(
        &self,
        account_id: i64,
        delta: i32,
        by: i64,
        audit: &AuditEntry,
    ) -> AdminResult<i64> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        // Same per-account serialisation as registration, so the balance check is race-free.
        let locked: Option<i64> =
            sqlx::query_scalar("SELECT id FROM accounts WHERE id = $1 FOR UPDATE")
                .bind(account_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        if locked.is_none() {
            return Err(AdminError::NotFound("account"));
        }
        let balance: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(delta), 0)::bigint FROM invite_credits_ledger WHERE account_id = $1",
        )
        .bind(account_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        if balance + i64::from(delta) < 0 {
            return Err(AdminError::Conflict(
                "The balance cannot go below zero".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO invite_credits_ledger (id, account_id, delta, reason, ref_id) \
             VALUES ($1,$2,$3,$4::invite_credit_reason,$5)",
        )
        .bind((self.ids)())
        .bind(account_id)
        .bind(delta)
        .bind(if delta > 0 {
            "staff_grant"
        } else {
            "staff_revoke"
        })
        .bind(by)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        self.audit(&mut tx, audit).await?;
        tx.commit().await.map_err(db)?;
        Ok(balance + i64::from(delta))
    }

    async fn list_invites(&self, q: &InviteQuery) -> AdminResult<Vec<InviteRow>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT id, code::text AS code, issuer_account_id, state::text AS state, redeemed_by_account_id, \
                    note, emergency, {} AS created_ms FROM platform_invites \
             WHERE ($1::bigint IS NULL OR issuer_account_id = $1) AND ($2::text IS NULL OR state::text = $2) \
               AND ($3::bigint IS NULL OR id < $3) ORDER BY id DESC LIMIT $4",
            ms("created_at")
        )))
        .bind(q.issuer_id)
        .bind(&q.state)
        .bind(q.after)
        .bind(q.limit)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(invite_from)
            .collect::<Result<_, _>>()
            .map_err(db)
    }

    async fn revoke_invite(&self, invite_id: i64, audit: &AuditEntry) -> AdminResult<InviteRow> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE platform_invites SET state = 'revoked', revoked_at = now(), updated_at = now() \
             WHERE id = $1 AND state = 'open' \
             RETURNING id, code::text AS code, issuer_account_id, state::text AS state, \
                       redeemed_by_account_id, note, emergency, {} AS created_ms",
            ms("created_at")
        )))
        .bind(invite_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        let Some(row) = row else {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM platform_invites WHERE id = $1)")
                    .bind(invite_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(db)?;
            return Err(if exists {
                AdminError::Conflict("Only an open invite can be revoked".into())
            } else {
                AdminError::NotFound("invite")
            });
        };
        self.audit(&mut tx, audit).await?;
        tx.commit().await.map_err(db)?;
        invite_from(&row).map_err(db)
    }

    // ---- platform settings ---------------------------------------------

    async fn settings(&self) -> AdminResult<HashMap<String, StoredSetting>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT key, value, updated_by, {} AS updated_ms FROM platform_settings",
            ms("updated_at")
        )))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(|r| {
                Ok((
                    r.try_get::<String, _>("key")?,
                    StoredSetting {
                        value: r.try_get("value")?,
                        updated_by: r.try_get("updated_by")?,
                        updated_at_ms: r.try_get("updated_ms")?,
                    },
                ))
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(db)
    }

    async fn put_settings(
        &self,
        values: &[(String, Value)],
        by: i64,
        audit: &AuditEntry,
    ) -> AdminResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        for (k, v) in values {
            sqlx::query(
                "INSERT INTO platform_settings (key, value, updated_by) VALUES ($1, $2, $3) \
                 ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_by = EXCLUDED.updated_by, \
                                                 updated_at = now()",
            )
            .bind(k)
            .bind(v)
            .bind(by)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        self.audit(&mut tx, audit).await?;
        tx.commit().await.map_err(db)
    }

    // ---- system ---------------------------------------------------------

    async fn set_delegation(
        &self,
        account_id: i64,
        max_permissions: u64,
        by: i64,
        reason: &str,
        audit: &AuditEntry,
    ) -> AdminResult<Delegation> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        if Self::staff_members(&mut tx, Some(account_id))
            .await?
            .is_empty()
        {
            return Err(AdminError::NotFound("staff member"));
        }
        sqlx::query(
            "UPDATE appointment_delegations SET revoked_at = now(), revoked_by = $2 \
             WHERE account_id = $1 AND revoked_at IS NULL",
        )
        .bind(account_id)
        .bind(by)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query(
            "INSERT INTO appointment_delegations (id, account_id, granted_by, max_permissions, reason) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind((self.ids)())
        .bind(account_id)
        .bind(by)
        .bind(max_permissions as i64)
        .bind(reason)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        self.audit(&mut tx, audit).await?;
        let d = Self::delegations_in(&mut tx, Some(account_id)).await?.pop();
        tx.commit().await.map_err(db)?;
        d.ok_or_else(|| AdminError::Storage("delegation vanished".into()))
    }

    async fn remove_delegation(&self, account_id: i64, audit: &AuditEntry) -> AdminResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let n = sqlx::query(
            "UPDATE appointment_delegations SET revoked_at = now(), revoked_by = $2 \
             WHERE account_id = $1 AND revoked_at IS NULL",
        )
        .bind(account_id)
        .bind(audit.actor_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        if n == 0 {
            return Err(AdminError::NotFound("delegation"));
        }
        self.audit(&mut tx, audit).await?;
        tx.commit().await.map_err(db)
    }

    async fn list_delegations(&self) -> AdminResult<Vec<Delegation>> {
        let mut c = self.pool.acquire().await.map_err(db)?;
        Self::delegations_in(&mut c, None).await
    }

    async fn rename_account(
        &self,
        id: i64,
        username: Option<&str>,
        display_name: Option<Option<&str>>,
        audit: &AuditEntry,
    ) -> AdminResult<AccountSummary> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::account_summary(&mut tx, id).await?;
        sqlx::query(
            "UPDATE accounts SET username = COALESCE($2::citext, username), \
                    display_name = CASE WHEN $3 THEN $4 ELSE display_name END, updated_at = now() \
             WHERE id = $1",
        )
        .bind(id)
        .bind(username)
        .bind(display_name.is_some())
        .bind(display_name.flatten())
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if is_unique(&e) {
                AdminError::Conflict("That username is taken".into())
            } else {
                db(e)
            }
        })?;
        self.audit(&mut tx, audit).await?;
        let out = Self::account_summary(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(out)
    }

    async fn set_account_email(
        &self,
        id: i64,
        email: &str,
        audit: &AuditEntry,
    ) -> AdminResult<AccountSummary> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::account_summary(&mut tx, id).await?;
        sqlx::query(
            "UPDATE accounts SET email = $2::citext, email_verified_at = NULL, updated_at = now() \
             WHERE id = $1",
        )
        .bind(id)
        .bind(email)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if is_unique(&e) {
                AdminError::Conflict("That email address is used by another account".into())
            } else {
                db(e)
            }
        })?;
        // Links mailed to the old address (password reset, verification) die with it.
        sqlx::query(
            "UPDATE email_tokens SET used_at = now(), updated_at = now() \
             WHERE account_id = $1 AND used_at IS NULL AND email <> $2::citext",
        )
        .bind(id)
        .bind(email)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        self.audit(&mut tx, audit).await?;
        let out = Self::account_summary(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(out)
    }

    async fn update_account_flags(
        &self,
        id: i64,
        set: i64,
        clear: i64,
        audit: &AuditEntry,
    ) -> AdminResult<AccountSummary> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::account_summary(&mut tx, id).await?;
        sqlx::query(
            "UPDATE accounts SET flags = (flags | $2) & ~$3, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(set)
        .bind(clear)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        self.audit(&mut tx, audit).await?;
        let out = Self::account_summary(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(out)
    }

    async fn force_password_reset(&self, id: i64, audit: &AuditEntry) -> AdminResult<u64> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::account_summary(&mut tx, id).await?;
        // An unusable hash: no password matches, so only "Forgot password" gets back in.
        sqlx::query("UPDATE accounts SET password_hash = '!', updated_at = now() WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let n = sqlx::query(
            "UPDATE sessions SET revoked_at = now(), updated_at = now() \
             WHERE account_id = $1 AND revoked_at IS NULL",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        sqlx::query(
            "UPDATE trusted_devices SET revoked_at = now(), updated_at = now() \
             WHERE account_id = $1 AND revoked_at IS NULL",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        self.audit(&mut tx, audit).await?;
        tx.commit().await.map_err(db)?;
        Ok(n)
    }

    async fn counts(&self) -> AdminResult<SystemCounts> {
        let r = sqlx::query(
            "SELECT \
              (SELECT count(*) FROM accounts WHERE state = 'active' AND NOT is_bot) AS active, \
              (SELECT count(*) FROM accounts WHERE state = 'deactivated' AND id <> 1) AS deactivated, \
              (SELECT count(*) FROM accounts WHERE state = 'platform_banned') AS banned, \
              (SELECT count(*) FROM accounts WHERE state = 'deleted') AS deleted, \
              (SELECT count(*) FROM sessions WHERE revoked_at IS NULL AND expires_at > now()) AS sessions, \
              (SELECT count(*) FROM jobs WHERE state = 'queued') AS queued, \
              (SELECT count(*) FROM jobs WHERE state = 'running') AS running, \
              (SELECT count(*) FROM jobs WHERE state = 'dead') AS dead, \
              (SELECT count(*) FROM staff_appointments WHERE revoked_at IS NULL) AS staff, \
              (SELECT count(*) FROM submissions WHERE state IN ('submitted', 'in_review')) AS review, \
              (SELECT count(*) FROM platform_invites WHERE state = 'open') AS invites_open, \
              (SELECT count(*) FROM staff_alerts WHERE acked_at IS NULL) AS alerts_unacked, \
              (SELECT COALESCE(sum(size_bytes), 0) FROM attachments)::bigint AS storage",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        let g = |c: &str| r.try_get::<i64, _>(c).map_err(db);
        Ok(SystemCounts {
            accounts_active: g("active")?,
            accounts_deactivated: g("deactivated")?,
            accounts_banned: g("banned")?,
            accounts_deleted: g("deleted")?,
            sessions_active: g("sessions")?,
            jobs_queued: g("queued")?,
            jobs_running: g("running")?,
            jobs_dead: g("dead")?,
            staff: g("staff")?,
            accounts_cap: 0,
            review_pending: g("review")?,
            invites_open: g("invites_open")?,
            alerts_unacked: g("alerts_unacked")?,
            storage_bytes: g("storage")?,
            storage_figures: crate::storage_figures::list(&self.pool)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|f| sr_admin::StorageFigure {
                    bucket: f.bucket,
                    bytes: f.bytes,
                    objects: f.objects,
                    truncated: f.truncated,
                    measured_at_ms: f.measured_at_ms,
                })
                .collect(),
        })
    }

    async fn list_jobs(
        &self,
        state: Option<&str>,
        after: Option<i64>,
        limit: i64,
    ) -> AdminResult<Vec<JobRow>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT id, kind, state::text AS state, attempts, max_attempts, last_error, \
                    {} AS run_ms, {} AS finished_ms FROM jobs \
             WHERE ($1::text IS NULL OR state::text = $1) AND ($2::bigint IS NULL OR id < $2) \
             ORDER BY id DESC LIMIT $3",
            ms("run_at"),
            ms("finished_at")
        )))
        .bind(state)
        .bind(after)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(job_from)
            .collect::<Result<_, _>>()
            .map_err(db)
    }

    async fn retry_job(&self, id: i64, audit: &AuditEntry) -> AdminResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        if !jobs::retry(&mut *tx, id).await.map_err(db2)? {
            return Err(AdminError::NotFound("dead job"));
        }
        self.audit(&mut tx, audit).await?;
        tx.commit().await.map_err(db)
    }

    async fn delete_job(&self, id: i64, audit: &AuditEntry) -> AdminResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let n = sqlx::query("DELETE FROM jobs WHERE id = $1 AND state = 'dead'")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected();
        if n == 0 {
            return Err(AdminError::NotFound("dead job"));
        }
        self.audit(&mut tx, audit).await?;
        tx.commit().await.map_err(db)
    }

    // ---- access log -----------------------------------------------------

    async fn append_log(&self, entry: &AuditEntry) -> AdminResult<AccessLogEntry> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let row = append_in(&mut tx, entry).await?;
        tx.commit().await.map_err(db)?;
        Ok(row)
    }

    async fn list_log(&self, f: &AccessLogFilter) -> AdminResult<Vec<AccessLogEntry>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {LOG_COLS} FROM staff_access_log \
             WHERE ($1::bigint IS NULL OR actor_id = $1) AND ($2::bigint IS NULL OR target_id = $2) \
               AND ($3::text IS NULL OR action = $3) AND ($4::text IS NULL OR kind = $4) \
               AND ($5::bigint IS NULL OR seq < $5) ORDER BY seq DESC LIMIT $6"
        )))
        .bind(f.actor_id)
        .bind(f.target_id)
        .bind(&f.action)
        .bind(f.kind.map(AccessKind::as_str))
        .bind(f.before)
        .bind(f.limit)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(log_from)
            .collect::<Result<_, _>>()
            .map_err(db)
    }

    async fn verify_log(&self) -> AdminResult<ChainReport> {
        let mut c = self.pool.acquire().await.map_err(db)?;
        verify_chain(&mut c).await
    }
}
