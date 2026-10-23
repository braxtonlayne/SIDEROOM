//! NSFW and age policy enforcement by staff (spec section 7; MOD-AGE-001, -002 and -003).
//!
//! * [`Moderation::set_age_restricted`] marks or unmarks a channel (`channels.nsfw`), a library series or a cinema
//!   title as age-restricted, from a case or from the item page. The change applies on the next request; the
//!   age gate reads the column. Logged in the platform access log and, when a case is named, on its timeline.
//! * [`Moderation::handle_underage`] is the underage handling of D-294: the age confirmation is reset (the gate
//!   asks again) and locked so the account cannot simply answer "yes" again (reset-and-restrict, under 18); an
//!   account under 13 is also suspended, and the inviter hears only that the invitee did not meet the entry
//!   rules. [`Moderation::lift_age_lock`] undoes the lock when staff find they were wrong.
//!
//! Defaults chosen where the spec leaves a detail open: marking needs `MANAGE_CASES`, or the review bit of the kind
//! (`REVIEW_READING` for a series, `REVIEW_MEDIA` for a title, `DELETE_CONTENT` for both); underage handling needs
//! `MANAGE_CASES`, and the suspension also `MANAGE_ACCOUNTS` (checked by the admin service).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use sr_admin::{AccessKind, Caller};
use sr_core::PlatformPermissions as P;
use utoipa::ToSchema;

use crate::actions::ActionError;
use crate::cases::load;
use crate::error::{ModError, ModResult, conflict, invalid};
use crate::model::{Access, CaseState, Who};
use crate::rules;
use crate::service::{Moderation, log_staff, push_event, require_staff};

/// Tells live clients that a channel changed (its `nsfw` flag), so open apps show the gate without a reload, and
/// writes the change to the server's audit log (`actor` is the staff member).
/// Implemented by the server over the event bus; without one clients see the change on their next fetch.
#[async_trait]
pub trait ChannelSink: Send + Sync {
    async fn channel_changed(&self, guild: i64, channel: i64, actor: i64, nsfw: bool);
}

/// Refreshes the search index entry of a library series or cinema title whose age flag just changed, so a search
/// that hides 18+ items for some viewers sees the new flag at once instead of after the next rebuild at boot.
/// Implemented by the server over the library and cinema services; without one the index catches up at boot.
#[async_trait]
pub trait ItemIndexSink: Send + Sync {
    async fn age_changed(&self, kind: AgeTargetKind, id: i64);
}

/// What can be marked age-restricted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgeTargetKind {
    Channel,
    LibrarySeries,
    CinemaTitle,
}

impl AgeTargetKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Channel => "channel",
            Self::LibrarySeries => "library_series",
            Self::CinemaTitle => "cinema_title",
        }
    }
}

/// How young the account was found to be (D-294).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgeBand {
    /// 13 to 17: the age confirmation is reset and locked.
    #[serde(rename = "under_18")]
    Under18,
    /// Below the minimum registration age: the same, and the account is suspended.
    #[serde(rename = "under_13")]
    Under13,
}

impl AgeBand {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Under18 => "under_18",
            Self::Under13 => "under_13",
        }
    }
}

/// The result of marking or unmarking.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AgeChange {
    pub kind: AgeTargetKind,
    #[serde(with = "sr_admin::model::id")]
    #[schema(value_type = String)]
    pub id: i64,
    /// The channel name or the series or title name.
    pub label: String,
    pub restricted: bool,
    /// `false` when the item already was in that state (nothing was written except the log row).
    pub changed: bool,
}

/// The result of underage handling.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UnderageOutcome {
    #[serde(with = "sr_admin::model::id")]
    #[schema(value_type = String)]
    pub account_id: i64,
    pub band: AgeBand,
    /// The account was suspended (under 13 and still active).
    pub suspended: bool,
    /// The account was told its age confirmation was reset (under 18, when a DM could be sent).
    pub member_notified: bool,
    /// The inviter was told the invitee did not meet the entry rules (under 13).
    pub inviter_notified: bool,
    /// The erasure request opened (or already open) for the account below the minimum age (D-341); nothing runs it.
    #[serde(default, with = "sr_admin::model::opt_id")]
    #[schema(value_type = Option<String>)]
    pub erasure_request_id: Option<i64>,
}

/// The system DM of an under-18 reset (D-341): the reason, what is closed, and where to write.
pub const UNDER18_DM: &str = "Staff have reset your age confirmation because they have reason to believe you are under 18. Content marked 18+ stays closed until staff lift this. If that is wrong, write to the staff at the contact shown on the Help page.";

