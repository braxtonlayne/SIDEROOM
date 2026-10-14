//! Scenario tests of `PaymentService` on the in-memory repository and a fake chain.
//! No network, no real keys: the wallet is the public BIP-84 vector re-serialised as a `tpub`.

use crate::{
    chain::FakeChain,
    descriptor::{Network, tests::bip84_tpub},
    entitlements::{EntitlementService, Entitlements, GrantKind},
    error::PayError,
    invoice::{InvoiceState, Purpose, ResolutionKind},
    memory::MemoryRepo,
    rates::{FakeRate, RateService, RateSettings},
    repo::PaymentRepo,
    service::{InvoiceRequest, PaymentService, RecordingEvents},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};

const MIN: i64 = 60_000;
const DAY: i64 = 86_400_000;

struct Rig {
    svc: PaymentService,
    repo: Arc<MemoryRepo>,
    chain: FakeChain,
    events: Arc<RecordingEvents>,
    clock: Arc<AtomicI64>,
    rate: Arc<FakeRate>,
}

impl Rig {
    async fn new(gap: u32) -> Self {
        let repo = Arc::new(MemoryRepo::new());
        let chain = FakeChain::new(100);
        let events = Arc::new(RecordingEvents::default());
        let clock = Arc::new(AtomicI64::new(1_000_000));
        let rate = Arc::new(FakeRate::new("fake", 6_500_000));
        let rates = Arc::new(RateService::new(
            vec![rate.clone()],
            RateSettings {
                cache_secs: 0,
                ..RateSettings::default()
            },
        ));
        let c = clock.clone();
        let now: Arc<dyn Fn() -> i64 + Send + Sync> = Arc::new(move || c.load(Ordering::SeqCst));
        let ents = Arc::new(EntitlementService::new(
            Arc::new(crate::repo::RepoSource(repo.clone())),
            now.clone(),
            Duration::from_millis(1),
        ));
        let next = Arc::new(AtomicI64::new(1000));
        let svc = PaymentService::new(
            repo.clone(),
            Arc::new(chain.clone()),
            rates,
            events.clone(),
            ents,
            now,
            Arc::new(move || next.fetch_add(1, Ordering::SeqCst)),
        );
        svc.set_wallet(
            &format!("wpkh({}/0/*)", bip84_tpub()),
            Network::Signet,
            gap,
            1,
        )
        .await
        .unwrap();
        Self {
            svc,
            repo,
            chain,
            events,
            clock,
            rate,
        }
    }

    fn advance(&self, ms: i64) {
        self.clock.fetch_add(ms, Ordering::SeqCst);
    }

    async fn invoice(&self, user: i64, req: InvoiceRequest) -> crate::invoice::Invoice {
        self.svc.create_invoice(user, req).await.unwrap()
    }

    async fn poll(&self, id: i64) -> crate::invoice::Invoice {
        let inv = self.repo.invoice(id).await.unwrap().unwrap();
        self.svc.poll_invoice(&inv).await.unwrap().invoice
    }
}

fn patron(months: u32) -> InvoiceRequest {
    InvoiceRequest {
        purpose: Purpose::Patron,
        usd_cents: None,
        months: Some(months),
        guild_id: None,
        credits: None,
        anonymous: false,
    }
}

fn server(guild: i64, cents: u64) -> InvoiceRequest {
    InvoiceRequest {
        purpose: Purpose::ServerLevel,
        usd_cents: Some(cents),
        months: None,
        guild_id: Some(guild),
        credits: None,
        anonymous: false,
    }
}

fn active_patron_grants(r: &Rig) -> usize {
    r.repo
        .grants()
        .iter()
        .filter(|g| g.kind == GrantKind::Patron && g.revoked_ms.is_none())
        .count()
}

