//! The storage port of the payments crate. `sr-db::payments_repo::PgPaymentRepo` implements it on
//! Postgres; [`crate::memory::MemoryRepo`] implements it for tests.
//!
//! The crediting methods (`save_evaluation`, `resolve`) are the transactional core: they lock the
//! invoice, change its state and apply or revert its [`CreditPlan`] exactly once, so polling,
//! restarts and concurrent workers can never credit twice.

use crate::{
    config::DonationConfig,
    entitlements::{Grant, GrantKind, GuildEntitlement, PatronStatus},
    error::PayResult,
    invoice::{Evaluation, Invoice, InvoiceState, Purpose, ResolutionKind, TxRecord},
};
use async_trait::async_trait;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wallet {
    pub id: i64,
    pub network: String,
    pub descriptor: String,
    pub fingerprint: String,
    pub next_index: u32,
    pub gap_limit: u32,
    pub active: bool,
    pub created_ms: i64,
}

#[derive(Debug, Clone)]
pub struct NewInvoice {
    pub id: i64,
    pub user_id: i64,
    pub purpose: Purpose,
    pub guild_id: Option<i64>,
    pub months: Option<u32>,
    pub credits: Option<u32>,
    pub usd_cents: i64,
    pub sats: i64,
    pub rate_cents_per_btc: i64,
    pub rate_source: String,
    pub rate_stale: bool,
    pub confirmations_required: u32,
    pub anonymous: bool,
    pub created_ms: i64,
    pub expires_ms: i64,
    pub quote_expires_ms: i64,
}

/// One transaction the chain shows right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainObs {
    pub txid: String,
    pub amount_sats: i64,
    pub block_height: Option<u32>,
    pub confirmations: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantSpec {
    pub kind: GrantKind,
    pub user_id: Option<i64>,
    pub guild_id: Option<i64>,
    pub days: Option<i64>,
    pub usd_cents: Option<i64>,
}

/// What crediting an invoice does.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreditPlan {
    pub grants: Vec<GrantSpec>,
    /// Platform invite credits to append to the ledger (reversed with a negative row on revert).
    pub invite_credits: i64,
    pub invite_user: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreditChange {
    None,
    Applied,
    Reverted,
}

