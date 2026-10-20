//! Packages, reviews, enablements, profile documents and the guestbook over Postgres and object storage.

use std::sync::Arc;

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row, postgres::PgRow};
use sr_storage::{ObjectStorage, StorageError};
use utoipa::ToSchema;

pub mod media;

use crate::error::{PackageError, PackageResult};
use crate::profile_doc::{self, ProfileDoc};
use crate::validate::{self, Checks, FileInfo, PackageKind};
use crate::zipfiles;

/// Source of snowflake ids.
pub type IdSource = Arc<dyn Fn() -> i64 + Send + Sync>;

/// Packages one person may own, and versions waiting for review at once.
pub const MAX_PACKAGES_PER_AUTHOR: i64 = 50;
pub const MAX_PENDING_PER_AUTHOR: i64 = 10;
/// Most entries on one guestbook, and entries one person may post per minute.
pub const MAX_GUESTBOOK_ENTRIES: i64 = 500;
pub const GUESTBOOK_PER_MINUTE: i64 = 5;
/// Page size of the directory and the guestbook.
pub const PAGE: i64 = 30;

fn ms(col: &str) -> String {
    format!("(EXTRACT(EPOCH FROM {col}) * 1000)::bigint")
}

/// Object key of a version's zip.
pub fn artifact_key(package: i64, version: i64) -> String {
    format!("packages/{package}/{version}.zip")
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct VersionView {
    pub id: String,
    pub package_id: String,
    pub kind: PackageKind,
    pub slug: String,
    pub name: String,
    pub version: String,
    /// `pending`, `approved`, `rejected`, `withdrawn` or `revoked`.
    pub review_state: String,
    pub review_note: Option<String>,
    pub size_bytes: i64,
    pub created_at_ms: i64,
    pub files: Vec<FileInfo>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PackageView {
    pub id: String,
    pub kind: PackageKind,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub author_id: String,
    pub author_name: String,
    /// The newest approved version.
    pub version_id: String,
    pub version: String,
    pub size_bytes: i64,
    pub updated_at_ms: i64,
    /// What the kind needs at runtime (theme tokens and CSS, layout document, pack items, recipe
    /// entries). Left out of listings for server templates.
    #[schema(value_type = Option<Object>)]
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MyPackage {
    pub id: String,
    pub kind: PackageKind,
    pub slug: String,
    pub name: String,
    pub versions: Vec<VersionView>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PendingVersion {
    #[serde(flatten)]
    pub version: VersionView,
    pub author_id: String,
    pub author_name: String,
    #[schema(value_type = Object)]
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[schema(as = PackageEnablement)]
pub struct Enablement {
    pub package_id: String,
    pub version_id: String,
    pub kind: PackageKind,
    pub slug: String,
    pub name: String,
    pub version: String,
    pub section: String,
    #[schema(value_type = Object)]
    pub config: Value,
    pub enabled_at_ms: i64,
    /// A newer approved version exists.
    pub update_available: bool,
    #[schema(value_type = Option<Object>)]
    pub payload: Option<Value>,
    /// For a pack turned on in a server: how many emoji, stickers or sounds it added and are still there to be
    /// taken away (D-282). Absent for every other enablement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_count: Option<i64>,
}

/// What a member's client applies for a server (`GET /guilds/{id}/appearance`).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[schema(as = PackageGuildAppearance)]
pub struct Appearance {
    /// The server accent colour as `0xRRGGBB`, null when unset.
    pub accent_color: Option<i32>,
    /// The server-wide theme, null when the server uses the built-in look.
    pub theme: Option<AppearanceTheme>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[schema(as = PackageGuildTheme)]
pub struct AppearanceTheme {
    pub package_id: String,
    pub version_id: String,
    pub slug: String,
    pub name: String,
    /// `light` or `dark`.
    pub scheme: String,
    /// `--ui-*` design tokens, validated when the package was reviewed.
    #[schema(value_type = Object)]
    pub tokens: Value,
}

/// One image or sound of a pack, ready to become a server emoji, sticker or sound.
#[derive(Debug, Clone)]
pub struct PackItemData {
    pub name: String,
    pub bytes: Bytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = PackageDecision)]
pub enum Decision {
    Approve,
    Reject,
    /// Take an approved version down (it stops being served and enabled).
    Revoke,
}

/// A person on a profile page.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[schema(as = ProfilePageUser)]
pub struct PageUser {
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar: Option<String>,
    pub banner: Option<String>,
    pub accent_color: Option<i32>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProfilePage {
    pub user: PageUser,
    pub doc: ProfileDoc,
    /// The people named in the Top friends block who are still friends.
    pub friends: Vec<PageUser>,
    /// The viewer is the owner.
    pub is_owner: bool,
    /// The viewer may sign the guestbook (a friend, or the owner).
    pub can_sign: bool,
    /// The document has any block (otherwise the page shows the standard profile).
    pub customised: bool,
    /// Entries on the guestbook; only the owner is told (K-18), next to [`MAX_GUESTBOOK_ENTRIES`].
    pub guestbook_count: Option<i64>,
    /// The most entries one guestbook holds.
    pub guestbook_limit: i64,
    /// The pictures the document names (a background image, gallery pictures), with their addresses.
    pub media: Vec<media::MediaView>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GuestbookEntry {
    pub id: String,
    pub author: PageUser,
    pub body: String,
    pub created_at_ms: i64,
    /// The viewer may remove it: they own the page or wrote the entry.
    pub can_remove: bool,
}

/// What the review conduct of D-258 needs from the platform: the `#review-queue` card of a version.
/// `outcome` is `None` when the version entered review, else `approved`, `rejected`, `revoked` or
/// `withdrawn`. Failures are the implementation's to log; they never fail a package call.
#[async_trait::async_trait]
pub trait PackageEvents: Send + Sync {
    async fn card(&self, version_id: i64, outcome: Option<&str>);
}

/// Posts nothing (tests and tools without a Staff Server).
pub struct NoPackageEvents;

#[async_trait::async_trait]
impl PackageEvents for NoPackageEvents {
    async fn card(&self, _version_id: i64, _outcome: Option<&str>) {}
}

struct Inner {
    pool: PgPool,
    ids: IdSource,
    storage: Option<Arc<dyn ObjectStorage>>,
    events: Arc<dyn PackageEvents>,
}

#[derive(Clone)]
pub struct PackageService {
    inner: Arc<Inner>,
}

fn user_of(r: &PgRow, prefix: &str) -> Result<PageUser, sqlx::Error> {
    let col = |c: &str| format!("{prefix}{c}");
    Ok(PageUser {
        id: r.try_get::<i64, _>(col("id").as_str())?.to_string(),
        username: r.try_get::<String, _>(col("username").as_str())?,
        display_name: r.try_get(col("display_name").as_str())?,
        avatar: r.try_get(col("avatar_key").as_str())?,
        banner: r.try_get(col("banner_key").as_str())?,
        accent_color: r.try_get(col("accent_color").as_str())?,
    })
}

const USER_COLS: &str = "a.id AS u_id, a.username::text AS u_username, a.display_name AS u_display_name, \
                         a.avatar_key AS u_avatar_key, a.banner_key AS u_banner_key, a.accent_color AS u_accent_color";

fn version_of(r: &PgRow) -> Result<VersionView, sqlx::Error> {
    let manifest: Value = r.try_get("manifest")?;
    let kind: String = r.try_get("kind")?;
    Ok(VersionView {
        id: r.try_get::<i64, _>("vid")?.to_string(),
        package_id: r.try_get::<i64, _>("pid")?.to_string(),
        kind: PackageKind::parse(&kind).unwrap_or(PackageKind::Recipe),
        slug: r.try_get("slug")?,
        name: r.try_get("name")?,
        version: r.try_get("version")?,
        review_state: r.try_get("review_state")?,
        review_note: r.try_get("review_note")?,
        size_bytes: r.try_get("size_bytes")?,
        created_at_ms: r.try_get("created_ms")?,
        files: serde_json::from_value(manifest.get("files").cloned().unwrap_or(json!([])))
            .unwrap_or_default(),
        warnings: serde_json::from_value(manifest.get("warnings").cloned().unwrap_or(json!([])))
            .unwrap_or_default(),
    })
}

fn version_select() -> String {
    format!(
        "SELECT v.id AS vid, p.id AS pid, p.kind, p.slug, p.name, v.version, v.review_state, v.review_note, \
         v.size_bytes, v.manifest, {} AS created_ms \
         FROM package_versions v JOIN packages p ON p.id = v.package_id",
        ms("v.created_at")
    )
}

impl PackageService {
    pub fn new(pool: PgPool, ids: IdSource) -> Self {
        Self {
            inner: Arc::new(Inner {
                pool,
                ids,
                storage: None,
                events: Arc::new(NoPackageEvents),
            }),
        }
    }

    /// Where review cards go. Call before the service is shared.
    #[must_use]
    pub fn with_events(mut self, e: Arc<dyn PackageEvents>) -> Self {
        if let Some(i) = Arc::get_mut(&mut self.inner) {
            i.events = e;
        }
        self
    }

    /// Where package zips are stored. Without it uploads are refused. Call before the service is shared.
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

    fn storage(&self) -> PackageResult<&Arc<dyn ObjectStorage>> {
        self.inner.storage.as_ref().ok_or_else(|| {
            PackageError::Conflict("Package storage is not available on this server.".to_owned())
        })
    }

    // ------------------------------------------------------------ submission

    /// Validate an uploaded zip and store it as a new `pending` version. A new slug creates the package
    /// for this author; an existing one must be theirs and of the same kind, with a version not used before.
    pub async fn submit(
        &self,
        author: i64,
        zip: Vec<u8>,
        checks: &Checks<'_>,
    ) -> PackageResult<VersionView> {
        let storage = self.storage()?.clone();
        let checksum = data_encoding::HEXLOWER.encode(Sha256::digest(&zip).as_slice());
        let files = zip_unpack(zip.clone()).await?;
        let v = validate::validate(&files, checks).map_err(PackageError::Refused)?;

        let mut tx = self.pool().begin().await?;
        // One submission at a time per author.
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(author)
            .execute(&mut *tx)
            .await?;
        let existing: Option<(i64, i64, String)> =
            sqlx::query_as("SELECT id, author_id, kind FROM packages WHERE slug = $1")
                .bind(&v.slug)
                .fetch_optional(&mut *tx)
                .await?
                .map(|r: (i64, i64, String)| r);
        let package_id = match existing {
            Some((id, owner, kind)) => {
                if owner != author {
                    return Err(PackageError::Invalid(
                        "id",
                        "That package id is taken by another author.".to_owned(),
                    ));
                }
                if kind != v.kind.as_str() {
                    return Err(PackageError::Invalid(
                        "id",
                        "That package id belongs to a package of another kind.".to_owned(),
                    ));
                }
                id
            }
            None => {
                let owned: i64 =
                    sqlx::query_scalar("SELECT count(*) FROM packages WHERE author_id = $1")
                        .bind(author)
                        .fetch_one(&mut *tx)
                        .await?;
                if owned >= MAX_PACKAGES_PER_AUTHOR {
                    return Err(PackageError::Conflict(format!(
                        "You can own at most {MAX_PACKAGES_PER_AUTHOR} packages."
                    )));
                }
                let id = (self.inner.ids)();
                sqlx::query(
                    "INSERT INTO packages (id, kind, author_id, slug, name, description) \
                     VALUES ($1, $2, $3, $4, $5, $6)",
                )
                .bind(id)
                .bind(v.kind.as_str())
                .bind(author)
                .bind(&v.slug)
                .bind(&v.name)
                .bind(&v.description)
                .execute(&mut *tx)
                .await?;
                id
            }
        };
        let pending: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM package_versions v JOIN packages p ON p.id = v.package_id \
             WHERE p.author_id = $1 AND v.review_state = 'pending'",
        )
        .bind(author)
        .fetch_one(&mut *tx)
        .await?;
        if pending >= MAX_PENDING_PER_AUTHOR {
            return Err(PackageError::Conflict(format!(
                "You have {MAX_PENDING_PER_AUTHOR} versions waiting for review; wait for a decision."
            )));
        }
        let used: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM package_versions WHERE package_id = $1 AND version = $2)",
        )
        .bind(package_id)
        .bind(&v.version)
        .fetch_one(&mut *tx)
        .await?;
        if used {
            return Err(PackageError::Conflict(format!(
                "Version {} was submitted before; raise the version number.",
                v.version
            )));
        }
        let version_id = (self.inner.ids)();
        let key = artifact_key(package_id, version_id);
        let manifest = json!({
            "name": v.name, "description": v.description, "author": v.author,
            "files": v.files, "warnings": v.warnings, "payload": v.payload,
        });
        sqlx::query(
            "INSERT INTO package_versions (id, package_id, version, manifest, artifact_key, checksum, size_bytes) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(version_id)
        .bind(package_id)
        .bind(&v.version)
        .bind(&manifest)
        .bind(&key)
        .bind(&checksum)
        .bind(i64::try_from(zip.len()).unwrap_or(i64::MAX))
        .execute(&mut *tx)
        .await?;
        // Keep the listing text in step with the newest submission.
        sqlx::query("UPDATE packages SET name = $2, description = $3 WHERE id = $1")
            .bind(package_id)
            .bind(&v.name)
            .bind(&v.description)
            .execute(&mut *tx)
            .await?;
        if let Err(e) = storage.put(&key, Bytes::from(zip)).await {
            return Err(PackageError::Storage(e.to_string()));
        }
        tx.commit().await?;
        self.inner.events.card(version_id, None).await;
        self.version(version_id).await
    }

    async fn version(&self, id: i64) -> PackageResult<VersionView> {
        let sql = format!("{} WHERE v.id = $1", version_select());
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(self.pool())
            .await?
            .ok_or(PackageError::NotFound("version"))?;
        Ok(version_of(&row)?)
    }

    /// My packages with their versions, newest first.
    pub async fn mine(&self, author: i64) -> PackageResult<Vec<MyPackage>> {
        let sql = format!(
            "{} WHERE p.author_id = $1 ORDER BY p.id DESC, v.id DESC LIMIT 400",
            version_select()
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(author)
            .fetch_all(self.pool())
            .await?;
        let mut out: Vec<MyPackage> = Vec::new();
        for r in &rows {
            let v = version_of(r)?;
            match out.last_mut() {
                Some(p) if p.id == v.package_id => p.versions.push(v),
                _ => out.push(MyPackage {
                    id: v.package_id.clone(),
                    kind: v.kind,
                    slug: v.slug.clone(),
                    name: v.name.clone(),
                    versions: vec![v],
                }),
            }
        }
        Ok(out)
    }

    /// Withdraw my pending version (it can be fixed and submitted under a new number).
    pub async fn withdraw(&self, author: i64, version_id: i64) -> PackageResult<VersionView> {
        let n = sqlx::query(
            "UPDATE package_versions v SET review_state = 'withdrawn' FROM packages p \
             WHERE v.id = $1 AND p.id = v.package_id AND p.author_id = $2 AND v.review_state = 'pending'",
        )
        .bind(version_id)
        .bind(author)
        .execute(self.pool())
        .await?
        .rows_affected();
        if n == 0 {
            return Err(PackageError::NotFound("pending version"));
        }
        self.inner.events.card(version_id, Some("withdrawn")).await;
        self.version(version_id).await
    }

    /// The author deletes a package that has no approved version (K-18): every version is rejected or
    /// withdrawn, and a pending one is withdrawn by this call. That frees the id and removes the stored
    /// artifacts. A package with an approved version can only be revoked by staff.
    pub async fn delete_package(&self, author: i64, package_id: i64) -> PackageResult<()> {
        let mut tx = self.pool().begin().await?;
        let owner: Option<i64> =
            sqlx::query_scalar("SELECT author_id FROM packages WHERE id = $1 FOR UPDATE")
                .bind(package_id)
                .fetch_optional(&mut *tx)
                .await?;
        if owner != Some(author) {
            return Err(PackageError::NotFound("package"));
        }
        let versions: Vec<(i64, String, String)> = sqlx::query_as(
            "SELECT id, review_state, artifact_key FROM package_versions WHERE package_id = $1 FOR UPDATE",
        )
        .bind(package_id)
        .fetch_all(&mut *tx)
        .await?;
        if versions.iter().any(|(_, s, _)| s == "approved") {
            return Err(PackageError::Conflict(
                "Approved packages can only be revoked by staff".to_owned(),
            ));
        }
        sqlx::query("DELETE FROM packages WHERE id = $1")
            .bind(package_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        for (id, state, key) in &versions {
            if state == "pending" {
                self.inner.events.card(*id, Some("withdrawn")).await;
            }
            if let Some(s) = &self.inner.storage
                && let Err(e) = s.delete(key).await
            {
                tracing::warn!(error = %e, key, "could not remove a deleted package's artifact");
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------ review

    pub async fn pending(&self, limit: i64) -> PackageResult<Vec<PendingVersion>> {
        let sql = format!(
            "SELECT x.*, a.id AS author_id, COALESCE(a.display_name, a.username::text) AS author_name FROM ({} \
             WHERE v.review_state = 'pending' ORDER BY v.created_at, v.id LIMIT $1) x \
             JOIN packages p2 ON p2.id = x.pid JOIN accounts a ON a.id = p2.author_id \
             ORDER BY x.created_ms, x.vid",
            version_select()
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(limit)
            .fetch_all(self.pool())
            .await?;
        rows.iter()
            .map(|r| {
                let manifest: Value = r.try_get("manifest")?;
                Ok(PendingVersion {
                    version: version_of(r)?,
                    author_id: r.try_get::<i64, _>("author_id")?.to_string(),
                    author_name: r.try_get("author_name")?,
                    payload: manifest.get("payload").cloned().unwrap_or(Value::Null),
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }

    /// The zip of a version, for the reviewer.
    pub async fn zip(&self, version_id: i64) -> PackageResult<(String, Vec<u8>)> {
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT artifact_key, checksum FROM package_versions WHERE id = $1")
                .bind(version_id)
                .fetch_optional(self.pool())
                .await?;
        let (key, sum) = row.ok_or(PackageError::NotFound("version"))?;
        match self.storage()?.get(&key).await {
            Ok(b) => Ok((sum, b.to_vec())),
            Err(StorageError::NotFound(_)) => Err(PackageError::NotFound("version")),
            Err(e) => Err(PackageError::Storage(e.to_string())),
        }
    }

    /// Approve or reject a pending version, or revoke an approved one.
    pub async fn decide(
        &self,
        version_id: i64,
        reviewer: i64,
        decision: Decision,
        note: Option<String>,
    ) -> PackageResult<VersionView> {
        let note = note.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty());
        if let Some(n) = &note
            && n.chars().count() > 500
        {
            return Err(PackageError::Invalid(
                "note",
                "The note is at most 500 characters.".to_owned(),
            ));
        }
        if decision == Decision::Reject && note.is_none() {
            return Err(PackageError::Invalid(
                "note",
                "Say why the version is rejected.".to_owned(),
            ));
        }
        let mut tx = self.pool().begin().await?;
        let row: Option<(i64, String, i64)> = sqlx::query_as(
            "SELECT v.package_id, v.review_state, p.author_id FROM package_versions v              JOIN packages p ON p.id = v.package_id WHERE v.id = $1 FOR UPDATE OF v",
        )
        .bind(version_id)
        .fetch_optional(&mut *tx)
        .await?;
        let (package_id, state, author) = row.ok_or(PackageError::NotFound("version"))?;
        // Two people per release (D-258): nobody decides on their own package, the Owner included.
        if author == reviewer {
            return Err(PackageError::SelfReview);
        }
        let (from, to) = match decision {
            Decision::Approve => ("pending", "approved"),
            Decision::Reject => ("pending", "rejected"),
            Decision::Revoke => ("approved", "revoked"),
        };
        if state != from {
            return Err(PackageError::Conflict(format!(
                "The version is {state}; it cannot be moved to {to}."
            )));
        }
        sqlx::query(
            "UPDATE package_versions SET review_state = $2, reviewed_by = $3, reviewed_at = now(), review_note = $4 \
             WHERE id = $1",
        )
        .bind(version_id)
        .bind(to)
        .bind(reviewer)
        .bind(&note)
        .execute(&mut *tx)
        .await?;
        // `latest_version_id` is the newest approved version of the package.
        sqlx::query(
            "UPDATE packages SET latest_version_id = (\
               SELECT id FROM package_versions WHERE package_id = $1 AND review_state = 'approved' \
               ORDER BY id DESC LIMIT 1) WHERE id = $1",
        )
        .bind(package_id)
        .execute(&mut *tx)
        .await?;
        if decision == Decision::Revoke {
            sqlx::query("DELETE FROM package_enablements WHERE version_id = $1")
                .bind(version_id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        self.inner.events.card(version_id, Some(to)).await;
        self.version(version_id).await
    }

    // ------------------------------------------------------------ browsing

    fn view_of(r: &PgRow, with_payload: bool) -> Result<PackageView, sqlx::Error> {
        let manifest: Value = r.try_get("manifest")?;
        let kind: String = r.try_get("kind")?;
        let kind = PackageKind::parse(&kind).unwrap_or(PackageKind::Recipe);
        let payload = manifest.get("payload").cloned();
        Ok(PackageView {
            id: r.try_get::<i64, _>("pid")?.to_string(),
            kind,
            slug: r.try_get("slug")?,
            name: r.try_get("name")?,
            description: r.try_get("description")?,
            author_id: r.try_get::<i64, _>("author_id")?.to_string(),
            author_name: r.try_get("author_name")?,
            version_id: r.try_get::<i64, _>("vid")?.to_string(),
            version: r.try_get("version")?,
            size_bytes: r.try_get("size_bytes")?,
            updated_at_ms: r.try_get("reviewed_ms")?,
            payload: if with_payload && kind != PackageKind::ServerTemplate {
                payload
            } else {
                None
            },
        })
    }

    fn browse_select() -> String {
        format!(
            "SELECT p.id AS pid, p.kind, p.slug, p.name, p.description, p.author_id, \
             COALESCE(a.display_name, a.username::text) AS author_name, v.id AS vid, v.version, \
             v.size_bytes, v.manifest, {} AS reviewed_ms \
             FROM packages p JOIN package_versions v ON v.id = p.latest_version_id \
             JOIN accounts a ON a.id = p.author_id \
             WHERE v.review_state = 'approved'",
            ms("COALESCE(v.reviewed_at, v.created_at)")
        )
    }

    /// Approved packages, newest first, optionally of one kind or matching `q` (name or slug).
    pub async fn directory(
        &self,
        kind: Option<PackageKind>,
        q: Option<&str>,
        before: Option<i64>,
    ) -> PackageResult<Vec<PackageView>> {
        let like = q.map(str::trim).filter(|s| !s.is_empty()).map(|s| {
            format!(
                "%{}%",
                s.replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            )
        });
        let sql = format!(
            "{} AND ($1::text IS NULL OR p.kind = $1) AND ($2::text IS NULL OR p.name ILIKE $2 OR p.slug ILIKE $2) \
             AND ($3::bigint IS NULL OR p.id < $3) ORDER BY p.id DESC LIMIT {PAGE}",
            Self::browse_select()
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(kind.map(PackageKind::as_str))
            .bind(like)
            .bind(before)
            .fetch_all(self.pool())
            .await?;
        rows.iter()
            .map(|r| Self::view_of(r, true))
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }

    /// One approved package, with its full payload.
    pub async fn get(&self, package_id: i64) -> PackageResult<PackageView> {
        let sql = format!("{} AND p.id = $1", Self::browse_select());
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(package_id)
            .fetch_optional(self.pool())
            .await?
            .ok_or(PackageError::NotFound("package"))?;
        let mut v = Self::view_of(&row, true)?;
        if v.payload.is_none() {
            v.payload = row
                .try_get::<Value, _>("manifest")
                .ok()
                .and_then(|m| m.get("payload").cloned());
        }
        Ok(v)
    }

    /// A file of an approved version (a pack's image or sound), with its content type.
    pub async fn file(
        &self,
        version_id: i64,
        path: &str,
    ) -> PackageResult<(Vec<u8>, &'static str)> {
        if zipfiles::path_problem(path).is_some() {
            return Err(PackageError::NotFound("file"));
        }
        let ct = content_type(path).ok_or(PackageError::NotFound("file"))?;
        let row: Option<(String, Value)> = sqlx::query_as(
            "SELECT artifact_key, manifest FROM package_versions WHERE id = $1 AND review_state = 'approved'",
        )
        .bind(version_id)
        .fetch_optional(self.pool())
        .await?;
        let (key, manifest) = row.ok_or(PackageError::NotFound("file"))?;
        let listed = manifest
            .get("files")
            .and_then(Value::as_array)
            .is_some_and(|fs| fs.iter().any(|f| f["path"] == path));
        if !listed {
            return Err(PackageError::NotFound("file"));
        }
        let zip = match self.storage()?.get(&key).await {
            Ok(b) => b.to_vec(),
            Err(StorageError::NotFound(_)) => return Err(PackageError::NotFound("file")),
            Err(e) => return Err(PackageError::Storage(e.to_string())),
        };
        let files = zip_unpack(zip).await?;
        files
            .get(path)
            .map(|d| (d.clone(), ct))
            .ok_or(PackageError::NotFound("file"))
    }

    /// The stored payload and kind of an approved version (the recipe applier, template instantiation).
    pub async fn approved_payload(&self, version_id: i64) -> PackageResult<(PackageKind, Value)> {
        let row: Option<(String, Value)> = sqlx::query_as(
            "SELECT p.kind, v.manifest FROM package_versions v JOIN packages p ON p.id = v.package_id \
             WHERE v.id = $1 AND v.review_state = 'approved'",
        )
        .bind(version_id)
        .fetch_optional(self.pool())
        .await?;
        let (kind, manifest) = row.ok_or(PackageError::NotFound("version"))?;
        Ok((
            PackageKind::parse(&kind).ok_or(PackageError::NotFound("version"))?,
            manifest.get("payload").cloned().unwrap_or(Value::Null),
        ))
    }

    // ------------------------------------------------------------ enablements

    /// Turn an approved package on for a server or a person, at its newest approved version. A theme
    /// replaces the theme already on in the same section.
    pub async fn enable(
        &self,
        scope: &str,
        scope_id: i64,
        package_id: i64,
        section: &str,
        config: Value,
        by: i64,
    ) -> PackageResult<Enablement> {
        if !matches!(scope, "guild" | "user") {
            return Err(PackageError::Invalid("scope", "Unknown scope.".to_owned()));
        }
        let ok_section = section == "all"
            || (2..=16).contains(&section.len()) && section.bytes().all(|b| b.is_ascii_lowercase());
        if !ok_section {
            return Err(PackageError::Invalid(
                "section",
                "Not a section name.".to_owned(),
            ));
        }
        let row: Option<(String, Option<i64>)> =
            sqlx::query_as("SELECT kind, latest_version_id FROM packages WHERE id = $1")
                .bind(package_id)
                .fetch_optional(self.pool())
                .await?;
        let (kind, latest) = row.ok_or(PackageError::NotFound("package"))?;
        let kind = PackageKind::parse(&kind).ok_or(PackageError::NotFound("package"))?;
        let version = latest.ok_or(PackageError::NotFound("package"))?;
        let allowed = match scope {
            "user" => matches!(kind, PackageKind::Theme),
            _ => matches!(
                kind,
                PackageKind::Theme
                    | PackageKind::Recipe
                    | PackageKind::EmojiPack
                    | PackageKind::StickerPack
                    | PackageKind::SoundPack
            ),
        };
        if !allowed {
            return Err(PackageError::Conflict(
                "That kind of package cannot be turned on here.".to_owned(),
            ));
        }
        // A server theme recolours the Lounge only (D-281): `all` and `lounge` mean the same and are stored as
        // `lounge`; the theme already on replaces any older row stored as `all` too.
        let guild_theme = scope == "guild" && kind == PackageKind::Theme;
        if guild_theme && !matches!(section, "all" | "lounge") {
            return Err(PackageError::Invalid(
                "section",
                "A server theme applies to the Lounge only.".to_owned(),
            ));
        }
        let section = if guild_theme { "lounge" } else { section };
        let mut tx = self.pool().begin().await?;
        if kind == PackageKind::Theme {
            sqlx::query(
                "DELETE FROM package_enablements e USING packages p \
                 WHERE e.scope = $1 AND e.scope_id = $2 AND (e.section = $3 OR ($4 AND e.section = 'all')) \
                   AND p.id = e.package_id AND p.kind = 'theme'",
            )
            .bind(scope)
            .bind(scope_id)
            .bind(section)
            .bind(guild_theme)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO package_enablements (scope, scope_id, package_id, version_id, section, config, enabled_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (scope, scope_id, package_id) DO UPDATE SET version_id = $4, section = $5, \
               config = $6, enabled_by = $7, enabled_at = now()",
        )
        .bind(scope)
        .bind(scope_id)
        .bind(package_id)
        .bind(version)
        .bind(section)
        .bind(&config)
        .bind(by)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.enabled(scope, scope_id)
            .await?
            .into_iter()
            .find(|e| e.package_id == package_id.to_string())
            .ok_or(PackageError::NotFound("package"))
    }

    pub async fn disable(&self, scope: &str, scope_id: i64, package_id: i64) -> PackageResult<()> {
        let n = sqlx::query(
            "DELETE FROM package_enablements WHERE scope = $1 AND scope_id = $2 AND package_id = $3",
        )
        .bind(scope)
        .bind(scope_id)
        .bind(package_id)
        .execute(self.pool())
        .await?
        .rows_affected();
        if n == 0 {
            return Err(PackageError::NotFound("package"));
        }
        Ok(())
    }

    // ------------------------------------------------------------ server appearance (LNG-SSET-050)

    /// What members' clients lay over their interface in a server: the server-wide theme (a guild theme
    /// enablement; the `lounge` one wins over `all`) and the accent colour.
    pub async fn appearance(&self, guild: i64) -> PackageResult<Appearance> {
        let accent: Option<Option<i32>> =
            sqlx::query_scalar("SELECT accent_color FROM guild_appearance WHERE guild_id = $1")
                .bind(guild)
                .fetch_optional(self.pool())
                .await?;
        let on = self.enabled("guild", guild).await?;
        let pick = |section: &str| {
            on.iter()
                .find(|e| e.kind == PackageKind::Theme && e.section == section)
        };
        let theme = pick("lounge").or_else(|| pick("all")).and_then(|e| {
            let p = e.payload.as_ref()?;
            Some(AppearanceTheme {
                package_id: e.package_id.clone(),
                version_id: e.version_id.clone(),
                slug: e.slug.clone(),
                name: e.name.clone(),
                scheme: if p.get("scheme").and_then(Value::as_str) == Some("light") {
                    "light"
                } else {
                    "dark"
                }
                .to_owned(),
                tokens: p.get("tokens").cloned().unwrap_or_else(|| json!({})),
            })
        });
        Ok(Appearance {
            accent_color: accent.flatten(),
            theme,
        })
    }

    /// Set or clear the server accent colour (`0xRRGGBB`).
    pub async fn set_accent(&self, guild: i64, color: Option<i32>, by: i64) -> PackageResult<()> {
        if color.is_some_and(|c| !(0..=0x00FF_FFFF).contains(&c)) {
            return Err(PackageError::Invalid(
                "accent_color",
                "A colour is 0 to 16777215.".to_owned(),
            ));
        }
        sqlx::query(
            "INSERT INTO guild_appearance (guild_id, accent_color, updated_by) VALUES ($1, $2, $3) \
             ON CONFLICT (guild_id) DO UPDATE SET accent_color = $2, updated_by = $3, updated_at = now()",
        )
        .bind(guild)
        .bind(color)
        .bind(by)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    // ------------------------------------------------------------ pack items in a server

    /// The images or sounds of an approved emoji, sticker or sound pack, read out of its zip once.
    pub async fn pack_contents(&self, version_id: i64) -> PackageResult<Vec<PackItemData>> {
        let (kind, payload) = self.approved_payload(version_id).await?;
        if !matches!(
            kind,
            PackageKind::EmojiPack | PackageKind::StickerPack | PackageKind::SoundPack
        ) {
            return Err(PackageError::Conflict("That is not a pack.".to_owned()));
        }
        let key: String =
            sqlx::query_scalar("SELECT artifact_key FROM package_versions WHERE id = $1")
                .bind(version_id)
                .fetch_one(self.pool())
                .await?;
        let zip = match self.storage()?.get(&key).await {
            Ok(b) => b.to_vec(),
            Err(StorageError::NotFound(_)) => return Err(PackageError::NotFound("version")),
            Err(e) => return Err(PackageError::Storage(e.to_string())),
        };
        let files = zip_unpack(zip).await?;
        let items = payload
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(items
            .iter()
            .filter_map(|i| {
                let name = i.get("name")?.as_str()?.to_owned();
                let file = i.get("file")?.as_str()?;
                Some(PackItemData {
                    name,
                    bytes: Bytes::from(files.get(file)?.clone()),
                })
            })
            .collect())
    }

    /// Remember that these items of a server came from a pack.
    pub async fn record_pack_items(
        &self,
        guild: i64,
        package: i64,
        item_kind: &str,
        ids: &[i64],
        by: i64,
    ) -> PackageResult<()> {
        for id in ids {
            sqlx::query(
                "INSERT INTO guild_pack_items (guild_id, package_id, item_kind, item_id, added_by) \
                 VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
            )
            .bind(guild)
            .bind(package)
            .bind(item_kind)
            .bind(id)
            .bind(by)
            .execute(self.pool())
            .await?;
        }
        Ok(())
    }

    /// The `(item kind, item id)` pairs a server got from a pack.
    pub async fn pack_items(&self, guild: i64, package: i64) -> PackageResult<Vec<(String, i64)>> {
        Ok(sqlx::query_as(
            "SELECT item_kind, item_id FROM guild_pack_items WHERE guild_id = $1 AND package_id = $2 \
             ORDER BY item_id",
        )
        .bind(guild)
        .bind(package)
        .fetch_all(self.pool())
        .await?)
    }

    /// Forget every item recorded for a pack in a server.
    pub async fn forget_pack_items(&self, guild: i64, package: i64) -> PackageResult<()> {
        sqlx::query("DELETE FROM guild_pack_items WHERE guild_id = $1 AND package_id = $2")
            .bind(guild)
            .bind(package)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// What is turned on for a server or a person. A revoked version is never listed.
    pub async fn enabled(&self, scope: &str, scope_id: i64) -> PackageResult<Vec<Enablement>> {
        let sql = format!(
            "SELECT e.package_id, e.version_id, p.kind, p.slug, p.name, v.version, v.manifest, e.section, e.config, \
             {} AS enabled_ms, (p.latest_version_id IS NOT NULL AND p.latest_version_id <> e.version_id) AS update_available, \
             (SELECT count(*) FROM guild_pack_items i WHERE e.scope = 'guild' AND i.guild_id = e.scope_id \
                AND i.package_id = e.package_id) AS item_count \
             FROM package_enablements e JOIN packages p ON p.id = e.package_id \
             JOIN package_versions v ON v.id = e.version_id \
             WHERE e.scope = $1 AND e.scope_id = $2 AND v.review_state = 'approved' ORDER BY e.enabled_at DESC LIMIT 200",
            ms("e.enabled_at")
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(scope)
            .bind(scope_id)
            .fetch_all(self.pool())
            .await?;
        rows.iter()
            .map(|r| {
                let kind: String = r.try_get("kind")?;
                let kind = PackageKind::parse(&kind).unwrap_or(PackageKind::Recipe);
                let manifest: Value = r.try_get("manifest")?;
                Ok(Enablement {
                    package_id: r.try_get::<i64, _>("package_id")?.to_string(),
                    version_id: r.try_get::<i64, _>("version_id")?.to_string(),
                    kind,
                    slug: r.try_get("slug")?,
                    name: r.try_get("name")?,
                    version: r.try_get("version")?,
                    section: r.try_get("section")?,
                    config: r.try_get("config")?,
                    enabled_at_ms: r.try_get("enabled_ms")?,
                    update_available: r.try_get("update_available")?,
                    payload: (kind != PackageKind::ServerTemplate)
                        .then(|| manifest.get("payload").cloned())
                        .flatten(),
                    item_count: if scope == "guild"
                        && matches!(
                            kind,
                            PackageKind::EmojiPack
                                | PackageKind::StickerPack
                                | PackageKind::SoundPack
                        ) {
                        Some(r.try_get::<i64, _>("item_count")?)
                    } else {
                        None
                    },
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }

    // ------------------------------------------------------------ profile documents

    async fn friends_among(&self, user: i64, ids: &[i64]) -> PackageResult<Vec<i64>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(sqlx::query_scalar(
            "SELECT target_id FROM relationships WHERE user_id = $1 AND type = 1 AND target_id = ANY($2)",
        )
        .bind(user)
        .bind(ids)
        .fetch_all(self.pool())
        .await?)
    }

    async fn blocked_between(&self, a: i64, b: i64) -> PackageResult<bool> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM relationships WHERE type = 2 AND \
             ((user_id = $1 AND target_id = $2) OR (user_id = $2 AND target_id = $1)))",
        )
        .bind(a)
        .bind(b)
        .fetch_one(self.pool())
        .await?)
    }

    /// My document (the empty default when none was saved).
    pub async fn own_doc(&self, user: i64) -> PackageResult<ProfileDoc> {
        let v: Option<Value> = sqlx::query_scalar("SELECT profile_doc FROM accounts WHERE id = $1")
            .bind(user)
            .fetch_optional(self.pool())
            .await?;
        Ok(v.and_then(|v| profile_doc::parse_doc(&v, false).ok())
            .unwrap_or_default())
    }

    /// Save my document after validating it. Top friends must be friends.
    pub async fn set_doc(&self, user: i64, value: &Value) -> PackageResult<ProfileDoc> {
        let mine = self.media_ids(user).await?;
        let doc = profile_doc::parse_doc_with(value, false, Some(&mine)).map_err(|ps| {
            PackageError::Refused(
                ps.into_iter()
                    .map(|p| format!("{}: {}", p.path, p.message))
                    .collect(),
            )
        })?;
        let named = profile_doc::top_friend_ids(&doc);
        let friends = self.friends_among(user, &named).await?;
        if named.iter().any(|id| !friends.contains(id)) {
            return Err(PackageError::Invalid(
                "blocks",
                "Top friends can only be people on your friends list.".to_owned(),
            ));
        }
        sqlx::query("UPDATE accounts SET profile_doc = $2, updated_at = now() WHERE id = $1 AND state <> 'deleted'")
            .bind(user)
            .bind(serde_json::to_value(&doc).unwrap_or(Value::Null))
            .execute(self.pool())
            .await?;
        self.drop_unreferenced_media(user, &doc, None).await?;
        Ok(doc)
    }

    /// Take a layout package's template as my document, keeping my content blocks' text where a block of the
    /// same type exists. Simple on purpose: the template replaces blocks and style; the person edits after.
    pub async fn apply_layout(&self, user: i64, version_id: i64) -> PackageResult<ProfileDoc> {
        let (kind, payload) = self.approved_payload(version_id).await?;
        if kind != PackageKind::ProfileLayout {
            return Err(PackageError::Conflict(
                "That is not a profile layout.".to_owned(),
            ));
        }
        let slug: String = sqlx::query_scalar(
            "SELECT p.slug FROM package_versions v JOIN packages p ON p.id = v.package_id WHERE v.id = $1",
        )
        .bind(version_id)
        .fetch_one(self.pool())
        .await?;
        let mut doc: ProfileDoc =
            serde_json::from_value(payload.get("doc").cloned().unwrap_or(Value::Null))
                .map_err(|_| PackageError::NotFound("layout"))?;
        let mine = self.own_doc(user).await?;
        for b in &mut doc.blocks {
            if let Some(old) = mine.blocks.iter().find(|o| o.kind == b.kind) {
                b.config = old.config.clone();
            }
        }
        doc.layout = Some(slug);
        self.set_doc(user, &serde_json::to_value(&doc).unwrap_or(Value::Null))
            .await
    }

    async fn user_by(
        &self,
        where_: &str,
        bind_i: Option<i64>,
        bind_s: Option<&str>,
    ) -> PackageResult<Option<(PageUser, Value)>> {
        let sql = format!(
            "SELECT {USER_COLS}, a.profile_doc FROM accounts a WHERE {where_} AND a.state = 'active'"
        );
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
        if let Some(i) = bind_i {
            q = q.bind(i);
        }
        if let Some(s) = bind_s {
            q = q.bind(s);
        }
        let row = q.fetch_optional(self.pool()).await?;
        row.map(|r| Ok((user_of(&r, "u_")?, r.try_get("profile_doc")?)))
            .transpose()
    }

    /// The profile page of a person, by username or id. `None` when there is no such active account or
    /// either side blocked the other.
    pub async fn page(
        &self,
        viewer: i64,
        username: Option<&str>,
        id: Option<i64>,
    ) -> PackageResult<ProfilePage> {
        let found = match (username, id) {
            (Some(n), _) => {
                self.user_by("a.username = $1::citext", None, Some(n))
                    .await?
            }
            (_, Some(i)) => self.user_by("a.id = $1", Some(i), None).await?,
            _ => None,
        };
        let (user, raw) = found.ok_or(PackageError::NotFound("profile"))?;
        let owner: i64 = user.id.parse().unwrap_or(0);
        let is_owner = viewer == owner;
        if !is_owner && self.blocked_between(viewer, owner).await? {
            return Err(PackageError::NotFound("profile"));
        }
        let doc = profile_doc::parse_doc(&raw, false).unwrap_or_default();
        let named = profile_doc::top_friend_ids(&doc);
        let still = self.friends_among(owner, &named).await?;
        let mut friends = Vec::new();
        for id in named.iter().filter(|i| still.contains(i)) {
            if let Some((u, _)) = self.user_by("a.id = $1", Some(*id), None).await? {
                friends.push(u);
            }
        }
        let can_sign = is_owner
            || sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM relationships WHERE user_id = $1 AND target_id = $2 AND type = 1)",
            )
            .bind(viewer)
            .bind(owner)
            .fetch_one(self.pool())
            .await?;
        let guestbook_count = if is_owner {
            Some(
                sqlx::query_scalar("SELECT count(*) FROM profile_guestbook WHERE profile_id = $1")
                    .bind(owner)
                    .fetch_one(self.pool())
                    .await?,
            )
        } else {
            None
        };
        let media = self.media_named_by(owner, &doc).await?;
        Ok(ProfilePage {
            media,
            guestbook_count,
            guestbook_limit: MAX_GUESTBOOK_ENTRIES,
            customised: !doc.blocks.is_empty(),
            user,
            doc,
            friends,
            is_owner,
            can_sign,
        })
    }

    // ------------------------------------------------------------ guestbook

    pub async fn guestbook(
        &self,
        viewer: i64,
        profile: i64,
        before: Option<i64>,
    ) -> PackageResult<Vec<GuestbookEntry>> {
        if viewer != profile && self.blocked_between(viewer, profile).await? {
            return Err(PackageError::NotFound("profile"));
        }
        let sql = format!(
            "SELECT g.id AS gid, g.body, {} AS created_ms, {USER_COLS} FROM profile_guestbook g \
             JOIN accounts a ON a.id = g.author_id WHERE g.profile_id = $1 AND ($2::bigint IS NULL OR g.id < $2) \
             ORDER BY g.id DESC LIMIT {PAGE}",
            ms("g.created_at")
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(profile)
            .bind(before)
            .fetch_all(self.pool())
            .await?;
        rows.iter()
            .map(|r| {
                Ok(GuestbookEntry {
                    id: r.try_get::<i64, _>("gid")?.to_string(),
                    author: user_of(r, "u_")?,
                    body: r.try_get("body")?,
                    created_at_ms: r.try_get("created_ms")?,
                    can_remove: viewer == profile || r.try_get::<i64, _>("u_id")? == viewer,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }

    /// Sign a guestbook: friends of the owner (or the owner), only while the owner's page has a Guestbook
    /// block, never when either side blocked the other.
    pub async fn sign(
        &self,
        author: i64,
        profile: i64,
        body: &str,
    ) -> PackageResult<GuestbookEntry> {
        let body = body.trim();
        let n = body.chars().count();
        if n == 0 || n > 500 || body.chars().any(|c| c.is_control() && c != '\n') {
            return Err(PackageError::Invalid(
                "body",
                "Write 1 to 500 characters.".to_owned(),
            ));
        }
        let page = self.page(author, None, Some(profile)).await?;
        if !page.can_sign {
            return Err(PackageError::Forbidden);
        }
        if !page
            .doc
            .blocks
            .iter()
            .any(|b| b.kind == profile_doc::BlockKind::Guestbook)
        {
            return Err(PackageError::Conflict(
                "This page has no guestbook.".to_owned(),
            ));
        }
        let recent: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM profile_guestbook WHERE author_id = $1 AND created_at > now() - interval '1 minute'",
        )
        .bind(author)
        .fetch_one(self.pool())
        .await?;
        if recent >= GUESTBOOK_PER_MINUTE {
            return Err(PackageError::Conflict(
                "You are signing guestbooks too fast. Try again in a minute.".to_owned(),
            ));
        }
        let total: i64 =
            sqlx::query_scalar("SELECT count(*) FROM profile_guestbook WHERE profile_id = $1")
                .bind(profile)
                .fetch_one(self.pool())
                .await?;
        if total >= MAX_GUESTBOOK_ENTRIES {
            return Err(PackageError::Conflict("This guestbook is full".to_owned()));
        }
        let id = (self.inner.ids)();
        sqlx::query("INSERT INTO profile_guestbook (id, profile_id, author_id, body) VALUES ($1, $2, $3, $4)")
            .bind(id)
            .bind(profile)
            .bind(author)
            .bind(body)
            .execute(self.pool())
            .await?;
        let mut list = self.guestbook(author, profile, Some(id + 1)).await?;
        list.retain(|e| e.id == id.to_string());
        list.pop().ok_or(PackageError::NotFound("entry"))
    }

    /// The owner removes any entry; an author removes their own.
    pub async fn remove_entry(&self, actor: i64, profile: i64, entry: i64) -> PackageResult<()> {
        let n = sqlx::query(
            "DELETE FROM profile_guestbook WHERE id = $1 AND profile_id = $2 AND (author_id = $3 OR profile_id = $3)",
        )
        .bind(entry)
        .bind(profile)
        .bind(actor)
        .execute(self.pool())
        .await?
        .rows_affected();
        if n == 0 {
            return Err(PackageError::NotFound("entry"));
        }
        Ok(())
    }
}

async fn zip_unpack(zip: Vec<u8>) -> PackageResult<zipfiles::Files> {
    tokio::task::spawn_blocking(move || zipfiles::unpack(&zip))
        .await
        .map_err(|e| PackageError::Storage(e.to_string()))?
        .map_err(|m| PackageError::Refused(vec![m]))
}

/// Content type of a file a package may serve: images and sounds only, never markup.
pub fn content_type(path: &str) -> Option<&'static str> {
    let (_, ext) = path.rsplit_once('.')?;
    Some(match ext.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        _ => return None,
    })
}
