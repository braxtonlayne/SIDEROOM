//! Postgres-backed tests of the Live TV repository and service. Ignored by default: they need
//! `SR_TEST_DATABASE_URL` (a role that may CREATEDB), run with `--ignored --test-threads=1`.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use sr_core::{ManualClock, PlatformPermissions as P};
use sr_db::testing::TestDb;
use sr_tv::catalog::{MemoryCatalog, episode};
use sr_tv::model::*;
use sr_tv::pg::PgTvRepo;
use sr_tv::repo::TvRepository;
use sr_tv::schedule::Change;
use sr_tv::service::{Actor, TvService};
use sr_tv::tz::{MS_DAY, days_from_civil};

fn input(number: i32) -> ChannelInput {
    serde_json::from_value(serde_json::json!({
        "number": number, "name": "Toonami", "kind": "rule_based", "timezone": "America/New_York",
        "filler_policy": "slate", "logo_key": "tv/toonami.png", "call_sign": "TOON",
        "rules": {"grid_minutes": 30, "dayparts": [
            {"name": "All day", "start": "00:00", "end": "24:00", "mode": "sequential",
             "pool": {"title_ids": ["7"], "genres": ["action"]}}]}
    }))
    .unwrap()
}

fn block(id: i64, channel: i64, start: i64, end: i64) -> BlockRecord {
    BlockRecord {
        id,
        channel_id: channel,
        start_ms: start,
        end_ms: end,
        kind: BlockKind::Program,
        title_id: Some(7),
        episode_id: Some(7000 + id),
        asset_id: Some(9000 + id),
        title: "Cowboy Bebop".into(),
        episode_title: Some("Asteroid Blues".into()),
        media_kind: Some(MediaKind::Anime),
        season_number: Some(1),
        episode_number: Some(1),
        year: Some(1998),
        age_restricted: false,
        offset_ms: 0,
        source: BlockSource::Generated,
        cursor_after: Some(Cursor {
            at_ms: end,
            seq: [("All day".to_owned(), 3u64)].into(),
            recent: vec![1, 2],
            rng: 99,
            bumper: 4,
        }),
        seq_base: None,
        disc_base: None,
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn channels_round_trip_and_numbers_are_unique() {
    let db = TestDb::new().await.expect("test database");
    let repo = PgTvRepo::new(db.pool().clone());
    let c = repo
        .create_channel(1, &input(101), 42, 1_700_000_000_000)
        .await
        .unwrap();
    assert_eq!(c.input.rules.dayparts[0].pool.title_ids, vec![7]);
    assert_eq!(c.input.rules.dayparts[0].pool.genres, vec!["action"]);
    assert_eq!((c.seed, c.created_at_ms), (42, 1_700_000_000_000));
    assert_eq!(c.input.filler_policy, FillerPolicy::Slate);
    let dup = repo.create_channel(2, &input(101), 1, 0).await;
    assert!(matches!(dup, Err(sr_tv::TvError::Conflict(_))), "{dup:?}");
    repo.create_channel(2, &input(102), 1, 0).await.unwrap();
    let mut upd = input(103);
    upd.name = "Renamed".into();
    let u = repo
        .update_channel(1, &upd, 1_700_000_100_000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (u.input.number, u.input.name.as_str(), u.updated_at_ms),
        (103, "Renamed", 1_700_000_100_000)
    );
    let clash = repo.update_channel(1, &input(102), 0).await;
    assert!(matches!(clash, Err(sr_tv::TvError::Conflict(_))));
    assert!(
        repo.update_channel(99, &input(5), 0)
            .await
            .unwrap()
            .is_none()
    );
    let nums: Vec<_> = repo
        .list_channels()
        .await
        .unwrap()
        .iter()
        .map(|c| c.input.number)
        .collect();
    assert_eq!(nums, [102, 103]);
    assert!(repo.delete_channel(2).await.unwrap());
    assert!(!repo.delete_channel(2).await.unwrap());
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn blocks_apply_query_and_number_atomically() {
    let db = TestDb::new().await.expect("test database");
    let repo = PgTvRepo::new(db.pool().clone());
    repo.create_channel(1, &input(101), 1, 0).await.unwrap();
    repo.create_channel(2, &input(102), 1, 0).await.unwrap();
    let change = Change {
        delete: vec![],
        upsert: vec![
            block(11, 1, 0, 100),
            block(12, 1, 100, 200),
            block(13, 1, 200, 300),
            block(21, 2, 0, 100),
        ],
    };
    assert_eq!(repo.apply(1, &change, false).await.unwrap(), 4);
    let got = repo.blocks_overlapping(&[1], 50, 150).await.unwrap();
    assert_eq!(got.iter().map(|b| b.id).collect::<Vec<_>>(), [11, 12]);
    assert_eq!(got[0].cursor_after.as_ref().unwrap().seq["All day"], 3);
    assert_eq!(got[0].media_kind, Some(MediaKind::Anime));
    assert_eq!(
        repo.blocks_overlapping(&[1, 2], 0, 1).await.unwrap().len(),
        2
    );
    assert_eq!(repo.last_block(1).await.unwrap().unwrap().id, 13);
    assert_eq!(repo.cursor_block(1, 250).await.unwrap().unwrap().id, 12);
    assert!(repo.cursor_block(1, 50).await.unwrap().is_none());

    // A conflicting start is skipped when asked to, and an error otherwise (and then nothing changes).
    let clash = Change {
        delete: vec![],
        upsert: vec![block(99, 1, 100, 150)],
    };
    assert_eq!(repo.apply(1, &clash, true).await.unwrap(), 0);
    assert!(matches!(
        repo.apply(1, &clash, false).await,
        Err(sr_tv::TvError::Conflict(_))
    ));
    assert!(repo.get_block(99).await.unwrap().is_none());

    // Trim, split and delete in one change; a placed block has no cursor.
    let mut head = block(12, 1, 100, 120);
    head.cursor_after = None;
    let mut placed = block(50, 1, 120, 140);
    placed.source = BlockSource::Override;
    placed.cursor_after = None;
    let mut tail = block(51, 1, 140, 200);
    tail.offset_ms = 40;
    let c = Change {
        delete: vec![13],
        upsert: vec![head, tail, placed],
    };
    repo.apply(1, &c, false).await.unwrap();
    let all = repo.blocks_overlapping(&[1], 0, 1000).await.unwrap();
    assert_eq!(
        all.iter()
            .map(|b| (b.id, b.start_ms, b.end_ms))
            .collect::<Vec<_>>(),
        [(11, 0, 100), (12, 100, 120), (50, 120, 140), (51, 140, 200)]
    );
    assert_eq!(all[2].source, BlockSource::Override);
    assert_eq!(all[3].offset_ms, 40);
    // The cursor query only sees generated blocks that have one.
    assert_eq!(repo.cursor_block(1, 1000).await.unwrap().unwrap().id, 51);

    // Bases are written once.
    assert_eq!(repo.set_bases(11, 500, 7, 10).await.unwrap(), (500, 7));
    assert_eq!(repo.set_bases(11, 900, 9, 10).await.unwrap(), (500, 7));
    assert_eq!(
        repo.get_block(11).await.unwrap().unwrap().seq_base,
        Some(500)
    );
    assert!(repo.set_bases(404, 1, 1, 1).await.is_err());

    // The channel chain anchor (D-285) follows the bases and only moves to a later end.
    assert_eq!(
        anchor(&repo, 1).await,
        Some(ChainAnchor {
            end_ms: 100,
            seq: 510,
            disc: 8,
            block_id: 11
        })
    );
    repo.set_bases(12, 600, 8, 4).await.unwrap();
    assert_eq!(
        anchor(&repo, 1).await,
        Some(ChainAnchor {
            end_ms: 120,
            seq: 604,
            disc: 9,
            block_id: 12
        })
    );
    repo.set_bases(50, 1, 1, 1).await.unwrap();
    assert_eq!(
        anchor(&repo, 1).await.unwrap().block_id,
        50,
        "ends at 140: later wins"
    );
    repo.set_bases(11, 1, 1, 1).await.unwrap();
    assert_eq!(
        anchor(&repo, 1).await.unwrap().block_id,
        50,
        "never moves back"
    );
    let moved = ChainAnchor {
        end_ms: 130,
        seq: 700,
        disc: 12,
        block_id: 50,
    };
    repo.set_anchor(1, moved).await.unwrap();
    assert_eq!(anchor(&repo, 1).await, Some(moved));
    assert_eq!(anchor(&repo, 2).await, None, "other channels are untouched");

    // Deleting a channel takes its blocks along.
    repo.delete_channel(1).await.unwrap();
    assert!(
        repo.blocks_overlapping(&[1], 0, 1000)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        repo.blocks_overlapping(&[2], 0, 1000).await.unwrap().len(),
        1
    );
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_service_extends_publishes_and_places_over_postgres() {
    let db = TestDb::new().await.expect("test database");
    let t0 = days_from_civil(2025, 6, 2) * MS_DAY + 20 * 3_600_000;
    let clock = Arc::new(ManualClock::new(t0));
    let catalog = MemoryCatalog::new();
    for n in 1..=40 {
        let mut e = episode(7, "Cowboy Bebop", 1, n, 30);
        e.genres = vec!["action".into()];
        catalog.add(e);
    }
    let next = Arc::new(AtomicI64::new(5000));
    let svc = TvService::new(
        Arc::new(PgTvRepo::new(db.pool().clone())),
        Arc::new(catalog),
        clock.clone(),
        Arc::new(move || next.fetch_add(1, Ordering::SeqCst)),
    );
    let staff = Actor {
        user_id: 1,
        permissions: P::MANAGE_TV,
    };
    let mut inp = input(101);
    inp.timezone = "UTC".into();
    let id = svc.create_channel(staff, inp).await.unwrap().id;
    svc.now(id).await.unwrap();
    clock.advance_ms(17 * 60_000);
    let now = svc.now(id).await.unwrap();
    assert_eq!(now.offset_ms, Some(17 * 60_000));
    assert_eq!(now.current.unwrap().episode_number, Some(1));
    assert_eq!(now.next.unwrap().episode_number, Some(2));
    let g = svc.guide(None, None, &[]).await.unwrap();
    assert_eq!(g.channels[0].blocks.len(), 8);
    // Publish regenerates the future identically (same cursor, same rules).
    let window = Some(t0 + 3 * 86_400_000);
    let before: Vec<_> = svc
        .admin_schedule(staff, id, None, window)
        .await
        .unwrap()
        .iter()
        .map(|b| (b.start_ms, b.episode_id))
        .collect();
    let done = svc.publish(staff, id, Some(24)).await.unwrap();
    assert!(done.removed > 0 && done.written > 0);
    let after: Vec<_> = svc
        .admin_schedule(staff, id, None, window)
        .await
        .unwrap()
        .iter()
        .map(|b| (b.start_ms, b.episode_id))
        .collect();
    assert_eq!(
        before[..48],
        after[..48],
        "the next day is the same after publishing"
    );
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn slots_imports_and_favorites_round_trip() {
    let db = TestDb::new().await.expect("test database");
    let pool = db.pool().clone();
    let repo = PgTvRepo::new(pool.clone());
    let mut guide = input(110);
    guide.kind = ChannelKind::ImportedGuide;
    repo.create_channel(1, &guide, 5, 0).await.unwrap();
    repo.create_channel(2, &input(111), 6, 0).await.unwrap();
    assert_eq!(repo.imported_channels().await.unwrap(), vec![1]);

    let slot = |id: i64, wd: u8, start: u16, title: Option<i64>| SlotRecord {
        id,
        channel_id: 1,
        weekday: wd,
        start_minute: start,
        duration_min: 30,
        show_name: format!("Show {id}"),
        title_id: title,
        episode_id: None,
        season_number: Some(1),
        episode_number: None,
        note: Some("n".into()),
        import_id: None,
    };
    repo.replace_slots(1, &[slot(11, 2, 1200, Some(7)), slot(10, 0, 1230, None)])
        .await
        .unwrap();
    let got = repo.list_slots(1).await.unwrap();
    assert_eq!(got.iter().map(|s| s.id).collect::<Vec<_>>(), [10, 11]);
    assert_eq!(
        (got[1].weekday, got[1].start_minute, got[1].title_id),
        (2, 1200, Some(7))
    );
    assert_eq!(got[0].season_number, Some(1));
    // A clash on (weekday, start) rolls the whole replacement back.
    let clash = repo
        .replace_slots(1, &[slot(20, 1, 600, None), slot(21, 1, 600, None)])
        .await;
    assert!(clash.is_err());
    assert_eq!(repo.list_slots(1).await.unwrap().len(), 2);
    repo.replace_slots(1, &[]).await.unwrap();
    assert!(repo.list_slots(1).await.unwrap().is_empty());

    // Imports: stored pending, listed without rows, confirmed once.
    let rec = ImportRecord {
        id: 50,
        channel_id: 1,
        created_by: None,
        format: "csv".into(),
        confirmed: false,
        rows: serde_json::json!([{"line": 2}]),
        report: serde_json::json!({"rows": [], "matched": 0}),
        created_at_ms: 1_700_000_000_000,
    };
    repo.put_import(&rec).await.unwrap();
    assert_eq!(repo.get_import(50).await.unwrap().unwrap(), rec);
    let listed = repo.list_imports(1, 10).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].rows.is_null() && listed[0].report == rec.report);
    assert!(repo.confirm_import(50, 1_700_000_001_000).await.unwrap());
    assert!(!repo.confirm_import(50, 1_700_000_002_000).await.unwrap());
    assert!(repo.get_import(50).await.unwrap().unwrap().confirmed);

    // Favorites are per account, idempotent, and gone with the channel.
    for (acct, person) in [(8_i64, 800_i64), (9, 900)] {
        sqlx::query("INSERT INTO persons (id) VALUES ($1)")
            .bind(person)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $2, $3, $4, 'x')",
        )
        .bind(acct)
        .bind(person)
        .bind(format!("u{acct}"))
        .bind(format!("u{acct}@example.test"))
        .execute(&pool)
        .await
        .unwrap();
    }
    repo.set_favorite(8, 1, true).await.unwrap();
    repo.set_favorite(8, 1, true).await.unwrap();
    repo.set_favorite(8, 2, true).await.unwrap();
    repo.set_favorite(9, 2, true).await.unwrap();
    // An unknown channel stars nothing.
    repo.set_favorite(8, 999, true).await.unwrap();
    assert_eq!(repo.favorites(8).await.unwrap(), vec![1, 2]);
    repo.set_favorite(8, 1, false).await.unwrap();
    assert_eq!(repo.favorites(8).await.unwrap(), vec![2]);
    repo.delete_channel(2).await.unwrap();
    assert!(repo.favorites(8).await.unwrap().is_empty());
    assert!(repo.favorites(9).await.unwrap().is_empty());
    db.cleanup().await.unwrap();
}

async fn anchor(repo: &PgTvRepo, channel: i64) -> Option<ChainAnchor> {
    repo.get_channel(channel).await.unwrap().unwrap().chain
}
