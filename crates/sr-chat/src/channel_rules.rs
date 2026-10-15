//! Pure channel CRUD and permission-overwrite rules (LNG-CHN-002, LNG-CHN-003).

use crate::domain::{Channel, ChannelType, Role};
use crate::error::{ChatError, ChatResult};
use crate::limits::ChatLimits;
use crate::perms::GuildActor;
use crate::threads::model::ForumPatch;
use crate::threads::rules::apply_forum_patch;
use sr_core::permissions::{Overwrite, OverwriteKind, Permissions};
use sr_core::{ChannelId, GuildId};

pub const CHANNEL_NAME_MAX: usize = 100;
pub const TOPIC_MAX: usize = 1024;
pub const FORUM_TOPIC_MAX: usize = 4096;
pub const SLOWMODE_MAX_SECS: u32 = 21_600;
pub const BITRATE_MIN: u32 = 8_000;
pub const BITRATE_MAX: u32 = 384_000;
pub const USER_LIMIT_MAX: u32 = 99;

#[derive(Debug, Clone)]
pub struct NewChannel {
    pub kind: ChannelType,
    pub name: String,
    pub topic: Option<String>,
    pub parent_id: Option<ChannelId>,
    pub position: Option<i32>,
    pub nsfw: bool,
    pub rate_limit_per_user: u32,
    pub bitrate: Option<u32>,
    pub user_limit: Option<u32>,
    /// Forum, media and thread-default settings.
    pub forum: ForumPatch,
}

impl NewChannel {
    pub fn new(kind: ChannelType, name: impl Into<String>) -> Self {
        Self {
            kind,
            name: name.into(),
            topic: None,
            parent_id: None,
            position: None,
            nsfw: false,
            rate_limit_per_user: 0,
            bitrate: None,
            user_limit: None,
            forum: ForumPatch::default(),
        }
    }
}

/// Text-like channels are lowercased with spaces turned into hyphens, as the
/// create-channel modal does (LNG-CHN-015). Voice, stage and category names keep
/// their case and spaces.
pub fn normalize_channel_name(kind: ChannelType, raw: &str) -> String {
    let raw = raw.trim();
    match kind {
        ChannelType::Voice | ChannelType::Stage | ChannelType::Category => raw.to_owned(),
        _ => {
            let mut out = String::with_capacity(raw.len());
            let mut last_dash = false;
            for c in raw.chars().flat_map(char::to_lowercase) {
                if c.is_whitespace() || c == '-' {
                    if !last_dash && !out.is_empty() {
                        out.push('-');
                    }
                    last_dash = true;
                } else {
                    out.push(c);
                    last_dash = false;
                }
            }
            out.trim_end_matches('-').to_owned()
        }
    }
}

fn validate_common(
    kind: ChannelType,
    name: &str,
    topic: Option<&str>,
    rate: u32,
    bitrate: Option<u32>,
    user_limit: Option<u32>,
) -> ChatResult<String> {
    let name = normalize_channel_name(kind, name);
    let n = name.chars().count();
    if n == 0 || n > CHANNEL_NAME_MAX {
        return Err(ChatError::invalid(
            "name",
            "BASE_TYPE_BAD_LENGTH",
            format!("Must be between 1 and {CHANNEL_NAME_MAX} in length."),
        ));
    }
    let topic_max = if kind.is_forum_like() {
        FORUM_TOPIC_MAX
    } else {
        TOPIC_MAX
    };
    if topic.is_some_and(|t| t.chars().count() > topic_max) {
        return Err(ChatError::invalid(
            "topic",
            "BASE_TYPE_MAX_LENGTH",
            format!("Must be {topic_max} or fewer in length."),
        ));
    }
    if rate > SLOWMODE_MAX_SECS {
        return Err(ChatError::invalid(
            "rate_limit_per_user",
            "NUMBER_TYPE_MAX",
            format!("Must be {SLOWMODE_MAX_SECS} or less."),
        ));
    }
    if bitrate.is_some_and(|b| !(BITRATE_MIN..=BITRATE_MAX).contains(&b)) {
        return Err(ChatError::invalid(
            "bitrate",
            "NUMBER_TYPE_RANGE",
            format!("Must be between {BITRATE_MIN} and {BITRATE_MAX}."),
        ));
    }
    if user_limit.is_some_and(|u| u > USER_LIMIT_MAX) {
        return Err(ChatError::invalid(
            "user_limit",
            "NUMBER_TYPE_MAX",
            format!("Must be {USER_LIMIT_MAX} or less."),
        ));
    }
    Ok(name)
}

