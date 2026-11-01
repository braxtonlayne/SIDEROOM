//! MOD-GRD-001: the per-member half of the graduated toolset (D-395): warn, timeout, timed invite freeze, revoking
//! unused invite codes, the restriction list with its appeals, and the checks the rest of the server asks
//! ("may this account post, join voice, create an invite right now?").
//!
//! Every staff act writes the hash-chained access log in the same transaction as the row it changes. A warning or
//! a timeout carries a strike weight: 1.0 times the account's accountability multiplier (the 44-B seam,
//! [`Moderation::strike_weight_multiplier`]).

use serde_json::json;
use sqlx::{PgConnection, Row, postgres::PgRow};
use sr_admin::{AccessKind, Caller};
use sr_core::PlatformPermissions as P;

use crate::error::{ModError, ModResult, conflict};
use crate::graduated_model::{MyRestriction, RestrictionView, RevokedInvites};
use crate::graduated_rules as gr;
use crate::model::{AppealDecision, Who};
use crate::rules;
use crate::service::{Moderation, log_staff, require_staff};

/// The system user's account id (the actor of the automatic freeze).
pub const SYSTEM_ACTOR: i64 = 1;

/// What an enforcement check asks about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocked {
    /// Sending messages and the other ways of posting.
    Posting,
    /// Joining a voice channel.
    Voice,
    /// Creating an invite code.
    Invites,
}

impl Blocked {
    fn kinds(self) -> &'static [&'static str] {
        match self {
            Self::Posting | Self::Voice => &["timeout", "case_freeze"],
            Self::Invites => &["invite_freeze", "case_freeze", "sterilization"],
        }
    }
}

/// How long a restriction lasts: months first, then seconds.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Span {
    pub months: i32,
    pub secs: i64,
}

impl Span {
    pub const fn secs(secs: i64) -> Self {
        Self { months: 0, secs }
    }
    pub const fn days(d: i64) -> Self {
        Self::secs(d * 86_400)
    }
    pub const fn months(months: i32) -> Self {
        Self { months, secs: 0 }
    }
}

pub(crate) const R_COLS: &str = "id, account_id, kind, reason, case_id, imposed_by, approved_by, weight, \
    (extract(epoch FROM starts_at) * 1000)::bigint AS starts_ms, (extract(epoch FROM ends_at) * 1000)::bigint AS ends_ms, \
    removes_account, state, appeal_message, appeal_state";

pub(crate) fn restriction_from(r: &PgRow) -> ModResult<RestrictionView> {
    Ok(RestrictionView {
        id: r.try_get("id")?,
        account_id: r.try_get("account_id")?,
        kind: r.try_get("kind")?,
        reason: r.try_get("reason")?,
        case_id: r.try_get("case_id")?,
        imposed_by: r.try_get("imposed_by")?,
        approved_by: r.try_get("approved_by")?,
        weight: r.try_get("weight")?,
        starts_at_ms: r.try_get("starts_ms")?,
        ends_at_ms: r.try_get("ends_ms")?,
        removes_account: r.try_get("removes_account")?,
        state: r.try_get("state")?,
        appeal_message: r.try_get("appeal_message")?,
        appeal_state: r.try_get("appeal_state")?,
    })
}

/// Insert a restriction row on this connection; the caller writes the audit entry.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn insert_restriction(
    conn: &mut PgConnection,
    id: i64,
    account: i64,
    kind: &str,
    reason: &str,
    case_id: Option<i64>,
    imposed_by: i64,
    approved_by: Option<i64>,
    weight: f32,
    span: Span,
    removes_account: bool,
) -> ModResult<()> {
    sqlx::query(
        "INSERT INTO member_restrictions (id, account_id, kind, reason, case_id, imposed_by, approved_by, weight, \
         ends_at, removes_account) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now() + make_interval(months => $9, secs => $10::double precision), $11)",
    )
    .bind(id)
    .bind(account)
    .bind(kind)
    .bind(reason)
    .bind(case_id)
    .bind(imposed_by)
    .bind(approved_by)
    .bind(weight)
    .bind(span.months)
    .bind(span.secs as f64)
    .bind(removes_account)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Whether the account is a staff member (an appointment, or the Staff Server's owner).
pub(crate) async fn is_staff_account(conn: &mut PgConnection, account: i64) -> ModResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM staff_appointments WHERE account_id = $1 AND revoked_at IS NULL) \
            OR EXISTS (SELECT 1 FROM guilds WHERE is_staff_guild AND owner_id = $1)",
    )
    .bind(account)
    .fetch_one(&mut *conn)
    .await?)
}

