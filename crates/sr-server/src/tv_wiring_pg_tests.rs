//! Live TV wired to the real catalog and tables (run with `SR_TEST_DATABASE_URL`, `--ignored`,
//! `--test-threads=1`): the catalog adapter over the Postgres catalog repository, and a channel scheduled
//! from it through `PgTvRepo`.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use sqlx::PgPool;
use sr_cinema::model::*;
use sr_cinema::{Actor, CinemaService, TitleIndex};
use sr_core::{ManualClock, PlatformPermissions as P, SystemClock};
use sr_db::accounts::{self, NewAccount};
use sr_db::cinema_repo::PgCinemaRepo;
use sr_db::testing::TestDb;
use sr_storage::cdn::CdnSigner;
use sr_tv::model::{ChannelInput, MediaKind, PoolFilter, Restricted};
use sr_tv::pg::PgTvRepo;
use sr_tv::{TvCatalog, TvService};

use crate::tv_catalog::RepoTvCatalog;

fn uid() -> i64 {
    static N: AtomicI64 = AtomicI64::new(0);
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + N.fetch_add(1, Ordering::Relaxed) % 1000
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

fn input(
    kind: TitleKind,
    title: &str,
    restricted: bool,
    tags: &[&str],
    runtime: i32,
) -> TitleInput {
    TitleInput {
        kind,
        title: title.into(),
        original_title: None,
        alt_titles: vec![],
        year: Some(1998),
        runtime_min: Some(runtime),
        content_rating: None,
        synopsis: "A story.".into(),
        tagline: None,
        genres: vec!["Action".into()],
        tags: tags.iter().map(|t| (*t).to_owned()).collect(),
        languages: vec![],
        subtitle_languages: vec![],
        cast: vec![],
        directors: vec![],
        writers: vec![],
        studios: vec![],
        crew: vec![],
        age_restricted: restricted,
        poster_key: None,
        backdrop_key: None,
        logo_key: None,
        external_ids: BTreeMap::new(),
    }
}

struct World {
    db: TestDb,
    catalog: CinemaService,
    repo: Arc<PgCinemaRepo>,
    publisher: Actor,
    asset: AtomicI64,
}

impl World {
    async fn new() -> Self {
        let db = TestDb::new().await.unwrap();
        let repo = Arc::new(PgCinemaRepo::new(db.pool().clone()));
        let catalog = CinemaService::new(
            repo.clone(),
            CdnSigner::new(vec![7u8; 32]),
            Arc::new(SystemClock),
            Arc::new(uid),
        )
        .with_index(Arc::new(TitleIndex::in_ram().unwrap()));
        let publisher = Actor {
            user_id: account(db.pool()).await,
            permissions: P::PUBLISH_MEDIA | P::DELETE_CONTENT,
        };
        Self {
            db,
            catalog,
            repo,
            publisher,
            asset: AtomicI64::new(7000),
        }
    }

    fn asset(&self) -> i64 {
        self.asset.fetch_add(1, Ordering::Relaxed)
    }

    async fn film(
        &self,
        kind: TitleKind,
        title: &str,
        restricted: bool,
        tags: &[&str],
        mins: i32,
    ) -> Title {
        let t = self
            .catalog
            .create_title(self.publisher, input(kind, title, restricted, tags, mins))
            .await
            .unwrap();
        self.catalog
            .link_title_asset(self.publisher, t.id, Some(self.asset()))
            .await
            .unwrap();
        self.catalog
            .set_title_published(self.publisher, t.id, true)
            .await
            .unwrap()
    }

    async fn series(&self, title: &str, n: i32) -> (Title, Vec<Episode>) {
        let t = self
            .catalog
            .create_title(
                self.publisher,
                input(TitleKind::Series, title, false, &[], 24),
            )
            .await
            .unwrap();
        let s = self
            .catalog
            .create_season(
                self.publisher,
                t.id,
                SeasonInput {
                    number: 1,
                    name: None,
                    synopsis: None,
                    poster_key: None,
                },
            )
            .await
            .unwrap();
        let mut eps = Vec::new();
        for number in 1..=n {
            let e = self
                .catalog
                .create_episode(
                    self.publisher,
                    s.id,
                    EpisodeInput {
                        number,
                        absolute_number: None,
                        title: format!("Episode {number}"),
                        synopsis: String::new(),
                        air_date: None,
                        runtime_min: Some(24),
                        still_key: None,
                    },
                )
                .await
                .unwrap();
            self.catalog
                .link_episode_asset(self.publisher, e.id, Some(self.asset()))
                .await
                .unwrap();
            eps.push(
                self.catalog
                    .set_episode_published(self.publisher, e.id, true)
                    .await
                    .unwrap(),
            );
        }
        let t = self
            .catalog
            .set_title_published(self.publisher, t.id, true)
            .await
            .unwrap();
        (t, eps)
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_catalog_adapter_offers_playable_films_and_episodes() {
    let w = World::new().await;
    let film = w.film(TitleKind::Movie, "Quiet Film", false, &[], 95).await;
    let hot = w.film(TitleKind::Movie, "Hot Film", true, &[], 110).await;
    let (series, eps) = w.series("Long Series", 3).await;
    let bump = w
        .film(TitleKind::Movie, "Bump", false, &["filler"], 2)
        .await;
    // A draft film and a film without a runtime are never offered.
    w.catalog
        .create_title(
            w.publisher,
            input(TitleKind::Movie, "Draft", false, &[], 90),
        )
        .await
        .unwrap();
    let cat = RepoTvCatalog::new(w.repo.clone());

    let all = cat.pool(&PoolFilter::default(), false, 100).await.unwrap();
    let names: Vec<_> = all
        .iter()
        .map(|i| (i.title.as_str(), i.episode_number))
        .collect();
    assert_eq!(
        names,
        [
            ("Bump", None),
            ("Long Series", Some(1)),
            ("Long Series", Some(2)),
            ("Long Series", Some(3)),
            ("Quiet Film", None)
        ],
        "restricted and draft titles are left out of a plain pool"
    );
    assert_eq!(all[1].episode_id, Some(eps[0].id));
    assert_eq!(all[1].media_kind, MediaKind::Series);
    assert_eq!(all[1].duration_ms, 24 * 60_000);
    assert_eq!(all[4].duration_ms, 95 * 60_000);
    assert!(all[4].genres.contains(&"action".to_owned()));

    // A restricted channel includes them; "only" selects them; title ids keep their order.
    let on_restricted = cat.pool(&PoolFilter::default(), true, 100).await.unwrap();
    assert!(
        on_restricted
            .iter()
            .any(|i| i.title_id == hot.id && i.age_restricted)
    );
    let only = PoolFilter {
        restricted: Some(Restricted::Only),
        ..PoolFilter::default()
    };
    let hot_only = cat.pool(&only, false, 100).await.unwrap();
    assert_eq!(hot_only.len(), 1);
    assert_eq!(hot_only[0].title_id, hot.id);
    let ordered = PoolFilter {
        title_ids: vec![film.id, series.id],
        ..PoolFilter::default()
    };
    let got = cat.pool(&ordered, false, 100).await.unwrap();
    assert_eq!(got[0].title_id, film.id);
    assert_eq!(got.len(), 4);
    let series_only = PoolFilter {
        kinds: vec![MediaKind::Series],
        ..PoolFilter::default()
    };
    assert_eq!(cat.pool(&series_only, false, 100).await.unwrap().len(), 3);
    assert_eq!(
        cat.pool(&PoolFilter::default(), false, 2)
            .await
            .unwrap()
            .len(),
        2
    );

    // Single items and bumpers.
    assert_eq!(
        cat.item(film.id, None).await.unwrap().unwrap().asset_id,
        film.asset_id.unwrap()
    );
    assert!(cat.item(series.id, None).await.unwrap().is_none());
    assert!(
        cat.item(series.id, Some(eps[1].id))
            .await
            .unwrap()
            .unwrap()
            .episode_number
            == Some(2)
    );
    assert!(cat.item(film.id, Some(eps[0].id)).await.unwrap().is_none());
    let bumpers = cat.bumpers(10).await.unwrap();
    assert_eq!(bumpers.len(), 1);
    assert_eq!(bumpers[0].title_id, bump.id);
    w.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_channel_is_scheduled_from_the_real_catalog_and_tables() {
    let w = World::new().await;
    let (series, _) = w.series("Cowboy Bebop", 12).await;
    let clock = Arc::new(ManualClock::new(1_748_894_400_000)); // 2025-06-02 20:00 UTC
    let next = Arc::new(AtomicI64::new(1));
    let tv = TvService::new(
        Arc::new(PgTvRepo::new(w.db.pool().clone())),
        Arc::new(RepoTvCatalog::new(w.repo.clone())),
        clock.clone(),
        Arc::new(move || next.fetch_add(1, Ordering::SeqCst) + 100_000),
    );
    let staff = sr_tv::Actor {
        user_id: w.publisher.user_id,
        permissions: P::MANAGE_TV,
    };
    let body: ChannelInput = serde_json::from_value(serde_json::json!({
        "number": 102, "name": "Anime", "kind": "rule_based", "timezone": "UTC", "filler_policy": "slate",
        "rules": {"grid_minutes": 30, "dayparts": [
            {"name": "All day", "start": "00:00", "end": "24:00", "mode": "sequential",
             "pool": {"title_ids": [series.id.to_string()]}}]}
    }))
    .unwrap();
    let id = tv.create_channel(staff, body).await.unwrap().id;
    let now = tv.now(id).await.unwrap();
    let cur = now.current.unwrap();
    assert_eq!(cur.title, "Cowboy Bebop");
    assert_eq!(cur.episode_number, Some(1));
    assert_eq!(cur.end_ms - cur.start_ms, 24 * 60_000);
    // 24 minute episodes on a half-hour grid: the rest of each slot is a slate.
    let g = tv
        .guide(None, Some(1_748_894_400_000 + 60 * 60_000), &[])
        .await
        .unwrap();
    let kinds: Vec<_> = g.channels[0]
        .blocks
        .iter()
        .map(|b| b.kind.as_str())
        .collect();
    assert_eq!(kinds, ["program", "slate", "program", "slate"]);
    w.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn block_length_comes_from_the_asset_when_it_is_shorter_than_the_runtime() {
    let w = World::new().await;
    // The catalog says 95 minutes, the probed asset is 80 minutes and 30 seconds.
    let film = w.film(TitleKind::Movie, "Short Cut", false, &[], 95).await;
    let other = w.film(TitleKind::Movie, "Unprobed", false, &[], 60).await;
    sqlx::query(
        "INSERT INTO video_assets (id, filename, original_key, original_ext, original_size, duration_ms) \
         VALUES ($1, 'a.mkv', 'originals/x/source.mkv', 'mkv', 10, 4830000)",
    )
    .bind(film.asset_id.unwrap())
    .execute(w.db.pool())
    .await
    .unwrap();
    let cat = RepoTvCatalog::new(w.repo.clone()).with_asset_durations(Arc::new(
        crate::tv_catalog::PgAssetDurations::new(w.db.pool().clone()),
    ));
    let pool = cat.pool(&PoolFilter::default(), false, 10).await.unwrap();
    let short = pool.iter().find(|i| i.title_id == film.id).unwrap();
    assert_eq!(short.duration_ms, 4_830_000, "the asset wins over runtime_min");
    let unprobed = pool.iter().find(|i| i.title_id == other.id).unwrap();
    assert_eq!(unprobed.duration_ms, 60 * 60_000, "no asset row: runtime_min");
    assert_eq!(
        cat.item(film.id, None).await.unwrap().unwrap().duration_ms,
        4_830_000
    );
    w.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn reminders_are_stored_replaced_taken_once_and_follow_the_account() {
    use sr_tv::model::ReminderRecord;
    use sr_tv::repo::TvRepository;
    let db = TestDb::new().await.unwrap();
    let who = account(db.pool()).await;
    let repo = PgTvRepo::new(db.pool().clone());
    let r = |block, at| ReminderRecord {
        account_id: who,
        block_id: block,
        channel_id: 5,
        title: "Cowboy Bebop".into(),
        start_ms: at + 300_000,
        remind_at_ms: at,
    };
    repo.put_reminder(&r(1, 1_000)).await.unwrap();
    repo.put_reminder(&r(2, 9_000)).await.unwrap();
    // Setting it again replaces the row.
    repo.put_reminder(&r(1, 2_000)).await.unwrap();
    assert!(repo.take_due_reminders(500, 10).await.unwrap().is_empty());
    let due = repo.take_due_reminders(5_000, 10).await.unwrap();
    assert_eq!(due, vec![r(1, 2_000)]);
    assert!(repo.take_due_reminders(5_000, 10).await.unwrap().is_empty());
    assert!(repo.delete_reminder(who, 2).await.unwrap());
    assert!(!repo.delete_reminder(who, 2).await.unwrap());
    // Deleting the account removes what is left.
    repo.put_reminder(&r(3, 1)).await.unwrap();
    sqlx::query("DELETE FROM accounts WHERE id = $1")
        .bind(who)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(repo.take_due_reminders(i64::MAX, 10).await.unwrap().is_empty());
    db.cleanup().await.unwrap();
}
