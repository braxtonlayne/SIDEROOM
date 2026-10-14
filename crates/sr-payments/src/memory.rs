//! In-memory [`PaymentRepo`]: the reference behaviour for tests (the Postgres repository must
//! behave the same; its ignored tests mirror the scenarios here).

use crate::{
    config::DonationConfig,
    entitlements::{
        Grant, GrantKind, GrantSource, GuildEntitlement, PatronStatus, notice_level, patron_status,
        pool_cents, stack_patron,
    },
    error::{PayError, PayResult},
    invoice::{Evaluation, Invoice, InvoiceState, Purpose, Resolution, ResolutionKind, TxRecord},
    repo::*,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, MutexGuard},
};

#[derive(Default)]
struct Mem {
    wallets: Vec<Wallet>,
    config: Option<DonationConfig>,
    invoices: Vec<Invoice>,
    txs: Vec<TxRecord>,
    audit: Vec<(i64, AuditRow)>,
    credits: HashMap<i64, bool>,
    grants: Vec<Grant>,
    next_grant: i64,
    next_wallet: i64,
    /// `(user, delta, invoice)`.
    ledger: Vec<(i64, i64, i64)>,
    levels: HashMap<i64, (u8, Option<i64>)>,
    prefs: HashMap<i64, DonorPrefs>,
    hidden_guilds: HashSet<i64>,
    staff_hidden_users: HashSet<i64>,
    staff_hidden_guilds: HashSet<i64>,
    members: HashMap<(i64, i64), GuildAccess>,
    notices: HashMap<i64, u8>,
    watcher: WatcherStatus,
    profiles: HashMap<i64, Profile>,
}

#[derive(Default)]
pub struct MemoryRepo {
    m: Mutex<Mem>,
}

impl MemoryRepo {
    pub fn new() -> Self {
        Self::default()
    }

    fn m(&self) -> MutexGuard<'_, Mem> {
        self.m.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Test setup: `user` belongs to `guild` (and owns it when `owner`).
    pub fn add_member(&self, user: i64, guild: i64, owner: bool) {
        self.m().members.insert(
            (user, guild),
            GuildAccess {
                member: true,
                owner,
            },
        );
    }

    pub fn add_profile(&self, id: i64, name: &str) {
        self.m().profiles.insert(
            id,
            Profile {
                id,
                name: name.to_owned(),
                avatar: None,
            },
        );
    }

    /// Invite credit balance of a user (sum of the ledger).
    pub fn invite_balance(&self, user: i64) -> i64 {
        self.m()
            .ledger
            .iter()
            .filter(|(u, _, _)| *u == user)
            .map(|(_, d, _)| d)
            .sum()
    }

    pub fn grants(&self) -> Vec<Grant> {
        self.m().grants.clone()
    }

    pub fn audit_kinds(&self, invoice: i64) -> Vec<String> {
        self.m()
            .audit
            .iter()
            .filter(|(i, _)| *i == invoice)
            .map(|(_, a)| a.kind.clone())
            .collect()
    }
}

fn invoice_mut(m: &mut Mem, id: i64) -> PayResult<&mut Invoice> {
    m.invoices
        .iter_mut()
        .find(|i| i.id == id)
        .ok_or(PayError::NotFound)
}

fn push_audit(m: &mut Mem, id: i64, kind: &str, detail: Value, now: i64) {
    m.audit.push((
        id,
        AuditRow {
            at_ms: now,
            kind: kind.to_owned(),
            detail,
        },
    ));
}

/// Applies a plan's grants and ledger rows for an invoice (shared by crediting paths).
fn apply_plan(m: &mut Mem, inv_id: i64, plan: &CreditPlan, now: i64) {
    for spec in &plan.grants {
        m.next_grant += 1;
        let id = m.next_grant;
        m.grants.push(Grant {
            id,
            kind: spec.kind,
            user_id: spec.user_id,
            guild_id: spec.guild_id,
            source: GrantSource::Invoice,
            source_id: Some(inv_id),
            days: spec.days,
            ends_ms: None,
            permanent: false,
            usd_cents: spec.usd_cents,
            starts_ms: now,
            revoked_ms: None,
            reason: None,
        });
    }
    if let (Some(u), n) = (plan.invite_user, plan.invite_credits)
        && n != 0
    {
        m.ledger.push((u, n, inv_id));
    }
}