/// The staff caller of a [`Who`], or `Forbidden` for a server moderator.
/// The platform Operator (D-395): the Owner, or a staff member the Owner granted `OPERATOR` (an Owner-only bit,
/// `sr_admin::rules::OWNER_ONLY`; the same "Operator" as the infrastructure routes). The branch action and the
/// drill are theirs alone; every other staff member, whatever else they hold, is refused.
pub fn is_operator(c: &Caller) -> bool {
    c.is_owner || c.permissions.has(P::OPERATOR)
}

pub(crate) fn require_operator(c: &Caller) -> ModResult<()> {
    if is_operator(c) {
        Ok(())
    } else {
        Err(ModError::MissingPermission("OPERATOR"))
    }
}

pub(crate) fn caller_of<'a>(who: &'a Who<'_>) -> ModResult<&'a Caller> {
    match who {
        Who::Staff(c) => Ok(c),
        Who::GuildMod { .. } => Err(ModError::Forbidden),
    }
}

/// An account staff may act on: it exists and is not deleted, is not the system user, is not the actor, and is
/// not a staff account unless the actor is the Owner (the rule of `AdminService::guard_account`).
pub(crate) async fn guard_target(
    conn: &mut PgConnection,
    c: &Caller,
    account: i64,
) -> ModResult<()> {
    let state: Option<String> =
        sqlx::query_scalar("SELECT state::text FROM accounts WHERE id = $1")
            .bind(account)
            .fetch_optional(&mut *conn)
            .await?;
    match state.as_deref() {
        None | Some("deleted") => return Err(ModError::NotFound("account")),
        _ => {}
    }
    if account == SYSTEM_ACTOR {
        return Err(ModError::Forbidden);
    }
    if account == c.user_id {
        return Err(ModError::Forbidden);
    }
    if !c.is_owner && is_staff_account(conn, account).await? {
        return Err(ModError::Forbidden);
    }
    Ok(())
}

