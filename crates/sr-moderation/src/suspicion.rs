//! Inviter-suspicion tooling (Phase 7 step 5; spec section 10; D-031, D-125).
//!
//! * The panel on the Person page: the history, the banned accounts below the Person in the invite tree, the
//!   invite path to the root, the automatic cases and the evasion signals (MOD-SUS-001).
//! * The case a threshold crossing opens, assigned to senior staff and linked to the banning case, and the
//!   one-line notice to the inviter that invite creation is paused (MOD-SUS-002, MOD-SUS-005).
//! * Evasion signals: review items about a pair of Persons, never acted on (MOD-SUS-003). A signal stores
//!   counts and words only. The shared addresses are read from `login_events` when a staff member opens the
//!   signal, so the 180-day window and the scrubbing on account deletion apply to them, and the read is written
//!   to the access log.
//! * Clearing suspicion is the admin service's action (reason required, logged, events kept); the observer
//!   here puts it on the timeline of the open automatic case (MOD-SUS-004).

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use serde_json::{Value, json};
use sqlx::{PgConnection, Row, postgres::PgRow};
use sr_admin::{AccessKind, SuspicionCrossing, SuspicionObserver, settings};
use sr_core::PlatformPermissions as P;

use crate::error::{ModError, ModResult, conflict, invalid};
use crate::model::{
    AccountRef, BannedDescendant, CaseState, InvitePath, PanelEvent, ScanOutcome, SharedAddress,
    SignalDetail, SignalFilter, SignalKind, SignalState, SignalView, SuspicionCaseRef,
    SuspicionPanel, Who,
};
use crate::service::{Moderation, log_staff, push_event, require_staff};
use crate::suspicion_rules::{
    self as rules, IP_WINDOW_DAYS, Lineage, MAX_SIGNALS_PER_SCAN, PairFindings,
};

const DEFAULT_FREEZE: i64 = 2;
const DEFAULT_ESCALATE: i64 = 3;
/// Rows of the sign-in history one scan reads per Person.
const SCAN_ROW_CAP: i64 = 2000;
/// Most Persons one housekeeping run scans.
pub const SCAN_BATCH: i64 = 500;
/// The most rows one purge statement removes.
const PURGE_BATCH: i64 = 20_000;

fn ms(col: &str) -> String {
    format!("(extract(epoch FROM {col}) * 1000)::bigint")
}

fn account_ref(id: Option<i64>, name: Option<String>) -> Option<AccountRef> {
    match (id, name) {
        (Some(id), Some(username)) => Some(AccountRef { id, username }),
        _ => None,
    }
}

fn signal_select() -> String {
    format!(
        "SELECT s.id, s.person_id, pa.id AS acc_id, pa.username::text AS acc_name, s.related_person_id, \
         ra.id AS racc_id, ra.username::text AS racc_name, s.kind, s.detail, s.state, {created} AS created_ms, \
         s.reviewed_by, {reviewed} AS reviewed_ms, s.review_reason \
         FROM evasion_signals s \
         LEFT JOIN LATERAL (SELECT id, username FROM accounts WHERE person_id = s.person_id AND id <> 1 \
                            ORDER BY id LIMIT 1) pa ON true \
         LEFT JOIN LATERAL (SELECT id, username FROM accounts WHERE person_id = s.related_person_id AND id <> 1 \
                            ORDER BY id LIMIT 1) ra ON true",
        created = ms("s.created_at"),
        reviewed = ms("s.reviewed_at"),
    )
}

fn signal_from(r: &PgRow) -> ModResult<SignalView> {
    Ok(SignalView {
        id: r.try_get("id")?,
        person_id: r.try_get("person_id")?,
        account: account_ref(r.try_get("acc_id")?, r.try_get("acc_name")?),
        related_person_id: r.try_get("related_person_id")?,
        related_account: account_ref(r.try_get("racc_id")?, r.try_get("racc_name")?),
        kind: SignalKind::parse(&r.try_get::<String, _>("kind")?)
            .unwrap_or(SignalKind::SharedLoginIp),
        detail: r.try_get("detail")?,
        state: SignalState::parse(&r.try_get::<String, _>("state")?).unwrap_or(SignalState::Open),
        created_at_ms: r.try_get("created_ms")?,
        reviewed_by: r.try_get("reviewed_by")?,
        reviewed_at_ms: r.try_get("reviewed_ms")?,
        review_reason: r.try_get("review_reason")?,
    })
}

