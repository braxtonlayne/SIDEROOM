//! FTR-SUGG-001 suggestions. A member posts a suggestion; it appears as a card in the server's suggestions
//! channel with up and down vote reactions that the system user seeds, so voting is one click with the ordinary
//! reaction machinery. Staff (people who can manage the server, or the configured staff role) mark it
//! `considering`, `approved`, `denied` or `implemented` with a response, and the card is edited to match.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

use crate::error::{UfError, UfResult, invalid};
use crate::poster::{Card, CardField, Poster};
use crate::{UserFeatures, features::SUGGESTIONS_ID};

pub const MAX_CONTENT_CHARS: usize = 2000;
pub const MAX_RESPONSE_CHARS: usize = 1000;
pub const STATUSES: [&str; 5] = ["open", "considering", "approved", "denied", "implemented"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow, ToSchema)]
pub struct Suggestion {
    pub id: String,
    pub channel_id: String,
    /// The card message in the suggestions channel.
    pub message_id: String,
    pub author_id: String,
    pub content: String,
    /// `open`, `considering`, `approved`, `denied` or `implemented`.
    pub status: String,
    pub response: String,
    pub responder_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SuggestionInput {
    pub content: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SuggestionResponse {
    pub status: String,
    #[serde(default)]
    pub response: Option<String>,
}

macro_rules! cols {
    () => {
        concat!(
            "id::text AS id, channel_id::text AS channel_id, message_id::text AS message_id, \
             author_id::text AS author_id, content, status, response, responder_id::text AS responder_id, ",
            iso!("created_at"),
            " AS created_at, ",
            iso!("updated_at"),
            " AS updated_at"
        )
    };
}

pub fn status_color(status: &str) -> u32 {
    match status {
        "approved" | "implemented" => 0x2ECC71,
        "denied" => 0xE74C3C,
        "considering" => 0xF1C40F,
        _ => 0x95A5A6,
    }
}

pub fn status_label(status: &str) -> &'static str {
    match status {
        "considering" => "Under consideration",
        "approved" => "Approved",
        "denied" => "Denied",
        "implemented" => "Implemented",
        _ => "Open for voting",
    }
}

/// The card for a suggestion in its current state.
pub fn card(s: &Suggestion) -> Card {
    let mut fields = vec![CardField {
        name: "Status".into(),
        value: status_label(&s.status).into(),
        inline: true,
    }];
    if !s.response.is_empty() {
        fields.push(CardField {
            name: match s.responder_id.as_deref() {
                Some(id) => format!("Response from staff ({id})"),
                None => "Response from staff".into(),
            },
            value: s.response.clone(),
            inline: false,
        });
    }
    Card {
        content: format!("Suggested by <@{}>", s.author_id),
        title: Some("Suggestion".into()),
        description: Some(s.content.clone()),
        color: status_color(&s.status),
        fields,
        footer: Some(format!("#{}", s.id)),
        ..Card::default()
    }
}

pub fn clean_content(content: &str) -> UfResult<String> {
    let c = content.trim();
    if c.is_empty() {
        return Err(invalid("content must not be empty"));
    }
    if c.chars().count() > MAX_CONTENT_CHARS {
        return Err(invalid(format!(
            "content must be {MAX_CONTENT_CHARS} characters or fewer"
        )));
    }
    Ok(c.to_owned())
}

impl UserFeatures {
    /// Post a suggestion. The member must be able to see the suggestions channel (checked by the caller).
    pub async fn submit_suggestion(
        &self,
        poster: &dyn Poster,
        guild: i64,
        author: i64,
        content: &str,
    ) -> UfResult<Suggestion> {
        self.require(SUGGESTIONS_ID, Some(guild)).await?;
        let cfg = self.settings(SUGGESTIONS_ID, Some(guild)).await?;
        let channel = cfg
            .id("channel")
            .ok_or_else(|| UfError::Conflict("No suggestions channel has been set up.".into()))?;
        let content = clean_content(content)?;
        let id = self.next_id();
        // The card needs the row's id, the row needs the card's message id: insert with a placeholder first.
        let mut row = sqlx::query_as::<_, Suggestion>(concat!(
            "INSERT INTO suggestions (id, guild_id, channel_id, message_id, author_id, content) \
             VALUES ($1, $2, $3, 0, $4, $5) RETURNING ",
            cols!()
        ))
        .bind(id)
        .bind(guild)
        .bind(channel)
        .bind(author)
        .bind(&content)
        .fetch_one(&self.pool)
        .await?;
        let message = match poster.post(channel, card(&row)).await {
            Ok(m) => m,
            Err(e) => {
                sqlx::query("DELETE FROM suggestions WHERE id = $1")
                    .bind(id)
                    .execute(&self.pool)
                    .await?;
                tracing::warn!(guild, error = %e, "suggestion card could not be posted");
                return Err(UfError::Conflict(
                    "The suggestions channel is not available right now.".into(),
                ));
            }
        };
        sqlx::query("UPDATE suggestions SET message_id = $2 WHERE id = $1")
            .bind(id)
            .bind(message)
            .execute(&self.pool)
            .await?;
        row.message_id = message.to_string();
        for key in ["up_emoji", "down_emoji"] {
            if let Some(e) = cfg.text(key) {
                poster.react(channel, message, e).await;
            }
        }
        Ok(row)
    }

