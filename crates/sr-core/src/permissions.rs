//! Guild permission bitflags and computation (FND-CORE-003). wasm-compatible.
//!
//! Bits 0-50 are Discord's, with identical semantics. SIDE ROOM adds bits 56-59
//! (doc 07 section 2). Serialized as decimal strings, like Discord.

use crate::snowflake::Snowflake;
use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct Permissions: u64 {
        const CREATE_INSTANT_INVITE = 1 << 0;
        const KICK_MEMBERS = 1 << 1;
        const BAN_MEMBERS = 1 << 2;
        const ADMINISTRATOR = 1 << 3;
        const MANAGE_CHANNELS = 1 << 4;
        const MANAGE_GUILD = 1 << 5;
        const ADD_REACTIONS = 1 << 6;
        const VIEW_AUDIT_LOG = 1 << 7;
        const PRIORITY_SPEAKER = 1 << 8;
        const STREAM = 1 << 9;
        const VIEW_CHANNEL = 1 << 10;
        const SEND_MESSAGES = 1 << 11;
        const SEND_TTS_MESSAGES = 1 << 12;
        const MANAGE_MESSAGES = 1 << 13;
        const EMBED_LINKS = 1 << 14;
        const ATTACH_FILES = 1 << 15;
        const READ_MESSAGE_HISTORY = 1 << 16;
        const MENTION_EVERYONE = 1 << 17;
        const USE_EXTERNAL_EMOJIS = 1 << 18;
        const VIEW_GUILD_INSIGHTS = 1 << 19;
        const CONNECT = 1 << 20;
        const SPEAK = 1 << 21;
        const MUTE_MEMBERS = 1 << 22;
        const DEAFEN_MEMBERS = 1 << 23;
        const MOVE_MEMBERS = 1 << 24;
        const USE_VAD = 1 << 25;
        const CHANGE_NICKNAME = 1 << 26;
        const MANAGE_NICKNAMES = 1 << 27;
        const MANAGE_ROLES = 1 << 28;
        const MANAGE_WEBHOOKS = 1 << 29;
        const MANAGE_GUILD_EXPRESSIONS = 1 << 30;
        const USE_APPLICATION_COMMANDS = 1 << 31;
        const REQUEST_TO_SPEAK = 1 << 32;
        const MANAGE_EVENTS = 1 << 33;
        const MANAGE_THREADS = 1 << 34;
        const CREATE_PUBLIC_THREADS = 1 << 35;
        const CREATE_PRIVATE_THREADS = 1 << 36;
        const USE_EXTERNAL_STICKERS = 1 << 37;
        const SEND_MESSAGES_IN_THREADS = 1 << 38;
        const USE_EMBEDDED_ACTIVITIES = 1 << 39;
        const MODERATE_MEMBERS = 1 << 40;
        const VIEW_CREATOR_MONETIZATION_ANALYTICS = 1 << 41;
        const USE_SOUNDBOARD = 1 << 42;
        const CREATE_GUILD_EXPRESSIONS = 1 << 43;
        const CREATE_EVENTS = 1 << 44;
        const USE_EXTERNAL_SOUNDS = 1 << 45;
        const SEND_VOICE_MESSAGES = 1 << 46;
        // Bits 47-48 are unassigned in Discord's current list.
        const SEND_POLLS = 1 << 49;
        const USE_EXTERNAL_APPS = 1 << 50;

        // SIDE ROOM additions (doc 07 section 2).
        const START_WATCH_PARTY = 1 << 56;
        const CONTROL_WATCH_PARTY = 1 << 57;
        const USE_LIBRARY_EMBEDS = 1 << 58;
        const MANAGE_EXTENSIONS = 1 << 59;
    }
}

impl Permissions {
    /// Every defined permission (what owners and administrators get).
    pub const ALL: Self = Self::all();

    /// Permission check that honours `ADMINISTRATOR` (use on computed results too).
    pub fn has(self, perm: Self) -> bool {
        self.contains(Self::ADMINISTRATOR) || self.contains(perm)
    }
}

impl serde::Serialize for Permissions {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(&self.bits())
    }
}

