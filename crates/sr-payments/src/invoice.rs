//! Invoices: the domain types and the pure state evaluator.
//!
//! The evaluator looks at what the chain shows *now* and says which state the invoice is in and
//! whether it is paid. It never mutates anything, so polling twice is harmless and a reorg is
//! just another evaluation that comes out lower (see `service::PaymentService::apply`).

use crate::config::DonationConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    /// A donation to the community; no entitlement.
    Donate,
    /// Patron months for the payer.
    Patron,
    /// A contribution to a server's monthly pool.
    ServerLevel,
    /// Extra platform invite credits (D-025).
    InviteCredit,
}

impl Purpose {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Donate => "donate",
            Self::Patron => "patron",
            Self::ServerLevel => "server_level",
            Self::InviteCredit => "invite_credit",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "donate" => Self::Donate,
            "patron" => Self::Patron,
            "server_level" => Self::ServerLevel,
            "invite_credit" => Self::InviteCredit,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceState {
    /// Waiting for a payment.
    Open,
    /// Paid in full, still in the mempool.
    Seen,
    /// Paid in full, below the required depth.
    Confirming,
    /// Paid in full and deep enough: credited.
    Confirmed,
    /// Credited; more than the tolerance was paid (excess noted for staff).
    Overpaid,
    /// Credited; the paying transaction first appeared after the invoice expired.
    LatePaid,
    /// Something arrived, but not enough (top-up is possible).
    Underpaid,
    /// Nothing arrived in time.
    Expired,
    /// The payer cancelled before paying.
    Cancelled,
    /// Staff closed it by hand (see [`Resolution`]).
    Resolved,
}

impl InvoiceState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Seen => "seen",
            Self::Confirming => "confirming",
            Self::Confirmed => "confirmed",
            Self::Overpaid => "overpaid",
            Self::LatePaid => "late_paid",
            Self::Underpaid => "underpaid",
            Self::Expired => "expired",
            Self::Cancelled => "cancelled",
            Self::Resolved => "resolved",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "open" => Self::Open,
            "seen" => Self::Seen,
            "confirming" => Self::Confirming,
            "confirmed" => Self::Confirmed,
            "overpaid" => Self::Overpaid,
            "late_paid" => Self::LatePaid,
            "underpaid" => Self::Underpaid,
            "expired" => Self::Expired,
            "cancelled" => Self::Cancelled,
            "resolved" => Self::Resolved,
            _ => return None,
        })
    }

    /// States in which the entitlement is applied.
    pub const fn is_credited(self) -> bool {
        matches!(self, Self::Confirmed | Self::Overpaid | Self::LatePaid)
    }

    /// States the payer or staff closed; the chain no longer changes them.
    pub const fn is_closed(self) -> bool {
        matches!(self, Self::Cancelled | Self::Resolved)
    }

    /// Needs a human: staff should look at it.
    pub const fn needs_attention(self) -> bool {
        matches!(self, Self::Underpaid | Self::Overpaid | Self::LatePaid)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionKind {
    /// Staff confirm the payment arrived (credits the purpose once).
    MarkPaid,
    /// The operator refunded outside this system; the note records how.
    RefundNoted,
    /// Nothing to do (spam, test, duplicate).
    Dismiss,
}

impl ResolutionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MarkPaid => "mark_paid",
            Self::RefundNoted => "refund_noted",
            Self::Dismiss => "dismiss",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "mark_paid" => Self::MarkPaid,
            "refund_noted" => Self::RefundNoted,
            "dismiss" => Self::Dismiss,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub kind: ResolutionKind,
    pub note: String,
    pub by: i64,
    pub at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invoice {
    pub id: i64,
    pub wallet_id: i64,
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
    pub address: String,
    pub derivation_index: u32,
    pub network: String,
    pub state: InvoiceState,
    pub confirmations_required: u32,
    /// Confirmations of the least-confirmed paying transaction (`None` when nothing was seen).
    pub confirmations: Option<u32>,
    pub received_sats: i64,
    pub confirmed_sats: i64,
    pub shortfall_sats: i64,
    pub excess_sats: i64,
    pub late: bool,
    pub anonymous: bool,
    pub created_ms: i64,
    pub expires_ms: i64,
    pub quote_expires_ms: i64,
    pub credited_ms: Option<i64>,
    pub next_check_ms: i64,
    pub resolution: Option<Resolution>,
}

/// A transaction as the repository stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxRecord {
    pub invoice_id: i64,
    pub txid: String,
    pub amount_sats: i64,
    pub block_height: Option<u32>,
    /// Confirmations at the last observation (0 for the mempool or a dropped transaction).
    pub confirmations: u32,
    pub first_seen_ms: i64,
    pub last_seen_ms: i64,
    /// The chain source stopped showing it (replaced or reorged out) at this time.
    pub dropped_ms: Option<i64>,
}

