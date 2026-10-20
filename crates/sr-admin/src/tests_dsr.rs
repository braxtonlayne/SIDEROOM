//! Tests of the data-request queue service over the in-memory stores (M-25 step c).

use crate::{
    AdminError, AdminService, DataRequestKind, DataRequestState, MemoryDataRequests, RETAINED,
    SYSTEM_USER_ID,
    data_requests::{DAY_MS, MAX_EXTENSION_DAYS},
    memory::MemoryAdminRepo,
    model::*,
    repo::AdminRepository,
};
use sr_core::{ManualClock, PlatformPermissions as P};
use std::sync::Arc;

const OWNER: i64 = 10;
const SENIOR: i64 = 11;
const MODR: i64 = 12;
const SUBJECT: i64 = 20;
const T0: i64 = 1_800_000_000_000;

struct Env {
    svc: AdminService,
    repo: Arc<MemoryAdminRepo>,
    reqs: Arc<MemoryDataRequests>,
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
        (SUBJECT, "subject"),
    ] {
        repo.add_account(id, name, AccountState::Active, None, Some(OWNER));
    }
    repo.add_role(1, "Owner", 7, P::OWNER);
    repo.add_role(2, "Senior Staff", 6, P::STAFF_BASIC | P::MANAGE_TAKEDOWNS);
    repo.add_role(3, "Moderator", 5, P::STAFF_BASIC | P::MANAGE_CASES);
    repo.set_owner(OWNER);
    let reqs = Arc::new(MemoryDataRequests::new(repo.clone()));
    let svc = AdminService::new(repo.clone())
        .with_data_requests(reqs.clone())
        .with_clock(clock.clone());
    Env {
        svc,
        repo,
        reqs,
        clock,
    }
}

async fn staff(e: &Env, id: i64, role: i64) -> Caller {
    let o = e.svc.resolve(OWNER, None).await.unwrap();
    e.svc.appoint(&o, id, &[role], Some("test")).await.unwrap();
    e.svc.resolve(id, None).await.unwrap()
}

#[tokio::test]
async fn due_at_is_computed_from_the_deadline_at_creation() {
    let e = env();
    let c = staff(&e, SENIOR, 2).await;
    let r = e
        .svc
        .create_data_request(
            &c,
            DataRequestKind::Export,
            SUBJECT,
            None,
            Some("by email"),
            30,
        )
        .await
        .unwrap();
    assert_eq!(r.state, DataRequestState::Open);
    assert_eq!(r.received_at_ms, T0);
    assert_eq!(r.due_at_ms, T0 + 30 * DAY_MS);
    assert_eq!(r.effective_due_ms, r.due_at_ms);
    assert!(!r.overdue);
    // A backdated arrival counts from the day it arrived.
    let r2 = e
        .svc
        .create_data_request(
            &c,
            DataRequestKind::Erasure,
            SUBJECT,
            Some(T0 - 10 * DAY_MS),
            None,
            7,
        )
        .await
        .unwrap();
    assert_eq!(r2.due_at_ms, T0 - 3 * DAY_MS);
    assert!(r2.overdue, "past its date and still active");
    assert!(!e.svc.get_data_request(&c, r.id).await.unwrap().overdue);
}

