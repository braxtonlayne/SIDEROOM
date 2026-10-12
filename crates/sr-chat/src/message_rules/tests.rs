use super::*;
use crate::perms::{default_everyone_permissions, fixtures};
use sr_core::{AttachmentId, GuildId, Snowflake};

const NOW: i64 = 1_800_000_000_000;
const AUTHOR: u64 = 10;

fn limits() -> ChatLimits {
    ChatLimits::default()
}

fn text_channel() -> Channel {
    Channel::new(
        ChannelId::new(50),
        Some(GuildId::new(fixtures::GUILD)),
        ChannelType::Text,
    )
}

fn msg(id: u64, author: u64, content: &str) -> Message {
    Message {
        id: MessageId::new(id),
        channel_id: ChannelId::new(50),
        guild_id: Some(GuildId::new(fixtures::GUILD)),
        author_id: UserId::new(author),
        kind: MessageType::Default,
        content: content.into(),
        flags: 0,
        edited_at_ms: None,
        reference: None,
        attachment_ids: vec![],
        embeds: vec![],
        components: vec![],
        sticker_ids: vec![],
        poll: None,
        mention_everyone: false,
        mention_user_ids: vec![],
        mention_role_ids: vec![],
        mention_channel_ids: vec![],
        pinned: false,
        visibility: Visibility::Visible,
        deleted_at_ms: None,
        purge_after_ms: None,
        hold_id: None,
    }
}

/// A snowflake whose embedded timestamp is `unix_ms`.
fn id_at(unix_ms: i64) -> u64 {
    let ts = (unix_ms - sr_core::snowflake::SIDEROOM_EPOCH_MS) as u64;
    ts << 22
}

fn text(s: &str) -> CreateMessageInput {
    CreateMessageInput {
        content: s.into(),
        ..Default::default()
    }
}

struct Env {
    channel: Channel,
    roles: Vec<Role>,
    limits: ChatLimits,
}

impl Env {
    fn new() -> Self {
        Self {
            channel: text_channel(),
            roles: vec![fixtures::everyone()],
            limits: limits(),
        }
    }

    fn create(
        &self,
        input: &CreateMessageInput,
        perms: Permissions,
        last_sent: Option<i64>,
        reply: Option<&Message>,
    ) -> ChatResult<ValidatedMessage> {
        validate_create(
            input,
            &CreateContext {
                author: UserId::new(AUTHOR),
                channel: &self.channel,
                perms,
                now_ms: NOW,
                reply_target: reply,
                guild_roles: &self.roles,
                last_sent_by_author_ms: last_sent,
                limits: &self.limits,
            },
        )
    }

    fn edit(
        &self,
        editor: u64,
        m: &Message,
        input: &EditMessageInput,
        perms: Permissions,
    ) -> ChatResult<EditPlan> {
        validate_edit(
            input,
            &EditContext {
                editor: UserId::new(editor),
                message: m,
                channel_kind: self.channel.kind,
                perms,
                now_ms: NOW,
                guild_roles: &self.roles,
                limits: &self.limits,
            },
        )
    }
}

fn member_perms() -> Permissions {
    default_everyone_permissions()
}

fn mod_perms() -> Permissions {
    member_perms() | Permissions::MANAGE_MESSAGES
}

/// Pinning needs PIN_MESSAGES (Q95), not MANAGE_MESSAGES.
fn pin_perms() -> Permissions {
    member_perms() | Permissions::PIN_MESSAGES
}

// ---- create

#[test]
fn create_plain_message_trims() {
    let v = Env::new()
        .create(&text("  hello  "), member_perms(), None, None)
        .unwrap();
    assert_eq!(v.content, "hello");
    assert_eq!(v.kind, MessageType::Default);
    assert!(v.reference.is_none());
}

