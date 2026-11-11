//! MOD-GRD-001: Postgres-backed tests of the graduated toolset (D-395): the serious-ban case on a four-generation
//! invite tree, the automatic freeze of unactivated descendants only, sterilization with a second approver, the
//! Operator's branch action, appeals of restrictions, the anonymised notice and the drill. Run with a scratch
//! Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-moderation --test graduated -- --include-ignored --test-threads=1`

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicI64, Ordering},
};

use async_trait::async_trait;
use sqlx::PgPool;
use sr_admin::{AdminService, BanCategory, Caller, StrikeWeight};
use sr_auth::store::InviteBlock;
use sr_core::PlatformPermissions as P;
use sr_db::{
    accounts::{self, NewAccount},
    member_invites,
    testing::TestDb,
};
use sr_moderation::{
    Blocked, ModError, Moderation, NodeBanner, SubjectNotifier,
    model::{AppealDecision, Who},
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

#[derive(Clone, Copy)]
struct Acct {
    person: i64,
    account: i64,
}

/// An active, verified account `age_days` old.
async fn acct(pool: &PgPool, name: &str, age_days: i32) -> Acct {
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
    sqlx::query("UPDATE accounts SET created_at = now() - make_interval(days => $2) WHERE id = $1")
        .bind(id)
        .bind(age_days)
        .execute(pool)
        .await
        .unwrap();
    Acct {
        person,
        account: id,
    }
}

/// `invitee` joined with a code `issuer` made.
async fn invite(pool: &PgPool, issuer: Acct, invitee: Acct) {
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
    sqlx::query("UPDATE accounts SET invited_by_invite_id = $1 WHERE id = $2")
        .bind(id)
        .bind(invitee.account)
        .execute(pool)
        .await
        .unwrap();
}

async fn credits(pool: &PgPool, a: Acct, n: i32) {
    sqlx::query(
        "INSERT INTO invite_credits_ledger (id, account_id, delta, reason) VALUES ($1, $2, $3, 'initial_grant')",
    )
    .bind(uid())
    .bind(a.account)
    .bind(n)
    .execute(pool)
    .await
    .unwrap();
}

async fn voice(m: &Moderation, a: Acct, secs: i64) {
    m.record_voice_seconds(a.account, secs).await.unwrap();
}

async fn can_invite(pool: &PgPool, a: Acct) -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    member_invites::create(
        pool,
        a.account,
        uid(),
        &format!("{:010}", uid() % 10_000_000_000),
        "",
        now,
    )
    .await
    .unwrap()
    .is_ok()
}

async fn blocked_reason(pool: &PgPool, a: Acct) -> Option<InviteBlock> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    member_invites::overview(pool, a.account, now)
        .await
        .unwrap()
        .blocked
}

fn staff(user_id: i64, perms: P) -> Caller {
    Caller {
        user_id,
        permissions: perms,
        is_owner: false,
        ip: Some("127.0.0.1".into()),
    }
}

fn owner(user_id: i64) -> Caller {
    Caller {
        user_id,
        permissions: P::OWNER,
        is_owner: true,
        ip: Some("127.0.0.1".into()),
    }
}

/// Appoint an account as staff (the appointment row is what the staff guard reads).
async fn appoint(pool: &PgPool, a: Acct) {
    let existing: Option<i64> =
        sqlx::query_scalar("SELECT id FROM guilds WHERE is_staff_guild LIMIT 1")
            .fetch_optional(pool)
            .await
            .unwrap();
    if existing.is_none() {
        sqlx::query(
            "INSERT INTO guilds (id, name, owner_id, is_staff_guild) VALUES ($1, 'Staff', $2, true)",
        )
        .bind(uid())
        .bind(a.account)
        .execute(pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO staff_appointments (id, account_id, appointed_by) VALUES ($1, $2, $2)",
    )
    .bind(uid())
    .bind(a.account)
    .execute(pool)
    .await
    .unwrap();
}

struct World {
    db: TestDb,
    m: Moderation,
    admin: AdminService,
    dms: Arc<Dms>,
}

impl World {
    fn pool(&self) -> &PgPool {
        self.db.pool()
    }
}

async fn world() -> World {
    let db = TestDb::new().await.expect("test db");
    let dms = Arc::new(Dms::default());
    let m = Moderation::new(db.pool().clone(), Arc::new(uid)).with_notifier(dms.clone());
    let repo = sr_db::admin_repo::PgAdminRepo::new(db.pool().clone(), Arc::new(uid));
    let admin = AdminService::new(Arc::new(repo));
    admin.subscribe_serious_ban(Arc::new(m.clone()));
    // The system user the banner is written under.
    sqlx::query("INSERT INTO persons (id) VALUES (1) ON CONFLICT DO NOTHING")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash, state) \
         VALUES (1, 1, 'sideroom', 'system@example.test', 'x', 'active') ON CONFLICT DO NOTHING",
    )
    .execute(db.pool())
    .await
    .unwrap();
    World { db, m, admin, dms }
}

/// The four-generation tree of the gate proof.
///
/// ```text
/// g1 -> g2 -> g3 (banned) -> g4_active (voice)
///                         -> g4_unactivated (40 days, nothing)
///                         -> g4_young (2 days, nothing)
/// g3 -> g4_unactivated -> g5 (unactivated, 40 days)
/// ```
struct Tree {
    g1: Acct,
    g2: Acct,
    g3: Acct,
    g4_active: Acct,
    g4_idle: Acct,
    g4_young: Acct,
    g5: Acct,
}