/// One live transaction for the evaluator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observed {
    pub amount_sats: u64,
    pub confirmations: u32,
    pub first_seen_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evaluation {
    pub state: InvoiceState,
    pub received_sats: u64,
    pub confirmed_sats: u64,
    pub confirmations: Option<u32>,
    pub late: bool,
    pub shortfall_sats: u64,
    pub excess_sats: u64,
}

impl Evaluation {
    pub fn credited(&self) -> bool {
        self.state.is_credited()
    }
}

/// What the chain shows for an invoice, as a state.
pub fn evaluate(
    sats: u64,
    expires_ms: i64,
    confirmations_required: u32,
    tolerance_bps: u32,
    txs: &[Observed],
    now_ms: i64,
) -> Evaluation {
    let tol = sats * u64::from(tolerance_bps) / 10_000;
    let min_ok = sats.saturating_sub(tol).max(1);
    let max_ok = sats + tol;
    let received: u64 = txs.iter().map(|t| t.amount_sats).sum();
    let confirmed: u64 = txs
        .iter()
        .filter(|t| t.confirmations >= confirmations_required)
        .map(|t| t.amount_sats)
        .sum();
    let min_conf = txs.iter().map(|t| t.confirmations).min();
    let excess = |paid: u64| paid.saturating_sub(sats);

    if confirmed >= min_ok {
        // The transaction that completed the payment decides "late".
        let mut counted: Vec<&Observed> = txs
            .iter()
            .filter(|t| t.confirmations >= confirmations_required)
            .collect();
        counted.sort_by_key(|t| t.first_seen_ms);
        let mut acc = 0u64;
        let mut completed_at = 0i64;
        for t in counted {
            acc += t.amount_sats;
            completed_at = t.first_seen_ms;
            if acc >= min_ok {
                break;
            }
        }
        let late = completed_at > expires_ms;
        let state = if confirmed > max_ok {
            InvoiceState::Overpaid
        } else if late {
            InvoiceState::LatePaid
        } else {
            InvoiceState::Confirmed
        };
        return Evaluation {
            state,
            received_sats: received,
            confirmed_sats: confirmed,
            confirmations: min_conf,
            late,
            shortfall_sats: 0,
            excess_sats: excess(confirmed),
        };
    }
    let (state, shortfall) = if received >= min_ok {
        let s = if min_conf == Some(0) {
            InvoiceState::Seen
        } else {
            InvoiceState::Confirming
        };
        (s, 0)
    } else if received > 0 {
        (InvoiceState::Underpaid, sats - received)
    } else if now_ms < expires_ms {
        (InvoiceState::Open, sats)
    } else {
        (InvoiceState::Expired, sats)
    };
    Evaluation {
        state,
        received_sats: received,
        confirmed_sats: confirmed,
        confirmations: min_conf,
        late: false,
        shortfall_sats: if state == InvoiceState::Underpaid {
            shortfall
        } else {
            0
        },
        excess_sats: 0,
    }
}

