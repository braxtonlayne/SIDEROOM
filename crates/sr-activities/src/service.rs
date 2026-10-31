//! The Activities backend over Postgres and object storage.

use std::collections::HashSet;
use std::sync::Arc;

use bytes::Bytes;
use futures::{StreamExt, TryStreamExt, stream};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};
use sr_storage::{ObjectStorage, StorageError};

use crate::bundle;
use crate::error::{ActivityError, ActivityResult};
use crate::model::{
    BundleDetail, BundleEntry, BundleState, BundleView, ConfigInput, ConfigView, InstanceView,
    ParticipantView, PendingView, ReviewState, ShelfItem, UrlMapping,
};
use crate::ports::{
    ActivityEvents, ChannelAccess, ChannelPerms, EVENT_INSTANCE_UPDATE, NoChannels, NoEvents,
};
use crate::rules;

/// Source of snowflake ids.
pub type IdSource = Arc<dyn Fn() -> i64 + Send + Sync>;

/// Largest guild a development build may be started in (the tester-guild size of D-204).
const DEV_GUILD_MAX_MEMBERS: i64 = 25;
/// Bundles listed in the review history.
const HISTORY: i64 = 20;
/// Objects written at once while unpacking a bundle.
const PUT_CONCURRENCY: usize = 8;

/// A timestamp column as epoch milliseconds.
fn ms(col: &str) -> String {
    format!("(EXTRACT(EPOCH FROM {col}) * 1000)::bigint")
}

/// Object key of one file of a bundle.
pub fn object_key(app: i64, sha256: &str, path: &str) -> String {
    format!("activities/{app}/{sha256}/{path}")
}

/// Object key of the zip as uploaded (for the reviewer's download).
pub fn zip_key(app: i64, sha256: &str) -> String {
    format!("activities/{app}/{sha256}.zip")
}

/// What the proxy serves for an application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Serving {
    /// The reviewed bundle with this SHA-256.
    Bundle { sha256: String },
    /// The live development URL (development applications only).
    Dev { url: String },
}

/// Everything the proxy needs for one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServeSpec {
    pub serving: Serving,
    pub url_mappings: Vec<UrlMapping>,
}

/// The proxy's view of an application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    /// No Activity here, or the application is suspended.
    Unknown,
    /// An Activity that has nothing to serve right now (not reviewed again, or not released).
    Paused,
    Ready(ServeSpec),
}

struct Inner {
    pool: PgPool,
    ids: IdSource,
    channels: Arc<dyn ChannelAccess>,
    events: Arc<dyn ActivityEvents>,
    storage: Option<Arc<dyn ObjectStorage>>,
    /// `platform.activities_datachannel` (OPS-FLG-001): off, no instance runs with the `datachannel` capability.
    datachannel_on: std::sync::atomic::AtomicBool,
}

/// The Activities backend. Cheap to clone.
#[derive(Clone)]
pub struct ActivityService {
    inner: Arc<Inner>,
}

fn bundle_of(r: &PgRow) -> Result<BundleView, sqlx::Error> {
    let state: String = r.try_get("state")?;
    Ok(BundleView {
        id: r.try_get::<i64, _>("id")?.to_string(),
        sha256: r.try_get("sha256")?,
        byte_size: r.try_get("byte_size")?,
        entry_count: r.try_get("entry_count")?,
        state: BundleState::parse(&state),
        uploaded_at_ms: r.try_get("uploaded_ms")?,
        reviewed_at_ms: r.try_get("reviewed_ms")?,
        note: r.try_get("note")?,
        capabilities: r.try_get("capabilities")?,
    })
}

fn bundle_columns() -> String {
    format!(
        "b.id, b.sha256, b.byte_size, b.entry_count, b.state, b.note, b.capabilities, {} AS uploaded_ms, \
         {} AS reviewed_ms",
        ms("b.uploaded_at"),
        ms("b.reviewed_at")
    )
}

impl ActivityService {
    pub fn new(pool: PgPool, ids: IdSource) -> Self {
        Self {
            inner: Arc::new(Inner {
                pool,
                ids,
                channels: Arc::new(NoChannels),
                events: Arc::new(NoEvents),
                storage: None,
                datachannel_on: std::sync::atomic::AtomicBool::new(true),
            }),
        }
    }

