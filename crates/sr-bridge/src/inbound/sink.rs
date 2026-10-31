//! What the inbound half needs from the chat side, as a trait: the server wiring implements it over the message
//! service (post as the integration author, then record the `sr_bridge` display data and publish the gateway
//! event, the way an incoming webhook does), and the tests implement it in memory.

use async_trait::async_trait;

use super::convert::InLink;
use crate::error::BridgeResult;

/// Who wrote the message on Discord.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundAuthor {
    /// `discord`.
    pub platform: &'static str,
    pub name: String,
    /// The Discord avatar URL; the API proxies it through `/cdn/external` at view time.
    pub avatar: Option<String>,
    pub discord_user_id: i64,
    pub links: Vec<InLink>,
}

/// One message to show here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundPost {
    pub guild_id: i64,
    pub channel_id: i64,
    /// The SIDE ROOM account the message is stored under (the one who connected the bot, like a webhook's
    /// creator). The shown author is [`InboundAuthor`].
    pub as_user: i64,
    /// Our message this one answers.
    pub reply_to: Option<i64>,
    /// May be empty when the message is only attachment links.
    pub content: String,
    pub author: InboundAuthor,
}

#[async_trait]
pub trait ChatSink: Send + Sync {
    /// Post and publish; the new message's id.
    async fn post(&self, post: InboundPost) -> BridgeResult<i64>;
    /// Replace the text and/or the attachment links of a bridged message.
    async fn edit(
        &self,
        channel_id: i64,
        message_id: i64,
        content: Option<String>,
        links: Option<Vec<InLink>>,
    ) -> BridgeResult<()>;
    /// Delete a bridged message (a Discord delete). Already gone is not an error.
    async fn delete(&self, channel_id: i64, message_id: i64) -> BridgeResult<()>;
    /// A Discord member started typing in a mapped channel: shown through the existing typing event path with the
    /// member's Discord id as the user id (clients resolve it against the channel's bridged members).
    async fn typing(&self, _channel_id: i64, _discord_user_id: i64) -> BridgeResult<()> {
        Ok(())
    }
    /// Add or take away the integration author's reaction with a Unicode emoji.
    async fn react(
        &self,
        channel_id: i64,
        message_id: i64,
        emoji: &str,
        add: bool,
    ) -> BridgeResult<()>;
}