fn revert_plan(m: &mut Mem, inv_id: i64, plan: &CreditPlan, now: i64) {
    for g in m.grants.iter_mut().filter(|g| {
        g.source == GrantSource::Invoice && g.source_id == Some(inv_id) && g.revoked_ms.is_none()
    }) {
        g.revoked_ms = Some(now);
    }
    if let (Some(u), n) = (plan.invite_user, plan.invite_credits)
        && n != 0
    {
        m.ledger.push((u, -n, inv_id));
    }
}

#[async_trait]
impl PaymentRepo for MemoryRepo {
    async fn active_wallet(&self) -> PayResult<Option<Wallet>> {
        Ok(self.m().wallets.iter().find(|w| w.active).cloned())
    }

    async fn wallet(&self, id: i64) -> PayResult<Option<Wallet>> {
        Ok(self.m().wallets.iter().find(|w| w.id == id).cloned())
    }

    async fn set_wallet(
        &self,
        network: &str,
        descriptor: &str,
        fingerprint: &str,
        gap_limit: u32,
        _by: Option<i64>,
        now_ms: i64,
    ) -> PayResult<Wallet> {
        let mut m = self.m();
        if let Some(w) = m
            .wallets
            .iter_mut()
            .find(|w| w.fingerprint == fingerprint && w.network == network)
        {
            // Re-activating a known wallet keeps its index counter (addresses are never reused).
            w.gap_limit = gap_limit;
            w.active = true;
            let id = w.id;
            for o in m.wallets.iter_mut().filter(|o| o.id != id) {
                o.active = false;
            }
            return Ok(m
                .wallets
                .iter()
                .find(|w| w.id == id)
                .cloned()
                .unwrap_or_else(|| unreachable!()));
        }
        for o in &mut m.wallets {
            o.active = false;
        }
        m.next_wallet += 1;
        let w = Wallet {
            id: m.next_wallet,
            network: network.to_owned(),
            descriptor: descriptor.to_owned(),
            fingerprint: fingerprint.to_owned(),
            next_index: 0,
            gap_limit,
            active: true,
            created_ms: now_ms,
        };
        m.wallets.push(w.clone());
        Ok(w)
    }

    async fn set_gap_limit(&self, wallet_id: i64, gap_limit: u32) -> PayResult<()> {
        let mut m = self.m();
        let w = m
            .wallets
            .iter_mut()
            .find(|w| w.id == wallet_id)
            .ok_or(PayError::NotFound)?;
        w.gap_limit = gap_limit;
        Ok(())
    }

    async fn config(&self) -> PayResult<DonationConfig> {
        Ok(self.m().config.clone().unwrap_or_default())
    }

    async fn set_config(&self, cfg: &DonationConfig, _by: i64, _now_ms: i64) -> PayResult<()> {
        self.m().config = Some(cfg.clone());
        Ok(())
    }

    async fn create_invoice(
        &self,
        wallet_id: i64,
        new: NewInvoice,
        max_open: u32,
        derive: &(dyn Fn(u32) -> PayResult<String> + Send + Sync),
    ) -> PayResult<Invoice> {
        let mut m = self.m();
        let open = m
            .invoices
            .iter()
            .filter(|i| {
                i.user_id == new.user_id
                    && matches!(
                        i.state,
                        InvoiceState::Open
                            | InvoiceState::Seen
                            | InvoiceState::Confirming
                            | InvoiceState::Underpaid
                    )
            })
            .count();
        if open >= max_open as usize {
            return Err(PayError::TooManyOpen);
        }
        let w = m
            .wallets
            .iter()
            .find(|w| w.id == wallet_id)
            .cloned()
            .ok_or(PayError::NotFound)?;
        let used_max = m
            .invoices
            .iter()
            .filter(|i| i.wallet_id == wallet_id && m.txs.iter().any(|t| t.invoice_id == i.id))
            .map(|i| i.derivation_index)
            .max();
        let run = w.next_index - used_max.map_or(0, |u| u + 1);
        if run >= w.gap_limit {
            return Err(PayError::GapLimit);
        }
        let index = w.next_index;
        let address = derive(index)?;
        if let Some(w) = m.wallets.iter_mut().find(|w| w.id == wallet_id) {
            w.next_index += 1;
        }
        let inv = Invoice {
            id: new.id,
            wallet_id,
            user_id: new.user_id,
            purpose: new.purpose,
            guild_id: new.guild_id,
            months: new.months,
            credits: new.credits,
            usd_cents: new.usd_cents,
            sats: new.sats,
            rate_cents_per_btc: new.rate_cents_per_btc,
            rate_source: new.rate_source,
            rate_stale: new.rate_stale,
            address,
            derivation_index: index,
            network: w.network,
            state: InvoiceState::Open,
            confirmations_required: new.confirmations_required,
            confirmations: None,
            received_sats: 0,
            confirmed_sats: 0,
            shortfall_sats: new.sats,
            excess_sats: 0,
            late: false,
            anonymous: new.anonymous,
            created_ms: new.created_ms,
            expires_ms: new.expires_ms,
            quote_expires_ms: new.quote_expires_ms,
            credited_ms: None,
            next_check_ms: new.created_ms,
            resolution: None,
        };
        m.invoices.push(inv.clone());
        Ok(inv)
    }

