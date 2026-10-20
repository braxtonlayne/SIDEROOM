//! Postgres side of the data-request queue (M-25 step c, MOD-DSR): [`DataRequestRepository`] for
//! [`PgAdminRepo`]. Every state change and its access-log row commit together. The export run
//! queues the self-service `user.data_export` job; the erasure run calls
//! [`crate::account_deletion::delete_account`]. Nothing here deletes a `data_requests` row.
//! Tests: `tests/data_requests.rs` (needs `SR_TEST_DATABASE_URL`).

use async_trait::async_trait;
use sqlx::{PgConnection, Row, postgres::PgRow};
use sr_admin::{
    AdminError, AdminResult, AuditEntry, DataRequest, DataRequestKind, DataRequestRepository,
    DataRequestState, DeletedArchive, ErasureOptions, ErasureReport, ErasureRun, NewDataRequest,
    data_requests::{DAY_MS, erasure_outcome, retained_ids},
};

use crate::{
    account_deletion::{self, Blocked},
    admin_repo::{PgAdminRepo, db, is_unique, ms},
    data_export,
};

fn from_ms(col: &str) -> String {
    format!("to_timestamp({col}::float8 / 1000.0)")
}

fn cols() -> String {
    format!(
        "r.id, r.kind, r.person_id, r.account_id, a.username::text AS username, r.state, r.note, \
         {} AS received_ms, {} AS due_ms, {} AS ext_ms, r.extension_reason, r.verified_by, \
         {} AS verified_ms, r.export_id, r.outcome, r.retained, {} AS completed_ms, r.created_by, \
         {} AS extended_ms, {} AS notified_ms",
        ms("r.received_at"),
        ms("r.due_at"),
        ms("r.extended_until"),
        ms("r.verified_at"),
        ms("r.completed_at"),
        ms("r.extended_at"),
        ms("r.member_notified_at"),
    )
}

const FROM: &str = "FROM data_requests r JOIN accounts a ON a.id = r.account_id";

fn row(r: &PgRow) -> Result<DataRequest, sqlx::Error> {
    let retained: serde_json::Value = r.try_get("retained")?;
    let kind: String = r.try_get("kind")?;
    let state: String = r.try_get("state")?;
    let due: i64 = r.try_get("due_ms")?;
    Ok(DataRequest {
        id: r.try_get("id")?,
        kind: DataRequestKind::parse(&kind).unwrap_or(DataRequestKind::Export),
        person_id: r.try_get("person_id")?,
        account_id: r.try_get("account_id")?,
        username: r.try_get("username")?,
        state: DataRequestState::parse(&state).unwrap_or(DataRequestState::Open),
        note: r.try_get("note")?,
        received_at_ms: r.try_get("received_ms")?,
        due_at_ms: due,
        extended_until_ms: r.try_get("ext_ms")?,
        extension_reason: r.try_get("extension_reason")?,
        effective_due_ms: due,
        overdue: false,
        verified_by: r.try_get("verified_by")?,
        verified_at_ms: r.try_get("verified_ms")?,
        export_id: r.try_get("export_id")?,
        outcome: r.try_get("outcome")?,
        retained: retained
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        completed_at_ms: r.try_get("completed_ms")?,
        extended_at_ms: r.try_get("extended_ms")?,
        extended_late: false,
        member_notified_at_ms: r.try_get("notified_ms")?,
        created_by: r.try_get("created_by")?,
    })
}

async fn load(conn: &mut PgConnection, id: i64, lock: bool) -> AdminResult<DataRequest> {
    let r = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {} {FROM} WHERE r.id = $1{}",
        cols(),
        if lock { " FOR UPDATE OF r" } else { "" }
    )))
    .bind(id)
    .fetch_optional(conn)
    .await
    .map_err(db)?
    .ok_or(AdminError::NotFound("data request"))?;
    row(&r).map_err(db)
}

impl PgAdminRepo {
    /// Messages and attachments of the account that a moderation hold keeps.
    async fn held_rows(conn: &mut PgConnection, account: i64) -> AdminResult<i64> {
        sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM messages WHERE author_id = $1 AND hold_id IS NOT NULL) \
                  + (SELECT count(*) FROM attachments WHERE uploader_id = $1 AND hold_id IS NOT NULL)",
        )
        .bind(account)
        .fetch_one(conn)
        .await
        .map_err(db)
    }
}