impl<'de> serde::Deserialize<'de> for Permissions {
    /// Accepts a decimal string (canonical) or integer. Unknown bits are dropped.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self::from_bits_truncate(Snowflake::deserialize(d)?.get()))
    }
}

/// Whether an overwrite targets a role or a member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OverwriteKind {
    Role,
    Member,
}

/// A channel permission overwrite. `id` is a role id or user id depending on `kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Overwrite {
    pub id: Snowflake,
    pub kind: OverwriteKind,
    pub allow: Permissions,
    pub deny: Permissions,
}

/// Guild-level permissions: owner bypass, `@everyone` | role permissions,
/// `ADMINISTRATOR` bypass.
pub fn compute_base(
    is_owner: bool,
    everyone_perms: Permissions,
    role_perms: impl IntoIterator<Item = Permissions>,
) -> Permissions {
    if is_owner {
        return Permissions::ALL;
    }
    let perms = role_perms
        .into_iter()
        .fold(everyone_perms, |acc, p| acc | p);
    if perms.contains(Permissions::ADMINISTRATOR) {
        Permissions::ALL
    } else {
        perms
    }
}

/// Apply channel overwrites to `base` (Discord algorithm):
/// 1. administrators bypass; 2. `@everyone` overwrite (id == guild id);
/// 3. all of the member's role overwrites, deny then allow (aggregated);
/// 4. the member's own overwrite, deny then allow.
///
/// `member_role_ids` must not include the `@everyone` role (its id is `guild_id`).
pub fn compute_overwrites(
    base: Permissions,
    guild_id: Snowflake,
    member_id: Snowflake,
    member_role_ids: &[Snowflake],
    overwrites: &[Overwrite],
) -> Permissions {
    if base.contains(Permissions::ADMINISTRATOR) {
        return Permissions::ALL;
    }
    let mut perms = base;
    if let Some(ow) = overwrites
        .iter()
        .find(|o| o.kind == OverwriteKind::Role && o.id == guild_id)
    {
        perms = (perms & !ow.deny) | ow.allow;
    }
    let (mut allow, mut deny) = (Permissions::empty(), Permissions::empty());
    for ow in overwrites.iter().filter(|o| {
        o.kind == OverwriteKind::Role && o.id != guild_id && member_role_ids.contains(&o.id)
    }) {
        allow |= ow.allow;
        deny |= ow.deny;
    }
    perms = (perms & !deny) | allow;
    if let Some(ow) = overwrites
        .iter()
        .find(|o| o.kind == OverwriteKind::Member && o.id == member_id)
    {
        perms = (perms & !ow.deny) | ow.allow;
    }
    perms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discord_bit_positions() {
        assert_eq!(Permissions::VIEW_CHANNEL.bits(), 1 << 10);
        assert_eq!(Permissions::SEND_MESSAGES.bits(), 1 << 11);
        assert_eq!(Permissions::ADMINISTRATOR.bits(), 8);
        assert_eq!(Permissions::USE_EXTERNAL_APPS.bits(), 1 << 50);
        assert_eq!(Permissions::START_WATCH_PARTY.bits(), 1 << 56);
        assert_eq!(Permissions::MANAGE_EXTENSIONS.bits(), 1 << 59);
    }

    #[test]
    fn serde_string_and_truncate() {
        let p = Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES;
        assert_eq!(serde_json::to_string(&p).unwrap(), "\"3072\"");
        assert_eq!(serde_json::from_str::<Permissions>("\"3072\"").unwrap(), p);
        assert_eq!(serde_json::from_str::<Permissions>("3072").unwrap(), p);
        // bit 63 is unknown here and must be dropped
        let all = serde_json::from_str::<Permissions>("\"9223372036854775808\"").unwrap();
        assert!(all.is_empty());
    }

    #[test]
    fn admin_and_owner_bypass() {
        let admin = compute_base(false, Permissions::empty(), [Permissions::ADMINISTRATOR]);
        assert_eq!(admin, Permissions::ALL);
        assert_eq!(
            compute_base(true, Permissions::empty(), []),
            Permissions::ALL
        );
    }
}