    async fn invoice(&self, id: i64) -> PayResult<Option<Invoice>> {
        Ok(self.m().invoices.iter().find(|i| i.id == id).cloned())
    }

    async fn invoice_txs(&self, id: i64) -> PayResult<Vec<TxRecord>> {
        Ok(self
            .m()
            .txs
            .iter()
            .filter(|t| t.invoice_id == id)
            .cloned()
            .collect())
    }

    async fn invoice_audit(&self, id: i64) -> PayResult<Vec<AuditRow>> {
        Ok(self
            .m()
            .audit
            .iter()
            .filter(|(i, _)| *i == id)
            .map(|(_, a)| a.clone())
            .collect())
    }

    async fn user_invoices(
        &self,
        user_id: i64,
        before_id: Option<i64>,
        limit: u32,
    ) -> PayResult<Vec<Invoice>> {
        let mut v: Vec<Invoice> = self
            .m()
            .invoices
            .iter()
            .filter(|i| i.user_id == user_id && before_id.is_none_or(|b| i.id < b))
            .cloned()
            .collect();
        v.sort_by_key(|i| std::cmp::Reverse(i.id));
        v.truncate(limit as usize);
        Ok(v)
    }

    async fn search_invoices(&self, f: &InvoiceFilter) -> PayResult<Vec<Invoice>> {
        let m = self.m();
        let mut v: Vec<Invoice> = m
            .invoices
            .iter()
            .filter(|i| f.state.is_none_or(|s| i.state == s))
            .filter(|i| f.purpose.is_none_or(|p| i.purpose == p))
            .filter(|i| f.user_id.is_none_or(|u| i.user_id == u))
            .filter(|i| !f.needs_attention || i.state.needs_attention())
            .filter(|i| f.before_id.is_none_or(|b| i.id < b))
            .filter(|i| {
                f.q.as_deref().is_none_or(|q| {
                    i.id.to_string() == q
                        || i.address == q
                        || m.txs
                            .iter()
                            .any(|t| t.invoice_id == i.id && t.txid.starts_with(q))
                })
            })
            .cloned()
            .collect();
        v.sort_by_key(|i| std::cmp::Reverse(i.id));
        v.truncate(f.limit.max(1) as usize);
        Ok(v)
    }

    async fn due_invoices(&self, now_ms: i64, limit: u32) -> PayResult<Vec<Invoice>> {
        let cfg = self.config().await?;
        let mut v: Vec<Invoice> = self
            .m()
            .invoices
            .iter()
            .filter(|i| i.next_check_ms <= now_ms && !crate::invoice::is_final(i, &cfg, now_ms))
            .cloned()
            .collect();
        v.sort_by_key(|i| i.next_check_ms);
        v.truncate(limit as usize);
        Ok(v)
    }

