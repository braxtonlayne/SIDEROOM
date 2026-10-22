//! Staff dashboards (spec section 9, step 4: MOD-DASH-002, -005 to -010).
//!
//! Everything here reads aggregates. Each query is bounded: a time window (at most [`MAX_WINDOW_DAYS`] days,
//! on an indexed timestamp), a small partial set (open cases, active bans, active holds) or a capped count
//! ([`COUNT_CAP`]). No result names a reporter, a reported person or a staff member: the inbox shows how many
//! reports a case has, never who filed them, and the health figures are totals with no per-staff split.
//!
//! The dashboard is one document whose sections follow the caller's platform bits, so the page shows a tab
//! per permission: `MANAGE_CASES` (inbox, health, retention, age), `PLATFORM_BAN` (bans), `MANAGE_TAKEDOWNS`
//! (register, data requests).

use serde::{Deserialize, Serialize};
use sqlx::Row;
use sr_admin::model::{Caller, id};
use sr_core::PlatformPermissions as P;
use utoipa::ToSchema;

use crate::Transition;
use crate::error::{ModError, ModResult, invalid};
use crate::model::{CaseState, Who};
use crate::service::{Moderation, require_staff};

/// The longest window a dashboard figure may cover.
pub const MAX_WINDOW_DAYS: i32 = 90;
/// Counts that scan a table stop here and say so, so one slow table cannot make the page slow.
pub const COUNT_CAP: i64 = 10_000;
/// Most cases one bulk dismissal takes.
pub const MAX_BULK: usize = 100;

/// A label and a count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DashCount {
    pub key: String,
    pub count: i64,
}

/// Reports filed on one day (UTC).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DashDay {
    /// `YYYY-MM-DD`.
    pub day: String,
    pub reports: i64,
}

/// Reports waiting for someone to pick them up (MOD-DASH-002).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashInbox {
    /// Cases that are still `open` with nobody assigned, and have a report that nobody has looked at.
    pub groups: i64,
    /// Reports in those cases.
    pub reports: i64,
    /// Age of the oldest waiting report, in seconds.
    pub oldest_seconds: Option<i64>,
}

/// Health of the process (MOD-DASH-008). Totals only; there is no per-staff figure.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashHealth {
    pub window_days: i32,
    /// One entry per day of the window, oldest first, zero days included.
    pub reports_per_day: Vec<DashDay>,
    pub reports: i64,
    pub cases_opened: i64,
    /// Median seconds from the opening of a case to the first action on it; `None` when no case in the window
    /// has had one.
    pub median_first_action_seconds: Option<i64>,
    pub cases_with_first_action: i64,
    /// Cases still waiting for a first action, whatever their age.
    pub cases_waiting: i64,
    /// Cases closed in the window.
    pub closed: i64,
    /// Of those, how many were reopened (or continued in a new case) afterwards.
    pub reopened: i64,
    /// `reopened / closed` as a fraction between 0 and 1; `None` when nothing was closed.
    pub reopen_rate: Option<f64>,
    /// Appeals decided in the window by outcome (`upheld`, `overturned`).
    pub appeal_outcomes: Vec<DashCount>,
    /// Appeals waiting for a decision, whatever their age.
    pub appeals_open: i64,
}

/// One kind of row a purge job skipped because of a hold (MOD-DASH-009).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashRetentionRow {
    /// `messages_held`, `messages_held_attachment`, `attachments_removed_held`, `attachments_expiry_held`,
    /// `submissions_held`.
    pub key: String,
    pub count: i64,
    /// The count stopped at the cap: there are at least this many.
    pub capped: bool,
}

/// What the purge jobs are leaving behind (MOD-DASH-009).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashRetention {
    pub rows: Vec<DashRetentionRow>,
    /// Holds in force.
    pub holds_active: i64,
    /// Holds that nobody has looked at for longer than `moderation.hold_review_days`.
    pub holds_review_due: i64,
}