#[tokio::test]
async fn patron_invoice_lifecycle_and_idempotent_crediting() {
    let r = Rig::new(20).await;
    let inv = r.invoice(7, patron(2)).await;
    assert_eq!(inv.usd_cents, 1000);
    assert_eq!(inv.sats, 15_385); // $10 at $65,000, rounded up
    assert!(inv.address.starts_with("tb1q"));
    assert_eq!(inv.derivation_index, 0);
    assert!(PaymentService::bip21_uri(&inv).starts_with("bitcoin:tb1q"));
    assert_eq!(inv.state, InvoiceState::Open);

    // Nothing yet.
    assert_eq!(r.poll(inv.id).await.state, InvoiceState::Open);

    // Mempool, then one block.
    r.chain.pay(&inv.address, "tx1", inv.sats as u64);
    let seen = r.poll(inv.id).await;
    assert_eq!(seen.state, InvoiceState::Seen);
    assert_eq!(active_patron_grants(&r), 0);
    r.chain.confirm("tx1", 101);
    let done = r.poll(inv.id).await;
    assert_eq!(done.state, InvoiceState::Confirmed);
    assert!(done.credited_ms.is_some());
    assert_eq!(active_patron_grants(&r), 1);

    // Polling again, or restarting the watcher, never credits twice.
    for _ in 0..3 {
        r.poll(inv.id).await;
    }
    assert_eq!(active_patron_grants(&r), 1);
    let ent = r.svc.entitlements().user(7).await.unwrap();
    assert!(ent.patron.active);
    assert_eq!(ent.patron.until_ms, Some(1_000_000 + 60 * DAY));

    // The payer heard about every change; the entitlement event carries the new limits.
    let pays = r.events.payment.lock().unwrap();
    assert!(pays.iter().all(|(u, _)| *u == 7));
    assert!(pays.iter().any(|(_, p)| p["state"] == "seen"));
    assert!(
        pays.iter()
            .any(|(_, p)| p["state"] == "confirmed" && p["credited"] == true)
    );
    let ents = r.events.user.lock().unwrap();
    assert_eq!(ents.len(), 1);
    assert_eq!(ents[0].1["patron"]["active"], true);
    assert_eq!(ents[0].1["limits"]["patron"], true);
    assert!(r.repo.audit_kinds(inv.id).contains(&"credited".to_owned()));
}

#[tokio::test]
async fn reorg_reverts_the_entitlement_and_a_second_confirmation_restores_it_once() {
    let r = Rig::new(20).await;
    let inv = r.invoice(7, patron(1)).await;
    r.chain
        .pay_confirmed(&inv.address, "tx1", inv.sats as u64, 101);
    r.chain.set_tip(101);
    assert_eq!(r.poll(inv.id).await.state, InvoiceState::Confirmed);
    assert!(r.svc.entitlements().user(7).await.unwrap().patron.active);

    // The block is reorged out and the transaction is back in the mempool.
    r.chain.unconfirm("tx1");
    let back = r.poll(inv.id).await;
    assert_eq!(back.state, InvoiceState::Seen);
    assert_eq!(back.credited_ms, None);
    assert_eq!(active_patron_grants(&r), 0);
    r.svc.entitlements().invalidate_user(7).await;
    assert!(
        !r.svc
            .entitlements()
            .user(7)
            .await
            .unwrap()
            .patron
            .perks_on()
    );
    assert!(r.repo.audit_kinds(inv.id).contains(&"reverted".to_owned()));

    // Worse: the transaction is replaced and vanishes.
    r.chain.drop_tx("tx1");
    assert_eq!(r.poll(inv.id).await.state, InvoiceState::Open);

    // It comes back in a new block: credited again, still one active grant.
    r.chain
        .pay_confirmed(&inv.address, "tx1", inv.sats as u64, 103);
    r.chain.set_tip(103);
    assert_eq!(r.poll(inv.id).await.state, InvoiceState::Confirmed);
    assert_eq!(active_patron_grants(&r), 1);
    let txs = r.repo.invoice_txs(inv.id).await.unwrap();
    assert_eq!(txs.len(), 1, "the same txid is matched, not duplicated");
}

#[tokio::test]
async fn underpaid_then_topped_up_and_staff_are_alerted() {
    let r = Rig::new(20).await;
    let inv = r.invoice(7, patron(1)).await;
    let half = (inv.sats / 2) as u64;
    r.chain.pay_confirmed(&inv.address, "a", half, 101);
    r.chain.set_tip(101);
    let u = r.poll(inv.id).await;
    assert_eq!(u.state, InvoiceState::Underpaid);
    assert_eq!(u.shortfall_sats, inv.sats - half as i64);
    assert_eq!(active_patron_grants(&r), 0);
    assert_eq!(r.events.alerts.lock().unwrap().len(), 1);

    r.chain
        .pay_confirmed(&inv.address, "b", inv.sats as u64 - half, 102);
    r.chain.set_tip(102);
    let ok = r.poll(inv.id).await;
    assert_eq!(ok.state, InvoiceState::Confirmed);
    assert_eq!(active_patron_grants(&r), 1);
}

