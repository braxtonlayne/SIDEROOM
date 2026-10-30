//! Postgres tests for the music library (MUS-LIB-001, migration 1550): ownership, the server grant, the caps,
//! item order, the recent list and what cascades.
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test music_repo -- --include-ignored --test-threads=1`

use sr_db::music_repo::PgMusicLib;
use sr_db::scopes_repo::PgScopeFacts;
use sr_db::testing::TestDb;
use sr_storage::music_lib::{LibError, MusicLibStore, PlaylistOwner, RECENT_KEEP};
use sr_storage::scopes::{Grant, GrantSet, ScopeFacts, ScopeStore, grants_for};

const ALICE: i64 = 1;
const BOB: i64 = 2;
const CARA: i64 = 3;
const GUILD: i64 = 50;

async fn account(pool: &sqlx::PgPool, id: i64) {
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(id + 100_000)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $2, $3, $4, 'x')",
    )
    .bind(id)
    .bind(id + 100_000)
    .bind(format!("u{id}"))
    .bind(format!("u{id}@example.test"))
    .execute(pool)
    .await
    .unwrap();
}

async fn track(pool: &sqlx::PgPool, id: i64, mount: i64, title: &str, state: &str) {
    sqlx::query(
        "INSERT INTO music_tracks (id, mount_id, mount_object_id, title, artist, album, shelf_state) \
         VALUES ($1, $2, $1, $3, 'Artist', 'Album', $4)",
    )
    .bind(id)
    .bind(mount)
    .bind(title)
    .bind(state)
    .execute(pool)
    .await
    .unwrap();
}

async fn world() -> (TestDb, PgMusicLib) {
    let db = TestDb::new().await.expect("db");
    let pool = db.pool();
    for id in [ALICE, BOB, CARA] {
        account(pool, id).await;
    }
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Room', $2)")
        .bind(GUILD)
        .bind(ALICE)
        .execute(pool)
        .await
        .unwrap();
    track(pool, 100, 10, "One", "listed").await;
    track(pool, 101, 10, "Two", "listed").await;
    track(pool, 102, 10, "Three", "listed").await;
    track(pool, 199, 10, "Gone", "vanished").await;
    let lib = PgMusicLib::new(pool.clone());
    (db, lib)
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn playlists_belong_to_their_owner_and_the_cap_holds_per_owner() {
    let (db, lib) = world().await;
    let me = PlaylistOwner::User(ALICE);
    let a = lib.create_playlist(1, me, "A", "", ALICE, 2).await.unwrap();
    lib.create_playlist(2, me, "B", "d", ALICE, 2)
        .await
        .unwrap();
    assert_eq!(
        lib.create_playlist(3, me, "C", "", ALICE, 2).await,
        Err(LibError::Cap)
    );
    // The cap is per owner: Bob and the server are not held back by Alice's two.
    lib.create_playlist(4, PlaylistOwner::User(BOB), "Bob", "", BOB, 2)
        .await
        .unwrap();
    lib.create_playlist(5, PlaylistOwner::Guild(GUILD), "Room", "", ALICE, 2)
        .await
        .unwrap();

    let mine = lib.playlists_of(me).await.unwrap();
    assert_eq!(
        mine.iter().map(|p| p.id).collect::<Vec<_>>(),
        [2, 1],
        "newest first"
    );
    assert_eq!(
        lib.playlists_of(PlaylistOwner::User(BOB))
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        lib.playlists_of(PlaylistOwner::User(CARA))
            .await
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        lib.playlists_of(PlaylistOwner::Guild(GUILD)).await.unwrap()[0].owner,
        PlaylistOwner::Guild(GUILD)
    );
    assert_eq!(a.owner, me);
    // Rename keeps the owner; deleting makes it gone for everyone.
    let r = lib.update_playlist(1, Some("A2"), None).await.unwrap();
    assert_eq!((r.name.as_str(), r.owner), ("A2", me));
    lib.delete_playlist(1).await.unwrap();
    assert_eq!(lib.playlist(1).await.unwrap(), None);
    assert_eq!(lib.delete_playlist(1).await, Err(LibError::NotFound));
    // Room for a new one after the delete.
    lib.create_playlist(6, me, "D", "", ALICE, 2).await.unwrap();
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_table_refuses_a_playlist_with_two_owners_or_none() {
    let (db, _lib) = world().await;
    let pool = db.pool();
    for (u, g) in [(Some(ALICE), Some(GUILD)), (None, None)] {
        let r = sqlx::query(
            "INSERT INTO music_playlists (id, owner_user_id, owner_guild_id, name) VALUES (77, $1, $2, 'x')",
        )
        .bind(u)
        .bind(g)
        .execute(pool)
        .await;
        assert!(r.is_err(), "{u:?} {g:?}");
    }
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn items_keep_a_dense_order_through_add_move_and_remove() {
    let (db, lib) = world().await;
    let me = PlaylistOwner::User(ALICE);
    lib.create_playlist(1, me, "P", "", ALICE, 10)
        .await
        .unwrap();
    assert_eq!(
        lib.add_items(1, &[(11, 100), (12, 101), (13, 102)], ALICE, 10)
            .await,
        Ok(3)
    );
    let order = |items: Vec<sr_storage::music_lib::ItemRow>| {
        items
            .iter()
            .map(|i| (i.item_id, i.position))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        order(lib.items(1).await.unwrap()),
        [(11, 0), (12, 1), (13, 2)]
    );

    lib.move_item(1, 13, 0).await.unwrap();
    assert_eq!(
        order(lib.items(1).await.unwrap()),
        [(13, 0), (11, 1), (12, 2)]
    );
    lib.move_item(1, 13, 99).await.unwrap(); // clamped to the end
    assert_eq!(
        order(lib.items(1).await.unwrap()),
        [(11, 0), (12, 1), (13, 2)]
    );
    lib.remove_item(1, 12).await.unwrap();
    assert_eq!(order(lib.items(1).await.unwrap()), [(11, 0), (13, 1)]);
    assert_eq!(lib.remove_item(1, 12).await, Err(LibError::NotFound));
    assert_eq!(lib.move_item(1, 12, 0).await, Err(LibError::NotFound));
    // Another list's item is not reachable through this one.
    lib.create_playlist(2, me, "Q", "", ALICE, 10)
        .await
        .unwrap();
    lib.add_items(2, &[(21, 100)], ALICE, 10).await.unwrap();
    assert_eq!(lib.remove_item(1, 21).await, Err(LibError::NotFound));
    // The same track twice is allowed.
    lib.add_items(1, &[(14, 100)], ALICE, 10).await.unwrap();
    assert_eq!(lib.playlist(1).await.unwrap().unwrap().item_count, 3);
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_item_cap_refuses_a_whole_batch_and_unknown_or_vanished_tracks() {
    let (db, lib) = world().await;
    let me = PlaylistOwner::User(ALICE);
    lib.create_playlist(1, me, "P", "", ALICE, 10)
        .await
        .unwrap();
    lib.add_items(1, &[(11, 100), (12, 101)], ALICE, 3)
        .await
        .unwrap();
    // Two more would make four: nothing of the batch lands.
    assert_eq!(
        lib.add_items(1, &[(13, 102), (14, 102)], ALICE, 3).await,
        Err(LibError::Cap)
    );
    assert_eq!(lib.items(1).await.unwrap().len(), 2);
    lib.add_items(1, &[(13, 102)], ALICE, 3).await.unwrap();
    // A vanished or missing track refuses the batch (404 on the wire).
    lib.create_playlist(2, me, "Q", "", ALICE, 10)
        .await
        .unwrap();
    assert_eq!(
        lib.add_items(2, &[(21, 100), (22, 199)], ALICE, 10).await,
        Err(LibError::NotFound)
    );
    assert_eq!(
        lib.add_items(2, &[(21, 424_242)], ALICE, 10).await,
        Err(LibError::NotFound)
    );
    assert_eq!(
        lib.add_items(999, &[(31, 100)], ALICE, 10).await,
        Err(LibError::NotFound)
    );
    assert_eq!(lib.items(2).await.unwrap().len(), 0);
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn tracks_that_vanish_leave_the_lists_and_deleted_ones_take_their_rows_along() {
    let (db, lib) = world().await;
    let pool = db.pool();
    let me = PlaylistOwner::User(ALICE);
    lib.create_playlist(1, me, "P", "", ALICE, 10)
        .await
        .unwrap();
    lib.add_items(1, &[(11, 100), (12, 101)], ALICE, 10)
        .await
        .unwrap();
    lib.add_favorite(ALICE, 100, 10).await.unwrap();
    lib.record_recent(ALICE, 100).await.unwrap();

    // The index marks a file as vanished: it drops out of every read, the rows stay (it may come back).
    sqlx::query("UPDATE music_tracks SET shelf_state = 'vanished' WHERE id = 100")
        .execute(pool)
        .await
        .unwrap();
    assert_eq!(
        lib.items(1)
            .await
            .unwrap()
            .iter()
            .map(|i| i.track.id)
            .collect::<Vec<_>>(),
        [101]
    );
    assert!(lib.favorites(ALICE).await.unwrap().is_empty());
    assert!(lib.recent(ALICE, 50).await.unwrap().is_empty());
    assert_eq!(lib.tracks(&[100, 101]).await.unwrap().len(), 1);
    sqlx::query("UPDATE music_tracks SET shelf_state = 'listed' WHERE id = 100")
        .execute(pool)
        .await
        .unwrap();
    assert_eq!(lib.items(1).await.unwrap().len(), 2);

    // The index deletes the track row: items, favorites and history go with it.
    sqlx::query("DELETE FROM music_tracks WHERE id = 100")
        .execute(pool)
        .await
        .unwrap();
    assert_eq!(lib.items(1).await.unwrap().len(), 1);
    let left: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM music_playlist_items WHERE track_id = 100) \
              + (SELECT count(*) FROM music_favorites WHERE track_id = 100) \
              + (SELECT count(*) FROM music_recent WHERE track_id = 100)",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(left, 0);
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn favorites_are_per_member_idempotent_and_capped() {
    let (db, lib) = world().await;
    lib.add_favorite(ALICE, 100, 2).await.unwrap();
    lib.add_favorite(ALICE, 100, 2).await.unwrap();
    lib.add_favorite(ALICE, 101, 2).await.unwrap();
    assert_eq!(lib.add_favorite(ALICE, 102, 2).await, Err(LibError::Cap));
    // Re-favoriting at the cap is still fine (nothing new is added).
    lib.add_favorite(ALICE, 101, 2).await.unwrap();
    assert_eq!(
        lib.add_favorite(ALICE, 199, 5).await,
        Err(LibError::NotFound)
    );
    assert_eq!(lib.favorites(ALICE).await.unwrap().len(), 2);
    assert_eq!(lib.favorites(BOB).await.unwrap().len(), 0);
    lib.add_favorite(BOB, 102, 2).await.unwrap();
    lib.remove_favorite(ALICE, 100).await.unwrap();
    lib.remove_favorite(ALICE, 100).await.unwrap();
    assert_eq!(
        lib.favorites(ALICE)
            .await
            .unwrap()
            .iter()
            .map(|f| f.track.id)
            .collect::<Vec<_>>(),
        [101]
    );
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn recent_moves_a_replayed_track_up_and_keeps_two_hundred() {
    let (db, lib) = world().await;
    let pool = db.pool();
    lib.record_recent(ALICE, 100).await.unwrap();
    lib.record_recent(ALICE, 101).await.unwrap();
    lib.record_recent(ALICE, 100).await.unwrap();
    let ids = |r: Vec<sr_storage::music_lib::StampedTrack>| {
        r.iter().map(|s| s.track.id).collect::<Vec<_>>()
    };
    assert_eq!(ids(lib.recent(ALICE, 50).await.unwrap()), [100, 101]);
    assert_eq!(lib.record_recent(ALICE, 199).await, Err(LibError::NotFound));

    for i in 0..205_i64 {
        track(pool, 1000 + i, 10, &format!("t{i}"), "listed").await;
    }
    for i in 0..205_i64 {
        lib.record_recent(BOB, 1000 + i).await.unwrap();
    }
    let all = lib.recent(BOB, 1000).await.unwrap();
    assert_eq!(all.len() as i64, RECENT_KEEP);
    assert_eq!(all[0].track.id, 1204);
    assert_eq!(all[(RECENT_KEEP - 1) as usize].track.id, 1005);
    // Bob's pruning did not touch Alice's list.
    assert_eq!(lib.recent(ALICE, 50).await.unwrap().len(), 2);
    lib.clear_recent(BOB).await.unwrap();
    assert!(lib.recent(BOB, 50).await.unwrap().is_empty());
    db.cleanup().await.unwrap();
}

/// The route rule for a server playlist, over the real facts: members read, `upload` or `manage` edits, a
/// stranger sees nothing. (`ShelfReach` in sr-api is these three calls.)
async fn server_rights(facts: &PgScopeFacts, user: i64) -> Option<bool> {
    let standing = facts.guild_standing(GUILD, user).await.unwrap()?;
    let roles = facts.role_grants(GUILD, &standing.role_ids).await.unwrap();
    let g = grants_for(&standing, &roles);
    Some(g.has(Grant::Upload) || g.has(Grant::Manage))
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_server_playlist_is_editable_with_the_upload_grant_and_gone_with_the_server() {
    let (db, lib) = world().await;
    let pool = db.pool();
    // Alice owns the server. Bob has the DJ role (upload); Cara is a member without it; Dan is not a member.
    account(pool, 4).await;
    sqlx::query("INSERT INTO roles (id, guild_id, name, permissions) VALUES ($1, $2, '@everyone', 0), (61, $2, 'dj', 0)")
        .bind(GUILD)
        .bind(GUILD)
        .execute(pool)
        .await
        .unwrap();
    for u in [ALICE, BOB, CARA] {
        sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
            .bind(GUILD)
            .bind(u)
            .execute(pool)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO member_roles (guild_id, user_id, role_id) VALUES ($1, $2, 61)")
        .bind(GUILD)
        .bind(BOB)
        .execute(pool)
        .await
        .unwrap();
    let facts = PgScopeFacts::new(pool.clone());
    facts
        .set_role_grants(GUILD, 61, &GrantSet::of([Grant::Upload]))
        .await
        .unwrap();

    assert_eq!(server_rights(&facts, ALICE).await, Some(true), "owner");
    assert_eq!(server_rights(&facts, BOB).await, Some(true), "upload grant");
    assert_eq!(
        server_rights(&facts, CARA).await,
        Some(false),
        "member without the grant"
    );
    assert_eq!(server_rights(&facts, 4).await, None, "not a member");

    let owner = PlaylistOwner::Guild(GUILD);
    lib.create_playlist(1, owner, "Room", "", BOB, 100)
        .await
        .unwrap();
    lib.add_items(1, &[(11, 100)], BOB, 5000).await.unwrap();
    assert_eq!(lib.items(1).await.unwrap().len(), 1);

    // Taking the grant away from the role takes the edit right away on the next request.
    facts
        .set_role_grants(GUILD, 61, &GrantSet::default())
        .await
        .unwrap();
    assert_eq!(server_rights(&facts, BOB).await, Some(false));

    // Deleting the server deletes its playlists and their items.
    sqlx::query("DELETE FROM guilds WHERE id = $1")
        .bind(GUILD)
        .execute(pool)
        .await
        .unwrap();
    assert_eq!(lib.playlist(1).await.unwrap(), None);
    let items: i64 = sqlx::query_scalar("SELECT count(*) FROM music_playlist_items")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(items, 0);
    db.cleanup().await.unwrap();
}