/// Age and child-safety metrics (MOD-DASH-010, spec 7).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashAge {
    pub window_days: i32,
    pub restricted_channels: i64,
    pub restricted_series: i64,
    pub restricted_titles: i64,
    /// Resets of the age confirmation in the window. Counts only, never whose.
    pub age_gate_resets: i64,
    /// Open cases in the `child_safety` category.
    pub child_safety_open_cases: i64,
    /// Child-safety reports filed in the window.
    pub child_safety_reports: i64,
    /// Platform bans in force with the reason `child_safety`.
    pub child_safety_active_bans: i64,
}

/// Platform bans in force (MOD-DASH-007).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashBans {
    pub active: i64,
    /// By `reason_category`; `none` for bans made without one.
    pub by_reason: Vec<DashCount>,
    /// By severity (`serious`, `standard`).
    pub by_severity: Vec<DashCount>,
    /// By age: `under_7d`, `7_to_30d`, `30_to_90d`, `over_90d`.
    pub by_age: Vec<DashCount>,
    /// Bans lifted in the window.
    pub lifted_in_window: i64,
}

/// The takedown register at a glance (MOD-DASH-005).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashTakedowns {
    pub window_days: i32,
    /// Notices not decided yet (`received`, `triage`).
    pub undecided: i64,
    /// Of those, past the triage target (one business day).
    pub triage_overdue: i64,
    /// Notices in `counter_noticed`.
    pub counter_noticed: i64,
    /// Of those, restorable now (the wait is over) and no court filing noted.
    pub restore_due: i64,
    /// Notices whose content was removed in the window.
    pub removed_in_window: i64,
    /// Notices received in the window.
    pub received_in_window: i64,
}

/// The data-request queue at a glance (MOD-DASH-006).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashDataRequests {
    pub window_days: i32,
    pub open_exports: i64,
    pub open_erasures: i64,
    /// Open requests past their deadline (the extended one when there is one).
    pub overdue: i64,
    /// Open requests due within seven days.
    pub due_soon: i64,
    pub completed_in_window: i64,
}

/// The dashboard. A section is absent when the caller lacks the bit that reads it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashboardView {
    pub generated_at_ms: i64,
    pub window_days: i32,
    pub inbox: Option<DashInbox>,
    pub health: Option<DashHealth>,
    pub retention: Option<DashRetention>,
    pub age: Option<DashAge>,
    pub bans: Option<DashBans>,
    pub takedowns: Option<DashTakedowns>,
    pub data_requests: Option<DashDataRequests>,
}

/// One case in the reports inbox: a target and the reports that arrived for it (MOD-DASH-002).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct InboxGroup {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub case_id: i64,
    pub summary: String,
    pub target_kind: String,
    pub severity: String,
    /// The report categories in the group, so a mixed group shows.
    pub categories: Vec<String>,
    /// Reports in the group that nobody has looked at; more than one means duplicates.
    pub reports: i64,
    pub first_report_at_ms: i64,
    pub last_report_at_ms: i64,
}

/// A page of the inbox, oldest first.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct InboxPage {
    pub items: Vec<InboxGroup>,
    pub next_cursor: Option<String>,
}

/// A case the bulk dismissal left alone, and why.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct InboxSkipped {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub case_id: i64,
    pub reason: String,
}

/// Result of a bulk dismissal.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct InboxDismissOutcome {
    #[serde(with = "id_list")]
    #[schema(value_type = Vec<String>)]
    pub dismissed: Vec<i64>,
    pub skipped: Vec<InboxSkipped>,
}

mod id_list {
    use serde::Serializer;

    pub fn serialize<S: Serializer>(v: &[i64], s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(v.iter().map(i64::to_string))
    }
}

fn window(days: Option<i32>) -> i32 {
    days.unwrap_or(30).clamp(1, MAX_WINDOW_DAYS)
}

/// A count that stops at [`COUNT_CAP`]: `(count, capped)`.
async fn capped(pool: &sqlx::PgPool, inner: &str) -> ModResult<(i64, bool)> {
    let sql = format!(
        "SELECT count(*) FROM (SELECT 1 FROM {inner} LIMIT {}) x",
        COUNT_CAP + 1
    );
    let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .fetch_one(pool)
        .await?;
    Ok((n.min(COUNT_CAP), n > COUNT_CAP))
}