#[tokio::test]
async fn late_and_over_payments_are_credited_and_flagged() {
    let r = Rig::new(20).await;
    let late = r.invoice(7, patron(1)).await;
    r.advance(61 * MIN);
    assert_eq!(r.poll(late.id).await.state, InvoiceState::Expired);
    r.chain
        .pay_confirmed(&late.address, "late", late.sats as u64, 101);
    r.chain.set_tip(101);
    r.advance(MIN);
    let l = r.poll(late.id).await;
    assert_eq!(l.state, InvoiceState::LatePaid);
    assert!(l.late);
    assert_eq!(active_patron_grants(&r), 1);
    assert!(
        r.events
            .alerts
            .lock()
            .unwrap()
            .iter()
            .any(|a| a.contains("late_paid"))
    );

    let over = r.invoice(8, patron(1)).await;
    r.chain
        .pay_confirmed(&over.address, "over", over.sats as u64 * 2, 102);
    r.chain.set_tip(102);
    let o = r.poll(over.id).await;
    assert_eq!(o.state, InvoiceState::Overpaid);
    assert_eq!(o.excess_sats, over.sats);
    assert_eq!(active_patron_grants(&r), 2);
}

#[tokio::test]
async fn gap_limit_stops_unused_addresses_piling_up() {
    let r = Rig::new(2).await;
    let a = r.invoice(1, patron(1)).await;
    let _b = r.invoice(2, patron(1)).await;
    let err = r.svc.create_invoice(3, patron(1)).await.unwrap_err();
    assert_eq!(err, PayError::GapLimit);
    // Address 0 receives a payment: the gap closes and a new address can be handed out.
    r.chain.pay(&a.address, "x", a.sats as u64);
    r.poll(a.id).await;
    let c = r.invoice(3, patron(1)).await;
    assert_eq!(c.derivation_index, 2);
    assert_ne!(c.address, a.address);
}

#[tokio::test]
async fn invite_credits_are_added_and_reversed() {
    let r = Rig::new(20).await;
    let inv = r
        .invoice(
            5,
            InvoiceRequest {
                purpose: Purpose::InviteCredit,
                usd_cents: None,
                months: None,
                guild_id: None,
                credits: Some(3),
                anonymous: false,
            },
        )
        .await;
    assert_eq!(inv.usd_cents, 6000);
    r.chain
        .pay_confirmed(&inv.address, "t", inv.sats as u64, 101);
    r.chain.set_tip(101);
    r.poll(inv.id).await;
    r.poll(inv.id).await;
    assert_eq!(r.repo.invite_balance(5), 3);
    r.chain.drop_tx("t");
    r.poll(inv.id).await;
    assert_eq!(r.repo.invite_balance(5), 0);
}

#[tokio::test]
async fn server_contributions_build_a_pool_and_a_level_with_grace() {
    let r = Rig::new(20).await;
    r.repo.add_member(1, 77, true);
    r.repo.add_member(2, 77, false);
    // Not a member: refused as unknown.
    assert_eq!(
        r.svc.create_invoice(3, server(77, 1000)).await.unwrap_err(),
        PayError::NotFound
    );
    let a = r.invoice(1, server(77, 600)).await;
    let b = r.invoice(2, server(77, 500)).await;
    for (inv, tx, h) in [(&a, "a", 101), (&b, "b", 102)] {
        r.chain.pay_confirmed(&inv.address, tx, inv.sats as u64, h);
    }
    r.chain.set_tip(102);
    r.poll(a.id).await;
    r.poll(b.id).await;
    r.svc.entitlements().invalidate_guild(77).await;
    let g = r.svc.entitlements().guild(77).await.unwrap();
    assert_eq!(g.pool_cents, 1100);
    assert_eq!(g.level, 1);
    assert!(
        r.events
            .guild
            .lock()
            .unwrap()
            .iter()
            .any(|(gid, p)| *gid == 77 && p["level"] == 1)
    );
    // Level 1 raises the upload limit of every member of the server.
    let l = r.svc.entitlements().limits(2, Some(77)).await.unwrap();
    assert_eq!(l.guild_level, 1);
    assert_eq!(l.video_senders, 8);

    // 31 days later the contributions left the window: the level is held for the grace period...
    r.advance(31 * DAY);
    r.svc.housekeeping().await.unwrap();
    r.svc.entitlements().invalidate_guild(77).await;
    assert_eq!(r.svc.entitlements().guild(77).await.unwrap().level, 1);
    // ...then drops.
    r.advance(4 * DAY);
    let changes = r.svc.housekeeping().await.unwrap();
    assert_eq!(changes.len(), 1);
    r.svc.entitlements().invalidate_guild(77).await;
    assert_eq!(r.svc.entitlements().guild(77).await.unwrap().level, 0);
}