    pub async fn list_suggestions(
        &self,
        guild: i64,
        status: Option<&str>,
        before: Option<i64>,
        limit: i64,
    ) -> UfResult<Vec<Suggestion>> {
        self.require(SUGGESTIONS_ID, Some(guild)).await?;
        Ok(sqlx::query_as::<_, Suggestion>(concat!(
            "SELECT ",
            cols!(),
            " FROM suggestions WHERE guild_id = $1 AND ($2::text IS NULL OR status = $2) \
             AND ($3::bigint IS NULL OR id < $3) ORDER BY id DESC LIMIT $4"
        ))
        .bind(guild)
        .bind(status)
        .bind(before)
        .bind(limit.clamp(1, 100))
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_suggestion(&self, guild: i64, id: i64) -> UfResult<Suggestion> {
        sqlx::query_as::<_, Suggestion>(concat!(
            "SELECT ",
            cols!(),
            " FROM suggestions WHERE guild_id = $1 AND id = $2"
        ))
        .bind(guild)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(UfError::NotFound("suggestion"))
    }

    /// Set the status and staff response and edit the card. The caller has checked that `responder` may.
    pub async fn respond_to_suggestion(
        &self,
        poster: &dyn Poster,
        guild: i64,
        id: i64,
        responder: i64,
        input: &SuggestionResponse,
    ) -> UfResult<Suggestion> {
        self.require(SUGGESTIONS_ID, Some(guild)).await?;
        if !STATUSES.contains(&input.status.as_str()) {
            return Err(invalid(
                "status must be open, considering, approved, denied or implemented",
            ));
        }
        let response = input.response.as_deref().unwrap_or("").trim();
        if response.chars().count() > MAX_RESPONSE_CHARS {
            return Err(invalid(format!(
                "response must be {MAX_RESPONSE_CHARS} characters or fewer"
            )));
        }
        let row = sqlx::query_as::<_, Suggestion>(concat!(
            "UPDATE suggestions SET status = $3, response = $4, responder_id = $5, updated_at = now() \
             WHERE guild_id = $1 AND id = $2 RETURNING ",
            cols!()
        ))
        .bind(guild)
        .bind(id)
        .bind(&input.status)
        .bind(response)
        .bind(responder)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(UfError::NotFound("suggestion"))?;
        if let (Ok(channel), Ok(message)) = (row.channel_id.parse(), row.message_id.parse())
            && message != 0
        {
            poster.edit(channel, message, card(&row)).await;
        }
        Ok(row)
    }

    pub async fn delete_suggestion(&self, guild: i64, id: i64) -> UfResult<()> {
        let n = sqlx::query("DELETE FROM suggestions WHERE guild_id = $1 AND id = $2")
            .bind(guild)
            .bind(id)
            .execute(&self.pool)
            .await?
            .rows_affected();
        if n == 1 {
            Ok(())
        } else {
            Err(UfError::NotFound("suggestion"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Suggestion {
        Suggestion {
            id: "5".into(),
            channel_id: "2".into(),
            message_id: "9".into(),
            author_id: "7".into(),
            content: "More emoji".into(),
            status: "open".into(),
            response: String::new(),
            responder_id: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn card_reflects_status_and_response() {
        let mut s = sample();
        let open = card(&s);
        assert_eq!(open.color, 0x95A5A6);
        assert_eq!(open.fields.len(), 1);
        assert!(open.content.contains("<@7>"));
        assert!(open.ping_users.is_empty(), "suggestion cards ping nobody");
        s.status = "approved".into();
        s.response = "Coming soon".into();
        s.responder_id = Some("3".into());
        let done = card(&s);
        assert_eq!(done.color, 0x2ECC71);
        assert_eq!(done.fields.len(), 2);
        assert_eq!(done.fields[1].value, "Coming soon");
    }

    #[test]
    fn content_rules() {
        assert!(clean_content("  ").is_err());
        assert!(clean_content(&"x".repeat(2001)).is_err());
        assert_eq!(clean_content(" ok ").unwrap(), "ok");
    }
}
