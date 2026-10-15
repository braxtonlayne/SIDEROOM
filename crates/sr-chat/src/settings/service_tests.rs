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

#[tokio::test]
async fn module_audit_entries_name_the_system_user_and_module() {
    let (svc, _clock) = setup();
    svc.record_audit_as_module(
        G,
        AuditAction::RoleUpdate,
        Some(7),
        vec![],
        None,
        ("lounge.auto_roles", "Auto roles"),
    )
    .await
    .unwrap();
    let all = svc
        .list_audit(Permissions::VIEW_AUDIT_LOG, &AuditQuery::new(G))
        .await
        .unwrap();
    assert_eq!(all[0].actor_id, UserId::new(1));
    let opts = all[0].options.as_ref().unwrap();
    assert_eq!(opts["module"], "Auto roles");
    assert_eq!(opts["module_id"], "lounge.auto_roles");
}

// ---- scheduled events: the clock, voice joins and stage links (G-91, G-93) ----------------

mod events_clock {
    use super::*;
    use crate::settings::scheduled_events::{
        ChannelKind, EntityType, EventPatch, EventStatus, EventWorld, NewEvent, VoiceView,
    };
    use std::sync::Mutex;

    const HOUR: i64 = 3_600_000;
    const MIN: i64 = 60_000;
    const T0: i64 = 1_800_000_000_000;
    const VC: ChannelId = ChannelId::new(30);
    const STAGE: ChannelId = ChannelId::new(31);

    /// Occupancy, live stages and a log of what the service asked of the stage side.
    #[derive(Default)]
    struct World {
        occupants: Mutex<Vec<(ChannelId, usize)>>,
        live: Mutex<Vec<ChannelId>>,
        log: Mutex<Vec<String>>,
        stage_busy: Mutex<bool>,
    }

    #[async_trait::async_trait]
    impl EventWorld for World {
        async fn view(&self, _channels: &[ChannelId]) -> VoiceView {
            VoiceView {
                occupants: self.occupants.lock().unwrap().iter().copied().collect(),
                live_stages: self.live.lock().unwrap().iter().copied().collect(),
            }
        }
        async fn start_stage(
            &self,
            _guild: GuildId,
            channel: ChannelId,
            topic: &str,
            _by: UserId,
            _event_id: u64,
        ) -> Result<(), String> {
            if *self.stage_busy.lock().unwrap() {
                return Err("A stage is already running in this channel.".into());
            }
            self.log
                .lock()
                .unwrap()
                .push(format!("start {channel} {topic}"));
            self.live.lock().unwrap().push(channel);
            Ok(())
        }
        async fn end_stage(&self, channel: ChannelId) {
            self.log.lock().unwrap().push(format!("end {channel}"));
            self.live.lock().unwrap().retain(|c| *c != channel);
        }
    }

    fn manage() -> Permissions {
        Permissions::MANAGE_EVENTS
    }

    fn start_patch() -> EventPatch {
        EventPatch {
            status: Some(EventStatus::Active),
            ..EventPatch::default()
        }
    }

    async fn make(
        svc: &SettingsService<MemorySettingsRepo>,
        entity: EntityType,
        channel: Option<ChannelId>,
        start: i64,
    ) -> u64 {
        let kind = match entity {
            EntityType::Voice => ChannelKind::Voice,
            EntityType::StageInstance => ChannelKind::Stage,
            EntityType::External => ChannelKind::Missing,
        };
        svc.create_event(
            manage(),
            ADMIN,
            G,
            NewEvent {
                id: None,
                image_key: None,
                channel_id: channel,
                name: "Movie night".into(),
                description: None,
                start_ms: start,
                end_ms: (entity == EntityType::External).then_some(start + HOUR),
                entity_type: entity,
                location: (entity == EntityType::External).then(|| "the park".into()),
            },
            kind,
        )
        .await
        .unwrap()
        .id
    }

    async fn audit_updates(svc: &SettingsService<MemorySettingsRepo>) -> usize {
        svc.list_audit(Permissions::VIEW_AUDIT_LOG, &AuditQuery::new(G))
            .await
            .unwrap()
            .into_iter()
            .filter(|e| e.action == AuditAction::ScheduledEventUpdate)
            .count()
    }

    #[tokio::test]
    async fn the_clock_starts_and_completes_external_events_and_audits_each_move() {
        let (svc, clock) = setup();
        let w = World::default();
        let id = make(&svc, EntityType::External, None, T0 + HOUR).await;
        assert!(svc.advance_events(&w).await.unwrap().is_empty());
        clock.advance_ms(HOUR);
        let moved = svc.advance_events(&w).await.unwrap();
        assert_eq!(moved.len(), 1);
        assert!(moved[0].started());
        assert_eq!(audit_updates(&svc).await, 1);
        // A second node (or tick) finds nothing left to do.
        assert!(svc.advance_events(&w).await.unwrap().is_empty());
        clock.advance_ms(HOUR);
        let done = svc.advance_events(&w).await.unwrap();
        assert_eq!(done[0].event.status, EventStatus::Completed);
        assert_eq!(audit_updates(&svc).await, 2);
        assert_eq!(
            svc.get_event(G, id).await.unwrap().status,
            EventStatus::Completed
        );
    }

