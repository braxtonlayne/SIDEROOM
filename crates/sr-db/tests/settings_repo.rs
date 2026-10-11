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
        format!("T{}", test_ids.generate().get()),
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
    c.community_enabled = true;
    c.invites_paused = true;
    repo.put_community(&c).await.unwrap();
    assert_eq!(repo.get_community(g).await.unwrap(), c);
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
