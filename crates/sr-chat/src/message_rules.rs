//! Pure, permission-aware message rules: create, edit, delete, bulk delete, pin and
//! reaction validation (LNG-MSG-002/004/005/006/008/009/011). No I/O: callers load
//! the context, call these functions, and persist the returned plan.

use crate::domain::{
    Channel, ChannelType, Message, MessageReference, MessageType, Role, Visibility, message_flags,
};
use crate::error::{ChatError, ChatResult};
use crate::limits::ChatLimits;
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, MessageId, RoleId, UserId};
use sr_markdown::{LimitError, Mentions};

/// Flags a client may set on a new message.
const CLIENT_FLAGS: u32 = message_flags::SUPPRESS_EMBEDS
    | message_flags::TTS
    | message_flags::SUPPRESS_NOTIFICATIONS
    | message_flags::IS_VOICE_MESSAGE;

fn map_limit(e: LimitError) -> ChatError {
    match e {
        LimitError::TooLong { max, .. } => ChatError::invalid(
            "content",
            "BASE_TYPE_MAX_LENGTH",
            format!("Must be {max} or fewer in length."),
        ),
        LimitError::TooManyMentions { max, .. } => ChatError::invalid(
            "content",
            "TOO_MANY_MENTIONS",
            format!("Too many mentions (max {max})."),
        ),
    }
}

/// Which mentions actually notify, after permission gates (LNG-MSG-011).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolvedMentions {
    pub everyone: bool,
    pub users: Vec<UserId>,
    pub roles: Vec<RoleId>,
    pub channels: Vec<ChannelId>,
}

/// `@everyone`/`@here` need MENTION_EVERYONE. A role is pinged only if it exists in
/// the guild and is mentionable, or the author has MENTION_EVERYONE. Without the
/// permission the text stays but does not ping (Discord parity), so this never errors.
pub fn resolve_mentions(
    parsed: &Mentions,
    perms: Permissions,
    guild_roles: &[Role],
) -> ResolvedMentions {
    let can_ping_all = perms.has(Permissions::MENTION_EVERYONE);
    ResolvedMentions {
        everyone: (parsed.everyone || parsed.here) && can_ping_all,
        users: parsed.users.iter().map(|u| UserId::new(*u)).collect(),
        roles: parsed
            .roles
            .iter()
            .filter(|id| {
                guild_roles.iter().any(|r| {
                    r.id.get() == **id && !r.is_everyone() && (r.mentionable || can_ping_all)
                })
            })
            .map(|id| RoleId::new(*id))
            .collect(),
        channels: parsed.channels.iter().map(|c| ChannelId::new(*c)).collect(),
    }
}

/// The request's `allowed_mentions` (Discord semantics, open question 152). When
/// `parse_*` is set, every mention of that kind may ping; otherwise only the ids
/// listed in `users`/`roles` may. `replied_user` adds the author of the replied-to
/// message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AllowedMentions {
    pub parse_users: bool,
    pub parse_roles: bool,
    pub parse_everyone: bool,
    pub users: Vec<UserId>,
    pub roles: Vec<RoleId>,
    pub replied_user: bool,
}

/// Drop the mentions the request does not allow to ping. Text is unchanged.
pub fn apply_allowed_mentions(
    mut mentions: ResolvedMentions,
    allowed: &AllowedMentions,
    reply_author: Option<UserId>,
) -> ResolvedMentions {
    mentions.everyone &= allowed.parse_everyone;
    mentions
        .users
        .retain(|u| allowed.parse_users || allowed.users.contains(u));
    mentions
        .roles
        .retain(|r| allowed.parse_roles || allowed.roles.contains(r));
    if allowed.replied_user
        && let Some(author) = reply_author
        && !mentions.users.contains(&author)
    {
        mentions.users.push(author);
    }
    mentions
}

// ---------------------------------------------------------------- create

