//! Postgres-backed tests of the takedown register (step 2): intake, triage, hold and disable, the DM, the strike
//! counter, counter-notices and the restore job, the access log, the export and the retention guards. Run with a
//! scratch Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-moderation --test takedowns -- --include-ignored --test-threads=1`

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicI64, Ordering},
};

use async_trait::async_trait;
use sqlx::PgPool;
use sr_admin::Caller;
use sr_core::PlatformPermissions as P;
use sr_db::{
    accounts::{self, NewAccount},
    testing::TestDb,
};
use sr_moderation::{
    ModError, Moderation, PolicySource, SubjectNotifier,
    blocklist::MediaBlocklist,
    model::{
        ItemKind, ItemState, NewItem, NewNotice, NoticeState, TakedownKind, TakedownPolicy, Who,
    },
    register_csv,
    takedown_rules::add_business_days,
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

struct Policy(TakedownPolicy);

#[async_trait]
impl PolicySource for Policy {
    async fn takedown_policy(&self) -> TakedownPolicy {
        self.0.clone()
    }
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

async fn count(pool: &PgPool, sql: &'static str, bind: i64) -> i64 {
    sqlx::query_scalar(sql)
        .bind(bind)
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
    dms: Arc<Dms>,
    staff_id: i64,
    uploader: i64,
    channel: i64,
    base: i64,
}

async fn world_with(policy: TakedownPolicy) -> World {
    let db = TestDb::new().await.expect("test db");
    let pool = db.pool().clone();
    let dms = Arc::new(Dms::default());
    let blocklist = Arc::new(MediaBlocklist::new(pool.clone()));
    let m = Moderation::new(pool.clone(), Arc::new(uid))
        .with_notifier(dms.clone())
        .with_policy(Arc::new(Policy(policy)))
        .with_blocklist(blocklist);
    let owner = account(&pool, "owner").await;
    let staff_id = account(&pool, "tkdstaff").await;
    let uploader = account(&pool, "uploader").await;
    let (g, channel) = (uid(), uid());
    exec(
        &pool,
        "INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Club', $2)",
        &[g, owner],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO channels (id, guild_id, type, name) VALUES ($1, $2, 0, 'general')",
        &[channel, g],
    )
    .await;
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
        db,
        m,
        dms,
        staff_id,
        uploader,
        channel,
        base,
    }
}

async fn world() -> World {
    world_with(TakedownPolicy::default()).await
}

impl World {
    fn pool(&self) -> &PgPool {
        self.db.pool()
    }

    fn caller(&self) -> Caller {
        staff(self.staff_id, P::MANAGE_TAKEDOWNS)
    }

    async fn message(&self, n: i64, author: i64) -> i64 {
        let id = self.base + n;
        sqlx::query(
            "INSERT INTO messages (id, channel_id, author_id, content) VALUES ($1, $2, $3, 'pirated text')",
        )
        .bind(id)
        .bind(self.channel)
        .bind(author)
        .execute(self.pool())
        .await
        .unwrap();
        id
    }

    async fn attachment(&self, message: i64) -> (i64, String) {
        let (id, key) = (uid(), format!("chat/{}/{}/film.mkv", self.channel, uid()));
        sqlx::query(
            "INSERT INTO attachments (id, message_id, uploader_id, filename, size_bytes, storage_key) \
             VALUES ($1, $2, $3, 'film.mkv', 10, $4)",
        )
        .bind(id)
        .bind(message)
        .bind(self.uploader)
        .bind(&key)
        .execute(self.pool())
        .await
        .unwrap();
        (id, key)
    }

    /// A published series with one published chapter; returns (series, chapter).
    async fn series(&self, owner: i64) -> (i64, i64) {
        let (s, e, c) = (uid(), uid(), uid());
        sqlx::query(
            "INSERT INTO reading_series (id, kind, title, created_by, published, published_at) \
             VALUES ($1, 'manga', 'Stolen Saga', $2, TRUE, now())",
        )
        .bind(s)
        .bind(owner)
        .execute(self.pool())
        .await
        .unwrap();
        exec(
            self.pool(),
            "INSERT INTO reading_editions (id, series_id, language, label, is_default) VALUES ($1, $2, 'en', 'Official', TRUE)",
            &[e, s],
        )
        .await;
        sqlx::query(
            "INSERT INTO reading_chapters (id, series_id, edition_id, number, published, published_at, created_by) \
             VALUES ($1, $2, $3, 1, TRUE, now(), $4)",
        )
        .bind(c)
        .bind(s)
        .bind(e)
        .bind(owner)
        .execute(self.pool())
        .await
        .unwrap();
        exec(
            self.pool(),
            "UPDATE reading_series SET chapter_count = 1 WHERE id = $1",
            &[s],
        )
        .await;
        (s, c)
    }

    /// A published film with one episode-less asset; returns (title, asset).
    async fn film(&self, owner: i64) -> (i64, i64) {
        let (t, a) = (uid(), uid());
        sqlx::query(
            "INSERT INTO video_titles (id, kind, title, sort_title, created_by, published, published_at, asset_id) \
             VALUES ($1, 'movie', 'Leaked Film', 'leaked film', $2, TRUE, now(), $3)",
        )
        .bind(t)
        .bind(owner)
        .bind(a)
        .execute(self.pool())
        .await
        .unwrap();
        (t, a)
    }

    fn notice(&self, kind: TakedownKind) -> NewNotice {
        NewNotice {
            kind,
            notifier_name: "Rights Holder Ltd".into(),
            notifier_contact: "legal@rights.example".into(),
            legal_basis: "Copyright in the work Stolen Saga".into(),
            locations: vec!["https://side.example/library/series/1".into()],
            original_text: "We are the owner. Please remove it.".into(),
            received_at_ms: None,
        }
    }

    /// Enter a notice and name the given items.
    async fn triaged(&self, items: &[(ItemKind, i64)]) -> i64 {
        let c = self.caller();
        let who = Who::Staff(&c);
        let n = self
            .m
            .create_notice(&who, &self.notice(TakedownKind::Copyright))
            .await
            .unwrap();
        for (kind, id) in items {
            self.m
                .add_item(
                    &who,
                    n.summary.id,
                    &NewItem {
                        kind: *kind,
                        id: *id,
                    },
                )
                .await
                .unwrap();
        }
        n.summary.id
    }
}

#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn intake_needs_the_bit_and_records_the_notice_and_its_case() {
    let w = world().await;
    // Without MANAGE_TAKEDOWNS nothing works, MANAGE_CASES included.
    let nobody = staff(w.staff_id, P::MANAGE_CASES);
    assert_eq!(
        w.m.create_notice(&Who::Staff(&nobody), &w.notice(TakedownKind::Copyright))
            .await
            .unwrap_err(),
        ModError::MissingPermission("MANAGE_TAKEDOWNS")
    );
    let c = w.caller();
    let who = Who::Staff(&c);
    let d =
        w.m.create_notice(&who, &w.notice(TakedownKind::Copyright))
            .await
            .unwrap();
    assert_eq!(d.summary.state, NoticeState::Received);
    assert_eq!(d.entered_by, w.staff_id);
    assert_eq!(d.locations.len(), 1);
    assert_eq!(d.original_text, "We are the owner. Please remove it.");
    // A case opened with the illegal_content category, handled by the platform.
    let case_id = d.summary.case_id.expect("case");
    let (cat, routing): (String, String) =
        sqlx::query_as("SELECT category, routing FROM moderation_cases WHERE id = $1")
            .bind(case_id)
            .fetch_one(w.pool())
            .await
            .unwrap();
    assert_eq!(
        (cat.as_str(), routing.as_str()),
        ("illegal_content", "platform")
    );
    // The triage target is one business day away.
    assert!(d.summary.triage_due_at_ms > d.summary.received_at_ms);
    assert!(!d.summary.triage_overdue);
    assert_eq!(log_count(w.pool(), "moderation.takedown.create").await, 1);
    // The required fields are required.
    let mut bad = w.notice(TakedownKind::Copyright);
    bad.notifier_name = "  ".into();
    assert!(matches!(
        w.m.create_notice(&who, &bad).await,
        Err(ModError::Invalid(_))
    ));
    // The register lists it and can filter by state.
    assert_eq!(w.m.list_notices(&who, None, 50).await.unwrap().len(), 1);
    assert!(
        w.m.list_notices(&who, Some(NoticeState::Removed), 50)
            .await
            .unwrap()
            .is_empty()
    );
    // Records are never deleted.
    let gone = sqlx::query("DELETE FROM takedown_notices WHERE id = $1")
        .bind(d.summary.id)
        .execute(w.pool())
        .await;
    assert!(gone.is_err());
}