#[test]
fn create_needs_view_and_send() {
    let e = Env::new();
    assert!(matches!(
        e.create(&text("x"), Permissions::empty(), None, None),
        Err(ChatError::MissingAccess)
    ));
    let no_send = member_perms() - Permissions::SEND_MESSAGES;
    assert!(matches!(
        e.create(&text("x"), no_send, None, None),
        Err(ChatError::MissingPermissions)
    ));
    // Administrators pass without explicit bits.
    assert!(e.create(&text("x"), Permissions::ALL, None, None).is_ok());
}

#[test]
fn create_in_thread_uses_thread_permission() {
    let mut e = Env::new();
    e.channel.kind = ChannelType::PublicThread;
    let perms = member_perms() - Permissions::SEND_MESSAGES_IN_THREADS;
    assert!(e.create(&text("x"), perms, None, None).is_err());
    let perms =
        (member_perms() - Permissions::SEND_MESSAGES) | Permissions::SEND_MESSAGES_IN_THREADS;
    assert!(e.create(&text("x"), perms, None, None).is_ok());
}

#[test]
fn create_rejects_non_message_channels() {
    for kind in [
        ChannelType::Category,
        ChannelType::Forum,
        ChannelType::Media,
    ] {
        let mut e = Env::new();
        e.channel.kind = kind;
        assert!(matches!(
            e.create(&text("x"), Permissions::ALL, None, None),
            Err(ChatError::Invalid { .. })
        ));
    }
}

#[test]
fn create_empty_rules() {
    let e = Env::new();
    assert!(matches!(
        e.create(&text("   \n "), member_perms(), None, None),
        Err(ChatError::EmptyMessage)
    ));
    let with_file = CreateMessageInput {
        attachment_count: 1,
        ..Default::default()
    };
    assert!(e.create(&with_file, member_perms(), None, None).is_ok());
    let with_sticker = CreateMessageInput {
        sticker_ids: vec![1],
        ..Default::default()
    };
    assert!(e.create(&with_sticker, member_perms(), None, None).is_ok());
    let with_poll = CreateMessageInput {
        has_poll: true,
        ..Default::default()
    };
    assert!(e.create(&with_poll, member_perms(), None, None).is_ok());
}

#[test]
fn create_length_limits_use_sr_markdown() {
    let e = Env::new();
    assert!(
        e.create(&text(&"a".repeat(2000)), member_perms(), None, None)
            .is_ok()
    );
    assert!(matches!(
        e.create(&text(&"a".repeat(2001)), member_perms(), None, None),
        Err(ChatError::Invalid { field, .. }) if field == "content"
    ));
    // Characters, not bytes.
    assert!(
        e.create(&text(&"é".repeat(2000)), member_perms(), None, None)
            .is_ok()
    );
    // Operator-extended limit.
    let mut big = Env::new();
    big.limits.max_message_chars = 4000;
    assert!(
        big.create(&text(&"a".repeat(4000)), member_perms(), None, None)
            .is_ok()
    );
    // Mention cap.
    let many: String = (1..=101).map(|i| format!("<@{i}> ")).collect();
    assert!(e.create(&text(&many), member_perms(), None, None).is_err());
}

#[test]
fn create_attachment_and_embed_gates() {
    let e = Env::new();
    let files = |n| CreateMessageInput {
        attachment_count: n,
        ..Default::default()
    };
    assert!(e.create(&files(10), member_perms(), None, None).is_ok());
    assert!(e.create(&files(11), member_perms(), None, None).is_err());
    let no_attach = member_perms() - Permissions::ATTACH_FILES;
    assert!(matches!(
        e.create(&files(1), no_attach, None, None),
        Err(ChatError::MissingPermissions)
    ));
    let embeds = |n| CreateMessageInput {
        content: "x".into(),
        embed_count: n,
        ..Default::default()
    };
    assert!(e.create(&embeds(10), member_perms(), None, None).is_ok());
    assert!(e.create(&embeds(11), member_perms(), None, None).is_err());
    let no_embed = member_perms() - Permissions::EMBED_LINKS;
    assert!(e.create(&embeds(1), no_embed, None, None).is_err());
    let poll = CreateMessageInput {
        has_poll: true,
        ..Default::default()
    };
    let no_poll = member_perms() - Permissions::SEND_POLLS;
    assert!(e.create(&poll, no_poll, None, None).is_err());
}

