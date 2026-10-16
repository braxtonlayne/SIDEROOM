//! Postgres-backed tests for the phase-12 lounge channel backend items (G-37, G-36, G-44,
//! G-47). Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test p12_lounge_ch -- --ignored`

use std::sync::Arc;

use sr_chat::admin_rules::GuildPatch;
use sr_chat::rich_repo::RichStore;
use sr_chat::settings::repo::CommunityRepository;
use sr_chat::social::dm::GroupInvite;
use sr_chat::social::repo::DmRepository;
use sr_chat::{ChatLimits, ChatService};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, Clock, GuildId, SnowflakeGenerator, SystemClock, UserId};
use sr_db::PgPool;
use sr_db::chat_repo::PgChatRepo;
use sr_db::rich_repo::PgRichStore;
use sr_db::settings_repo::PgSettingsRepo;
use sr_db::social_repo::PgSocialRepo;
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

fn service(pool: &PgPool) -> ChatService<PgChatRepo> {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock,
        ids(5),
        ChatLimits::default(),
    )
}

async fn guild(pool: &PgPool, test_ids: &Ids) -> (GuildId, ChannelId, UserId) {
    let owner = account(pool, test_ids).await;
    let bp = service(pool)
        .create_guild(owner, "Lounge Club")
        .await
        .unwrap();
    (bp.guild.id, bp.channels[2].id, owner)
}