/// When to poll an invoice again.
pub fn next_check_ms(
    state: InvoiceState,
    now_ms: i64,
    expires_ms: i64,
    cfg: &DonationConfig,
) -> i64 {
    let poll = i64::from(cfg.poll_secs) * 1000;
    let delay = match state {
        InvoiceState::Open | InvoiceState::Seen | InvoiceState::Confirming => poll,
        InvoiceState::Underpaid if now_ms < expires_ms => poll,
        InvoiceState::Confirmed | InvoiceState::Overpaid | InvoiceState::LatePaid => poll * 4,
        InvoiceState::Expired | InvoiceState::Underpaid => {
            // Recently expired: every 5 minutes; after a day: hourly.
            if now_ms - expires_ms < 86_400_000 {
                300_000
            } else {
                3_600_000
            }
        }
        InvoiceState::Cancelled | InvoiceState::Resolved => 3_600_000,
    };
    now_ms + delay
}

/// Whether an invoice still has to be watched at `now_ms`.
pub fn watch_until_ms(inv: &Invoice, cfg: &DonationConfig) -> Option<i64> {
    match inv.state {
        InvoiceState::Open | InvoiceState::Seen | InvoiceState::Confirming => None,
        InvoiceState::Expired | InvoiceState::Underpaid => {
            Some(inv.expires_ms + i64::from(cfg.late_watch_days) * 86_400_000)
        }
        // Credited: until the payment is deep enough to be final (checked by the caller via
        // `confirmations`). Cancelled and resolved invoices are not watched.
        _ => None,
    }
}