impl Moderation {
    async fn restriction_by_id(
        &self,
        conn: &mut PgConnection,
        id: i64,
        lock: bool,
    ) -> ModResult<RestrictionView> {
        let sql = format!(
            "SELECT {R_COLS} FROM member_restrictions WHERE id = $1{}",
            if lock { " FOR UPDATE" } else { "" }
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or(ModError::NotFound("restriction"))?;
        restriction_from(&row)
    }

    /// Send the member the plain words of a warning or timeout. A failure to deliver never fails the act.
    async fn tell_member(&self, account: i64, text: &str) {
        let _ = self.dm_subject(account, text).await;
    }

    /// Warn a member (`MANAGE_CASES`). The warning stands on record for [`gr::WARNING_RECORD_DAYS`] days and
    /// carries the strike weight of the moment.
    pub async fn warn_member(
        &self,
        who: &Who<'_>,
        account: i64,
        reason: Option<&str>,
    ) -> ModResult<RestrictionView> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let c = caller_of(who)?;
        let reason = rules::reason(reason)?;
        let weight = self.strike_weight_multiplier(account).await;
        let id = self.next_id();
        let mut tx = self.pool.begin().await?;
        guard_target(&mut tx, c, account).await?;
        insert_restriction(
            &mut tx,
            id,
            account,
            "warn",
            &reason,
            None,
            c.user_id,
            None,
            weight,
            Span::days(gr::WARNING_RECORD_DAYS),
            false,
        )
        .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.member.warn",
            ("account", account),
            &reason,
            json!({ "restriction_id": id.to_string(), "weight": weight }),
        )
        .await?;
        let v = self.restriction_by_id(&mut tx, id, false).await?;
        tx.commit().await?;
        self.tell_member(
            account,
            &format!("You have received a warning from the SIDE ROOM moderators: {reason}"),
        )
        .await;
        Ok(v)
    }

    /// Time a member out of posting and voice for `minutes` (`MANAGE_CASES`).
    pub async fn timeout_member(
        &self,
        who: &Who<'_>,
        account: i64,
        minutes: i64,
        reason: Option<&str>,
    ) -> ModResult<RestrictionView> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let c = caller_of(who)?;
        let minutes = gr::timeout_minutes(minutes)?;
        let reason = rules::reason(reason)?;
        let weight = self.strike_weight_multiplier(account).await;
        let id = self.next_id();
        let mut tx = self.pool.begin().await?;
        guard_target(&mut tx, c, account).await?;
        if self.blocked_in(&mut tx, account, &["timeout"]).await? {
            return Err(conflict("The member is already timed out"));
        }
        insert_restriction(
            &mut tx,
            id,
            account,
            "timeout",
            &reason,
            None,
            c.user_id,
            None,
            weight,
            Span::secs(minutes * 60),
            false,
        )
        .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.member.timeout",
            ("account", account),
            &reason,
            json!({ "restriction_id": id.to_string(), "minutes": minutes, "weight": weight }),
        )
        .await?;
        let v = self.restriction_by_id(&mut tx, id, false).await?;
        tx.commit().await?;
        self.tell_member(
            account,
            &format!(
                "A SIDE ROOM moderator has timed you out for {minutes} minutes. You cannot post or join voice until it ends. Reason: {reason}"
            ),
        )
        .await;
        Ok(v)
    }

    /// Freeze a member's invite creation for `days` (`MANAGE_INVITES`). Replaces nothing: a second freeze while
    /// one is active is a conflict (lift it first).
    pub async fn freeze_member_invites(
        &self,
        who: &Who<'_>,
        account: i64,
        days: i64,
        reason: Option<&str>,
    ) -> ModResult<RestrictionView> {
        require_staff(who, P::MANAGE_INVITES, "MANAGE_INVITES")?;
        let c = caller_of(who)?;
        let days = gr::invite_freeze_days(days)?;
        let reason = rules::reason(reason)?;
        let id = self.next_id();
        let mut tx = self.pool.begin().await?;
        guard_target(&mut tx, c, account).await?;
        if self
            .blocked_in(&mut tx, account, &["invite_freeze"])
            .await?
        {
            return Err(conflict("The member's invites are already frozen"));
        }
        insert_restriction(
            &mut tx,
            id,
            account,
            "invite_freeze",
            &reason,
            None,
            c.user_id,
            None,
            1.0,
            Span::days(days),
            false,
        )
        .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.member.invite_freeze",
            ("account", account),
            &reason,
            json!({ "restriction_id": id.to_string(), "days": days }),
        )
        .await?;
        let v = self.restriction_by_id(&mut tx, id, false).await?;
        tx.commit().await?;
        Ok(v)
    }

    /// Revoke every unused invite code the member holds (`MANAGE_INVITES`). Credits are only spent when a code is
    /// redeemed, so nothing is refunded and nothing is lost. The invite graph is never touched.
    pub async fn revoke_unused_invites(
        &self,
        who: &Who<'_>,
        account: i64,
        reason: Option<&str>,
    ) -> ModResult<RevokedInvites> {
        require_staff(who, P::MANAGE_INVITES, "MANAGE_INVITES")?;
        let c = caller_of(who)?;
        let reason = rules::reason(reason)?;
        let mut tx = self.pool.begin().await?;
        guard_target(&mut tx, c, account).await?;
        let n = revoke_open_codes(&mut tx, account).await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.member.revoke_invites",
            ("account", account),
            &reason,
            json!({ "codes_revoked": n }),
        )
        .await?;
        tx.commit().await?;
        Ok(RevokedInvites { codes_revoked: n })
    }

    /// Stamp the `ban_reason_category` on a ban made through the admin service (it cannot write the column itself).
    pub async fn stamp_ban_reason(&self, ban_id: i64, category: &str) -> ModResult<()> {
        let category = gr::reason_category(category)?;
        sqlx::query(
            "UPDATE person_bans SET reason_category = $2::ban_reason_category WHERE id = $1 AND reason_category IS NULL",
        )
        .bind(ban_id)
        .bind(category)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Every restriction and warning on an account, newest first (`MANAGE_CASES`; the read is logged).
    pub async fn member_history(
        &self,
        who: &Who<'_>,
        account: i64,
    ) -> ModResult<Vec<RestrictionView>> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let sql = format!(
            "SELECT {R_COLS} FROM member_restrictions WHERE account_id = $1 ORDER BY id DESC LIMIT 200"
        );
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(account)
            .fetch_all(&mut *tx)
            .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::Read,
            "moderation.member.history",
            ("account", account),
            "",
            json!({}),
        )
        .await?;
        tx.commit().await?;
        rows.iter().map(restriction_from).collect()
    }

    /// Lift a restriction early (`MANAGE_CASES`). A reason is required.
    pub async fn lift_restriction(
        &self,
        who: &Who<'_>,
        id: i64,
        reason: Option<&str>,
    ) -> ModResult<RestrictionView> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let c = caller_of(who)?;
        let reason = rules::reason(reason)?;
        let mut tx = self.pool.begin().await?;
        let r = self.restriction_by_id(&mut tx, id, true).await?;
        if r.state != "active" {
            return Err(conflict("That restriction is no longer active"));
        }
        sqlx::query(
            "UPDATE member_restrictions SET state = 'lifted', lifted_by = $2, lifted_at = now(), lift_reason = $3 \
             WHERE id = $1",
        )
        .bind(id)
        .bind(c.user_id)
        .bind(&reason)
        .execute(&mut *tx)
        .await?;
        if r.kind == "case_freeze" {
            sqlx::query(
                "UPDATE moderation_subtree_nodes SET status = 'cleared', decided_by = $2, decided_at = now(), note = $3 \
                 WHERE account_id = $1 AND case_id = $4 AND status IN ('auto_frozen', 'frozen')",
            )
            .bind(r.account_id)
            .bind(c.user_id)
            .bind(&reason)
            .bind(r.case_id)
            .execute(&mut *tx)
            .await?;
        }
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.restriction.lift",
            ("account", r.account_id),
            &reason,
            json!({ "restriction_id": id.to_string(), "kind": r.kind, "case_id": r.case_id.map(|c| c.to_string()) }),
        )
        .await?;
        let v = self.restriction_by_id(&mut tx, id, false).await?;
        tx.commit().await?;
        Ok(v)
    }

    // ---- what the member sees and does -----------------------------------------------------------------------

    /// The restrictions on the member's own account that are in force or appealed: the plain words, no case, no
    /// staff member, no tree.
    pub async fn my_restrictions(&self, account: i64) -> ModResult<Vec<MyRestriction>> {
        let sql = format!(
            "SELECT {R_COLS} FROM member_restrictions WHERE account_id = $1 AND (state = 'active' AND ends_at > now() \
             OR appeal_state = 'open') ORDER BY id DESC LIMIT 50"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(account)
            .fetch_all(&self.pool)
            .await?;
        rows.iter()
            .map(|r| restriction_from(r).map(|v| my_view(&v)))
            .collect()
    }

    /// File the one appeal of a restriction on the member's own account. Works while frozen (a frozen account
    /// can log in and appeal). A warning can be appealed too.
    pub async fn appeal_restriction(
        &self,
        account: i64,
        id: i64,
        message: &str,
    ) -> ModResult<MyRestriction> {
        let message = gr::appeal_message(message)?;
        let mut tx = self.pool.begin().await?;
        let r = self.restriction_by_id(&mut tx, id, true).await?;
        if r.account_id != account {
            // Another member's row is simply not there.
            return Err(ModError::NotFound("restriction"));
        }
        if r.state != "active" {
            return Err(conflict("That decision is no longer in force"));
        }
        if r.appeal_state.is_some() {
            return Err(conflict("This decision was already appealed"));
        }
        sqlx::query(
            "UPDATE member_restrictions SET appeal_message = $2, appealed_at = now(), appeal_state = 'open' WHERE id = $1",
        )
        .bind(id)
        .bind(&message)
        .execute(&mut *tx)
        .await?;
        let v = self.restriction_by_id(&mut tx, id, false).await?;
        tx.commit().await?;
        Ok(my_view(&v))
    }

    /// Restriction appeals waiting for a decision, oldest first (`MANAGE_CASES`).
    pub async fn open_restriction_appeals(&self, who: &Who<'_>) -> ModResult<Vec<RestrictionView>> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let sql = format!(
            "SELECT {R_COLS} FROM member_restrictions WHERE appeal_state = 'open' ORDER BY appealed_at LIMIT 200"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(restriction_from).collect()
    }

    /// Decide an appeal of a restriction (`MANAGE_CASES`): a staff member other than the one who imposed it (or
    /// the Owner when nobody else can). `Overturned` ends the restriction. Same rule as the ban appeals (D-215).
    pub async fn decide_restriction_appeal(
        &self,
        who: &Who<'_>,
        id: i64,
        decision: AppealDecision,
        reason: Option<&str>,
        another_staff_can_review: bool,
    ) -> ModResult<RestrictionView> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let c = caller_of(who)?;
        let reason = rules::reason(reason)?;
        let mut tx = self.pool.begin().await?;
        let r = self.restriction_by_id(&mut tx, id, true).await?;
        if r.appeal_state.as_deref() != Some("open") {
            return Err(conflict("There is no open appeal on that restriction"));
        }
        let imposer = (r.imposed_by != SYSTEM_ACTOR).then_some(r.imposed_by);
        let approver_is_me = r.approved_by == Some(c.user_id);
        if !rules::appeal_reviewer_ok(c.user_id, imposer, c.is_owner, another_staff_can_review)
            || (approver_is_me && !(c.is_owner && !another_staff_can_review))
        {
            return Err(ModError::Forbidden);
        }
        sqlx::query(
            "UPDATE member_restrictions SET appeal_state = $2, appeal_decided_by = $3, appeal_decided_at = now(), \
             appeal_reason = $4 WHERE id = $1",
        )
        .bind(id)
        .bind(decision.as_str())
        .bind(c.user_id)
        .bind(&reason)
        .execute(&mut *tx)
        .await?;
        if decision == AppealDecision::Overturned {
            sqlx::query(
                "UPDATE member_restrictions SET state = 'overturned', lifted_by = $2, lifted_at = now(), lift_reason = $3 \
                 WHERE id = $1",
            )
            .bind(id)
            .bind(c.user_id)
            .bind(&reason)
            .execute(&mut *tx)
            .await?;
            if r.kind == "case_freeze" {
                sqlx::query(
                    "UPDATE moderation_subtree_nodes SET status = 'cleared', decided_by = $2, decided_at = now(), \
                     note = 'appeal overturned' WHERE account_id = $1 AND case_id = $3 AND status IN ('auto_frozen', 'frozen')",
                )
                .bind(r.account_id)
                .bind(c.user_id)
                .bind(r.case_id)
                .execute(&mut *tx)
                .await?;
            }
        }
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.restriction.appeal_decide",
            ("account", r.account_id),
            &reason,
            json!({ "restriction_id": id.to_string(), "kind": r.kind, "decision": decision.as_str() }),
        )
        .await?;
        let v = self.restriction_by_id(&mut tx, id, false).await?;
        tx.commit().await?;
        Ok(v)
    }

    // ---- enforcement -------------------------------------------------------------------------------------------

    async fn blocked_in(
        &self,
        conn: &mut PgConnection,
        account: i64,
        kinds: &[&str],
    ) -> ModResult<bool> {
        let kinds: Vec<String> = kinds.iter().map(|k| (*k).to_owned()).collect();
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM member_restrictions WHERE account_id = $1 AND state = 'active' \
             AND ends_at > now() AND kind = ANY($2))",
        )
        .bind(account)
        .bind(kinds)
        .fetch_one(&mut *conn)
        .await?)
    }

    /// Whether a restriction in force stops `account` from `what` right now. A frozen or timed-out account can
    /// still log in and appeal.
    pub async fn is_blocked(&self, account: i64, what: Blocked) -> ModResult<bool> {
        let mut conn = self.pool.acquire().await?;
        self.blocked_in(&mut conn, account, what.kinds()).await
    }

    /// Whether `account` is sterilized right now (an active, unexpired `sterilization` restriction): inviting, and
    /// with it vouching (MOD-VCH-001), is taken from it.
    pub async fn is_sterilized(&self, account: i64) -> ModResult<bool> {
        is_sterilized(&self.pool, account).await
    }

    /// Add voice time to an account's activity (D-392 reads it). Called when a voice session ends.
    pub async fn record_voice_seconds(&self, account: i64, seconds: i64) -> ModResult<()> {
        if seconds <= 0 {
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO account_activity (account_id, voice_seconds) VALUES ($1, $2) \
             ON CONFLICT (account_id) DO UPDATE SET voice_seconds = account_activity.voice_seconds + EXCLUDED.voice_seconds, \
             updated_at = now()",
        )
        .bind(account)
        .bind(seconds)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The length of an automated timeout after the accountability multiplier (never shorter than `base`).
    pub async fn automated_timeout_minutes(&self, account: i64, base_minutes: i64) -> i64 {
        let m = self.strike_weight_multiplier(account).await;
        sr_admin::scale_up(base_minutes, m).min(gr::MAX_TIMEOUT_MINUTES)
    }

    /// An account's total strike weight over the warnings and timeouts still on record.
    pub async fn strike_total(&self, account: i64) -> ModResult<f32> {
        let t: Option<f64> = sqlx::query_scalar(
            "SELECT sum(weight)::double precision FROM member_restrictions WHERE account_id = $1 \
             AND kind IN ('warn', 'timeout') AND state IN ('active', 'expired') AND ends_at > now() - interval '180 days'",
        )
        .bind(account)
        .fetch_one(&self.pool)
        .await?;
        Ok(t.unwrap_or(0.0) as f32)
    }
}

