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
        // Bit 47 is unassigned in Discord's current list.
        const SET_VOICE_CHANNEL_STATUS = 1 << 48;
        const SEND_POLLS = 1 << 49;
        const USE_EXTERNAL_APPS = 1 << 50;
        const PIN_MESSAGES = 1 << 51;
        const BYPASS_SLOWMODE = 1 << 52;

        // SIDE ROOM additions (doc 07 section 2).
        const START_WATCH_PARTY = 1 << 56;
        const CONTROL_WATCH_PARTY = 1 << 57;
        const USE_LIBRARY_EMBEDS = 1 << 58;
        const MANAGE_EXTENSIONS = 1 << 59;
        /// Use this server's emoji, stickers and sounds in other servers and DMs (D-376).
        /// A source-server bit: evaluated with guild permissions, never with overwrites.
        const USE_EXPRESSIONS_EXTERNALLY = 1 << 60;
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

/// The Discord bridge's classification of the permission word (BRG-SYN-002, doc 07 section 2, wave 35b design
/// section 4). SIDE ROOM is the master copy of permissions: every bit is exactly one of
///
/// * **shared** ([`Permissions::BRIDGE_SHARED`]): bits 0 to 46 and 48 to 52, the bits Discord defines and SIDE
///   ROOM mirrors. Sent to Discord, SIDE ROOM-authoritative;
/// * **SIDE ROOM-only** ([`Permissions::SR_ONLY`]): bits 56 to 62 (as far as they are defined). They stay on the
///   SIDE ROOM role and overwrite, are never sent, never cleared by a sync, and can never be granted from Discord;
/// * **Discord-only** ([`DISCORD_ONLY_MASK`]): bits 47 and 53 to 55. They never enter a [`Permissions`] value
///   (the deserializer truncates them); the bridge keeps them raw and preserves them on Discord.
///
/// Bit 63 is unusable and belongs to nobody. `unclassified_bits_are_none` (below) pins that a bit added to
/// [`Permissions`] later fails the build until it is classified here.
impl Permissions {
    /// Bits 0 to 46 and 48 to 52 (every bit SIDE ROOM defines below 56).
    pub const BRIDGE_SHARED: Self = Self::from_bits_truncate(SHARED_RAW);
    /// The defined SIDE ROOM-only bits (56 to 60 now; 61 and 62 join when they are assigned, O-1).
    pub const SR_ONLY: Self = Self::from_bits_truncate(SR_RESERVED_RAW);
}

const SHARED_RAW: u64 = 0x001F_7FFF_FFFF_FFFF;
/// Bits 56 to 62: reserved for SIDE ROOM whether or not they are assigned yet.
pub const SR_RESERVED_RAW: u64 = 0x7F00_0000_0000_0000;

/// The bits only Discord defines (47 and 53 to 55), raw. On Discord they are Discord's alone.
pub const DISCORD_ONLY_MASK: u64 = (1 << 47) | (0b111 << 53);

/// A raw permission word as Discord sends it, split by who owns each bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClassifiedBits {
    /// The shared bits, as a [`Permissions`] value.
    pub shared: Permissions,
    /// The bits only Discord defines, raw.
    pub discord_only: u64,
}

impl ClassifiedBits {
    /// Split what Discord sent. SIDE ROOM-only bits in `raw` (Discord has none; a hostile or buggy payload may
    /// carry them) are dropped here: they can never be granted from Discord.
    pub const fn from_discord(raw: u64) -> Self {
        Self {
            shared: Permissions::from_bits_truncate(raw & SHARED_RAW),
            discord_only: raw & DISCORD_ONLY_MASK,
        }
    }

