//! The rows of the bridge and the queries over them (migration 1720).
//!
//! Ids are Snowflakes held as `i64` in the database and as `i64` here, like the other feature crates. Secrets
//! (`bot_token_enc`, `webhook_token_enc`) never appear in these public types: they are read by the service
//! only, opened with the sealer and handed to the Discord client, never returned by a route nor logged.

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row, postgres::PgRow};

use crate::error::{BridgeError, BridgeResult};

/// Which way messages flow over one channel map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Both ways (the default).
    Both,
    /// SIDE ROOM to Discord only.
    ToDiscord,
    /// Discord to SIDE ROOM only.
    FromDiscord,
}

impl Direction {
    pub const ALL: [Direction; 3] = [Self::Both, Self::ToDiscord, Self::FromDiscord];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::ToDiscord => "to_discord",
            Self::FromDiscord => "from_discord",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.as_str() == s)
    }

    /// Messages written here are posted to Discord.
    pub const fn sends(self) -> bool {
        matches!(self, Self::Both | Self::ToDiscord)
    }

    /// Messages written on Discord appear here.
    pub const fn receives(self) -> bool {
        matches!(self, Self::Both | Self::FromDiscord)
    }
}

/// The connection state of a bridge, as the gateway half reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeStatus {
    Connecting,
    Connected,
    Disconnected,
    /// Discord refused the token (revoked or reset in the developer portal).
    TokenInvalid,
}

impl BridgeStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Connecting => "connecting",
            Self::Connected => "connected",
            Self::Disconnected => "disconnected",
            Self::TokenInvalid => "token_invalid",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [
            Self::Connecting,
            Self::Connected,
            Self::Disconnected,
            Self::TokenInvalid,
        ]
        .into_iter()
        .find(|x| x.as_str() == s)
    }
}

/// One server's bridge (without the sealed token).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Bridge {
    pub guild_id: i64,
    pub discord_guild_id: i64,
    pub discord_guild_name: String,
    pub bot_user_id: i64,
    pub bot_name: String,
    /// 'selective' (only the mapped channels); 'whole_guild' is reserved for wave 35b and refused for now.
    pub mode: String,
    pub status: BridgeStatus,
    pub intents_granted: i64,
    pub created_by: i64,
    pub created_at_ms: i64,
}

/// A SIDE ROOM channel mapped to a Discord channel (without the sealed webhook token).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChannelMap {
    pub id: i64,
    pub guild_id: i64,
    pub channel_id: i64,
    pub discord_channel_id: i64,
    pub discord_channel_name: String,
    /// The Discord webhook the bridge created for posting; `None` for a `from_discord` map.
    pub discord_webhook_id: Option<i64>,
    pub direction: Direction,
    pub created_by: i64,
    pub created_at_ms: i64,
}

/// Whether a mapped message started here (`Out`: mirrored to Discord) or there (`In`: a Discord message
/// shown here).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageMapDirection {
    Out,
    In,
}

impl MessageMapDirection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Out => "out",
            Self::In => "in",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "out" => Some(Self::Out),
            "in" => Some(Self::In),
            _ => None,
        }
    }
}

/// Our message id and theirs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MessageMap {
    pub sr_message_id: i64,
    pub discord_message_id: i64,
    pub map_id: i64,
    /// The webhook that posted the Discord side of an `Out` message.
    pub webhook_id: Option<i64>,
    /// The SIDE ROOM author of an `Out` message.
    pub author_id: Option<i64>,
    pub direction: MessageMapDirection,
}

pub(crate) const BRIDGE_COLS: &str = "guild_id, discord_guild_id, discord_guild_name, bot_user_id, bot_name, mode, status, \
     intents_granted, created_by, (extract(epoch from created_at) * 1000)::bigint AS created_ms";
pub(crate) const MAP_COLS: &str = "id, guild_id, channel_id, discord_channel_id, discord_channel_name, \
     discord_webhook_id, direction, created_by, (extract(epoch from created_at) * 1000)::bigint AS created_ms";