#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn a_late_notice_is_marked_overdue() {
    let w = world().await;
    let c = w.caller();
    let mut n = w.notice(TakedownKind::IllegalContent);
    n.received_at_ms = Some(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
            - 10 * 86_400_000,
    );
    let d = w.m.create_notice(&Who::Staff(&c), &n).await.unwrap();
    assert!(d.summary.triage_overdue);
}

#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn triage_rejects_an_incomplete_notice_with_a_templated_reply() {
    let w = world().await;
    let c = w.caller();
    let who = Who::Staff(&c);
    let d =
        w.m.create_notice(&who, &w.notice(TakedownKind::Copyright))
            .await
            .unwrap();
    assert_eq!(
        w.m.reject_notice(&who, d.summary.id, None)
            .await
            .unwrap_err(),
        ModError::ReasonRequired
    );
    let out =
        w.m.reject_notice(&who, d.summary.id, Some("which chapter?"))
            .await
            .unwrap();
    assert!(out.reply.contains(&format!("TK-{}", d.summary.id)));
    assert!(out.reply.contains("which chapter?"));
    let after = w.m.get_notice(&who, d.summary.id).await.unwrap();
    assert_eq!(after.summary.state, NoticeState::Rejected);
    assert_eq!(after.rejection_reason, "which chapter?");
    // The case closed, and a rejected notice cannot be upheld.
    let state: String = sqlx::query_scalar("SELECT state FROM moderation_cases WHERE id = $1")
        .bind(d.summary.case_id)
        .fetch_one(w.pool())
        .await
        .unwrap();
    assert_eq!(state, "closed");
    assert!(matches!(
        w.m.uphold_notice(&who, d.summary.id, Some("x")).await,
        Err(ModError::Conflict(_))
    ));
    assert_eq!(log_count(w.pool(), "moderation.takedown.reject").await, 1);
}