    /// The word to send to Discord for a SIDE ROOM word: its shared bits, plus the Discord-only bits to
    /// preserve. SIDE ROOM-only bits never appear.
    pub const fn to_discord(sr: Permissions, preserved_discord_only: u64) -> u64 {
        (sr.bits() & SHARED_RAW) | (preserved_discord_only & DISCORD_ONLY_MASK)
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

/// Whether a member is in a timeout at `now_ms`
/// (`communication_disabled_until` is unix milliseconds, as in the time module).
pub fn is_timed_out(communication_disabled_until_ms: Option<i64>, now_ms: i64) -> bool {
    communication_disabled_until_ms.is_some_and(|until| until > now_ms)
}

/// Inputs to [`compute`]. Slices are borrowed to keep the call cheap in WASM.
#[derive(Debug, Clone, Copy)]
pub struct ComputeInput<'a> {
    pub is_owner: bool,
    /// The `@everyone` role permissions (role id == guild id).
    pub everyone: Permissions,
    pub role_perms: &'a [Permissions],
    pub guild_id: Snowflake,
    pub member_id: Snowflake,
    /// Member roles, excluding `@everyone`.
    pub member_role_ids: &'a [Snowflake],
    pub overwrites: &'a [Overwrite],
    /// Result of [`is_timed_out`] for the member.
    pub timed_out: bool,
}

/// Full channel permission computation: Discord's base and overwrite algorithm
/// ([`compute_base`], [`compute_overwrites`]) followed by Discord's implicit rules:
/// 1. a timed-out member keeps only `VIEW_CHANNEL | READ_MESSAGE_HISTORY`;
/// 2. without `VIEW_CHANNEL` the channel set is empty;
/// 3. without `SEND_MESSAGES`, `SEND_TTS_MESSAGES`, `MENTION_EVERYONE`, `EMBED_LINKS`
///    and `ATTACH_FILES` are removed;
/// 4. without `CONNECT`, the voice bits (`SPEAK`, `STREAM`, `USE_VAD`,
///    `PRIORITY_SPEAKER`) are removed.
///
/// Administrators (and owners) bypass every rule.
pub fn compute(input: &ComputeInput<'_>) -> Permissions {
    let base = compute_base(
        input.is_owner,
        input.everyone,
        input.role_perms.iter().copied(),
    );
    let mut perms = compute_overwrites(
        base,
        input.guild_id,
        input.member_id,
        input.member_role_ids,
        input.overwrites,
    );
    if perms.contains(Permissions::ADMINISTRATOR) {
        return Permissions::ALL;
    }
    if input.timed_out {
        perms &= Permissions::VIEW_CHANNEL | Permissions::READ_MESSAGE_HISTORY;
    }
    if !perms.contains(Permissions::VIEW_CHANNEL) {
        return Permissions::empty();
    }
    if !perms.contains(Permissions::SEND_MESSAGES) {
        perms &= !(Permissions::SEND_TTS_MESSAGES
            | Permissions::MENTION_EVERYONE
            | Permissions::EMBED_LINKS
            | Permissions::ATTACH_FILES);
    }
    if !perms.contains(Permissions::CONNECT) {
        perms &= !(Permissions::SPEAK
            | Permissions::STREAM
            | Permissions::USE_VAD
            | Permissions::PRIORITY_SPEAKER);
    }
    perms
}

#[cfg(test)]
mod bridge_mask_tests {
    use super::*;

    #[test]
    fn unclassified_bits_are_none() {
        // A bit added to `Permissions` must be classified: shared or SIDE ROOM-only, never neither.
        assert_eq!(
            Permissions::BRIDGE_SHARED | Permissions::SR_ONLY,
            Permissions::ALL,
            "a new permission bit is not classified for the Discord bridge: add it to BRIDGE_SHARED or SR_ONLY"
        );
        assert!((Permissions::BRIDGE_SHARED & Permissions::SR_ONLY).is_empty());
        assert_eq!(SHARED_RAW & SR_RESERVED_RAW, 0);
        assert_eq!(DISCORD_ONLY_MASK & (SHARED_RAW | SR_RESERVED_RAW), 0);
    }

    #[test]
    fn the_shared_mask_is_discords_bits() {
        for bit in (0..=46).chain(48..=52) {
            assert!(SHARED_RAW & (1 << bit) != 0, "bit {bit} is shared");
        }
        for bit in (53..=63).chain([47]) {
            assert!(SHARED_RAW & (1 << bit) == 0, "bit {bit} is not shared");
        }
        for bit in 56..=60 {
            assert!(Permissions::SR_ONLY.bits() & (1 << bit) != 0, "bit {bit}");
        }
        assert_eq!(Permissions::SR_ONLY.bits(), 0x1F00_0000_0000_0000);
    }

    #[test]
    fn every_defined_sideroom_bit_is_sr_only() {
        for p in [
            Permissions::START_WATCH_PARTY,
            Permissions::CONTROL_WATCH_PARTY,
            Permissions::USE_LIBRARY_EMBEDS,
            Permissions::MANAGE_EXTENSIONS,
            Permissions::USE_EXPRESSIONS_EXTERNALLY,
        ] {
            assert!(Permissions::SR_ONLY.contains(p));
            assert!(!Permissions::BRIDGE_SHARED.intersects(p));
        }
    }

    #[test]
    fn from_discord_never_yields_a_sideroom_only_bit() {
        let c = ClassifiedBits::from_discord(u64::MAX);
        assert!(!c.shared.intersects(Permissions::SR_ONLY));
        assert_eq!(c.shared, Permissions::BRIDGE_SHARED);
        assert_eq!(c.discord_only, DISCORD_ONLY_MASK);
        assert_eq!(
            ClassifiedBits::from_discord(SR_RESERVED_RAW | (1 << 63)),
            ClassifiedBits::default()
        );
    }

