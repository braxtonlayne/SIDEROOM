//! Connecting the owner's bot: validate the token, seal it, remember which Discord server it works in.
//!
//! The token is the owner's property on Discord's side. It is checked with `GET /users/@me` and must belong to a
//! **bot** (`bot: true`): a user token (a self-bot) is refused and never stored. It is sealed with
//! `server.secret_key` before it touches the database, bound to the server's id, and no route returns it.

use serde_json::json;

use crate::discord::CallError;
use crate::error::{BridgeError, BridgeResult};
use crate::model::{Bridge, get_bridge};
use crate::service::{AUDIT_INTEGRATION_CREATE, AuditRow, BridgeService};

/// Longest bot token accepted (Discord's are about 70 characters).
const MAX_TOKEN_LEN: usize = 200;

fn bad_token(e: CallError) -> BridgeError {
    match e {
        CallError::Status {
            status: 401 | 403, ..
        } => BridgeError::BadToken("Discord refused this token. Paste the bot token from the Bot page of your application.".into()),
        other => BridgeError::Discord(other.to_string()),
    }
}

impl BridgeService {
    /// Connect (or replace the token of) the server's bridge.
    ///
    /// `pick` names the Discord server when the bot is in several; with exactly one it may be left out.
    /// Replacing the token is allowed for the same bot only (a reset token); a different bot needs a revoke first,
    /// because the webhooks and maps belong to the first one.
    pub async fn connect(
        &self,
        guild: i64,
        actor: i64,
        raw_token: &str,
        pick: Option<i64>,
    ) -> BridgeResult<Bridge> {
        let token = raw_token.trim();
        let token = token.strip_prefix("Bot ").unwrap_or(token).trim();
        if token.is_empty() || token.len() > MAX_TOKEN_LEN || token.chars().any(char::is_whitespace)
        {
            return Err(BridgeError::Invalid(
                "token: paste the bot token only".into(),
            ));
        }
        let me = self.discord.bot_user(token).await.map_err(bad_token)?;
        if !me.bot {
            return Err(BridgeError::BadToken(
                "That is not a bot token. Only a bot application's token is accepted, never a user account's.".into(),
            ));
        }
        let guilds = self.discord.bot_guilds(token).await.map_err(bad_token)?;
        let chosen = match (pick, guilds.as_slice()) {
            (Some(id), list) => list.iter().find(|g| g.id == id).cloned().ok_or_else(|| {
                BridgeError::Conflict("The bot is not a member of that Discord server.".into())
            })?,
            (None, []) => {
                return Err(BridgeError::Conflict(
                    "The bot is not in any Discord server yet. Invite it with the link on this page first.".into(),
                ));
            }
            (None, [only]) => only.clone(),
            (None, many) => {
                let names: Vec<String> = many
                    .iter()
                    .take(10)
                    .map(|g| format!("{} ({})", g.name, g.id))
                    .collect();
                return Err(BridgeError::Invalid(format!(
                    "discord_guild_id: the bot is in several Discord servers, choose one of: {}",
                    names.join(", ")
                )));
            }
        };
        let sealed = self.sealer.seal(&Self::bot_aad(guild), token.as_bytes())?;
        let mut tx = self.pool.begin().await?;
        let existing: Option<(i64,)> =
            sqlx::query_as("SELECT bot_user_id FROM bridges WHERE guild_id = $1 FOR UPDATE")
                .bind(guild)
                .fetch_optional(&mut *tx)
                .await?;
        match existing {
            Some((bot,)) if bot != me.id => {
                return Err(BridgeError::Conflict(
                    "A different bot is already connected. Revoke its token first.".into(),
                ));
            }
            Some(_) => {
                sqlx::query(
                    "UPDATE bridges SET bot_token_enc = $2, bot_name = $3, status = 'connecting', updated_at = now() \
                     WHERE guild_id = $1",
                )
                .bind(guild)
                .bind(&sealed)
                .bind(&me.name)
                .execute(&mut *tx)
                .await?;
            }
            None => {
                sqlx::query(
                    "INSERT INTO bridges (guild_id, discord_guild_id, discord_guild_name, bot_user_id, bot_name, \
                     bot_token_enc, created_by) VALUES ($1, $2, $3, $4, $5, $6, $7)",
                )
                .bind(guild)
                .bind(chosen.id)
                .bind(&chosen.name)
                .bind(me.id)
                .bind(&me.name)
                .bind(&sealed)
                .bind(actor)
                .execute(&mut *tx)
                .await?;
                // A fresh connection starts clean: last time's leftovers were shown to the owner already.
                sqlx::query("DELETE FROM bridge_teardown_residue WHERE guild_id = $1")
                    .bind(guild)
                    .execute(&mut *tx)
                    .await?;
                self.audit(
                    &mut tx,
                    AuditRow {
                        guild,
                        actor,
                        action: AUDIT_INTEGRATION_CREATE,
                        target: Some(chosen.id),
                        changes: json!([{"key": "bridge", "new_value": {"discord_guild_id": chosen.id.to_string(), "bot_user_id": me.id.to_string()}}]),
                        options: json!({"integration": "discord_bridge"}),
                    },
                )
                .await?;
            }
        }
        tx.commit().await?;
        get_bridge(&self.pool, guild)
            .await?
            .ok_or(BridgeError::NotFound("bridge"))
    }

    /// Record what the gateway half learned: the connection state and the intents Discord granted.
    pub async fn set_status(
        &self,
        guild: i64,
        status: crate::model::BridgeStatus,
        intents: Option<i64>,
    ) -> BridgeResult<()> {
        sqlx::query(
            "UPDATE bridges SET status = $2, intents_granted = COALESCE($3, intents_granted), updated_at = now() \
             WHERE guild_id = $1",
        )
        .bind(guild)
        .bind(status.as_str())
        .bind(intents)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Every server that holds a bridge (the gateway half starts one shard each).
    pub async fn all_bridges(&self) -> BridgeResult<Vec<Bridge>> {
        let sql = format!(
            "SELECT {} FROM bridges ORDER BY guild_id",
            crate::model::BRIDGE_COLS
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(crate::model::bridge_from).collect()
    }
}