const FREE_BITS: u64 = (1 << 45) | (1 << 56);

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn migration_0791_gives_existing_servers_the_free_bits() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(6);
    let (g, _, _) = guild(&pool, &test_ids).await;
    // An old server: @everyone predates the two bits, and another role never had them.
    sqlx::query("UPDATE roles SET permissions = permissions & ~$2 WHERE id = $1")
        .bind(g.get() as i64)
        .bind(FREE_BITS as i64)
        .execute(&pool)
        .await
        .unwrap();
    let other = test_ids.generate().get() as i64;
    sqlx::query(
        "INSERT INTO roles (id, guild_id, name, position, permissions) VALUES ($1, $2, 'mods', 1, 8192)",
    )
    .bind(other)
    .bind(g.get() as i64)
    .execute(&pool)
    .await
    .unwrap();
    let bits = |id: i64| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>("SELECT permissions FROM roles WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap() as u64
        }
    };
    assert_eq!(bits(g.get() as i64).await & FREE_BITS, 0);

    sqlx::raw_sql(include_str!("../migrations/0791_backfill_free_bits.sql"))
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(bits(g.get() as i64).await & FREE_BITS, FREE_BITS);
    assert_eq!(bits(other).await, 8192, "other roles are untouched");
    // Idempotent: running it again changes nothing.
    sqlx::raw_sql(include_str!("../migrations/0791_backfill_free_bits.sql"))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(bits(g.get() as i64).await & FREE_BITS, FREE_BITS);
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn guild_safety_fields_round_trip_and_mfa_is_owner_only() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(6);
    let (g, general, owner) = guild(&pool, &test_ids).await;
    let svc = service(&pool);
    let before = svc.get_guild(owner, g).await.unwrap().0;
    assert_eq!(
        (
            before.mfa_level,
            before.system_channel_flags,
            before.widget_enabled
        ),
        (0, 0, false)
    );

    let g2 = svc
        .edit_guild(
            owner,
            g,
            GuildPatch {
                mfa_level: Some(1),
                system_channel_flags: Some(1 | 4),
                widget_enabled: Some(true),
                widget_channel_id: Some(Some(general)),
                ..GuildPatch::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(g2.mfa_level, 1);
    let back = svc.get_guild(owner, g).await.unwrap().0;
    assert_eq!(back.system_channel_flags, 5);
    assert!(back.widget_enabled);
    assert_eq!(back.widget_channel_id, Some(general));

    // Unknown bits are refused.
    let bad_bits = GuildPatch {
        system_channel_flags: Some(1 << 9),
        ..GuildPatch::default()
    };
    assert!(svc.edit_guild(owner, g, bad_bits).await.is_err());

    // A manager who is not the owner cannot change the 2FA requirement.
    let manager = account(&pool, &test_ids).await;
    sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
        .bind(g.get() as i64)
        .bind(manager.get() as i64)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE roles SET permissions = permissions | $2 WHERE id = $1")
        .bind(g.get() as i64)
        .bind(Permissions::MANAGE_GUILD.bits() as i64)
        .execute(&pool)
        .await
        .unwrap();
    let off = GuildPatch {
        mfa_level: Some(0),
        ..GuildPatch::default()
    };
    assert!(svc.edit_guild(manager, g, off).await.is_err());
    assert_eq!(svc.get_guild(owner, g).await.unwrap().0.mfa_level, 1);
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn community_dm_and_spam_protection_round_trip() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(6);
    let (g, _, _) = guild(&pool, &test_ids).await;
    let repo = PgSettingsRepo::new(pool.clone());
    let mut c = repo.get_community(g).await.unwrap();
    assert!(!c.dm_spam_filter && !c.explicit_media_filter_dms && !c.hide_suspicious_links);
    assert_eq!(c.warn_new_account_days, 0);
    c.dm_spam_filter = true;
    c.explicit_media_filter_dms = true;
    c.hide_suspicious_links = true;
    c.warn_new_account_days = 14;
    repo.put_community(&c).await.unwrap();
    let back = repo.get_community(g).await.unwrap();
    assert!(back.dm_spam_filter && back.explicit_media_filter_dms && back.hide_suspicious_links);
    assert_eq!(back.warn_new_account_days, 14);
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn group_dm_invites_round_trip() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(6);
    let a = account(&pool, &test_ids).await;
    let b = account(&pool, &test_ids).await;
    let c = account(&pool, &test_ids).await;
    let repo = PgSocialRepo::new(pool.clone());
    let ch = ChannelId::new(test_ids.generate().get());
    repo.insert_group(ch, &[a, b], Some("crew")).await.unwrap();

    let now = 1_800_000_000_000;
    let inv = GroupInvite::new("abcd1234".into(), ch, a, now);
    repo.insert_group_invite(&inv).await.unwrap();
    let got = repo.get_group_invite("abcd1234").await.unwrap().unwrap();
    assert_eq!(got.channel_id, ch);
    assert_eq!(got.inviter_id, a);
    assert!(got.usable(now + 1000));
    assert!(!got.usable(got.expires_at_ms), "expires after 24 hours");
    assert!(repo.get_group_invite("nope").await.unwrap().is_none());
    assert_eq!(repo.list_group_invites(ch).await.unwrap().len(), 1);

    // Accepting adds the recipient through the ordinary group path.
    repo.add_recipients(ch, &[c]).await.unwrap();
    assert!(repo.get_dm(ch).await.unwrap().unwrap().has(c));

    assert!(repo.revoke_group_invite("abcd1234").await.unwrap());
    assert!(!repo.revoke_group_invite("abcd1234").await.unwrap());
    let revoked = repo.get_group_invite("abcd1234").await.unwrap().unwrap();
    assert!(revoked.revoked && !revoked.usable(now));
    // The invite dies with the channel.
    repo.delete_dm(ch).await.unwrap();
    assert!(repo.get_group_invite("abcd1234").await.unwrap().is_none());
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn sticker_packs_list_published_packs_and_their_stickers() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(6);
    let creator = account(&pool, &test_ids).await;
    let store = PgRichStore::new(pool.clone());
    assert!(
        store.sticker_packs().await.unwrap().is_empty(),
        "no default packs ship"
    );

    let pack = test_ids.generate().get() as i64;
    let hidden = test_ids.generate().get() as i64;
    for (id, name, published) in [(pack, "Blobs", true), (hidden, "Draft", false)] {
        sqlx::query("INSERT INTO sticker_packs (id, name, published) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(name)
            .bind(published)
            .execute(&pool)
            .await
            .unwrap();
    }
    let mut sticker_ids = Vec::new();
    for (pack_id, name) in [(pack, "wave"), (pack, "cheer"), (hidden, "secret")] {
        let sid = test_ids.generate().get() as i64;
        sticker_ids.push(sid);
        sqlx::query(
            "INSERT INTO stickers (id, guild_id, pack_id, name, tags, format_type, creator_id, media_key) VALUES ($1, NULL, $2, $3, 'blob', 1, $4, $5)",
        )
        .bind(sid)
        .bind(pack_id)
        .bind(name)
        .bind(creator.get() as i64)
        .bind(format!("sticker/pack/{sid}.png"))
        .execute(&pool)
        .await
        .unwrap();
    }
    let packs = store.sticker_packs().await.unwrap();
    assert_eq!(packs.len(), 1, "unpublished packs stay out");
    assert_eq!(packs[0].name, "Blobs");
    let names: Vec<_> = packs[0].stickers.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["wave", "cheer"]);
    assert_eq!(packs[0].cover_sticker_id, Some(packs[0].stickers[0].id));

    // A pack sticker is a standard sticker: the message path resolves it with no guild.
    let rows = store.stickers(&[packs[0].stickers[0].id]).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].guild_id.is_none());
    assert!(
        store
            .pack_sticker_media(packs[0].stickers[0].id)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .pack_sticker_media(sticker_ids[2] as u64)
            .await
            .unwrap()
            .is_none(),
        "unpublished pack"
    );

    // A pack sticker cannot also belong to a guild.
    let (g, _, _) = guild(&pool, &test_ids).await;
    let both = sqlx::query(
        "INSERT INTO stickers (id, guild_id, pack_id, name, tags, format_type, creator_id, media_key) VALUES ($1, $2, $3, 'bad', 'x', 1, $4, 'sticker/x.png')",
    )
    .bind(test_ids.generate().get() as i64)
    .bind(g.get() as i64)
    .bind(pack)
    .bind(creator.get() as i64)
    .execute(&pool)
    .await;
    assert!(both.is_err());
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn scheduled_events_round_trip_interest_and_due_queries() {
    use sr_chat::settings::repo::ScheduledEventRepository;
    use sr_chat::settings::scheduled_events::{EntityType, EventStatus, ScheduledEvent};
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(6);
    let owner = account(&pool, &test_ids).await;
    let bp = service(&pool)
        .create_guild(owner, "Events Club")
        .await
        .unwrap();
    let (g, voice) = (bp.guild.id, bp.channels[3].id);
    let repo = PgSettingsRepo::new(pool.clone());
    let t0 = 1_800_000_000_000_i64;
    let mk = |id: u64, start: i64, end: Option<i64>, kind: EntityType| ScheduledEvent {
        id,
        guild_id: g,
        channel_id: (kind != EntityType::External).then_some(voice),
        creator_id: owner,
        name: format!("event {id}"),
        description: Some("d".into()),
        start_ms: start,
        end_ms: end,
        status: EventStatus::Scheduled,
        entity_type: kind,
        location: (kind == EntityType::External).then(|| "park".to_owned()),
        created_at_ms: t0,
        entity_id: None,
        image_key: Some("0123456789abcdef0123456789abcdef".to_owned()),
        empty_since_ms: None,
    };
    let a = mk(
        test_ids.generate().get(),
        t0 + 3_000,
        None,
        EntityType::Voice,
    );
    let b = mk(
        test_ids.generate().get(),
        t0 + 1_000,
        Some(t0 + 2_000),
        EntityType::External,
    );
    repo.insert_event(&a).await.unwrap();
    repo.insert_event(&b).await.unwrap();

    // Soonest first, round-trips every field, counts only open ones.
    let open = repo.list_open_events(g).await.unwrap();
    assert_eq!(open.iter().map(|e| e.id).collect::<Vec<_>>(), [b.id, a.id]);
    assert_eq!(open[0], b);
    assert_eq!(open[1], a);
    assert_eq!(repo.count_open_events(g).await.unwrap(), 2);
    assert_eq!(repo.get_event(g, a.id).await.unwrap().unwrap(), a);
    assert!(
        repo.get_event(GuildId::new(7), a.id)
            .await
            .unwrap()
            .is_none(),
        "events belong to their guild"
    );

    // Interest: idempotent, counted, listed and paged by user id.
    let users = [
        account(&pool, &test_ids).await,
        account(&pool, &test_ids).await,
        account(&pool, &test_ids).await,
    ];
    for u in users {
        assert!(repo.add_interest(a.id, u, t0).await.unwrap());
    }
    assert!(!repo.add_interest(a.id, users[0], t0).await.unwrap());
    assert_eq!(repo.interest_counts(&[a.id, b.id]).await.unwrap()[&a.id], 3);
    assert!(
        repo.interested_in(users[1], &[a.id, b.id])
            .await
            .unwrap()
            .contains(&a.id)
    );
    let all = repo.list_interested(a.id, None, None, 100).await.unwrap();
    assert_eq!(all, users);
    let after = repo
        .list_interested(a.id, None, Some(users[0]), 1)
        .await
        .unwrap();
    assert_eq!(after, [users[1]]);
    let before = repo
        .list_interested(a.id, Some(users[2]), None, 1)
        .await
        .unwrap();
    assert_eq!(before, [users[1]], "before pages from the end");
    assert!(repo.remove_interest(a.id, users[0]).await.unwrap());
    assert!(!repo.remove_interest(a.id, users[0]).await.unwrap());

    // The clock: b is due at its start, a only at its own; a running event ends at its end time.
    assert!(repo.due_events(t0).await.unwrap().is_empty());
    let due = repo.due_events(t0 + 1_500).await.unwrap();
    assert_eq!(due.iter().map(|e| e.id).collect::<Vec<_>>(), [b.id]);
    let mut running = b.clone();
    running.status = EventStatus::Active;
    repo.update_event(&running).await.unwrap();
    // A running event is always the clock's to look at (occupancy, end time, empty countdown).
    let due = repo.due_events(t0 + 1_500).await.unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].status, EventStatus::Active);
    // The clock's bookkeeping round-trips: stage link, cover and the empty-channel mark.
    running.entity_id = Some(voice.get());
    running.empty_since_ms = Some(t0 + 1_800);
    running.image_key = None;
    repo.update_event(&running).await.unwrap();
    assert_eq!(repo.get_event(g, b.id).await.unwrap().unwrap(), running);
    running.entity_id = None;
    running.empty_since_ms = None;
    running.status = EventStatus::Completed;
    repo.update_event(&running).await.unwrap();
    assert_eq!(repo.count_open_events(g).await.unwrap(), 1);
    assert_eq!(repo.list_open_events(g).await.unwrap().len(), 1);

    // A deleted voice channel leaves an orphan the scheduler can wind up.
    sqlx::query("DELETE FROM channels WHERE id = $1")
        .bind(voice.get() as i64)
        .execute(&pool)
        .await
        .unwrap();
    let orphan = repo.due_events(t0).await.unwrap();
    assert_eq!(orphan.len(), 1);
    assert!(orphan[0].channel_id.is_none());

    // Deleting an event takes its interest with it.
    assert!(repo.delete_event(g, a.id).await.unwrap());
    assert!(!repo.delete_event(g, a.id).await.unwrap());
    assert!(repo.interest_counts(&[a.id]).await.unwrap().is_empty());
    db.cleanup().await.unwrap();
}