    #[test]
    fn to_discord_sends_shared_and_preserved_only() {
        let sr = Permissions::SEND_MESSAGES
            | Permissions::START_WATCH_PARTY
            | Permissions::MANAGE_EXTENSIONS;
        let out = ClassifiedBits::to_discord(sr, (1 << 47) | (1 << 57));
        assert_eq!(out, Permissions::SEND_MESSAGES.bits() | (1 << 47));
        let raw = Permissions::VIEW_CHANNEL.bits() | (1 << 47) | (1 << 54);
        let c = ClassifiedBits::from_discord(raw);
        assert_eq!(ClassifiedBits::to_discord(c.shared, c.discord_only), raw);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sf(n: u64) -> Snowflake {
        Snowflake::new(n)
    }

    /// Guild 1, member 2, everyone role id 1 (== guild id).
    fn input<'a>(
        everyone: Permissions,
        roles: &'a [Permissions],
        ows: &'a [Overwrite],
        timed_out: bool,
    ) -> ComputeInput<'a> {
        ComputeInput {
            is_owner: false,
            everyone,
            role_perms: roles,
            guild_id: sf(1),
            member_id: sf(2),
            member_role_ids: &[],
            overwrites: ows,
            timed_out,
        }
    }

    #[test]
    fn timeout_detection() {
        assert!(is_timed_out(Some(2_000), 1_000));
        assert!(!is_timed_out(Some(1_000), 1_000));
        assert!(!is_timed_out(None, 1_000));
    }

    #[test]
    fn timed_out_member_keeps_only_view_and_history() {
        let all_text = Permissions::VIEW_CHANNEL
            | Permissions::SEND_MESSAGES
            | Permissions::READ_MESSAGE_HISTORY
            | Permissions::ADD_REACTIONS;
        let out = compute(&input(all_text, &[], &[], true));
        assert_eq!(
            out,
            Permissions::VIEW_CHANNEL | Permissions::READ_MESSAGE_HISTORY
        );
    }

    #[test]
    fn timed_out_admin_bypasses() {
        let out = compute(&input(
            Permissions::empty(),
            &[Permissions::ADMINISTRATOR],
            &[],
            true,
        ));
        assert_eq!(out, Permissions::ALL);
    }

    #[test]
    fn no_view_channel_means_empty() {
        let out = compute(&input(Permissions::SEND_MESSAGES, &[], &[], false));
        assert!(out.is_empty());
    }

    #[test]
    fn no_send_strips_send_adjacent_bits() {
        let perms = Permissions::VIEW_CHANNEL
            | Permissions::SEND_TTS_MESSAGES
            | Permissions::MENTION_EVERYONE
            | Permissions::EMBED_LINKS
            | Permissions::ATTACH_FILES
            | Permissions::READ_MESSAGE_HISTORY;
        let out = compute(&input(perms, &[], &[], false));
        assert_eq!(
            out,
            Permissions::VIEW_CHANNEL | Permissions::READ_MESSAGE_HISTORY
        );
    }

    #[test]
    fn no_connect_strips_voice_bits() {
        let perms = Permissions::VIEW_CHANNEL
            | Permissions::SEND_MESSAGES
            | Permissions::SPEAK
            | Permissions::STREAM
            | Permissions::USE_VAD
            | Permissions::PRIORITY_SPEAKER;
        let out = compute(&input(perms, &[], &[], false));
        assert_eq!(out, Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES);
        let with_connect = compute(&input(perms | Permissions::CONNECT, &[], &[], false));
        assert!(with_connect.contains(Permissions::SPEAK));
    }

    #[test]
    fn overwrite_denying_view_empties_channel() {
        let ows = [Overwrite {
            id: sf(1),
            kind: OverwriteKind::Role,
            allow: Permissions::empty(),
            deny: Permissions::VIEW_CHANNEL,
        }];
        let base = Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES;
        assert!(compute(&input(base, &[], &ows, false)).is_empty());
    }

    #[test]
    fn new_bits_and_positions() {
        assert_eq!(Permissions::SET_VOICE_CHANNEL_STATUS.bits(), 1 << 48);
        assert_eq!(Permissions::PIN_MESSAGES.bits(), 1 << 51);
        assert_eq!(Permissions::BYPASS_SLOWMODE.bits(), 1 << 52);
    }

    #[test]
    fn discord_bit_positions() {
        assert_eq!(Permissions::VIEW_CHANNEL.bits(), 1 << 10);
        assert_eq!(Permissions::SEND_MESSAGES.bits(), 1 << 11);
        assert_eq!(Permissions::ADMINISTRATOR.bits(), 8);
        assert_eq!(Permissions::USE_EXTERNAL_APPS.bits(), 1 << 50);
        assert_eq!(Permissions::START_WATCH_PARTY.bits(), 1 << 56);
        assert_eq!(Permissions::MANAGE_EXTENSIONS.bits(), 1 << 59);
        assert_eq!(Permissions::USE_EXPRESSIONS_EXTERNALLY.bits(), 1 << 60);
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
