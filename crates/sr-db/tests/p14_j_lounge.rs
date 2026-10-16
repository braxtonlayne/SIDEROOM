//! Postgres-backed tests for the lounge safety, member-flag and video-quality columns (J-02, J-03,
//! J-04, J-05, J-11). Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test p14_j_lounge -- --ignored --test-threads=1`

use std::sync::Arc;

use sr_chat::admin_repo::GuildAdminRepository;
use sr_chat::repo::{ChannelRepository, GuildRepository};
use sr_chat::settings::automod::AutoModAlert;
use sr_chat::settings::repo::{AutoModRepository, CommunityRepository};
use sr_chat::social::repo::DmRepository;
use sr_chat::{ChatLimits, ChatService};
use sr_core::{ChannelId, Clock, GuildId, SnowflakeGenerator, SystemClock, UserId};
use sr_db::PgPool;
use sr_db::chat_repo::PgChatRepo;
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

async fn guild(pool: &PgPool, owner: UserId) -> GuildId {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let svc = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock,
        ids(5),
        ChatLimits::default(),
    );
    svc.create_guild(owner, "Lounge").await.unwrap().guild.id
}

async fn join(pool: &PgPool, g: GuildId, u: UserId) {
    sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
        .bind(g.get() as i64)
        .bind(u.get() as i64)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn incident_columns_round_trip_and_the_raid_alert_respects_its_cooldown() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(6);
    let owner = account(&pool, &test_ids).await;
    let g = guild(&pool, owner).await;
    let settings = PgSettingsRepo::new(pool.clone());
    let chat = PgChatRepo::new(pool.clone());
    const NOW: i64 = 1_800_000_000_000;
    const DAY: i64 = 86_400_000;

    // Defaults: nothing paused, the automatic pause is on.
    let c = settings.get_community(g).await.unwrap();
    assert_eq!(
        (
            c.invites_disabled_until_ms,
            c.dms_disabled_until_ms,
            c.raid_detected_at_ms,
            c.raid_auto_pause_invites
        ),
        (None, None, None, true)
    );
    // No row yet: the alert has nothing to update.
    assert_eq!(
        settings
            .mark_raid(g, NOW, 600_000, NOW + DAY)
            .await
            .unwrap(),
        None
    );

    // The two timed pauses are stored and show on the guild object.
    let mut c = settings.get_community(g).await.unwrap();
    c.invites_disabled_until_ms = Some(NOW + 3_600_000);
    c.dms_disabled_until_ms = Some(NOW + 7_200_000);
    settings.put_community(&c).await.unwrap();
    let back = settings.get_community(g).await.unwrap();
    assert_eq!(back.invites_disabled_until_ms, c.invites_disabled_until_ms);
    assert_eq!(back.dms_disabled_until_ms, c.dms_disabled_until_ms);
    let guild = chat.get_guild(g).await.unwrap().unwrap();
    assert_eq!(
        guild.incidents.invites_disabled_until_ms,
        c.invites_disabled_until_ms
    );
    assert_eq!(
        guild.incidents.dms_disabled_until_ms,
        c.dms_disabled_until_ms
    );
    assert_eq!(guild.incidents.raid_detected_at_ms, None);

    // The alert pauses invites for a day once, then waits out its cooldown.
    let fired = settings
        .mark_raid(g, NOW, 600_000, NOW + DAY)
        .await
        .unwrap();
    assert_eq!(fired, Some(true));
    let c = settings.get_community(g).await.unwrap();
    assert_eq!(c.raid_detected_at_ms, Some(NOW));
    assert_eq!(c.invites_disabled_until_ms, Some(NOW + DAY));
    assert_eq!(
        settings
            .mark_raid(g, NOW + 60_000, 600_000, NOW + DAY)
            .await
            .unwrap(),
        None,
        "inside the cooldown"
    );
    assert_eq!(
        settings
            .mark_raid(g, NOW + 700_000, 600_000, NOW + DAY)
            .await
            .unwrap(),
        Some(true)
    );

    // Resume clears the pause once.
    assert!(settings.clear_invite_pause(g).await.unwrap());
    assert!(!settings.clear_invite_pause(g).await.unwrap());
    assert_eq!(
        settings
            .get_community(g)
            .await
            .unwrap()
            .invites_disabled_until_ms,
        None
    );

    // With the automatic pause off the alert is recorded but leaves invites alone.
    let mut c = settings.get_community(g).await.unwrap();
    c.raid_auto_pause_invites = false;
    c.raid_detected_at_ms = None;
    settings.put_community(&c).await.unwrap();
    assert_eq!(
        settings
            .mark_raid(g, NOW + DAY, 600_000, NOW + 2 * DAY)
            .await
            .unwrap(),
        Some(false)
    );
    assert_eq!(
        settings
            .get_community(g)
            .await
            .unwrap()
            .invites_disabled_until_ms,
        None
    );
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_paused_invite_is_refused_until_the_time_passes_and_joins_are_counted() {
    use sr_chat::settings::SettingsError;
    use sr_chat::settings::invites::{GuildInvite, InviteOptions};
    use sr_chat::settings::join::InviteJoinRepository;
    use sr_chat::settings::repo::InviteRepository;
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(7);
    let owner = account(&pool, &test_ids).await;
    let g = guild(&pool, owner).await;
    let settings = PgSettingsRepo::new(pool.clone());
    let chat = PgChatRepo::new(pool.clone());
    let now = SystemClock.now_unix_ms();
    let channel = chat
        .list_guild_channels(g)
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.kind == sr_chat::ChannelType::Text)
        .unwrap()
        .id;
    let invite = GuildInvite::new(
        "pausedcode".into(),
        g,
        channel,
        owner,
        InviteOptions::default(),
        now,
    )
    .unwrap();
    settings.insert_invite(&invite).await.unwrap();
    let joiner = sr_db::invite_join::PgInviteJoinRepo::new(pool.clone());

    let mut c = settings.get_community(g).await.unwrap();
    c.invites_disabled_until_ms = Some(now + 3_600_000);
    settings.put_community(&c).await.unwrap();
    let who = account(&pool, &test_ids).await;
    let err = joiner
        .join_via_invite("pausedcode", who, now)
        .await
        .unwrap_err();
    assert!(matches!(err, SettingsError::InvitesPaused), "{err:?}");
    // The directory path answers the same.
    let err = joiner.join_listed(g, who, now).await.unwrap_err();
    assert!(matches!(err, SettingsError::InvitesPaused), "{err:?}");

    // An hour later the same invite works, and the join shows in the one-minute count.
    let later = now + 3_600_001;
    joiner
        .join_via_invite("pausedcode", who, later)
        .await
        .unwrap();
    assert_eq!(chat.count_recent_joins(g, later - 60_000).await.unwrap(), 1);
    assert_eq!(chat.count_recent_joins(g, later + 1).await.unwrap(), 0);
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn dms_between_strangers_are_blocked_only_when_every_shared_server_pauses_them() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(8);
    let a = account(&pool, &test_ids).await;
    let b = account(&pool, &test_ids).await;
    let (g1, g2) = (guild(&pool, a).await, guild(&pool, a).await);
    join(&pool, g1, b).await;
    join(&pool, g2, b).await;
    let settings = PgSettingsRepo::new(pool.clone());
    let social = PgSocialRepo::new(pool.clone());
    let now = SystemClock.now_unix_ms();

    assert!(!social.shared_protection(a, b).await.unwrap().dms_paused_all);
    let mut c1 = settings.get_community(g1).await.unwrap();
    c1.dms_disabled_until_ms = Some(now + 3_600_000);
    settings.put_community(&c1).await.unwrap();
    assert!(
        !social.shared_protection(a, b).await.unwrap().dms_paused_all,
        "one of two shared servers is not enough"
    );
    let mut c2 = settings.get_community(g2).await.unwrap();
    c2.dms_disabled_until_ms = Some(now + 3_600_000);
    settings.put_community(&c2).await.unwrap();
    assert!(social.shared_protection(a, b).await.unwrap().dms_paused_all);
    // A pause that ended no longer counts.
    c2.dms_disabled_until_ms = Some(now - 1_000);
    settings.put_community(&c2).await.unwrap();
    assert!(!social.shared_protection(a, b).await.unwrap().dms_paused_all);
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn member_flags_are_stored_and_read_back() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(9);
    let owner = account(&pool, &test_ids).await;
    let user = account(&pool, &test_ids).await;
    let g = guild(&pool, owner).await;
    join(&pool, g, user).await;
    let chat = PgChatRepo::new(pool.clone());

    let mut m = chat.get_member(g, user).await.unwrap().unwrap();
    assert_eq!(m.extras.flags, 0);
    m.extras.flags |= sr_core::member_flags::COMPLETED_ONBOARDING;
    chat.update_member(&m).await.unwrap();
    let back = chat.get_member(g, user).await.unwrap().unwrap();
    assert_eq!(back.extras.flags, 2);
    let page = chat.page_members(g, None, 10).await.unwrap();
    assert!(
        page.iter()
            .any(|x| x.user_id == user && x.extras.flags == 2)
    );
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn voice_channels_keep_their_video_quality_mode() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(10);
    let owner = account(&pool, &test_ids).await;
    let g = guild(&pool, owner).await;
    let chat = PgChatRepo::new(pool.clone());
    let voice = chat
        .list_guild_channels(g)
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.kind == sr_chat::ChannelType::Voice)
        .unwrap();
    assert_eq!(voice.video_quality_mode, 1, "auto by default");
    let mut edited = voice.clone();
    edited.video_quality_mode = 2;
    chat.update_channel(&edited).await.unwrap();
    let back = chat.get_channel(voice.id).await.unwrap().unwrap();
    assert_eq!(back.video_quality_mode, 2);
    // The column refuses anything but 1 and 2.
    let bad = sqlx::query("UPDATE channels SET video_quality_mode = 3 WHERE id = $1")
        .bind(voice.id.get() as i64)
        .execute(&pool)
        .await;
    assert!(bad.is_err());
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn automod_alerts_are_stored_and_resolved_once() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(11);
    let owner = account(&pool, &test_ids).await;
    let target = account(&pool, &test_ids).await;
    let g = guild(&pool, owner).await;
    let settings = PgSettingsRepo::new(pool.clone());
    let chat = PgChatRepo::new(pool.clone());
    let channel: ChannelId = chat
        .list_guild_channels(g)
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.kind == sr_chat::ChannelType::Text)
        .unwrap()
        .id;
    let alert = AutoModAlert {
        id: test_ids.generate().get(),
        guild_id: g,
        rule_id: 7,
        rule_name: "No links".into(),
        channel_id: channel,
        target_user_id: target,
        flagged_channel_id: channel,
        flagged_message_id: Some(99),
        resolved_by: None,
    };
    settings.insert_alert(&alert).await.unwrap();
    assert_eq!(
        settings.get_alert(alert.id).await.unwrap(),
        Some(alert.clone())
    );
    assert!(
        settings
            .resolve_alert(alert.id, owner, 1_800_000_000_000)
            .await
            .unwrap()
    );
    assert!(
        !settings
            .resolve_alert(alert.id, target, 1_800_000_000_001)
            .await
            .unwrap(),
        "a second press changes nothing"
    );
    let back = settings.get_alert(alert.id).await.unwrap().unwrap();
    assert_eq!(back.resolved_by, Some(owner));
    assert_eq!(settings.get_alert(1).await.unwrap(), None);
    db.cleanup().await.unwrap();
}