/// Build a new guild channel. `parent` must be the loaded category when
/// `input.parent_id` is set; `existing_channel_count` includes categories.
pub fn plan_create_channel(
    actor: &GuildActor,
    guild_id: GuildId,
    id: ChannelId,
    existing_channel_count: usize,
    parent: Option<&Channel>,
    input: NewChannel,
    limits: &ChatLimits,
) -> ChatResult<Channel> {
    if !actor.perms.has(Permissions::MANAGE_CHANNELS) {
        return Err(ChatError::MissingPermissions);
    }
    if !input.kind.creatable_in_guild() {
        return Err(ChatError::invalid(
            "type",
            "CHANNEL_TYPE_INVALID",
            "This channel type cannot be created here.",
        ));
    }
    if existing_channel_count >= limits.max_channels_per_guild {
        return Err(ChatError::MaxChannels);
    }
    let name = validate_common(
        input.kind,
        &input.name,
        input.topic.as_deref(),
        input.rate_limit_per_user,
        input.bitrate,
        input.user_limit,
    )?;
    match (input.parent_id, parent) {
        (None, _) => {}
        (Some(_), _) if input.kind == ChannelType::Category => {
            return Err(ChatError::invalid(
                "parent_id",
                "CATEGORY_CANNOT_HAVE_PARENT",
                "Categories cannot be nested.",
            ));
        }
        (Some(pid), Some(p))
            if p.id == pid && p.kind == ChannelType::Category && p.guild_id == Some(guild_id) => {}
        (Some(_), _) => {
            return Err(ChatError::invalid(
                "parent_id",
                "CHANNEL_PARENT_INVALID",
                "Parent must be a category in this server.",
            ));
        }
    }
    let mut c = Channel::new(id, Some(guild_id), input.kind);
    c.name = Some(name);
    // Categories hold no messages, so topic/slowmode/nsfw only apply to other kinds.
    let is_cat = input.kind == ChannelType::Category;
    c.topic = if is_cat { None } else { input.topic };
    c.nsfw = !is_cat && input.nsfw;
    c.rate_limit_per_user = if is_cat { 0 } else { input.rate_limit_per_user };
    if input.kind.is_voice_like() {
        c.bitrate = Some(input.bitrate.unwrap_or(64_000));
        c.user_limit = input.user_limit;
    }
    c.parent_id = input.parent_id;
    c.position = input.position.unwrap_or(0).max(0);
    apply_forum_patch(&mut c, &input.forum)?;
    Ok(c)
}

#[derive(Debug, Clone, Default)]
pub struct ChannelPatch {
    /// Convert between a text and an announcement channel (Discord's `type` 0 and 5). Any
    /// other change of type is refused.
    pub kind: Option<ChannelType>,
    pub name: Option<String>,
    pub topic: Option<Option<String>>,
    pub nsfw: Option<bool>,
    pub rate_limit_per_user: Option<u32>,
    pub bitrate: Option<u32>,
    pub user_limit: Option<u32>,
    pub forum: ForumPatch,
}

pub fn plan_edit_channel(
    actor: &GuildActor,
    channel: &Channel,
    patch: ChannelPatch,
) -> ChatResult<Channel> {
    if !actor.perms.has(Permissions::MANAGE_CHANNELS) {
        return Err(ChatError::MissingPermissions);
    }
    if channel.guild_id.is_none() {
        return Err(ChatError::NotInGuild);
    }
    let mut out = channel.clone();
    if let Some(k) = patch.kind
        && k != channel.kind
    {
        let swap = matches!(
            (channel.kind, k),
            (ChannelType::Text, ChannelType::Announcement)
                | (ChannelType::Announcement, ChannelType::Text)
        );
        if !swap {
            return Err(ChatError::invalid(
                "type",
                "CHANNEL_TYPE_INVALID",
                "Only a text channel can become an announcement channel and back.",
            ));
        }
        out.kind = k;
    }
    let name_in = patch
        .name
        .as_deref()
        .or(channel.name.as_deref())
        .unwrap_or_default();
    let topic = match &patch.topic {
        Some(t) => t.as_deref(),
        None => channel.topic.as_deref(),
    };
    let name = validate_common(
        channel.kind,
        name_in,
        topic,
        patch
            .rate_limit_per_user
            .unwrap_or(channel.rate_limit_per_user),
        patch.bitrate.or(channel.bitrate),
        patch.user_limit.or(channel.user_limit),
    )?;
    out.name = Some(name);
    if let Some(t) = patch.topic {
        out.topic = t;
    }
    if let Some(n) = patch.nsfw {
        out.nsfw = n;
    }
    if let Some(r) = patch.rate_limit_per_user {
        out.rate_limit_per_user = r;
    }
    if channel.kind.is_voice_like() {
        out.bitrate = patch.bitrate.or(channel.bitrate);
        out.user_limit = patch.user_limit.or(channel.user_limit);
    }
    apply_forum_patch(&mut out, &patch.forum)?;
    Ok(out)
}

