//! Interaction lifecycle: creation, the 3 second acknowledgement window, the 15 minute
//! token (DEV-BOT-004).

use super::AppsService;
use crate::{
    error::{AppsError, AppsResult},
    interactions::{ACK_WINDOW_MS, TOKEN_LIFETIME_MS},
    model::*,
};
use serde_json::Value;
use sr_auth::oauth::{self as authoauth, OpaqueKind, OpaqueToken, hash_presented};

/// What the API layer hands over to create an interaction.
#[derive(Debug, Clone)]
pub struct NewInteraction {
    /// Application.
    pub application_id: i64,
    /// 2, 3, 4 or 5.
    pub kind: i16,
    /// Invoking user.
    pub user_id: i64,
    /// Guild.
    pub guild_id: Option<i64>,
    /// Channel.
    pub channel_id: i64,
    /// Command (kind 2 and 4).
    pub command_id: Option<i64>,
    /// Message the component sits on (kind 3).
    pub source_message_id: Option<i64>,
    /// Client nonce.
    pub nonce: Option<String>,
}

impl AppsService {
    /// Create the interaction and its one-time token.
    pub async fn create_interaction(
        &self,
        n: NewInteraction,
    ) -> AppsResult<(InteractionRow, String)> {
        let app = self.application(n.application_id).await?;
        if !app.is_live() {
            return Err(AppsError::Suspended);
        }
        let token = OpaqueToken::generate(OpaqueKind::InteractionToken)
            .map_err(|e| AppsError::Storage(e.to_string()))?;
        let now = self.now_ms();
        let row = InteractionRow {
            id: self.next_id(),
            application_id: n.application_id,
            kind: n.kind,
            user_id: n.user_id,
            guild_id: n.guild_id,
            channel_id: n.channel_id,
            command_id: n.command_id,
            source_message_id: n.source_message_id,
            nonce: n.nonce,
            state: "created".to_owned(),
            response_type: None,
            ephemeral: false,
            original_message_id: None,
            created_at_ms: now,
            expires_at_ms: now + TOKEN_LIFETIME_MS,
            token_hash: *token.hash().as_bytes(),
        };
        self.repo.insert_interaction(&row).await?;
        Ok((row, token.expose().to_owned()))
    }

    /// Find an interaction by id and check its token in constant time. An unknown id is
    /// `UnknownInteraction`, a wrong token `InvalidToken`, a spent token (15 minutes)
    /// `UnknownInteraction`.
    pub async fn interaction_by_token(&self, id: i64, token: &str) -> AppsResult<InteractionRow> {
        let row = self
            .repo
            .interaction(id)
            .await?
            .ok_or(AppsError::UnknownInteraction)?;
        let presented =
            hash_presented(OpaqueKind::InteractionToken, token).ok_or(AppsError::InvalidToken)?;
        let stored = sr_auth::session::TokenHash::from_bytes(&row.token_hash)
            .map_err(|e| AppsError::Storage(e.to_string()))?;
        if !authoauth::digests_equal(&presented, &stored) {
            return Err(AppsError::InvalidToken);
        }
        if row.expires_at_ms <= self.now_ms() {
            return Err(AppsError::UnknownInteraction);
        }
        Ok(row)
    }

    /// Find the live interaction behind a `/webhooks/{application_id}/{token}` call. `None`
    /// for anything that is not one (the caller then treats the path as a plain webhook).
    pub async fn interaction_for_webhook(
        &self,
        application_id: i64,
        token: &str,
    ) -> AppsResult<Option<InteractionRow>> {
        let Some(presented) = hash_presented(OpaqueKind::InteractionToken, token) else {
            return Ok(None);
        };
        let Some(row) = self.repo.interaction_by_hash(&presented).await? else {
            return Ok(None);
        };
        let stored = sr_auth::session::TokenHash::from_bytes(&row.token_hash)
            .map_err(|e| AppsError::Storage(e.to_string()))?;
        if row.application_id != application_id
            || !authoauth::digests_equal(&presented, &stored)
            || row.expires_at_ms <= self.now_ms()
        {
            return Ok(None);
        }
        Ok(Some(row))
    }

    /// First response: only within [`ACK_WINDOW_MS`], only once. `state` is `responded` or
    /// `deferred`.
    pub async fn acknowledge(
        &self,
        row: &InteractionRow,
        state: &str,
        callback_type: i16,
        ephemeral: bool,
    ) -> AppsResult<()> {
        let now = self.now_ms();
        if row.state != "created" {
            return Err(AppsError::AlreadyAcknowledged);
        }
        if now - row.created_at_ms > ACK_WINDOW_MS {
            return Err(AppsError::UnknownInteraction);
        }
        if self
            .repo
            .ack_interaction(row.id, state, callback_type, ephemeral, now)
            .await?
        {
            Ok(())
        } else {
            Err(AppsError::AlreadyAcknowledged)
        }
    }

    /// Has the interaction been acknowledged (re-read from storage)?
    pub async fn is_acknowledged(&self, id: i64) -> AppsResult<bool> {
        Ok(self
            .repo
            .interaction(id)
            .await?
            .is_some_and(|r| r.state != "created"))
    }

    /// Remember a message as belonging to the interaction.
    pub async fn attach_message(
        &self,
        interaction: &InteractionRow,
        message_id: i64,
        ephemeral: bool,
        is_original: bool,
        payload: Option<Value>,
    ) -> AppsResult<()> {
        self.repo
            .add_interaction_message(&InteractionMessage {
                message_id,
                interaction_id: interaction.id,
                ephemeral,
                is_original,
                payload,
            })
            .await?;
        if is_original {
            self.repo
                .set_original_message(interaction.id, message_id)
                .await?;
        }
        Ok(())
    }

    /// The mapping of a message, only if it belongs to `interaction`.
    pub async fn message_of(
        &self,
        interaction: &InteractionRow,
        message_id: i64,
    ) -> AppsResult<InteractionMessage> {
        self.repo
            .interaction_message(message_id)
            .await?
            .filter(|m| m.interaction_id == interaction.id)
            .ok_or(AppsError::NotFound("message"))
    }

    /// Replace the stored object of an ephemeral message.
    pub async fn set_ephemeral_payload(&self, message_id: i64, payload: &Value) -> AppsResult<()> {
        self.repo.set_ephemeral_payload(message_id, payload).await
    }

    /// Forget a message mapping.
    pub async fn detach_message(&self, message_id: i64) -> AppsResult<()> {
        self.repo.delete_interaction_message(message_id).await
    }

    /// Housekeeping: drop interactions a day past their token.
    pub async fn purge_interactions(&self) -> AppsResult<u64> {
        self.repo
            .purge_interactions(self.now_ms() - 24 * 3600 * 1000)
            .await
    }

    /// The command behind an interaction request, checked against the application and the
    /// scope (global, or this guild).
    pub async fn command_for_invocation(
        &self,
        app: i64,
        guild: Option<i64>,
        command_id: i64,
    ) -> AppsResult<Command> {
        self.repo
            .command_by_id(command_id)
            .await?
            .filter(|c| c.application_id == app && (c.guild_id.is_none() || c.guild_id == guild))
            .ok_or(AppsError::UnknownCommand)
    }

    /// Overwrites that apply to an invocation: the command's own and the application default.
    pub async fn invocation_permissions(
        &self,
        app: i64,
        guild: i64,
        command_id: i64,
    ) -> AppsResult<(Vec<CommandPermission>, Vec<CommandPermission>)> {
        Ok((
            self.repo
                .command_permissions(app, guild, command_id)
                .await?,
            self.repo.command_permissions(app, guild, app).await?,
        ))
    }
}
