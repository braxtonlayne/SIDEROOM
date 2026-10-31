//! Service tests over the in-memory repository.

use crate::{
    AdminError, AdminService, SYSTEM_USER_ID, memory::MemoryAdminRepo, model::*,
    repo::AdminRepository,
};
use futures::StreamExt;
use serde_json::json;
use sr_core::PlatformPermissions as P;
use sr_events::{DomainEvent, EventBus, InProcessBus, StaffAlertKind, Topic};
use std::sync::Arc;

const OWNER: i64 = 10;
const MOD: i64 = 11;
const SUPPORT: i64 = 12;
const ALICE: i64 = 20; // invited by OWNER
const BOB: i64 = 21; // invited by ALICE
const CAROL: i64 = 22; // invited by ALICE
const DAVE: i64 = 23; // invited by BOB
const OUTSIDER: i64 = 30; // plain member, never staff

const R_OWNER: i64 = 1;
const R_SENIOR: i64 = 2;
const R_MOD: i64 = 3;
const R_SUPPORT: i64 = 4;
const R_LOGGER: i64 = 5;

struct Env {
    svc: AdminService,
    repo: Arc<MemoryAdminRepo>,
    bus: Arc<InProcessBus>,
}

fn env() -> Env {
    let repo = Arc::new(MemoryAdminRepo::default());
    repo.add_account(
        SYSTEM_USER_ID,
        "sideroom",
        AccountState::Deactivated,
        None,
        None,
    );
    repo.add_account(OWNER, "owner", AccountState::Active, None, None);
    for (id, name, by) in [
        (MOD, "mod", OWNER),
        (SUPPORT, "support", OWNER),
        (ALICE, "alice", OWNER),
        (BOB, "bob", ALICE),
        (CAROL, "carol", ALICE),
        (DAVE, "dave", BOB),
        (OUTSIDER, "outsider", OWNER),
    ] {
        repo.add_account(id, name, AccountState::Active, None, Some(by));
    }
    let senior = crate::rules::default_staff_roles()
        .into_iter()
        .find(|r| r.name == "Senior Staff")
        .unwrap();
    repo.add_role(R_OWNER, "Owner", 7, P::OWNER);
    repo.add_role(R_SENIOR, "Senior Staff", 6, senior.permissions);
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
    repo.add_role(R_LOGGER, "Logger", 2, P::STAFF_BASIC | P::VIEW_ACCESS_LOG);
    repo.set_owner(OWNER);
    let bus = Arc::new(InProcessBus::new());
    let svc = AdminService::new(repo.clone()).with_bus(bus.clone());
    Env { svc, repo, bus }
}

async fn appointed(e: &Env, id: i64, role: i64) -> Caller {
    let owner = e.svc.resolve(OWNER, None).await.unwrap();
    e.svc
        .appoint(&owner, id, &[role], Some("test"))
        .await
        .unwrap();
    e.svc.resolve(id, None).await.unwrap()
}

#[tokio::test]
async fn standing_requires_appointment_and_membership() {
    let e = env();
    // The Staff Server owner is the Owner without any appointment row.
    let o = e.svc.resolve(OWNER, None).await.unwrap();
    assert!(o.is_owner && o.permissions.contains(P::OWNER));
    // A plain account is not staff.
    assert_eq!(
        e.svc.resolve(OUTSIDER, None).await.unwrap_err(),
        AdminError::NotStaff
    );
    // A Staff Server role without an appointment has no platform effect.
    e.repo.add_member_without_appointment(OUTSIDER, &[R_SENIOR]);
    assert_eq!(
        e.svc.resolve(OUTSIDER, None).await.unwrap_err(),
        AdminError::NotStaff
    );
    // Appointed: permissions are the roles' union.
    let m = appointed(&e, MOD, R_MOD).await;
    assert!(m.permissions.has(P::PLATFORM_BAN) && !m.permissions.has(P::MANAGE_FEATURES));
    // Leaving or being kicked from the Staff Server ends access on the next request.
    e.repo.kick_from_staff_guild(MOD);
    assert_eq!(
        e.svc.resolve(MOD, None).await.unwrap_err(),
        AdminError::NotStaff
    );
}

