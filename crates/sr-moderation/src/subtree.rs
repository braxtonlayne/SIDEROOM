//! MOD-GRD-001: the subtree case a serious ban opens (D-395), the inviter sterilization, the Operator's branch
//! action, the anonymised notice and the monthly transparency counts.
//!
//! * A serious ban opens a case on the banned account's `invited_by` subtree ([`Moderation::open_subtree_case`],
//!   subscribed to the admin service's serious-ban event). Only descendants that are **unactivated under D-392**
//!   are frozen automatically (they cannot post, invite or join voice; they can log in and appeal) and are
//!   removed after 30 days unless staff clear them. Activated descendants, accounts younger than the D-392 window,
//!   the direct inviter and the inviter's inviter are only listed; staff clear, freeze or ban each one on its own
//!   evidence.
//! * Inviter sterilization and the branch action go through [`moderation_requests`]: one staff member proposes,
//!   a second approves, and both calls are made behind a 2FA step-up by the route. The branch action is the
//!   Operator's alone.
//! * The notice and the transparency page read `moderation_notices`, which holds a category and counts and
//!   nothing else.

use async_trait::async_trait;
use serde_json::json;
use sqlx::{PgConnection, Row};
use sr_admin::{AccessKind, Caller, SeriousBan, SeriousBanObserver};
use sr_core::PlatformPermissions as P;

use crate::error::{ModError, ModResult, conflict, invalid};
use crate::graduated::{
    SYSTEM_ACTOR, Span, caller_of, guard_target, insert_restriction, revoke_open_codes,
};
use crate::graduated_model::{
    BranchOutcome, CategoryCount, ModRequest, NoticeView, SubtreeCaseDetail, SubtreeCaseSummary,
    SubtreeNode, Transparency, TransparencyMonth,
};
use crate::graduated_rules as gr;
use crate::model::Who;
use crate::rules;
use crate::service::{Moderation, audit_entry, log_staff, require_staff};

/// What opening a case needs.
pub(crate) struct OpenInput {
    pub ban_id: Option<i64>,
    pub root_account: i64,
    /// The ban reason category, or `serious` when the ban named none.
    pub category: String,
    /// The staff member whose ban opened it.
    pub actor: i64,
}

/// What opening a case did.
#[derive(Debug, Clone)]
pub struct OpenedCase {
    pub case_id: i64,
    pub nodes: i64,
    pub auto_frozen: i64,
    pub waiting_review: i64,
    pub notice: NoticeView,
}

/// Bans one account on behalf of a branch action (implemented over the admin service; see below).
#[async_trait]
pub trait NodeBanner: Send + Sync {
    /// Ban the Person behind `account` (a standard-severity ban: it never starts another case). `Err` carries a
    /// word for the log; an account that is already banned is not an error.
    async fn ban_account(
        &self,
        by: &Caller,
        account: i64,
        reason_category: &str,
        note: &str,
    ) -> Result<(), String>;
}

