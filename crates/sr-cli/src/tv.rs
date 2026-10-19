//! `sr-cli tv slate` (L-14, D-269): make the Live TV slate loop and register it.
//!
//! ffmpeg synthesises a short colour-bars (or moving test pattern) clip with a -20 dBFS tone from `lavfi`
//! sources only (`sr_transcoder::slate::slate_source_args`: no input file, nothing downloaded). The MP4 is then
//! ingested like any original: stored under `originals/<id>/source.mp4`, recorded as a system-owned asset (no
//! title, so it is not in the Cinema catalogue and no guide entry can use it) and queued for the ordinary
//! probe and transcode jobs (standard ladder, standard segment length). When the renditions are ready the
//! asset id is written to the platform setting `tv.slate_asset`, which a running server follows without a
//! restart. A second run replaces the asset and removes the previous one.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use serde_json::json;
use sr_admin::{AdminService, Caller, SYSTEM_USER_ID, settings::TV_SLATE_ASSET};
use sr_core::config::Config;
use sr_db::PgPool;
use sr_db::admin_repo::PgAdminRepo;
use sr_db::jobs::{self, NewJob};
use sr_db::media_repo::{IdSource, PgMediaRepo};
use sr_media::assets::{AssetState, MediaRepository, NewAsset};
use sr_storage::ObjectStorage;
use sr_transcoder::slate::{SlatePicture, slate_source_args};

/// The probe job kind a running server handles (`sr_server::media_jobs::PROBE_KIND`; sr-cli cannot depend on the
/// server crate, a test of the server keeps the two equal).
pub const PROBE_KIND: &str = "cinema.probe";
const PROBE_ATTEMPTS: i32 = 3;
/// Size of the slate: the standard top rung, so the whole ladder can be cut from it.
pub const SLATE_SIZE: (u32, u32) = (1920, 1080);
pub const SLATE_FPS: u32 = 30;
/// How long to wait for the transcode before giving up (the asset stays queued).
pub const DEFAULT_WAIT_SECS: u64 = 1800;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Picture {
    /// SMPTE colour bars.
    Bars,
    /// A moving test pattern, shows that the stream is alive.
    Moving,
}

impl From<Picture> for SlatePicture {
    fn from(p: Picture) -> Self {
        match p {
            Picture::Bars => SlatePicture::Bars,
            Picture::Moving => SlatePicture::Moving,
        }
    }
}

/// What `tv slate` was asked to do.
#[derive(Debug, Clone)]
pub struct SlateOptions {
    pub picture: Picture,
    pub seconds: u32,
    pub ffmpeg: String,
    /// How long to wait for the transcode; `None` registers the asset right away (the server plays it once its
    /// renditions are ready, and answers off air until then).
    pub wait: Option<Duration>,
}

/// The ffmpeg arguments for the slate source written to `out`.
pub fn ffmpeg_args(picture: Picture, seconds: u32, out: &str) -> Result<Vec<String>> {
    slate_source_args(picture.into(), out, SLATE_SIZE, SLATE_FPS, seconds)
        .map_err(|e| anyhow::anyhow!("the slate settings are not usable: {e:?}"))
}

/// Why the slate cannot be made here, in words for the operator; `None` when it can.
pub fn refusal(storage_endpoint: &str, ffmpeg_found: bool, ffmpeg: &str) -> Option<String> {
    if storage_endpoint == "memory:" {
        return Some(
            "storage.endpoint is memory:, so the slate would not outlive this command; point storage at the bucket the server uses"
                .into(),
        );
    }
    if !ffmpeg_found {
        return Some(format!(
            "ffmpeg was not found ({ffmpeg}); install it on this machine (it is not needed on the web server) or pass --ffmpeg <path>"
        ));
    }
    None
}

/// Whether `ffmpeg -version` runs.
pub async fn ffmpeg_found(ffmpeg: &str) -> bool {
    let ffmpeg = ffmpeg.to_owned();
    tokio::task::spawn_blocking(move || {
        std::process::Command::new(ffmpeg)
            .arg("-version")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    })
    .await
    .unwrap_or(false)
}

