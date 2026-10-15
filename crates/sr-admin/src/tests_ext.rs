//! Tests of the phase-10 service additions over the in-memory repository.

use crate::{
    AdminError, AdminService, SYSTEM_USER_ID,
    ext::*,
    memory::{MemoryAdminRepo, edit},
    model::*,
    repo::AdminRepository,
};
use async_trait::async_trait;
use sr_core::{Clock, ManualClock, PlatformPermissions as P};
use std::sync::{Arc, Mutex};

const OWNER: i64 = 10;
const SENIOR: i64 = 11;
const MODR: i64 = 12;
const SUPPORT: i64 = 13;
const NEWBIE: i64 = 20;
const OTHER: i64 = 21;

const R_OWNER: i64 = 1;
const R_SENIOR: i64 = 2;
const R_MOD: i64 = 3;
const R_SUPPORT: i64 = 4;
const R_READER: i64 = 5;

const T0: i64 = 1_800_000_000_000;
const DAY: i64 = 24 * 3600 * 1000;

#[derive(Default)]
struct Notes {
    offers: Mutex<Vec<StaffOffer>>,
    notices: Mutex<Vec<String>>,
    broadcasts: Mutex<Vec<Announcement>>,
}

#[async_trait]
impl StaffNotifier for Notes {
    async fn appointment_offer(&self, offer: &StaffOffer) -> Option<(i64, i64)> {
        self.offers.lock().unwrap().push(offer.clone());
        Some((7_000 + offer.id, 8_000 + offer.id))
    }
    async fn staff_notice(&self, text: &str) {
        self.notices.lock().unwrap().push(text.to_owned());
    }
    fn broadcast_announcement(&self, a: &Announcement) {
        self.broadcasts.lock().unwrap().push(a.clone());
    }
}

struct Env {
    svc: AdminService,
    repo: Arc<MemoryAdminRepo>,
    notes: Arc<Notes>,
    clock: Arc<ManualClock>,
}

fn env() -> Env {
    let clock = Arc::new(ManualClock::new(T0));
    let repo = Arc::new(MemoryAdminRepo::new(clock.clone()));
    repo.add_account(
        SYSTEM_USER_ID,
        "sideroom",
        AccountState::Deactivated,
        None,
        None,
    );
    for (id, name) in [
        (OWNER, "owner"),
        (SENIOR, "senior"),
        (MODR, "modr"),
        (SUPPORT, "support"),
        (NEWBIE, "newbie"),
        (OTHER, "other"),
    ] {
        repo.add_account(id, name, AccountState::Active, None, Some(OWNER));
    }
    repo.add_role(R_OWNER, "Owner", 7, P::OWNER);
    repo.add_role(
        R_SENIOR,
        "Senior Staff",
        6,
        P::all()
            .difference(P::OWNER)
            .difference(P::VIEW_ACCESS_LOG)
            .difference(P::OPERATOR),
    );
    repo.add_role(
        R_MOD,
        "Moderator",
        5,
        P::STAFF_BASIC | P::MANAGE_ACCOUNTS | P::PLATFORM_BAN,
    );
    repo.add_role(
        R_SUPPORT,
        "Support",
        1,
        P::STAFF_BASIC | P::MANAGE_ACCOUNTS | P::MANAGE_INVITES,
    );
    repo.add_role(
        R_READER,
        "Reader",
        4,
        P::STAFF_BASIC | P::READ_PRIVATE_CONTENT,
    );
    repo.set_owner(OWNER);
    let notes = Arc::new(Notes::default());
    let svc = AdminService::new(repo.clone())
        .with_extras(repo.clone())
        .with_notifier(notes.clone())
        .with_clock(clock.clone());
    Env {
        svc,
        repo,
        notes,
        clock,
    }
}

async fn owner(e: &Env) -> Caller {
    e.svc.resolve(OWNER, None).await.unwrap()
}

async fn appointed(e: &Env, id: i64, role: i64) -> Caller {
    e.svc
        .appoint(&owner(e).await, id, &[role], Some("test"))
        .await
        .unwrap();
    e.svc.resolve(id, None).await.unwrap()
}

// ---- ADM-STF-004: appointment offers ------------------------------------

