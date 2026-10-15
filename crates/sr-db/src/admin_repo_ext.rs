//! Postgres side of the phase-10 admin features: delegations, account edits and the
//! [`StaffExtrasRepository`] (offers, announcements, alerts, private reads).
//! Tests: `tests/admin_repo.rs` (needs `SR_TEST_DATABASE_URL`).

use async_trait::async_trait;
use sqlx::{PgConnection, Row, postgres::PgRow};
use sr_admin::{
    AccountRef, AdminError, AdminResult, Announcement, AnnouncementPatch, AnnouncementSeverity,
    AuditEntry, NewAnnouncement, NewOffer, OfferState, PrivateChannel, PrivateEdit, PrivateMessage,
    StaffAlertRow, StaffExtrasRepository, StaffOffer,
};

use crate::admin_repo::{PgAdminRepo, db, is_unique, ms};

fn from_ms(col: &str) -> String {
    format!("to_timestamp({col}::float8 / 1000.0)")
}

fn offer_cols() -> String {
    format!(
        "o.id, o.account_id, a.username::text AS username, o.offered_by, \
         b.username::text AS offered_by_name, o.role_ids, o.reason, o.state, \
         {} AS created_ms, {} AS expires_ms, {} AS responded_ms, o.dm_channel_id, o.dm_message_id, \
         COALESCE((SELECT array_agg(r.name ORDER BY r.position) FROM roles r \
                   WHERE r.id = ANY(o.role_ids)), '{{}}') AS role_names",
        ms("o.created_at"),
        ms("o.expires_at"),
        ms("o.responded_at")
    )
}

const OFFER_FROM: &str = "FROM staff_appointment_offers o \
     JOIN accounts a ON a.id = o.account_id JOIN accounts b ON b.id = o.offered_by";

fn offer_from(r: &PgRow) -> Result<StaffOffer, sqlx::Error> {
    let role_ids: Vec<i64> = r.try_get("role_ids")?;
    Ok(StaffOffer {
        id: r.try_get("id")?,
        account_id: r.try_get("account_id")?,
        username: r.try_get("username")?,
        offered_by: r.try_get("offered_by")?,
        offered_by_name: r.try_get("offered_by_name")?,
        role_ids: role_ids.iter().map(i64::to_string).collect(),
        role_names: r.try_get("role_names")?,
        reason: r.try_get("reason")?,
        state: OfferState::parse(&r.try_get::<String, _>("state")?).unwrap_or(OfferState::Expired),
        created_at_ms: r.try_get("created_ms")?,
        expires_at_ms: r.try_get("expires_ms")?,
        responded_at_ms: r.try_get("responded_ms")?,
        dm_channel_id: r.try_get("dm_channel_id")?,
        dm_message_id: r.try_get("dm_message_id")?,
    })
}

fn announcement_cols() -> String {
    format!(
        "id, title, body, severity, send_dm, created_by, {} AS created_ms, {} AS starts_ms, \
         {} AS ends_ms, {} AS ended_ms",
        ms("created_at"),
        ms("starts_at"),
        ms("ends_at"),
        ms("ended_at")
    )
}

fn announcement_from(r: &PgRow) -> Result<Announcement, sqlx::Error> {
    Ok(Announcement {
        id: r.try_get("id")?,
        title: r.try_get("title")?,
        body: r.try_get("body")?,
        severity: AnnouncementSeverity::parse(&r.try_get::<String, _>("severity")?)
            .unwrap_or(AnnouncementSeverity::Info),
        send_dm: r.try_get("send_dm")?,
        created_by: r.try_get("created_by")?,
        created_at_ms: r.try_get("created_ms")?,
        starts_at_ms: r.try_get("starts_ms")?,
        ends_at_ms: r.try_get("ends_ms")?,
        ended_at_ms: r.try_get("ended_ms")?,
    })
}

fn alert_cols() -> String {
    format!(
        "id, kind, detail, {} AS created_ms, acked_by, {} AS acked_ms",
        ms("created_at"),
        ms("acked_at")
    )
}