    async fn record_observations(
        &self,
        invoice_id: i64,
        obs: &[ChainObs],
        now_ms: i64,
    ) -> PayResult<Vec<TxRecord>> {
        let mut m = self.m();
        for o in obs {
            if let Some(t) = m
                .txs
                .iter_mut()
                .find(|t| t.invoice_id == invoice_id && t.txid == o.txid)
            {
                t.amount_sats = o.amount_sats;
                t.block_height = o.block_height;
                t.confirmations = o.confirmations;
                t.last_seen_ms = now_ms;
                t.dropped_ms = None;
            } else {
                m.txs.push(TxRecord {
                    invoice_id,
                    txid: o.txid.clone(),
                    amount_sats: o.amount_sats,
                    block_height: o.block_height,
                    confirmations: o.confirmations,
                    first_seen_ms: now_ms,
                    last_seen_ms: now_ms,
                    dropped_ms: None,
                });
                push_audit(
                    &mut m,
                    invoice_id,
                    "tx_seen",
                    json!({ "txid": o.txid, "sats": o.amount_sats }),
                    now_ms,
                );
            }
        }
        let mut dropped = Vec::new();
        for t in m
            .txs
            .iter_mut()
            .filter(|t| t.invoice_id == invoice_id && t.dropped_ms.is_none())
        {
            if !obs.iter().any(|o| o.txid == t.txid) {
                t.dropped_ms = Some(now_ms);
                t.confirmations = 0;
                t.block_height = None;
                dropped.push(t.txid.clone());
            }
        }
        for txid in dropped {
            push_audit(
                &mut m,
                invoice_id,
                "tx_dropped",
                json!({ "txid": txid }),
                now_ms,
            );
        }
        Ok(m.txs
            .iter()
            .filter(|t| t.invoice_id == invoice_id)
            .cloned()
            .collect())
    }

    async fn save_evaluation(
        &self,
        invoice_id: i64,
        eval: &Evaluation,
        plan: &CreditPlan,
        next_check_ms: i64,
        now_ms: i64,
    ) -> PayResult<SaveOutcome> {
        let mut m = self.m();
        let inv = invoice_mut(&mut m, invoice_id)?.clone();
        if inv.state.is_closed() {
            return Ok(SaveOutcome {
                old_state: inv.state,
                invoice: inv,
                credit: CreditChange::None,
                changed: false,
            });
        }
        let active = m.credits.get(&invoice_id).copied().unwrap_or(false);
        let mut credit = CreditChange::None;
        if eval.credited() && !active {
            apply_plan(&mut m, invoice_id, plan, now_ms);
            m.credits.insert(invoice_id, true);
            credit = CreditChange::Applied;
            push_audit(
                &mut m,
                invoice_id,
                "credited",
                json!({ "state": eval.state.as_str() }),
                now_ms,
            );
        } else if !eval.credited() && active {
            revert_plan(&mut m, invoice_id, plan, now_ms);
            m.credits.insert(invoice_id, false);
            credit = CreditChange::Reverted;
            push_audit(
                &mut m,
                invoice_id,
                "reverted",
                json!({ "state": eval.state.as_str() }),
                now_ms,
            );
        }
        if inv.state != eval.state {
            push_audit(
                &mut m,
                invoice_id,
                "state",
                json!({ "from": inv.state.as_str(), "to": eval.state.as_str() }),
                now_ms,
            );
        }
        let i = invoice_mut(&mut m, invoice_id)?;
        let changed = i.state != eval.state
            || i.confirmations != eval.confirmations
            || i.received_sats != eval.received_sats as i64
            || credit != CreditChange::None;
        i.state = eval.state;
        i.received_sats = eval.received_sats as i64;
        i.confirmed_sats = eval.confirmed_sats as i64;
        i.confirmations = eval.confirmations;
        i.late = eval.late;
        i.shortfall_sats = eval.shortfall_sats as i64;
        i.excess_sats = eval.excess_sats as i64;
        i.next_check_ms = next_check_ms;
        match credit {
            CreditChange::Applied => i.credited_ms = Some(now_ms),
            CreditChange::Reverted => i.credited_ms = None,
            CreditChange::None => {}
        }
        Ok(SaveOutcome {
            old_state: inv.state,
            invoice: i.clone(),
            credit,
            changed,
        })
    }

