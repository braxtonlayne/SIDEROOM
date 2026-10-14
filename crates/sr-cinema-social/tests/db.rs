//! Postgres-backed tests of the Cinema social backend: progress conflicts and the 90% rule, history,
//! continue watching, presence privacy, lists, ratings/reviews/comments, trending and recommendations.
//! Run with a scratch Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-cinema-social --test db -- --ignored --test-threads=1`

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicI64, Ordering},
};

use async_trait::async_trait;
use serde_json::Value;
use sr_cinema_social::model::*;
use sr_cinema_social::{
    CinemaEvents, CinemaService, ContentKind, MemoryCatalog, ModerationHook, NewReport, Screening,
    SocialError, TitleKind, Viewer,
};
use sr_db::{
    PgPool,
    accounts::{self, NewAccount},
    testing::TestDb,
};

static SEQ: AtomicI64 = AtomicI64::new(0);

fn uid() -> i64 {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + SEQ.fetch_add(1, Ordering::Relaxed) % 1000
}

async fn account(pool: &PgPool) -> i64 {
    let id = uid();
    let name = format!("t{id}");
    let ok = accounts::create_account(
        pool,
        &NewAccount {
            id,
            person_id: uid(),
            username: &name,
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

#[derive(Default)]
struct Recorder(Mutex<Vec<(Vec<i64>, String, Value)>>);

#[async_trait]
impl CinemaEvents for Recorder {
    async fn emit(&self, users: Vec<i64>, name: &'static str, payload: Value) {
        self.0
            .lock()
            .unwrap()
            .push((users, name.to_owned(), payload));
    }
}

impl Recorder {
    /// Payload types of `name` events addressed to `user`.
    fn types(&self, user: i64, name: &str) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(u, n, _)| n == name && u.contains(&user))
            .filter_map(|(_, _, p)| p["type"].as_str().map(str::to_owned))
            .collect()
    }

    fn payloads(&self, user: i64, name: &str) -> Vec<Value> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(u, n, _)| n == name && u.contains(&user))
            .map(|(_, _, p)| p.clone())
            .collect()
    }

    fn clear(&self) {
        self.0.lock().unwrap().clear();
    }
}

#[derive(Default)]
struct Hook {
    reports: Mutex<Vec<NewReport>>,
}

#[async_trait]
impl ModerationHook for Hook {
    async fn screen(&self, _a: i64, _k: ContentKind, text: &str) -> Screening {
        if text.contains("forbidden") {
            Screening::Reject("that word is not allowed".into())
        } else {
            Screening::Allow
        }
    }

    async fn on_report(&self, r: &NewReport) {
        self.reports.lock().unwrap().push(r.clone());
    }
}

struct Env {
    db: TestDb,
    svc: CinemaService,
    cat: MemoryCatalog,
    events: Arc<Recorder>,
    hook: Arc<Hook>,
}

impl Env {
    async fn new() -> Self {
        let db = TestDb::new()
            .await
            .expect("SR_TEST_DATABASE_URL and a migrated template");
        let cat = MemoryCatalog::new();
        // 1 movie, 2 series (episodes 21..23), 3 restricted movie, 4 anime (episodes 41, 42), 5 movie.
        cat.add_title(MemoryCatalog::title_with(
            1,
            TitleKind::Movie,
            "Akira",
            &["scifi", "anime"],
        ));
        cat.add_title(MemoryCatalog::title_with(
            2,
            TitleKind::Series,
            "Dark",
            &["scifi", "drama"],
        ));
        for (i, n) in [(21, 1), (22, 2), (23, 3)] {
            cat.add_episode(MemoryCatalog::episode_with(i, 2, 1, n));
        }
        cat.add_episode(MemoryCatalog::episode_with(24, 2, 2, 1));
        let mut secret = MemoryCatalog::title_with(3, TitleKind::Movie, "Secret Film", &["scifi"]);
        secret.age_restricted = true;
        cat.add_title(secret);
        cat.add_title(MemoryCatalog::title_with(
            4,
            TitleKind::Anime,
            "Cowboy",
            &["anime", "scifi"],
        ));
        cat.add_episode(MemoryCatalog::episode_with(41, 4, 1, 1));
        cat.add_episode(MemoryCatalog::episode_with(42, 4, 1, 2));
        cat.add_title(MemoryCatalog::title_with(
            5,
            TitleKind::Movie,
            "Heat",
            &["crime"],
        ));
        let events = Arc::new(Recorder::default());
        let hook = Arc::new(Hook::default());
        let svc = CinemaService::new(db.pool().clone(), Arc::new(cat.clone()), Arc::new(uid))
            .with_events(events.clone())
            .with_moderation(hook.clone());
        Self {
            db,
            svc,
            cat,
            events,
            hook,
        }
    }

    fn pool(&self) -> &PgPool {
        self.db.pool()
    }

    async fn user(&self) -> Viewer {
        Viewer {
            user_id: account(self.pool()).await,
            adult: false,
        }
    }

    async fn adult(&self) -> Viewer {
        Viewer {
            adult: true,
            ..self.user().await
        }
    }

    async fn befriend(&self, a: i64, b: i64) {
        for (x, y) in [(a, b), (b, a)] {
            sqlx::query("INSERT INTO relationships (user_id, target_id, type) VALUES ($1, $2, 1)")
                .bind(x)
                .bind(y)
                .execute(self.pool())
                .await
                .unwrap();
        }
    }

    async fn block(&self, who: i64, target: i64) {
        sqlx::query("INSERT INTO relationships (user_id, target_id, type) VALUES ($1, $2, 2)")
            .bind(who)
            .bind(target)
            .execute(self.pool())
            .await
            .unwrap();
    }

    async fn share_server(&self, users: &[i64]) {
        let gid = uid();
        sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Test guild', $2)")
            .bind(gid)
            .bind(users[0])
            .execute(self.pool())
            .await
            .unwrap();
        for u in users {
            sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
                .bind(gid)
                .bind(u)
                .execute(self.pool())
                .await
                .unwrap();
        }
    }

    async fn set_share(&self, user: i64, level: &str) {
        let settings = serde_json::json!({ "cinema": { "share_watching": level } });
        sqlx::query(
            "INSERT INTO user_settings (user_id, settings) VALUES ($1, $2) \
             ON CONFLICT (user_id) DO UPDATE SET settings = $2",
        )
        .bind(user)
        .bind(settings)
        .execute(self.pool())
        .await
        .unwrap();
    }
}

fn beat(pos: i32, dur: i32) -> ProgressInput {
    ProgressInput {
        position_secs: pos,
        duration_secs: dur,
        completed: None,
        playing: None,
        base_updated_at_ms: None,
        force: None,
    }
}