#[test]
fn create_flags_are_masked_and_voice_messages_checked() {
    let e = Env::new();
    let mut i = text("hi");
    i.flags =
        message_flags::SUPPRESS_EMBEDS | message_flags::CROSSPOSTED | message_flags::EPHEMERAL;
    let v = e.create(&i, member_perms(), None, None).unwrap();
    assert_eq!(v.flags, message_flags::SUPPRESS_EMBEDS);

    let voice = CreateMessageInput {
        attachment_count: 1,
        flags: message_flags::IS_VOICE_MESSAGE,
        ..Default::default()
    };
    assert!(e.create(&voice, member_perms(), None, None).is_ok());
    let no_voice = member_perms() - Permissions::SEND_VOICE_MESSAGES;
    assert!(e.create(&voice, no_voice, None, None).is_err());
    let with_text = CreateMessageInput {
        content: "hi".into(),
        ..voice.clone()
    };
    assert!(e.create(&with_text, member_perms(), None, None).is_err());
    let two = CreateMessageInput {
        attachment_count: 2,
        ..voice
    };
    assert!(e.create(&two, member_perms(), None, None).is_err());
}

#[test]
fn create_mention_gates() {
    let mut e = Env::new();
    let mut open = fixtures::role(200, 1, Permissions::empty());
    open.mentionable = true;
    let closed = fixtures::role(201, 1, Permissions::empty());
    e.roles.extend([open, closed]);
    let i = text("@everyone @here <@&200> <@&201> <@&999> <@7> <#8>");

    let plain = e.create(&i, member_perms(), None, None).unwrap().mentions;
    assert!(!plain.everyone);
    assert_eq!(plain.roles, vec![RoleId::new(200)]);
    assert_eq!(plain.users, vec![UserId::new(7)]);
    assert_eq!(plain.channels, vec![ChannelId::new(8)]);

    let priv_ = member_perms() | Permissions::MENTION_EVERYONE;
    let m = e.create(&i, priv_, None, None).unwrap().mentions;
    assert!(m.everyone);
    // Unknown role 999 never pings even for privileged authors.
    assert_eq!(m.roles, vec![RoleId::new(200), RoleId::new(201)]);

    // Code never mentions.
    let code = e
        .create(&text("`@everyone`"), priv_, None, None)
        .unwrap()
        .mentions;
    assert!(!code.everyone);
}

#[test]
fn create_reply_rules() {
    let e = Env::new();
    let target = msg(77, 3, "orig");
    let mut i = text("re");
    i.reply_to = Some(target.id);
    let v = e.create(&i, member_perms(), None, Some(&target)).unwrap();
    assert_eq!(v.kind, MessageType::Reply);
    assert_eq!(v.reference.unwrap().message_id, target.id);

    assert!(matches!(
        e.create(&i, member_perms(), None, None),
        Err(ChatError::UnknownMessage)
    ));
    let mut deleted = target.clone();
    deleted.visibility = Visibility::UserDeleted;
    assert!(e.create(&i, member_perms(), None, Some(&deleted)).is_err());
    let mut elsewhere = target.clone();
    elsewhere.channel_id = ChannelId::new(51);
    assert!(
        e.create(&i, member_perms(), None, Some(&elsewhere))
            .is_err()
    );
    let no_history = member_perms() - Permissions::READ_MESSAGE_HISTORY;
    assert!(matches!(
        e.create(&i, no_history, None, Some(&target)),
        Err(ChatError::MissingPermissions)
    ));
}