    /// Follow the platform switch `platform.activities_datachannel`: off, an instance is shown and started without
    /// the `datachannel` capability, so its host never asks the call for a data channel. Takes effect at once.
    pub fn set_datachannel_allowed(&self, on: bool) {
        self.inner
            .datachannel_on
            .store(on, std::sync::atomic::Ordering::Relaxed);
    }

    /// Whether instances may carry the `datachannel` capability now.
    pub fn datachannel_allowed(&self) -> bool {
        self.inner
            .datachannel_on
            .load(std::sync::atomic::Ordering::Relaxed)
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

    /// Where bundles are stored. Without it uploads are refused. Call before the service is shared.
    #[must_use]
    pub fn with_storage(mut self, s: Arc<dyn ObjectStorage>) -> Self {
        if let Some(i) = Arc::get_mut(&mut self.inner) {
            i.storage = Some(s);
        }
        self
    }

    fn pool(&self) -> &PgPool {
        &self.inner.pool
    }

    fn storage(&self) -> ActivityResult<&Arc<dyn ObjectStorage>> {
        self.inner.storage.as_ref().ok_or_else(|| {
            ActivityError::Conflict("Bundle storage is not available on this server.".to_owned())
        })
    }

    // ------------------------------------------------------------------ settings

    async fn bundles_of(&self, app: i64) -> ActivityResult<Vec<BundleView>> {
        let sql = format!(
            "SELECT {} FROM application_activity_bundles b WHERE b.application_id = $1 \
             ORDER BY b.uploaded_at DESC, b.id DESC LIMIT {HISTORY}",
            bundle_columns()
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(app)
            .fetch_all(self.pool())
            .await?;
        rows.iter()
            .map(bundle_of)
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }

    /// The settings of an application (an empty draft when none were saved).
    pub async fn config(&self, app: i64) -> ActivityResult<ConfigView> {
        let sql = format!(
            "SELECT c.url_mappings, c.dev_entry_url, c.review_state, c.review_note, \
             c.reviewed_bundle_id, {} AS updated_ms FROM application_activities c \
             WHERE c.application_id = $1",
            ms("c.updated_at")
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(app)
            .fetch_optional(self.pool())
            .await?;
        let bundles = self.bundles_of(app).await?;
        let Some(r) = row else {
            return Ok(ConfigView {
                application_id: app.to_string(),
                dev_entry_url: None,
                url_mappings: Vec::new(),
                review_state: ReviewState::Draft,
                reviewed_bundle: None,
                bundles,
                review_note: None,
                updated_at_ms: 0,
            });
        };
        let mappings: Value = r.try_get("url_mappings")?;
        let state: String = r.try_get("review_state")?;
        let reviewed: Option<i64> = r.try_get("reviewed_bundle_id")?;
        let reviewed_bundle = reviewed.and_then(|id| {
            let id = id.to_string();
            bundles.iter().find(|b| b.id == id).cloned()
        });
        Ok(ConfigView {
            application_id: app.to_string(),
            dev_entry_url: r.try_get("dev_entry_url")?,
            url_mappings: serde_json::from_value(mappings).unwrap_or_default(),
            review_state: ReviewState::parse(&state),
            reviewed_bundle,
            bundles,
            review_note: r.try_get("review_note")?,
            updated_at_ms: r.try_get("updated_ms")?,
        })
    }

    /// Whether the application has an Activity row (settings saved or a bundle uploaded). Such an
    /// application has the implicit redirect URI of its Activity origin (J-13).
    pub async fn has_activity(&self, app: i64) -> ActivityResult<bool> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM application_activities WHERE application_id = $1)",
        )
        .bind(app)
        .fetch_one(self.pool())
        .await?)
    }

    /// Recompute the summary state of an application from its bundles.
    async fn refresh_state(tx: &mut Transaction<'_, Postgres>, app: i64) -> ActivityResult<()> {
        sqlx::query(
            "UPDATE application_activities c SET updated_at = now(), review_state = CASE \
               WHEN EXISTS (SELECT 1 FROM application_activity_bundles b \
                            WHERE b.application_id = c.application_id AND b.state = 'pending') THEN 'pending' \
               WHEN c.reviewed_bundle_id IS NOT NULL THEN 'approved' \
               WHEN EXISTS (SELECT 1 FROM application_activity_bundles b \
                            WHERE b.application_id = c.application_id AND b.state = 'rejected') THEN 'rejected' \
               ELSE 'draft' END \
             WHERE c.application_id = $1",
        )
        .bind(app)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Create the settings row if the application has none, and lock it for the rest of the transaction.
    async fn lock_row(tx: &mut Transaction<'_, Postgres>, app: i64) -> ActivityResult<()> {
        sqlx::query(
            "INSERT INTO application_activities (application_id) VALUES ($1) ON CONFLICT DO NOTHING",
        )
        .bind(app)
        .execute(&mut **tx)
        .await?;
        sqlx::query("SELECT 1 FROM application_activities WHERE application_id = $1 FOR UPDATE")
            .bind(app)
            .fetch_one(&mut **tx)
            .await?;
        Ok(())
    }

    /// Save the development URL and the URL mappings. A change to the mappings withdraws the approval of the
    /// reviewed bundle (the Activity is paused until it is reviewed again). The development URL is never
    /// reviewed and is accepted only for an application in development. Saving the same values changes nothing.
    pub async fn save_config(&self, app: i64, input: ConfigInput) -> ActivityResult<ConfigView> {
        let dev = input
            .dev_entry_url
            .map(|u| u.trim().to_owned())
            .filter(|u| !u.is_empty());
        if let Some(p) = dev.as_deref().and_then(rules::https_url_problem) {
            return Err(ActivityError::Invalid("dev_entry_url", p.to_owned()));
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
        if current.dev_entry_url == dev && current.url_mappings == mappings {
            return Ok(current);
        }
        if dev.is_some() && current.dev_entry_url != dev {
            let state: Option<String> =
                sqlx::query_scalar("SELECT release_state::text FROM applications WHERE id = $1")
                    .bind(app)
                    .fetch_optional(self.pool())
                    .await?;
            if state.as_deref() != Some("development") {
                return Err(ActivityError::Invalid(
                    "dev_entry_url",
                    "Only an application in development can use a development URL.".to_owned(),
                ));
            }
        }
        let mut tx = self.pool().begin().await?;
        Self::lock_row(&mut tx, app).await?;
        sqlx::query(
            "UPDATE application_activities SET dev_entry_url = $2, url_mappings = $3 \
             WHERE application_id = $1",
        )
        .bind(app)
        .bind(&dev)
        .bind(json!(mappings))
        .execute(&mut *tx)
        .await?;
        if current.url_mappings != mappings {
            // The mappings are part of what staff approved: withdraw the approval.
            sqlx::query(
                "UPDATE application_activity_bundles SET state = 'superseded' \
                 WHERE application_id = $1 AND state = 'approved'",
            )
            .bind(app)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE application_activities SET reviewed_bundle_id = NULL, review_note = NULL \
                 WHERE application_id = $1",
            )
            .bind(app)
            .execute(&mut *tx)
            .await?;
        }
        Self::refresh_state(&mut tx, app).await?;
        tx.commit().await?;
        self.config(app).await
    }

    // ------------------------------------------------------------------ bundles

    /// Store an uploaded zip as a new `draft` bundle. The zip is validated first (see [`bundle`]); every
    /// earlier draft or rejected bundle is superseded. Uploading the same zip again returns the existing
    /// bundle. The row is written before the objects, so the collector never deletes files of a live bundle.
    pub async fn upload_bundle(
        &self,
        app: i64,
        user: i64,
        zip: Vec<u8>,
    ) -> ActivityResult<BundleDetail> {
        let storage = self.storage()?.clone();
        let zip = Bytes::from(zip);
        let zip_for_store = zip.clone();
        let unpacked = tokio::task::spawn_blocking(move || bundle::unpack(&zip))
            .await
            .map_err(|e| ActivityError::Storage(e.to_string()))?
            .map_err(|e| ActivityError::Invalid("bundle", e.text()))?;
        let total = unpacked.total;
        let manifest: Vec<BundleEntry> = unpacked
            .files
            .iter()
            .map(|f| BundleEntry {
                path: f.path.clone(),
                size: i64::try_from(f.data.len()).unwrap_or(i64::MAX),
                content_type: f.content_type.to_owned(),
            })
            .collect();
        let sha = unpacked.sha256.clone();

        let mut tx = self.pool().begin().await?;
        Self::lock_row(&mut tx, app).await?;
        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM application_activity_bundles \
             WHERE application_id = $1 AND sha256 = $2 AND state <> 'superseded'",
        )
        .bind(app)
        .bind(&sha)
        .fetch_optional(&mut *tx)
        .await?;
        let (id, fresh) = match existing {
            Some(id) => (id, false),
            None => {
                sqlx::query(
                    "UPDATE application_activity_bundles SET state = 'superseded' \
                     WHERE application_id = $1 AND state IN ('draft', 'rejected')",
                )
                .bind(app)
                .execute(&mut *tx)
                .await?;
                let id = (self.inner.ids)();
                sqlx::query(
                    "INSERT INTO application_activity_bundles \
                     (id, application_id, sha256, byte_size, entry_count, manifest, uploaded_by, \
                      capabilities) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                )
                .bind(id)
                .bind(app)
                .bind(&sha)
                .bind(i64::try_from(total).unwrap_or(i64::MAX))
                .bind(i32::try_from(unpacked.entries).unwrap_or(i32::MAX))
                .bind(json!(manifest))
                .bind(user)
                .bind(&unpacked.capabilities)
                .execute(&mut *tx)
                .await?;
                (id, true)
            }
        };
        Self::refresh_state(&mut tx, app).await?;
        tx.commit().await?;

        // Objects second; a repeated upload writes them again, which repairs a half-written bundle.
        let mut puts: Vec<(String, Bytes)> = unpacked
            .files
            .into_iter()
            .map(|f| (object_key(app, &sha, &f.path), Bytes::from(f.data)))
            .collect();
        puts.push((zip_key(app, &sha), zip_for_store));
        let written = stream::iter(puts.into_iter().map(|(key, data)| {
            let storage = storage.clone();
            async move { storage.put(&key, data).await }
        }))
        .buffer_unordered(PUT_CONCURRENCY)
        .try_collect::<Vec<()>>()
        .await;
        if let Err(e) = written {
            if fresh {
                let _ = sqlx::query(
                    "UPDATE application_activity_bundles SET state = 'superseded' WHERE id = $1",
                )
                .bind(id)
                .execute(self.pool())
                .await;
            }
            return Err(ActivityError::Storage(e.to_string()));
        }
        self.bundle_detail(app, id).await
    }

    /// A bundle with its file listing.
    pub async fn bundle_detail(&self, app: i64, id: i64) -> ActivityResult<BundleDetail> {
        let sql = format!(
            "SELECT {}, b.manifest FROM application_activity_bundles b \
             WHERE b.application_id = $1 AND b.id = $2",
            bundle_columns()
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(app)
            .bind(id)
            .fetch_optional(self.pool())
            .await?
            .ok_or(ActivityError::NotFound("bundle"))?;
        let manifest: Value = row.try_get("manifest")?;
        Ok(BundleDetail {
            bundle: bundle_of(&row)?,
            entries: serde_json::from_value(manifest).unwrap_or_default(),
        })
    }

    /// The zip of a bundle as uploaded, for the reviewer: `(sha256, bytes)`.
    pub async fn bundle_zip(&self, app: i64, id: i64) -> ActivityResult<(String, Bytes)> {
        let storage = self.storage()?.clone();
        let detail = self.bundle_detail(app, id).await?;
        if detail.bundle.state == BundleState::Superseded {
            return Err(ActivityError::NotFound("bundle"));
        }
        let key = zip_key(app, &detail.bundle.sha256);
        let data = match storage.get(&key).await {
            Ok(d) => d,
            Err(StorageError::NotFound(_)) => return Err(ActivityError::NotFound("bundle")),
            Err(e) => return Err(ActivityError::Storage(e.to_string())),
        };
        Ok((detail.bundle.sha256, data))
    }

    /// One file of a bundle, or `None` when the bundle has no such file.
    pub async fn bundle_file(
        &self,
        app: i64,
        sha256: &str,
        path: &str,
    ) -> ActivityResult<Option<Bytes>> {
        let storage = self.storage()?;
        match storage.get(&object_key(app, sha256, path)).await {
            Ok(d) => Ok(Some(d)),
            Err(StorageError::NotFound(_)) => Ok(None),
            Err(e) => Err(ActivityError::Storage(e.to_string())),
        }
    }

    /// Submit an uploaded draft for review. An earlier pending bundle is superseded; an approved one keeps
    /// being served until staff decide.
    pub async fn submit(&self, app: i64, bundle_id: i64) -> ActivityResult<ConfigView> {
        let mut tx = self.pool().begin().await?;
        Self::lock_row(&mut tx, app).await?;
        let state: Option<String> = sqlx::query_scalar(
            "SELECT state FROM application_activity_bundles \
             WHERE id = $1 AND application_id = $2 FOR UPDATE",
        )
        .bind(bundle_id)
        .bind(app)
        .fetch_optional(&mut *tx)
        .await?;
        match state.as_deref().map(BundleState::parse) {
            None => return Err(ActivityError::NotFound("bundle")),
            Some(BundleState::Draft) => {}
            Some(_) => {
                return Err(ActivityError::Conflict(
                    "Only an uploaded draft can be submitted. Upload the bundle again to change it."
                        .to_owned(),
                ));
            }
        }
        sqlx::query(
            "UPDATE application_activity_bundles SET state = 'superseded' \
             WHERE application_id = $1 AND state = 'pending'",
        )
        .bind(app)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE application_activity_bundles SET state = 'pending', note = NULL WHERE id = $1",
        )
        .bind(bundle_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE application_activities SET review_note = NULL WHERE application_id = $1",
        )
        .bind(app)
        .execute(&mut *tx)
        .await?;
        Self::refresh_state(&mut tx, app).await?;
        tx.commit().await?;
        self.config(app).await
    }

    /// Staff: the submitted Activities, oldest first.
    pub async fn pending(&self, limit: i64) -> ActivityResult<Vec<PendingView>> {
        let rows = sqlx::query(
            "SELECT b.application_id, b.id, a.name AS app_name, a.owner_id AS app_owner \
             FROM application_activity_bundles b JOIN applications a ON a.id = b.application_id \
             WHERE b.state = 'pending' ORDER BY b.uploaded_at, b.id LIMIT $1",
        )
        .bind(limit.clamp(1, 100))
        .fetch_all(self.pool())
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let app: i64 = r.try_get("application_id")?;
            let id: i64 = r.try_get("id")?;
            out.push(PendingView {
                application_name: r.try_get("app_name")?,
                owner_id: r.try_get::<i64, _>("app_owner")?.to_string(),
                config: self.config(app).await?,
                bundle: self.bundle_detail(app, id).await?,
            });
        }
        Ok(out)
    }

    /// Staff: approve or reject the pending bundle `bundle_id`. Approving makes it the reviewed bundle (the
    /// previously approved one is superseded); the proxy then serves it once the application is released.
    pub async fn decide(
        &self,
        app: i64,
        staff: i64,
        bundle_id: i64,
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
        // Two people per release, as for application versions: nobody reviews their own Activity.
        let owner: Option<i64> =
            sqlx::query_scalar("SELECT owner_id FROM applications WHERE id = $1")
                .bind(app)
                .fetch_optional(self.pool())
                .await?;
        if owner == Some(staff) {
            return Err(ActivityError::Forbidden);
        }
        let mut tx = self.pool().begin().await?;
        Self::lock_row(&mut tx, app).await?;
        let moved = sqlx::query(
            "UPDATE application_activity_bundles SET state = $3, reviewed_by = $4, \
             reviewed_at = now(), note = $5 \
             WHERE id = $1 AND application_id = $2 AND state = 'pending'",
        )
        .bind(bundle_id)
        .bind(app)
        .bind(if approve { "approved" } else { "rejected" })
        .bind(staff)
        .bind(note)
        .execute(&mut *tx)
        .await?;
        if moved.rows_affected() == 0 {
            return Err(ActivityError::Conflict(
                "Only the submitted bundle can be decided.".to_owned(),
            ));
        }
        if approve {
            sqlx::query(
                "UPDATE application_activity_bundles SET state = 'superseded' \
                 WHERE application_id = $1 AND state = 'approved' AND id <> $2",
            )
            .bind(app)
            .bind(bundle_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE application_activities SET reviewed_bundle_id = $2, review_note = $3 \
                 WHERE application_id = $1",
            )
            .bind(app)
            .bind(bundle_id)
            .bind(note)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "UPDATE application_activities SET review_note = $2 WHERE application_id = $1",
            )
            .bind(app)
            .bind(note)
            .execute(&mut *tx)
            .await?;
        }
        Self::refresh_state(&mut tx, app).await?;
        tx.commit().await?;
        self.config(app).await
    }

    /// What the proxy serves for `app` right now (D-252). An application released at least once and not
    /// suspended (`released`, or `in_review` with a prior release) serves its reviewed bundle; an
    /// application in development serves its live development URL; a suspended one nothing.
    pub async fn lookup(&self, app: i64) -> ActivityResult<Lookup> {
        let row = sqlx::query(
            "SELECT c.url_mappings, c.dev_entry_url, b.sha256, a.release_state::text AS state, \
               (a.release_state::text = 'released' OR (a.release_state::text = 'in_review' AND EXISTS \
                 (SELECT 1 FROM application_versions v WHERE v.application_id = a.id AND v.state = 'approved'))) \
                 AS released \
             FROM application_activities c JOIN applications a ON a.id = c.application_id \
             LEFT JOIN application_activity_bundles b \
               ON b.id = c.reviewed_bundle_id AND b.state = 'approved' \
             WHERE c.application_id = $1",
        )
        .bind(app)
        .fetch_optional(self.pool())
        .await?;
        let Some(r) = row else {
            return Ok(Lookup::Unknown);
        };
        let state: String = r.try_get("state")?;
        let released: bool = r.try_get("released")?;
        let sha: Option<String> = r.try_get("sha256")?;
        let dev: Option<String> = r.try_get("dev_entry_url")?;
        let mappings: Value = r.try_get("url_mappings")?;
        let spec = |serving| {
            Lookup::Ready(ServeSpec {
                serving,
                url_mappings: serde_json::from_value(mappings.clone()).unwrap_or_default(),
            })
        };
        Ok(match (state.as_str(), sha, dev) {
            ("suspended", _, _) => Lookup::Unknown,
            (_, Some(sha256), _) if released => spec(Serving::Bundle { sha256 }),
            ("development", _, Some(url)) => spec(Serving::Dev { url }),
            _ => Lookup::Paused,
        })
    }

    /// The launcher's shelf for `user`: released Activities with a reviewed bundle, and the development
    /// builds of applications the user owns or tests.
    pub async fn shelf(&self, user: i64) -> ActivityResult<Vec<ShelfItem>> {
        let rows = sqlx::query(
            "SELECT a.id, a.name, a.description, (a.release_state::text = 'development') AS dev \
             FROM application_activities c JOIN applications a ON a.id = c.application_id \
             WHERE ((a.release_state::text = 'released' OR (a.release_state::text = 'in_review' AND EXISTS \
                  (SELECT 1 FROM application_versions v WHERE v.application_id = a.id AND v.state = 'approved'))) \
                  AND c.reviewed_bundle_id IS NOT NULL) \
                OR (a.release_state::text = 'development' AND c.dev_entry_url IS NOT NULL \
                    AND (a.owner_id = $1 OR EXISTS (SELECT 1 FROM app_testers t \
                         WHERE t.application_id = a.id AND t.account_id = $1 AND t.state = 'accepted'))) \
             ORDER BY lower(a.name), a.id LIMIT 100",
        )
        .bind(user)
        .fetch_all(self.pool())
        .await?;
        rows.iter()
            .map(|r| {
                Ok(ShelfItem {
                    application_id: r.try_get::<i64, _>("id")?.to_string(),
                    name: r.try_get("name")?,
                    description: r.try_get("description")?,
                    development: r.try_get("dev")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }

    /// Whether `user` may start `app` in a channel of `guild` (D-246): a released application with a
    /// reviewed bundle for anybody, a development build only for its owner and testers in a small guild.
    async fn may_start(&self, user: i64, app: i64, guild: i64) -> ActivityResult<bool> {
        let row = sqlx::query(
            "SELECT a.owner_id, a.release_state::text AS state, c.reviewed_bundle_id, c.dev_entry_url, \
               EXISTS (SELECT 1 FROM application_versions v WHERE v.application_id = a.id \
                       AND v.state = 'approved') AS has_release, \
               EXISTS (SELECT 1 FROM app_testers t WHERE t.application_id = a.id \
                       AND t.account_id = $2 AND t.state = 'accepted') AS tester \
             FROM application_activities c JOIN applications a ON a.id = c.application_id \
             WHERE c.application_id = $1",
        )
        .bind(app)
        .bind(user)
        .fetch_optional(self.pool())
        .await?;
        let Some(r) = row else { return Ok(false) };
        let state: String = r.try_get("state")?;
        let reviewed: Option<i64> = r.try_get("reviewed_bundle_id")?;
        let dev: Option<String> = r.try_get("dev_entry_url")?;
        let has_release: bool = r.try_get("has_release")?;
        match state.as_str() {
            "released" => Ok(reviewed.is_some()),
            "in_review" => Ok(has_release && reviewed.is_some()),
            "development" if dev.is_some() => {
                let owner: i64 = r.try_get("owner_id")?;
                let tester: bool = r.try_get("tester")?;
                if owner != user && !tester {
                    return Ok(false);
                }
                let members: i64 =
                    sqlx::query_scalar("SELECT count(*) FROM guild_members WHERE guild_id = $1")
                        .bind(guild)
                        .fetch_one(self.pool())
                        .await?;
                Ok(members <= DEV_GUILD_MAX_MEMBERS)
            }
            _ => Ok(false),
        }
    }

    /// Delete the objects of bundles that are superseded (`activities.gc`). Returns the number of objects
    /// deleted.
    pub async fn gc(&self) -> ActivityResult<usize> {
        let storage = self.storage()?.clone();
        let live: HashSet<(i64, String)> = sqlx::query(
            "SELECT application_id, sha256 FROM application_activity_bundles WHERE state <> 'superseded'",
        )
        .fetch_all(self.pool())
        .await?
        .iter()
        .map(|r| Ok((r.try_get("application_id")?, r.try_get("sha256")?)))
        .collect::<Result<_, sqlx::Error>>()?;
        let objects = storage
            .list("activities/")
            .await
            .map_err(|e| ActivityError::Storage(e.to_string()))?;
        let mut removed = 0;
        for o in objects {
            let mut parts = o.key.strip_prefix("activities/").unwrap_or("").split('/');
            let (Some(app), Some(second)) = (parts.next(), parts.next()) else {
                continue;
            };
            let sha = second.strip_suffix(".zip").unwrap_or(second);
            let Ok(app) = app.parse::<i64>() else {
                continue;
            };
            if live.contains(&(app, sha.to_owned())) {
                continue;
            }
            storage
                .delete(&o.key)
                .await
                .map_err(|e| ActivityError::Storage(e.to_string()))?;
            removed += 1;
        }
        Ok(removed)
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

    /// The capabilities an instance row runs with: its reviewed bundle's, or every one in development.
    fn capabilities(r: &PgRow) -> ActivityResult<Vec<String>> {
        let development: bool = r.try_get("dev")?;
        Ok(if development {
            bundle::CAPABILITIES
                .iter()
                .map(|c| (*c).to_owned())
                .collect()
        } else {
            r.try_get::<Option<Vec<String>>, _>("caps")?
                .unwrap_or_default()
        })
    }

    async fn view(&self, r: &PgRow) -> ActivityResult<InstanceView> {
        let id: i64 = r.try_get("id")?;
        let development: bool = r.try_get("dev")?;
        let mut capabilities = Self::capabilities(r)?;
        if !self.datachannel_allowed() {
            capabilities.retain(|c| c != bundle::DATACHANNEL);
        }
        Ok(InstanceView {
            id: id.to_string(),
            application_id: r.try_get::<i64, _>("application_id")?.to_string(),
            application_name: r.try_get("name")?,
            guild_id: r.try_get::<i64, _>("guild_id")?.to_string(),
            channel_id: r.try_get::<i64, _>("channel_id")?.to_string(),
            channel_type: r.try_get("channel_type")?,
            development,
            started_by: r.try_get::<i64, _>("started_by")?.to_string(),
            created_at_ms: r.try_get("created_ms")?,
            participants: self.participants(id).await?,
            capabilities,
        })
    }

    fn instance_select() -> String {
        format!(
            "SELECT i.id, i.application_id, a.name, i.guild_id, i.channel_id, i.channel_type, \
             i.started_by, (a.release_state::text = 'development') AS dev, \
             (SELECT b.capabilities FROM application_activities c \
              JOIN application_activity_bundles b ON b.id = c.reviewed_bundle_id \
              WHERE c.application_id = i.application_id) AS caps, \
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

    /// The activity running in a channel, for anybody who can see the channel.
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

    /// Whether the permissions let a member take part: `CONNECT` and `USE_EMBEDDED_ACTIVITIES` in a voice
    /// channel, `USE_EMBEDDED_ACTIVITIES` in a text channel (`VIEW_CHANNEL` is checked before).
    fn may_take_part(p: &ChannelPerms) -> bool {
        p.use_activities && (p.connect || !p.voice)
    }

    /// Start an Activity in a voice or text channel of a guild. The starter joins it. One Activity runs per
    /// channel. The application must be released with a reviewed bundle, or be the caller's own development
    /// build.
    pub async fn launch(&self, user: i64, app: i64, channel: i64) -> ActivityResult<InstanceView> {
        let p = self.perms(user, channel).await?;
        if !p.view {
            return Err(ActivityError::NotFound("channel"));
        }
        if !p.voice && !p.text {
            return Err(ActivityError::Invalid(
                "channel_id",
                "Activities run in voice and text channels.".to_owned(),
            ));
        }
        if !Self::may_take_part(&p) {
            return Err(ActivityError::Forbidden);
        }
        if !self.may_start(user, app, p.guild_id).await? {
            return Err(ActivityError::NotFound("activity"));
        }
        let id = (self.inner.ids)();
        let mut tx = self.pool().begin().await?;
        let inserted = sqlx::query(
            "INSERT INTO activity_instances \
             (id, application_id, guild_id, channel_id, channel_type, started_by) \
             VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(app)
        .bind(p.guild_id)
        .bind(channel)
        .bind(if p.voice { "voice" } else { "text" })
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
        if !Self::may_take_part(&p) {
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

    /// Mark `user` as gone from `instance`; the last one out ends it. Announces the change.
    ///
    /// An instance that uses the data channel is host-authoritative: the member who launched it is the only
    /// writer of `host:*` keys and the only receiver of inputs, and there is no host handover in v1 (P36-ACT-1).
    /// So when its starter leaves (or leaves the call), the instance ends for everybody.
    async fn remove_participant(
        &self,
        row: &PgRow,
        instance: i64,
        user: i64,
    ) -> ActivityResult<()> {
        let channel: i64 = row.try_get("channel_id")?;
        let gone = sqlx::query(
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
        let starter: i64 = row.try_get("started_by")?;
        let host_left = gone.rows_affected() > 0
            && user == starter
            && Self::capabilities(row)?
                .iter()
                .any(|c| c == bundle::DATACHANNEL);
        if left == 0 || host_left {
            sqlx::query(
                "UPDATE activity_instances SET ended_at = now() WHERE id = $1 AND ended_at IS NULL",
            )
            .bind(instance)
            .execute(self.pool())
            .await?;
            self.announce(channel, None).await;
        } else if gone.rows_affected() > 0 {
            let view = self.view(row).await?;
            self.announce(channel, Some(&view)).await;
        }
        Ok(())
    }

    /// Leave an activity. The last participant leaving ends it.
    pub async fn leave(&self, user: i64, instance: i64) -> ActivityResult<()> {
        let row = self
            .running_by_id(instance)
            .await?
            .ok_or(ActivityError::NotFound("activity"))?;
        self.remove_participant(&row, instance, user).await
    }

    /// `user` is no longer in the voice channel `channel` (J-16, D-248): they leave the Activity running
    /// there. A text-channel Activity is not tied to a call. Does nothing when they were not in it.
    pub async fn voice_left(&self, user: i64, channel: i64) -> ActivityResult<()> {
        let sql = format!(
            "{} WHERE i.channel_id = $1 AND i.channel_type = 'voice' AND i.ended_at IS NULL",
            Self::instance_select()
        );
        let Some(row) = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(channel)
            .fetch_optional(self.pool())
            .await?
        else {
            return Ok(());
        };
        let instance: i64 = row.try_get("id")?;
        let was_in: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM activity_participants \
             WHERE instance_id = $1 AND user_id = $2 AND left_at IS NULL)",
        )
        .bind(instance)
        .bind(user)
        .fetch_one(self.pool())
        .await?;
        if !was_in {
            return Ok(());
        }
        self.remove_participant(&row, instance, user).await
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