#[tokio::test]
async fn permission_matrix_for_a_moderator() {
    let e = env();
    let m = appointed(&e, MOD, R_MOD).await;
    // Allowed.
    assert!(
        e.svc
            .search_accounts(&m, AccountQuery::default(), None)
            .await
            .is_ok()
    );
    assert!(e.svc.account_detail(&m, ALICE).await.is_ok());
    assert!(e.svc.stats(&m).await.is_ok());
    // Refused: each names the missing bit.
    let miss = |r: Result<_, AdminError>, bit: &str| {
        assert_eq!(
            r.err(),
            Some(AdminError::MissingPermission(match bit {
                "MANAGE_INVITES" => "MANAGE_INVITES",
                "MANAGE_FEATURES" => "MANAGE_FEATURES",
                "VIEW_ACCESS_LOG" => "VIEW_ACCESS_LOG",
                "APPOINT_STAFF" => "APPOINT_STAFF",
                "OPERATOR" => "OPERATOR",
                "MANAGE_TRANSCODE" => "MANAGE_TRANSCODE",
                _ => unreachable!(),
            }))
        );
    };
    miss(
        e.svc.invite_tree(&m, ALICE).await.map(|_| ()),
        "MANAGE_INVITES",
    );
    miss(e.svc.settings_view(&m).await.map(|_| ()), "MANAGE_FEATURES");
    miss(
        e.svc
            .access_log(&m, AccessLogFilter::default(), None)
            .await
            .map(|_| ()),
        "VIEW_ACCESS_LOG",
    );
    miss(e.svc.list_staff(&m).await.map(|_| ()), "APPOINT_STAFF");
    miss(e.svc.system_counts(&m).await.map(|_| ()), "OPERATOR");
    miss(
        e.svc.list_jobs(&m, None, None, None).await.map(|_| ()),
        "MANAGE_TRANSCODE",
    );
    miss(
        e.svc.verify_access_log(&m).await.map(|_| ()),
        "VIEW_ACCESS_LOG",
    );
    // A denial by a staff member is itself logged.
    let denied = e
        .repo
        .log_rows()
        .iter()
        .filter(|r| r.kind == AccessKind::Denied && r.actor_id == MOD)
        .count();
    assert!(denied >= 6);
}

#[tokio::test]
async fn appointment_rules() {
    let e = env();
    let owner = e.svc.resolve(OWNER, None).await.unwrap();
    // A reason is required.
    assert_eq!(
        e.svc
            .appoint(&owner, ALICE, &[R_MOD], None)
            .await
            .unwrap_err(),
        AdminError::ReasonRequired
    );
    // The Owner role cannot be handed out.
    assert!(matches!(
        e.svc.appoint(&owner, ALICE, &[R_OWNER], Some("x")).await,
        Err(AdminError::Forbidden(_))
    ));
    // Only active accounts; not the system user; not twice.
    assert!(matches!(
        e.svc
            .appoint(&owner, SYSTEM_USER_ID, &[R_MOD], Some("x"))
            .await,
        Err(AdminError::Forbidden(_))
    ));
    let m = e
        .svc
        .appoint(&owner, MOD, &[R_MOD], Some("trusted"))
        .await
        .unwrap();
    assert_eq!(m.roles.len(), 1);
    assert!(matches!(
        e.svc.appoint(&owner, MOD, &[R_MOD], Some("x")).await,
        Err(AdminError::Conflict(_))
    ));
    // A delegate with APPOINT_STAFF (a custom role) can appoint within its bits only,
    // and never an owner-only role such as Senior Staff.
    e.repo.add_role(
        6,
        "Delegate",
        3,
        P::STAFF_BASIC | P::MANAGE_ACCOUNTS | P::APPOINT_STAFF,
    );
    let d = appointed(&e, SUPPORT, 6).await;
    assert!(
        e.svc
            .appoint(&d, ALICE, &[R_SUPPORT], Some("x"))
            .await
            .is_err()
    ); // MANAGE_INVITES not held
    assert!(matches!(
        e.svc.appoint(&d, ALICE, &[R_SENIOR], Some("x")).await,
        Err(AdminError::Forbidden(_))
    ));
    e.repo
        .add_role(7, "Helper", 1, P::STAFF_BASIC | P::MANAGE_ACCOUNTS);
    assert!(e.svc.appoint(&d, ALICE, &[7], Some("x")).await.is_ok());
    // The delegate cannot revoke the moderator (PLATFORM_BAN is beyond it) nor change its own roles.
    assert!(matches!(
        e.svc.revoke_staff(&d, MOD, Some("x")).await,
        Err(AdminError::Forbidden(_))
    ));
    assert!(matches!(
        e.svc.set_staff_roles(&d, SUPPORT, &[7], Some("x")).await,
        Err(AdminError::Forbidden(_))
    ));
    // The Owner is immutable, even for the Owner.
    assert!(matches!(
        e.svc.revoke_staff(&owner, OWNER, Some("x")).await,
        Err(AdminError::Forbidden(_))
    ));
    // The Owner can revoke; the account leaves staff immediately.
    e.svc.revoke_staff(&owner, MOD, Some("left")).await.unwrap();
    assert_eq!(
        e.svc.resolve(MOD, None).await.unwrap_err(),
        AdminError::NotStaff
    );
}