#[test]
fn slowmode() {
    let mut e = Env::new();
    e.channel.rate_limit_per_user = 5;
    let last = NOW - 2_000;
    match e.create(&text("x"), member_perms(), Some(last), None) {
        Err(ChatError::Slowmode { retry_after_ms }) => assert_eq!(retry_after_ms, 3_000),
        other => panic!("expected slowmode, got {other:?}"),
    }
    assert!(
        e.create(&text("x"), member_perms(), Some(NOW - 5_000), None)
            .is_ok()
    );
    assert!(e.create(&text("x"), member_perms(), None, None).is_ok());
    // Bypass with MANAGE_MESSAGES / MANAGE_CHANNELS.
    assert!(e.create(&text("x"), mod_perms(), Some(last), None).is_ok());
    let mc = member_perms() | Permissions::MANAGE_CHANNELS;
    assert!(e.create(&text("x"), mc, Some(last), None).is_ok());
}

// ---- edit

#[test]
fn edit_by_author_records_history() {
    let e = Env::new();
    let m = msg(1, AUTHOR, "old");
    let plan = e
        .edit(
            AUTHOR,
            &m,
            &EditMessageInput {
                content: Some(" new ".into()),
                flags: None,
            },
            member_perms(),
        )
        .unwrap();
    assert_eq!(plan.content, "new");
    assert_eq!(plan.history_previous_content.as_deref(), Some("old"));
    assert_eq!(plan.edited_at_ms, NOW);
}

#[test]
fn edit_identical_content_is_a_noop() {
    let e = Env::new();
    let mut m = msg(1, AUTHOR, "same");
    m.edited_at_ms = Some(5);
    let plan = e
        .edit(
            AUTHOR,
            &m,
            &EditMessageInput {
                content: Some("same".into()),
                flags: None,
            },
            member_perms(),
        )
        .unwrap();
    assert!(plan.history_previous_content.is_none());
    assert_eq!(plan.edited_at_ms, 5);
}

#[test]
fn edit_by_other_user_is_refused_even_for_moderators() {
    let e = Env::new();
    let m = msg(1, AUTHOR, "old");
    let input = EditMessageInput {
        content: Some("hax".into()),
        flags: None,
    };
    assert!(matches!(
        e.edit(99, &m, &input, mod_perms()),
        Err(ChatError::NotAuthor)
    ));
    assert!(matches!(
        e.edit(99, &m, &input, Permissions::ALL),
        Err(ChatError::NotAuthor)
    ));
}

#[test]
fn moderator_can_suppress_embeds_on_others() {
    let e = Env::new();
    let m = msg(1, AUTHOR, "link");
    let input = EditMessageInput {
        content: None,
        flags: Some(message_flags::SUPPRESS_EMBEDS),
    };
    let plan = e.edit(99, &m, &input, mod_perms()).unwrap();
    assert_eq!(
        plan.flags & message_flags::SUPPRESS_EMBEDS,
        message_flags::SUPPRESS_EMBEDS
    );
    assert!(plan.history_previous_content.is_none());
    assert!(matches!(
        e.edit(99, &m, &input, member_perms()),
        Err(ChatError::MissingPermissions)
    ));
    // Author may toggle their own.
    assert!(e.edit(AUTHOR, &m, &input, member_perms()).is_ok());
}

#[test]
fn edit_rejects_system_deleted_empty_and_overlong() {
    let e = Env::new();
    let input = |s: &str| EditMessageInput {
        content: Some(s.into()),
        flags: None,
    };
    let mut sys = msg(1, AUTHOR, "x");
    sys.kind = MessageType::UserJoin;
    assert!(e.edit(AUTHOR, &sys, &input("y"), member_perms()).is_err());

    let mut gone = msg(2, AUTHOR, "x");
    gone.visibility = Visibility::UserDeleted;
    assert!(matches!(
        e.edit(AUTHOR, &gone, &input("y"), member_perms()),
        Err(ChatError::UnknownMessage)
    ));

    let m = msg(3, AUTHOR, "x");
    assert!(matches!(
        e.edit(AUTHOR, &m, &input("  "), member_perms()),
        Err(ChatError::EmptyMessage)
    ));
    let mut with_file = msg(4, AUTHOR, "x");
    with_file.attachment_ids = vec![AttachmentId::new(1)];
    assert!(
        e.edit(AUTHOR, &with_file, &input(""), member_perms())
            .is_ok()
    );

    assert!(
        e.edit(AUTHOR, &m, &input(&"a".repeat(2001)), member_perms())
            .is_err()
    );
}

