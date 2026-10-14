//! `PaymentService`: creating invoices, watching them, crediting, and the entitlement events.

use crate::{
    chain::ChainSource,
    config::DonationConfig,
    descriptor::{Network, WatchDescriptor, bip21},
    entitlements::{EntitlementService, Entitlements, GrantKind},
    error::{PayError, PayResult},
    invoice::{Evaluation, Invoice, Observed, Purpose, ResolutionKind, evaluate, next_check_ms},
    rates::{RateService, sats_for_cents},
    repo::{
        CreditChange, CreditPlan, GrantSpec, LevelChange, NewInvoice, PatronTransitionKind,
        PaymentRepo, SaveOutcome, Wallet,
    },
};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;
pub type IdSource = Arc<dyn Fn() -> i64 + Send + Sync>;

/// Where the service reports changes (gateway dispatches and staff alerts).
#[async_trait]
pub trait PaymentEvents: Send + Sync {
    /// `SR_PAYMENT_UPDATE` to the payer's sessions.
    async fn payment_update(&self, user_id: i64, payload: Value);
    /// `SR_ENTITLEMENT_UPDATE` to one user's sessions.
    async fn entitlement_user(&self, user_id: i64, payload: Value);
    /// `SR_ENTITLEMENT_UPDATE` to the members of a guild.
    async fn entitlement_guild(&self, guild_id: i64, payload: Value);
    /// A staff alert (never contains the descriptor or addresses of other people).
    async fn staff_alert(&self, detail: String);
}

pub struct NoEvents;

#[async_trait]
impl PaymentEvents for NoEvents {
    async fn payment_update(&self, _: i64, _: Value) {}
    async fn entitlement_user(&self, _: i64, _: Value) {}
    async fn entitlement_guild(&self, _: i64, _: Value) {}
    async fn staff_alert(&self, _: String) {}
}

/// Collects events (tests).
#[derive(Default)]
pub struct RecordingEvents {
    pub payment: Mutex<Vec<(i64, Value)>>,
    pub user: Mutex<Vec<(i64, Value)>>,
    pub guild: Mutex<Vec<(i64, Value)>>,
    pub alerts: Mutex<Vec<String>>,
}

#[async_trait]
impl PaymentEvents for RecordingEvents {
    async fn payment_update(&self, u: i64, p: Value) {
        self.payment
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((u, p));
    }
    async fn entitlement_user(&self, u: i64, p: Value) {
        self.user
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((u, p));
    }
    async fn entitlement_guild(&self, g: i64, p: Value) {
        self.guild
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((g, p));
    }
    async fn staff_alert(&self, d: String) {
        self.alerts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(d);
    }
}

/// What a member asks for.
#[derive(Debug, Clone)]
pub struct InvoiceRequest {
    pub purpose: Purpose,
    pub usd_cents: Option<u64>,
    pub months: Option<u32>,
    pub guild_id: Option<i64>,
    pub credits: Option<u32>,
    pub anonymous: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PollSummary {
    pub polled: u32,
    pub changed: u32,
    pub errors: u32,
    pub tip: Option<u32>,
}

pub struct PaymentService {
    repo: Arc<dyn PaymentRepo>,
    chain: Arc<dyn ChainSource>,
    rates: Arc<RateService>,
    events: Arc<dyn PaymentEvents>,
    ents: Arc<EntitlementService>,
    clock: Clock,
    ids: IdSource,
    checks: Mutex<HashMap<i64, i64>>,
}

/// The credit plan of an invoice under the current configuration.
pub fn credit_plan(inv: &Invoice, cfg: &DonationConfig) -> CreditPlan {
    match inv.purpose {
        Purpose::Donate => CreditPlan::default(),
        Purpose::Patron => CreditPlan {
            grants: vec![GrantSpec {
                kind: GrantKind::Patron,
                user_id: Some(inv.user_id),
                guild_id: None,
                days: Some(
                    i64::from(inv.months.unwrap_or(1)) * i64::from(cfg.patron_days_per_month),
                ),
                usd_cents: None,
            }],
            ..CreditPlan::default()
        },
        Purpose::ServerLevel => CreditPlan {
            grants: vec![GrantSpec {
                kind: GrantKind::ServerPool,
                user_id: Some(inv.user_id),
                guild_id: inv.guild_id,
                days: None,
                usd_cents: Some(inv.usd_cents),
            }],
            ..CreditPlan::default()
        },
        Purpose::InviteCredit => CreditPlan {
            grants: Vec::new(),
            invite_credits: i64::from(inv.credits.unwrap_or(1)),
            invite_user: Some(inv.user_id),
        },
    }
}

impl PaymentService {
    pub fn new(
        repo: Arc<dyn PaymentRepo>,
        chain: Arc<dyn ChainSource>,
        rates: Arc<RateService>,
        events: Arc<dyn PaymentEvents>,
        ents: Arc<EntitlementService>,
        clock: Clock,
        ids: IdSource,
    ) -> Self {
        Self {
            repo,
            chain,
            rates,
            events,
            ents,
            clock,
            ids,
            checks: Mutex::new(HashMap::new()),
        }
    }