#[async_trait]
impl DataRequestRepository for PgAdminRepo {
    async fn create(&self, n: &NewDataRequest, audit: &AuditEntry) -> AdminResult<DataRequest> {
        let mut tx = self.pool().begin().await.map_err(db)?;
        let person: Option<i64> =
            sqlx::query_scalar("SELECT person_id FROM accounts WHERE id = $1")
                .bind(n.account_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        let person = person.ok_or(AdminError::NotFound("account"))?;
        let id = self.next_id();
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO data_requests (id, kind, person_id, account_id, note, received_at, due_at, created_by) \
             VALUES ($1, $2, $3, $4, $5, {}, {}, $8)",
            from_ms("$6"),
            from_ms("$7")
        )))
        .bind(id)
        .bind(n.kind.as_str())
        .bind(person)
        .bind(n.account_id)
        .bind(&n.note)
        .bind(n.received_at_ms)
        .bind(n.due_at_ms)
        .bind(n.created_by)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if is_unique(&e) {
                AdminError::Conflict("That account already has an open request of this kind".into())
            } else {
                db(e)
            }
        })?;
        self.audit_in(&mut tx, audit).await?;
        let out = load(&mut tx, id, false).await?;
        tx.commit().await.map_err(db)?;
        Ok(out)
    }

    async fn get(&self, id: i64) -> AdminResult<Option<DataRequest>> {
        let mut c = self.pool().acquire().await.map_err(db)?;
        match load(&mut c, id, false).await {
            Ok(r) => Ok(Some(r)),
            Err(AdminError::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn list(&self, active_only: bool, limit: i64) -> AdminResult<Vec<DataRequest>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {} {FROM} WHERE (NOT $1 OR r.state IN ('open', 'verified', 'running')) \
             ORDER BY (r.state NOT IN ('open', 'verified', 'running')), \
                      CASE WHEN r.state IN ('open', 'verified', 'running') \
                           THEN COALESCE(r.extended_until, r.due_at) END ASC, \
                      r.id DESC \
             LIMIT $2",
            cols()
        )))
        .bind(active_only)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(db)?;
        rows.iter().map(|r| row(r).map_err(db)).collect()
    }

    async fn deleted_archive(&self, account_id: i64) -> AdminResult<Option<DeletedArchive>> {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT account_id, username::text AS username, email::text AS email, display_name, \
             {} AS deleted_ms, {} AS release_ms, credits_forfeited, invites_revoked \
             FROM deleted_accounts WHERE account_id = $1",
            ms("deleted_at"),
            ms("username_release_at")
        )))
        .bind(account_id)
        .fetch_optional(self.pool())
        .await
        .map_err(db)?;
        row.map(|r| {
            Ok(DeletedArchive {
                account_id: r.try_get("account_id")?,
                username: r.try_get("username")?,
                email: r.try_get("email")?,
                display_name: r.try_get("display_name")?,
                deleted_at_ms: r.try_get("deleted_ms")?,
                username_release_at_ms: r.try_get("release_ms")?,
                credits_forfeited: r.try_get("credits_forfeited")?,
                invites_revoked: r.try_get("invites_revoked")?,
            })
        })
        .transpose()
        .map_err(db)
    }

    async fn verify(
        &self,
        id: i64,
        by: i64,
        now_ms: i64,
        audit: &AuditEntry,
    ) -> AdminResult<DataRequest> {
        let mut tx = self.pool().begin().await.map_err(db)?;
        let r = load(&mut tx, id, true).await?;
        if r.state != DataRequestState::Open {
            return Err(AdminError::Conflict(
                "Only an open request can be verified".into(),
            ));
        }
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE data_requests SET state = 'verified', verified_by = $2, verified_at = {} WHERE id = $1",
            from_ms("$3")
        )))
        .bind(id)
        .bind(by)
        .bind(now_ms)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        self.audit_in(&mut tx, audit).await?;
        let out = load(&mut tx, id, false).await?;
        tx.commit().await.map_err(db)?;
        Ok(out)
    }

    async fn decline(
        &self,
        id: i64,
        outcome: &str,
        now_ms: i64,
        audit: &AuditEntry,
    ) -> AdminResult<DataRequest> {
        let mut tx = self.pool().begin().await.map_err(db)?;
        let r = load(&mut tx, id, true).await?;
        if !matches!(r.state, DataRequestState::Open | DataRequestState::Verified) {
            return Err(AdminError::Conflict(
                "That request can no longer be declined".into(),
            ));
        }
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE data_requests SET state = 'declined', outcome = $2, completed_at = {} WHERE id = $1",
            from_ms("$3")
        )))
        .bind(id)
        .bind(outcome)
        .bind(now_ms)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        self.audit_in(&mut tx, audit).await?;
        let out = load(&mut tx, id, false).await?;
        tx.commit().await.map_err(db)?;
        Ok(out)
    }

    async fn extend(
        &self,
        id: i64,
        days: i64,
        reason: &str,
        by: i64,
        now_ms: i64,
        audit: &AuditEntry,
    ) -> AdminResult<DataRequest> {
        let mut tx = self.pool().begin().await.map_err(db)?;
        let r = load(&mut tx, id, true).await?;
        if !r.state.is_active() {
            return Err(AdminError::Conflict("The request is closed".into()));
        }
        if r.extended_until_ms.is_some() {
            return Err(AdminError::Conflict(
                "The deadline was already extended once".into(),
            ));
        }
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE data_requests SET extended_until = {}, extension_reason = $3, extended_by = $4, \
             extended_at = {} WHERE id = $1 AND extended_until IS NULL",
            from_ms("$2"),
            from_ms("$5")
        )))
        .bind(id)
        .bind(r.due_at_ms + days * DAY_MS)
        .bind(reason)
        .bind(by)
        .bind(now_ms)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        self.audit_in(&mut tx, audit).await?;
        let out = load(&mut tx, id, false).await?;
        tx.commit().await.map_err(db)?;
        Ok(out)
    }

    async fn member_notice(
        &self,
        id: i64,
        delivered: bool,
        now_ms: i64,
        audit: &AuditEntry,
    ) -> AdminResult<DataRequest> {
        let mut tx = self.pool().begin().await.map_err(db)?;
        load(&mut tx, id, true).await?;
        if delivered {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE data_requests SET member_notified_at = {} WHERE id = $1",
                from_ms("$2")
            )))
            .bind(id)
            .bind(now_ms)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        self.audit_in(&mut tx, audit).await?;
        let out = load(&mut tx, id, false).await?;
        tx.commit().await.map_err(db)?;
        Ok(out)
    }

    async fn run_export(
        &self,
        id: i64,
        _now_ms: i64,
        audit: &AuditEntry,
    ) -> AdminResult<DataRequest> {
        let mut tx = self.pool().begin().await.map_err(db)?;
        let r = load(&mut tx, id, true).await?;
        if r.kind != DataRequestKind::Export {
            return Err(AdminError::Conflict("This is an erasure request".into()));
        }
        if r.state != DataRequestState::Verified {
            return Err(AdminError::Conflict(
                "Verify who is asking before running the export".into(),
            ));
        }
        // Serialise with the person's own requests, as the self-service route does.
        sqlx::query("SELECT id FROM accounts WHERE id = $1 FOR UPDATE")
            .bind(r.account_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
        let export_id = self.next_id();
        data_export::queue_in(&mut tx, export_id, self.next_id(), r.account_id)
            .await
            .map_err(|e| match e {
                crate::DbError::Sqlx(ref s) if is_unique(s) => AdminError::Conflict(
                    "A data package is already being built for this account".into(),
                ),
                other => AdminError::Storage(other.to_string()),
            })?;
        sqlx::query("UPDATE data_requests SET state = 'running', export_id = $2 WHERE id = $1")
            .bind(id)
            .bind(export_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        self.audit_in(&mut tx, audit).await?;
        let out = load(&mut tx, id, false).await?;
        tx.commit().await.map_err(db)?;
        Ok(out)
    }

    async fn run_erasure(
        &self,
        id: i64,
        now_ms: i64,
        opts: ErasureOptions,
        audit: &AuditEntry,
    ) -> AdminResult<ErasureRun> {
        let r = {
            let mut c = self.pool().acquire().await.map_err(db)?;
            load(&mut c, id, false).await?
        };
        if r.kind != DataRequestKind::Erasure {
            return Err(AdminError::Conflict("This is an export request".into()));
        }
        if r.state != DataRequestState::Verified {
            return Err(AdminError::Conflict(
                "Verify who is asking before running the erasure".into(),
            ));
        }
        let held_rows = {
            let mut c = self.pool().acquire().await.map_err(db)?;
            Self::held_rows(&mut c, r.account_id).await?
        };
        let mut report = ErasureReport {
            held_rows,
            ..ErasureReport::default()
        };
        match account_deletion::delete_account(self.pool(), r.account_id, self.next_id())
            .await
            .map_err(|e| AdminError::Storage(e.to_string()))?
        {
            Ok(d) => {
                report.guilds_left = d.guilds_left;
                report.friends = d.friends;
                report.other_relations = d.other_relations;
                report.credits_forfeited = d.credits_forfeited;
                report.invites_revoked = d.invites_revoked;
                report.avatar_key = d.avatar_key;
                report.banner_key = d.banner_key;
                report.profile_media_keys = d.profile_media_keys;
            }
            Err(Blocked::NotActive) => {
                // Deleted by its owner in the meantime, or never active: finish the request only
                // when the account really is deleted.
                let state: Option<String> =
                    sqlx::query_scalar("SELECT state::text FROM accounts WHERE id = $1")
                        .bind(r.account_id)
                        .fetch_optional(self.pool())
                        .await
                        .map_err(db)?;
                if state.as_deref() != Some("deleted") {
                    return Err(AdminError::Conflict(
                        "The account is not active, so there is nothing to erase".into(),
                    ));
                }
                report.already_deleted = true;
            }
            Err(Blocked::OwnsGuilds(g)) => {
                return Err(AdminError::Conflict(format!(
                    "The account still owns {} server(s); transfer or delete them first",
                    g.len()
                )));
            }
            Err(Blocked::Staff) => {
                return Err(AdminError::Conflict(
                    "The account is on the staff; the appointment must end first".into(),
                ));
            }
            Err(Blocked::Owner) => {
                return Err(AdminError::Conflict(
                    "The account is the platform Owner".into(),
                ));
            }
        }
        // N-06: the member also asked for their messages. Queued after the account is erased, so a
        // blocked erasure leaves everything as it was; the job finds the messages by author id.
        if opts.delete_messages {
            let scope = crate::self_delete::Scope {
                servers: true,
                dms: true,
                before_ms: None,
            };
            let visible = crate::self_delete::count_visible(self.pool(), r.account_id, scope)
                .await
                .map_err(|e| AdminError::Storage(e.to_string()))?;
            // An open run already covers them; the staff run skips only the 24-hour limit.
            let _ = crate::self_delete::request(
                self.pool(),
                self.next_id(),
                self.next_id(),
                r.account_id,
                scope,
                crate::self_delete::Via::Staff,
            )
            .await
            .map_err(|e| AdminError::Storage(e.to_string()))?;
            report.messages_queued = Some(visible);
        }
        let outcome = erasure_outcome(&report, opts.invoice_years);
        let mut tx = self.pool().begin().await.map_err(db)?;
        let done = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE data_requests SET state = 'completed', outcome = $2, retained = $3, completed_at = {} \
             WHERE id = $1 AND state = 'verified'",
            from_ms("$4")
        )))
        .bind(id)
        .bind(&outcome)
        .bind(serde_json::json!(retained_ids()))
        .bind(now_ms)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        if done.rows_affected() == 0 {
            return Err(AdminError::Conflict(
                "The request changed while it was running".into(),
            ));
        }
        // The account's pending staff deletion stub, if any, is done too.
        sqlx::query(
            "UPDATE account_delete_requests SET state = 'completed', resolved_at = now() \
             WHERE account_id = $1 AND state = 'pending'",
        )
        .bind(r.account_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        self.audit_in(&mut tx, audit).await?;
        let request = load(&mut tx, id, false).await?;
        tx.commit().await.map_err(db)?;
        Ok(ErasureRun { request, report })
    }
}
