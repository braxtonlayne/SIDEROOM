use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use sr_core::permissions::Permissions;
use sr_core::{ChannelId, Clock, GuildId, ManualClock, SnowflakeGenerator, UserId};

use super::audit::{AuditAction, AuditQuery, AuditRetention, DAY_MS};
use super::automod::{Action, AutoModRule, EventType, MessageContext, Trigger};
use super::error::SettingsError;
use super::expressions::{ExpressionLimits, StickerFormat};
use super::invites::InviteOptions;
use super::memory::MemorySettingsRepo;
use super::service::{NewEmoji, NewSticker, SettingsService};

const G: GuildId = GuildId::new(1);
const ADMIN: UserId = UserId::new(10);
const CH: ChannelId = ChannelId::new(20);

fn setup() -> (SettingsService<MemorySettingsRepo>, Arc<ManualClock>) {
    let clock = Arc::new(ManualClock::new(1_800_000_000_000));
    let dyn_clock: Arc<dyn Clock> = clock.clone();
    let ids = SnowflakeGenerator::new(dyn_clock.clone(), 1, 1).unwrap();
    let counter = Arc::new(AtomicU64::new(0x9e37_79b9_7f4a_7c15));
    let entropy = Arc::new(move || {
        counter
            .fetch_add(0x1234_5678_9abc_def1, Ordering::SeqCst)
            .wrapping_mul(0xff51_afd7_ed55_8ccd)
    });
    (
        SettingsService::new(MemorySettingsRepo::default(), dyn_clock, ids, entropy),
        clock,
    )
}

fn opts(age: u32, uses: u32) -> InviteOptions {
    InviteOptions {
        max_age_secs: age,
        max_uses: uses,
        temporary: false,
    }
}

#[tokio::test]
async fn invite_lifecycle_expiry_and_max_uses() {
    let (svc, clock) = setup();
    let p = Permissions::CREATE_INSTANT_INVITE;
    assert!(matches!(
        svc.create_invite(Permissions::empty(), G, CH, ADMIN, opts(0, 0))
            .await,
        Err(SettingsError::MissingPermissions)
    ));
    let inv = svc
        .create_invite(p, G, CH, ADMIN, opts(60, 2))
        .await
        .unwrap();
    let u = |n: u64| UserId::new(100 + n);
    assert_eq!(svc.use_invite(&inv.code, u(1)).await.unwrap().uses, 1);
    assert_eq!(svc.use_invite(&inv.code, u(2)).await.unwrap().uses, 2);
    assert!(matches!(
        svc.use_invite(&inv.code, u(3)).await,
        Err(SettingsError::InviteExhausted)
    ));

    let timed = svc
        .create_invite(p, G, CH, ADMIN, opts(60, 0))
        .await
        .unwrap();
    clock.advance_ms(61_000);
    assert!(matches!(
        svc.use_invite(&timed.code, u(4)).await,
        Err(SettingsError::InviteExpired)
    ));
    assert!(matches!(
        svc.use_invite("nope", u(4)).await,
        Err(SettingsError::UnknownInvite)
    ));
    // Creation was audited.
    let log = svc
        .list_audit(Permissions::VIEW_AUDIT_LOG, &AuditQuery::new(G))
        .await
        .unwrap();
    assert_eq!(
        log.iter()
            .filter(|e| e.action == AuditAction::InviteCreate)
            .count(),
        2
    );
}

