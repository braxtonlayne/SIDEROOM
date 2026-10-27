//! The catalog indexer of a storage mount (BYOS-CAT-001).
//!
//! The `storage.index` job lists a mount (page and time capped like `storage.measure`), classifies every key from
//! its folder, its file name and an optional `.sideroom.json` sidecar ([`classify`]), and hands the result to an
//! [`IndexStore`], which writes `storage_objects` and the catalog rows (`source = 'mount'`). Everything here is
//! pure: no I/O, no JSON, no database. The walker and the Postgres store live in `sr-server` and `sr-db`, so this
//! crate stays below `sr-media` (whose `naming` module parses video names; it reaches the classifier through the
//! [`NameParser`] trait).
//!
//! The one allowed header read (the first 1 MiB of a file, for container facts) is [`sniff`].

pub mod classify;
pub mod shelf;
pub mod sniff;

pub use classify::{
    ArtRole, Class, Classified, NameParser, Options, Sidecar, SidecarMap, VideoFacts, VideoRole,
    classify, classify_all, is_sidecar_name,
};

use async_trait::async_trait;

/// The job kind of the indexer (`storage.index`): a one-shot job carries `{"mount_id": n}`, the daily singleton none.
pub const JOB_KIND: &str = "storage.index";

/// The folder sidecar file name.
pub const SIDECAR_NAME: &str = ".sideroom.json";

/// Most objects one run lists before it stops and says so (a lower bound), the default of the job's cap.
pub const DEFAULT_MAX_OBJECTS: u64 = 200_000;

/// How many classified objects the store takes in one call.
pub const APPLY_BATCH: usize = 500;

/// What one run did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunReport {
    /// Objects listed.
    pub objects: u64,
    /// Objects that are new or changed since the last run.
    pub added: u64,
    /// Objects that were there and are gone.
    pub vanished: u64,
    /// The walk stopped at the cap or the budget; `vanished` is then not computed (nothing is hidden on a partial
    /// listing).
    pub truncated: bool,
}

/// Where a run writes. Implemented over Postgres by `sr_db::index_repo` and over memory in tests.
#[async_trait]
pub trait IndexStore: Send + Sync {
    /// Start a run for `mount_id`; returns the run number (monotonic per mount).
    async fn begin(&self, mount_id: i64, now_ms: i64) -> Result<i64, StoreError>;
    /// Upsert a batch: `storage_objects` rows seen in `run` and the catalog rows they build. Returns how many of
    /// the batch were new or changed.
    async fn apply(&self, mount_id: i64, run: i64, batch: &[Classified])
    -> Result<u64, StoreError>;
    /// Which of `keys` a takedown delisted on this mount (BYOS-LEG-001). The indexer leaves them out, so a delisted
    /// object never gets a catalog row and one indexed earlier is hidden as vanished. Fails closed: an error stops
    /// the run.
    async fn delisted(&self, _mount_id: i64, _keys: &[String]) -> Result<Vec<String>, StoreError> {
        Ok(Vec::new())
    }
    /// End a run. With `complete` the objects of the mount that `run` did not see are marked vanished and their
    /// catalog rows hidden; returns how many vanished. A partial run (`complete = false`) hides nothing.
    async fn finish(
        &self,
        mount_id: i64,
        run: i64,
        report: &RunReport,
        complete: bool,
        error: Option<&str>,
        now_ms: i64,
    ) -> Result<u64, StoreError>;
}

/// The store failed; the run stops and records the message.
#[derive(Debug, thiserror::Error)]
#[error("index store: {0}")]
pub struct StoreError(pub String);

/// The last index run of a mount, as the Re-index button shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunStatus {
    /// Run number.
    pub run: i64,
    /// `idle`, `running`, `done` or `failed`.
    pub state: String,
    /// Start, unix ms.
    pub started_ms: Option<i64>,
    /// End, unix ms.
    pub finished_ms: Option<i64>,
    /// Objects listed.
    pub objects: i32,
    /// New or changed objects.
    pub added: i32,
    /// Objects that vanished.
    pub vanished: i32,
    /// The listing stopped at the cap or the budget.
    pub truncated: bool,
    /// The failure, when `state` is `failed`.
    pub error: Option<String>,
}

/// Changes made through the bucket browser (BYOS-BRW-001), applied to the index at once instead of at the next
/// run (wired at the wave-28 integration). Both are called after the bucket call succeeded.
#[async_trait]
pub trait ObjectChanges: Send + Sync {
    /// An object, or a folder of them (a key ending in `/`), was deleted. Returns how many objects were hidden.
    async fn deleted(&self, mount_id: i64, key: &str) -> Result<u64, StoreError>;
    /// An object or a folder was moved from `from` to `to`. Returns how many objects were re-keyed.
    async fn moved(&self, mount_id: i64, from: &str, to: &str) -> Result<u64, StoreError>;
}

/// What the routes use to ask for a run and read the last one.
#[async_trait]
pub trait IndexControl: Send + Sync {
    /// Queue a run for `mount_id` unless one is already queued or running. `true` when a job was queued.
    async fn request(&self, mount_id: i64) -> Result<bool, StoreError>;
    /// The last run of the mount; `None` before the first.
    async fn status(&self, mount_id: i64) -> Result<Option<RunStatus>, StoreError>;
}
