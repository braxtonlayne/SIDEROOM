//! Pure thread and forum rules (LNG-THR-001, LNG-FRM-001). No I/O: the service loads
//! the context, calls these, persists the returned channel.

use super::model::{
    AUTO_ARCHIVE_DURATIONS, DEFAULT_AUTO_ARCHIVE_MINUTES, DefaultReaction, ForumConfig, ForumPatch,
    ForumTag, MAX_APPLIED_TAGS, MAX_AVAILABLE_TAGS, MAX_THREAD_MEMBERS, TAG_NAME_MAX,
    THREAD_NAME_MAX, ThreadInfo, channel_flags, forum_layout, sort_order,
};
use crate::channel_rules::SLOWMODE_MAX_SECS;
use crate::domain::{Channel, ChannelType, Message};
use crate::error::{ChatError, ChatResult};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, GuildId, UserId};

/// Permissions a member holds in a thread: those of the parent channel, except that a
/// private thread is invisible to non-members who lack `MANAGE_THREADS`.
pub fn thread_permissions(
    parent_perms: Permissions,
    thread: &Channel,
    is_member: bool,
) -> Permissions {
    if parent_perms.is_empty() {
        return Permissions::empty();
    }
    if thread.kind == ChannelType::PrivateThread
        && !is_member
        && !parent_perms.has(Permissions::MANAGE_THREADS)
    {
        return Permissions::empty();
    }
    parent_perms
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

/// Thread name: trimmed, 1 to 100 characters, case and spaces kept.
pub fn validate_thread_name(raw: &str) -> ChatResult<String> {
    let name = raw.trim();
    let n = name.chars().count();
    if n == 0 || n > THREAD_NAME_MAX {
        return Err(ChatError::invalid(
            "name",
            "BASE_TYPE_BAD_LENGTH",
            format!("Must be between 1 and {THREAD_NAME_MAX} in length."),
        ));
    }
    Ok(name.to_owned())
}

pub fn validate_auto_archive(minutes: u32) -> ChatResult<u32> {
    if AUTO_ARCHIVE_DURATIONS.contains(&minutes) {
        Ok(minutes)
    } else {
        Err(ChatError::invalid(
            "auto_archive_duration",
            "ENUM_TYPE_INVALID",
            "Must be one of 60, 1440, 4320, 10080.",
        ))
    }
}

pub fn validate_slowmode(secs: u32) -> ChatResult<u32> {
    if secs > SLOWMODE_MAX_SECS {
        return Err(ChatError::invalid(
            "rate_limit_per_user",
            "NUMBER_TYPE_MAX",
            format!("Must be {SLOWMODE_MAX_SECS} or less."),
        ));
    }
    Ok(secs)
}

/// Request to start a thread in a text or announcement channel.
#[derive(Debug, Clone, Default)]
pub struct StartThread {
    pub name: String,
    pub auto_archive_duration: Option<u32>,
    pub rate_limit_per_user: Option<u32>,
    /// Requested thread type; ignored when started from a message.
    pub kind: Option<ChannelType>,
    pub invitable: Option<bool>,
}

/// Request to create a forum or media post (the starter message is separate).
#[derive(Debug, Clone, Default)]
pub struct ForumPost {
    pub name: String,
    pub auto_archive_duration: Option<u32>,
    pub rate_limit_per_user: Option<u32>,
    pub applied_tags: Vec<u64>,
}

#[allow(clippy::too_many_arguments)]
fn base_thread(
    id: ChannelId,
    guild: GuildId,
    parent: &Channel,
    kind: ChannelType,
    owner: UserId,
    name: String,
    now_ms: i64,
    duration: u32,
) -> Channel {
    let mut c = Channel::new(id, Some(guild), kind);
    c.name = Some(name);
    c.parent_id = Some(parent.id);
    c.owner_id = Some(owner);
    c.nsfw = parent.nsfw;
    c.thread = Some(ThreadInfo::new(now_ms, duration));
    c
}

fn duration_for(requested: Option<u32>, parent: &Channel) -> ChatResult<u32> {
    match requested {
        Some(d) => validate_auto_archive(d),
        None => Ok(parent
            .forum
            .default_auto_archive_duration
            .filter(|d| AUTO_ARCHIVE_DURATIONS.contains(d))
            .unwrap_or(DEFAULT_AUTO_ARCHIVE_MINUTES)),
    }
}

/// Plan a thread started from `message` in `parent`. The thread id is the message id.
pub fn plan_thread_from_message(
    actor: UserId,
    perms: Permissions,
    parent: &Channel,
    message: &Message,
    input: &StartThread,
    now_ms: i64,
) -> ChatResult<Channel> {
    let guild = parent.guild_id.ok_or(ChatError::NotInGuild)?;
    require_view(perms)?;
    if !parent.kind.hosts_threads() {
        return Err(ChatError::invalid(
            "channel_id",
            "CHANNEL_TYPE_INVALID",
            "Threads cannot be started in this channel type.",
        ));
    }
    require(perms, Permissions::READ_MESSAGE_HISTORY)?;
    require(perms, Permissions::CREATE_PUBLIC_THREADS)?;
    if !message.is_visible() || message.kind.is_system() {
        return Err(ChatError::invalid(
            "message_id",
            "MESSAGE_TYPE_INVALID",
            "A thread cannot be started from this message.",
        ));
    }
    let name = validate_thread_name(&input.name)?;
    let kind = if parent.kind == ChannelType::Announcement {
        ChannelType::AnnouncementThread
    } else {
        ChannelType::PublicThread
    };
    let duration = duration_for(input.auto_archive_duration, parent)?;
    let mut c = base_thread(
        ChannelId::new(message.id.get()),
        guild,
        parent,
        kind,
        actor,
        name,
        now_ms,
        duration,
    );
    c.rate_limit_per_user = validate_slowmode(
        input
            .rate_limit_per_user
            .or(parent.forum.default_thread_rate_limit)
            .unwrap_or(0),
    )?;
    Ok(c)
}

/// Plan a thread started without a message (public or private).
pub fn plan_standalone_thread(
    id: ChannelId,
    actor: UserId,
    perms: Permissions,
    parent: &Channel,
    input: &StartThread,
    now_ms: i64,
) -> ChatResult<Channel> {
    let guild = parent.guild_id.ok_or(ChatError::NotInGuild)?;
    require_view(perms)?;
    if !parent.kind.hosts_threads() {
        return Err(ChatError::invalid(
            "channel_id",
            "CHANNEL_TYPE_INVALID",
            "Threads cannot be started in this channel type.",
        ));
    }
    let requested = input.kind.unwrap_or(ChannelType::PublicThread);
    let kind = match (requested, parent.kind) {
        (ChannelType::PublicThread, ChannelType::Announcement)
        | (ChannelType::AnnouncementThread, ChannelType::Announcement) => {
            require(perms, Permissions::CREATE_PUBLIC_THREADS)?;
            ChannelType::AnnouncementThread
        }
        (ChannelType::PublicThread, ChannelType::Text) => {
            require(perms, Permissions::CREATE_PUBLIC_THREADS)?;
            ChannelType::PublicThread
        }
        (ChannelType::PrivateThread, ChannelType::Text) => {
            require(perms, Permissions::CREATE_PRIVATE_THREADS)?;
            ChannelType::PrivateThread
        }
        _ => {
            return Err(ChatError::invalid(
                "type",
                "CHANNEL_TYPE_INVALID",
                "This thread type cannot be created in this channel.",
            ));
        }
    };
    let name = validate_thread_name(&input.name)?;
    let duration = duration_for(input.auto_archive_duration, parent)?;
    let mut c = base_thread(id, guild, parent, kind, actor, name, now_ms, duration);
    c.rate_limit_per_user = validate_slowmode(
        input
            .rate_limit_per_user
            .or(parent.forum.default_thread_rate_limit)
            .unwrap_or(0),
    )?;
    if let Some(t) = c.thread.as_mut()
        && kind == ChannelType::PrivateThread
    {
        t.invitable = input.invitable.unwrap_or(true);
    }
    Ok(c)
}

/// Validate a forum post's tag set against the forum's `available_tags`.
/// `previous` is the tag set being replaced (empty for a new post).
pub fn validate_applied_tags(
    forum: &Channel,
    tags: &[u64],
    previous: &[u64],
    perms: Permissions,
) -> ChatResult<Vec<u64>> {
    let mut out: Vec<u64> = Vec::with_capacity(tags.len());
    for t in tags {
        if !out.contains(t) {
            out.push(*t);
        }
    }
    if out.len() > MAX_APPLIED_TAGS {
        return Err(ChatError::invalid(
            "applied_tags",
            "BASE_TYPE_MAX_LENGTH",
            format!("Must be {MAX_APPLIED_TAGS} or fewer in length."),
        ));
    }
    let manage = perms.has(Permissions::MANAGE_THREADS);
    for t in &out {
        if forum.forum.tag(*t).is_none() {
            return Err(ChatError::invalid(
                "applied_tags",
                "UNKNOWN_TAG",
                "Unknown forum tag.",
            ));
        }
    }
    // Moderated tags are applied or removed by moderators only.
    let changed = out
        .iter()
        .filter(|t| !previous.contains(t))
        .chain(previous.iter().filter(|t| !out.contains(t)));
    for t in changed {
        if !manage && forum.forum.tag(*t).is_some_and(|tag| tag.moderated) {
            return Err(ChatError::MissingPermissions);
        }
    }
    if out.is_empty() && forum.flags & channel_flags::REQUIRE_TAG != 0 && !manage {
        return Err(ChatError::TagRequired);
    }
    Ok(out)
}

/// Plan a forum or media post. The creator needs `SEND_MESSAGES` in the forum.
pub fn plan_forum_post(
    id: ChannelId,
    actor: UserId,
    perms: Permissions,
    forum: &Channel,
    input: &ForumPost,
    now_ms: i64,
) -> ChatResult<Channel> {
    let guild = forum.guild_id.ok_or(ChatError::NotInGuild)?;
    require_view(perms)?;
    if !forum.kind.is_forum_like() {
        return Err(ChatError::invalid(
            "channel_id",
            "CHANNEL_TYPE_INVALID",
            "Posts can only be created in forum and media channels.",
        ));
    }
    require(perms, Permissions::SEND_MESSAGES)?;
    let name = validate_thread_name(&input.name)?;
    let tags = validate_applied_tags(forum, &input.applied_tags, &[], perms)?;
    let duration = duration_for(input.auto_archive_duration, forum)?;
    let mut c = base_thread(
        id,
        guild,
        forum,
        ChannelType::PublicThread,
        actor,
        name,
        now_ms,
        duration,
    );
    c.rate_limit_per_user = validate_slowmode(
        input
            .rate_limit_per_user
            .or(forum.forum.default_thread_rate_limit)
            .unwrap_or(0),
    )?;
    if let Some(t) = c.thread.as_mut() {
        t.applied_tags = tags;
    }
    Ok(c)
}

/// Changes to a thread (`PATCH /channels/{thread}`).
#[derive(Debug, Clone, Default)]
pub struct ThreadPatch {
    pub name: Option<String>,
    pub archived: Option<bool>,
    pub auto_archive_duration: Option<u32>,
    pub locked: Option<bool>,
    pub invitable: Option<bool>,
    pub rate_limit_per_user: Option<u32>,
    pub applied_tags: Option<Vec<u64>>,
    /// Only the `PINNED` bit is honoured.
    pub flags: Option<u32>,
}

/// Plan a thread edit. `perms` are the actor's permissions in the thread (inherited
/// from the parent); `parent` is needed for forum tags and pinning.
pub fn plan_modify_thread(
    actor: UserId,
    perms: Permissions,
    thread: &Channel,
    parent: Option<&Channel>,
    patch: &ThreadPatch,
    now_ms: i64,
) -> ChatResult<Channel> {
    require_view(perms)?;
    let info = thread.thread.as_ref().ok_or(ChatError::UnknownChannel)?;
    let manage = perms.has(Permissions::MANAGE_THREADS);
    let owner = thread.owner_id == Some(actor);
    let manage_or_owner = manage || owner;
    let mut out = thread.clone();
    let t = out.thread.as_mut().ok_or(ChatError::UnknownChannel)?;

    if info.archived && patch.archived != Some(false) {
        return Err(ChatError::ThreadArchived);
    }
    if let Some(archive) = patch.archived
        && archive != info.archived
    {
        if archive {
            if !manage_or_owner {
                return Err(ChatError::MissingPermissions);
            }
        } else {
            if info.locked && !manage {
                return Err(ChatError::ThreadLocked);
            }
            if !manage_or_owner {
                require(perms, Permissions::SEND_MESSAGES_IN_THREADS)?;
            }
            t.last_activity_ms = now_ms;
        }
        t.archived = archive;
        t.archive_timestamp_ms = now_ms;
    }
    if let Some(n) = &patch.name {
        if !manage_or_owner {
            return Err(ChatError::MissingPermissions);
        }
        out.name = Some(validate_thread_name(n)?);
    }
    if let Some(d) = patch.auto_archive_duration {
        if !manage_or_owner {
            return Err(ChatError::MissingPermissions);
        }
        t.auto_archive_duration = validate_auto_archive(d)?;
    }
    if let Some(i) = patch.invitable {
        if !manage_or_owner {
            return Err(ChatError::MissingPermissions);
        }
        if thread.kind != ChannelType::PrivateThread {
            return Err(ChatError::invalid(
                "invitable",
                "CHANNEL_TYPE_INVALID",
                "Only private threads have an invitable setting.",
            ));
        }
        t.invitable = i;
    }
    if let Some(l) = patch.locked {
        require(perms, Permissions::MANAGE_THREADS)?;
        t.locked = l;
    }
    if let Some(r) = patch.rate_limit_per_user {
        require(perms, Permissions::MANAGE_THREADS)?;
        out.rate_limit_per_user = validate_slowmode(r)?;
    }
    if let Some(tags) = &patch.applied_tags {
        if !manage_or_owner {
            return Err(ChatError::MissingPermissions);
        }
        let forum = parent.filter(|p| p.kind.is_forum_like()).ok_or_else(|| {
            ChatError::invalid(
                "applied_tags",
                "CHANNEL_TYPE_INVALID",
                "Tags only apply to forum and media posts.",
            )
        })?;
        t.applied_tags = validate_applied_tags(forum, tags, &info.applied_tags, perms)?;
    }
    if let Some(f) = patch.flags {
        let pinned = f & channel_flags::PINNED != 0;
        if pinned != (thread.flags & channel_flags::PINNED != 0) {
            require(perms, Permissions::MANAGE_THREADS)?;
            if !parent.is_some_and(|p| p.kind.is_forum_like()) {
                return Err(ChatError::invalid(
                    "flags",
                    "CHANNEL_TYPE_INVALID",
                    "Only forum and media posts can be pinned.",
                ));
            }
            out.flags = if pinned {
                out.flags | channel_flags::PINNED
            } else {
                out.flags & !channel_flags::PINNED
            };
        }
    }
    Ok(out)
}

/// Whether the actor may delete the thread: `MANAGE_THREADS`, or the owner of a
/// forum or media post.
pub fn check_delete_thread(
    actor: UserId,
    perms: Permissions,
    thread: &Channel,
    parent: Option<&Channel>,
) -> ChatResult<()> {
    require_view(perms)?;
    let owner_of_post =
        thread.owner_id == Some(actor) && parent.is_some_and(|p| p.kind.is_forum_like());
    if perms.has(Permissions::MANAGE_THREADS) || owner_of_post {
        Ok(())
    } else {
        Err(ChatError::MissingPermissions)
    }
}

/// Sending into a thread: a locked thread needs `MANAGE_THREADS`. An archived thread
/// accepts the message and is unarchived by it.
pub fn check_can_send(thread: &Channel, perms: Permissions) -> ChatResult<()> {
    let Some(info) = thread.thread.as_ref() else {
        return Ok(());
    };
    if info.locked && !perms.has(Permissions::MANAGE_THREADS) {
        return Err(if info.archived {
            ChatError::ThreadArchived
        } else {
            ChatError::ThreadLocked
        });
    }
    Ok(())
}

/// Joining or adding a member to a thread. `actor == target` is the self-join.
pub fn check_add_member(
    actor: UserId,
    target: UserId,
    perms: Permissions,
    actor_is_member: bool,
    thread: &Channel,
) -> ChatResult<()> {
    require_view(perms)?;
    let info = thread.thread.as_ref().ok_or(ChatError::UnknownChannel)?;
    if info.archived {
        return Err(ChatError::ThreadArchived);
    }
    if info.member_count as usize >= MAX_THREAD_MEMBERS {
        return Err(ChatError::invalid(
            "user_id",
            "MAX_THREAD_MEMBERS",
            format!("A thread holds at most {MAX_THREAD_MEMBERS} members."),
        ));
    }
    let manage = perms.has(Permissions::MANAGE_THREADS);
    if thread.kind == ChannelType::PrivateThread {
        if actor == target {
            // Joining a private thread by yourself needs an invitation (a membership
            // row already) or MANAGE_THREADS; the former makes this a no-op.
            return if manage || actor_is_member {
                Ok(())
            } else {
                Err(ChatError::MissingAccess)
            };
        }
        let allowed =
            manage || (actor_is_member && (info.invitable || thread.owner_id == Some(actor)));
        return if allowed {
            Ok(())
        } else {
            Err(ChatError::MissingPermissions)
        };
    }
    if actor != target && !manage && !perms.has(Permissions::SEND_MESSAGES_IN_THREADS) {
        return Err(ChatError::MissingPermissions);
    }
    Ok(())
}

/// Removing a member. Leaving (`actor == target`) is always allowed for members; removing
/// another member needs `MANAGE_THREADS`, or the owner of a private thread.
pub fn check_remove_member(
    actor: UserId,
    target: UserId,
    perms: Permissions,
    thread: &Channel,
) -> ChatResult<()> {
    require_view(perms)?;
    if actor == target {
        return Ok(());
    }
    let owner_of_private =
        thread.kind == ChannelType::PrivateThread && thread.owner_id == Some(actor);
    if perms.has(Permissions::MANAGE_THREADS) || owner_of_private {
        Ok(())
    } else {
        Err(ChatError::MissingPermissions)
    }
}

/// Validate and apply forum/media settings to `channel` (create and modify).
/// Settings that do not belong to the channel's type are ignored.
pub fn apply_forum_patch(channel: &mut Channel, patch: &ForumPatch) -> ChatResult<()> {
    let kind = channel.kind;
    if kind == crate::domain::ChannelType::Announcement
        && let Some(f) = patch.flags
    {
        let keep = channel.flags & !channel_flags::AUTO_PUBLISH;
        channel.flags = keep | (f & channel_flags::AUTO_PUBLISH);
    }
    if kind.hosts_threads() || kind.is_forum_like() {
        if let Some(d) = patch.default_auto_archive_duration {
            channel.forum.default_auto_archive_duration = Some(validate_auto_archive(d)?);
        }
        if let Some(r) = patch.default_thread_rate_limit {
            channel.forum.default_thread_rate_limit = Some(validate_slowmode(r)?);
        }
    }
    if !kind.is_forum_like() {
        return Ok(());
    }
    if let Some(tags) = &patch.available_tags {
        if tags.len() > MAX_AVAILABLE_TAGS {
            return Err(ChatError::invalid(
                "available_tags",
                "BASE_TYPE_MAX_LENGTH",
                format!("Must be {MAX_AVAILABLE_TAGS} or fewer in length."),
            ));
        }
        let mut out: Vec<ForumTag> = Vec::with_capacity(tags.len());
        for (n, t) in tags.iter().enumerate() {
            let name = t.name.trim().to_owned();
            let len = name.chars().count();
            if len == 0 || len > TAG_NAME_MAX {
                return Err(ChatError::invalid(
                    &format!("available_tags.{n}.name"),
                    "BASE_TYPE_BAD_LENGTH",
                    format!("Must be between 1 and {TAG_NAME_MAX} in length."),
                ));
            }
            let id = t.id.ok_or_else(|| {
                ChatError::invalid(
                    &format!("available_tags.{n}.id"),
                    "BASE_TYPE_REQUIRED",
                    "Tag ids are assigned before planning.",
                )
            })?;
            if out
                .iter()
                .any(|o| o.id == id || o.name.eq_ignore_ascii_case(&name))
            {
                return Err(ChatError::invalid(
                    &format!("available_tags.{n}"),
                    "DUPLICATE_TAG",
                    "Tag names and ids must be unique.",
                ));
            }
            out.push(ForumTag {
                id,
                name,
                moderated: t.moderated,
                emoji_id: t.emoji_id,
                emoji_name: t.emoji_name.clone(),
            });
        }
        channel.forum.available_tags = out;
    }
    if let Some(r) = &patch.default_reaction_emoji {
        channel.forum.default_reaction_emoji = r.clone().filter(|d: &DefaultReaction| {
            d.emoji_id.is_some() || d.emoji_name.as_deref().is_some_and(|n| !n.is_empty())
        });
    }
    if let Some(s) = patch.default_sort_order {
        if let Some(v) = s
            && v != sort_order::LATEST_ACTIVITY
            && v != sort_order::CREATION_DATE
        {
            return Err(ChatError::invalid(
                "default_sort_order",
                "ENUM_TYPE_INVALID",
                "Must be 0 (latest activity) or 1 (creation date).",
            ));
        }
        channel.forum.default_sort_order = s;
    }
    if let Some(l) = patch.default_forum_layout {
        if l > forum_layout::GALLERY {
            return Err(ChatError::invalid(
                "default_forum_layout",
                "ENUM_TYPE_INVALID",
                "Must be 0, 1 (list) or 2 (gallery).",
            ));
        }
        channel.forum.default_forum_layout = Some(l);
    }
    if let Some(f) = patch.flags {
        let keep = channel.flags & !channel_flags::FORUM_SETTABLE;
        channel.flags = keep | (f & channel_flags::FORUM_SETTABLE);
    }
    // Tags that disappeared are dropped from existing posts by the repository.
    Ok(())
}

/// Tags present in `before` but not in `after` (so posts can lose them).
pub fn removed_tag_ids(before: &ForumConfig, after: &ForumConfig) -> Vec<u64> {
    before
        .available_tags
        .iter()
        .filter(|t| after.tag(t.id).is_none())
        .map(|t| t.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Message, MessageType, Visibility};
    use sr_core::{MessageId, UserId};

    const ME: u64 = 5;

    fn text() -> Channel {
        let mut c = Channel::new(ChannelId::new(10), Some(GuildId::new(1)), ChannelType::Text);
        c.name = Some("general".into());
        c
    }

    fn forum() -> Channel {
        let mut c = Channel::new(
            ChannelId::new(11),
            Some(GuildId::new(1)),
            ChannelType::Forum,
        );
        c.forum.available_tags = vec![
            ForumTag {
                id: 100,
                name: "help".into(),
                moderated: false,
                emoji_id: None,
                emoji_name: None,
            },
            ForumTag {
                id: 101,
                name: "official".into(),
                moderated: true,
                emoji_id: None,
                emoji_name: None,
            },
        ];
        c
    }

    fn member_perms() -> Permissions {
        crate::perms::default_everyone_permissions()
    }

    fn message(kind: MessageType) -> Message {
        Message {
            id: MessageId::new(77),
            channel_id: ChannelId::new(10),
            guild_id: Some(GuildId::new(1)),
            author_id: UserId::new(2),
            kind,
            content: "hi".into(),
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

    fn start(name: &str) -> StartThread {
        StartThread {
            name: name.into(),
            ..StartThread::default()
        }
    }

    #[test]
    fn private_threads_hide_from_non_members() {
        let mut t = text();
        t.kind = ChannelType::PrivateThread;
        let p = member_perms();
        assert!(thread_permissions(p, &t, false).is_empty());
        assert_eq!(thread_permissions(p, &t, true), p);
        let mods = p | Permissions::MANAGE_THREADS;
        assert_eq!(thread_permissions(mods, &t, false), mods);
        t.kind = ChannelType::PublicThread;
        assert_eq!(thread_permissions(p, &t, false), p);
        assert!(thread_permissions(Permissions::empty(), &t, true).is_empty());
    }

    #[test]
    fn thread_from_message_uses_the_message_id() {
        let c = plan_thread_from_message(
            UserId::new(ME),
            member_perms(),
            &text(),
            &message(MessageType::Default),
            &start("Plans"),
            1_000,
        )
        .unwrap();
        assert_eq!(c.id.get(), 77);
        assert_eq!(c.kind, ChannelType::PublicThread);
        assert_eq!(c.parent_id, Some(ChannelId::new(10)));
        assert_eq!(c.owner_id, Some(UserId::new(ME)));
        let info = c.thread.unwrap();
        assert_eq!(info.auto_archive_duration, 1440);
        assert!(!info.archived);
    }

    #[test]
    fn thread_from_message_rules() {
        let sys = message(MessageType::UserJoin);
        assert!(
            plan_thread_from_message(
                UserId::new(ME),
                member_perms(),
                &text(),
                &sys,
                &start("x"),
                0
            )
            .is_err()
        );
        let no_perm = member_perms() - Permissions::CREATE_PUBLIC_THREADS;
        assert!(matches!(
            plan_thread_from_message(
                UserId::new(ME),
                no_perm,
                &text(),
                &message(MessageType::Default),
                &start("x"),
                0
            ),
            Err(ChatError::MissingPermissions)
        ));
        assert!(matches!(
            plan_thread_from_message(
                UserId::new(ME),
                member_perms(),
                &forum(),
                &message(MessageType::Default),
                &start("x"),
                0
            ),
            Err(ChatError::Invalid { .. })
        ));
        assert!(
            plan_thread_from_message(
                UserId::new(ME),
                member_perms(),
                &text(),
                &message(MessageType::Default),
                &start("  "),
                0
            )
            .is_err()
        );
        let mut bad = start("ok");
        bad.auto_archive_duration = Some(61);
        assert!(
            plan_thread_from_message(
                UserId::new(ME),
                member_perms(),
                &text(),
                &message(MessageType::Default),
                &bad,
                0
            )
            .is_err()
        );
    }

    #[test]
    fn announcement_parents_make_announcement_threads() {
        let mut p = text();
        p.kind = ChannelType::Announcement;
        let c = plan_thread_from_message(
            UserId::new(ME),
            member_perms(),
            &p,
            &message(MessageType::Default),
            &start("n"),
            0,
        )
        .unwrap();
        assert_eq!(c.kind, ChannelType::AnnouncementThread);
        // Private threads are not available in announcement channels.
        let mut input = start("p");
        input.kind = Some(ChannelType::PrivateThread);
        let perms = member_perms() | Permissions::CREATE_PRIVATE_THREADS;
        assert!(
            plan_standalone_thread(ChannelId::new(1), UserId::new(ME), perms, &p, &input, 0)
                .is_err()
        );
    }

    #[test]
    fn private_thread_needs_its_own_permission() {
        let mut input = start("secret");
        input.kind = Some(ChannelType::PrivateThread);
        input.invitable = Some(false);
        assert!(matches!(
            plan_standalone_thread(
                ChannelId::new(1),
                UserId::new(ME),
                member_perms(),
                &text(),
                &input,
                0
            ),
            Err(ChatError::MissingPermissions)
        ));
        let perms = member_perms() | Permissions::CREATE_PRIVATE_THREADS;
        let c = plan_standalone_thread(
            ChannelId::new(1),
            UserId::new(ME),
            perms,
            &text(),
            &input,
            0,
        )
        .unwrap();
        assert_eq!(c.kind, ChannelType::PrivateThread);
        assert!(!c.thread.unwrap().invitable);
        // Public by default.
        let c = plan_standalone_thread(
            ChannelId::new(2),
            UserId::new(ME),
            member_perms(),
            &text(),
            &start("p"),
            0,
        )
        .unwrap();
        assert_eq!(c.kind, ChannelType::PublicThread);
    }

    #[test]
    fn forum_posts_check_tags() {
        let post = |tags: Vec<u64>| ForumPost {
            name: "How do I?".into(),
            applied_tags: tags,
            ..ForumPost::default()
        };
        let p = member_perms();
        let ok = plan_forum_post(
            ChannelId::new(1),
            UserId::new(ME),
            p,
            &forum(),
            &post(vec![100]),
            0,
        )
        .unwrap();
        assert_eq!(ok.thread.unwrap().applied_tags, vec![100]);
        assert!(
            plan_forum_post(
                ChannelId::new(1),
                UserId::new(ME),
                p,
                &forum(),
                &post(vec![999]),
                0
            )
            .is_err()
        );
        // Moderated tags are for moderators.
        assert!(matches!(
            plan_forum_post(
                ChannelId::new(1),
                UserId::new(ME),
                p,
                &forum(),
                &post(vec![101]),
                0
            ),
            Err(ChatError::MissingPermissions)
        ));
        assert!(
            plan_forum_post(
                ChannelId::new(1),
                UserId::new(ME),
                p | Permissions::MANAGE_THREADS,
                &forum(),
                &post(vec![101]),
                0
            )
            .is_ok()
        );
        // At most five, duplicates collapse.
        let mut many = forum();
        many.forum.available_tags = (1..=8)
            .map(|i| ForumTag {
                id: i,
                name: format!("t{i}"),
                moderated: false,
                emoji_id: None,
                emoji_name: None,
            })
            .collect();
        assert!(
            plan_forum_post(
                ChannelId::new(1),
                UserId::new(ME),
                p,
                &many,
                &post(vec![1, 2, 3, 4, 5, 6]),
                0
            )
            .is_err()
        );
        let dup = plan_forum_post(
            ChannelId::new(1),
            UserId::new(ME),
            p,
            &many,
            &post(vec![1, 1, 2]),
            0,
        )
        .unwrap();
        assert_eq!(dup.thread.unwrap().applied_tags, vec![1, 2]);
        // Required tag.
        let mut req = forum();
        req.flags |= channel_flags::REQUIRE_TAG;
        assert!(matches!(
            plan_forum_post(
                ChannelId::new(1),
                UserId::new(ME),
                p,
                &req,
                &post(vec![]),
                0
            ),
            Err(ChatError::TagRequired)
        ));
        // Not in a text channel.
        assert!(
            plan_forum_post(
                ChannelId::new(1),
                UserId::new(ME),
                p,
                &text(),
                &post(vec![]),
                0
            )
            .is_err()
        );
    }

    #[test]
    fn forum_post_defaults_come_from_the_forum() {
        let mut f = forum();
        f.forum.default_thread_rate_limit = Some(30);
        f.forum.default_auto_archive_duration = Some(10_080);
        let c = plan_forum_post(
            ChannelId::new(1),
            UserId::new(ME),
            member_perms(),
            &f,
            &ForumPost {
                name: "p".into(),
                ..ForumPost::default()
            },
            5,
        )
        .unwrap();
        assert_eq!(c.rate_limit_per_user, 30);
        assert_eq!(c.thread.unwrap().auto_archive_duration, 10_080);
    }

    fn thread() -> Channel {
        let mut c = Channel::new(
            ChannelId::new(77),
            Some(GuildId::new(1)),
            ChannelType::PublicThread,
        );
        c.parent_id = Some(ChannelId::new(10));
        c.owner_id = Some(UserId::new(ME));
        c.name = Some("t".into());
        c.thread = Some(ThreadInfo::new(0, 60));
        c
    }

    #[test]
    fn owner_can_archive_and_rename_others_cannot() {
        let p = member_perms();
        let archive = ThreadPatch {
            archived: Some(true),
            ..ThreadPatch::default()
        };
        let out = plan_modify_thread(UserId::new(ME), p, &thread(), None, &archive, 500).unwrap();
        let info = out.thread.unwrap();
        assert!(info.archived);
        assert_eq!(info.archive_timestamp_ms, 500);
        assert!(matches!(
            plan_modify_thread(UserId::new(9), p, &thread(), None, &archive, 500),
            Err(ChatError::MissingPermissions)
        ));
        let rename = ThreadPatch {
            name: Some("new".into()),
            ..ThreadPatch::default()
        };
        assert!(plan_modify_thread(UserId::new(ME), p, &thread(), None, &rename, 0).is_ok());
        assert!(plan_modify_thread(UserId::new(9), p, &thread(), None, &rename, 0).is_err());
        assert!(
            plan_modify_thread(
                UserId::new(9),
                p | Permissions::MANAGE_THREADS,
                &thread(),
                None,
                &rename,
                0
            )
            .is_ok()
        );
    }

    #[test]
    fn locking_and_slowmode_need_manage_threads() {
        let p = member_perms();
        let lock = ThreadPatch {
            locked: Some(true),
            ..ThreadPatch::default()
        };
        assert!(matches!(
            plan_modify_thread(UserId::new(ME), p, &thread(), None, &lock, 0),
            Err(ChatError::MissingPermissions)
        ));
        let m = p | Permissions::MANAGE_THREADS;
        let out = plan_modify_thread(UserId::new(9), m, &thread(), None, &lock, 0).unwrap();
        assert!(out.thread.unwrap().locked);
        let slow = ThreadPatch {
            rate_limit_per_user: Some(10),
            ..ThreadPatch::default()
        };
        assert!(plan_modify_thread(UserId::new(ME), p, &thread(), None, &slow, 0).is_err());
        assert_eq!(
            plan_modify_thread(UserId::new(9), m, &thread(), None, &slow, 0)
                .unwrap()
                .rate_limit_per_user,
            10
        );
        let too = ThreadPatch {
            rate_limit_per_user: Some(99_999),
            ..ThreadPatch::default()
        };
        assert!(plan_modify_thread(UserId::new(9), m, &thread(), None, &too, 0).is_err());
    }

    #[test]
    fn archived_threads_only_accept_an_unarchive() {
        let mut t = thread();
        t.thread.as_mut().unwrap().archived = true;
        let p = member_perms();
        let rename = ThreadPatch {
            name: Some("x".into()),
            ..ThreadPatch::default()
        };
        assert!(matches!(
            plan_modify_thread(UserId::new(ME), p, &t, None, &rename, 0),
            Err(ChatError::ThreadArchived)
        ));
        let open = ThreadPatch {
            archived: Some(false),
            ..ThreadPatch::default()
        };
        let out = plan_modify_thread(UserId::new(ME), p, &t, None, &open, 9_000).unwrap();
        let info = out.thread.unwrap();
        assert!(!info.archived);
        assert_eq!(info.last_activity_ms, 9_000);
        // A locked archived thread stays closed for its owner.
        t.thread.as_mut().unwrap().locked = true;
        assert!(matches!(
            plan_modify_thread(UserId::new(ME), p, &t, None, &open, 0),
            Err(ChatError::ThreadLocked)
        ));
        assert!(
            plan_modify_thread(
                UserId::new(9),
                p | Permissions::MANAGE_THREADS,
                &t,
                None,
                &open,
                0
            )
            .is_ok()
        );
    }

    #[test]
    fn pinning_and_tags_on_posts() {
        let p = member_perms();
        let m = p | Permissions::MANAGE_THREADS;
        let f = forum();
        let mut post = thread();
        post.parent_id = Some(f.id);
        let pin = ThreadPatch {
            flags: Some(channel_flags::PINNED),
            ..ThreadPatch::default()
        };
        assert!(plan_modify_thread(UserId::new(ME), p, &post, Some(&f), &pin, 0).is_err());
        let pinned = plan_modify_thread(UserId::new(9), m, &post, Some(&f), &pin, 0).unwrap();
        assert_eq!(pinned.flags & channel_flags::PINNED, channel_flags::PINNED);
        // Pinning only makes sense in forums.
        assert!(plan_modify_thread(UserId::new(9), m, &thread(), Some(&text()), &pin, 0).is_err());
        let tag = ThreadPatch {
            applied_tags: Some(vec![100]),
            ..ThreadPatch::default()
        };
        let tagged = plan_modify_thread(UserId::new(ME), p, &post, Some(&f), &tag, 0).unwrap();
        assert_eq!(tagged.thread.unwrap().applied_tags, vec![100]);
        assert!(plan_modify_thread(UserId::new(ME), p, &thread(), Some(&text()), &tag, 0).is_err());
    }

    #[test]
    fn locked_threads_reject_senders_without_manage() {
        let mut t = thread();
        assert!(check_can_send(&t, member_perms()).is_ok());
        t.thread.as_mut().unwrap().locked = true;
        assert!(matches!(
            check_can_send(&t, member_perms()),
            Err(ChatError::ThreadLocked)
        ));
        assert!(check_can_send(&t, member_perms() | Permissions::MANAGE_THREADS).is_ok());
        t.thread.as_mut().unwrap().archived = true;
        assert!(matches!(
            check_can_send(&t, member_perms()),
            Err(ChatError::ThreadArchived)
        ));
    }

    #[test]
    fn member_rules() {
        let p = member_perms();
        let t = thread();
        assert!(check_add_member(UserId::new(3), UserId::new(3), p, false, &t).is_ok());
        assert!(check_add_member(UserId::new(3), UserId::new(4), p, false, &t).is_ok());
        assert!(
            check_add_member(
                UserId::new(3),
                UserId::new(4),
                p - Permissions::SEND_MESSAGES_IN_THREADS,
                false,
                &t
            )
            .is_err()
        );
        let mut arch = thread();
        arch.thread.as_mut().unwrap().archived = true;
        assert!(matches!(
            check_add_member(UserId::new(3), UserId::new(3), p, false, &arch),
            Err(ChatError::ThreadArchived)
        ));
        // Private: invitable gate.
        let mut private = thread();
        private.kind = ChannelType::PrivateThread;
        private.thread.as_mut().unwrap().invitable = false;
        assert!(check_add_member(UserId::new(3), UserId::new(4), p, true, &private).is_err());
        assert!(check_add_member(UserId::new(ME), UserId::new(4), p, true, &private).is_ok());
        assert!(
            check_add_member(
                UserId::new(3),
                UserId::new(4),
                p | Permissions::MANAGE_THREADS,
                false,
                &private
            )
            .is_ok()
        );
        // Removal.
        assert!(check_remove_member(UserId::new(3), UserId::new(3), p, &t).is_ok());
        assert!(check_remove_member(UserId::new(3), UserId::new(4), p, &t).is_err());
        assert!(check_remove_member(UserId::new(ME), UserId::new(4), p, &private).is_ok());
        assert!(
            check_remove_member(
                UserId::new(3),
                UserId::new(4),
                p | Permissions::MANAGE_THREADS,
                &t
            )
            .is_ok()
        );
        // Full threads refuse new members.
        let mut full = thread();
        full.thread.as_mut().unwrap().member_count = 1000;
        assert!(check_add_member(UserId::new(3), UserId::new(3), p, false, &full).is_err());
    }

    #[test]
    fn deleting_threads() {
        let p = member_perms();
        let f = forum();
        let t = thread();
        assert!(check_delete_thread(UserId::new(ME), p, &t, Some(&text())).is_err());
        assert!(check_delete_thread(UserId::new(ME), p, &t, Some(&f)).is_ok());
        assert!(
            check_delete_thread(
                UserId::new(9),
                p | Permissions::MANAGE_THREADS,
                &t,
                Some(&text())
            )
            .is_ok()
        );
    }

    fn input(name: &str, moderated: bool, id: Option<u64>) -> super::super::model::TagInput {
        super::super::model::TagInput {
            id,
            name: name.into(),
            moderated,
            emoji_id: None,
            emoji_name: None,
        }
    }

    #[test]
    fn forum_settings_validate() {
        let mut f = forum();
        let patch = ForumPatch {
            available_tags: Some(vec![input("a", false, Some(1)), input("b", true, Some(2))]),
            default_sort_order: Some(Some(1)),
            default_forum_layout: Some(2),
            default_thread_rate_limit: Some(60),
            flags: Some(channel_flags::REQUIRE_TAG | channel_flags::PINNED),
            ..ForumPatch::default()
        };
        apply_forum_patch(&mut f, &patch).unwrap();
        assert_eq!(f.forum.available_tags.len(), 2);
        assert_eq!(f.forum.default_sort_order, Some(1));
        assert_eq!(f.forum.default_forum_layout, Some(2));
        // PINNED is not a forum-channel setting.
        assert_eq!(f.flags, channel_flags::REQUIRE_TAG);
        assert_eq!(removed_tag_ids(&forum().forum, &f.forum), vec![100, 101]);

        let dup = ForumPatch {
            available_tags: Some(vec![input("a", false, Some(1)), input("A", false, Some(2))]),
            ..ForumPatch::default()
        };
        assert!(apply_forum_patch(&mut f, &dup).is_err());
        let long = ForumPatch {
            available_tags: Some(vec![input(&"x".repeat(21), false, Some(1))]),
            ..ForumPatch::default()
        };
        assert!(apply_forum_patch(&mut f, &long).is_err());
        let bad = ForumPatch {
            default_forum_layout: Some(3),
            ..ForumPatch::default()
        };
        assert!(apply_forum_patch(&mut f, &bad).is_err());
        let bad = ForumPatch {
            default_sort_order: Some(Some(2)),
            ..ForumPatch::default()
        };
        assert!(apply_forum_patch(&mut f, &bad).is_err());
        // Text channels only take the thread defaults.
        let mut t = text();
        let patch = ForumPatch {
            default_auto_archive_duration: Some(60),
            default_forum_layout: Some(1),
            ..ForumPatch::default()
        };
        apply_forum_patch(&mut t, &patch).unwrap();
        assert_eq!(t.forum.default_auto_archive_duration, Some(60));
        assert_eq!(t.forum.default_forum_layout, None);
    }
}