pub(crate) fn bridge_from(r: &PgRow) -> BridgeResult<Bridge> {
    Ok(Bridge {
        guild_id: r.try_get("guild_id")?,
        discord_guild_id: r.try_get("discord_guild_id")?,
        discord_guild_name: r.try_get("discord_guild_name")?,
        bot_user_id: r.try_get("bot_user_id")?,
        bot_name: r.try_get("bot_name")?,
        mode: r.try_get("mode")?,
        status: BridgeStatus::parse(&r.try_get::<String, _>("status")?)
            .ok_or_else(|| BridgeError::Storage("unknown bridge status".into()))?,
        intents_granted: r.try_get("intents_granted")?,
        created_by: r.try_get("created_by")?,
        created_at_ms: r.try_get("created_ms")?,
    })
}

pub(crate) fn map_from(r: &PgRow) -> BridgeResult<ChannelMap> {
    Ok(ChannelMap {
        id: r.try_get("id")?,
        guild_id: r.try_get("guild_id")?,
        channel_id: r.try_get("channel_id")?,
        discord_channel_id: r.try_get("discord_channel_id")?,
        discord_channel_name: r.try_get("discord_channel_name")?,
        discord_webhook_id: r.try_get("discord_webhook_id")?,
        direction: Direction::parse(&r.try_get::<String, _>("direction")?)
            .ok_or_else(|| BridgeError::Storage("unknown map direction".into()))?,
        created_by: r.try_get("created_by")?,
        created_at_ms: r.try_get("created_ms")?,
    })
}

/// The bridge of a server, if one is connected.
pub async fn get_bridge(pool: &PgPool, guild: i64) -> BridgeResult<Option<Bridge>> {
    let sql = format!("SELECT {BRIDGE_COLS} FROM bridges WHERE guild_id = $1");
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(guild)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(bridge_from).transpose()
}

/// Every channel map of a server, oldest first.
pub async fn list_maps(pool: &PgPool, guild: i64) -> BridgeResult<Vec<ChannelMap>> {
    let sql = format!("SELECT {MAP_COLS} FROM bridge_channel_maps WHERE guild_id = $1 ORDER BY id");
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(guild)
        .fetch_all(pool)
        .await?;
    rows.iter().map(map_from).collect()
}

/// The map of one SIDE ROOM channel, if it is bridged.
pub async fn map_for_channel(pool: &PgPool, channel: i64) -> BridgeResult<Option<ChannelMap>> {
    let sql = format!("SELECT {MAP_COLS} FROM bridge_channel_maps WHERE channel_id = $1");
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(channel)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(map_from).transpose()
}

/// The message map row of one of our messages.
pub async fn message_map_for(pool: &PgPool, sr_message: i64) -> BridgeResult<Option<MessageMap>> {
    let row = sqlx::query(
        "SELECT sr_message_id, discord_message_id, map_id, webhook_id, author_id, direction \
         FROM bridge_message_map WHERE sr_message_id = $1",
    )
    .bind(sr_message)
    .fetch_optional(pool)
    .await?;
    row.map(|r| -> BridgeResult<MessageMap> {
        Ok(MessageMap {
            sr_message_id: r.try_get("sr_message_id")?,
            discord_message_id: r.try_get("discord_message_id")?,
            map_id: r.try_get("map_id")?,
            webhook_id: r.try_get("webhook_id")?,
            author_id: r.try_get("author_id")?,
            direction: MessageMapDirection::parse(&r.try_get::<String, _>("direction")?)
                .ok_or_else(|| BridgeError::Storage("unknown message map direction".into()))?,
        })
    })
    .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_round_trips_and_says_who_sends() {
        for d in Direction::ALL {
            assert_eq!(Direction::parse(d.as_str()), Some(d));
        }
        assert!(Direction::Both.sends() && Direction::Both.receives());
        assert!(Direction::ToDiscord.sends() && !Direction::ToDiscord.receives());
        assert!(!Direction::FromDiscord.sends() && Direction::FromDiscord.receives());
        assert_eq!(Direction::parse("sideways"), None);
    }

    #[test]
    fn status_round_trips() {
        for s in ["connecting", "connected", "disconnected", "token_invalid"] {
            assert_eq!(BridgeStatus::parse(s).map(BridgeStatus::as_str), Some(s));
        }
    }
}