/// Whether `account` carries an active, unexpired sterilization (wave 44-A state). Read by the vouch system's
/// `SterilizationCheck` (wave 44-B): a sterilized member cannot vouch.
pub async fn is_sterilized(pool: &sqlx::PgPool, account: i64) -> ModResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM member_restrictions WHERE account_id = $1 AND kind = 'sterilization'          AND state = 'active' AND ends_at > now())",
    )
    .bind(account)
    .fetch_one(pool)
    .await?)
}

/// Revoke the account's open (unused) invite codes on this connection; the number revoked.
pub(crate) async fn revoke_open_codes(conn: &mut PgConnection, account: i64) -> ModResult<i64> {
    let n = sqlx::query(
        "UPDATE platform_invites SET state = 'revoked', revoked_at = now(), updated_at = now() \
         WHERE issuer_account_id = $1 AND state = 'open' AND NOT emergency",
    )
    .bind(account)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    Ok(n as i64)
}

/// The member's words for a restriction.
fn my_view(v: &RestrictionView) -> MyRestriction {
    let (effect, reason) = match v.kind.as_str() {
        "warn" => ("A warning on your record.", v.reason.clone()),
        "timeout" => (
            "You cannot post or join voice until this ends.",
            v.reason.clone(),
        ),
        "invite_freeze" => (
            "You cannot create invites until this ends.",
            v.reason.clone(),
        ),
        "case_freeze" => (
            "Your account is frozen while it is reviewed: you cannot post, invite or join voice. You can still sign in and appeal.",
            "Your account is under review.".to_owned(),
        ),
        _ => (
            "You cannot create invites until this ends.",
            "Your account is under review.".to_owned(),
        ),
    };
    MyRestriction {
        id: v.id,
        kind: v.kind.clone(),
        effect: effect.to_owned(),
        reason,
        ends_at_ms: v.ends_at_ms,
        state: v.state.clone(),
        can_appeal: v.state == "active" && v.appeal_state.is_none(),
        appeal_state: v.appeal_state.clone(),
    }
}