#[tokio::test]
async fn an_offer_makes_nobody_staff_until_accepted() {
    let e = env();
    let o = owner(&e).await;
    let offer = e
        .svc
        .offer_appointment(&o, NEWBIE, &[R_MOD], Some("needs a hand"))
        .await
        .unwrap();
    assert_eq!(offer.state, OfferState::Pending);
    assert_eq!(offer.role_names, ["Moderator"]);
    // The system DM was sent and remembered.
    assert_eq!(e.notes.offers.lock().unwrap().len(), 1);
    assert_eq!(offer.dm_channel_id, Some(7_000 + offer.id));
    assert_eq!(
        e.svc.resolve(NEWBIE, None).await.unwrap_err(),
        AdminError::NotStaff
    );

    let closed = e
        .svc
        .respond_to_offer(NEWBIE, offer.id, true, None)
        .await
        .unwrap();
    assert_eq!(closed.state, OfferState::Accepted);
    let c = e.svc.resolve(NEWBIE, None).await.unwrap();
    assert!(c.permissions.has(P::PLATFORM_BAN));
    assert!(e.notes.notices.lock().unwrap()[0].contains("newbie joined the staff as Moderator"));
    // Both the offer and the acceptance are in the hash chain, the latter by the appointee.
    let rows = e.repo.log_rows();
    assert!(
        rows.iter()
            .any(|r| r.action == "staff.offer" && r.actor_id == OWNER)
    );
    assert!(
        rows.iter()
            .any(|r| r.action == "staff.offer_accept" && r.actor_id == NEWBIE)
    );
    assert!(e.repo.verify_log().await.unwrap().ok);
    // Answering twice is refused.
    assert!(matches!(
        e.svc.respond_to_offer(NEWBIE, offer.id, true, None).await,
        Err(AdminError::Conflict(_))
    ));
}

