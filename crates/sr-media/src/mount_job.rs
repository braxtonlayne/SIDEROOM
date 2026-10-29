//! The `mount_transcode` job (BYOS-CNV-003): `sr-transcoder` as an optional worker that reads a member's
//! bucket and writes its outputs back into it.
//!
//! The server never carries the bytes. It hands the worker a presigned `GET` for the source and, one file at
//! a time, a presigned `PUT` for each output object (the worker never holds bucket credentials); the outputs
//! land under `transcoded/<key>/` in the same mount. This module holds the pieces that are not about the job
//! queue: the member's choices ([`MountFormat`], [`MountPreset`]), the local planning the worker does after
//! it has downloaded the source ([`plan_for`]; the server has no probe of a mount object, because probing
//! would mean reading the whole file), the output key rule, and [`MountTransfer`], the port the worker
//! service signs through, with [`MountBucket`] as the production implementation over the mount resolver.

use crate::plan::{
    JobPlan, PlanError, ProxyPlan, TranscodeSettings, plan_ladder_job, plan_proxy_job,
};
use crate::probe::ProbeResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sr_storage::guard::EndpointGuard;
use sr_storage::mounts::{MountResolver, MountStatus, Sealer};
use sr_storage::{ObjectStorage, PresignMethod};
use std::sync::Arc;
use std::time::Duration;
use utoipa::ToSchema;

/// The folder (mount-relative) every output goes under, followed by the source key and `/`.
pub const OUTPUT_ROOT: &str = "transcoded";

/// What the member wants out of the job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MountFormat {
    /// An HLS ladder (fMP4 segments, one playlist per rung, subtitles, sprites).
    Hls,
    /// One faststart MP4.
    Mp4,
}

impl MountFormat {
    /// The wire / column name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hls => "hls",
            Self::Mp4 => "mp4",
        }
    }
    fn extension(self) -> &'static str {
        match self {
            Self::Hls => ".m3u8",
            Self::Mp4 => ".mp4",
        }
    }
}

/// The ladder preset: the tallest rung the job produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MountPreset {
    /// Up to 480p.
    Low,
    /// Up to 720p.
    Standard,
    /// Up to 1080p.
    High,
}

impl MountPreset {
    /// The wire / column name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Standard => "standard",
            Self::High => "high",
        }
    }
    /// The tallest output in pixels.
    pub fn max_height(self) -> u32 {
        match self {
            Self::Low => 480,
            Self::Standard => 720,
            Self::High => 1080,
        }
    }
    fn mp4_kbps(self) -> u32 {
        match self {
            Self::Low => 1500,
            Self::Standard => 3000,
            Self::High => 6000,
        }
    }
}

/// The settings a preset stands for: the default ladder cut at the preset's height. A single MP4 is the
/// "proxy" encoder at that height and bitrate.
pub fn settings_for(preset: MountPreset) -> TranscodeSettings {
    TranscodeSettings {
        max_height: preset.max_height(),
        proxy_height: preset.max_height(),
        proxy_kbps: preset.mp4_kbps(),
        ..TranscodeSettings::default()
    }
}

/// What the worker runs: the plan its format asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum LocalPlan {
    /// An HLS ladder.
    Ladder(Box<JobPlan>),
    /// One MP4.
    Proxy(Box<ProxyPlan>),
}

/// Plan a mount job from the local probe of the downloaded source.
pub fn plan_for(
    probe: &ProbeResult,
    format: MountFormat,
    preset: MountPreset,
) -> Result<LocalPlan, PlanError> {
    let settings = settings_for(preset);
    Ok(match format {
        MountFormat::Hls => LocalPlan::Ladder(Box::new(plan_ladder_job(probe, &settings)?)),
        MountFormat::Mp4 => LocalPlan::Proxy(Box::new(plan_proxy_job(probe, &settings)?)),
    })
}

/// The mount-relative key of one output of the job for source `key`: `transcoded/<key>/<path>`. Both parts
/// have been vetted (`safe_key` for the source, `validate_output_path` for the path).
pub fn output_object_key(key: &str, path: &str) -> String {
    format!("{OUTPUT_ROOT}/{key}/{path}")
}