async fn tree(w: &World) -> Tree {
    let p = w.pool();
    let t = Tree {
        g1: acct(p, "gen1", 90).await,
        g2: acct(p, "gen2", 80).await,
        g3: acct(p, "gen3", 70).await,
        g4_active: acct(p, "gen4active", 40).await,
        g4_idle: acct(p, "gen4idle", 40).await,
        g4_young: acct(p, "gen4young", 2).await,
        g5: acct(p, "gen5", 40).await,
    };
    invite(p, t.g1, t.g2).await;
    invite(p, t.g2, t.g3).await;
    invite(p, t.g3, t.g4_active).await;
    invite(p, t.g3, t.g4_idle).await;
    invite(p, t.g3, t.g4_young).await;
    invite(p, t.g4_idle, t.g5).await;
    voice(&w.m, t.g4_active, 900).await;
    for a in [t.g1, t.g2] {
        credits(p, a, 5).await;
    }
    t
}

async fn restriction_state(pool: &PgPool, account: i64, kind: &str) -> Option<String> {
    sqlx::query_scalar(
        "SELECT state FROM member_restrictions WHERE account_id = $1 AND kind = $2 ORDER BY id DESC LIMIT 1",
    )
    .bind(account)
    .bind(kind)
    .fetch_optional(pool)
    .await
    .unwrap()
}

