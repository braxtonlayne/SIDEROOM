//! Postgres-backed social tests. Run with a scratch database:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test social_repo -- --ignored`

use std::sync::Arc;

use sr_chat::social::dm::{DmAccess, DmKind, GroupEdit};
use sr_chat::social::notifications::{MuteDuration, NotificationLevel};
use sr_chat::social::presence::{CustomStatus, Status};
use sr_chat::social::profile::{Connection, ProfileEdit};
use sr_chat::social::relationships::RelationshipKind;
use sr_chat::social::repo::{DmRepository, Privacy, RelationshipRepository};
use sr_chat::social::{SocialError, SocialLimits, SocialService};
use sr_core::{Clock, MessageId, SnowflakeGenerator, SystemClock, UserId};
use sr_db::social_repo::PgSocialRepo;
use sr_db::{DbConfig, PgPool};

async fn pool() -> PgPool {
    let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
    let pool = sr_db::connect(&DbConfig::new(url)).await.expect("connect");
    sr_db::migrate(&pool).await.expect("migrate");
    pool
}

type Ids = SnowflakeGenerator<Arc<dyn Clock>>;

fn service(pool: &PgPool) -> (SocialService<PgSocialRepo>, Ids) {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    // Distinct worker/process ids keep test-created ids apart from the service's, and
    // deriving them from the OS pid keeps separate test processes (nextest runs one per
    // test) from minting the same timestamp+worker id on a reused database.
    let w = ((std::process::id() % 16) * 2) as u8;
    let svc_ids = SnowflakeGenerator::new(clock.clone(), w, w).expect("ids");
    let test_ids = SnowflakeGenerator::new(clock.clone(), w + 1, w + 1).expect("ids");
    (
        SocialService::new(
            PgSocialRepo::new(pool.clone()),
            clock,
            svc_ids,
            SocialLimits::default(),
        ),
        test_ids,
    )
}

/// Insert a person + account directly (accounts are owned by the identity crates).
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

