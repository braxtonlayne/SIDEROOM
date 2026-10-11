//! Server templates (LNG-SSET-070/071): snapshot roles, channels, overwrites and
//! settings; detect drift (Sync); instantiate a snapshot into fresh entities.
//!
//! Snapshots use small local ids (`@everyone` is role 0) so they can be applied to
//! any guild. Member-specific overwrites and managed roles are not captured.

use serde::{Deserialize, Serialize};
use sr_core::permissions::{Overwrite, OverwriteKind, Permissions};
use sr_core::{ChannelId, GuildId, RoleId, Snowflake, UserId};

use crate::domain::{Channel, ChannelType, ChannelWithOverwrites, Role};

use super::error::{SettingsError, SettingsResult};

pub const MAX_NAME_CHARS: usize = 100;
pub const MAX_DESCRIPTION_CHARS: usize = 120;
pub const CODE_LEN: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapRole {
    /// 0 is `@everyone`.
    pub local_id: u32,
    pub name: String,
    pub color: u32,
    pub hoist: bool,
    pub position: i32,
    pub permissions: u64,
    pub mentionable: bool,
    pub unicode_emoji: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapOverwrite {
    pub role_local_id: u32,
    pub allow: u64,
    pub deny: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapChannel {
    pub local_id: u32,
    pub kind: i16,
    pub name: String,
    pub topic: Option<String>,
    pub position: i32,
    pub parent_local_id: Option<u32>,
    pub nsfw: bool,
    pub rate_limit_per_user: u32,
    pub bitrate: Option<u32>,
    pub user_limit: Option<u32>,
    pub overwrites: Vec<SnapOverwrite>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapSettings {
    pub preferred_locale: String,
    pub verification_level: u8,
    pub default_message_notifications: u8,
    pub explicit_content_filter: u8,
    pub system_channel_local_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateSnapshot {
    pub settings: SnapSettings,
    pub roles: Vec<SnapRole>,
    pub channels: Vec<SnapChannel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuildTemplate {
    pub code: String,
    pub guild_id: GuildId,
    pub creator_id: UserId,
    pub name: String,
    pub description: Option<String>,
    pub usage_count: u32,
    pub snapshot: TemplateSnapshot,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

pub fn validate_template_text(name: &str, description: Option<&str>) -> SettingsResult<()> {
    if !(1..=MAX_NAME_CHARS).contains(&name.trim().chars().count()) {
        return Err(SettingsError::invalid(
            "name",
            "BASE_TYPE_BAD_LENGTH",
            "must be between 1 and 100 in length",
        ));
    }
    if description.is_some_and(|d| d.chars().count() > MAX_DESCRIPTION_CHARS) {
        return Err(SettingsError::invalid(
            "description",
            "BASE_TYPE_MAX_LENGTH",
            "must be 120 or fewer in length",
        ));
    }
    Ok(())
}

/// Capture the current guild structure. `guild_settings` come from the guild row.
pub fn snapshot_guild(
    settings: SnapSettings,
    roles: &[Role],
    channels: &[ChannelWithOverwrites],
) -> TemplateSnapshot {
    let mut role_local: std::collections::HashMap<RoleId, u32> = std::collections::HashMap::new();
    let mut snap_roles = Vec::new();
    let mut next = 1u32;
    for r in roles.iter().filter(|r| r.is_everyone() || !r.managed) {
        let local = if r.is_everyone() {
            0
        } else {
            next += 1;
            next - 1
        };
        role_local.insert(r.id, local);
        snap_roles.push(SnapRole {
            local_id: local,
            name: r.name.clone(),
            color: r.color,
            hoist: r.hoist,
            position: r.position,
            permissions: r.permissions.bits(),
            mentionable: r.mentionable,
            unicode_emoji: r.unicode_emoji.clone(),
        });
    }
    snap_roles.sort_by_key(|r| r.local_id);

    let kept: Vec<&ChannelWithOverwrites> = channels
        .iter()
        .filter(|c| !c.channel.kind.is_dm() && !c.channel.kind.is_thread())
        .collect();
    let chan_local: std::collections::HashMap<ChannelId, u32> = kept
        .iter()
        .enumerate()
        .map(|(i, c)| (c.channel.id, i as u32 + 1))
        .collect();
    let snap_channels = kept
        .iter()
        .map(|c| SnapChannel {
            local_id: chan_local[&c.channel.id],
            kind: c.channel.kind.code(),
            name: c.channel.name.clone().unwrap_or_default(),
            topic: c.channel.topic.clone(),
            position: c.channel.position,
            parent_local_id: c
                .channel
                .parent_id
                .and_then(|p| chan_local.get(&p).copied()),
            nsfw: c.channel.nsfw,
            rate_limit_per_user: c.channel.rate_limit_per_user,
            bitrate: c.channel.bitrate,
            user_limit: c.channel.user_limit,
            overwrites: c
                .overwrites
                .iter()
                .filter(|o| o.kind == OverwriteKind::Role)
                .filter_map(|o| {
                    role_local
                        .get(&RoleId::new(o.id.get()))
                        .map(|l| SnapOverwrite {
                            role_local_id: *l,
                            allow: o.allow.bits(),
                            deny: o.deny.bits(),
                        })
                })
                .collect(),
        })
        .collect();
    TemplateSnapshot {
        settings,
        roles: snap_roles,
        channels: snap_channels,
    }
}

impl GuildTemplate {
    pub fn new(
        code: String,
        guild_id: GuildId,
        creator_id: UserId,
        name: &str,
        description: Option<&str>,
        snapshot: TemplateSnapshot,
        now_ms: i64,
    ) -> SettingsResult<Self> {
        validate_template_text(name, description)?;
        Ok(Self {
            code,
            guild_id,
            creator_id,
            name: name.trim().to_owned(),
            description: description.map(str::to_owned).filter(|d| !d.is_empty()),
            usage_count: 0,
            snapshot,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
        })
    }

    /// The template differs from the live guild; the UI offers "Sync".
    pub fn is_dirty(&self, current: &TemplateSnapshot) -> bool {
        &self.snapshot != current
    }

    pub fn sync(&mut self, current: TemplateSnapshot, now_ms: i64) {
        self.snapshot = current;
        self.updated_at_ms = now_ms;
    }
}

/// Entities to create when a template is applied to a new guild.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateInstance {
    pub roles: Vec<Role>,
    pub channels: Vec<ChannelWithOverwrites>,
    pub system_channel_id: Option<ChannelId>,
}

/// Turn a snapshot into new roles and channels for `guild_id`, drawing ids from
/// `next_id`. `@everyone` takes the guild id. Unknown channel kinds are skipped.
pub fn instantiate(
    snapshot: &TemplateSnapshot,
    guild_id: GuildId,
    mut next_id: impl FnMut() -> u64,
) -> TemplateInstance {
    let mut role_ids = std::collections::HashMap::new();
    let mut roles = Vec::new();
    for r in &snapshot.roles {
        let id = if r.local_id == 0 {
            RoleId::new(guild_id.get())
        } else {
            RoleId::new(next_id())
        };
        role_ids.insert(r.local_id, id);
        roles.push(Role {
            id,
            guild_id,
            name: r.name.clone(),
            color: r.color,
            hoist: r.hoist,
            position: r.position,
            permissions: Permissions::from_bits_truncate(r.permissions),
            mentionable: r.mentionable,
            managed: false,
            unicode_emoji: r.unicode_emoji.clone(),
            icon_key: None,
        });
    }
    let mut chan_ids = std::collections::HashMap::new();
    for c in &snapshot.channels {
        chan_ids.insert(c.local_id, ChannelId::new(next_id()));
    }
    let mut channels = Vec::new();
    for c in &snapshot.channels {
        let Some(kind) = ChannelType::from_code(c.kind) else {
            continue;
        };
        let id = chan_ids[&c.local_id];
        let mut ch = Channel::new(id, Some(guild_id), kind);
        ch.name = Some(c.name.clone());
        ch.topic = c.topic.clone();
        ch.position = c.position;
        ch.parent_id = c.parent_local_id.and_then(|p| chan_ids.get(&p).copied());
        ch.nsfw = c.nsfw;
        ch.rate_limit_per_user = c.rate_limit_per_user;
        ch.bitrate = c.bitrate;
        ch.user_limit = c.user_limit;
        let overwrites = c
            .overwrites
            .iter()
            .filter_map(|o| {
                role_ids.get(&o.role_local_id).map(|r| Overwrite {
                    id: Snowflake::new(r.get()),
                    kind: OverwriteKind::Role,
                    allow: Permissions::from_bits_truncate(o.allow),
                    deny: Permissions::from_bits_truncate(o.deny),
                })
            })
            .collect();
        channels.push(ChannelWithOverwrites {
            channel: ch,
            overwrites,
        });
    }
    let system_channel_id = snapshot
        .settings
        .system_channel_local_id
        .and_then(|l| chan_ids.get(&l).copied());
    TemplateInstance {
        roles,
        channels,
        system_channel_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn role(id: u64, guild: u64, name: &str, perms: Permissions, managed: bool) -> Role {
        Role {
            id: RoleId::new(id),
            guild_id: GuildId::new(guild),
            name: name.into(),
            color: 0,
            hoist: false,
            position: 0,
            permissions: perms,
            mentionable: false,
            managed,
            unicode_emoji: None,
            icon_key: None,
        }
    }

    fn settings() -> SnapSettings {
        SnapSettings {
            preferred_locale: "en-US".into(),
            verification_level: 1,
            default_message_notifications: 0,
            explicit_content_filter: 2,
            system_channel_local_id: Some(2),
        }
    }

    fn fixture() -> (Vec<Role>, Vec<ChannelWithOverwrites>) {
        let roles = vec![
            role(1, 1, "@everyone", Permissions::VIEW_CHANNEL, false),
            role(50, 1, "Mods", Permissions::KICK_MEMBERS, false),
            role(51, 1, "Some Bot", Permissions::empty(), true),
        ];
        let mut cat = Channel::new(
            ChannelId::new(100),
            Some(GuildId::new(1)),
            ChannelType::Category,
        );
        cat.name = Some("Text".into());
        let mut chat = Channel::new(
            ChannelId::new(101),
            Some(GuildId::new(1)),
            ChannelType::Text,
        );
        chat.name = Some("general".into());
        chat.parent_id = Some(cat.id);
        let ow = vec![
            Overwrite {
                id: Snowflake::new(50),
                kind: OverwriteKind::Role,
                allow: Permissions::SEND_MESSAGES,
                deny: Permissions::empty(),
            },
            Overwrite {
                id: Snowflake::new(777),
                kind: OverwriteKind::Member,
                allow: Permissions::SEND_MESSAGES,
                deny: Permissions::empty(),
            },
        ];
        let thread = Channel::new(
            ChannelId::new(102),
            Some(GuildId::new(1)),
            ChannelType::PublicThread,
        );
        (
            roles,
            vec![
                ChannelWithOverwrites {
                    channel: cat,
                    overwrites: vec![],
                },
                ChannelWithOverwrites {
                    channel: chat,
                    overwrites: ow,
                },
                ChannelWithOverwrites {
                    channel: thread,
                    overwrites: vec![],
                },
            ],
        )
    }

    #[test]
    fn snapshot_drops_managed_member_overwrites_and_threads() {
        let (roles, channels) = fixture();
        let s = snapshot_guild(settings(), &roles, &channels);
        assert_eq!(s.roles.len(), 2);
        assert_eq!(s.roles[0].local_id, 0);
        assert_eq!(s.roles[1].name, "Mods");
        assert_eq!(s.channels.len(), 2);
        assert_eq!(s.channels[1].parent_local_id, Some(1));
        assert_eq!(s.channels[1].overwrites.len(), 1);
        assert_eq!(s.channels[1].overwrites[0].role_local_id, 1);
    }

    #[test]
    fn dirty_and_sync() {
        let (roles, channels) = fixture();
        let snap = snapshot_guild(settings(), &roles, &channels);
        let mut t = GuildTemplate::new(
            "tmpl".into(),
            GuildId::new(1),
            UserId::new(2),
            "My template",
            None,
            snap.clone(),
            10,
        )
        .unwrap();
        assert!(!t.is_dirty(&snap));
        let mut changed = snap.clone();
        changed.channels.pop();
        assert!(t.is_dirty(&changed));
        t.sync(changed.clone(), 20);
        assert!(!t.is_dirty(&changed));
        assert_eq!(t.updated_at_ms, 20);
        assert!(
            GuildTemplate::new(
                "c".into(),
                GuildId::new(1),
                UserId::new(2),
                "",
                None,
                snap.clone(),
                0
            )
            .is_err()
        );
        assert!(
            GuildTemplate::new(
                "c".into(),
                GuildId::new(1),
                UserId::new(2),
                "n",
                Some(&"d".repeat(121)),
                snap,
                0
            )
            .is_err()
        );
    }

    #[test]
    fn instantiate_remaps_ids() {
        let (roles, channels) = fixture();
        let snap = snapshot_guild(settings(), &roles, &channels);
        let mut n = 1000;
        let inst = instantiate(&snap, GuildId::new(9), || {
            n += 1;
            n
        });
        assert_eq!(inst.roles.len(), 2);
        assert!(inst.roles[0].is_everyone());
        assert_eq!(inst.roles[0].id.get(), 9);
        let mods = &inst.roles[1];
        assert!(mods.id.get() > 1000);
        assert_eq!(inst.channels.len(), 2);
        let (cat, chat) = (&inst.channels[0].channel, &inst.channels[1]);
        assert_eq!(chat.channel.parent_id, Some(cat.id));
        assert_eq!(chat.overwrites[0].id.get(), mods.id.get());
        assert_eq!(chat.channel.guild_id, Some(GuildId::new(9)));
        assert_eq!(inst.system_channel_id, Some(chat.channel.id));
    }
}