async fn render(ffmpeg: &str, picture: Picture, seconds: u32, dir: &Path) -> Result<Vec<u8>> {
    let out = dir.join("slate.mp4");
    let out_str = out.to_string_lossy().into_owned();
    let args = ffmpeg_args(picture, seconds, &out_str)?;
    let bin = ffmpeg.to_owned();
    let run = tokio::task::spawn_blocking(move || {
        std::process::Command::new(bin)
            .args(&args)
            .stdin(std::process::Stdio::null())
            .output()
    })
    .await
    .context("running ffmpeg")?
    .with_context(|| format!("starting {ffmpeg}"))?;
    if !run.status.success() {
        let tail: String = String::from_utf8_lossy(&run.stderr)
            .lines()
            .rev()
            .take(5)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        bail!("ffmpeg failed ({}):\n{tail}", run.status);
    }
    std::fs::read(&out).with_context(|| format!("reading {out_str}"))
}

/// Store `bytes` as the original of a new system-owned asset, as an upload would leave it: object stored, asset
/// `uploaded` (no title, so outside the Cinema catalogue), probe job queued. Returns the asset id.
pub async fn ingest_original(
    pool: &PgPool,
    storage: &dyn ObjectStorage,
    ids: &IdSource,
    bytes: Vec<u8>,
) -> Result<i64> {
    let repo = PgMediaRepo::new(pool.clone(), ids.clone());
    let id = (ids)();
    let key = format!("originals/{id}/source.mp4");
    let size = i64::try_from(bytes.len()).context("the slate is too large")?;
    storage
        .put(&key, bytes.into())
        .await
        .context("storing the slate original")?;
    repo.create_asset(&NewAsset {
        id,
        title_id: None,
        episode_id: None,
        uploaded_by: Some(SYSTEM_USER_ID),
        filename: "slate.mp4".into(),
        original_key: key,
        original_ext: "mp4".into(),
        original_size: size,
        auto_ladder: true,
    })
    .await
    .map_err(|e| anyhow::anyhow!("recording the slate asset: {e}"))?;
    repo.mark_uploaded(id, size)
        .await
        .map_err(|e| anyhow::anyhow!("recording the slate asset: {e}"))?
        .context("the slate asset changed state")?;
    let mut job = NewJob::new((ids)(), PROBE_KIND, json!({ "asset": id }));
    job.max_attempts = PROBE_ATTEMPTS;
    jobs::enqueue(pool, &job)
        .await
        .context("queueing the probe job")?;
    Ok(id)
}

/// Wait until the asset has renditions to play. A failed or removed asset, or the deadline, is an error that says
/// what to do.
pub async fn wait_playable(
    pool: &PgPool,
    ids: &IdSource,
    id: i64,
    wait: Duration,
    poll: Duration,
) -> Result<()> {
    let repo = PgMediaRepo::new(pool.clone(), ids.clone());
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        let a = repo
            .get_asset(id)
            .await
            .map_err(|e| anyhow::anyhow!("reading the slate asset: {e}"))?
            .context("the slate asset disappeared")?;
        if a.playable {
            return Ok(());
        }
        if matches!(
            a.state,
            AssetState::Failed | AssetState::Rejected | AssetState::Removed
        ) {
            bail!(
                "the slate asset {id} ended up {:?}: {}",
                a.state,
                a.error.unwrap_or_default()
            );
        }
        if tokio::time::Instant::now() >= deadline {
            bail!(
                "the slate asset {id} is still {:?} after {} s; it stays queued. When it is ready, set the {TV_SLATE_ASSET} setting to {id} (Admin -> Limits) or run this command again",
                a.state,
                wait.as_secs()
            );
        }
        tokio::time::sleep(poll).await;
    }
}

