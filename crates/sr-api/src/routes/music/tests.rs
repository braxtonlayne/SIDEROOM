//! The rules of the music library without a database: ownership, the server grant, the caps and the reach
//! filter, over the in-memory store and a fake of the storage scopes.
//! (The same rules over Postgres: `crates/sr-db/tests/music_lib_repo.rs`, `--include-ignored`.)

use super::*;
use crate::routes::music::playlists::{
    AddItemsBody, PlaylistBody, PlaylistPatchBody, svc_add_items, svc_create, svc_detail, svc_edit,
    svc_list_guild, svc_list_own, svc_move_item, svc_remove, svc_remove_item,
};
use sr_storage::music_lib::{MemoryMusicLib, PlaylistOwner};
use std::sync::{
    Mutex,
    atomic::{AtomicI64, Ordering},
};

const ALICE: i64 = 1;
const BOB: i64 = 2;
const CAROL: i64 = 3;
const GUILD: i64 = 50;

/// Alice reads mounts 10 and 11; Bob reads 10; Carol reads nothing. In the server: Alice may edit, Bob only
/// views, Carol is not a member.
#[derive(Default)]
struct FakeReach {
    mounts: Mutex<Vec<(i64, i64)>>,
    guild: Mutex<Vec<(i64, i64, bool)>>,
}

#[async_trait]
impl MusicReach for FakeReach {
    async fn readable_mounts(&self, user: i64) -> Result<HashSet<i64>, ReachError> {
        let g = self.mounts.lock().unwrap();
        Ok(g.iter()
            .filter(|(u, _)| *u == user)
            .map(|(_, m)| *m)
            .collect())
    }
    async fn guild_rights(&self, user: i64, guild: i64) -> Result<Option<GuildRights>, ReachError> {
        let g = self.guild.lock().unwrap();
        Ok(g.iter()
            .find(|(u, gid, _)| *u == user && *gid == guild)
            .map(|(_, _, e)| GuildRights { can_edit: *e }))
    }
}

fn track(id: i64, mount: i64, title: &str) -> TrackRow {
    TrackRow {
        id,
        mount_id: mount,
        title: title.to_owned(),
        artist: "Artist".to_owned(),
        album: "Album".to_owned(),
        album_artist: String::new(),
        track_no: Some(1),
        disc_no: None,
        year: None,
        duration_ms: Some(1000),
        genres: vec![],
        tags: vec![],
        cover_key: None,
    }
}

struct World {
    deps: MusicLibDeps,
    store: Arc<MemoryMusicLib>,
}

fn world() -> World {
    let store = Arc::new(MemoryMusicLib::new());
    store.add_track(track(100, 10, "Open"));
    store.add_track(track(101, 10, "Also open"));
    store.add_track(track(200, 11, "Alice only"));
    let reach = FakeReach::default();
    *reach.mounts.lock().unwrap() = vec![(ALICE, 10), (ALICE, 11), (BOB, 10)];
    *reach.guild.lock().unwrap() = vec![(ALICE, GUILD, true), (BOB, GUILD, false)];
    let next = Arc::new(AtomicI64::new(1000));
    let deps = MusicLibDeps::new(
        store.clone(),
        Arc::new(reach),
        Arc::new(move || next.fetch_add(1, Ordering::SeqCst)),
    );
    World { deps, store }
}

fn body(name: &str) -> PlaylistBody {
    PlaylistBody {
        name: name.to_owned(),
        description: None,
    }
}

fn add(ids: &[i64]) -> AddItemsBody {
    AddItemsBody {
        track_ids: ids.iter().map(ToString::to_string).collect(),
    }
}

fn status(e: &MsgError) -> String {
    format!("{e:?}")
}