#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn upholding_holds_disables_blocks_and_tells_the_uploader() {
    let w = world().await;
    let msg = w.message(1, w.uploader).await;
    let (att, key) = w.attachment(msg).await;
    let (series, chapter) = w.series(w.uploader).await;
    let (title, asset) = w.film(w.uploader).await;
    let id = w
        .triaged(&[
            (ItemKind::Message, msg),
            (ItemKind::LibrarySeries, series),
            (ItemKind::CinemaTitle, title),
        ])
        .await;
    let c = w.caller();
    let who = Who::Staff(&c);

    // Upholding without a reason is refused; nothing changed.
    assert_eq!(
        w.m.uphold_notice(&who, id, None).await.unwrap_err(),
        ModError::ReasonRequired
    );
    let out =
        w.m.uphold_notice(&who, id, Some("valid claim"))
            .await
            .unwrap();
    assert_eq!(out.items_disabled, 3);
    assert_eq!(out.notified, 1);
    assert_eq!(out.review_case_id, None);

    // The message is staff_removed and held (so the purge never takes it), its attachment held and blocked.
    let (vis, hold): (String, Option<i64>) =
        sqlx::query_as("SELECT visibility::text, hold_id FROM messages WHERE id = $1")
            .bind(msg)
            .fetch_one(w.pool())
            .await
            .unwrap();
    assert_eq!(vis, "staff_removed");
    assert!(hold.is_some());
    assert_eq!(
        count(
            w.pool(),
            "SELECT count(*) FROM attachments WHERE id = $1 AND hold_id IS NOT NULL",
            att
        )
        .await,
        1
    );
    // The series is unpublished and held; so is its chapter; the film is unpublished and held.
    assert_eq!(
        count(w.pool(), "SELECT count(*) FROM reading_series WHERE id = $1 AND NOT published AND hold_id IS NOT NULL", series).await,
        1
    );
    assert_eq!(
        count(
            w.pool(),
            "SELECT count(*) FROM reading_chapters WHERE id = $1 AND hold_id IS NOT NULL",
            chapter
        )
        .await,
        1
    );
    assert_eq!(
        count(w.pool(), "SELECT count(*) FROM video_titles WHERE id = $1 AND NOT published AND hold_id IS NOT NULL", title).await,
        1
    );
    // The stored objects are blocked on the media path.
    let blocklist = MediaBlocklist::new(w.pool().clone());
    assert!(blocklist.is_blocked_key(&key).await);
    assert!(
        blocklist
            .is_blocked_key(&format!("library/{series}/{chapter}/001.webp"))
            .await
    );
    assert!(blocklist.is_blocked_asset(asset).await);
    assert!(!blocklist.is_blocked_key("chat/other/object.png").await);
    // A held row cannot be deleted from under the hold.
    let del = sqlx::query("DELETE FROM reading_series WHERE id = $1")
        .bind(series)
        .execute(w.pool())
        .await;
    assert!(del.is_err());
    let del = sqlx::query("DELETE FROM video_titles WHERE id = $1")
        .bind(title)
        .execute(w.pool())
        .await;
    assert!(del.is_err());
    // The uploader got one system DM that names the removal and the counter-notice path, not the notifier.
    let dms = w.dms.0.lock().unwrap().clone();
    assert_eq!(dms.len(), 1);
    assert_eq!(dms[0].0, w.uploader);
    assert!(dms[0].1.contains(&format!("TK-{id}")));
    assert!(dms[0].1.contains("counter-notice"));
    assert!(!dms[0].1.contains("legal@rights.example"));
    assert!(!dms[0].1.contains("Rights Holder"));
    // The case moved to actioned; the notice is removed.
    let d = w.m.get_notice(&who, id).await.unwrap();
    assert_eq!(d.summary.state, NoticeState::Removed);
    assert!(
        d.items
            .iter()
            .all(|i| i.state == ItemState::Disabled && i.hold_id.is_some())
    );
    let case_state: String = sqlx::query_scalar("SELECT state FROM moderation_cases WHERE id = $1")
        .bind(d.summary.case_id)
        .fetch_one(w.pool())
        .await
        .unwrap();
    assert_eq!(case_state, "actioned");
    // Every state change is an access-log row.
    assert_eq!(log_count(w.pool(), "moderation.takedown.create").await, 1);
    assert_eq!(log_count(w.pool(), "moderation.takedown.item_add").await, 3);
    assert_eq!(log_count(w.pool(), "moderation.takedown.uphold").await, 1);
    // The item pages see it: the chapter answers for its series.
    assert_eq!(
        w.m.item_status(ItemKind::LibraryChapter, chapter)
            .await
            .unwrap()
            .state,
        "taken_down"
    );
    assert_eq!(
        w.m.item_status(ItemKind::CinemaTitle, title)
            .await
            .unwrap()
            .state,
        "taken_down"
    );
    assert_eq!(
        w.m.item_status(ItemKind::LibrarySeries, uid())
            .await
            .unwrap()
            .state,
        "none"
    );
}

