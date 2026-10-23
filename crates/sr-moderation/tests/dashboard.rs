//! Postgres-backed tests of the staff dashboards (MOD-DASH-002, -005 to -010, MOD-BAN-003): the aggregates, the
//! reports inbox with its bulk dismissal, and what a caller without the bit never sees. Run with a scratch
//! Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-moderation --test dashboard -- --ignored --test-threads=1`

use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use sqlx::PgPool;
use sr_admin::{AdminRepository, BanCategory, BanFilter, Caller};
use sr_core::PlatformPermissions as P;
use sr_db::{
    accounts::{self, NewAccount},
    admin_repo::PgAdminRepo,
    testing::TestDb,
};
use sr_moderation::{
    HoldScope, ModError, Moderation, Transition,
    model::{CaseState, NewReport, ReportCategory, TargetKind, Who},
};

static SEQ: AtomicI64 = AtomicI64::new(0);

fn uid() -> i64 {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + SEQ.fetch_add(1, Ordering::Relaxed) % 1000
}

async fn account(pool: &PgPool, name: &str) -> i64 {
    let id = uid();
    let ok = accounts::create_account(
        pool,
        &NewAccount {
            id,
            person_id: uid(),
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
    id
}

async fn exec(pool: &PgPool, sql: &'static str, binds: &[i64]) {
    let mut q = sqlx::query(sql);
    for b in binds {
        q = q.bind(*b);
    }
    q.execute(pool).await.unwrap();
}

fn staff(user_id: i64, perms: P) -> Caller {
    Caller {
        user_id,
        permissions: perms,
        is_owner: false,
        ip: Some("127.0.0.1".into()),
    }
}

struct World {
    db: TestDb,
    m: Moderation,
    owner: i64,
    author: i64,
    reporter: i64,
    reporter2: i64,
    staff_id: i64,
    guild: i64,
    channel: i64,
    base: i64,
}

async fn world() -> World {
    let db = TestDb::new().await.expect("test db");
    let pool = db.pool().clone();
    let owner = account(&pool, "owner").await;
    let author = account(&pool, "author").await;
    let reporter = account(&pool, "reporter").await;
    let reporter2 = account(&pool, "reporter2").await;
    let staff_id = account(&pool, "staff1").await;
    let (g, c) = (uid(), uid());
    exec(
        &pool,
        "INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Club', $2)",
        &[g, owner],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO channels (id, guild_id, type, name) VALUES ($1, $2, 0, 'general')",
        &[c, g],
    )
    .await;
    for m in [owner, author, reporter, reporter2] {
        exec(
            &pool,
            "INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            &[g, m],
        )
        .await;
    }
    let _: String = sqlx::query_scalar(
        "SELECT sr_ensure_message_partition((now() - interval '10 minutes')::date)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let base: i64 =
        sqlx::query_scalar("SELECT sr_snowflake_floor(now() - interval '10 minutes') + 1000")
            .fetch_one(&pool)
            .await
            .unwrap();
    World {
        m: Moderation::new(pool, Arc::new(uid)),
        db,
        owner,
        author,
        reporter,
        reporter2,
        staff_id,
        guild: g,
        channel: c,
        base,
    }
}

impl World {
    fn pool(&self) -> &PgPool {
        self.db.pool()
    }

    async fn message(&self, n: i64) -> i64 {
        let id = self.base + n;
        sqlx::query(
            "INSERT INTO messages (id, channel_id, author_id, content) VALUES ($1, $2, $3, 'line')",
        )
        .bind(id)
        .bind(self.channel)
        .bind(self.author)
        .execute(self.pool())
        .await
        .unwrap();
        id
    }

    /// A report on message `n` by `by`. A child-safety one goes to platform staff at once (D-209).
    async fn report(&self, by: i64, n: i64, cat: ReportCategory) -> i64 {
        let id = self.base + n;
        let r = self
            .m
            .file_report(
                by,
                &NewReport {
                    target_kind: TargetKind::Message,
                    target_id: id,
                    channel_id: Some(self.channel),
                    guild_id: Some(self.guild),
                    category: cat,
                    note: "this is bad".into(),
                    moderators_may_see_name: false,
                },
            )
            .await
            .unwrap();
        sqlx::query_scalar("SELECT case_id FROM moderation_reports WHERE id = $1")
            .bind(r.id)
            .fetch_one(self.pool())
            .await
            .unwrap()
    }
}

fn caller(w: &World, p: P) -> Caller {
    staff(w.staff_id, p)
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_inbox_groups_duplicates_hides_reporters_and_skips_guild_cases() {
    let w = world().await;
    w.message(1).await;
    w.message(2).await;
    w.message(3).await;
    // Two people report the same message as child safety: one case, two reports.
    let dup = w.report(w.reporter, 1, ReportCategory::ChildSafety).await;
    let again = w.report(w.reporter2, 1, ReportCategory::ChildSafety).await;
    assert_eq!(dup, again, "duplicates attach to the case");
    let single = w.report(w.reporter, 2, ReportCategory::ChildSafety).await;
    // A harassment report in a server stays with the server's moderators.
    let guild_case = w.report(w.reporter, 3, ReportCategory::Harassment).await;

    let c = caller(&w, P::MANAGE_CASES);
    let who = Who::Staff(&c);
    let page = w.m.inbox(&who, None, 25).await.unwrap();
    let ids: Vec<i64> = page.items.iter().map(|g| g.case_id).collect();
    assert_eq!(ids, vec![dup.min(single), dup.max(single)]);
    assert!(
        !ids.contains(&guild_case),
        "a server's own case is not in the platform inbox"
    );
    let group = page.items.iter().find(|g| g.case_id == dup).unwrap();
    assert_eq!(group.reports, 2, "the duplicate group counts both reports");
    assert_eq!(group.categories, vec!["child_safety".to_owned()]);

    // The answer carries no reporter: not the ids, not the note.
    let json = serde_json::to_string(&page).unwrap();
    for needle in [
        w.reporter.to_string(),
        w.reporter2.to_string(),
        "this is bad".to_owned(),
    ] {
        assert!(!json.contains(&needle), "{needle} leaked into the inbox");
    }

    // The hub tile's count matches the list: two cases, three reports (the guild case is not counted).
    let n = w.m.inbox_count(&who).await.unwrap();
    assert_eq!((n.groups, n.reports), (2, 3));
    assert!(n.oldest_seconds.is_some());

    // Paging is oldest first.
    let first = w.m.inbox(&who, None, 1).await.unwrap();
    assert_eq!(first.items.len(), 1);
    let cursor: i64 = first.next_cursor.as_deref().unwrap().parse().unwrap();
    let second = w.m.inbox(&who, Some(cursor), 1).await.unwrap();
    assert_eq!(second.items.len(), 1);
    assert_ne!(first.items[0].case_id, second.items[0].case_id);
    assert!(second.next_cursor.is_none());

    // Once somebody claims a case it leaves the inbox.
    w.m.assign(&who, single, true).await.unwrap();
    let after = w.m.inbox(&who, None, 25).await.unwrap();
    assert_eq!(after.items.len(), 1);
    assert_eq!(after.items[0].case_id, dup);

    // The inbox is for MANAGE_CASES only.
    let nobody = caller(&w, P::MANAGE_TAKEDOWNS);
    assert_eq!(
        w.m.inbox(&Who::Staff(&nobody), None, 25).await.unwrap_err(),
        ModError::MissingPermission("MANAGE_CASES")
    );
    let n = w.m.inbox_count(&who).await.unwrap();
    assert_eq!(
        (n.groups, n.reports),
        (1, 2),
        "a claimed case leaves the count"
    );
    assert_eq!(
        w.m.inbox_count(&Who::Staff(&nobody)).await.unwrap_err(),
        ModError::MissingPermission("MANAGE_CASES")
    );
    let gm = Who::GuildMod {
        user_id: w.owner,
        guild_id: w.guild,
    };
    assert_eq!(
        w.m.inbox(&gm, None, 25).await.unwrap_err(),
        ModError::Forbidden
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn bulk_dismiss_needs_a_reason_and_reports_what_it_skipped() {
    let w = world().await;
    w.message(1).await;
    w.message(2).await;
    let a = w.report(w.reporter, 1, ReportCategory::ChildSafety).await;
    let b = w.report(w.reporter, 2, ReportCategory::ChildSafety).await;
    let c = caller(&w, P::MANAGE_CASES);
    let who = Who::Staff(&c);

    assert_eq!(
        w.m.dismiss_inbox(&who, &[a, b], None).await.unwrap_err(),
        ModError::ReasonRequired
    );
    assert!(matches!(
        w.m.dismiss_inbox(&who, &[], Some("duplicate")).await,
        Err(ModError::Invalid(_))
    ));
    let too_many: Vec<i64> = (1..=101).collect();
    assert!(matches!(
        w.m.dismiss_inbox(&who, &too_many, Some("x")).await,
        Err(ModError::Invalid(_))
    ));
    let weak = caller(&w, P::MANAGE_TAKEDOWNS);
    assert_eq!(
        w.m.dismiss_inbox(&Who::Staff(&weak), &[a], Some("x"))
            .await
            .unwrap_err(),
        ModError::MissingPermission("MANAGE_CASES")
    );

    // One is claimed meanwhile, one id is unknown, one is listed twice.
    w.m.assign(&who, b, true).await.unwrap();
    let out =
        w.m.dismiss_inbox(&who, &[a, b, a, 42], Some("spam wave"))
            .await
            .unwrap();
    assert_eq!(out.dismissed, vec![a]);
    let skipped: Vec<i64> = out.skipped.iter().map(|s| s.case_id).collect();
    assert_eq!(skipped, vec![b, 42]);

    let (state, outcome): (String, String) =
        sqlx::query_as("SELECT state, outcome FROM moderation_cases WHERE id = $1")
            .bind(a)
            .fetch_one(w.pool())
            .await
            .unwrap();
    assert_eq!(
        (state.as_str(), outcome.as_str()),
        ("dismissed", "spam wave")
    );
    // The normal path ran: a timeline event and an access-log row, and the case left the inbox.
    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM moderation_case_events WHERE case_id = $1 AND kind = 'state_changed'",
    )
    .bind(a)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(events, 1);
    let logged: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM staff_access_log WHERE action = 'moderation.case.state'",
    )
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(logged, 1);
    assert!(w.m.inbox(&who, None, 25).await.unwrap().items.is_empty());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn sections_follow_the_callers_bits() {
    let w = world().await;
    let none = caller(&w, P::empty());
    assert!(matches!(
        w.m.dashboard(&none, None).await,
        Err(ModError::MissingPermission(_))
    ));

    let cases =
        w.m.dashboard(&caller(&w, P::MANAGE_CASES), None)
            .await
            .unwrap();
    assert!(cases.inbox.is_some() && cases.health.is_some());
    assert!(cases.retention.is_some() && cases.age.is_some());
    assert!(cases.bans.is_none() && cases.takedowns.is_none() && cases.data_requests.is_none());

    let bans =
        w.m.dashboard(&caller(&w, P::PLATFORM_BAN), None)
            .await
            .unwrap();
    assert!(bans.bans.is_some());
    assert!(bans.inbox.is_none() && bans.health.is_none() && bans.age.is_none());

    let tk =
        w.m.dashboard(&caller(&w, P::MANAGE_TAKEDOWNS), Some(1000))
            .await
            .unwrap();
    assert_eq!(tk.window_days, 90, "the window is bounded");
    assert!(tk.takedowns.is_some() && tk.data_requests.is_some());
    assert!(tk.bans.is_none() && tk.retention.is_none());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn health_counts_reports_first_action_reopens_and_appeals() {
    let w = world().await;
    for n in 1..=3 {
        w.message(n).await;
    }
    let a = w.report(w.reporter, 1, ReportCategory::ChildSafety).await;
    let b = w.report(w.reporter, 2, ReportCategory::ChildSafety).await;
    w.report(w.reporter, 3, ReportCategory::Harassment).await; // a server's case: not counted
    let c = caller(&w, P::MANAGE_CASES);
    let who = Who::Staff(&c);

    let h = w.m.dashboard(&c, Some(7)).await.unwrap().health.unwrap();
    assert_eq!(h.reports_per_day.len(), 7);
    assert_eq!(
        h.reports_per_day.last().unwrap().reports,
        2,
        "today's two platform reports"
    );
    assert_eq!(h.reports, 2);
    assert_eq!(h.cases_opened, 2);
    assert_eq!(h.cases_waiting, 2);
    assert_eq!(h.median_first_action_seconds, None);
    assert_eq!(h.reopen_rate, None);

    // The first action on `a`: make it two minutes old to see the median.
    exec(
        w.pool(),
        "UPDATE moderation_cases SET opened_at = now() - interval '2 minutes' WHERE id = $1",
        &[a],
    )
    .await;
    w.m.set_state(
        &who,
        a,
        &Transition {
            to: CaseState::Dismissed,
            outcome: Some("not an issue".into()),
            reporter_line: None,
        },
    )
    .await
    .unwrap();
    w.m.set_state(
        &who,
        a,
        &Transition {
            to: CaseState::Closed,
            outcome: Some("done".into()),
            reporter_line: None,
        },
    )
    .await
    .unwrap();
    w.m.set_state(
        &who,
        b,
        &Transition {
            to: CaseState::Dismissed,
            outcome: Some("done".into()),
            reporter_line: None,
        },
    )
    .await
    .unwrap();
    w.m.set_state(
        &who,
        b,
        &Transition {
            to: CaseState::Closed,
            outcome: Some("done".into()),
            reporter_line: None,
        },
    )
    .await
    .unwrap();
    // `a` is reopened inside the window, `b` is not.
    w.m.reopen_case(&who, a, Some("new facts")).await.unwrap();

    let h = w.m.dashboard(&c, Some(7)).await.unwrap().health.unwrap();
    assert_eq!(h.cases_with_first_action, 2);
    let median = h.median_first_action_seconds.unwrap();
    assert!(
        (1..=121).contains(&median),
        "median {median}s sits between the two cases"
    );
    assert_eq!(h.cases_waiting, 0);
    assert_eq!((h.closed, h.reopened), (2, 1));
    assert_eq!(h.reopen_rate, Some(0.5));

    // Appeal outcomes by state, in the window.
    let banned = w.author;
    let person: i64 = sqlx::query_scalar("SELECT person_id FROM accounts WHERE id = $1")
        .bind(banned)
        .fetch_one(w.pool())
        .await
        .unwrap();
    let ban = uid();
    exec(
        w.pool(),
        "INSERT INTO person_bans (id, person_id, category, banned_by) VALUES ($1, $2, 'standard', $3)",
        &[ban, person, w.owner],
    )
    .await;
    sqlx::query(
        "INSERT INTO moderation_appeals (id, ban_id, appellant_account_id, message, state, decided_by, decided_at) \
         VALUES ($1, $2, $3, 'please', 'overturned', $4, now())",
    )
    .bind(uid())
    .bind(ban)
    .bind(banned)
    .bind(w.staff_id)
    .execute(w.pool())
    .await
    .unwrap();
    let h = w.m.dashboard(&c, Some(7)).await.unwrap().health.unwrap();
    assert_eq!(h.appeal_outcomes.len(), 1);
    assert_eq!(h.appeal_outcomes[0].key, "overturned");
    assert_eq!(h.appeal_outcomes[0].count, 1);
    assert_eq!(h.appeals_open, 0);
    // No staff member is named anywhere in the health figures.
    let json = serde_json::to_string(&h).unwrap();
    assert!(!json.contains(&w.staff_id.to_string()));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_retention_check_counts_what_a_hold_keeps_from_the_purge() {
    let w = world().await;
    let held = w.message(1).await;
    let free = w.message(2).await;
    let case = w.report(w.reporter, 1, ReportCategory::ChildSafety).await;
    let c = caller(&w, P::MANAGE_CASES);
    let who = Who::Staff(&c);
    // The report already froze message 1 (D-328). Both messages are deleted and due for purge.
    exec(
        w.pool(),
        "UPDATE messages SET purge_after = now() - interval '1 day' WHERE id = ANY(ARRAY[$1, $2])",
        &[held, free],
    )
    .await;
    let r = w.m.dashboard(&c, None).await.unwrap().retention.unwrap();
    let row = |k: &str| r.rows.iter().find(|x| x.key == k).unwrap().count;
    assert_eq!(row("messages_held"), 1, "only the held message is skipped");
    assert_eq!(r.holds_active, 1);
    assert_eq!(r.holds_review_due, 0);

    // A hold nobody has looked at for longer than the review limit is flagged.
    exec(
        w.pool(),
        "UPDATE moderation_holds SET created_at = now() - interval '400 days'",
        &[],
    )
    .await;
    let r = w.m.dashboard(&c, None).await.unwrap().retention.unwrap();
    assert_eq!(r.holds_review_due, 1);

    // Releasing it removes the row from the count.
    let hold: i64 = sqlx::query_scalar("SELECT id FROM moderation_holds LIMIT 1")
        .fetch_one(w.pool())
        .await
        .unwrap();
    w.m.release_hold(&who, hold, Some("done")).await.unwrap();
    let r = w.m.dashboard(&c, None).await.unwrap().retention.unwrap();
    assert_eq!(
        r.rows
            .iter()
            .find(|x| x.key == "messages_held")
            .unwrap()
            .count,
        0
    );
    assert_eq!(r.holds_active, 0);
    assert_eq!(r.holds_erasure_pending, 0);
    assert!(r.rows.iter().all(|x| !x.capped));
    let _ = (case, HoldScope::Message(held));

    // The sign-in purge (D-347) has no hold exception: the check shows what is still past the window and what the
    // last run of the housekeeping job deleted, read from the job's own row.
    assert_eq!(r.sign_ins.window_days, 180);
    assert_eq!((r.sign_ins.past_window, r.sign_ins.last_purged), (0, None));
    for (id, age) in [(uid(), 200), (uid(), 10)] {
        sqlx::query(
            "INSERT INTO login_events (id, account_id, ip, success, created_at) \
             VALUES ($1, $2, '192.0.2.1', true, now() - make_interval(days => $3))",
        )
        .bind(id)
        .bind(w.reporter)
        .bind(age)
        .execute(w.pool())
        .await
        .unwrap();
    }
    let r = w.m.dashboard(&c, None).await.unwrap().retention.unwrap();
    assert_eq!(
        r.sign_ins.past_window, 1,
        "one sign-in is older than the window"
    );
    exec(
        w.pool(),
        "INSERT INTO jobs (id, kind, payload) VALUES ($1, 'moderation.suspicion_housekeeping', '{}')",
        &[sr_moderation::HOUSEKEEPING_JOB_ID],
    )
    .await;
    assert_eq!(w.m.purge_login_events().await.unwrap(), (1, false));
    let r = w.m.dashboard(&c, None).await.unwrap().retention.unwrap();
    assert_eq!(r.sign_ins.past_window, 0);
    assert_eq!(r.sign_ins.last_purged, Some(1));
    assert!(r.sign_ins.last_run_at_ms.is_some() && !r.sign_ins.backlog);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn bans_by_reason_severity_and_age_and_the_list_filters() {
    let w = world().await;
    let pool = w.pool().clone();
    // Five bans: (reason, severity, age in days, lifted).
    let rows: [(Option<&str>, &str, i32, bool); 5] = [
        (Some("spam"), "standard", 2, false),
        (Some("fraud"), "serious", 20, false),
        (Some("fraud"), "serious", 200, false),
        (None, "standard", 60, false),
        (Some("spam"), "standard", 3, true),
    ];
    for (i, (reason, sev, age, lifted)) in rows.iter().enumerate() {
        let acc = account(&pool, &format!("banned{i}")).await;
        let person: i64 = sqlx::query_scalar("SELECT person_id FROM accounts WHERE id = $1")
            .bind(acc)
            .fetch_one(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO person_bans (id, person_id, category, reason_category, banned_by, created_at, lifted_at, lifted_by) \
             VALUES ($1, $2, $3::ban_category, $4::ban_reason_category, $5, now() - make_interval(days => $6), \
                     CASE WHEN $7 THEN now() END, CASE WHEN $7 THEN $5 END)",
        )
        .bind(uid())
        .bind(person)
        .bind(sev)
        .bind(reason)
        .bind(w.owner)
        .bind(age)
        .bind(lifted)
        .execute(&pool)
        .await
        .unwrap();
    }
    let b =
        w.m.dashboard(&caller(&w, P::PLATFORM_BAN), Some(30))
            .await
            .unwrap()
            .bans
            .unwrap();
    assert_eq!(b.active, 4, "a lifted ban is not counted");
    let count = |v: &[sr_moderation::dashboard::DashCount], k: &str| {
        v.iter().find(|c| c.key == k).map_or(0, |c| c.count)
    };
    assert_eq!(count(&b.by_reason, "fraud"), 2);
    assert_eq!(count(&b.by_reason, "spam"), 1);
    assert_eq!(count(&b.by_reason, "none"), 1);
    assert_eq!(b.by_reason[0].key, "fraud", "biggest group first");
    assert_eq!(count(&b.by_severity, "serious"), 2);
    assert_eq!(count(&b.by_severity, "standard"), 2);
    assert_eq!(count(&b.by_age, "under_7d"), 1);
    assert_eq!(count(&b.by_age, "7_to_30d"), 1);
    assert_eq!(count(&b.by_age, "30_to_90d"), 1);
    assert_eq!(count(&b.by_age, "over_90d"), 1);
    assert_eq!(b.lifted_in_window, 1);

    // The list filters (MOD-BAN-003), on the Postgres repository.
    let repo = PgAdminRepo::new(pool.clone(), Arc::new(uid));
    let list = |f: BanFilter| {
        let repo = repo.clone();
        async move { repo.list_bans(&f, None, 50).await.unwrap() }
    };
    assert_eq!(list(BanFilter::default()).await.len(), 5);
    assert_eq!(
        list(BanFilter {
            active_only: true,
            ..Default::default()
        })
        .await
        .len(),
        4
    );
    let fraud = list(BanFilter {
        reason_category: Some("fraud".into()),
        ..Default::default()
    })
    .await;
    assert_eq!(fraud.len(), 2);
    assert!(
        fraud
            .iter()
            .all(|b| b.reason_category.as_deref() == Some("fraud"))
    );
    let none = list(BanFilter {
        reason_category: Some("none".into()),
        ..Default::default()
    })
    .await;
    assert_eq!(none.len(), 1);
    assert_eq!(none[0].reason_category, None);
    assert_eq!(
        list(BanFilter {
            category: Some(BanCategory::Serious),
            ..Default::default()
        })
        .await
        .len(),
        2
    );
    assert_eq!(
        list(BanFilter {
            older_than_days: Some(30),
            ..Default::default()
        })
        .await
        .len(),
        2,
        "the 60 and 200 day old bans"
    );
    assert_eq!(
        list(BanFilter {
            newer_than_days: Some(7),
            ..Default::default()
        })
        .await
        .len(),
        2,
        "the 2 and 3 day old bans"
    );
    assert_eq!(
        list(BanFilter {
            active_only: true,
            category: Some(BanCategory::Standard),
            reason_category: Some("spam".into()),
            newer_than_days: Some(7),
            ..Default::default()
        })
        .await
        .len(),
        1
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn age_and_child_safety_metrics() {
    let w = world().await;
    w.message(1).await;
    w.report(w.reporter, 1, ReportCategory::ChildSafety).await;
    exec(
        w.pool(),
        "UPDATE channels SET nsfw = TRUE WHERE id = $1",
        &[w.channel],
    )
    .await;
    exec(
        w.pool(),
        "INSERT INTO age_gate_resets (source) VALUES ('self'), ('self')",
        &[],
    )
    .await;
    exec(
        w.pool(),
        "INSERT INTO age_gate_resets (source, reset_at) VALUES ('staff', now() - interval '100 days')",
        &[],
    )
    .await;
    let a =
        w.m.dashboard(&caller(&w, P::MANAGE_CASES), Some(30))
            .await
            .unwrap()
            .age
            .unwrap();
    assert_eq!(a.restricted_channels, 1);
    assert_eq!(a.age_gate_resets, 2, "the old reset is outside the window");
    assert_eq!(a.child_safety_open_cases, 1);
    assert_eq!(a.child_safety_reports, 1);
    assert_eq!(a.child_safety_active_bans, 0);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_register_and_the_data_requests_at_a_glance() {
    let w = world().await;
    let pool = w.pool().clone();
    let person: i64 = sqlx::query_scalar("SELECT person_id FROM accounts WHERE id = $1")
        .bind(w.author)
        .fetch_one(&pool)
        .await
        .unwrap();
    // Takedown notices: one overdue in triage, one fresh, one counter-noticed and restorable.
    for (state, due, restore) in [
        ("received", "now() - interval '2 days'", "NULL"),
        ("triage", "now() + interval '1 day'", "NULL"),
        (
            "counter_noticed",
            "now() - interval '9 days'",
            "now() - interval '1 hour'",
        ),
    ] {
        let sql = format!(
            "INSERT INTO takedown_notices (id, kind, state, notifier_name, notifier_contact, legal_basis, original_text, \
             received_at, entered_by, triage_due_at, restore_after, counter_noticed_at, removed_at) \
             VALUES ($1, 'copyright', '{state}', 'n', 'c', 'b', 't', now() - interval '3 days', $2, {due}, {restore}, \
             CASE WHEN '{state}' = 'counter_noticed' THEN now() END, \
             CASE WHEN '{state}' = 'counter_noticed' THEN now() - interval '2 days' END)"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(uid())
            .bind(w.owner)
            .execute(&pool)
            .await
            .unwrap();
    }
    // Data requests: an overdue export, an erasure due in three days (extended), a completed one.
    for (kind, state, due, ext, done) in [
        ("export", "open", "now() - interval '1 day'", "NULL", false),
        (
            "erasure",
            "verified",
            "now() - interval '1 day'",
            "now() + interval '3 days'",
            false,
        ),
        (
            "export",
            "completed",
            "now() + interval '10 days'",
            "NULL",
            true,
        ),
    ] {
        let acc = account(&pool, &format!("req{}{}", kind, uid())).await;
        let sql = format!(
            "INSERT INTO data_requests (id, kind, person_id, account_id, state, received_at, due_at, extended_until, \
             extension_reason, extended_by, extended_at, outcome, completed_at, created_by) \
             VALUES ($1, '{kind}', (SELECT person_id FROM accounts WHERE id = $2), $2, '{state}', now() - interval '20 days', \
             {due}, {ext}, CASE WHEN {ext} IS NULL THEN NULL ELSE 'complex' END, \
             CASE WHEN {ext} IS NULL THEN NULL ELSE $3 END, CASE WHEN {ext} IS NULL THEN NULL ELSE now() END, \
             CASE WHEN {done} THEN 'exported' END, CASE WHEN {done} THEN now() END, $3)"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(uid())
            .bind(acc)
            .bind(w.owner)
            .execute(&pool)
            .await
            .unwrap();
    }
    let _ = person;
    let d =
        w.m.dashboard(&caller(&w, P::MANAGE_TAKEDOWNS), Some(30))
            .await
            .unwrap();
    let t = d.takedowns.unwrap();
    assert_eq!((t.undecided, t.triage_overdue), (2, 1));
    assert_eq!((t.counter_noticed, t.restore_due), (1, 1));
    assert_eq!(t.removed_in_window, 1);
    assert_eq!(t.received_in_window, 3);
    let r = d.data_requests.unwrap();
    assert_eq!((r.open_exports, r.open_erasures), (1, 1));
    assert_eq!(r.overdue, 1, "the extension moves the erasure's deadline");
    assert_eq!(r.due_soon, 1);
    assert_eq!(r.completed_in_window, 1);
}
