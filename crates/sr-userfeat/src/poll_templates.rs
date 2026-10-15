//! FTR-POLLT-001 poll templates: saved poll setups per server. Managing them needs `MANAGE_GUILD` (checked by the
//! API layer); anyone who can create a poll in the channel can start one from a template.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

use crate::error::{UfError, UfResult, invalid};
use crate::{UserFeatures, features::POLL_TEMPLATES_ID};

/// Discord's poll limits.
pub const MAX_QUESTION_CHARS: usize = 300;
pub const MAX_ANSWER_CHARS: usize = 55;
pub const MIN_ANSWERS: usize = 2;
pub const MAX_ANSWERS: usize = 10;
pub const MAX_NAME_CHARS: usize = 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PollTemplateInput {
    /// What the template is called in the picker (unique in the server).
    pub name: String,
    pub question: String,
    pub answers: Vec<String>,
    #[serde(default)]
    pub allow_multiselect: bool,
    /// How long the poll runs, 1 to 768 hours. Defaults to 24.
    #[serde(default)]
    pub duration_hours: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct PollTemplate {
    pub id: String,
    pub name: String,
    pub question: String,
    pub answers: Vec<String>,
    pub allow_multiselect: bool,
    pub duration_hours: i32,
    pub created_by: Option<String>,
    pub created_at: String,
}

#[derive(FromRow)]
struct Row {
    id: String,
    name: String,
    question: String,
    answers: serde_json::Value,
    allow_multiselect: bool,
    duration_hours: i32,
    created_by: Option<String>,
    created_at: String,
}

impl From<Row> for PollTemplate {
    fn from(r: Row) -> Self {
        Self {
            id: r.id,
            name: r.name,
            question: r.question,
            answers: serde_json::from_value(r.answers).unwrap_or_default(),
            allow_multiselect: r.allow_multiselect,
            duration_hours: r.duration_hours,
            created_by: r.created_by,
            created_at: r.created_at,
        }
    }
}

macro_rules! cols {
    () => {
        concat!(
            "id::text AS id, name, question, answers, allow_multiselect, duration_hours, \
             created_by::text AS created_by, ",
            iso!("created_at"),
            " AS created_at"
        )
    };
}

/// Validate and normalise a template.
pub fn check(input: &PollTemplateInput) -> UfResult<PollTemplateInput> {
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(invalid(format!(
            "name must be 1 to {MAX_NAME_CHARS} characters"
        )));
    }
    let question = input.question.trim();
    if question.is_empty() || question.chars().count() > MAX_QUESTION_CHARS {
        return Err(invalid(format!(
            "question must be 1 to {MAX_QUESTION_CHARS} characters"
        )));
    }
    let answers: Vec<String> = input.answers.iter().map(|a| a.trim().to_owned()).collect();
    if !(MIN_ANSWERS..=MAX_ANSWERS).contains(&answers.len()) {
        return Err(invalid(format!(
            "a poll needs {MIN_ANSWERS} to {MAX_ANSWERS} answers"
        )));
    }
    if answers
        .iter()
        .any(|a| a.is_empty() || a.chars().count() > MAX_ANSWER_CHARS)
    {
        return Err(invalid(format!(
            "each answer must be 1 to {MAX_ANSWER_CHARS} characters"
        )));
    }
    let mut seen = std::collections::BTreeSet::new();
    if !answers.iter().all(|a| seen.insert(a.to_lowercase())) {
        return Err(invalid("answers must be different from each other"));
    }
    let duration = input.duration_hours.unwrap_or(24);
    if !(1..=768).contains(&duration) {
        return Err(invalid("duration_hours must be 1 to 768"));
    }
    Ok(PollTemplateInput {
        name: name.to_owned(),
        question: question.to_owned(),
        answers,
        allow_multiselect: input.allow_multiselect,
        duration_hours: Some(duration),
    })
}