#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn holding_first_shows_held_and_upholding_still_works() {
    let w = world().await;
    let (series, chapter) = w.series(w.uploader).await;
    let id = w.triaged(&[(ItemKind::LibraryChapter, chapter)]).await;
    let c = w.caller();
    let who = Who::Staff(&c);
    // Items can be named by mistake and taken back before they are held.
    let extra = w.message(2, w.uploader).await;
    let d =
        w.m.add_item(
            &who,
            id,
            &NewItem {
                kind: ItemKind::Message,
                id: extra,
            },
        )
        .await
        .unwrap();
    assert_eq!(d.items.len(), 2);
    let item = d
        .items
        .iter()
        .find(|i| i.kind == ItemKind::Message)
        .unwrap()
        .id;
    assert_eq!(
        w.m.remove_item(&who, id, item).await.unwrap().items.len(),
        1
    );
    // The same content twice is a conflict.
    assert!(matches!(
        w.m.add_item(
            &who,
            id,
            &NewItem {
                kind: ItemKind::LibraryChapter,
                id: chapter
            }
        )
        .await,
        Err(ModError::Conflict(_))
    ));

    let d =
        w.m.hold_notice(&who, id, Some("preserve evidence"))
            .await
            .unwrap();
    assert!(d.items.iter().all(|i| i.state == ItemState::Held));
    // Held is not removed: the chapter is still published, but the pages see the hold.
    assert_eq!(
        count(w.pool(), "SELECT count(*) FROM reading_chapters WHERE id = $1 AND published AND hold_id IS NOT NULL", chapter).await,
        1
    );
    assert_eq!(
        w.m.item_status(ItemKind::LibraryChapter, chapter)
            .await
            .unwrap()
            .state,
        "held"
    );
    // A held item cannot be taken off the notice any more.
    assert!(matches!(
        w.m.remove_item(&who, id, d.items[0].id).await,
        Err(ModError::Conflict(_))
    ));
    // Upholding disables what is held without a second hold.
    let out = w.m.uphold_notice(&who, id, Some("valid")).await.unwrap();
    assert_eq!(out.items_disabled, 1);
    assert_eq!(
        count(w.pool(), "SELECT count(*) FROM moderation_holds WHERE case_id IN (SELECT case_id FROM takedown_notices WHERE id = $1)", id).await,
        1
    );
    // The series stays as it was; only the chapter went.
    assert_eq!(
        count(
            w.pool(),
            "SELECT count(*) FROM reading_series WHERE id = $1 AND published",
            series
        )
        .await,
        1
    );
    assert_eq!(
        count(
            w.pool(),
            "SELECT chapter_count::bigint FROM reading_series WHERE id = $1",
            series
        )
        .await,
        0
    );
}

