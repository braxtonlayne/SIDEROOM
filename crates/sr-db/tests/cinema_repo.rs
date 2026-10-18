//! DB-backed tests of the Cinema catalog: the Postgres repository driven through the real
//! service (permissions, visibility, publish rules, filters and keyset paging, age rules,
//! merge, collections, rows, search). Each test gets its own database from `TestDb`.
//! Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test cinema_repo -- --ignored --test-threads=1`

use std::collections::BTreeMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicI64, Ordering},
};

use async_trait::async_trait;
use sr_cinema::model::*;
use sr_cinema::rows::{self, ProvidedRow, RowContext, RowEntry, RowProvider};
use sr_cinema::{Actor, CatalogObserver, CinemaError, CinemaService, PlayRef, TitleIndex, Viewer};
use sr_core::{PlatformPermissions as P, SystemClock};
use sr_db::{
    PgPool,
    accounts::{self, NewAccount},
    cinema_repo::PgCinemaRepo,
    testing::TestDb,
};
use sr_storage::cdn::CdnSigner;

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

#[derive(Default)]
struct Seen {
    merged: Mutex<Vec<(i64, i64, EpisodeMap)>>,
    deleted: Mutex<Vec<(i64, Vec<i64>)>>,
    episodes_deleted: Mutex<Vec<(i64, Vec<i64>)>>,
}

#[async_trait]
impl CatalogObserver for Seen {
    async fn title_merged(&self, source: i64, target: i64, map: &EpisodeMap) {
        self.merged
            .lock()
            .unwrap()
            .push((source, target, map.clone()));
    }
    async fn title_deleted(&self, title: i64, episodes: &[i64]) {
        self.deleted
            .lock()
            .unwrap()
            .push((title, episodes.to_vec()));
    }
    async fn episodes_deleted(&self, title: i64, episodes: &[i64]) {
        self.episodes_deleted
            .lock()
            .unwrap()
            .push((title, episodes.to_vec()));
    }
}

struct Fixed {
    key: &'static str,
    entries: Vec<RowEntry>,
}

#[async_trait]
impl RowProvider for Fixed {
    fn key(&self) -> &'static str {
        self.key
    }
    async fn row(&self, ctx: &RowContext) -> sr_cinema::CinemaResult<Option<ProvidedRow>> {
        Ok(Some(ProvidedRow {
            title: ctx.params.get("title").cloned(),
            entries: self.entries.clone(),
            ..ProvidedRow::default()
        }))
    }
}

struct World {
    db: TestDb,
    svc: CinemaService,
    seen: Arc<Seen>,
    publisher: Actor,
    curator: Actor,
    member: Viewer,
    adult: Viewer,
    asset: AtomicI64,
}

async fn world_with(providers: Vec<Arc<dyn RowProvider>>) -> World {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let seen = Arc::new(Seen::default());
    let mut svc = CinemaService::new(
        Arc::new(PgCinemaRepo::new(pool.clone())),
        CdnSigner::new(vec![7u8; 32]),
        Arc::new(SystemClock),
        Arc::new(uid),
    )
    .with_index(Arc::new(TitleIndex::in_ram().unwrap()))
    .with_observer(seen.clone());
    for p in providers {
        svc = svc.with_provider(p);
    }
    let p = account(&pool).await;
    let c = account(&pool).await;
    let m = account(&pool).await;
    let a = account(&pool).await;
    World {
        db,
        svc,
        seen,
        publisher: Actor {
            user_id: p,
            permissions: P::PUBLISH_MEDIA | P::DELETE_CONTENT,
        },
        curator: Actor {
            user_id: c,
            permissions: P::MANAGE_COLLECTIONS,
        },
        member: Viewer {
            user_id: m,
            adult: false,
            hide_restricted: false,
        },
        adult: Viewer {
            user_id: a,
            adult: true,
            hide_restricted: false,
        },
        asset: AtomicI64::new(1000),
    }
}

async fn world() -> World {
    world_with(vec![]).await
}