#[tokio::test]
async fn paused_banned_and_revoked_invites() {
    let (svc, _) = setup();
    let p = Permissions::CREATE_INSTANT_INVITE;
    let inv = svc
        .create_invite(p, G, CH, ADMIN, opts(0, 0))
        .await
        .unwrap();

    svc.set_invites_paused(Permissions::MANAGE_GUILD, ADMIN, G, true)
        .await
        .unwrap();
    assert!(matches!(
        svc.use_invite(&inv.code, UserId::new(1)).await,
        Err(SettingsError::InvitesPaused)
    ));
    svc.set_invites_paused(Permissions::MANAGE_GUILD, ADMIN, G, false)
        .await
        .unwrap();

    svc.repo().with(|s| s.bans.insert((G, UserId::new(2))));
    assert!(matches!(
        svc.use_invite(&inv.code, UserId::new(2)).await,
        Err(SettingsError::Banned)
    ));
    assert!(svc.use_invite(&inv.code, UserId::new(3)).await.is_ok());

    assert!(matches!(
        svc.revoke_invite(Permissions::empty(), ADMIN, G, &inv.code, None)
            .await,
        Err(SettingsError::MissingPermissions)
    ));
    svc.revoke_invite(
        Permissions::MANAGE_GUILD,
        ADMIN,
        G,
        &inv.code,
        Some("leaked"),
    )
    .await
    .unwrap();
    assert!(matches!(
        svc.use_invite(&inv.code, UserId::new(4)).await,
        Err(SettingsError::UnknownInvite)
    ));
    // Wrong guild cannot revoke.
    assert!(
        svc.revoke_invite(
            Permissions::MANAGE_GUILD,
            ADMIN,
            GuildId::new(2),
            &inv.code,
            None
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn audit_filters_and_retention() {
    let (svc, clock) = setup();
    for (actor, action) in [
        (1, AuditAction::RoleCreate),
        (2, AuditAction::RoleDelete),
        (1, AuditAction::MemberKick),
    ] {
        svc.record_audit(
            G,
            UserId::new(actor),
            action,
            Some(5),
            vec![],
            Some("because"),
        )
        .await
        .unwrap();
        clock.advance_ms(1);
    }
    let v = Permissions::VIEW_AUDIT_LOG;
    let all = svc.list_audit(v, &AuditQuery::new(G)).await.unwrap();
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].action, AuditAction::MemberKick); // newest first
    assert_eq!(all[0].reason.as_deref(), Some("because"));

    let mut q = AuditQuery::new(G);
    q.actor_id = Some(UserId::new(1));
    assert_eq!(svc.list_audit(v, &q).await.unwrap().len(), 2);
    q.action = Some(AuditAction::RoleCreate);
    assert_eq!(svc.list_audit(v, &q).await.unwrap().len(), 1);
    let mut page = AuditQuery::new(G);
    page.limit = 1;
    page.before = Some(all[0].id);
    assert_eq!(svc.list_audit(v, &page).await.unwrap()[0].id, all[1].id);
    assert!(
        svc.list_audit(Permissions::empty(), &AuditQuery::new(G))
            .await
            .is_err()
    );
    assert!(
        svc.record_audit(
            G,
            ADMIN,
            AuditAction::GuildUpdate,
            None,
            vec![],
            Some(&"x".repeat(600))
        )
        .await
        .is_err()
    );

    assert_eq!(svc.purge_audit().await.unwrap(), 0);
    clock.advance_ms(46 * DAY_MS);
    assert_eq!(svc.purge_audit().await.unwrap(), 3);
    let keep = setup().0.with_retention(AuditRetention { keep_days: 0 });
    assert_eq!(keep.purge_audit().await.unwrap(), 0);
}

#[tokio::test]
async fn emoji_and_sticker_limits() {
    let (svc, _) = setup();
    let svc = svc.with_limits(ExpressionLimits {
        max_static_emoji: 1,
        max_stickers: 1,
        ..Default::default()
    });
    let p = Permissions::MANAGE_GUILD_EXPRESSIONS;
    let new = |name: &str| NewEmoji {
        name: name.into(),
        animated: false,
        size_bytes: 1000,
        media_key: "k".into(),
        role_ids: vec![],
    };
    let e = svc.create_emoji(p, ADMIN, G, new("party")).await.unwrap();
    assert!(matches!(
        svc.create_emoji(p, ADMIN, G, new("second")).await,
        Err(SettingsError::LimitReached(_))
    ));
    assert!(
        svc.create_emoji(Permissions::empty(), ADMIN, G, new("third"))
            .await
            .is_err()
    );
    let renamed = svc.rename_emoji(p, ADMIN, G, e.id, "party2").await.unwrap();
    assert_eq!(renamed.name, "party2");
    svc.delete_emoji(p, ADMIN, G, e.id).await.unwrap();
    assert!(svc.create_emoji(p, ADMIN, G, new("second")).await.is_ok());
    assert!(matches!(
        svc.delete_emoji(p, ADMIN, G, e.id).await,
        Err(SettingsError::UnknownEmoji)
    ));

    let sticker = |n: &str| NewSticker {
        name: n.into(),
        description: None,
        tags: "wave".into(),
        format: StickerFormat::Png,
        size_bytes: 1000,
        media_key: "s".into(),
    };
    svc.create_sticker(p, ADMIN, G, sticker("hello"))
        .await
        .unwrap();
    assert!(matches!(
        svc.create_sticker(p, ADMIN, G, sticker("again")).await,
        Err(SettingsError::LimitReached(_))
    ));
}

#[tokio::test]
async fn automod_rules_are_enforced_and_checked() {
    let (svc, _) = setup();
    let m = Permissions::MANAGE_GUILD;
    let rule = AutoModRule {
        id: 0,
        guild_id: G,
        name: "bad words".into(),
        creator_id: UserId::default(),
        event_type: EventType::MessageSend,
        trigger: Trigger::Keyword {
            keywords: vec!["heck*".into()],
            regex_patterns: vec![],
            allow_list: vec![],
        },
        actions: vec![Action::Block {
            custom_message: Some("no".into()),
        }],
        enabled: true,
        exempt_role_ids: vec![],
        exempt_channel_ids: vec![],
    };
    assert!(
        svc.put_automod_rule(Permissions::empty(), ADMIN, rule.clone())
            .await
            .is_err()
    );
    let saved = svc.put_automod_rule(m, ADMIN, rule.clone()).await.unwrap();
    assert_ne!(saved.id, 0);
    let ctx = MessageContext {
        author_id: UserId::new(5),
        channel_id: CH,
        content: "what the heckin".into(),
        ..Default::default()
    };
    let hits = svc.check_message(G, &ctx).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].rule_id, saved.id);

    // Other guilds are unaffected; bad regexes are rejected before saving.
    assert!(
        svc.check_message(GuildId::new(2), &ctx)
            .await
            .unwrap()
            .is_empty()
    );
    let mut bad = rule.clone();
    bad.trigger = Trigger::Keyword {
        keywords: vec![],
        regex_patterns: vec!["(".into()],
        allow_list: vec![],
    };
    assert!(svc.put_automod_rule(m, ADMIN, bad).await.is_err());
    // Updating an unknown id fails.
    let mut ghost = saved.clone();
    ghost.id = 12345;
    assert!(matches!(
        svc.put_automod_rule(m, ADMIN, ghost).await,
        Err(SettingsError::UnknownRule)
    ));
    svc.delete_automod_rule(m, ADMIN, G, saved.id)
        .await
        .unwrap();
    assert!(svc.check_message(G, &ctx).await.unwrap().is_empty());
}