impl Moderation {
    /// The dashboard for the caller's bits (spec 9). At least one of `MANAGE_CASES`, `PLATFORM_BAN` and
    /// `MANAGE_TAKEDOWNS` is needed.
    pub async fn dashboard(&self, c: &Caller, days: Option<i32>) -> ModResult<DashboardView> {
        let days = window(days);
        let cases = c.permissions.has(P::MANAGE_CASES);
        let bans = c.permissions.has(P::PLATFORM_BAN);
        let takedowns = c.permissions.has(P::MANAGE_TAKEDOWNS);
        if !(cases || bans || takedowns) {
            return Err(ModError::MissingPermission("MANAGE_CASES"));
        }
        let generated_at_ms: i64 =
            sqlx::query_scalar("SELECT (extract(epoch FROM now()) * 1000)::bigint")
                .fetch_one(&self.pool)
                .await?;
        let mut v = DashboardView {
            generated_at_ms,
            window_days: days,
            inbox: None,
            health: None,
            retention: None,
            age: None,
            bans: None,
            takedowns: None,
            data_requests: None,
        };
        if cases {
            v.inbox = Some(self.dash_inbox().await?);
            v.health = Some(self.dash_health(days).await?);
            v.retention = Some(self.dash_retention().await?);
            v.age = Some(self.dash_age(days).await?);
        }
        if bans {
            v.bans = Some(self.dash_bans(days).await?);
        }
        if takedowns {
            v.takedowns = Some(self.dash_takedowns(days).await?);
            v.data_requests = Some(self.dash_data_requests(days).await?);
        }
        Ok(v)
    }

    /// The part of the inbox every figure and list shares: an open case that nobody has picked up, routed to
    /// platform staff or reviewers (a server's own cases stay with its moderators, D-209). This is what
    /// "unattached" means in spec 9.2: report `received`, case `open`, no assignee (D-339).
    const INBOX_CASES: &'static str = "c.state = 'open' AND c.assignee_id IS NULL \
        AND c.routing IN ('platform', 'reviewers')";