    #[tokio::test]
    async fn the_first_join_at_or_after_the_start_time_starts_a_voice_event() {
        let (svc, clock) = setup();
        let w = World::default();
        let id = make(&svc, EntityType::Voice, Some(VC), T0 + HOUR).await;
        // Too early, and the wrong channel.
        assert!(svc.start_on_voice_join(G, VC, &w).await.unwrap().is_none());
        clock.advance_ms(HOUR);
        assert!(
            svc.start_on_voice_join(G, ChannelId::new(99), &w)
                .await
                .unwrap()
                .is_none()
        );
        let started = svc.start_on_voice_join(G, VC, &w).await.unwrap().unwrap();
        assert!(started.started());
        assert_eq!(started.event.id, id);
        assert_eq!(audit_updates(&svc).await, 1);
        // A second join changes nothing.
        assert!(svc.start_on_voice_join(G, VC, &w).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn an_unstarted_event_is_canceled_after_a_day_and_an_empty_one_ends_after_15_minutes() {
        let (svc, clock) = setup();
        let w = World::default();
        let stale = make(&svc, EntityType::Voice, Some(VC), T0 + HOUR).await;
        clock.advance_ms(HOUR + 24 * HOUR - 1);
        assert!(svc.advance_events(&w).await.unwrap().is_empty());
        clock.advance_ms(1);
        let c = svc.advance_events(&w).await.unwrap();
        assert_eq!(c[0].event.id, stale);
        assert_eq!(c[0].event.status, EventStatus::Canceled);

        // A manually started event whose channel nobody enters.
        let now = T0 + 25 * HOUR;
        let id = make(&svc, EntityType::Voice, Some(VC), now + HOUR).await;
        svc.update_event(
            manage(),
            ADMIN,
            G,
            id,
            start_patch(),
            ChannelKind::Voice,
            &w,
        )
        .await
        .unwrap();
        assert!(
            svc.advance_events(&w).await.unwrap().is_empty(),
            "countdown starts"
        );
        clock.advance_ms(14 * MIN);
        assert!(svc.advance_events(&w).await.unwrap().is_empty());
        w.occupants.lock().unwrap().push((VC, 2));
        clock.advance_ms(2 * MIN);
        assert!(
            svc.advance_events(&w).await.unwrap().is_empty(),
            "someone is there"
        );
        w.occupants.lock().unwrap().clear();
        assert!(
            svc.advance_events(&w).await.unwrap().is_empty(),
            "countdown restarts"
        );
        clock.advance_ms(15 * MIN);
        let done = svc.advance_events(&w).await.unwrap();
        assert_eq!(done[0].event.status, EventStatus::Completed);
    }

    #[tokio::test]
    async fn a_stage_event_opens_and_closes_its_stage_instance() {
        let (svc, clock) = setup();
        let w = World::default();
        let id = make(&svc, EntityType::StageInstance, Some(STAGE), T0 + HOUR).await;
        // A stage already running there blocks the start.
        *w.stage_busy.lock().unwrap() = true;
        let busy = svc
            .update_event(
                manage(),
                ADMIN,
                G,
                id,
                start_patch(),
                ChannelKind::Stage,
                &w,
            )
            .await;
        assert!(busy.is_err());
        assert_eq!(
            svc.get_event(G, id).await.unwrap().status,
            EventStatus::Scheduled
        );
        *w.stage_busy.lock().unwrap() = false;

        let ev = svc
            .update_event(
                manage(),
                ADMIN,
                G,
                id,
                start_patch(),
                ChannelKind::Stage,
                &w,
            )
            .await
            .unwrap();
        assert_eq!(ev.entity_id, Some(STAGE.get()));
        assert_eq!(
            w.log.lock().unwrap().as_slice(),
            [format!("start {STAGE} Movie night")]
        );
        w.occupants.lock().unwrap().push((STAGE, 1));
        assert!(svc.advance_events(&w).await.unwrap().is_empty());

        // The stage instance is deleted from outside: the event completes.
        w.live.lock().unwrap().clear();
        clock.advance_ms(MIN);
        let done = svc.advance_events(&w).await.unwrap();
        assert_eq!(done[0].event.status, EventStatus::Completed);
        assert_eq!(done[0].event.entity_id, None);

        // Ending a running stage event by hand deletes the instance.
        let id2 = make(&svc, EntityType::StageInstance, Some(STAGE), T0 + 50 * HOUR).await;
        svc.update_event(
            manage(),
            ADMIN,
            G,
            id2,
            start_patch(),
            ChannelKind::Stage,
            &w,
        )
        .await
        .unwrap();
        assert!(w.live.lock().unwrap().contains(&STAGE));
        let end = EventPatch {
            status: Some(EventStatus::Completed),
            ..EventPatch::default()
        };
        svc.update_event(manage(), ADMIN, G, id2, end, ChannelKind::Stage, &w)
            .await
            .unwrap();
        assert!(w.live.lock().unwrap().is_empty());
        assert_eq!(
            w.log.lock().unwrap().last().map(String::as_str),
            Some(format!("end {STAGE}").as_str())
        );
    }

    #[tokio::test]
    async fn all_interested_pages_through_every_member() {
        let (svc, _) = setup();
        let id = make(&svc, EntityType::External, None, T0 + HOUR).await;
        for n in 1..=130u64 {
            svc.set_interested(G, id, UserId::new(1000 + n), true)
                .await
                .unwrap();
        }
        assert_eq!(svc.all_interested(id).await.unwrap().len(), 130);
    }
}
