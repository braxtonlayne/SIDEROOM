//! DB-backed tests for the profile fields and badges (LNG-PRF-001/002). Run with a scratch
//! Postgres: `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test profile_be -- --ignored`

use sr_chat::social::profile::{Connection, Profile};
use sr_chat::social::repo::{ProfileRepository, RelationshipRepository};
use sr_core::{GuildId, UserId};
use sr_db::PgPool;
use sr_db::social_repo::PgSocialRepo;
use sr_db::testing::TestDb;

async fn account(pool: &PgPool, id: i64, name: &str) {
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) \
         VALUES ($1, $1, $2, $3, 'x')",
    )
    .bind(id)
    .bind(name)
    .bind(format!("{name}@example.test"))
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn profile_fields_round_trip_and_badges_are_derived() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    let repo = PgSocialRepo::new(pool.clone());
    let (me, plain) = (UserId::new(11), UserId::new(12));
    account(pool, 11, "badged").await;
    account(pool, 12, "plain").await;

    let mut p = repo.get_profile(me).await.unwrap().unwrap();
    assert!(p.badges.is_empty() && p.avatar_decoration.is_none() && p.theme_colors.is_none());
    assert!(
        p.created_ms > 1_600_000_000_000,
        "member since is the creation time"
    );
    p.avatar_decoration = Some("neon-halo".into());
    p.profile_effect = Some("snow".into());
    p.theme_colors = Some([0x00FF_0000, 0x0000_00FF]);
    repo.put_profile(&p).await.unwrap();
    let back = repo.get_profile(me).await.unwrap().unwrap();
    assert_eq!(back.avatar_decoration.as_deref(), Some("neon-halo"));
    assert_eq!(back.profile_effect.as_deref(), Some("snow"));
    assert_eq!(back.theme_colors, Some([0x00FF_0000, 0x0000_00FF]));

    // Badges: flags, an open staff appointment, the staff guild's owner, a live patron grant.
    sqlx::query("UPDATE accounts SET flags = (1 << 8) | (1 << 9) | (1 << 1) WHERE id = 11")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO staff_appointments (id, account_id, appointed_by) VALUES (1, 11, 12)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO guilds (id, name, owner_id, is_staff_guild) VALUES (50, 'Staff', 11, TRUE)",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO entitlement_grants (id, kind, user_id, source, permanent, starts_ms) \
         VALUES (60, 'patron', 11, 'admin', TRUE, 0)",
    )
    .execute(pool)
    .await
    .unwrap();
    let badges = repo.get_profile(me).await.unwrap().unwrap().badges;
    assert_eq!(
        badges,
        [
            "owner",
            "staff",
            "patron",
            "contributor",
            "early_member",
            "app_developer"
        ]
    );
    // A donor can hide the patron badge.
    sqlx::query("INSERT INTO donor_preferences (user_id, hide_badge) VALUES (11, TRUE)")
        .execute(pool)
        .await
        .unwrap();
    assert!(
        !repo
            .get_profile(me)
            .await
            .unwrap()
            .unwrap()
            .badges
            .contains(&"patron".to_owned())
    );
    // G-25: `staff.show_badge = false` hides the staff and owner badges.
    sqlx::query(
        "INSERT INTO user_settings (user_id, settings, version)          VALUES (11, '{\"staff\": {\"show_badge\": false}}', 1)",
    )
    .execute(pool)
    .await
    .unwrap();
    let hidden = repo.get_profile(me).await.unwrap().unwrap().badges;
    assert!(!hidden.contains(&"staff".to_owned()) && !hidden.contains(&"owner".to_owned()));
    assert!(hidden.contains(&"contributor".to_owned()));
    assert!(
        repo.get_profile(plain)
            .await
            .unwrap()
            .unwrap()
            .badges
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn memberships_carry_the_per_server_profile() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    let repo = PgSocialRepo::new(pool.clone());
    account(pool, 21, "member").await;
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES (70, 'Room', 21)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO guild_members (guild_id, user_id, nick, avatar_key, banner_key, bio) \
         VALUES (70, 21, 'Nick', 'av', 'bn', 'server bio')",
    )
    .execute(pool)
    .await
    .unwrap();
    let m = repo
        .list_guild_memberships(UserId::new(21))
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(m.guild, GuildId::new(70));
    assert_eq!(
        (
            m.nick.as_deref(),
            m.avatar_key.as_deref(),
            m.banner_key.as_deref(),
            m.bio.as_deref()
        ),
        (Some("Nick"), Some("av"), Some("bn"), Some("server bio"))
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn connection_extras_persist() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    let repo = PgSocialRepo::new(pool.clone());
    account(pool, 31, "linker").await;
    repo.upsert_connection(&Connection {
        user: UserId::new(31),
        provider: "github".into(),
        external_id: "42".into(),
        display_name: "octo".into(),
        verified: true,
        visible: true,
        url: Some("https://github.com/octo".into()),
        show_activity: true,
        stats: serde_json::json!({"followers": 7}),
        created_ms: 1,
    })
    .await
    .unwrap();
    let p: Profile = repo.get_profile(UserId::new(31)).await.unwrap().unwrap();
    let c = &p.connections[0];
    assert_eq!(c.url.as_deref(), Some("https://github.com/octo"));
    assert!(c.show_activity && c.verified);
    assert_eq!(c.stats["followers"], 7);
}