    async fn resolve(
        &self,
        invoice_id: i64,
        kind: ResolutionKind,
        note: &str,
        by: i64,
        plan: &CreditPlan,
        now_ms: i64,
    ) -> PayResult<SaveOutcome> {
        let mut m = self.m();
        let inv = invoice_mut(&mut m, invoice_id)?.clone();
        if inv.state.is_closed() {
            return Err(PayError::Conflict("the invoice is already closed".into()));
        }
        let active = m.credits.get(&invoice_id).copied().unwrap_or(false);
        let mut credit = CreditChange::None;
        match kind {
            ResolutionKind::MarkPaid if !active => {
                apply_plan(&mut m, invoice_id, plan, now_ms);
                m.credits.insert(invoice_id, true);
                credit = CreditChange::Applied;
            }
            ResolutionKind::RefundNoted | ResolutionKind::Dismiss if active => {
                // The plan of the original credit is needed to reverse the ledger.
                let undo = CreditPlan {
                    grants: vec![],
                    invite_credits: m
                        .ledger
                        .iter()
                        .filter(|(_, d, i)| *i == invoice_id && *d > 0)
                        .map(|(_, d, _)| *d)
                        .sum(),
                    invite_user: m
                        .ledger
                        .iter()
                        .find(|(_, _, i)| *i == invoice_id)
                        .map(|(u, _, _)| *u),
                };
                revert_plan(&mut m, invoice_id, &undo, now_ms);
                m.credits.insert(invoice_id, false);
                credit = CreditChange::Reverted;
            }
            _ => {}
        }
        push_audit(
            &mut m,
            invoice_id,
            "resolved",
            json!({ "kind": kind.as_str(), "by": by.to_string(), "note": note }),
            now_ms,
        );
        let i = invoice_mut(&mut m, invoice_id)?;
        i.state = InvoiceState::Resolved;
        i.resolution = Some(Resolution {
            kind,
            note: note.to_owned(),
            by,
            at_ms: now_ms,
        });
        if credit == CreditChange::Applied {
            i.credited_ms = Some(now_ms);
        }
        Ok(SaveOutcome {
            old_state: inv.state,
            invoice: i.clone(),
            credit,
            changed: true,
        })
    }

    async fn cancel(&self, invoice_id: i64, user_id: i64, now_ms: i64) -> PayResult<Invoice> {
        let mut m = self.m();
        let has_tx = m
            .txs
            .iter()
            .any(|t| t.invoice_id == invoice_id && t.dropped_ms.is_none());
        let inv = invoice_mut(&mut m, invoice_id)?;
        if inv.user_id != user_id {
            return Err(PayError::NotFound);
        }
        if inv.state != InvoiceState::Open || has_tx {
            return Err(PayError::Conflict(
                "only an unpaid open invoice can be cancelled".into(),
            ));
        }
        inv.state = InvoiceState::Cancelled;
        let out = inv.clone();
        push_audit(&mut m, invoice_id, "cancelled", json!({}), now_ms);
        Ok(out)
    }

    async fn schedule_check(&self, invoice_id: i64, next_check_ms: i64) -> PayResult<()> {
        invoice_mut(&mut self.m(), invoice_id)?.next_check_ms = next_check_ms;
        Ok(())
    }

    async fn audit(
        &self,
        invoice_id: i64,
        kind: &str,
        detail: Value,
        now_ms: i64,
    ) -> PayResult<()> {
        push_audit(&mut self.m(), invoice_id, kind, detail, now_ms);
        Ok(())
    }

    async fn user_patron(
        &self,
        user_id: i64,
        now_ms: i64,
        grace_days: u32,
    ) -> PayResult<PatronStatus> {
        let m = self.m();
        let g: Vec<Grant> = m
            .grants
            .iter()
            .filter(|g| g.kind == GrantKind::Patron && g.user_id == Some(user_id))
            .cloned()
            .collect();
        Ok(patron_status(stack_patron(&g), now_ms, grace_days))
    }

    async fn guild_entitlement(&self, guild_id: i64, now_ms: i64) -> PayResult<GuildEntitlement> {
        let cfg = self.config().await?;
        let m = self.m();
        let g: Vec<Grant> = m
            .grants
            .iter()
            .filter(|g| g.guild_id == Some(guild_id))
            .cloned()
            .collect();
        let (level, below) = m.levels.get(&guild_id).copied().unwrap_or((0, None));
        Ok(GuildEntitlement {
            guild_id,
            level,
            pool_cents: pool_cents(&g, now_ms, cfg.pool_window_days),
            grace_until_ms: below
                .map(|b| b + i64::from(cfg.grace_days) * crate::entitlements::DAY_MS),
        })
    }

