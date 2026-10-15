//! DB-backed tests of the Library: the Postgres repository driven through the real
//! service (permissions, visibility, age gate, signed URLs, search). Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test library_repo -- --ignored --test-threads=1`

use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use sr_core::{PlatformPermissions as P, SystemClock};
use sr_db::{
    DbConfig, PgPool,
    accounts::{self, NewAccount},
    library_repo::PgLibraryRepo,
};
use sr_library::model::*;
use sr_library::{Actor, LibraryError, LibraryService, SeriesIndex, Viewer};
use sr_storage::cdn::CdnSigner;

fn uid() -> i64 {
    static N: AtomicI64 = AtomicI64::new(0);
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + N.fetch_add(1, Ordering::Relaxed) % 1000
}

async fn pool() -> PgPool {
    let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
    let pool = sr_db::connect(&DbConfig::new(url)).await.expect("connect");
    sr_db::migrate(&pool).await.expect("migrate");
    pool
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

fn signer() -> CdnSigner {
    CdnSigner::new(vec![7u8; 32])
}

fn service(pool: &PgPool) -> LibraryService {
    LibraryService::new(
        Arc::new(PgLibraryRepo::new(pool.clone())),
        signer(),
        Arc::new(SystemClock),
        Arc::new(uid),
    )
    .with_index(Arc::new(SeriesIndex::in_ram().unwrap()))
}

struct World {
    svc: LibraryService,
    publisher: Actor,
    member: Viewer,
    adult: Viewer,
    /// A genre unique to this test run, so listings can be asserted exactly.
    genre: String,
}

async fn world() -> World {
    let pool = pool().await;
    let svc = service(&pool);
    let p = account(&pool).await;
    let m = account(&pool).await;
    let a = account(&pool).await;
    World {
        svc,
        publisher: Actor {
            user_id: p,
            permissions: P::PUBLISH_READING | P::DELETE_CONTENT,
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
        genre: format!("g{}", uid()),
    }
}

impl World {
    fn input(&self, title: &str) -> SeriesInput {
        SeriesInput {
            kind: SeriesKind::Manga,
            title: format!("{title} {}", self.genre),
            alt_titles: vec![],
            authors: vec!["Test Author".into()],
            artists: vec![],
            status: SeriesStatus::Ongoing,
            year: Some(2020),
            synopsis: "A story.".into(),
            genres: vec![self.genre.clone()],
            tags: vec![],
            reading_direction: None,
            age_restricted: false,
            cover_key: None,
        }
    }

    fn browse(&self) -> BrowseQuery {
        BrowseQuery {
            genres: vec![self.genre.clone()],
            ..BrowseQuery::default()
        }
    }

    /// A published series with one published chapter of `pages` pages; returns
    /// (series id, edition id, chapter id).
    async fn published(&self, title: &str, restricted: bool, chapter: f64) -> (i64, i64, i64) {
        let mut input = self.input(title);
        input.age_restricted = restricted;
        let s = self.svc.create_series(self.publisher, input).await.unwrap();
        let e = self
            .svc
            .create_edition(
                self.publisher,
                s.id,
                EditionInput {
                    language: "en".into(),
                    label: "Official English".into(),
                    is_default: false,
                },
            )
            .await
            .unwrap();
        let c = self.chapter(s.id, e.id, chapter).await;
        self.svc
            .set_series_published(self.publisher, s.id, true)
            .await
            .unwrap();
        (s.id, e.id, c)
    }

    async fn chapter(&self, series: i64, edition: i64, number: f64) -> i64 {
        let c = self
            .svc
            .create_chapter(
                self.publisher,
                edition,
                ChapterInput {
                    number,
                    title: Some(format!("Chapter {number}")),
                    volume_id: None,
                    format: ChapterFormat::Images,
                    source_key: None,
                    release_group: Some("Group A".into()),
                },
            )
            .await
            .unwrap();
        self.svc
            .replace_pages(self.publisher, c.id, pages(series, c.id, 3))
            .await
            .unwrap();
        self.svc
            .set_chapter_published(self.publisher, c.id, true)
            .await
            .unwrap();
        c.id
    }
}

fn pages(series: i64, chapter: i64, n: usize) -> Vec<PageInput> {
    (0..n)
        .map(|i| PageInput {
            key: format!("library/{series}/{chapter}/{i:04}.webp"),
            variants: [720u32, 1080, 1600]
                .iter()
                .map(|w| {
                    (
                        w.to_string(),
                        format!("library/{series}/{chapter}/{i:04}_{w}.webp"),
                    )
                })
                .collect(),
            width: 1600,
            height: 2400,
            is_double_spread: i == 1,
            blurhash: Some("LEHV6nWB2yk8pyo0adR*.7kCMdnj".into()),
            crop: (i == 1).then_some(sr_library::ingest::crop::PageCrop {
                left: 10,
                top: 20,
                right: 30,
                bottom: 40,
            }),
        })
        .collect()
}

fn not_found(e: LibraryError) -> bool {
    matches!(e, LibraryError::NotFound(_))
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn catalog_lifecycle_and_page_manifest() {
    let w = world().await;
    let s = w
        .svc
        .create_series(w.publisher, w.input("Lifecycle"))
        .await
        .unwrap();
    assert!(!s.published);
    assert_eq!(s.reading_direction, ReadingDirection::Rtl);
    // Unpublished: invisible to members, visible to publishers.
    assert!(not_found(
        w.svc.series_detail(w.member, s.id).await.unwrap_err()
    ));
    assert!(w.svc.admin_series(w.publisher, s.id).await.is_ok());

    let e = w
        .svc
        .create_edition(
            w.publisher,
            s.id,
            EditionInput {
                language: "EN".into(),
                label: "Official English".into(),
                is_default: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(e.language, "en");
    // The first edition becomes the default.
    assert!(e.is_default);
    let dup = w
        .svc
        .create_edition(
            w.publisher,
            s.id,
            EditionInput {
                language: "en".into(),
                label: "Official English".into(),
                is_default: false,
            },
        )
        .await;
    assert!(matches!(dup, Err(LibraryError::Conflict(_))));

    let ch = w
        .svc
        .create_chapter(
            w.publisher,
            e.id,
            ChapterInput {
                number: 1.0,
                title: None,
                volume_id: None,
                format: ChapterFormat::Images,
                source_key: None,
                release_group: None,
            },
        )
        .await
        .unwrap();
    // No pages yet: cannot publish.
    let empty = w.svc.set_chapter_published(w.publisher, ch.id, true).await;
    assert!(matches!(empty, Err(LibraryError::Conflict(_))));
    // Pages outside the chapter's key prefix are refused.
    let mut bad = pages(s.id, ch.id, 1);
    bad[0].key = format!("library/{}/999/0000.webp", s.id);
    assert!(matches!(
        w.svc.replace_pages(w.publisher, ch.id, bad).await,
        Err(LibraryError::Invalid(_))
    ));
    let c = w
        .svc
        .replace_pages(w.publisher, ch.id, pages(s.id, ch.id, 3))
        .await
        .unwrap();
    assert_eq!(c.page_count, 3);
    w.svc
        .set_chapter_published(w.publisher, ch.id, true)
        .await
        .unwrap();
    // A published chapter of an unpublished series is still hidden.
    assert!(not_found(
        w.svc.page_manifest(w.member, ch.id).await.unwrap_err()
    ));

    w.svc
        .set_series_published(w.publisher, s.id, true)
        .await
        .unwrap();
    let detail = w.svc.series_detail(w.member, s.id).await.unwrap();
    assert!(detail.series.published && detail.series.published_at_ms.is_some());
    assert_eq!(detail.series.chapter_count, 1);
    assert_eq!(detail.editions.len(), 1);
    assert!(detail.follow.is_none());

    let list = w
        .svc
        .chapters(w.member, s.id, ChapterQuery::default())
        .await
        .unwrap();
    assert_eq!(list.chapters.len(), 1);
    assert!(list.chapters[0].progress.is_none());

    let m = w.svc.page_manifest(w.member, ch.id).await.unwrap();
    assert_eq!(m.pages.len(), 3);
    assert_eq!(m.reading_direction, ReadingDirection::Rtl);
    assert!(m.pages[1].is_double_spread);
    // Detected borders round-trip through the page table (LIB-ING-007).
    assert_eq!(m.pages[0].crop, None);
    assert_eq!(
        m.pages[1].crop.map(|c| (c.left, c.top, c.right, c.bottom)),
        Some((10, 20, 30, 40))
    );
    assert_eq!(
        m.pages[0]
            .variants
            .iter()
            .map(|v| v.width)
            .collect::<Vec<_>>(),
        [720, 1080, 1600]
    );
    // URLs are signed CDN links that verify.
    let url = &m.pages[0].url;
    let (path, query) = url.split_once('?').unwrap();
    assert_eq!(path, format!("/cdn/library/{}/{}/0000.webp", s.id, ch.id));
    let now = SystemClockSecs::now();
    signer().verify_query(path, query, now).unwrap();
    assert!(
        signer()
            .verify_query(path, &query.replace("sig=", "sig=x"), now)
            .is_err()
    );
    assert!(m.prev_chapter_id.is_none() && m.next_chapter_id.is_none());

    // Unpublishing the chapter hides it again and updates the counter.
    w.svc
        .set_chapter_published(w.publisher, ch.id, false)
        .await
        .unwrap();
    assert!(not_found(
        w.svc.page_manifest(w.member, ch.id).await.unwrap_err()
    ));
    assert_eq!(
        w.svc
            .series_detail(w.member, s.id)
            .await
            .unwrap()
            .series
            .chapter_count,
        0
    );
}

struct SystemClockSecs;
impl SystemClockSecs {
    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn neighbours_and_reordering_pages() {
    let w = world().await;
    let (s, e, c1) = w.published("Neighbours", false, 1.0).await;
    let c2 = w.chapter(s, e, 2.0).await;
    let c3 = w.chapter(s, e, 3.0).await;
    let m = w.svc.page_manifest(w.member, c2).await.unwrap();
    assert_eq!((m.prev_chapter_id, m.next_chapter_id), (Some(c1), Some(c3)));

    // Reorder: reverse the pages, drop one.
    let mut p = pages(s, c2, 3);
    p.reverse();
    p.pop();
    let ch = w
        .svc
        .replace_pages(w.publisher, c2, p.clone())
        .await
        .unwrap();
    assert_eq!(ch.page_count, 2);
    let m = w.svc.page_manifest(w.member, c2).await.unwrap();
    assert_eq!(m.pages.len(), 2);
    assert!(m.pages[0].url.contains("/0002.webp?"));
    assert_eq!(m.pages.iter().map(|p| p.index).collect::<Vec<_>>(), [0, 1]);
    // A published chapter keeps at least one page.
    assert!(matches!(
        w.svc.replace_pages(w.publisher, c2, vec![]).await,
        Err(LibraryError::Conflict(_))
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn publishers_need_the_right_permissions() {
    let w = world().await;
    let nobody = Actor {
        user_id: w.publisher.user_id,
        permissions: P::STAFF_BASIC,
    };
    let curator = Actor {
        user_id: w.publisher.user_id,
        permissions: P::PUBLISH_READING,
    };
    let remover = Actor {
        user_id: w.publisher.user_id,
        permissions: P::DELETE_CONTENT,
    };
    assert!(matches!(
        w.svc.create_series(nobody, w.input("Nope")).await,
        Err(LibraryError::MissingPermission("PUBLISH_READING"))
    ));
    assert!(matches!(
        w.svc.admin_list(nobody, w.browse()).await,
        Err(LibraryError::MissingPermission(_))
    ));
    let s = w.svc.create_series(curator, w.input("Perm")).await.unwrap();
    // Only DELETE_CONTENT may delete; a curator may not.
    assert!(matches!(
        w.svc.delete_series(curator, s.id).await,
        Err(LibraryError::MissingPermission("DELETE_CONTENT"))
    ));
    // Publishing needs PUBLISH_READING; unpublishing is allowed to either.
    assert!(matches!(
        w.svc.set_series_published(remover, s.id, true).await,
        Err(LibraryError::MissingPermission(_))
    ));
    w.svc
        .set_series_published(curator, s.id, true)
        .await
        .unwrap();
    assert!(
        !w.svc
            .set_series_published(remover, s.id, false)
            .await
            .unwrap()
            .published
    );
    // The platform Owner implies everything.
    let owner = Actor {
        user_id: w.publisher.user_id,
        permissions: P::OWNER,
    };
    w.svc.delete_series(owner, s.id).await.unwrap();
    assert!(not_found(
        w.svc.admin_series(owner, s.id).await.unwrap_err()
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn age_restricted_series_are_visible_but_pages_need_confirmation() {
    let w = world().await;
    let (ok_s, _, _) = w.published("Plain", false, 1.0).await;
    let (adult_s, _, adult_c) = w.published("Mature", true, 1.0).await;
    w.svc.reindex_all().await.unwrap();
    let ids = |p: &SeriesPage| p.items.iter().map(|s| s.id).collect::<Vec<_>>();

    // Browse: everyone sees restricted series by default; `hide` and `only` narrow it.
    for who in [w.member, w.adult] {
        let all = w.svc.browse(who, w.browse()).await.unwrap();
        assert_eq!(all.total, 2);
        let hide = BrowseQuery {
            age: AgeFilter::Hide,
            ..w.browse()
        };
        assert_eq!(ids(&w.svc.browse(who, hide).await.unwrap()), [ok_s]);
        let only = BrowseQuery {
            age: AgeFilter::Only,
            ..w.browse()
        };
        assert_eq!(ids(&w.svc.browse(who, only).await.unwrap()), [adult_s]);
    }

    // Detail, chapter list, follow: open to the unconfirmed.
    let d = w.svc.series_detail(w.member, adult_s).await.unwrap();
    assert!(d.series.age_restricted);
    assert!(
        w.svc
            .chapters(w.member, adult_s, ChapterQuery::default())
            .await
            .is_ok()
    );
    w.svc
        .follow(w.member, adult_s, Shelf::Reading, true)
        .await
        .unwrap();

    // Opening the pages is the gated step.
    assert_eq!(
        w.svc.page_manifest(w.member, adult_c).await.unwrap_err(),
        LibraryError::AgeConfirmationRequired
    );
    assert!(w.svc.page_manifest(w.adult, adult_c).await.is_ok());
    // An unknown chapter is still just not found.
    assert!(not_found(
        w.svc.page_manifest(w.member, 1).await.unwrap_err()
    ));
    // Staff are never gated.
    assert!(
        w.svc
            .admin_page_manifest(w.publisher, adult_c)
            .await
            .is_ok()
    );

    // Search, updates and follows include restricted series for everyone.
    for who in [w.member, w.adult] {
        let hits = w.svc.search(who, &w.genre, 20).await.unwrap();
        assert_eq!(hits.series.len(), 2);
    }
    w.svc
        .follow(w.member, ok_s, Shelf::Reading, true)
        .await
        .unwrap();
    let mine = |p: UpdatesPage| {
        let mut v: Vec<i64> = p
            .items
            .iter()
            .map(|u| u.series_id)
            .filter(|i| *i == adult_s || *i == ok_s)
            .collect();
        v.sort_unstable();
        v
    };
    let mut want = vec![ok_s, adult_s];
    want.sort_unstable();
    assert_eq!(
        mine(w.svc.updates(w.member, false, None, 100).await.unwrap()),
        want
    );
    assert_eq!(
        mine(w.svc.updates(w.member, true, None, 100).await.unwrap()),
        want
    );
    let f = w.svc.my_follows(w.member, None, None, None).await.unwrap();
    assert_eq!(f.total, 2);

    // `library.hide_age_restricted`: the same account drops restricted series from
    // search, updates and follows (browse takes its default from the router).
    let hider = Viewer {
        hide_restricted: true,
        ..w.member
    };
    let hits = w.svc.search(hider, &w.genre, 20).await.unwrap();
    assert_eq!(hits.series.iter().map(|s| s.id).collect::<Vec<_>>(), [ok_s]);
    assert_eq!(
        mine(w.svc.updates(hider, true, None, 100).await.unwrap()),
        [ok_s]
    );
    let f = w.svc.my_follows(hider, None, None, None).await.unwrap();
    assert_eq!(
        f.items.iter().map(|i| i.series.id).collect::<Vec<_>>(),
        [ok_s]
    );
    // Opening a restricted series directly is not affected by the preference.
    assert!(w.svc.series_detail(hider, adult_s).await.is_ok());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn browse_filters_sort_and_pagination() {
    let w = world().await;
    let mut ids = Vec::new();
    for (i, (title, kind, year, extra)) in [
        ("Alpha", SeriesKind::Manga, 2001, "action"),
        ("Bravo", SeriesKind::Manhwa, 2010, "romance"),
        ("Charlie", SeriesKind::Manga, 2020, "romance"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut inp = w.input(title);
        inp.kind = kind;
        inp.year = Some(year);
        inp.genres.push(format!("{extra}{}", w.genre));
        inp.tags = vec![format!("tag{i}{}", w.genre)];
        let s = w.svc.create_series(w.publisher, inp).await.unwrap();
        w.svc
            .set_series_published(w.publisher, s.id, true)
            .await
            .unwrap();
        ids.push(s.id);
    }
    let run = |q: BrowseQuery| {
        let svc = w.svc.clone();
        let v = w.member;
        async move { svc.browse(v, q).await.unwrap() }
    };
    let titles = |p: &SeriesPage| p.items.iter().map(|s| s.id).collect::<Vec<_>>();

    let all = run(BrowseQuery {
        sort: SortKey::Title,
        ..w.browse()
    })
    .await;
    assert_eq!((all.total, titles(&all)), (3, ids.clone()));
    let rev = run(BrowseQuery {
        sort: SortKey::Title,
        descending: Some(true),
        ..w.browse()
    })
    .await;
    assert_eq!(titles(&rev), [ids[2], ids[1], ids[0]]);
    let page2 = run(BrowseQuery {
        sort: SortKey::Title,
        limit: 2,
        offset: 2,
        ..w.browse()
    })
    .await;
    assert_eq!((page2.total, titles(&page2)), (3, vec![ids[2]]));

    let manga = run(BrowseQuery {
        kind: Some(SeriesKind::Manga),
        sort: SortKey::Title,
        ..w.browse()
    })
    .await;
    assert_eq!(titles(&manga), [ids[0], ids[2]]);
    let years = run(BrowseQuery {
        year_from: Some(2005),
        year_to: Some(2015),
        ..w.browse()
    })
    .await;
    assert_eq!(titles(&years), [ids[1]]);
    let mut q = w.browse();
    q.genres.push(format!("romance{}", w.genre));
    let rom = run(BrowseQuery {
        sort: SortKey::Title,
        ..q
    })
    .await;
    assert_eq!(titles(&rom), [ids[1], ids[2]]);
    let mut q = w.browse();
    q.exclude_genres.push(format!("romance{}", w.genre));
    assert_eq!(titles(&run(q).await), [ids[0]]);
    let tagged = run(BrowseQuery {
        tags: vec![format!("tag1{}", w.genre)],
        ..w.browse()
    })
    .await;
    assert_eq!(titles(&tagged), [ids[1]]);
    // Language needs a published chapter in that language.
    assert_eq!(
        run(BrowseQuery {
            language: Some("en".into()),
            ..w.browse()
        })
        .await
        .total,
        0
    );
    let e = w
        .svc
        .create_edition(
            w.publisher,
            ids[0],
            EditionInput {
                language: "en".into(),
                label: "Official".into(),
                is_default: true,
            },
        )
        .await
        .unwrap();
    w.chapter(ids[0], e.id, 1.0).await;
    let en = run(BrowseQuery {
        language: Some("en".into()),
        ..w.browse()
    })
    .await;
    assert_eq!(titles(&en), [ids[0]]);
    assert_eq!(
        run(BrowseQuery {
            language: Some("ja".into()),
            ..w.browse()
        })
        .await
        .total,
        0
    );
    // Most recently updated first: only the series with a chapter has a date.
    let upd = run(BrowseQuery {
        sort: SortKey::Updated,
        ..w.browse()
    })
    .await;
    assert_eq!(upd.items[0].id, ids[0]);

    // Staff scope sees unpublished series and can filter on state.
    let hidden = w
        .svc
        .create_series(w.publisher, w.input("Hidden"))
        .await
        .unwrap();
    let staff = w
        .svc
        .admin_list(
            w.publisher,
            BrowseQuery {
                age: AgeFilter::Include,
                published: Some(false),
                ..w.browse()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        staff.items.iter().map(|s| s.id).collect::<Vec<_>>(),
        [hidden.id]
    );
    let t = w
        .svc
        .admin_list(
            w.publisher,
            BrowseQuery {
                age: AgeFilter::Include,
                title_contains: Some("brav%".into()),
                ..w.browse()
            },
        )
        .await
        .unwrap();
    // `%` is literal, not a wildcard.
    assert_eq!(t.total, 0);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn latest_updates_feed_paginates_and_filters_to_followed() {
    let w = world().await;
    let (s1, e1, _) = w.published("Feed one", false, 1.0).await;
    let (s2, _, _) = w.published("Feed two", false, 1.0).await;
    let c = w.chapter(s1, e1, 2.0).await;
    w.svc
        .follow(w.member, s1, Shelf::PlanToRead, false)
        .await
        .unwrap();

    let global = w.svc.updates(w.member, false, None, 100).await.unwrap();
    let mine: Vec<_> = global
        .items
        .iter()
        .filter(|u| u.series_id == s1 || u.series_id == s2)
        .collect();
    assert_eq!(mine.len(), 3);
    // Newest first.
    assert_eq!(mine[0].chapter_id, c);
    assert!(
        mine.windows(2)
            .all(|p| p[0].published_at_ms >= p[1].published_at_ms)
    );
    assert_eq!(mine[0].language, "en");
    assert_eq!(mine[0].release_group.as_deref(), Some("Group A"));

    let followed = w.svc.updates(w.member, true, None, 100).await.unwrap();
    assert!(followed.items.iter().all(|u| u.series_id == s1));
    assert_eq!(followed.items.len(), 2);

    // Cursor pagination walks the whole followed feed without repeats.
    let p1 = w.svc.updates(w.member, true, None, 1).await.unwrap();
    assert_eq!(p1.items.len(), 1);
    let cursor = p1.next_before.clone().unwrap();
    let p2 = w
        .svc
        .updates(w.member, true, Some(&cursor), 1)
        .await
        .unwrap();
    assert_eq!(p2.items.len(), 1);
    assert_ne!(p1.items[0].chapter_id, p2.items[0].chapter_id);
    assert!(p2.next_before.is_none());
    assert!(matches!(
        w.svc.updates(w.member, true, Some("garbage"), 1).await,
        Err(LibraryError::Invalid(_))
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn follows_shelves_and_counts() {
    let w = world().await;
    let (s, _, _) = w.published("Follow me", false, 1.0).await;
    let f = w
        .svc
        .follow(w.member, s, Shelf::Reading, true)
        .await
        .unwrap();
    assert_eq!(f.shelf, Shelf::Reading);
    w.svc
        .follow(w.adult, s, Shelf::Dropped, false)
        .await
        .unwrap();
    assert_eq!(
        w.svc
            .series_detail(w.member, s)
            .await
            .unwrap()
            .series
            .follower_count,
        2
    );
    // Re-following changes the shelf, not the count.
    let f = w
        .svc
        .follow(w.member, s, Shelf::OnHold, false)
        .await
        .unwrap();
    assert_eq!((f.shelf, f.notify), (Shelf::OnHold, false));
    let d = w.svc.series_detail(w.member, s).await.unwrap();
    assert_eq!(d.series.follower_count, 2);
    assert_eq!(d.follow.unwrap().shelf, Shelf::OnHold);
    let shelf = w
        .svc
        .my_follows(w.member, Some(Shelf::OnHold), None, None)
        .await
        .unwrap();
    assert_eq!(shelf.total, 1);
    assert!(
        w.svc
            .my_follows(w.member, Some(Shelf::Reading), None, None)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert!(w.svc.unfollow(w.member, s).await.unwrap());
    assert!(!w.svc.unfollow(w.member, s).await.unwrap());
    assert_eq!(
        w.svc
            .series_detail(w.adult, s)
            .await
            .unwrap()
            .series
            .follower_count,
        1
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn instant_search_tracks_publishing_and_edits() {
    let w = world().await;
    let mut inp = w.input("Searchable Saga");
    inp.authors = vec![format!("Zed{}", w.genre)];
    inp.tags = vec![format!("slowburn{}", w.genre)];
    let s = w.svc.create_series(w.publisher, inp.clone()).await.unwrap();
    // Unpublished series are not indexed.
    assert!(
        w.svc
            .search(w.member, "searchable", 5)
            .await
            .unwrap()
            .series
            .is_empty()
    );
    w.svc
        .set_series_published(w.publisher, s.id, true)
        .await
        .unwrap();
    let r = w.svc.search(w.member, "searcha sa", 5).await.unwrap();
    assert_eq!(r.series.iter().map(|x| x.id).collect::<Vec<_>>(), [s.id]);
    let r = w
        .svc
        .search(w.member, &format!("zed{}", w.genre), 5)
        .await
        .unwrap();
    assert_eq!(r.authors, [format!("Zed{}", w.genre)]);
    let r = w.svc.search(w.member, "slowburn", 5).await.unwrap();
    assert!(r.tags.contains(&format!("slowburn{}", w.genre)));
    // Edits are reindexed; unpublishing and deleting drop the series.
    inp.title = format!("Renamed Epic {}", w.genre);
    w.svc.update_series(w.publisher, s.id, inp).await.unwrap();
    assert!(
        w.svc
            .search(w.member, "searchable", 5)
            .await
            .unwrap()
            .series
            .is_empty()
    );
    assert_eq!(
        w.svc
            .search(w.member, "renamed epic", 5)
            .await
            .unwrap()
            .series
            .len(),
        1
    );
    w.svc
        .set_series_published(w.publisher, s.id, false)
        .await
        .unwrap();
    assert!(
        w.svc
            .search(w.member, "renamed", 5)
            .await
            .unwrap()
            .series
            .is_empty()
    );
    // A full rebuild from the database restores exactly the published set.
    w.svc
        .set_series_published(w.publisher, s.id, true)
        .await
        .unwrap();
    assert!(w.svc.reindex_all().await.unwrap() >= 1);
    assert_eq!(
        w.svc
            .search(w.member, "renamed epic", 5)
            .await
            .unwrap()
            .series
            .len(),
        1
    );
    w.svc.delete_series(w.publisher, s.id).await.unwrap();
    assert!(
        w.svc
            .search(w.member, "renamed", 5)
            .await
            .unwrap()
            .series
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn merge_moves_everything_and_redirects() {
    let w = world().await;
    let (src, src_e, src_c) = w.published("Source", false, 1.0).await;
    let (dst, _, _) = w.published("Target", false, 5.0).await;
    w.svc
        .follow(w.member, src, Shelf::Reading, true)
        .await
        .unwrap();
    w.svc
        .follow(w.member, dst, Shelf::Completed, true)
        .await
        .unwrap();
    w.svc
        .follow(w.adult, src, Shelf::Reading, true)
        .await
        .unwrap();

    // Merging needs both permissions and two distinct series.
    let curator = Actor {
        user_id: w.publisher.user_id,
        permissions: P::PUBLISH_READING,
    };
    assert!(matches!(
        w.svc.merge_series(curator, src, dst).await,
        Err(LibraryError::MissingPermission(_))
    ));
    assert!(matches!(
        w.svc.merge_series(w.publisher, src, src).await,
        Err(LibraryError::Invalid(_))
    ));

    let merged = w.svc.merge_series(w.publisher, src, dst).await.unwrap();
    assert_eq!(merged.id, dst);
    assert_eq!(merged.chapter_count, 2);
    // The edition label collided and was disambiguated; exactly one default remains.
    let d = w.svc.series_detail(w.member, dst).await.unwrap();
    assert_eq!(d.editions.len(), 2);
    assert_eq!(d.editions.iter().filter(|e| e.is_default).count(), 1);
    assert!(d.editions.iter().any(|e| e.id == src_e));
    // Follows were unioned (the member keeps their own shelf on the target).
    assert_eq!(d.series.follower_count, 2);
    assert_eq!(d.follow.unwrap().shelf, Shelf::Completed);
    // The old id redirects; its chapter now belongs to the target.
    let via_old = w.svc.series_detail(w.member, src).await.unwrap();
    assert_eq!(via_old.series.id, dst);
    let m = w.svc.page_manifest(w.member, src_c).await.unwrap();
    assert_eq!(m.chapter.series_id, dst);
    // Merged series are out of browse and cannot be merged again or published.
    let page = w.svc.browse(w.member, w.browse()).await.unwrap();
    assert_eq!(page.items.iter().map(|s| s.id).collect::<Vec<_>>(), [dst]);
    assert!(matches!(
        w.svc.merge_series(w.publisher, src, dst).await,
        Err(LibraryError::Conflict(_))
    ));
    assert!(not_found(
        w.svc
            .set_series_published(w.publisher, src, true)
            .await
            .unwrap_err()
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn editions_volumes_and_chapter_constraints() {
    let w = world().await;
    let s = w
        .svc
        .create_series(w.publisher, w.input("Structure"))
        .await
        .unwrap();
    let mk = |lang: &str, label: &str, d: bool| EditionInput {
        language: lang.into(),
        label: label.into(),
        is_default: d,
    };
    let e1 = w
        .svc
        .create_edition(w.publisher, s.id, mk("en", "One", false))
        .await
        .unwrap();
    let e2 = w
        .svc
        .create_edition(w.publisher, s.id, mk("ja", "Raw", true))
        .await
        .unwrap();
    let eds = w
        .svc
        .admin_series(w.publisher, s.id)
        .await
        .unwrap()
        .editions;
    assert_eq!(
        eds.iter()
            .filter(|e| e.is_default)
            .map(|e| e.id)
            .collect::<Vec<_>>(),
        [e2.id]
    );
    // Promoting another edition demotes the old default.
    w.svc
        .update_edition(w.publisher, e1.id, mk("en", "One", true))
        .await
        .unwrap();
    let eds = w
        .svc
        .admin_series(w.publisher, s.id)
        .await
        .unwrap()
        .editions;
    assert_eq!(
        eds.iter()
            .filter(|e| e.is_default)
            .map(|e| e.id)
            .collect::<Vec<_>>(),
        [e1.id]
    );
    // Deleting the default promotes the remaining edition.
    w.svc.delete_edition(w.publisher, e1.id).await.unwrap();
    let eds = w
        .svc
        .admin_series(w.publisher, s.id)
        .await
        .unwrap()
        .editions;
    assert_eq!((eds.len(), eds[0].id, eds[0].is_default), (1, e2.id, true));

    let v = w
        .svc
        .create_volume(
            w.publisher,
            e2.id,
            VolumeInput {
                number: 1.0,
                title: Some("Vol 1".into()),
                cover_key: None,
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        w.svc
            .create_volume(
                w.publisher,
                e2.id,
                VolumeInput {
                    number: 1.0,
                    title: None,
                    cover_key: None
                }
            )
            .await,
        Err(LibraryError::Conflict(_))
    ));
    let input = |n: f64, vol: Option<i64>| ChapterInput {
        number: n,
        title: None,
        volume_id: vol,
        format: ChapterFormat::Images,
        source_key: None,
        release_group: None,
    };
    let c = w
        .svc
        .create_chapter(w.publisher, e2.id, input(10.5, Some(v.id)))
        .await
        .unwrap();
    assert_eq!((c.number, c.volume_id), (10.5, Some(v.id)));
    assert!(matches!(
        w.svc
            .create_chapter(w.publisher, e2.id, input(10.5, None))
            .await,
        Err(LibraryError::Conflict(_))
    ));
    // A volume of another edition is refused.
    let e3 = w
        .svc
        .create_edition(w.publisher, s.id, mk("fr", "Fan", false))
        .await
        .unwrap();
    assert!(matches!(
        w.svc
            .create_chapter(w.publisher, e3.id, input(1.0, Some(v.id)))
            .await,
        Err(LibraryError::Invalid(_))
    ));
    // Unknown parents.
    assert!(not_found(
        w.svc
            .create_chapter(w.publisher, 1, input(1.0, None))
            .await
            .unwrap_err()
    ));
    assert!(not_found(
        w.svc
            .create_edition(w.publisher, 1, mk("en", "x", false))
            .await
            .unwrap_err()
    ));
    let list = w
        .svc
        .admin_chapters(w.publisher, s.id, ChapterQuery::default())
        .await
        .unwrap();
    assert_eq!(
        (list.chapters.len(), list.volumes.len(), list.editions.len()),
        (1, 1, 2)
    );
    // Deleting a chapter and a volume.
    w.svc
        .update_chapter(w.publisher, c.id, input(11.0, None))
        .await
        .unwrap();
    w.svc.delete_chapter(w.publisher, c.id).await.unwrap();
    w.svc.delete_volume(w.publisher, v.id).await.unwrap();
    assert!(not_found(
        w.svc.delete_chapter(w.publisher, c.id).await.unwrap_err()
    ));
}