#[tokio::test]
async fn only_takedown_staff_may_use_the_queue_and_a_denial_is_logged() {
    let e = env();
    let m = staff(&e, MODR, 3).await;
    let err = e
        .svc
        .create_data_request(&m, DataRequestKind::Export, SUBJECT, None, None, 30)
        .await
        .unwrap_err();
    assert!(matches!(err, AdminError::MissingPermission(_)));
    assert!(e.svc.list_data_requests(&m, true, None).await.is_err());
    let log = e
        .repo
        .list_log(&AccessLogFilter {
            limit: 50,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        log.iter()
            .any(|l| l.kind == AccessKind::Denied && l.action == "data_request.create")
    );
}

#[tokio::test]
async fn extend_is_allowed_once_bounded_and_logged_with_its_reason() {
    let e = env();
    let c = staff(&e, SENIOR, 2).await;
    let r = e
        .svc
        .create_data_request(&c, DataRequestKind::Export, SUBJECT, None, None, 30)
        .await
        .unwrap();
    for bad in [0, MAX_EXTENSION_DAYS + 1, -5] {
        assert!(matches!(
            e.svc
                .extend_data_request(&c, r.id, bad, Some("complex"))
                .await,
            Err(AdminError::Invalid(_))
        ));
    }
    assert_eq!(
        e.svc
            .extend_data_request(&c, r.id, 30, None)
            .await
            .unwrap_err(),
        AdminError::ReasonRequired
    );
    let x = e
        .svc
        .extend_data_request(&c, r.id, 30, Some("many systems involved"))
        .await
        .unwrap();
    assert_eq!(x.extended_until_ms, Some(r.due_at_ms + 30 * DAY_MS));
    assert_eq!(x.effective_due_ms, r.due_at_ms + 30 * DAY_MS);
    assert_eq!(x.extension_reason.as_deref(), Some("many systems involved"));
    // The original date stays visible; a second extension is refused.
    assert_eq!(x.due_at_ms, r.due_at_ms);
    assert!(matches!(
        e.svc.extend_data_request(&c, r.id, 5, Some("again")).await,
        Err(AdminError::Conflict(_))
    ));
    let log = e
        .repo
        .list_log(&AccessLogFilter {
            action: Some("data_request.extend".into()),
            limit: 10,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].reason, "many systems involved");
}

#[tokio::test]
async fn overdue_follows_the_effective_date_and_stops_when_closed() {
    let e = env();
    let c = staff(&e, SENIOR, 2).await;
    let r = e
        .svc
        .create_data_request(&c, DataRequestKind::Export, SUBJECT, None, None, 10)
        .await
        .unwrap();
    e.clock.set(T0 + 11 * DAY_MS);
    assert!(e.svc.get_data_request(&c, r.id).await.unwrap().overdue);
    let x = e
        .svc
        .extend_data_request(&c, r.id, 5, Some("legal review"))
        .await
        .unwrap();
    assert!(!x.overdue, "the extension moved the date");
    e.clock.set(T0 + 16 * DAY_MS);
    assert!(e.svc.get_data_request(&c, r.id).await.unwrap().overdue);
    let d = e
        .svc
        .decline_data_request(&c, r.id, Some("could not verify identity"))
        .await
        .unwrap();
    assert_eq!(d.state, DataRequestState::Declined);
    assert!(!d.overdue);
    // A closed request cannot be extended.
    assert!(
        e.svc
            .extend_data_request(&c, r.id, 5, Some("x"))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn running_needs_verification_first_and_one_open_request_per_kind() {
    let e = env();
    let c = staff(&e, SENIOR, 2).await;
    let r = e
        .svc
        .create_data_request(&c, DataRequestKind::Export, SUBJECT, None, None, 30)
        .await
        .unwrap();
    assert!(matches!(
        e.svc
            .create_data_request(&c, DataRequestKind::Export, SUBJECT, None, None, 30)
            .await,
        Err(AdminError::Conflict(_))
    ));
    assert!(matches!(
        e.svc.run_data_export(&c, r.id).await,
        Err(AdminError::Conflict(_))
    ));
    assert_eq!(
        e.svc.verify_data_request(&c, r.id, None).await.unwrap_err(),
        AdminError::ReasonRequired
    );
    let v = e
        .svc
        .verify_data_request(&c, r.id, Some("reply from the account email"))
        .await
        .unwrap();
    assert_eq!(
        (v.state, v.verified_by),
        (DataRequestState::Verified, Some(SENIOR))
    );
    let running = e.svc.run_data_export(&c, r.id).await.unwrap();
    assert_eq!(running.state, DataRequestState::Running);
    assert!(running.export_id.is_some());
    // An export request is not an erasure.
    assert!(matches!(
        e.svc.run_data_erasure(&c, r.id).await,
        Err(AdminError::Conflict(_))
    ));
}

#[tokio::test]
async fn erasure_names_the_retained_records_and_a_blocked_account_stays_verified() {
    let e = env();
    let c = staff(&e, SENIOR, 2).await;
    let r = e
        .svc
        .create_data_request(&c, DataRequestKind::Erasure, SUBJECT, None, None, 30)
        .await
        .unwrap();
    e.svc
        .verify_data_request(&c, r.id, Some("signed form"))
        .await
        .unwrap();
    e.reqs.block_erasure(SUBJECT, "The account owns 1 server");
    assert!(matches!(
        e.svc.run_data_erasure(&c, r.id).await,
        Err(AdminError::Conflict(_))
    ));
    assert_eq!(
        e.svc.get_data_request(&c, r.id).await.unwrap().state,
        DataRequestState::Verified
    );

    let e2 = env();
    let c2 = staff(&e2, SENIOR, 2).await;
    let r2 = e2
        .svc
        .create_data_request(&c2, DataRequestKind::Erasure, SUBJECT, None, None, 30)
        .await
        .unwrap();
    e2.svc
        .verify_data_request(&c2, r2.id, Some("signed form"))
        .await
        .unwrap();
    let run = e2.svc.run_data_erasure(&c2, r2.id).await.unwrap();
    assert_eq!(run.request.state, DataRequestState::Completed);
    let outcome = run.request.outcome.unwrap();
    for rec in RETAINED {
        assert!(outcome.contains(rec.label), "{} is named", rec.label);
    }
    assert_eq!(run.request.retained.len(), RETAINED.len());
}
