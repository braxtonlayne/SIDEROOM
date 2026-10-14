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

fn new_sound(name: &str) -> super::service::NewSound {
    super::service::NewSound {
        name: name.into(),
        volume: 0.8,
        emoji_id: None,
        emoji_name: Some("\u{1F514}".into()),
        format: super::soundboard::SoundFormat::Wav,
        size_bytes: 40_000,
        duration_ms: 1_000,
        media_key: "sounds/1/a.wav".into(),
    }
}

#[tokio::test]
async fn soundboard_sounds_follow_discords_permissions_and_are_audited() {
    use super::service::SoundPatch;
    let (svc, _) = setup();
    let manage = Permissions::MANAGE_GUILD_EXPRESSIONS;
    let create = Permissions::CREATE_GUILD_EXPRESSIONS;
    let other = UserId::new(11);

    // Creating needs either expression permission.
    assert!(matches!(
        svc.create_sound(Permissions::empty(), ADMIN, G, new_sound("ding"))
            .await,
        Err(SettingsError::MissingPermissions)
    ));
    let s = svc
        .create_sound(create, ADMIN, G, new_sound(" ding "))
        .await
        .unwrap();
    assert_eq!(s.name, "ding");
    assert_eq!(s.creator_id, ADMIN);
    assert!(
        svc.create_sound(manage, other, G, new_sound("boop"))
            .await
            .is_ok()
    );

    // A creator edits and deletes their own sound; another creator cannot; a manager can.
    let patch = SoundPatch {
        name: Some("dong".into()),
        volume: Some(0.5),
        ..SoundPatch::default()
    };
    assert!(matches!(
        svc.edit_sound(create, other, G, s.id, patch.clone()).await,
        Err(SettingsError::MissingPermissions)
    ));
    let edited = svc.edit_sound(create, ADMIN, G, s.id, patch).await.unwrap();
    assert_eq!((edited.name.as_str(), edited.volume), ("dong", 0.5));
    // Nothing changed: no write and no audit entry.
    let before = svc
        .list_audit(Permissions::VIEW_AUDIT_LOG, &AuditQuery::new(G))
        .await
        .unwrap()
        .len();
    svc.edit_sound(manage, other, G, s.id, SoundPatch::default())
        .await
        .unwrap();
    assert_eq!(
        svc.list_audit(Permissions::VIEW_AUDIT_LOG, &AuditQuery::new(G))
            .await
            .unwrap()
            .len(),
        before
    );
    // Clearing the emoji.
    let cleared = svc
        .edit_sound(
            manage,
            other,
            G,
            s.id,
            SoundPatch {
                emoji: Some(None),
                ..SoundPatch::default()
            },
        )
        .await
        .unwrap();
    assert!(cleared.emoji_name.is_none() && cleared.emoji_id.is_none());

    assert!(matches!(
        svc.delete_sound(create, other, G, s.id).await,
        Err(SettingsError::MissingPermissions)
    ));
    svc.delete_sound(manage, other, G, s.id).await.unwrap();
    assert!(matches!(
        svc.delete_sound(manage, other, G, s.id).await,
        Err(SettingsError::UnknownSound)
    ));
    assert!(matches!(
        svc.edit_sound(manage, other, G, s.id, SoundPatch::default())
            .await,
        Err(SettingsError::UnknownSound)
    ));

    let log = svc
        .list_audit(Permissions::VIEW_AUDIT_LOG, &AuditQuery::new(G))
        .await
        .unwrap();
    let kinds: Vec<_> = log.iter().map(|e| e.action).collect();
    assert!(kinds.contains(&AuditAction::SoundboardSoundCreate));
    assert!(kinds.contains(&AuditAction::SoundboardSoundUpdate));
    assert!(kinds.contains(&AuditAction::SoundboardSoundDelete));
}

#[tokio::test]
async fn soundboard_uploads_are_checked_and_limited() {
    let (svc, _) = setup();
    let manage = Permissions::MANAGE_GUILD_EXPRESSIONS;
    for bad in [
        super::service::NewSound {
            duration_ms: 5_001,
            ..new_sound("long")
        },
        super::service::NewSound {
            size_bytes: 600 * 1024,
            ..new_sound("big")
        },
        super::service::NewSound {
            volume: 1.5,
            ..new_sound("loud")
        },
        new_sound("x"),
    ] {
        assert!(matches!(
            svc.create_sound(manage, ADMIN, G, bad).await,
            Err(SettingsError::Invalid { .. })
        ));
    }
    for i in 0..48 {
        svc.create_sound(manage, ADMIN, G, new_sound(&format!("snd{i}")))
            .await
            .unwrap();
    }
    assert!(matches!(
        svc.create_sound(manage, ADMIN, G, new_sound("one more"))
            .await,
        Err(SettingsError::LimitReached("soundboard sounds"))
    ));
    // Another server has its own 48.
    assert!(
        svc.create_sound(manage, ADMIN, GuildId::new(2), new_sound("fresh"))
            .await
            .is_ok()
    );
}
