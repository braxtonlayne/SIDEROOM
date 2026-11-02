//! Postgres-backed tests of the Arcade (ARC-ROOM-001): the tier a manifest declares and what it allows, the
//! Operator's Front Page, Coming Soon with its "Notify me" toggle, Guild Cartridges and the closed tier list in
//! the database. Run with a scratch Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-activities --test arcade -- --ignored --test-threads=1`

use std::io::{Cursor, Write};
use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use async_trait::async_trait;

use sr_activities::arcade::{
    ArcadeService, CartridgeInput, ComingSoonInput, FeatureInput, Tier, TipInput,
};
use sr_activities::model::{ConfigInput, UrlMapping};
use sr_activities::{ActivityError, ActivityService, ChannelAccess, ChannelPerms};
use sr_db::{
    PgPool,
    accounts::{self, NewAccount},
    testing::TestDb,
};
use sr_storage::{ObjectStorage, ObjectStoreStorage};

static SEQ: AtomicI64 = AtomicI64::new(0);

fn uid() -> i64 {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + SEQ.fetch_add(1, Ordering::Relaxed) % 1000
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

async fn application(pool: &PgPool, owner: i64, name: &str) -> i64 {
    let bot = account(pool).await;
    let id = uid();
    sqlx::query(
        "INSERT INTO applications (id, owner_id, bot_user_id, name, client_secret_hash) \
         VALUES ($1, $2, $3, $4, decode(repeat('ab', 32), 'hex'))",
    )
    .bind(id)
    .bind(owner)
    .bind(bot)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// Everybody may use voice channel 1 of guild 9.
struct OpenVoice;

#[async_trait]
impl ChannelAccess for OpenVoice {
    async fn perms(&self, _user: i64, _channel: i64) -> Result<Option<ChannelPerms>, String> {
        Ok(Some(ChannelPerms {
            guild_id: 9,
            voice: true,
            text: false,
            view: true,
            connect: true,
            use_activities: true,
            manage: false,
        }))
    }
    async fn viewers(&self, _channel: i64) -> Result<Vec<i64>, String> {
        Ok(vec![])
    }
}

struct Rig {
    db: TestDb,
    acts: ActivityService,
    arcade: ArcadeService,
}

async fn rig() -> Rig {
    let db = TestDb::new().await.unwrap();
    let storage: Arc<dyn ObjectStorage> = Arc::new(ObjectStoreStorage::in_memory());
    let acts = ActivityService::new(db.pool().clone(), Arc::new(uid))
        .with_storage(storage)
        .with_channels(Arc::new(OpenVoice));
    let arcade = ArcadeService::new(db.pool().clone(), Arc::new(uid));
    Rig { db, acts, arcade }
}

fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (n, d) in files {
        z.start_file(*n, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(d).unwrap();
    }
    z.finish().unwrap().into_inner()
}

fn input() -> ConfigInput {
    ConfigInput {
        dev_entry_url: None,
        url_mappings: vec![UrlMapping {
            prefix: "/api".into(),
            target: "https://api.example.com".into(),
        }],
    }
}

/// A released application whose reviewed bundle carries `manifest` (or none).
async fn game(r: &Rig, name: &str, manifest: Option<&str>) -> i64 {
    let owner = account(r.db.pool()).await;
    let app = application(r.db.pool(), owner, name).await;
    r.acts.save_config(app, input()).await.unwrap();
    let mut files: Vec<(&str, &[u8])> = vec![("index.html", b"<h1>game</h1>")];
    if let Some(m) = manifest {
        files.push(("sideroom.json", m.as_bytes()));
    }
    let up = r
        .acts
        .upload_bundle(app, owner, zip_of(&files))
        .await
        .unwrap();
    let id: i64 = up.bundle.id.parse().unwrap();
    r.acts.submit(app, id).await.unwrap();
    r.acts.decide(app, 1, id, true, None).await.unwrap();
    set_state(r, app, "released").await;
    app
}

async fn set_state(r: &Rig, app: i64, state: &str) {
    sqlx::query("UPDATE applications SET release_state = $2::app_release_state WHERE id = $1")
        .bind(app)
        .bind(state)
        .execute(r.db.pool())
        .await
        .unwrap();
}

const T1: &str = r#"{"tier":"TIER_1_ORIGINAL","capabilities":["datachannel"],"tip_links":[{"kind":"kofi","url":"https://ko-fi.com/dev"}]}"#;
const T2: &str = r#"{"tier":"TIER_2_REIMPLEMENTATION"}"#;

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_bundle_keeps_the_tier_and_tips_its_manifest_declared() {
    let r = rig().await;
    let owner = account(r.db.pool()).await;
    let app = application(r.db.pool(), owner, "Pong").await;
    r.acts.save_config(app, input()).await.unwrap();
    let up = r
        .acts
        .upload_bundle(
            app,
            owner,
            zip_of(&[("index.html", b"<p>x"), ("sideroom.json", T1.as_bytes())]),
        )
        .await
        .unwrap();
    assert_eq!(up.bundle.tier.as_deref(), Some("TIER_1_ORIGINAL"));
    assert_eq!(up.bundle.tip_links.len(), 1);
    assert_eq!(up.bundle.tip_links[0].kind, "kofi");

    // A third tier or a tip host off the list never reaches the database: the upload is refused by name.
    for body in [
        r#"{"tier":"TIER_3_FAN_IP"}"#,
        r#"{"tier":"TIER_1_ORIGINAL","tip_links":[{"kind":"kofi","url":"https://evil.example/x"}]}"#,
    ] {
        let e = r
            .acts
            .upload_bundle(
                app,
                owner,
                zip_of(&[("index.html", b"<p>y"), ("sideroom.json", body.as_bytes())]),
            )
            .await
            .unwrap_err();
        match e {
            ActivityError::Invalid("bundle", m) => assert!(m.contains("sideroom.json"), "{m}"),
            other => panic!("{other:?}"),
        }
    }
    r.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_database_holds_only_the_two_tiers() {
    let r = rig().await;
    let owner = account(r.db.pool()).await;
    let res = sqlx::query(
        "INSERT INTO arcade_coming_soon (id, title, summary, tier, repo_url, created_by) \
         VALUES ($1, 'Fan', 'x', 'TIER_3_FAN_IP', 'https://github.com/x/y', $2)",
    )
    .bind(uid())
    .bind(owner)
    .execute(r.db.pool())
    .await;
    assert!(res.is_err());
    let g = uid();
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Guild', $2)")
        .bind(g)
        .bind(owner)
        .execute(r.db.pool())
        .await
        .unwrap();
    let res = sqlx::query(
        "INSERT INTO arcade_guild_cartridges (id, guild_id, title, kind, tier, source_ref, added_by) \
         VALUES ($1, $2, 'Fan', 'game', 'TIER_3_FAN_IP', 'a', $3)",
    )
    .bind(uid())
    .bind(g)
    .bind(owner)
    .execute(r.db.pool())
    .await;
    assert!(res.is_err());
    r.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_operator_features_games_that_declare_a_tier() {
    let r = rig().await;
    let op = account(r.db.pool()).await;
    let one = game(&r, "Land", Some(T1)).await;
    let two = game(&r, "Horizon", Some(T2)).await;
    let legacy = game(&r, "Old", None).await;
    let viewer = account(r.db.pool()).await;

    // A build that declares no tier cannot be featured, and neither can an unknown application.
    assert!(matches!(
        r.arcade.feature(op, legacy, FeatureInput::default()).await,
        Err(ActivityError::Invalid("application_id", _))
    ));
    assert!(matches!(
        r.arcade.feature(op, uid(), FeatureInput::default()).await,
        Err(ActivityError::NotFound(_))
    ));
    assert!(r.arcade.front(viewer).await.unwrap().featured.is_empty());

    let c = r
        .arcade
        .feature(
            op,
            two,
            FeatureInput {
                position: Some(2),
                blurb: Some("Clean-room engine".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(c.tier, Tier::Reimplementation);
    assert!(c.eligibility.desktop_only && !c.eligibility.browser_play);
    let c = r
        .arcade
        .feature(
            op,
            one,
            FeatureInput {
                position: Some(1),
                blurb: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(c.tier, Tier::Original);
    assert_eq!(c.capabilities, vec!["datachannel"]);
    assert_eq!(c.tip_links[0].url, "https://ko-fi.com/dev");
    assert_eq!(c.bundle_sha256.len(), 64);

    let names: Vec<String> = r
        .arcade
        .front(viewer)
        .await
        .unwrap()
        .featured
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert_eq!(names, ["Land", "Horizon"]);

    // Featuring again changes the place, not the count.
    r.arcade
        .feature(
            op,
            one,
            FeatureInput {
                position: Some(5),
                blurb: None,
            },
        )
        .await
        .unwrap();
    let names: Vec<String> = r
        .arcade
        .featured()
        .await
        .unwrap()
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert_eq!(names, ["Horizon", "Land"]);

    // The candidates list shows tiered games and which are featured.
    let cands = r.arcade.candidates().await.unwrap();
    assert_eq!(cands.len(), 2);
    assert!(cands.iter().all(|c| c.featured));

    // A suspended application drops off the Front Page by itself; unfeature removes the row.
    set_state(&r, two, "suspended").await;
    assert_eq!(r.arcade.featured().await.unwrap().len(), 1);
    assert!(r.arcade.unfeature(one).await.unwrap());
    assert!(!r.arcade.unfeature(one).await.unwrap());
    assert!(r.arcade.featured().await.unwrap().is_empty());

    // A blurb is one short line.
    assert!(matches!(
        r.arcade
            .feature(
                op,
                one,
                FeatureInput {
                    position: None,
                    blurb: Some("x".repeat(121))
                }
            )
            .await,
        Err(ActivityError::Invalid("blurb", _))
    ));
    r.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_tier_two_game_will_not_launch_in_the_browser_and_reserved_flags_are_not_handed_out() {
    let r = rig().await;
    let member = account(r.db.pool()).await;
    let two = game(&r, "Horizon", Some(T2)).await;
    assert!(r.arcade.is_desktop_only(two).await.unwrap());
    match r.acts.launch(member, two, 1).await {
        Err(ActivityError::Conflict(m)) => assert!(m.contains("desktop"), "{m}"),
        other => panic!("{other:?}"),
    }

    let one = game(
        &r,
        "Land",
        Some(
            r#"{"tier":"TIER_1_ORIGINAL","capabilities":["datachannel","guest_play","ingame_voice","asset_cache"]}"#,
        ),
    )
    .await;
    assert!(!r.arcade.is_desktop_only(one).await.unwrap());
    let inst = r.acts.launch(member, one, 1).await.unwrap();
    // ARC-GST-001: guest_play is enforced now; ingame_voice and asset_cache are accepted but never handed out.
    assert_eq!(inst.capabilities, vec!["datachannel", "guest_play"]);
    r.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn coming_soon_has_a_per_member_notify_toggle() {
    let r = rig().await;
    let op = account(r.db.pool()).await;
    let (a, b) = (account(r.db.pool()).await, account(r.db.pool()).await);
    let entry = ComingSoonInput {
        title: "OpenMW".into(),
        summary: "A clean-room engine.".into(),
        tier: Tier::Reimplementation,
        repo_url: "https://github.com/OpenMW/openmw".into(),
    };
    let card = r.arcade.add_coming_soon(op, entry.clone()).await.unwrap();
    assert!(!card.notify);
    let id: i64 = card.id.parse().unwrap();

    r.arcade.set_notify(a, id, true).await.unwrap();
    r.arcade.set_notify(a, id, true).await.unwrap();
    let seen_a = r.arcade.coming_soon(a).await.unwrap();
    let seen_b = r.arcade.coming_soon(b).await.unwrap();
    assert!(seen_a[0].notify && !seen_b[0].notify);
    assert_eq!(r.arcade.watchers(id).await.unwrap(), vec![a]);
    r.arcade.set_notify(a, id, false).await.unwrap();
    assert!(r.arcade.watchers(id).await.unwrap().is_empty());
    assert!(matches!(
        r.arcade.set_notify(a, uid(), true).await,
        Err(ActivityError::NotFound(_))
    ));

    // A repo link must be plain https.
    for bad in [
        "http://github.com/x/y",
        "https://u:p@github.com/x/y",
        "nope",
    ] {
        let e = r
            .arcade
            .add_coming_soon(
                op,
                ComingSoonInput {
                    repo_url: bad.into(),
                    ..entry.clone()
                },
            )
            .await;
        assert!(
            matches!(e, Err(ActivityError::Invalid("repo_url", _))),
            "{bad}"
        );
    }

    // Removing an entry takes the toggles with it.
    r.arcade.set_notify(b, id, true).await.unwrap();
    assert!(r.arcade.remove_coming_soon(id).await.unwrap());
    assert!(!r.arcade.remove_coming_soon(id).await.unwrap());
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM arcade_coming_soon_notify")
        .fetch_one(r.db.pool())
        .await
        .unwrap();
    assert_eq!(left, 0);

    // FU-3: releasing hands back the title and the watchers and removes the entry and its opt-ins.
    let next = r.arcade.add_coming_soon(op, entry.clone()).await.unwrap();
    let next_id: i64 = next.id.parse().unwrap();
    r.arcade.set_notify(a, next_id, true).await.unwrap();
    r.arcade.set_notify(b, next_id, true).await.unwrap();
    let (title, who) = r
        .arcade
        .release_coming_soon(next_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(title, entry.title);
    assert_eq!(who, vec![a.min(b), a.max(b)]);
    assert!(r.arcade.coming_soon(a).await.unwrap().is_empty());
    assert!(
        r.arcade
            .release_coming_soon(next_id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(r.arcade.watchers(next_id).await.unwrap().is_empty());
    r.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn guild_cartridges_are_per_server_and_follow_the_manifest_rules() {
    let r = rig().await;
    let user = account(r.db.pool()).await;
    let (g1, g2) = (uid(), uid());
    for g in [g1, g2] {
        sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Guild', $2)")
            .bind(g)
            .bind(user)
            .execute(r.db.pool())
            .await
            .unwrap();
    }
    let cart = |tier: Tier, caps: &[&str], tips: Vec<TipInput>| CartridgeInput {
        title: "Pong".into(),
        description: "Two paddles".into(),
        kind: "game".into(),
        tier,
        source_ref: "shelf-1/games/pong.zip".into(),
        shelf_id: None,
        object_hash: None,
        capabilities: caps.iter().map(|c| (*c).to_owned()).collect(),
        tip_links: tips,
    };
    let v = r
        .arcade
        .add_cartridge(
            g1,
            user,
            cart(
                Tier::Original,
                &["guest_play", "datachannel", "datachannel"],
                vec![TipInput {
                    kind: "patreon".into(),
                    url: "https://www.patreon.com/dev".into(),
                }],
            ),
        )
        .await
        .unwrap();
    assert_eq!(v.capabilities, vec!["datachannel", "guest_play"]);
    assert_eq!(v.tip_links[0].kind, "patreon");
    assert_eq!(r.arcade.cartridges(g1).await.unwrap().len(), 1);
    assert!(r.arcade.cartridges(g2).await.unwrap().is_empty());
    // Unlisted: nothing about it reaches the Front Page.
    assert!(r.arcade.front(user).await.unwrap().featured.is_empty());

    // The same rules as a manifest: a flag the tier does not allow, an unknown flag, a tip off the list,
    // an unknown kind, a path that climbs.
    let bad = [
        cart(Tier::Reimplementation, &["guest_play"], vec![]),
        cart(Tier::Original, &["root"], vec![]),
        cart(
            Tier::Original,
            &[],
            vec![TipInput {
                kind: "kofi".into(),
                url: "https://evil.example/a".into(),
            }],
        ),
        CartridgeInput {
            kind: "app".into(),
            ..cart(Tier::Original, &[], vec![])
        },
        CartridgeInput {
            source_ref: "../etc".into(),
            ..cart(Tier::Original, &[], vec![])
        },
    ];
    for b in bad {
        assert!(matches!(
            r.arcade.add_cartridge(g1, user, b).await,
            Err(ActivityError::Invalid(..))
        ));
    }

    // Removal is scoped to the server.
    let id: i64 = v.id.parse().unwrap();
    assert!(!r.arcade.remove_cartridge(g2, id).await.unwrap());
    assert!(r.arcade.remove_cartridge(g1, id).await.unwrap());
    assert!(r.arcade.cartridges(g1).await.unwrap().is_empty());
    r.db.cleanup().await.unwrap();
}

/// Wave 42 integration: a cartridge names a catalog object of a wave-41 shelf (`shelf_id`, `object_hash`). Only an
/// object on a shelf the lister owns, the server owns or that is shared with one of the server's roles, not
/// delisted; a delisting later takes it out of the listing (P28-LEG-1).
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_cartridge_names_a_shelf_object_and_a_delisting_hides_it() {
    let r = rig().await;
    let pool = r.db.pool();
    let user = account(pool).await;
    let stranger = account(pool).await;
    let guild = uid();
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Guild', $2)")
        .bind(guild)
        .bind(user)
        .execute(pool)
        .await
        .unwrap();
    let (mine, theirs) = (uid(), uid());
    for (id, owner) in [(mine, user), (theirs, stranger)] {
        sqlx::query(
            "INSERT INTO storage_mounts (id, owner_kind, owner_id, label, provider, endpoint, bucket, access_key_id, secret_key_enc) \
             VALUES ($1, 'user', $2, 'm', 'garage', 'http://127.0.0.1:3900', 'b', 'AK', $3)",
        )
        .bind(id)
        .bind(owner)
        .bind(vec![0u8])
        .execute(pool)
        .await
        .unwrap();
    }
    let hash = "c".repeat(64);
    for shelf in [mine, theirs] {
        sqlx::query(
            "INSERT INTO shelf_objects (shelf_id, object_hash, path, kind) VALUES ($1, $2, 'games/pong.zip', 'games')",
        )
        .bind(shelf)
        .bind(&hash)
        .execute(pool)
        .await
        .unwrap();
    }
    let cart = |shelf: Option<i64>, h: Option<&str>| CartridgeInput {
        title: "Pong".into(),
        description: String::new(),
        kind: "game".into(),
        tier: Tier::Original,
        source_ref: String::new(),
        shelf_id: shelf.map(|s| s.to_string()),
        object_hash: h.map(str::to_owned),
        capabilities: vec![],
        tip_links: vec![],
    };
    let v = r
        .arcade
        .add_cartridge(guild, user, cart(Some(mine), Some(&hash)))
        .await
        .unwrap();
    assert_eq!(v.shelf_id.as_deref(), Some(mine.to_string().as_str()));
    assert_eq!(v.object_hash.as_deref(), Some(hash.as_str()));
    assert_eq!(v.source_ref, format!("shelf:{mine}/{hash}"));

    // Someone else's shelf that is not shared with this server, a missing object, half a pair, a bad hash.
    for bad in [
        cart(Some(theirs), Some(&hash)),
        cart(Some(mine), Some(&"d".repeat(64))),
        cart(Some(mine), None),
        cart(Some(mine), Some("ABC")),
    ] {
        assert!(matches!(
            r.arcade.add_cartridge(guild, user, bad).await,
            Err(ActivityError::Invalid(..))
        ));
    }
    // Shared with a role of this server: accepted.
    sqlx::query(
        "INSERT INTO shelf_grants (id, shelf_id, grantee_kind, grantee_id, guild_id, granted_by) \
         VALUES ($1, $2, 'guild_role', $3, $3, $4)",
    )
    .bind(uid())
    .bind(theirs)
    .bind(guild)
    .bind(stranger)
    .execute(pool)
    .await
    .unwrap();
    r.arcade
        .add_cartridge(guild, user, cart(Some(theirs), Some(&hash)))
        .await
        .unwrap();
    assert_eq!(r.arcade.cartridges(guild).await.unwrap().len(), 2);

    // Staff delist the object on my shelf: its cartridge leaves the listing, and it cannot be listed again.
    let strike = uid();
    sqlx::query(
        "INSERT INTO shelf_delist_strikes (id, shelf_id, object_hash, path, subject_account, reason, delisted_by) \
         VALUES ($1, $2, $3, 'games/pong.zip', $4, 'test', $4)",
    )
    .bind(strike)
    .bind(mine)
    .bind(&hash)
    .bind(user)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE shelf_objects SET delisted_at = now(), delist_strike_id = $2 WHERE shelf_id = $1",
    )
    .bind(mine)
    .bind(strike)
    .execute(pool)
    .await
    .unwrap();
    let left = r.arcade.cartridges(guild).await.unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(
        left[0].shelf_id.as_deref(),
        Some(theirs.to_string().as_str())
    );
    assert!(
        r.arcade
            .add_cartridge(guild, user, cart(Some(mine), Some(&hash)))
            .await
            .is_err()
    );
    r.db.cleanup().await.unwrap();
}
