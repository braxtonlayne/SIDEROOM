//! Assets, jobs and workers as the server stores them (CIN-ING-004, CIN-ING-009..011).
//!
//! These are the domain types of the media tables (migrations 0420..0423) and the
//! [`MediaRepository`] port the services in [`crate::service`] and [`crate::downloads`] are
//! written against. `sr-db` implements it over Postgres. Ids are Snowflakes and travel as
//! decimal strings in JSON; timestamps are Unix milliseconds.

use crate::plan::SpritePlan;
use crate::playlist::MediaSet;
use crate::retention::{AssetFacts, AssetState as RetentionState};
use crate::retranscode::AssetSummary;
use crate::worker_api::{
    JobKind, JobState, JobStore, ProgressRequest, Stage, StoreError, WorkerCapabilities,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use utoipa::ToSchema;

/// Serde adapters: `i64` ids as JSON strings (numbers accepted on input).
pub mod ids {
    pub mod id {
        use serde::{Deserialize, Deserializer, Serializer};

        pub fn serialize<S: Serializer>(v: &i64, s: S) -> Result<S::Ok, S::Error> {
            s.collect_str(v)
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
            #[derive(Deserialize)]
            #[serde(untagged)]
            enum Raw {
                S(String),
                N(i64),
            }
            match Raw::deserialize(d)? {
                Raw::S(s) => s.parse().map_err(serde::de::Error::custom),
                Raw::N(n) => Ok(n),
            }
        }
    }

    pub mod opt_id {
        use serde::{Deserialize, Deserializer, Serializer};

        pub fn serialize<S: Serializer>(v: &Option<i64>, s: S) -> Result<S::Ok, S::Error> {
            match v {
                Some(v) => s.collect_str(v),
                None => s.serialize_none(),
            }
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
            #[derive(Deserialize)]
            #[serde(untagged)]
            enum Raw {
                S(String),
                N(i64),
            }
            match Option::<Raw>::deserialize(d)? {
                None => Ok(None),
                Some(Raw::S(s)) => s.parse().map(Some).map_err(serde::de::Error::custom),
                Some(Raw::N(n)) => Ok(Some(n)),
            }
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MediaError {
    #[error("{0} not found")]
    NotFound(&'static str),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Conflict(String),
    /// The server cannot do this right now (no ffprobe, no storage presigning, ...), with the
    /// words an operator needs.
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    RateLimited(String),
    #[error("storage: {0}")]
    Storage(String),
    #[error("database: {0}")]
    Backend(String),
}

pub type MediaResult<T> = Result<T, MediaError>;

impl From<StoreError> for MediaError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::NotFound => Self::NotFound("job"),
            StoreError::Conflict(m) => Self::Conflict(m.to_owned()),
            StoreError::Backend(m) => Self::Backend(m),
        }
    }
}

// ---- assets -------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssetState {
    AwaitingUpload,
    Uploaded,
    Probing,
    ProxyReady,
    InReview,
    Approved,
    Transcoding,
    Published,
    Rejected,
    Removed,
    Failed,
    Expired,
}

impl AssetState {
    pub const ALL: [Self; 12] = [
        Self::AwaitingUpload,
        Self::Uploaded,
        Self::Probing,
        Self::ProxyReady,
        Self::InReview,
        Self::Approved,
        Self::Transcoding,
        Self::Published,
        Self::Rejected,
        Self::Removed,
        Self::Failed,
        Self::Expired,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::AwaitingUpload => "awaiting_upload",
            Self::Uploaded => "uploaded",
            Self::Probing => "probing",
            Self::ProxyReady => "proxy_ready",
            Self::InReview => "in_review",
            Self::Approved => "approved",
            Self::Transcoding => "transcoding",
            Self::Published => "published",
            Self::Rejected => "rejected",
            Self::Removed => "removed",
            Self::Failed => "failed",
            Self::Expired => "expired",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|x| x.as_str() == s)
    }

    /// The retention module's view of the state (`None` for slots that never got a file).
    pub fn retention(self) -> Option<RetentionState> {
        Some(match self {
            Self::Uploaded => RetentionState::Uploaded,
            Self::Probing => RetentionState::Probing,
            Self::ProxyReady => RetentionState::ProxyReady,
            Self::InReview => RetentionState::InReview,
            Self::Approved => RetentionState::Approved,
            Self::Transcoding => RetentionState::Transcoding,
            Self::Published => RetentionState::Published,
            Self::Rejected => RetentionState::Rejected,
            Self::Removed => RetentionState::Removed,
            Self::Failed => RetentionState::Failed,
            Self::AwaitingUpload | Self::Expired => return None,
        })
    }
}