    pub fn repo(&self) -> &Arc<dyn PaymentRepo> {
        &self.repo
    }

    pub fn entitlements(&self) -> &Arc<EntitlementService> {
        &self.ents
    }

    pub fn rates(&self) -> &Arc<RateService> {
        &self.rates
    }

    pub fn chain(&self) -> &Arc<dyn ChainSource> {
        &self.chain
    }

    fn now(&self) -> i64 {
        (self.clock)()
    }

    // ---- wallet ----------------------------------------------------------------------------

    /// Validates a watch-only descriptor and makes it the active wallet.
    pub async fn set_wallet(
        &self,
        descriptor: &str,
        network: Network,
        gap_limit: u32,
        by: i64,
    ) -> PayResult<(Wallet, Vec<String>)> {
        if !(1..=1000).contains(&gap_limit) {
            return Err(PayError::Invalid(
                "gap_limit must be between 1 and 1000".into(),
            ));
        }
        let d = WatchDescriptor::parse(descriptor, network)?;
        let addrs = (0..3)
            .map(|i| d.address(i))
            .collect::<Result<Vec<_>, _>>()?;
        let w = self
            .repo
            .set_wallet(
                network.as_str(),
                d.body(),
                &d.fingerprint(),
                gap_limit,
                Some(by),
                self.now(),
            )
            .await?;
        Ok((w, addrs))
    }

    /// First `n` receive addresses of a descriptor, without saving anything.
    pub fn test_derivation(descriptor: &str, network: Network, n: u32) -> PayResult<Vec<String>> {
        let d = WatchDescriptor::parse(descriptor, network)?;
        Ok((0..n.min(20))
            .map(|i| d.address(i))
            .collect::<Result<Vec<_>, _>>()?)
    }

    // ---- invoices --------------------------------------------------------------------------