/// The asset id the setting names now, if any.
pub async fn current_slate(admin: &AdminService) -> Result<Option<i64>> {
    let text = admin
        .setting_value_text(TV_SLATE_ASSET)
        .await
        .map_err(|e| anyhow::anyhow!("reading the {TV_SLATE_ASSET} setting: {e}"))?;
    Ok(text.trim().parse::<i64>().ok().filter(|n| *n > 0))
}

/// Write `id` to the `tv.slate_asset` setting (through the settings registry, so it is validated and audited)
/// and retire the asset that was the slate before. Returns the retired id.
pub async fn register(pool: &PgPool, ids: &IdSource, id: i64) -> Result<Option<i64>> {
    let admin = AdminService::new(Arc::new(PgAdminRepo::new(pool.clone(), ids.clone())));
    let previous = current_slate(&admin).await?.filter(|old| *old != id);
    admin
        .patch_settings(
            &Caller::operator(SYSTEM_USER_ID),
            &[(TV_SLATE_ASSET.to_owned(), json!(id.to_string()))],
            Some("sr-cli tv slate"),
        )
        .await
        .map_err(|e| anyhow::anyhow!("writing the {TV_SLATE_ASSET} setting: {e}"))?;
    if let Some(old) = previous {
        retire(pool, ids, old).await;
    }
    Ok(previous)
}

/// Retire a slate asset: state `removed`; retention deletes the files after its grace period.
async fn retire(pool: &PgPool, ids: &IdSource, asset: i64) {
    let repo = PgMediaRepo::new(pool.clone(), ids.clone());
    let _ = repo
        .transition_asset(
            asset,
            &[
                AssetState::Uploaded,
                AssetState::Probing,
                AssetState::ProxyReady,
                AssetState::InReview,
                AssetState::Approved,
                AssetState::Transcoding,
                AssetState::Published,
                AssetState::Failed,
            ],
            AssetState::Removed,
            None,
        )
        .await;
}

/// What `tv slate --remove` did.
#[derive(Debug, PartialEq, Eq)]
pub struct Removal {
    /// The asset that was the slate and is now retired; `None` when the setting was empty already.
    pub removed: Option<i64>,
    /// The `SR_TV_SLATE_ASSET` fallback, when set: the server keeps using it until the variable is unset.
    pub env_fallback: Option<i64>,
}

/// `tv slate --remove` (D-286): clear `tv.slate_asset` through the settings registry (audited, as the system user)
/// and retire the asset the way a replacement retires the previous one. With the setting empty already it does
/// nothing. The environment fallback is reported, never touched.
pub async fn remove_slate(
    pool: &PgPool,
    ids: &IdSource,
    env_fallback: Option<i64>,
) -> Result<Removal> {
    let admin = AdminService::new(Arc::new(PgAdminRepo::new(pool.clone(), ids.clone())));
    let current = current_slate(&admin).await?;
    if let Some(id) = current {
        admin
            .patch_settings(
                &Caller::operator(SYSTEM_USER_ID),
                &[(TV_SLATE_ASSET.to_owned(), json!(""))],
                Some("sr-cli tv slate --remove"),
            )
            .await
            .map_err(|e| anyhow::anyhow!("clearing the {TV_SLATE_ASSET} setting: {e}"))?;
        retire(pool, ids, id).await;
    }
    Ok(Removal {
        removed: current,
        env_fallback,
    })
}

/// The sentences `tv slate --remove` prints.
pub fn removal_message(r: &Removal) -> String {
    let mut out = match r.removed {
        Some(id) => format!(
            "slate cleared; asset {id} retired (its files are deleted after the retention grace period)"
        ),
        None => "no slate is set; nothing to remove".to_owned(),
    };
    if let Some(env) = r.env_fallback {
        out.push_str(&format!(
            "\nSR_TV_SLATE_ASSET={env} is still set in the environment, so the server keeps playing that asset; unset the variable (and restart) to finish"
        ));
    }
    out
}

