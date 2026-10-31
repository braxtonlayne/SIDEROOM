//! `bridgeable`: the one predicate both modes use to decide whether a SIDE ROOM channel may be mirrored at all
//! (design section 9.1), and how: with messages, or as structure only.

/// What is mirrored of an eligible channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mirrored {
    /// Structure and messages (text, announcement, forum and media channels, threads).
    Messages,
    /// Structure only: category, voice, stage. No audio ever (voice is end-to-end encrypted) and no text-in-voice.
    StructureOnly,
}

/// Why a channel is not mirrored. Shown in the plan's "Not mirrored" tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// A direct message or group chat.
    DirectMessage,
    /// The channel is (or was flagged) end-to-end encrypted.
    Encrypted,
    /// The staff server never enables the bridge.
    StaffServer,
    /// A type that has no counterpart (Discord directory channels and anything unknown).
    UnsupportedType,
}

impl Refusal {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::DirectMessage => {
                "Direct messages and group chats are never bridged: they are private."
            }
            Self::Encrypted => {
                "End-to-end encrypted conversations cannot be read by the server and are never bridged."
            }
            Self::StaffServer => "The staff server cannot be bridged.",
            Self::UnsupportedType => "This channel type has no Discord counterpart.",
        }
    }
}

/// Whether a channel may be mirrored. `guild_id` is the channel's server (`None` for a DM), `kind` the channel
/// type (Discord enum), `e2ee` whether the channel is flagged end-to-end encrypted (there are no E2EE guild text
/// channels today; this is where one would be excluded), `staff_guild` whether its server is the staff server.
pub fn bridgeable(
    guild_id: Option<i64>,
    kind: i16,
    e2ee: bool,
    staff_guild: bool,
) -> Result<Mirrored, Refusal> {
    if guild_id.is_none() || matches!(kind, 1 | 3) {
        return Err(Refusal::DirectMessage);
    }
    if e2ee {
        return Err(Refusal::Encrypted);
    }
    if staff_guild {
        return Err(Refusal::StaffServer);
    }
    match kind {
        0 | 5 | 10 | 11 | 12 | 15 | 16 => Ok(Mirrored::Messages),
        2 | 4 | 13 => Ok(Mirrored::StructureOnly),
        _ => Err(Refusal::UnsupportedType),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_one_predicate() {
        for k in [0, 5, 10, 11, 12, 15, 16] {
            assert_eq!(bridgeable(Some(1), k, false, false), Ok(Mirrored::Messages));
        }
        for k in [2, 4, 13] {
            assert_eq!(
                bridgeable(Some(1), k, false, false),
                Ok(Mirrored::StructureOnly)
            );
        }
        assert_eq!(
            bridgeable(None, 1, false, false),
            Err(Refusal::DirectMessage)
        );
        assert_eq!(
            bridgeable(Some(1), 3, false, false),
            Err(Refusal::DirectMessage)
        );
        assert_eq!(bridgeable(Some(1), 0, true, false), Err(Refusal::Encrypted));
        assert_eq!(
            bridgeable(Some(1), 0, false, true),
            Err(Refusal::StaffServer)
        );
        assert_eq!(
            bridgeable(Some(1), 14, false, false),
            Err(Refusal::UnsupportedType)
        );
        assert!(Refusal::Encrypted.reason().contains("encrypted"));
    }
}