impl UserFeatures {
    pub async fn list_poll_templates(&self, guild: i64) -> UfResult<Vec<PollTemplate>> {
        self.require(POLL_TEMPLATES_ID, Some(guild)).await?;
        let rows = sqlx::query_as::<_, Row>(concat!(
            "SELECT ",
            cols!(),
            " FROM poll_templates WHERE guild_id = $1 ORDER BY lower(name), id"
        ))
        .bind(guild)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn get_poll_template(&self, guild: i64, id: i64) -> UfResult<PollTemplate> {
        self.require(POLL_TEMPLATES_ID, Some(guild)).await?;
        sqlx::query_as::<_, Row>(concat!(
            "SELECT ",
            cols!(),
            " FROM poll_templates WHERE guild_id = $1 AND id = $2"
        ))
        .bind(guild)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .map(Into::into)
        .ok_or(UfError::NotFound("poll template"))
    }

    pub async fn create_poll_template(
        &self,
        guild: i64,
        by: i64,
        input: &PollTemplateInput,
    ) -> UfResult<PollTemplate> {
        self.require(POLL_TEMPLATES_ID, Some(guild)).await?;
        let cfg = self.settings(POLL_TEMPLATES_ID, Some(guild)).await?;
        let t = check(input)?;
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM poll_templates WHERE guild_id = $1")
                .bind(guild)
                .fetch_one(&self.pool)
                .await?;
        let max = cfg.int("max_templates", 25);
        if count >= max {
            return Err(UfError::Conflict(format!(
                "A server can keep {max} poll templates."
            )));
        }
        let inserted = sqlx::query_as::<_, Row>(concat!(
            "INSERT INTO poll_templates \
             (id, guild_id, name, question, answers, allow_multiselect, duration_hours, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING ",
            cols!()
        ))
        .bind(self.next_id())
        .bind(guild)
        .bind(&t.name)
        .bind(&t.question)
        .bind(serde_json::json!(t.answers))
        .bind(t.allow_multiselect)
        .bind(t.duration_hours.unwrap_or(24))
        .bind(by)
        .fetch_one(&self.pool)
        .await;
        match inserted {
            Ok(r) => Ok(r.into()),
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(UfError::Conflict(
                "A template with that name already exists.".into(),
            )),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn update_poll_template(
        &self,
        guild: i64,
        id: i64,
        input: &PollTemplateInput,
    ) -> UfResult<PollTemplate> {
        self.require(POLL_TEMPLATES_ID, Some(guild)).await?;
        let t = check(input)?;
        let updated = sqlx::query_as::<_, Row>(concat!(
            "UPDATE poll_templates SET name = $3, question = $4, answers = $5, \
             allow_multiselect = $6, duration_hours = $7 \
             WHERE guild_id = $1 AND id = $2 RETURNING ",
            cols!()
        ))
        .bind(guild)
        .bind(id)
        .bind(&t.name)
        .bind(&t.question)
        .bind(serde_json::json!(t.answers))
        .bind(t.allow_multiselect)
        .bind(t.duration_hours.unwrap_or(24))
        .fetch_optional(&self.pool)
        .await;
        match updated {
            Ok(Some(r)) => Ok(r.into()),
            Ok(None) => Err(UfError::NotFound("poll template")),
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(UfError::Conflict(
                "A template with that name already exists.".into(),
            )),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn delete_poll_template(&self, guild: i64, id: i64) -> UfResult<()> {
        let n = sqlx::query("DELETE FROM poll_templates WHERE guild_id = $1 AND id = $2")
            .bind(guild)
            .bind(id)
            .execute(&self.pool)
            .await?
            .rows_affected();
        if n == 1 {
            Ok(())
        } else {
            Err(UfError::NotFound("poll template"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> PollTemplateInput {
        PollTemplateInput {
            name: " Movie night ".into(),
            question: " What do we watch? ".into(),
            answers: vec!["Akira ".into(), "Paprika".into()],
            allow_multiselect: false,
            duration_hours: None,
        }
    }

    #[test]
    fn trims_and_defaults() {
        let t = check(&input()).unwrap();
        assert_eq!(t.name, "Movie night");
        assert_eq!(t.question, "What do we watch?");
        assert_eq!(t.answers, vec!["Akira", "Paprika"]);
        assert_eq!(t.duration_hours, Some(24));
    }

    #[test]
    fn rejects_bad_shapes() {
        let mut i = input();
        i.answers = vec!["only one".into()];
        assert!(check(&i).is_err());
        i.answers = (0..11).map(|n| format!("a{n}")).collect();
        assert!(check(&i).is_err());
        i.answers = vec!["A".into(), "a".into()];
        assert!(check(&i).is_err(), "answers differ only by case");
        i.answers = vec!["A".into(), "x".repeat(56)];
        assert!(check(&i).is_err());
        i.answers = vec!["A".into(), " ".into()];
        assert!(check(&i).is_err());
        let mut j = input();
        j.duration_hours = Some(0);
        assert!(check(&j).is_err());
        j.duration_hours = Some(769);
        assert!(check(&j).is_err());
        j.duration_hours = Some(768);
        assert!(check(&j).is_ok());
        j.name = " ".into();
        assert!(check(&j).is_err());
    }
}
