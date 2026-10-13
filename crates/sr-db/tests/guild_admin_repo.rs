//! Postgres-backed guild administration tests. Run with a scratch database:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test guild_admin_repo -- --ignored`

use std::sync::Arc;

use sr_chat::admin_rules::{ChannelMove, GuildPatch, MemberPatch};
use sr_chat::role_rules::NewRole;
use sr_chat::{ChatError, ChatLimits, ChatService};
use sr_core::permissions::{Overwrite, OverwriteKind, Permissions};
use sr_core::{Clock, SnowflakeGenerator, SystemClock, UserId};
use sr_db::chat_repo::PgChatRepo;
use sr_db::{DbConfig, PgPool};

async fn pool() -> PgPool {
    let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
    let pool = sr_db::connect(&DbConfig::new(url)).await.expect("connect");
    sr_db::migrate(&pool).await.expect("migrate");
    pool
}

type Ids = SnowflakeGenerator<Arc<dyn Clock>>;

fn service(pool: &PgPool) -> (ChatService<PgChatRepo>, Ids) {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let svc_ids = SnowflakeGenerator::new(clock.clone(), 5, 5).expect("ids");
    let test_ids = SnowflakeGenerator::new(clock.clone(), 6, 6).expect("ids");
    (
        ChatService::new(
            PgChatRepo::new(pool.clone()),
            clock,
            svc_ids,
            ChatLimits::default(),
        ),
        test_ids,
    )
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
        "INSERT INTO accounts (id, person_id, username, email, password_hash) \
         VALUES ($1, $2, $3, $4, 'x')",
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

async fn join(pool: &PgPool, guild: sr_core::GuildId, user: UserId) {
    sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
        .bind(guild.get() as i64)
        .bind(user.get() as i64)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn guild_admin_round_trip() {
    let pool = pool().await;
    let (svc, ids) = service(&pool);
    let owner = account(&pool, &ids).await;
    let bob = account(&pool, &ids).await;
    let carol = account(&pool, &ids).await;
    let bp = svc.create_guild(owner, "Admin Test").await.unwrap();
    let gid = bp.guild.id;
    join(&pool, gid, bob).await;
    join(&pool, gid, carol).await;

    // Guild edit, listing, members.
    let g = svc
        .edit_guild(
            owner,
            gid,
            GuildPatch {
                name: Some("Admin Renamed".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(g.name, "Admin Renamed");
    assert_eq!(svc.list_my_guilds(bob).await.unwrap().len(), 1);
    assert_eq!(
        svc.list_members(owner, gid, Default::default(), None, 10)
            .await
            .unwrap()
            .len(),
        3
    );

    // Roles: create, assign, reorder.
    let role = svc
        .create_role(
            owner,
            gid,
            NewRole {
                name: Some("mods".into()),
                permissions: Some(Permissions::KICK_MEMBERS | Permissions::BAN_MEMBERS),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    svc.assign_role(owner, gid, bob, role.id, true)
        .await
        .unwrap();
    let m = svc.get_member_view(owner, gid, bob).await.unwrap();
    assert_eq!(m.role_ids, vec![role.id]);
    svc.edit_member(
        bob,
        gid,
        carol,
        MemberPatch {
            nick: Some(Some("c".into())),
            ..Default::default()
        },
    )
    .await
    .unwrap_err();

    // Overwrites and layout.
    let general = bp.channels[2].id;
    let ow = Overwrite {
        id: gid.snowflake(),
        kind: OverwriteKind::Role,
        allow: Permissions::empty(),
        deny: Permissions::SEND_MESSAGES,
    };
    let view = svc.set_overwrite(owner, general, ow).await.unwrap();
    assert_eq!(view.overwrites, vec![ow]);
    let moved = svc
        .reorder_channels(
            owner,
            gid,
            vec![(
                general,
                ChannelMove {
                    position: Some(3),
                    ..Default::default()
                },
            )],
        )
        .await
        .unwrap();
    assert_eq!(moved[0].channel.position, 3);
    svc.delete_overwrite(owner, general, gid.snowflake())
        .await
        .unwrap();

    // Kick and ban.
    svc.kick_member(bob, gid, carol).await.unwrap();
    let (ban, was_member) = svc
        .ban_member(owner, gid, bob, Some("x".into()))
        .await
        .unwrap();
    assert!(was_member);
    assert_eq!(
        svc.get_ban(owner, gid, bob).await.unwrap().reason,
        ban.reason
    );
    svc.unban(owner, gid, bob).await.unwrap();
    assert!(matches!(
        svc.unban(owner, gid, bob).await,
        Err(ChatError::UnknownBan)
    ));

    // Delete.
    svc.delete_guild(owner, gid).await.unwrap();
    assert!(matches!(
        svc.get_guild(owner, gid).await,
        Err(ChatError::UnknownGuild)
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn member_count_and_automod_style_timeout_round_trip() {
    use sr_chat::admin_repo::GuildAdminRepository;
    use sr_chat::repo::GuildRepository;

    let pool = pool().await;
    let (svc, ids) = service(&pool);
    let owner = account(&pool, &ids).await;
    let bob = account(&pool, &ids).await;
    let bp = svc.create_guild(owner, "Count Test").await.unwrap();
    let gid = bp.guild.id;
    let repo = PgChatRepo::new(pool.clone());
    assert_eq!(repo.count_members(gid).await.unwrap(), 1);
    join(&pool, gid, bob).await;
    assert_eq!(repo.count_members(gid).await.unwrap(), 2);

    // The write the AutoMod timeout performs: read, set the deadline, update.
    let until = 4_000_000_000_000_i64;
    let mut m = repo.get_member(gid, bob).await.unwrap().unwrap();
    assert_eq!(m.communication_disabled_until_ms, None);
    m.communication_disabled_until_ms = Some(until);
    repo.update_member(&m).await.unwrap();
    let m = repo.get_member(gid, bob).await.unwrap().unwrap();
    assert_eq!(m.communication_disabled_until_ms, Some(until));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn member_search_role_filter_and_prune() {
    use sr_chat::SendMessage;
    use sr_chat::admin_repo::MemberFilter;
    let pool = pool().await;
    let (svc, ids) = service(&pool);
    let [owner, bob, carol, dave, erin] = [
        account(&pool, &ids).await,
        account(&pool, &ids).await,
        account(&pool, &ids).await,
        account(&pool, &ids).await,
        account(&pool, &ids).await,
    ];
    let bp = svc.create_guild(owner, "Prune Test").await.unwrap();
    let (gid, general) = (bp.guild.id, bp.channels[2].id);
    for u in [bob, carol, dave, erin] {
        join(&pool, gid, u).await;
    }
    let tag = format!("zq{}", bob.get() % 100_000);
    sqlx::query("UPDATE accounts SET display_name = $2 WHERE id = $1")
        .bind(bob.get() as i64)
        .bind(format!("Robert {tag}%_"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE guild_members SET nick = $3 WHERE guild_id = $1 AND user_id = $2")
        .bind(gid.get() as i64)
        .bind(carol.get() as i64)
        .bind(format!("Caro {tag}"))
        .execute(&pool)
        .await
        .unwrap();
    let listed = |f: MemberFilter| {
        let svc = &svc;
        async move {
            svc.list_members(owner, gid, f, None, 100)
                .await
                .unwrap()
                .iter()
                .map(|m| m.user_id)
                .collect::<Vec<_>>()
        }
    };
    let q = |s: &str| MemberFilter {
        query: Some(s.into()),
        role: None,
    };
    // Display name and nickname, case-insensitive; LIKE metacharacters are literal.
    assert_eq!(listed(q(&tag.to_uppercase())).await, vec![bob, carol]);
    assert_eq!(listed(q(&format!("{tag}%_"))).await, vec![bob]);
    assert_eq!(listed(q(&format!("{tag}_%"))).await, Vec::<UserId>::new());
    // A bare "%" or "_" matches only the literal character (only Bob has them).
    assert_eq!(listed(q("%")).await, vec![bob]);
    assert_eq!(listed(q("_")).await, vec![bob]);
    // Username search.
    let uname: String = sqlx::query_scalar("SELECT username::text FROM accounts WHERE id = $1")
        .bind(dave.get() as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(listed(q(&uname)).await, vec![dave]);

    // Role filter.
    let role = svc
        .create_role(
            owner,
            gid,
            NewRole {
                name: Some("crew".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    svc.assign_role(owner, gid, dave, role.id, true)
        .await
        .unwrap();
    let by_role = MemberFilter {
        query: None,
        role: Some(role.id),
    };
    assert_eq!(listed(by_role).await, vec![dave]);
    let everyone = MemberFilter {
        query: None,
        role: Some(sr_core::RoleId::new(gid.get())),
    };
    assert_eq!(listed(everyone).await.len(), 5);
    assert_eq!(
        svc.list_role_members(owner, gid, role.id, None, 10)
            .await
            .unwrap()
            .len(),
        1
    );

    // Prune: Bob, Carol, Dave and Erin joined 40 days ago; Carol posts; Dave has a role.
    sqlx::query(
        "UPDATE guild_members SET joined_at = now() - interval '40 days' \
         WHERE guild_id = $1 AND user_id <> $2",
    )
    .bind(gid.get() as i64)
    .bind(erin.get() as i64)
    .execute(&pool)
    .await
    .unwrap();
    svc.send_message(
        carol,
        general,
        SendMessage {
            content: "still around".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(svc.prune_preview(owner, gid, 7, &[]).await.unwrap(), 1); // Bob only
    assert_eq!(
        svc.prune_preview(owner, gid, 7, &[role.id]).await.unwrap(),
        2
    );
    assert!(matches!(
        svc.prune_preview(bob, gid, 7, &[]).await,
        Err(ChatError::MissingPermissions)
    ));
    let removed = svc.prune(owner, gid, 7, &[]).await.unwrap();
    assert_eq!(removed, vec![bob]);
    let left = listed(MemberFilter::default()).await;
    assert_eq!(left.len(), 4);
    assert!(!left.contains(&bob));
    assert!(left.contains(&owner), "the owner is never pruned");
    // Removing nobody is fine.
    assert_eq!(svc.prune(owner, gid, 7, &[]).await.unwrap(), vec![]);
}
