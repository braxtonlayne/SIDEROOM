//! Postgres-backed tests for server traits and role colour gradients (J-07, J-08). Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test p14_profile -- --ignored`

use std::sync::Arc;

use sr_chat::admin_rules::GuildPatch;
use sr_chat::guild_traits::GuildTrait;
use sr_chat::role_rules::{HOLOGRAPHIC, NewRole, RoleColors, RolePatch};
use sr_chat::{ChatLimits, ChatService};
use sr_core::{Clock, GuildId, SnowflakeGenerator, SystemClock, UserId};
use sr_db::PgPool;
use sr_db::chat_repo::PgChatRepo;
use sr_db::testing::TestDb;

type Ids = SnowflakeGenerator<Arc<dyn Clock>>;

fn ids(worker: u8) -> Ids {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    SnowflakeGenerator::new(clock, worker, worker).expect("ids")
}

async fn account(pool: &PgPool, ids: &Ids) -> UserId {
    let person = ids.generate().get() as i64;
    let acct = ids.generate();
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(person)
        .execute(pool)
        .await
        .unwrap();
    let name = format!("u{}", acct.get());
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $2, $3, $4, 'x')",
    )
    .bind(acct.get() as i64)
    .bind(person)
    .bind(&name[..name.len().min(32)])
    .bind(format!("{name}@example.test"))
    .execute(pool)
    .await
    .unwrap();
    UserId::new(acct.get())
}

async fn setup(pool: &PgPool) -> (ChatService<PgChatRepo>, UserId, GuildId) {
    let test_ids = ids(8);
    let owner = account(pool, &test_ids).await;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let svc = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock,
        ids(5),
        ChatLimits::default(),
    );
    let g = svc
        .create_guild(owner, "Profile Club")
        .await
        .unwrap()
        .guild
        .id;
    (svc, owner, g)
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn role_gradient_stops_round_trip_and_the_tertiary_rule_is_a_constraint() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let (svc, owner, g) = setup(&pool).await;

    let role = svc
        .create_role(
            owner,
            g,
            NewRole {
                name: Some("holo".into()),
                colors: Some(HOLOGRAPHIC),
                ..NewRole::default()
            },
        )
        .await
        .unwrap();
    let listed = svc.list_roles(owner, g).await.unwrap();
    let found = listed.iter().find(|r| r.id == role.id).unwrap();
    assert_eq!(
        (found.color, found.secondary_color, found.tertiary_color),
        (11_127_295, Some(16_759_788), Some(16_761_760))
    );

    // Two stops, then back to a plain colour.
    svc.edit_role(
        owner,
        g,
        role.id,
        RolePatch {
            colors: Some(RoleColors {
                primary: 1,
                secondary: Some(2),
                tertiary: None,
            }),
            ..RolePatch::default()
        },
    )
    .await
    .unwrap();
    let plain = svc
        .edit_role(
            owner,
            g,
            role.id,
            RolePatch {
                colors: Some(RoleColors {
                    primary: 3,
                    secondary: None,
                    tertiary: None,
                }),
                ..RolePatch::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        (plain.color, plain.secondary_color, plain.tertiary_color),
        (3, None, None)
    );

    // The database refuses a tertiary stop without a secondary one, and out-of-range stops.
    let id = role.id.get() as i64;
    for sql in [
        "UPDATE roles SET secondary_color = NULL, tertiary_color = 5 WHERE id = $1",
        "UPDATE roles SET tertiary_color = 16777216, secondary_color = 1 WHERE id = $1",
    ] {
        assert!(
            sqlx::query(sql).bind(id).execute(&pool).await.is_err(),
            "{sql}"
        );
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn server_traits_round_trip_and_the_list_is_capped_by_a_constraint() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let (svc, owner, g) = setup(&pool).await;
    assert!(svc.get_guild(owner, g).await.unwrap().0.traits.is_empty());

    let traits = vec![
        GuildTrait {
            emoji_name: Some("🎮".into()),
            label: "Gamers".into(),
            ..GuildTrait::default()
        },
        GuildTrait {
            emoji_id: Some("123456789".into()),
            emoji_name: Some("blob".into()),
            label: "Cozy".into(),
        },
    ];
    svc.edit_guild(
        owner,
        g,
        GuildPatch {
            traits: Some(traits.clone()),
            ..GuildPatch::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(svc.get_guild(owner, g).await.unwrap().0.traits, traits);

    // An edit that leaves traits out keeps them.
    svc.edit_guild(
        owner,
        g,
        GuildPatch {
            name: Some("Renamed Club".into()),
            ..GuildPatch::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(svc.get_guild(owner, g).await.unwrap().0.traits, traits);

    // Six entries and a non-array never reach the table.
    let id = g.get() as i64;
    for json in ["[1,2,3,4,5,6]", "{}", "\"x\""] {
        let r = sqlx::query("UPDATE guilds SET traits = $2::jsonb WHERE id = $1")
            .bind(id)
            .bind(json)
            .execute(&pool)
            .await;
        assert!(r.is_err(), "{json}");
    }
}