    pub async fn create_invoice(&self, user_id: i64, req: InvoiceRequest) -> PayResult<Invoice> {
        let cfg = self.ents.config().await?;
        let wallet = self.repo.active_wallet().await?.ok_or(PayError::Disabled)?;
        if !cfg.enabled {
            return Err(PayError::Disabled);
        }
        let usd_cents: u64 = match req.purpose {
            Purpose::Patron => {
                let months = req
                    .months
                    .ok_or_else(|| PayError::Invalid("months is required".into()))?;
                if months == 0 || months > cfg.patron_max_months {
                    return Err(PayError::Invalid(format!(
                        "months must be between 1 and {}",
                        cfg.patron_max_months
                    )));
                }
                u64::from(months) * u64::from(cfg.patron_price_cents)
            }
            Purpose::InviteCredit => {
                let n = req
                    .credits
                    .ok_or_else(|| PayError::Invalid("credits is required".into()))?;
                if n == 0 || n > cfg.max_invite_credits {
                    return Err(PayError::Invalid(format!(
                        "credits must be between 1 and {}",
                        cfg.max_invite_credits
                    )));
                }
                u64::from(n) * u64::from(cfg.invite_credit_price_cents)
            }
            Purpose::Donate | Purpose::ServerLevel => {
                let c = req
                    .usd_cents
                    .ok_or_else(|| PayError::Invalid("usd_cents is required".into()))?;
                if c < u64::from(cfg.min_donation_cents) || c > u64::from(cfg.max_donation_cents) {
                    return Err(PayError::Invalid(format!(
                        "the amount must be between {} and {} cents",
                        cfg.min_donation_cents, cfg.max_donation_cents
                    )));
                }
                c
            }
        };
        if req.purpose == Purpose::ServerLevel {
            let g = req
                .guild_id
                .ok_or_else(|| PayError::Invalid("guild_id is required".into()))?;
            if !self.repo.guild_access(user_id, g).await?.member {
                return Err(PayError::NotFound);
            }
        } else if req.guild_id.is_some() {
            return Err(PayError::Invalid(
                "guild_id only applies to server_level".into(),
            ));
        }
        let now = self.now();
        let quote = self.rates.quote(now).await?;
        let sats = sats_for_cents(usd_cents, quote.cents_per_btc);
        if sats < cfg.min_invoice_sats {
            return Err(PayError::Invalid(
                "the amount is too small to send on-chain".into(),
            ));
        }
        let network = Network::parse(&wallet.network)
            .ok_or_else(|| PayError::Storage("bad wallet network".into()))?;
        let desc = WatchDescriptor::parse(&wallet.descriptor, network)?;
        let new = NewInvoice {
            id: (self.ids)(),
            user_id,
            purpose: req.purpose,
            guild_id: req.guild_id,
            months: (req.purpose == Purpose::Patron)
                .then_some(req.months)
                .flatten(),
            credits: (req.purpose == Purpose::InviteCredit)
                .then_some(req.credits)
                .flatten(),
            usd_cents: i64::try_from(usd_cents).unwrap_or(i64::MAX),
            sats: i64::try_from(sats).unwrap_or(i64::MAX),
            rate_cents_per_btc: i64::try_from(quote.cents_per_btc).unwrap_or(i64::MAX),
            rate_source: quote.source.clone(),
            rate_stale: quote.stale,
            confirmations_required: cfg.confirmations,
            anonymous: req.anonymous,
            created_ms: now,
            expires_ms: now + i64::from(cfg.invoice_expiry_minutes) * 60_000,
            quote_expires_ms: now + i64::from(cfg.rate_lock_minutes) * 60_000,
        };
        let inv = self
            .repo
            .create_invoice(wallet.id, new, cfg.max_open_invoices, &|i| {
                desc.address(i).map_err(PayError::from)
            })
            .await?;
        self.repo
            .audit(
                inv.id,
                "created",
                json!({ "purpose": inv.purpose.as_str(), "usd_cents": inv.usd_cents, "sats": inv.sats,
                        "rate": inv.rate_cents_per_btc, "rate_source": inv.rate_source, "stale": inv.rate_stale,
                        "index": inv.derivation_index }),
                now,
            )
            .await?;
        self.events
            .payment_update(user_id, payment_payload(&inv))
            .await;
        Ok(inv)
    }

    /// The BIP-21 URI of an invoice.
    pub fn bip21_uri(inv: &Invoice) -> String {
        bip21(
            &inv.address,
            u64::try_from(inv.sats).unwrap_or(0),
            Some("SIDE ROOM"),
        )
    }

    /// An invoice of `user_id` (other people's invoices do not exist).
    pub async fn own_invoice(&self, user_id: i64, id: i64) -> PayResult<Invoice> {
        match self.repo.invoice(id).await? {
            Some(i) if i.user_id == user_id => Ok(i),
            _ => Err(PayError::NotFound),
        }
    }

    /// The "I've paid" button: polls the invoice now, at most once per 10 seconds.
    pub async fn check_now(&self, user_id: i64, id: i64) -> PayResult<Invoice> {
        let inv = self.own_invoice(user_id, id).await?;
        let now = self.now();
        {
            let mut m = self.checks.lock().unwrap_or_else(|e| e.into_inner());
            if m.get(&id).is_some_and(|t| now - *t < 10_000) {
                return Ok(inv);
            }
            m.insert(id, now);
            if m.len() > 10_000 {
                m.retain(|_, t| now - *t < 60_000);
            }
        }
        if inv.state.is_closed() {
            return Ok(inv);
        }
        match self.poll_invoice(&inv).await {
            Ok(o) => Ok(o.invoice),
            Err(PayError::Chain(_)) => Ok(inv),
            Err(e) => Err(e),
        }
    }

    pub async fn cancel(&self, user_id: i64, id: i64) -> PayResult<Invoice> {
        let inv = self.repo.cancel(id, user_id, self.now()).await?;
        self.events
            .payment_update(user_id, payment_payload(&inv))
            .await;
        Ok(inv)
    }

    // ---- the watcher -----------------------------------------------------------------------

    /// Polls every due invoice once.
    pub async fn poll_due(&self, limit: u32) -> PayResult<PollSummary> {
        use futures::StreamExt;
        let now = self.now();
        let due = self.repo.due_invoices(now, limit).await?;
        let mut sum = PollSummary::default();
        if due.is_empty() {
            return Ok(sum);
        }
        let results: Vec<PayResult<SaveOutcome>> = futures::stream::iter(due.iter())
            .map(|inv| self.poll_invoice(inv))
            .buffer_unordered(4)
            .collect()
            .await;
        for r in results {
            sum.polled += 1;
            match r {
                Ok(o) => sum.changed += u32::from(o.changed),
                Err(_) => sum.errors += 1,
            }
        }
        sum.tip = self.chain.tip_height().await.ok();
        Ok(sum)
    }

