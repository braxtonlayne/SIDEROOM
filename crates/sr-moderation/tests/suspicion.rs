//! Postgres-backed tests of the suspicion tooling (step 5): the panel, the automatic case, the evasion signals and
//! their privacy rules. Run with a scratch Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-moderation --test suspicion -- --include-ignored --test-threads=1`

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicI64, Ordering},
};

use async_trait::async_trait;
use sqlx::PgPool;
use sr_admin::{Caller, SuspicionCrossing, SuspicionObserver};
use sr_core::PlatformPermissions as P;
use sr_db::{
    accounts::{self, NewAccount},
    testing::TestDb,
};
use sr_moderation::{
    ModError, Moderation, SubjectNotifier,
    model::{
        CaseFilter, CaseState, NewCase, ReportCategory, SignalFilter, SignalKind, SignalState,
        TargetKind, Who,
    },
    suspicion_rules::INVITER_NOTICE,
};

static SEQ: AtomicI64 = AtomicI64::new(0);

fn uid() -> i64 {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + SEQ.fetch_add(1, Ordering::Relaxed) % 1000
}

#[derive(Default)]
struct Dms(Mutex<Vec<(i64, String)>>);

#[async_trait]
impl SubjectNotifier for Dms {
    async fn system_dm(&self, to: i64, text: &str) -> bool {
        self.0.lock().unwrap().push((to, text.to_owned()));
        true
    }
}

struct Person {
    person: i64,
    account: i64,
}

async fn person(pool: &PgPool, name: &str) -> Person {
    let (id, person) = (uid(), uid());
    let ok = accounts::create_account(
        pool,
        &NewAccount {
            id,
            person_id: person,
            username: name,
            display_name: None,
            email: &format!("{name}@example.test"),
            password_hash: "x",
            state: "active",
            email_verified: true,
            initial_credits: 0,
            flags: 0,
        },
        uid(),
    )
    .await
    .unwrap();
    assert!(ok);
    Person {
        person,
        account: id,
    }
}

async fn exec(pool: &PgPool, sql: &str, binds: &[i64]) {
    let mut q = sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()));
    for b in binds {
        q = q.bind(*b);
    }
    q.execute(pool).await.unwrap();
}

/// `invitee` joined with a code `issuer` made.
async fn invite(pool: &PgPool, issuer: &Person, invitee: &Person) {
    let id = uid();
    let code = format!("{:010}", id % 10_000_000_000);
    sqlx::query(
        "INSERT INTO platform_invites (id, code, issuer_account_id, state, redeemed_by_account_id, redeemed_at) \
         VALUES ($1, $2, $3, 'redeemed', $4, now())",
    )
    .bind(id)
    .bind(code)
    .bind(issuer.account)
    .bind(invitee.account)
    .execute(pool)
    .await
    .unwrap();
    exec(
        pool,
        "UPDATE accounts SET invited_by_invite_id = $1 WHERE id = $2",
        &[id, invitee.account],
    )
    .await;
}

async fn ban(pool: &PgPool, p: &Person, by: i64, category: &str, reason: &str) {
    sqlx::query(
        "INSERT INTO person_bans (id, person_id, category, banned_by, reason_category) \
         VALUES ($1, $2, $3::ban_category, $4, $5::ban_reason_category)",
    )
    .bind(uid())
    .bind(p.person)
    .bind(category)
    .bind(by)
    .bind(reason)
    .execute(pool)
    .await
    .unwrap();
}

async fn login(pool: &PgPool, p: &Person, ip: &str, days_ago: i32) {
    sqlx::query(
        "INSERT INTO login_events (id, account_id, ip, success, created_at) \
         VALUES ($1, $2, $3::inet, true, now() - make_interval(days => $4))",
    )
    .bind(uid())
    .bind(p.account)
    .bind(ip)
    .bind(days_ago)
    .execute(pool)
    .await
    .unwrap();
}

fn staff(user_id: i64, perms: P) -> Caller {
    Caller {
        user_id,
        permissions: perms,
        is_owner: false,
        ip: Some("127.0.0.1".into()),
    }
}