    async fn grant(
        &self,
        spec: &GrantSpec,
        by: i64,
        permanent: bool,
        ends_ms: Option<i64>,
        reason: &str,
        now_ms: i64,
    ) -> PayResult<Grant> {
        let mut m = self.m();
        m.next_grant += 1;
        let g = Grant {
            id: m.next_grant,
            kind: spec.kind,
            user_id: spec.user_id,
            guild_id: spec.guild_id,
            source: GrantSource::Admin,
            source_id: Some(by),
            days: spec.days,
            ends_ms,
            permanent,
            usd_cents: spec.usd_cents,
            starts_ms: now_ms,
            revoked_ms: None,
            reason: Some(reason.to_owned()),
        };
        m.grants.push(g.clone());
        Ok(g)
    }

    async fn revoke_grant(&self, id: i64, _by: i64, reason: &str, now_ms: i64) -> PayResult<Grant> {
        let mut m = self.m();
        let g = m
            .grants
            .iter_mut()
            .find(|g| g.id == id)
            .ok_or(PayError::NotFound)?;
        if g.revoked_ms.is_some() {
            return Err(PayError::Conflict("already revoked".into()));
        }
        g.revoked_ms = Some(now_ms);
        g.reason = Some(reason.to_owned());
        Ok(g.clone())
    }

    async fn list_grants(&self, f: &GrantFilter) -> PayResult<Vec<Grant>> {
        let mut v: Vec<Grant> = self
            .m()
            .grants
            .iter()
            .filter(|g| f.kind.is_none_or(|k| g.kind == k))
            .filter(|g| f.user_id.is_none_or(|u| g.user_id == Some(u)))
            .filter(|g| f.guild_id.is_none_or(|u| g.guild_id == Some(u)))
            .filter(|g| !f.active_only || g.revoked_ms.is_none())
            .filter(|g| f.before_id.is_none_or(|b| g.id < b))
            .cloned()
            .collect();
        v.sort_by_key(|g| std::cmp::Reverse(g.id));
        v.truncate(f.limit.max(1) as usize);
        Ok(v)
    }

    async fn refresh_guild(
        &self,
        guild_id: i64,
        cfg: &DonationConfig,
        now_ms: i64,
    ) -> PayResult<Option<LevelChange>> {
        let mut m = self.m();
        let g: Vec<Grant> = m
            .grants
            .iter()
            .filter(|g| g.guild_id == Some(guild_id))
            .cloned()
            .collect();
        let pool = pool_cents(&g, now_ms, cfg.pool_window_days);
        let (cur, below) = m.levels.get(&guild_id).copied().unwrap_or((0, None));
        let (level, below) = crate::entitlements::next_level(
            cur,
            below,
            pool,
            cfg.level_thresholds_cents,
            cfg.grace_days,
            now_ms,
        );
        m.levels.insert(guild_id, (level, below));
        Ok((level != cur).then_some(LevelChange {
            guild_id,
            old: cur,
            new: level,
        }))
    }

    async fn guilds_to_refresh(&self, _now_ms: i64) -> PayResult<Vec<i64>> {
        let m = self.m();
        let mut s: HashSet<i64> = m
            .levels
            .iter()
            .filter(|(_, (l, _))| *l > 0)
            .map(|(g, _)| *g)
            .collect();
        s.extend(
            m.grants
                .iter()
                .filter(|g| g.kind == GrantKind::ServerPool && g.revoked_ms.is_none())
                .filter_map(|g| g.guild_id),
        );
        Ok(s.into_iter().collect())
    }

    async fn patron_transitions(
        &self,
        now_ms: i64,
        grace_days: u32,
    ) -> PayResult<Vec<PatronTransition>> {
        let mut m = self.m();
        let users: HashSet<i64> = m
            .grants
            .iter()
            .filter(|g| g.kind == GrantKind::Patron)
            .filter_map(|g| g.user_id)
            .collect();
        let mut out = Vec::new();
        for u in users {
            let g: Vec<Grant> = m
                .grants
                .iter()
                .filter(|g| g.kind == GrantKind::Patron && g.user_id == Some(u))
                .cloned()
                .collect();
            let want = notice_level(stack_patron(&g), now_ms, grace_days);
            let have = m.notices.get(&u).copied().unwrap_or(0);
            if want > have {
                out.push(PatronTransition {
                    user_id: u,
                    kind: match want {
                        1 => PatronTransitionKind::Reminder,
                        2 => PatronTransitionKind::EnteredGrace,
                        _ => PatronTransitionKind::Lapsed,
                    },
                });
            }
            if want != have {
                m.notices.insert(u, want);
            }
        }
        Ok(out)
    }