async fn sleep_ms(n: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(n)).await;
}

// ---- progress ---------------------------------------------------------------

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn progress_conflicts_and_the_ninety_percent_rule() {
    let e = Env::new().await;
    let a = e.user().await;

    assert_eq!(
        e.svc.progress(a, 1, 0).await.unwrap_err(),
        SocialError::NotFound("progress")
    );
    let first = e.svc.save_progress(a, 1, 0, beat(600, 7200)).await.unwrap();
    assert!(first.applied && first.recorded);
    assert!(!first.progress.completed);
    assert!((first.progress.percent - 8.3333).abs() < 0.01);
    assert_eq!(e.svc.progress(a, 1, 0).await.unwrap(), first.progress);

    // A second device that last saw nothing (base 0) loses against the stored row.
    sleep_ms(5).await;
    let mut other = beat(30, 7200);
    other.base_updated_at_ms = Some(0);
    let lost = e.svc.save_progress(a, 1, 0, other.clone()).await.unwrap();
    assert!(!lost.applied);
    assert_eq!(lost.progress.position_secs, 600);
    // With the stored base it wins, and force overwrites without one.
    let mut mine = beat(700, 7200);
    mine.base_updated_at_ms = Some(first.progress.updated_at_ms);
    let won = e.svc.save_progress(a, 1, 0, mine).await.unwrap();
    assert!(won.applied);
    assert_eq!(won.progress.position_secs, 700);
    other.force = Some(true);
    let forced = e.svc.save_progress(a, 1, 0, other).await.unwrap();
    assert!(forced.applied);
    assert_eq!(forced.progress.position_secs, 30);

    // 90% completes; the mark is sticky across a rewatch and cleared only on request.
    let done = e
        .svc
        .save_progress(a, 1, 0, beat(6480, 7200))
        .await
        .unwrap();
    assert!(done.progress.completed);
    let rewatch = e.svc.save_progress(a, 1, 0, beat(60, 7200)).await.unwrap();
    assert!(rewatch.progress.completed);
    let mut reset = beat(60, 7200);
    reset.completed = Some(false);
    assert!(
        !e.svc
            .save_progress(a, 1, 0, reset)
            .await
            .unwrap()
            .progress
            .completed
    );
    // 89% does not complete.
    let near = e
        .svc
        .save_progress(a, 1, 0, beat(6400, 7200))
        .await
        .unwrap();
    assert!(!near.progress.completed);

    // The account's other devices hear about each applied save and nothing else.
    let types = e.events.types(a.user_id, "SR_CINEMA_UPDATE");
    assert!(types.iter().all(|t| t == "progress"));
    assert_eq!(types.len(), 7);

    // Per-item delete.
    assert!(e.svc.clear_progress(a, 1, 0).await.unwrap());
    assert!(!e.svc.clear_progress(a, 1, 0).await.unwrap());
    assert!(
        e.events
            .types(a.user_id, "SR_CINEMA_UPDATE")
            .contains(&"progress_cleared".to_owned())
    );
    e.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn items_are_checked_against_the_catalog() {
    let e = Env::new().await;
    let a = e.user().await;
    // A series is played by episode; a movie has no episodes; unknown things are 404.
    assert!(matches!(
        e.svc.save_progress(a, 2, 0, beat(1, 100)).await,
        Err(SocialError::Invalid(_))
    ));
    assert_eq!(
        e.svc
            .save_progress(a, 1, 99, beat(1, 100))
            .await
            .unwrap_err(),
        SocialError::NotFound("episode")
    );
    assert_eq!(
        e.svc
            .save_progress(a, 2, 41, beat(1, 100))
            .await
            .unwrap_err(),
        SocialError::NotFound("episode")
    );
    assert_eq!(
        e.svc
            .save_progress(a, 99, 0, beat(1, 100))
            .await
            .unwrap_err(),
        SocialError::NotFound("title")
    );
    assert!(matches!(
        e.svc.save_progress(a, 1, 0, beat(5000, 100)).await,
        Err(SocialError::Invalid(_))
    ));
    assert!(
        e.svc
            .save_progress(a, 2, 22, beat(10, 1500))
            .await
            .unwrap()
            .applied
    );
    // Age-restricted titles need the confirmation.
    assert_eq!(
        e.svc
            .save_progress(a, 3, 0, beat(1, 100))
            .await
            .unwrap_err(),
        SocialError::AgeConfirmationRequired
    );
    let adult = e.adult().await;
    assert!(
        e.svc
            .save_progress(adult, 3, 0, beat(1, 100))
            .await
            .unwrap()
            .applied
    );
    // An unpublished title vanishes.
    e.cat.remove_title(1);
    assert_eq!(
        e.svc.progress(a, 1, 0).await.unwrap_err(),
        SocialError::NotFound("title")
    );
    e.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn history_pages_pauses_and_clears() {
    let e = Env::new().await;
    let a = e.user().await;
    e.svc.save_progress(a, 1, 0, beat(10, 7200)).await.unwrap();
    sleep_ms(5).await;
    e.svc.save_progress(a, 2, 21, beat(10, 1500)).await.unwrap();
    sleep_ms(5).await;
    e.svc.save_progress(a, 2, 22, beat(10, 1500)).await.unwrap();
    sleep_ms(5).await;
    e.svc.save_progress(a, 5, 0, beat(10, 7200)).await.unwrap();

    // Newest first, episodes separately, with a keyset cursor.
    let p1 = e.svc.history(a, None, Some(2)).await.unwrap();
    let ids: Vec<(i64, i64)> = p1
        .items
        .iter()
        .map(|i| (i.title.id, i.progress.episode_id))
        .collect();
    assert_eq!(ids, vec![(5, 0), (2, 22)]);
    assert_eq!(p1.items[1].episode.as_ref().unwrap().number, 2);
    let p2 = e
        .svc
        .history(a, p1.next_before.as_deref(), Some(2))
        .await
        .unwrap();
    let ids: Vec<(i64, i64)> = p2
        .items
        .iter()
        .map(|i| (i.title.id, i.progress.episode_id))
        .collect();
    assert_eq!(ids, vec![(2, 21), (1, 0)]);
    assert!(p2.next_before.is_none());
    assert!(matches!(
        e.svc.history(a, Some("garbage"), None).await,
        Err(SocialError::Invalid(_))
    ));

    // Pause: heartbeats are accepted but not stored, and the stored history is untouched.
    assert!(e.svc.set_history_paused(a, true).await.unwrap().paused);
    let paused = e
        .svc
        .save_progress(a, 1, 0, beat(5000, 7200))
        .await
        .unwrap();
    assert!(paused.applied && !paused.recorded);
    assert_eq!(e.svc.progress(a, 1, 0).await.unwrap().position_secs, 10);
    assert!(e.svc.history_settings(a).await.unwrap().paused);
    assert!(!e.svc.set_history_paused(a, false).await.unwrap().paused);
    assert!(
        e.svc
            .save_progress(a, 1, 0, beat(20, 7200))
            .await
            .unwrap()
            .recorded
    );

    // Clear.
    assert_eq!(e.svc.clear_history(a).await.unwrap(), 4);
    assert!(e.svc.history(a, None, None).await.unwrap().items.is_empty());
    let types = e.events.types(a.user_id, "SR_CINEMA_UPDATE");
    assert!(types.contains(&"history_cleared".to_owned()));
    assert!(types.contains(&"history_settings".to_owned()));
    // Someone else's history is not touched.
    let b = e.user().await;
    e.svc.save_progress(b, 1, 0, beat(1, 7200)).await.unwrap();
    e.svc.clear_history(a).await.unwrap();
    assert_eq!(e.svc.history(b, None, None).await.unwrap().items.len(), 1);
    e.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn continue_watching_resumes_and_suggests_the_next_episode() {
    let e = Env::new().await;
    let a = e.user().await;
    // Unfinished movie: resume. Finished movie: gone.
    e.svc.save_progress(a, 1, 0, beat(100, 7200)).await.unwrap();
    sleep_ms(5).await;
    e.svc
        .save_progress(a, 5, 0, beat(7000, 7200))
        .await
        .unwrap();
    sleep_ms(5).await;
    // Series: ep 21 finished, then ep 22 in progress -> resume 22 (latest touched).
    e.svc
        .save_progress(a, 2, 21, beat(1450, 1500))
        .await
        .unwrap();
    sleep_ms(5).await;
    e.svc
        .save_progress(a, 2, 22, beat(100, 1500))
        .await
        .unwrap();
    sleep_ms(5).await;
    // Anime: ep 41 finished -> up next 42.
    e.svc
        .save_progress(a, 4, 41, beat(1490, 1500))
        .await
        .unwrap();

    let c = e.svc.continue_watching(a, None).await.unwrap();
    let got: Vec<(i64, ContinueReason, Option<i64>)> = c
        .items
        .iter()
        .map(|i| (i.title.id, i.reason, i.episode.as_ref().map(|x| x.id)))
        .collect();
    assert_eq!(
        got,
        vec![
            (4, ContinueReason::UpNext, Some(42)),
            (2, ContinueReason::Resume, Some(22)),
            (1, ContinueReason::Resume, None),
        ]
    );
    assert!(c.items[0].progress.is_none());
    assert_eq!(c.items[1].progress.as_ref().unwrap().position_secs, 100);
    // The last episode of a series has no next one: nothing to suggest.
    e.svc
        .save_progress(a, 4, 42, beat(1490, 1500))
        .await
        .unwrap();
    let c = e.svc.continue_watching(a, None).await.unwrap();
    assert!(c.items.iter().all(|i| i.title.id != 4));
    e.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn marking_watched_covers_episodes_seasons_and_titles() {
    let e = Env::new().await;
    let a = e.user().await;
    let w = |watched, episode_id, season| WatchedInput {
        watched,
        episode_id,
        season,
    };
    assert_eq!(
        e.svc
            .mark_watched(a, 2, w(true, Some(21), None))
            .await
            .unwrap()
            .changed,
        1
    );
    // Already watched: nothing changes.
    assert_eq!(
        e.svc
            .mark_watched(a, 2, w(true, Some(21), None))
            .await
            .unwrap()
            .changed,
        0
    );
    // The rest of season 1.
    assert_eq!(
        e.svc
            .mark_watched(a, 2, w(true, None, Some(1)))
            .await
            .unwrap()
            .changed,
        2
    );
    let state = e.svc.watch_state(a, 2).await.unwrap();
    assert_eq!(state.items.len(), 3);
    assert!(
        state
            .items
            .iter()
            .all(|p| p.completed && p.percent == 100.0)
    );
    // The whole title.
    assert_eq!(
        e.svc
            .mark_watched(a, 2, w(true, None, None))
            .await
            .unwrap()
            .changed,
        1
    );
    assert_eq!(e.svc.watch_state(a, 2).await.unwrap().items.len(), 4);
    // Unwatch one season.
    assert_eq!(
        e.svc
            .mark_watched(a, 2, w(false, None, Some(2)))
            .await
            .unwrap()
            .changed,
        1
    );
    // Movies.
    assert_eq!(
        e.svc
            .mark_watched(a, 1, w(true, None, None))
            .await
            .unwrap()
            .changed,
        1
    );
    assert!(e.svc.progress(a, 1, 0).await.unwrap().completed);
    assert!(matches!(
        e.svc.mark_watched(a, 1, w(true, Some(21), None)).await,
        Err(SocialError::Invalid(_))
    ));
    e.db.cleanup().await.unwrap();
}

// ---- presence ------------------------------------------------------------------

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn watching_presence_follows_the_share_setting() {
    let e = Env::new().await;
    let a = e.adult().await;
    let friend = e.user().await;
    let mate = e.user().await;
    let stranger = e.user().await;
    e.befriend(a.user_id, friend.user_id).await;
    e.share_server(&[a.user_id, mate.user_id]).await;

    // Default (friends): only the friend hears, once for repeated heartbeats of the same item.
    e.svc.save_progress(a, 2, 22, beat(10, 1500)).await.unwrap();
    e.svc.save_progress(a, 2, 22, beat(20, 1500)).await.unwrap();
    let got = e.events.payloads(friend.user_id, "SR_ACTIVITY_UPDATE");
    assert_eq!(got.len(), 1);
    assert_eq!(got[0]["type"], "watching");
    assert_eq!(got[0]["activity"]["type"], 3);
    assert_eq!(got[0]["activity"]["details"], "Dark");
    assert_eq!(got[0]["activity"]["state"], "S1 E2 · Episode 2");
    assert_eq!(got[0]["title_id"], "2");
    assert!(
        e.events
            .payloads(mate.user_id, "SR_ACTIVITY_UPDATE")
            .is_empty()
    );
    assert!(
        e.events
            .payloads(stranger.user_id, "SR_ACTIVITY_UPDATE")
            .is_empty()
    );
    // Friends see it in the row; strangers and shared-server mates do not.
    let row = e.svc.friends_watching(friend).await.unwrap();
    assert_eq!(row.items.len(), 1);
    assert_eq!(row.items[0].user_id, a.user_id);
    assert_eq!(row.items[0].episode.as_ref().unwrap().id, 22);
    assert!(e.svc.friends_watching(mate).await.unwrap().items.is_empty());

    // Another episode is announced again; a pause ends it.
    e.svc.save_progress(a, 2, 23, beat(10, 1500)).await.unwrap();
    assert_eq!(
        e.events
            .payloads(friend.user_id, "SR_ACTIVITY_UPDATE")
            .len(),
        2
    );
    let mut pause = beat(30, 1500);
    pause.playing = Some(false);
    e.svc.save_progress(a, 2, 23, pause).await.unwrap();
    let kinds = e.events.types(friend.user_id, "SR_ACTIVITY_UPDATE");
    assert_eq!(kinds, vec!["watching", "watching", "stopped"]);
    assert!(
        e.svc
            .friends_watching(friend)
            .await
            .unwrap()
            .items
            .is_empty()
    );

    // Everyone: friends and the person sharing a server, not the stranger.
    e.set_share(a.user_id, "everyone").await;
    e.events.clear();
    e.svc.save_progress(a, 1, 0, beat(10, 7200)).await.unwrap();
    assert_eq!(
        e.events.types(friend.user_id, "SR_ACTIVITY_UPDATE"),
        vec!["watching"]
    );
    assert_eq!(
        e.events.types(mate.user_id, "SR_ACTIVITY_UPDATE"),
        vec!["watching"]
    );
    assert!(
        e.events
            .types(stranger.user_id, "SR_ACTIVITY_UPDATE")
            .is_empty()
    );
    // A mate who blocked the watcher hears nothing.
    let blocker = e.user().await;
    e.share_server(&[a.user_id, blocker.user_id]).await;
    e.block(blocker.user_id, a.user_id).await;
    e.svc.save_progress(a, 5, 0, beat(10, 7200)).await.unwrap();
    assert!(
        e.events
            .types(blocker.user_id, "SR_ACTIVITY_UPDATE")
            .is_empty()
    );
    assert_eq!(e.events.types(mate.user_id, "SR_ACTIVITY_UPDATE").len(), 2);

    // Off: a running presence is ended once, and nothing more is sent or listed.
    e.set_share(a.user_id, "off").await;
    e.events.clear();
    e.svc.save_progress(a, 1, 0, beat(20, 7200)).await.unwrap();
    e.svc.save_progress(a, 1, 0, beat(30, 7200)).await.unwrap();
    assert_eq!(
        e.events.types(friend.user_id, "SR_ACTIVITY_UPDATE"),
        vec!["stopped"]
    );
    assert!(
        e.svc
            .friends_watching(friend)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    e.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn restricted_titles_reach_friends_only_and_masked() {
    let e = Env::new().await;
    let a = e.adult().await;
    let adult_friend = e.adult().await;
    let minor_friend = e.user().await;
    let mate = e.adult().await;
    e.befriend(a.user_id, adult_friend.user_id).await;
    e.befriend(a.user_id, minor_friend.user_id).await;
    e.share_server(&[a.user_id, mate.user_id]).await;
    e.set_share(a.user_id, "everyone").await;

    e.svc.save_progress(a, 3, 0, beat(10, 7200)).await.unwrap();
    // The server-side mate (not a friend) is never told, even with "everyone".
    assert!(
        e.events
            .payloads(mate.user_id, "SR_ACTIVITY_UPDATE")
            .is_empty()
    );
    // Friends are told, but without the title or its ids.
    for f in [adult_friend, minor_friend] {
        let p = e.events.payloads(f.user_id, "SR_ACTIVITY_UPDATE");
        assert_eq!(p.len(), 1);
        let text = p[0].to_string();
        assert!(!text.contains("Secret"), "{text}");
        assert!(p[0].get("title_id").is_none());
        assert_eq!(p[0]["activity"]["details"], "An age-restricted title");
    }
    // The row shows the real title to a confirmed adult only.
    let row = e.svc.friends_watching(adult_friend).await.unwrap();
    assert_eq!(row.items.len(), 1);
    assert_eq!(row.items[0].title.title, "Secret Film");
    assert!(
        e.svc
            .friends_watching(minor_friend)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert!(e.svc.friends_watching(mate).await.unwrap().items.is_empty());
    e.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn pausing_history_and_clearing_end_the_presence() {
    let e = Env::new().await;
    let a = e.user().await;
    let f = e.user().await;
    e.befriend(a.user_id, f.user_id).await;
    e.svc.save_progress(a, 1, 0, beat(10, 7200)).await.unwrap();
    assert_eq!(e.svc.friends_watching(f).await.unwrap().items.len(), 1);
    e.svc.set_history_paused(a, true).await.unwrap();
    assert!(e.svc.friends_watching(f).await.unwrap().items.is_empty());
    // While paused, a heartbeat starts no presence.
    e.svc.save_progress(a, 1, 0, beat(20, 7200)).await.unwrap();
    assert!(e.svc.friends_watching(f).await.unwrap().items.is_empty());
    e.svc.set_history_paused(a, false).await.unwrap();
    e.svc.save_progress(a, 1, 0, beat(30, 7200)).await.unwrap();
    assert_eq!(e.svc.friends_watching(f).await.unwrap().items.len(), 1);
    e.svc.clear_history(a).await.unwrap();
    assert!(e.svc.friends_watching(f).await.unwrap().items.is_empty());
    // Finishing the title ends it too.
    e.svc.save_progress(a, 1, 0, beat(10, 7200)).await.unwrap();
    e.svc
        .save_progress(a, 1, 0, beat(7000, 7200))
        .await
        .unwrap();
    assert!(e.svc.friends_watching(f).await.unwrap().items.is_empty());
    e.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn lapsed_presence_is_ignored_and_swept() {
    let e = Env::new().await;
    let a = e.user().await;
    let f = e.user().await;
    e.befriend(a.user_id, f.user_id).await;
    e.svc.save_progress(a, 1, 0, beat(10, 7200)).await.unwrap();
    sqlx::query("UPDATE cinema_watching SET expires_at = now() - interval '1 second'")
        .execute(e.pool())
        .await
        .unwrap();
    assert!(e.svc.friends_watching(f).await.unwrap().items.is_empty());
    e.events.clear();
    assert_eq!(e.svc.sweep_presence().await.unwrap(), 1);
    assert_eq!(
        e.events.types(f.user_id, "SR_ACTIVITY_UPDATE"),
        vec!["stopped"]
    );
    assert_eq!(e.svc.sweep_presence().await.unwrap(), 0);
    // A heartbeat after the lapse announces again.
    e.svc.save_progress(a, 1, 0, beat(20, 7200)).await.unwrap();
    assert_eq!(e.events.types(f.user_id, "SR_ACTIVITY_UPDATE").len(), 2);
    e.db.cleanup().await.unwrap();
}

// ---- lists -----------------------------------------------------------------------

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn my_list_saves_and_unsaves() {
    let e = Env::new().await;
    let a = e.user().await;
    assert!(
        e.svc
            .saved_items(a, None, None)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    e.svc.save_title(a, 1).await.unwrap();
    e.svc.save_title(a, 1).await.unwrap(); // idempotent
    e.svc.save_title(a, 2).await.unwrap();
    e.svc.save_title(a, 4).await.unwrap();
    assert_eq!(
        e.svc.save_title(a, 99).await.unwrap_err(),
        SocialError::NotFound("title")
    );
    let page = e.svc.saved_items(a, None, Some(2)).await.unwrap();
    let ids: Vec<i64> = page.items.iter().map(|i| i.title.id).collect();
    assert_eq!(ids, vec![4, 2]); // newest first
    let rest = e
        .svc
        .saved_items(a, page.next_before.as_deref(), Some(2))
        .await
        .unwrap();
    assert_eq!(
        rest.items.iter().map(|i| i.title.id).collect::<Vec<_>>(),
        vec![1]
    );
    assert!(e.svc.unsave_title(a, 2).await.unwrap());
    assert!(!e.svc.unsave_title(a, 2).await.unwrap());
    let lists = e.svc.my_lists(a).await.unwrap().lists;
    assert_eq!(lists.len(), 1);
    assert_eq!(lists[0].kind, ListKind::Saved);
    assert_eq!(lists[0].item_count, 2);
    // The saved list can neither be renamed nor deleted.
    assert!(matches!(
        e.svc
            .update_list(
                a,
                lists[0].id,
                ListPatch {
                    name: Some("x".into()),
                    description: None,
                    visibility: None
                }
            )
            .await,
        Err(SocialError::Invalid(_))
    ));
    assert!(matches!(
        e.svc.delete_list(a, lists[0].id).await,
        Err(SocialError::Invalid(_))
    ));
    // An unconfirmed account does not see restricted titles in its list; a confirmed one does.
    e.svc.save_title(a, 3).await.unwrap();
    assert!(
        e.svc
            .saved_items(a, None, None)
            .await
            .unwrap()
            .items
            .iter()
            .all(|i| i.title.id != 3)
    );
    let adult = Viewer { adult: true, ..a };
    assert!(
        e.svc
            .saved_items(adult, None, None)
            .await
            .unwrap()
            .items
            .iter()
            .any(|i| i.title.id == 3)
    );
    e.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn custom_lists_have_order_and_visibility() {
    let e = Env::new().await;
    let owner = e.user().await;
    let friend = e.user().await;
    let stranger = e.user().await;
    e.befriend(owner.user_id, friend.user_id).await;

    let list = e
        .svc
        .create_list(
            owner,
            ListInput {
                name: "  Date night ".into(),
                description: None,
                visibility: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(list.name, "Date night");
    assert_eq!(list.visibility, Visibility::Private);
    assert!(matches!(
        e.svc
            .create_list(
                owner,
                ListInput {
                    name: "  ".into(),
                    description: None,
                    visibility: None
                }
            )
            .await,
        Err(SocialError::Invalid(_))
    ));
    for t in [5, 1, 4] {
        e.svc
            .add_item(owner, list.id, t, Some("good"))
            .await
            .unwrap();
    }
    e.svc.add_item(owner, list.id, 1, None).await.unwrap(); // duplicate: no-op
    let detail = e.svc.get_list(owner, list.id, None, None).await.unwrap();
    assert_eq!(detail.list.item_count, 3);
    assert_eq!(
        detail.items.iter().map(|i| i.title.id).collect::<Vec<_>>(),
        vec![5, 1, 4]
    );
    assert_eq!(detail.items[0].note, "good");

    // Private: nobody else can even tell it exists.
    for v in [friend, stranger] {
        assert_eq!(
            e.svc.get_list(v, list.id, None, None).await.unwrap_err(),
            SocialError::NotFound("list")
        );
        assert!(
            e.svc
                .lists_of(v, owner.user_id)
                .await
                .unwrap()
                .lists
                .is_empty()
        );
    }
    // Shared with friends.
    e.svc
        .update_list(
            owner,
            list.id,
            ListPatch {
                name: None,
                description: Some("for two".into()),
                visibility: Some(Visibility::Friends),
            },
        )
        .await
        .unwrap();
    assert!(e.svc.get_list(friend, list.id, None, None).await.is_ok());
    assert!(e.svc.get_list(stranger, list.id, None, None).await.is_err());
    assert_eq!(
        e.svc
            .lists_of(friend, owner.user_id)
            .await
            .unwrap()
            .lists
            .len(),
        1
    );
    // Public, but not for someone the owner blocked.
    e.svc
        .update_list(
            owner,
            list.id,
            ListPatch {
                name: None,
                description: None,
                visibility: Some(Visibility::Public),
            },
        )
        .await
        .unwrap();
    assert!(e.svc.get_list(stranger, list.id, None, None).await.is_ok());
    e.block(owner.user_id, stranger.user_id).await;
    assert!(e.svc.get_list(stranger, list.id, None, None).await.is_err());
    // Only the owner edits.
    assert_eq!(
        e.svc.add_item(friend, list.id, 2, None).await.unwrap_err(),
        SocialError::NotFound("list")
    );
    assert_eq!(
        e.svc.delete_list(friend, list.id).await.unwrap_err(),
        SocialError::NotFound("list")
    );

    // Reorder needs exactly the same set.
    assert!(matches!(
        e.svc.reorder(owner, list.id, vec![1, 4]).await,
        Err(SocialError::Invalid(_))
    ));
    assert!(matches!(
        e.svc.reorder(owner, list.id, vec![1, 4, 4]).await,
        Err(SocialError::Invalid(_))
    ));
    assert!(matches!(
        e.svc.reorder(owner, list.id, vec![1, 4, 2]).await,
        Err(SocialError::Invalid(_))
    ));
    e.svc.reorder(owner, list.id, vec![4, 5, 1]).await.unwrap();
    let detail = e.svc.get_list(owner, list.id, None, Some(2)).await.unwrap();
    assert_eq!(
        detail.items.iter().map(|i| i.title.id).collect::<Vec<_>>(),
        vec![4, 5]
    );
    let next = e
        .svc
        .get_list(owner, list.id, detail.next_before.as_deref(), Some(2))
        .await
        .unwrap();
    assert_eq!(
        next.items.iter().map(|i| i.title.id).collect::<Vec<_>>(),
        vec![1]
    );
    // New items go to the end; removal and delete.
    e.svc.add_item(owner, list.id, 2, None).await.unwrap();
    assert_eq!(
        e.svc
            .get_list(owner, list.id, None, None)
            .await
            .unwrap()
            .items
            .last()
            .unwrap()
            .title
            .id,
        2
    );
    assert!(e.svc.remove_item(owner, list.id, 5).await.unwrap());
    assert!(!e.svc.remove_item(owner, list.id, 5).await.unwrap());
    e.svc.delete_list(owner, list.id).await.unwrap();
    assert!(e.svc.get_list(owner, list.id, None, None).await.is_err());
    assert!(
        e.events
            .types(owner.user_id, "SR_CINEMA_UPDATE")
            .contains(&"list_deleted".to_owned())
    );
    e.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn lists_are_capped() {
    let e = Env::new().await;
    let a = e.user().await;
    for i in 0..100 {
        e.svc
            .create_list(
                a,
                ListInput {
                    name: format!("l{i}"),
                    description: None,
                    visibility: None,
                },
            )
            .await
            .unwrap();
    }
    assert!(matches!(
        e.svc
            .create_list(
                a,
                ListInput {
                    name: "one more".into(),
                    description: None,
                    visibility: None
                }
            )
            .await,
        Err(SocialError::Conflict(_))
    ));
    e.db.cleanup().await.unwrap();
}

// ---- ratings, reviews, comments ------------------------------------------------------

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn ratings_aggregate_per_scope() {
    let e = Env::new().await;
    let me = e.user().await;
    let friend = e.user().await;
    let mate = e.user().await;
    let stranger = e.user().await;
    e.befriend(me.user_id, friend.user_id).await;
    e.share_server(&[me.user_id, mate.user_id]).await;
    e.svc.rate(me, 1, 0, 8).await.unwrap();
    e.svc.rate(friend, 1, 0, 6).await.unwrap();
    e.svc.rate(mate, 1, 0, 10).await.unwrap();
    e.svc.rate(stranger, 1, 0, 2).await.unwrap();
    let r = e.svc.ratings(me, 1, 0).await.unwrap();
    assert_eq!(r.mine, Some(8));
    assert_eq!((r.global.count, r.global.average), (4, Some(6.5)));
    assert_eq!((r.friends.count, r.friends.average), (1, Some(6.0)));
    assert_eq!((r.known.count, r.known.average), (2, Some(9.0))); // me and the server mate
    // Re-rating replaces; removing works; out of range and wrong targets are refused.
    e.svc.rate(me, 1, 0, 9).await.unwrap();
    assert_eq!(e.svc.ratings(me, 1, 0).await.unwrap().mine, Some(9));
    assert!(e.svc.unrate(me, 1, 0).await.unwrap());
    assert!(!e.svc.unrate(me, 1, 0).await.unwrap());
    assert!(matches!(
        e.svc.rate(me, 1, 0, 11).await,
        Err(SocialError::Invalid(_))
    ));
    assert!(matches!(
        e.svc.rate(me, 1, 0, 0).await,
        Err(SocialError::Invalid(_))
    ));
    assert_eq!(
        e.svc.rate(me, 1, 21, 5).await.unwrap_err(),
        SocialError::NotFound("episode")
    );
    // Episodes and titles are rated separately.
    e.svc.rate(me, 2, 22, 7).await.unwrap();
    e.svc.rate(me, 2, 0, 9).await.unwrap();
    assert_eq!(e.svc.ratings(me, 2, 22).await.unwrap().global.count, 1);
    assert_eq!(e.svc.ratings(me, 2, 0).await.unwrap().mine, Some(9));
    assert_eq!(e.svc.ratings(me, 2, 23).await.unwrap().mine, None);
    assert_eq!(e.svc.ratings(me, 2, 23).await.unwrap().global.average, None);
    e.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn reviews_are_one_per_account_with_helpful_votes() {
    let e = Env::new().await;
    let a = e.user().await;
    let b = e.user().await;
    let c = e.user().await;
    let rv = e
        .svc
        .put_review(
            a,
            1,
            ReviewInput {
                body: " Loved it ".into(),
                spoiler: Some(true),
                rating: Some(9),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        (rv.body.as_str(), rv.spoiler, rv.rating, rv.mine),
        ("Loved it", true, Some(9), true)
    );
    // The rating link: the title rating now reads 9.
    assert_eq!(e.svc.ratings(a, 1, 0).await.unwrap().mine, Some(9));
    // Editing replaces the one review (same id).
    let edited = e
        .svc
        .put_review(
            a,
            1,
            ReviewInput {
                body: "Still good".into(),
                spoiler: None,
                rating: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(edited.id, rv.id);
    assert!(!edited.spoiler);
    let rb = e
        .svc
        .put_review(
            b,
            1,
            ReviewInput {
                body: "Fine".into(),
                spoiler: None,
                rating: Some(5),
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        e.svc.put_review(a, 1, ReviewInput { body: "this is forbidden".into(), spoiler: None, rating: None }).await,
        Err(SocialError::Invalid(m)) if m.contains("not allowed")
    ));
    assert!(matches!(
        e.svc
            .put_review(
                a,
                1,
                ReviewInput {
                    body: "  ".into(),
                    spoiler: None,
                    rating: None
                }
            )
            .await,
        Err(SocialError::Invalid(_))
    ));

    // Helpful votes: not on one's own, idempotent, counted, undone.
    assert!(matches!(
        e.svc.set_helpful(a, rv.id, true).await,
        Err(SocialError::Invalid(_))
    ));
    assert_eq!(e.svc.set_helpful(c, rv.id, true).await.unwrap(), 1);
    assert_eq!(e.svc.set_helpful(c, rv.id, true).await.unwrap(), 1);
    assert_eq!(e.svc.set_helpful(b, rv.id, true).await.unwrap(), 2);
    assert_eq!(e.svc.set_helpful(b, rv.id, false).await.unwrap(), 1);
    assert_eq!(e.svc.set_helpful(b, rv.id, false).await.unwrap(), 1);

    let helpful = e
        .svc
        .reviews(c, 1, ReviewSort::Helpful, None, None)
        .await
        .unwrap();
    assert_eq!(
        helpful.items.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec![rv.id, rb.id]
    );
    assert!(helpful.items[0].voted && !helpful.items[1].voted);
    let recent = e
        .svc
        .reviews(c, 1, ReviewSort::Recent, None, Some(1))
        .await
        .unwrap();
    assert_eq!(recent.items[0].id, rb.id);
    let more = e
        .svc
        .reviews(
            c,
            1,
            ReviewSort::Recent,
            recent.next_before.as_deref(),
            Some(1),
        )
        .await
        .unwrap();
    assert_eq!(more.items[0].id, rv.id);
    assert!(more.next_before.is_none());
    // Pagination by helpfulness too.
    let h1 = e
        .svc
        .reviews(c, 1, ReviewSort::Helpful, None, Some(1))
        .await
        .unwrap();
    let h2 = e
        .svc
        .reviews(
            c,
            1,
            ReviewSort::Helpful,
            h1.next_before.as_deref(),
            Some(1),
        )
        .await
        .unwrap();
    assert_eq!((h1.items[0].id, h2.items[0].id), (rv.id, rb.id));

    // Blocked authors are left out; moderators can hide; authors can delete.
    e.block(c.user_id, b.user_id).await;
    assert_eq!(
        e.svc
            .reviews(c, 1, ReviewSort::Recent, None, None)
            .await
            .unwrap()
            .items
            .len(),
        1
    );
    assert!(
        e.svc
            .set_hidden(ContentKind::Review, rv.id, true)
            .await
            .unwrap()
    );
    assert!(
        e.svc
            .reviews(c, 1, ReviewSort::Recent, None, None)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert!(
        e.svc
            .set_hidden(ContentKind::Review, rv.id, false)
            .await
            .unwrap()
    );
    assert!(e.svc.delete_review(a, 1).await.unwrap());
    assert!(!e.svc.delete_review(a, 1).await.unwrap());
    e.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn comments_thread_one_level_with_likes_and_reports() {
    let e = Env::new().await;
    let a = e.user().await;
    let b = e.user().await;
    let c = e.user().await;
    let say = |body: &str, parent: Option<i64>| CommentInput {
        body: body.into(),
        spoiler: None,
        parent_id: parent,
    };

    let top = e
        .svc
        .post_comment(a, 2, 22, say("first!", None))
        .await
        .unwrap();
    assert!(top.mine && top.parent_id.is_none() && top.episode_id == 22);
    let reply = e
        .svc
        .post_comment(b, 2, 22, say("welcome", Some(top.id)))
        .await
        .unwrap();
    assert_eq!(reply.parent_id, Some(top.id));
    // Replies are one level deep; the parent must be on the same episode.
    assert!(matches!(
        e.svc
            .post_comment(c, 2, 22, say("nested", Some(reply.id)))
            .await,
        Err(SocialError::Invalid(_))
    ));
    assert_eq!(
        e.svc
            .post_comment(c, 2, 23, say("wrong place", Some(top.id)))
            .await
            .unwrap_err(),
        SocialError::NotFound("comment")
    );
    assert!(matches!(
        e.svc
            .post_comment(c, 2, 22, say("a forbidden word", None))
            .await,
        Err(SocialError::Invalid(_))
    ));
    assert_eq!(
        e.svc
            .post_comment(c, 1, 21, say("x", None))
            .await
            .unwrap_err(),
        SocialError::NotFound("episode")
    );
    // A title-level comment and an episode comment are separate threads.
    e.svc
        .post_comment(c, 2, 0, say("on the show", None))
        .await
        .unwrap();
    let page = e.svc.comments(c, 2, 22, None, None).await.unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].reply_count, 1);
    assert_eq!(
        e.svc
            .comments(c, 2, 0, None, None)
            .await
            .unwrap()
            .items
            .len(),
        1
    );
    let replies = e.svc.replies(c, top.id, None, None).await.unwrap();
    assert_eq!(replies.items.len(), 1);
    assert_eq!(replies.items[0].body, "welcome");

    // Newest first with a cursor.
    sleep_ms(5).await;
    e.svc
        .post_comment(b, 2, 22, say("second", None))
        .await
        .unwrap();
    let p1 = e.svc.comments(c, 2, 22, None, Some(1)).await.unwrap();
    assert_eq!(p1.items[0].body, "second");
    let p2 = e
        .svc
        .comments(c, 2, 22, p1.next_before.as_deref(), Some(1))
        .await
        .unwrap();
    assert_eq!(p2.items[0].body, "first!");

    // Likes are idempotent per account.
    assert_eq!(e.svc.set_like(b, top.id, true).await.unwrap(), 1);
    assert_eq!(e.svc.set_like(b, top.id, true).await.unwrap(), 1);
    assert_eq!(e.svc.set_like(c, top.id, true).await.unwrap(), 2);
    assert_eq!(e.svc.set_like(c, top.id, false).await.unwrap(), 1);
    let seen = e.svc.comments(b, 2, 22, None, None).await.unwrap();
    let t = seen.items.iter().find(|x| x.id == top.id).unwrap();
    assert!(t.liked && t.like_count == 1);

    // Only the author edits and deletes; deleting a parent removes the replies.
    assert!(
        e.svc
            .edit_comment(
                b,
                top.id,
                CommentPatch {
                    body: Some("hax".into()),
                    spoiler: None
                }
            )
            .await
            .is_err()
    );
    let edited = e
        .svc
        .edit_comment(
            a,
            top.id,
            CommentPatch {
                body: Some("first, edited".into()),
                spoiler: Some(true),
            },
        )
        .await
        .unwrap();
    assert!(edited.edited_at_ms.is_some() && edited.spoiler);
    assert!(!e.svc.delete_comment(b, top.id).await.unwrap());

    // Blocked authors disappear from the viewer's listing.
    e.block(c.user_id, b.user_id).await;
    let for_c = e.svc.comments(c, 2, 22, None, None).await.unwrap();
    assert!(for_c.items.iter().all(|x| x.author_id != b.user_id));
    assert!(
        e.svc
            .replies(c, top.id, None, None)
            .await
            .unwrap()
            .items
            .is_empty()
    );

    // Reports: stored once per reporter, hook called, unknown targets 404.
    let input = || ReportInput {
        reason: "spam".into(),
    };
    assert!(
        e.svc
            .report(c, ContentKind::Comment, top.id, input())
            .await
            .unwrap()
    );
    assert!(
        !e.svc
            .report(c, ContentKind::Comment, top.id, input())
            .await
            .unwrap()
    );
    assert!(
        e.svc
            .report(b, ContentKind::Comment, top.id, input())
            .await
            .unwrap()
    );
    assert_eq!(e.hook.reports.lock().unwrap().len(), 2);
    assert_eq!(
        e.svc
            .report(c, ContentKind::Review, 12345, input())
            .await
            .unwrap_err(),
        SocialError::NotFound("review")
    );
    // Hidden by staff: gone from listings.
    assert!(
        e.svc
            .set_hidden(ContentKind::Comment, top.id, true)
            .await
            .unwrap()
    );
    assert!(
        e.svc
            .comments(b, 2, 22, None, None)
            .await
            .unwrap()
            .items
            .iter()
            .all(|x| x.id != top.id)
    );
    assert_eq!(
        e.svc.set_like(b, top.id, true).await.unwrap_err(),
        SocialError::NotFound("comment")
    );

    assert!(e.svc.delete_comment(a, top.id).await.unwrap());
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM cinema_comments WHERE parent_id = $1")
        .bind(top.id)
        .fetch_one(e.pool())
        .await
        .unwrap();
    assert_eq!(n, 0);
    e.db.cleanup().await.unwrap();
}

// ---- trending and recommendations ----------------------------------------------------

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn trending_respects_scope_and_privacy() {
    let e = Env::new().await;
    let me = e.adult().await;
    let friend = e.user().await;
    let mate = e.user().await; // shares a server, setting "everyone"
    let quiet_mate = e.user().await; // shares a server, default setting
    let hidden = e.user().await; // friend with "off"
    let stranger = e.user().await;
    e.befriend(me.user_id, friend.user_id).await;
    e.befriend(me.user_id, hidden.user_id).await;
    e.share_server(&[me.user_id, mate.user_id, quiet_mate.user_id])
        .await;
    e.set_share(mate.user_id, "everyone").await;
    e.set_share(hidden.user_id, "off").await;

    let watch = |v: Viewer, t: i64, ep: i64, pos: i32, dur: i32| {
        let svc = e.svc.clone();
        async move { svc.save_progress(v, t, ep, beat(pos, dur)).await.unwrap() }
    };
    // Title 1: friend, mate, stranger. Title 5: only the hidden friend and a stranger.
    watch(friend, 1, 0, 7000, 7200).await; // completed
    watch(mate, 1, 0, 100, 7200).await;
    watch(stranger, 1, 0, 100, 7200).await;
    watch(quiet_mate, 4, 41, 10, 1500).await;
    watch(hidden, 5, 0, 100, 7200).await;
    watch(stranger, 5, 0, 100, 7200).await;
    let adult_friend = e.adult().await;
    e.befriend(me.user_id, adult_friend.user_id).await;
    watch(adult_friend, 3, 0, 100, 7200).await; // restricted

    let ids = |t: &Trending| t.items.iter().map(|i| i.title.id).collect::<Vec<_>>();
    // Friends: friend (title 1) + adult friend's restricted film (visible to an adult viewer).
    let friends = e
        .svc
        .trending(me, TrendScope::Friends, None, None)
        .await
        .unwrap();
    assert_eq!(ids(&friends), vec![1, 3]);
    assert_eq!(friends.items[0].viewers, 1);
    assert_eq!(friends.items[0].score, 1.0 + 2.0);
    // The same scope for an unconfirmed account hides the restricted title.
    let minor = Viewer { adult: false, ..me };
    assert_eq!(
        ids(&e
            .svc
            .trending(minor, TrendScope::Friends, None, None)
            .await
            .unwrap()),
        vec![1]
    );
    // Known: friends plus "everyone" mates; the default-setting mate and the "off" friend are out.
    let known = e
        .svc
        .trending(me, TrendScope::Known, None, None)
        .await
        .unwrap();
    assert_eq!(ids(&known), vec![1]);
    assert_eq!(known.items[0].viewers, 2);
    // Global: needs two viewers, ignores "off", never shows restricted titles.
    let global = e
        .svc
        .trending(me, TrendScope::Global, None, None)
        .await
        .unwrap();
    assert_eq!(ids(&global), vec![1]);
    assert_eq!(global.items[0].viewers, 3);
    // Kind filter and limit.
    assert!(
        e.svc
            .trending(me, TrendScope::Global, Some(TitleKind::Series), None)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert_eq!(
        e.svc
            .trending(me, TrendScope::Friends, None, Some(1))
            .await
            .unwrap()
            .items
            .len(),
        1
    );
    // High ratings lift the score within scope.
    e.svc.rate(friend, 1, 0, 9).await.unwrap();
    let friends = e
        .svc
        .trending(me, TrendScope::Friends, None, None)
        .await
        .unwrap();
    assert_eq!(friends.items[0].score, 1.0 + 2.0 + 1.0);
    e.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn recommendations_combine_cowatching_and_tags() {
    let e = Env::new().await;
    let me = e.user().await;
    // I watched Akira (1; tags scifi, anime).
    e.svc
        .save_progress(me, 1, 0, beat(6500, 7200))
        .await
        .unwrap();
    // Two other viewers of Akira also watched Heat (5); one also watched Dark ep (2).
    for i in 0..2 {
        let o = e.user().await;
        e.svc
            .save_progress(o, 1, 0, beat(7000, 7200))
            .await
            .unwrap();
        e.svc
            .save_progress(o, 5, 0, beat(7000, 7200))
            .await
            .unwrap();
        if i == 0 {
            e.svc
                .save_progress(o, 2, 21, beat(1500, 1500))
                .await
                .unwrap();
        }
    }
    // A viewer who keeps their activity private does not count as a co-watcher.
    let private = e.user().await;
    e.set_share(private.user_id, "off").await;
    e.svc
        .save_progress(private, 1, 0, beat(7000, 7200))
        .await
        .unwrap();
    e.svc
        .save_progress(private, 2, 22, beat(1500, 1500))
        .await
        .unwrap();
    e.svc
        .save_progress(private, 2, 21, beat(1500, 1500))
        .await
        .unwrap();

    let recs = e.svc.recommendations(me).await.unwrap();
    assert_eq!(recs.rows.len(), 1);
    let row = &recs.rows[0];
    assert_eq!(row.because.id, 1);
    let by: std::collections::HashMap<i64, &Recommendation> =
        row.items.iter().map(|r| (r.title.id, r)).collect();
    // Heat: two co-watchers, no shared tags.
    assert_eq!(by[&5].reasons, vec!["cowatch"]);
    // Dark (2) has one co-watcher (below the floor) but shares "scifi"; Cowboy (4) shares both tags.
    assert_eq!(by[&2].reasons, vec!["tags"]);
    assert_eq!(by[&4].reasons, vec!["tags"]);
    // Never the seed, anything already started, or restricted titles.
    assert!(!by.contains_key(&1));
    assert!(!by.contains_key(&3));
    // Ranking: Heat (2) beats Cowboy (3 * 1.0 = 3)? Cowboy shares both tags: 3.0 > 2.0.
    assert_eq!(row.items[0].title.id, 4);
    // Watching a recommendation removes it from the next list.
    e.svc.save_progress(me, 5, 0, beat(10, 7200)).await.unwrap();
    let recs = e.svc.recommendations(me).await.unwrap();
    assert!(recs.rows[0].items.iter().all(|r| r.title.id != 5));
    // Nothing watched, nothing recommended.
    let fresh = e.user().await;
    assert!(e.svc.recommendations(fresh).await.unwrap().rows.is_empty());
    e.db.cleanup().await.unwrap();
}