async fn node_status(pool: &PgPool, case: i64, account: i64) -> String {
    sqlx::query_scalar(
        "SELECT status FROM moderation_subtree_nodes WHERE case_id = $1 AND account_id = $2",
    )
    .bind(case)
    .bind(account)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn the_case(pool: &PgPool, root: Acct) -> i64 {
    sqlx::query_scalar("SELECT id FROM moderation_subtree_cases WHERE root_account_id = $1")
        .bind(root.account)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn log_count(pool: &PgPool, action: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM staff_access_log WHERE action = $1")
        .bind(action)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// The gate proof at service level: a serious ban of Gen 3 opens one case; only the unactivated Gen 4 node (and
/// the unactivated node below it) is frozen; the young one, the active one and both inviters are only listed;
/// Gen 1 and Gen 2 still create invites; the notice carries the category and counts.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn serious_ban_freezes_only_unactivated_descendants() {
    let w = world().await;
    let pool = w.pool();
    let t = tree(&w).await;
    let op = staff(t.g1.account, P::PLATFORM_BAN | P::MANAGE_CASES);
    // The staff member is a separate account in a real install; here Gen 1 is not staff, so use a fresh one.
    let mod_acct = acct(pool, "moderator", 100).await;
    let op = Caller {
        user_id: mod_acct.account,
        ..op
    };
    assert!(can_invite(pool, t.g1).await, "gen 1 invites before the ban");

    let out = w
        .admin
        .ban_person_with_reason(
            &op,
            t.g3.person,
            Some(t.g3.account),
            BanCategory::Serious,
            Some("fraud"),
            Some("chargeback ring"),
        )
        .await
        .unwrap();
    w.m.stamp_ban_reason(out.ban.id, "fraud").await.unwrap();

    let case = the_case(pool, t.g3).await;
    // Frozen: the unactivated Gen 4 node and the unactivated Gen 5 node below it.
    assert_eq!(
        node_status(pool, case, t.g4_idle.account).await,
        "auto_frozen"
    );
    assert_eq!(node_status(pool, case, t.g5.account).await, "auto_frozen");
    // Listed, untouched: active (voice), too young to judge, and both inviters.
    assert_eq!(node_status(pool, case, t.g4_active.account).await, "review");
    assert_eq!(node_status(pool, case, t.g4_young.account).await, "review");
    assert_eq!(node_status(pool, case, t.g2.account).await, "review");
    assert_eq!(node_status(pool, case, t.g1.account).await, "review");
    assert_eq!(node_status(pool, case, t.g3.account).await, "banned");
    for untouched in [t.g1, t.g2, t.g4_active, t.g4_young] {
        assert_eq!(
            restriction_state(pool, untouched.account, "case_freeze").await,
            None
        );
        assert!(!m_blocked(&w.m, untouched, Blocked::Posting).await);
    }
    // The frozen nodes cannot post, invite or join voice.
    for frozen in [t.g4_idle, t.g5] {
        assert!(m_blocked(&w.m, frozen, Blocked::Posting).await);
        assert!(m_blocked(&w.m, frozen, Blocked::Voice).await);
        assert!(m_blocked(&w.m, frozen, Blocked::Invites).await);
    }
    // Gen 1 and Gen 2 still create invites.
    assert!(can_invite(pool, t.g1).await);
    assert!(can_invite(pool, t.g2).await);
    // A second serious event for the same ban opens nothing new.
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM moderation_subtree_cases")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(n, 1);

    // The notice: category and counts, nothing else.
    let (title, body, actioned, frozen): (String, String, i32, i32) = sqlx::query_as(
        "SELECT a.title, a.body, n.accounts_actioned, n.accounts_frozen FROM moderation_notices n \
         JOIN platform_announcements a ON a.id = n.announcement_id",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(title, "Safety notice: Fraud");
    assert_eq!(body, "1 account actioned, 2 frozen pending review.");
    assert_eq!((actioned, frozen), (1, 2));
    for name in ["gen1", "gen2", "gen3", "gen4", "gen5"] {
        assert!(!title.contains(name) && !body.contains(name));
    }
    assert_eq!(log_count(pool, "moderation.subtree_case.open").await, 1);

    // The case view lists every node with its stats and lineage.
    let who = Who::Staff(&op);
    let detail = w.m.subtree_case(&who, case).await.unwrap();
    assert_eq!(detail.nodes.len(), 7);
    let idle = detail
        .nodes
        .iter()
        .find(|n| n.account_id == t.g4_idle.account)
        .unwrap();
    assert_eq!(
        (
            idle.messages,
            idle.voice_seconds,
            idle.unactivated,
            idle.restricted
        ),
        (0, 0, true, true)
    );
    assert_eq!(idle.parent_account_id, Some(t.g3.account));
    let inviter = detail.nodes.iter().find(|n| n.role == "inviter").unwrap();
    assert_eq!(inviter.account_id, t.g2.account);
    assert_eq!(inviter.depth, -1);

    // The frozen member reads the plain words and can appeal; the other cannot see another's row.
    let mine = w.m.my_restrictions(t.g4_idle.account).await.unwrap();
    assert_eq!(mine.len(), 1);
    assert!(mine[0].reason.contains("under review"), "{mine:?}");
    assert!(mine[0].can_appeal);
    assert!(w.m.my_restrictions(t.g1.account).await.unwrap().is_empty());
}

async fn m_blocked(m: &Moderation, a: Acct, what: Blocked) -> bool {
    m.is_blocked(a.account, what).await.unwrap()
}

/// A standard ban opens no case; a serious one without a reason category still does.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn only_serious_bans_open_cases() {
    let w = world().await;
    let pool = w.pool();
    let t = tree(&w).await;
    let mod_acct = acct(pool, "moderator", 100).await;
    let op = staff(mod_acct.account, P::PLATFORM_BAN);
    w.admin
        .ban_person(
            &op,
            t.g4_young.person,
            Some(t.g4_young.account),
            BanCategory::Standard,
            Some("rude"),
        )
        .await
        .unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM moderation_subtree_cases")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
    w.admin
        .ban_person(
            &op,
            t.g3.person,
            Some(t.g3.account),
            BanCategory::Serious,
            Some("no category given"),
        )
        .await
        .unwrap();
    let (cat,): (String,) = sqlx::query_as("SELECT category FROM moderation_subtree_cases")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(cat, "serious");
}

/// Per-member acts: warn, timeout, a timed invite freeze, revoking unused codes; each audited, each appealable,
/// each reading the multiplier seam.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn per_member_acts_are_audited_weighted_and_appealable() {
    struct Heavy;
    #[async_trait]
    impl StrikeWeight for Heavy {
        async fn strike_weight_multiplier(&self, account: i64) -> f32 {
            if account % 2 == 0 { 1.5 } else { 3.0 }
        }
    }
    let w = world().await;
    let pool = w.pool();
    let m = w.m.clone().with_strike_weight(Arc::new(Heavy));
    let mod_acct = acct(pool, "moderator", 100).await;
    let member = acct(pool, "member", 50).await;
    credits(pool, member, 3).await;
    let c = staff(mod_acct.account, P::MANAGE_CASES | P::MANAGE_INVITES);
    let who = Who::Staff(&c);

    let weight = if member.account % 2 == 0 { 1.5 } else { 3.0 };
    let warn = m
        .warn_member(&who, member.account, Some("spammed a channel"))
        .await
        .unwrap();
    assert_eq!((warn.kind.as_str(), warn.weight), ("warn", weight));
    assert_eq!(w.dms.0.lock().unwrap().len(), 1);
    let timeout = m
        .timeout_member(&who, member.account, 60, Some("cool off"))
        .await
        .unwrap();
    assert_eq!(timeout.weight, weight);
    assert!(m_blocked(&m, member, Blocked::Posting).await);
    assert!(!m_blocked(&m, member, Blocked::Invites).await);
    assert!(matches!(
        m.timeout_member(&who, member.account, 60, Some("again"))
            .await,
        Err(ModError::Conflict(_))
    ));
    assert!((m.strike_total(member.account).await.unwrap() - 2.0 * weight).abs() < 0.01);
    // Automated timeouts only ever get longer.
    assert_eq!(
        m.automated_timeout_minutes(member.account, 10).await,
        sr_admin::scale_up(10, weight)
    );
    assert_eq!(w.m.automated_timeout_minutes(member.account, 10).await, 10);

    // Reasons are required; bounds hold; a server moderator cannot.
    assert!(matches!(
        m.warn_member(&who, member.account, None).await,
        Err(ModError::ReasonRequired)
    ));
    assert!(
        m.timeout_member(&who, member.account, 0, Some("x"))
            .await
            .is_err()
    );
    assert!(matches!(
        m.warn_member(
            &Who::GuildMod {
                user_id: 1,
                guild_id: 2
            },
            member.account,
            Some("x")
        )
        .await,
        Err(ModError::Forbidden)
    ));
    // Not the system user, not oneself.
    assert!(m.warn_member(&who, 1, Some("x")).await.is_err());
    assert!(
        m.warn_member(&who, mod_acct.account, Some("x"))
            .await
            .is_err()
    );

    // A timed invite freeze blocks `create`; lifting it opens it again.
    assert!(can_invite(pool, member).await);
    let freeze = m
        .freeze_member_invites(&who, member.account, 14, Some("invited strangers"))
        .await
        .unwrap();
    assert_eq!(
        blocked_reason(pool, member).await,
        Some(InviteBlock::Frozen)
    );
    assert!(!can_invite(pool, member).await);
    m.lift_restriction(&who, freeze.id, Some("resolved"))
        .await
        .unwrap();
    assert!(can_invite(pool, member).await);

    // Revoking unused codes.
    let open = member_invites::overview(
        pool,
        member.account,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64,
    )
    .await
    .unwrap()
    .open
    .len();
    assert!(open >= 1);
    let r = m
        .revoke_unused_invites(&who, member.account, Some("account sold"))
        .await
        .unwrap();
    assert_eq!(r.codes_revoked as usize, open);

    // The member appeals the timeout; another staff member overturns it (the imposer may not decide it).
    let mine = m.my_restrictions(member.account).await.unwrap();
    assert!(mine.iter().any(|r| r.kind == "timeout" && r.can_appeal));
    m.appeal_restriction(member.account, timeout.id, "I was joking")
        .await
        .unwrap();
    assert!(matches!(
        m.appeal_restriction(member.account, timeout.id, "again")
            .await,
        Err(ModError::Conflict(_))
    ));
    assert!(matches!(
        m.decide_restriction_appeal(
            &who,
            timeout.id,
            AppealDecision::Overturned,
            Some("ok"),
            true
        )
        .await,
        Err(ModError::Forbidden)
    ));
    let other = acct(pool, "second", 100).await;
    let c2 = staff(other.account, P::MANAGE_CASES);
    let done = m
        .decide_restriction_appeal(
            &Who::Staff(&c2),
            timeout.id,
            AppealDecision::Overturned,
            Some("heard the member out"),
            true,
        )
        .await
        .unwrap();
    assert_eq!(done.state, "overturned");
    assert!(!m_blocked(&m, member, Blocked::Posting).await);
    // Another member cannot appeal it.
    assert!(matches!(
        m.appeal_restriction(other.account, warn.id, "not mine")
            .await,
        Err(ModError::NotFound(_))
    ));

    for action in [
        "moderation.member.warn",
        "moderation.member.timeout",
        "moderation.member.invite_freeze",
        "moderation.member.revoke_invites",
        "moderation.restriction.lift",
        "moderation.restriction.appeal_decide",
    ] {
        assert_eq!(log_count(pool, action).await, 1, "{action}");
    }
}

/// Sterilization: only inside a case, a written reason, a second approver, 12 months, appealable.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn sterilization_needs_a_second_approver_and_expires_in_twelve_months() {
    let w = world().await;
    let pool = w.pool();
    let t = tree(&w).await;
    let a = acct(pool, "staffa", 100).await;
    let b = acct(pool, "staffb", 100).await;
    appoint(pool, a).await;
    appoint(pool, b).await;
    let ca = staff(a.account, P::PLATFORM_BAN | P::MANAGE_CASES);
    let cb = staff(b.account, P::PLATFORM_BAN | P::MANAGE_CASES);
    w.admin
        .ban_person_with_reason(
            &ca,
            t.g3.person,
            Some(t.g3.account),
            BanCategory::Serious,
            Some("ban_evasion"),
            Some("evader"),
        )
        .await
        .unwrap();
    let case = the_case(pool, t.g3).await;
    let wa = Who::Staff(&ca);
    let wb = Who::Staff(&cb);

    // A short reason is refused; a staff account cannot be a target; the banned root cannot either.
    assert!(
        w.m.request_sterilization(&wa, case, t.g2.account, "too short")
            .await
            .is_err()
    );
    assert!(
        w.m.request_sterilization(
            &wa,
            case,
            t.g3.account,
            "the root is already banned so this must fail"
        )
        .await
        .is_err()
    );
    assert!(
        matches!(
            w.m.request_sterilization(
                &wa,
                case + 1,
                t.g2.account,
                "an account that is not in this case at all"
            )
            .await,
            Err(ModError::NotFound(_))
        ),
        "a wrong case names nothing"
    );

    let req =
        w.m.request_sterilization(
            &wa,
            case,
            t.g2.account,
            "Gen 2 vouched and invited a ring of fraud accounts knowingly.",
        )
        .await
        .unwrap();
    assert_eq!(req.state, "pending");
    // Nothing is in force before the second approver.
    assert!(can_invite(pool, t.g2).await);
    assert_eq!(
        restriction_state(pool, t.g2.account, "sterilization").await,
        None
    );
    // The proposer cannot approve their own; a second proposal for the same account is a conflict.
    assert!(matches!(
        w.m.approve_request(&wa, req.id).await,
        Err(ModError::Forbidden)
    ));
    assert!(matches!(
        w.m.request_sterilization(
            &wb,
            case,
            t.g2.account,
            "A second proposal for the same account, no."
        )
        .await,
        Err(ModError::Conflict(_))
    ));
    // Without MANAGE_CASES a staff member cannot approve either.
    let weak = staff(b.account, P::PLATFORM_BAN);
    assert!(matches!(
        w.m.approve_request(&Who::Staff(&weak), req.id).await,
        Err(ModError::MissingPermission(_))
    ));
    // The second approver applies it.
    let done = w.m.approve_request(&wb, req.id).await.unwrap();
    assert_eq!(done.state, "executed");
    assert_eq!(done.approved_by, Some(b.account));
    assert!(!can_invite(pool, t.g2).await);
    assert_eq!(
        restriction_state(pool, t.g2.account, "sterilization").await,
        Some("active".into())
    );
    assert_eq!(node_status(pool, case, t.g2.account).await, "sterilized");
    // It ends in twelve months (a day either way for the calendar).
    let days: f64 = sqlx::query_scalar(
        "SELECT (extract(epoch FROM (ends_at - starts_at)) / 86400)::double precision FROM member_restrictions WHERE kind = 'sterilization'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert!((364.0..=366.5).contains(&days), "{days}");
    let months_ok: bool = sqlx::query_scalar(
        "SELECT ends_at BETWEEN starts_at + interval '12 months' - interval '1 minute' AND starts_at + interval '12 months' + interval '1 minute' \
         FROM member_restrictions WHERE kind = 'sterilization'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(months_ok);
    // No profile mark: the member sees only the plain effect; nothing names a case or staff.
    let mine = w.m.my_restrictions(t.g2.account).await.unwrap();
    assert_eq!(mine.len(), 1);
    assert!(mine[0].reason.contains("under review") && mine[0].can_appeal);
    // Both staff members are in the audit entry.
    let (detail,): (String,) = sqlx::query_as(
        "SELECT detail FROM staff_access_log WHERE action = 'moderation.sterilization.apply'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(detail.contains(&a.account.to_string()) && detail.contains(&b.account.to_string()));
    // Posting still works: sterilization removes the invite right only.
    assert!(!m_blocked(&w.m, t.g2, Blocked::Posting).await);
    // Appealable: a third staff member overturns it and the member may invite again.
    w.m.appeal_restriction(
        t.g2.account,
        done_restriction(pool, t.g2).await,
        "I did not know",
    )
    .await
    .unwrap();
    let c3 = acct(pool, "staffc", 100).await;
    let cc = staff(c3.account, P::MANAGE_CASES);
    w.m.decide_restriction_appeal(
        &Who::Staff(&cc),
        done_restriction(pool, t.g2).await,
        AppealDecision::Overturned,
        Some("no evidence of intent"),
        true,
    )
    .await
    .unwrap();
    assert!(can_invite(pool, t.g2).await);
    // Expiry: after twelve months the sweep marks it expired and the right is back.
    let again =
        w.m.request_sterilization(
            &wa,
            case,
            t.g1.account,
            "Gen 1 ignored three reports about this branch over a year.",
        )
        .await
        .unwrap();
    w.m.approve_request(&wb, again.id).await.unwrap();
    assert!(!can_invite(pool, t.g1).await);
    sqlx::query("UPDATE member_restrictions SET ends_at = now() - interval '1 minute', starts_at = now() - interval '12 months' WHERE kind = 'sterilization' AND state = 'active'")
        .execute(pool)
        .await
        .unwrap();
    let (expired, removed) = w.m.sweep_restrictions().await.unwrap();
    assert_eq!((expired, removed), (1, 0));
    assert!(can_invite(pool, t.g1).await);
}

async fn done_restriction(pool: &PgPool, a: Acct) -> i64 {
    sqlx::query_scalar(
        "SELECT id FROM member_restrictions WHERE account_id = $1 AND kind = 'sterilization'",
    )
    .bind(a.account)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The branch action: Owner only, a second approver, one chosen action on every descendant, one audit entry with
/// the case, the approver and the node count, an anonymised notice, every affected account keeps its appeal.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn branch_action_is_the_operators_with_a_second_approver() {
    let w = world().await;
    let pool = w.pool();
    let t = tree(&w).await;
    let op = acct(pool, "operator", 200).await;
    let other = acct(pool, "approver", 100).await;
    appoint(pool, op).await;
    appoint(pool, other).await;
    let owner_c = owner(op.account);
    let approver = staff(other.account, P::PLATFORM_BAN | P::MANAGE_CASES);
    w.admin
        .ban_person_with_reason(
            &owner_c,
            t.g3.person,
            Some(t.g3.account),
            BanCategory::Serious,
            Some("spam"),
            Some("spam ring"),
        )
        .await
        .unwrap();
    let case = the_case(pool, t.g3).await;
    let reason = "The whole branch was provisioned by one script; freezing it pending review.";

    // Only the Operator proposes and executes.
    assert!(matches!(
        w.m.request_branch_action(&Who::Staff(&approver), case, "freeze", reason)
            .await,
        Err(ModError::MissingPermission("OPERATOR"))
    ));
    // The Operator is the Owner or a holder of the Owner-only OPERATOR bit: every other bit is not enough, and a
    // delegated Operator without PLATFORM_BAN may not propose a branch ban.
    let almost = staff(other.account, P::all() - P::OWNER - P::OPERATOR);
    assert!(matches!(
        w.m.request_branch_action(&Who::Staff(&almost), case, "freeze", reason)
            .await,
        Err(ModError::MissingPermission("OPERATOR"))
    ));
    let delegated = staff(other.account, P::STAFF_BASIC | P::OPERATOR);
    assert!(matches!(
        w.m.request_branch_action(&Who::Staff(&delegated), case, "ban", reason)
            .await,
        Err(ModError::MissingPermission("PLATFORM_BAN"))
    ));
    assert!(
        w.m.request_branch_action(&Who::Staff(&owner_c), case, "freeze", "short")
            .await
            .is_err()
    );
    assert!(
        w.m.request_branch_action(&Who::Staff(&owner_c), case, "delete", reason)
            .await
            .is_err()
    );
    let req =
        w.m.request_branch_action(&Who::Staff(&owner_c), case, "freeze", reason)
            .await
            .unwrap();
    struct NoBan;
    #[async_trait]
    impl NodeBanner for NoBan {
        async fn ban_account(&self, _: &Caller, _: i64, _: &str, _: &str) -> Result<(), String> {
            Err("never called for a freeze".into())
        }
    }
    // Not approved yet: refused.
    assert!(matches!(
        w.m.execute_branch_action(&Who::Staff(&owner_c), req.id, &NoBan)
            .await,
        Err(ModError::Conflict(_))
    ));
    // The proposer cannot approve; a non-Owner cannot execute even an approved one.
    assert!(matches!(
        w.m.approve_request(&Who::Staff(&owner_c), req.id).await,
        Err(ModError::Forbidden)
    ));
    let approved =
        w.m.approve_request(&Who::Staff(&approver), req.id)
            .await
            .unwrap();
    assert_eq!(approved.state, "approved");
    assert!(matches!(
        w.m.execute_branch_action(&Who::Staff(&approver), req.id, &NoBan)
            .await,
        Err(ModError::MissingPermission("OPERATOR"))
    ));

    let out =
        w.m.execute_branch_action(&Who::Staff(&owner_c), req.id, &NoBan)
            .await
            .unwrap();
    // Every descendant of the case: 4 below Gen 3 plus Gen 5 = 5 nodes... (g4 x3, g5) = 4 descendants.
    assert_eq!(out.nodes_actioned, 4);
    assert_eq!(out.action, "freeze");
    assert_eq!(out.notice.category, "spam");
    assert_eq!(
        out.notice.body,
        "4 accounts actioned, 4 frozen pending review."
    );
    for d in [t.g4_active, t.g4_idle, t.g4_young, t.g5] {
        assert_eq!(node_status(pool, case, d.account).await, "frozen");
        assert!(m_blocked(&w.m, d, Blocked::Posting).await);
        // 12 months, no automatic removal, each with its own appeal.
        let (removes, ok): (bool, bool) = sqlx::query_as(
            "SELECT removes_account, ends_at > now() + interval '11 months 20 days' FROM member_restrictions \
             WHERE account_id = $1 AND kind = 'case_freeze' AND state = 'active'",
        )
        .bind(d.account)
        .fetch_one(pool)
        .await
        .unwrap();
        assert!(!removes && ok);
        assert!(w.m.my_restrictions(d.account).await.unwrap()[0].can_appeal);
    }
    // The inviters are not part of the subtree: untouched.
    assert!(can_invite(pool, t.g1).await && can_invite(pool, t.g2).await);
    // One hash-chained entry names the case, the approver and the node count.
    let (detail, actor): (String, i64) = sqlx::query_as(
        "SELECT detail, actor_id FROM staff_access_log WHERE action = 'moderation.branch_action.execute'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(actor, op.account);
    assert!(
        detail.contains(&case.to_string())
            && detail.contains(&other.account.to_string())
            && detail.contains("\"node_count\":4"),
        "{detail}"
    );
    // It cannot run twice.
    assert!(matches!(
        w.m.execute_branch_action(&Who::Staff(&owner_c), req.id, &NoBan)
            .await,
        Err(ModError::Conflict(_))
    ));

    // The ban variant goes through a NodeBanner; a failure leaves that node for review.
    struct Bans(Mutex<Vec<i64>>);
    #[async_trait]
    impl NodeBanner for Bans {
        async fn ban_account(&self, _: &Caller, a: i64, cat: &str, _: &str) -> Result<(), String> {
            assert_eq!(cat, "spam");
            self.0.lock().unwrap().push(a);
            Ok(())
        }
    }
    let w2 = world().await;
    let t2 = tree(&w2).await;
    let pool2 = w2.pool();
    let op2 = acct(pool2, "operator", 200).await;
    let ap2 = acct(pool2, "approver", 100).await;
    appoint(pool2, op2).await;
    appoint(pool2, ap2).await;
    let o2 = owner(op2.account);
    w2.admin
        .ban_person_with_reason(
            &o2,
            t2.g3.person,
            Some(t2.g3.account),
            BanCategory::Serious,
            Some("spam"),
            Some("ring"),
        )
        .await
        .unwrap();
    let case2 = the_case(pool2, t2.g3).await;
    let req2 =
        w2.m.request_branch_action(&Who::Staff(&o2), case2, "ban", reason)
            .await
            .unwrap();
    w2.m.approve_request(
        &Who::Staff(&staff(ap2.account, P::PLATFORM_BAN | P::MANAGE_CASES)),
        req2.id,
    )
    .await
    .unwrap();
    let bans = Bans(Mutex::default());
    let out2 =
        w2.m.execute_branch_action(&Who::Staff(&o2), req2.id, &bans)
            .await
            .unwrap();
    assert_eq!((out2.action.as_str(), out2.nodes_actioned), ("ban", 4));
    assert_eq!(bans.0.lock().unwrap().len(), 4);
    assert_eq!(node_status(pool2, case2, t2.g5.account).await, "banned");
}

/// The 30-day removal: an unactivated, uncleared descendant is deactivated and queued for erasure; a cleared one
/// is not.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn thirty_days_removes_the_unactivated_unless_cleared() {
    let w = world().await;
    let pool = w.pool();
    let t = tree(&w).await;
    let mod_acct = acct(pool, "moderator", 100).await;
    let op = staff(mod_acct.account, P::PLATFORM_BAN | P::MANAGE_CASES);
    w.admin
        .ban_person_with_reason(
            &op,
            t.g3.person,
            Some(t.g3.account),
            BanCategory::Serious,
            Some("fraud"),
            Some("x"),
        )
        .await
        .unwrap();
    let case = the_case(pool, t.g3).await;
    let who = Who::Staff(&op);
    // Staff clear one of the two frozen accounts, with a reason.
    assert!(
        w.m.clear_node(&who, case, t.g5.account, None)
            .await
            .is_err()
    );
    w.m.clear_node(
        &who,
        case,
        t.g5.account,
        Some("a friend of the owner, verified"),
    )
    .await
    .unwrap();
    assert_eq!(node_status(pool, case, t.g5.account).await, "cleared");
    assert!(!m_blocked(&w.m, t.g5, Blocked::Posting).await);
    // Nothing happens before the day.
    assert_eq!(w.m.sweep_restrictions().await.unwrap(), (0, 0));
    sqlx::query("UPDATE member_restrictions SET starts_at = now() - interval '31 days', ends_at = now() - interval '1 minute' WHERE kind = 'case_freeze' AND state = 'active'")
        .execute(pool)
        .await
        .unwrap();
    assert_eq!(w.m.sweep_restrictions().await.unwrap(), (0, 1));
    assert_eq!(node_status(pool, case, t.g4_idle.account).await, "removed");
    let state: String = sqlx::query_scalar("SELECT state::text FROM accounts WHERE id = $1")
        .bind(t.g4_idle.account)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(state, "deactivated");
    let queued: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM account_delete_requests WHERE account_id = $1 AND state = 'pending'",
    )
    .bind(t.g4_idle.account)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(queued, 1);
    let g5: String = sqlx::query_scalar("SELECT state::text FROM accounts WHERE id = $1")
        .bind(t.g5.account)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(g5, "active");
    assert_eq!(
        log_count(pool, "moderation.subtree_case.remove_unactivated").await,
        1
    );
    // Manual node decisions: freeze the active one on evidence, ban the young one via the route's two calls.
    w.m.freeze_node(
        &who,
        case,
        t.g4_active.account,
        Some("posts match the ring's template"),
    )
    .await
    .unwrap();
    assert!(m_blocked(&w.m, t.g4_active, Blocked::Posting).await);
    let person =
        w.m.node_person(&who, case, t.g4_young.account)
            .await
            .unwrap();
    assert_eq!(person, t.g4_young.person);
    w.m.mark_node_banned(&who, case, t.g4_young.account, "same ring")
        .await
        .unwrap();
    assert_eq!(node_status(pool, case, t.g4_young.account).await, "banned");
    w.m.close_subtree_case(&who, case, Some("reviewed every node"))
        .await
        .unwrap();
    assert!(matches!(
        w.m.clear_node(&who, case, t.g2.account, Some("closed"))
            .await,
        Err(ModError::Conflict(_))
    ));
}

/// The transparency page carries counts by month and category and nothing that names an account.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn transparency_counts_hold_no_identity() {
    let w = world().await;
    let pool = w.pool();
    let t = tree(&w).await;
    let mod_acct = acct(pool, "moderator", 100).await;
    let op = staff(mod_acct.account, P::PLATFORM_BAN | P::MANAGE_CASES);
    w.admin
        .ban_person_with_reason(
            &op,
            t.g3.person,
            Some(t.g3.account),
            BanCategory::Serious,
            Some("child_safety"),
            Some("x"),
        )
        .await
        .unwrap();
    let page = w.m.transparency().await.unwrap();
    assert_eq!(page.months.len(), 1);
    let m = &page.months[0];
    assert_eq!(
        (
            m.serious_bans,
            m.branch_actions,
            m.accounts_actioned,
            m.accounts_frozen
        ),
        (1, 0, 1, 2)
    );
    assert_eq!(m.by_category.len(), 1);
    assert_eq!(m.by_category[0].category, "Illegal content");
    let json = serde_json::to_string(&page).unwrap();
    for name in ["gen3", "gen4", "child_safety"] {
        assert!(!json.contains(name), "{json}");
    }
    assert!(!json.contains(&t.g3.account.to_string()));
}

/// The drill builds a synthetic tree, runs a case to the notice and leaves nothing behind.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn drill_runs_the_pipeline_and_rolls_everything_back() {
    let w = world().await;
    let pool = w.pool();
    let op = acct(pool, "operator", 200).await;
    let o = owner(op.account);
    assert!(matches!(
        w.m.run_drill(&Who::Staff(&staff(op.account, P::MANAGE_CASES)))
            .await,
        Err(ModError::MissingPermission("OPERATOR"))
    ));
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM accounts")
        .fetch_one(pool)
        .await
        .unwrap();
    let report = w.m.run_drill(&Who::Staff(&o)).await.unwrap();
    // inviter2, inviter, root, three below it, one more below the first.
    assert_eq!(report.tree_size, 7);
    assert_eq!(report.auto_frozen, 2, "the idle child and its child");
    assert_eq!(
        report.waiting_review, 4,
        "voice-active, too young, both inviters"
    );
    assert_eq!(
        report.notice.body,
        "1 account actioned, 2 frozen pending review."
    );
    assert_eq!(report.rows_left_behind, 0);
    for table in [
        "moderation_subtree_cases",
        "moderation_subtree_nodes",
        "member_restrictions",
        "moderation_notices",
        "platform_announcements",
        "account_activity",
    ] {
        let n: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(n, 0, "{table}");
    }
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM accounts")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(log_count(pool, "moderation.drill").await, 1);
}