#[tokio::test]
async fn declining_leaves_the_account_alone_and_only_the_appointee_can_answer() {
    let e = env();
    let o = owner(&e).await;
    let offer = e
        .svc
        .offer_appointment(&o, NEWBIE, &[R_SUPPORT], Some("x"))
        .await
        .unwrap();
    // Someone else cannot answer, and cannot even tell the offer exists.
    assert_eq!(
        e.svc
            .respond_to_offer(OTHER, offer.id, true, None)
            .await
            .unwrap_err(),
        AdminError::NotFound("appointment offer")
    );
    let closed = e
        .svc
        .respond_to_offer(NEWBIE, offer.id, false, None)
        .await
        .unwrap();
    assert_eq!(closed.state, OfferState::Declined);
    assert_eq!(
        e.svc.resolve(NEWBIE, None).await.unwrap_err(),
        AdminError::NotStaff
    );
    // A fresh offer is possible again.
    assert!(
        e.svc
            .offer_appointment(&o, NEWBIE, &[R_SUPPORT], Some("again"))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn offers_expire_cancel_and_stay_unique() {
    let e = env();
    let o = owner(&e).await;
    let a = e
        .svc
        .offer_appointment(&o, NEWBIE, &[R_SUPPORT], Some("x"))
        .await
        .unwrap();
    // One pending offer per account.
    assert!(matches!(
        e.svc
            .offer_appointment(&o, NEWBIE, &[R_SUPPORT], Some("x"))
            .await,
        Err(AdminError::Conflict(_))
    ));
    // Cancelled offers cannot be accepted.
    e.svc
        .cancel_offer(&o, a.id, Some("changed my mind"))
        .await
        .unwrap();
    assert!(matches!(
        e.svc.respond_to_offer(NEWBIE, a.id, true, None).await,
        Err(AdminError::Conflict(_))
    ));
    // An unanswered offer expires after a week.
    let b = e
        .svc
        .offer_appointment(&o, NEWBIE, &[R_SUPPORT], Some("x"))
        .await
        .unwrap();
    e.clock.advance_ms(8 * DAY);
    assert!(matches!(
        e.svc.respond_to_offer(NEWBIE, b.id, true, None).await,
        Err(AdminError::Conflict(m)) if m.contains("expired")
    ));
    assert_eq!(
        e.svc.resolve(NEWBIE, None).await.unwrap_err(),
        AdminError::NotStaff
    );
}

#[tokio::test]
async fn accepting_rechecks_the_offerer_and_the_offer_rules() {
    let e = env();
    let o = owner(&e).await;
    let senior = appointed(&e, SENIOR, R_SENIOR).await;
    let offer = e
        .svc
        .offer_appointment(&senior, NEWBIE, &[R_SUPPORT], Some("x"))
        .await
        .unwrap();
    // The offerer is removed before the answer: the offer is void.
    e.svc.revoke_staff(&o, SENIOR, Some("gone")).await.unwrap();
    assert!(matches!(
        e.svc.respond_to_offer(NEWBIE, offer.id, true, None).await,
        Err(AdminError::Conflict(_))
    ));
    assert_eq!(
        e.svc.resolve(NEWBIE, None).await.unwrap_err(),
        AdminError::NotStaff
    );
    // Rules at offer time: the Owner role, yourself, the system user, existing staff.
    assert!(matches!(
        e.svc
            .offer_appointment(&o, NEWBIE, &[R_OWNER], Some("x"))
            .await,
        Err(AdminError::Forbidden(_))
    ));
    assert!(matches!(
        e.svc
            .offer_appointment(&o, OWNER, &[R_MOD], Some("x"))
            .await,
        Err(AdminError::Forbidden(_))
    ));
    assert!(matches!(
        e.svc
            .offer_appointment(&o, SYSTEM_USER_ID, &[R_MOD], Some("x"))
            .await,
        Err(AdminError::Forbidden(_))
    ));
    assert_eq!(
        e.svc
            .offer_appointment(&o, NEWBIE, &[R_MOD], None)
            .await
            .unwrap_err(),
        AdminError::ReasonRequired
    );
    // A moderator cannot offer at all.
    let m = appointed(&e, MODR, R_MOD).await;
    assert!(matches!(
        e.svc
            .offer_appointment(&m, NEWBIE, &[R_MOD], Some("x"))
            .await,
        Err(AdminError::MissingPermission("APPOINT_STAFF"))
    ));
}

// ---- delegations ----------------------------------------------------------

#[tokio::test]
async fn the_owner_delegates_appointment_rights_within_a_ceiling() {
    let e = env();
    let o = owner(&e).await;
    let m = appointed(&e, MODR, R_MOD).await;
    // Without a delegation a moderator cannot appoint.
    assert!(matches!(
        e.svc
            .offer_appointment(&m, NEWBIE, &[R_SUPPORT], Some("x"))
            .await,
        Err(AdminError::MissingPermission(_))
    ));
    // Only the Owner delegates.
    assert!(
        e.svc
            .delegate(&m, MODR, (P::STAFF_BASIC).bits(), Some("x"))
            .await
            .is_err()
    );
    // Owner-only bits cannot be delegated.
    assert!(matches!(
        e.svc
            .delegate(&o, MODR, P::READ_PRIVATE_CONTENT.bits(), Some("x"))
            .await,
        Err(AdminError::Invalid(_))
    ));
    let d = e
        .svc
        .delegate(
            &o,
            MODR,
            (P::STAFF_BASIC | P::MANAGE_ACCOUNTS).bits(),
            Some("trusted"),
        )
        .await
        .unwrap();
    assert_eq!(d.username, "modr");
    let m = e.svc.resolve(MODR, None).await.unwrap();
    assert!(m.permissions.has(P::APPOINT_STAFF));
    // Inside the ceiling...
    let offer = e
        .svc
        .offer_appointment(&m, NEWBIE, &[R_SUPPORT], Some("x"))
        .await;
    // ...Support holds MANAGE_INVITES, which the moderator lacks.
    assert!(matches!(offer, Err(AdminError::Forbidden(_))));
    e.repo
        .add_role(9, "Helper", 3, P::STAFF_BASIC | P::MANAGE_ACCOUNTS);
    assert!(
        e.svc
            .offer_appointment(&m, NEWBIE, &[9], Some("x"))
            .await
            .is_ok()
    );
    // PLATFORM_BAN is held by the moderator but outside the ceiling.
    e.repo
        .add_role(10, "Banner", 3, P::STAFF_BASIC | P::PLATFORM_BAN);
    assert!(matches!(
        e.svc.offer_appointment(&m, OTHER, &[10], Some("x")).await,
        Err(AdminError::Forbidden(m)) if m.contains("delegated")
    ));
    assert_eq!(e.svc.list_delegations(&o).await.unwrap().len(), 1);
    // Revoking the delegation, or the appointment, removes the right.
    e.svc.undelegate(&o, MODR, Some("done")).await.unwrap();
    let m = e.svc.resolve(MODR, None).await.unwrap();
    assert!(!m.permissions.has(P::APPOINT_STAFF));
    e.svc
        .delegate(&o, MODR, P::STAFF_BASIC.bits(), Some("again"))
        .await
        .unwrap();
    e.svc.revoke_staff(&o, MODR, Some("bye")).await.unwrap();
    assert!(e.svc.list_delegations(&o).await.unwrap().is_empty());
}

// ---- ADM-STF-005: leaving the Staff Server closes the appointment -----------

#[tokio::test]
async fn leaving_the_staff_server_ends_the_appointment_for_good() {
    let e = env();
    appointed(&e, MODR, R_MOD).await;
    e.repo.kick_from_staff_guild(MODR);
    assert_eq!(
        e.svc.resolve(MODR, None).await.unwrap_err(),
        AdminError::NotStaff
    );
    // Being put back into the Staff Server (an invite is impossible, but a member row
    // could reappear) does not bring the appointment back.
    e.repo.add_member_without_appointment(MODR, &[R_MOD]);
    assert_eq!(
        e.svc.resolve(MODR, None).await.unwrap_err(),
        AdminError::NotStaff
    );
}

// ---- ADM-PPL-003: account edits ----------------------------------------------

#[tokio::test]
async fn account_edits_validate_guard_and_log() {
    let e = env();
    let m = appointed(&e, MODR, R_MOD).await;
    let a = e
        .svc
        .rename_account(
            &m,
            NEWBIE,
            Some(" New.Name "),
            Some(Some("The Newbie")),
            Some("asked"),
        )
        .await
        .unwrap();
    assert_eq!(a.username, "new.name");
    assert_eq!(a.display_name.as_deref(), Some("The Newbie"));
    // Clearing the display name.
    let a = e
        .svc
        .rename_account(&m, NEWBIE, None, Some(Some("  ")), Some("asked"))
        .await
        .unwrap();
    assert_eq!(a.display_name, None);
    // Bad and taken names.
    for bad in [
        "A",
        "x y",
        "UPPER!",
        "way.too.long.way.too.long.way.too.long",
    ] {
        assert!(matches!(
            e.svc
                .rename_account(&m, NEWBIE, Some(bad), None, Some("r"))
                .await,
            Err(AdminError::Invalid(_))
        ));
    }
    assert!(matches!(
        e.svc
            .rename_account(&m, NEWBIE, Some("other"), None, Some("r"))
            .await,
        Err(AdminError::Conflict(_))
    ));
    assert_eq!(
        e.svc
            .rename_account(&m, NEWBIE, Some("fine"), None, None)
            .await
            .unwrap_err(),
        AdminError::ReasonRequired
    );
    // Email: shape, uniqueness, becomes unverified, the log keeps it masked.
    assert!(matches!(
        e.svc.change_email(&m, NEWBIE, "nope", Some("r")).await,
        Err(AdminError::Invalid(_))
    ));
    let a = e
        .svc
        .change_email(&m, NEWBIE, "Fresh@Example.Test", Some("lost access"))
        .await
        .unwrap();
    assert_eq!(a.email, "fresh@example.test");
    assert!(!a.email_verified);
    let row = e
        .repo
        .log_rows()
        .into_iter()
        .find(|r| r.action == "account.change_email")
        .unwrap();
    assert!(row.detail.contains("f***@example.test") && !row.detail.contains("fresh@"));
    assert!(matches!(
        e.svc
            .change_email(&m, NEWBIE, "other@example.test", Some("r"))
            .await,
        Err(AdminError::Conflict(_))
    ));
    // Badges.
    let a = e
        .svc
        .set_badges(
            &m,
            NEWBIE,
            &["early_member".into(), "contributor".into()],
            &[],
            Some("thanks"),
        )
        .await
        .unwrap();
    assert_eq!(a.flags, (1 << 8) | (1 << 9));
    let a = e
        .svc
        .set_badges(&m, NEWBIE, &[], &["early_member".into()], Some("oops"))
        .await
        .unwrap();
    assert_eq!(a.flags, 1 << 9);
    assert!(matches!(
        e.svc
            .set_badges(&m, NEWBIE, &["staff".into()], &[], Some("r"))
            .await,
        Err(AdminError::Invalid(_))
    ));
    assert!(matches!(
        e.svc
            .set_badges(
                &m,
                NEWBIE,
                &["honorary".into()],
                &["honorary".into()],
                Some("r")
            )
            .await,
        Err(AdminError::Invalid(_))
    ));
    // Forced password reset ends sessions and flags the account.
    assert_eq!(e.repo.sessions_of(NEWBIE), 2);
    let n = e
        .svc
        .force_password_reset(&m, NEWBIE, Some("compromised"))
        .await
        .unwrap();
    assert_eq!(n, 2);
    assert_eq!(e.repo.sessions_of(NEWBIE), 0);
    assert!(e.repo.password_reset_forced(NEWBIE));
    // Guards: not the system user, not yourself, not staff unless Owner.
    assert!(matches!(
        e.svc
            .force_password_reset(&m, SYSTEM_USER_ID, Some("r"))
            .await,
        Err(AdminError::Forbidden(_))
    ));
    assert!(matches!(
        e.svc.force_password_reset(&m, MODR, Some("r")).await,
        Err(AdminError::Forbidden(_))
    ));
    let s = appointed(&e, SUPPORT, R_SUPPORT).await;
    assert!(matches!(
        e.svc
            .rename_account(&s, MODR, Some("zzz"), None, Some("r"))
            .await,
        Err(AdminError::Forbidden(_))
    ));
    // A support agent has MANAGE_ACCOUNTS; a reader does not.
    let reader = appointed(&e, OTHER, R_READER).await;
    assert!(matches!(
        e.svc
            .rename_account(&reader, NEWBIE, Some("zzz"), None, Some("r"))
            .await,
        Err(AdminError::MissingPermission("MANAGE_ACCOUNTS"))
    ));
    assert!(e.repo.verify_log().await.unwrap().ok);
}

// ---- ADM-DASH-002: cached stats ------------------------------------------------

#[tokio::test]
async fn stats_are_cached_for_thirty_seconds() {
    let e = env();
    let o = owner(&e).await;
    let first = e.svc.stats(&o).await.unwrap();
    assert_eq!(first.accounts_cap, 20_000);
    e.repo
        .add_account(99, "late", AccountState::Active, None, None);
    // Still the cached numbers.
    assert_eq!(
        e.svc.stats(&o).await.unwrap().accounts_active,
        first.accounts_active
    );
    e.clock.advance_ms(29_000);
    assert_eq!(
        e.svc.stats(&o).await.unwrap().accounts_active,
        first.accounts_active
    );
    e.clock.advance_ms(2_000);
    assert_eq!(
        e.svc.stats(&o).await.unwrap().accounts_active,
        first.accounts_active + 1
    );
    // Permission is checked before the cache is consulted.
    let nobody = Caller {
        user_id: OTHER,
        permissions: P::empty(),
        is_owner: false,
        ip: None,
    };
    assert!(e.svc.stats(&nobody).await.is_err());
}

// ---- ADM-COM-004: private content ------------------------------------------------

fn pm(id: i64, content: &str, vis: &str, deleted_at: Option<i64>) -> PrivateMessage {
    PrivateMessage {
        id,
        author_id: NEWBIE,
        author_name: "newbie".into(),
        content: content.into(),
        created_at_ms: T0,
        edited_at_ms: None,
        visibility: vis.into(),
        deleted_at_ms: deleted_at,
        attachment_count: 0,
        edits: vec![],
    }
}

fn seed_dm(e: &Env) {
    e.repo.add_private_channel(PrivateChannel {
        id: 500,
        kind: 1,
        guild_id: None,
        name: None,
        recipients: vec![
            AccountRef {
                id: NEWBIE,
                username: "newbie".into(),
            },
            AccountRef {
                id: OTHER,
                username: "other".into(),
            },
        ],
        private: true,
    });
    e.repo
        .add_private_message(500, pm(1, "hello", "visible", None));
    let mut edited = pm(2, "after", "visible", None);
    edited.edited_at_ms = Some(T0 + 1);
    edited.edits = vec![edit(T0 + 1, "before")];
    e.repo.add_private_message(500, edited);
    e.repo
        .add_private_message(500, pm(3, "oops", "user_deleted", Some(T0 - 5 * DAY)));
    e.repo
        .add_private_message(500, pm(4, "ancient", "user_deleted", Some(T0 - 40 * DAY)));
}

#[tokio::test]
async fn reading_private_content_needs_the_permission_and_a_reason_and_is_logged_first() {
    let e = env();
    seed_dm(&e);
    let m = appointed(&e, MODR, R_MOD).await;
    // A moderator may not read, and the refusal is logged.
    assert!(matches!(
        e.svc
            .read_private_messages(&m, 500, None, None, Some("r"))
            .await,
        Err(AdminError::MissingPermission("READ_PRIVATE_CONTENT"))
    ));
    assert!(
        e.repo
            .log_rows()
            .iter()
            .any(|r| r.kind == AccessKind::Denied)
    );

    let o = owner(&e).await;
    let rows_before = e.repo.log_rows().len();
    assert_eq!(
        e.svc
            .read_private_messages(&o, 500, None, None, None)
            .await
            .unwrap_err(),
        AdminError::ReasonRequired
    );
    assert_eq!(
        e.repo.log_rows().len(),
        rows_before,
        "no reason, nothing to log"
    );

    let page = e
        .svc
        .read_private_messages(&o, 500, None, None, Some("abuse report #12"))
        .await
        .unwrap();
    assert!(page.channel.private);
    // Newest first; the 5-day-old deletion is visible, the 40-day-old one is gone.
    let ids: Vec<_> = page.items.iter().map(|m| m.id).collect();
    assert_eq!(ids, [3, 2, 1]);
    assert_eq!(page.deleted_retention_days, 30);
    assert_eq!(page.items[1].edits[0].previous_content, "before");
    // The request is in the chain with its reason.
    let row = e
        .repo
        .log_rows()
        .into_iter()
        .rfind(|r| r.kind == AccessKind::PrivateRead)
        .unwrap();
    assert_eq!(row.action, "private.read");
    assert_eq!(row.target_id, Some(500));
    assert_eq!(row.reason, "abuse report #12");
    assert_eq!(row.actor_id, OWNER);

    // Paging.
    let p1 = e
        .svc
        .read_private_messages(&o, 500, None, Some(2), Some("r"))
        .await
        .unwrap();
    assert_eq!(p1.items.len(), 2);
    let before: i64 = p1.next_before.unwrap().parse().unwrap();
    let p2 = e
        .svc
        .read_private_messages(&o, 500, Some(before), Some(2), Some("r"))
        .await
        .unwrap();
    assert_eq!(p2.items.iter().map(|m| m.id).collect::<Vec<_>>(), [1]);
    assert!(p2.next_before.is_none());

    // An unknown channel is a 404 but the attempt is still logged.
    let n = e.repo.log_rows().len();
    assert_eq!(
        e.svc
            .read_private_messages(&o, 999, None, None, Some("r"))
            .await
            .unwrap_err(),
        AdminError::NotFound("channel")
    );
    assert_eq!(e.repo.log_rows().len(), n + 1);
    assert!(e.repo.verify_log().await.unwrap().ok);
}

#[tokio::test]
async fn listing_private_conversations_is_logged_too() {
    let e = env();
    seed_dm(&e);
    let reader = appointed(&e, OTHER, R_READER).await;
    // READ_PRIVATE_CONTENT is Owner-only to grant, but a reader role exists in this fixture.
    let list = e
        .svc
        .private_dm_channels(&reader, NEWBIE, Some("investigation"))
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    let row = e.repo.log_rows().pop().unwrap();
    assert_eq!(
        (row.kind, row.action.as_str(), row.target_id),
        (AccessKind::PrivateRead, "private.list_dms", Some(NEWBIE))
    );
    assert_eq!(
        e.svc
            .private_dm_channels(&reader, NEWBIE, None)
            .await
            .unwrap_err(),
        AdminError::ReasonRequired
    );
    assert!(
        e.svc
            .private_guild_channels(&reader, 77, Some("r"))
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        e.repo.log_rows().pop().unwrap().action,
        "private.list_channels"
    );
}

// ---- ADM-PLT-009: announcements -----------------------------------------------------

#[tokio::test]
async fn announcements_show_between_their_times_and_can_be_ended() {
    let e = env();
    let o = owner(&e).await;
    let m = appointed(&e, MODR, R_MOD).await;
    let input = |title: &str| AnnouncementInput {
        title: title.into(),
        body: "Maintenance tonight at 22:00 UTC.".into(),
        severity: Some(AnnouncementSeverity::Warning),
        critical: false,
        send_dm: true,
        starts_at_ms: None,
        ends_at_ms: Some(T0 + DAY),
    };
    assert!(matches!(
        e.svc.announce(&m, input("x"), None).await,
        Err(AdminError::MissingPermission("MANAGE_FEATURES"))
    ));
    assert!(matches!(
        e.svc.announce(&o, input(""), None).await,
        Err(AdminError::Invalid(_))
    ));
    let mut bad = input("t");
    bad.ends_at_ms = Some(T0 - 1);
    assert!(matches!(
        e.svc.announce(&o, bad, None).await,
        Err(AdminError::Invalid(_))
    ));
    let a = e.svc.announce(&o, input("Downtime"), None).await.unwrap();
    assert_eq!(a.severity, AnnouncementSeverity::Warning);
    // The DM broadcast was requested because send_dm is set.
    assert_eq!(e.notes.broadcasts.lock().unwrap().len(), 1);
    assert_eq!(e.svc.active_announcements().await.unwrap().len(), 1);
    // A banner without DM does not broadcast.
    let mut quiet = input("Quiet");
    quiet.send_dm = false;
    e.svc.announce(&o, quiet, None).await.unwrap();
    assert_eq!(e.notes.broadcasts.lock().unwrap().len(), 1);
    assert_eq!(e.svc.active_announcements().await.unwrap().len(), 2);
    // D-192: a critical notice is always sent as a DM, even without `send_dm`.
    let mut critical = input("Outage");
    critical.send_dm = false;
    critical.critical = true;
    critical.severity = None;
    let c = e.svc.announce(&o, critical, None).await.unwrap();
    assert_eq!(c.severity, AnnouncementSeverity::Critical);
    assert!(c.send_dm);
    assert_eq!(e.notes.broadcasts.lock().unwrap().len(), 2);
    e.svc.end_announcement(&o, c.id, None).await.unwrap();
    // Ending one early.
    e.svc
        .end_announcement(&o, a.id, Some("done"))
        .await
        .unwrap();
    assert_eq!(e.svc.active_announcements().await.unwrap().len(), 1);
    // They lapse by themselves.
    e.clock.advance_ms(2 * DAY);
    assert!(e.svc.active_announcements().await.unwrap().is_empty());
    assert_eq!(e.svc.list_announcements(&o).await.unwrap().len(), 3);
}

// ---- ADM-ALR: alerts and card actions -------------------------------------------------

#[tokio::test]
async fn alerts_are_stored_acked_and_counted() {
    let e = env();
    let m = appointed(&e, MODR, R_MOD).await;
    let a = e
        .svc
        .record_alert("disk_low", "Disk 91% full")
        .await
        .unwrap();
    e.svc
        .record_alert("backup_failed", "No backup in 3 days")
        .await
        .unwrap();
    assert_eq!(e.svc.recent_alerts(&m, None).await.unwrap().len(), 2);
    assert_eq!(e.repo.counts().await.unwrap().alerts_unacked, 2);
    let acked = e.svc.ack_alert(&m, a.id).await.unwrap();
    assert_eq!(acked.acked_by, Some(MODR));
    assert_eq!(e.repo.counts().await.unwrap().alerts_unacked, 1);
    assert!(e.repo.log_rows().iter().any(|r| r.action == "alert.ack"));
}

#[tokio::test]
async fn card_actions_check_the_permission_of_the_action_behind_them() {
    let e = env();
    let o = owner(&e).await;
    let m = appointed(&e, MODR, R_MOD).await;
    let alert = e
        .svc
        .record_alert("transcode_failed", "job 9 died")
        .await
        .unwrap();
    let r = e
        .svc
        .card_action(&m, &format!("sr:ack:{}", alert.id))
        .await
        .unwrap();
    assert!(r.ok);
    // A moderator may not retry transcode jobs.
    let job = e.repo.add_job("transcode", "dead");
    assert!(matches!(
        e.svc.card_action(&m, &format!("sr:retry-job:{job}")).await,
        Err(AdminError::MissingPermission("MANAGE_TRANSCODE"))
    ));
    assert!(
        e.svc
            .card_action(&o, &format!("sr:retry-job:{job}"))
            .await
            .unwrap()
            .ok
    );
    // Garbage is refused.
    for bad in ["x", "sr:ack", "sr:ack:abc", "sr:launch:1", "other:ack:1"] {
        assert!(matches!(
            e.svc.card_action(&o, bad).await,
            Err(AdminError::Invalid(_))
        ));
    }
}

#[test]
fn username_and_email_shapes() {
    use crate::service_ext::{valid_email, valid_username};
    assert!(valid_username("a.b_c9"));
    assert!(!valid_username("a"));
    assert!(!valid_username("Ab"));
    assert!(!valid_username("a b"));
    assert!(valid_email("a@b.co"));
    assert!(!valid_email("a@b"));
    assert!(!valid_email("@b.co"));
    assert!(!valid_email("a b@c.co"));
    assert!(!valid_email("a@@b.co"));
    let _ = ManualClock::new(0).now_unix_ms();
}

// ---- G-69: editing an active announcement ---------------------------------------------

#[tokio::test]
async fn an_active_announcement_can_be_edited_but_not_an_ended_one() {
    use crate::ext::AnnouncementPatch;
    let e = env();
    let o = owner(&e).await;
    let m = appointed(&e, MODR, R_MOD).await;
    let a = e
        .svc
        .announce(
            &o,
            AnnouncementInput {
                title: "Downtime".into(),
                body: "Tonight.".into(),
                severity: Some(AnnouncementSeverity::Warning),
                critical: false,
                send_dm: true,
                starts_at_ms: None,
                ends_at_ms: Some(T0 + DAY),
            },
            None,
        )
        .await
        .unwrap();
    let patch = |title: Option<&str>, body: Option<&str>, ends: Option<i64>| AnnouncementPatch {
        title: title.map(str::to_owned),
        body: body.map(str::to_owned),
        ends_at_ms: ends,
    };
    // Needs the permission, a change, and valid values.
    assert!(matches!(
        e.svc
            .edit_announcement(&m, a.id, patch(Some("x"), None, None), None)
            .await,
        Err(AdminError::MissingPermission("MANAGE_FEATURES"))
    ));
    for bad in [
        patch(None, None, None),
        patch(Some(" "), None, None),
        patch(None, None, Some(T0 - 1)),
    ] {
        assert!(matches!(
            e.svc.edit_announcement(&o, a.id, bad, None).await,
            Err(AdminError::Invalid(_))
        ));
    }
    let edited = e
        .svc
        .edit_announcement(
            &o,
            a.id,
            patch(
                Some("Downtime moved"),
                Some("Tomorrow, 22:00."),
                Some(T0 + 2 * DAY),
            ),
            None,
        )
        .await
        .unwrap();
    assert_eq!(edited.title, "Downtime moved");
    assert_eq!(edited.body, "Tomorrow, 22:00.");
    assert_eq!(edited.ends_at_ms, Some(T0 + 2 * DAY));
    // Severity and the DM flag are unchanged, and no DM went out again.
    assert_eq!(edited.severity, AnnouncementSeverity::Warning);
    assert!(edited.send_dm);
    assert_eq!(e.notes.broadcasts.lock().unwrap().len(), 1);
    let log = e.repo.log_rows();
    let row = log
        .iter()
        .rfind(|r| r.action == "announcement.edit")
        .expect("the edit is audited");
    assert!(row.detail.contains("Downtime moved") && row.detail.contains("diff"), "{}", row.detail);
    // Once ended, an edit is a conflict.
    e.svc.end_announcement(&o, a.id, None).await.unwrap();
    assert!(matches!(
        e.svc
            .edit_announcement(&o, a.id, patch(Some("late"), None, None), None)
            .await,
        Err(AdminError::Conflict(_))
    ));
    assert!(matches!(
        e.svc
            .edit_announcement(&o, a.id + 999, patch(Some("x"), None, None), None)
            .await,
        Err(AdminError::NotFound("announcement"))
    ));
}