#[derive(Debug, Clone, Default)]
pub struct CreateMessageInput {
    pub content: String,
    pub nonce: Option<String>,
    pub reply_to: Option<MessageId>,
    pub attachment_count: usize,
    pub embed_count: usize,
    pub sticker_ids: Vec<u64>,
    pub has_poll: bool,
    pub flags: u32,
}

pub struct CreateContext<'a> {
    pub author: UserId,
    pub channel: &'a Channel,
    /// Effective channel permissions of the author (timeouts already applied).
    pub perms: Permissions,
    pub now_ms: i64,
    /// The replied-to message, loaded by the caller when `reply_to` is set.
    pub reply_target: Option<&'a Message>,
    pub guild_roles: &'a [Role],
    /// When the author last posted here (visible or deleted), for slowmode.
    pub last_sent_by_author_ms: Option<i64>,
    pub limits: &'a ChatLimits,
}

/// A message that passed every check, ready to be inserted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedMessage {
    pub kind: MessageType,
    pub content: String,
    pub flags: u32,
    pub nonce: Option<String>,
    pub reference: Option<MessageReference>,
    pub mentions: ResolvedMentions,
}

fn require(perms: Permissions, needed: Permissions) -> ChatResult<()> {
    if perms.has(needed) {
        Ok(())
    } else {
        Err(ChatError::MissingPermissions)
    }
}

fn require_view(perms: Permissions) -> ChatResult<()> {
    if perms.has(Permissions::VIEW_CHANNEL) {
        Ok(())
    } else {
        Err(ChatError::MissingAccess)
    }
}

pub fn validate_create(
    input: &CreateMessageInput,
    ctx: &CreateContext<'_>,
) -> ChatResult<ValidatedMessage> {
    let ch = ctx.channel;
    if !ch.kind.accepts_messages() {
        return Err(ChatError::invalid(
            "channel_id",
            "CHANNEL_TYPE_INVALID",
            "Cannot send messages in this channel type.",
        ));
    }
    require_view(ctx.perms)?;
    require(
        ctx.perms,
        if ch.kind.is_thread() {
            Permissions::SEND_MESSAGES_IN_THREADS
        } else {
            Permissions::SEND_MESSAGES
        },
    )?;

    if input.attachment_count > ctx.limits.max_attachments {
        return Err(ChatError::invalid(
            "attachments",
            "BASE_TYPE_MAX_LENGTH",
            format!("Must be {} or fewer in length.", ctx.limits.max_attachments),
        ));
    }
    if input.embed_count > ctx.limits.max_embeds {
        return Err(ChatError::invalid(
            "embeds",
            "BASE_TYPE_MAX_LENGTH",
            format!("Must be {} or fewer in length.", ctx.limits.max_embeds),
        ));
    }
    if input.attachment_count > 0 {
        require(ctx.perms, Permissions::ATTACH_FILES)?;
    }
    if input.embed_count > 0 {
        require(ctx.perms, Permissions::EMBED_LINKS)?;
    }
    if input.has_poll {
        require(ctx.perms, Permissions::SEND_POLLS)?;
    }

    let content = input.content.trim().to_owned();
    let flags = input.flags & CLIENT_FLAGS;
    if flags & message_flags::TTS != 0 {
        require(ctx.perms, Permissions::SEND_TTS_MESSAGES)?;
    }
    if flags & message_flags::IS_VOICE_MESSAGE != 0 {
        require(ctx.perms, Permissions::SEND_VOICE_MESSAGES)?;
        if input.attachment_count != 1 || !content.is_empty() {
            return Err(ChatError::invalid(
                "flags",
                "VOICE_MESSAGE_SHAPE",
                "A voice message is exactly one audio attachment with no text.",
            ));
        }
    }

    let empty = content.is_empty()
        && input.attachment_count == 0
        && input.embed_count == 0
        && input.sticker_ids.is_empty()
        && !input.has_poll;
    if empty {
        return Err(ChatError::EmptyMessage);
    }

    let parsed = sr_markdown::check_limits(&content, &ctx.limits.markdown()).map_err(map_limit)?;
    let mentions = resolve_mentions(&parsed, ctx.perms, ctx.guild_roles);

    // Reply: needs history access, and a visible message in this same channel.
    let (kind, reference) = match input.reply_to {
        None => (MessageType::Default, None),
        Some(target_id) => {
            require(ctx.perms, Permissions::READ_MESSAGE_HISTORY)?;
            match ctx.reply_target {
                Some(t) if t.id == target_id && t.channel_id == ch.id && t.is_visible() => (
                    MessageType::Reply,
                    Some(MessageReference {
                        message_id: t.id,
                        channel_id: ch.id,
                    }),
                ),
                _ => return Err(ChatError::UnknownMessage),
            }
        }
    };

    check_slowmode(ch, ctx.perms, ctx.last_sent_by_author_ms, ctx.now_ms)?;

    Ok(ValidatedMessage {
        kind,
        content,
        flags,
        nonce: input.nonce.clone(),
        reference,
        mentions,
    })
}

