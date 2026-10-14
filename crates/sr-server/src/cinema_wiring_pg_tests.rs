//! The Cinema backends wired together against real Postgres (run with `SR_TEST_DATABASE_URL`,
//! `--ignored`, `--test-threads=1`): the catalog adapter behind the social service, the merge/delete cascade into
//! the social tables, the provider rows on the catalog service, and the presence sweep job registration.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use sqlx::PgPool;
use sqlx::Row;
use sr_cinema::model::*;
use sr_cinema::{Actor, CinemaService, TitleIndex, Viewer};
use sr_cinema_social::model::{CommentInput, ProgressInput, ReviewInput};
use sr_cinema_social::{CinemaService as Social, SocialError, SocialObserver, Viewer as SV};
use sr_core::{PlatformPermissions as P, SystemClock};
use sr_db::accounts::{self, NewAccount};
use sr_db::cinema_repo::PgCinemaRepo;
use sr_db::testing::TestDb;
use sr_storage::cdn::CdnSigner;

use crate::cinema_catalog::RepoCatalog;

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

struct World {
    _db: TestDb,
    pool: PgPool,
    catalog: CinemaService,
    social: Social,
    publisher: Actor,
    asset: AtomicI64,
}

async fn world() -> World {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let repo = Arc::new(PgCinemaRepo::new(pool.clone()));
    let social = Social::new(
        pool.clone(),
        Arc::new(RepoCatalog::new(repo.clone())),
        Arc::new(uid),
    );
    let mut catalog = CinemaService::new(
        repo,
        CdnSigner::new(vec![7u8; 32]),
        Arc::new(SystemClock),
        Arc::new(uid),
    )
    .with_index(Arc::new(TitleIndex::in_ram().unwrap()))
    .with_observer(Arc::new(SocialObserver::new(pool.clone())));
    for p in sr_cinema_social::row_providers(&social) {
        catalog = catalog.with_provider(p);
    }
    let publisher = Actor {
        user_id: account(&pool).await,
        permissions: P::PUBLISH_MEDIA | P::DELETE_CONTENT,
    };
    World {
        _db: db,
        pool,
        catalog,
        social,
        publisher,
        asset: AtomicI64::new(5000),
    }
}

fn input(kind: TitleKind, title: &str, restricted: bool) -> TitleInput {
    TitleInput {
        kind,
        title: title.into(),
        original_title: None,
        alt_titles: vec![],
        year: Some(2001),
        runtime_min: Some(100),
        content_rating: None,
        synopsis: "A story.".into(),
        tagline: None,
        genres: vec!["Drama".into()],
        tags: vec!["slow-burn".into()],
        languages: vec![],
        subtitle_languages: vec![],
        cast: vec![],
        directors: vec![],
        writers: vec![],
        studios: vec![],
        crew: vec![],
        age_restricted: restricted,
        poster_key: Some("cinema/1/poster.webp".into()),
        backdrop_key: None,
        logo_key: None,
        external_ids: BTreeMap::new(),
    }
}

impl World {
    fn next_asset(&self) -> i64 {
        self.asset.fetch_add(1, Ordering::Relaxed)
    }

    async fn film(&self, title: &str, restricted: bool) -> Title {
        let t = self
            .catalog
            .create_title(self.publisher, input(TitleKind::Movie, title, restricted))
            .await
            .unwrap();
        self.catalog
            .link_title_asset(self.publisher, t.id, Some(self.next_asset()))
            .await
            .unwrap();
        self.catalog
            .set_title_published(self.publisher, t.id, true)
            .await
            .unwrap()
    }

    /// A published series with one season and `n` published episodes with assets.
    async fn series(&self, title: &str, n: i32) -> (Title, Vec<Episode>) {
        let t = self
            .catalog
            .create_title(self.publisher, input(TitleKind::Series, title, false))
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
                .link_episode_asset(self.publisher, e.id, Some(self.next_asset()))
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

    async fn count(&self, table: &str, title: i64) -> i64 {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) AS n FROM {table} WHERE title_id = $1"
        )))
        .bind(title)
        .fetch_one(&self.pool)
        .await
        .unwrap()
        .get("n")
    }
}