/// Deleting a category orphans (does not delete) its children; the repository
/// clears `parent_id` (`ON DELETE SET NULL`).
pub fn check_delete_channel(actor: &GuildActor, channel: &Channel) -> ChatResult<()> {
    if !actor.perms.has(Permissions::MANAGE_CHANNELS) {
        return Err(ChatError::MissingPermissions);
    }
    if channel.guild_id.is_none() {
        return Err(ChatError::NotInGuild);
    }
    Ok(())
}

/// Validate setting an overwrite for `target`. Rules (Discord parity):
/// the editor needs MANAGE_ROLES *in the channel*, may only allow/deny bits they hold
/// there, and cannot edit an overwrite for a role at or above their top role.
pub fn check_overwrite_edit(
    actor: &GuildActor,
    actor_channel_perms: Permissions,
    target_kind: OverwriteKind,
    target_role: Option<&Role>,
    allow: Permissions,
    deny: Permissions,
) -> ChatResult<()> {
    if !actor_channel_perms.has(Permissions::MANAGE_ROLES) {
        return Err(ChatError::MissingPermissions);
    }
    if !(allow & deny).is_empty() {
        return Err(ChatError::invalid(
            "allow",
            "OVERWRITE_CONFLICT",
            "A permission cannot be both allowed and denied.",
        ));
    }
    let bypass = actor.is_owner || actor_channel_perms.contains(Permissions::ADMINISTRATOR);
    if !bypass && !actor_channel_perms.contains(allow | deny) {
        return Err(ChatError::MissingPermissions);
    }
    if target_kind == OverwriteKind::Role {
        let role = target_role.ok_or(ChatError::UnknownRole)?;
        // `@everyone` (position 0) is always editable; otherwise outrank the role.
        if !role.is_everyone() && !actor.outranks_position(role.position) {
            return Err(ChatError::MissingPermissions);
        }
    }
    Ok(())
}

/// Replace-or-insert an overwrite in a list (`PUT /channels/{id}/permissions/{target}`);
/// empty allow+deny removes it.
pub fn apply_overwrite(list: &mut Vec<Overwrite>, new: Overwrite) {
    list.retain(|o| !(o.id == new.id && o.kind == new.kind));
    if !(new.allow.is_empty() && new.deny.is_empty()) {
        list.push(new);
    }
}

/// Make a channel's overwrites identical to its category's ("sync permissions").
pub fn synced_overwrites(category_overwrites: &[Overwrite]) -> Vec<Overwrite> {
    category_overwrites.to_vec()
}

