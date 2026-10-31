//! Channel maps: which SIDE ROOM channel mirrors which Discord channel, and the webhook that posts for it.
//!
//! A map that sends to Discord owns one Discord webhook, created by the bridge with the owner's bot in that
//! channel; its token is sealed. Removing the map deletes the webhook on Discord (three attempts), and a
//! webhook that could not be deleted is remembered by id (`bridge_teardown_residue`) so the owner can remove it
//! by hand from Discord's Integrations page.
//!
//! End-to-end encrypted conversations are never bridged: the server cannot read them, and a bridge would
//! defeat the point. Direct messages, group chats and anything that is not a text or announcement channel of the
//! server are refused ([`refusal_for_channel`]).

use std::time::Duration;

use sqlx::Row;

use crate::discord::CallError;
use crate::error::{BridgeError, BridgeResult};
use crate::model::{ChannelMap, Direction, MAP_COLS, get_bridge, map_from};
use crate::service::BridgeService;

/// The name the webhooks carry in Discord's Integrations list.
pub const WEBHOOK_NAME: &str = "SIDE ROOM bridge";

/// Why a SIDE ROOM channel cannot be bridged, or `None` when it can. `guild_id` is the channel's server (a DM has
/// none); `kind` is the channel type (0 text, 5 announcement; 1 and 3 are direct and group messages).
pub fn refusal_for_channel(guild_id: Option<i64>, kind: i16) -> Option<&'static str> {
    if guild_id.is_none() || matches!(kind, 1 | 3) {
        return Some(
            "Direct messages and group chats are never bridged: they are private, and the end-to-end encrypted ones cannot be read by the server at all.",
        );
    }
    if !matches!(kind, 0 | 5 | 15 | 16) {
        return Some(
            "Only text, announcement, forum and media channels can be picked on their own.",
        );
    }
    None
}