#[tokio::test]
async fn stale_rate_is_flagged_and_no_rate_blocks_new_invoices() {
    let r = Rig::new(20).await;
    let fresh = r.invoice(1, patron(1)).await;
    assert!(!fresh.rate_stale);
    r.rate.set(Err("down".into()));
    r.advance(2 * MIN);
    let stale = r.invoice(1, patron(1)).await;
    assert!(stale.rate_stale);
    assert_eq!(stale.rate_cents_per_btc, fresh.rate_cents_per_btc);
    r.advance(30 * MIN);
    assert_eq!(
        r.svc.create_invoice(1, patron(1)).await.unwrap_err(),
        PayError::RateUnavailable
    );
}

#[tokio::test]
async fn validation_privacy_and_cancel() {
    let r = Rig::new(20).await;
    assert!(matches!(
        r.svc.create_invoice(1, patron(13)).await,
        Err(PayError::Invalid(_))
    ));
    assert!(matches!(
        r.svc
            .create_invoice(
                1,
                InvoiceRequest {
                    purpose: Purpose::Donate,
                    usd_cents: Some(10),
                    months: None,
                    guild_id: None,
                    credits: None,
                    anonymous: false
                }
            )
            .await,
        Err(PayError::Invalid(_))
    ));
    let inv = r.invoice(1, patron(1)).await;
    // Other people's invoices do not exist.
    assert_eq!(
        r.svc.own_invoice(2, inv.id).await.unwrap_err(),
        PayError::NotFound
    );
    assert_eq!(
        r.svc.cancel(2, inv.id).await.unwrap_err(),
        PayError::NotFound
    );
    assert_eq!(
        r.svc.cancel(1, inv.id).await.unwrap().state,
        InvoiceState::Cancelled
    );
    assert!(matches!(
        r.svc.cancel(1, inv.id).await,
        Err(PayError::Conflict(_))
    ));
    // A paid invoice cannot be cancelled.
    let paid = r.invoice(1, patron(1)).await;
    r.chain.pay(&paid.address, "p", paid.sats as u64);
    r.poll(paid.id).await;
    assert!(matches!(
        r.svc.cancel(1, paid.id).await,
        Err(PayError::Conflict(_))
    ));
}

#[tokio::test]
async fn check_now_is_rate_limited_and_survives_a_chain_outage() {
    let r = Rig::new(20).await;
    let inv = r.invoice(1, patron(1)).await;
    r.chain.pay(&inv.address, "t", inv.sats as u64);
    assert_eq!(
        r.svc.check_now(1, inv.id).await.unwrap().state,
        InvoiceState::Seen
    );
    // Within 10 seconds the chain is not asked again.
    r.chain.confirm("t", 101);
    assert_eq!(
        r.svc.check_now(1, inv.id).await.unwrap().state,
        InvoiceState::Seen
    );
    r.advance(11_000);
    assert_eq!(
        r.svc.check_now(1, inv.id).await.unwrap().state,
        InvoiceState::Confirmed
    );
    // An outage is not an error for the payer and does not touch the invoice.
    let other = r.invoice(1, patron(1)).await;
    r.chain.set_failing(true);
    assert_eq!(
        r.svc.check_now(1, other.id).await.unwrap().state,
        InvoiceState::Open
    );
    let err = r.svc.poll_invoice(&other).await.unwrap_err();
    assert!(matches!(err, PayError::Chain(_)));
}

#[tokio::test]
async fn poll_due_polls_each_invoice_on_schedule() {
    let r = Rig::new(20).await;
    let a = r.invoice(1, patron(1)).await;
    let _b = r.invoice(2, patron(1)).await;
    r.chain.pay_confirmed(&a.address, "t", a.sats as u64, 101);
    r.chain.set_tip(101);
    let s = r.svc.poll_due(50).await.unwrap();
    assert_eq!((s.polled, s.changed, s.errors), (2, 1, 0));
    assert_eq!(s.tip, Some(101));
    // Nothing is due right after.
    assert_eq!(r.svc.poll_due(50).await.unwrap().polled, 0);
    r.advance(31_000);
    assert_eq!(
        r.svc.poll_due(50).await.unwrap().polled,
        1,
        "only the still-open invoice"
    );
}