#[tokio::test]
async fn a_member_playlist_is_the_owners_alone() {
    let w = world();
    let p = svc_create(
        &w.deps,
        100,
        ALICE,
        PlaylistOwner::User(ALICE),
        body("Mine"),
    )
    .await
    .unwrap();
    assert!(p.can_edit);
    assert_eq!(svc_list_own(&w.deps, ALICE).await.unwrap().len(), 1);
    assert!(svc_list_own(&w.deps, BOB).await.unwrap().is_empty());
    // Bob sees nothing: not the playlist, not that it exists (404, never 403).
    for r in [
        svc_detail(&w.deps, BOB, &p.id).await.err(),
        svc_edit(
            &w.deps,
            BOB,
            &p.id,
            PlaylistPatchBody {
                name: Some("x".into()),
                description: None,
            },
        )
        .await
        .err(),
        svc_remove(&w.deps, BOB, &p.id).await.err(),
        svc_add_items(&w.deps, 100, BOB, &p.id, add(&[100]))
            .await
            .err(),
    ] {
        let e = r.expect("refused");
        assert!(status(&e).contains("SrNotFound"), "{e:?}");
    }
    // The owner renames and deletes.
    let renamed = svc_edit(
        &w.deps,
        ALICE,
        &p.id,
        PlaylistPatchBody {
            name: Some("  Renamed ".into()),
            description: Some("d".into()),
        },
    )
    .await
    .unwrap();
    assert_eq!(renamed.name, "Renamed");
    svc_remove(&w.deps, ALICE, &p.id).await.unwrap();
    assert!(svc_detail(&w.deps, ALICE, &p.id).await.is_err());
}

