//! Typed ID newtypes over [`Snowflake`] (FND-CORE-005).
//!
//! Each newtype serializes exactly like a Snowflake (a JSON string).

use crate::snowflake::Snowflake;

macro_rules! id_types {
    ($($(#[$m:meta])* $name:ident),* $(,)?) => {$(
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default,
                 serde::Serialize, serde::Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Snowflake);

        impl $name {
            pub const fn new(raw: u64) -> Self { Self(Snowflake::new(raw)) }
            pub const fn get(self) -> u64 { self.0.get() }
            pub const fn as_i64(self) -> i64 { self.0.as_i64() }
            pub const fn from_i64(v: i64) -> Self { Self(Snowflake::from_i64(v)) }
        }

        impl From<Snowflake> for $name {
            fn from(s: Snowflake) -> Self { Self(s) }
        }

        impl From<$name> for Snowflake {
            fn from(id: $name) -> Self { id.0 }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { self.0.fmt(f) }
        }

        impl std::str::FromStr for $name {
            type Err = crate::snowflake::ParseSnowflakeError;
            fn from_str(s: &str) -> Result<Self, Self::Err> { s.parse().map(Self) }
        }
    )*};
}

id_types!(
    /// An account, exposed as `user.id` in the API.
    UserId,
    /// Staff-only record of a human (D-030).
    PersonId,
    GuildId,
    ChannelId,
    MessageId,
    RoleId,
    EmojiId,
    StickerId,
    AttachmentId,
    WebhookId,
    ApplicationId,
    PlatformInviteId,
    SessionId,
    JobId,
    SubmissionId,
    InvoiceId,
);

/// The system user "SIDE ROOM" (docs/06 §8).
pub const SYSTEM_USER_ID: UserId = UserId::new(1);

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn ids_serialize_as_strings() {
        let id = GuildId::new(42);
        assert_eq!(serde_json::to_string(&id).unwrap(), "\"42\"");
        assert_eq!(serde_json::from_str::<GuildId>("\"42\"").unwrap(), id);
        assert_eq!("42".parse::<GuildId>().unwrap(), id);
    }
}