/// One uploaded video and what became of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Asset {
    #[serde(with = "ids::id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "ids::opt_id", default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub title_id: Option<i64>,
    #[serde(with = "ids::opt_id", default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub episode_id: Option<i64>,
    #[serde(with = "ids::opt_id", default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub uploaded_by: Option<i64>,
    pub state: AssetState,
    /// Why the asset is `failed`, in words for staff.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub filename: String,
    pub original_size: i64,
    /// `hot` or `cold`.
    pub original_tier: String,
    /// The original file is gone (retention deleted it); a re-transcode needs a new upload.
    pub original_deleted: bool,
    /// Members with the download perk may fetch the original.
    pub original_downloads: bool,
    /// The ladder is queued right after the probe instead of waiting for approval.
    pub auto_ladder: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_version: Option<i32>,
    /// A review proxy exists.
    pub has_proxy: bool,
    /// The ladder is ready to play (renditions exist).
    pub playable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at_ms: Option<i64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    /// Storage key of the original. Internal.
    #[serde(skip)]
    #[schema(ignore)]
    pub original_key: String,
    #[serde(skip)]
    #[schema(ignore)]
    pub original_ext: String,
    /// `transcode/<job>/<attempt>` of the published outputs. Internal.
    #[serde(skip)]
    #[schema(ignore)]
    pub hls_prefix: Option<String>,
    #[serde(skip)]
    #[schema(ignore)]
    pub proxy_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewAsset {
    pub id: i64,
    pub title_id: Option<i64>,
    pub episode_id: Option<i64>,
    pub uploaded_by: Option<i64>,
    pub filename: String,
    pub original_key: String,
    pub original_ext: String,
    pub original_size: i64,
    pub auto_ladder: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AssetQuery {
    pub state: Option<AssetState>,
    pub title_id: Option<i64>,
    pub uploaded_by: Option<i64>,
    /// 1..=200.
    pub limit: u32,
    /// Keyset: only assets with an id below this.
    pub before: Option<i64>,
}

// ---- jobs, tokens, workers (admin views) ---------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct JobSummary {
    pub job_id: String,
    #[serde(with = "ids::id")]
    #[schema(value_type = String)]
    pub asset_id: i64,
    pub kind: JobKind,
    pub state: JobState,
    pub priority: u8,
    pub attempt: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_id: Option<String>,
    pub cancel_requested: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<ProgressRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub created_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct JobDetail {
    #[serde(flatten)]
    pub job: JobSummary,
    /// The worker's log tail (last 32 KiB), control characters removed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_tail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall_secs: Option<f64>,
    pub output_count: u32,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct JobQuery {
    pub state: Option<JobState>,
    pub kind: Option<JobKind>,
    pub asset_id: Option<i64>,
    pub limit: u32,
    pub before: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct QueueCounts {
    pub queued: u32,
    pub running: u32,
    pub failed_last_day: u32,
    pub succeeded_last_day: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct WorkerToken {
    #[serde(with = "ids::id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub name: String,
    #[serde(with = "ids::opt_id", default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub created_by: Option<i64>,
    pub created_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct WorkerView {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(with = "ids::id")]
    #[schema(value_type = String)]
    pub token_id: i64,
    pub token_name: String,
    pub token_revoked: bool,
    pub capabilities: WorkerCapabilities,
    pub last_seen_ms: i64,
    /// Heard from within the last minute.
    pub online: bool,
    pub drain: bool,
    pub active_jobs: u32,
}

// ---- playback ----------------------------------------------------------------------------

/// Everything the playback endpoint and the playlist renderer need about a ready asset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetPlayback {
    pub asset_id: i64,
    /// Unsigned CDN prefix of the outputs, `/cdn/hls/<job>/<attempt>` (no trailing slash).
    pub base_path: String,
    pub duration_secs: f64,
    pub profile_version: u32,
    pub media: MediaSet,
    /// Sprite sheets, when the ladder produced them (`sprites/sprites.vtt`).
    pub sprite: Option<SpritePlan>,
    /// Staff-uploaded subtitle files retained as ASS, by subtitle `vtt_path`.
    pub ass_paths: Vec<(String, String)>,
}

// ---- the port -----------------------------------------------------------------------------

/// Persistence of assets, jobs, workers and tokens. `sr-db` implements it; the worker API's
/// [`JobStore`] is a supertrait because the same rows serve both.
#[async_trait]
pub trait MediaRepository: JobStore {
    async fn create_asset(&self, a: &NewAsset) -> MediaResult<Asset>;
    async fn get_asset(&self, id: i64) -> MediaResult<Option<Asset>>;
    async fn list_assets(&self, q: &AssetQuery) -> MediaResult<Vec<Asset>>;
    /// Move to `to` when currently in one of `from`; `None` when missing or in another state.
    async fn transition_asset(
        &self,
        id: i64,
        from: &[AssetState],
        to: AssetState,
        error: Option<&str>,
    ) -> MediaResult<Option<Asset>>;
    /// `awaiting_upload` to `uploaded` with the stored size.
    async fn mark_uploaded(&self, id: i64, size: i64) -> MediaResult<Option<Asset>>;
    async fn record_probe(&self, id: i64, probe: &Value, duration_ms: i64) -> MediaResult<()>;
    async fn asset_probe(&self, id: i64) -> MediaResult<Option<Value>>;
    async fn link_asset(
        &self,
        id: i64,
        title: Option<i64>,
        episode: Option<i64>,
    ) -> MediaResult<Option<Asset>>;
    async fn set_original_downloads(&self, id: i64, on: bool) -> MediaResult<Option<Asset>>;
    /// Slots still `awaiting_upload` created before `before_ms`, marked `expired`.
    async fn expire_slots(&self, before_ms: i64, limit: u32) -> MediaResult<Vec<Asset>>;

    async fn list_jobs(&self, q: &JobQuery) -> MediaResult<Vec<JobSummary>>;
    async fn job_detail(&self, job_id: i64) -> MediaResult<Option<JobDetail>>;
    /// `failed` or `cancelled` back to `queued` with a fresh attempt counter.
    async fn retry_job(&self, job_id: i64) -> MediaResult<bool>;
    async fn queue_counts(&self) -> MediaResult<QueueCounts>;

    async fn create_token(
        &self,
        id: i64,
        name: &str,
        sha256: &str,
        created_by: i64,
    ) -> MediaResult<WorkerToken>;
    async fn list_tokens(&self) -> MediaResult<Vec<WorkerToken>>;
    /// Revoking also tells the token's workers to drain. `false` when unknown or already revoked.
    async fn revoke_token(&self, id: i64) -> MediaResult<bool>;
    async fn list_workers(&self) -> MediaResult<Vec<WorkerView>>;
    async fn set_worker_drain(&self, worker_id: &str, drain: bool) -> MediaResult<bool>;

    /// The playable form of a ready asset (`None` while no ladder exists).
    async fn playback(&self, asset_id: i64) -> MediaResult<Option<AssetPlayback>>;

    /// Facts for the retention sweep, oldest first, at most `limit`.
    async fn retention_facts(&self, limit: u32) -> MediaResult<Vec<AssetFacts>>;
    /// Assets in scope of a re-transcode request.
    async fn retranscode_summaries(
        &self,
        asset: Option<i64>,
        title: Option<i64>,
        limit: u32,
    ) -> MediaResult<Vec<AssetSummary>>;
    /// Forget the original (its object was deleted) and/or the proxy.
    async fn mark_original_deleted(&self, id: i64) -> MediaResult<()>;
    async fn mark_proxy_deleted(&self, id: i64) -> MediaResult<()>;
    async fn mark_cold(&self, id: i64) -> MediaResult<()>;
    /// Storage prefixes to collect later (a replaced ladder).
    /// `gc_due` leaves out prefixes some asset references again.
    async fn queue_gc(&self, prefixes: &[String], asset: Option<i64>) -> MediaResult<()>;
    async fn gc_due(&self, before_ms: i64, limit: u32) -> MediaResult<Vec<String>>;
    async fn gc_done(&self, prefixes: &[String]) -> MediaResult<()>;
}

/// Stage names for the admin UI.
pub fn stage_label(s: Stage) -> &'static str {
    match s {
        Stage::Download => "download",
        Stage::Video => "video",
        Stage::Audio => "audio",
        Stage::Subtitles => "subtitles",
        Stage::Sprites => "sprites",
        Stage::Upload => "upload",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_round_trip() {
        for s in AssetState::ALL {
            assert_eq!(AssetState::parse(s.as_str()), Some(s));
            let j = serde_json::to_string(&s).unwrap();
            assert_eq!(j, format!("\"{}\"", s.as_str()));
        }
        assert_eq!(AssetState::parse("nope"), None);
        assert!(AssetState::AwaitingUpload.retention().is_none());
        assert_eq!(
            AssetState::Published.retention(),
            Some(RetentionState::Published)
        );
    }

    #[test]
    fn ids_travel_as_strings() {
        let t = WorkerToken {
            id: 42,
            name: "gpu".into(),
            created_by: Some(7),
            created_at_ms: 1,
            last_used_at_ms: None,
            revoked_at_ms: None,
        };
        let j = serde_json::to_value(&t).unwrap();
        assert_eq!(j["id"], "42");
        assert_eq!(j["created_by"], "7");
        let back: WorkerToken = serde_json::from_value(j).unwrap();
        assert_eq!(back, t);
    }
}