    async fn guild_access(&self, user_id: i64, guild_id: i64) -> PayResult<GuildAccess> {
        Ok(self
            .m()
            .members
            .get(&(user_id, guild_id))
            .copied()
            .unwrap_or(GuildAccess {
                member: false,
                owner: false,
            }))
    }

    async fn donor_prefs(&self, user_id: i64) -> PayResult<DonorPrefs> {
        Ok(self.m().prefs.get(&user_id).copied().unwrap_or_default())
    }

    async fn set_donor_prefs(&self, user_id: i64, prefs: DonorPrefs) -> PayResult<()> {
        self.m().prefs.insert(user_id, prefs);
        Ok(())
    }

    async fn guild_hidden(&self, guild_id: i64) -> PayResult<bool> {
        Ok(self.m().hidden_guilds.contains(&guild_id))
    }

    async fn set_guild_hidden(&self, guild_id: i64, hidden: bool) -> PayResult<()> {
        let mut m = self.m();
        if hidden {
            m.hidden_guilds.insert(guild_id);
        } else {
            m.hidden_guilds.remove(&guild_id);
        }
        Ok(())
    }

    async fn leaderboard_donors(
        &self,
        since_ms: Option<i64>,
        limit: u32,
    ) -> PayResult<Vec<LeaderRow>> {
        let m = self.m();
        let mut totals: HashMap<i64, i64> = HashMap::new();
        for i in m.invoices.iter().filter(|i| {
            m.credits.get(&i.id).copied().unwrap_or(false)
                && !m.staff_hidden_users.contains(&i.user_id)
                && i.purpose != Purpose::InviteCredit
                && since_ms.is_none_or(|s| i.credited_ms.is_some_and(|c| c >= s))
        }) {
            *totals.entry(i.user_id).or_default() += i.usd_cents;
        }
        let mut rows: Vec<LeaderRow> = totals
            .into_iter()
            .map(|(u, cents)| {
                let p = m.prefs.get(&u).copied().unwrap_or_default();
                LeaderRow {
                    user_id: Some(u),
                    guild_id: None,
                    cents,
                    hide_name: p.hide_name,
                    hide_amount: p.hide_amount,
                }
            })
            .collect();
        rows.sort_by_key(|r| (std::cmp::Reverse(r.cents), r.user_id));
        rows.truncate(limit as usize);
        Ok(rows)
    }

    async fn leaderboard_servers(
        &self,
        since_ms: Option<i64>,
        limit: u32,
    ) -> PayResult<Vec<LeaderRow>> {
        let m = self.m();
        let mut totals: HashMap<i64, i64> = HashMap::new();
        for g in m.grants.iter().filter(|g| {
            g.kind == GrantKind::ServerPool
                && g.revoked_ms.is_none()
                && since_ms.is_none_or(|s| g.starts_ms >= s)
        }) {
            if let Some(gid) = g.guild_id.filter(|gid| {
                !m.hidden_guilds.contains(gid) && !m.staff_hidden_guilds.contains(gid)
            }) {
                *totals.entry(gid).or_default() += g.usd_cents.unwrap_or(0);
            }
        }
        let mut rows: Vec<LeaderRow> = totals
            .into_iter()
            .map(|(gid, cents)| LeaderRow {
                user_id: None,
                guild_id: Some(gid),
                cents,
                hide_name: false,
                hide_amount: false,
            })
            .collect();
        rows.sort_by_key(|r| (std::cmp::Reverse(r.cents), r.guild_id));
        rows.truncate(limit as usize);
        Ok(rows)
    }

    async fn donor_rank(
        &self,
        user_id: i64,
        since_ms: Option<i64>,
    ) -> PayResult<Option<(u32, i64)>> {
        let all = self.leaderboard_donors(since_ms, u32::MAX).await?;
        Ok(all
            .iter()
            .position(|r| r.user_id == Some(user_id))
            .map(|p| (p as u32 + 1, all[p].cents)))
    }