impl BridgeService {
    /// The webhook id and opened token of a map, if it has one.
    pub(crate) async fn webhook_of(&self, map_id: i64) -> BridgeResult<Option<(i64, String)>> {
        let row = sqlx::query(
            "SELECT discord_webhook_id, webhook_token_enc FROM bridge_channel_maps WHERE id = $1",
        )
        .bind(map_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let id: Option<i64> = row.try_get("discord_webhook_id")?;
        let enc: Option<Vec<u8>> = row.try_get("webhook_token_enc")?;
        match (id, enc) {
            (Some(id), Some(enc)) => {
                let plain = self.sealer.open(&Self::webhook_aad(map_id), &enc)?;
                let token = String::from_utf8(plain).map_err(|_| BridgeError::Seal)?;
                Ok(Some((id, token)))
            }
            _ => Ok(None),
        }
    }

    /// Delete a webhook on Discord: up to `delete_backoff.len() + 1` attempts. A webhook that is already gone
    /// counts as deleted. Errors never carry the token.
    pub(crate) async fn delete_webhook_with_retries(
        &self,
        webhook: i64,
        token: &str,
    ) -> Result<(), String> {
        let mut waits = self.tuning.delete_backoff.iter();
        loop {
            let err = match self.discord.delete_webhook(webhook, token).await {
                Ok(()) | Err(CallError::Gone(_)) => return Ok(()),
                Err(e) => e,
            };
            match waits.next() {
                Some(wait) => {
                    let wait = match &err {
                        CallError::RateLimited(d) => (*d).min(Duration::from_secs(10)).max(*wait),
                        _ => *wait,
                    };
                    tokio::time::sleep(wait).await;
                }
                None => return Err(err.to_string()),
            }
        }
    }

    /// Remember a webhook that could not be deleted, by id only.
    pub(crate) async fn remember_residue(
        &self,
        guild: i64,
        webhook: i64,
        discord_channel: i64,
        reason: &str,
    ) {
        tracing::warn!(
            guild,
            webhook_id = webhook,
            discord_channel,
            "could not delete a bridge webhook on Discord; the owner removes it by hand"
        );
        let reason: String = reason.chars().take(200).collect();
        if let Err(e) = sqlx::query(
            "INSERT INTO bridge_teardown_residue (guild_id, discord_webhook_id, discord_channel_id, reason) \
             VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
        )
        .bind(guild)
        .bind(webhook)
        .bind(discord_channel)
        .bind(reason)
        .execute(&self.pool)
        .await
        {
            tracing::error!(error = %e, webhook_id = webhook, "could not record a leftover bridge webhook");
        }
    }

    /// Map `channel` (a SIDE ROOM channel of the server) to `discord_channel`, creating the webhook when the
    /// direction sends to Discord.
    pub async fn create_map(
        &self,
        guild: i64,
        actor: i64,
        channel: i64,
        discord_channel: i64,
        direction: Direction,
    ) -> BridgeResult<ChannelMap> {
        let bridge = get_bridge(&self.pool, guild)
            .await?
            .ok_or_else(|| BridgeError::Conflict("Connect the bot token first.".into()))?;
        let row: Option<(Option<i64>, i16)> =
            sqlx::query_as("SELECT guild_id, type FROM channels WHERE id = $1")
                .bind(channel)
                .fetch_optional(&self.pool)
                .await?;
        let (ch_guild, kind) = row.ok_or(BridgeError::NotFound("channel"))?;
        if let Some(why) = refusal_for_channel(ch_guild, kind) {
            return Err(BridgeError::Invalid(format!("channel_id: {why}")));
        }
        if ch_guild != Some(guild) {
            return Err(BridgeError::NotFound("channel"));
        }
        let token = self.open_bot_token(guild).await?;
        let channels = self
            .discord
            .guild_channels(&token, bridge.discord_guild_id)
            .await?;
        let target = channels
            .into_iter()
            .find(|c| c.id == discord_channel)
            .ok_or_else(|| {
                BridgeError::Invalid(
                    "discord_channel_id: the bot cannot see that channel in the connected Discord server.".into(),
                )
            })?;
        let taken: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM bridge_channel_maps WHERE channel_id = $1 OR (guild_id = $2 AND discord_channel_id = $3)",
        )
        .bind(channel)
        .bind(guild)
        .bind(discord_channel)
        .fetch_optional(&self.pool)
        .await?;
        if taken.is_some() {
            return Err(BridgeError::Conflict(
                "That channel, or that Discord channel, is already mapped.".into(),
            ));
        }
        let map_id = self.next_id();
        // The webhook as Discord made it (plain token, only in memory) and as we store it (sealed).
        let created = if direction.sends() {
            Some(
                self.discord
                    .create_webhook(&token, discord_channel, WEBHOOK_NAME)
                    .await?,
            )
        } else {
            None
        };
        let sealed_webhook = created.as_ref().map(|w| {
            self.sealer
                .seal(&Self::webhook_aad(map_id), w.token.as_bytes())
        });
        let sealed_webhook = match sealed_webhook.transpose() {
            Ok(v) => v,
            Err(e) => {
                self.discard_webhook(guild, discord_channel, created.as_ref())
                    .await;
                return Err(e);
            }
        };
        let inserted = sqlx::query(
            "INSERT INTO bridge_channel_maps (id, guild_id, channel_id, discord_channel_id, discord_channel_name, \
             discord_webhook_id, webhook_token_enc, direction, created_by) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(map_id)
        .bind(guild)
        .bind(channel)
        .bind(discord_channel)
        .bind(&target.name)
        .bind(created.as_ref().map(|w| w.id))
        .bind(sealed_webhook)
        .bind(direction.as_str())
        .bind(actor)
        .execute(&self.pool)
        .await;
        if let Err(e) = inserted {
            // The row did not land: do not leave a webhook behind on Discord.
            self.discard_webhook(guild, discord_channel, created.as_ref())
                .await;
            return Err(e.into());
        }
        let map = self.map_by_id(map_id).await?;
        self.notices
            .system_message(
                channel,
                &format!(
                    "This channel is now bridged with #{} on the Discord server {}. Messages here are mirrored there and back, with a DISCORD tag on messages from Discord.",
                    map.discord_channel_name, bridge.discord_guild_name
                ),
            )
            .await;
        self.notices.channel_changed(channel).await;
        Ok(map)
    }

    /// A webhook was created but the map that should own it could not be stored: delete it again, or remember
    /// it for the owner when Discord will not.
    async fn discard_webhook(
        &self,
        guild: i64,
        discord_channel: i64,
        created: Option<&crate::discord::CreatedWebhook>,
    ) {
        let Some(w) = created else { return };
        if let Err(reason) = self.delete_webhook_with_retries(w.id, &w.token).await {
            self.remember_residue(guild, w.id, discord_channel, &reason)
                .await;
        }
    }

    pub(crate) async fn map_by_id(&self, map_id: i64) -> BridgeResult<ChannelMap> {
        let sql = format!("SELECT {MAP_COLS} FROM bridge_channel_maps WHERE id = $1");
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(map_id)
            .fetch_optional(&self.pool)
            .await?;
        row.as_ref()
            .map(map_from)
            .transpose()?
            .ok_or(BridgeError::NotFound("map"))
    }

    /// A map of this server, by id.
    pub async fn get_map(&self, guild: i64, map_id: i64) -> BridgeResult<ChannelMap> {
        let map = self.map_by_id(map_id).await?;
        if map.guild_id != guild {
            return Err(BridgeError::NotFound("map"));
        }
        Ok(map)
    }

    /// Change the direction of a map: a map that starts sending gets a webhook, one that stops loses it.
    pub async fn update_map(
        &self,
        guild: i64,
        map_id: i64,
        direction: Direction,
    ) -> BridgeResult<ChannelMap> {
        let map = self.get_map(guild, map_id).await?;
        let have = self.webhook_of(map_id).await?;
        match (direction.sends(), have) {
            (true, None) => {
                let token = self.open_bot_token(guild).await?;
                let w = self
                    .discord
                    .create_webhook(&token, map.discord_channel_id, WEBHOOK_NAME)
                    .await?;
                let enc = self
                    .sealer
                    .seal(&Self::webhook_aad(map_id), w.token.as_bytes())?;
                sqlx::query(
                    "UPDATE bridge_channel_maps SET direction = $2, discord_webhook_id = $3, webhook_token_enc = $4 WHERE id = $1",
                )
                .bind(map_id)
                .bind(direction.as_str())
                .bind(w.id)
                .bind(enc)
                .execute(&self.pool)
                .await?;
            }
            (false, Some((id, token))) => {
                if let Err(reason) = self.delete_webhook_with_retries(id, &token).await {
                    self.remember_residue(guild, id, map.discord_channel_id, &reason)
                        .await;
                }
                sqlx::query(
                    "UPDATE bridge_channel_maps SET direction = $2, discord_webhook_id = NULL, webhook_token_enc = NULL WHERE id = $1",
                )
                .bind(map_id)
                .bind(direction.as_str())
                .execute(&self.pool)
                .await?;
                // Anything still waiting to be posted has nowhere to go.
                sqlx::query("DELETE FROM bridge_outbox WHERE map_id = $1")
                    .bind(map_id)
                    .execute(&self.pool)
                    .await?;
            }
            _ => {
                sqlx::query("UPDATE bridge_channel_maps SET direction = $2 WHERE id = $1")
                    .bind(map_id)
                    .bind(direction.as_str())
                    .execute(&self.pool)
                    .await?;
            }
        }
        self.notices.channel_changed(map.channel_id).await;
        self.map_by_id(map_id).await
    }

    /// Remove one map: its webhook is deleted on Discord, its rows go, and the channel is told.
    pub async fn delete_map(&self, guild: i64, map_id: i64) -> BridgeResult<()> {
        let map = self.get_map(guild, map_id).await?;
        self.remove_map(&map, true).await
    }

    /// The channel behind a map was deleted: same cleanup, without a notice (there is no channel to tell).
    pub(crate) async fn remove_map(&self, map: &ChannelMap, notify: bool) -> BridgeResult<()> {
        if let Some((id, token)) = self.webhook_of(map.id).await?
            && let Err(reason) = self.delete_webhook_with_retries(id, &token).await
        {
            self.remember_residue(map.guild_id, id, map.discord_channel_id, &reason)
                .await;
        }
        sqlx::query("DELETE FROM bridge_channel_maps WHERE id = $1")
            .bind(map.id)
            .execute(&self.pool)
            .await?;
        if notify {
            self.notices
                .system_message(
                    map.channel_id,
                    "This channel is no longer bridged to Discord.",
                )
                .await;
        }
        self.notices.channel_changed(map.channel_id).await;
        Ok(())
    }

    /// The Discord channels the bot sees in the connected server (text and announcement).
    pub async fn discord_channels(
        &self,
        guild: i64,
    ) -> BridgeResult<Vec<crate::discord::DiscordChannel>> {
        let bridge = get_bridge(&self.pool, guild)
            .await?
            .ok_or_else(|| BridgeError::Conflict("Connect the bot token first.".into()))?;
        let token = self.open_bot_token(guild).await?;
        Ok(self
            .discord
            .guild_channels(&token, bridge.discord_guild_id)
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_server_text_channels_can_be_bridged() {
        assert!(refusal_for_channel(Some(1), 0).is_none());
        assert!(refusal_for_channel(Some(1), 5).is_none());
        assert!(
            refusal_for_channel(Some(1), 15).is_none()
                && refusal_for_channel(Some(1), 16).is_none()
        );
        for kind in [2, 4, 13] {
            assert!(refusal_for_channel(Some(1), kind).is_some(), "type {kind}");
        }
        assert!(refusal_for_channel(None, 1).unwrap().contains("end-to-end"));
        assert!(refusal_for_channel(None, 3).is_some());
        assert!(refusal_for_channel(Some(1), 3).is_some());
    }
}
