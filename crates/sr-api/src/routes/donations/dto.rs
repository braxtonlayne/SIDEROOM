//! Request and response shapes of the donation endpoints. Ids are strings; money is integer USD
//! cents and satoshis; times are Unix milliseconds.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sr_payments::{
    Invoice, PatronStatus,
    invoice::{Resolution, TxRecord},
};
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct RateView {
    /// USD cents for one BTC.
    pub cents_per_btc: u64,
    pub source: String,
    pub age_secs: u64,
    /// The sources are failing and this is the last good answer.
    pub stale: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PatronOffer {
    pub price_cents: u32,
    pub max_months: u32,
    pub days_per_month: u32,
    pub grace_days: u32,
    /// What Patron unlocks (the `PerkSet` of the configuration).
    #[schema(value_type = Object)]
    pub perks: Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LevelOffer {
    pub level: u8,
    pub threshold_cents: u32,
    #[schema(value_type = Object)]
    pub perks: Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HostingView {
    pub cost_cents: u32,
    pub donated_this_month_cents: i64,
    pub month_start_ms: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DonationConfigView {
    /// Invoices can be created (a wallet is configured and donations are switched on).
    pub enabled: bool,
    pub network: Option<String>,
    pub confirmations: u32,
    pub invoice_expiry_minutes: u32,
    pub rate_lock_minutes: u32,
    pub patron: PatronOffer,
    pub levels: Vec<LevelOffer>,
    #[schema(value_type = Object)]
    pub free_perks: Value,
    pub invite_credit_price_cents: u32,
    pub max_invite_credits: u32,
    pub min_donation_cents: u32,
    pub max_donation_cents: u32,
    pub rate: Option<RateView>,
    pub hosting: HostingView,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateInvoiceBody {
    /// `donate`, `patron`, `server_level` or `invite_credit`.
    pub purpose: String,
    /// USD cents (`donate`, `server_level`).
    pub usd_cents: Option<u64>,
    /// Months of Patron (`patron`).
    pub months: Option<u32>,
    /// The server (`server_level`).
    pub guild_id: Option<String>,
    /// Platform invite credits (`invite_credit`).
    pub credits: Option<u32>,
    /// Contribute without showing the name (server contributions).
    pub anonymous: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TxView {
    pub txid: String,
    pub amount_sats: i64,
    pub confirmations: u32,
    /// The chain source stopped showing it (replaced or reorged out).
    pub dropped: bool,
}

impl From<&TxRecord> for TxView {
    fn from(t: &TxRecord) -> Self {
        Self {
            txid: t.txid.clone(),
            amount_sats: t.amount_sats,
            confirmations: t.confirmations,
            dropped: t.dropped_ms.is_some(),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InvoiceView {
    pub id: String,
    pub purpose: String,
    /// `open`, `seen`, `confirming`, `confirmed`, `overpaid`, `late_paid`, `underpaid`, `expired`,
    /// `cancelled` or `resolved`.
    pub state: String,
    pub usd_cents: i64,
    pub sats: i64,
    /// `sats` as a BTC decimal string.
    pub btc: String,
    pub address: String,
    /// `bitcoin:ADDRESS?amount=BTC&label=...`
    pub bip21: String,
    pub network: String,
    pub guild_id: Option<String>,
    pub months: Option<u32>,
    pub credits: Option<u32>,
    pub anonymous: bool,
    pub rate: RateView,
    pub confirmations_required: u32,
    pub confirmations: Option<u32>,
    pub received_sats: i64,
    pub confirmed_sats: i64,
    pub shortfall_sats: i64,
    pub excess_sats: i64,
    pub late: bool,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
    pub quote_expires_at_ms: i64,
    pub credited_at_ms: Option<i64>,
    pub explorer_url: Option<String>,
    pub txs: Vec<TxView>,
    /// How staff closed it, if they did (`mark_paid`, `refund_noted`, `dismiss`); notes stay staff-only.
    pub resolution: Option<String>,
}

impl InvoiceView {
    pub fn build(inv: &Invoice, txs: &[TxRecord], explorer: Option<String>, now_ms: i64) -> Self {
        let sats = u64::try_from(inv.sats).unwrap_or(0);
        Self {
            id: inv.id.to_string(),
            purpose: inv.purpose.as_str().to_owned(),
            state: inv.state.as_str().to_owned(),
            usd_cents: inv.usd_cents,
            sats: inv.sats,
            btc: sr_payments::descriptor::btc_string(sats),
            address: inv.address.clone(),
            bip21: sr_payments::PaymentService::bip21_uri(inv),
            network: inv.network.clone(),
            guild_id: inv.guild_id.map(|g| g.to_string()),
            months: inv.months,
            credits: inv.credits,
            anonymous: inv.anonymous,
            rate: RateView {
                cents_per_btc: u64::try_from(inv.rate_cents_per_btc).unwrap_or(0),
                source: inv.rate_source.clone(),
                age_secs: u64::try_from((now_ms - inv.created_ms) / 1000).unwrap_or(0),
                stale: inv.rate_stale,
            },
            confirmations_required: inv.confirmations_required,
            confirmations: inv.confirmations,
            received_sats: inv.received_sats,
            confirmed_sats: inv.confirmed_sats,
            shortfall_sats: inv.shortfall_sats,
            excess_sats: inv.excess_sats,
            late: inv.late,
            created_at_ms: inv.created_ms,
            expires_at_ms: inv.expires_ms,
            quote_expires_at_ms: inv.quote_expires_ms,
            credited_at_ms: inv.credited_ms,
            explorer_url: explorer,
            txs: txs.iter().map(TxView::from).collect(),
            resolution: inv.resolution.as_ref().map(|r| r.kind.as_str().to_owned()),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InvoicePage {
    pub items: Vec<InvoiceView>,
    pub next_before: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PatronView {
    pub active: bool,
    pub in_grace: bool,
    pub permanent: bool,
    pub until_ms: Option<i64>,
    pub started_ms: Option<i64>,
}

impl From<PatronStatus> for PatronView {
    fn from(p: PatronStatus) -> Self {
        Self {
            active: p.active,
            in_grace: p.in_grace,
            permanent: p.permanent,
            until_ms: p.until_ms,
            started_ms: p.started_ms,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PrefsView {
    pub hide_name: bool,
    pub hide_amount: bool,
    pub hide_badge: bool,
    /// Off the contributor boards (movies, episodes, chapters).
    pub hide_contributions: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PrefsBody {
    pub hide_name: Option<bool>,
    pub hide_amount: Option<bool>,
    pub hide_badge: Option<bool>,
    pub hide_contributions: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MeView {
    pub patron: PatronView,
    /// The caller's effective limits outside any server (`Limits`).
    #[schema(value_type = Object)]
    pub limits: Value,
    pub preferences: PrefsView,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ContributorView {
    pub user_id: Option<String>,
    pub name: String,
    pub avatar: Option<String>,
    /// Omitted when the contributor hides the amount.
    pub cents: Option<i64>,
    pub anonymous: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GuildSupportView {
    pub guild_id: String,
    pub level: u8,
    pub pool_cents: i64,
    pub pool_window_days: u32,
    pub next_threshold_cents: Option<u32>,
    /// While set, the pool is below the level and the level is held until then.
    pub grace_until_ms: Option<i64>,
    /// What the server's current level gives every member (`PerkSet`).
    #[schema(value_type = Object)]
    pub perks: Value,
    pub top_contributors: Vec<ContributorView>,
    pub hidden_from_leaderboard: bool,
    pub is_owner: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GuildSupportPatch {
    pub hidden_from_leaderboard: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BoardItem {
    pub rank: u32,
    pub user_id: Option<String>,
    pub guild_id: Option<String>,
    /// "Anonymous patron" when the donor hides the name.
    pub name: String,
    pub avatar: Option<String>,
    /// Omitted when the donor hides the amount.
    pub cents: Option<i64>,
    pub anonymous: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MyRank {
    pub rank: u32,
    pub cents: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BoardView {
    pub board: String,
    pub window: String,
    pub items: Vec<BoardItem>,
    pub me: Option<MyRank>,
    pub generated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ContributorItem {
    pub rank: u32,
    pub user_id: String,
    pub name: String,
    pub avatar: Option<String>,
    /// Approved items that still count (revoked credits do not).
    pub count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ContributorRank {
    pub rank: u32,
    pub count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ContributorBoardView {
    /// `movies`, `episodes` or `chapters`.
    pub kind: String,
    pub items: Vec<ContributorItem>,
    pub me: Option<ContributorRank>,
    pub generated_at_ms: i64,
}

// ---- admin -------------------------------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct WalletStatus {
    pub network: String,
    /// First 8 hex characters of SHA-256 of the descriptor. The descriptor itself is never returned.
    pub fingerprint: String,
    pub next_index: u32,
    pub gap_limit: u32,
    pub gap_used: u32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WatcherView {
    pub last_run_ms: Option<i64>,
    pub last_ok_ms: Option<i64>,
    pub last_error: Option<String>,
    pub tip_height: Option<u32>,
    pub polled: u32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RateSourceView {
    pub name: String,
    pub last_cents: Option<u64>,
    pub last_ok_ms: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StatusView {
    pub enabled: bool,
    pub wallet: Option<WalletStatus>,
    pub watcher: WatcherView,
    /// Counts by invoice state over the last 90 days.
    #[schema(value_type = Object)]
    pub counts: Value,
    pub needs_attention: i64,
    pub rate: Option<RateView>,
    pub rate_sources: Vec<RateSourceView>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetWalletBody {
    /// A watch-only descriptor such as `wpkh(xpub.../0/*)`. Private keys are refused.
    pub descriptor: String,
    /// `mainnet`, `testnet`, `signet` or `regtest`.
    pub network: String,
    pub gap_limit: Option<u32>,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WalletResult {
    pub network: String,
    pub fingerprint: String,
    /// The first addresses, to compare with the operator's own wallet.
    pub addresses: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TestWalletBody {
    pub descriptor: String,
    pub network: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TestWalletResult {
    pub fingerprint: String,
    pub addresses: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PatchConfigBody {
    /// A JSON merge patch over the configuration (`DonationConfig` fields).
    #[schema(value_type = Object)]
    pub changes: Value,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuditView {
    pub at_ms: i64,
    pub kind: String,
    #[schema(value_type = Object)]
    pub detail: Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminInvoiceView {
    #[serde(flatten)]
    pub invoice: InvoiceView,
    pub user_id: String,
    pub user_name: Option<String>,
    pub derivation_index: u32,
    pub resolution_note: Option<String>,
    pub resolved_by: Option<String>,
    pub audit: Vec<AuditView>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminInvoicePage {
    pub items: Vec<AdminInvoiceView>,
    pub next_before: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ResolveBody {
    /// `mark_paid`, `refund_noted` or `dismiss`.
    pub resolution: String,
    /// What was done (a refund note says how and when it was sent from the operator's own wallet).
    pub note: String,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GrantBody {
    /// `patron` or `server_pool`.
    pub kind: String,
    pub user_id: Option<String>,
    pub guild_id: Option<String>,
    /// Patron: days of time.
    pub days: Option<i64>,
    /// Patron: never ends.
    pub permanent: Option<bool>,
    /// Server pool: USD cents.
    pub usd_cents: Option<i64>,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GrantView {
    pub id: String,
    pub kind: String,
    pub user_id: Option<String>,
    pub guild_id: Option<String>,
    /// `invoice`, `admin` or `migrated`.
    pub source: String,
    pub source_id: Option<String>,
    pub days: Option<i64>,
    pub ends_at_ms: Option<i64>,
    pub permanent: bool,
    pub usd_cents: Option<i64>,
    pub starts_at_ms: i64,
    pub revoked_at_ms: Option<i64>,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GrantPage {
    pub items: Vec<GrantView>,
    pub next_before: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RatesView {
    pub current: Option<RateView>,
    pub sources: Vec<RateSourceView>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReportView {
    pub from_ms: i64,
    pub to_ms: i64,
    pub credited_cents: i64,
    /// `[purpose, cents, count]`.
    #[schema(value_type = Vec<Object>)]
    pub by_purpose: Vec<Value>,
    #[schema(value_type = Vec<Object>)]
    pub by_day: Vec<Value>,
    #[schema(value_type = Object)]
    pub by_state: Value,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct HideBody {
    pub user_id: Option<String>,
    pub guild_id: Option<String>,
    pub hidden: bool,
    pub reason: Option<String>,
}

pub(crate) fn resolution_note(r: &Option<Resolution>) -> Option<String> {
    r.as_ref().map(|r| r.note.clone())
}