fn may_mark(c: &Caller, kind: AgeTargetKind) -> bool {
    if c.permissions.has(P::MANAGE_CASES) || c.permissions.has(P::DELETE_CONTENT) {
        return true;
    }
    match kind {
        AgeTargetKind::LibrarySeries => c.permissions.has(P::REVIEW_READING),
        AgeTargetKind::CinemaTitle => c.permissions.has(P::REVIEW_MEDIA),
        AgeTargetKind::Channel => false,
    }
}

impl Moderation {
    /// The sink that keeps the library and cinema search indexes in step with the age flag.
    pub fn with_item_index_sink(mut self, s: std::sync::Arc<dyn ItemIndexSink>) -> Self {
        self.item_index_sink = Some(s);
        self
    }

    /// The channel sink live clients are told through.
    pub fn with_channel_sink(mut self, s: std::sync::Arc<dyn ChannelSink>) -> Self {
        self.channel_sink = Some(s);
        self
    }

    /// Mark (`restricted = true`) or unmark a channel, series or title (MOD-AGE-001). A reason is required. With
    /// `case_id` the change joins the case's timeline and moves a case being worked to `actioned`. A channel must
    /// be a server channel that is not a category or a thread (a thread follows its parent).
    pub async fn set_age_restricted(
        &self,
        who: &Who<'_>,
        kind: AgeTargetKind,
        id: i64,
        restricted: bool,
        reason: Option<&str>,
        case_id: Option<i64>,
    ) -> ModResult<AgeChange> {
        match who {
            Who::Staff(c) if may_mark(c, kind) => {}
            Who::Staff(_) => return Err(ModError::MissingPermission("MANAGE_CASES")),
            Who::GuildMod { .. } => return Err(ModError::Forbidden),
        }
        let reason = rules::reason(reason)?;
        let mut tx = self.pool.begin().await?;
        let case = match case_id {
            Some(c) => Some(load(&mut tx, who, c, Access::Work, true).await?),
            None => None,
        };
        let (label, old, guild): (String, bool, Option<i64>) = match kind {
            AgeTargetKind::Channel => {
                let r = sqlx::query(
                    "SELECT COALESCE(name, '') AS label, nsfw AS flag, guild_id, type FROM channels \
                     WHERE id = $1 FOR UPDATE",
                )
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(ModError::NotFound("channel"))?;
                let guild: Option<i64> = r.try_get("guild_id")?;
                let ty: i16 = r.try_get("type")?;
                if guild.is_none() {
                    return Err(conflict("Only server channels can be age-restricted"));
                }
                if ty == 4 {
                    return Err(conflict(
                        "A category cannot be age-restricted; mark its channels",
                    ));
                }
                if matches!(ty, 10..=12) {
                    return Err(conflict(
                        "A thread follows its parent channel; mark the parent instead",
                    ));
                }
                (r.try_get("label")?, r.try_get("flag")?, guild)
            }
            AgeTargetKind::LibrarySeries => {
                let r = sqlx::query(
                    "SELECT title AS label, age_restricted AS flag FROM reading_series WHERE id = $1 FOR UPDATE",
                )
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(ModError::NotFound("series"))?;
                (r.try_get("label")?, r.try_get("flag")?, None)
            }
            AgeTargetKind::CinemaTitle => {
                let r = sqlx::query(
                    "SELECT title AS label, age_restricted AS flag FROM video_titles WHERE id = $1 FOR UPDATE",
                )
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(ModError::NotFound("title"))?;
                (r.try_get("label")?, r.try_get("flag")?, None)
            }
        };
        let changed = old != restricted;
        if changed {
            let sql = match kind {
                AgeTargetKind::Channel => "UPDATE channels SET nsfw = $2 WHERE id = $1",
                AgeTargetKind::LibrarySeries => {
                    "UPDATE reading_series SET age_restricted = $2, updated_at = now() WHERE id = $1"
                }
                AgeTargetKind::CinemaTitle => {
                    "UPDATE video_titles SET age_restricted = $2, updated_at = now() WHERE id = $1"
                }
            };
            sqlx::query(sql)
                .bind(id)
                .bind(restricted)
                .execute(&mut *tx)
                .await?;
        }
        if let Some(case) = &case {
            push_event(
                &mut tx,
                self.next_id(),
                case.summary.id,
                Some(who.user_id()),
                "age_restriction_set",
                json!({ "kind": kind.as_str(), "target": id.to_string(), "label": label,
                        "restricted": restricted, "changed": changed, "reason": reason }),
            )
            .await?;
            self.move_to_actioned(&mut tx, who, case.summary.id, case.summary.state)
                .await?;
        }
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            if restricted {
                "moderation.age.restrict"
            } else {
                "moderation.age.unrestrict"
            },
            (kind.as_str(), id),
            &reason,
            json!({ "label": label, "changed": changed,
                    "case_id": case_id.map(|c| c.to_string()) }),
        )
        .await?;
        tx.commit().await?;
        if changed && let (Some(g), Some(sink)) = (guild, &self.channel_sink) {
            sink.channel_changed(g, id, who.user_id(), restricted).await;
        }
        if changed
            && matches!(
                kind,
                AgeTargetKind::LibrarySeries | AgeTargetKind::CinemaTitle
            )
            && let Some(sink) = &self.item_index_sink
        {
            sink.age_changed(kind, id).await;
        }
        Ok(AgeChange {
            kind,
            id,
            label,
            restricted,
            changed,
        })
    }

    /// A case being worked moves to `actioned`, with the timeline entry.
    async fn move_to_actioned(
        &self,
        tx: &mut sqlx::PgConnection,
        who: &Who<'_>,
        case_id: i64,
        from: CaseState,
    ) -> ModResult<()> {
        if !rules::can_transition(from, CaseState::Actioned) {
            return Ok(());
        }
        sqlx::query(
            "UPDATE moderation_cases SET state = 'actioned', updated_at = now(), \
             first_action_at = COALESCE(first_action_at, now()) WHERE id = $1",
        )
        .bind(case_id)
        .execute(&mut *tx)
        .await?;
        push_event(
            tx,
            self.next_id(),
            case_id,
            Some(who.user_id()),
            "state_changed",
            json!({ "from": from.as_str(), "to": "actioned" }),
        )
        .await
    }

    /// Underage handling (MOD-AGE-003; D-294, spec 7): reset the account's age confirmation and lock it, and for
    /// `Under13` suspend the account too. A note is required. With `case_id` the case's timeline records it. The
    /// suspension goes through the case-action services (`MANAGE_ACCOUNTS`, its own audit row) before anything is
    /// changed, so a refusal leaves the account untouched. Staff only (`MANAGE_CASES`).
    pub async fn handle_underage(
        &self,
        who: &Who<'_>,
        account_id: i64,
        band: AgeBand,
        note: Option<&str>,
        case_id: Option<i64>,
    ) -> ModResult<UnderageOutcome> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let note = rules::reason(note)?;
        let row = sqlx::query(
            "SELECT a.state::text AS state, a.username::text AS username, pi.issuer_account_id AS inviter \
             FROM accounts a LEFT JOIN platform_invites pi ON pi.id = a.invited_by_invite_id \
             WHERE a.id = $1",
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(ModError::NotFound("account"))?;
        let state: String = row.try_get("state")?;
        let username: String = row.try_get("username")?;
        let inviter: Option<i64> = row.try_get("inviter")?;
        if state == "deleted" {
            return Err(conflict("The account has been deleted"));
        }
        let Who::Staff(by) = who else {
            return Err(ModError::Forbidden);
        };
        // The suspension first: it is the part another service can refuse.
        let mut suspended = false;
        if band == AgeBand::Under13 && state == "active" {
            let Some(actions) = self.case_actions.clone() else {
                return Err(conflict("Suspending from here is not available"));
            };
            let reason = format!("other: below the minimum registration age (D-294). {note}");
            actions
                .suspend_account(by, case_id.unwrap_or(0), account_id, &reason)
                .await
                .map_err(|e| match e {
                    ActionError::Unsupported => conflict("Suspending from here is not available"),
                    ActionError::Refused(_) => ModError::MissingPermission("MANAGE_ACCOUNTS"),
                    ActionError::Conflict(m) => conflict(m),
                    ActionError::Failed(m) => ModError::Storage(m),
                })?;
            suspended = true;
        }
        let deadline_days = i64::from(self.deadline_days().await);
        let mut tx = self.pool.begin().await?;
        let case = match case_id {
            Some(c) => Some(load(&mut tx, who, c, Access::Work, true).await?),
            None => None,
        };
        // The reset is counted for the dashboard in the same statement, without the account (MOD-DASH-010,
        // migration 1140; the self-service reset writes `self`).
        sqlx::query(
            "WITH u AS (UPDATE accounts SET age_confirmed = NULL, age_confirmed_at = NULL, \
             age_locked_at = now(), age_locked_by = $2, updated_at = now() WHERE id = $1 RETURNING 1) \
             INSERT INTO age_gate_resets (source) SELECT 'staff' FROM u",
        )
        .bind(account_id)
        .bind(by.user_id)
        .execute(&mut *tx)
        .await?;
        // Below the minimum age: an erasure request is opened and verified by the acting staff member; nothing runs
        // it (D-341). One active erasure request per account, so an existing one is reported instead.
        let mut erasure_request_id: Option<i64> = None;
        if band == AgeBand::Under13 {
            erasure_request_id = sqlx::query_scalar(
                "SELECT id FROM data_requests WHERE account_id = $1 AND kind = 'erasure' \n                 AND state IN ('open', 'verified', 'running')",
            )
            .bind(account_id)
            .fetch_optional(&mut *tx)
            .await?;
            if erasure_request_id.is_none() {
                let rid = self.next_id();
                sqlx::query(
                    "INSERT INTO data_requests (id, kind, person_id, account_id, state, note, received_at, due_at, \n                     verified_by, verified_at, created_by) \n                     SELECT $1, 'erasure', person_id, id, 'verified', $3, now(), now() + make_interval(days => $4), \n                     $2, now(), $2 FROM accounts WHERE id = $5",
                )
                .bind(rid)
                .bind(by.user_id)
                .bind("below the minimum registration age (D-341)")
                .bind(i32::try_from(deadline_days).unwrap_or(30))
                .bind(account_id)
                .execute(&mut *tx)
                .await?;
                erasure_request_id = Some(rid);
            }
        }
        if let Some(case) = &case {
            push_event(
                &mut tx,
                self.next_id(),
                case.summary.id,
                Some(who.user_id()),
                "underage_handled",
                json!({ "account": account_id.to_string(), "band": band.as_str(),
                        "suspended": suspended, "note": note,
                        "erasure_request_id": erasure_request_id.map(|r| r.to_string()) }),
            )
            .await?;
            self.move_to_actioned(&mut tx, who, case.summary.id, case.summary.state)
                .await?;
        }
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.age.underage",
            ("account", account_id),
            &note,
            json!({ "band": band.as_str(), "suspended": suspended,
                    "erasure_request_id": erasure_request_id.map(|r| r.to_string()),
                    "case_id": case_id.map(|c| c.to_string()) }),
        )
        .await?;
        tx.commit().await?;
        let mut member_notified = false;
        let mut inviter_notified = false;
        if band == AgeBand::Under18 {
            member_notified = self.dm_subject(account_id, UNDER18_DM).await;
        } else if let Some(inviter) = inviter {
            // Only that the invitee did not meet the entry rules (spec 7); never the age or the reason.
            inviter_notified = self
                .dm_subject(
                    inviter,
                    &format!(
                        "An account you invited, @{username}, did not meet the entry rules and has been suspended."
                    ),
                )
                .await;
        }
        Ok(UnderageOutcome {
            account_id,
            band,
            suspended,
            member_notified,
            inviter_notified,
            erasure_request_id,
        })
    }

    /// Lift the lock of [`Self::handle_underage`] (the account may answer the age gate again). A reason is
    /// required. `Invalid` when the account is not locked.
    pub async fn lift_age_lock(
        &self,
        who: &Who<'_>,
        account_id: i64,
        reason: Option<&str>,
    ) -> ModResult<()> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let reason = rules::reason(reason)?;
        let mut tx = self.pool.begin().await?;
        let locked: Option<bool> = sqlx::query_scalar(
            "SELECT age_locked_at IS NOT NULL FROM accounts WHERE id = $1 FOR UPDATE",
        )
        .bind(account_id)
        .fetch_optional(&mut *tx)
        .await?;
        match locked {
            None => return Err(ModError::NotFound("account")),
            Some(false) => return Err(invalid("The age confirmation is not locked")),
            Some(true) => {}
        }
        sqlx::query(
            "UPDATE accounts SET age_locked_at = NULL, age_locked_by = NULL, updated_at = now() WHERE id = $1",
        )
        .bind(account_id)
        .execute(&mut *tx)
        .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.age.unlock",
            ("account", account_id),
            &reason,
            json!({}),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
