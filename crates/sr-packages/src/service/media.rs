//! Pictures a person uploads for their customised profile page (K-17, D-260): one background and up to
//! twelve gallery pictures. The files are normalised by the caller (`sr-api` runs them through the same
//! pipeline as a banner) and live under `avatars/<user>/<hash>` in object storage, which `/cdn/avatars`
//! serves. A profile document refers to a row by id; saving a document drops the rows it no longer
//! names, and deleting a row takes its references out of the saved document.

use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::Value;
use sqlx::Row;
use utoipa::ToSchema;

use super::{PackageService, ms};
use crate::error::{PackageError, PackageResult};
use crate::profile_doc::{self, MediaIds, ProfileDoc};

/// Most gallery pictures one person may keep.
pub const MAX_GALLERY_MEDIA: i64 = 12;
/// Most background pictures one person may keep.
pub const MAX_BACKGROUND_MEDIA: i64 = 1;

/// One uploaded picture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[schema(as = ProfileMedia)]
pub struct MediaView {
    pub id: String,
    /// `background` or `gallery`.
    pub kind: String,
    /// Where the picture is served from (`/cdn/avatars/<user>/<hash>.<ext>`).
    pub url: String,
    pub content_type: String,
    pub width: i32,
    pub height: i32,
    pub bytes: i64,
    pub created_at_ms: i64,
}

/// A stored, normalised picture ready to be recorded.
#[derive(Debug, Clone)]
pub struct NewMedia {
    pub id: i64,
    pub kind: String,
    /// `avatars/<user>/<hash>`.
    pub object_key: String,
    pub content_type: String,
    pub width: i32,
    pub height: i32,
    pub bytes: i64,
}

fn ext_of(content_type: &str) -> &'static str {
    match content_type {
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => "png",
    }
}

fn view_of(r: &sqlx::postgres::PgRow) -> Result<MediaView, sqlx::Error> {
    let key: String = r.try_get("object_key")?;
    let content_type: String = r.try_get("content_type")?;
    Ok(MediaView {
        id: r.try_get::<i64, _>("id")?.to_string(),
        kind: r.try_get("kind")?,
        url: format!("/cdn/{key}.{}", ext_of(&content_type)),
        content_type,
        width: r.try_get("width")?,
        height: r.try_get("height")?,
        bytes: r.try_get("bytes")?,
        created_at_ms: r.try_get("created_ms")?,
    })
}

fn select() -> String {
    format!(
        "SELECT id, kind, object_key, content_type, width, height, bytes, {} AS created_ms FROM profile_media",
        ms("created_at")
    )
}

fn valid_kind(kind: &str) -> bool {
    matches!(kind, "background" | "gallery")
}

impl PackageService {
    /// A fresh id for a picture (the caller uses it to scope the stored object's hash).
    pub fn media_id(&self) -> i64 {
        (self.inner.ids)()
    }

    /// My pictures, oldest first.
    pub async fn media_list(&self, user: i64) -> PackageResult<Vec<MediaView>> {
        let sql = format!("{} WHERE account_id = $1 ORDER BY id", select());
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(user)
            .fetch_all(self.pool())
            .await?;
        rows.iter()
            .map(view_of)
            .collect::<Result<_, _>>()
            .map_err(Into::into)
    }