#[test]
fn edit_needs_send_permission_and_view() {
    let e = Env::new();
    let m = msg(1, AUTHOR, "old");
    let input = EditMessageInput {
        content: Some("new".into()),
        flags: None,
    };
    let timed_out = Permissions::VIEW_CHANNEL | Permissions::READ_MESSAGE_HISTORY;
    assert!(matches!(
        e.edit(AUTHOR, &m, &input, timed_out),
        Err(ChatError::MissingPermissions)
    ));
    assert!(matches!(
        e.edit(AUTHOR, &m, &input, Permissions::empty()),
        Err(ChatError::MissingAccess)
    ));
}

#[test]
fn edit_recomputes_mentions_with_gates_but_flag_toggle_keeps_stored() {
    let e = Env::new();
    let m = msg(1, AUTHOR, "old");
    let input = EditMessageInput {
        content: Some("@everyone <@5>".into()),
        flags: None,
    };
    let plan = e.edit(AUTHOR, &m, &input, member_perms()).unwrap();
    assert!(!plan.mentions.everyone);
    assert_eq!(plan.mentions.users, vec![UserId::new(5)]);

    let mut pinged = msg(2, AUTHOR, "@everyone");
    pinged.mention_everyone = true;
    let toggle = EditMessageInput {
        content: None,
        flags: Some(message_flags::SUPPRESS_EMBEDS),
    };
    let plan = e.edit(99, &pinged, &toggle, mod_perms()).unwrap();
    assert!(plan.mentions.everyone);
}

// ---- delete

#[test]
fn author_delete_sets_30_day_purge() {
    let m = msg(1, AUTHOR, "x");
    let plan = validate_delete(UserId::new(AUTHOR), &m, member_perms(), NOW, &limits()).unwrap();
    assert_eq!(plan.visibility, Visibility::UserDeleted);
    assert_eq!(plan.deleted_at_ms, NOW);
    assert_eq!(plan.purge_after_ms, Some(NOW + 30 * 86_400_000));
}

#[test]
fn hold_blocks_purge_but_not_user_delete() {
    let mut m = msg(1, AUTHOR, "x");
    m.hold_id = Some(9);
    let plan = validate_delete(UserId::new(AUTHOR), &m, member_perms(), NOW, &limits()).unwrap();
    assert_eq!(plan.visibility, Visibility::UserDeleted);
    assert_eq!(plan.purge_after_ms, None);
}

#[test]
fn delete_permissions() {
    let m = msg(1, AUTHOR, "x");
    let l = limits();
    assert!(matches!(
        validate_delete(UserId::new(99), &m, member_perms(), NOW, &l),
        Err(ChatError::MissingPermissions)
    ));
    assert!(validate_delete(UserId::new(99), &m, mod_perms(), NOW, &l).is_ok());
    // Authors need no SEND permission to remove their own messages (timed-out case).
    let read_only = Permissions::VIEW_CHANNEL | Permissions::READ_MESSAGE_HISTORY;
    assert!(validate_delete(UserId::new(AUTHOR), &m, read_only, NOW, &l).is_ok());
    // But not system messages, which are manager-only.
    let mut sys = msg(2, AUTHOR, "");
    sys.kind = MessageType::ChannelPinnedMessage;
    assert!(validate_delete(UserId::new(AUTHOR), &sys, member_perms(), NOW, &l).is_err());
    assert!(validate_delete(UserId::new(99), &sys, mod_perms(), NOW, &l).is_ok());
    // Already deleted.
    let mut gone = msg(3, AUTHOR, "x");
    gone.visibility = Visibility::UserDeleted;
    assert!(matches!(
        validate_delete(UserId::new(AUTHOR), &gone, member_perms(), NOW, &l),
        Err(ChatError::UnknownMessage)
    ));
}