async fn log_count(pool: &PgPool, action: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM staff_access_log WHERE action = $1")
        .bind(action)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// A staff guild (the one the migrations seed, or a new one) with one account holding `perms` through a role.
async fn staff_member(pool: &PgPool, p: &Person, perms: P) {
    let existing: Option<i64> =
        sqlx::query_scalar("SELECT id FROM guilds WHERE is_staff_guild LIMIT 1")
            .fetch_optional(pool)
            .await
            .unwrap();
    let guild = match existing {
        Some(g) => g,
        None => {
            let g = uid();
            exec(
                pool,
                "INSERT INTO guilds (id, name, owner_id, is_staff_guild) VALUES ($1, 'Staff', $2, true)",
                &[g, p.account],
            )
            .await;
            g
        }
    };
    let role = uid();
    exec(
        pool,
        "INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        &[guild, p.account],
    )
    .await;
    sqlx::query(
        "INSERT INTO roles (id, guild_id, name, platform_permissions) VALUES ($1, $2, $3, $4)",
    )
    .bind(role)
    .bind(guild)
    .bind(format!("role {role}"))
    .bind(perms.bits() as i64)
    .execute(pool)
    .await
    .unwrap();
    exec(
        pool,
        "INSERT INTO member_roles (guild_id, user_id, role_id) VALUES ($1, $2, $3)",
        &[guild, p.account, role],
    )
    .await;
    exec(
        pool,
        "INSERT INTO staff_appointments (id, account_id, appointed_by) VALUES ($1, $2, $2)",
        &[uid(), p.account],
    )
    .await;
}

struct World {
    db: TestDb,
    m: Moderation,
    dms: Arc<Dms>,
}

async fn world() -> World {
    let db = TestDb::new().await.expect("test db");
    let dms = Arc::new(Dms::default());
    let m = Moderation::new(db.pool().clone(), Arc::new(uid)).with_notifier(dms.clone());
    World { db, m, dms }
}

impl World {
    fn pool(&self) -> &PgPool {
        self.db.pool()
    }
}

fn crossing(
    inviter: &Person,
    banned: &Person,
    level: i32,
    freeze: bool,
    escalate: bool,
) -> SuspicionCrossing {
    SuspicionCrossing {
        person_id: inviter.person,
        inviter_account_id: inviter.account,
        banned_person_id: banned.person,
        level,
        freeze_crossed: freeze,
        escalate_crossed: escalate,
        actor_id: banned.account,
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_crossing_opens_one_case_for_senior_staff_linked_to_the_banning_case() {
    let w = world().await;
    let pool = w.pool();
    let inviter = person(pool, "inviter").await;
    let a = person(pool, "invitee_a").await;
    let b = person(pool, "invitee_b").await;
    let senior = person(pool, "senior").await;
    let moderator = person(pool, "moderator").await;
    staff_member(pool, &moderator, P::MANAGE_CASES).await;
    staff_member(pool, &senior, P::MANAGE_CASES | P::MANAGE_TAKEDOWNS).await;
    invite(pool, &inviter, &a).await;
    invite(pool, &inviter, &b).await;

    // The staff member opened a case about invitee A, then banned them.
    let admin = staff(senior.account, P::MANAGE_CASES | P::MANAGE_TAKEDOWNS);
    let banning =
        w.m.open_case(
            &Who::Staff(&admin),
            &NewCase {
                category: ReportCategory::Fraud,
                target_kind: TargetKind::User,
                target_id: Some(a.account),
                subject_account_id: Some(a.account),
                summary: "fraud".into(),
            },
        )
        .await
        .unwrap();

    w.m.threshold_crossed(crossing(&inviter, &a, 2, true, false))
        .await;
    let cases =
        w.m.list_cases(
            &Who::Staff(&admin),
            &CaseFilter {
                limit: 50,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let auto: Vec<_> = cases.items.iter().filter(|c| c.id != banning.id).collect();
    assert_eq!(auto.len(), 1, "one automatic case");
    let case = auto[0];
    assert_eq!(
        case.assignee_id,
        Some(senior.account),
        "assigned to senior staff, not the moderator"
    );
    assert_eq!(case.state, CaseState::Open);

    let (kind, linked, subject): (String, Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT auto_kind, linked_case_id, subject_person_id FROM moderation_cases WHERE id = $1",
    )
    .bind(case.id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(kind, "suspicion_freeze");
    assert_eq!(linked, Some(banning.id), "linked to the banning case");
    assert_eq!(subject, Some(inviter.person));

    // The inviter is told only that invite creation is paused for review.
    {
        let dms = w.dms.0.lock().unwrap();
        assert_eq!(dms.len(), 1);
        assert_eq!(dms[0].0, inviter.account);
        assert_eq!(dms[0].1, INVITER_NOTICE);
        assert!(!dms[0].1.to_lowercase().contains("invitee_a"));
    }

    // A second crossing (the escalation) upgrades the same case and sends no second notice.
    w.m.threshold_crossed(crossing(&inviter, &b, 3, false, true))
        .await;
    let open: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM moderation_cases WHERE subject_person_id = $1 AND auto_kind IS NOT NULL",
    )
    .bind(inviter.person)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(open, 1);
    let (kind, severity): (String, String) =
        sqlx::query_as("SELECT auto_kind, severity FROM moderation_cases WHERE id = $1")
            .bind(case.id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(kind, "suspicion_escalation");
    assert_eq!(severity, "serious");
    assert_eq!(w.dms.0.lock().unwrap().len(), 1, "no second notice");

    // Nothing was banned or frozen by the case itself.
    let banned: i64 = sqlx::query_scalar("SELECT count(*) FROM person_bans WHERE person_id = $1")
        .bind(inviter.person)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(banned, 0);

    // A clear lands on the timeline and the case stays open.
    w.m.cleared(inviter.person, senior.account, "reviewed, fine", false)
        .await;
    let events: Vec<String> = sqlx::query_scalar(
        "SELECT kind FROM moderation_case_events WHERE case_id = $1 ORDER BY id",
    )
    .bind(case.id)
    .fetch_all(pool)
    .await
    .unwrap();
    assert!(events.contains(&"suspicion_cleared".to_owned()));
    assert!(events.contains(&"suspicion_escalated".to_owned()));
    let detail = w.m.get_case(&Who::Staff(&admin), case.id).await.unwrap();
    assert_eq!(detail.case.state, CaseState::Open);
    w.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn with_no_senior_staff_the_case_waits_unassigned() {
    let w = world().await;
    let pool = w.pool();
    let inviter = person(pool, "inviter").await;
    let a = person(pool, "invitee_a").await;
    w.m.threshold_crossed(crossing(&inviter, &a, 2, true, false))
        .await;
    let assignee: Option<i64> =
        sqlx::query_scalar("SELECT assignee_id FROM moderation_cases WHERE subject_person_id = $1")
            .bind(inviter.person)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(assignee, None);
    w.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_panel_shows_history_banned_descendants_and_the_path_to_the_root() {
    let w = world().await;
    let pool = w.pool();
    let root = person(pool, "root").await;
    let inviter = person(pool, "inviter").await;
    let a = person(pool, "invitee_a").await;
    let deep = person(pool, "grandchild").await;
    let ok = person(pool, "fine").await;
    let senior = person(pool, "senior").await;
    invite(pool, &root, &inviter).await;
    invite(pool, &inviter, &a).await;
    invite(pool, &inviter, &ok).await;
    invite(pool, &a, &deep).await;
    ban(pool, &a, senior.account, "serious", "fraud").await;
    ban(pool, &deep, senior.account, "standard", "spam").await;
    exec(
        pool,
        "UPDATE persons SET suspicion_level = 1 WHERE id = $1",
        &[inviter.person],
    )
    .await;
    sqlx::query(
        "INSERT INTO suspicion_events (id, person_id, delta, level_after, reason, source_account_id, actor_id) \
         VALUES ($1, $2, 1, 1, 'serious ban of an invitee', $3, $4)",
    )
    .bind(uid())
    .bind(inviter.person)
    .bind(a.account)
    .bind(senior.account)
    .execute(pool)
    .await
    .unwrap();

    let admin = staff(senior.account, P::MANAGE_INVITES);
    let panel =
        w.m.suspicion_panel(&Who::Staff(&admin), inviter.person)
            .await
            .unwrap();
    assert_eq!(panel.level, 1);
    assert_eq!(panel.freeze_threshold, 2);
    assert!(!panel.invites_paused);
    assert_eq!(panel.events.len(), 1);
    assert_eq!(panel.events[0].kind, "raised");
    assert_eq!(
        panel.events[0].source_account.as_ref().unwrap().username,
        "invitee_a"
    );
    let names: Vec<_> = panel
        .banned_descendants
        .iter()
        .map(|d| (d.account.username.as_str(), d.depth))
        .collect();
    assert_eq!(names, vec![("invitee_a", 1), ("grandchild", 2)]);
    assert_eq!(
        panel.banned_descendants[0].reason_category.as_deref(),
        Some("fraud")
    );
    assert_eq!(panel.banned_descendants[0].ban_category, "serious");
    assert_eq!(panel.banned_descendants[1].ban_category, "standard");
    let path: Vec<_> = panel.invite_paths[0]
        .nodes
        .iter()
        .map(|n| n.username.as_str())
        .collect();
    assert_eq!(path, vec!["root", "inviter"]);
    // Signals are IP-derived: hidden without MANAGE_ACCOUNTS.
    assert!(panel.signals.is_none());
    assert_eq!(log_count(pool, "moderation.suspicion.view").await, 1);

    let both = staff(senior.account, P::MANAGE_INVITES | P::MANAGE_ACCOUNTS);
    let panel =
        w.m.suspicion_panel(&Who::Staff(&both), inviter.person)
            .await
            .unwrap();
    assert_eq!(panel.signals.as_ref().map(Vec::len), Some(0));

    // Without MANAGE_INVITES there is no panel at all.
    let nobody = staff(senior.account, P::MANAGE_ACCOUNTS);
    assert!(matches!(
        w.m.suspicion_panel(&Who::Staff(&nobody), inviter.person)
            .await,
        Err(ModError::MissingPermission(_))
    ));
    assert!(matches!(
        w.m.suspicion_panel(&Who::Staff(&admin), uid()).await,
        Err(ModError::NotFound("person"))
    ));
    w.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_shared_address_raises_a_review_item_and_nothing_else() {
    let w = world().await;
    let pool = w.pool();
    let banned = person(pool, "banned").await;
    let newcomer = person(pool, "newcomer").await;
    let stranger = person(pool, "stranger").await;
    let senior = person(pool, "senior").await;
    ban(pool, &banned, senior.account, "serious", "ban_evasion").await;
    login(pool, &banned, "203.0.113.7", 20).await;
    login(pool, &newcomer, "203.0.113.7", 1).await;
    login(pool, &stranger, "198.51.100.9", 1).await;

    assert_eq!(w.m.scan_person(newcomer.person).await.unwrap(), 1);
    assert_eq!(
        w.m.scan_person(newcomer.person).await.unwrap(),
        0,
        "idempotent"
    );
    assert_eq!(w.m.scan_person(stranger.person).await.unwrap(), 0);

    let admin = staff(senior.account, P::MANAGE_ACCOUNTS);
    let list =
        w.m.list_signals(
            &Who::Staff(&admin),
            &SignalFilter {
                limit: 50,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    let s = &list[0];
    assert_eq!(s.kind, SignalKind::SharedLoginIp);
    assert_eq!(s.person_id, newcomer.person);
    assert_eq!(s.related_person_id, banned.person);
    assert_eq!(s.detail["shared_ips"], 1);
    // The stored item names no address.
    assert!(!s.detail.to_string().contains("203.0.113"));
    let stored: String =
        sqlx::query_scalar("SELECT detail::text FROM evasion_signals WHERE id = $1")
            .bind(s.id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert!(!stored.contains("203.0.113"));

    // The address appears only on the logged detail read.
    let before = log_count(pool, "moderation.evasion.detail").await;
    let d = w.m.signal_detail(&Who::Staff(&admin), s.id).await.unwrap();
    assert_eq!(d.shared_addresses.len(), 1);
    assert_eq!(d.shared_addresses[0].ip, "203.0.113.7");
    assert_eq!(d.shared_addresses[0].persons, 2);
    assert_eq!(
        log_count(pool, "moderation.evasion.detail").await,
        before + 1
    );
    assert!(log_count(pool, "moderation.evasion.list").await >= 1);

    // Never auto-acted: nobody was banned, suspended or frozen.
    let bans: i64 = sqlx::query_scalar("SELECT count(*) FROM person_bans WHERE person_id = $1")
        .bind(newcomer.person)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(bans, 0);
    let state: String = sqlx::query_scalar("SELECT state::text FROM accounts WHERE id = $1")
        .bind(newcomer.account)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(state, "active");

    // Reviewing needs a reason, is logged, and happens once.
    let who = Who::Staff(&admin);
    assert_eq!(
        w.m.resolve_signal(&who, s.id, SignalState::Dismissed, "  ")
            .await
            .unwrap_err(),
        ModError::ReasonRequired
    );
    let done =
        w.m.resolve_signal(&who, s.id, SignalState::Dismissed, "roommates")
            .await
            .unwrap();
    assert_eq!(done.state, SignalState::Dismissed);
    assert_eq!(done.review_reason.as_deref(), Some("roommates"));
    assert_eq!(done.reviewed_by, Some(senior.account));
    assert_eq!(log_count(pool, "moderation.evasion.resolve").await, 1);
    assert!(matches!(
        w.m.resolve_signal(&who, s.id, SignalState::Confirmed, "x")
            .await,
        Err(ModError::Conflict(_))
    ));
    // A dismissed pair is not raised again.
    assert_eq!(w.m.scan_person(newcomer.person).await.unwrap(), 0);
    let open =
        w.m.list_signals(
            &who,
            &SignalFilter {
                state: Some(SignalState::Open),
                limit: 50,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(open.is_empty());

    // IP-derived data is for MANAGE_ACCOUNTS only.
    let invites_only = staff(senior.account, P::MANAGE_INVITES | P::MANAGE_CASES);
    let who = Who::Staff(&invites_only);
    assert!(matches!(
        w.m.list_signals(&who, &SignalFilter::default()).await,
        Err(ModError::MissingPermission(_))
    ));
    assert!(matches!(
        w.m.signal_detail(&who, s.id).await,
        Err(ModError::MissingPermission(_))
    ));
    assert!(matches!(
        w.m.scan_person_as(&who, newcomer.person).await,
        Err(ModError::MissingPermission(_))
    ));
    w.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn shared_networks_old_history_and_scrubbed_addresses_raise_nothing() {
    let w = world().await;
    let pool = w.pool();
    let banned = person(pool, "banned").await;
    let senior = person(pool, "senior").await;
    ban(pool, &banned, senior.account, "serious", "spam").await;

    // An address five Persons used is a shared network.
    login(pool, &banned, "192.0.2.50", 5).await;
    let crowd = [
        person(pool, "c1").await,
        person(pool, "c2").await,
        person(pool, "c3").await,
        person(pool, "c4").await,
    ];
    for c in &crowd {
        login(pool, c, "192.0.2.50", 2).await;
    }
    for c in &crowd {
        assert_eq!(
            w.m.scan_person(c.person).await.unwrap(),
            0,
            "shared network"
        );
    }

    // History outside the 180-day window does not count, and the purge removes it.
    let old = person(pool, "old").await;
    login(pool, &banned, "192.0.2.80", 5).await;
    login(pool, &old, "192.0.2.80", 181).await;
    assert_eq!(w.m.scan_person(old.person).await.unwrap(), 0);
    assert_eq!(w.m.purge_login_events().await.unwrap(), (1, false));
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM login_events WHERE account_id = $1")
        .bind(old.account)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(left, 0);

    // Account deletion blanks login addresses (phase 18); nothing is left to compare.
    let gone = person(pool, "gone").await;
    login(pool, &banned, "192.0.2.90", 5).await;
    login(pool, &gone, "192.0.2.90", 1).await;
    exec(
        pool,
        "UPDATE login_events SET ip = NULL WHERE account_id = $1",
        &[gone.account],
    )
    .await;
    assert_eq!(w.m.scan_person(gone.person).await.unwrap(), 0);
    w.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn lineage_signals_need_corroboration_unless_invited_by_the_banned() {
    let w = world().await;
    let pool = w.pool();
    let inviter = person(pool, "inviter").await;
    let banned = person(pool, "banned").await;
    let sibling = person(pool, "sibling").await;
    let child = person(pool, "child").await;
    let senior = person(pool, "senior").await;
    invite(pool, &inviter, &banned).await;
    invite(pool, &inviter, &sibling).await;
    invite(pool, &banned, &child).await;
    ban(pool, &banned, senior.account, "serious", "ban_evasion").await;

    // Same inviter alone: nothing. Invited by the banned person: a signal on its own.
    assert_eq!(w.m.scan_person(sibling.person).await.unwrap(), 0);
    assert_eq!(w.m.scan_person(child.person).await.unwrap(), 1);

    // The sibling made their account right after the ban: the pair now has two kinds of evidence.
    exec(
        pool,
        "UPDATE accounts SET created_at = now() + interval '1 day' WHERE id = $1",
        &[sibling.account],
    )
    .await;
    exec(
        pool,
        "UPDATE person_bans SET created_at = now() WHERE person_id = $1",
        &[banned.person],
    )
    .await;
    assert_eq!(w.m.scan_person(sibling.person).await.unwrap(), 2);
    let kinds: Vec<String> =
        sqlx::query_scalar("SELECT kind FROM evasion_signals WHERE person_id = $1 ORDER BY kind")
            .bind(sibling.person)
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(
        kinds,
        vec!["new_account_after_ban", "shared_invite_lineage"]
    );
    w.db.cleanup().await.unwrap();
}

/// The whole chain with the real admin service: serious bans of invitees raise the inviter's suspicion, the
/// crossings open and upgrade one case, the clear keeps the events and lands on the case timeline.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn serious_bans_through_the_admin_service_open_and_upgrade_the_case() {
    use sr_admin::{AdminService, BanCategory};
    let w = world().await;
    let pool = w.pool();
    let inviter = person(pool, "inviter").await;
    let invitees = [
        person(pool, "inv1").await,
        person(pool, "inv2").await,
        person(pool, "inv3").await,
    ];
    let senior = person(pool, "senior").await;
    for i in &invitees {
        invite(pool, &inviter, i).await;
    }
    let repo = sr_db::admin_repo::PgAdminRepo::new(pool.clone(), Arc::new(uid));
    let svc = AdminService::new(Arc::new(repo));
    svc.set_suspicion_observer(Arc::new(w.m.clone()));
    let op = Caller {
        user_id: senior.account,
        permissions: P::PLATFORM_BAN | P::MANAGE_INVITES,
        is_owner: false,
        ip: None,
    };

    let case_count = || async {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM moderation_cases WHERE subject_person_id = $1 AND auto_kind IS NOT NULL",
        )
        .bind(inviter.person)
        .fetch_one(pool)
        .await
        .unwrap()
    };

    // Level 1: below the freeze threshold (2), no case, no notice.
    svc.ban_account(
        &op,
        invitees[0].account,
        BanCategory::Serious,
        Some("fraud"),
    )
    .await
    .unwrap();
    assert_eq!(case_count().await, 0);
    assert!(w.dms.0.lock().unwrap().is_empty());

    // A standard ban does not count.
    // Level 2: the freeze threshold. One case, one notice to the inviter.
    svc.ban_account(
        &op,
        invitees[1].account,
        BanCategory::Serious,
        Some("fraud"),
    )
    .await
    .unwrap();
    assert_eq!(case_count().await, 1);
    assert_eq!(w.dms.0.lock().unwrap().len(), 1);
    assert_eq!(w.dms.0.lock().unwrap()[0].1, INVITER_NOTICE);
    let (kind, linked_event): (String, i64) = sqlx::query_as(
        "SELECT c.auto_kind, (SELECT count(*) FROM suspicion_events e WHERE e.case_id = c.id) \
         FROM moderation_cases c WHERE c.subject_person_id = $1 AND c.auto_kind IS NOT NULL",
    )
    .bind(inviter.person)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(kind, "suspicion_freeze");
    assert_eq!(
        linked_event, 1,
        "the event that crossed the line points at the case"
    );

    // Level 3: the escalation upgrades the same case.
    svc.ban_account(
        &op,
        invitees[2].account,
        BanCategory::Serious,
        Some("fraud"),
    )
    .await
    .unwrap();
    assert_eq!(case_count().await, 1);
    let kind: String = sqlx::query_scalar(
        "SELECT auto_kind FROM moderation_cases WHERE subject_person_id = $1 AND auto_kind IS NOT NULL",
    )
    .bind(inviter.person)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(kind, "suspicion_escalation");
    assert_eq!(w.dms.0.lock().unwrap().len(), 1);

    assert_eq!(w.dms.0.lock().unwrap().len(), 1, "none at the escalation");
    // Clear with a reason: the level is 0, the events stay and carry the reason, the case hears of it.
    assert!(
        svc.clear_suspicion(&op, inviter.person, None)
            .await
            .is_err()
    );
    svc.clear_suspicion(&op, inviter.person, Some("spoke to the member"))
        .await
        .unwrap();
    let level: i32 = sqlx::query_scalar("SELECT suspicion_level::int FROM persons WHERE id = $1")
        .bind(inviter.person)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(level, 0);
    let reasons: Vec<String> =
        sqlx::query_scalar("SELECT reason FROM suspicion_events WHERE person_id = $1 ORDER BY id")
            .bind(inviter.person)
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(
        reasons.len(),
        4,
        "three raises and the clear; nothing deleted"
    );
    assert_eq!(reasons[3], "cleared by staff: spoke to the member");
    let noted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM moderation_case_events e JOIN moderation_cases c ON c.id = e.case_id \
         WHERE c.subject_person_id = $1 AND e.kind = 'suspicion_cleared'",
    )
    .bind(inviter.person)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(noted, 1);
    // The mirror notice: the clear took the Person out of the pause, so one DM says invites are back (D-345).
    {
        let dms = w.dms.0.lock().unwrap();
        assert_eq!(dms.len(), 2);
        assert_eq!(dms[1].0, inviter.account);
        assert_eq!(
            dms[1].1,
            sr_moderation::suspicion_rules::INVITER_CLEARED_NOTICE
        );
    }
    // The panel shows the clear.
    let reader = staff(senior.account, P::MANAGE_INVITES);
    let panel =
        w.m.suspicion_panel(&Who::Staff(&reader), inviter.person)
            .await
            .unwrap();
    assert_eq!(
        panel.events.iter().filter(|e| e.kind == "cleared").count(),
        1
    );
    assert!(panel.events[0].reason.contains("spoke to the member"));
    assert_eq!(panel.banned_descendants.len(), 3);
    w.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_clear_that_leaves_the_pause_in_place_sends_nothing() {
    let w = world().await;
    let pool = w.pool();
    let inviter = person(pool, "inviter").await;
    // Not paused before the clear: no mirror notice.
    w.m.cleared(inviter.person, inviter.account, "tidy", false)
        .await;
    assert!(w.dms.0.lock().unwrap().is_empty());
    // Paused, but the account is also frozen by hand: still paused afterwards, so nothing is sent.
    exec(
        pool,
        "UPDATE accounts SET invites_frozen_at = now() WHERE id = $1",
        &[inviter.account],
    )
    .await;
    w.m.cleared(inviter.person, inviter.account, "tidy", true)
        .await;
    assert!(w.dms.0.lock().unwrap().is_empty());
    exec(
        pool,
        "UPDATE accounts SET invites_frozen_at = NULL WHERE id = $1",
        &[inviter.account],
    )
    .await;
    w.m.cleared(inviter.person, inviter.account, "tidy", true)
        .await;
    assert_eq!(w.dms.0.lock().unwrap().len(), 1);
    w.db.cleanup().await.unwrap();
}

async fn set_setting(pool: &PgPool, key: &str, value: i64) {
    sqlx::query(
        "INSERT INTO platform_settings (key, value) VALUES ($1, to_jsonb($2::bigint)) \
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_shared_network_cut_off_and_the_ban_window_are_settings() {
    let w = world().await;
    let pool = w.pool();
    let banned = person(pool, "banned").await;
    let a = person(pool, "alpha").await;
    let b = person(pool, "bravo").await;
    let senior = person(pool, "senior").await;
    ban(pool, &banned, senior.account, "serious", "spam").await;
    // Three people used the address: banned, a and b.
    for p in [&banned, &a, &b] {
        login(pool, p, "198.51.100.5", 3).await;
    }
    // Default cut-off is four people: the pair is a signal.
    assert_eq!(w.m.scan_person(a.person).await.unwrap(), 1);
    // Lowered to two, a three-person address is a shared network.
    set_setting(pool, "moderation.shared_ip_max_persons", 2).await;
    assert_eq!(w.m.scan_person(b.person).await.unwrap(), 0);
    // The ban window: a new account 20 days after the ban corroborates only when the window is long enough.
    let c = person(pool, "charlie").await;
    invite(pool, &banned, &c).await;
    exec(
        pool,
        "UPDATE person_bans SET created_at = now() - interval '25 days' WHERE person_id = $1",
        &[banned.person],
    )
    .await;
    exec(
        pool,
        "UPDATE accounts SET created_at = now() - interval '5 days' WHERE id = $1",
        &[c.account],
    )
    .await;
    assert_eq!(w.m.scan_person(c.person).await.unwrap(), 1, "lineage alone");
    exec(
        pool,
        "DELETE FROM evasion_signals WHERE person_id = $1",
        &[c.person],
    )
    .await;
    set_setting(pool, "moderation.new_account_after_ban_days", 30).await;
    assert_eq!(
        w.m.scan_person(c.person).await.unwrap(),
        2,
        "lineage plus new account within 30 days"
    );
    w.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_confirmed_signal_keeps_its_finding_after_the_sign_in_rows_are_purged() {
    let w = world().await;
    let pool = w.pool();
    let inviter = person(pool, "inviter").await;
    let banned = person(pool, "banned").await;
    let newcomer = person(pool, "newcomer").await;
    let senior = person(pool, "senior").await;
    ban(pool, &banned, senior.account, "serious", "ban_evasion").await;
    login(pool, &banned, "203.0.113.77", 175).await;
    login(pool, &newcomer, "203.0.113.77", 170).await;
    assert_eq!(w.m.scan_person(newcomer.person).await.unwrap(), 1);
    // The newcomer has an open automatic case, so the finding also lands there as evidence.
    w.m.threshold_crossed(SuspicionCrossing {
        person_id: newcomer.person,
        inviter_account_id: newcomer.account,
        banned_person_id: banned.person,
        level: 2,
        freeze_crossed: true,
        escalate_crossed: false,
        actor_id: senior.account,
    })
    .await;
    let _ = inviter;
    let admin = staff(senior.account, P::MANAGE_ACCOUNTS);
    let who = Who::Staff(&admin);
    let list =
        w.m.list_signals(
            &who,
            &SignalFilter {
                limit: 5,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let sid = list[0].id;
    // A dismissal keeps no snapshot.
    w.m.resolve_signal(
        &who,
        sid,
        SignalState::Confirmed,
        "same person, same address",
    )
    .await
    .unwrap();

    let snap: serde_json::Value =
        sqlx::query_scalar("SELECT detail_snapshot FROM evasion_signals WHERE id = $1")
            .bind(sid)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(snap["shared_addresses"], 1);
    assert_eq!(snap["window_days"], 180);
    assert!(
        !snap.to_string().contains("203.0.113"),
        "the snapshot holds no address"
    );
    let ev: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM moderation_case_evidence e JOIN moderation_cases c ON c.id = e.case_id \
         WHERE e.kind = 'evasion_signal' AND c.subject_person_id = $1",
    )
    .bind(newcomer.person)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(ev, 1);

    // The 180-day purge removes the rows; the detail still shows what the reviewer saw.
    exec(
        pool,
        "UPDATE login_events SET created_at = now() - interval '200 days'",
        &[],
    )
    .await;
    assert_eq!(w.m.purge_login_events().await.unwrap(), (2, false));
    let d = w.m.signal_detail(&who, sid).await.unwrap();
    assert!(d.shared_addresses.is_empty());
    assert_eq!(d.snapshot.unwrap()["shared_addresses"], 1);
    w.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_panel_lists_a_refused_re_registration_and_linking_a_signal_works_once() {
    let w = world().await;
    let pool = w.pool();
    let banned = person(pool, "banned").await;
    let newcomer = person(pool, "newcomer").await;
    let senior = person(pool, "senior").await;
    ban(pool, &banned, senior.account, "serious", "ban_evasion").await;
    sqlx::query(
        "INSERT INTO suspicion_events (id, person_id, delta, level_after, reason, kind) \
         VALUES ($1, $2, 0, 0, 'registration refused: matched by discord; invite from person 9', 'reregistration_refused')",
    )
    .bind(uid())
    .bind(banned.person)
    .execute(pool)
    .await
    .unwrap();
    let both = staff(
        senior.account,
        P::MANAGE_INVITES | P::MANAGE_ACCOUNTS | P::MANAGE_CASES,
    );
    let who = Who::Staff(&both);
    let panel = w.m.suspicion_panel(&who, banned.person).await.unwrap();
    assert_eq!(panel.events.len(), 1);
    assert_eq!(panel.events[0].kind, "reregistration_refused");
    assert_eq!(panel.level, 0, "no level change");

    // Link a signal to a case, once.
    login(pool, &banned, "192.0.2.9", 2).await;
    login(pool, &newcomer, "192.0.2.9", 1).await;
    assert_eq!(w.m.scan_person(newcomer.person).await.unwrap(), 1);
    let sid =
        w.m.list_signals(
            &who,
            &SignalFilter {
                limit: 5,
                ..Default::default()
            },
        )
        .await
        .unwrap()[0]
            .id;
    let case =
        w.m.open_case(
            &who,
            &NewCase {
                category: ReportCategory::BanEvasion,
                target_kind: TargetKind::User,
                target_id: Some(newcomer.account),
                subject_account_id: Some(newcomer.account),
                summary: String::new(),
            },
        )
        .await
        .unwrap();
    let linked = w.m.link_signal_case(&who, sid, case.id).await.unwrap();
    assert_eq!(linked.case_id, Some(case.id));
    assert!(matches!(
        w.m.link_signal_case(&who, sid, case.id).await,
        Err(ModError::Conflict(_))
    ));
    assert_eq!(log_count(pool, "moderation.evasion.link_case").await, 1);
    let only_accounts = staff(senior.account, P::MANAGE_ACCOUNTS);
    assert!(matches!(
        w.m.link_signal_case(&Who::Staff(&only_accounts), sid, case.id)
            .await,
        Err(ModError::MissingPermission(_))
    ));
    assert_eq!(w.m.count_open_signals(&who).await.unwrap(), 1);
    w.db.cleanup().await.unwrap();
}