#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn a_counter_notice_schedules_the_restore_and_the_job_applies_it() {
    let w = world().await;
    let msg = w.message(3, w.uploader).await;
    let (_, key) = w.attachment(msg).await;
    let (series, chapter) = w.series(w.uploader).await;
    let id = w
        .triaged(&[(ItemKind::Message, msg), (ItemKind::LibrarySeries, series)])
        .await;
    let c = w.caller();
    let who = Who::Staff(&c);
    w.m.uphold_notice(&who, id, Some("valid")).await.unwrap();

    // A counter-notice can only follow a takedown, and needs text.
    assert!(matches!(
        w.m.record_counter_notice(&who, id, "  ").await,
        Err(ModError::Invalid(_))
    ));
    let d =
        w.m.record_counter_notice(&who, id, "I own this work")
            .await
            .unwrap();
    assert_eq!(d.summary.state, NoticeState::CounterNoticed);
    let (counter_ms, restore_ms) = (
        d.counter_noticed_at_ms.unwrap(),
        d.summary.restore_after_ms.unwrap(),
    );
    // Ten business days: never fewer than 14 calendar days over a Saturday-Sunday week boundary pair, never more
    // than 16, and exactly what the pure rule says.
    assert_eq!(
        restore_ms / 1000,
        add_business_days(
            counter_ms,
            10,
            sr_moderation::takedown_rules::Weekend::SAT_SUN
        ) / 1000
    );
    assert!((14..=16).contains(&((restore_ms - counter_ms) / 86_400_000)));
    assert_eq!(
        log_count(w.pool(), "moderation.takedown.counter_notice").await,
        1
    );

    // Too early: nothing is restored.
    assert!(
        w.m.run_due_restores(restore_ms - 1000)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        count(
            w.pool(),
            "SELECT count(*) FROM messages WHERE id = $1 AND visibility = 'staff_removed'",
            msg
        )
        .await,
        1
    );
    // The wait is over: the job restores everything.
    let restored = w.m.run_due_restores(restore_ms + 1000).await.unwrap();
    assert_eq!(restored, vec![id]);
    let after = w.m.get_notice(&who, id).await.unwrap();
    assert_eq!(after.summary.state, NoticeState::Restored);
    assert!(after.items.iter().all(|i| i.state == ItemState::Restored));
    assert_eq!(
        count(w.pool(), "SELECT count(*) FROM messages WHERE id = $1 AND visibility = 'visible' AND deleted_at IS NULL AND hold_id IS NULL", msg).await,
        1
    );
    assert_eq!(
        count(
            w.pool(),
            "SELECT count(*) FROM reading_series WHERE id = $1 AND published AND hold_id IS NULL",
            series
        )
        .await,
        1
    );
    assert_eq!(
        count(
            w.pool(),
            "SELECT count(*) FROM reading_chapters WHERE id = $1 AND hold_id IS NULL",
            chapter
        )
        .await,
        1
    );
    assert_eq!(
        count(
            w.pool(),
            "SELECT chapter_count::bigint FROM reading_series WHERE id = $1",
            series
        )
        .await,
        1
    );
    // Blocks are gone, the holds are released, the restore is a logged action, the uploader was told.
    let blocklist = MediaBlocklist::new(w.pool().clone());
    assert!(!blocklist.is_blocked_key(&key).await);
    assert_eq!(
        count(w.pool(), "SELECT count(*) FROM moderation_holds WHERE released_at IS NOT NULL AND case_id IN (SELECT case_id FROM takedown_notices WHERE id = $1)", id).await,
        2
    );
    assert_eq!(
        log_count(w.pool(), "moderation.takedown.auto_restore").await,
        1
    );
    assert_eq!(
        w.m.item_status(ItemKind::LibrarySeries, series)
            .await
            .unwrap()
            .state,
        "none"
    );
    let dms = w.dms.0.lock().unwrap().clone();
    assert_eq!(dms.len(), 2, "one for the removal, one for the restore");
    assert!(dms[1].1.contains("available again"));
    // The case closed; the job does nothing the second time.
    assert!(
        w.m.run_due_restores(restore_ms + 2000)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn a_noted_court_filing_stops_the_job_until_it_is_cleared() {
    let w = world().await;
    let (series, _) = w.series(w.uploader).await;
    let id = w.triaged(&[(ItemKind::LibrarySeries, series)]).await;
    let c = w.caller();
    let who = Who::Staff(&c);
    // A court filing makes no sense before the takedown.
    assert!(matches!(
        w.m.set_court_filing(&who, id, true, Some("filed")).await,
        Err(ModError::Conflict(_))
    ));
    w.m.uphold_notice(&who, id, Some("valid")).await.unwrap();
    let d = w.m.record_counter_notice(&who, id, "mine").await.unwrap();
    let due = d.summary.restore_after_ms.unwrap() + 1000;
    assert_eq!(
        w.m.set_court_filing(&who, id, true, None)
            .await
            .unwrap_err(),
        ModError::ReasonRequired
    );
    let d =
        w.m.set_court_filing(&who, id, true, Some("case no. 4711 filed"))
            .await
            .unwrap();
    assert!(d.summary.court_filing_noted);
    assert!(w.m.run_due_restores(due).await.unwrap().is_empty());
    assert_eq!(
        w.m.get_notice(&who, id).await.unwrap().summary.state,
        NoticeState::CounterNoticed
    );
    assert_eq!(
        log_count(w.pool(), "moderation.takedown.court_filing").await,
        1
    );
    // Cleared: the job restores.
    w.m.set_court_filing(&who, id, false, Some("case dropped"))
        .await
        .unwrap();
    assert_eq!(w.m.run_due_restores(due).await.unwrap(), vec![id]);
}

#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn staff_can_restore_early_or_the_notifier_can_withdraw() {
    let w = world().await;
    let msg = w.message(4, w.uploader).await;
    let (series, _) = w.series(w.uploader).await;
    let c = w.caller();
    let who = Who::Staff(&c);

    let a = w.triaged(&[(ItemKind::Message, msg)]).await;
    w.m.uphold_notice(&who, a, Some("valid")).await.unwrap();
    assert_eq!(
        w.m.restore_notice(&who, a, None).await.unwrap_err(),
        ModError::ReasonRequired
    );
    let d =
        w.m.restore_notice(&who, a, Some("sent in error"))
            .await
            .unwrap();
    assert_eq!(d.summary.state, NoticeState::Restored);
    assert_eq!(log_count(w.pool(), "moderation.takedown.restore").await, 1);

    let b = w.triaged(&[(ItemKind::LibrarySeries, series)]).await;
    w.m.uphold_notice(&who, b, Some("valid")).await.unwrap();
    let d =
        w.m.withdraw_notice(&who, b, Some("notifier retracted"))
            .await
            .unwrap();
    assert_eq!(d.summary.state, NoticeState::Withdrawn);
    assert_eq!(log_count(w.pool(), "moderation.takedown.withdraw").await, 1);
    // A restored notice cannot be restored again, nor counter-noticed.
    assert!(matches!(
        w.m.restore_notice(&who, a, Some("again")).await,
        Err(ModError::Conflict(_))
    ));
    assert!(matches!(
        w.m.record_counter_notice(&who, a, "late").await,
        Err(ModError::Conflict(_))
    ));
}