fn alert_from(r: &PgRow) -> Result<StaffAlertRow, sqlx::Error> {
    Ok(StaffAlertRow {
        id: r.try_get("id")?,
        kind: r.try_get("kind")?,
        detail: r.try_get("detail")?,
        created_at_ms: r.try_get("created_ms")?,
        acked_by: r.try_get("acked_by")?,
        acked_at_ms: r.try_get("acked_ms")?,
    })
}

impl PgAdminRepo {
    pub(crate) async fn load_offer(conn: &mut PgConnection, id: i64) -> AdminResult<StaffOffer> {
        let r = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {} {OFFER_FROM} WHERE o.id = $1",
            offer_cols()
        )))
        .bind(id)
        .fetch_optional(conn)
        .await
        .map_err(db)?
        .ok_or(AdminError::NotFound("appointment offer"))?;
        offer_from(&r).map_err(db)
    }

    /// Recipients of a DM or group DM as account refs.
    async fn recipients(conn: &mut PgConnection, channel: i64) -> AdminResult<Vec<AccountRef>> {
        let rows = sqlx::query(
            "SELECT a.id, a.username::text AS username FROM channel_recipients cr \
             JOIN accounts a ON a.id = cr.user_id WHERE cr.channel_id = $1 ORDER BY a.id",
        )
        .bind(channel)
        .fetch_all(conn)
        .await
        .map_err(db)?;
        rows.iter()
            .map(|r| {
                Ok(AccountRef {
                    id: r.try_get("id")?,
                    username: r.try_get("username")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(db)
    }

    async fn channel_from(conn: &mut PgConnection, r: &PgRow) -> AdminResult<PrivateChannel> {
        let id: i64 = r.try_get("id").map_err(db)?;
        let kind: i16 = r.try_get("type").map_err(db)?;
        let guild_id: Option<i64> = r.try_get("guild_id").map_err(db)?;
        let recipients = if guild_id.is_none() {
            Self::recipients(conn, id).await?
        } else {
            Vec::new()
        };
        Ok(PrivateChannel {
            id,
            kind,
            guild_id,
            name: r.try_get("name").map_err(db)?,
            recipients,
            private: r.try_get("private").map_err(db)?,
        })
    }
}

/// `@everyone` cannot view the channel: a channel overwrite denies VIEW_CHANNEL (bit 10)
/// to the role whose id is the guild id.
const PRIVATE_FLAG: &str = "(c.guild_id IS NULL OR EXISTS ( \
     SELECT 1 FROM permission_overwrites ow \
     WHERE ow.channel_id IN (c.id, c.parent_id) AND ow.target_id = c.guild_id \
       AND ow.target_type = 'role' AND (ow.deny & 1024) <> 0))";

const CHANNEL_SELECT: &str = "SELECT c.id, c.type, c.guild_id, c.name";

#[async_trait]
impl StaffExtrasRepository for PgAdminRepo {
    // ---- appointment offers ------------------------------------------------

    async fn create_offer(&self, o: &NewOffer, audit: &AuditEntry) -> AdminResult<StaffOffer> {
        let mut tx = self.pool().begin().await.map_err(db)?;
        let id = self.next_id();
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO staff_appointment_offers (id, account_id, offered_by, role_ids, reason, expires_at) \
             VALUES ($1, $2, $3, $4, $5, {})",
            from_ms("$6")
        )))
        .bind(id)
        .bind(o.account_id)
        .bind(o.offered_by)
        .bind(&o.role_ids)
        .bind(&o.reason)
        .bind(o.expires_at_ms)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if is_unique(&e) {
                AdminError::Conflict("The account already has a pending offer".into())
            } else {
                db(e)
            }
        })?;
        self.audit_in(&mut tx, audit).await?;
        let offer = Self::load_offer(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(offer)
    }

    async fn get_offer(&self, id: i64) -> AdminResult<Option<StaffOffer>> {
        let mut c = self.pool().acquire().await.map_err(db)?;
        match Self::load_offer(&mut c, id).await {
            Ok(o) => Ok(Some(o)),
            Err(AdminError::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn list_offers(&self, pending_only: bool, limit: i64) -> AdminResult<Vec<StaffOffer>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {} {OFFER_FROM} WHERE (NOT $1 OR o.state = 'pending') \
             ORDER BY o.id DESC LIMIT $2",
            offer_cols()
        )))
        .bind(pending_only)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(db)?;
        rows.iter()
            .map(offer_from)
            .collect::<Result<_, _>>()
            .map_err(db)
    }

    async fn close_offer(
        &self,
        id: i64,
        to: OfferState,
        audit: &AuditEntry,
    ) -> AdminResult<StaffOffer> {
        let mut tx = self.pool().begin().await.map_err(db)?;
        let n = sqlx::query(
            "UPDATE staff_appointment_offers SET state = $2, responded_at = now() \
             WHERE id = $1 AND state = 'pending'",
        )
        .bind(id)
        .bind(to.as_str())
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        if n == 0 {
            let current = Self::load_offer(&mut tx, id).await?;
            return Err(AdminError::Conflict(format!(
                "This offer was already {}",
                current.state.as_str()
            )));
        }
        self.audit_in(&mut tx, audit).await?;
        let offer = Self::load_offer(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(offer)
    }

    async fn set_offer_message(
        &self,
        id: i64,
        channel_id: i64,
        message_id: i64,
    ) -> AdminResult<()> {
        sqlx::query(
            "UPDATE staff_appointment_offers SET dm_channel_id = $2, dm_message_id = $3 WHERE id = $1",
        )
        .bind(id)
        .bind(channel_id)
        .bind(message_id)
        .execute(self.pool())
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn expire_offers(&self, now_ms: i64) -> AdminResult<Vec<StaffOffer>> {
        let ids: Vec<i64> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "UPDATE staff_appointment_offers SET state = 'expired', responded_at = now() \
             WHERE state = 'pending' AND expires_at <= {} RETURNING id",
            from_ms("$1")
        )))
        .bind(now_ms)
        .fetch_all(self.pool())
        .await
        .map_err(db)?;
        let mut c = self.pool().acquire().await.map_err(db)?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            out.push(Self::load_offer(&mut c, id).await?);
        }
        Ok(out)
    }

    // ---- announcements -----------------------------------------------------

    async fn create_announcement(
        &self,
        a: &NewAnnouncement,
        audit: &AuditEntry,
    ) -> AdminResult<Announcement> {
        let mut tx = self.pool().begin().await.map_err(db)?;
        let id = self.next_id();
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO platform_announcements \
               (id, title, body, severity, send_dm, created_by, starts_at, ends_at) \
             VALUES ($1, $2, $3, $4, $5, $6, {}, CASE WHEN $8::bigint IS NULL THEN NULL ELSE {} END)",
            from_ms("$7"),
            from_ms("$8")
        )))
        .bind(id)
        .bind(&a.title)
        .bind(&a.body)
        .bind(a.severity.as_str())
        .bind(a.send_dm)
        .bind(a.created_by)
        .bind(a.starts_at_ms)
        .bind(a.ends_at_ms)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        self.audit_in(&mut tx, audit).await?;
        let r = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM platform_announcements WHERE id = $1",
            announcement_cols()
        )))
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        announcement_from(&r).map_err(db)
    }

    async fn end_announcement(&self, id: i64, audit: &AuditEntry) -> AdminResult<Announcement> {
        let mut tx = self.pool().begin().await.map_err(db)?;
        let r = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE platform_announcements SET ended_at = COALESCE(ended_at, now()) \
             WHERE id = $1 RETURNING {}",
            announcement_cols()
        )))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or(AdminError::NotFound("announcement"))?;
        self.audit_in(&mut tx, audit).await?;
        tx.commit().await.map_err(db)?;
        announcement_from(&r).map_err(db)
    }

    async fn get_announcement(&self, id: i64) -> AdminResult<Announcement> {
        let r = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM platform_announcements WHERE id = $1",
            announcement_cols()
        )))
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(db)?
        .ok_or(AdminError::NotFound("announcement"))?;
        announcement_from(&r).map_err(db)
    }

    async fn edit_announcement(
        &self,
        id: i64,
        patch: &AnnouncementPatch,
        now_ms: i64,
        audit: &AuditEntry,
    ) -> AdminResult<Announcement> {
        let mut tx = self.pool().begin().await.map_err(db)?;
        // Lock the row so an edit and an end cannot cross.
        let cur = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM platform_announcements WHERE id = $1 FOR UPDATE",
            announcement_cols()
        )))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or(AdminError::NotFound("announcement"))?;
        let cur = announcement_from(&cur).map_err(db)?;
        if cur.ended_at_ms.is_some() || cur.ends_at_ms.is_some_and(|e| e <= now_ms) {
            return Err(AdminError::Conflict("this announcement has ended".into()));
        }
        let r = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE platform_announcements SET title = COALESCE($2, title), body = COALESCE($3, body),                ends_at = CASE WHEN $4::bigint IS NULL THEN ends_at ELSE {} END              WHERE id = $1 RETURNING {}",
            from_ms("$4"),
            announcement_cols()
        )))
        .bind(id)
        .bind(patch.title.as_deref())
        .bind(patch.body.as_deref())
        .bind(patch.ends_at_ms)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        self.audit_in(&mut tx, audit).await?;
        tx.commit().await.map_err(db)?;
        announcement_from(&r).map_err(db)
    }

    async fn list_announcements(&self, limit: i64) -> AdminResult<Vec<Announcement>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM platform_announcements ORDER BY id DESC LIMIT $1",
            announcement_cols()
        )))
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(db)?;
        rows.iter()
            .map(announcement_from)
            .collect::<Result<_, _>>()
            .map_err(db)
    }

    async fn active_announcements(&self, now_ms: i64) -> AdminResult<Vec<Announcement>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM platform_announcements \
             WHERE ended_at IS NULL AND starts_at <= {t} AND (ends_at IS NULL OR ends_at > {t}) \
             ORDER BY id DESC",
            announcement_cols(),
            t = from_ms("$1")
        )))
        .bind(now_ms)
        .fetch_all(self.pool())
        .await
        .map_err(db)?;
        rows.iter()
            .map(announcement_from)
            .collect::<Result<_, _>>()
            .map_err(db)
    }

    // ---- alerts --------------------------------------------------------------

    async fn record_alert(&self, kind: &str, detail: &str) -> AdminResult<StaffAlertRow> {
        let r = sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO staff_alerts (id, kind, detail) VALUES ($1, $2, $3) RETURNING {}",
            alert_cols()
        )))
        .bind(self.next_id())
        .bind(kind)
        .bind(detail)
        .fetch_one(self.pool())
        .await
        .map_err(db)?;
        alert_from(&r).map_err(db)
    }

    async fn recent_alerts(&self, limit: i64) -> AdminResult<Vec<StaffAlertRow>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM staff_alerts ORDER BY id DESC LIMIT $1",
            alert_cols()
        )))
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(db)?;
        rows.iter()
            .map(alert_from)
            .collect::<Result<_, _>>()
            .map_err(db)
    }

    async fn ack_alert(&self, id: i64, by: i64, audit: &AuditEntry) -> AdminResult<StaffAlertRow> {
        let mut tx = self.pool().begin().await.map_err(db)?;
        let r = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE staff_alerts SET acked_by = COALESCE(acked_by, $2), \
                    acked_at = COALESCE(acked_at, now()) WHERE id = $1 RETURNING {}",
            alert_cols()
        )))
        .bind(id)
        .bind(by)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or(AdminError::NotFound("alert"))?;
        self.audit_in(&mut tx, audit).await?;
        tx.commit().await.map_err(db)?;
        alert_from(&r).map_err(db)
    }

    // ---- private content -------------------------------------------------------

    async fn private_channel(&self, channel_id: i64) -> AdminResult<Option<PrivateChannel>> {
        let mut c = self.pool().acquire().await.map_err(db)?;
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{CHANNEL_SELECT}, {PRIVATE_FLAG} AS private FROM channels c WHERE c.id = $1"
        )))
        .bind(channel_id)
        .fetch_optional(&mut *c)
        .await
        .map_err(db)?;
        match row {
            Some(r) => Ok(Some(Self::channel_from(&mut c, &r).await?)),
            None => Ok(None),
        }
    }

    async fn dm_channels_of(&self, account_id: i64) -> AdminResult<Vec<PrivateChannel>> {
        let mut c = self.pool().acquire().await.map_err(db)?;
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{CHANNEL_SELECT}, TRUE AS private FROM channels c \
             JOIN channel_recipients cr ON cr.channel_id = c.id AND cr.user_id = $1 \
             WHERE c.guild_id IS NULL ORDER BY c.id DESC LIMIT 500"
        )))
        .bind(account_id)
        .fetch_all(&mut *c)
        .await
        .map_err(db)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            out.push(Self::channel_from(&mut c, r).await?);
        }
        Ok(out)
    }

    async fn private_channels_of_guild(&self, guild_id: i64) -> AdminResult<Vec<PrivateChannel>> {
        let mut c = self.pool().acquire().await.map_err(db)?;
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{CHANNEL_SELECT}, TRUE AS private FROM channels c \
             WHERE c.guild_id = $1 AND c.type NOT IN (4) AND {PRIVATE_FLAG} ORDER BY c.position, c.id"
        )))
        .bind(guild_id)
        .fetch_all(&mut *c)
        .await
        .map_err(db)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            out.push(Self::channel_from(&mut c, r).await?);
        }
        Ok(out)
    }

    async fn private_messages(
        &self,
        channel_id: i64,
        before: Option<i64>,
        limit: i64,
        deleted_days: i64,
    ) -> AdminResult<Vec<PrivateMessage>> {
        // Deleted messages stay readable for the retention window; a message on legal hold
        // (`hold_id`) is kept until its case closes, so staff can read it as well.
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT m.id, m.author_id, COALESCE(a.display_name, a.username::text) AS author_name, \
                    m.content, {created} AS created_ms, {edited} AS edited_ms, \
                    m.visibility::text AS visibility, {deleted} AS deleted_ms, \
                    (SELECT count(*) FROM attachments att WHERE att.message_id = m.id)::int AS attachments \
             FROM messages m LEFT JOIN accounts a ON a.id = m.author_id \
             WHERE m.channel_id = $1 AND ($2::bigint IS NULL OR m.id < $2) \
               AND (m.visibility = 'visible' \
                    OR m.deleted_at > now() - make_interval(days => $3::int) \
                    OR m.hold_id IS NOT NULL) \
             ORDER BY m.id DESC LIMIT $4",
            created = ms("m.created_at"),
            edited = ms("m.edited_at"),
            deleted = ms("m.deleted_at"),
        )))
        .bind(channel_id)
        .bind(before)
        .bind(i32::try_from(deleted_days).unwrap_or(30))
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(db)?;
        let ids: Vec<i64> = rows.iter().map(|r| r.get::<i64, _>("id")).collect();
        let edit_rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT message_id, {} AS at_ms, previous_content FROM message_edits \
             WHERE message_id = ANY($1) ORDER BY edited_at",
            ms("edited_at")
        )))
        .bind(&ids)
        .fetch_all(self.pool())
        .await
        .map_err(db)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let id: i64 = r.try_get("id").map_err(db)?;
            let mut edits = Vec::new();
            for e in edit_rows
                .iter()
                .filter(|e| e.get::<i64, _>("message_id") == id)
            {
                edits.push(PrivateEdit {
                    edited_at_ms: e.try_get("at_ms").map_err(db)?,
                    previous_content: e.try_get("previous_content").map_err(db)?,
                });
            }
            out.push(PrivateMessage {
                id,
                author_id: r.try_get("author_id").map_err(db)?,
                author_name: r
                    .try_get::<Option<String>, _>("author_name")
                    .map_err(db)?
                    .unwrap_or_default(),
                content: r.try_get("content").map_err(db)?,
                created_at_ms: r.try_get("created_ms").map_err(db)?,
                edited_at_ms: r.try_get("edited_ms").map_err(db)?,
                visibility: r.try_get("visibility").map_err(db)?,
                deleted_at_ms: r.try_get("deleted_ms").map_err(db)?,
                attachment_count: r.try_get("attachments").map_err(db)?,
                edits,
            });
        }
        Ok(out)
    }
}