/// Integrator (wave 44, MOD-VCH-001 x MOD-GRD-001): the automatic freeze of an unactivated descendant is an
/// automated penalty, so its strike weight carries the accountability multiplier (D-427). Its 30-day length is not
/// scaled: the freeze ends in the account's removal, so a longer one would be the lighter outcome.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_automatic_case_freeze_weighs_by_the_multiplier() {
    struct Pillar;
    #[async_trait]
    impl StrikeWeight for Pillar {
        async fn strike_weight_multiplier(&self, _account: i64) -> f32 {
            1.5
        }
    }
    let w = world().await;
    let pool = w.pool();
    let m = w.m.clone().with_strike_weight(Arc::new(Pillar));
    let repo = sr_db::admin_repo::PgAdminRepo::new(pool.clone(), Arc::new(uid));
    let admin = AdminService::new(Arc::new(repo));
    admin.subscribe_serious_ban(Arc::new(m.clone()));
    let t = tree(&w).await;
    let mod_acct = acct(pool, "moderator", 100).await;
    let op = staff(mod_acct.account, P::PLATFORM_BAN | P::MANAGE_CASES);
    admin
        .ban_person_with_reason(
            &op,
            t.g3.person,
            Some(t.g3.account),
            BanCategory::Serious,
            Some("fraud"),
            Some("chargeback ring"),
        )
        .await
        .unwrap();
    for frozen in [t.g4_idle, t.g5] {
        let (weight, days): (f32, i32) = sqlx::query_as(
            "SELECT weight, round(extract(epoch FROM ends_at - starts_at) / 86400)::int FROM member_restrictions \
             WHERE account_id = $1 AND kind = 'case_freeze'",
        )
        .bind(frozen.account)
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!((weight, days), (1.5, 30));
    }
}