#[tokio::test]
async fn a_non_owner_cannot_hold_or_grant_the_owner_only_bits() {
    let e = env();
    let owner = e.svc.resolve(OWNER, None).await.unwrap();
    e.svc
        .appoint(&owner, MOD, &[R_SENIOR], Some("senior"))
        .await
        .unwrap();
    let senior = e.svc.resolve(MOD, None).await.unwrap();
    // Senior Staff holds APPOINT_STAFF and READ_PRIVATE_CONTENT but cannot hand either on.
    assert!(matches!(
        e.svc.appoint(&senior, ALICE, &[R_SENIOR], Some("x")).await,
        Err(AdminError::Forbidden(_))
    ));
    assert!(
        e.svc
            .appoint(&senior, ALICE, &[R_MOD], Some("x"))
            .await
            .is_ok()
    );
    // And the access log stays Owner-only unless the Owner grants VIEW_ACCESS_LOG.
    assert!(matches!(
        e.svc
            .access_log(&senior, AccessLogFilter::default(), None)
            .await,
        Err(AdminError::MissingPermission("VIEW_ACCESS_LOG"))
    ));
}

#[tokio::test]
async fn suspend_unsuspend_and_force_logout() {
    let e = env();
    let m = appointed(&e, MOD, R_MOD).await;
    assert_eq!(e.repo.sessions_of(ALICE), 2);
    // Reason first.
    assert_eq!(
        e.svc
            .set_suspended(&m, ALICE, true, None)
            .await
            .unwrap_err(),
        AdminError::ReasonRequired
    );
    let s = e
        .svc
        .set_suspended(&m, ALICE, true, Some("spam"))
        .await
        .unwrap();
    assert_eq!(s.state, AccountState::Deactivated);
    assert_eq!(
        e.repo.sessions_of(ALICE),
        0,
        "sessions die with the suspension"
    );
    // Suspending twice conflicts; unsuspending restores.
    assert!(matches!(
        e.svc.set_suspended(&m, ALICE, true, Some("again")).await,
        Err(AdminError::Conflict(_))
    ));
    let s = e
        .svc
        .set_suspended(&m, ALICE, false, Some("sorry"))
        .await
        .unwrap();
    assert_eq!(s.state, AccountState::Active);
    // Unsuspension respects the cap setting.
    e.svc.set_suspended(&m, BOB, true, Some("x")).await.unwrap();
    let owner = e.svc.resolve(OWNER, None).await.unwrap();
    e.svc
        .patch_settings(&owner, &[("accounts.cap".into(), json!(1))], None)
        .await
        .unwrap();
    assert!(matches!(
        e.svc.set_suspended(&m, BOB, false, Some("x")).await,
        Err(AdminError::Conflict(_))
    ));
    // Force logout reports the sessions it ended.
    assert_eq!(e.repo.sessions_of(CAROL), 2);
    assert_eq!(
        e.svc
            .force_logout(&m, CAROL, Some("lost phone"))
            .await
            .unwrap(),
        2
    );
    assert_eq!(e.repo.sessions_of(CAROL), 0);
    // Not oneself, not the system user, not staff (unless Owner).
    assert!(matches!(
        e.svc.set_suspended(&m, MOD, true, Some("x")).await,
        Err(AdminError::Forbidden(_))
    ));
    assert!(matches!(
        e.svc
            .set_suspended(&m, SYSTEM_USER_ID, true, Some("x"))
            .await,
        Err(AdminError::Forbidden(_))
    ));
    e.svc
        .appoint(&owner, SUPPORT, &[R_SUPPORT], Some("x"))
        .await
        .unwrap();
    assert!(matches!(
        e.svc.set_suspended(&m, SUPPORT, true, Some("x")).await,
        Err(AdminError::Forbidden(_))
    ));
    assert!(
        e.svc
            .set_suspended(&owner, SUPPORT, true, Some("x"))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn reset_2fa_and_delete_request_are_logged() {
    let e = env();
    let m = appointed(&e, MOD, R_MOD).await;
    let r = e
        .svc
        .reset_2fa(&m, ALICE, Some("locked out"))
        .await
        .unwrap();
    assert_eq!(r.totp, 1);
    let d = e
        .svc
        .request_delete(&m, ALICE, Some("asked by email"))
        .await
        .unwrap();
    assert_eq!(d.state, "pending");
    let actions: Vec<_> = e.repo.log_rows().into_iter().map(|r| r.action).collect();
    assert!(actions.contains(&"account.reset_2fa".to_owned()));
    assert!(actions.contains(&"account.delete_request".to_owned()));
}

#[tokio::test]
async fn serious_ban_raises_inviter_suspicion_and_standard_does_not() {
    let e = env();
    let m = appointed(&e, MOD, R_MOD).await;
    let mut events = e.bus.subscribe(Topic::Platform);
    let person = |id: i64| async move { id + 500_000 };

    // Standard ban of BOB: banned, sessions killed, no propagation.
    let out = e
        .svc
        .ban_person(
            &m,
            person(BOB).await,
            Some(BOB),
            BanCategory::Standard,
            Some("rude"),
        )
        .await
        .unwrap();
    assert_eq!(out.accounts_banned, 1);
    assert_eq!(out.sessions_revoked, 2);
    assert!(out.inviter_suspicion.is_none());
    assert_eq!(e.repo.state_of(BOB), Some(AccountState::PlatformBanned));
    // Banning again conflicts.
    assert!(matches!(
        e.svc
            .ban_person(
                &m,
                person(BOB).await,
                Some(BOB),
                BanCategory::Standard,
                Some("x")
            )
            .await,
        Err(AdminError::Conflict(_))
    ));

    // Serious ban of CAROL (also invited by ALICE): ALICE's Person gains suspicion 1.
    let out = e
        .svc
        .ban_person(
            &m,
            person(CAROL).await,
            Some(CAROL),
            BanCategory::Serious,
            Some("fraud"),
        )
        .await
        .unwrap();
    let s = out.inviter_suspicion.unwrap();
    assert_eq!(
        (s.inviter_account_id, s.level, s.frozen, s.escalated),
        (ALICE, 1, false, false)
    );

    // A second serious ban of DAVE (invited by BOB, whose Person is banned): BOB's Person level 1.
    // Use a fresh account under ALICE for the second strike.
    e.repo
        .add_account(24, "erin", AccountState::Active, None, Some(ALICE));
    let out = e
        .svc
        .ban_person(
            &m,
            24 + 500_000,
            Some(24),
            BanCategory::Serious,
            Some("fraud"),
        )
        .await
        .unwrap();
    let s = out.inviter_suspicion.unwrap();
    assert_eq!(
        (s.level, s.frozen, s.escalated),
        (2, true, false),
        "freeze at 2"
    );
    e.repo
        .add_account(25, "frank", AccountState::Active, None, Some(ALICE));
    let s = e
        .svc
        .ban_person(
            &m,
            25 + 500_000,
            Some(25),
            BanCategory::Serious,
            Some("fraud"),
        )
        .await
        .unwrap()
        .inviter_suspicion
        .unwrap();
    assert_eq!(
        (s.level, s.frozen, s.escalated),
        (3, true, true),
        "escalate at 3"
    );

    // Alerts were published for staff.
    let mut kinds = Vec::new();
    while let Ok(Some(ev)) =
        tokio::time::timeout(std::time::Duration::from_millis(50), events.next()).await
    {
        if let DomainEvent::StaffAlert { kind, .. } = ev {
            kinds.push(kind);
        }
    }
    assert!(kinds.contains(&StaffAlertKind::InviterSuspicion));
    assert!(kinds.contains(&StaffAlertKind::InviterEscalated));

    // Suspects list (needs MANAGE_INVITES), then clearing needs a reason.
    let owner = e.svc.resolve(OWNER, None).await.unwrap();
    let suspects = e.svc.list_suspects(&owner).await.unwrap();
    assert_eq!(suspects[0].suspicion_level, 3);
    assert!(suspects[0].frozen);
    assert_eq!(
        e.svc
            .clear_suspicion(&owner, ALICE + 500_000, None)
            .await
            .unwrap_err(),
        AdminError::ReasonRequired
    );
    e.svc
        .clear_suspicion(&owner, ALICE + 500_000, Some("reviewed"))
        .await
        .unwrap();
    assert!(e.svc.list_suspects(&owner).await.unwrap().is_empty());

    // Unban brings the account back.
    let u = e
        .svc
        .unban_person(&m, person(BOB).await, Some("appealed"))
        .await
        .unwrap();
    assert_eq!(u.accounts_reactivated, 1);
    assert_eq!(e.repo.state_of(BOB), Some(AccountState::Active));
    assert!(matches!(
        e.svc.unban_person(&m, person(BOB).await, Some("x")).await,
        Err(AdminError::Conflict(_))
    ));
    assert_eq!(
        e.svc
            .list_bans(
                &m,
                &BanFilter {
                    active_only: true,
                    ..Default::default()
                },
                None,
                None
            )
            .await
            .unwrap()
            .items
            .len(),
        3
    );
}

#[tokio::test]
async fn staff_and_self_cannot_be_banned() {
    let e = env();
    let m = appointed(&e, MOD, R_MOD).await;
    assert!(matches!(
        e.svc
            .ban_person(&m, MOD + 500_000, None, BanCategory::Standard, Some("x"))
            .await,
        Err(AdminError::Forbidden(_))
    ));
    let owner = e.svc.resolve(OWNER, None).await.unwrap();
    e.svc
        .appoint(&owner, SUPPORT, &[R_SUPPORT], Some("x"))
        .await
        .unwrap();
    assert!(matches!(
        e.svc
            .ban_person(
                &m,
                SUPPORT + 500_000,
                None,
                BanCategory::Standard,
                Some("x")
            )
            .await,
        Err(AdminError::Conflict(_))
    ));
}

#[tokio::test]
async fn invite_graph_and_credits() {
    let e = env();
    let s = appointed(&e, SUPPORT, R_SUPPORT).await;
    e.repo.add_credits(ALICE, 5);
    e.repo.add_invite(ALICE, "AAAAAAAAAA", Some(BOB));
    let open = e.repo.add_invite(ALICE, "BBBBBBBBBB", None);

    let t = e.svc.invite_tree(&s, DAVE).await.unwrap();
    let path: Vec<_> = t.ancestors.iter().map(|n| n.username.as_str()).collect();
    assert_eq!(
        path,
        ["owner", "alice", "bob"],
        "root first, direct inviter last"
    );
    assert_eq!(t.account.depth, 0);
    let t = e.svc.invite_tree(&s, ALICE).await.unwrap();
    assert_eq!(
        (
            t.stats.issued,
            t.stats.open,
            t.stats.redeemed,
            t.stats.descendants
        ),
        (2, 1, 1, 3)
    );
    let d = e.svc.invite_descendants(&s, ALICE, Some(1)).await.unwrap();
    assert_eq!(d.nodes.len(), 2);
    let d = e.svc.invite_descendants(&s, ALICE, Some(5)).await.unwrap();
    assert_eq!(d.nodes.len(), 3);

    // Credits.
    assert_eq!(
        e.svc
            .adjust_credits(&s, ALICE, 3, Some("gift"))
            .await
            .unwrap(),
        8
    );
    assert_eq!(
        e.svc
            .adjust_credits(&s, ALICE, -8, Some("abuse"))
            .await
            .unwrap(),
        0
    );
    assert!(matches!(
        e.svc.adjust_credits(&s, ALICE, -1, Some("x")).await,
        Err(AdminError::Conflict(_))
    ));
    assert!(matches!(
        e.svc.adjust_credits(&s, ALICE, 0, Some("x")).await,
        Err(AdminError::Invalid(_))
    ));
    assert_eq!(
        e.svc.adjust_credits(&s, ALICE, 1, None).await.unwrap_err(),
        AdminError::ReasonRequired
    );
    let l = e.svc.credit_ledger(&s, ALICE, None).await.unwrap();
    assert_eq!(l.balance, 0);
    assert_eq!(l.entries[0].reason, "staff_revoke");

    // Invites: list and revoke.
    let page = e
        .svc
        .list_invites(
            &s,
            InviteQuery {
                issuer_id: Some(ALICE),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(page.items.len(), 2);
    let r = e
        .svc
        .revoke_invite(&s, open, Some("misused"))
        .await
        .unwrap();
    assert_eq!(r.state, "revoked");
    assert!(matches!(
        e.svc.revoke_invite(&s, open, Some("x")).await,
        Err(AdminError::Conflict(_))
    ));
    assert!(matches!(
        e.svc
            .list_invites(
                &s,
                InviteQuery {
                    state: Some("bogus".into()),
                    ..Default::default()
                },
                None
            )
            .await,
        Err(AdminError::Invalid(_))
    ));
}

#[tokio::test]
async fn account_search_paginates() {
    let e = env();
    let m = appointed(&e, MOD, R_MOD).await;
    let p1 = e
        .svc
        .search_accounts(&m, AccountQuery::default(), Some(3))
        .await
        .unwrap();
    assert_eq!(p1.items.len(), 3);
    let cursor: i64 = p1.next_after.unwrap().parse().unwrap();
    let p2 = e
        .svc
        .search_accounts(
            &m,
            AccountQuery {
                after: Some(cursor),
                ..Default::default()
            },
            Some(100),
        )
        .await
        .unwrap();
    assert!(p2.items.iter().all(|a| a.id > cursor));
    assert!(p2.next_after.is_none());
    let f = e
        .svc
        .search_accounts(
            &m,
            AccountQuery {
                q: Some("ALI".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(f.items.len(), 1);
    let f = e
        .svc
        .search_accounts(
            &m,
            AccountQuery {
                q: Some(BOB.to_string()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(f.items[0].username, "bob");
}

#[tokio::test]
async fn settings_are_validated_and_thresholds_ordered() {
    let e = env();
    let owner = e.svc.resolve(OWNER, None).await.unwrap();
    let view = e.svc.settings_view(&owner).await.unwrap();
    let cap = view.iter().find(|s| s.key == "accounts.cap").unwrap();
    assert!(cap.is_default);
    assert_eq!(cap.value, json!(20_000));

    let v = e
        .svc
        .patch_settings(
            &owner,
            &[
                ("accounts.cap".into(), json!(500)),
                ("invites.default_credits".into(), json!(9)),
            ],
            Some("tuning"),
        )
        .await
        .unwrap();
    let cap = v.iter().find(|s| s.key == "accounts.cap").unwrap();
    assert_eq!((cap.value.clone(), cap.is_default), (json!(500), false));

    // Unknown key and bad value reject the whole patch.
    let r = e
        .svc
        .patch_settings(
            &owner,
            &[("accounts.cap".into(), json!(7)), ("nope".into(), json!(1))],
            None,
        )
        .await;
    assert!(matches!(r, Err(AdminError::Invalid(m)) if m.contains("nope")));
    assert!(
        e.svc
            .patch_settings(&owner, &[("accounts.cap".into(), json!(-5))], None)
            .await
            .is_err()
    );
    assert!(e.svc.patch_settings(&owner, &[], None).await.is_err());
    let view = e.svc.settings_view(&owner).await.unwrap();
    assert_eq!(
        view.iter().find(|s| s.key == "accounts.cap").unwrap().value,
        json!(500)
    );
    // Escalation below freeze is refused.
    assert!(
        e.svc
            .patch_settings(
                &owner,
                &[("invites.suspicion_escalate_threshold".into(), json!(1))],
                None
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn job_queue_admin() {
    let e = env();
    let owner = e.svc.resolve(OWNER, None).await.unwrap();
    let dead = e.repo.add_job("media.transcode", "dead");
    e.repo.add_job("media.transcode", "queued");
    let page = e
        .svc
        .list_jobs(&owner, Some("dead"), None, None)
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert!(
        e.svc
            .list_jobs(&owner, Some("bogus"), None, None)
            .await
            .is_err()
    );
    e.svc.retry_job(&owner, dead, None).await.unwrap();
    assert!(
        e.svc.retry_job(&owner, dead, None).await.is_err(),
        "no longer dead"
    );
    let dead2 = e.repo.add_job("x", "dead");
    e.svc.delete_job(&owner, dead2, Some("junk")).await.unwrap();
    let c = e.svc.system_counts(&owner).await.unwrap();
    assert_eq!(c.jobs_dead, 0);
    assert_eq!(c.staff, 1);
}

#[tokio::test]
async fn access_log_is_chained_filterable_and_detects_tampering() {
    let e = env();
    let owner = e.svc.resolve(OWNER, None).await.unwrap();
    let m = appointed(&e, MOD, R_MOD).await;
    e.svc
        .set_suspended(&m, ALICE, true, Some("spam"))
        .await
        .unwrap();
    e.svc.account_detail(&m, ALICE).await.unwrap();
    let _ = e.svc.settings_view(&m).await; // denied

    let report = e.svc.verify_access_log(&owner).await.unwrap();
    assert!(report.ok, "{report:?}");
    assert!(report.rows_checked >= 4);

    let page = e
        .svc
        .access_log(
            &owner,
            AccessLogFilter {
                actor_id: Some(MOD),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
    assert!(page.items.iter().all(|r| r.actor_id == MOD));
    let kinds: Vec<_> = page.items.iter().map(|r| r.kind).collect();
    assert!(
        kinds.contains(&AccessKind::Denied)
            && kinds.contains(&AccessKind::Read)
            && kinds.contains(&AccessKind::AdminAction)
    );
    let by_action = e
        .svc
        .access_log(
            &owner,
            AccessLogFilter {
                action: Some("account.suspend".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(by_action.items.len(), 1);
    assert_eq!(by_action.items[0].reason, "spam");
    // Paging: newest first with a cursor.
    let p1 = e
        .svc
        .access_log(&owner, AccessLogFilter::default(), Some(2))
        .await
        .unwrap();
    assert_eq!(p1.items.len(), 2);
    let p2 = e
        .svc
        .access_log(
            &owner,
            AccessLogFilter {
                before: p1.next_before,
                ..Default::default()
            },
            Some(2),
        )
        .await
        .unwrap();
    assert!(p2.items[0].seq < p1.items[1].seq);

    // Tampering with a stored row is detected and raises an alert.
    let mut events = e.bus.subscribe(Topic::Platform);
    e.repo.tamper_log(2, "nothing to see");
    let report = e.svc.verify_access_log(&owner).await.unwrap();
    assert!(!report.ok);
    assert_eq!(report.first_bad_seq, Some(2));
    let ev = tokio::time::timeout(std::time::Duration::from_millis(200), events.next())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        ev,
        DomainEvent::StaffAlert {
            kind: StaffAlertKind::AccessLogChainBroken,
            ..
        }
    ));
}

#[tokio::test]
async fn private_reads_need_the_bit_and_a_reason_and_are_logged() {
    let e = env();
    let owner = e.svc.resolve(OWNER, None).await.unwrap();
    let m = appointed(&e, MOD, R_MOD).await;
    assert!(matches!(
        e.svc
            .log_private_read(&m, "channel", 99, Some("report #1"))
            .await,
        Err(AdminError::MissingPermission("READ_PRIVATE_CONTENT"))
    ));
    assert_eq!(
        e.svc
            .log_private_read(&owner, "channel", 99, Some("  "))
            .await
            .unwrap_err(),
        AdminError::ReasonRequired
    );
    e.svc
        .log_private_read(&owner, "channel", 99, Some("report #1"))
        .await
        .unwrap();
    let row = e
        .repo
        .log_rows()
        .into_iter()
        .rev()
        .find(|r| r.kind == AccessKind::PrivateRead)
        .unwrap();
    assert_eq!(
        (row.reason.as_str(), row.target_id),
        ("report #1", Some(99))
    );
}

#[tokio::test]
async fn transfer_owner_is_operator_only() {
    let e = env();
    let m = appointed(&e, MOD, R_MOD).await;
    assert_eq!(
        e.svc
            .transfer_owner(&m, ALICE, Some("x"))
            .await
            .unwrap_err(),
        AdminError::NotStaff
    );
    let op = Caller::operator(SYSTEM_USER_ID);
    e.svc
        .transfer_owner(&op, ALICE, Some("handover"))
        .await
        .unwrap();
    let a = e.svc.resolve(ALICE, None).await.unwrap();
    assert!(a.is_owner);
    assert!(matches!(
        e.svc.transfer_owner(&op, SYSTEM_USER_ID, Some("x")).await,
        Err(AdminError::Conflict(_))
    ));
    let _ = e.repo.verify_log().await.unwrap();
}

#[tokio::test]
async fn cli_appointment_is_audited_and_idempotent() {
    let e = env();
    let op = Caller::operator(SYSTEM_USER_ID);
    let m = e
        .svc
        .appoint_cli(&op, MOD, &[R_MOD], OWNER, Some("ops"))
        .await
        .unwrap();
    assert_eq!(m.appointed_by, Some(OWNER));
    // Same roles again: no change, no second audit row.
    let again = e
        .svc
        .appoint_cli(&op, MOD, &[R_MOD], OWNER, Some("ops"))
        .await
        .unwrap();
    assert_eq!(again.account_id, MOD);
    let rows: Vec<_> = e
        .repo
        .log_rows()
        .into_iter()
        .filter(|r| r.action == "staff.appoint_cli")
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].actor_id, SYSTEM_USER_ID);
    assert_eq!(rows[0].target_id, Some(OWNER));
    // Different roles on an existing member conflict (use set_roles).
    assert!(matches!(
        e.svc
            .appoint_cli(&op, MOD, &[R_SUPPORT], OWNER, Some("ops"))
            .await,
        Err(AdminError::Conflict(_))
    ));
    // The Owner role still cannot be handed out.
    assert!(matches!(
        e.svc
            .appoint_cli(&op, ALICE, &[R_OWNER], OWNER, Some("ops"))
            .await,
        Err(AdminError::Forbidden(_))
    ));
}

#[tokio::test]
async fn the_bans_list_filters_by_severity_reason_and_age() {
    let e = env();
    let m = appointed(&e, MOD, R_MOD).await;
    for (acct, cat) in [(BOB, BanCategory::Standard), (CAROL, BanCategory::Serious)] {
        e.svc
            .ban_person(&m, acct + 500_000, Some(acct), cat, Some("x"))
            .await
            .unwrap();
    }
    let list = |f: BanFilter| {
        let svc = &e.svc;
        let m = &m;
        async move { svc.list_bans(m, &f, None, None).await.unwrap().items }
    };
    assert_eq!(list(BanFilter::default()).await.len(), 2);
    let serious = list(BanFilter {
        category: Some(BanCategory::Serious),
        ..Default::default()
    })
    .await;
    assert_eq!(serious.len(), 1);
    assert_eq!(serious[0].person_id, CAROL + 500_000);
    // Bans made without a reason category are listed under "none"; no other reason matches them.
    for (reason, n) in [("none", 2), ("fraud", 0)] {
        let f = BanFilter {
            reason_category: Some(reason.into()),
            ..Default::default()
        };
        assert_eq!(list(f).await.len(), n, "{reason}");
    }
    // Just banned: not old, but new.
    let old = BanFilter {
        older_than_days: Some(1),
        ..Default::default()
    };
    assert!(list(old).await.is_empty());
    let new = BanFilter {
        newer_than_days: Some(1),
        ..Default::default()
    };
    assert_eq!(list(new).await.len(), 2);
    // An unknown reason is refused, not silently ignored.
    let bad = BanFilter {
        reason_category: Some("nonsense".into()),
        ..Default::default()
    };
    assert!(matches!(
        e.svc.list_bans(&m, &bad, None, None).await,
        Err(AdminError::Invalid(_))
    ));
}

#[tokio::test]
async fn a_saved_boolean_setting_reads_as_one_or_zero() {
    let repo = Arc::new(MemoryAdminRepo::default());
    let svc = AdminService::new(repo.clone());
    let key = crate::settings::BYOS_WORKER_FOR_MEMBERS;
    // Unset: the default (off).
    assert_eq!(svc.setting_value_i64(key).await.unwrap(), 0);
    let audit = AuditEntry {
        actor_id: OWNER,
        kind: AccessKind::AdminAction,
        action: "settings.test".into(),
        target_kind: None,
        target_id: None,
        reason: "test".into(),
        ip: None,
        detail: "{}".into(),
    };
    // What the settings validator stores for a boolean is JSON `true`.
    let stored = crate::settings::find(key)
        .unwrap()
        .validate(&json!(true))
        .unwrap();
    repo.put_settings(&[(key.to_owned(), stored)], OWNER, &audit)
        .await
        .unwrap();
    assert_eq!(svc.setting_value_i64(key).await.unwrap(), 1);
    repo.put_settings(&[(key.to_owned(), json!(false))], OWNER, &audit)
        .await
        .unwrap();
    assert_eq!(svc.setting_value_i64(key).await.unwrap(), 0);
}
