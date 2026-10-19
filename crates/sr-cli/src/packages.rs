//! `sr-cli package seed <dir>`: install a starter pack that ships in the repository (M-03, D-289).
//!
//! The folder is validated with the same rules as an upload, then installed as an approved layer-4
//! `sticker_pack` package authored by the system user (so it shows in the package directory like any other)
//! and as the platform sticker pack the picker reads (`sticker_packs` plus standard `stickers` rows, G-44).
//! The version row carries `"seeded": true` in its manifest. Running it again changes nothing: the package,
//! the version and the stickers are found by slug and name, and only text or image bytes that differ are
//! rewritten.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use sqlx::PgPool;
use sr_db::auth_repo::IdSource;
use sr_packages::service::PackageService;
use sr_packages::{Checks, PackageKind, zipfiles};
use sr_storage::ObjectStorage;

/// The system user (`accounts.id = 1`) authors seeded packages.
pub const SYSTEM_USER_ID: i64 = 1;
/// Starter packs sort before the packs an operator adds.
const SORT_ORDER: i32 = -100;

/// What a seed run did.
#[derive(Debug, PartialEq, Eq)]
pub struct Seeded {
    pub slug: String,
    pub version: String,
    /// A package version was created and approved by this run.
    pub new_version: bool,
    pub stickers: usize,
    pub new_stickers: usize,
}

/// Files of a package folder as they are installed: the authoring sources under `src/` (SVG drawings) are not
/// part of the package.
pub fn installable(mut files: BTreeMap<String, Vec<u8>>) -> BTreeMap<String, Vec<u8>> {
    files.retain(|path, _| !path.starts_with("src/"));
    files
}

fn format_type(bytes: &[u8]) -> i16 {
    if bytes.starts_with(b"GIF8") {
        4
    } else if bytes.windows(4).take(64).any(|w| w == b"acTL") {
        2
    } else {
        1
    }
}

fn extension(format: i16) -> &'static str {
    match format {
        4 => "gif",
        _ => "png",
    }
}