/// SEC-AUD-082 (D-605): a staff account in a banned subtree is left for review, never auto-frozen; a branch freeze
/// by a non-Owner leaves it alone, and the Owner's reaches it.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn automatic_moderation_leaves_staff_for_review() {
    let w = world().await;
    let pool = w.pool();
    let t = tree(&w).await;
    // An old, silent staff member invited by the banned account: unactivated by every measure.
    let mate = acct(pool, "staffmate", 40).await;
    invite(pool, t.g3, mate).await;
    appoint(pool, mate).await;
    let op = acct(pool, "operator", 200).await;
    let other = acct(pool, "approver", 100).await;
    appoint(pool, op).await;
    appoint(pool, other).await;
    let owner_c = owner(op.account);
    w.admin
        .ban_person_with_reason(
            &owner_c,
            t.g3.person,
            Some(t.g3.account),
            BanCategory::Serious,
            Some("spam"),
            Some("spam ring"),
        )
        .await
        .unwrap();
    let case = the_case(pool, t.g3).await;
    assert_eq!(node_status(pool, case, mate.account).await, "review");
    assert_eq!(
        restriction_state(pool, mate.account, "case_freeze").await,
        None
    );
    // The unactivated non-staff sibling is still auto-frozen.
    assert_eq!(
        node_status(pool, case, t.g4_idle.account).await,
        "auto_frozen"
    );

    // A branch freeze by a delegated (non-Owner) Operator skips the staff node.
    let reason = "The whole branch was provisioned by one script; freezing it pending review.";
    let req =
        w.m.request_branch_action(&Who::Staff(&owner_c), case, "freeze", reason)
            .await
            .unwrap();
    let approver = staff(other.account, P::PLATFORM_BAN | P::MANAGE_CASES);
    w.m.approve_request(&Who::Staff(&approver), req.id)
        .await
        .unwrap();
    struct NoBan;
    #[async_trait]
    impl NodeBanner for NoBan {
        async fn ban_account(&self, _: &Caller, _: i64, _: &str, _: &str) -> Result<(), String> {
            Err("never called for a freeze".into())
        }
    }
    let delegate = acct(pool, "delegate", 100).await;
    let delegated = staff(delegate.account, P::STAFF_BASIC | P::OPERATOR);
    let out =
        w.m.execute_branch_action(&Who::Staff(&delegated), req.id, &NoBan)
            .await
            .unwrap();
    assert_eq!(out.nodes_actioned, 4, "the four non-staff descendants");
    assert_eq!(node_status(pool, case, mate.account).await, "review");
    assert_eq!(
        restriction_state(pool, mate.account, "case_freeze").await,
        None
    );
    assert_eq!(node_status(pool, case, t.g5.account).await, "frozen");

    // The 30-day sweep never removes a staff account, even one that was frozen by an older build.
    sqlx::query(
        "INSERT INTO member_restrictions (id, account_id, kind, reason, case_id, imposed_by, state, starts_at, ends_at, removes_account) \
         VALUES ($1, $2, 'case_freeze', 'old', $3, 1, 'active', now() - interval '31 days', now() - interval '1 day', true)",
    )
    .bind(uid())
    .bind(mate.account)
    .bind(case)
    .execute(pool)
    .await
    .unwrap_or_else(|e| panic!("seed an old freeze: {e}"));
    w.m.sweep_restrictions().await.unwrap();
    let state: String = sqlx::query_scalar("SELECT state::text FROM accounts WHERE id = $1")
        .bind(mate.account)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(state, "active");
}