/// A slate of another loop length numbers its segments differently, so a client that is on the slate while it is
/// replaced may see a renumbering (D-285); say so when the lengths differ or are not both known yet.
async fn loop_length_warning(pool: &PgPool, ids: &IdSource, old: i64, new: i64) -> Option<String> {
    let repo = PgMediaRepo::new(pool.clone(), ids.clone());
    let len = |id| {
        let repo = &repo;
        async move {
            repo.get_asset(id)
                .await
                .ok()
                .flatten()
                .and_then(|a| a.duration_ms)
        }
    };
    let (a, b) = (len(old).await, len(new).await);
    (a.is_none() || a != b).then(|| {
        "note: the new loop may differ in length from the old one; clients on the slate at this moment may see the segment numbers jump once".to_owned()
    })
}

/// Make the slate, ingest it, wait for the renditions, register it and retire the previous one. Returns the new
/// asset id.
pub async fn make_slate(
    pool: &PgPool,
    cfg: &Config,
    ids: IdSource,
    opts: &SlateOptions,
) -> Result<i64> {
    let found = ffmpeg_found(&opts.ffmpeg).await;
    if let Some(why) = refusal(&cfg.storage.endpoint, found, &opts.ffmpeg) {
        bail!("{why}");
    }
    let dir = std::env::temp_dir().join(format!("sr-slate-{}", (ids)()));
    std::fs::create_dir_all(&dir).context("creating a scratch directory")?;
    let bytes = render(&opts.ffmpeg, opts.picture, opts.seconds, &dir).await;
    let _ = std::fs::remove_dir_all(&dir);
    let bytes = bytes?;

    let s = &cfg.storage;
    let storage = sr_storage::ObjectStoreStorage::s3(
        &s.endpoint,
        &s.bucket,
        &s.region,
        s.access_key.expose(),
        s.secret_key.expose(),
    )
    .context("configuring object storage")?;
    let id = ingest_original(pool, &storage, &ids, bytes).await?;
    println!(
        "slate asset {id} stored; waiting for the transcode (needs sr-server and a transcoder worker running)"
    );
    if let Some(wait) = opts.wait {
        wait_playable(pool, &ids, id, wait, Duration::from_secs(3)).await?;
    }
    if let Some(old) = register(pool, &ids, id).await? {
        println!("previous slate asset {old} removed");
        if let Some(w) = loop_length_warning(pool, &ids, old, id).await {
            println!("{w}");
        }
    }
    println!("{id}");
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arguments_are_a_1080p_30fps_lavfi_source() {
        let a = ffmpeg_args(Picture::Bars, 12, "/tmp/slate.mp4").unwrap();
        assert!(a.contains(&"smptebars=size=1920x1080:rate=30:duration=12".to_owned()));
        assert!(a.contains(&"volume=-20dB".to_owned()));
        assert_eq!(a.last().unwrap(), "/tmp/slate.mp4");
        let m = ffmpeg_args(Picture::Moving, 6, "o.mp4").unwrap();
        assert!(m.iter().any(|x| x.starts_with("testsrc2=size=1920x1080")));
        // The length is bounded by the source builder.
        assert!(ffmpeg_args(Picture::Bars, 0, "o.mp4").is_err());
        assert!(ffmpeg_args(Picture::Bars, 601, "o.mp4").is_err());
    }

    #[test]
    fn it_refuses_without_ffmpeg_or_with_memory_storage() {
        let no_ffmpeg = refusal("http://localhost:9000", false, "ffmpeg").unwrap();
        assert!(no_ffmpeg.contains("ffmpeg was not found"), "{no_ffmpeg}");
        assert!(no_ffmpeg.contains("--ffmpeg"));
        let memory = refusal("memory:", true, "ffmpeg").unwrap();
        assert!(memory.contains("memory:"), "{memory}");
        assert_eq!(refusal("http://localhost:9000", true, "ffmpeg"), None);
    }

    #[tokio::test]
    async fn a_missing_binary_is_not_found() {
        assert!(!ffmpeg_found("definitely-not-a-real-ffmpeg-binary").await);
    }
}