    async fn top_contributors(
        &self,
        guild_id: i64,
        since_ms: i64,
        limit: u32,
    ) -> PayResult<Vec<LeaderRow>> {
        let m = self.m();
        let mut totals: HashMap<i64, (i64, bool)> = HashMap::new();
        for g in m.grants.iter().filter(|g| {
            g.kind == GrantKind::ServerPool
                && g.guild_id == Some(guild_id)
                && g.revoked_ms.is_none()
                && g.starts_ms >= since_ms
        }) {
            let anon = g
                .source_id
                .and_then(|id| m.invoices.iter().find(|i| i.id == id))
                .is_some_and(|i| i.anonymous);
            let e = totals.entry(g.user_id.unwrap_or(0)).or_insert((0, true));
            e.0 += g.usd_cents.unwrap_or(0);
            e.1 &= anon;
        }
        let mut rows: Vec<LeaderRow> = totals
            .into_iter()
            .map(|(u, (cents, anon))| {
                let p = m.prefs.get(&u).copied().unwrap_or_default();
                LeaderRow {
                    user_id: Some(u),
                    guild_id: Some(guild_id),
                    cents,
                    hide_name: anon || p.hide_name,
                    hide_amount: p.hide_amount,
                }
            })
            .collect();
        rows.sort_by_key(|r| (std::cmp::Reverse(r.cents), r.user_id));
        rows.truncate(limit as usize);
        Ok(rows)
    }

    async fn profiles(&self, ids: &[i64]) -> PayResult<Vec<Profile>> {
        let m = self.m();
        Ok(ids
            .iter()
            .filter_map(|i| m.profiles.get(i).cloned())
            .collect())
    }

    async fn guild_profiles(&self, ids: &[i64]) -> PayResult<Vec<Profile>> {
        self.profiles(ids).await
    }

    async fn set_staff_hidden(
        &self,
        user_id: Option<i64>,
        guild_id: Option<i64>,
        hidden: bool,
    ) -> PayResult<()> {
        let mut m = self.m();
        if let Some(u) = user_id {
            if hidden {
                m.staff_hidden_users.insert(u);
            } else {
                m.staff_hidden_users.remove(&u);
            }
        }
        if let Some(g) = guild_id {
            if hidden {
                m.staff_hidden_guilds.insert(g);
            } else {
                m.staff_hidden_guilds.remove(&g);
            }
        }
        Ok(())
    }

    async fn donated_since(&self, since_ms: i64) -> PayResult<i64> {
        let m = self.m();
        Ok(m.invoices
            .iter()
            .filter(|i| {
                m.credits.get(&i.id).copied().unwrap_or(false)
                    && i.credited_ms.is_some_and(|c| c >= since_ms)
            })
            .map(|i| i.usd_cents)
            .sum())
    }

    async fn report(&self, from_ms: i64, to_ms: i64) -> PayResult<Report> {
        let m = self.m();
        let mut r = Report::default();
        let mut by_p: HashMap<String, (i64, i64)> = HashMap::new();
        let mut by_state: HashMap<InvoiceState, i64> = HashMap::new();
        for i in m
            .invoices
            .iter()
            .filter(|i| i.created_ms >= from_ms && i.created_ms < to_ms)
        {
            *by_state.entry(i.state).or_default() += 1;
            if m.credits.get(&i.id).copied().unwrap_or(false) {
                let e = by_p.entry(i.purpose.as_str().to_owned()).or_default();
                e.0 += i.usd_cents;
                e.1 += 1;
                r.credited_cents += i.usd_cents;
            }
        }
        r.by_purpose = by_p.into_iter().map(|(p, (c, n))| (p, c, n)).collect();
        r.by_purpose.sort();
        r.by_state = by_state.into_iter().collect();
        r.by_state.sort_by_key(|(s, _)| s.as_str());
        Ok(r)
    }

    async fn watcher_status(&self) -> PayResult<WatcherStatus> {
        Ok(self.m().watcher.clone())
    }

    async fn set_watcher_status(&self, s: &WatcherStatus) -> PayResult<()> {
        self.m().watcher = s.clone();
        Ok(())
    }
}