fn input(kind: TitleKind, title: &str) -> TitleInput {
    TitleInput {
        kind,
        title: title.into(),
        original_title: None,
        alt_titles: vec![],
        year: Some(2000),
        runtime_min: Some(100),
        content_rating: Some("PG".into()),
        synopsis: "A story.".into(),
        tagline: None,
        genres: vec!["drama".into()],
        tags: vec![],
        languages: vec!["en".into()],
        subtitle_languages: vec![],
        cast: vec![],
        directors: vec![],
        writers: vec![],
        studios: vec![],
        crew: vec![],
        age_restricted: false,
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

    /// A published film (has an asset).
    async fn film(&self, title: &str, edit: impl FnOnce(&mut TitleInput)) -> Title {
        let mut i = input(TitleKind::Movie, title);
        edit(&mut i);
        let t = self.svc.create_title(self.publisher, i).await.unwrap();
        self.svc
            .link_title_asset(self.publisher, t.id, Some(self.next_asset()))
            .await
            .unwrap();
        self.svc
            .set_title_published(self.publisher, t.id, true)
            .await
            .unwrap()
    }

    /// A draft series with a season 1 and `n` episodes, each with an asset and published.
    async fn series(&self, title: &str, n: i32) -> (Title, Season, Vec<Episode>) {
        let t = self
            .svc
            .create_title(self.publisher, input(TitleKind::Series, title))
            .await
            .unwrap();
        let s = self
            .svc
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
        for k in 1..=n {
            eps.push(self.episode(s.id, k, true).await);
        }
        (t, s, eps)
    }

    async fn episode(&self, season: i64, number: i32, publish: bool) -> Episode {
        let e = self
            .svc
            .create_episode(
                self.publisher,
                season,
                EpisodeInput {
                    number,
                    absolute_number: None,
                    title: format!("Episode {number}"),
                    synopsis: String::new(),
                    air_date: Some("2024-02-29".into()),
                    runtime_min: Some(24),
                    still_key: None,
                },
            )
            .await
            .unwrap();
        if !publish {
            return e;
        }
        self.svc
            .link_episode_asset(self.publisher, e.id, Some(self.next_asset()))
            .await
            .unwrap();
        self.svc
            .set_episode_published(self.publisher, e.id, true)
            .await
            .unwrap()
    }

    async fn publish(&self, id: i64) -> Title {
        self.svc
            .set_title_published(self.publisher, id, true)
            .await
            .unwrap()
    }

    async fn browse(&self, q: BrowseQuery) -> TitlePage {
        self.svc.browse(self.member, q, None).await.unwrap()
    }
}

fn titles(p: &TitlePage) -> Vec<&str> {
    p.items.iter().map(|t| t.title.as_str()).collect()
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn permissions_and_the_publish_rule() {
    let w = world().await;
    let nobody = Actor {
        user_id: w.member.user_id,
        permissions: P::empty(),
    };
    let err = w
        .svc
        .create_title(nobody, input(TitleKind::Movie, "X"))
        .await
        .unwrap_err();
    assert_eq!(err, CinemaError::MissingPermission("PUBLISH_MEDIA"));
    assert!(
        w.svc
            .admin_list(nobody, BrowseQuery::default())
            .await
            .is_err()
    );

    // A draft without an asset cannot be published.
    let t = w
        .svc
        .create_title(w.publisher, input(TitleKind::Movie, "Draft Film"))
        .await
        .unwrap();
    assert!(!t.published && !t.playable);
    assert!(matches!(
        w.svc.set_title_published(w.publisher, t.id, true).await,
        Err(CinemaError::Conflict(_))
    ));
    // Invisible to members while a draft, visible to staff.
    assert!(matches!(
        w.svc.title_detail(w.member, t.id).await,
        Err(CinemaError::NotFound(_))
    ));
    assert!(w.svc.admin_title(w.publisher, t.id).await.is_ok());

    // Linking the asset allows publishing; the staff view carries the asset id, the member view not.
    let linked = w
        .svc
        .link_title_asset(w.publisher, t.id, Some(777))
        .await
        .unwrap();
    assert_eq!(linked.asset_id, Some(777));
    assert!(linked.playable);
    let published = w.publish(t.id).await;
    assert!(published.published && published.published_at_ms.is_some());
    let seen = w.svc.title_detail(w.member, t.id).await.unwrap();
    assert_eq!(seen.title.asset_id, None, "assets are not shown to members");
    assert!(seen.title.playable);

    // Editing metadata keeps the asset link.
    let mut edit = input(TitleKind::Movie, "Draft Film");
    edit.tagline = Some("Now with a tagline".into());
    let edited = w.svc.update_title(w.publisher, t.id, edit).await.unwrap();
    assert_eq!(edited.asset_id, Some(777));

    // Unpublishing takes PUBLISH_MEDIA or DELETE_CONTENT; deleting only DELETE_CONTENT.
    let only_delete = Actor {
        user_id: w.publisher.user_id,
        permissions: P::DELETE_CONTENT,
    };
    assert!(
        w.svc
            .set_title_published(only_delete, t.id, false)
            .await
            .is_ok()
    );
    assert!(
        w.svc
            .set_title_published(only_delete, t.id, true)
            .await
            .is_err()
    );
    let only_publish = Actor {
        user_id: w.publisher.user_id,
        permissions: P::PUBLISH_MEDIA,
    };
    assert!(w.svc.delete_title(only_publish, t.id).await.is_err());
    w.svc.delete_title(only_delete, t.id).await.unwrap();
    assert!(matches!(
        w.svc.admin_title(w.publisher, t.id).await,
        Err(CinemaError::NotFound(_))
    ));
    assert_eq!(w.seen.deleted.lock().unwrap().len(), 1);
    w.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn browse_filters_sorts_and_keyset_paging() {
    let w = world().await;
    let mut ids = BTreeMap::new();
    for (name, kind, year, runtime, rating, lang, genres) in [
        (
            "Alpha",
            TitleKind::Movie,
            1990,
            80,
            "PG",
            "en",
            vec!["drama", "crime"],
        ),
        (
            "Bravo",
            TitleKind::Movie,
            2001,
            95,
            "R",
            "ja",
            vec!["drama"],
        ),
        (
            "The Charlie",
            TitleKind::Movie,
            2005,
            120,
            "PG-13",
            "en",
            vec!["comedy"],
        ),
        (
            "Delta",
            TitleKind::Movie,
            2010,
            150,
            "R",
            "fr",
            vec!["horror", "drama"],
        ),
        (
            "Echo",
            TitleKind::Movie,
            2020,
            60,
            "PG",
            "en",
            vec!["drama"],
        ),
    ] {
        let t = w
            .film(name, |i| {
                i.kind = kind;
                i.year = Some(year);
                i.runtime_min = Some(runtime);
                i.content_rating = Some(rating.into());
                i.languages = vec![lang.into()];
                i.genres = genres.iter().map(|s| s.to_string()).collect();
            })
            .await;
        ids.insert(name, t.id);
    }
    // A draft and another kind never show up in a movie listing.
    w.svc
        .create_title(w.publisher, input(TitleKind::Movie, "Hidden draft"))
        .await
        .unwrap();
    let anime = w
        .svc
        .create_title(w.publisher, input(TitleKind::Anime, "Anime One"))
        .await
        .unwrap();
    w.svc
        .link_title_asset(w.publisher, anime.id, Some(5))
        .await
        .unwrap();
    w.publish(anime.id).await;

    let movies = |q: BrowseQuery| BrowseQuery {
        kind: Some(TitleKind::Movie),
        ..q
    };
    let all = w.browse(movies(BrowseQuery::default())).await;
    assert_eq!(all.total, 5);
    // The title sort ignores the leading article: The Charlie sorts under C.
    let by_title = w
        .browse(movies(BrowseQuery {
            sort: SortKey::Title,
            ..BrowseQuery::default()
        }))
        .await;
    assert_eq!(
        titles(&by_title),
        ["Alpha", "Bravo", "The Charlie", "Delta", "Echo"]
    );

    let drama = w
        .browse(movies(BrowseQuery {
            genres: vec!["drama".into()],
            sort: SortKey::Year,
            ..BrowseQuery::default()
        }))
        .await;
    assert_eq!(
        titles(&drama),
        ["Echo", "Delta", "Bravo", "Alpha"],
        "year descending"
    );
    let no_horror = w
        .browse(movies(BrowseQuery {
            genres: vec!["drama".into()],
            exclude_genres: vec!["horror".into()],
            sort: SortKey::Title,
            ..BrowseQuery::default()
        }))
        .await;
    assert_eq!(titles(&no_horror), ["Alpha", "Bravo", "Echo"]);
    let years = w
        .browse(movies(BrowseQuery {
            year_from: Some(2001),
            year_to: Some(2010),
            sort: SortKey::Title,
            ..BrowseQuery::default()
        }))
        .await;
    assert_eq!(titles(&years), ["Bravo", "The Charlie", "Delta"]);
    let rated = w
        .browse(movies(BrowseQuery {
            ratings: vec!["R".into(), "PG-13".into()],
            sort: SortKey::Title,
            ..BrowseQuery::default()
        }))
        .await;
    assert_eq!(titles(&rated), ["Bravo", "The Charlie", "Delta"]);
    let runtime = w
        .browse(movies(BrowseQuery {
            runtime_min: Some(90),
            runtime_max: Some(130),
            sort: SortKey::Runtime,
            ..BrowseQuery::default()
        }))
        .await;
    assert_eq!(
        titles(&runtime),
        ["Bravo", "The Charlie"],
        "runtime ascending"
    );
    let english = w
        .browse(movies(BrowseQuery {
            language: Some("en".into()),
            sort: SortKey::Title,
            ..BrowseQuery::default()
        }))
        .await;
    assert_eq!(titles(&english), ["Alpha", "The Charlie", "Echo"]);

    // Keyset paging walks every sort in both directions without repeats or gaps.
    for sort in [
        SortKey::Updated,
        SortKey::Newest,
        SortKey::Title,
        SortKey::Year,
        SortKey::Runtime,
    ] {
        for descending in [false, true] {
            let mut seen = Vec::new();
            let mut cursor: Option<String> = None;
            loop {
                let page = w
                    .svc
                    .browse(
                        w.member,
                        movies(BrowseQuery {
                            sort,
                            descending: Some(descending),
                            limit: 2,
                            ..BrowseQuery::default()
                        }),
                        cursor.as_deref(),
                    )
                    .await
                    .unwrap();
                assert_eq!(page.total, 5, "total ignores the cursor ({sort:?})");
                seen.extend(page.items.iter().map(|t| t.id));
                match page.next_cursor {
                    Some(c) => cursor = Some(c),
                    None => break,
                }
            }
            let mut sorted = seen.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(seen.len(), 5, "{sort:?} {descending}: {seen:?}");
            assert_eq!(sorted.len(), 5);
        }
    }
    // A cursor made for another sort is refused.
    let page = w
        .svc
        .browse(
            w.member,
            movies(BrowseQuery {
                sort: SortKey::Title,
                limit: 2,
                ..BrowseQuery::default()
            }),
            None,
        )
        .await
        .unwrap();
    let err = w
        .svc
        .browse(
            w.member,
            movies(BrowseQuery {
                sort: SortKey::Year,
                limit: 2,
                ..BrowseQuery::default()
            }),
            page.next_cursor.as_deref(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, CinemaError::Invalid(_)));
    // Staff list sees drafts; members do not.
    let staff = w
        .svc
        .admin_list(
            w.publisher,
            BrowseQuery {
                kind: Some(TitleKind::Movie),
                published: Some(false),
                ..BrowseQuery::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(staff.total, 1);
    assert_eq!(staff.items[0].title, "Hidden draft");
    let found = w
        .svc
        .admin_list(
            w.publisher,
            BrowseQuery {
                title_contains: Some("hidden".into()),
                ..BrowseQuery::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(found.total, 1);
    w.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn age_restricted_titles_are_visible_but_gated() {
    let w = world().await;
    let plain = w.film("Plain", |_| {}).await;
    let spicy = w
        .film("Spicy", |i| {
            i.age_restricted = true;
        })
        .await;
    // Everyone sees both by default.
    let page = w.browse(BrowseQuery::default()).await;
    assert_eq!(page.total, 2);
    assert!(w.svc.title_detail(w.member, spicy.id).await.is_ok());
    // hide / only.
    let hide = w
        .browse(BrowseQuery {
            age: AgeFilter::Hide,
            ..BrowseQuery::default()
        })
        .await;
    assert_eq!(titles(&hide), ["Plain"]);
    let only = w
        .browse(BrowseQuery {
            age: AgeFilter::Only,
            ..BrowseQuery::default()
        })
        .await;
    assert_eq!(titles(&only), ["Spicy"]);
    // Opening: the unconfirmed account gets the age error, the adult gets the target.
    let play = |v: Viewer, id: i64| {
        let svc = w.svc.clone();
        async move {
            svc.resolve_play(v, PlayRef::Title { id, variant: None })
                .await
        }
    };
    assert!(play(w.member, plain.id).await.is_ok());
    assert_eq!(
        play(w.member, spicy.id).await.unwrap_err(),
        CinemaError::AgeConfirmationRequired
    );
    let target = play(w.adult, spicy.id).await.unwrap();
    assert!(target.age_restricted);
    // Trailers of a restricted title withhold their asset until age is confirmed.
    w.svc
        .create_trailer(
            w.publisher,
            spicy.id,
            TrailerInput {
                label: "Trailer".into(),
                kind: TrailerKind::Trailer,
                language: None,
                asset_id: Some(4242),
            },
        )
        .await
        .unwrap();
    let gated = w.svc.title_detail(w.member, spicy.id).await.unwrap();
    assert_eq!(gated.trailers.len(), 1);
    assert!(gated.trailers[0].age_gated && gated.trailers[0].asset_id.is_none());
    assert!(gated.title.has_trailer);
    let open = w.svc.title_detail(w.adult, spicy.id).await.unwrap();
    assert_eq!(open.trailers[0].asset_id, Some(4242));
    // A member who hides restricted titles loses them from search, rows and facets.
    let hider = Viewer {
        hide_restricted: true,
        ..w.member
    };
    w.svc.reindex_all().await.unwrap();
    let hits = w.svc.search(hider, "spicy", 10, None).await.unwrap();
    assert!(hits.titles.is_empty());
    let hits = w.svc.search(w.member, "spicy", 10, None).await.unwrap();
    assert_eq!(hits.titles.len(), 1);
    assert_eq!(w.svc.facets(hider, None).await.unwrap().total, 1);
    assert_eq!(w.svc.facets(w.member, None).await.unwrap().total, 2);
    let row = w
        .svc
        .row(hider, rows::KEY_RECENTLY_ADDED, None, BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(row.items.len(), 1);
    w.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn seasons_episodes_play_and_the_playable_invariant() {
    let w = world().await;
    let (t, s1, eps) = w.series("Show", 3).await;
    // Season 2 with one episode and a specials season.
    let s2 = w
        .svc
        .create_season(
            w.publisher,
            t.id,
            SeasonInput {
                number: 2,
                name: Some("The second".into()),
                synopsis: None,
                poster_key: None,
            },
        )
        .await
        .unwrap();
    let e21 = w.episode(s2.id, 1, true).await;
    let draft = w.episode(s2.id, 2, false).await;
    // Duplicate season / episode numbers conflict.
    assert!(matches!(
        w.svc
            .create_season(
                w.publisher,
                t.id,
                SeasonInput {
                    number: 2,
                    name: None,
                    synopsis: None,
                    poster_key: None
                }
            )
            .await,
        Err(CinemaError::Conflict(_))
    ));
    // An episode without an asset cannot be published.
    assert!(matches!(
        w.svc
            .set_episode_published(w.publisher, draft.id, true)
            .await,
        Err(CinemaError::Conflict(_))
    ));
    // Publish the title: the published episodes make it playable.
    let published = w.publish(t.id).await;
    assert_eq!(published.episode_count, 4);
    assert_eq!(published.season_count, 2);
    assert!(published.last_content_at_ms.is_some());

    let detail = w.svc.title_detail(w.member, t.id).await.unwrap();
    assert_eq!(detail.seasons.len(), 2);
    assert_eq!(detail.seasons[0].episode_count, 3);
    assert_eq!(
        detail.seasons[1].episode_count, 1,
        "the draft episode is not counted"
    );
    // Episode lists: order, paging, season filter.
    let all = w.svc.episodes(t.id, None, None, None).await.unwrap();
    assert_eq!(all.episodes.len(), 4);
    assert_eq!(
        all.episodes
            .iter()
            .map(|e| (e.season_number, e.number))
            .collect::<Vec<_>>(),
        [(1, 1), (1, 2), (1, 3), (2, 1)]
    );
    assert!(
        all.episodes
            .iter()
            .all(|e| e.asset_id.is_none() && e.playable)
    );
    assert_eq!(all.episodes[0].air_date.as_deref(), Some("2024-02-29"));
    let mut seen = Vec::new();
    let mut after: Option<String> = None;
    loop {
        let page = w
            .svc
            .episodes(t.id, None, Some(3), after.as_deref())
            .await
            .unwrap();
        seen.extend(page.episodes.iter().map(|e| e.id));
        match page.next_cursor {
            Some(c) => after = Some(c),
            None => break,
        }
    }
    assert_eq!(seen, all.episodes.iter().map(|e| e.id).collect::<Vec<_>>());
    let only_s2 = w.svc.episodes(t.id, Some(s2.id), None, None).await.unwrap();
    assert_eq!(only_s2.episodes.len(), 1);
    assert!(matches!(
        w.svc.episode(draft.id).await,
        Err(CinemaError::NotFound(_))
    ));
    // Staff see the draft episode and its asset.
    let admin = w.svc.admin_title(w.publisher, t.id).await.unwrap();
    assert_eq!(admin.episodes.len(), 5);
    assert!(
        admin
            .episodes
            .iter()
            .any(|e| e.id == draft.id && !e.published)
    );

    // Play: the title plays its first episode; episodes know their neighbours.
    let first = w
        .svc
        .resolve_play(
            w.member,
            PlayRef::Title {
                id: t.id,
                variant: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(first.episode_id, Some(eps[0].id));
    assert_eq!(first.next_episode_id, Some(eps[1].id));
    assert_eq!(first.previous_episode_id, None);
    let last_of_s1 = w
        .svc
        .resolve_play(w.member, PlayRef::Episode(eps[2].id))
        .await
        .unwrap();
    assert_eq!(last_of_s1.previous_episode_id, Some(eps[1].id));
    assert_eq!(
        last_of_s1.next_episode_id,
        Some(e21.id),
        "the next season follows"
    );
    assert_eq!(last_of_s1.season_number, Some(1));
    assert_eq!(last_of_s1.episode_title.as_deref(), Some("Episode 3"));
    let end = w
        .svc
        .resolve_play(w.member, PlayRef::Episode(e21.id))
        .await
        .unwrap();
    assert_eq!(end.next_episode_id, None, "the draft episode is skipped");

    // Removing the last playable thing unpublishes the title.
    for e in [&eps[0], &eps[1], &eps[2]] {
        w.svc
            .set_episode_published(w.publisher, e.id, false)
            .await
            .unwrap();
    }
    assert!(
        w.svc.title_detail(w.member, t.id).await.is_ok(),
        "one episode is left"
    );
    w.svc.delete_episode(w.publisher, e21.id).await.unwrap();
    assert!(matches!(
        w.svc.title_detail(w.member, t.id).await,
        Err(CinemaError::NotFound(_))
    ));
    let admin = w.svc.admin_title(w.publisher, t.id).await.unwrap();
    assert!(!admin.title.published, "auto-unpublished");
    assert_eq!(w.seen.episodes_deleted.lock().unwrap().len(), 1);
    // Deleting a season takes its episodes with it and tells the observer.
    w.svc.delete_season(w.publisher, s1.id).await.unwrap();
    let after = w.svc.admin_title(w.publisher, t.id).await.unwrap();
    assert_eq!(after.seasons.len(), 1);
    assert_eq!(w.seen.episodes_deleted.lock().unwrap().len(), 2);
    w.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn variants_trailers_and_markers() {
    let w = world().await;
    let t = w.film("Cuts", |_| {}).await;
    let v = w
        .svc
        .create_variant(
            w.publisher,
            t.id,
            VariantInput {
                label: "Director's Cut".into(),
                notes: None,
                runtime_min: Some(140),
                asset_id: Some(9001),
                position: None,
            },
        )
        .await
        .unwrap();
    // A second variant without an asset is staff-only; a duplicate label conflicts.
    w.svc
        .create_variant(
            w.publisher,
            t.id,
            VariantInput {
                label: "Extended".into(),
                notes: None,
                runtime_min: None,
                asset_id: None,
                position: None,
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        w.svc
            .create_variant(
                w.publisher,
                t.id,
                VariantInput {
                    label: "Extended".into(),
                    notes: None,
                    runtime_min: None,
                    asset_id: None,
                    position: None
                }
            )
            .await,
        Err(CinemaError::Conflict(_))
    ));
    let detail = w.svc.title_detail(w.member, t.id).await.unwrap();
    assert_eq!(
        detail.variants.len(),
        1,
        "only variants with an asset are listed"
    );
    assert_eq!(detail.variants[0].label, "Director's Cut");
    assert_eq!(
        w.svc
            .admin_title(w.publisher, t.id)
            .await
            .unwrap()
            .variants
            .len(),
        2
    );
    let play = w
        .svc
        .resolve_play(
            w.member,
            PlayRef::Title {
                id: t.id,
                variant: Some(v.id),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        (play.asset_id, play.variant_id, play.runtime_min),
        (9001, Some(v.id), Some(140))
    );
    assert!(matches!(
        w.svc
            .resolve_play(
                w.member,
                PlayRef::Title {
                    id: t.id,
                    variant: Some(uid())
                }
            )
            .await,
        Err(CinemaError::NotFound(_))
    ));

    // Markers: replaced as a set, one of each kind, shown to the player.
    let m = |kind, a, b| MarkerInput {
        kind,
        start_ms: a,
        end_ms: b,
    };
    let list = w
        .svc
        .replace_markers(
            w.publisher,
            Some(t.id),
            None,
            vec![
                m(MarkerKind::Credits, 5_000_000, 5_400_000),
                m(MarkerKind::Intro, 1000, 60_000),
            ],
        )
        .await
        .unwrap();
    assert_eq!(list.markers.len(), 2);
    assert!(list.markers[0].id.is_some(), "staff see ids");
    let public = w.svc.title_markers(t.id).await.unwrap();
    assert_eq!(public.markers[0].kind, MarkerKind::Intro);
    assert!(public.markers[0].id.is_none() && public.markers[0].source.is_none());
    let replaced = w
        .svc
        .replace_markers(
            w.publisher,
            Some(t.id),
            None,
            vec![m(MarkerKind::Recap, 0, 500)],
        )
        .await
        .unwrap();
    assert_eq!(replaced.markers.len(), 1);
    assert_eq!(w.svc.title_markers(t.id).await.unwrap().markers.len(), 1);
    assert!(
        w.svc
            .replace_markers(w.publisher, Some(uid()), None, vec![])
            .await
            .is_err(),
        "unknown title"
    );
    // Episode markers belong to the episode only.
    let (st, _s, eps) = w.series("Marked", 2).await;
    w.publish(st.id).await;
    w.svc
        .replace_markers(
            w.publisher,
            None,
            Some(eps[0].id),
            vec![m(MarkerKind::Intro, 100, 900)],
        )
        .await
        .unwrap();
    assert_eq!(
        w.svc
            .episode_markers(eps[0].id)
            .await
            .unwrap()
            .markers
            .len(),
        1
    );
    assert!(
        w.svc
            .episode_markers(eps[1].id)
            .await
            .unwrap()
            .markers
            .is_empty()
    );
    assert!(w.svc.title_markers(st.id).await.unwrap().markers.is_empty());
    let target = w
        .svc
        .resolve_play(w.member, PlayRef::Episode(eps[0].id))
        .await
        .unwrap();
    assert_eq!(target.markers.len(), 1);
    assert_eq!(
        w.svc
            .admin_episode_markers(w.publisher, eps[0].id)
            .await
            .unwrap()
            .markers[0]
            .source
            .as_deref(),
        Some("manual")
    );
    // Trailers without an asset are hidden from members.
    w.svc
        .create_trailer(
            w.publisher,
            t.id,
            TrailerInput {
                label: "Teaser".into(),
                kind: TrailerKind::Teaser,
                language: Some("EN".into()),
                asset_id: None,
            },
        )
        .await
        .unwrap();
    let hosted = w
        .svc
        .create_trailer(
            w.publisher,
            t.id,
            TrailerInput {
                label: "Trailer".into(),
                kind: TrailerKind::Trailer,
                language: None,
                asset_id: Some(55),
            },
        )
        .await
        .unwrap();
    let detail = w.svc.title_detail(w.member, t.id).await.unwrap();
    assert_eq!(detail.trailers.len(), 1);
    assert_eq!(detail.trailers[0].asset_id, Some(55));
    assert!(detail.title.has_trailer);
    w.svc.delete_trailer(w.publisher, hosted.id).await.unwrap();
    assert!(
        !w.svc
            .title_detail(w.member, t.id)
            .await
            .unwrap()
            .title
            .has_trailer
    );
    w.svc.delete_variant(w.publisher, v.id).await.unwrap();
    assert!(w.svc.delete_variant(w.publisher, v.id).await.is_err());
    w.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn merging_duplicates() {
    let w = world().await;
    // Target: season 1 with episodes 1 (asset) and 2 (no asset, draft). Source: season 1 with
    // episodes 2 (asset) and 3, and a season 2.
    let (target, ts1, teps) = w.series("Show", 1).await;
    let t_e2 = w.episode(ts1.id, 2, false).await;
    let source = w
        .svc
        .create_title(w.publisher, input(TitleKind::Series, "Show (duplicate)"))
        .await
        .unwrap();
    let ss1 = w
        .svc
        .create_season(
            w.publisher,
            source.id,
            SeasonInput {
                number: 1,
                name: Some("Season one".into()),
                synopsis: None,
                poster_key: None,
            },
        )
        .await
        .unwrap();
    let s_e2 = w.episode(ss1.id, 2, true).await;
    let s_e3 = w.episode(ss1.id, 3, true).await;
    let ss2 = w
        .svc
        .create_season(
            w.publisher,
            source.id,
            SeasonInput {
                number: 2,
                name: None,
                synopsis: None,
                poster_key: None,
            },
        )
        .await
        .unwrap();
    let s2_e1 = w.episode(ss2.id, 1, true).await;
    w.svc
        .replace_markers(
            w.publisher,
            None,
            Some(s_e2.id),
            vec![MarkerInput {
                kind: MarkerKind::Intro,
                start_ms: 10,
                end_ms: 20,
            }],
        )
        .await
        .unwrap();
    w.svc
        .create_variant(
            w.publisher,
            source.id,
            VariantInput {
                label: "Extended".into(),
                notes: None,
                runtime_min: None,
                asset_id: Some(3),
                position: None,
            },
        )
        .await
        .unwrap();
    w.svc
        .create_variant(
            w.publisher,
            target.id,
            VariantInput {
                label: "Extended".into(),
                notes: None,
                runtime_min: None,
                asset_id: Some(4),
                position: None,
            },
        )
        .await
        .unwrap();
    w.publish(source.id).await;
    w.publish(target.id).await;
    // A collection holding the source.
    let c = w
        .svc
        .create_collection(
            w.curator,
            CollectionInput {
                slug: "picks".into(),
                name: "Picks".into(),
                eyebrow: None,
                blurb: None,
                display: CollectionDisplay::Row,
                surfaces: vec![Surface::Discover],
                position: 0,
                published: true,
            },
        )
        .await
        .unwrap();
    w.svc
        .set_collection_items(w.curator, c.id, vec![source.id])
        .await
        .unwrap();

    // Needs both permissions; a title cannot merge into itself.
    let only_publish = Actor {
        user_id: w.publisher.user_id,
        permissions: P::PUBLISH_MEDIA,
    };
    assert!(
        w.svc
            .merge_titles(only_publish, source.id, target.id)
            .await
            .is_err()
    );
    assert!(matches!(
        w.svc.merge_titles(w.publisher, source.id, source.id).await,
        Err(CinemaError::Invalid(_))
    ));
    let merged = w
        .svc
        .merge_titles(w.publisher, source.id, target.id)
        .await
        .unwrap();
    assert_eq!(merged.id, target.id);
    assert!(merged.alt_titles.contains(&"Show (duplicate)".to_owned()));

    // Seasons and episodes: target episode 2 took the source's asset (it had none) and became
    // published; the source's duplicate was dropped; episode 3 and season 2 moved.
    let admin = w.svc.admin_title(w.publisher, target.id).await.unwrap();
    assert_eq!(
        admin.seasons.iter().map(|s| s.number).collect::<Vec<_>>(),
        [1, 2]
    );
    let numbers: Vec<(i32, i32)> = admin
        .episodes
        .iter()
        .map(|e| (e.season_number, e.number))
        .collect();
    assert_eq!(numbers, [(1, 1), (1, 2), (1, 3), (2, 1)]);
    let e2 = admin.episodes.iter().find(|e| e.id == t_e2.id).unwrap();
    assert_eq!(e2.asset_id, s_e2.asset_id);
    assert!(
        e2.asset_id.is_some() && e2.published,
        "the target filled its gap from the source"
    );
    assert!(admin.episodes.iter().any(|e| e.id == s_e3.id));
    assert!(admin.episodes.iter().any(|e| e.id == s2_e1.id));
    assert!(admin.episodes.iter().all(|e| e.id != s_e2.id));
    assert_eq!(
        admin
            .episodes
            .iter()
            .find(|e| e.id == teps[0].id)
            .map(|e| e.number),
        Some(1)
    );
    // The duplicate's marker moved onto the surviving episode.
    assert_eq!(
        w.svc.episode_markers(t_e2.id).await.unwrap().markers.len(),
        1
    );
    // Variants: the clashing label got a suffix.
    let labels: Vec<&str> = admin.variants.iter().map(|v| v.label.as_str()).collect();
    assert_eq!(labels.len(), 2);
    assert!(labels.contains(&"Extended") && labels.iter().any(|l| l.starts_with("Extended (")));
    // Redirect: the source answers with the target and leaves search and browse.
    let via_old = w.svc.title_detail(w.member, source.id).await.unwrap();
    assert_eq!(via_old.title.id, target.id);
    let page = w.browse(BrowseQuery::default()).await;
    assert_eq!(titles(&page), ["Show"]);
    // Collection places followed.
    let detail = w.svc.admin_collection(w.curator, c.id).await.unwrap();
    assert_eq!(
        detail.items.iter().map(|t| t.id).collect::<Vec<_>>(),
        [target.id]
    );
    // The observer heard about it with the dropped duplicate.
    let seen = w.seen.merged.lock().unwrap().clone();
    assert_eq!(seen, vec![(source.id, target.id, vec![(s_e2.id, t_e2.id)])]);
    // A merged title cannot be merged again or published.
    assert!(matches!(
        w.svc.merge_titles(w.publisher, source.id, target.id).await,
        Err(CinemaError::Conflict(_))
    ));
    assert!(matches!(
        w.svc
            .set_title_published(w.publisher, source.id, true)
            .await,
        Err(CinemaError::Conflict(_))
    ));
    w.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn collections_rows_and_discover() {
    let ghost = uid();
    let w = world().await;
    let a = w.film("Alpha", |i| i.genres = vec!["noir".into()]).await;
    let b = w
        .film("Bravo", |i| {
            i.genres = vec!["noir".into()];
            i.backdrop_key = Some("cinema/2/backdrop.webp".into());
        })
        .await;
    let anime = w
        .svc
        .create_title(w.publisher, {
            let mut i = input(TitleKind::Anime, "Cowboy");
            i.backdrop_key = Some("cinema/3/backdrop.webp".into());
            i
        })
        .await
        .unwrap();
    w.svc
        .link_title_asset(w.publisher, anime.id, Some(1))
        .await
        .unwrap();
    w.publish(anime.id).await;
    let (show, _s, _eps) = w.series("Episodic", 2).await;
    w.publish(show.id).await;
    let draft = w
        .svc
        .create_title(w.publisher, input(TitleKind::Movie, "Draft"))
        .await
        .unwrap();

    // Collections need MANAGE_COLLECTIONS.
    let input_c = |slug: &str, display, published| CollectionInput {
        slug: slug.into(),
        name: format!("Collection {slug}"),
        eyebrow: Some("SELECTED FOR THE ROOM".into()),
        blurb: None,
        display,
        surfaces: vec![Surface::Discover, Surface::Movies],
        position: 0,
        published,
    };
    assert!(
        w.svc
            .create_collection(w.publisher, input_c("nope", CollectionDisplay::Row, true))
            .await
            .is_err()
    );
    let good = w
        .svc
        .create_collection(
            w.curator,
            input_c("good-place", CollectionDisplay::Row, true),
        )
        .await
        .unwrap();
    assert!(matches!(
        w.svc
            .create_collection(
                w.curator,
                input_c("good-place", CollectionDisplay::Row, true)
            )
            .await,
        Err(CinemaError::Conflict(_))
    ));
    let hero = w
        .svc
        .create_collection(w.curator, input_c("tonight", CollectionDisplay::Hero, true))
        .await
        .unwrap();
    let hidden = w
        .svc
        .create_collection(w.curator, input_c("secret", CollectionDisplay::Row, false))
        .await
        .unwrap();
    // Items: order is kept, unknown titles are refused, drafts are accepted but not shown.
    let detail = w
        .svc
        .set_collection_items(w.curator, good.id, vec![b.id, draft.id, a.id])
        .await
        .unwrap();
    assert_eq!(
        detail.items.iter().map(|t| t.id).collect::<Vec<_>>(),
        [b.id, draft.id, a.id]
    );
    assert!(matches!(
        w.svc
            .set_collection_items(w.curator, good.id, vec![a.id, ghost])
            .await,
        Err(CinemaError::Invalid(_))
    ));
    w.svc
        .set_collection_items(w.curator, hero.id, vec![anime.id])
        .await
        .unwrap();
    w.svc
        .set_collection_items(w.curator, hidden.id, vec![a.id])
        .await
        .unwrap();

    let list = w.svc.collections(Some(Surface::Discover)).await.unwrap();
    let names: Vec<&str> = list.items.iter().map(|c| c.slug.as_str()).collect();
    assert_eq!(names.len(), 2, "unpublished collections are not listed");
    assert!(names.contains(&"good-place") && names.contains(&"tonight"));
    assert_eq!(
        list.items
            .iter()
            .find(|c| c.slug == "good-place")
            .unwrap()
            .item_count,
        2,
        "drafts not counted"
    );
    assert!(
        w.svc
            .collections(Some(Surface::Anime))
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert_eq!(
        w.svc
            .admin_collections(w.curator)
            .await
            .unwrap()
            .items
            .len(),
        3
    );

    // Browse by collection ("Room selection"): curated order, drafts and hidden collections excluded.
    let curated = w
        .browse(BrowseQuery {
            collection: Some(good.id),
            sort: SortKey::Curated,
            ..BrowseQuery::default()
        })
        .await;
    assert_eq!(titles(&curated), ["Bravo", "Alpha"]);
    let none = w
        .browse(BrowseQuery {
            collection: Some(hidden.id),
            sort: SortKey::Curated,
            ..BrowseQuery::default()
        })
        .await;
    assert_eq!(none.total, 0);
    // Keyset paging inside a curated collection.
    let p1 = w
        .svc
        .browse(
            w.member,
            BrowseQuery {
                collection: Some(good.id),
                sort: SortKey::Curated,
                limit: 1,
                ..BrowseQuery::default()
            },
            None,
        )
        .await
        .unwrap();
    let p2 = w
        .svc
        .browse(
            w.member,
            BrowseQuery {
                collection: Some(good.id),
                sort: SortKey::Curated,
                limit: 1,
                ..BrowseQuery::default()
            },
            p1.next_cursor.as_deref(),
        )
        .await
        .unwrap();
    assert_eq!((titles(&p1), titles(&p2)), (vec!["Bravo"], vec!["Alpha"]));
    assert!(p2.next_cursor.is_none());

    // Generated rows by key.
    let row = |key: &'static str| {
        let svc = w.svc.clone();
        let v = w.member;
        async move { svc.row(v, key, None, BTreeMap::new()).await }
    };
    let anime_room = row(rows::KEY_ANIME_ROOM).await.unwrap();
    assert_eq!(anime_room.kind, RowKind::Generated);
    assert_eq!(
        anime_room
            .items
            .iter()
            .map(|i| i.title.title.as_str())
            .collect::<Vec<_>>(),
        ["Cowboy"]
    );
    let films = row(rows::KEY_FILMS).await.unwrap();
    assert_eq!(films.items.len(), 2);
    let one_more = row(rows::KEY_ONE_MORE_EPISODE).await.unwrap();
    assert_eq!(
        one_more
            .items
            .iter()
            .map(|i| i.title.title.as_str())
            .collect::<Vec<_>>(),
        ["Episodic"]
    );
    let fresh = row(rows::KEY_NEW_THIS_WEEK).await.unwrap();
    assert_eq!(fresh.items.len(), 4, "everything was published just now");
    let noir = w
        .svc
        .row(w.member, "genre:Noir", None, BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(noir.items.len(), 2);
    assert_eq!(noir.explore.as_deref(), Some("genres=noir"));
    let coll = w
        .svc
        .row(
            w.member,
            &format!("collection:{}", good.id),
            None,
            BTreeMap::new(),
        )
        .await
        .unwrap();
    assert_eq!(coll.kind, RowKind::Curated);
    assert_eq!(coll.items.len(), 2);
    assert_eq!(coll.eyebrow.as_deref(), Some("SELECTED FOR THE ROOM"));
    assert!(matches!(
        w.svc
            .row(
                w.member,
                &format!("collection:{}", hidden.id),
                None,
                BTreeMap::new()
            )
            .await,
        Err(CinemaError::NotFound(_))
    ));
    assert!(matches!(
        row("nonsense").await,
        Err(CinemaError::NotFound(_))
    ));
    assert!(
        matches!(
            row(rows::KEY_CONTINUE_WATCHING).await,
            Err(CinemaError::NotFound(_))
        ),
        "nobody provides it"
    );

    // Discover: hero from the curated collection, curated row, generated rows; empty rows are left out.
    let d = w.svc.discover(w.member).await.unwrap();
    assert_eq!(d.hero.kind, RowKind::Curated);
    assert_eq!(
        d.hero
            .items
            .iter()
            .map(|i| i.title.title.as_str())
            .collect::<Vec<_>>(),
        ["Cowboy"]
    );
    let keys: Vec<&str> = d.rows.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            format!("collection:{}", good.id).as_str(),
            "animation-banner",
            "anime-room",
            "one-more-episode",
            "films",
            "new-this-week",
            "recently-added",
        ]
    );
    let banner = d.rows.iter().find(|r| r.key == "animation-banner").unwrap();
    assert_eq!(banner.display, CollectionDisplay::Banner);
    // Without a hero collection the hero falls back to the newest titles with a backdrop.
    w.svc
        .update_collection(
            w.curator,
            hero.id,
            input_c("tonight", CollectionDisplay::Hero, false),
        )
        .await
        .unwrap();
    let d = w.svc.discover(w.member).await.unwrap();
    assert_eq!(d.hero.kind, RowKind::Generated);
    assert_eq!(d.hero.items.len(), 2, "Bravo and Cowboy have a backdrop");
    w.svc.delete_collection(w.curator, hidden.id).await.unwrap();
    assert!(w.svc.delete_collection(w.curator, hidden.id).await.is_err());
    w.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn row_providers_plug_in_and_cannot_leak_drafts() {
    // The provider names a real title, a draft and an unknown id.
    let probe = world().await;
    let published = probe.film("Seen It", |_| {}).await;
    let draft = probe
        .svc
        .create_title(probe.publisher, input(TitleKind::Movie, "Not Yet"))
        .await
        .unwrap();
    let entries = vec![
        RowEntry {
            title_id: published.id,
            episode_id: Some(77),
            progress_percent: Some(140.0),
            reason: Some("Because you watched Other".into()),
        },
        RowEntry::title(draft.id),
        RowEntry::title(uid()),
    ];
    // A second service over the same database with the provider registered.
    let svc = CinemaService::new(
        Arc::new(PgCinemaRepo::new(probe.db.pool().clone())),
        CdnSigner::new(vec![7u8; 32]),
        Arc::new(SystemClock),
        Arc::new(uid),
    )
    .with_provider(Arc::new(Fixed {
        key: rows::KEY_CONTINUE_WATCHING,
        entries: entries.clone(),
    }))
    .with_provider(Arc::new(Fixed {
        key: rows::KEY_TRENDING,
        entries,
    }));
    let row = svc
        .row(
            probe.member,
            rows::KEY_CONTINUE_WATCHING,
            None,
            BTreeMap::new(),
        )
        .await
        .unwrap();
    assert_eq!(row.kind, RowKind::Provided);
    assert_eq!(row.title, "Continue watching");
    assert_eq!(row.items.len(), 1, "draft and unknown titles are dropped");
    let item = &row.items[0];
    assert_eq!(item.title.id, published.id);
    assert_eq!(item.episode_id, Some(77));
    assert_eq!(item.progress_percent, Some(100.0), "clamped");
    assert_eq!(item.reason.as_deref(), Some("Because you watched Other"));
    // Request parameters reach the provider, which may retitle the row.
    let scoped = svc
        .row(
            probe.member,
            rows::KEY_TRENDING,
            Some(5),
            BTreeMap::from([("title".to_owned(), "Trending among friends".to_owned())]),
        )
        .await
        .unwrap();
    assert_eq!(scoped.title, "Trending among friends");
    // Discover places provided rows where the page wants them.
    let d = svc.discover(probe.member).await.unwrap();
    let keys: Vec<&str> = d.rows.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(keys.first(), Some(&"continue-watching"));
    assert!(keys.contains(&"trending"));
    // Wire shape: the extras sit next to the flattened title fields.
    let json = serde_json::to_value(&row.items[0]).unwrap();
    assert_eq!(json["title"], "Seen It");
    assert_eq!(json["episode_id"], "77");
    assert_eq!(json["progress_percent"], 100.0);
    probe.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn instant_search_is_rebuilt_from_the_database_and_follows_changes() {
    let w = world().await;
    let a = w
        .film("Blade Runner", |i| {
            i.cast = vec!["Harrison Ford".into()];
            i.original_title = Some("Do Androids Dream".into());
            i.genres = vec!["science fiction".into()];
        })
        .await;
    let b = w
        .film("Bladerunners", |i| {
            i.directors = vec!["Ridley Scott".into()];
        })
        .await;
    w.svc
        .create_title(w.publisher, input(TitleKind::Movie, "Blade Draft"))
        .await
        .unwrap();
    let anime = w
        .svc
        .create_title(
            w.publisher,
            input(TitleKind::Anime, "Blade of the Immortal"),
        )
        .await
        .unwrap();
    w.svc
        .link_title_asset(w.publisher, anime.id, Some(9))
        .await
        .unwrap();
    w.publish(anime.id).await;
    // Rebuild from Postgres like at boot: a fresh service with an empty index.
    let fresh = CinemaService::new(
        Arc::new(PgCinemaRepo::new(w.db.pool().clone())),
        CdnSigner::new(vec![7u8; 32]),
        Arc::new(SystemClock),
        Arc::new(uid),
    )
    .with_index(Arc::new(TitleIndex::in_ram().unwrap()));
    assert!(
        fresh
            .search(w.member, "blade", 10, None)
            .await
            .unwrap()
            .titles
            .is_empty()
    );
    assert_eq!(
        fresh.reindex_all().await.unwrap(),
        3,
        "only published, non-merged titles"
    );
    let ids = |s: &InstantSearch| s.titles.iter().map(|t| t.id).collect::<Vec<_>>();
    let hits = fresh.search(w.member, "blade", 10, None).await.unwrap();
    assert_eq!(hits.titles.len(), 3, "the draft is not indexed");
    let by_cast = fresh.search(w.member, "harris", 10, None).await.unwrap();
    assert_eq!(ids(&by_cast), [a.id]);
    assert_eq!(by_cast.people, ["Harrison Ford"]);
    let by_original = fresh
        .search(w.member, "androids dr", 10, None)
        .await
        .unwrap();
    assert_eq!(ids(&by_original), [a.id]);
    let genre = fresh.search(w.member, "science", 10, None).await.unwrap();
    assert_eq!(genre.genres, ["science fiction"]);
    let anime_only = fresh
        .search(w.member, "blade", 10, Some(TitleKind::Anime))
        .await
        .unwrap();
    assert_eq!(ids(&anime_only), [anime.id]);
    // The service keeps its own index in step with publishing, editing, merging and deleting.
    w.svc.reindex_all().await.unwrap();
    w.svc
        .set_title_published(w.publisher, b.id, false)
        .await
        .unwrap();
    assert_eq!(
        w.svc
            .search(w.member, "bladerun", 10, None)
            .await
            .unwrap()
            .titles
            .len(),
        0
    );
    let mut edited = input(TitleKind::Movie, "Replicant");
    edited.cast = vec!["Rutger Hauer".into()];
    w.svc.update_title(w.publisher, a.id, edited).await.unwrap();
    assert!(
        w.svc
            .search(w.member, "blade runner", 10, None)
            .await
            .unwrap()
            .titles
            .is_empty()
    );
    assert_eq!(
        w.svc
            .search(w.member, "hauer", 10, None)
            .await
            .unwrap()
            .titles
            .len(),
        1
    );
    w.svc.delete_title(w.publisher, a.id).await.unwrap();
    assert!(
        w.svc
            .search(w.member, "hauer", 10, None)
            .await
            .unwrap()
            .titles
            .is_empty()
    );
    // A stale index cannot show a draft: the hit is re-read through the public scope.
    w.svc
        .set_title_published(w.publisher, anime.id, false)
        .await
        .unwrap();
    assert!(
        w.svc
            .search(w.member, "immortal", 10, None)
            .await
            .unwrap()
            .titles
            .is_empty()
    );
    w.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn facets_count_the_published_catalog() {
    let w = world().await;
    w.film("One", |i| {
        i.genres = vec!["drama".into(), "crime".into()];
        i.year = Some(1980);
        i.runtime_min = Some(90);
        i.content_rating = Some("R".into());
        i.languages = vec!["en".into(), "fr".into()];
        i.subtitle_languages = vec!["es".into()];
    })
    .await;
    w.film("Two", |i| {
        i.genres = vec!["drama".into()];
        i.year = Some(2015);
        i.runtime_min = Some(150);
        i.content_rating = Some("R".into());
        i.languages = vec!["en".into()];
    })
    .await;
    w.svc
        .create_title(w.publisher, input(TitleKind::Movie, "Unseen draft"))
        .await
        .unwrap();
    let f = w.svc.facets(w.member, None).await.unwrap();
    assert_eq!(f.total, 2);
    assert_eq!(
        (f.year_min, f.year_max, f.runtime_max),
        (Some(1980), Some(2015), Some(150))
    );
    assert_eq!(
        f.genres[0],
        FacetCount {
            name: "drama".into(),
            count: 2
        }
    );
    assert_eq!(f.genres.len(), 2);
    assert_eq!(
        f.ratings,
        vec![FacetCount {
            name: "R".into(),
            count: 2
        }]
    );
    assert_eq!(
        f.languages[0],
        FacetCount {
            name: "en".into(),
            count: 2
        }
    );
    assert_eq!(
        f.subtitle_languages,
        vec![FacetCount {
            name: "es".into(),
            count: 1
        }]
    );
    assert_eq!(
        w.svc
            .facets(w.member, Some(TitleKind::Anime))
            .await
            .unwrap()
            .total,
        0
    );
    w.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn artwork_is_signed_and_external_ids_round_trip() {
    let w = world().await;
    let t = w
        .film("Pictured", |i| {
            i.backdrop_key = Some("cinema/9/backdrop.webp".into());
            i.logo_key = Some("cinema/9/logo.png".into());
            i.external_ids =
                BTreeMap::from([("imdb".into(), "tt1".into()), ("tmdb".into(), "2".into())]);
        })
        .await;
    let seen = w.svc.title_detail(w.member, t.id).await.unwrap().title;
    for url in [&seen.poster_url, &seen.backdrop_url, &seen.logo_url] {
        let u = url.as_deref().unwrap();
        assert!(u.starts_with("/cdn/cinema/"), "{u}");
        assert!(
            u.contains("sig=") || u.contains("signature") || u.contains('?'),
            "{u}"
        );
    }
    assert_eq!(
        seen.external_ids.get("imdb").map(String::as_str),
        Some("tt1")
    );
    // Bad keys are refused before the database sees them.
    let mut bad = input(TitleKind::Movie, "Bad");
    bad.poster_key = Some("library/x.webp".into());
    assert!(matches!(
        w.svc.create_title(w.publisher, bad).await,
        Err(CinemaError::Invalid(_))
    ));
    w.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn assets_are_authorized_through_their_catalog_owner() {
    let w = world().await;
    let film = w.film("Asset Film", |_| {}).await;
    let hot = w
        .film("Asset Restricted", |i| i.age_restricted = true)
        .await;
    let (series, _s, eps) = w.series("Asset Series", 2).await;
    let w_series = w.publish(series.id).await;
    assert!(w_series.published);

    let film_asset = w
        .svc
        .admin_title(w.publisher, film.id)
        .await
        .unwrap()
        .title
        .asset_id
        .unwrap();
    let ok = w.svc.authorize_asset(w.member, film_asset).await.unwrap();
    assert_eq!((ok.title_id, ok.episode_id), (film.id, None));

    // An episode asset resolves to its episode.
    let staff = w.svc.admin_title(w.publisher, series.id).await.unwrap();
    let ep = staff.episodes.iter().find(|e| e.id == eps[0].id).unwrap();
    let ok = w
        .svc
        .authorize_asset(w.member, ep.asset_id.unwrap())
        .await
        .unwrap();
    assert_eq!((ok.title_id, ok.episode_id), (series.id, Some(eps[0].id)));

    // Unknown asset: not found.
    assert_eq!(
        w.svc
            .authorize_asset(w.member, 987_654_321)
            .await
            .unwrap_err(),
        CinemaError::NotFound("asset")
    );

    // Restricted: visible title, gated asset (D-184).
    let hot_asset = w
        .svc
        .admin_title(w.publisher, hot.id)
        .await
        .unwrap()
        .title
        .asset_id
        .unwrap();
    assert_eq!(
        w.svc
            .authorize_asset(w.member, hot_asset)
            .await
            .unwrap_err(),
        CinemaError::AgeConfirmationRequired
    );
    assert!(w.svc.authorize_asset(w.adult, hot_asset).await.is_ok());

    // Unpublished owner: not found, whatever the viewer.
    w.svc
        .set_title_published(w.publisher, film.id, false)
        .await
        .unwrap();
    assert_eq!(
        w.svc
            .authorize_asset(w.adult, film_asset)
            .await
            .unwrap_err(),
        CinemaError::NotFound("asset")
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn contributors_count_live_credits_for_the_title_and_its_episodes() {
    let w = world().await;
    let pool = w.db.pool().clone();
    let (show, _s, eps) = w.series("Credited show", 4).await;
    w.publish(show.id).await;
    let other = w.film("Other film", |_| {}).await;
    let (a, b, c) = (
        account(&pool).await,
        account(&pool).await,
        account(&pool).await,
    );
    let credit = |user: i64, ref_id: i64| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO contribution_credits (id, section, ref_id, user_id) \
                 VALUES ($1, 'cinema', $2, $3)",
            )
            .bind(uid())
            .bind(ref_id)
            .bind(user)
            .execute(&pool)
            .await
            .unwrap();
        }
    };
    credit(a, show.id).await;
    credit(a, eps[0].id).await;
    credit(b, eps[1].id).await;
    credit(b, eps[2].id).await;
    credit(b, other.id).await; // another title: not counted here
    credit(c, eps[3].id).await; // will be revoked

    let repo = PgCinemaRepo::new(pool.clone());
    use sr_cinema::repo::CinemaRepository;
    sqlx::query("UPDATE contribution_credits SET revoked_at = now() WHERE user_id = $1")
        .bind(c)
        .execute(&pool)
        .await
        .unwrap();
    let got = repo.title_contributors(show.id).await.unwrap();
    let rows: Vec<(i64, i64, i64)> = got
        .iter()
        .map(|x| (x.user_id, x.titles, x.episodes))
        .collect();
    // Both have two credits here; the lower id comes first.
    let mut want = vec![(a, 1, 1), (b, 0, 2)];
    want.sort_by_key(|x| x.0);
    assert_eq!(rows, want);

    // An opt-out removes the member from the list.
    sqlx::query("INSERT INTO donor_preferences (user_id, hide_contributions) VALUES ($1, TRUE)")
        .bind(a)
        .execute(&pool)
        .await
        .unwrap();
    let got = repo.title_contributors(show.id).await.unwrap();
    assert_eq!(got.iter().map(|x| x.user_id).collect::<Vec<_>>(), vec![b]);
    // Through the service an unknown title is not found.
    assert!(w.svc.title_contributors(1).await.is_err());
}