/// A channel is "synced" when its overwrites equal its parent's, ignoring order.
pub fn is_synced(channel: &[Overwrite], category: &[Overwrite]) -> bool {
    channel.len() == category.len() && channel.iter().all(|o| category.contains(o))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perms::fixtures::*;
    use sr_core::Snowflake;

    fn admin() -> GuildActor {
        GuildActor {
            is_owner: false,
            perms: Permissions::ALL,
            top_position: 10,
        }
    }

    fn manager() -> GuildActor {
        GuildActor {
            is_owner: false,
            perms: Permissions::MANAGE_CHANNELS | Permissions::MANAGE_ROLES,
            top_position: 3,
        }
    }

    fn make(
        a: &GuildActor,
        count: usize,
        parent: Option<&Channel>,
        input: NewChannel,
    ) -> ChatResult<Channel> {
        plan_create_channel(
            a,
            GuildId::new(GUILD),
            ChannelId::new(500),
            count,
            parent,
            input,
            &ChatLimits::default(),
        )
    }

    fn category() -> Channel {
        let mut c = Channel::new(
            ChannelId::new(400),
            Some(GuildId::new(GUILD)),
            ChannelType::Category,
        );
        c.name = Some("cat".into());
        c
    }

    #[test]
    fn name_normalisation() {
        use ChannelType::*;
        assert_eq!(
            normalize_channel_name(Text, "  My  Cool Channel "),
            "my-cool-channel"
        );
        assert_eq!(normalize_channel_name(Text, "a - b"), "a-b");
        assert_eq!(normalize_channel_name(Text, "---"), "");
        assert_eq!(normalize_channel_name(Voice, " Game Night "), "Game Night");
        assert_eq!(
            normalize_channel_name(Category, "Text Channels"),
            "Text Channels"
        );
    }

    #[test]
    fn create_requires_permission_and_valid_type() {
        let none = GuildActor {
            is_owner: false,
            perms: Permissions::empty(),
            top_position: 0,
        };
        assert!(matches!(
            make(&none, 0, None, NewChannel::new(ChannelType::Text, "x")),
            Err(ChatError::MissingPermissions)
        ));
        assert!(
            make(
                &manager(),
                0,
                None,
                NewChannel::new(ChannelType::PublicThread, "x")
            )
            .is_err()
        );
        assert!(make(&manager(), 0, None, NewChannel::new(ChannelType::Dm, "x")).is_err());
    }

    #[test]
    fn create_enforces_limits() {
        let a = manager();
        assert!(matches!(
            make(&a, 500, None, NewChannel::new(ChannelType::Text, "x")),
            Err(ChatError::MaxChannels)
        ));
        assert!(make(&a, 499, None, NewChannel::new(ChannelType::Text, "x")).is_ok());
        assert!(make(&a, 0, None, NewChannel::new(ChannelType::Text, "!!!")).is_ok());
        assert!(make(&a, 0, None, NewChannel::new(ChannelType::Text, "   ")).is_err());
        assert!(
            make(
                &a,
                0,
                None,
                NewChannel::new(ChannelType::Text, "n".repeat(101))
            )
            .is_err()
        );
        let mut i = NewChannel::new(ChannelType::Text, "x");
        i.topic = Some("t".repeat(1025));
        assert!(make(&a, 0, None, i).is_err());
        let mut i = NewChannel::new(ChannelType::Text, "x");
        i.rate_limit_per_user = 21_601;
        assert!(make(&a, 0, None, i).is_err());
        let mut i = NewChannel::new(ChannelType::Voice, "v");
        i.bitrate = Some(7_999);
        assert!(make(&a, 0, None, i).is_err());
        let mut i = NewChannel::new(ChannelType::Voice, "v");
        i.user_limit = Some(100);
        assert!(make(&a, 0, None, i).is_err());
    }

    #[test]
    fn parent_rules() {
        let a = admin();
        let cat = category();
        let mut i = NewChannel::new(ChannelType::Text, "x");
        i.parent_id = Some(cat.id);
        let c = make(&a, 0, Some(&cat), i.clone()).unwrap();
        assert_eq!(c.parent_id, Some(cat.id));
        // Parent missing, or not a category, or in another guild.
        assert!(make(&a, 0, None, i.clone()).is_err());
        let mut text = cat.clone();
        text.kind = ChannelType::Text;
        assert!(make(&a, 0, Some(&text), i.clone()).is_err());
        let mut other = cat.clone();
        other.guild_id = Some(GuildId::new(999));
        assert!(make(&a, 0, Some(&other), i).is_err());
        // Categories cannot be nested.
        let mut ci = NewChannel::new(ChannelType::Category, "c");
        ci.parent_id = Some(cat.id);
        assert!(make(&a, 0, Some(&cat), ci).is_err());
    }

    #[test]
    fn voice_gets_default_bitrate_and_categories_drop_text_fields() {
        let a = admin();
        let v = make(&a, 0, None, NewChannel::new(ChannelType::Voice, "Lobby")).unwrap();
        assert_eq!(v.bitrate, Some(64_000));
        let mut ci = NewChannel::new(ChannelType::Category, "Cat");
        ci.topic = Some("ignored".into());
        ci.nsfw = true;
        ci.rate_limit_per_user = 5;
        let c = make(&a, 0, None, ci).unwrap();
        assert_eq!((c.topic, c.nsfw, c.rate_limit_per_user), (None, false, 0));
    }

    #[test]
    fn edit_and_delete() {
        let a = manager();
        let mut ch = Channel::new(
            ChannelId::new(5),
            Some(GuildId::new(GUILD)),
            ChannelType::Text,
        );
        ch.name = Some("old".into());
        let patch = ChannelPatch {
            name: Some("New Name".into()),
            topic: Some(Some("hi".into())),
            rate_limit_per_user: Some(10),
            ..Default::default()
        };
        let out = plan_edit_channel(&a, &ch, patch).unwrap();
        assert_eq!(out.name.as_deref(), Some("new-name"));
        assert_eq!(out.topic.as_deref(), Some("hi"));
        assert_eq!(out.rate_limit_per_user, 10);
        let clear = plan_edit_channel(
            &a,
            &out,
            ChannelPatch {
                topic: Some(None),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(clear.topic, None);
        let bad = ChannelPatch {
            rate_limit_per_user: Some(99_999),
            ..Default::default()
        };
        assert!(plan_edit_channel(&a, &ch, bad).is_err());
        assert!(check_delete_channel(&a, &ch).is_ok());
        let none = GuildActor {
            is_owner: false,
            perms: Permissions::empty(),
            top_position: 0,
        };
        assert!(check_delete_channel(&none, &ch).is_err());
        let dm = Channel::new(ChannelId::new(6), None, ChannelType::Dm);
        assert!(matches!(
            check_delete_channel(&a, &dm),
            Err(ChatError::NotInGuild)
        ));
    }

    #[test]
    fn overwrite_rules() {
        let a = manager();
        let perms =
            Permissions::MANAGE_ROLES | Permissions::SEND_MESSAGES | Permissions::VIEW_CHANNEL;
        let low = role(300, 2, Permissions::empty());
        let high = role(301, 3, Permissions::empty());
        let check = |role: Option<&Role>, allow, deny| {
            check_overwrite_edit(&a, perms, OverwriteKind::Role, role, allow, deny)
        };
        assert!(check(Some(&low), Permissions::SEND_MESSAGES, Permissions::empty()).is_ok());
        // Not outranking the role.
        assert!(
            check(
                Some(&high),
                Permissions::SEND_MESSAGES,
                Permissions::empty()
            )
            .is_err()
        );
        // @everyone always allowed.
        assert!(
            check(
                Some(&everyone()),
                Permissions::empty(),
                Permissions::SEND_MESSAGES
            )
            .is_ok()
        );
        // Bits the editor lacks in the channel.
        assert!(check(Some(&low), Permissions::KICK_MEMBERS, Permissions::empty()).is_err());
        // Conflicting bits.
        assert!(
            check(
                Some(&low),
                Permissions::SEND_MESSAGES,
                Permissions::SEND_MESSAGES
            )
            .is_err()
        );
        // Unknown role target.
        assert!(matches!(
            check(None, Permissions::empty(), Permissions::empty()),
            Err(ChatError::UnknownRole)
        ));
        // Needs MANAGE_ROLES in the channel.
        assert!(
            check_overwrite_edit(
                &a,
                Permissions::SEND_MESSAGES,
                OverwriteKind::Member,
                None,
                Permissions::empty(),
                Permissions::empty()
            )
            .is_err()
        );
        // Member targets skip the hierarchy check.
        assert!(
            check_overwrite_edit(
                &a,
                perms,
                OverwriteKind::Member,
                None,
                Permissions::SEND_MESSAGES,
                Permissions::empty()
            )
            .is_ok()
        );
    }

    #[test]
    fn apply_and_sync() {
        let ow = |id, allow| Overwrite {
            id: Snowflake::new(id),
            kind: OverwriteKind::Role,
            allow,
            deny: Permissions::empty(),
        };
        let mut list = vec![ow(1, Permissions::VIEW_CHANNEL)];
        apply_overwrite(&mut list, ow(1, Permissions::SEND_MESSAGES));
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].allow, Permissions::SEND_MESSAGES);
        apply_overwrite(&mut list, ow(2, Permissions::VIEW_CHANNEL));
        assert_eq!(list.len(), 2);
        apply_overwrite(&mut list, ow(1, Permissions::empty()));
        assert_eq!(list.len(), 1);
        let cat = vec![ow(2, Permissions::VIEW_CHANNEL)];
        assert!(is_synced(&list, &cat));
        assert!(!is_synced(&[], &cat));
        assert_eq!(synced_overwrites(&cat), cat);
    }
}
