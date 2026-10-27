//! Postgres tests for the shelves (BYOS-CAT-002): who sees which mount's rows, paging and filters, the editor's
//! overrides and that a re-index keeps them.
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test shelf_repo -- --include-ignored --test-threads=1`

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use async_trait::async_trait;
use sr_db::index_repo::PgIndexStore;
use sr_db::shelf_repo::PgShelfStore;
use sr_db::testing::TestDb;
use sr_storage::index::shelf::{
    PartPatch, Section, ShelfFilter, ShelfPatch, ShelfStore, drop_delisted, reachable,
};
use sr_storage::index::{
    IndexStore, NameParser, Options, RunReport, SidecarMap, VideoFacts, classify_all,
};
use sr_storage::mounts::{
    Mount, MountOwner, MountResolver, MountResult, MountStatus, Provider, SealedSecret,
};
use sr_storage::scopes::{MemoryScopeFacts, Share, Viewer};

struct Fake;
impl NameParser for Fake {
    fn video(&self, path: &str) -> VideoFacts {
        let file = path.rsplit('/').next().unwrap_or(path);
        let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
        let mut f = VideoFacts {
            title: stem.to_owned(),
            ..VideoFacts::default()
        };
        if let Some(p) = stem.find(" S01E") {
            f.episode = true;
            f.title = stem[..p].to_owned();
            f.season = Some(1);
            f.episodes = vec![stem[p + 5..p + 7].parse().unwrap_or(1)];
        }
        f
    }
}

struct Mounts(HashMap<i64, Mount>);
#[async_trait]
impl MountResolver for Mounts {
    async fn by_id(&self, id: i64) -> MountResult<Option<Mount>> {
        Ok(self.0.get(&id).cloned())
    }
    async fn for_owner(&self, _o: MountOwner) -> MountResult<Vec<Mount>> {
        Ok(Vec::new())
    }
}

fn mount(id: i64, owner: MountOwner) -> Mount {
    Mount {
        id,
        owner,
        label: format!("m{id}"),
        provider: Provider::Garage,
        endpoint: "http://localhost:3900".into(),
        region: "garage".into(),
        bucket: "b".into(),
        prefix: String::new(),
        path_style: true,
        access_key_id: "AK".into(),
        secret_key_enc: SealedSecret::from_bytes(vec![1]),
        key_hint: "AK".into(),
        status: MountStatus::Active,
        cors_ok_at: Some(1),
        last_test_at: None,
        last_test: None,
        created_at: 0,
        updated_at: 0,
    }
}

static NEXT: AtomicI64 = AtomicI64::new(700_000);

fn ids() -> sr_db::index_repo::IdSource {
    Arc::new(|| NEXT.fetch_add(1, Ordering::SeqCst))
}

async fn index(db: &TestDb, mount: i64, keys: &[&str]) {
    let s = PgIndexStore::new(db.pool().clone(), ids());
    let r = s.begin(mount, 0).await.unwrap();
    let listing: Vec<(String, u64)> = keys.iter().map(|k| ((*k).to_owned(), 100)).collect();
    let all: Vec<_> = classify_all(&listing, &Fake, &SidecarMap::new(), Options::default())
        .into_iter()
        .filter(|c| !c.ignored && c.class.kind().is_some())
        .collect();
    s.apply(mount, r, &all).await.unwrap();
    s.finish(mount, r, &RunReport::default(), true, None, 0)
        .await
        .unwrap();
}

const ALICE: i64 = 1;
const BOB: i64 = 2;
const CARA: i64 = 3;

const TREE: &[&str] = &[
    "Movies/Alien.mp4",
    "Movies/Blade Runner.mkv",
    "Series/Show/Show S01E01.mp4",
    "Series/Show/Show S01E02.mp4",
    "Manga/Berserk/Vol 01.cbz",
    "Manga/Berserk/Vol 02.cbz",
    "Books/Dune.epub",
    "Music/Daft Punk/Discovery/01 - One More Time.flac",
    "Music/Daft Punk/Discovery/02 - Aerodynamic.flac",
];

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn each_viewer_sees_only_the_mounts_scopes_admit() {
    let db = TestDb::new().await.expect("db");
    // Alice owns mount 10; Bob owns mount 20. Alice is friends with Bob and shares only music with him; Cara is
    // nobody's friend.
    index(&db, 10, TREE).await;
    index(&db, 20, &["Movies/Bobs Film.mp4"]).await;
    let store = PgShelfStore::new(db.pool().clone(), ids());
    let mounts = Mounts(HashMap::from([
        (10, mount(10, MountOwner::User(ALICE))),
        (20, mount(20, MountOwner::User(BOB))),
    ]));
    let facts = MemoryScopeFacts::new();
    facts.befriend(ALICE, BOB);
    facts.share_with(
        10,
        BOB,
        Share {
            music: true,
            ..Share::default()
        },
    );
    let who = |id| Viewer { id, staff: false };
    let ids_of = |r: Vec<sr_storage::index::shelf::Reach>| {
        let mut v: Vec<i64> = r.iter().map(|x| x.mount.id).collect();
        v.sort_unstable();
        v
    };
    for (viewer, section, want) in [
        (ALICE, Section::Cinema, vec![10]),
        (ALICE, Section::Music, vec![10]),
        (BOB, Section::Cinema, vec![20]),
        (BOB, Section::Library, vec![]),
        (BOB, Section::Music, vec![10]),
        (CARA, Section::Cinema, vec![]),
        (CARA, Section::Music, vec![]),
    ] {
        let r = reachable(&facts, &mounts, &store, who(viewer), section)
            .await
            .unwrap();
        assert_eq!(ids_of(r), want, "viewer {viewer} section {section:?}");
    }

    // Alice's cinema page: 2 films and the series; kind and text filters; the mount filter.
    let reach = ids_of(
        reachable(&facts, &mounts, &store, who(ALICE), Section::Cinema)
            .await
            .unwrap(),
    );
    let page = store
        .list(
            Section::Cinema,
            &reach,
            &ShelfFilter {
                limit: 50,
                ..ShelfFilter::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page.total, 3);
    let titles: Vec<_> = page.items.iter().map(|i| i.title.as_str()).collect();
    assert_eq!(titles, ["Alien", "Blade Runner", "Show"]);
    let alien = &page.items[0];
    assert!(
        alien.web_native
            && alien.container == "mp4"
            && alien.object_key.as_deref() == Some("Movies/Alien.mp4")
    );
    assert!(!page.items[1].web_native, "mkv is not web native");
    assert_eq!(page.items[2].parts, 2);
    let series_only = store
        .list(
            Section::Cinema,
            &reach,
            &ShelfFilter {
                kind: Some("series".into()),
                limit: 50,
                ..ShelfFilter::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(series_only.total, 1);
    let q = store
        .list(
            Section::Cinema,
            &reach,
            &ShelfFilter {
                q: Some("blade".into()),
                limit: 50,
                ..ShelfFilter::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(q.items.len(), 1);
    // Paging.
    let p1 = store
        .list(
            Section::Cinema,
            &reach,
            &ShelfFilter {
                limit: 2,
                ..ShelfFilter::default()
            },
        )
        .await
        .unwrap();
    let p2 = store
        .list(
            Section::Cinema,
            &reach,
            &ShelfFilter {
                limit: 2,
                offset: 2,
                ..ShelfFilter::default()
            },
        )
        .await
        .unwrap();
    assert_eq!((p1.items.len(), p2.items.len(), p1.total), (2, 1, 3));

    // A series opens with its episodes, but only from a reachable mount.
    let show = page.items[2].id;
    let d = store
        .get(Section::Cinema, show, &reach)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(d.parts.len(), 2);
    assert_eq!(d.parts[1].object_key, "Series/Show/Show S01E02.mp4");
    assert!(
        store
            .get(Section::Cinema, show, &[20])
            .await
            .unwrap()
            .is_none(),
        "another mount's id"
    );
    assert!(
        store
            .get(Section::Cinema, show, &[])
            .await
            .unwrap()
            .is_none()
    );

    // Library and music.
    let lib = store
        .list(
            Section::Library,
            &[10],
            &ShelfFilter {
                limit: 50,
                ..ShelfFilter::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(lib.total, 2);
    let dune = lib.items.iter().find(|i| i.title == "Dune").unwrap();
    assert_eq!(
        dune.object_key.as_deref(),
        Some("Books/Dune.epub"),
        "a single book carries its file"
    );
    let berserk = lib.items.iter().find(|i| i.title == "Berserk").unwrap();
    assert_eq!((berserk.parts, berserk.object_key.clone()), (2, None));
    let tracks = store
        .list(
            Section::Music,
            &[10],
            &ShelfFilter {
                q: Some("daft".into()),
                limit: 50,
                ..ShelfFilter::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(tracks.total, 2);
    assert_eq!(tracks.items[0].subtitle, "Daft Punk - Discovery");

    // Delisting drops the item from a page.
    facts.delist(10, "Movies/Alien.mp4");
    let kept = drop_delisted(&facts, &store, Section::Cinema, page.items.clone())
        .await
        .unwrap();
    assert_eq!(kept.len(), 2);
    assert!(kept.iter().all(|i| i.title != "Alien"));

    // A disabled mount, or a store failure, shows nothing.
    let mut off = mount(10, MountOwner::User(ALICE));
    off.status = MountStatus::Disabled;
    let mounts_off = Mounts(HashMap::from([(10, off)]));
    assert!(
        reachable(&facts, &mounts_off, &store, who(ALICE), Section::Cinema)
            .await
            .unwrap()
            .is_empty()
    );
    facts.break_store();
    assert!(
        reachable(&facts, &mounts, &store, who(ALICE), Section::Cinema)
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn edits_are_overrides_that_a_reindex_keeps() {
    let db = TestDb::new().await.expect("db");
    index(&db, 10, TREE).await;
    let store = PgShelfStore::new(db.pool().clone(), ids());
    let page = store
        .list(
            Section::Cinema,
            &[10],
            &ShelfFilter {
                limit: 50,
                ..ShelfFilter::default()
            },
        )
        .await
        .unwrap();
    let alien = page.items.iter().find(|i| i.title == "Alien").unwrap().id;
    let show = page.items.iter().find(|i| i.title == "Show").unwrap().id;

    store
        .patch(
            Section::Cinema,
            alien,
            &ShelfPatch {
                title: Some("Alien (Director's Cut)".into()),
                year: Some(Some(1979)),
                tags: Some(vec!["scifi".into()]),
                synopsis: Some("In space.".into()),
                external_ids: Some(r#"{"tmdb":"348"}"#.into()),
                attribution: Some(
                    "This product uses the TMDB API but is not endorsed or certified by TMDB."
                        .into(),
                ),
                ..ShelfPatch::default()
            },
        )
        .await
        .unwrap();
    let d = store
        .get(Section::Cinema, alien, &[10])
        .await
        .unwrap()
        .unwrap()
        .item;
    assert_eq!(
        (d.title.as_str(), d.year),
        ("Alien (Director's Cut)", Some(1979))
    );
    assert_eq!(d.tags, ["scifi"]);
    assert!(d.attribution.as_deref().unwrap().contains("TMDB"));
    for f in ["title", "year", "tags", "synopsis", "details"] {
        assert!(
            d.overridden.iter().any(|x| x == f),
            "{f} in {:?}",
            d.overridden
        );
    }

    // An episode: retitle, renumber, move to season 2; the same number twice is refused.
    let detail = store
        .get(Section::Cinema, show, &[10])
        .await
        .unwrap()
        .unwrap();
    let e1 = detail.parts[0].id;
    store
        .patch_part(
            Section::Cinema,
            show,
            e1,
            10,
            &PartPatch {
                title: Some("Pilot".into()),
                number: Some(5.0),
                season: Some(2),
            },
        )
        .await
        .unwrap();
    let again = store
        .get(Section::Cinema, show, &[10])
        .await
        .unwrap()
        .unwrap();
    let moved = again.parts.iter().find(|p| p.id == e1).unwrap();
    assert_eq!(
        (moved.title.as_str(), moved.season, moved.number as i32),
        ("Pilot", Some(2), 5)
    );
    let e2 = again.parts.iter().find(|p| p.id != e1).unwrap().id;
    assert!(
        store
            .patch_part(
                Section::Cinema,
                show,
                e2,
                10,
                &PartPatch {
                    number: Some(5.0),
                    season: Some(2),
                    ..PartPatch::default()
                }
            )
            .await
            .is_err()
    );
    // A part is only editable through its own mount.
    assert!(
        store
            .patch_part(
                Section::Cinema,
                show,
                e2,
                99,
                &PartPatch {
                    title: Some("x".into()),
                    ..PartPatch::default()
                }
            )
            .await
            .is_err()
    );

    // A chapter and a track.
    let lib = store
        .list(
            Section::Library,
            &[10],
            &ShelfFilter {
                limit: 50,
                ..ShelfFilter::default()
            },
        )
        .await
        .unwrap();
    let berserk = lib.items.iter().find(|i| i.title == "Berserk").unwrap().id;
    store
        .patch(
            Section::Library,
            berserk,
            &ShelfPatch {
                kind: Some("comic".into()),
                title: Some("Berserk Deluxe".into()),
                ..ShelfPatch::default()
            },
        )
        .await
        .unwrap();
    let ch = store
        .get(Section::Library, berserk, &[10])
        .await
        .unwrap()
        .unwrap();
    store
        .patch_part(
            Section::Library,
            berserk,
            ch.parts[0].id,
            10,
            &PartPatch {
                number: Some(9.0),
                ..PartPatch::default()
            },
        )
        .await
        .unwrap();
    let trk = store
        .list(
            Section::Music,
            &[10],
            &ShelfFilter {
                limit: 1,
                ..ShelfFilter::default()
            },
        )
        .await
        .unwrap()
        .items[0]
        .id;
    store
        .patch(
            Section::Music,
            trk,
            &ShelfPatch {
                artist: Some("Daft Punk!".into()),
                track_no: Some(Some(9)),
                ..ShelfPatch::default()
            },
        )
        .await
        .unwrap();

    // Re-index: nothing the member edited is rewritten; everything else is.
    index(&db, 10, TREE).await;
    let a = store
        .get(Section::Cinema, alien, &[10])
        .await
        .unwrap()
        .unwrap()
        .item;
    assert_eq!(a.title, "Alien (Director's Cut)");
    assert_eq!(a.year, Some(1979));
    let s = store
        .get(Section::Cinema, show, &[10])
        .await
        .unwrap()
        .unwrap();
    let e = s.parts.iter().find(|p| p.id == e1).unwrap();
    assert_eq!((e.title.as_str(), e.number as i32), ("Pilot", 5));
    let b = store
        .get(Section::Library, berserk, &[10])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(b.item.title, "Berserk Deluxe");
    assert_eq!(b.item.kind, "comic");
    assert!(b.parts.iter().any(|p| p.number as i32 == 9));
    let t = store
        .list(
            Section::Music,
            &[10],
            &ShelfFilter {
                q: Some("Daft Punk!".into()),
                limit: 5,
                ..ShelfFilter::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        t.total, 1,
        "the edited artist survives; the other track is rewritten to the folder's artist"
    );
    assert!(
        store
            .patch(
                Section::Cinema,
                424_242,
                &ShelfPatch {
                    title: Some("x".into()),
                    ..ShelfPatch::default()
                }
            )
            .await
            .is_err()
    );
}