#[tokio::test]
async fn names_are_checked() {
    let w = world();
    for bad in ["", "   ", &"x".repeat(101)] {
        let r = svc_create(&w.deps, 100, ALICE, PlaylistOwner::User(ALICE), body(bad)).await;
        assert!(r.is_err(), "{bad:?}");
    }
    let long = PlaylistBody {
        name: "ok".into(),
        description: Some("d".repeat(501)),
    };
    assert!(
        svc_create(&w.deps, 100, ALICE, PlaylistOwner::User(ALICE), long)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn a_server_playlist_follows_the_upload_grant() {
    let w = world();
    let g = PlaylistOwner::Guild(GUILD);
    // Bob is a member without the grant: 403 on create, he may still read once Alice made one.
    let e = svc_create(&w.deps, 100, BOB, g, body("Nope"))
        .await
        .unwrap_err();
    assert!(status(&e).contains("MissingPermissions"), "{e:?}");
    // Carol is no member: 404, nothing about the server.
    let e = svc_create(&w.deps, 100, CAROL, g, body("Nope"))
        .await
        .unwrap_err();
    assert!(status(&e).contains("SrNotFound"), "{e:?}");

    let p = svc_create(&w.deps, 100, ALICE, g, body("Room"))
        .await
        .unwrap();
    assert_eq!(p.owner_type, "guild");
    svc_add_items(&w.deps, 100, ALICE, &p.id, add(&[100]))
        .await
        .unwrap();

    // Bob: reads (can_edit false), cannot edit.
    let seen = svc_list_guild(&w.deps, BOB, GUILD).await.unwrap();
    assert_eq!(seen.len(), 1);
    assert!(!seen[0].can_edit);
    let d = svc_detail(&w.deps, BOB, &p.id).await.unwrap();
    assert_eq!(d.items.len(), 1);
    assert!(!d.playlist.can_edit);
    for r in [
        svc_add_items(&w.deps, 100, BOB, &p.id, add(&[101]))
            .await
            .err(),
        svc_remove(&w.deps, BOB, &p.id).await.err(),
        svc_move_item(&w.deps, BOB, &p.id, &d.items[0].item_id, 0)
            .await
            .err(),
        svc_remove_item(&w.deps, BOB, &p.id, &d.items[0].item_id)
            .await
            .err(),
    ] {
        let e = r.expect("refused");
        assert!(status(&e).contains("MissingPermissions"), "{e:?}");
    }
    // Carol: 404 everywhere.
    assert!(svc_list_guild(&w.deps, CAROL, GUILD).await.is_err());
    assert!(svc_detail(&w.deps, CAROL, &p.id).await.is_err());
}

#[tokio::test]
async fn caps_hold_per_owner_and_per_playlist() {
    let w = world();
    let me = PlaylistOwner::User(ALICE);
    svc_create(&w.deps, 2, ALICE, me, body("a")).await.unwrap();
    svc_create(&w.deps, 2, ALICE, me, body("b")).await.unwrap();
    let e = svc_create(&w.deps, 2, ALICE, me, body("c"))
        .await
        .unwrap_err();
    assert!(status(&e).contains("PLAYLIST_LIMIT"), "{e:?}");
    // The cap is per owner: Bob and the server still create.
    svc_create(&w.deps, 2, BOB, PlaylistOwner::User(BOB), body("b1"))
        .await
        .unwrap();
    svc_create(&w.deps, 2, ALICE, PlaylistOwner::Guild(GUILD), body("g1"))
        .await
        .unwrap();

    let p = svc_create(&w.deps, 100, BOB, PlaylistOwner::User(BOB), body("full"))
        .await
        .unwrap();
    svc_add_items(&w.deps, 3, BOB, &p.id, add(&[100, 101]))
        .await
        .unwrap();
    // A batch that would pass the cap is refused whole, the list keeps its two.
    let e = svc_add_items(&w.deps, 3, BOB, &p.id, add(&[100, 101]))
        .await
        .unwrap_err();
    assert!(status(&e).contains("PLAYLIST_FULL"), "{e:?}");
    assert_eq!(
        svc_detail(&w.deps, BOB, &p.id).await.unwrap().items.len(),
        2
    );
    // One more fits exactly; the same track twice is allowed.
    svc_add_items(&w.deps, 3, BOB, &p.id, add(&[100]))
        .await
        .unwrap();
    assert_eq!(
        svc_detail(&w.deps, BOB, &p.id)
            .await
            .unwrap()
            .playlist
            .item_count,
        3
    );
}

#[tokio::test]
async fn a_track_the_member_cannot_read_cannot_be_added() {
    let w = world();
    let p = svc_create(&w.deps, 100, BOB, PlaylistOwner::User(BOB), body("bob"))
        .await
        .unwrap();
    // 200 sits in Alice's own bucket; 999 does not exist: the same 404.
    for t in [200, 999] {
        let e = svc_add_items(&w.deps, 100, BOB, &p.id, add(&[100, t]))
            .await
            .unwrap_err();
        assert!(status(&e).contains("SrNotFound"), "{e:?}");
    }
    // Nothing of the refused batch landed.
    assert_eq!(
        svc_detail(&w.deps, BOB, &p.id)
            .await
            .unwrap()
            .playlist
            .item_count,
        0
    );
    // A malformed id and an empty or oversized batch are 400/404, never a panic.
    assert!(
        svc_add_items(&w.deps, 100, BOB, &p.id, add(&[]))
            .await
            .is_err()
    );
    let bad = AddItemsBody {
        track_ids: vec!["abc".into()],
    };
    assert!(svc_add_items(&w.deps, 100, BOB, &p.id, bad).await.is_err());
    let many = AddItemsBody {
        track_ids: vec!["100".into(); MAX_ADD_BATCH + 1],
    };
    assert!(svc_add_items(&w.deps, 100, BOB, &p.id, many).await.is_err());
}

#[tokio::test]
async fn a_server_list_hides_what_the_viewer_cannot_read() {
    let w = world();
    let g = PlaylistOwner::Guild(GUILD);
    let p = svc_create(&w.deps, 100, ALICE, g, body("Room"))
        .await
        .unwrap();
    // Alice adds a track from her own bucket (mount 11) and one from the shared mount 10.
    svc_add_items(&w.deps, 100, ALICE, &p.id, add(&[200, 100]))
        .await
        .unwrap();
    let alice = svc_detail(&w.deps, ALICE, &p.id).await.unwrap();
    assert_eq!(alice.items.len(), 2);
    assert_eq!(alice.hidden, 0);
    // Bob reads mount 10 only: he gets the one track and a count, never the title of the other.
    let bob = svc_detail(&w.deps, BOB, &p.id).await.unwrap();
    assert_eq!(bob.items.len(), 1);
    assert_eq!(bob.items[0].track.title, "Open");
    assert_eq!(bob.hidden, 1);
    assert_eq!(bob.playlist.item_count, 2);
    assert!(!serde_json::to_string(&bob).unwrap().contains("Alice only"));
}

#[tokio::test]
async fn order_move_remove_and_vanished_tracks() {
    let w = world();
    let p = svc_create(&w.deps, 100, ALICE, PlaylistOwner::User(ALICE), body("o"))
        .await
        .unwrap();
    svc_add_items(&w.deps, 100, ALICE, &p.id, add(&[100, 101, 200]))
        .await
        .unwrap();
    let d = svc_detail(&w.deps, ALICE, &p.id).await.unwrap();
    let titles = |d: &playlists::PlaylistDetailView| {
        d.items
            .iter()
            .map(|i| i.track.title.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(titles(&d), ["Open", "Also open", "Alice only"]);
    svc_move_item(&w.deps, ALICE, &p.id, &d.items[2].item_id, 0)
        .await
        .unwrap();
    let d = svc_detail(&w.deps, ALICE, &p.id).await.unwrap();
    assert_eq!(titles(&d), ["Alice only", "Open", "Also open"]);
    svc_remove_item(&w.deps, ALICE, &p.id, &d.items[1].item_id)
        .await
        .unwrap();
    let d = svc_detail(&w.deps, ALICE, &p.id).await.unwrap();
    assert_eq!(titles(&d), ["Alice only", "Also open"]);
    // Removing it twice is a 404.
    assert!(svc_remove_item(&w.deps, ALICE, &p.id, "1").await.is_err());
    // The index forgets a file: the list drops it quietly.
    w.store.vanish(101);
    let d = svc_detail(&w.deps, ALICE, &p.id).await.unwrap();
    assert_eq!(titles(&d), ["Alice only"]);
}

#[tokio::test]
async fn favorites_and_recent() {
    use super::{favorites, recent};
    let w = world();
    favorites::svc_add(&w.deps, 2, ALICE, "100").await.unwrap();
    favorites::svc_add(&w.deps, 2, ALICE, "100").await.unwrap();
    favorites::svc_add(&w.deps, 2, ALICE, "200").await.unwrap();
    // The cap counts distinct tracks; a third is refused.
    let e = favorites::svc_add(&w.deps, 2, ALICE, "101")
        .await
        .unwrap_err();
    assert!(status(&e).contains("FAVORITES_FULL"), "{e:?}");
    let l = favorites::svc_list(&w.deps, ALICE).await.unwrap();
    assert_eq!(l.items.len(), 2);
    assert_eq!(l.items[0].track.title, "Alice only", "newest first");
    // Bob cannot favorite a track he cannot read, and sees none of Alice's.
    assert!(favorites::svc_add(&w.deps, 2, BOB, "200").await.is_err());
    assert!(
        favorites::svc_list(&w.deps, BOB)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    favorites::svc_remove(&w.deps, ALICE, "200").await.unwrap();
    favorites::svc_remove(&w.deps, ALICE, "200").await.unwrap();
    assert_eq!(
        favorites::svc_list(&w.deps, ALICE)
            .await
            .unwrap()
            .items
            .len(),
        1
    );

    recent::svc_record(&w.deps, ALICE, "100").await.unwrap();
    recent::svc_record(&w.deps, ALICE, "101").await.unwrap();
    recent::svc_record(&w.deps, ALICE, "100").await.unwrap();
    let r = recent::svc_list(&w.deps, ALICE, None).await.unwrap();
    let t: Vec<_> = r.items.iter().map(|i| i.track.title.as_str()).collect();
    assert_eq!(
        t,
        ["Open", "Also open"],
        "a replay moves to the top, no duplicate"
    );
    assert!(recent::svc_record(&w.deps, BOB, "200").await.is_err());
    // Losing access to a bucket hides the rows without deleting them.
    let r = recent::svc_list(&w.deps, BOB, None).await.unwrap();
    assert!(r.items.is_empty());
}

#[tokio::test]
async fn recent_keeps_two_hundred() {
    let w = world();
    for i in 0..205 {
        w.store.add_track(track(1000 + i, 10, &format!("t{i}")));
    }
    for i in 0..205 {
        w.store.record_recent(BOB, 1000 + i).await.unwrap();
    }
    let all = w.store.recent(BOB, 500).await.unwrap();
    assert_eq!(all.len(), 200);
    assert_eq!(all[0].track.title, "t204");
    assert_eq!(all[199].track.title, "t5");
}

#[test]
fn the_router_and_document_build() {
    let _ = router();
    let doc = serde_json::to_value(openapi()).unwrap();
    let paths = doc["paths"].as_object().unwrap();
    assert!(paths.contains_key("/music/playlists/{playlist_id}/items/{item_id}"));
    assert!(paths.contains_key("/users/@me/music/recent"));
}