#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn the_strike_counter_opens_a_review_at_the_threshold_and_never_suspends() {
    let w = world_with(TakedownPolicy {
        strike_review_threshold: 3,
        strike_window_months: 12,
        operator_country: None,
        operator_contact: None,
    })
    .await;
    let c = w.caller();
    let who = Who::Staff(&c);
    let mut reviews = Vec::new();
    for i in 0..3 {
        let (series, _) = w.series(w.uploader).await;
        let id = w.triaged(&[(ItemKind::LibrarySeries, series)]).await;
        let out = w.m.uphold_notice(&who, id, Some("valid")).await.unwrap();
        reviews.push(out.review_case_id);
        assert_eq!(i < 2, out.review_case_id.is_none(), "strike {}", i + 1);
    }
    let review = reviews[2].expect("the third strike opens a review");
    let (cat, state, subject): (String, String, Option<i64>) = sqlx::query_as(
        "SELECT category, state, subject_account_id FROM moderation_cases WHERE id = $1",
    )
    .bind(review)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!((cat.as_str(), state.as_str()), ("illegal_content", "open"));
    assert_eq!(subject, Some(w.uploader));
    // Nobody was suspended or banned.
    let acct: String = sqlx::query_scalar("SELECT state::text FROM accounts WHERE id = $1")
        .bind(w.uploader)
        .fetch_one(w.pool())
        .await
        .unwrap();
    assert_eq!(acct, "active");
    assert_eq!(
        count(w.pool(), "SELECT count(*) FROM person_bans WHERE person_id = (SELECT person_id FROM accounts WHERE id = $1)", w.uploader).await,
        0
    );
    // A fourth strike while the review is open adds no second review.
    let (series, _) = w.series(w.uploader).await;
    let id = w.triaged(&[(ItemKind::LibrarySeries, series)]).await;
    let out = w.m.uphold_notice(&who, id, Some("valid")).await.unwrap();
    assert_eq!(out.review_case_id, None);
    let d = w.m.get_notice(&who, id).await.unwrap();
    assert_eq!(d.strikes[0].strikes, 4);
    assert_eq!(d.strikes[0].review_case_id, Some(review));
    // Restoring takes the strike back.
    w.m.restore_notice(&who, id, Some("mistake")).await.unwrap();
    assert_eq!(
        w.m.get_notice(&who, id).await.unwrap().strikes[0].strikes,
        3
    );
}

#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn takedowns_outside_the_window_do_not_count() {
    let w = world_with(TakedownPolicy {
        strike_review_threshold: 2,
        strike_window_months: 6,
        operator_country: None,
        operator_contact: None,
    })
    .await;
    let c = w.caller();
    let who = Who::Staff(&c);
    // One old takedown: upheld eight months ago.
    let (s1, _) = w.series(w.uploader).await;
    let old = w.triaged(&[(ItemKind::LibrarySeries, s1)]).await;
    w.m.uphold_notice(&who, old, Some("valid")).await.unwrap();
    exec(
        w.pool(),
        "UPDATE takedown_notices SET removed_at = now() - interval '8 months' WHERE id = $1",
        &[old],
    )
    .await;
    // The second strike is inside the window, but the first is not: no review.
    let (s2, _) = w.series(w.uploader).await;
    let id = w.triaged(&[(ItemKind::LibrarySeries, s2)]).await;
    let out = w.m.uphold_notice(&who, id, Some("valid")).await.unwrap();
    assert_eq!(out.review_case_id, None);
    assert_eq!(
        w.m.get_notice(&who, id).await.unwrap().strikes[0].strikes,
        1
    );
    // A third one inside the window reaches the threshold of two.
    let (s3, _) = w.series(w.uploader).await;
    let id = w.triaged(&[(ItemKind::LibrarySeries, s3)]).await;
    let out = w.m.uphold_notice(&who, id, Some("valid")).await.unwrap();
    assert!(out.review_case_id.is_some());
}

#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn the_register_exports_to_csv_and_the_read_is_logged() {
    let w = world().await;
    let c = w.caller();
    let who = Who::Staff(&c);
    let (series, _) = w.series(w.uploader).await;
    let id = w.triaged(&[(ItemKind::LibrarySeries, series)]).await;
    w.m.uphold_notice(&who, id, Some("valid")).await.unwrap();
    let rows = w.m.export_register(&who).await.unwrap();
    assert_eq!(rows.len(), 1);
    let csv = register_csv(&rows);
    let mut lines = csv.lines();
    assert!(lines.next().unwrap().starts_with("reference,kind,state,"));
    let row = lines.next().unwrap();
    assert!(row.contains(&format!("TK-{id}")));
    assert!(row.contains("Rights Holder Ltd"));
    assert!(row.contains("removed"));
    assert_eq!(log_count(w.pool(), "moderation.takedown.export").await, 1);
    // Without the bit: refused.
    let other = staff(w.staff_id, P::MANAGE_CASES);
    assert!(w.m.export_register(&Who::Staff(&other)).await.is_err());
}