/// Install (or refresh) the starter pack made of `files`.
pub async fn seed_pack(
    pool: &PgPool,
    storage: Arc<dyn ObjectStorage>,
    ids: IdSource,
    files: BTreeMap<String, Vec<u8>>,
) -> Result<Seeded> {
    let files = installable(files);
    let v = sr_packages::validate::validate(&files, &Checks::lenient())
        .map_err(|problems| anyhow::anyhow!("the pack is not valid:\n{}", problems.join("\n")))?;
    if v.kind != PackageKind::StickerPack {
        bail!(
            "package seed installs sticker packs; this is a {}",
            v.kind.as_str()
        );
    }
    let author: Option<i64> = sqlx::query_scalar("SELECT id FROM accounts WHERE id = $1")
        .bind(SYSTEM_USER_ID)
        .fetch_optional(pool)
        .await?;
    if author.is_none() {
        bail!("the system user does not exist yet: run `sr-cli bootstrap` first");
    }

    // 1. The package and its approved, seeded version.
    let service = PackageService::new(pool.clone(), ids.clone()).with_storage(storage.clone());
    let existing: Option<(i64, i64)> =
        sqlx::query_as("SELECT id, author_id FROM packages WHERE slug = $1")
            .bind(&v.slug)
            .fetch_optional(pool)
            .await?;
    if let Some((_, owner)) = existing
        && owner != SYSTEM_USER_ID
    {
        bail!(
            "the id {} belongs to another author; a starter pack must be authored by the system user",
            v.slug
        );
    }
    let version_known: bool = match existing {
        Some((package_id, _)) => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM package_versions WHERE package_id = $1 AND version = $2)",
        )
        .bind(package_id)
        .bind(&v.version)
        .fetch_one(pool)
        .await?,
        None => false,
    };
    if !version_known {
        let entries: Vec<(&str, &[u8])> = files
            .iter()
            .map(|(p, d)| (p.as_str(), d.as_slice()))
            .collect();
        let zip = zipfiles::build_zip(&entries);
        let view = service
            .submit(SYSTEM_USER_ID, zip, &Checks::lenient())
            .await
            .map_err(|e| anyhow::anyhow!("storing the package: {e}"))?;
        let version_id: i64 = view.id.parse().context("version id")?;
        let package_id: i64 = view.package_id.parse().context("package id")?;
        let mut tx = pool.begin().await?;
        sqlx::query(
            "UPDATE package_versions SET review_state = 'approved', reviewed_by = $2, reviewed_at = now(), \
             review_note = 'Seeded starter pack', manifest = manifest || '{\"seeded\": true}'::jsonb \
             WHERE id = $1",
        )
        .bind(version_id)
        .bind(SYSTEM_USER_ID)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE packages SET latest_version_id = (\
               SELECT id FROM package_versions WHERE package_id = $1 AND review_state = 'approved' \
               ORDER BY id DESC LIMIT 1) WHERE id = $1",
        )
        .bind(package_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
    }
    let package_id: i64 = sqlx::query_scalar("SELECT id FROM packages WHERE slug = $1")
        .bind(&v.slug)
        .fetch_one(pool)
        .await?;

    // 2. The platform pack the picker reads.
    let package_key = package_id.to_string();
    let pack_id: i64 = match sqlx::query_scalar::<_, i64>(
        "SELECT id FROM sticker_packs WHERE package_id = $1",
    )
    .bind(&package_key)
    .fetch_optional(pool)
    .await?
    {
        Some(id) => {
            sqlx::query("UPDATE sticker_packs SET name = $2, description = $3 WHERE id = $1")
                .bind(id)
                .bind(&v.name)
                .bind(Some(v.description.as_str()).filter(|d| !d.is_empty()))
                .execute(pool)
                .await?;
            id
        }
        None => {
            let id = ids();
            sqlx::query(
                "INSERT INTO sticker_packs (id, name, description, sort_order, package_id, published) \
                 VALUES ($1, $2, $3, $4, $5, TRUE)",
            )
            .bind(id)
            .bind(&v.name)
            .bind(Some(v.description.as_str()).filter(|d| !d.is_empty()))
            .bind(SORT_ORDER)
            .bind(&package_key)
            .execute(pool)
            .await?;
            id
        }
    };
    let items = v
        .payload
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut kept = BTreeSet::new();
    let mut new_stickers = 0;
    let mut cover: Option<i64> = None;
    for item in &items {
        let (Some(name), Some(file)) = (
            item.get("name").and_then(Value::as_str),
            item.get("file").and_then(Value::as_str),
        ) else {
            continue;
        };
        let bytes = files.get(file).cloned().unwrap_or_default();
        let tags = item
            .get("tags")
            .and_then(Value::as_array)
            .map(|t| {
                t.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| name.replace('_', " "));
        let description = item.get("description").and_then(Value::as_str);
        let format = format_type(&bytes);
        let found: Option<(i64, String)> =
            sqlx::query_as("SELECT id, media_key FROM stickers WHERE pack_id = $1 AND name = $2")
                .bind(pack_id)
                .bind(name)
                .fetch_optional(pool)
                .await?;
        let id = match found {
            Some((id, key)) => {
                // Same key, so a changed image replaces the old bytes without a new id.
                storage
                    .put(&key, bytes.into())
                    .await
                    .map_err(|e| anyhow::anyhow!("storing {file}: {e}"))?;
                sqlx::query(
                    "UPDATE stickers SET description = $2, tags = $3, format_type = $4, updated_at = now() \
                     WHERE id = $1",
                )
                .bind(id)
                .bind(description)
                .bind(&tags)
                .bind(format)
                .execute(pool)
                .await?;
                id
            }
            None => {
                let id = ids();
                let key = format!("sticker/pack/{pack_id}/{id}.{}", extension(format));
                storage
                    .put(&key, bytes.into())
                    .await
                    .map_err(|e| anyhow::anyhow!("storing {file}: {e}"))?;
                sqlx::query(
                    "INSERT INTO stickers (id, guild_id, pack_id, name, description, tags, format_type, \
                     available, creator_id, media_key) VALUES ($1, NULL, $2, $3, $4, $5, $6, TRUE, $7, $8)",
                )
                .bind(id)
                .bind(pack_id)
                .bind(name)
                .bind(description)
                .bind(&tags)
                .bind(format)
                .bind(SYSTEM_USER_ID)
                .bind(&key)
                .execute(pool)
                .await?;
                new_stickers += 1;
                id
            }
        };
        cover.get_or_insert(id);
        kept.insert(name.to_owned());
    }
    // A sticker that left the pack goes away; its object stays until the sweep job (messages may point at it).
    let all: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, name FROM stickers WHERE pack_id = $1")
            .bind(pack_id)
            .fetch_all(pool)
            .await?;
    for (id, name) in all {
        if !kept.contains(&name) {
            sqlx::query("DELETE FROM stickers WHERE id = $1")
                .bind(id)
                .execute(pool)
                .await?;
        }
    }
    sqlx::query("UPDATE sticker_packs SET cover_sticker_id = $2 WHERE id = $1")
        .bind(pack_id)
        .bind(cover)
        .execute(pool)
        .await?;

    Ok(Seeded {
        slug: v.slug,
        version: v.version,
        new_version: !version_known,
        stickers: kept.len(),
        new_stickers,
    })
}