#[tokio::test]
async fn manual_resolution_is_idempotent_and_never_refunds_automatically() {
    let r = Rig::new(20).await;
    let inv = r.invoice(1, patron(1)).await;
    r.chain
        .pay_confirmed(&inv.address, "t", inv.sats as u64 / 2, 101);
    r.chain.set_tip(101);
    assert_eq!(r.poll(inv.id).await.state, InvoiceState::Underpaid);
    // Staff accept the partial payment.
    let out = r
        .svc
        .resolve(
            inv.id,
            ResolutionKind::MarkPaid,
            "paid the rest off-chain",
            9,
        )
        .await
        .unwrap();
    assert_eq!(out.invoice.state, InvoiceState::Resolved);
    assert_eq!(active_patron_grants(&r), 1);
    // Resolving twice is refused; polling a resolved invoice changes nothing.
    assert!(matches!(
        r.svc
            .resolve(inv.id, ResolutionKind::MarkPaid, "again", 9)
            .await,
        Err(PayError::Conflict(_))
    ));
    assert_eq!(r.poll(inv.id).await.state, InvoiceState::Resolved);
    assert_eq!(active_patron_grants(&r), 1);

    // A refund note on a paid invoice takes the credit away; it only records, it moves no funds.
    let paid = r.invoice(2, patron(1)).await;
    r.chain
        .pay_confirmed(&paid.address, "p", paid.sats as u64, 101);
    r.poll(paid.id).await;
    assert_eq!(active_patron_grants(&r), 2);
    let out = r
        .svc
        .resolve(
            paid.id,
            ResolutionKind::RefundNoted,
            "refunded from the operator wallet",
            9,
        )
        .await
        .unwrap();
    assert_eq!(
        out.invoice.resolution.unwrap().kind,
        ResolutionKind::RefundNoted
    );
    assert_eq!(active_patron_grants(&r), 1);
}

#[tokio::test]
async fn staff_grants_and_revocations_update_entitlements() {
    let r = Rig::new(20).await;
    let spec = crate::repo::GrantSpec {
        kind: GrantKind::Patron,
        user_id: Some(4),
        guild_id: None,
        days: Some(10),
        usd_cents: None,
    };
    let g = r
        .svc
        .admin_grant(spec, false, 9, "contest winner")
        .await
        .unwrap();
    assert!(r.svc.entitlements().user(4).await.unwrap().patron.active);
    r.svc.admin_revoke(g.id, 9, "mistake").await.unwrap();
    r.svc.entitlements().invalidate_user(4).await;
    assert!(
        !r.svc
            .entitlements()
            .user(4)
            .await
            .unwrap()
            .patron
            .perks_on()
    );
    assert!(matches!(
        r.svc.admin_revoke(g.id, 9, "again").await,
        Err(PayError::Conflict(_))
    ));
    assert_eq!(r.events.user.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn patron_expiry_events_fire_once_per_stage() {
    let r = Rig::new(20).await;
    let spec = crate::repo::GrantSpec {
        kind: GrantKind::Patron,
        user_id: Some(4),
        guild_id: None,
        days: Some(30),
        usd_cents: None,
    };
    r.svc.admin_grant(spec, false, 9, "test").await.unwrap();
    let base = r.events.user.lock().unwrap().len();
    r.svc.housekeeping().await.unwrap();
    assert_eq!(
        r.events.user.lock().unwrap().len(),
        base,
        "nothing to say yet"
    );
    r.advance(28 * DAY);
    r.svc.housekeeping().await.unwrap();
    r.svc.housekeeping().await.unwrap();
    assert_eq!(
        r.events.user.lock().unwrap().len(),
        base + 1,
        "one reminder"
    );
    assert_eq!(
        r.events.user.lock().unwrap().last().unwrap().1["reason"],
        "expiring"
    );
    r.advance(3 * DAY);
    r.svc.housekeeping().await.unwrap();
    assert_eq!(
        r.events.user.lock().unwrap().last().unwrap().1["reason"],
        "grace"
    );
    r.advance(4 * DAY);
    r.svc.housekeeping().await.unwrap();
    assert_eq!(
        r.events.user.lock().unwrap().last().unwrap().1["reason"],
        "lapsed"
    );
}