    /// Reads the chain for one invoice, evaluates, and stores the result (crediting or
    /// reverting exactly once).
    pub async fn poll_invoice(&self, inv: &Invoice) -> PayResult<SaveOutcome> {
        let cfg = self.ents.config().await?;
        let now = self.now();
        let (tip, txs) = match async {
            let tip = self.chain.tip_height().await?;
            let txs = self.chain.address_txs(&inv.address).await?;
            Ok::<_, PayError>((tip, txs))
        }
        .await
        {
            Ok(v) => v,
            Err(e) => {
                // Try again later; the invoice itself is untouched.
                let retry = now + i64::from(cfg.poll_secs) * 2000;
                self.repo.schedule_check(inv.id, retry).await?;
                return Err(e);
            }
        };
        let obs: Vec<_> = txs
            .iter()
            .map(|t| crate::repo::ChainObs {
                txid: t.txid.clone(),
                amount_sats: i64::try_from(t.amount_sats).unwrap_or(i64::MAX),
                block_height: t.block_height,
                confirmations: t.confirmations(tip),
            })
            .collect();
        let recs = self.repo.record_observations(inv.id, &obs, now).await?;
        let live: Vec<Observed> = recs
            .iter()
            .filter(|r| r.dropped_ms.is_none())
            .map(|r| Observed {
                amount_sats: u64::try_from(r.amount_sats).unwrap_or(0),
                confirmations: r.confirmations,
                first_seen_ms: r.first_seen_ms,
            })
            .collect();
        let eval: Evaluation = evaluate(
            u64::try_from(inv.sats).unwrap_or(0),
            inv.expires_ms,
            inv.confirmations_required,
            cfg.tolerance_bps,
            &live,
            now,
        );
        let plan = credit_plan(inv, &cfg);
        let next = next_check_ms(eval.state, now, inv.expires_ms, &cfg);
        let out = self
            .repo
            .save_evaluation(inv.id, &eval, &plan, next, now)
            .await?;
        self.after(&out).await;
        Ok(out)
    }

    /// Events and cache invalidation after a stored change.
    async fn after(&self, out: &SaveOutcome) {
        let inv = &out.invoice;
        if out.changed {
            self.events
                .payment_update(inv.user_id, payment_payload(inv))
                .await;
        }
        if inv.state != out.old_state && inv.state.needs_attention() {
            self.events
                .staff_alert(format!(
                    "Invoice {} is {} ({} sats expected, {} received)",
                    inv.id,
                    inv.state.as_str(),
                    inv.sats,
                    inv.received_sats
                ))
                .await;
        }
        if out.credit != CreditChange::None {
            let word = if out.credit == CreditChange::Applied {
                "credited"
            } else {
                "reverted"
            };
            tracing::info!(invoice = inv.id, "payment {word}");
            match inv.purpose {
                Purpose::Patron | Purpose::InviteCredit => {
                    self.notify_user(inv.user_id, word).await
                }
                Purpose::ServerLevel => {
                    if let Some(g) = inv.guild_id {
                        self.refresh_guild(g).await;
                    }
                    self.notify_user(inv.user_id, word).await;
                }
                Purpose::Donate => {}
            }
        }
    }

    /// Emits `SR_ENTITLEMENT_UPDATE` for a user after dropping the cache entry.
    pub async fn notify_user(&self, user_id: i64, reason: &str) {
        self.ents.invalidate_user(user_id).await;
        let payload = match (
            self.ents.user(user_id).await,
            self.ents.limits(user_id, None).await,
        ) {
            (Ok(u), Ok(limits)) => json!({
                "type": "user",
                "user_id": user_id.to_string(),
                "reason": reason,
                "patron": {
                    "active": u.patron.active,
                    "in_grace": u.patron.in_grace,
                    "permanent": u.patron.permanent,
                    "until_ms": u.patron.until_ms,
                },
                "limits": limits,
            }),
            _ => json!({ "type": "user", "user_id": user_id.to_string(), "reason": reason }),
        };
        self.events.entitlement_user(user_id, payload).await;
    }