fn pin(position: i32) -> ProgressInput {
    ProgressInput {
        position_secs: position,
        duration_secs: 1000,
        completed: None,
        playing: Some(false),
        base_updated_at_ms: None,
        force: None,
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_catalog_adapter_resolves_titles_and_episodes() {
    let w = world().await;
    let user = SV {
        user_id: account(&w.pool).await,
        adult: false,
    };
    let adult = SV {
        user_id: account(&w.pool).await,
        adult: true,
    };
    let film = w.film("Quiet Film", false).await;
    let hot = w.film("Restricted Film", true).await;
    let (series, eps) = w.series("Long Series", 3).await;

    // A film: progress, rating and comments resolve it.
    let saved = w
        .social
        .save_progress(user, film.id, 0, pin(100))
        .await
        .unwrap();
    assert!(saved.applied && saved.progress.percent > 9.0);
    w.social.rate(user, film.id, 0, 8).await.unwrap();

    // An episode of a series; a series without an episode id is refused, an unknown episode is 404.
    w.social
        .save_progress(user, series.id, eps[0].id, pin(950))
        .await
        .unwrap();
    assert!(matches!(
        w.social.save_progress(user, series.id, 0, pin(1)).await,
        Err(SocialError::Invalid(_))
    ));
    assert!(matches!(
        w.social
            .save_progress(user, series.id, 424_242, pin(1))
            .await,
        Err(SocialError::NotFound(_))
    ));
    // Finished episode: continue watching offers the next one.
    let cont = w.social.continue_watching(user, None).await.unwrap();
    let up_next = cont
        .items
        .iter()
        .find(|i| i.title.id == series.id)
        .expect("series in continue watching");
    assert_eq!(up_next.episode.as_ref().unwrap().id, eps[1].id);
    assert_eq!(
        up_next.episode.as_ref().unwrap().runtime_secs,
        Some(24 * 60)
    );

    // D-184: the restricted title is visible, playing is gated.
    assert!(matches!(
        w.social.save_progress(user, hot.id, 0, pin(10)).await,
        Err(SocialError::AgeConfirmationRequired)
    ));
    assert!(
        w.social
            .save_progress(adult, hot.id, 0, pin(10))
            .await
            .is_ok()
    );

    // Drafts and unpublished titles are not found.
    let draft = w
        .catalog
        .create_title(w.publisher, input(TitleKind::Movie, "Draft", false))
        .await
        .unwrap();
    assert!(matches!(
        w.social.rate(user, draft.id, 0, 5).await,
        Err(SocialError::NotFound(_))
    ));

    // My list shows the poster key and drops what is gone.
    w.social.save_title(user, film.id).await.unwrap();
    let saved = w.social.saved_items(user, None, None).await.unwrap();
    assert_eq!(saved.items.len(), 1);
    assert_eq!(saved.items[0].title.title, "Quiet Film");
    assert_eq!(
        saved.items[0].title.poster_key.as_deref(),
        Some("cinema/1/poster.webp")
    );

    // Tag recommendations: someone who watched the film is offered titles sharing its genre.
    w.social
        .save_progress(user, film.id, 0, pin(990))
        .await
        .unwrap();
    let other = w.film("Another Drama", false).await;
    let rec = w.social.recommendations(user).await.unwrap();
    let ids: BTreeSet<i64> = rec
        .rows
        .iter()
        .flat_map(|r| r.items.iter().map(|i| i.title.id))
        .collect();
    assert!(ids.contains(&other.id), "{ids:?}");
    assert!(
        !ids.contains(&hot.id),
        "restricted titles are never recommended"
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_merge_rewrites_the_social_rows() {
    let w = world().await;
    let (u1, u2) = (
        SV {
            user_id: account(&w.pool).await,
            adult: true,
        },
        SV {
            user_id: account(&w.pool).await,
            adult: true,
        },
    );
    let (target, t_eps) = w.series("Target Series", 2).await;
    let (source, s_eps) = w.series("Source Series", 3).await;

    // u1 watched source ep 1 only; u2 watched both versions of ep 1 (the target one first, completed).
    w.social
        .save_progress(u1, source.id, s_eps[0].id, pin(300))
        .await
        .unwrap();
    w.social
        .save_progress(u2, target.id, t_eps[0].id, pin(980))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    w.social
        .save_progress(u2, source.id, s_eps[0].id, pin(120))
        .await
        .unwrap();
    // Episode 3 exists only in the source and keeps its id.
    w.social
        .save_progress(u2, source.id, s_eps[2].id, pin(40))
        .await
        .unwrap();
    w.social.rate(u1, source.id, 0, 9).await.ok(); // series rating is by episode: expect refusal or row
    w.social.rate(u1, source.id, s_eps[1].id, 7).await.unwrap();
    w.social.save_title(u1, source.id).await.unwrap();
    w.social.save_title(u2, source.id).await.unwrap();
    w.social.save_title(u2, target.id).await.unwrap();
    w.social
        .put_review(
            u1,
            source.id,
            ReviewInput {
                body: "Loved it".into(),
                spoiler: None,
                rating: None,
            },
        )
        .await
        .unwrap();
    w.social
        .post_comment(
            u1,
            source.id,
            s_eps[0].id,
            CommentInput {
                body: "great opener".into(),
                spoiler: None,
                parent_id: None,
            },
        )
        .await
        .unwrap();

    w.catalog
        .merge_titles(w.publisher, source.id, target.id)
        .await
        .unwrap();

    for table in [
        "cinema_progress",
        "cinema_ratings",
        "cinema_list_items",
        "cinema_reviews",
        "cinema_comments",
    ] {
        assert_eq!(
            w.count(table, source.id).await,
            0,
            "{table} still points at the merged title"
        );
    }
    // u1's progress moved onto the surviving episode 1.
    let p = w.social.progress(u1, target.id, t_eps[0].id).await.unwrap();
    assert_eq!(p.position_secs, 300);
    // u2 had both: the newer position wins, "watched" stays (sticky).
    let p = w.social.progress(u2, target.id, t_eps[0].id).await.unwrap();
    assert_eq!(p.position_secs, 120);
    assert!(p.completed);
    // The episode that exists only in the source was re-parented under the same id.
    let p = w.social.progress(u2, target.id, s_eps[2].id).await.unwrap();
    assert_eq!(p.position_secs, 40);
    // One list entry for u2, one for u1, a review and a comment on the survivor.
    assert_eq!(w.count("cinema_list_items", target.id).await, 2);
    assert_eq!(w.count("cinema_reviews", target.id).await, 1);
    assert_eq!(w.count("cinema_comments", target.id).await, 1);
    assert_eq!(w.count("cinema_ratings", target.id).await, 2);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn deleting_titles_and_episodes_removes_the_social_rows() {
    let w = world().await;
    let u = SV {
        user_id: account(&w.pool).await,
        adult: true,
    };
    let film = w.film("Doomed Film", false).await;
    let (series, eps) = w.series("Doomed Series", 3).await;
    w.social
        .save_progress(u, film.id, 0, pin(10))
        .await
        .unwrap();
    w.social.rate(u, film.id, 0, 6).await.unwrap();
    w.social.save_title(u, film.id).await.unwrap();
    w.social
        .put_review(
            u,
            film.id,
            ReviewInput {
                body: "meh".into(),
                spoiler: None,
                rating: None,
            },
        )
        .await
        .unwrap();
    w.social
        .save_progress(u, series.id, eps[0].id, pin(10))
        .await
        .unwrap();
    w.social
        .save_progress(u, series.id, eps[1].id, pin(10))
        .await
        .unwrap();

    // One episode goes: only its rows.
    w.catalog
        .delete_episode(w.publisher, eps[0].id)
        .await
        .unwrap();
    assert_eq!(w.count("cinema_progress", series.id).await, 1);

    // A whole title goes: everything.
    w.catalog.delete_title(w.publisher, film.id).await.unwrap();
    for table in [
        "cinema_progress",
        "cinema_ratings",
        "cinema_list_items",
        "cinema_reviews",
    ] {
        assert_eq!(w.count(table, film.id).await, 0, "{table}");
    }
    w.catalog
        .delete_title(w.publisher, series.id)
        .await
        .unwrap();
    assert_eq!(w.count("cinema_progress", series.id).await, 0);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn user_data_rows_come_from_the_social_providers() {
    let w = world().await;
    let uid_ = account(&w.pool).await;
    let sv = SV {
        user_id: uid_,
        adult: false,
    };
    let viewer = Viewer {
        user_id: uid_,
        adult: false,
        hide_restricted: false,
    };
    let film = w.film("Resumable Film", false).await;
    let (series, eps) = w.series("Binge Series", 2).await;

    // Nothing watched yet: the row is empty (no entries).
    let row = w
        .catalog
        .row(viewer, "continue-watching", None, BTreeMap::new())
        .await
        .unwrap();
    assert!(row.items.is_empty());

    w.social
        .save_progress(sv, film.id, 0, pin(400))
        .await
        .unwrap();
    w.social
        .save_progress(sv, series.id, eps[0].id, pin(980))
        .await
        .unwrap();
    let row = w
        .catalog
        .row(viewer, "continue-watching", None, BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(row.kind, RowKind::Provided);
    let by_title: BTreeMap<i64, &RowItem> = row.items.iter().map(|i| (i.title.id, i)).collect();
    let film_item = by_title[&film.id];
    assert_eq!(film_item.reason.as_deref(), Some("resume"));
    assert!(film_item.progress_percent.unwrap() > 39.0);
    let series_item = by_title[&series.id];
    assert_eq!(series_item.reason.as_deref(), Some("up_next"));
    assert_eq!(series_item.episode_id, Some(eps[1].id));

    // Recently explored lists each title once.
    let row = w
        .catalog
        .row(viewer, "recently-explored", None, BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(row.items.len(), 2);

    // Discover places the provider rows the viewer has content for.
    let d = w.catalog.discover(viewer).await.unwrap();
    assert!(d.rows.iter().any(|r| r.key == "continue-watching"));

    // Trending takes its scope from the request and never errors on an empty community.
    let mut params = BTreeMap::new();
    params.insert("scope".to_owned(), "friends".to_owned());
    assert!(
        w.catalog
            .row(viewer, "trending", None, params)
            .await
            .unwrap()
            .items
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_presence_sweep_is_scheduled_and_removes_lapsed_rows() {
    let w = world().await;
    crate::cinema_jobs::schedule(&w.pool).await.unwrap();
    let kind: String = sqlx::query_scalar("SELECT kind FROM jobs WHERE id = $1")
        .bind(crate::cinema_jobs::PRESENCE_SWEEP_JOB_ID)
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(kind, crate::cinema_jobs::PRESENCE_SWEEP_KIND);
    // Arming twice is idempotent.
    crate::cinema_jobs::schedule(&w.pool).await.unwrap();

    let u = account(&w.pool).await;
    sqlx::query(
        "INSERT INTO cinema_watching (user_id, title_id, episode_id, expires_at) \
         VALUES ($1, 77, 0, now() - interval '1 minute')",
    )
    .bind(u)
    .execute(&w.pool)
    .await
    .unwrap();
    assert_eq!(w.social.sweep_presence().await.unwrap(), 1);
    assert_eq!(w.social.sweep_presence().await.unwrap(), 0);
}