#[async_trait]
impl NodeBanner for sr_admin::AdminService {
    async fn ban_account(
        &self,
        by: &Caller,
        account: i64,
        reason_category: &str,
        note: &str,
    ) -> Result<(), String> {
        let detail = self
            .repo()
            .account_detail(account)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "unknown account".to_owned())?;
        match self
            .ban_person_with_reason(
                by,
                detail.account.person_id,
                Some(account),
                sr_admin::BanCategory::Standard,
                Some(reason_category),
                Some(note),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(sr_admin::AdminError::Conflict(_)) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

fn caller_for(actor: i64) -> Caller {
    Caller {
        user_id: actor,
        permissions: P::empty(),
        is_owner: false,
        ip: None,
    }
}

const NODE_COLS: &str = "n.account_id, a.username::text AS username, n.role, n.depth, n.parent_account_id, \
    (extract(epoch FROM n.registered_at) * 1000)::bigint AS reg_ms, n.messages, n.voice_seconds, n.unactivated, \
    n.status, n.note, \
    EXISTS (SELECT 1 FROM member_restrictions r WHERE r.account_id = n.account_id AND r.state = 'active' \
            AND r.ends_at > now() AND r.kind IN ('case_freeze', 'sterilization')) AS restricted";

const REQ_COLS: &str = "id, kind, case_id, target_account_id, action, reason, proposed_by, approved_by, state, \
    (extract(epoch FROM created_at) * 1000)::bigint AS created_ms, nodes_actioned";

fn request_from(r: &sqlx::postgres::PgRow) -> ModResult<ModRequest> {
    Ok(ModRequest {
        id: r.try_get("id")?,
        kind: r.try_get("kind")?,
        case_id: r.try_get("case_id")?,
        target_account_id: r.try_get("target_account_id")?,
        action: r.try_get("action")?,
        reason: r.try_get("reason")?,
        proposed_by: r.try_get("proposed_by")?,
        approved_by: r.try_get("approved_by")?,
        state: r.try_get("state")?,
        created_at_ms: r.try_get("created_ms")?,
        nodes_actioned: r.try_get("nodes_actioned")?,
    })
}

impl Moderation {
    // ---- opening ---------------------------------------------------------------------------------------------

    /// Open the subtree case of a serious ban. `Ok(None)` when the ban already has one or there is no account to
    /// root it on.
    pub async fn open_subtree_case(&self, ban: &SeriousBan) -> ModResult<Option<OpenedCase>> {
        let root = match ban.source_account_id {
            Some(a) => a,
            None => {
                let first: Option<i64> =
                    sqlx::query_scalar("SELECT min(id) FROM accounts WHERE person_id = $1")
                        .bind(ban.person_id)
                        .fetch_one(&self.pool)
                        .await?;
                match first {
                    Some(a) => a,
                    None => return Ok(None),
                }
            }
        };
        let category = ban
            .reason_category
            .as_deref()
            .filter(|c| gr::REASON_CATEGORIES.contains(c))
            .unwrap_or("serious")
            .to_owned();
        let mut tx = self.pool.begin().await?;
        let opened = self
            .open_case_in(
                &mut tx,
                &OpenInput {
                    ban_id: Some(ban.ban_id),
                    root_account: root,
                    category,
                    actor: ban.actor_id,
                },
            )
            .await?;
        tx.commit().await?;
        if opened.is_some() {
            self.notice_published().await;
        }
        Ok(opened)
    }

    /// Open a case on this connection: nodes, the automatic freezes, the notice and the audit entry. `None` when
    /// the ban already has a case.
    pub(crate) async fn open_case_in(
        &self,
        conn: &mut PgConnection,
        input: &OpenInput,
    ) -> ModResult<Option<OpenedCase>> {
        let case_id = self.next_id();
        let inserted = sqlx::query(
            "INSERT INTO moderation_subtree_cases (id, ban_id, root_account_id, category, opened_by) \
             VALUES ($1, $2, $3, $4, $5) ON CONFLICT (ban_id) DO NOTHING",
        )
        .bind(case_id)
        .bind(input.ban_id)
        .bind(input.root_account)
        .bind(&input.category)
        .bind(input.actor)
        .execute(&mut *conn)
        .await?
        .rows_affected();
        if inserted == 0 {
            return Ok(None);
        }

        // The root, then everything below it (nearest first, capped), then the two inviters above it.
        let mut rows = sqlx::query(
            "WITH RECURSIVE d AS ( \
                SELECT pi.redeemed_by_account_id AS id, pi.issuer_account_id AS parent_id, 1 AS depth \
                FROM platform_invites pi WHERE pi.issuer_account_id = $1 AND pi.state = 'redeemed' \
                UNION ALL \
                SELECT pi.redeemed_by_account_id, pi.issuer_account_id, d.depth + 1 FROM d \
                JOIN platform_invites pi ON pi.issuer_account_id = d.id AND pi.state = 'redeemed' \
                WHERE d.depth < 64) \
             SELECT a.id, a.state::text AS state, d.parent_id, d.depth, \
                    floor(extract(epoch FROM (now() - a.created_at)) / 86400)::bigint AS age_days, \
\
                    (SELECT count(*) FROM (SELECT 1 FROM messages m WHERE m.author_id = a.id LIMIT 1000) x) AS messages, \
                    COALESCE((SELECT v.voice_seconds FROM account_activity v WHERE v.account_id = a.id), 0) AS voice \
             FROM d JOIN accounts a ON a.id = d.id ORDER BY d.depth, a.id LIMIT $2",
        )
        .bind(input.root_account)
        .bind(gr::MAX_CASE_NODES + 1)
        .fetch_all(&mut *conn)
        .await?;
        let truncated = rows.len() as i64 > gr::MAX_CASE_NODES;
        rows.truncate(gr::MAX_CASE_NODES as usize);

        self.insert_node(
            conn,
            case_id,
            input.root_account,
            "root",
            0,
            "banned",
            false,
        )
        .await?;
        let mut nodes = 1_i64;
        let (mut auto_frozen, mut waiting) = (0_i64, 0_i64);
        let freeze_reason = "Unactivated account in the invite subtree of a serious ban";
        for r in &rows {
            let id: i64 = r.try_get("id")?;
            let state: String = r.try_get("state")?;
            let age: i64 = r.try_get("age_days")?;
            let messages: i64 = r.try_get("messages")?;
            let voice: i64 = r.try_get("voice")?;
            let unactivated = gr::is_unactivated(age, messages, voice);
            let status = if state != "active" {
                "not_active"
            } else if unactivated {
                "auto_frozen"
            } else {
                "review"
            };
            sqlx::query(
                "INSERT INTO moderation_subtree_nodes (case_id, account_id, role, depth, parent_account_id, registered_at, \
                 messages, voice_seconds, unactivated, status)                  SELECT $1, a.id, 'descendant', $3, $4, a.created_at, $5, $6, $7, $8 FROM accounts a WHERE a.id = $2",
            )
            .bind(case_id)
            .bind(id)
            .bind(r.try_get::<i32, _>("depth")?)
            .bind(r.try_get::<Option<i64>, _>("parent_id")?)
            .bind(messages)
            .bind(voice)
            .bind(unactivated)
            .bind(status)
            .execute(&mut *conn)
            .await?;
            nodes += 1;
            match status {
                "auto_frozen" => {
                    insert_restriction(
                        conn,
                        self.next_id(),
                        id,
                        "case_freeze",
                        freeze_reason,
                        Some(case_id),
                        SYSTEM_ACTOR,
                        None,
                        1.0,
                        Span::days(gr::FROZEN_REMOVAL_DAYS),
                        true,
                    )
                    .await?;
                    revoke_open_codes(conn, id).await?;
                    auto_frozen += 1;
                }
                "review" => waiting += 1,
                _ => {}
            }
        }

        // The inviter and the inviter's inviter: listed for review, never touched.
        let mut cur = input.root_account;
        for (role, depth) in [("inviter", -1), ("inviter2", -2)] {
            let up: Option<i64> = sqlx::query_scalar(
                "SELECT pi.issuer_account_id FROM accounts a JOIN platform_invites pi ON pi.id = a.invited_by_invite_id \
                 WHERE a.id = $1",
            )
            .bind(cur)
            .fetch_optional(&mut *conn)
            .await?;
            let Some(up) = up.filter(|u| *u != SYSTEM_ACTOR && *u != input.root_account) else {
                break;
            };
            let state: Option<String> =
                sqlx::query_scalar("SELECT state::text FROM accounts WHERE id = $1")
                    .bind(up)
                    .fetch_optional(&mut *conn)
                    .await?;
            let status = if state.as_deref() == Some("active") {
                waiting += 1;
                "review"
            } else {
                "not_active"
            };
            self.insert_node(conn, case_id, up, role, depth, status, false)
                .await?;
            nodes += 1;
            cur = up;
        }

        let notice = self
            .emit_notice(conn, "serious_ban", &input.category, 1, auto_frozen)
            .await?;
        sqlx::query("UPDATE moderation_subtree_cases SET nodes_truncated = $2, notice_id = $3 WHERE id = $1")
            .bind(case_id)
            .bind(truncated)
            .bind(notice.0)
            .execute(&mut *conn)
            .await?;
        let caller = caller_for(input.actor);
        if let Some(e) = audit_entry(
            &Who::Staff(&caller),
            AccessKind::AdminAction,
            "moderation.subtree_case.open",
            ("case", case_id),
            &format!("serious ban: {}", input.category),
            json!({
                "root_account": input.root_account.to_string(),
                "ban_id": input.ban_id.map(|b| b.to_string()),
                "nodes": nodes, "auto_frozen": auto_frozen, "waiting_review": waiting,
                "truncated": truncated, "notice_id": notice.0.to_string(),
            }),
        ) {
            sr_db::admin_repo::append_in(conn, &e).await?;
        }
        Ok(Some(OpenedCase {
            case_id,
            nodes,
            auto_frozen,
            waiting_review: waiting,
            notice: notice.1,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_node(
        &self,
        conn: &mut PgConnection,
        case_id: i64,
        account: i64,
        role: &str,
        depth: i32,
        status: &str,
        unactivated: bool,
    ) -> ModResult<()> {
        // The root's and an inviter's own stats; `parent_account_id` is who invited them.
        let parent: Option<i64> = sqlx::query_scalar(
            "SELECT pi.issuer_account_id FROM accounts a JOIN platform_invites pi ON pi.id = a.invited_by_invite_id \
             WHERE a.id = $1",
        )
        .bind(account)
        .fetch_optional(&mut *conn)
        .await?;
        sqlx::query(
            "INSERT INTO moderation_subtree_nodes (case_id, account_id, role, depth, parent_account_id, registered_at, \
             messages, voice_seconds, unactivated, status) \
             SELECT $1, a.id, $3, $4, $5, a.created_at, \
                    (SELECT count(*) FROM (SELECT 1 FROM messages m WHERE m.author_id = a.id LIMIT 1000) x), \
                    COALESCE((SELECT v.voice_seconds FROM account_activity v WHERE v.account_id = a.id), 0), $6, $7 \
             FROM accounts a WHERE a.id = $2",
        )
        .bind(case_id)
        .bind(account)
        .bind(role)
        .bind(depth)
        .bind(parent)
        .bind(unactivated)
        .bind(status)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// Store the notice and show it as a platform banner (the MOTD): the category and two counts. Returns the
    /// notice id with its view. The banner is only inserted when the system user exists.
    pub(crate) async fn emit_notice(
        &self,
        conn: &mut PgConnection,
        kind: &str,
        category: &str,
        actioned: i64,
        frozen: i64,
    ) -> ModResult<(i64, NoticeView)> {
        let (title, body) = gr::notice_text(category, actioned, frozen);
        let notice_id = self.next_id();
        let banner_id = self.next_id();
        sqlx::query(
            "INSERT INTO platform_announcements (id, title, body, severity, send_dm, created_by, ends_at) \
             SELECT $1, $2, $3, 'info', FALSE, a.id, now() + interval '7 days' FROM accounts a WHERE a.id = $4",
        )
        .bind(banner_id)
        .bind(&title)
        .bind(&body)
        .bind(SYSTEM_ACTOR)
        .execute(&mut *conn)
        .await?;
        let created: i64 = sqlx::query_scalar(
            "INSERT INTO moderation_notices (id, kind, category, accounts_actioned, accounts_frozen, announcement_id) \
             VALUES ($1, $2, $3, $4, $5, (SELECT id FROM platform_announcements WHERE id = $6)) \
             RETURNING (extract(epoch FROM created_at) * 1000)::bigint",
        )
        .bind(notice_id)
        .bind(kind)
        .bind(category)
        .bind(actioned as i32)
        .bind(frozen as i32)
        .bind(banner_id)
        .fetch_one(&mut *conn)
        .await?;
        Ok((
            notice_id,
            NoticeView {
                kind: kind.to_owned(),
                category: category.to_owned(),
                title,
                body,
                accounts_actioned: actioned,
                accounts_frozen: frozen,
                created_at_ms: created,
            },
        ))
    }

    // ---- reading ---------------------------------------------------------------------------------------------

    /// Cases, open first and newest first (`MANAGE_CASES`).
    pub async fn list_subtree_cases(
        &self,
        who: &Who<'_>,
        open_only: bool,
    ) -> ModResult<Vec<SubtreeCaseSummary>> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{SUMMARY_SELECT} WHERE ($1 = FALSE OR c.state = 'open') ORDER BY (c.state = 'open') DESC, c.id DESC LIMIT 200"
        )))
        .bind(open_only)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(summary_from).collect()
    }

    /// One case with every node and the requests on it (`MANAGE_CASES`; the read is logged).
    pub async fn subtree_case(&self, who: &Who<'_>, id: i64) -> ModResult<SubtreeCaseDetail> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let mut tx = self.pool.begin().await?;
        let head = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{SUMMARY_SELECT} WHERE c.id = $1"
        )))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ModError::NotFound("case"))?;
        let case = summary_from(&head)?;
        let meta = sqlx::query("SELECT opened_by, nodes_truncated FROM moderation_subtree_cases WHERE id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        let node_rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {NODE_COLS} FROM moderation_subtree_nodes n JOIN accounts a ON a.id = n.account_id \
             WHERE n.case_id = $1 ORDER BY n.depth, n.account_id"
        )))
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        let nodes = node_rows
            .iter()
            .map(|r| {
                Ok(SubtreeNode {
                    account_id: r.try_get("account_id")?,
                    username: r.try_get("username")?,
                    role: r.try_get("role")?,
                    depth: r.try_get("depth")?,
                    parent_account_id: r.try_get("parent_account_id")?,
                    registered_at_ms: r.try_get("reg_ms")?,
                    messages: r.try_get("messages")?,
                    voice_seconds: r.try_get("voice_seconds")?,
                    unactivated: r.try_get("unactivated")?,
                    status: r.try_get("status")?,
                    restricted: r.try_get("restricted")?,
                    note: r.try_get("note")?,
                })
            })
            .collect::<ModResult<Vec<_>>>()?;
        let req_rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {REQ_COLS} FROM moderation_requests WHERE case_id = $1 ORDER BY id DESC LIMIT 100"
        )))
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        let requests = req_rows.iter().map(request_from).collect::<ModResult<_>>()?;
        log_staff(
            &mut tx,
            who,
            AccessKind::Read,
            "moderation.subtree_case.view",
            ("case", id),
            "",
            json!({ "nodes": nodes.len() }),
        )
        .await?;
        tx.commit().await?;
        Ok(SubtreeCaseDetail {
            case,
            opened_by: meta.try_get("opened_by")?,
            nodes_truncated: meta.try_get("nodes_truncated")?,
            nodes,
            requests,
        })
    }

    // ---- per-node decisions ----------------------------------------------------------------------------------

    /// The status of a node of an open case, locked; `NotFound` when the account is not in the case.
    async fn node_status(
        conn: &mut PgConnection,
        case_id: i64,
        account: i64,
    ) -> ModResult<(String, String)> {
        let open: Option<String> =
            sqlx::query_scalar("SELECT state FROM moderation_subtree_cases WHERE id = $1 FOR UPDATE")
                .bind(case_id)
                .fetch_optional(&mut *conn)
                .await?;
        match open.as_deref() {
            None => return Err(ModError::NotFound("case")),
            Some("open") => {}
            Some(_) => return Err(conflict("That case is closed")),
        }
        let row = sqlx::query(
            "SELECT status, role FROM moderation_subtree_nodes WHERE case_id = $1 AND account_id = $2 FOR UPDATE",
        )
        .bind(case_id)
        .bind(account)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(ModError::NotFound("node"))?;
        Ok((row.try_get("status")?, row.try_get("role")?))
    }

    /// Clear a node: lift its freeze (if any) and mark it cleared (`MANAGE_CASES`, reason required).
    pub async fn clear_node(
        &self,
        who: &Who<'_>,
        case_id: i64,
        account: i64,
        reason: Option<&str>,
    ) -> ModResult<()> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let c = caller_of(who)?;
        let reason = rules::reason(reason)?;
        let mut tx = self.pool.begin().await?;
        let (status, _) = Self::node_status(&mut tx, case_id, account).await?;
        if !matches!(status.as_str(), "auto_frozen" | "frozen" | "review") {
            return Err(conflict(format!("A node that is {status} cannot be cleared")));
        }
        sqlx::query(
            "UPDATE member_restrictions SET state = 'lifted', lifted_by = $3, lifted_at = now(), lift_reason = $4 \
             WHERE account_id = $1 AND case_id = $2 AND kind = 'case_freeze' AND state = 'active'",
        )
        .bind(account)
        .bind(case_id)
        .bind(c.user_id)
        .bind(&reason)
        .execute(&mut *tx)
        .await?;
        set_node(&mut tx, case_id, account, "cleared", c.user_id, &reason).await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.subtree_case.clear",
            ("account", account),
            &reason,
            json!({ "case_id": case_id.to_string(), "was": status }),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Freeze a node on staff's own evidence (`MANAGE_CASES`): 12 months, no automatic removal, appealable.
    pub async fn freeze_node(
        &self,
        who: &Who<'_>,
        case_id: i64,
        account: i64,
        reason: Option<&str>,
    ) -> ModResult<()> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let c = caller_of(who)?;
        let reason = rules::reason(reason)?;
        let mut tx = self.pool.begin().await?;
        let (status, role) = Self::node_status(&mut tx, case_id, account).await?;
        if role == "root" || !matches!(status.as_str(), "review" | "cleared") {
            return Err(conflict(format!("A node that is {status} cannot be frozen")));
        }
        guard_target(&mut tx, c, account).await?;
        let weight = self.strike_weight_multiplier(account).await;
        insert_restriction(
            &mut tx,
            self.next_id(),
            account,
            "case_freeze",
            &reason,
            Some(case_id),
            c.user_id,
            None,
            weight,
            Span::months(gr::LONG_RESTRICTION_MONTHS),
            false,
        )
        .await?;
        set_node(&mut tx, case_id, account, "frozen", c.user_id, &reason).await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.subtree_case.freeze",
            ("account", account),
            &reason,
            json!({ "case_id": case_id.to_string(), "role": role }),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Check that `account` is a node staff may ban in this case and return its Person (the route bans through the
    /// admin service, then calls [`Self::mark_node_banned`]).
    pub async fn node_person(
        &self,
        who: &Who<'_>,
        case_id: i64,
        account: i64,
    ) -> ModResult<i64> {
        require_staff(who, P::PLATFORM_BAN, "PLATFORM_BAN")?;
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let mut tx = self.pool.begin().await?;
        let (status, role) = Self::node_status(&mut tx, case_id, account).await?;
        if role == "root" || matches!(status.as_str(), "banned" | "removed" | "not_active") {
            return Err(conflict(format!("A node that is {status} cannot be banned")));
        }
        let person: i64 = sqlx::query_scalar("SELECT person_id FROM accounts WHERE id = $1")
            .bind(account)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(ModError::NotFound("account"))?;
        tx.commit().await?;
        Ok(person)
    }

    /// Record that a node was banned from the case page; lifts its case freeze (the ban supersedes it).
    pub async fn mark_node_banned(
        &self,
        who: &Who<'_>,
        case_id: i64,
        account: i64,
        reason: &str,
    ) -> ModResult<()> {
        let c = caller_of(who)?;
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "UPDATE member_restrictions SET state = 'lifted', lifted_by = $3, lifted_at = now(), lift_reason = 'banned' \
             WHERE account_id = $1 AND case_id = $2 AND state = 'active' AND kind = 'case_freeze'",
        )
        .bind(account)
        .bind(case_id)
        .bind(c.user_id)
        .execute(&mut *tx)
        .await?;
        set_node(&mut tx, case_id, account, "banned", c.user_id, reason).await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.subtree_case.node_ban",
            ("account", account),
            reason,
            json!({ "case_id": case_id.to_string() }),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Close a case (`MANAGE_CASES`, reason required). Restrictions already imposed stand until they end, are
    /// lifted or are overturned on appeal.
    pub async fn close_subtree_case(
        &self,
        who: &Who<'_>,
        case_id: i64,
        reason: Option<&str>,
    ) -> ModResult<()> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let c = caller_of(who)?;
        let reason = rules::reason(reason)?;
        let mut tx = self.pool.begin().await?;
        let n = sqlx::query(
            "UPDATE moderation_subtree_cases SET state = 'closed', closed_at = now(), closed_by = $2, close_reason = $3 \
             WHERE id = $1 AND state = 'open'",
        )
        .bind(case_id)
        .bind(c.user_id)
        .bind(&reason)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if n == 0 {
            return Err(ModError::NotFound("case"));
        }
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.subtree_case.close",
            ("case", case_id),
            &reason,
            json!({}),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    // ---- the two-person step ---------------------------------------------------------------------------------

    /// Propose a sterilization (`can_invite = false`, 12 months) of one node of an open case. A second staff
    /// member must approve it ([`Self::approve_request`]); both calls are made behind a 2FA step-up by the route.
    pub async fn request_sterilization(
        &self,
        who: &Who<'_>,
        case_id: i64,
        account: i64,
        reason: &str,
    ) -> ModResult<ModRequest> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let c = caller_of(who)?;
        let reason = gr::case_reason(reason)?;
        let mut tx = self.pool.begin().await?;
        let (status, role) = Self::node_status(&mut tx, case_id, account).await?;
        if role == "root" || matches!(status.as_str(), "banned" | "removed" | "not_active" | "sterilized") {
            return Err(conflict(format!("A node that is {status} cannot be sterilized")));
        }
        guard_target(&mut tx, c, account).await?;
        let dup: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM moderation_requests WHERE kind = 'sterilize' AND target_account_id = $1 \
             AND state IN ('pending', 'approved'))",
        )
        .bind(account)
        .fetch_one(&mut *tx)
        .await?;
        if dup {
            return Err(conflict("A sterilization of that account is already waiting for approval"));
        }
        let id = self.next_id();
        sqlx::query(
            "INSERT INTO moderation_requests (id, kind, case_id, target_account_id, reason, proposed_by) \
             VALUES ($1, 'sterilize', $2, $3, $4, $5)",
        )
        .bind(id)
        .bind(case_id)
        .bind(account)
        .bind(&reason)
        .bind(c.user_id)
        .execute(&mut *tx)
        .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.sterilization.propose",
            ("account", account),
            &reason,
            json!({ "case_id": case_id.to_string(), "request_id": id.to_string() }),
        )
        .await?;
        let v = request_by_id(&mut tx, id).await?;
        tx.commit().await?;
        Ok(v)
    }

    /// Propose the Operator's branch action on a case: freeze or ban every descendant (Owner only).
    pub async fn request_branch_action(
        &self,
        who: &Who<'_>,
        case_id: i64,
        action: &str,
        reason: &str,
    ) -> ModResult<ModRequest> {
        let c = caller_of(who)?;
        if !c.is_owner {
            return Err(ModError::MissingPermission("OWNER"));
        }
        if !matches!(action, "freeze" | "ban") {
            return Err(invalid("action must be freeze or ban"));
        }
        let reason = gr::case_reason(reason)?;
        let mut tx = self.pool.begin().await?;
        let open: Option<String> =
            sqlx::query_scalar("SELECT state FROM moderation_subtree_cases WHERE id = $1 FOR UPDATE")
                .bind(case_id)
                .fetch_optional(&mut *tx)
                .await?;
        match open.as_deref() {
            None => return Err(ModError::NotFound("case")),
            Some("open") => {}
            Some(_) => return Err(conflict("That case is closed")),
        }
        let dup: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM moderation_requests WHERE kind = 'branch_action' AND case_id = $1 \
             AND state IN ('pending', 'approved'))",
        )
        .bind(case_id)
        .fetch_one(&mut *tx)
        .await?;
        if dup {
            return Err(conflict("A branch action on that case is already waiting"));
        }
        let id = self.next_id();
        sqlx::query(
            "INSERT INTO moderation_requests (id, kind, case_id, action, reason, proposed_by) \
             VALUES ($1, 'branch_action', $2, $3, $4, $5)",
        )
        .bind(id)
        .bind(case_id)
        .bind(action)
        .bind(&reason)
        .bind(c.user_id)
        .execute(&mut *tx)
        .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.branch_action.propose",
            ("case", case_id),
            &reason,
            json!({ "action": action, "request_id": id.to_string() }),
        )
        .await?;
        let v = request_by_id(&mut tx, id).await?;
        tx.commit().await?;
        Ok(v)
    }

    /// Requests waiting or recently decided (`MANAGE_CASES`).
    pub async fn list_requests(&self, who: &Who<'_>) -> ModResult<Vec<ModRequest>> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {REQ_COLS} FROM moderation_requests ORDER BY (state IN ('pending', 'approved')) DESC, id DESC LIMIT 100"
        )))
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(request_from).collect()
    }

    /// The second staff member approves (`MANAGE_CASES`; a branch action also needs `PLATFORM_BAN`). Never the
    /// proposer, never the sterilization's target. A sterilization takes effect now (12 months, appealable); a
    /// branch action becomes executable by the Operator.
    pub async fn approve_request(&self, who: &Who<'_>, id: i64) -> ModResult<ModRequest> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let c = caller_of(who)?;
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {REQ_COLS} FROM moderation_requests WHERE id = $1 FOR UPDATE"
        )))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ModError::NotFound("request"))?;
        let req = request_from(&row)?;
        if req.state != "pending" {
            return Err(conflict("That request is no longer waiting"));
        }
        gr::approver_ok(req.proposed_by, c.user_id, req.target_account_id)?;
        if req.kind == "branch_action" {
            require_staff(who, P::PLATFORM_BAN, "PLATFORM_BAN")?;
        }
        sqlx::query(
            "UPDATE moderation_requests SET state = 'approved', approved_by = $2, decided_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(c.user_id)
        .execute(&mut *tx)
        .await?;
        if req.kind == "sterilize" {
            let account = req.target_account_id.ok_or(ModError::NotFound("account"))?;
            let state: Option<String> =
                sqlx::query_scalar("SELECT state::text FROM accounts WHERE id = $1")
                    .bind(account)
                    .fetch_optional(&mut *tx)
                    .await?;
            if state.as_deref() != Some("active") {
                return Err(conflict("That account is no longer active"));
            }
            insert_restriction(
                &mut tx,
                self.next_id(),
                account,
                "sterilization",
                &req.reason,
                Some(req.case_id),
                req.proposed_by,
                Some(c.user_id),
                1.0,
                Span::months(gr::LONG_RESTRICTION_MONTHS),
                false,
            )
            .await?;
            // `can_invite = false`: the codes it still holds stop working too.
            revoke_open_codes(&mut tx, account).await?;
            sqlx::query(
                "UPDATE moderation_subtree_nodes SET status = 'sterilized', decided_by = $3, decided_at = now(), note = $4 \
                 WHERE case_id = $1 AND account_id = $2 AND status <> 'banned'",
            )
            .bind(req.case_id)
            .bind(account)
            .bind(req.proposed_by)
            .bind(&req.reason)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE moderation_requests SET state = 'executed', executed_at = now() WHERE id = $1",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
            log_staff(
                &mut tx,
                who,
                AccessKind::AdminAction,
                "moderation.sterilization.apply",
                ("account", account),
                &req.reason,
                json!({
                    "case_id": req.case_id.to_string(), "request_id": id.to_string(),
                    "proposed_by": req.proposed_by.to_string(), "approved_by": c.user_id.to_string(),
                    "expires_in_months": gr::LONG_RESTRICTION_MONTHS,
                }),
            )
            .await?;
        } else {
            log_staff(
                &mut tx,
                who,
                AccessKind::AdminAction,
                "moderation.branch_action.approve",
                ("case", req.case_id),
                &req.reason,
                json!({ "request_id": id.to_string(), "action": req.action }),
            )
            .await?;
        }
        let v = request_by_id(&mut tx, id).await?;
        tx.commit().await?;
        Ok(v)
    }

    /// Reject a waiting request (`MANAGE_CASES`; the proposer withdrawing their own is `cancelled`).
    pub async fn reject_request(
        &self,
        who: &Who<'_>,
        id: i64,
        reason: Option<&str>,
    ) -> ModResult<ModRequest> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let c = caller_of(who)?;
        let reason = rules::reason(reason)?;
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {REQ_COLS} FROM moderation_requests WHERE id = $1 FOR UPDATE"
        )))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ModError::NotFound("request"))?;
        let req = request_from(&row)?;
        if !matches!(req.state.as_str(), "pending" | "approved") {
            return Err(conflict("That request is no longer waiting"));
        }
        let to = if req.proposed_by == c.user_id {
            "cancelled"
        } else {
            "rejected"
        };
        sqlx::query("UPDATE moderation_requests SET state = $2, decided_at = now() WHERE id = $1")
            .bind(id)
            .bind(to)
            .execute(&mut *tx)
            .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.request.reject",
            ("case", req.case_id),
            &reason,
            json!({ "request_id": id.to_string(), "kind": req.kind, "result": to }),
        )
        .await?;
        let v = request_by_id(&mut tx, id).await?;
        tx.commit().await?;
        Ok(v)
    }

    /// The Operator's branch action (D-395): apply the approved request's action to every descendant of the case.
    /// Owner only; the request must be approved by a second staff member. Every affected account keeps its own
    /// appeal; freezes last 12 months. One hash-chained audit entry names the case, the approver and the node
    /// count, and an anonymised notice (category and counts) is emitted.
    pub async fn execute_branch_action(
        &self,
        who: &Who<'_>,
        request_id: i64,
        banner: &dyn NodeBanner,
    ) -> ModResult<BranchOutcome> {
        let c = caller_of(who)?;
        if !c.is_owner {
            return Err(ModError::MissingPermission("OWNER"));
        }
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {REQ_COLS} FROM moderation_requests WHERE id = $1"
        )))
        .bind(request_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(ModError::NotFound("request"))?;
        let req = request_from(&row)?;
        if req.kind != "branch_action" {
            return Err(ModError::NotFound("request"));
        }
        let (Some(approver), Some(action)) = (req.approved_by, req.action.clone()) else {
            return Err(conflict("A second staff member has not approved that request"));
        };
        if req.state != "approved" {
            return Err(conflict("That request is not waiting for the Operator"));
        }
        if approver == c.user_id {
            return Err(ModError::Forbidden);
        }
        let category: String =
            sqlx::query_scalar("SELECT category FROM moderation_subtree_cases WHERE id = $1 AND state = 'open'")
                .bind(req.case_id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or_else(|| conflict("That case is closed"))?;

        let todo: Vec<i64> = sqlx::query_scalar(
            "SELECT n.account_id FROM moderation_subtree_nodes n WHERE n.case_id = $1 AND n.role = 'descendant' \
             AND n.status IN ('auto_frozen', 'review', 'frozen', 'cleared') ORDER BY n.depth, n.account_id",
        )
        .bind(req.case_id)
        .fetch_all(&self.pool)
        .await?;
        let gone: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM moderation_subtree_nodes WHERE case_id = $1 AND role = 'descendant' \
             AND status IN ('banned', 'removed', 'not_active')",
        )
        .bind(req.case_id)
        .fetch_one(&self.pool)
        .await?;

        let mut actioned = 0_i64;
        if action == "freeze" {
            let mut tx = self.pool.begin().await?;
            for account in &todo {
                // A node already frozen keeps its freeze (an automatic one is replaced by the 12-month one).
                sqlx::query(
                    "UPDATE member_restrictions SET state = 'lifted', lifted_by = $3, lifted_at = now(), \
                     lift_reason = 'replaced by a branch action' \
                     WHERE account_id = $1 AND case_id = $2 AND kind = 'case_freeze' AND state = 'active'",
                )
                .bind(account)
                .bind(req.case_id)
                .bind(c.user_id)
                .execute(&mut *tx)
                .await?;
                insert_restriction(
                    &mut tx,
                    self.next_id(),
                    *account,
                    "case_freeze",
                    &req.reason,
                    Some(req.case_id),
                    c.user_id,
                    Some(approver),
                    1.0,
                    Span::months(gr::LONG_RESTRICTION_MONTHS),
                    false,
                )
                .await?;
                set_node(&mut tx, req.case_id, *account, "frozen", c.user_id, "branch action").await?;
                actioned += 1;
            }
            tx.commit().await?;
        } else {
            for account in &todo {
                match banner
                    .ban_account(c, *account, ban_category_for(&category), &req.reason)
                    .await
                {
                    Ok(()) => {
                        let mut tx = self.pool.begin().await?;
                        sqlx::query(
                            "UPDATE member_restrictions SET state = 'lifted', lifted_by = $3, lifted_at = now(), \
                             lift_reason = 'banned by a branch action' \
                             WHERE account_id = $1 AND case_id = $2 AND kind = 'case_freeze' AND state = 'active'",
                        )
                        .bind(account)
                        .bind(req.case_id)
                        .bind(c.user_id)
                        .execute(&mut *tx)
                        .await?;
                        set_node(&mut tx, req.case_id, *account, "banned", c.user_id, "branch action").await?;
                        tx.commit().await?;
                        actioned += 1;
                    }
                    Err(e) => {
                        tracing::warn!(account, error = %e, "branch action: ban failed, node left for review");
                    }
                }
            }
        }

        let mut tx = self.pool.begin().await?;
        let frozen: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM moderation_subtree_nodes WHERE case_id = $1 AND status IN ('auto_frozen', 'frozen')",
        )
        .bind(req.case_id)
        .fetch_one(&mut *tx)
        .await?;
        let (_, notice) = self
            .emit_notice(&mut tx, "branch_action", &category, actioned, frozen)
            .await?;
        sqlx::query(
            "UPDATE moderation_requests SET state = 'executed', executed_at = now(), nodes_actioned = $2 WHERE id = $1",
        )
        .bind(request_id)
        .bind(actioned as i32)
        .execute(&mut *tx)
        .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.branch_action.execute",
            ("case", req.case_id),
            &req.reason,
            json!({
                "case_id": req.case_id.to_string(), "request_id": request_id.to_string(),
                "approver": approver.to_string(), "action": action,
                "node_count": actioned, "skipped_already_gone": gone,
            }),
        )
        .await?;
        tx.commit().await?;
        self.notice_published().await;
        Ok(BranchOutcome {
            request_id,
            case_id: req.case_id,
            action,
            nodes_actioned: actioned,
            already_banned_or_gone: gone,
            notice,
        })
    }

    // ---- the page everyone may read --------------------------------------------------------------------------

    /// The monthly transparency counts: notices, accounts actioned and frozen, sterilizations and appeals, by
    /// offense category. Counts only; any signed-in member may read it.
    pub async fn transparency(&self) -> ModResult<Transparency> {
        use std::collections::BTreeMap;
        let mut months: BTreeMap<String, TransparencyMonth> = BTreeMap::new();
        let blank = |m: &str| TransparencyMonth {
            month: m.to_owned(),
            serious_bans: 0,
            branch_actions: 0,
            accounts_actioned: 0,
            accounts_frozen: 0,
            sterilizations: 0,
            appeals_filed: 0,
            appeals_overturned: 0,
            by_category: Vec::new(),
        };
        let notices = sqlx::query(
            "SELECT to_char(date_trunc('month', created_at), 'YYYY-MM') AS m, kind, category, count(*) AS n, \
                    COALESCE(sum(accounts_actioned), 0)::bigint AS actioned, COALESCE(sum(accounts_frozen), 0)::bigint AS frozen \
             FROM moderation_notices WHERE created_at > now() - interval '24 months' GROUP BY 1, 2, 3",
        )
        .fetch_all(&self.pool)
        .await?;
        for r in &notices {
            let m: String = r.try_get("m")?;
            let e = months.entry(m.clone()).or_insert_with(|| blank(&m));
            let n: i64 = r.try_get("n")?;
            match r.try_get::<String, _>("kind")?.as_str() {
                "branch_action" => e.branch_actions += n,
                _ => e.serious_bans += n,
            }
            e.accounts_actioned += r.try_get::<i64, _>("actioned")?;
            e.accounts_frozen += r.try_get::<i64, _>("frozen")?;
            let cat = r.try_get::<String, _>("category")?;
            let label = gr::category_label(&cat).to_owned();
            match e.by_category.iter_mut().find(|c| c.category == label) {
                Some(c) => c.notices += n,
                None => e.by_category.push(CategoryCount {
                    category: label,
                    notices: n,
                }),
            }
        }
        let others = sqlx::query(
            "SELECT m, what, count(*) AS n FROM ( \
                SELECT to_char(date_trunc('month', starts_at), 'YYYY-MM') AS m, 'sterilization' AS what \
                  FROM member_restrictions WHERE kind = 'sterilization' AND starts_at > now() - interval '24 months' \
                UNION ALL \
                SELECT to_char(date_trunc('month', appealed_at), 'YYYY-MM'), 'appeal' \
                  FROM member_restrictions WHERE appealed_at > now() - interval '24 months' \
                UNION ALL \
                SELECT to_char(date_trunc('month', appeal_decided_at), 'YYYY-MM'), 'overturned' \
                  FROM member_restrictions WHERE appeal_state = 'overturned' AND appeal_decided_at > now() - interval '24 months') x \
             GROUP BY 1, 2",
        )
        .fetch_all(&self.pool)
        .await?;
        for r in &others {
            let m: String = r.try_get("m")?;
            let e = months.entry(m.clone()).or_insert_with(|| blank(&m));
            let n: i64 = r.try_get("n")?;
            match r.try_get::<String, _>("what")?.as_str() {
                "sterilization" => e.sterilizations += n,
                "appeal" => e.appeals_filed += n,
                _ => e.appeals_overturned += n,
            }
        }
        let mut list: Vec<TransparencyMonth> = months.into_values().collect();
        list.reverse();
        for m in &mut list {
            m.by_category.sort_by(|a, b| a.category.cmp(&b.category));
        }
        list.truncate(24);
        Ok(Transparency { months: list })
    }

    // ---- the housekeeping pass -------------------------------------------------------------------------------

    /// One pass of `moderation.restrictions`: restrictions that ran out become `expired`; a due freeze of an
    /// unactivated descendant that nobody cleared removes the account (deactivated, sessions revoked, an erasure
    /// request queued under the system user), each with an audit entry. Returns `(expired, removed)`.
    pub async fn sweep_restrictions(&self) -> ModResult<(i64, i64)> {
        let mut tx = self.pool.begin().await?;
        let due = sqlx::query(
            "SELECT id, account_id, case_id FROM member_restrictions \
             WHERE state = 'active' AND removes_account AND ends_at <= now() FOR UPDATE SKIP LOCKED LIMIT 500",
        )
        .fetch_all(&mut *tx)
        .await?;
        let mut removed = 0_i64;
        let system = caller_for(SYSTEM_ACTOR);
        for r in &due {
            let (rid, account): (i64, i64) = (r.try_get("id")?, r.try_get("account_id")?);
            let case: Option<i64> = r.try_get("case_id")?;
            sqlx::query("UPDATE member_restrictions SET state = 'removed', lifted_at = now() WHERE id = $1")
                .bind(rid)
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "UPDATE moderation_subtree_nodes SET status = 'removed', decided_at = now(), note = '30 days without a staff clearance' \
                 WHERE account_id = $1 AND case_id = $2 AND status = 'auto_frozen'",
            )
            .bind(account)
            .bind(case)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE accounts SET state = 'deactivated', state_changed_at = now(), updated_at = now() \
                 WHERE id = $1 AND state = 'active'",
            )
            .bind(account)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE sessions SET revoked_at = now(), updated_at = now() WHERE account_id = $1 AND revoked_at IS NULL",
            )
            .bind(account)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO account_delete_requests (id, account_id, requested_by, reason) \
                 SELECT $1, a.id, s.id, $3 FROM accounts a, accounts s WHERE a.id = $2 AND s.id = $4 \
                 ON CONFLICT DO NOTHING",
            )
            .bind(self.next_id())
            .bind(account)
            .bind("Unactivated account frozen by a safety case; 30 days passed without a staff clearance")
            .bind(SYSTEM_ACTOR)
            .execute(&mut *tx)
            .await?;
            if let Some(e) = audit_entry(
                &Who::Staff(&system),
                AccessKind::AdminAction,
                "moderation.subtree_case.remove_unactivated",
                ("account", account),
                "30 days without a staff clearance",
                json!({ "case_id": case.map(|c| c.to_string()), "restriction_id": rid.to_string() }),
            ) {
                sr_db::admin_repo::append_in(&mut tx, &e).await?;
            }
            removed += 1;
        }
        let expired = sqlx::query(
            "UPDATE member_restrictions SET state = 'expired' WHERE state = 'active' AND NOT removes_account AND ends_at <= now()",
        )
        .execute(&mut *tx)
        .await?
        .rows_affected() as i64;
        tx.commit().await?;
        Ok((expired, removed))
    }

    /// Tell whoever shows banners that the list changed (the server wires this to `SR_ANNOUNCEMENT_UPDATE`).
    pub(crate) async fn notice_published(&self) {
        if let Some(s) = &self.notice_sink {
            s.published().await;
        }
    }
}

