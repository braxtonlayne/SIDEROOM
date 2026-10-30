//! PRF-PRO-001 (D-382) acceptance on Postgres: every tier against every viewer rank against every block.
//! Run with a scratch Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-packages --test profile_privacy -- --ignored --test-threads=1`
//!
//! Each block carries a distinctive seeded string; the property is that the serialised page holds exactly the
//! strings of the blocks the table admits, and none of the others, in any form.

use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use serde_json::{Value, json};
use sr_db::{
    PgPool,
    accounts::{self, NewAccount},
    testing::TestDb,
};
use sr_packages::PackageError;
use sr_packages::profile_doc::BlockKind;
use sr_packages::profile_privacy::{ALL_KINDS, Preset, PrivacyPolicy, Tier};
use sr_packages::service::PackageService;
use sr_packages::service::privacy::{PrivacyUpdate, ViewAs};

static SEQ: AtomicI64 = AtomicI64::new(0);

fn uid() -> i64 {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + SEQ.fetch_add(1, Ordering::Relaxed) % 1000
}

async fn account(pool: &PgPool, name: Option<&str>) -> i64 {
    let id = uid();
    let name = name.map_or_else(|| format!("t{id}"), str::to_owned);
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

async fn username(pool: &PgPool, id: i64) -> String {
    sqlx::query_scalar("SELECT username::text FROM accounts WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn befriend(pool: &PgPool, a: i64, b: i64) {
    sqlx::query("INSERT INTO relationships (user_id, target_id, type) VALUES ($1,$2,1),($2,$1,1)")
        .bind(a)
        .bind(b)
        .execute(pool)
        .await
        .unwrap();
}

async fn share_a_server(pool: &PgPool, a: i64, b: i64) {
    let g = uid();
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, $2, $3)")
        .bind(g)
        .bind(format!("srv{g}"))
        .bind(a)
        .execute(pool)
        .await
        .unwrap();
    for u in [a, b] {
        sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
            .bind(g)
            .bind(u)
            .execute(pool)
            .await
            .unwrap();
    }
}

struct Rig {
    _db: TestDb,
    pool: PgPool,
    svc: PackageService,
}

async fn rig() -> Rig {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let svc = PackageService::new(pool.clone(), Arc::new(uid));
    Rig { _db: db, pool, svc }
}

const BIO: &str = "SEEDBIO-sentence-zq1";
const LINK: &str = "seedlinks-zq2.example";
const TRACK: &str = "SEEDTRACK-zq3";
const GUEST: &str = "SEEDGUESTBODY-zq4";
const FAV: &str = "SEEDFAV-zq5";
const CUSTOM: &str = "SEEDCUSTOM-zq6";
const SONG: &str = "SEEDSONG-zq7";
const PIC: &str = "SEEDGALLERYCAPTION-zq8";
const FRIEND: &str = "seedfriendnamezq9";

/// Distinctive string per block kind that carries one (the guestbook is read through its own route).
const SECRETS: &[(BlockKind, &str)] = &[
    (BlockKind::About, BIO),
    (BlockKind::Linktree, LINK),
    (BlockKind::NowPlaying, TRACK),
    (BlockKind::Favorites, FAV),
    (BlockKind::Custom, CUSTOM),
    (BlockKind::Song, SONG),
    (BlockKind::Gallery, PIC),
];

fn doc_with_secrets(friend_ids: &[String]) -> Value {
    json!({"version": 1, "blocks": [
        {"id": "head", "type": "header", "x": 0, "y": 0, "w": 12, "h": 2, "config": {"tagline": "SEEDHEAD-zq0"}},
        {"id": "bio", "type": "about", "x": 0, "y": 2, "w": 6, "h": 2, "config": {"text": BIO}},
        {"id": "links", "type": "linktree", "x": 6, "y": 2, "w": 6, "h": 2,
         "config": {"links": [{"label": "mine", "url": format!("https://{LINK}/x")}]}},
        {"id": "np", "type": "now_playing", "x": 0, "y": 4, "w": 6, "h": 2,
         "config": {"mode": "manual", "title": TRACK}},
        {"id": "gb", "type": "guestbook", "x": 6, "y": 4, "w": 6, "h": 2, "config": {}},
        {"id": "fav", "type": "favorites", "x": 0, "y": 6, "w": 6, "h": 2, "config": {"items": [FAV]}},
        {"id": "cust", "type": "custom", "x": 6, "y": 6, "w": 6, "h": 2, "config": {"text": CUSTOM}},
        {"id": "song", "type": "song", "x": 0, "y": 8, "w": 6, "h": 2, "config": {"title": SONG}},
        {"id": "pics", "type": "gallery", "x": 6, "y": 8, "w": 6, "h": 2,
         "config": {"items": [{"url": "https://img.example/a.png", "caption": PIC}]}},
        {"id": "top", "type": "top_friends", "x": 0, "y": 10, "w": 12, "h": 2, "config": {"user_ids": friend_ids}}
    ]})
}

const IN_DOC: [BlockKind; 10] = [
    BlockKind::Header,
    BlockKind::About,
    BlockKind::Linktree,
    BlockKind::NowPlaying,
    BlockKind::Guestbook,
    BlockKind::Favorites,
    BlockKind::Custom,
    BlockKind::Song,
    BlockKind::Gallery,
    BlockKind::TopFriends,
];

#[tokio::test]
#[ignore = "needs Postgres"]
async fn every_tier_and_rank_shows_only_what_the_member_allowed() {
    let r = rig().await;
    let owner = account(&r.pool, None).await;
    let named = account(&r.pool, Some(FRIEND)).await;
    befriend(&r.pool, owner, named).await;
    r.svc
        .set_doc(owner, &doc_with_secrets(&[named.to_string()]))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO profile_guestbook (id, profile_id, author_id, body) VALUES ($1,$2,$3,$4)",
    )
    .bind(uid())
    .bind(owner)
    .bind(named)
    .bind(GUEST)
    .execute(&r.pool)
    .await
    .unwrap();

    let member = account(&r.pool, None).await;
    let mutual = account(&r.pool, None).await;
    share_a_server(&r.pool, owner, mutual).await;
    let friend = account(&r.pool, None).await;
    befriend(&r.pool, owner, friend).await;
    let viewers: [(Option<i64>, Tier); 5] = [
        (None, Tier::Public),
        (Some(member), Tier::Members),
        (Some(mutual), Tier::MutualServers),
        (Some(friend), Tier::Friends),
        (Some(owner), Tier::OnlyMe),
    ];
    let owner_name = username(&r.pool, owner).await;

    let mut checked = 0;
    for public_possible in [true, false] {
        for page in Tier::ALL {
            for block in Tier::ALL {
                let upd = PrivacyUpdate {
                    page_tier: Some(page),
                    links_tier: Some(block),
                    top8_tier: Some(block),
                    now_playing_tier: Some(block),
                    bio_tier: Some(block),
                    guestbook_tier: Some(block),
                    media_tier: Some(block),
                    ring_tier: Some(block),
                    ..PrivacyUpdate::default()
                };
                r.svc
                    .set_privacy(owner, upd, public_possible)
                    .await
                    .unwrap();
                let policy: PrivacyPolicy = r
                    .svc
                    .privacy_row(owner)
                    .await
                    .unwrap()
                    .1
                    .effective(public_possible);
                for (viewer, rank) in viewers {
                    let got = match viewer {
                        Some(v) => r.svc.page(v, None, Some(owner), public_possible).await.ok(),
                        None => r
                            .svc
                            .public_page(&owner_name, public_possible)
                            .await
                            .ok()
                            .map(|p| p.page),
                    };
                    let tag = format!(
                        "public={public_possible} page={page:?} block={block:?} rank={rank:?}"
                    );
                    if !policy.page_visible(rank) {
                        assert!(got.is_none(), "page must be 404: {tag}");
                        continue;
                    }
                    let out = got.unwrap_or_else(|| panic!("page must be visible: {tag}"));
                    let kinds: Vec<BlockKind> = out.doc.blocks.iter().map(|b| b.kind).collect();
                    for kind in ALL_KINDS {
                        let want = IN_DOC.contains(&kind) && policy.visible(kind, rank);
                        assert_eq!(kinds.contains(&kind), want, "{kind:?}: {tag}");
                    }
                    let body = serde_json::to_string(&out).unwrap();
                    for (kind, secret) in SECRETS {
                        assert_eq!(
                            body.contains(secret),
                            policy.visible(*kind, rank),
                            "{kind:?} {secret}: {tag}"
                        );
                    }
                    assert_eq!(
                        body.contains(FRIEND),
                        policy.visible(BlockKind::TopFriends, rank),
                        "top8: {tag}"
                    );
                    assert!(!body.contains(GUEST), "guestbook body in the page: {tag}");
                    if let Some(v) = viewer {
                        let gb = r.svc.guestbook(v, owner, None, public_possible).await;
                        match (policy.visible(BlockKind::Guestbook, rank), gb) {
                            (true, Ok(list)) => {
                                assert!(list.iter().any(|e| e.body == GUEST), "{tag}")
                            }
                            (false, Err(PackageError::NotFound(_))) => {}
                            (want, other) => panic!("guestbook want={want} got={other:?}: {tag}"),
                        }
                    }
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 100, "walked {checked} combinations");

    // The owner's "view as" strip goes through the same filter.
    r.svc
        .set_privacy(
            owner,
            PrivacyUpdate {
                preset: Some(Preset::Public),
                ..PrivacyUpdate::default()
            },
            true,
        )
        .await
        .unwrap();
    let p = r
        .svc
        .preview_as(owner, ViewAs::Public, true)
        .await
        .unwrap()
        .unwrap();
    let body = serde_json::to_string(&p).unwrap();
    assert!(body.contains(BIO) && !body.contains(FRIEND));
    assert!(
        r.svc
            .preview_as(owner, ViewAs::Public, false)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn blocked_parties_see_nothing_in_every_preset() {
    let r = rig().await;
    let owner = account(&r.pool, None).await;
    let blocked_by_owner = account(&r.pool, None).await;
    let blocker = account(&r.pool, None).await;
    r.svc.set_doc(owner, &doc_with_secrets(&[])).await.unwrap();
    for (a, b) in [(owner, blocked_by_owner), (blocker, owner)] {
        sqlx::query("INSERT INTO relationships (user_id, target_id, type) VALUES ($1,$2,2)")
            .bind(a)
            .bind(b)
            .execute(&r.pool)
            .await
            .unwrap();
    }
    for preset in [
        Preset::Public,
        Preset::Members,
        Preset::Friends,
        Preset::Private,
    ] {
        r.svc
            .set_privacy(
                owner,
                PrivacyUpdate {
                    preset: Some(preset),
                    ..PrivacyUpdate::default()
                },
                true,
            )
            .await
            .unwrap();
        for v in [blocked_by_owner, blocker] {
            assert!(
                r.svc.page(v, None, Some(owner), true).await.is_err(),
                "{preset:?}"
            );
            assert!(
                r.svc.guestbook(v, owner, None, true).await.is_err(),
                "{preset:?}"
            );
            assert!(
                r.svc.sign(v, owner, "hi", true).await.is_err(),
                "{preset:?}"
            );
        }
    }
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn the_public_guestbook_holds_posts_until_the_owner_approves() {
    let r = rig().await;
    let owner = account(&r.pool, Some("pubowner")).await;
    r.svc
        .set_doc(
            owner,
            &json!({"version": 1, "blocks": [
                {"id": "gb", "type": "guestbook", "x": 0, "y": 0, "w": 12, "h": 4, "config": {}}]}),
        )
        .await
        .unwrap();
    // Public page and guestbook, but no opt-in: an anonymous post is a 404.
    r.svc
        .set_privacy(
            owner,
            PrivacyUpdate {
                page_tier: Some(Tier::Public),
                guestbook_tier: Some(Tier::Public),
                ..PrivacyUpdate::default()
            },
            true,
        )
        .await
        .unwrap();
    assert!(
        r.svc
            .sign_public("pubowner", "hello", "198.51.100.1", true)
            .await
            .is_err()
    );
    r.svc
        .set_privacy(
            owner,
            PrivacyUpdate {
                guestbook_accepts_public: Some(true),
                ..PrivacyUpdate::default()
            },
            true,
        )
        .await
        .unwrap();
    // The operator's switch off: 404 whatever the member chose.
    assert!(
        r.svc
            .sign_public("pubowner", "hello", "198.51.100.1", false)
            .await
            .is_err()
    );
    // Links are refused; plain text is held.
    assert!(matches!(
        r.svc
            .sign_public("pubowner", "see https://x.example", "198.51.100.1", true)
            .await,
        Err(PackageError::Invalid(..))
    ));
    r.svc
        .sign_public("pubowner", "hello there", "198.51.100.1", true)
        .await
        .unwrap();
    // A second anonymous fetch does not see it; the owner does, flagged pending.
    let anon = r.svc.public_page("pubowner", true).await.unwrap();
    assert!(anon.accepts_guestbook);
    assert!(anon.guestbook.unwrap().is_empty());
    let mine = r.svc.guestbook(owner, owner, None, true).await.unwrap();
    assert_eq!(mine.len(), 1);
    assert!(mine[0].pending && mine[0].author.is_none());
    // Approved posts show to everyone, and no address hash is left behind.
    r.svc
        .approve_entry(owner, mine[0].id.parse().unwrap())
        .await
        .unwrap();
    let list = r
        .svc
        .public_page("pubowner", true)
        .await
        .unwrap()
        .guestbook
        .unwrap();
    assert_eq!(list.len(), 1);
    assert!(!list[0].pending);
    let hash: Option<String> = sqlx::query_scalar("SELECT ip_hash FROM profile_guestbook")
        .fetch_one(&r.pool)
        .await
        .unwrap();
    assert!(hash.is_none());
    // Five per address per hour.
    for i in 0..5 {
        r.svc
            .sign_public("pubowner", &format!("note {i}"), "203.0.113.5", true)
            .await
            .unwrap();
    }
    assert!(matches!(
        r.svc
            .sign_public("pubowner", "note 5", "203.0.113.5", true)
            .await,
        Err(PackageError::RateLimited)
    ));
    // A guestbook narrowed to members refuses anonymous posts.
    r.svc
        .set_privacy(
            owner,
            PrivacyUpdate {
                guestbook_tier: Some(Tier::Members),
                ..PrivacyUpdate::default()
            },
            true,
        )
        .await
        .unwrap();
    assert!(
        r.svc
            .sign_public("pubowner", "again", "192.0.2.77", true)
            .await
            .is_err()
    );
}