#[test]
fn purge_clock_arithmetic_uses_configured_period() {
    let mut l = limits();
    l.purge_after_ms = 1_000;
    let m = msg(1, AUTHOR, "x");
    let plan = validate_delete(UserId::new(AUTHOR), &m, member_perms(), 5_000, &l).unwrap();
    assert_eq!(plan.purge_after_ms, Some(6_000));
}

// ---- bulk delete

fn recent_msgs(n: usize) -> Vec<Message> {
    (0..n)
        .map(|i| msg(id_at(NOW - 1_000) + i as u64, 3, "x"))
        .collect()
}

#[test]
fn bulk_delete_happy_path_and_skips() {
    let ch = text_channel();
    let found = recent_msgs(3);
    let mut ids: Vec<MessageId> = found.iter().map(|m| m.id).collect();
    ids.push(MessageId::new(id_at(NOW - 500) + 99)); // unknown: skipped
    let plans = validate_bulk_delete(&ch, mod_perms(), &ids, &found, NOW, &limits()).unwrap();
    assert_eq!(plans.len(), 3);
    assert!(
        plans
            .iter()
            .all(|p| p.purge_after_ms == Some(NOW + 30 * 86_400_000))
    );
}

#[test]
fn bulk_delete_bounds_and_age() {
    let ch = text_channel();
    let l = limits();
    let found = recent_msgs(2);
    let one = [found[0].id];
    assert!(validate_bulk_delete(&ch, mod_perms(), &one, &found, NOW, &l).is_err());
    // Duplicates collapse, so [a, a] is one id.
    let dup = [found[0].id, found[0].id];
    assert!(validate_bulk_delete(&ch, mod_perms(), &dup, &found, NOW, &l).is_err());
    let many: Vec<MessageId> = (0..101)
        .map(|i| MessageId::new(id_at(NOW - 10) + i))
        .collect();
    assert!(validate_bulk_delete(&ch, mod_perms(), &many, &found, NOW, &l).is_err());
    let hundred_msgs = recent_msgs(100);
    let hundred: Vec<MessageId> = hundred_msgs.iter().map(|m| m.id).collect();
    assert!(validate_bulk_delete(&ch, mod_perms(), &hundred, &hundred_msgs, NOW, &l).is_ok());

    // 14-day boundary: exactly 14 days is allowed, one ms older is not.
    let edge = MessageId::new(id_at(NOW - 14 * 86_400_000));
    let old = MessageId::new(id_at(NOW - 14 * 86_400_000 - 1));
    let m_edge = msg(edge.get(), 3, "x");
    let ids_ok = [edge, found[0].id];
    let ok_found = [m_edge, found[0].clone()];
    assert!(validate_bulk_delete(&ch, mod_perms(), &ids_ok, &ok_found, NOW, &l).is_ok());
    let ids_old = [old, found[0].id];
    assert!(matches!(
        validate_bulk_delete(&ch, mod_perms(), &ids_old, &found, NOW, &l),
        Err(ChatError::Invalid {
            code: "MESSAGE_TOO_OLD",
            ..
        })
    ));
}

#[test]
fn bulk_delete_permissions_and_dm() {
    let found = recent_msgs(2);
    let ids: Vec<MessageId> = found.iter().map(|m| m.id).collect();
    let l = limits();
    assert!(matches!(
        validate_bulk_delete(&text_channel(), member_perms(), &ids, &found, NOW, &l),
        Err(ChatError::MissingPermissions)
    ));
    let dm = Channel::new(ChannelId::new(50), None, ChannelType::Dm);
    assert!(matches!(
        validate_bulk_delete(&dm, Permissions::ALL, &ids, &found, NOW, &l),
        Err(ChatError::NotInGuild)
    ));
    // Nothing deletable remains.
    let mut gone = found.clone();
    gone.iter_mut()
        .for_each(|m| m.visibility = Visibility::UserDeleted);
    assert!(matches!(
        validate_bulk_delete(&text_channel(), mod_perms(), &ids, &gone, NOW, &l),
        Err(ChatError::UnknownMessage)
    ));
}