/// Whether a finished job's outputs are what its format promises: an MP4 job produced an `.mp4`, an HLS job a
/// playlist. (The worker plans locally, so the server checks the shape, not the file list.)
pub fn outputs_fit(format: MountFormat, paths: &[&str]) -> bool {
    entry_of(format, paths).is_some()
}

/// The entry a player opens, from a finished job's output paths: the single MP4, or the first playlist.
pub fn entry_of(format: MountFormat, paths: &[&str]) -> Option<String> {
    let ext = format.extension();
    let mut hits: Vec<&str> = paths
        .iter()
        .copied()
        .filter(|p| p.to_ascii_lowercase().ends_with(ext))
        .collect();
    hits.sort_unstable();
    hits.first().map(|p| (*p).to_owned())
}

/// Signing and checking objects of a mount for the worker service. Keys are mount-relative.
#[async_trait]
pub trait MountTransfer: Send + Sync {
    /// A presigned `GET` of an existing object. `Err` is a reason fit for a log, not for a member.
    async fn presign_get(&self, mount_id: i64, key: &str, ttl_secs: u64) -> Result<String, String>;
    /// A presigned `PUT` (one request, no multipart) of a new object.
    async fn presign_put(&self, mount_id: i64, key: &str, ttl_secs: u64) -> Result<String, String>;
    /// The size of an object, `None` when it is not there.
    async fn head_size(&self, mount_id: i64, key: &str) -> Result<Option<u64>, String>;
}

/// [`MountTransfer`] over the mount resolver: the mount's sealed credentials sign the URLs. Only active
/// mounts are served; the endpoint guard (when set) vets the member's endpoint before the one network call
/// ([`head_size`](MountTransfer::head_size)).
pub struct MountBucket {
    mounts: Arc<dyn MountResolver>,
    sealer: Arc<dyn Sealer>,
    guard: Option<Arc<dyn EndpointGuard>>,
}

impl MountBucket {
    /// Over the resolver and sealer of the storage routes.
    pub fn new(mounts: Arc<dyn MountResolver>, sealer: Arc<dyn Sealer>) -> Self {
        Self {
            mounts,
            sealer,
            guard: None,
        }
    }

    /// Vet the endpoint before a network call.
    #[must_use]
    pub fn with_guard(mut self, guard: Arc<dyn EndpointGuard>) -> Self {
        self.guard = Some(guard);
        self
    }

    async fn open(
        &self,
        mount_id: i64,
        key: &str,
    ) -> Result<(sr_storage::mounts::Mount, sr_storage::ObjectStoreStorage), String> {
        sr_storage::path::safe_key(key, false).map_err(|e| format!("key refused: {e}"))?;
        let mount = self
            .mounts
            .by_id(mount_id)
            .await
            .map_err(|e| format!("mount lookup: {e}"))?
            .ok_or_else(|| "mount not found".to_owned())?;
        if mount.status != MountStatus::Active {
            return Err("mount is not active".to_owned());
        }
        let storage = mount
            .storage(&*self.sealer)
            .map_err(|e| format!("mount client: {e}"))?;
        Ok((mount, storage))
    }
}

#[async_trait]
impl MountTransfer for MountBucket {
    async fn presign_get(&self, mount_id: i64, key: &str, ttl_secs: u64) -> Result<String, String> {
        let (mount, storage) = self.open(mount_id, key).await?;
        storage
            .presign(
                PresignMethod::Get,
                &mount.bucket_key(key),
                Duration::from_secs(ttl_secs),
            )
            .await
            .map_err(|e| e.to_string())
    }

    async fn presign_put(&self, mount_id: i64, key: &str, ttl_secs: u64) -> Result<String, String> {
        let (mount, storage) = self.open(mount_id, key).await?;
        storage
            .presign(
                PresignMethod::Put,
                &mount.bucket_key(key),
                Duration::from_secs(ttl_secs),
            )
            .await
            .map_err(|e| e.to_string())
    }

    async fn head_size(&self, mount_id: i64, key: &str) -> Result<Option<u64>, String> {
        let (mount, storage) = self.open(mount_id, key).await?;
        if let Some(g) = &self.guard {
            g.validated_addrs(&mount.endpoint).await?;
        }
        match tokio::time::timeout(Duration::from_secs(8), storage.head(&mount.bucket_key(key)))
            .await
        {
            Ok(Ok(info)) => Ok(Some(info.size)),
            Ok(Err(sr_storage::StorageError::NotFound(_))) => Ok(None),
            Ok(Err(e)) => Err(e.to_string()),
            Err(_) => Err("timed out".to_owned()),
        }
    }
}

