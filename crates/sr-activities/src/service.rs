//! The Activities backend over Postgres.

use std::sync::Arc;

use serde_json::{Value, json};
use sqlx::{PgPool, Row, postgres::PgRow};

use crate::error::{ActivityError, ActivityResult};
use crate::model::{
    ConfigInput, ConfigView, InstanceView, ParticipantView, PendingView, ReviewState, ShelfItem,
    UrlMapping,
};
use crate::ports::{
    ActivityEvents, ChannelAccess, ChannelPerms, EVENT_INSTANCE_UPDATE, NoChannels, NoEvents,
};
use crate::rules;

/// Source of snowflake ids.
pub type IdSource = Arc<dyn Fn() -> i64 + Send + Sync>;

/// A timestamp column as epoch milliseconds.
fn ms(col: &str) -> String {
    format!("(EXTRACT(EPOCH FROM {col}) * 1000)::bigint")
}

struct Inner {
    pool: PgPool,
    ids: IdSource,
    channels: Arc<dyn ChannelAccess>,
    events: Arc<dyn ActivityEvents>,
}

/// The Activities backend. Cheap to clone.
#[derive(Clone)]
pub struct ActivityService {
    inner: Arc<Inner>,
}

fn config_of(r: &PgRow) -> Result<ConfigView, sqlx::Error> {
    let mappings: Value = r.try_get("url_mappings")?;
    let state: String = r.try_get("review_state")?;
    Ok(ConfigView {
        application_id: r.try_get::<i64, _>("application_id")?.to_string(),
        entry_url: r.try_get("entry_url")?,
        url_mappings: serde_json::from_value(mappings).unwrap_or_default(),
        review_state: ReviewState::parse(&state),
        pinned_sha256: r.try_get("pinned_sha256")?,
        reviewed_sha256: r.try_get("reviewed_sha256")?,
        review_note: r.try_get("review_note")?,
        updated_at_ms: r.try_get("updated_ms")?,
    })
}

fn config_select() -> String {
    format!(
        "SELECT c.application_id, c.entry_url, c.url_mappings, c.review_state, c.pinned_sha256, \
         c.reviewed_sha256, c.review_note, {} AS updated_ms FROM application_activities c",
        ms("c.updated_at")
    )
}

/// A 64-character lowercase hex digest.
fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

impl ActivityService {
    pub fn new(pool: PgPool, ids: IdSource) -> Self {
        Self {
            inner: Arc::new(Inner {
                pool,
                ids,
                channels: Arc::new(NoChannels),
                events: Arc::new(NoEvents),
            }),
        }
    }

    /// Replace the channel permission source. Call before the service is shared.
    #[must_use]
    pub fn with_channels(mut self, c: Arc<dyn ChannelAccess>) -> Self {
        if let Some(i) = Arc::get_mut(&mut self.inner) {
            i.channels = c;
        }
        self
    }

    /// Replace the event sink. Call before the service is shared.
    #[must_use]
    pub fn with_events(mut self, e: Arc<dyn ActivityEvents>) -> Self {
        if let Some(i) = Arc::get_mut(&mut self.inner) {
            i.events = e;
        }
        self
    }

    fn pool(&self) -> &PgPool {
        &self.inner.pool
    }

    // ------------------------------------------------------------------ settings and review

    /// The settings of an application (an empty draft when none were saved).
    pub async fn config(&self, app: i64) -> ActivityResult<ConfigView> {
        let sql = format!("{} WHERE c.application_id = $1", config_select());
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(app)
            .fetch_optional(self.pool())
            .await?;
        match row {
            Some(r) => Ok(config_of(&r)?),
            None => Ok(ConfigView {
                application_id: app.to_string(),
                entry_url: None,
                url_mappings: Vec::new(),
                review_state: ReviewState::Draft,
                pinned_sha256: None,
                reviewed_sha256: None,
                review_note: None,
                updated_at_ms: 0,
            }),
        }
    }