async fn thresholds(conn: &mut PgConnection) -> ModResult<(i64, i64)> {
    let rows = sqlx::query("SELECT key, value FROM platform_settings WHERE key = ANY($1)")
        .bind(vec![
            settings::SUSPICION_FREEZE.to_owned(),
            settings::SUSPICION_ESCALATE.to_owned(),
        ])
        .fetch_all(&mut *conn)
        .await?;
    let (mut freeze, mut escalate) = (DEFAULT_FREEZE, DEFAULT_ESCALATE);
    for r in rows {
        let key: String = r.try_get("key")?;
        let v: Value = r.try_get("value")?;
        let Some(n) = v.as_i64() else { continue };
        if key == settings::SUSPICION_FREEZE {
            freeze = n;
        } else if key == settings::SUSPICION_ESCALATE {
            escalate = n;
        }
    }
    Ok((freeze, escalate))
}

/// Senior staff are the accounts whose roles together hold both `MANAGE_CASES` and `MANAGE_TAKEDOWNS` (D-210:
/// Senior Staff holds both, Moderator only cases), so a renamed role still counts. The one with the fewest open
/// cases gets the case; nobody when there is no one.
async fn pick_senior(conn: &mut PgConnection) -> ModResult<Option<i64>> {
    let need = (P::MANAGE_CASES | P::MANAGE_TAKEDOWNS).bits() as i64;
    let id: Option<i64> = sqlx::query_scalar(
        "SELECT mr.user_id FROM member_roles mr \
         JOIN roles r ON r.guild_id = mr.guild_id AND r.id = mr.role_id \
         JOIN guilds g ON g.id = r.guild_id AND g.is_staff_guild \
         JOIN staff_appointments sa ON sa.account_id = mr.user_id AND sa.revoked_at IS NULL \
         JOIN accounts a ON a.id = mr.user_id AND a.state = 'active' \
         GROUP BY mr.user_id \
         HAVING (bit_or(r.platform_permissions) & $1) = $1 \
         ORDER BY (SELECT count(*) FROM moderation_cases c \
                   WHERE c.assignee_id = mr.user_id AND c.state NOT IN ('dismissed', 'closed')), mr.user_id \
         LIMIT 1",
    )
    .bind(need)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(id)
}

impl Moderation {
    // ---- the panel (MOD-SUS-001) ------------------------------------------------------------------------------