#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn unknown_content_is_not_found() {
    let w = world().await;
    let c = w.caller();
    let who = Who::Staff(&c);
    let d =
        w.m.create_notice(&who, &w.notice(TakedownKind::IllegalContent))
            .await
            .unwrap();
    for kind in [
        ItemKind::Message,
        ItemKind::Attachment,
        ItemKind::LibrarySeries,
        ItemKind::LibraryChapter,
        ItemKind::CinemaTitle,
        ItemKind::CinemaEpisode,
    ] {
        assert!(matches!(
            w.m.add_item(&who, d.summary.id, &NewItem { kind, id: uid() })
                .await,
            Err(ModError::NotFound(_))
        ));
    }
    // Upholding a notice with no items is refused (and a notice still in "received" cannot be upheld).
    assert!(matches!(
        w.m.uphold_notice(&who, d.summary.id, Some("x")).await,
        Err(ModError::Conflict(_))
    ));
}

#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn contested_strikes_show_and_a_restore_that_drops_the_count_closes_the_review() {
    let w = world_with(TakedownPolicy {
        strike_review_threshold: 2,
        strike_window_months: 12,
        operator_country: None,
        operator_contact: None,
    })
    .await;
    let c = w.caller();
    let who = Who::Staff(&c);
    let (s1, _) = w.series(w.uploader).await;
    let first = w.triaged(&[(ItemKind::LibrarySeries, s1)]).await;
    w.m.uphold_notice(&who, first, Some("valid")).await.unwrap();
    let (s2, _) = w.series(w.uploader).await;
    let second = w.triaged(&[(ItemKind::LibrarySeries, s2)]).await;
    // The first notice is counter-noticed before the second is upheld: the review lists it as contested.
    w.m.record_counter_notice(&who, first, "mine")
        .await
        .unwrap();
    let out =
        w.m.uphold_notice(&who, second, Some("valid"))
            .await
            .unwrap();
    let review = out.review_case_id.expect("review at the threshold");
    let d = w.m.get_notice(&who, second).await.unwrap();
    assert_eq!(d.strikes[0].strikes, 2);
    assert_eq!(d.strikes[0].contested, 1);
    assert_eq!(d.strikes[0].counted.len(), 2);
    assert!(
        d.strikes[0]
            .counted
            .iter()
            .any(|n| n.contested && n.id == first)
    );
    let note: String =
        sqlx::query_scalar("SELECT body FROM moderation_case_notes WHERE case_id = $1")
            .bind(review)
            .fetch_one(w.pool())
            .await
            .unwrap();
    assert!(note.contains(&format!("TK-{first}")) && note.contains("counter-notice pending"));
    assert!(note.contains(&format!("TK-{second}")));
    let ev: serde_json::Value = sqlx::query_scalar(
        "SELECT detail FROM moderation_case_events WHERE case_id = $1 AND kind = 'strike_review_opened'",
    )
    .bind(review)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(ev["counted"].as_array().unwrap().len(), 2);
    // The counter-noticed notice is restored: the count falls to 1 and the open review closes as no_action.
    w.m.restore_notice(&who, first, Some("counter-notice accepted"))
        .await
        .unwrap();
    let (state, outcome): (String, String) =
        sqlx::query_as("SELECT state, outcome FROM moderation_cases WHERE id = $1")
            .bind(review)
            .fetch_one(w.pool())
            .await
            .unwrap();
    assert_eq!(state, "closed");
    assert!(outcome.starts_with("no_action"));
    assert_eq!(
        count(w.pool(), "SELECT count(*) FROM moderation_case_events WHERE case_id = $1 AND kind = 'state_changed'", review).await,
        1
    );
}

