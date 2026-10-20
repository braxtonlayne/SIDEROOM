//! Postgres-backed tests of the moderation backend: reports, routing and reporter privacy, evidence, cases,
//! holds, appeals. Run with a scratch Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-moderation --test db -- --ignored --test-threads=1`

use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use sqlx::PgPool;
use sr_admin::Caller;
use sr_core::PlatformPermissions as P;
use sr_db::{
    accounts::{self, NewAccount},
    testing::TestDb,
};
use sr_moderation::{
    HoldScope, ModError, Moderation, Transition,
    model::{
        AppealDecision, CaseFilter, CaseSeverity, CaseState, NewCase, NewReport, ReportCategory,
        ReportState, Routing, TargetKind, Who,
    },
};

static SEQ: AtomicI64 = AtomicI64::new(0);

fn uid() -> i64 {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + SEQ.fetch_add(1, Ordering::Relaxed) % 1000
}

fn mods(pool: PgPool) -> Moderation {
    Moderation::new(pool, Arc::new(uid))
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

/// A guild with a text channel and the given members; returns (guild, channel).
async fn guild_with(pool: &PgPool, owner: i64, members: &[i64]) -> (i64, i64) {
    let (g, c) = (uid(), uid());
    exec(
        pool,
        "INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Club', $2)",
        &[g, owner],
    )
    .await;
    exec(
        pool,
        "INSERT INTO channels (id, guild_id, type, name) VALUES ($1, $2, 0, 'general')",
        &[c, g],
    )
    .await;
    for m in std::iter::once(&owner).chain(members) {
        exec(
            pool,
            "INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            &[g, *m],
        )
        .await;
    }
    (g, c)
}

/// Ids of messages created a few minutes ago (ids embed the time), in a partition that exists.
async fn message_base(pool: &PgPool) -> i64 {
    let _: String = sqlx::query_scalar(
        "SELECT sr_ensure_message_partition((now() - interval '10 minutes')::date)",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query_scalar("SELECT sr_snowflake_floor(now() - interval '10 minutes') + 1000")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn message(pool: &PgPool, id: i64, channel: i64, author: i64, text: &str) {
    sqlx::query(
        "INSERT INTO messages (id, channel_id, author_id, content) VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(channel)
    .bind(author)
    .bind(text)
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

fn report(kind: TargetKind, id: i64, channel: Option<i64>, cat: ReportCategory) -> NewReport {
    NewReport {
        target_kind: kind,
        target_id: id,
        channel_id: channel,
        guild_id: None,
        category: cat,
        note: "this is bad".into(),
        moderators_may_see_name: false,
    }
}

async fn log_count(pool: &PgPool, action: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM staff_access_log WHERE action = $1")
        .bind(action)
        .fetch_one(pool)
        .await
        .unwrap()
}

struct World {
    db: TestDb,
    m: Moderation,
    owner: i64,
    author: i64,
    reporter: i64,
    reporter2: i64,
    staff_id: i64,
    other_staff_id: i64,
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
    let other_staff_id = account(&pool, "staff2").await;
    let (guild, channel) = guild_with(&pool, owner, &[author, reporter, reporter2]).await;
    let base = message_base(&pool).await;
    World {
        m: mods(pool),
        db,
        owner,
        author,
        reporter,
        reporter2,
        staff_id,
        other_staff_id,
        guild,
        channel,
        base,
    }
}

impl World {
    fn pool(&self) -> &PgPool {
        self.db.pool()
    }

    /// Thirty messages in the channel by the author; the target is the 15th.
    async fn talk(&self) -> i64 {
        for i in 0..30 {
            message(
                self.pool(),
                self.base + i,
                self.channel,
                self.author,
                &format!("line {i}"),
            )
            .await;
        }
        self.base + 14
    }

    fn gm(&self) -> Who<'static> {
        Who::GuildMod {
            user_id: self.owner,
            guild_id: self.guild,
        }
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn report_snapshots_evidence_routes_to_the_guild_and_freezes_the_message() {
    let w = world().await;
    let target = w.talk().await;
    exec(
        w.pool(),
        "INSERT INTO attachments (id, message_id, uploader_id, filename, size_bytes, storage_key) VALUES ($1, $2, $3, 'a.png', 10, 'k')",
        &[uid(), target, w.author],
    )
    .await;

    let r =
        w.m.file_report(
            w.reporter,
            &report(
                TargetKind::Message,
                target,
                Some(w.channel),
                ReportCategory::Harassment,
            ),
        )
        .await
        .unwrap();
    assert_eq!(r.state, ReportState::Received);

    let page =
        w.m.list_cases(
            &w.gm(),
            &CaseFilter {
                limit: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    let case = &page.items[0];
    assert_eq!(case.routing, Routing::Guild);
    assert_eq!(case.severity, CaseSeverity::Standard);
    assert_eq!(case.guild_id, Some(w.guild));
    assert_eq!(case.active_holds, 1, "the reported message is frozen");

    let held: i64 = sqlx::query_scalar("SELECT count(*) FROM messages WHERE hold_id IS NOT NULL")
        .fetch_one(w.pool())
        .await
        .unwrap();
    assert_eq!(held, 1);
    let held_att: i64 =
        sqlx::query_scalar("SELECT count(*) FROM attachments WHERE hold_id IS NOT NULL")
            .fetch_one(w.pool())
            .await
            .unwrap();
    assert_eq!(held_att, 1);

    let d = w.m.get_case(&w.gm(), case.id).await.unwrap();
    let snap = &d.evidence[0].payload;
    let msgs = snap["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 21, "ten before, the message, ten after");
    assert_eq!(msgs.iter().filter(|m| m["target"] == true).count(), 1);
    assert_eq!(msgs[10]["content"], "line 14");
    assert_eq!(msgs[10]["attachments"][0]["filename"], "a.png");

    // Editing or deleting the message afterwards does not change the evidence.
    sqlx::query("UPDATE messages SET content = 'edited', visibility = 'user_deleted', deleted_at = now() WHERE id = $1")
        .bind(target)
        .execute(w.pool())
        .await
        .unwrap();
    let again = w.m.get_case(&w.gm(), case.id).await.unwrap();
    assert_eq!(
        again.evidence[0].payload["messages"][10]["content"],
        "line 14"
    );
    // And the table refuses to change it at all.
    let err = sqlx::query("UPDATE moderation_case_evidence SET payload = '{}'")
        .execute(w.pool())
        .await;
    assert!(err.is_err());
    assert!(
        sqlx::query("DELETE FROM moderation_case_events")
            .execute(w.pool())
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn one_open_report_per_target_duplicates_attach_and_the_daily_limit_holds() {
    let w = world().await;
    let target = w.talk().await;
    let req = report(
        TargetKind::Message,
        target,
        Some(w.channel),
        ReportCategory::Spam,
    );
    w.m.file_report(w.reporter, &req).await.unwrap();
    assert!(matches!(
        w.m.file_report(w.reporter, &req).await,
        Err(ModError::Conflict(_))
    ));
    // A second reporter attaches to the same open case.
    w.m.file_report(w.reporter2, &req).await.unwrap();
    let page =
        w.m.list_cases(
            &w.gm(),
            &CaseFilter {
                limit: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].report_count, 2);

    // Your own message, a message you cannot see, and an unknown one are refused.
    let mine = w.base + 3;
    assert!(matches!(
        w.m.file_report(
            w.author,
            &report(
                TargetKind::Message,
                mine,
                Some(w.channel),
                ReportCategory::Spam
            )
        )
        .await,
        Err(ModError::Invalid(_))
    ));
    let stranger = account(w.pool(), "stranger").await;
    assert_eq!(
        w.m.file_report(
            stranger,
            &report(
                TargetKind::Message,
                mine,
                Some(w.channel),
                ReportCategory::Spam
            )
        )
        .await,
        Err(ModError::NotFound("message"))
    );
    assert_eq!(
        w.m.file_report(
            w.reporter,
            &report(
                TargetKind::Message,
                5,
                Some(w.channel),
                ReportCategory::Spam
            )
        )
        .await,
        Err(ModError::NotFound("message"))
    );

    // The daily limit is a platform setting.
    sqlx::query("INSERT INTO platform_settings (key, value) VALUES ('moderation.reports_per_day', '2'::jsonb)")
        .execute(w.pool())
        .await
        .unwrap();
    w.m.file_report(
        w.reporter,
        &report(
            TargetKind::Message,
            w.base + 1,
            Some(w.channel),
            ReportCategory::Spam,
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        w.m.file_report(
            w.reporter,
            &report(
                TargetKind::Message,
                w.base + 2,
                Some(w.channel),
                ReportCategory::Spam
            )
        )
        .await,
        Err(ModError::RateLimited)
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn reporter_is_hidden_from_moderators_unless_they_allowed_it_and_platform_reads_are_logged() {
    let w = world().await;
    let target = w.talk().await;
    w.m.file_report(
        w.reporter,
        &report(
            TargetKind::Message,
            target,
            Some(w.channel),
            ReportCategory::Harassment,
        ),
    )
    .await
    .unwrap();
    let mut open = report(
        TargetKind::Message,
        target,
        Some(w.channel),
        ReportCategory::Harassment,
    );
    open.moderators_may_see_name = true;
    w.m.file_report(w.reporter2, &open).await.unwrap();
    let case =
        w.m.list_cases(
            &w.gm(),
            &CaseFilter {
                limit: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .items[0]
            .id;

    let d = w.m.get_case(&w.gm(), case).await.unwrap();
    assert_eq!(d.reports.len(), 2);
    assert!(
        d.reports[0].reporter.is_none(),
        "the first reporter stays hidden"
    );
    assert_eq!(
        d.reports[1].reporter.as_ref().map(|r| r.username.as_str()),
        Some("reporter2")
    );
    // The timeline never carries a reporter.
    for e in &d.events {
        assert!(e.actor_id.is_none() || e.actor_id == Some(w.owner));
    }

    // Platform staff do not see a server's case until the server's moderators hand it over (D-209).
    let s = staff(w.staff_id, P::MANAGE_CASES | P::STAFF_BASIC);
    let staff_who = Who::Staff(&s);
    assert_eq!(
        w.m.get_case(&staff_who, case).await.err(),
        Some(ModError::NotFound("case"))
    );
    assert!(
        w.m.list_cases(
            &staff_who,
            &CaseFilter {
                limit: 10,
                ..Default::default()
            }
        )
        .await
        .unwrap()
        .items
        .is_empty()
    );

    // Escalating needs a reason, only moderators do it, and platform staff then see the reporters.
    assert_eq!(
        w.m.escalate(&w.gm(), case, None).await.err(),
        Some(ModError::ReasonRequired)
    );
    assert!(w.m.escalate(&staff_who, case, Some("x")).await.is_err());
    let before = log_count(w.pool(), "moderation.reporter.read").await;
    w.m.escalate(&w.gm(), case, Some("needs the platform"))
        .await
        .unwrap();
    // Server moderators keep a list row and lose the detail.
    let row =
        w.m.list_cases(
            &w.gm(),
            &CaseFilter {
                limit: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(row.items[0].escalated);
    assert_eq!(
        w.m.get_case(&w.gm(), case).await.err(),
        Some(ModError::NotFound("case"))
    );
    let d = w.m.get_case(&staff_who, case).await.unwrap();
    assert!(d.reports.iter().all(|r| r.reporter.is_some()));
    assert_eq!(
        log_count(w.pool(), "moderation.reporter.read").await,
        before + 1,
        "opening the reporters is access-logged"
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn dm_reports_go_to_platform_staff_with_a_logged_private_snapshot() {
    let w = world().await;
    let dm = uid();
    exec(
        w.pool(),
        "INSERT INTO channels (id, type) VALUES ($1, 1)",
        &[dm],
    )
    .await;
    for u in [w.author, w.reporter] {
        exec(
            w.pool(),
            "INSERT INTO channel_recipients (channel_id, user_id) VALUES ($1, $2)",
            &[dm, u],
        )
        .await;
    }
    message(w.pool(), w.base + 1, dm, w.author, "secret threat").await;
    let stranger = account(w.pool(), "nosy").await;
    assert_eq!(
        w.m.file_report(
            stranger,
            &report(
                TargetKind::Message,
                w.base + 1,
                Some(dm),
                ReportCategory::Harassment
            )
        )
        .await,
        Err(ModError::NotFound("message")),
        "only a participant can report a DM"
    );
    w.m.file_report(
        w.reporter,
        &report(
            TargetKind::Message,
            w.base + 1,
            Some(dm),
            ReportCategory::Harassment,
        ),
    )
    .await
    .unwrap();

    // Server moderators cannot read it; platform staff can, and the snapshot read is logged with the case id.
    assert!(
        w.m.list_cases(
            &w.gm(),
            &CaseFilter {
                limit: 10,
                ..Default::default()
            }
        )
        .await
        .unwrap()
        .items
        .is_empty()
    );
    let s = staff(w.staff_id, P::MANAGE_CASES);
    let who = Who::Staff(&s);
    let page =
        w.m.list_cases(
            &who,
            &CaseFilter {
                limit: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items[0].routing, Routing::Platform);
    let id = page.items[0].id;
    let d = w.m.get_case(&who, id).await.unwrap();
    assert_eq!(
        d.evidence[0].payload["messages"][0]["content"],
        "secret threat"
    );
    let row: (String, String) = sqlx::query_as(
        "SELECT kind, reason FROM staff_access_log WHERE action = 'moderation.evidence.read'",
    )
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(row, ("private_read".to_owned(), format!("case {id}")));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn child_safety_and_library_reports_skip_the_guild() {
    let w = world().await;
    let target = w.talk().await;
    w.m.file_report(
        w.reporter,
        &report(
            TargetKind::Message,
            target,
            Some(w.channel),
            ReportCategory::ChildSafety,
        ),
    )
    .await
    .unwrap();
    assert!(
        w.m.list_cases(
            &w.gm(),
            &CaseFilter {
                limit: 10,
                ..Default::default()
            }
        )
        .await
        .unwrap()
        .items
        .is_empty(),
        "never for the server's moderators"
    );
    let s = staff(w.staff_id, P::MANAGE_CASES);
    let page =
        w.m.list_cases(
            &Who::Staff(&s),
            &CaseFilter {
                limit: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].severity, CaseSeverity::Serious);

    // A later illegal-content report on a guild case flips its routing and raises it.
    let t2 = w.base + 20;
    w.m.file_report(
        w.reporter,
        &report(
            TargetKind::Message,
            t2,
            Some(w.channel),
            ReportCategory::Spam,
        ),
    )
    .await
    .unwrap();
    w.m.file_report(
        w.reporter2,
        &report(
            TargetKind::Message,
            t2,
            Some(w.channel),
            ReportCategory::IllegalContent,
        ),
    )
    .await
    .unwrap();
    let guild_view =
        w.m.list_cases(
            &w.gm(),
            &CaseFilter {
                limit: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(
        guild_view.items.is_empty(),
        "the system handed it over silently"
    );
    assert_eq!(
        w.m.list_cases(
            &Who::Staff(&s),
            &CaseFilter {
                limit: 10,
                ..Default::default()
            }
        )
        .await
        .unwrap()
        .items
        .len(),
        2
    );

    // Library items go to the reviewers.
    let series = uid();
    exec(
        w.pool(),
        "INSERT INTO reading_series (id, kind, title) VALUES ($1, 'comic', 'Comic')",
        &[series],
    )
    .await;
    w.m.file_report(
        w.reporter,
        &report(TargetKind::Library, series, None, ReportCategory::Other),
    )
    .await
    .unwrap();
    let reviewer = staff(w.other_staff_id, P::REVIEW_READING);
    let rv =
        w.m.list_cases(
            &Who::Staff(&reviewer),
            &CaseFilter {
                limit: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(rv.items.len(), 1);
    assert_eq!(rv.items[0].routing, Routing::Reviewers);
    let nobody = staff(w.other_staff_id, P::STAFF_BASIC);
    assert!(
        w.m.list_cases(
            &Who::Staff(&nobody),
            &CaseFilter {
                limit: 10,
                ..Default::default()
            }
        )
        .await
        .is_err()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn case_lifecycle_notes_claims_and_reopening() {
    let w = world().await;
    let target = w.talk().await;
    w.m.file_report(
        w.reporter,
        &report(
            TargetKind::Message,
            target,
            Some(w.channel),
            ReportCategory::Spam,
        ),
    )
    .await
    .unwrap();
    let gm = w.gm();
    let id =
        w.m.list_cases(
            &gm,
            &CaseFilter {
                limit: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .items[0]
            .id;

    // Closing straight from open is not a move; closing needs an outcome.
    let to = |s: CaseState, o: Option<&str>| Transition {
        to: s,
        outcome: o.map(str::to_owned),
        reporter_line: Some("Thanks, we removed it.".into()),
    };
    assert!(matches!(
        w.m.set_state(&gm, id, &to(CaseState::Closed, Some("x")))
            .await,
        Err(ModError::Conflict(_))
    ));
    let s =
        w.m.set_state(&gm, id, &to(CaseState::Investigating, None))
            .await
            .unwrap();
    assert_eq!(s.state, CaseState::Investigating);
    let mine = w.m.my_reports(w.reporter, 10).await.unwrap();
    assert_eq!(mine[0].state, ReportState::UnderReview);

    let claimed = w.m.assign(&gm, id, true).await.unwrap();
    assert_eq!(claimed.assignee_id, Some(w.owner));
    let n = w.m.add_note(&gm, id, "  looks like spam  ").await.unwrap();
    assert_eq!(n.body, "looks like spam");
    assert!(w.m.add_note(&gm, id, "   ").await.is_err());

    w.m.set_state(&gm, id, &to(CaseState::Dismissed, Some("Not spam")))
        .await
        .unwrap();
    assert_eq!(
        w.m.set_state(&gm, id, &to(CaseState::Closed, None))
            .await
            .err(),
        Some(ModError::Invalid(
            "closing a case needs the outcome in a few words".into()
        ))
    );
    let closed =
        w.m.set_state(&gm, id, &to(CaseState::Closed, Some("Not spam")))
            .await
            .unwrap();
    assert!(closed.closed_at_ms.is_some());
    let mine = w.m.my_reports(w.reporter, 10).await.unwrap();
    assert_eq!(mine[0].state, ReportState::Closed);
    assert_eq!(mine[0].outcome_line, "Thanks, we removed it.");

    // Inside the window it reopens; outside, a new case links back.
    assert_eq!(
        w.m.reopen_case(&gm, id, None).await.err(),
        Some(ModError::ReasonRequired)
    );
    let r =
        w.m.reopen_case(&gm, id, Some("new evidence"))
            .await
            .unwrap();
    assert_eq!(r.id, id);
    assert_eq!(r.state, CaseState::Investigating);
    w.m.set_state(&gm, id, &to(CaseState::Dismissed, Some("Still not")))
        .await
        .unwrap();
    w.m.set_state(&gm, id, &to(CaseState::Closed, Some("Still not")))
        .await
        .unwrap();
    sqlx::query("UPDATE moderation_cases SET closed_at = now() - interval '40 days' WHERE id = $1")
        .bind(id)
        .execute(w.pool())
        .await
        .unwrap();
    let later = w.m.reopen_case(&gm, id, Some("again")).await.unwrap();
    assert_ne!(
        later.id, id,
        "past the window a new case continues the old one"
    );
    let d = w.m.get_case(&gm, later.id).await.unwrap();
    assert_eq!(d.linked_case_id, Some(id));

    // The timeline kept it all.
    let old = w.m.get_case(&gm, id).await.unwrap();
    let kinds: Vec<&str> = old.events.iter().map(|e| e.kind.as_str()).collect();
    for k in [
        "case_opened",
        "state_changed",
        "claimed",
        "note_added",
        "reopened",
        "continued_in",
    ] {
        assert!(kinds.contains(&k), "missing {k}: {kinds:?}");
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn staff_open_cases_warn_and_ban_notes_and_everything_is_logged() {
    let w = world().await;
    let s = staff(w.staff_id, P::MANAGE_CASES);
    let who = Who::Staff(&s);
    let c =
        w.m.open_case(
            &who,
            &NewCase {
                category: ReportCategory::Fraud,
                target_kind: TargetKind::User,
                target_id: Some(w.author),
                subject_account_id: Some(w.author),
                summary: "Chargebacks".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(c.severity, CaseSeverity::Serious);
    assert_eq!(c.routing, Routing::Platform);
    // Server moderators cannot open cases through this call.
    assert!(
        w.m.open_case(
            &w.gm(),
            &NewCase {
                category: ReportCategory::Spam,
                target_kind: TargetKind::Other,
                target_id: None,
                subject_account_id: None,
                summary: String::new(),
            }
        )
        .await
        .is_err()
    );
    let weak = staff(w.other_staff_id, P::STAFF_BASIC | P::DELETE_CONTENT);
    assert_eq!(
        w.m.open_case(
            &Who::Staff(&weak),
            &NewCase {
                category: ReportCategory::Spam,
                target_kind: TargetKind::Other,
                target_id: None,
                subject_account_id: None,
                summary: String::new(),
            }
        )
        .await
        .err(),
        Some(ModError::MissingPermission("MANAGE_CASES"))
    );
    assert!(
        !w.m.warn(&who, c.id, "Please stop").await.unwrap(),
        "no notifier is wired in this test"
    );
    let after =
        w.m.record_action(&who, c.id, "timeout", "10 minutes")
            .await
            .unwrap();
    assert_eq!(after.state, CaseState::Actioned);
    assert!(w.m.record_action(&who, c.id, "explode", "").await.is_err());
    for a in [
        "moderation.case.open",
        "moderation.case.warn",
        "moderation.case.action",
    ] {
        assert_eq!(log_count(w.pool(), a).await, 1, "{a}");
    }

    // A platform ban made through the admin service is noted on the case with its reason category.
    let person: i64 = sqlx::query_scalar("SELECT person_id FROM accounts WHERE id = $1")
        .bind(w.author)
        .fetch_one(w.pool())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO person_bans (id, person_id, category, note, banned_by) VALUES ($1, $2, 'serious', 'n', $3)",
    )
    .bind(uid())
    .bind(person)
    .bind(w.staff_id)
    .execute(w.pool())
    .await
    .unwrap();
    let ban =
        w.m.record_platform_ban(&who, c.id, ReportCategory::Fraud, "chargebacks")
            .await
            .unwrap();
    let rc: Option<String> =
        sqlx::query_scalar("SELECT reason_category::text FROM person_bans WHERE id = $1")
            .bind(ban)
            .fetch_one(w.pool())
            .await
            .unwrap();
    assert_eq!(rc.as_deref(), Some("fraud"));
    // The ban is on the timeline with its id, which is how an appeal finds the case (N-25).
    let detail = w.m.get_case(&who, c.id).await.unwrap();
    let ev = detail
        .events
        .iter()
        .find(|e| e.kind == "platform_ban")
        .expect("platform_ban event");
    assert_eq!(ev.detail["ban_id"], ban.to_string());
    assert_eq!(ev.detail["reason_category"], "fraud");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn holds_are_platform_staff_only_and_release_makes_old_rows_purgeable() {
    let w = world().await;
    let target = w.talk().await;
    let s = staff(w.staff_id, P::MANAGE_CASES);
    let who = Who::Staff(&s);
    let case =
        w.m.open_case(
            &who,
            &NewCase {
                category: ReportCategory::IllegalContent,
                target_kind: TargetKind::Message,
                target_id: Some(target),
                subject_account_id: Some(w.author),
                summary: String::new(),
            },
        )
        .await
        .unwrap();

    // Server moderators do not place or release holds (D-213); neither does staff without the bit.
    assert_eq!(
        w.m.place_hold(&w.gm(), case.id, HoldScope::Message(target), Some("x"))
            .await
            .err(),
        Some(ModError::Forbidden)
    );
    let weak = staff(w.other_staff_id, P::DELETE_CONTENT | P::PLATFORM_BAN);
    assert_eq!(
        w.m.place_hold(
            &Who::Staff(&weak),
            case.id,
            HoldScope::Message(target),
            Some("x")
        )
        .await
        .err(),
        Some(ModError::MissingPermission("MANAGE_CASES"))
    );
    assert_eq!(
        w.m.place_hold(&who, case.id, HoldScope::Message(target), None)
            .await
            .err(),
        Some(ModError::ReasonRequired)
    );
    assert_eq!(
        w.m.place_hold(&who, case.id, HoldScope::Message(w.base - 1), Some("x"))
            .await
            .err(),
        Some(ModError::NotFound("message"))
    );

    exec(
        w.pool(),
        "INSERT INTO attachments (id, message_id, uploader_id, filename, size_bytes, storage_key) VALUES ($1, $2, $3, 'a.png', 10, 'k')",
        &[uid(), target, w.author],
    )
    .await;
    let out =
        w.m.place_hold(&who, case.id, HoldScope::Message(target), Some("evidence"))
            .await
            .unwrap();
    assert_eq!((out.messages, out.attachments), (1, 1));
    let hold = out.hold_id.unwrap();
    assert!(matches!(
        w.m.place_hold(&who, case.id, HoldScope::Message(target), Some("again"))
            .await,
        Err(ModError::Conflict(_))
    ));
    assert_eq!(log_count(w.pool(), "moderation.hold.place").await, 1);

    // The author deleting a held message clears purge_after (D-122); the hold stays.
    sqlx::query("UPDATE messages SET visibility = 'user_deleted', deleted_at = now() - interval '60 days', purge_after = NULL WHERE id = $1")
        .bind(target)
        .execute(w.pool())
        .await
        .unwrap();

    let list = w.m.list_holds(&who, true, 50).await.unwrap();
    assert_eq!(list.len(), 1);
    assert!(!list[0].review_due);
    assert_eq!((list[0].messages, list[0].attachments), (1, 1));
    sqlx::query(
        "UPDATE moderation_holds SET created_at = now() - interval '100 days' WHERE id = $1",
    )
    .bind(hold)
    .execute(w.pool())
    .await
    .unwrap();
    assert!(w.m.list_holds(&who, true, 50).await.unwrap()[0].review_due);
    w.m.mark_hold_reviewed(&who, hold).await.unwrap();
    assert!(
        !w.m.list_holds(&who, true, 50).await.unwrap()[0].review_due,
        "reviewed holds leave the warning list"
    );

    // Release needs a reason, clears the freeze, and the old deleted message is purgeable at the next run.
    assert_eq!(
        w.m.release_hold(&who, hold, Some("  ")).await.err(),
        Some(ModError::ReasonRequired)
    );
    let rel =
        w.m.release_hold(&who, hold, Some("case closed"))
            .await
            .unwrap();
    assert_eq!(
        (rel.messages, rel.attachments, rel.purgeable_messages),
        (1, 1, 1)
    );
    let (hold_id, purge_past): (Option<i64>, bool) =
        sqlx::query_as("SELECT hold_id, purge_after < now() FROM messages WHERE id = $1")
            .bind(target)
            .fetch_one(w.pool())
            .await
            .unwrap();
    assert_eq!(hold_id, None);
    assert!(
        purge_past,
        "deleted 60 days ago, so due at the next job run"
    );
    assert!(matches!(
        w.m.release_hold(&who, hold, Some("again")).await,
        Err(ModError::Conflict(_))
    ));
    assert_eq!(log_count(w.pool(), "moderation.hold.release").await, 1);
    assert!(w.m.list_holds(&who, true, 50).await.unwrap().is_empty());
    assert_eq!(w.m.list_holds(&who, false, 50).await.unwrap().len(), 1);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn user_message_windows_and_submissions_can_be_held() {
    let w = world().await;
    w.talk().await;
    let s = staff(w.staff_id, P::MANAGE_CASES);
    let who = Who::Staff(&s);
    let case =
        w.m.open_case(
            &who,
            &NewCase {
                category: ReportCategory::Fraud,
                target_kind: TargetKind::User,
                target_id: Some(w.author),
                subject_account_id: Some(w.author),
                summary: String::new(),
            },
        )
        .await
        .unwrap();
    let now_ms: i64 = sqlx::query_scalar("SELECT (extract(epoch FROM now()) * 1000)::bigint")
        .fetch_one(w.pool())
        .await
        .unwrap();
    let out =
        w.m.place_hold(
            &who,
            case.id,
            HoldScope::UserMessages {
                account_id: w.author,
                since_ms: now_ms - 3_600_000,
                until_ms: now_ms + 3_600_000,
            },
            Some("fraud window"),
        )
        .await
        .unwrap();
    assert_eq!(out.messages, 30);
    // An inverted window is refused.
    assert!(
        w.m.place_hold(
            &who,
            case.id,
            HoldScope::UserMessages {
                account_id: w.author,
                since_ms: 10,
                until_ms: 5
            },
            Some("x"),
        )
        .await
        .is_err()
    );

    // Submission holds go through the same table.
    let sub = uid();
    sqlx::query("INSERT INTO submissions (id, kind, submitter_id, state, title) VALUES ($1, 'library.series', $2, 'rejected', 'Draft')")
        .bind(sub)
        .bind(w.author)
        .execute(w.pool())
        .await
        .unwrap();
    let out =
        w.m.place_hold(&who, case.id, HoldScope::Submission(sub), Some("copyright"))
            .await
            .unwrap();
    assert_eq!(out.submissions, 1);
    let held: Option<i64> = sqlx::query_scalar("SELECT hold_id FROM submissions WHERE id = $1")
        .bind(sub)
        .fetch_one(w.pool())
        .await
        .unwrap();
    assert!(held.is_some());
    let d = w.m.get_case(&who, case.id).await.unwrap();
    assert_eq!(d.holds.len(), 2);
    assert_eq!(d.access, "manage");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn appeals_one_per_ban_reviewed_by_somebody_else() {
    let w = world().await;
    let person: i64 = sqlx::query_scalar("SELECT person_id FROM accounts WHERE id = $1")
        .bind(w.author)
        .fetch_one(w.pool())
        .await
        .unwrap();
    let ban = uid();
    sqlx::query(
        "INSERT INTO person_bans (id, person_id, category, note, banned_by, reason_category) \
         VALUES ($1, $2, 'standard', 'n', $3, 'spam')",
    )
    .bind(ban)
    .bind(person)
    .bind(w.staff_id)
    .execute(w.pool())
    .await
    .unwrap();
    assert!(matches!(
        w.m.create_appeal(w.reporter, "unbanned person").await,
        Err(ModError::Conflict(_))
    ));
    assert!(w.m.create_appeal(w.author, "   ").await.is_err());
    let a = w.m.create_appeal(w.author, "It was not me").await.unwrap();
    assert_eq!(a.state, "open");
    assert_eq!(a.banned_by, Some(w.staff_id));
    assert!(
        matches!(
            w.m.create_appeal(w.author, "again").await,
            Err(ModError::Conflict(_))
        ),
        "no second appeal"
    );

    let banner = staff(w.staff_id, P::MANAGE_CASES);
    let other = staff(w.other_staff_id, P::MANAGE_CASES);
    // The appeal reached the queue as a case.
    let page =
        w.m.list_cases(
            &Who::Staff(&other),
            &CaseFilter {
                limit: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].category, ReportCategory::Spam);
    let detail =
        w.m.get_case(&Who::Staff(&other), page.items[0].id)
            .await
            .unwrap();
    assert_eq!(detail.appeal.as_ref().map(|x| x.id), Some(a.id));

    // The staff member who banned cannot decide, unless they are the Owner and nobody else can.
    assert_eq!(
        w.m.decide_appeal(
            &Who::Staff(&banner),
            a.id,
            AppealDecision::Upheld,
            Some("no"),
            true
        )
        .await
        .err(),
        Some(ModError::Forbidden)
    );
    assert_eq!(
        w.m.decide_appeal(
            &Who::Staff(&other),
            a.id,
            AppealDecision::Upheld,
            None,
            true
        )
        .await
        .err(),
        Some(ModError::ReasonRequired)
    );
    let weak = staff(w.other_staff_id, P::STAFF_BASIC);
    assert!(
        w.m.decide_appeal(
            &Who::Staff(&weak),
            a.id,
            AppealDecision::Upheld,
            Some("x"),
            true
        )
        .await
        .is_err()
    );
    let done =
        w.m.decide_appeal(
            &Who::Staff(&other),
            a.id,
            AppealDecision::Overturned,
            Some("mistaken identity"),
            true,
        )
        .await
        .unwrap();
    assert_eq!(done.state, "overturned");
    assert_eq!(done.decided_by, Some(w.other_staff_id));
    assert!(matches!(
        w.m.decide_appeal(
            &Who::Staff(&other),
            a.id,
            AppealDecision::Upheld,
            Some("x"),
            true
        )
        .await,
        Err(ModError::Conflict(_))
    ));
    let closed =
        w.m.get_case(&Who::Staff(&other), page.items[0].id)
            .await
            .unwrap();
    assert_eq!(closed.case.state, CaseState::Closed);
    assert_eq!(closed.outcome, "Appeal overturned");
    assert_eq!(log_count(w.pool(), "moderation.appeal.decide").await, 1);

    // The Owner alone may review their own decision.
    let owner = Caller {
        user_id: w.staff_id,
        permissions: P::OWNER,
        is_owner: true,
        ip: None,
    };
    let b = account(w.pool(), "other_banned").await;
    let bp: i64 = sqlx::query_scalar("SELECT person_id FROM accounts WHERE id = $1")
        .bind(b)
        .fetch_one(w.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO person_bans (id, person_id, category, note, banned_by) VALUES ($1, $2, 'standard', 'n', $3)")
        .bind(uid())
        .bind(bp)
        .bind(w.staff_id)
        .execute(w.pool())
        .await
        .unwrap();
    let a2 = w.m.create_appeal(b, "please").await.unwrap();
    assert_eq!(
        w.m.check_appeal_review(&Who::Staff(&owner), a2.id, true)
            .await
            .err(),
        Some(ModError::Forbidden)
    );
    assert!(
        w.m.check_appeal_review(&Who::Staff(&owner), a2.id, false)
            .await
            .is_ok()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn dismissal_releases_the_system_hold_and_reopening_puts_it_back() {
    let w = world().await;
    let target = w.talk().await;
    let other = w.base + 20;
    w.m.file_report(
        w.reporter,
        &report(
            TargetKind::Message,
            target,
            Some(w.channel),
            ReportCategory::Spam,
        ),
    )
    .await
    .unwrap();
    let gm = w.gm();
    let id =
        w.m.list_cases(
            &gm,
            &CaseFilter {
                limit: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .items[0]
            .id;
    let held = |m: i64| {
        let pool = w.pool().clone();
        async move {
            sqlx::query_scalar::<_, Option<i64>>("SELECT hold_id FROM messages WHERE id = $1")
                .bind(m)
                .fetch_one(&pool)
                .await
                .unwrap()
                .is_some()
        }
    };
    assert!(held(target).await);

    // The author deletes the reported message while it is held (no purge_after, D-122).
    sqlx::query("UPDATE messages SET visibility = 'user_deleted', deleted_at = now() - interval '60 days', purge_after = NULL WHERE id = $1")
        .bind(target)
        .execute(w.pool())
        .await
        .unwrap();
    // A staff hold on another message of the same case stays through the dismissal.
    sqlx::query("UPDATE moderation_cases SET routing = 'platform' WHERE id = $1")
        .bind(id)
        .execute(w.pool())
        .await
        .unwrap();
    let s = staff(w.staff_id, P::MANAGE_CASES);
    let who = Who::Staff(&s);
    w.m.place_hold(&who, id, HoldScope::Message(other), Some("related"))
        .await
        .unwrap();

    let t = |to, o: Option<&str>| Transition {
        to,
        outcome: o.map(str::to_owned),
        reporter_line: None,
    };
    let dismissed =
        w.m.set_state(&who, id, &t(CaseState::Dismissed, Some("not a breach")))
            .await
            .unwrap();
    assert_eq!(dismissed.active_holds, 1, "only the staff hold remains");
    assert!(!held(target).await, "the report hold is released");
    assert!(held(other).await, "a staff hold is not touched");
    let purge_past: bool =
        sqlx::query_scalar("SELECT purge_after < now() FROM messages WHERE id = $1")
            .bind(target)
            .fetch_one(w.pool())
            .await
            .unwrap();
    assert!(purge_past, "a message deleted while held becomes purgeable");
    let reason: String = sqlx::query_scalar(
        "SELECT release_reason FROM moderation_holds WHERE case_id = $1 AND created_by IS NULL",
    )
    .bind(id)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(reason, "case dismissed: not a breach");

    // Back to work: the report hold is placed again on a message that still exists.
    sqlx::query("UPDATE messages SET visibility = 'visible', deleted_at = NULL, purge_after = NULL WHERE id = $1")
        .bind(target)
        .execute(w.pool())
        .await
        .unwrap();
    let back =
        w.m.set_state(&who, id, &t(CaseState::Investigating, None))
            .await
            .unwrap();
    assert_eq!(back.active_holds, 2);
    assert!(held(target).await);

    // A message that is gone is skipped without a stray hold.
    w.m.set_state(&who, id, &t(CaseState::Dismissed, None))
        .await
        .unwrap();
    sqlx::query("DELETE FROM messages WHERE id = $1")
        .bind(target)
        .execute(w.pool())
        .await
        .unwrap();
    let again =
        w.m.set_state(&who, id, &t(CaseState::Investigating, None))
            .await
            .unwrap();
    assert_eq!(again.active_holds, 1);
}