/// A sink told when a notice was published (so connected clients re-read the banners).
#[async_trait]
pub trait NoticeSink: Send + Sync {
    async fn published(&self);
}

/// The reason category a branch ban is filed under: the case's own when it is one of the vocabulary, else `other`.
fn ban_category_for(category: &str) -> &'static str {
    gr::REASON_CATEGORIES
        .iter()
        .copied()
        .find(|c| *c == category)
        .unwrap_or("other")
}

async fn set_node(
    conn: &mut PgConnection,
    case_id: i64,
    account: i64,
    status: &str,
    by: i64,
    note: &str,
) -> ModResult<()> {
    sqlx::query(
        "UPDATE moderation_subtree_nodes SET status = $3, decided_by = $4, decided_at = now(), note = $5 \
         WHERE case_id = $1 AND account_id = $2",
    )
    .bind(case_id)
    .bind(account)
    .bind(status)
    .bind(by)
    .bind(note)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

async fn request_by_id(conn: &mut PgConnection, id: i64) -> ModResult<ModRequest> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {REQ_COLS} FROM moderation_requests WHERE id = $1"
    )))
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    request_from(&row)
}

const SUMMARY_SELECT: &str = "SELECT c.id, c.root_account_id, COALESCE(a.username::text, '') AS root_username, c.category, c.state, \
    (extract(epoch FROM c.opened_at) * 1000)::bigint AS opened_ms, \
    (SELECT count(*) FROM moderation_subtree_nodes n WHERE n.case_id = c.id) AS nodes, \
    (SELECT count(*) FROM moderation_subtree_nodes n WHERE n.case_id = c.id AND n.status IN ('auto_frozen', 'frozen')) AS frozen, \
    (SELECT count(*) FROM moderation_subtree_nodes n WHERE n.case_id = c.id AND n.status = 'review') AS review \
    FROM moderation_subtree_cases c LEFT JOIN accounts a ON a.id = c.root_account_id";

fn summary_from(r: &sqlx::postgres::PgRow) -> ModResult<SubtreeCaseSummary> {
    Ok(SubtreeCaseSummary {
        id: r.try_get("id")?,
        root_account_id: r.try_get("root_account_id")?,
        root_username: r.try_get("root_username")?,
        category: r.try_get("category")?,
        state: r.try_get("state")?,
        opened_at_ms: r.try_get("opened_ms")?,
        nodes: r.try_get("nodes")?,
        auto_frozen: r.try_get("frozen")?,
        waiting_review: r.try_get("review")?,
    })
}

#[async_trait]
impl SeriousBanObserver for Moderation {
    async fn serious_ban(&self, ban: &SeriousBan) {
        if let Err(e) = self.open_subtree_case(ban).await {
            tracing::error!(error = %e, ban = ban.ban_id, "could not open the subtree case of a serious ban");
        }
    }
}