#[tokio::test]
async fn spam_rule_uses_the_member_window_and_the_rule_cache() {
    let (svc, _clock) = setup();
    let m = Permissions::MANAGE_GUILD;
    let rule = AutoModRule {
        id: 0,
        guild_id: G,
        name: "spam".into(),
        creator_id: UserId::default(),
        event_type: EventType::MessageSend,
        trigger: Trigger::Spam,
        actions: vec![Action::Block {
            custom_message: None,
        }],
        enabled: true,
        exempt_role_ids: vec![],
        exempt_channel_ids: vec![],
    };
    let saved = svc.put_automod_rule(m, ADMIN, rule).await.unwrap();
    let ctx = MessageContext {
        author_id: UserId::new(5),
        channel_id: CH,
        content: "buy now".into(),
        ..Default::default()
    };
    // Defaults: the fourth identical message inside the window is a repeat.
    for _ in 0..3 {
        assert!(svc.check_message(G, &ctx).await.unwrap().is_empty());
    }
    let hits = svc.check_message(G, &ctx).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].rule_id, saved.id);
    assert_eq!(hits[0].matched, "duplicate messages");
    // Another author is unaffected; a bypass author is never tracked.
    let other = MessageContext {
        author_id: UserId::new(6),
        ..ctx.clone()
    };
    assert!(svc.check_message(G, &other).await.unwrap().is_empty());
    let exempt = MessageContext {
        bypass: true,
        ..ctx
    };
    assert!(svc.check_message(G, &exempt).await.unwrap().is_empty());
}

#[tokio::test]
async fn audit_observer_sees_each_stored_entry() {
    let (svc, _clock) = setup();
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = seen.clone();
    let svc = svc.with_audit_observer(Arc::new(move |e| {
        sink.lock().unwrap().push((e.id, e.action));
    }));
    let e = svc
        .record_audit(G, ADMIN, AuditAction::RoleCreate, Some(5), vec![], None)
        .await
        .unwrap();
    assert_eq!(*seen.lock().unwrap(), vec![(e.id, AuditAction::RoleCreate)]);
}