/// SEC-AUD-081: two racing `execute_branch_action` calls act once.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_branch_action_runs_once_under_a_race() {
    let w = world().await;
    let pool = w.pool();
    let t = tree(&w).await;
    let op = acct(pool, "operator", 200).await;
    let other = acct(pool, "approver", 100).await;
    appoint(pool, op).await;
    appoint(pool, other).await;
    let owner_c = owner(op.account);
    w.admin
        .ban_person_with_reason(
            &owner_c,
            t.g3.person,
            Some(t.g3.account),
            BanCategory::Serious,
            Some("spam"),
            Some("spam ring"),
        )
        .await
        .unwrap();
    let case = the_case(pool, t.g3).await;
    let req =
        w.m.request_branch_action(
            &Who::Staff(&owner_c),
            case,
            "freeze",
            "The whole branch was provisioned by one script; freezing it pending review.",
        )
        .await
        .unwrap();
    w.m.approve_request(
        &Who::Staff(&staff(other.account, P::PLATFORM_BAN | P::MANAGE_CASES)),
        req.id,
    )
    .await
    .unwrap();
    struct NoBan;
    #[async_trait]
    impl NodeBanner for NoBan {
        async fn ban_account(&self, _: &Caller, _: i64, _: &str, _: &str) -> Result<(), String> {
            Err("never called for a freeze".into())
        }
    }
    let who_owner = Who::Staff(&owner_c);
    let (a, b) = tokio::join!(
        w.m.execute_branch_action(&who_owner, req.id, &NoBan),
        w.m.execute_branch_action(&who_owner, req.id, &NoBan),
    );
    let oks = [a.is_ok(), b.is_ok()].iter().filter(|x| **x).count();
    assert_eq!(oks, 1, "exactly one call acts: {a:?} / {b:?}");
    let loser = if a.is_ok() { b } else { a };
    assert!(matches!(loser, Err(ModError::Conflict(_))), "{loser:?}");
    for d in [t.g4_active, t.g4_idle, t.g4_young, t.g5] {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM member_restrictions WHERE account_id = $1 AND kind = 'case_freeze' AND state = 'active'",
        )
        .bind(d.account)
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(n, 1, "one live case_freeze per account");
    }
    let state: String = sqlx::query_scalar("SELECT state FROM moderation_requests WHERE id = $1")
        .bind(req.id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(state, "executed");
}