/// A one-line audit detail for the platform audit log.
pub fn audit_detail(s: &Seeded) -> String {
    json!({
        "package": s.slug, "version": s.version, "new_version": s.new_version,
        "stickers": s.stickers, "new_stickers": s.new_stickers,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_db::testing::TestDb;
    use sr_storage::ObjectStoreStorage;
    use std::path::Path;

    fn folder() -> BTreeMap<String, Vec<u8>> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/starter-packs/side-room-words");
        let mut out = BTreeMap::new();
        fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
            for e in std::fs::read_dir(dir).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    walk(root, &p, out);
                } else {
                    let rel = p
                        .strip_prefix(root)
                        .unwrap()
                        .components()
                        .map(|c| c.as_os_str().to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join("/");
                    out.insert(rel, std::fs::read(&p).unwrap());
                }
            }
        }
        walk(&root, &root, &mut out);
        out
    }

    #[test]
    fn the_shipped_folder_validates_as_a_sticker_pack_of_sixteen() {
        let files = installable(folder());
        assert!(files.keys().all(|k| !k.starts_with("src/")));
        let v = sr_packages::validate::validate(&files, &Checks::lenient())
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(v.kind, PackageKind::StickerPack);
        assert_eq!(v.slug, "side-room-words");
        assert_eq!(v.payload["items"].as_array().unwrap().len(), 16);
        // The SVG sources and the licence are in the folder; the sources are not installed.
        assert!(folder().keys().any(|k| k.starts_with("src/")));
        assert!(files.contains_key("LICENSE"));
    }

    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn seeding_twice_changes_nothing_the_second_time() {
        let db = TestDb::new().await.unwrap();
        let pool = db.pool().clone();
        sqlx::query("INSERT INTO persons (id) VALUES (1)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO accounts (id, person_id, username, email, password_hash) \
             VALUES (1, 1, 'sideroom', 'system@example.test', 'x')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let next = Arc::new(std::sync::atomic::AtomicI64::new(5_000));
        let ids: IdSource = {
            let next = next.clone();
            Arc::new(move || next.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
        };
        let storage: Arc<dyn ObjectStorage> = Arc::new(ObjectStoreStorage::in_memory());

        let first = seed_pack(&pool, storage.clone(), ids.clone(), folder())
            .await
            .unwrap();
        assert!(first.new_version);
        assert_eq!((first.stickers, first.new_stickers), (16, 16));
        let second = seed_pack(&pool, storage.clone(), ids.clone(), folder())
            .await
            .unwrap();
        assert!(!second.new_version);
        assert_eq!((second.stickers, second.new_stickers), (16, 0));

        let count = |sql: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query_scalar::<_, i64>(sql)
                    .fetch_one(&pool)
                    .await
                    .unwrap()
            }
        };
        assert_eq!(count("SELECT count(*) FROM packages").await, 1);
        assert_eq!(count("SELECT count(*) FROM package_versions").await, 1);
        assert_eq!(count("SELECT count(*) FROM sticker_packs").await, 1);
        assert_eq!(
            count("SELECT count(*) FROM stickers WHERE guild_id IS NULL").await,
            16
        );
        let (state, seeded, reviewer): (String, bool, i64) = sqlx::query_as(
            "SELECT review_state, (manifest->>'seeded')::boolean, reviewed_by FROM package_versions",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((state.as_str(), seeded, reviewer), ("approved", true, 1));
        let latest: Option<i64> = sqlx::query_scalar("SELECT latest_version_id FROM packages")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(latest.is_some());
        let (cover, name): (Option<i64>, String) =
            sqlx::query_as("SELECT cover_sticker_id, name FROM sticker_packs")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(cover.is_some());
        assert_eq!(name, "Side Room Words");
        // Every sticker's image is in storage, once.
        let keys: Vec<String> = sqlx::query_scalar("SELECT media_key FROM stickers")
            .fetch_all(&pool)
            .await
            .unwrap();
        for k in &keys {
            assert!(storage.get(k).await.is_ok(), "{k}");
        }
        assert_eq!(storage.list("sticker/pack/").await.unwrap().len(), 16);
        db.cleanup().await.unwrap();
    }
}