    /// Recomputes a guild's level and announces a change.
    pub async fn refresh_guild(&self, guild_id: i64) {
        let cfg = match self.ents.config().await {
            Ok(c) => c,
            Err(_) => return,
        };
        match self.repo.refresh_guild(guild_id, &cfg, self.now()).await {
            Ok(change) => {
                self.ents.invalidate_guild(guild_id).await;
                let ent = self.ents.guild(guild_id).await.ok();
                let payload = json!({
                    "type": "guild",
                    "guild_id": guild_id.to_string(),
                    "level": ent.as_ref().map(|e| e.level),
                    "pool_cents": ent.as_ref().map(|e| e.pool_cents),
                    "changed": change.is_some(),
                });
                self.events.entitlement_guild(guild_id, payload).await;
            }
            Err(e) => tracing::warn!(error = %e, guild = guild_id, "guild level refresh failed"),
        }
    }

    /// Periodic work: guild levels with grace, Patron grace/lapse/reminder events.
    pub async fn housekeeping(&self) -> PayResult<Vec<LevelChange>> {
        let cfg = self.ents.config().await?;
        let now = self.now();
        let mut changes = Vec::new();
        for g in self.repo.guilds_to_refresh(now).await? {
            let before = self.ents.guild(g).await.ok().map(|e| e.level);
            self.ents.invalidate_guild(g).await;
            if let Some(c) = self.repo.refresh_guild(g, &cfg, now).await? {
                changes.push(c);
                self.ents.invalidate_guild(g).await;
                let ent = self.ents.guild(g).await?;
                self.events
                    .entitlement_guild(
                        g,
                        json!({ "type": "guild", "guild_id": g.to_string(), "level": ent.level,
                                "pool_cents": ent.pool_cents, "changed": true, "was": before }),
                    )
                    .await;
            }
        }
        for t in self.repo.patron_transitions(now, cfg.grace_days).await? {
            let reason = match t.kind {
                PatronTransitionKind::EnteredGrace => "grace",
                PatronTransitionKind::Lapsed => "lapsed",
                PatronTransitionKind::Reminder => "expiring",
            };
            self.notify_user(t.user_id, reason).await;
        }
        Ok(changes)
    }

    // ---- staff ------------------------------------------------------------------------------

    pub async fn resolve(
        &self,
        invoice_id: i64,
        kind: ResolutionKind,
        note: &str,
        by: i64,
    ) -> PayResult<SaveOutcome> {
        let cfg = self.ents.config().await?;
        let inv = self
            .repo
            .invoice(invoice_id)
            .await?
            .ok_or(PayError::NotFound)?;
        let plan = if kind == ResolutionKind::MarkPaid {
            credit_plan(&inv, &cfg)
        } else {
            CreditPlan::default()
        };
        let out = self
            .repo
            .resolve(invoice_id, kind, note, by, &plan, self.now())
            .await?;
        self.after(&out).await;
        Ok(out)
    }

    /// Staff grant (Patron time or a pool contribution).
    pub async fn admin_grant(
        &self,
        spec: GrantSpec,
        permanent: bool,
        by: i64,
        reason: &str,
    ) -> PayResult<crate::entitlements::Grant> {
        let g = self
            .repo
            .grant(&spec, by, permanent, None, reason, self.now())
            .await?;
        match (spec.kind, spec.user_id, spec.guild_id) {
            (GrantKind::Patron, Some(u), _) => self.notify_user(u, "granted").await,
            (GrantKind::ServerPool, _, Some(gid)) => self.refresh_guild(gid).await,
            _ => {}
        }
        Ok(g)
    }

    pub async fn admin_revoke(
        &self,
        id: i64,
        by: i64,
        reason: &str,
    ) -> PayResult<crate::entitlements::Grant> {
        let g = self.repo.revoke_grant(id, by, reason, self.now()).await?;
        if let Some(u) = g.user_id.filter(|_| g.kind == GrantKind::Patron) {
            self.notify_user(u, "revoked").await;
        }
        if let Some(gid) = g.guild_id.filter(|_| g.kind == GrantKind::ServerPool) {
            self.refresh_guild(gid).await;
        }
        Ok(g)
    }
}

/// The `SR_PAYMENT_UPDATE` payload of an invoice.
pub fn payment_payload(inv: &Invoice) -> Value {
    json!({
        "type": "invoice",
        "invoice_id": inv.id.to_string(),
        "state": inv.state.as_str(),
        "confirmations": inv.confirmations,
        "confirmations_required": inv.confirmations_required,
        "received_sats": inv.received_sats,
        "confirmed_sats": inv.confirmed_sats,
        "credited": inv.state.is_credited(),
        "late": inv.late,
    })
}
