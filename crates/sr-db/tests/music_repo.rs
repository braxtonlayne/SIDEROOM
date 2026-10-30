//! Postgres tests for the music library (MUS-IDX-001): audio indexed under any prefix, the scoped listing, the
//! tag writes (by the owner, by a server role with `upload`, refused for a friend and for staff), the tagger's
//! writes that never replace a hand edit, and "Rescan tags".
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test music_repo -- --include-ignored --test-threads=1`

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use sr_db::index_repo::PgIndexStore;
use sr_db::music_repo::PgMusicStore;
use sr_db::testing::TestDb;
use sr_storage::index::audio::{
    LibraryFilter, MusicStore, TagSource, TagWrite, TagsState, cover_key_for,
};
use sr_storage::index::{
    IndexStore, NameParser, Options, RunReport, SidecarMap, VideoFacts, classify_all,
};
use sr_storage::mounts::{Mount, MountOwner, MountStatus, Provider, SealedSecret};
use sr_storage::scopes::{
    GrantSet, GuildStanding, MemoryScopeFacts, ResolveError, Share, Viewer, access, may_upload,
};

struct Fake;
impl NameParser for Fake {
    fn video(&self, path: &str) -> VideoFacts {
        VideoFacts {
            title: path.to_owned(),
            ..VideoFacts::default()
        }
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

static NEXT: AtomicI64 = AtomicI64::new(900_000);

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

const ALICE: i64 = 1; // owns mount 10
const BOB: i64 = 2; // Alice's friend, music shared
const CARA: i64 = 3; // nobody; also stands in for platform staff (never reaches a member's bucket)
const GUILD: i64 = 500;
const UPLOADER: i64 = 4; // server member with a role that has `upload`
const LISTENER: i64 = 5; // server member with `view_music` only
const UPLOAD_ROLE: i64 = 71;
const VIEW_ROLE: i64 = 72;

const TREE: &[&str] = &[
    "Music/Daft Punk/Discovery/01 - One More Time.flac",
    "Stuff/Mixes/Late Set.mp3",
    "loose.ogg",
    "Movies/Alien.mp4",
];

#[derive(Debug, PartialEq, Eq)]
enum Decision {
    NotFound,
    Forbidden,
    Allowed,
}

/// The route's rule: a viewer who does not reach the mount for music gets 404, one who reaches it but may not
/// upload gets 403 (`sr_api::routes::music::tracks::patch_track`). The viewer is never staff.
async fn decide(facts: &MemoryScopeFacts, m: &Mount, who: i64) -> Decision {
    let v = Viewer {
        id: who,
        staff: false,
    };
    match access(facts, v, m).await {
        Ok(a) if a.unrestricted || a.kinds.music => {
            if may_upload(facts, v, m).await.unwrap() {
                Decision::Allowed
            } else {
                Decision::Forbidden
            }
        }
        Ok(_) | Err(ResolveError::Refused(_)) => Decision::NotFound,
        Err(ResolveError::Facts(e)) => panic!("{e}"),
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn audio_under_any_prefix_is_listed_with_its_container_facts() {
    let db = TestDb::new().await.expect("db");
    index(&db, 10, TREE).await;
    let store = PgMusicStore::new(db.pool().clone());
    assert_eq!(store.mounts().await.unwrap(), vec![10]);
    let page = store
        .list(
            &[10],
            &LibraryFilter {
                limit: 50,
                ..LibraryFilter::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page.total, 3, "the film is not a track");
    let keys: Vec<_> = page.items.iter().map(|t| t.key.as_str()).collect();
    for k in ["Stuff/Mixes/Late Set.mp3", "loose.ogg"] {
        assert!(keys.contains(&k), "{keys:?}");
    }
    let flac = page
        .items
        .iter()
        .find(|t| t.key.ends_with(".flac"))
        .unwrap();
    assert_eq!(
        (flac.container.as_str(), flac.size, flac.tags_state.as_str()),
        ("flac", 100, "pending")
    );
    assert!(flac.web_native);
    // Filters: text, exact album, pending only; and a mount the caller does not reach lists nothing.
    let by = |f: LibraryFilter| {
        let store = &store;
        async move {
            store
                .list(&[10], &LibraryFilter { limit: 50, ..f })
                .await
                .unwrap()
                .total
        }
    };
    assert_eq!(
        by(LibraryFilter {
            q: Some("late".into()),
            ..LibraryFilter::default()
        })
        .await,
        1
    );
    assert_eq!(
        by(LibraryFilter {
            q: Some("100%".into()),
            ..LibraryFilter::default()
        })
        .await,
        0,
        "wildcards are literal"
    );
    assert_eq!(
        by(LibraryFilter {
            album: Some("discovery".into()),
            ..LibraryFilter::default()
        })
        .await,
        1
    );
    assert_eq!(
        by(LibraryFilter {
            pending: true,
            ..LibraryFilter::default()
        })
        .await,
        3
    );
    assert!(
        store
            .list(&[], &LibraryFilter::default())
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert!(store.get(page.items[0].id, &[99]).await.unwrap().is_none());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn who_may_write_tags_owner_and_upload_role_yes_friend_and_staff_no() {
    let owner_mount = mount(10, MountOwner::User(ALICE));
    let guild_mount = mount(20, MountOwner::Guild(GUILD));
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
    let member = |roles: Vec<i64>| GuildStanding {
        role_ids: roles,
        ..GuildStanding::default()
    };
    facts.member(GUILD, UPLOADER, member(vec![UPLOAD_ROLE]));
    facts.member(GUILD, LISTENER, member(vec![VIEW_ROLE]));
    facts.grant(
        GUILD,
        UPLOAD_ROLE,
        GrantSet::of([
            sr_storage::scopes::Grant::ViewMusic,
            sr_storage::scopes::Grant::Upload,
        ]),
    );
    facts.grant(
        GUILD,
        VIEW_ROLE,
        GrantSet::of([sr_storage::scopes::Grant::ViewMusic]),
    );
    assert_eq!(decide(&facts, &owner_mount, ALICE).await, Decision::Allowed);
    assert_eq!(
        decide(&facts, &owner_mount, BOB).await,
        Decision::Forbidden,
        "a friend who can listen cannot write tags"
    );
    assert_eq!(
        decide(&facts, &owner_mount, CARA).await,
        Decision::NotFound,
        "staff and strangers do not reach a member's bucket (the route never resolves staff)"
    );
    assert_eq!(
        decide(&facts, &guild_mount, UPLOADER).await,
        Decision::Allowed
    );
    assert_eq!(
        decide(&facts, &guild_mount, LISTENER).await,
        Decision::Forbidden
    );
    assert_eq!(decide(&facts, &guild_mount, CARA).await, Decision::NotFound);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_tagger_writes_tags_and_never_replaces_a_hand_edit() {
    let db = TestDb::new().await.expect("db");
    index(&db, 10, TREE).await;
    let store = PgMusicStore::new(db.pool().clone());
    let id = store
        .list(
            &[10],
            &LibraryFilter {
                q: Some("late".into()),
                limit: 5,
                ..LibraryFilter::default()
            },
        )
        .await
        .unwrap()
        .items[0]
        .id;

    // The tagger reads the file.
    let from_file = TagWrite {
        title: Some("Late Set (file)".into()),
        artist: Some("DJ File".into()),
        album: Some("Mixes".into()),
        track_no: Some(Some(2)),
        year: Some(Some(2001)),
        duration_ms: Some(Some(181_000)),
        genres: Some(vec!["House".into()]),
        cover: Some(Some((cover_key_for("0123abcd"), "0123abcd".into()))),
        state: Some(TagsState::Read),
        ..TagWrite::default()
    };
    store.write(id, &from_file, TagSource::File).await.unwrap();
    let t = store.get(id, &[10]).await.unwrap().unwrap();
    assert_eq!(
        (t.title.as_str(), t.artist.as_str(), t.album.as_str()),
        ("Late Set (file)", "DJ File", "Mixes")
    );
    assert_eq!(
        (t.track_no, t.year, t.duration_ms),
        (Some(2), Some(2001), Some(181_000))
    );
    assert_eq!(t.genres, ["House"]);
    assert_eq!(t.cover_key.as_deref(), Some("covers/0123abcd.jpg"));
    assert_eq!(t.tags_state, "read");
    assert!(t.tags_read_ms.is_some());
    assert!(
        t.overridden.contains(&"title".to_owned()),
        "a re-index keeps file tags"
    );

    // A member edits the title by hand.
    let edit = TagWrite {
        title: Some("My name for it".into()),
        ..TagWrite::default()
    };
    store.write(id, &edit, TagSource::Edit).await.unwrap();

    // A rescan: the tagger writes again. The artist (from the file) changes, the title (hand edit) does not.
    assert_eq!(
        store.rescan(10).await.unwrap(),
        1,
        "only the track that was read goes back to pending"
    );
    let again = TagWrite {
        title: Some("Late Set (file, again)".into()),
        artist: Some("DJ File 2".into()),
        state: Some(TagsState::Read),
        ..TagWrite::default()
    };
    store.write(id, &again, TagSource::File).await.unwrap();
    let t = store.get(id, &[10]).await.unwrap().unwrap();
    assert_eq!(t.title, "My name for it");
    assert_eq!(t.artist, "DJ File 2");
    assert_eq!(t.tags_state, "read");

    // A re-index (file names only) leaves both alone.
    index(&db, 10, TREE).await;
    let t = store.get(id, &[10]).await.unwrap().unwrap();
    assert_eq!(
        (t.title.as_str(), t.artist.as_str()),
        ("My name for it", "DJ File 2")
    );

    // Failed tags are retried by the next pending listing; a missing row is reported.
    store
        .write(
            id,
            &TagWrite {
                state: Some(TagsState::Failed),
                ..TagWrite::default()
            },
            TagSource::File,
        )
        .await
        .unwrap();
    let pending = store
        .list(
            &[10],
            &LibraryFilter {
                pending: true,
                limit: 50,
                ..LibraryFilter::default()
            },
        )
        .await
        .unwrap();
    assert!(pending.items.iter().any(|p| p.id == id));
    assert!(matches!(
        store
            .write(
                1,
                &TagWrite {
                    title: Some("x".into()),
                    ..TagWrite::default()
                },
                TagSource::Edit
            )
            .await,
        Err(sr_storage::index::shelf::PatchError::NotFound)
    ));
}