    /// Save the entry URL and mappings. A change drops the review: the state returns to `draft` and the
    /// approved hash is cleared, so the proxy stops serving until the new version is approved. Saving the
    /// same values again changes nothing.
    pub async fn save_config(&self, app: i64, input: ConfigInput) -> ActivityResult<ConfigView> {
        let entry = input
            .entry_url
            .map(|u| u.trim().to_owned())
            .filter(|u| !u.is_empty());
        if let Some(p) = entry.as_deref().and_then(rules::https_url_problem) {
            return Err(ActivityError::Invalid("entry_url", p.to_owned()));
        }
        let mappings: Vec<UrlMapping> = input
            .url_mappings
            .into_iter()
            .map(|m| UrlMapping {
                prefix: m.prefix.trim().to_owned(),
                target: m.target.trim().to_owned(),
            })
            .collect();
        rules::validate_mappings(&mappings)
            .map_err(|m| ActivityError::Invalid("url_mappings", m))?;
        let current = self.config(app).await?;
        if current.entry_url == entry && current.url_mappings == mappings {
            return Ok(current);
        }
        sqlx::query(
            "INSERT INTO application_activities (application_id, entry_url, url_mappings) \
             VALUES ($1, $2, $3) \
             ON CONFLICT (application_id) DO UPDATE SET entry_url = $2, url_mappings = $3, \
             review_state = 'draft', pinned_sha256 = NULL, reviewed_sha256 = NULL, \
             reviewed_by = NULL, reviewed_at = NULL, review_note = NULL, updated_at = now()",
        )
        .bind(app)
        .bind(&entry)
        .bind(json!(mappings))
        .execute(self.pool())
        .await?;
        self.config(app).await
    }

    /// Submit for review with the SHA-256 (lowercase hex) of the entry document as the application served it.
    pub async fn submit(&self, app: i64, sha256: &str) -> ActivityResult<ConfigView> {
        if !is_sha256_hex(sha256) {
            return Err(ActivityError::Invalid(
                "sha256",
                "Not a SHA-256 digest.".to_owned(),
            ));
        }
        let r = sqlx::query(
            "UPDATE application_activities SET review_state = 'pending', pinned_sha256 = $2, \
             review_note = NULL, updated_at = now() \
             WHERE application_id = $1 AND entry_url IS NOT NULL \
             AND review_state IN ('draft', 'rejected', 'pending')",
        )
        .bind(app)
        .bind(sha256)
        .execute(self.pool())
        .await?;
        if r.rows_affected() == 0 {
            return Err(ActivityError::Conflict(
                "Set an entry URL before submitting. An approved Activity changes by editing it, which asks for a new review."
                    .to_owned(),
            ));
        }
        self.config(app).await
    }

