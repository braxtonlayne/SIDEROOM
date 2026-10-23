//! A staff change of the age flag reaches the library and cinema search indexes at once (MOD-AGE-001; run with
//! `SR_TEST_DATABASE_URL`, `--ignored`, `--test-threads=1`). The indexes are in RAM and filled by the services
//! themselves, as at runtime; only the flag change goes through `Moderation::set_age_restricted`.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use sqlx::PgPool;
use sr_admin::Caller;
use sr_cinema::model::{TitleInput, TitleKind};
use sr_cinema::{CinemaService, TitleIndex};
use sr_core::{PlatformPermissions as P, SystemClock};
use sr_db::accounts::{self, NewAccount};
use sr_db::cinema_repo::PgCinemaRepo;
use sr_db::library_repo::PgLibraryRepo;
use sr_db::testing::TestDb;
use sr_library::model::{SeriesInput, SeriesKind, SeriesStatus};
use sr_library::{LibraryService, SeriesIndex};
use sr_moderation::{AgeTargetKind, Moderation, model::Who};
use sr_storage::cdn::CdnSigner;

use crate::moderation_index::CatalogIndexSink;

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

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_age_change_updates_the_library_and_cinema_search_at_once() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let staff_id = account(&pool).await;
    let library = LibraryService::new(
        Arc::new(PgLibraryRepo::new(pool.clone())),
        CdnSigner::new(vec![7u8; 32]),
        Arc::new(SystemClock),
        Arc::new(uid),
    )
    .with_index(Arc::new(SeriesIndex::in_ram().unwrap()));
    let cinema = CinemaService::new(
        Arc::new(PgCinemaRepo::new(pool.clone())),
        CdnSigner::new(vec![7u8; 32]),
        Arc::new(SystemClock),
        Arc::new(uid),
    )
    .with_index(Arc::new(TitleIndex::in_ram().unwrap()));
    let m = Moderation::new(pool.clone(), Arc::new(uid)).with_item_index_sink(Arc::new(
        CatalogIndexSink {
            library: library.clone(),
            cinema: cinema.clone(),
        },
    ));

    let lib_actor = sr_library::Actor {
        user_id: staff_id,
        permissions: P::PUBLISH_READING | P::DELETE_CONTENT,
    };
    let series = library
        .create_series(
            lib_actor,
            SeriesInput {
                kind: SeriesKind::Manga,
                title: "Zebra Chronicle".into(),
                alt_titles: vec![],
                authors: vec![],
                artists: vec![],
                status: SeriesStatus::Ongoing,
                year: None,
                synopsis: String::new(),
                genres: vec![],
                tags: vec![],
                reading_direction: None,
                age_restricted: false,
                cover_key: None,
            },
        )
        .await
        .unwrap();
    library
        .set_series_published(lib_actor, series.id, true)
        .await
        .unwrap();

    let cin_actor = sr_cinema::Actor {
        user_id: staff_id,
        permissions: P::PUBLISH_MEDIA | P::DELETE_CONTENT,
    };
    let title = cinema
        .create_title(
            cin_actor,
            TitleInput {
                kind: TitleKind::Movie,
                title: "Zebra Night".into(),
                original_title: None,
                alt_titles: vec![],
                year: Some(2001),
                runtime_min: Some(100),
                content_rating: None,
                synopsis: "A story.".into(),
                tagline: None,
                genres: vec![],
                tags: vec![],
                languages: vec![],
                subtitle_languages: vec![],
                cast: vec![],
                directors: vec![],
                writers: vec![],
                studios: vec![],
                crew: vec![],
                age_restricted: false,
                poster_key: None,
                backdrop_key: None,
                logo_key: None,
                external_ids: BTreeMap::new(),
            },
        )
        .await
        .unwrap();
    cinema
        .link_title_asset(cin_actor, title.id, Some(uid()))
        .await
        .unwrap();
    cinema
        .set_title_published(cin_actor, title.id, true)
        .await
        .unwrap();

    // A viewer who hides 18+ items sees both while neither is marked.
    let lv = sr_library::Viewer {
        user_id: staff_id,
        adult: true,
        hide_restricted: true,
    };
    let cv = sr_cinema::Viewer {
        user_id: staff_id,
        adult: true,
        hide_restricted: true,
    };
    let lib_hits = || async { library.search(lv, "zebra", 10).await.unwrap().series.len() };
    let cin_hits = || async {
        cinema
            .search(cv, "zebra", 10, None)
            .await
            .unwrap()
            .titles
            .len()
    };
    assert_eq!((lib_hits().await, cin_hits().await), (1, 1));

    let s = Caller {
        user_id: staff_id,
        permissions: P::MANAGE_CASES,
        is_owner: false,
        ip: None,
    };
    let who = Who::Staff(&s);
    // Marked: gone from that viewer's search in the very next request, no rebuild.
    for (kind, id) in [
        (AgeTargetKind::LibrarySeries, series.id),
        (AgeTargetKind::CinemaTitle, title.id),
    ] {
        let r = m
            .set_age_restricted(&who, kind, id, true, Some("test"), None)
            .await
            .unwrap();
        assert!(r.changed);
    }
    assert_eq!((lib_hits().await, cin_hits().await), (0, 0));

    // Unmarked: back.
    for (kind, id) in [
        (AgeTargetKind::LibrarySeries, series.id),
        (AgeTargetKind::CinemaTitle, title.id),
    ] {
        m.set_age_restricted(&who, kind, id, false, Some("test"), None)
            .await
            .unwrap();
    }
    assert_eq!((lib_hits().await, cin_hits().await), (1, 1));
}