#[derive(Debug, Clone)]
pub struct SaveOutcome {
    pub old_state: InvoiceState,
    pub invoice: Invoice,
    pub credit: CreditChange,
    /// State, counters or credit changed (worth an event).
    pub changed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct InvoiceFilter {
    pub state: Option<InvoiceState>,
    pub purpose: Option<Purpose>,
    pub user_id: Option<i64>,
    /// An invoice id, an address, or a txid prefix.
    pub q: Option<String>,
    pub needs_attention: bool,
    pub before_id: Option<i64>,
    pub limit: u32,
}

#[derive(Debug, Clone)]
pub struct AuditRow {
    pub at_ms: i64,
    pub kind: String,
    pub detail: Value,
}

#[derive(Debug, Clone, Default)]
pub struct WatcherStatus {
    pub last_run_ms: Option<i64>,
    pub last_ok_ms: Option<i64>,
    pub last_error: Option<String>,
    pub tip_height: Option<u32>,
    pub polled: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DonorPrefs {
    pub hide_name: bool,
    pub hide_amount: bool,
    pub hide_badge: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaderRow {
    pub user_id: Option<i64>,
    pub guild_id: Option<i64>,
    pub cents: i64,
    pub hide_name: bool,
    pub hide_amount: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub id: i64,
    pub name: String,
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuildAccess {
    pub member: bool,
    pub owner: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelChange {
    pub guild_id: i64,
    pub old: u8,
    pub new: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatronTransitionKind {
    /// Paid time ran out; perks continue for the grace period.
    EnteredGrace,
    /// Grace ended; perks are off.
    Lapsed,
    /// Perks end in about three days.
    Reminder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatronTransition {
    pub user_id: i64,
    pub kind: PatronTransitionKind,
}

#[derive(Debug, Clone, Default)]
pub struct GrantFilter {
    pub kind: Option<GrantKind>,
    pub user_id: Option<i64>,
    pub guild_id: Option<i64>,
    pub active_only: bool,
    pub before_id: Option<i64>,
    pub limit: u32,
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    /// `(purpose, credited usd cents, credited count)`.
    pub by_purpose: Vec<(String, i64, i64)>,
    /// `(YYYY-MM-DD, credited usd cents)`.
    pub by_day: Vec<(String, i64)>,
    pub by_state: Vec<(InvoiceState, i64)>,
    pub credited_cents: i64,
}

#[async_trait]
pub trait PaymentRepo: Send + Sync {
    // ---- wallet and configuration ----
    async fn active_wallet(&self) -> PayResult<Option<Wallet>>;
    async fn wallet(&self, id: i64) -> PayResult<Option<Wallet>>;
    async fn set_wallet(
        &self,
        network: &str,
        descriptor: &str,
        fingerprint: &str,
        gap_limit: u32,
        by: Option<i64>,
        now_ms: i64,
    ) -> PayResult<Wallet>;
    async fn set_gap_limit(&self, wallet_id: i64, gap_limit: u32) -> PayResult<()>;
    /// Consecutive unused addresses at the end of the allocated range (compare with `gap_limit`).
    async fn gap_used(&self, wallet_id: i64) -> PayResult<u32>;
    async fn config(&self) -> PayResult<DonationConfig>;
    async fn set_config(&self, cfg: &DonationConfig, by: i64, now_ms: i64) -> PayResult<()>;

    // ---- invoices ----
    /// Reserves the next derivation index (refusing past the gap limit and `max_open` open
    /// invoices of the user), derives the address with `derive`, stores the invoice.
    async fn create_invoice(
        &self,
        wallet_id: i64,
        new: NewInvoice,
        max_open: u32,
        derive: &(dyn Fn(u32) -> PayResult<String> + Send + Sync),
    ) -> PayResult<Invoice>;
    async fn invoice(&self, id: i64) -> PayResult<Option<Invoice>>;
    async fn invoice_txs(&self, id: i64) -> PayResult<Vec<TxRecord>>;
    async fn invoice_audit(&self, id: i64) -> PayResult<Vec<AuditRow>>;
    async fn user_invoices(
        &self,
        user_id: i64,
        before_id: Option<i64>,
        limit: u32,
    ) -> PayResult<Vec<Invoice>>;
    async fn search_invoices(&self, f: &InvoiceFilter) -> PayResult<Vec<Invoice>>;
    /// Invoices whose `next_check_ms` has passed and that are not final.
    async fn due_invoices(&self, now_ms: i64, limit: u32) -> PayResult<Vec<Invoice>>;
    /// Stores what the chain shows: upserts the transactions, marks vanished ones dropped,
    /// revives reappeared ones. Returns every transaction of the invoice.
    async fn record_observations(
        &self,
        invoice_id: i64,
        obs: &[ChainObs],
        now_ms: i64,
    ) -> PayResult<Vec<TxRecord>>;
    async fn save_evaluation(
        &self,
        invoice_id: i64,
        eval: &Evaluation,
        plan: &CreditPlan,
        next_check_ms: i64,
        now_ms: i64,
    ) -> PayResult<SaveOutcome>;
    async fn resolve(
        &self,
        invoice_id: i64,
        kind: ResolutionKind,
        note: &str,
        by: i64,
        plan: &CreditPlan,
        now_ms: i64,
    ) -> PayResult<SaveOutcome>;
    async fn cancel(&self, invoice_id: i64, user_id: i64, now_ms: i64) -> PayResult<Invoice>;
    async fn schedule_check(&self, invoice_id: i64, next_check_ms: i64) -> PayResult<()>;
    async fn audit(&self, invoice_id: i64, kind: &str, detail: Value, now_ms: i64)
    -> PayResult<()>;

    // ---- entitlements ----
    async fn user_patron(
        &self,
        user_id: i64,
        now_ms: i64,
        grace_days: u32,
    ) -> PayResult<PatronStatus>;
    async fn guild_entitlement(&self, guild_id: i64, now_ms: i64) -> PayResult<GuildEntitlement>;
    async fn grant(
        &self,
        spec: &GrantSpec,
        by: i64,
        permanent: bool,
        ends_ms: Option<i64>,
        reason: &str,
        now_ms: i64,
    ) -> PayResult<Grant>;
    async fn revoke_grant(&self, id: i64, by: i64, reason: &str, now_ms: i64) -> PayResult<Grant>;
    async fn list_grants(&self, f: &GrantFilter) -> PayResult<Vec<Grant>>;
    /// Recomputes a guild's pool and level (with the grace rule); `Some` when the level changed.
    async fn refresh_guild(
        &self,
        guild_id: i64,
        cfg: &DonationConfig,
        now_ms: i64,
    ) -> PayResult<Option<LevelChange>>;
    /// Guilds with a level above 0 or a live contribution.
    async fn guilds_to_refresh(&self, now_ms: i64) -> PayResult<Vec<i64>>;
    async fn patron_transitions(
        &self,
        now_ms: i64,
        grace_days: u32,
    ) -> PayResult<Vec<PatronTransition>>;
    async fn guild_access(&self, user_id: i64, guild_id: i64) -> PayResult<GuildAccess>;

    // ---- preferences, leaderboards, reports ----
    async fn donor_prefs(&self, user_id: i64) -> PayResult<DonorPrefs>;
    async fn set_donor_prefs(&self, user_id: i64, prefs: DonorPrefs) -> PayResult<()>;
    async fn guild_hidden(&self, guild_id: i64) -> PayResult<bool>;
    async fn set_guild_hidden(&self, guild_id: i64, hidden: bool) -> PayResult<()>;
    /// Ranked donors (credited, not reverted), `since_ms` `None` for lifetime.
    async fn leaderboard_donors(
        &self,
        since_ms: Option<i64>,
        limit: u32,
    ) -> PayResult<Vec<LeaderRow>>;
    async fn leaderboard_servers(
        &self,
        since_ms: Option<i64>,
        limit: u32,
    ) -> PayResult<Vec<LeaderRow>>;
    /// A user's total and 1-based rank among donors.
    async fn donor_rank(
        &self,
        user_id: i64,
        since_ms: Option<i64>,
    ) -> PayResult<Option<(u32, i64)>>;
    async fn top_contributors(
        &self,
        guild_id: i64,
        since_ms: i64,
        limit: u32,
    ) -> PayResult<Vec<LeaderRow>>;
    async fn profiles(&self, ids: &[i64]) -> PayResult<Vec<Profile>>;
    /// Guild names and icons (for the server leaderboard).
    async fn guild_profiles(&self, ids: &[i64]) -> PayResult<Vec<Profile>>;
    /// Staff moderation: keep a donor or a server off the leaderboards entirely.
    async fn set_staff_hidden(
        &self,
        user_id: Option<i64>,
        guild_id: Option<i64>,
        hidden: bool,
    ) -> PayResult<()>;
    async fn donated_since(&self, since_ms: i64) -> PayResult<i64>;
    async fn report(&self, from_ms: i64, to_ms: i64) -> PayResult<Report>;
    async fn watcher_status(&self) -> PayResult<WatcherStatus>;
    async fn set_watcher_status(&self, s: &WatcherStatus) -> PayResult<()>;
}

/// Adapts a [`PaymentRepo`] to the [`crate::entitlements::EntitlementSource`] the cache reads from.
pub struct RepoSource(pub std::sync::Arc<dyn PaymentRepo>);

#[async_trait]
impl crate::entitlements::EntitlementSource for RepoSource {
    async fn user_entitlement(
        &self,
        user_id: i64,
        now_ms: i64,
        grace_days: u32,
    ) -> PayResult<PatronStatus> {
        self.0.user_patron(user_id, now_ms, grace_days).await
    }

    async fn guild_entitlement(&self, guild_id: i64, now_ms: i64) -> PayResult<GuildEntitlement> {
        self.0.guild_entitlement(guild_id, now_ms).await
    }

    async fn config(&self) -> PayResult<DonationConfig> {
        self.0.config().await
    }
}