async fn befriend(svc: &SocialService<PgSocialRepo>, a: UserId, b: UserId) {
    svc.send_friend_request(a, b).await.unwrap();
    svc.accept_friend_request(b, a).await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn relationship_lifecycle() {
    let pool = pool().await;
    let (svc, ids) = service(&pool);
    let (a, b, c) = (
        account(&pool, &ids).await,
        account(&pool, &ids).await,
        account(&pool, &ids).await,
    );
    assert!(matches!(
        svc.send_friend_request(a, UserId::new(1)).await,
        Err(SocialError::UnknownUser)
    ));
    svc.send_friend_request(a, b).await.unwrap();
    assert_eq!(
        svc.list_relationships(b).await.unwrap()[0].kind,
        RelationshipKind::PendingIncoming
    );
    svc.accept_friend_request(b, a).await.unwrap();
    svc.set_friend_nickname(a, b, Some(" Pal ")).await.unwrap();
    let rels = svc.list_relationships(a).await.unwrap();
    assert_eq!(rels[0].kind, RelationshipKind::Friend);
    assert_eq!(rels[0].nickname.as_deref(), Some("Pal"));
    // Blocking drops the friendship on both sides and the nickname with it.
    svc.block(a, b).await.unwrap();
    assert_eq!(
        svc.list_relationships(a).await.unwrap()[0].kind,
        RelationshipKind::Blocked
    );
    assert!(svc.list_relationships(b).await.unwrap().is_empty());
    assert!(matches!(
        svc.send_friend_request(b, a).await,
        Err(SocialError::BlockedByThem)
    ));
    svc.unblock(a, b).await.unwrap();
    // Friend-request sources.
    let mut p = Privacy::default();
    p.friend_requests.everyone = false;
    p.friend_requests.friends_of_friends = false;
    p.friend_requests.server_members = false;
    svc.repo().put_privacy(c, &p).await.unwrap();
    assert!(matches!(
        svc.send_friend_request(a, c).await,
        Err(SocialError::RequestsNotAccepted)
    ));
    let counts = svc.repo().counts(a).await.unwrap();
    assert_eq!(counts.friends, 0);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn dm_and_group_lifecycle() {
    let pool = pool().await;
    let (svc, ids) = service(&pool);
    let mut u = Vec::new();
    for _ in 0..4 {
        u.push(account(&pool, &ids).await);
    }
    for other in &u[1..] {
        befriend(&svc, u[0], *other).await;
    }
    let dm = svc.open_dm(u[0], u[1]).await.unwrap();
    assert!(dm.created);
    assert_eq!(dm.access, DmAccess::Allowed);
    let again = svc.open_dm(u[1], u[0]).await.unwrap();
    assert!(!again.created);
    assert_eq!(again.channel.id, dm.channel.id);
    svc.close_dm(u[0], dm.channel.id).await.unwrap();
    assert!(svc.list_dms(u[0]).await.unwrap().is_empty());
    assert_eq!(
        svc.reopen_for_message(dm.channel.id).await.unwrap(),
        vec![u[0]]
    );
    assert_eq!(svc.list_dms(u[0]).await.unwrap().len(), 1);
    // Nothing is hidden any more, so a second reopen changes nothing.
    assert!(
        svc.reopen_for_message(dm.channel.id)
            .await
            .unwrap()
            .is_empty()
    );
    let repo = svc.repo();
    assert!(repo.set_hidden(u[0], dm.channel.id, true).await.unwrap());
    assert!(!repo.set_hidden(u[0], dm.channel.id, true).await.unwrap());
    assert!(repo.set_hidden(u[0], dm.channel.id, false).await.unwrap());
    assert!(!repo.set_hidden(u[0], dm.channel.id, false).await.unwrap());

    let g = svc
        .create_group_dm(u[0], &[u[1], u[2]], Some("crew"))
        .await
        .unwrap();
    assert_eq!(g.kind, DmKind::Group);
    assert_eq!(g.recipients, vec![u[0], u[1], u[2]]);
    let g2 = svc
        .add_recipients(u[0], g.id, &[u[3]])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(g2.recipients.len(), 4);
    let g3 = svc
        .edit_group(
            u[1],
            g.id,
            &GroupEdit {
                name: Some("raid".into()),
                icon_key: Some(Some("icons/a".into())),
            },
        )
        .await
        .unwrap();
    assert_eq!(g3.name.as_deref(), Some("raid"));
    // Owner leaves: the longest-standing member inherits.
    let g4 = svc
        .remove_recipient(u[0], g.id, u[0])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(g4.owner, Some(u[1]));
    let g5 = svc.transfer_owner(u[1], g.id, u[2]).await.unwrap();
    assert_eq!(g5.owner, Some(u[2]));
    // Adding to a 1:1 spawns a separate group.
    let spawned = svc
        .add_recipients(u[0], dm.channel.id, &[u[2]])
        .await
        .unwrap()
        .unwrap();
    assert_ne!(spawned.id, dm.channel.id);
    assert_eq!(spawned.owner, Some(u[0]));
    // Blocks stop DMs.
    svc.block(u[1], u[0]).await.unwrap();
    assert!(matches!(
        svc.open_dm(u[0], u[1]).await,
        Err(SocialError::DmNotAllowed)
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn read_states_and_notifications() {
    let pool = pool().await;
    let (svc, ids) = service(&pool);
    let (a, b) = (account(&pool, &ids).await, account(&pool, &ids).await);
    befriend(&svc, a, b).await;
    let ch = svc.open_dm(a, b).await.unwrap().channel.id;

    svc.record_mentions(a, &[a, b, b], ch).await.unwrap();
    svc.record_mentions(a, &[b], ch).await.unwrap();
    let rs = svc.read_states(b).await.unwrap();
    assert_eq!(rs[0].mention_count, 2);
    assert!(svc.read_states(a).await.unwrap().is_empty());
    let s = svc.ack(b, ch, MessageId::new(100)).await.unwrap();
    assert_eq!(s.mention_count, 0);
    let s = svc.ack(b, ch, MessageId::new(50)).await.unwrap();
    assert_eq!(s.last_read, Some(MessageId::new(100)));
    let s = svc
        .mark_unread(b, ch, Some(MessageId::new(10)), 3)
        .await
        .unwrap();
    assert_eq!(
        (s.last_read, s.mention_count),
        (Some(MessageId::new(10)), 3)
    );
    assert!(svc.ack_pins(b, ch).await.unwrap().last_pin_ms.is_some());

    // DM-scope settings and a channel override with a timed mute.
    let s = svc
        .update_scope(
            b,
            None,
            &sr_chat::social::service::ScopeUpdate {
                level: Some(Some(NotificationLevel::OnlyMentions)),
                suppress_everyone: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(s.suppress_everyone);
    svc.update_override(b, ch, None, Some(Some(MuteDuration::Hour1)))
        .await
        .unwrap();
    let r = svc
        .resolve_notifications(b, None, ch, None, NotificationLevel::All)
        .await
        .unwrap();
    assert_eq!((r.level, r.muted), (NotificationLevel::OnlyMentions, true));
    assert!(
        svc.update_override(b, ch, Some(None), Some(None))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn profile_and_presence() {
    let pool = pool().await;
    let (svc, ids) = service(&pool);
    let (a, b) = (account(&pool, &ids).await, account(&pool, &ids).await);
    let p = svc
        .edit_profile(
            a,
            &ProfileEdit {
                bio: Some(Some("hi there".into())),
                pronouns: Some(Some("she/her".into())),
                accent_color: Some(Some(0x00FF_8800)),
                banner_key: Some(Some("banners/x".into())),
                avatar_key: Some(Some("0123456789abcdef0123456789abcdef".into())),
                display_name: Some(Some("Al Ice".into())),
            },
        )
        .await
        .unwrap();
    assert_eq!(p.accent_color, Some(0x00FF_8800));
    assert_eq!(p.display_name.as_deref(), Some("Al Ice"));
    // The avatar hash is persisted and read back with the profile.
    assert_eq!(
        svc.get_profile(a, a).await.unwrap().avatar_key.as_deref(),
        Some("0123456789abcdef0123456789abcdef")
    );
    let seen = svc.get_profile(b, a).await.unwrap();
    assert_eq!(seen.bio.as_deref(), Some("hi there"));
    for (name, visible) in [("pub", true), ("priv", false)] {
        svc.upsert_connection(Connection {
            user: a,
            provider: "github".into(),
            external_id: name.into(),
            display_name: name.into(),
            verified: false,
            visible,
            created_ms: 0,
        })
        .await
        .unwrap();
    }
    assert_eq!(svc.get_profile(b, a).await.unwrap().connections.len(), 1);
    assert_eq!(svc.get_profile(a, a).await.unwrap().connections.len(), 2);
    svc.remove_connection(a, "github", "priv").await.unwrap();
    assert_eq!(svc.get_profile(a, a).await.unwrap().connections.len(), 1);

    assert_eq!(svc.presence(a).await.unwrap().status, Status::Online);
    svc.set_status(a, Status::Dnd, Some(3_600_000))
        .await
        .unwrap();
    let pr = svc
        .set_custom_status(
            a,
            CustomStatus {
                text: Some("Glooping".into()),
                emoji_name: Some("tada".into()),
                emoji_id: None,
                expires_ms: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(pr.status, Status::Dnd);
    let loaded = svc.presence(a).await.unwrap();
    assert_eq!(loaded.custom.text.as_deref(), Some("Glooping"));
    assert!(loaded.status_expires_ms.is_some());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn username_lookup_and_friend_request_by_username() {
    let pool = pool().await;
    let (svc, ids) = service(&pool);
    let (a, b) = (account(&pool, &ids).await, account(&pool, &ids).await);
    let name: String = sqlx::query_scalar("SELECT username::text FROM accounts WHERE id = $1")
        .bind(b.get() as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
    let repo = svc.repo();
    assert_eq!(
        repo.find_user_by_username(&name.to_uppercase())
            .await
            .unwrap(),
        Some(b)
    );
    assert_eq!(
        repo.find_user_by_username("no_such_user_x").await.unwrap(),
        None
    );

    let (them, plan) = svc
        .send_friend_request_by_username(a, &name.to_uppercase())
        .await
        .unwrap();
    assert_eq!(them, b);
    assert!(!plan.ops.is_empty());
    assert_eq!(
        svc.list_relationships(b).await.unwrap()[0].kind,
        RelationshipKind::PendingIncoming
    );
    // Unknown names look like a closed door.
    assert!(matches!(
        svc.send_friend_request_by_username(a, "no_such_user_x")
            .await,
        Err(SocialError::RequestsNotAccepted)
    ));
    // A block by the target is indistinguishable from an unknown name.
    svc.block(b, a).await.unwrap();
    assert!(matches!(
        svc.send_friend_request_by_username(a, &name).await,
        Err(SocialError::RequestsNotAccepted)
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn guild_memberships_peers_and_mutual_friends() {
    use sr_chat::repo::GuildRepository;
    let pool = pool().await;
    let (svc, ids) = service(&pool);
    let (owner, member, stranger, friend) = (
        account(&pool, &ids).await,
        account(&pool, &ids).await,
        account(&pool, &ids).await,
        account(&pool, &ids).await,
    );
    let gen_ids = SnowflakeGenerator::new(Arc::new(SystemClock) as Arc<dyn Clock>, 7, 7).unwrap();
    let bp = sr_chat::guild_rules::plan_new_guild(owner, "Peers", 1_800_000_000_000, || {
        gen_ids.generate()
    })
    .unwrap();
    sr_db::chat_repo::PgChatRepo::new(pool.clone())
        .insert_guild(&bp)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO guild_members (guild_id, user_id, nick, joined_at) \
         VALUES ($1, $2, 'Memby', to_timestamp(1800000100))",
    )
    .bind(bp.guild.id.get() as i64)
    .bind(member.get() as i64)
    .execute(&pool)
    .await
    .unwrap();

    let repo = svc.repo();
    let m = repo.list_guild_memberships(member).await.unwrap();
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].guild, bp.guild.id);
    assert_eq!(m[0].nick.as_deref(), Some("Memby"));
    assert_eq!(m[0].joined_at_ms, 1_800_000_100_000);
    assert!(
        repo.list_guild_memberships(stranger)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(repo.guild_peers(member).await.unwrap(), vec![owner]);
    assert!(repo.guild_peers(stranger).await.unwrap().is_empty());

    befriend(&svc, owner, friend).await;
    befriend(&svc, member, friend).await;
    assert_eq!(
        repo.mutual_friends(owner, member).await.unwrap(),
        vec![friend]
    );
    assert!(
        repo.mutual_friends(owner, stranger)
            .await
            .unwrap()
            .is_empty()
    );

    let extras = svc.profile_extras(owner, member).await.unwrap();
    assert_eq!(extras.mutual_friends, vec![friend]);
    assert_eq!(extras.mutual_guilds.len(), 1);
    assert!(extras.friends_since_ms.is_none());
    let audience = svc.audience(member).await.unwrap();
    assert_eq!(audience.friends, vec![friend]);
    assert_eq!(audience.guilds, vec![bp.guild.id]);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn user_settings_blob_default_level_and_privacy_persist() {
    use serde_json::json;
    use sr_chat::social::user_settings::PrivacyUpdate;
    let pool = pool().await;
    let (svc, ids) = service(&pool);
    let (a, b) = (account(&pool, &ids).await, account(&pool, &ids).await);

    // Blob: merge, version bumps, per user.
    assert_eq!(svc.user_settings(a).await.unwrap().version, 0);
    let s = svc
        .patch_user_settings(a, &json!({"theme": "ash", "ui": {"compact": true}}))
        .await
        .unwrap();
    assert_eq!(s.version, 1);
    let s = svc
        .patch_user_settings(a, &json!({"theme": null, "ui": {"zoom": 90}}))
        .await
        .unwrap();
    assert_eq!(s.version, 2);
    let stored = svc.user_settings(a).await.unwrap();
    assert_eq!(stored.data, json!({"ui": {"compact": true, "zoom": 90}}));
    assert_eq!(svc.user_settings(b).await.unwrap().version, 0);

    // A stale writer loses the race at the repository.
    use sr_chat::social::repo::UserSettingsRepository;
    let repo = svc.repo();
    assert_eq!(
        repo.put_user_settings(a, &json!({"x": 1}), 1)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        repo.put_user_settings(a, &json!({"x": 1}), 2)
            .await
            .unwrap(),
        Some(3)
    );
    // Two writers patching concurrently both land.
    let (left, right) = (json!({"left": 1}), json!({"right": 2}));
    let (r1, r2) = tokio::join!(
        svc.patch_user_settings(a, &left),
        svc.patch_user_settings(a, &right)
    );
    r1.unwrap();
    r2.unwrap();
    let merged = svc.user_settings(a).await.unwrap().data;
    assert_eq!(merged["left"], 1);
    assert_eq!(merged["right"], 2);

    // Default level lives in the same row and does not disturb the blob.
    assert_eq!(svc.default_notification_level(a).await.unwrap(), None);
    svc.set_default_notification_level(a, Some(NotificationLevel::OnlyMentions))
        .await
        .unwrap();
    assert_eq!(
        svc.default_notification_level(a).await.unwrap(),
        Some(NotificationLevel::OnlyMentions)
    );
    assert_eq!(svc.user_settings(a).await.unwrap().data, merged);
    // A user who never saved a blob can still set the level and then save one.
    svc.set_default_notification_level(b, Some(NotificationLevel::Nothing))
        .await
        .unwrap();
    assert_eq!(svc.user_settings(b).await.unwrap().version, 0);
    assert_eq!(
        svc.patch_user_settings(b, &json!({"k": 1}))
            .await
            .unwrap()
            .version,
        1
    );
    assert_eq!(
        svc.default_notification_level(b).await.unwrap(),
        Some(NotificationLevel::Nothing)
    );
    svc.set_default_notification_level(b, None).await.unwrap();
    assert_eq!(svc.default_notification_level(b).await.unwrap(), None);

    // Privacy.
    let p = svc
        .update_privacy(
            a,
            PrivacyUpdate {
                friend_everyone: Some(false),
                message_requests: Some(false),
                ..PrivacyUpdate::default()
            },
        )
        .await
        .unwrap();
    assert!(!p.friend_requests.everyone && !p.dms.message_requests);
    assert_eq!(svc.privacy(a).await.unwrap(), p);
    assert!(svc.privacy(b).await.unwrap().friend_requests.everyone);
}