#[cfg(test)]
mod pg_tests {
    use super::*;
    use sr_core::SystemClock;
    use sr_storage::ObjectStoreStorage;

    fn ids() -> IdSource {
        let gen_ = Arc::new(
            sr_core::SnowflakeGenerator::new(SystemClock, 1, 9).expect("snowflake generator"),
        );
        Arc::new(move || gen_.generate().get() as i64)
    }

    /// The slate is ingested as a system-owned asset with a probe queued, registered through the settings
    /// registry, and a second slate replaces it and removes the first. No ffmpeg runs.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn the_slate_is_ingested_registered_and_replaced() {
        let db = sr_db::testing::TestDb::new().await.expect("test db");
        let pool = db.pool().clone();
        let ids = ids();
        let person = (ids)();
        sqlx::query("INSERT INTO persons (id) VALUES ($1)")
            .bind(person)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $2, 'sideroom', 'system@example.test', 'x')",
        )
        .bind(SYSTEM_USER_ID)
        .bind(person)
        .execute(&pool)
        .await
        .unwrap();
        let storage = ObjectStoreStorage::in_memory();
        let repo = PgMediaRepo::new(pool.clone(), ids.clone());

        let first = ingest_original(&pool, &storage, &ids, b"not really an mp4".to_vec())
            .await
            .unwrap();
        let asset = repo.get_asset(first).await.unwrap().unwrap();
        assert_eq!(asset.state, AssetState::Uploaded);
        assert_eq!((asset.title_id, asset.episode_id), (None, None));
        assert_eq!(asset.uploaded_by, Some(SYSTEM_USER_ID));
        assert!(storage.get(&asset.original_key).await.is_ok());
        let queued: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM jobs WHERE kind = $1 AND payload->>'asset' = $2",
        )
        .bind(PROBE_KIND)
        .bind(first.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(queued, 1, "the probe job is queued for the server");
        // Not transcoded yet: waiting ends with advice, not a hang.
        let err = wait_playable(
            &pool,
            &ids,
            first,
            Duration::from_millis(10),
            Duration::from_millis(5),
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("still") && err.contains(TV_SLATE_ASSET),
            "{err}"
        );

        // Registered through the registry; nothing to retire the first time.
        assert_eq!(register(&pool, &ids, first).await.unwrap(), None);
        let admin = AdminService::new(Arc::new(PgAdminRepo::new(pool.clone(), ids.clone())));
        assert_eq!(current_slate(&admin).await.unwrap(), Some(first));

        // A second run replaces it and retires the first.
        let second = ingest_original(&pool, &storage, &ids, b"another".to_vec())
            .await
            .unwrap();
        assert_eq!(register(&pool, &ids, second).await.unwrap(), Some(first));
        assert_eq!(current_slate(&admin).await.unwrap(), Some(second));
        let old = repo.get_asset(first).await.unwrap().unwrap();
        assert_eq!(old.state, AssetState::Removed);
        // Registering the same asset again retires nothing.
        assert_eq!(register(&pool, &ids, second).await.unwrap(), None);

        // --remove clears the setting and retires the asset; the environment fallback is only reported.
        let done = remove_slate(&pool, &ids, Some(77)).await.unwrap();
        assert_eq!(
            done,
            Removal {
                removed: Some(second),
                env_fallback: Some(77)
            }
        );
        assert_eq!(current_slate(&admin).await.unwrap(), None);
        let gone = repo.get_asset(second).await.unwrap().unwrap();
        assert_eq!(gone.state, AssetState::Removed);
        assert!(removal_message(&done).contains("SR_TV_SLATE_ASSET=77"));
        // A second --remove is a no-op that says so.
        let again = remove_slate(&pool, &ids, None).await.unwrap();
        assert_eq!(
            again,
            Removal {
                removed: None,
                env_fallback: None
            }
        );
        assert!(removal_message(&again).contains("nothing to remove"));
        db.cleanup().await.unwrap();
    }
}