    /// Everything the Person page shows about suspicion. `MANAGE_INVITES`; the read is logged. The evasion
    /// signals are IP-derived and appear only for staff who also hold `MANAGE_ACCOUNTS`.
    pub async fn suspicion_panel(
        &self,
        who: &Who<'_>,
        person_id: i64,
    ) -> ModResult<SuspicionPanel> {
        require_staff(who, P::MANAGE_INVITES, "MANAGE_INVITES")?;
        let see_signals = matches!(who, Who::Staff(c) if c.permissions.has(P::MANAGE_ACCOUNTS));
        let mut tx = self.pool.begin().await?;
        let level: Option<i32> =
            sqlx::query_scalar("SELECT suspicion_level::int FROM persons WHERE id = $1")
                .bind(person_id)
                .fetch_optional(&mut *tx)
                .await?;
        let level = level.ok_or(ModError::NotFound("person"))?;
        let (freeze, escalate) = thresholds(&mut tx).await?;

        let accounts: Vec<AccountRef> = sqlx::query(
            "SELECT id, username::text AS username FROM accounts WHERE person_id = $1 AND id <> 1 ORDER BY id",
        )
        .bind(person_id)
        .fetch_all(&mut *tx)
        .await?
        .iter()
        .map(|r| {
            Ok(AccountRef {
                id: r.try_get("id")?,
                username: r.try_get("username")?,
            })
        })
        .collect::<ModResult<_>>()?;

        let events = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT e.id, e.delta::int AS delta, e.level_after, e.reason, e.source_account_id, \
             sa.username::text AS source_name, e.actor_id, e.case_id, {} AS created_ms \
             FROM suspicion_events e LEFT JOIN accounts sa ON sa.id = e.source_account_id \
             WHERE e.person_id = $1 ORDER BY e.id DESC LIMIT 100",
            ms("e.created_at")
        )))
        .bind(person_id)
        .fetch_all(&mut *tx)
        .await?
        .iter()
        .map(|r| {
            let delta: i32 = r.try_get("delta")?;
            Ok(PanelEvent {
                id: r.try_get("id")?,
                delta,
                level_after: r.try_get("level_after")?,
                kind: if delta < 0 { "cleared" } else { "raised" }.to_owned(),
                reason: r.try_get("reason")?,
                source_account: account_ref(
                    r.try_get("source_account_id")?,
                    r.try_get("source_name")?,
                ),
                actor_id: r.try_get("actor_id")?,
                case_id: r.try_get("case_id")?,
                created_at_ms: r.try_get("created_ms")?,
            })
        })
        .collect::<ModResult<Vec<_>>>()?;

        let banned_descendants = sqlx::query(sqlx::AssertSqlSafe(format!(
            "WITH RECURSIVE tree AS ( \
                SELECT a.id, a.person_id, 1 AS depth, ARRAY[a.id] AS path \
                FROM accounts a JOIN platform_invites pi ON pi.id = a.invited_by_invite_id \
                WHERE pi.issuer_account_id IN (SELECT id FROM accounts WHERE person_id = $1) \
                UNION ALL \
                SELECT a.id, a.person_id, t.depth + 1, t.path || a.id \
                FROM tree t JOIN platform_invites pi ON pi.issuer_account_id = t.id \
                JOIN accounts a ON a.invited_by_invite_id = pi.id \
                WHERE t.depth < 6 AND NOT (a.id = ANY(t.path))) \
             SELECT t.id, ac.username::text AS username, t.person_id, t.depth, b.category::text AS category, \
                    b.reason_category::text AS reason_category, {} AS banned_ms, \
                    (b.lifted_at IS NOT NULL) AS lifted, \
                    (SELECT max(c.id) FROM moderation_cases c WHERE c.subject_person_id = t.person_id) AS case_id \
             FROM tree t JOIN accounts ac ON ac.id = t.id \
             JOIN LATERAL (SELECT * FROM person_bans x WHERE x.person_id = t.person_id ORDER BY x.id DESC LIMIT 1) b ON true \
             WHERE t.person_id <> $1 ORDER BY t.depth, b.created_at DESC, t.id LIMIT 200",
            ms("b.created_at")
        )))
        .bind(person_id)
        .fetch_all(&mut *tx)
        .await?
        .iter()
        .map(|r| {
            Ok(BannedDescendant {
                account: AccountRef {
                    id: r.try_get("id")?,
                    username: r.try_get("username")?,
                },
                person_id: r.try_get("person_id")?,
                depth: r.try_get("depth")?,
                ban_category: r.try_get("category")?,
                reason_category: r.try_get("reason_category")?,
                banned_at_ms: r.try_get("banned_ms")?,
                lifted: r.try_get("lifted")?,
                case_id: r.try_get("case_id")?,
            })
        })
        .collect::<ModResult<Vec<_>>>()?;

        let mut invite_paths = Vec::new();
        for a in &accounts {
            let nodes = sqlx::query(
                "WITH RECURSIVE chain AS ( \
                    SELECT a.id, a.username::text AS username, pi.issuer_account_id AS parent_id, 0 AS depth \
                    FROM accounts a LEFT JOIN platform_invites pi ON pi.id = a.invited_by_invite_id \
                    WHERE a.id = $1 \
                    UNION ALL \
                    SELECT p.id, p.username::text, pi.issuer_account_id, c.depth - 1 \
                    FROM chain c JOIN accounts p ON p.id = c.parent_id \
                    LEFT JOIN platform_invites pi ON pi.id = p.invited_by_invite_id \
                    WHERE c.depth > -50) \
                 SELECT id, username FROM chain ORDER BY depth",
            )
            .bind(a.id)
            .fetch_all(&mut *tx)
            .await?
            .iter()
            .map(|r| {
                Ok(AccountRef {
                    id: r.try_get("id")?,
                    username: r.try_get("username")?,
                })
            })
            .collect::<ModResult<Vec<_>>>()?;
            invite_paths.push(InvitePath {
                account_id: a.id,
                nodes,
            });
        }

        let cases = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT id, state, auto_kind, assignee_id, linked_case_id, {} AS opened_ms \
             FROM moderation_cases WHERE subject_person_id = $1 AND auto_kind IS NOT NULL \
             ORDER BY id DESC LIMIT 20",
            ms("opened_at")
        )))
        .bind(person_id)
        .fetch_all(&mut *tx)
        .await?
        .iter()
        .map(|r| {
            Ok(SuspicionCaseRef {
                id: r.try_get("id")?,
                state: CaseState::parse(&r.try_get::<String, _>("state")?)
                    .unwrap_or(CaseState::Open),
                auto_kind: r.try_get("auto_kind")?,
                assignee_id: r.try_get("assignee_id")?,
                linked_case_id: r.try_get("linked_case_id")?,
                opened_at_ms: r.try_get("opened_ms")?,
            })
        })
        .collect::<ModResult<Vec<_>>>()?;

        let signals = if see_signals {
            let sql = format!(
                "{} WHERE s.person_id = $1 OR s.related_person_id = $1 ORDER BY s.id DESC LIMIT 50",
                signal_select()
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(person_id)
                .fetch_all(&mut *tx)
                .await?;
            Some(
                rows.iter()
                    .map(signal_from)
                    .collect::<ModResult<Vec<_>>>()?,
            )
        } else {
            None
        };

        log_staff(
            &mut tx,
            who,
            AccessKind::Read,
            "moderation.suspicion.view",
            ("person", person_id),
            "",
            json!({ "signals_shown": see_signals }),
        )
        .await?;
        tx.commit().await?;
        Ok(SuspicionPanel {
            person_id,
            level,
            freeze_threshold: freeze,
            escalate_threshold: escalate,
            invites_paused: i64::from(level) >= freeze,
            accounts,
            events,
            banned_descendants,
            invite_paths,
            cases,
            signals,
        })
    }

    // ---- the automatic case (MOD-SUS-002, MOD-SUS-005) --------------------------------------------------------

    /// Open (or upgrade) the case for a threshold crossing. One automatic case is open per Person: a later
    /// escalation upgrades the freeze case instead of opening a second. The case is platform business, assigned
    /// to the senior staff member with the fewest open cases and linked to the newest case about the banned
    /// invitee. The inviter gets the one-line notice when the freeze is crossed. Returns the case id.
    pub async fn open_suspicion_case(&self, x: &SuspicionCrossing) -> ModResult<i64> {
        let Some(kind) = rules::auto_kind(x.freeze_crossed, x.escalate_crossed) else {
            return Err(invalid("no threshold was crossed"));
        };
        let mut tx = self.pool.begin().await?;
        let existing: Option<(i64, String, Option<i64>)> = sqlx::query_as(
            "SELECT id, auto_kind, assignee_id FROM moderation_cases \
             WHERE subject_person_id = $1 AND auto_kind IS NOT NULL AND state NOT IN ('dismissed', 'closed') \
             FOR UPDATE",
        )
        .bind(x.person_id)
        .fetch_optional(&mut *tx)
        .await?;
        let detail = json!({
            "level": x.level,
            "freeze_crossed": x.freeze_crossed,
            "escalate_crossed": x.escalate_crossed,
            "banned_person_id": x.banned_person_id.to_string(),
        });
        let case_id = if let Some((id, current, assignee)) = existing {
            if kind == rules::AutoKind::Escalation
                && rules::AutoKind::parse(&current) == Some(rules::AutoKind::Freeze)
            {
                let senior = match assignee {
                    Some(a) => Some(a),
                    None => pick_senior(&mut tx).await?,
                };
                sqlx::query(
                    "UPDATE moderation_cases SET auto_kind = 'suspicion_escalation', severity = 'serious', \
                     summary = $2, assignee_id = $3, updated_at = now() WHERE id = $1",
                )
                .bind(id)
                .bind(rules::case_summary(kind, x.level))
                .bind(senior)
                .execute(&mut *tx)
                .await?;
                push_event(
                    &mut tx,
                    self.next_id(),
                    id,
                    None,
                    "suspicion_escalated",
                    detail,
                )
                .await?;
            } else {
                push_event(
                    &mut tx,
                    self.next_id(),
                    id,
                    None,
                    "suspicion_raised",
                    detail,
                )
                .await?;
            }
            id
        } else {
            let id = self.next_id();
            let banning: Option<i64> = sqlx::query_scalar(
                "SELECT max(id) FROM moderation_cases WHERE subject_person_id = $1 AND auto_kind IS NULL",
            )
            .bind(x.banned_person_id)
            .fetch_one(&mut *tx)
            .await?;
            let senior = pick_senior(&mut tx).await?;
            sqlx::query(
                "INSERT INTO moderation_cases (id, category, severity, routing, summary, target_kind, target_id, \
                 subject_person_id, subject_account_id, opened_by_kind, opened_by_id, assignee_id, linked_case_id, \
                 auto_kind) \
                 VALUES ($1, 'other', 'serious', 'platform', $2, 'person', $3, $3, $4, 'staff', $5, $6, $7, $8)",
            )
            .bind(id)
            .bind(rules::case_summary(kind, x.level))
            .bind(x.person_id)
            .bind(x.inviter_account_id)
            .bind(x.actor_id)
            .bind(senior)
            .bind(banning)
            .bind(kind.as_str())
            .execute(&mut *tx)
            .await?;
            push_event(
                &mut tx,
                self.next_id(),
                id,
                None,
                "case_opened",
                json!({ "by": "system", "auto_kind": kind.as_str(), "category": "other" }),
            )
            .await?;
            push_event(
                &mut tx,
                self.next_id(),
                id,
                None,
                "suspicion_raised",
                detail,
            )
            .await?;
            if let Some(a) = senior {
                push_event(
                    &mut tx,
                    self.next_id(),
                    id,
                    None,
                    "assigned",
                    json!({ "assignee_id": a.to_string(), "by": "system", "why": "senior staff" }),
                )
                .await?;
            }
            id
        };
        sqlx::query(
            "UPDATE suspicion_events SET case_id = $2 \
             WHERE id = (SELECT max(id) FROM suspicion_events WHERE person_id = $1 AND delta > 0)",
        )
        .bind(x.person_id)
        .bind(case_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        if x.freeze_crossed {
            // Only that invite creation is paused; never which invitee, nor that a ban was involved.
            let _ = self
                .dm_subject(x.inviter_account_id, rules::INVITER_NOTICE)
                .await;
        }
        Ok(case_id)
    }

    /// Put a clear on the timeline of the Person's open automatic case. The events stay (MOD-SUS-004).
    pub async fn note_suspicion_cleared(
        &self,
        person_id: i64,
        actor_id: i64,
        reason: &str,
    ) -> ModResult<u64> {
        let mut tx = self.pool.begin().await?;
        let cases: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM moderation_cases WHERE subject_person_id = $1 AND auto_kind IS NOT NULL \
             AND state NOT IN ('dismissed', 'closed')",
        )
        .bind(person_id)
        .fetch_all(&mut *tx)
        .await?;
        for id in &cases {
            push_event(
                &mut tx,
                self.next_id(),
                *id,
                Some(actor_id),
                "suspicion_cleared",
                json!({ "reason": reason }),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(cases.len() as u64)
    }

    // ---- evasion signals (MOD-SUS-003) ------------------------------------------------------------------------

    /// Compare one Person with the banned Persons and raise the signals the findings justify (system, no
    /// permission check). Returns how many were newly raised. Nothing is banned, frozen or hidden by this.
    pub async fn scan_person(&self, person_id: i64) -> ModResult<i64> {
        let mut tx = self.pool.begin().await?;
        let n = self.scan_person_in(&mut tx, person_id).await?;
        tx.commit().await?;
        Ok(n)
    }

    /// Staff-triggered scan of one Person (`MANAGE_ACCOUNTS`; logged).
    pub async fn scan_person_as(&self, who: &Who<'_>, person_id: i64) -> ModResult<ScanOutcome> {
        require_staff(who, P::MANAGE_ACCOUNTS, "MANAGE_ACCOUNTS")?;
        let mut tx = self.pool.begin().await?;
        let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM persons WHERE id = $1")
            .bind(person_id)
            .fetch_optional(&mut *tx)
            .await?;
        if exists.is_none() {
            return Err(ModError::NotFound("person"));
        }
        let raised = self.scan_person_in(&mut tx, person_id).await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::Read,
            "moderation.evasion.scan",
            ("person", person_id),
            "",
            json!({ "raised": raised }),
        )
        .await?;
        tx.commit().await?;
        Ok(ScanOutcome { raised })
    }

    async fn scan_person_in(&self, conn: &mut PgConnection, person_id: i64) -> ModResult<i64> {
        let mut found: BTreeMap<i64, PairFindings> = BTreeMap::new();

        // Addresses both Persons signed in from inside the window, where the address is not a shared network.
        let ips = sqlx::query(
            "SELECT ba.person_id AS related, host(le.ip) AS ip, \
                    (SELECT count(DISTINCT a2.person_id) FROM login_events x \
                     JOIN accounts a2 ON a2.id = x.account_id \
                     WHERE x.ip = le.ip AND x.success AND x.created_at > now() - make_interval(days => $2::int)) AS persons \
             FROM login_events le \
             JOIN accounts me ON me.id = le.account_id AND me.person_id = $1 \
             JOIN login_events ob ON ob.ip = le.ip AND ob.success \
                  AND ob.created_at > now() - make_interval(days => $2::int) \
             JOIN accounts ba ON ba.id = ob.account_id AND ba.person_id <> $1 \
             WHERE le.success AND le.ip IS NOT NULL AND le.created_at > now() - make_interval(days => $2::int) \
               AND EXISTS (SELECT 1 FROM person_bans b WHERE b.person_id = ba.person_id) \
             GROUP BY ba.person_id, le.ip LIMIT $3",
        )
        .bind(person_id)
        .bind(IP_WINDOW_DAYS as i32)
        .bind(SCAN_ROW_CAP)
        .fetch_all(&mut *conn)
        .await?;
        let mut credible: BTreeMap<i64, BTreeSet<String>> = BTreeMap::new();
        for r in &ips {
            let persons: i64 = r.try_get("persons")?;
            if rules::ip_is_signal(persons) {
                credible
                    .entry(r.try_get("related")?)
                    .or_default()
                    .insert(r.try_get("ip")?);
            }
        }
        for (related, set) in credible {
            found.entry(related).or_default().shared_ips = set.len() as i64;
        }

        // Invited by a banned Person's account.
        let direct: Vec<i64> = sqlx::query_scalar(
            "SELECT DISTINCT ia.person_id FROM accounts me \
             JOIN platform_invites pi ON pi.id = me.invited_by_invite_id \
             JOIN accounts ia ON ia.id = pi.issuer_account_id \
             WHERE me.person_id = $1 AND ia.person_id <> $1 AND ia.id <> 1 \
               AND EXISTS (SELECT 1 FROM person_bans b WHERE b.person_id = ia.person_id)",
        )
        .bind(person_id)
        .fetch_all(&mut *conn)
        .await?;
        for p in direct {
            found.entry(p).or_default().lineage = Some(Lineage::InvitedByBanned);
        }
        // Invited by the same account as a banned Person.
        let siblings: Vec<i64> = sqlx::query_scalar(
            "SELECT DISTINCT sib.person_id FROM accounts me \
             JOIN platform_invites pi ON pi.id = me.invited_by_invite_id \
             JOIN platform_invites pj ON pj.issuer_account_id = pi.issuer_account_id \
             JOIN accounts sib ON sib.invited_by_invite_id = pj.id \
             WHERE me.person_id = $1 AND pi.issuer_account_id <> 1 AND sib.person_id <> $1 \
               AND EXISTS (SELECT 1 FROM person_bans b WHERE b.person_id = sib.person_id)",
        )
        .bind(person_id)
        .fetch_all(&mut *conn)
        .await?;
        for p in siblings {
            found
                .entry(p)
                .or_default()
                .lineage
                .get_or_insert(Lineage::SameInviter);
        }

        // A new account shortly after a ban of a Person we already found a link to.
        if !found.is_empty() {
            let created: Option<i64> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "SELECT {} FROM accounts WHERE person_id = $1 AND id <> 1",
                ms("min(created_at)")
            )))
            .bind(person_id)
            .fetch_one(&mut *conn)
            .await?;
            if let Some(created) = created {
                let related: Vec<i64> = found.keys().copied().collect();
                let bans = sqlx::query(sqlx::AssertSqlSafe(format!(
                    "SELECT person_id, {} AS banned_ms FROM person_bans WHERE person_id = ANY($1)",
                    ms("created_at")
                )))
                .bind(&related)
                .fetch_all(&mut *conn)
                .await?;
                for b in &bans {
                    let p: i64 = b.try_get("person_id")?;
                    let at: i64 = b.try_get("banned_ms")?;
                    if rules::created_shortly_after_ban(created, at)
                        && let Some(f) = found.get_mut(&p)
                    {
                        f.new_after_ban = true;
                    }
                }
            }
        }

        let mut raised = 0;
        let mut budget = MAX_SIGNALS_PER_SCAN;
        for (related, f) in &found {
            for s in rules::signals_for(f) {
                if budget == 0 {
                    return Ok(raised);
                }
                let n = sqlx::query(
                    "INSERT INTO evasion_signals (id, person_id, related_person_id, kind, detail) \
                     VALUES ($1, $2, $3, $4, $5) ON CONFLICT (person_id, related_person_id, kind) DO NOTHING",
                )
                .bind(self.next_id())
                .bind(person_id)
                .bind(related)
                .bind(s.kind.as_str())
                .bind(s.detail)
                .execute(&mut *conn)
                .await?
                .rows_affected();
                if n > 0 {
                    raised += 1;
                    budget -= 1;
                }
            }
        }
        Ok(raised)
    }

    /// Housekeeping: scan the Persons who signed in during the last `hours` hours (and are not banned).
    pub async fn scan_recent_logins(&self, hours: i32, limit: i64) -> ModResult<i64> {
        let people: Vec<i64> = sqlx::query_scalar(
            "SELECT DISTINCT a.person_id FROM login_events le JOIN accounts a ON a.id = le.account_id \
             WHERE le.success AND le.created_at > now() - make_interval(hours => $1) AND a.id <> 1 \
               AND NOT EXISTS (SELECT 1 FROM person_bans b WHERE b.person_id = a.person_id AND b.lifted_at IS NULL) \
             LIMIT $2",
        )
        .bind(hours)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let mut raised = 0;
        for p in people {
            raised += self.scan_person(p).await?;
        }
        Ok(raised)
    }

    /// Housekeeping: sign-in history older than the 180-day window is deleted (doc 09). Returns the rows removed.
    pub async fn purge_login_events(&self) -> ModResult<u64> {
        let mut total = 0;
        loop {
            let n = sqlx::query(
                "DELETE FROM login_events WHERE id IN (SELECT id FROM login_events \
                 WHERE created_at < now() - make_interval(days => $1::int) LIMIT $2)",
            )
            .bind(IP_WINDOW_DAYS as i32)
            .bind(PURGE_BATCH)
            .execute(&self.pool)
            .await?
            .rows_affected();
            total += n;
            if n < PURGE_BATCH as u64 {
                return Ok(total);
            }
        }
    }

    /// The signal list (`MANAGE_ACCOUNTS`; the read is logged), newest first.
    pub async fn list_signals(
        &self,
        who: &Who<'_>,
        f: &SignalFilter,
    ) -> ModResult<Vec<SignalView>> {
        require_staff(who, P::MANAGE_ACCOUNTS, "MANAGE_ACCOUNTS")?;
        let sql = format!(
            "{} WHERE ($1::text IS NULL OR s.state = $1) \
             AND ($2::bigint IS NULL OR s.person_id = $2 OR s.related_person_id = $2) \
             ORDER BY s.id DESC LIMIT $3",
            signal_select()
        );
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(f.state.map(SignalState::as_str))
            .bind(f.person_id)
            .bind(f.limit.clamp(1, 200))
            .fetch_all(&mut *tx)
            .await?;
        let out = rows
            .iter()
            .map(signal_from)
            .collect::<ModResult<Vec<_>>>()?;
        log_staff(
            &mut tx,
            who,
            AccessKind::Read,
            "moderation.evasion.list",
            ("signals", 0),
            "",
            json!({ "count": out.len() }),
        )
        .await?;
        tx.commit().await?;
        Ok(out)
    }

    /// One signal with the addresses behind it, read live from the sign-in history (`MANAGE_ACCOUNTS`). Only
    /// addresses still inside the window and not scrubbed are shown; the read is logged.
    pub async fn signal_detail(&self, who: &Who<'_>, id: i64) -> ModResult<SignalDetail> {
        require_staff(who, P::MANAGE_ACCOUNTS, "MANAGE_ACCOUNTS")?;
        let mut tx = self.pool.begin().await?;
        let sql = format!("{} WHERE s.id = $1", signal_select());
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(ModError::NotFound("signal"))?;
        let signal = signal_from(&row)?;
        let mut shared_addresses = Vec::new();
        if signal.kind == SignalKind::SharedLoginIp {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT host(le.ip) AS ip, {mine} AS mine_ms, {theirs} AS theirs_ms, \
                        (SELECT count(DISTINCT a2.person_id) FROM login_events x \
                         JOIN accounts a2 ON a2.id = x.account_id \
                         WHERE x.ip = le.ip AND x.success AND x.created_at > now() - make_interval(days => $3::int)) AS persons \
                 FROM login_events le JOIN accounts me ON me.id = le.account_id AND me.person_id = $1 \
                 JOIN login_events ob ON ob.ip = le.ip AND ob.success \
                      AND ob.created_at > now() - make_interval(days => $3::int) \
                 JOIN accounts ba ON ba.id = ob.account_id AND ba.person_id = $2 \
                 WHERE le.success AND le.ip IS NOT NULL AND le.created_at > now() - make_interval(days => $3::int) \
                 GROUP BY le.ip ORDER BY max(le.created_at) DESC LIMIT 50",
                mine = ms("max(le.created_at)"),
                theirs = ms("max(ob.created_at)"),
            )))
            .bind(signal.person_id)
            .bind(signal.related_person_id)
            .bind(IP_WINDOW_DAYS as i32)
            .fetch_all(&mut *tx)
            .await?;
            for r in &rows {
                let persons: i64 = r.try_get("persons")?;
                if rules::ip_is_signal(persons) {
                    shared_addresses.push(SharedAddress {
                        ip: r.try_get("ip")?,
                        person_last_seen_ms: r.try_get("mine_ms")?,
                        related_last_seen_ms: r.try_get("theirs_ms")?,
                        persons,
                    });
                }
            }
        }
        log_staff(
            &mut tx,
            who,
            AccessKind::Read,
            "moderation.evasion.detail",
            ("signal", id),
            "",
            json!({ "kind": signal.kind.as_str(), "addresses_shown": shared_addresses.len() }),
        )
        .await?;
        tx.commit().await?;
        Ok(SignalDetail {
            signal,
            window_days: IP_WINDOW_DAYS,
            shared_addresses,
        })
    }

    /// Mark a signal reviewed: `dismissed` (not the same person) or `confirmed` (looks like the same person; any
    /// ban is a separate decision under `PLATFORM_BAN`). A reason is required and the change is logged.
    pub async fn resolve_signal(
        &self,
        who: &Who<'_>,
        id: i64,
        decision: SignalState,
        reason: &str,
    ) -> ModResult<SignalView> {
        require_staff(who, P::MANAGE_ACCOUNTS, "MANAGE_ACCOUNTS")?;
        if decision == SignalState::Open {
            return Err(invalid("decision must be dismissed or confirmed"));
        }
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(ModError::ReasonRequired);
        }
        if reason.chars().count() > 500 {
            return Err(invalid("reason must be 500 characters or fewer"));
        }
        let mut tx = self.pool.begin().await?;
        let n = sqlx::query(
            "UPDATE evasion_signals SET state = $2, reviewed_by = $3, reviewed_at = now(), review_reason = $4 \
             WHERE id = $1 AND state = 'open'",
        )
        .bind(id)
        .bind(decision.as_str())
        .bind(who.user_id())
        .bind(reason)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if n == 0 {
            let exists: Option<i64> =
                sqlx::query_scalar("SELECT id FROM evasion_signals WHERE id = $1")
                    .bind(id)
                    .fetch_optional(&mut *tx)
                    .await?;
            return Err(if exists.is_some() {
                conflict("The signal was already reviewed")
            } else {
                ModError::NotFound("signal")
            });
        }
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.evasion.resolve",
            ("signal", id),
            reason,
            json!({ "decision": decision.as_str() }),
        )
        .await?;
        let sql = format!("{} WHERE s.id = $1", signal_select());
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        let out = signal_from(&row)?;
        tx.commit().await?;
        Ok(out)
    }
}

#[async_trait]
impl SuspicionObserver for Moderation {
    async fn threshold_crossed(&self, crossing: SuspicionCrossing) {
        if let Err(e) = self.open_suspicion_case(&crossing).await {
            tracing::error!(error = %e, person = crossing.person_id, "could not open the suspicion case");
        }
    }

    async fn cleared(&self, person_id: i64, actor_id: i64, reason: &str) {
        if let Err(e) = self
            .note_suspicion_cleared(person_id, actor_id, reason)
            .await
        {
            tracing::error!(error = %e, person = person_id, "could not note the cleared suspicion on the case");
        }
    }
}