/// Slowmode (LNG-CHN-005). MANAGE_MESSAGES, MANAGE_CHANNELS and BYPASS_SLOWMODE bypass it.
pub fn check_slowmode(
    channel: &Channel,
    perms: Permissions,
    last_sent_ms: Option<i64>,
    now_ms: i64,
) -> ChatResult<()> {
    if channel.rate_limit_per_user == 0
        || perms.has(Permissions::MANAGE_MESSAGES)
        || perms.has(Permissions::MANAGE_CHANNELS)
        || perms.has(Permissions::BYPASS_SLOWMODE)
    {
        return Ok(());
    }
    if let Some(last) = last_sent_ms {
        let ready_at = last + i64::from(channel.rate_limit_per_user) * 1000;
        if now_ms < ready_at {
            return Err(ChatError::Slowmode {
                retry_after_ms: ready_at - now_ms,
            });
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ edit

#[derive(Debug, Clone, Default)]
pub struct EditMessageInput {
    pub content: Option<String>,
    /// Only `SUPPRESS_EMBEDS` can change; other bits are ignored.
    pub flags: Option<u32>,
}

pub struct EditContext<'a> {
    pub editor: UserId,
    pub message: &'a Message,
    pub channel_kind: ChannelType,
    pub perms: Permissions,
    pub now_ms: i64,
    pub guild_roles: &'a [Role],
    pub limits: &'a ChatLimits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditPlan {
    pub message_id: MessageId,
    pub content: String,
    pub flags: u32,
    pub edited_at_ms: i64,
    pub mentions: ResolvedMentions,
    /// Row for `message_edits`; `None` when the content did not change.
    pub history_previous_content: Option<String>,
}

pub fn validate_edit(input: &EditMessageInput, ctx: &EditContext<'_>) -> ChatResult<EditPlan> {
    let m = ctx.message;
    if !m.is_visible() {
        return Err(ChatError::UnknownMessage);
    }
    require_view(ctx.perms)?;
    let is_author = m.author_id == ctx.editor;
    let new_content = input.content.as_ref().map(|c| c.trim().to_owned());
    let content_changes = new_content.as_ref().is_some_and(|c| *c != m.content);

    let mut flags = m.flags;
    if let Some(f) = input.flags {
        let want = f & message_flags::SUPPRESS_EMBEDS;
        let changes = (flags & message_flags::SUPPRESS_EMBEDS) != want;
        // The author or a MANAGE_MESSAGES holder may toggle embed suppression.
        if changes && !is_author && !ctx.perms.has(Permissions::MANAGE_MESSAGES) {
            return Err(ChatError::MissingPermissions);
        }
        flags = (flags & !message_flags::SUPPRESS_EMBEDS) | want;
    }

    if content_changes {
        if !is_author {
            return Err(ChatError::NotAuthor);
        }
        if !m.kind.is_editable() {
            return Err(ChatError::invalid(
                "message",
                "SYSTEM_MESSAGE",
                "System messages cannot be edited.",
            ));
        }
        // A timed-out author loses SEND_MESSAGES and with it the right to edit.
        let needed = if ctx.channel_kind.is_thread() {
            Permissions::SEND_MESSAGES_IN_THREADS
        } else {
            Permissions::SEND_MESSAGES
        };
        require(ctx.perms, needed)?;
    }

    let content = if content_changes {
        new_content.unwrap_or_default()
    } else {
        m.content.clone()
    };
    if content_changes {
        let has_other = !m.attachment_ids.is_empty()
            || m.embed_count() > 0
            || !m.sticker_ids.is_empty()
            || m.has_poll();
        if content.is_empty() && !has_other {
            return Err(ChatError::EmptyMessage);
        }
    }
    let parsed = sr_markdown::check_limits(&content, &ctx.limits.markdown()).map_err(map_limit)?;
    // Mentions are recomputed with the *author's* permissions; a manager toggling
    // flags must not grant pings the author never had, so unchanged content keeps
    // its stored mentions.
    let mentions = if content_changes {
        resolve_mentions(&parsed, ctx.perms, ctx.guild_roles)
    } else {
        ResolvedMentions {
            everyone: m.mention_everyone,
            users: m.mention_user_ids.clone(),
            roles: m.mention_role_ids.clone(),
            channels: m.mention_channel_ids.clone(),
        }
    };
    Ok(EditPlan {
        message_id: m.id,
        history_previous_content: content_changes.then(|| m.content.clone()),
        content,
        flags,
        edited_at_ms: if content_changes {
            ctx.now_ms
        } else {
            m.edited_at_ms.unwrap_or(ctx.now_ms)
        },
        mentions,
    })
}

// ---------------------------------------------------------------- delete

/// What to persist when a message is deleted (D-121/D-122).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletePlan {
    pub message_id: MessageId,
    pub visibility: Visibility,
    pub deleted_at_ms: i64,
    /// `None` while a moderation hold blocks purging.
    pub purge_after_ms: Option<i64>,
}

fn delete_plan(m: &Message, now_ms: i64, limits: &ChatLimits) -> DeletePlan {
    DeletePlan {
        message_id: m.id,
        visibility: Visibility::UserDeleted,
        deleted_at_ms: now_ms,
        purge_after_ms: m.hold_id.is_none().then(|| now_ms + limits.purge_after_ms),
    }
}

/// Authors may delete their own user-authored messages; MANAGE_MESSAGES holders may
/// delete anything (including system messages). Users can delete held messages too:
/// the hold only suppresses `purge_after` (D-122).
pub fn validate_delete(
    deleter: UserId,
    message: &Message,
    perms: Permissions,
    now_ms: i64,
    limits: &ChatLimits,
) -> ChatResult<DeletePlan> {
    if !message.is_visible() {
        return Err(ChatError::UnknownMessage);
    }
    require_view(perms)?;
    let is_manager = perms.has(Permissions::MANAGE_MESSAGES);
    let own = message.author_id == deleter && !message.kind.is_system();
    if !own && !is_manager {
        return Err(ChatError::MissingPermissions);
    }
    Ok(delete_plan(message, now_ms, limits))
}

/// Bulk delete (LNG-MSG-006): guild channels only, MANAGE_MESSAGES, 2-100 distinct ids,
/// every requested id younger than 14 days. Ids that are unknown or already deleted
/// are skipped; if none remain the request fails.
pub fn validate_bulk_delete(
    channel: &Channel,
    perms: Permissions,
    requested: &[MessageId],
    found: &[Message],
    now_ms: i64,
    limits: &ChatLimits,
) -> ChatResult<Vec<DeletePlan>> {
    if channel.guild_id.is_none() {
        return Err(ChatError::NotInGuild);
    }
    require_view(perms)?;
    require(perms, Permissions::MANAGE_MESSAGES)?;
    let mut ids: Vec<MessageId> = requested.to_vec();
    ids.sort_unstable();
    ids.dedup();
    if !(limits.bulk_delete_min..=limits.bulk_delete_max).contains(&ids.len()) {
        return Err(ChatError::invalid(
            "messages",
            "BASE_TYPE_BAD_LENGTH",
            format!(
                "Must provide between {} and {} messages.",
                limits.bulk_delete_min, limits.bulk_delete_max
            ),
        ));
    }
    if let Some(old) = ids
        .iter()
        .find(|id| now_ms - id.snowflake().unix_ms() > limits.bulk_delete_max_age_ms)
    {
        return Err(ChatError::invalid(
            "messages",
            "MESSAGE_TOO_OLD",
            format!("Message {old} is older than 14 days and cannot be bulk deleted."),
        ));
    }
    let plans: Vec<DeletePlan> = found
        .iter()
        .filter(|m| m.channel_id == channel.id && m.is_visible() && ids.contains(&m.id))
        .map(|m| delete_plan(m, now_ms, limits))
        .collect();
    if plans.is_empty() {
        return Err(ChatError::UnknownMessage);
    }
    Ok(plans)
}

// ------------------------------------------------------------------ pins

fn check_pin_access(perms: Permissions, message: &Message, channel: &Channel) -> ChatResult<()> {
    if message.channel_id != channel.id || !message.is_visible() {
        return Err(ChatError::UnknownMessage);
    }
    require_view(perms)?;
    // Discord 2025 split pinning out of MANAGE_MESSAGES: PIN_MESSAGES is required (Q95).
    require(perms, Permissions::PIN_MESSAGES)
}

pub fn validate_pin(
    channel: &Channel,
    message: &Message,
    perms: Permissions,
    current_pin_count: usize,
    limits: &ChatLimits,
) -> ChatResult<()> {
    check_pin_access(perms, message, channel)?;
    if !message.kind.is_pinnable() {
        return Err(ChatError::invalid(
            "message",
            "SYSTEM_MESSAGE",
            "System messages cannot be pinned.",
        ));
    }
    if message.pinned {
        return Err(ChatError::invalid(
            "message",
            "ALREADY_PINNED",
            "This message is already pinned.",
        ));
    }
    if current_pin_count >= limits.max_pins_per_channel {
        return Err(ChatError::MaxPins);
    }
    Ok(())
}

pub fn validate_unpin(channel: &Channel, message: &Message, perms: Permissions) -> ChatResult<()> {
    check_pin_access(perms, message, channel)?;
    if !message.pinned {
        return Err(ChatError::UnknownMessage);
    }
    Ok(())
}

// ------------------------------------------------------------- reactions

/// Adding a reaction: history access is required; ADD_REACTIONS only when this emoji
/// is new on the message (Discord parity); at most 20 distinct emoji per message.
pub fn validate_add_reaction(
    message: &Message,
    perms: Permissions,
    emoji_already_present: bool,
    distinct_emoji_count: usize,
    limits: &ChatLimits,
) -> ChatResult<()> {
    if !message.is_visible() {
        return Err(ChatError::UnknownMessage);
    }
    require_view(perms)?;
    require(perms, Permissions::READ_MESSAGE_HISTORY)?;
    if !emoji_already_present {
        require(perms, Permissions::ADD_REACTIONS)?;
        if distinct_emoji_count >= limits.max_distinct_reactions {
            return Err(ChatError::MaxReactions);
        }
    }
    Ok(())
}

/// Removing someone else's reaction needs MANAGE_MESSAGES; your own is always allowed.
pub fn validate_remove_reaction(
    actor: UserId,
    reaction_owner: UserId,
    perms: Permissions,
) -> ChatResult<()> {
    require_view(perms)?;
    if actor == reaction_owner {
        Ok(())
    } else {
        require(perms, Permissions::MANAGE_MESSAGES)
    }
}

#[cfg(test)]
mod tests;