/// An in-memory [`MountTransfer`] for tests: fake URLs that name the mount, key and method, and a set of
/// objects that "exist".
#[derive(Default)]
pub struct MemoryMountTransfer {
    /// `(mount id, key)` to size.
    pub objects: std::sync::Mutex<std::collections::BTreeMap<(i64, String), u64>>,
    /// Every signing request, in order: `(method, mount id, key)`.
    pub signed: std::sync::Mutex<Vec<(&'static str, i64, String)>>,
}

impl MemoryMountTransfer {
    /// Put an object in the fake bucket.
    pub fn put(&self, mount_id: i64, key: &str, size: u64) {
        if let Ok(mut o) = self.objects.lock() {
            o.insert((mount_id, key.to_owned()), size);
        }
    }
}

#[async_trait]
impl MountTransfer for MemoryMountTransfer {
    async fn presign_get(&self, mount_id: i64, key: &str, _ttl: u64) -> Result<String, String> {
        if let Ok(mut s) = self.signed.lock() {
            s.push(("GET", mount_id, key.to_owned()));
        }
        Ok(format!(
            "https://bucket.invalid/{mount_id}/{key}?X-Method=GET"
        ))
    }
    async fn presign_put(&self, mount_id: i64, key: &str, _ttl: u64) -> Result<String, String> {
        if let Ok(mut s) = self.signed.lock() {
            s.push(("PUT", mount_id, key.to_owned()));
        }
        Ok(format!(
            "https://bucket.invalid/{mount_id}/{key}?X-Method=PUT"
        ))
    }
    async fn head_size(&self, mount_id: i64, key: &str) -> Result<Option<u64>, String> {
        Ok(self
            .objects
            .lock()
            .map_err(|e| e.to_string())?
            .get(&(mount_id, key.to_owned()))
            .copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::H264_SDR;
    use crate::probe::parse_ffprobe_json;

    #[test]
    fn output_keys_sit_under_transcoded_and_the_source_key() {
        assert_eq!(
            output_object_key("Films/a b.mkv", "720p/index.m3u8"),
            "transcoded/Films/a b.mkv/720p/index.m3u8"
        );
    }

    #[test]
    fn a_preset_cuts_the_ladder() {
        let probe = parse_ffprobe_json(H264_SDR.as_bytes()).unwrap();
        let LocalPlan::Ladder(low) = plan_for(&probe, MountFormat::Hls, MountPreset::Low).unwrap()
        else {
            panic!("hls plans a ladder")
        };
        assert!(!low.rungs.is_empty());
        assert!(low.rungs.iter().all(|r| r.height <= 480), "{:?}", low.rungs);
        let LocalPlan::Ladder(high) =
            plan_for(&probe, MountFormat::Hls, MountPreset::High).unwrap()
        else {
            panic!("hls plans a ladder")
        };
        assert!(high.rungs.len() > low.rungs.len());
    }

    #[test]
    fn mp4_plans_one_file() {
        let probe = parse_ffprobe_json(H264_SDR.as_bytes()).unwrap();
        let LocalPlan::Proxy(p) =
            plan_for(&probe, MountFormat::Mp4, MountPreset::Standard).unwrap()
        else {
            panic!("mp4 plans one file")
        };
        assert!(p.output.ends_with(".mp4"));
        assert!(p.height <= 720);
    }

    #[test]
    fn outputs_must_fit_the_format() {
        assert!(outputs_fit(
            MountFormat::Hls,
            &["720p/a.m3u8", "720p/s1.m4s"]
        ));
        assert!(!outputs_fit(MountFormat::Hls, &["out.mp4"]));
        assert!(outputs_fit(MountFormat::Mp4, &["proxy/out.MP4"]));
        assert!(!outputs_fit(MountFormat::Mp4, &["a.m3u8"]));
        assert_eq!(
            entry_of(MountFormat::Hls, &["b/x.m3u8", "a/x.m3u8", "a/i.mp4"]).as_deref(),
            Some("a/x.m3u8")
        );
        assert_eq!(entry_of(MountFormat::Mp4, &["a.m3u8"]), None);
    }
}