/// Whether the poller can stop looking at this invoice for good.
pub fn is_final(inv: &Invoice, cfg: &DonationConfig, now_ms: i64) -> bool {
    match inv.state {
        InvoiceState::Cancelled | InvoiceState::Resolved => true,
        InvoiceState::Expired | InvoiceState::Underpaid => {
            watch_until_ms(inv, cfg).is_some_and(|t| now_ms > t)
        }
        s if s.is_credited() => inv
            .confirmations
            .is_some_and(|c| c >= cfg.finality_confirmations.max(cfg.confirmations)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tx(amount: u64, conf: u32, seen: i64) -> Observed {
        Observed {
            amount_sats: amount,
            confirmations: conf,
            first_seen_ms: seen,
        }
    }

    const EXP: i64 = 3_600_000;

    fn ev(txs: &[Observed], now: i64) -> Evaluation {
        evaluate(10_000, EXP, 1, 100, txs, now)
    }

    #[test]
    fn open_until_expiry_then_expired() {
        assert_eq!(ev(&[], 0).state, InvoiceState::Open);
        assert_eq!(ev(&[], EXP - 1).state, InvoiceState::Open);
        let e = ev(&[], EXP);
        assert_eq!(e.state, InvoiceState::Expired);
        assert!(!e.credited());
    }

    #[test]
    fn mempool_then_confirming_then_confirmed() {
        assert_eq!(ev(&[tx(10_000, 0, 10)], 20).state, InvoiceState::Seen);
        let mut e = evaluate(10_000, EXP, 3, 100, &[tx(10_000, 1, 10)], 20);
        assert_eq!(e.state, InvoiceState::Confirming);
        assert_eq!(e.confirmations, Some(1));
        e = evaluate(10_000, EXP, 3, 100, &[tx(10_000, 3, 10)], 20);
        assert_eq!(e.state, InvoiceState::Confirmed);
        assert!(e.credited());
        assert_eq!(e.excess_sats, 0);
    }

    #[test]
    fn tolerance_both_ways() {
        // 1% of 10,000 is 100 sats.
        assert_eq!(ev(&[tx(9_900, 1, 1)], 2).state, InvoiceState::Confirmed);
        assert_eq!(ev(&[tx(9_899, 1, 1)], 2).state, InvoiceState::Underpaid);
        assert_eq!(ev(&[tx(10_100, 1, 1)], 2).state, InvoiceState::Confirmed);
        let o = ev(&[tx(10_101, 1, 1)], 2);
        assert_eq!(o.state, InvoiceState::Overpaid);
        assert_eq!(o.excess_sats, 101);
        assert!(o.credited());
    }

    #[test]
    fn underpaid_and_topup() {
        let e = ev(&[tx(4_000, 1, 1)], 2);
        assert_eq!(e.state, InvoiceState::Underpaid);
        assert_eq!(e.shortfall_sats, 6_000);
        // The payer tops up the same address.
        let e = ev(&[tx(4_000, 1, 1), tx(6_000, 1, 5)], 6);
        assert_eq!(e.state, InvoiceState::Confirmed);
        // Top-up still in the mempool: not credited yet.
        let e = ev(&[tx(4_000, 1, 1), tx(6_000, 0, 5)], 6);
        assert_eq!(e.state, InvoiceState::Seen);
    }

    #[test]
    fn late_payment_is_credited_and_flagged() {
        let e = ev(&[tx(10_000, 1, EXP + 5_000)], EXP + 10_000);
        assert_eq!(e.state, InvoiceState::LatePaid);
        assert!(e.late && e.credited());
        // Paid in time (seen before expiry) but confirmed after: not late.
        let e = ev(&[tx(10_000, 1, EXP - 5_000)], EXP + 10_000);
        assert_eq!(e.state, InvoiceState::Confirmed);
        assert!(!e.late);
        // A top-up after expiry that completes the amount is late.
        let e = ev(&[tx(4_000, 1, 10), tx(6_000, 1, EXP + 1)], EXP + 2);
        assert_eq!(e.state, InvoiceState::LatePaid);
        // Overpaid wins the state but keeps the late flag.
        let e = ev(&[tx(20_000, 1, EXP + 1)], EXP + 2);
        assert_eq!(e.state, InvoiceState::Overpaid);
        assert!(e.late);
    }

    #[test]
    fn reorg_lowers_the_evaluation() {
        let paid = ev(&[tx(10_000, 2, 1)], 5);
        assert!(paid.credited());
        // The block was reorged out: back in the mempool.
        let back = ev(&[tx(10_000, 0, 1)], 6);
        assert_eq!(back.state, InvoiceState::Seen);
        assert!(!back.credited());
        // The transaction vanished: open again (or expired).
        assert_eq!(ev(&[], 7).state, InvoiceState::Open);
        assert_eq!(ev(&[], EXP + 1).state, InvoiceState::Expired);
    }

    #[test]
    fn zero_confirmation_policy_credits_from_the_mempool() {
        let e = evaluate(10_000, EXP, 0, 100, &[tx(10_000, 0, 1)], 2);
        assert_eq!(e.state, InvoiceState::Confirmed);
    }

    #[test]
    fn polling_schedule_and_finality() {
        let cfg = DonationConfig::default();
        assert_eq!(next_check_ms(InvoiceState::Open, 1_000, EXP, &cfg), 31_000);
        assert_eq!(
            next_check_ms(InvoiceState::Expired, EXP + 1, EXP, &cfg),
            EXP + 1 + 300_000
        );
        assert_eq!(
            next_check_ms(InvoiceState::Expired, EXP + 2 * 86_400_000, EXP, &cfg),
            EXP + 2 * 86_400_000 + 3_600_000
        );
    }

    #[test]
    fn state_names_round_trip() {
        for s in [
            InvoiceState::Open,
            InvoiceState::Seen,
            InvoiceState::Confirming,
            InvoiceState::Confirmed,
            InvoiceState::Overpaid,
            InvoiceState::LatePaid,
            InvoiceState::Underpaid,
            InvoiceState::Expired,
            InvoiceState::Cancelled,
            InvoiceState::Resolved,
        ] {
            assert_eq!(InvoiceState::parse(s.as_str()), Some(s));
        }
        for p in [
            Purpose::Donate,
            Purpose::Patron,
            Purpose::ServerLevel,
            Purpose::InviteCredit,
        ] {
            assert_eq!(Purpose::parse(p.as_str()), Some(p));
        }
    }
}
