//! Postgres-backed settings tests (never run on the dev machine without Postgres).
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test settings_repo -- --ignored`

use std::sync::Arc;

use sr_chat::settings::audit::{AuditAction, AuditEntry, AuditQuery};
use sr_chat::settings::automod::{Action, AutoModRule, EventType, Trigger};
use sr_chat::settings::community::{CommunitySettings, Onboarding, WelcomeChannel, WelcomeScreen};
use sr_chat::settings::expressions::Emoji;
use sr_chat::settings::invites::{GuildInvite, InviteOptions};
use sr_chat::settings::repo::{
    AuditRepository, AutoModRepository, CommunityRepository, ExpressionRepository,
    InviteRepository, TemplateRepository,
};
use sr_chat::settings::templates::{GuildTemplate, SnapSettings, TemplateSnapshot};
use sr_chat::{ChatLimits, ChatService};
use sr_core::{ChannelId, Clock, EmojiId, GuildId, SnowflakeGenerator, SystemClock, UserId};
use sr_db::chat_repo::PgChatRepo;
use sr_db::settings_repo::PgSettingsRepo;
use sr_db::{DbConfig, PgPool};

type Ids = SnowflakeGenerator<Arc<dyn Clock>>;

async fn pool() -> PgPool {
    let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
    let pool = sr_db::connect(&DbConfig::new(url)).await.expect("connect");
    sr_db::migrate(&pool).await.expect("migrate");
    pool
}

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

/// A guild with an owner; returns `(guild, general channel, owner)`.
async fn guild(pool: &PgPool, test_ids: &Ids) -> (GuildId, ChannelId, UserId) {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let svc = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock,
        ids(5),
        ChatLimits::default(),
    );
    let owner = account(pool, test_ids).await;
    let bp = svc.create_guild(owner, "Settings Club").await.unwrap();
    (bp.guild.id, bp.channels[2].id, owner)
}

const NOW: i64 = 1_800_000_000_000;

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn invite_consume_is_exact_and_respects_expiry() {
    let pool = pool().await;
    let test_ids = ids(6);
    let repo = PgSettingsRepo::new(pool.clone());
    let (g, ch, owner) = guild(&pool, &test_ids).await;
    let mk = |code: &str, age, uses| {
        GuildInvite::new(
            code.to_owned(),
            g,
            ch,
            owner,
            InviteOptions {
                max_age_secs: age,
                max_uses: uses,
                temporary: false,
            },
            NOW,
        )
        .unwrap()
    };
    let code = format!("t{}", test_ids.generate().get());
    repo.insert_invite(&mk(&code, 60, 2)).await.unwrap();
    assert_eq!(repo.count_active_invites(g, NOW).await.unwrap(), 1);
    assert_eq!(
        repo.consume_invite(&code, NOW + 1_000)
            .await
            .unwrap()
            .unwrap()
            .uses,
        1
    );
    assert!(
        repo.consume_invite(&code, NOW + 2_000)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        repo.consume_invite(&code, NOW + 3_000)
            .await
            .unwrap()
            .is_none()
    );

    let code2 = format!("e{}", test_ids.generate().get());
    repo.insert_invite(&mk(&code2, 10, 0)).await.unwrap();
    assert!(
        repo.consume_invite(&code2, NOW + 10_000)
            .await
            .unwrap()
            .is_none()
    );
    assert!(repo.revoke_invite(&code2, NOW).await.unwrap());
    assert!(!repo.revoke_invite(&code2, NOW).await.unwrap());
    assert!(repo.purge_dead_invites(NOW + 1_000_000).await.unwrap() >= 1);
    assert!(!repo.is_banned(g, owner).await.unwrap());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn audit_log_is_append_only_until_retention() {
    let pool = pool().await;
    let test_ids = ids(7);
    let repo = PgSettingsRepo::new(pool.clone());
    let actor = UserId::new(1);
    let guild = GuildId::new(test_ids.generate().get());
    for (n, action) in [AuditAction::RoleCreate, AuditAction::RoleDelete]
        .into_iter()
        .enumerate()
    {
        let e = AuditEntry::new(
            test_ids.generate().get(),
            guild,
            actor,
            action,
            Some(5),
            Some("because"),
            NOW + n as i64,
        )
        .unwrap();
        repo.append_audit(&e).await.unwrap();
    }
    let all = repo.list_audit(&AuditQuery::new(guild)).await.unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].action, AuditAction::RoleDelete);
    let mut q = AuditQuery::new(guild);
    q.action = Some(AuditAction::RoleCreate);
    assert_eq!(repo.list_audit(&q).await.unwrap().len(), 1);

    // Direct UPDATE / DELETE are rejected by the trigger.
    assert!(
        sqlx::query("UPDATE audit_log_entries SET reason = 'x' WHERE guild_id = $1")
            .bind(guild.get() as i64)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM audit_log_entries WHERE guild_id = $1")
            .bind(guild.get() as i64)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(repo.purge_audit_before(NOW + 1).await.unwrap() >= 1);
    assert_eq!(
        repo.list_audit(&AuditQuery::new(guild))
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn emoji_rules_templates_and_community_round_trip() {
    let pool = pool().await;
    let test_ids = ids(8);
    let repo = PgSettingsRepo::new(pool.clone());
    let (g, ch, owner) = guild(&pool, &test_ids).await;

    let emoji = Emoji {
        id: EmojiId::new(test_ids.generate().get()),
        guild_id: g,
        name: "party".into(),
        animated: false,
        available: true,
        role_ids: vec![],
        creator_id: owner,
        media_key: "k".into(),
        created_at_ms: NOW,
    };
    repo.insert_emoji(&emoji).await.unwrap();
    assert_eq!(repo.list_emoji(g).await.unwrap(), vec![emoji.clone()]);
    assert!(repo.delete_emoji(g, emoji.id).await.unwrap());

    let rule = AutoModRule {
        id: test_ids.generate().get(),
        guild_id: g,
        name: "words".into(),
        creator_id: owner,
        event_type: EventType::MessageSend,
        trigger: Trigger::Keyword {
            keywords: vec!["heck*".into()],
            regex_patterns: vec![],
            allow_list: vec![],
        },
        actions: vec![Action::SendAlert { channel_id: ch }],
        enabled: true,
        exempt_role_ids: vec![],
        exempt_channel_ids: vec![ch],
    };
    repo.put_rule(&rule).await.unwrap();
    assert_eq!(repo.get_rule(g, rule.id).await.unwrap(), Some(rule.clone()));
    assert!(repo.delete_rule(g, rule.id).await.unwrap());

    let snapshot = TemplateSnapshot {
        settings: SnapSettings {
            preferred_locale: "en-US".into(),
            verification_level: 0,
            default_message_notifications: 0,
            explicit_content_filter: 0,
            system_channel_local_id: None,
        },
        roles: vec![],
        channels: vec![],
    };
    let tmpl = GuildTemplate::new(
        // 16 alphanumeric characters, the length the real generator produces.
        format!("T{:015}", test_ids.generate().get() % 1_000_000_000_000_000),
        g,
        owner,
        "Template",
        Some("desc"),
        snapshot,
        NOW,
    )
    .unwrap();
    repo.put_template(&tmpl).await.unwrap();
    repo.bump_template_usage(&tmpl.code).await.unwrap();
    assert_eq!(
        repo.get_template(&tmpl.code)
            .await
            .unwrap()
            .unwrap()
            .usage_count,
        1
    );
    assert!(repo.delete_template(g).await.unwrap());

    let mut c = CommunitySettings::new(g);
    c.rules_channel_id = Some(ch);
    c.updates_channel_id = Some(ch);
    c.safety_alerts_channel_id = Some(ch);
    c.community_enabled = true;
    c.invites_paused = true;
    repo.put_community(&c).await.unwrap();
    assert_eq!(repo.get_community(g).await.unwrap(), c);
    // G-76: the guild itself carries the three community channels.
    use sr_chat::repo::GuildRepository as _;
    let guild_row = PgChatRepo::new(pool.clone())
        .get_guild(g)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(guild_row.rules_channel_id, Some(ch));
    assert_eq!(guild_row.public_updates_channel_id, Some(ch));
    assert_eq!(guild_row.safety_alerts_channel_id, Some(ch));
    let ob = Onboarding {
        guild_id: g,
        enabled: false,
        default_channel_ids: vec![ch],
        prompts: vec![],
    };
    repo.put_onboarding(&ob).await.unwrap();
    assert_eq!(repo.get_onboarding(g).await.unwrap(), ob);
    let ws = WelcomeScreen {
        guild_id: g,
        enabled: true,
        description: Some("hi".into()),
        channels: vec![WelcomeChannel {
            channel_id: ch,
            description: "start".into(),
            emoji: None,
        }],
    };
    repo.put_welcome_screen(&ws).await.unwrap();
    assert_eq!(repo.get_welcome_screen(g).await.unwrap(), ws);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn webhook_update_replaces_the_token_hash() {
    use sr_chat::settings::repo::WebhookRepository;
    use sr_chat::settings::webhooks::{Webhook, hash_token};
    use sr_core::WebhookId;
    let pool = pool().await;
    let test_ids = ids(6);
    let repo = PgSettingsRepo::new(pool.clone());
    let (g, ch, owner) = guild(&pool, &test_ids).await;
    let mut w = Webhook::new_incoming(
        WebhookId::new(test_ids.generate().get()),
        g,
        ch,
        owner,
        "Deploys",
        hash_token("first"),
        NOW,
    )
    .unwrap();
    repo.insert_webhook(&w).await.unwrap();
    w.token_hash = Some(hash_token("second"));
    repo.update_webhook(&w).await.unwrap();
    let stored = repo.get_webhook(w.id).await.unwrap().unwrap();
    assert!(stored.token_matches(&hash_token("second")));
    assert!(!stored.token_matches(&hash_token("first")));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn webhook_avatar_key_round_trips_and_clears() {
    use sr_chat::settings::repo::WebhookRepository;
    use sr_chat::settings::webhooks::{Webhook, hash_token};
    use sr_core::WebhookId;
    let pool = pool().await;
    let test_ids = ids(7);
    let repo = PgSettingsRepo::new(pool.clone());
    let (g, ch, owner) = guild(&pool, &test_ids).await;
    let mut w = Webhook::new_incoming(
        WebhookId::new(test_ids.generate().get()),
        g,
        ch,
        owner,
        "Deploys",
        hash_token("t"),
        NOW,
    )
    .unwrap();
    w.avatar_key = Some("0123456789abcdef0123456789abcdef".into());
    repo.insert_webhook(&w).await.unwrap();
    let stored = repo.get_webhook(w.id).await.unwrap().unwrap();
    assert_eq!(stored.avatar_key, w.avatar_key);
    w.avatar_key = None;
    repo.update_webhook(&w).await.unwrap();
    assert!(
        repo.get_webhook(w.id)
            .await
            .unwrap()
            .unwrap()
            .avatar_key
            .is_none()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn server_tag_adoption_shows_only_for_members_while_allowed() {
    use sr_chat::repo::UserLookup;
    use sr_chat::settings::repo::TagRepository;
    use sr_chat::settings::tags::GuildTag;

    let pool = pool().await;
    let test_ids = ids(7);
    let repo = PgSettingsRepo::new(pool.clone());
    let (gid, _, owner) = guild(&pool, &test_ids).await;
    let lookup = PgChatRepo::new(pool.clone());
    let shown = |u: UserId| {
        let lookup = lookup.clone();
        async move { lookup.users(&[u]).await.unwrap().remove(0).primary_guild }
    };

    let mut tag = GuildTag {
        guild_id: gid,
        tag: "SIDE".into(),
        badge: 4,
        color: 0x00FF_00FF,
        members_can_display: true,
    };
    repo.put_guild_tag(&tag).await.unwrap();
    assert_eq!(repo.get_guild_tag(gid).await.unwrap(), Some(tag.clone()));
    assert_eq!(shown(owner).await, None, "nobody adopted yet");

    repo.set_primary_guild(owner, Some(gid)).await.unwrap();
    assert_eq!(repo.get_primary_guild(owner).await.unwrap(), Some(gid));
    let pg = shown(owner).await.expect("adopted tag is shown");
    assert_eq!((pg.guild_id, pg.tag.as_str(), pg.badge), (gid, "SIDE", 4));
    assert_eq!(pg.color, 0x00FF_00FF);

    // The toggle hides it everywhere without forgetting the adoption.
    tag.members_can_display = false;
    repo.put_guild_tag(&tag).await.unwrap();
    assert_eq!(shown(owner).await, None);
    tag.members_can_display = true;
    repo.put_guild_tag(&tag).await.unwrap();
    assert!(shown(owner).await.is_some());

    // Leaving the guild hides it.
    sqlx::query("DELETE FROM guild_members WHERE guild_id = $1 AND user_id = $2")
        .bind(gid.get() as i64)
        .bind(owner.get() as i64)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(shown(owner).await, None);

    // Deleting the tag clears every adoption of it.
    assert!(repo.delete_guild_tag(gid).await.unwrap());
    assert_eq!(repo.get_primary_guild(owner).await.unwrap(), None);
    assert!(!repo.delete_guild_tag(gid).await.unwrap());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn insights_are_computed_per_day_and_read_back() {
    use sr_chat::settings::repo::InsightsRepository;
    use sr_db::settings_repo::compute_insights_day;

    let pool = pool().await;
    let test_ids = ids(8);
    let repo = PgSettingsRepo::new(pool.clone());
    let (gid, general, _owner) = guild(&pool, &test_ids).await;
    let _: String = sqlx::query_scalar("SELECT sr_ensure_message_partition(CURRENT_DATE - 1)")
        .fetch_one(&pool)
        .await
        .unwrap();
    // Two messages at noon yesterday UTC.
    let day_start_ms: i64 = sqlx::query_scalar(
        "SELECT (extract(epoch FROM date_trunc('day', now() AT TIME ZONE 'UTC') - interval '1 day') * 1000)::bigint",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let noon_floor: i64 = sqlx::query_scalar(
        "SELECT sr_snowflake_floor(to_timestamp($1::double precision / 1000.0) + interval '12 hours')",
    )
    .bind(day_start_ms)
    .fetch_one(&pool)
    .await
    .unwrap();
    let author = account(&pool, &test_ids).await;
    // Distinct ids per run: the floor is the same for every run on a shared database.
    let jitter = i64::from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos(),
    ) % 1_000_000
        * 2;
    for k in 0..2 {
        sqlx::query(
            "INSERT INTO messages (id, channel_id, guild_id, author_id, content) \
             VALUES ($1, $2, $3, $4, 'hi')",
        )
        .bind(noon_floor + jitter + k)
        .bind(general.get() as i64)
        .bind(gid.get() as i64)
        .bind(author.get() as i64)
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "UPDATE guild_members SET joined_at = now() - interval '2 days' WHERE guild_id = $1",
    )
    .bind(gid.get() as i64)
    .execute(&pool)
    .await
    .unwrap();
    compute_insights_day(&pool, day_start_ms).await.unwrap();
    // Recomputing replaces rather than doubles.
    compute_insights_day(&pool, day_start_ms).await.unwrap();

    let now_ms = day_start_ms + 86_400_000 + 3_600_000;
    let ins = repo.insights(gid, 7, now_ms).await.unwrap();
    assert_eq!(ins.days.len(), 1);
    assert_eq!(ins.days[0].messages, 2);
    assert_eq!((ins.days[0].members, ins.days[0].joins), (1, 0));
    assert_eq!(ins.days[0].voice_minutes, 0);
    assert_eq!(ins.top_channels.len(), 1);
    assert_eq!(ins.top_channels[0].channel_id, general);
    assert_eq!(ins.top_channels[0].messages, 2);
    // A window that ends before that day sees nothing.
    let earlier = repo.insights(gid, 7, day_start_ms).await.unwrap();
    assert!(earlier.days.is_empty());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn guide_round_trips() {
    use sr_chat::settings::guide::{Guide, GuideResource, GuideTodo};
    use sr_chat::settings::repo::GuideRepository;

    let pool = pool().await;
    let test_ids = ids(9);
    let repo = PgSettingsRepo::new(pool.clone());
    let (gid, general, _) = guild(&pool, &test_ids).await;
    assert_eq!(
        repo.get_guide(gid).await.unwrap(),
        Guide {
            guild_id: gid,
            ..Guide::default()
        }
    );
    let g = Guide {
        guild_id: gid,
        welcome_message: Some("hello".into()),
        todos: vec![GuideTodo {
            id: 7,
            title: "Say hi".into(),
            description: Some("in general".into()),
            channel_id: Some(general),
        }],
        resources: vec![GuideResource {
            id: 8,
            title: "Rules".into(),
            content: "# Rules".into(),
        }],
    };
    repo.put_guide(&g).await.unwrap();
    assert_eq!(repo.get_guide(gid).await.unwrap(), g);
    let cleared = Guide {
        guild_id: gid,
        ..Guide::default()
    };
    repo.put_guide(&cleared).await.unwrap();
    assert_eq!(repo.get_guide(gid).await.unwrap(), cleared);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn applications_queue_and_approval_adds_the_member() {
    use sr_chat::settings::applications::{
        Answer, Application, ApplicationForm, ApplicationStatus, Question, QuestionKind,
    };
    use sr_chat::settings::repo::ApplicationRepository;

    let pool = pool().await;
    let test_ids = ids(10);
    let repo = PgSettingsRepo::new(pool.clone());
    let (gid, _, owner) = guild(&pool, &test_ids).await;
    let applicant = account(&pool, &test_ids).await;
    let other = account(&pool, &test_ids).await;

    let form = ApplicationForm {
        guild_id: gid,
        enabled: true,
        description: Some("hi".into()),
        questions: vec![Question {
            id: 5,
            label: "Why?".into(),
            kind: QuestionKind::Paragraph,
            required: true,
        }],
    };
    repo.put_application_form(&form).await.unwrap();
    assert_eq!(repo.get_application_form(gid).await.unwrap(), form);

    let app = |user: UserId| Application {
        id: test_ids.generate().get(),
        guild_id: gid,
        user_id: user,
        answers: vec![Answer {
            question_id: 5,
            value: "because".into(),
        }],
        status: ApplicationStatus::Pending,
        created_at_ms: NOW,
        reviewed_by: None,
        reviewed_at_ms: None,
        rejection_reason: None,
    };
    let first = app(applicant);
    assert!(repo.insert_application(&first).await.unwrap());
    assert!(
        !repo.insert_application(&app(applicant)).await.unwrap(),
        "one pending application per person"
    );
    let second = app(other);
    assert!(repo.insert_application(&second).await.unwrap());

    let pending = repo
        .list_applications(gid, Some(ApplicationStatus::Pending), None, 10)
        .await
        .unwrap();
    assert_eq!(
        pending.iter().map(|a| a.id).collect::<Vec<_>>(),
        vec![second.id, first.id],
        "newest first"
    );
    let paged = repo
        .list_applications(gid, None, Some(second.id), 10)
        .await
        .unwrap();
    assert_eq!(paged.len(), 1);

    // Rejecting keeps the person out and records the reason.
    let rejected = repo
        .decide_application(gid, second.id, false, owner, NOW + 5, Some("no"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rejected.status, ApplicationStatus::Rejected);
    assert_eq!(rejected.rejection_reason.as_deref(), Some("no"));
    assert_eq!(rejected.reviewed_by, Some(owner));
    let member_of = |user: UserId| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (SELECT 1 FROM guild_members WHERE guild_id = $1 AND user_id = $2)",
            )
            .bind(gid.get() as i64)
            .bind(user.get() as i64)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert!(!member_of(other).await);

    // Approving adds the member in the same transaction; a decided one cannot be redone.
    let approved = repo
        .decide_application(gid, first.id, true, owner, NOW + 9, Some("ignored"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(approved.status, ApplicationStatus::Approved);
    assert_eq!(approved.rejection_reason, None);
    assert_eq!(approved.reviewed_at_ms, Some(NOW + 9));
    assert!(member_of(applicant).await);
    assert!(
        repo.decide_application(gid, first.id, false, owner, NOW + 10, None)
            .await
            .unwrap()
            .is_none()
    );

    // A rejected person may apply again.
    assert!(repo.insert_application(&app(other)).await.unwrap());
    let latest = repo.latest_application(gid, other).await.unwrap().unwrap();
    assert_eq!(latest.status, ApplicationStatus::Pending);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn soundboard_sounds_round_trip_and_follow_their_guild() {
    use sr_chat::settings::soundboard::{Sound, SoundFormat};
    use sr_core::SoundId;

    let pool = pool().await;
    let test_ids = ids(7);
    let repo = PgSettingsRepo::new(pool.clone());
    let (g, _, owner) = guild(&pool, &test_ids).await;
    let (other, _, _) = guild(&pool, &test_ids).await;

    let mk = |name: &str, emoji: Option<&str>| Sound {
        id: SoundId::new(test_ids.generate().get()),
        guild_id: g,
        name: name.into(),
        volume: 0.75,
        emoji_id: None,
        emoji_name: emoji.map(str::to_owned),
        available: true,
        creator_id: owner,
        media_key: format!("sound/{g}/{name}.wav"),
        format: SoundFormat::Wav,
        duration_ms: 1_250,
        created_at_ms: NOW,
    };
    let a = mk("ding", Some("\u{1F514}"));
    let b = mk("dong", None);
    repo.insert_sound(&a).await.unwrap();
    repo.insert_sound(&b).await.unwrap();

    let list = repo.list_sounds(g).await.unwrap();
    assert_eq!(list.len(), 2);
    let got = repo.get_sound(g, a.id).await.unwrap().unwrap();
    assert_eq!(got.name, "ding");
    assert_eq!(got.volume, 0.75);
    assert_eq!(got.emoji_name.as_deref(), Some("\u{1F514}"));
    assert_eq!(got.format, SoundFormat::Wav);
    assert_eq!(got.duration_ms, 1_250);
    assert_eq!(got.media_key, a.media_key);
    assert_eq!(got.creator_id, owner);

    // Another server cannot see or touch it.
    assert!(repo.get_sound(other, a.id).await.unwrap().is_none());
    assert!(repo.list_sounds(other).await.unwrap().is_empty());
    assert!(!repo.delete_sound(other, a.id).await.unwrap());

    // Update keeps the file and the author.
    let mut changed = got.clone();
    changed.name = "bong".into();
    changed.volume = 0.2;
    changed.emoji_name = None;
    repo.update_sound(&changed).await.unwrap();
    let after = repo.get_sound(g, a.id).await.unwrap().unwrap();
    assert_eq!((after.name.as_str(), after.volume), ("bong", 0.2));
    assert!(after.emoji_name.is_none());
    assert_eq!(after.media_key, a.media_key);

    // The database refuses what the domain would: a name that is too short, a sound longer
    // than five seconds, an unknown format.
    let mut bad = mk("ok", None);
    bad.name = "x".into();
    assert!(repo.insert_sound(&bad).await.is_err());
    let mut long = mk("long", None);
    long.duration_ms = 5_001;
    assert!(repo.insert_sound(&long).await.is_err());

    assert!(repo.delete_sound(g, a.id).await.unwrap());
    assert!(!repo.delete_sound(g, a.id).await.unwrap());
    assert_eq!(repo.list_sounds(g).await.unwrap().len(), 1);
    // Deleting the guild takes its sounds along.
    sqlx::query("DELETE FROM guilds WHERE id = $1")
        .bind(g.get() as i64)
        .execute(&pool)
        .await
        .unwrap();
    assert!(repo.list_sounds(g).await.unwrap().is_empty());
}