#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn the_case_list_names_the_notice_and_filters_by_takedown_source() {
    use sr_moderation::model::CaseFilter;
    let w = world().await;
    let c = w.caller();
    let who = Who::Staff(&c);
    let d =
        w.m.create_notice(&who, &w.notice(TakedownKind::IllegalContent))
            .await
            .unwrap();
    // An unrelated case opened by staff, which has no notice.
    let cs = staff(w.staff_id, P::MANAGE_CASES | P::MANAGE_TAKEDOWNS);
    let cases = Who::Staff(&cs);
    w.m.open_case(
        &cases,
        &sr_moderation::model::NewCase {
            category: sr_moderation::model::ReportCategory::Spam,
            target_kind: sr_moderation::model::TargetKind::Other,
            target_id: None,
            subject_account_id: None,
            summary: "other".into(),
        },
    )
    .await
    .unwrap();
    let all =
        w.m.list_cases(
            &cases,
            &CaseFilter {
                limit: 50,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(all.items.len(), 2);
    let only =
        w.m.list_cases(
            &cases,
            &CaseFilter {
                limit: 50,
                takedown_only: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(only.items.len(), 1);
    let t = only.items[0].takedown.as_ref().expect("badge data");
    assert_eq!(
        (t.notice_id, t.kind),
        (d.summary.id, TakedownKind::IllegalContent)
    );
    assert!(all.items.iter().any(|i| i.takedown.is_none()));
}

#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn uphold_blocks_transcode_and_original_objects_and_restore_lifts_them() {
    let w = world().await;
    let (title, asset) = w.film(w.uploader).await;
    sqlx::query(
        "INSERT INTO video_assets (id, filename, original_key, original_ext, original_size) \
         VALUES ($1, 'film.mkv', $2, 'mkv', 10)",
    )
    .bind(asset)
    .bind(format!("originals/{asset}/source.mkv"))
    .execute(w.pool())
    .await
    .unwrap();
    let jobs = [uid(), uid()];
    for j in jobs {
        sqlx::query(
            "INSERT INTO transcode_jobs (id, asset_id, kind, spec, source_key, source_size, source_ext) \
             VALUES ($1, $2, 'remux', '{}'::jsonb, 'k', 1, 'mkv')",
        )
        .bind(j)
        .bind(asset)
        .execute(w.pool())
        .await
        .unwrap();
    }
    let id = w.triaged(&[(ItemKind::CinemaTitle, title)]).await;
    let c = w.caller();
    let who = Who::Staff(&c);
    w.m.uphold_notice(&who, id, Some("valid")).await.unwrap();
    let list = MediaBlocklist::new(w.pool().clone());
    for j in jobs {
        assert!(
            list.is_blocked_key(&format!("transcode/{j}/1/remux/video.mp4"))
                .await
        );
        assert!(
            list.is_blocked_key(&format!("transcode/{j}/2/v/seg.m4s"))
                .await,
            "every attempt"
        );
    }
    assert!(
        list.is_blocked_key(&format!("originals/{asset}/source.mkv"))
            .await
    );
    assert!(list.is_blocked_asset(asset).await);
    w.m.restore_notice(&who, id, Some("mistake")).await.unwrap();
    assert_eq!(
        count(
            w.pool(),
            "SELECT count(*) FROM blocked_objects WHERE notice_id = $1",
            id
        )
        .await,
        0
    );
    let fresh = MediaBlocklist::new(w.pool().clone());
    assert!(
        !fresh
            .is_blocked_key(&format!("transcode/{}/1/remux/video.mp4", jobs[0]))
            .await
    );
}

#[derive(Default)]
struct Removed(Mutex<Vec<(Option<i64>, i64, i64)>>);

#[async_trait]
impl sr_moderation::RemovalSink for Removed {
    async fn message_removed(&self, guild: Option<i64>, channel: i64, message_id: i64) {
        self.0.lock().unwrap().push((guild, channel, message_id));
    }
}

#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn uphold_announces_each_removed_message_once_and_restore_sends_nothing() {
    let mut w = world().await;
    let sink = Arc::new(Removed::default());
    w.m = w.m.clone().with_removals(sink.clone());
    let m1 = w.message(10, w.uploader).await;
    let m2 = w.message(11, w.uploader).await;
    // Already hidden by its author: nothing to announce.
    let gone = w.message(12, w.uploader).await;
    exec(
        w.pool(),
        "UPDATE messages SET visibility = 'user_deleted', deleted_at = now() WHERE id = $1",
        &[gone],
    )
    .await;
    let (series, _) = w.series(w.uploader).await;
    let id = w
        .triaged(&[
            (ItemKind::Message, m1),
            (ItemKind::Message, m2),
            (ItemKind::Message, gone),
            (ItemKind::LibrarySeries, series),
        ])
        .await;
    let c = w.caller();
    let who = Who::Staff(&c);
    w.m.uphold_notice(&who, id, Some("valid")).await.unwrap();
    let mut got = sink.0.lock().unwrap().clone();
    got.sort_by_key(|g| g.2);
    assert_eq!(
        got.len(),
        2,
        "one event per message removed from view: {got:?}"
    );
    assert_eq!(got.iter().map(|g| g.2).collect::<Vec<_>>(), vec![m1, m2]);
    assert!(got.iter().all(|g| g.1 == w.channel));
    w.m.restore_notice(&who, id, Some("mistake")).await.unwrap();
    assert_eq!(sink.0.lock().unwrap().len(), 2, "a restore sends no event");
}

#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn counter_notices_per_staff_member_per_day_are_limited_by_a_setting() {
    // RDY-SEC-006: `abuse.counter_notices_per_day`.
    let w = world().await;
    let c = w.caller();
    let who = Who::Staff(&c);
    let mut ids = Vec::new();
    for _ in 0..3 {
        let (series, _) = w.series(w.uploader).await;
        let id = w.triaged(&[(ItemKind::LibrarySeries, series)]).await;
        w.m.uphold_notice(&who, id, Some("valid")).await.unwrap();
        ids.push(id);
    }
    sqlx::query(
        "INSERT INTO platform_settings (key, value) VALUES ('abuse.counter_notices_per_day', '2'::jsonb) \
         ON CONFLICT (key) DO UPDATE SET value = '2'::jsonb",
    )
    .execute(w.pool())
    .await
    .unwrap();
    for id in &ids[..2] {
        w.m.record_counter_notice(&who, *id, "mine").await.unwrap();
    }
    assert!(matches!(
        w.m.record_counter_notice(&who, ids[2], "mine").await,
        Err(ModError::RateLimited)
    ));
    // Nothing was recorded by the refused call, and raising the setting lets it through.
    assert_eq!(
        w.m.get_notice(&who, ids[2]).await.unwrap().summary.state,
        NoticeState::Removed
    );
    sqlx::query("UPDATE platform_settings SET value = '3'::jsonb WHERE key = 'abuse.counter_notices_per_day'")
        .execute(w.pool())
        .await
        .unwrap();
    w.m.record_counter_notice(&who, ids[2], "mine")
        .await
        .unwrap();
}