    async fn dash_inbox(&self) -> ModResult<DashInbox> {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT count(DISTINCT c.id) AS groups, count(r.id) AS reports, \
             (extract(epoch FROM now() - min(r.created_at)))::bigint AS oldest \
             FROM moderation_cases c JOIN moderation_reports r ON r.case_id = c.id AND r.state = 'received' \
             WHERE {}",
            Self::INBOX_CASES
        )))
        .fetch_one(&self.pool)
        .await?;
        Ok(DashInbox {
            groups: row.try_get("groups")?,
            reports: row.try_get("reports")?,
            oldest_seconds: row.try_get("oldest")?,
        })
    }

    async fn dash_health(&self, days: i32) -> ModResult<DashHealth> {
        // Only the cases platform staff may see count (D-209): a server's own cases are its moderators'.
        let visible = "c.routing IN ('platform', 'reviewers')";
        let day_rows = sqlx::query(
            "WITH s AS (SELECT date_trunc('day', now() AT TIME ZONE 'UTC') - make_interval(days => $1 - 1) AS d0) \
             SELECT to_char(g, 'YYYY-MM-DD') AS day, COALESCE(x.n, 0)::bigint AS n \
             FROM s, generate_series(s.d0, s.d0 + make_interval(days => $1 - 1), interval '1 day') g \
             LEFT JOIN (SELECT date_trunc('day', r.created_at AT TIME ZONE 'UTC') AS d, count(*) AS n \
                        FROM moderation_reports r JOIN moderation_cases c ON c.id = r.case_id \
                        WHERE r.created_at >= (SELECT d0 FROM s) AT TIME ZONE 'UTC' \
                          AND c.routing IN ('platform', 'reviewers') GROUP BY 1) x ON x.d = g \
             ORDER BY g",
        )
        .bind(days)
        .fetch_all(&self.pool)
        .await?;
        let mut reports_per_day = Vec::with_capacity(day_rows.len());
        for r in &day_rows {
            reports_per_day.push(DashDay {
                day: r.try_get("day")?,
                reports: r.try_get("n")?,
            });
        }
        let reports = reports_per_day.iter().map(|d| d.reports).sum();

        let cases = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) AS opened, \
             count(c.first_action_at) AS acted, \
             (percentile_cont(0.5) WITHIN GROUP (ORDER BY extract(epoch FROM c.first_action_at - c.opened_at)))::float8 AS median \
             FROM moderation_cases c WHERE c.opened_at >= now() - make_interval(days => $1) AND {visible}"
        )))
        .bind(days)
        .fetch_one(&self.pool)
        .await?;
        let median: Option<f64> = cases.try_get("median")?;

        let waiting: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM moderation_cases c WHERE c.state = 'open' AND c.first_action_at IS NULL AND {visible}"
        )))
        .fetch_one(&self.pool)
        .await?;

        // Closed in the window, and of those how many have a later reopening (the events are append-only,
        // so a later `reopened` or `continued_in` event is the record).
        let reopen = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) AS closed, \
             count(*) FILTER (WHERE EXISTS (SELECT 1 FROM moderation_case_events x \
                 WHERE x.case_id = e.case_id AND x.id > e.id AND x.kind IN ('reopened', 'continued_in'))) AS reopened \
             FROM moderation_case_events e JOIN moderation_cases c ON c.id = e.case_id \
             WHERE e.kind = 'state_changed' AND e.detail->>'to' = 'closed' \
               AND e.created_at >= now() - make_interval(days => $1) AND {visible}"
        )))
        .bind(days)
        .fetch_one(&self.pool)
        .await?;
        let closed: i64 = reopen.try_get("closed")?;
        let reopened: i64 = reopen.try_get("reopened")?;

        let outcomes = sqlx::query(
            "SELECT state, count(*) AS n FROM moderation_appeals \
             WHERE decided_at >= now() - make_interval(days => $1) GROUP BY state ORDER BY state",
        )
        .bind(days)
        .fetch_all(&self.pool)
        .await?;
        let mut appeal_outcomes = Vec::new();
        for r in &outcomes {
            appeal_outcomes.push(DashCount {
                key: r.try_get("state")?,
                count: r.try_get("n")?,
            });
        }
        let appeals_open: i64 =
            sqlx::query_scalar("SELECT count(*) FROM moderation_appeals WHERE state = 'open'")
                .fetch_one(&self.pool)
                .await?;

        Ok(DashHealth {
            window_days: days,
            reports_per_day,
            reports,
            cases_opened: cases.try_get("opened")?,
            median_first_action_seconds: median.map(|m| m.round() as i64),
            cases_with_first_action: cases.try_get("acted")?,
            cases_waiting: waiting,
            closed,
            reopened,
            reopen_rate: (closed > 0).then(|| reopened as f64 / closed as f64),
            appeal_outcomes,
            appeals_open,
        })
    }

    async fn dash_retention(&self) -> ModResult<DashRetention> {
        let limits = self.limits().await?;
        let queries: [(&str, &str); 5] = [
            (
                "messages_held",
                "messages WHERE hold_id IS NOT NULL AND purge_after IS NOT NULL AND purge_after <= now()",
            ),
            (
                "messages_held_attachment",
                "attachments a JOIN messages m ON m.id = a.message_id \
                 WHERE a.hold_id IS NOT NULL AND m.hold_id IS NULL \
                   AND m.purge_after IS NOT NULL AND m.purge_after <= now()",
            ),
            (
                "attachments_removed_held",
                "attachments WHERE removed_at IS NOT NULL AND message_id IS NULL AND hold_id IS NOT NULL",
            ),
            (
                "attachments_expiry_held",
                "attachments a JOIN messages m ON m.id = a.message_id \
                 WHERE a.is_large AND a.state = 'ready' AND a.expires_at <= now() \
                   AND (a.hold_id IS NOT NULL OR m.hold_id IS NOT NULL)",
            ),
            (
                "submissions_held",
                "submissions WHERE hold_id IS NOT NULL AND state IN ('draft', 'rejected')",
            ),
        ];
        let mut rows = Vec::with_capacity(queries.len());
        for (key, inner) in queries {
            let (count, is_capped) = capped(&self.pool, inner).await?;
            rows.push(DashRetentionRow {
                key: key.to_owned(),
                count,
                capped: is_capped,
            });
        }
        let holds = sqlx::query(
            "SELECT count(*) AS active, \
             count(*) FILTER (WHERE coalesce(reviewed_at, created_at) < now() - make_interval(days => $1)) AS due \
             FROM moderation_holds WHERE released_at IS NULL",
        )
        .bind(i32::try_from(limits.hold_review_days).unwrap_or(90))
        .fetch_one(&self.pool)
        .await?;
        Ok(DashRetention {
            rows,
            holds_active: holds.try_get("active")?,
            holds_review_due: holds.try_get("due")?,
        })
    }

    async fn dash_age(&self, days: i32) -> ModResult<DashAge> {
        let one = |sql: &'static str| async move {
            sqlx::query_scalar::<_, i64>(sql)
                .fetch_one(&self.pool)
                .await
        };
        let restricted_channels = one("SELECT count(*) FROM channels WHERE nsfw").await?;
        let restricted_series =
            one("SELECT count(*) FROM reading_series WHERE age_restricted").await?;
        let restricted_titles =
            one("SELECT count(*) FROM video_titles WHERE age_restricted").await?;
        let age_gate_resets: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM age_gate_resets WHERE reset_at >= now() - make_interval(days => $1)",
        )
        .bind(days)
        .fetch_one(&self.pool)
        .await?;
        let child_safety_open_cases = one(
            "SELECT count(*) FROM moderation_cases WHERE category = 'child_safety' \
             AND state IN ('open', 'triage', 'investigating')",
        )
        .await?;
        let child_safety_reports: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM moderation_reports WHERE category = 'child_safety' \
             AND created_at >= now() - make_interval(days => $1)",
        )
        .bind(days)
        .fetch_one(&self.pool)
        .await?;
        let child_safety_active_bans = one(
            "SELECT count(*) FROM person_bans WHERE lifted_at IS NULL AND reason_category = 'child_safety'",
        )
        .await?;
        Ok(DashAge {
            window_days: days,
            restricted_channels,
            restricted_series,
            restricted_titles,
            age_gate_resets,
            child_safety_open_cases,
            child_safety_reports,
            child_safety_active_bans,
        })
    }

    async fn dash_bans(&self, days: i32) -> ModResult<DashBans> {
        let rows = sqlx::query(
            "SELECT COALESCE(reason_category::text, 'none') AS reason, category::text AS severity, \
             CASE WHEN created_at > now() - interval '7 days' THEN 'under_7d' \
                  WHEN created_at > now() - interval '30 days' THEN '7_to_30d' \
                  WHEN created_at > now() - interval '90 days' THEN '30_to_90d' \
                  ELSE 'over_90d' END AS age, count(*) AS n \
             FROM person_bans WHERE lifted_at IS NULL GROUP BY 1, 2, 3",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut active = 0;
        let mut by_reason: Vec<DashCount> = Vec::new();
        let mut by_severity: Vec<DashCount> = Vec::new();
        let mut by_age: Vec<DashCount> = ["under_7d", "7_to_30d", "30_to_90d", "over_90d"]
            .iter()
            .map(|k| DashCount {
                key: (*k).to_owned(),
                count: 0,
            })
            .collect();
        fn bump(v: &mut Vec<DashCount>, key: String, n: i64) {
            match v.iter_mut().find(|c| c.key == key) {
                Some(c) => c.count += n,
                None => v.push(DashCount { key, count: n }),
            }
        }
        for r in &rows {
            let n: i64 = r.try_get("n")?;
            active += n;
            bump(&mut by_reason, r.try_get("reason")?, n);
            bump(&mut by_severity, r.try_get("severity")?, n);
            bump(&mut by_age, r.try_get("age")?, n);
        }
        by_reason.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.key.cmp(&b.key)));
        by_severity.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.key.cmp(&b.key)));
        let lifted_in_window: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM person_bans WHERE lifted_at >= now() - make_interval(days => $1)",
        )
        .bind(days)
        .fetch_one(&self.pool)
        .await?;
        Ok(DashBans {
            active,
            by_reason,
            by_severity,
            by_age,
            lifted_in_window,
        })
    }

    async fn dash_takedowns(&self, days: i32) -> ModResult<DashTakedowns> {
        let r = sqlx::query(
            "SELECT \
             count(*) FILTER (WHERE state IN ('received', 'triage')) AS undecided, \
             count(*) FILTER (WHERE state IN ('received', 'triage') AND triage_due_at < now()) AS overdue, \
             count(*) FILTER (WHERE state = 'counter_noticed') AS counter, \
             count(*) FILTER (WHERE state = 'counter_noticed' AND NOT court_filing_noted AND restore_after <= now()) AS restorable \
             FROM takedown_notices WHERE state IN ('received', 'triage', 'counter_noticed')",
        )
        .fetch_one(&self.pool)
        .await?;
        let removed: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM takedown_notices WHERE removed_at >= now() - make_interval(days => $1)",
        )
        .bind(days)
        .fetch_one(&self.pool)
        .await?;
        let received: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM takedown_notices WHERE received_at >= now() - make_interval(days => $1)",
        )
        .bind(days)
        .fetch_one(&self.pool)
        .await?;
        Ok(DashTakedowns {
            window_days: days,
            undecided: r.try_get("undecided")?,
            triage_overdue: r.try_get("overdue")?,
            counter_noticed: r.try_get("counter")?,
            restore_due: r.try_get("restorable")?,
            removed_in_window: removed,
            received_in_window: received,
        })
    }

    async fn dash_data_requests(&self, days: i32) -> ModResult<DashDataRequests> {
        let r = sqlx::query(
            "SELECT \
             count(*) FILTER (WHERE kind = 'export') AS exports, \
             count(*) FILTER (WHERE kind = 'erasure') AS erasures, \
             count(*) FILTER (WHERE COALESCE(extended_until, due_at) < now()) AS overdue, \
             count(*) FILTER (WHERE COALESCE(extended_until, due_at) >= now() \
                                AND COALESCE(extended_until, due_at) < now() + interval '7 days') AS soon \
             FROM data_requests WHERE state IN ('open', 'verified', 'running')",
        )
        .fetch_one(&self.pool)
        .await?;
        let done: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM data_requests WHERE completed_at >= now() - make_interval(days => $1)",
        )
        .bind(days)
        .fetch_one(&self.pool)
        .await?;
        Ok(DashDataRequests {
            window_days: days,
            open_exports: r.try_get("exports")?,
            open_erasures: r.try_get("erasures")?,
            overdue: r.try_get("overdue")?,
            due_soon: r.try_get("soon")?,
            completed_in_window: done,
        })
    }

    // ---- the reports inbox (MOD-DASH-002) ----------------------------------------------------------------

    /// Reports nobody has picked up, grouped by case (a case is one target, so a group of several reports is a
    /// group of duplicates), oldest first. Platform staff only; the reporter is never in the answer.
    pub async fn inbox(
        &self,
        who: &Who<'_>,
        cursor: Option<i64>,
        limit: i64,
    ) -> ModResult<InboxPage> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let limit = limit.clamp(1, 100);
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT c.id, c.summary, c.target_kind, c.severity, \
             array_agg(DISTINCT r.category ORDER BY r.category) AS categories, \
             count(r.id) AS reports, \
             (extract(epoch FROM min(r.created_at)) * 1000)::bigint AS first_ms, \
             (extract(epoch FROM max(r.created_at)) * 1000)::bigint AS last_ms \
             FROM moderation_cases c JOIN moderation_reports r ON r.case_id = c.id AND r.state = 'received' \
             WHERE {} AND ($1::bigint IS NULL OR c.id > $1) \
             GROUP BY c.id ORDER BY c.id LIMIT $2",
            Self::INBOX_CASES
        )))
        .bind(cursor)
        .bind(limit + 1)
        .fetch_all(&self.pool)
        .await?;
        let mut items = Vec::with_capacity(rows.len());
        for r in &rows {
            items.push(InboxGroup {
                case_id: r.try_get("id")?,
                summary: r.try_get("summary")?,
                target_kind: r.try_get("target_kind")?,
                severity: r.try_get("severity")?,
                categories: r.try_get("categories")?,
                reports: r.try_get("reports")?,
                first_report_at_ms: r.try_get("first_ms")?,
                last_report_at_ms: r.try_get("last_ms")?,
            });
        }
        let next_cursor = if items.len() as i64 > limit {
            items.truncate(usize::try_from(limit).unwrap_or(100));
            items.last().map(|g| g.case_id.to_string())
        } else {
            None
        };
        Ok(InboxPage { items, next_cursor })
    }

    /// Dismiss the chosen inbox cases with one reason. Each goes through the normal state change (its own
    /// timeline event, hold release and access-log row); a case that left the inbox meanwhile, or that the
    /// state machine refuses, is reported as skipped and the rest go ahead.
    pub async fn dismiss_inbox(
        &self,
        who: &Who<'_>,
        case_ids: &[i64],
        reason: Option<&str>,
    ) -> ModResult<InboxDismissOutcome> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let reason = crate::rules::reason(reason)?;
        if case_ids.is_empty() {
            return Err(invalid("choose at least one case"));
        }
        if case_ids.len() > MAX_BULK {
            return Err(invalid(format!(
                "dismiss at most {MAX_BULK} cases at a time"
            )));
        }
        let still: Vec<i64> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT c.id FROM moderation_cases c WHERE c.id = ANY($1) AND {}",
            Self::INBOX_CASES
        )))
        .bind(case_ids)
        .fetch_all(&self.pool)
        .await?;
        let mut out = InboxDismissOutcome {
            dismissed: Vec::new(),
            skipped: Vec::new(),
        };
        for &cid in case_ids {
            if out.dismissed.contains(&cid) || out.skipped.iter().any(|s| s.case_id == cid) {
                continue;
            }
            if !still.contains(&cid) {
                out.skipped.push(InboxSkipped {
                    case_id: cid,
                    reason: "The case is no longer waiting in the inbox.".to_owned(),
                });
                continue;
            }
            let t = Transition {
                to: CaseState::Dismissed,
                outcome: Some(reason.clone()),
                reporter_line: None,
            };
            match self.set_state(who, cid, &t).await {
                Ok(_) => out.dismissed.push(cid),
                Err(ModError::Conflict(m)) => out.skipped.push(InboxSkipped {
                    case_id: cid,
                    reason: m,
                }),
                Err(e @ (ModError::Storage(_) | ModError::MissingPermission(_))) => return Err(e),
                Err(_) => out.skipped.push(InboxSkipped {
                    case_id: cid,
                    reason: "The case could not be dismissed.".to_owned(),
                }),
            }
        }
        Ok(out)
    }
}

/// Unit tests of the pure parts (the SQL is tested against Postgres in `tests/dashboard.rs`).
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_is_bounded() {
        assert_eq!(window(None), 30);
        assert_eq!(window(Some(0)), 1);
        assert_eq!(window(Some(-4)), 1);
        assert_eq!(window(Some(10_000)), MAX_WINDOW_DAYS);
        assert_eq!(window(Some(7)), 7);
    }

    #[test]
    fn every_report_category_is_a_ban_reason_filter() {
        // The age metrics and the bans view name `child_safety`; the vocabulary must stay the spec's.
        for c in crate::model::ReportCategory::ALL {
            assert!(
                sr_admin::BAN_REASON_FILTERS.contains(&c.as_str()),
                "{} is missing from the ban reason filters",
                c.as_str()
            );
        }
    }
}