    /// Refuse early, before a file is processed, when `kind` cannot take another picture. A new
    /// background replaces the old one, so only the gallery has a cap that refuses.
    pub async fn media_check(&self, user: i64, kind: &str) -> PackageResult<()> {
        if !valid_kind(kind) {
            return Err(PackageError::Invalid(
                "kind",
                "Must be background or gallery.".to_owned(),
            ));
        }
        if kind == "gallery" {
            let n: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM profile_media WHERE account_id = $1 AND kind = 'gallery'",
            )
            .bind(user)
            .fetch_one(self.pool())
            .await?;
            if n >= MAX_GALLERY_MEDIA {
                return Err(PackageError::Conflict(format!(
                    "You can keep at most {MAX_GALLERY_MEDIA} gallery pictures."
                )));
            }
        }
        Ok(())
    }

    /// Record a stored picture. The caps are checked here, under a lock per person; a new background
    /// replaces the one before it (its file is removed). On an error the caller
    /// deletes the new file itself.
    pub async fn media_add(&self, user: i64, new: NewMedia) -> PackageResult<MediaView> {
        if !valid_kind(&new.kind) {
            return Err(PackageError::Invalid(
                "kind",
                "Must be background or gallery.".to_owned(),
            ));
        }
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(user)
            .execute(&mut *tx)
            .await?;
        let mut replaced: Vec<(i64, String)> = Vec::new();
        if new.kind == "gallery" {
            let n: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM profile_media WHERE account_id = $1 AND kind = 'gallery'",
            )
            .bind(user)
            .fetch_one(&mut *tx)
            .await?;
            if n >= MAX_GALLERY_MEDIA {
                return Err(PackageError::Conflict(format!(
                    "You can keep at most {MAX_GALLERY_MEDIA} gallery pictures."
                )));
            }
        } else {
            replaced = sqlx::query_as(
                "DELETE FROM profile_media WHERE account_id = $1 AND kind = 'background' RETURNING id, object_key",
            )
            .bind(user)
            .fetch_all(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO profile_media (id, account_id, kind, object_key, content_type, width, height, bytes) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(new.id)
        .bind(user)
        .bind(&new.kind)
        .bind(&new.object_key)
        .bind(&new.content_type)
        .bind(new.width)
        .bind(new.height)
        .bind(new.bytes)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        if !replaced.is_empty() {
            // The old background is no longer a valid reference in a saved document.
            let ids: BTreeSet<String> = replaced.iter().map(|(i, _)| i.to_string()).collect();
            self.strip_media_refs(user, &BTreeSet::new(), &ids).await;
            for (_, key) in &replaced {
                self.remove_object(user, key).await;
            }
        }
        let sql = format!("{} WHERE id = $1", select());
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(new.id)
            .fetch_one(self.pool())
            .await?;
        Ok(view_of(&row)?)
    }

    /// Delete one of my pictures: the row, the file, and every reference to it in my saved document.
    pub async fn media_delete(&self, user: i64, id: i64) -> PackageResult<()> {
        let key: Option<String> = sqlx::query_scalar(
            "DELETE FROM profile_media WHERE id = $1 AND account_id = $2 RETURNING object_key",
        )
        .bind(id)
        .bind(user)
        .fetch_optional(self.pool())
        .await?;
        let key = key.ok_or(PackageError::NotFound("picture"))?;
        let ids: BTreeSet<String> = BTreeSet::from([id.to_string()]);
        self.strip_media_refs(user, &ids, &ids).await;
        self.remove_object(user, &key).await;
        Ok(())
    }

    /// The ids of my pictures, for the document validator.
    pub(super) async fn media_ids(&self, user: i64) -> PackageResult<MediaIds> {
        let rows: Vec<(i64, String)> =
            sqlx::query_as("SELECT id, kind FROM profile_media WHERE account_id = $1")
                .bind(user)
                .fetch_all(self.pool())
                .await?;
        let mut out = MediaIds::default();
        for (id, kind) in rows {
            if kind == "background" {
                out.background.insert(id.to_string());
            } else {
                out.gallery.insert(id.to_string());
            }
        }
        Ok(out)
    }

    /// After a document was saved: drop the pictures it no longer names (rows and files).
    pub(super) async fn drop_unreferenced_media(
        &self,
        user: i64,
        doc: &ProfileDoc,
    ) -> PackageResult<()> {
        let (bg, gallery) = profile_doc::media_refs(doc);
        let keep: Vec<i64> = bg
            .iter()
            .chain(gallery.iter())
            .filter_map(|s| s.parse().ok())
            .collect();
        let gone: Vec<String> = sqlx::query_scalar(
            "DELETE FROM profile_media WHERE account_id = $1 AND id <> ALL($2) RETURNING object_key",
        )
        .bind(user)
        .bind(&keep)
        .fetch_all(self.pool())
        .await?;
        for key in &gone {
            self.remove_object(user, key).await;
        }
        Ok(())
    }

    /// The media of `user` that `doc` names, for a served page.
    pub(super) async fn media_named_by(
        &self,
        user: i64,
        doc: &ProfileDoc,
    ) -> PackageResult<Vec<MediaView>> {
        let (bg, gallery) = profile_doc::media_refs(doc);
        if bg.is_empty() && gallery.is_empty() {
            return Ok(Vec::new());
        }
        let named: BTreeSet<String> = bg.into_iter().chain(gallery).collect();
        Ok(self
            .media_list(user)
            .await?
            .into_iter()
            .filter(|m| named.contains(&m.id))
            .collect())
    }

    /// Remove references to deleted pictures from my saved document: gallery entries with those ids go,
    /// and a picture background falls back to a pattern.
    async fn strip_media_refs(
        &self,
        user: i64,
        gallery: &BTreeSet<String>,
        background: &BTreeSet<String>,
    ) {
        let Ok(Some(raw)) =
            sqlx::query_scalar::<_, Value>("SELECT profile_doc FROM accounts WHERE id = $1")
                .bind(user)
                .fetch_optional(self.pool())
                .await
        else {
            return;
        };
        let Ok(mut doc) = serde_json::from_value::<ProfileDoc>(raw) else {
            return;
        };
        let mut changed = false;
        if let Some(bg) = &doc.style.background
            && bg.image_id.as_ref().is_some_and(|i| background.contains(i))
        {
            doc.style.background = Some(profile_doc::Background {
                kind: "pattern".to_owned(),
                pattern: bg.pattern.clone(),
                image_id: None,
            });
            changed = true;
        }
        for block in doc
            .blocks
            .iter_mut()
            .filter(|b| b.kind == profile_doc::BlockKind::Gallery)
        {
            if let Some(items) = block.config.get_mut("items").and_then(Value::as_array_mut) {
                let before = items.len();
                items.retain(|i| {
                    !i.get("image_id")
                        .and_then(Value::as_str)
                        .is_some_and(|id| gallery.contains(id) || background.contains(id))
                });
                changed |= items.len() != before;
            }
        }
        if changed {
            let _ = sqlx::query(
                "UPDATE accounts SET profile_doc = $2, updated_at = now() WHERE id = $1",
            )
            .bind(user)
            .bind(serde_json::to_value(&doc).unwrap_or(Value::Null))
            .execute(self.pool())
            .await;
        }
    }

    /// Delete a stored picture and its cached variants (`<key>@...`). Best effort.
    pub(super) async fn remove_object(&self, user: i64, key: &str) {
        let Some(storage) = &self.inner.storage else {
            return;
        };
        let dir = format!("avatars/{user}");
        match storage.list(&dir).await {
            Ok(objects) => {
                let variant = format!("{key}@");
                for o in objects {
                    if (o.key == key || o.key.starts_with(&variant))
                        && let Err(e) = storage.delete(&o.key).await
                    {
                        tracing::warn!(error = %e, key = %o.key, "deleting a profile picture failed");
                    }
                }
            }
            Err(e) => tracing::warn!(error = %e, key, "listing profile picture variants failed"),
        }
    }

    /// Every picture key of `user`, for account deletion (the rows go with the account).
    pub async fn media_keys(&self, user: i64) -> PackageResult<Vec<String>> {
        Ok(
            sqlx::query_scalar("SELECT object_key FROM profile_media WHERE account_id = $1")
                .bind(user)
                .fetch_all(self.pool())
                .await?,
        )
    }
}