    /// Staff: the submitted Activities, oldest first.
    pub async fn pending(&self, limit: i64) -> ActivityResult<Vec<PendingView>> {
        let sql = format!(
            "{}, applications a WHERE a.id = c.application_id AND c.review_state = 'pending' \
             ORDER BY c.updated_at, c.application_id LIMIT $1",
            config_select().replace(
                "c.review_note,",
                "c.review_note, a.name AS app_name, a.owner_id AS app_owner,"
            )
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(limit.clamp(1, 100))
            .fetch_all(self.pool())
            .await?;
        rows.iter()
            .map(|r| {
                Ok(PendingView {
                    application_name: r.try_get("app_name")?,
                    owner_id: r.try_get::<i64, _>("app_owner")?.to_string(),
                    config: config_of(r)?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }

    /// Staff: approve (the proxy then serves the submitted entry document) or reject a pending Activity.
    pub async fn decide(
        &self,
        app: i64,
        staff: i64,
        approve: bool,
        note: Option<&str>,
    ) -> ActivityResult<ConfigView> {
        let note = note.map(str::trim).filter(|n| !n.is_empty());
        if note.is_some_and(|n| n.chars().count() > 1000) {
            return Err(ActivityError::Invalid(
                "note",
                "Must be 1000 characters or fewer.".to_owned(),
            ));
        }
        let r = sqlx::query(
            "UPDATE application_activities SET review_state = $2, \
             reviewed_sha256 = CASE WHEN $2 = 'approved' THEN pinned_sha256 ELSE NULL END, \
             reviewed_by = $3, reviewed_at = now(), review_note = $4, updated_at = now() \
             WHERE application_id = $1 AND review_state = 'pending'",
        )
        .bind(app)
        .bind(if approve { "approved" } else { "rejected" })
        .bind(staff)
        .bind(note)
        .execute(self.pool())
        .await?;
        if r.rows_affected() == 0 {
            return Err(ActivityError::Conflict(
                "Only a submitted Activity can be decided.".to_owned(),
            ));
        }
        self.config(app).await
    }

    /// What the proxy needs: the approved settings of a running (not suspended) application.
    pub async fn approved(&self, app: i64) -> ActivityResult<Option<ConfigView>> {
        let sql = format!(
            "{} JOIN applications a ON a.id = c.application_id \
             WHERE c.application_id = $1 AND c.review_state = 'approved' \
             AND c.entry_url IS NOT NULL AND c.reviewed_sha256 IS NOT NULL \
             AND a.release_state::text <> 'suspended'",
            config_select()
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(app)
            .fetch_optional(self.pool())
            .await?;
        Ok(row.as_ref().map(config_of).transpose()?)
    }

    /// The launcher's shelf: approved Activities of applications that are not suspended.
    pub async fn shelf(&self) -> ActivityResult<Vec<ShelfItem>> {
        let rows = sqlx::query(
            "SELECT a.id, a.name, a.description FROM application_activities c \
             JOIN applications a ON a.id = c.application_id \
             WHERE c.review_state = 'approved' AND c.reviewed_sha256 IS NOT NULL \
             AND a.release_state::text <> 'suspended' ORDER BY lower(a.name), a.id LIMIT 100",
        )
        .fetch_all(self.pool())
        .await?;
        rows.iter()
            .map(|r| {
                Ok(ShelfItem {
                    application_id: r.try_get::<i64, _>("id")?.to_string(),
                    name: r.try_get("name")?,
                    description: r.try_get("description")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }

    // ------------------------------------------------------------------ instances

    async fn perms(&self, user: i64, channel: i64) -> ActivityResult<ChannelPerms> {
        self.inner
            .channels
            .perms(user, channel)
            .await
            .map_err(ActivityError::Storage)?
            .ok_or(ActivityError::NotFound("channel"))
    }

    async fn participants(&self, instance: i64) -> ActivityResult<Vec<ParticipantView>> {
        let sql = format!(
            "SELECT user_id, {} AS joined_ms FROM activity_participants \
             WHERE instance_id = $1 AND left_at IS NULL ORDER BY joined_at, user_id",
            ms("joined_at")
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(instance)
            .fetch_all(self.pool())
            .await?;
        rows.iter()
            .map(|r| {
                Ok(ParticipantView {
                    user_id: r.try_get::<i64, _>("user_id")?.to_string(),
                    joined_at_ms: r.try_get("joined_ms")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }

    async fn view(&self, r: &PgRow) -> ActivityResult<InstanceView> {
        let id: i64 = r.try_get("id")?;
        Ok(InstanceView {
            id: id.to_string(),
            application_id: r.try_get::<i64, _>("application_id")?.to_string(),
            application_name: r.try_get("name")?,
            guild_id: r.try_get::<i64, _>("guild_id")?.to_string(),
            channel_id: r.try_get::<i64, _>("channel_id")?.to_string(),
            started_by: r.try_get::<i64, _>("started_by")?.to_string(),
            created_at_ms: r.try_get("created_ms")?,
            participants: self.participants(id).await?,
        })
    }

    fn instance_select() -> String {
        format!(
            "SELECT i.id, i.application_id, a.name, i.guild_id, i.channel_id, i.started_by, \
             {} AS created_ms FROM activity_instances i \
             JOIN applications a ON a.id = i.application_id",
            ms("i.created_at")
        )
    }

    async fn running_by_id(&self, id: i64) -> ActivityResult<Option<PgRow>> {
        let sql = format!(
            "{} WHERE i.id = $1 AND i.ended_at IS NULL",
            Self::instance_select()
        );
        Ok(sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(self.pool())
            .await?)
    }

    async fn announce(&self, channel: i64, instance: Option<&InstanceView>) {
        let viewers = match self.inner.channels.viewers(channel).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "activity viewers lookup failed");
                return;
            }
        };
        let payload = json!({"channel_id": channel.to_string(), "instance": instance});
        self.inner
            .events
            .emit(viewers, EVENT_INSTANCE_UPDATE, payload)
            .await;
    }

    /// The activity running in a voice channel, for anybody who can see the channel.
    pub async fn in_channel(
        &self,
        user: i64,
        channel: i64,
    ) -> ActivityResult<Option<InstanceView>> {
        let p = self.perms(user, channel).await?;
        if !p.view {
            return Err(ActivityError::NotFound("channel"));
        }
        let sql = format!(
            "{} WHERE i.channel_id = $1 AND i.ended_at IS NULL",
            Self::instance_select()
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(channel)
            .fetch_optional(self.pool())
            .await?;
        match row {
            Some(r) => Ok(Some(self.view(&r).await?)),
            None => Ok(None),
        }
    }

    /// One running instance, for anybody who can see its channel.
    pub async fn get(&self, user: i64, instance: i64) -> ActivityResult<InstanceView> {
        let row = self
            .running_by_id(instance)
            .await?
            .ok_or(ActivityError::NotFound("activity"))?;
        let view = self.view(&row).await?;
        let channel: i64 = row.try_get("channel_id")?;
        match self.inner.channels.perms(user, channel).await {
            Ok(Some(p)) if p.view => Ok(view),
            Ok(_) => Err(ActivityError::NotFound("activity")),
            Err(e) => Err(ActivityError::Storage(e)),
        }
    }

    /// Start an approved Activity in a voice channel. The starter joins it. Needs `CONNECT` and
    /// `USE_EMBEDDED_ACTIVITIES` in the channel; one activity runs per channel.
    pub async fn launch(&self, user: i64, app: i64, channel: i64) -> ActivityResult<InstanceView> {
        let p = self.perms(user, channel).await?;
        if !p.view {
            return Err(ActivityError::NotFound("channel"));
        }
        if !p.voice {
            return Err(ActivityError::Invalid(
                "channel_id",
                "Activities run in voice channels.".to_owned(),
            ));
        }
        if !(p.connect && p.use_activities) {
            return Err(ActivityError::Forbidden);
        }
        if self.approved(app).await?.is_none() {
            return Err(ActivityError::NotFound("activity"));
        }
        let id = (self.inner.ids)();
        let mut tx = self.pool().begin().await?;
        let inserted = sqlx::query(
            "INSERT INTO activity_instances (id, application_id, guild_id, channel_id, started_by) \
             VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(app)
        .bind(p.guild_id)
        .bind(channel)
        .bind(user)
        .execute(&mut *tx)
        .await?;
        if inserted.rows_affected() == 0 {
            return Err(ActivityError::Conflict(
                "An activity is already running in this channel.".to_owned(),
            ));
        }
        sqlx::query("INSERT INTO activity_participants (instance_id, user_id) VALUES ($1, $2)")
            .bind(id)
            .bind(user)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        let row = self
            .running_by_id(id)
            .await?
            .ok_or(ActivityError::NotFound("activity"))?;
        let view = self.view(&row).await?;
        self.announce(channel, Some(&view)).await;
        Ok(view)
    }

    /// Join (or rejoin) a running activity. Needs the same channel permissions as launching.
    pub async fn join(&self, user: i64, instance: i64) -> ActivityResult<InstanceView> {
        let row = self
            .running_by_id(instance)
            .await?
            .ok_or(ActivityError::NotFound("activity"))?;
        let channel: i64 = row.try_get("channel_id")?;
        let p = self.perms(user, channel).await?;
        if !p.view {
            return Err(ActivityError::NotFound("activity"));
        }
        if !(p.connect && p.use_activities) {
            return Err(ActivityError::Forbidden);
        }
        sqlx::query(
            "INSERT INTO activity_participants (instance_id, user_id) VALUES ($1, $2) \
             ON CONFLICT (instance_id, user_id) DO UPDATE SET left_at = NULL, joined_at = now() \
             WHERE activity_participants.left_at IS NOT NULL",
        )
        .bind(instance)
        .bind(user)
        .execute(self.pool())
        .await?;
        let view = self.view(&row).await?;
        self.announce(channel, Some(&view)).await;
        Ok(view)
    }

    /// Leave an activity. The last participant leaving ends it.
    pub async fn leave(&self, user: i64, instance: i64) -> ActivityResult<()> {
        let row = self
            .running_by_id(instance)
            .await?
            .ok_or(ActivityError::NotFound("activity"))?;
        let channel: i64 = row.try_get("channel_id")?;
        sqlx::query(
            "UPDATE activity_participants SET left_at = now() \
             WHERE instance_id = $1 AND user_id = $2 AND left_at IS NULL",
        )
        .bind(instance)
        .bind(user)
        .execute(self.pool())
        .await?;
        let left: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM activity_participants WHERE instance_id = $1 AND left_at IS NULL",
        )
        .bind(instance)
        .fetch_one(self.pool())
        .await?;
        if left == 0 {
            sqlx::query(
                "UPDATE activity_instances SET ended_at = now() WHERE id = $1 AND ended_at IS NULL",
            )
            .bind(instance)
            .execute(self.pool())
            .await?;
            self.announce(channel, None).await;
        } else {
            let view = self.view(&row).await?;
            self.announce(channel, Some(&view)).await;
        }
        Ok(())
    }

    /// End an activity for everybody: its starter, or anybody who manages the channel.
    pub async fn end(&self, user: i64, instance: i64) -> ActivityResult<()> {
        let row = self
            .running_by_id(instance)
            .await?
            .ok_or(ActivityError::NotFound("activity"))?;
        let channel: i64 = row.try_get("channel_id")?;
        let starter: i64 = row.try_get("started_by")?;
        if starter != user {
            let p = self.perms(user, channel).await?;
            if !p.view {
                return Err(ActivityError::NotFound("activity"));
            }
            if !p.manage {
                return Err(ActivityError::Forbidden);
            }
        }
        sqlx::query(
            "UPDATE activity_instances SET ended_at = now() WHERE id = $1 AND ended_at IS NULL",
        )
        .bind(instance)
        .execute(self.pool())
        .await?;
        self.announce(channel, None).await;
        Ok(())
    }
}
