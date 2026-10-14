//! Typed domain IDs (FND-CORE-005). Each wraps a [`Snowflake`] and serializes as a string.

use crate::snowflake::Snowflake;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

macro_rules! id_types {
    ($($(#[$m:meta])* $name:ident),* $(,)?) => {$(
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Snowflake);

        impl $name {
            pub const fn new(raw: u64) -> Self { Self(Snowflake::new(raw)) }
            pub const fn get(self) -> u64 { self.0.get() }
            pub const fn snowflake(self) -> Snowflake { self.0 }
        }
        impl From<Snowflake> for $name { fn from(s: Snowflake) -> Self { Self(s) } }
        impl From<u64> for $name { fn from(v: u64) -> Self { Self::new(v) } }
        impl From<$name> for Snowflake { fn from(v: $name) -> Self { v.0 } }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { self.0.fmt(f) }
        }
        impl FromStr for $name {
            type Err = std::num::ParseIntError;
            fn from_str(s: &str) -> Result<Self, Self::Err> { s.parse().map(Self) }
        }
    )*};
}

id_types!(
    UserId,
    /// Person: the identity behind one or more accounts (doc 06).
    PersonId,
    AccountId,
    GuildId,
    ChannelId,
    MessageId,
    RoleId,
    EmojiId,
    StickerId,
    SoundId,
    WebhookId,
    ApplicationId,
    AttachmentId,
    InviteId,
    SessionId,
    MediaId,
    LibraryItemId,
    SubmissionId,
    TvChannelId,
    PartyId,
    ThreadId,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_and_parse() {
        let g = GuildId::new(123);
        assert_eq!(serde_json::to_string(&g).unwrap(), "\"123\"");
        assert_eq!(serde_json::from_str::<GuildId>("\"123\"").unwrap(), g);
        assert_eq!("123".parse::<GuildId>().unwrap(), g);
        assert_eq!(g.to_string(), "123");
    }
}