#[tokio::test]
async fn onboarding_saves_log_each_question_and_the_settings_separately() {
    use super::community::{Onboarding, OnboardingOption, OnboardingPrompt};
    let (svc, _) = setup();
    let prompt = |id: u64, title: &str| OnboardingPrompt {
        id,
        title: title.into(),
        single_select: true,
        required: false,
        in_onboarding: true,
        options: vec![OnboardingOption {
            id: id * 10,
            title: "Option".into(),
            description: None,
            emoji: None,
            channel_ids: vec![],
            role_ids: vec![],
        }],
    };
    let save = |o: Onboarding| svc.save_onboarding(Permissions::MANAGE_GUILD, ADMIN, o, |_| true);
    let base = |prompts: Vec<OnboardingPrompt>, enabled: bool| Onboarding {
        guild_id: G,
        enabled,
        default_channel_ids: if enabled {
            (30..37).map(ChannelId::new).collect()
        } else {
            vec![]
        },
        prompts,
    };
    save(base(vec![prompt(1, "Pick"), prompt(2, "Gone")], false))
        .await
        .unwrap();
    save(base(vec![prompt(1, "Pick one")], true)).await.unwrap();

    let log = svc
        .list_audit(Permissions::VIEW_AUDIT_LOG, &AuditQuery::new(G))
        .await
        .unwrap();
    let count = |a: AuditAction| log.iter().filter(|e| e.action == a).count();
    assert_eq!(count(AuditAction::OnboardingPromptCreate), 2);
    assert_eq!(count(AuditAction::OnboardingPromptUpdate), 1);
    assert_eq!(count(AuditAction::OnboardingPromptDelete), 1);
    // Only the second save flipped `enabled`.
    assert_eq!(count(AuditAction::OnboardingUpdate), 1);
}

#[tokio::test]
async fn guild_tags_are_adopted_only_while_members_may_display_them() {
    use super::tags::GuildTag;
    let (svc, _) = setup();
    let member = UserId::new(50);
    let tag = |text: &str, display: bool| GuildTag {
        guild_id: G,
        tag: text.into(),
        badge: 2,
        color: 0xAA00FF,
        members_can_display: display,
    };
    // Managers only.
    assert!(matches!(
        svc.save_guild_tag(Permissions::empty(), ADMIN, tag("SIDE", true))
            .await,
        Err(SettingsError::MissingPermissions)
    ));
    // No tag yet, and a non-member, cannot adopt.
    assert!(svc.adopt_tag(member, Some(G), true).await.is_err());
    svc.save_guild_tag(Permissions::MANAGE_GUILD, ADMIN, tag("SIDE", true))
        .await
        .unwrap();
    assert!(svc.adopt_tag(member, Some(G), false).await.is_err());
    let adopted = svc.adopt_tag(member, Some(G), true).await.unwrap().unwrap();
    assert_eq!((adopted.tag.as_str(), adopted.badge), ("SIDE", 2));
    assert_eq!(svc.primary_guild(member).await.unwrap(), Some(adopted));

    // Switching the toggle off hides it everywhere, and nobody can newly adopt.
    svc.save_guild_tag(Permissions::MANAGE_GUILD, ADMIN, tag("SIDE", false))
        .await
        .unwrap();
    assert_eq!(svc.primary_guild(member).await.unwrap(), None);
    assert!(svc.adopt_tag(UserId::new(51), Some(G), true).await.is_err());

    // Removing the tag drops adoptions; clearing is always allowed.
    svc.save_guild_tag(Permissions::MANAGE_GUILD, ADMIN, tag("SIDE", true))
        .await
        .unwrap();
    assert!(svc.primary_guild(member).await.unwrap().is_some());
    svc.clear_guild_tag(Permissions::MANAGE_GUILD, ADMIN, G)
        .await
        .unwrap();
    assert_eq!(svc.primary_guild(member).await.unwrap(), None);
    assert_eq!(svc.adopt_tag(member, None, false).await.unwrap(), None);
    // Tag changes were audited as GuildUpdate with key diffs.
    let log = svc
        .list_audit(Permissions::VIEW_AUDIT_LOG, &AuditQuery::new(G))
        .await
        .unwrap();
    assert!(
        log.iter()
            .any(|e| e.changes.iter().any(|c| c.key == "tag_members_can_display"))
    );
}