// ---- pins

#[test]
fn pin_rules() {
    let ch = text_channel();
    let l = limits();
    let m = msg(1, AUTHOR, "x");
    assert!(validate_pin(&ch, &m, pin_perms(), 0, &l).is_ok());
    // MANAGE_MESSAGES alone no longer grants pinning.
    assert!(matches!(
        validate_pin(&ch, &m, mod_perms(), 0, &l),
        Err(ChatError::MissingPermissions)
    ));
    assert!(validate_pin(&ch, &m, pin_perms(), 249, &l).is_ok());
    assert!(matches!(
        validate_pin(&ch, &m, pin_perms(), 250, &l),
        Err(ChatError::MaxPins)
    ));
    assert!(matches!(
        validate_pin(&ch, &m, member_perms(), 0, &l),
        Err(ChatError::MissingPermissions)
    ));
    let mut pinned = m.clone();
    pinned.pinned = true;
    assert!(validate_pin(&ch, &pinned, pin_perms(), 0, &l).is_err());
    let mut sys = m.clone();
    sys.kind = MessageType::UserJoin;
    assert!(validate_pin(&ch, &sys, pin_perms(), 0, &l).is_err());
    let mut gone = m.clone();
    gone.visibility = Visibility::UserDeleted;
    assert!(matches!(
        validate_pin(&ch, &gone, pin_perms(), 0, &l),
        Err(ChatError::UnknownMessage)
    ));
    let mut other = m.clone();
    other.channel_id = ChannelId::new(51);
    assert!(validate_pin(&ch, &other, pin_perms(), 0, &l).is_err());
}

#[test]
fn unpin_rules() {
    let ch = text_channel();
    let mut m = msg(1, AUTHOR, "x");
    assert!(matches!(
        validate_unpin(&ch, &m, pin_perms()),
        Err(ChatError::UnknownMessage)
    ));
    m.pinned = true;
    assert!(validate_unpin(&ch, &m, pin_perms()).is_ok());
    assert!(validate_unpin(&ch, &m, member_perms()).is_err());
}

// ---- reactions

#[test]
fn reaction_rules() {
    let l = limits();
    let m = msg(1, AUTHOR, "x");
    assert!(validate_add_reaction(&m, member_perms(), false, 0, &l).is_ok());
    // Existing emoji: ADD_REACTIONS not required, cap not applied.
    let no_react = member_perms() - Permissions::ADD_REACTIONS;
    assert!(matches!(
        validate_add_reaction(&m, no_react, false, 0, &l),
        Err(ChatError::MissingPermissions)
    ));
    assert!(validate_add_reaction(&m, no_react, true, 20, &l).is_ok());
    assert!(validate_add_reaction(&m, member_perms(), false, 19, &l).is_ok());
    assert!(matches!(
        validate_add_reaction(&m, member_perms(), false, 20, &l),
        Err(ChatError::MaxReactions)
    ));
    let no_history = member_perms() - Permissions::READ_MESSAGE_HISTORY;
    assert!(validate_add_reaction(&m, no_history, true, 0, &l).is_err());
    let mut gone = m.clone();
    gone.visibility = Visibility::StaffRemoved;
    assert!(validate_add_reaction(&gone, member_perms(), false, 0, &l).is_err());
}

#[test]
fn remove_reaction_rules() {
    let (me, other) = (UserId::new(1), UserId::new(2));
    assert!(validate_remove_reaction(me, me, member_perms()).is_ok());
    assert!(validate_remove_reaction(me, other, member_perms()).is_err());
    assert!(validate_remove_reaction(me, other, mod_perms()).is_ok());
}

#[test]
fn snowflake_helper_matches_core() {
    // Guards the test helper itself: id_at must round-trip through Snowflake::unix_ms.
    assert_eq!(Snowflake::new(id_at(NOW)).unix_ms(), NOW);
}
