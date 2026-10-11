//! TODO(FND-CORE): placeholder ID newtypes. Replace with the snowflake types
//! from `sr-core` once that crate lands, and re-export them from here.

use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u64);
    };
}

id_type!(
    /// Placeholder guild (server) id.
    GuildId
);
id_type!(
    /// Placeholder channel id.
    ChannelId
);
id_type!(
    /// Placeholder user id.
    UserId
);
