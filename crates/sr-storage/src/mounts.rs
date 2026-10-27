//! Storage mounts (BYOS-MNT-001): one S3-compatible bucket plus key prefix that a user, a guild or the
//! platform owns, the sealed credentials that open it, and the [`MountResolver`] every media read goes through.
//!
//! This module is the contract; it has no database. The Postgres implementation is
//! `sr_db::mounts_repo::PgMountRepo` (sr-db already depends on this crate through sr-media, and sr-db owns
//! the sealing key, so the SQL cannot live here without a crate cycle), and [`MemoryMountRepo`] is the
//! in-memory implementation that tests and the other agents build against.
//!
//! - [`Sealer`] seals and opens bytes under the server secret (`sr_db::sealed::SecretBox` implements it).
//! - [`SealedSecret`] is the secret access key at rest: opaque bytes whose associated data is
//!   [`mount_aad`]`(mount id)`, so a blob cannot be moved to another row. Its `Debug` never prints content.
//! - [`Secret`] is an opened secret: `Debug` is redacted; keep it short lived.
//! - [`Mount::storage`] builds the `object_store` client for the mount's provider.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use object_store::aws::AmazonS3Builder;

use crate::storage::ObjectStoreStorage;

/// The owner id stored for the platform (`owner_kind = platform`).
pub const PLATFORM_OWNER_ID: i64 = 0;
/// The instance bucket's mount id (BYOS-INS-001 seeds it).
pub const INSTANCE_MOUNT_ID: i64 = 1;

/// Errors from the mount layer.
#[derive(Debug, thiserror::Error)]
pub enum MountError {
    /// Sealing or opening failed (wrong key, wrong row, corruption).
    #[error("sealed secret is unreadable")]
    Unseal,
    /// The owner already has the maximum number of live mounts.
    #[error("mount cap reached ({0})")]
    CapReached(u32),
    /// A live mount of this owner already has this label.
    #[error("label already used")]
    LabelTaken,
    /// The mount id is already taken.
    #[error("mount id already exists")]
    IdTaken,
    /// The mount's fields cannot build a client.
    #[error("invalid mount: {0}")]
    Invalid(String),
    /// The backing store failed.
    #[error("mount repository error: {0}")]
    Repo(String),
}

/// Convenience alias.
pub type MountResult<T> = Result<T, MountError>;

/// Who owns a mount.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MountOwner {
    /// A member's own bucket (the id is the account id).
    User(i64),
    /// A server's bucket (the id is the guild id).
    Guild(i64),
    /// The instance itself (mount 1).
    Platform,
}

impl MountOwner {
    /// The `owner_kind` column value.
    pub fn kind(self) -> &'static str {
        match self {
            Self::User(_) => "user",
            Self::Guild(_) => "guild",
            Self::Platform => "platform",
        }
    }
    /// The `owner_id` column value.
    pub fn id(self) -> i64 {
        match self {
            Self::User(id) | Self::Guild(id) => id,
            Self::Platform => PLATFORM_OWNER_ID,
        }
    }
    /// Rebuild from the two columns.
    pub fn from_parts(kind: &str, id: i64) -> Option<Self> {
        match kind {
            "user" => Some(Self::User(id)),
            "guild" => Some(Self::Guild(id)),
            "platform" => Some(Self::Platform),
            _ => None,
        }
    }
}

macro_rules! text_enum {
    ($(#[$m:meta])* $name:ident { $($(#[$vm:meta])* $var:ident => $text:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name { $($(#[$vm])* $var),+ }
        impl $name {
            /// The column / wire value.
            pub fn as_str(self) -> &'static str { match self { $(Self::$var => $text),+ } }
            /// Parse the column / wire value.
            pub fn parse(s: &str) -> Option<Self> { match s { $($text => Some(Self::$var),)+ _ => None } }
        }
    };
}

text_enum! {
    /// The S3-compatible service behind a mount.
    Provider {
        /// Amazon S3.
        S3 => "s3",
        /// Cloudflare R2 (region `auto`).
        R2 => "r2",
        /// Backblaze B2's S3-compatible API (per-region endpoint).
        B2 => "b2",
        /// MinIO (path style).
        Minio => "minio",
        /// Wasabi (path style).
        Wasabi => "wasabi",
        /// Garage (path style).
        Garage => "garage",
    }
}

impl Provider {
    /// Whether the service wants path-style addressing by default (S3 and R2 take virtual-hosted style).
    pub fn default_path_style(self) -> bool {
        matches!(self, Self::Minio | Self::Wasabi | Self::Garage | Self::B2)
    }
    /// The region to sign with: R2 only accepts `auto`; others use what the owner entered.
    pub fn signing_region(self, entered: &str) -> String {
        if self == Self::R2 {
            "auto".to_owned()
        } else {
            entered.to_owned()
        }
    }
}

text_enum! {
    /// Health of a mount.
    MountStatus {
        /// In use.
        Active => "active",
        /// The last test or recheck failed.
        Failing => "failing",
        /// Switched off by the owner or staff.
        Disabled => "disabled",
        /// Taken down by staff (BYOS-LEG-001): nothing of the mount is served, whoever asks. Only staff clear it.
        Delisted => "delisted",
    }
}

/// An opened secret access key. `Debug` is redacted.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wrap a plaintext secret.
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    /// The plaintext. Never log or serialise it.
    pub fn expose(&self) -> &str {
        &self.0
    }
    /// The non-secret hint shown in the UI: the last four characters of `access_key_id`.
    pub fn key_hint(access_key_id: &str) -> String {
        let chars: Vec<char> = access_key_id.chars().collect();
        chars[chars.len().saturating_sub(4)..].iter().collect()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(..)")
    }
}

/// Seals and opens bytes under the server secret. `sr_db::sealed::SecretBox` is the production implementation.
pub trait Sealer: Send + Sync {
    /// Encrypt `plain` bound to `aad`.
    fn seal(&self, aad: &[u8], plain: &[u8]) -> Result<Vec<u8>, MountError>;
    /// Decrypt a blob made by [`Self::seal`] with the same `aad`.
    fn open(&self, aad: &[u8], blob: &[u8]) -> Result<Vec<u8>, MountError>;
}

/// The associated data that binds a sealed secret to its mount row.
pub fn mount_aad(mount_id: i64) -> Vec<u8> {
    format!("mount:{mount_id}").into_bytes()
}

/// A secret access key sealed at rest (`secret_key_enc`). `Debug` prints only the length.
#[derive(Clone, PartialEq, Eq)]
pub struct SealedSecret(Vec<u8>);

impl SealedSecret {
    /// Seal `secret` for mount `mount_id`.
    pub fn seal(sealer: &dyn Sealer, mount_id: i64, secret: &Secret) -> MountResult<Self> {
        sealer
            .seal(&mount_aad(mount_id), secret.expose().as_bytes())
            .map(Self)
    }
    /// Open it again.
    pub fn open(&self, sealer: &dyn Sealer, mount_id: i64) -> MountResult<Secret> {
        let plain = sealer.open(&mount_aad(mount_id), &self.0)?;
        String::from_utf8(plain)
            .map(Secret)
            .map_err(|_| MountError::Unseal)
    }
    /// Open with `sealer` (which may hold the previous keys) and seal again under its current key:
    /// the step `sr-cli rotate-secret-key` runs per row.
    pub fn reseal(&self, sealer: &dyn Sealer, mount_id: i64) -> MountResult<Self> {
        Self::seal(sealer, mount_id, &self.open(sealer, mount_id)?)
    }
    /// The stored bytes (the `secret_key_enc` column).
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
    /// Rebuild from the stored bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl fmt::Debug for SealedSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SealedSecret({} bytes)", self.0.len())
    }
}

/// The fields an owner enters (everything but the id and the secret).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountInput {
    /// Who owns it.
    pub owner: MountOwner,
    /// A label, 1 to 64 characters, unique per owner among live mounts (case-insensitive).
    pub label: String,
    /// The service.
    pub provider: Provider,
    /// The S3 endpoint URL.
    pub endpoint: String,
    /// The region (`auto` for R2).
    pub region: String,
    /// The bucket.
    pub bucket: String,
    /// Key prefix inside the bucket (empty for the whole bucket); no leading slash.
    pub prefix: String,
    /// Path-style addressing.
    pub path_style: bool,
    /// The access key id (not secret).
    pub access_key_id: String,
}

/// A row to insert: the input, the Snowflake id and the sealed secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMount {
    /// Application-generated id (the AAD of the sealed secret).
    pub id: i64,
    /// What the owner entered.
    pub input: MountInput,
    /// The secret access key, sealed with [`mount_aad`]`(id)`.
    pub secret_key_enc: SealedSecret,
    /// Last four characters of the access key id.
    pub key_hint: String,
}

impl NewMount {
    /// Seal `secret` for `id` and compute the hint.
    pub fn seal(
        sealer: &dyn Sealer,
        id: i64,
        input: MountInput,
        secret: &Secret,
    ) -> MountResult<Self> {
        Ok(Self {
            id,
            key_hint: Secret::key_hint(&input.access_key_id),
            secret_key_enc: SealedSecret::seal(sealer, id, secret)?,
            input,
        })
    }
}

/// Changes to a live mount; `None` leaves a field alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MountPatch {
    /// New label.
    pub label: Option<String>,
    /// New endpoint.
    pub endpoint: Option<String>,
    /// New region.
    pub region: Option<String>,
    /// New bucket.
    pub bucket: Option<String>,
    /// New prefix.
    pub prefix: Option<String>,
    /// New addressing style.
    pub path_style: Option<bool>,
    /// New access key id together with its sealed secret and hint (they change as a pair).
    pub credentials: Option<(String, SealedSecret, String)>,
    /// New status.
    pub status: Option<MountStatus>,
}

/// A mount row. The secret is sealed; there is no way to read it without a [`Sealer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    /// Id (Snowflake; 1 is the instance bucket).
    pub id: i64,
    /// Owner.
    pub owner: MountOwner,
    /// Label.
    pub label: String,
    /// Service.
    pub provider: Provider,
    /// Endpoint URL.
    pub endpoint: String,
    /// Region.
    pub region: String,
    /// Bucket.
    pub bucket: String,
    /// Key prefix inside the bucket.
    pub prefix: String,
    /// Path-style addressing.
    pub path_style: bool,
    /// Access key id.
    pub access_key_id: String,
    /// Sealed secret access key.
    pub secret_key_enc: SealedSecret,
    /// Last four characters of the access key id.
    pub key_hint: String,
    /// Health.
    pub status: MountStatus,
    /// When the CORS check last passed (unix ms); `None` means presigning is refused (409 `CORS_NOT_READY`).
    pub cors_ok_at: Option<i64>,
    /// When the connection test last ran (unix ms).
    pub last_test_at: Option<i64>,
    /// The last test's result rows, as JSON text.
    pub last_test: Option<String>,
    /// Created (unix ms).
    pub created_at: i64,
    /// Updated (unix ms).
    pub updated_at: i64,
}

impl Mount {
    /// The object key inside the bucket for a mount-relative `key`: the prefix joined with a single `/`.
    pub fn bucket_key(&self, key: &str) -> String {
        let prefix = self.prefix.trim_matches('/');
        let key = key.trim_start_matches('/');
        if prefix.is_empty() {
            key.to_owned()
        } else {
            format!("{prefix}/{key}")
        }
    }

    /// Build the `object_store` S3 client for this mount's provider. The client works on bucket keys:
    /// pass keys through [`Self::bucket_key`]. Presigning is local signing, no round trip.
    pub fn storage(&self, sealer: &dyn Sealer) -> MountResult<ObjectStoreStorage> {
        let secret = self.secret_key_enc.open(sealer, self.id)?;
        if !(self.endpoint.starts_with("https://") || self.endpoint.starts_with("http://")) {
            return Err(MountError::Invalid(
                "endpoint must be an http(s) URL".into(),
            ));
        }
        let s3 = Arc::new(
            AmazonS3Builder::new()
                .with_endpoint(&self.endpoint)
                .with_bucket_name(&self.bucket)
                .with_region(self.provider.signing_region(&self.region))
                .with_access_key_id(&self.access_key_id)
                .with_secret_access_key(secret.expose())
                .with_virtual_hosted_style_request(!self.path_style)
                // A dead endpoint must fail in seconds, not minutes. `allow_http` lives in the client options,
                // so it is set on them: a `with_allow_http` before `with_client_options` was overwritten and
                // every request to an http:// endpoint failed with "builder error" (storage.yml on PR #56).
                .with_client_options(
                    object_store::ClientOptions::new()
                        .with_allow_http(self.endpoint.starts_with("http://"))
                        .with_connect_timeout(std::time::Duration::from_secs(5))
                        .with_timeout(std::time::Duration::from_secs(8)),
                )
                .with_retry(object_store::RetryConfig {
                    backoff: Default::default(),
                    max_retries: 1,
                    retry_timeout: std::time::Duration::from_secs(10),
                })
                .build()
                .map_err(|e| MountError::Invalid(e.to_string()))?,
        );
        Ok(ObjectStoreStorage::new(s3.clone(), Some(s3)))
    }

    /// The signed multipart, copy, delete and raw-listing client of this mount (BYOS-BRW-001). Its calls take
    /// bucket keys: pass keys through [`Self::bucket_key`]. Requests give up after `timeout`.
    pub fn multipart(
        &self,
        sealer: &dyn Sealer,
        timeout: std::time::Duration,
    ) -> MountResult<crate::s3multipart::S3Multipart> {
        let secret = self.secret_key_enc.open(sealer, self.id)?;
        if !(self.endpoint.starts_with("https://") || self.endpoint.starts_with("http://")) {
            return Err(MountError::Invalid(
                "endpoint must be an http(s) URL".into(),
            ));
        }
        crate::s3multipart::S3Multipart::new(
            &self.endpoint,
            &self.bucket,
            &self.provider.signing_region(&self.region),
            &self.access_key_id,
            secret.expose(),
        )
        .map(|m| m.with_path_style(self.path_style).with_timeout(timeout))
        .map_err(|e| MountError::Invalid(e.to_string()))
    }
}

/// Finds mounts. Cinema, Library and chat media reads take this instead of one fixed storage handle.
#[async_trait]
pub trait MountResolver: Send + Sync {
    /// A live (not deleted) mount by id.
    async fn by_id(&self, id: i64) -> MountResult<Option<Mount>>;
    /// The live mounts of an owner, oldest first.
    async fn for_owner(&self, owner: MountOwner) -> MountResult<Vec<Mount>>;
}

/// Reads and writes mount rows. The cap is passed in so the count and the insert are one atomic step.
#[async_trait]
pub trait MountRepo: MountResolver {
    /// Insert; fails with [`MountError::CapReached`] when the owner already has `max_per_owner` live mounts
    /// (`None` means no cap, used for the platform mount), [`MountError::LabelTaken`] or [`MountError::IdTaken`].
    async fn create(&self, new: NewMount, max_per_owner: Option<u32>) -> MountResult<Mount>;
    /// Apply a patch to a live mount; `None` when it does not exist or is deleted.
    async fn update(&self, id: i64, patch: MountPatch) -> MountResult<Option<Mount>>;
    /// Soft delete; `false` when there was no live mount.
    async fn soft_delete(&self, id: i64) -> MountResult<bool>;

    /// Store a connection test result (`result_json`) at `at_ms` and, when given, set the status
    /// (a failed test sets `failing`; a clean one on a `failing` mount sets `active`). `false` when no live mount.
    async fn record_test(
        &self,
        id: i64,
        at_ms: i64,
        result_json: String,
        status: Option<MountStatus>,
    ) -> MountResult<bool>;
    /// Set or clear (`None`) when the CORS check last passed. `false` when no live mount.
    async fn set_cors_ok(&self, id: i64, at_ms: Option<i64>) -> MountResult<bool>;
    /// Every live mount that is not disabled, for the daily CORS recheck.
    async fn list_enabled(&self) -> MountResult<Vec<Mount>>;
    /// Every live mount, any status, platform included, for `rotate-secret-key`.
    async fn list_all(&self) -> MountResult<Vec<Mount>>;
    /// Replace only the sealed secret (key rotation). `false` when no live mount.
    async fn set_sealed(&self, id: i64, sealed: SealedSecret) -> MountResult<bool>;
}

type Rows = BTreeMap<i64, (Mount, bool)>;

/// In-memory [`MountRepo`] for tests and for building against the contract.
#[derive(Debug, Default)]
pub struct MemoryMountRepo {
    rows: Mutex<Rows>,
}

impl MemoryMountRepo {
    /// An empty repo.
    pub fn new() -> Self {
        Self::default()
    }
    /// An empty repo behind the trait object most callers hold.
    pub fn shared() -> Arc<dyn MountRepo> {
        Arc::new(Self::new())
    }
    fn lock(&self) -> MountResult<std::sync::MutexGuard<'_, Rows>> {
        self.rows
            .lock()
            .map_err(|_| MountError::Repo("poisoned".into()))
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

#[async_trait]
impl MountResolver for MemoryMountRepo {
    async fn by_id(&self, id: i64) -> MountResult<Option<Mount>> {
        Ok(self
            .lock()?
            .get(&id)
            .filter(|(_, deleted)| !deleted)
            .map(|(m, _)| m.clone()))
    }

    async fn for_owner(&self, owner: MountOwner) -> MountResult<Vec<Mount>> {
        let mut v: Vec<Mount> = self
            .lock()?
            .values()
            .filter(|(m, deleted)| !deleted && m.owner == owner)
            .map(|(m, _)| m.clone())
            .collect();
        v.sort_by_key(|m| (m.created_at, m.id));
        Ok(v)
    }
}

#[async_trait]
impl MountRepo for MemoryMountRepo {
    async fn create(&self, new: NewMount, max_per_owner: Option<u32>) -> MountResult<Mount> {
        let mut rows = self.lock()?;
        if rows.contains_key(&new.id) {
            return Err(MountError::IdTaken);
        }
        let owner = new.input.owner;
        let live = rows.values().filter(|(m, d)| !d && m.owner == owner);
        if let Some(max) = max_per_owner
            && live.clone().count() >= max as usize
        {
            return Err(MountError::CapReached(max));
        }
        let label = new.input.label.to_lowercase();
        if live.clone().any(|(m, _)| m.label.to_lowercase() == label) {
            return Err(MountError::LabelTaken);
        }
        let now = now_ms();
        let i = new.input;
        let mount = Mount {
            id: new.id,
            owner: i.owner,
            label: i.label,
            provider: i.provider,
            endpoint: i.endpoint,
            region: i.region,
            bucket: i.bucket,
            prefix: i.prefix,
            path_style: i.path_style,
            access_key_id: i.access_key_id,
            secret_key_enc: new.secret_key_enc,
            key_hint: new.key_hint,
            status: MountStatus::Active,
            cors_ok_at: None,
            last_test_at: None,
            last_test: None,
            created_at: now,
            updated_at: now,
        };
        rows.insert(mount.id, (mount.clone(), false));
        Ok(mount)
    }

    async fn update(&self, id: i64, patch: MountPatch) -> MountResult<Option<Mount>> {
        let mut rows = self.lock()?;
        let Some((owner, current_label)) = rows
            .get(&id)
            .filter(|(_, d)| !d)
            .map(|(m, _)| (m.owner, m.label.clone()))
        else {
            return Ok(None);
        };
        if let Some(label) = &patch.label {
            let want = label.to_lowercase();
            if want != current_label.to_lowercase()
                && rows
                    .values()
                    .any(|(m, d)| !d && m.owner == owner && m.label.to_lowercase() == want)
            {
                return Err(MountError::LabelTaken);
            }
        }
        let Some((m, _)) = rows.get_mut(&id) else {
            return Ok(None);
        };
        if let Some(v) = patch.label {
            m.label = v;
        }
        if let Some(v) = patch.endpoint {
            m.endpoint = v;
        }
        if let Some(v) = patch.region {
            m.region = v;
        }
        if let Some(v) = patch.bucket {
            m.bucket = v;
        }
        if let Some(v) = patch.prefix {
            m.prefix = v;
        }
        if let Some(v) = patch.path_style {
            m.path_style = v;
        }
        if let Some((key_id, enc, hint)) = patch.credentials {
            m.access_key_id = key_id;
            m.secret_key_enc = enc;
            m.key_hint = hint;
        }
        if let Some(v) = patch.status {
            m.status = v;
        }
        m.updated_at = now_ms();
        Ok(Some(m.clone()))
    }

    async fn soft_delete(&self, id: i64) -> MountResult<bool> {
        let mut rows = self.lock()?;
        Ok(match rows.get_mut(&id) {
            Some((_, deleted)) if !*deleted => {
                *deleted = true;
                true
            }
            _ => false,
        })
    }

    async fn record_test(
        &self,
        id: i64,
        at_ms: i64,
        result_json: String,
        status: Option<MountStatus>,
    ) -> MountResult<bool> {
        let mut rows = self.lock()?;
        Ok(match rows.get_mut(&id) {
            Some((m, false)) => {
                m.last_test_at = Some(at_ms);
                m.last_test = Some(result_json);
                if let Some(s) = status {
                    m.status = s;
                }
                m.updated_at = now_ms();
                true
            }
            _ => false,
        })
    }

    async fn set_cors_ok(&self, id: i64, at_ms: Option<i64>) -> MountResult<bool> {
        let mut rows = self.lock()?;
        Ok(match rows.get_mut(&id) {
            Some((m, false)) => {
                m.cors_ok_at = at_ms;
                m.updated_at = now_ms();
                true
            }
            _ => false,
        })
    }

    async fn list_enabled(&self) -> MountResult<Vec<Mount>> {
        Ok(self
            .lock()?
            .values()
            .filter(|(m, d)| !d && m.status != MountStatus::Disabled)
            .map(|(m, _)| m.clone())
            .collect())
    }

    async fn list_all(&self) -> MountResult<Vec<Mount>> {
        Ok(self
            .lock()?
            .values()
            .filter(|(_, d)| !d)
            .map(|(m, _)| m.clone())
            .collect())
    }

    async fn set_sealed(&self, id: i64, sealed: SealedSecret) -> MountResult<bool> {
        let mut rows = self.lock()?;
        Ok(match rows.get_mut(&id) {
            Some((m, false)) => {
                m.secret_key_enc = sealed;
                m.updated_at = now_ms();
                true
            }
            _ => false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test sealer: XOR with a key byte and bind the AAD with a checksum. Not secure; the real
    /// AES-GCM `SecretBox` is tested in sr-db.
    struct XorSealer(u8);

    fn tag(aad: &[u8]) -> u8 {
        aad.iter().fold(0u8, |a, b| a.wrapping_add(*b))
    }

    impl Sealer for XorSealer {
        fn seal(&self, aad: &[u8], plain: &[u8]) -> Result<Vec<u8>, MountError> {
            let mut out = vec![tag(aad)];
            out.extend(plain.iter().map(|b| b ^ self.0));
            Ok(out)
        }
        fn open(&self, aad: &[u8], blob: &[u8]) -> Result<Vec<u8>, MountError> {
            match blob.split_first() {
                Some((t, rest)) if *t == tag(aad) => Ok(rest.iter().map(|b| b ^ self.0).collect()),
                _ => Err(MountError::Unseal),
            }
        }
    }

    fn input(owner: MountOwner, label: &str) -> MountInput {
        MountInput {
            owner,
            label: label.into(),
            provider: Provider::Garage,
            endpoint: "http://127.0.0.1:3900".into(),
            region: "garage".into(),
            bucket: "shelf".into(),
            prefix: "media/".into(),
            path_style: true,
            access_key_id: "GK1234567890abcd".into(),
        }
    }

    fn new_mount(id: i64, owner: MountOwner, label: &str) -> NewMount {
        NewMount::seal(
            &XorSealer(0x5a),
            id,
            input(owner, label),
            &Secret::new("s3cr3t-value"),
        )
        .unwrap()
    }

    fn mount_for_keys(prefix: &str) -> Mount {
        let mut i = input(MountOwner::User(1), "x");
        i.prefix = prefix.into();
        let n = NewMount::seal(&XorSealer(1), 3, i, &Secret::new("k")).unwrap();
        Mount {
            id: n.id,
            owner: n.input.owner,
            label: n.input.label,
            provider: n.input.provider,
            endpoint: n.input.endpoint,
            region: n.input.region,
            bucket: n.input.bucket,
            prefix: n.input.prefix,
            path_style: n.input.path_style,
            access_key_id: n.input.access_key_id,
            secret_key_enc: n.secret_key_enc,
            key_hint: n.key_hint,
            status: MountStatus::Active,
            cors_ok_at: None,
            last_test_at: None,
            last_test: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn sealed_secret_round_trips_and_is_bound_to_the_row() {
        let s = XorSealer(7);
        let sealed = SealedSecret::seal(&s, 42, &Secret::new("hunter2")).unwrap();
        assert_eq!(sealed.open(&s, 42).unwrap().expose(), "hunter2");
        assert!(matches!(sealed.open(&s, 43), Err(MountError::Unseal)));
        assert!(SealedSecret::from_bytes(vec![]).open(&s, 42).is_err());
        let again = sealed.reseal(&s, 42).unwrap();
        assert_eq!(again.open(&s, 42).unwrap().expose(), "hunter2");
    }

    #[test]
    fn secrets_never_print() {
        let sealed = SealedSecret::seal(&XorSealer(1), 1, &Secret::new("topsecret")).unwrap();
        assert_eq!(format!("{:?}", Secret::new("topsecret")), "Secret(..)");
        assert!(format!("{sealed:?}").starts_with("SealedSecret("));
        let m = new_mount(9, MountOwner::User(1), "a");
        assert!(!format!("{m:?}").contains("s3cr3t-value"));
    }

    #[test]
    fn key_hint_is_the_last_four() {
        assert_eq!(Secret::key_hint("GK1234567890abcd"), "abcd");
        assert_eq!(Secret::key_hint("ab"), "ab");
    }

    #[test]
    fn provider_and_owner_round_trip_their_text() {
        for p in [
            Provider::S3,
            Provider::R2,
            Provider::B2,
            Provider::Minio,
            Provider::Wasabi,
            Provider::Garage,
        ] {
            assert_eq!(Provider::parse(p.as_str()), Some(p));
        }
        assert_eq!(Provider::parse("gcs"), None);
        assert_eq!(Provider::R2.signing_region("eu"), "auto");
        assert_eq!(Provider::S3.signing_region("eu-west-1"), "eu-west-1");
        for o in [
            MountOwner::User(5),
            MountOwner::Guild(6),
            MountOwner::Platform,
        ] {
            assert_eq!(MountOwner::from_parts(o.kind(), o.id()), Some(o));
        }
        assert_eq!(MountOwner::from_parts("team", 1), None);
    }

    #[test]
    fn bucket_key_joins_the_prefix_once() {
        let m = mount_for_keys("media/");
        assert_eq!(m.bucket_key("a/b.mp4"), "media/a/b.mp4");
        assert_eq!(m.bucket_key("/a"), "media/a");
        assert_eq!(mount_for_keys("").bucket_key("a"), "a");
    }

    #[test]
    fn storage_builds_a_client_and_refuses_a_foreign_seal() {
        let mut m = mount_for_keys("p/");
        let s = XorSealer(1);
        assert!(m.storage(&s).is_ok());
        m.id = 4; // sealed for id 3: the AAD no longer matches
        assert!(matches!(m.storage(&s), Err(MountError::Unseal)));
        let mut bad = mount_for_keys("");
        bad.endpoint = "ftp://x".into();
        assert!(matches!(bad.storage(&s), Err(MountError::Invalid(_))));
    }

    /// An http:// endpoint must reach the network: a refused connection, never reqwest's "builder error"
    /// (which is what an overwritten `allow_http` produced, before any byte was sent).
    #[tokio::test]
    async fn an_http_endpoint_is_allowed_to_connect() {
        use crate::storage::ObjectStorage;
        let mut m = mount_for_keys("p/");
        m.endpoint = "http://127.0.0.1:9".into();
        m.path_style = true;
        let st = m.storage(&XorSealer(1)).unwrap();
        let err = st
            .put(&m.bucket_key("a.txt"), bytes::Bytes::from_static(b"x"))
            .await
            .expect_err("nothing listens on port 9");
        let text = format!("{err:?}");
        assert!(!text.contains("builder error"), "{text}");
    }

    #[tokio::test]
    async fn memory_repo_resolves_lists_caps_and_soft_deletes() {
        let repo = MemoryMountRepo::new();
        let me = MountOwner::User(1);
        let a = repo
            .create(new_mount(10, me, "One"), Some(2))
            .await
            .unwrap();
        assert_eq!(a.status, MountStatus::Active);
        assert_eq!(a.key_hint, "abcd");
        assert!(matches!(
            repo.create(new_mount(11, me, "one"), Some(2)).await,
            Err(MountError::LabelTaken)
        ));
        assert!(matches!(
            repo.create(new_mount(10, me, "Other"), Some(2)).await,
            Err(MountError::IdTaken)
        ));
        repo.create(new_mount(12, me, "Two"), Some(2))
            .await
            .unwrap();
        assert!(matches!(
            repo.create(new_mount(13, me, "Three"), Some(2)).await,
            Err(MountError::CapReached(2))
        ));
        // Another owner is unaffected by the cap.
        repo.create(new_mount(14, MountOwner::Guild(1), "One"), Some(2))
            .await
            .unwrap();

        assert_eq!(repo.by_id(10).await.unwrap().unwrap().label, "One");
        assert_eq!(repo.for_owner(me).await.unwrap().len(), 2);
        assert!(repo.by_id(99).await.unwrap().is_none());

        let as_resolver: Arc<dyn MountResolver> = Arc::new(MemoryMountRepo::new());
        assert!(as_resolver.by_id(1).await.unwrap().is_none());

        let patched = repo
            .update(
                10,
                MountPatch {
                    status: Some(MountStatus::Failing),
                    prefix: Some("x/".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(patched.status, MountStatus::Failing);
        assert_eq!(patched.prefix, "x/");
        let clash = MountPatch {
            label: Some("TWO".into()),
            ..Default::default()
        };
        assert!(matches!(
            repo.update(10, clash).await,
            Err(MountError::LabelTaken)
        ));

        assert!(repo.soft_delete(10).await.unwrap());
        assert!(!repo.soft_delete(10).await.unwrap());
        assert!(repo.by_id(10).await.unwrap().is_none());
        assert!(
            repo.update(10, MountPatch::default())
                .await
                .unwrap()
                .is_none()
        );
        // The cap frees up and the label can be reused after a soft delete.
        repo.create(new_mount(15, me, "One"), Some(2))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn memory_repo_records_tests_cors_and_lists() {
        let repo = MemoryMountRepo::new();
        let me = MountOwner::User(1);
        repo.create(new_mount(10, me, "A"), None).await.unwrap();
        repo.create(new_mount(11, me, "B"), None).await.unwrap();
        assert!(
            repo.record_test(10, 5, "{}".into(), Some(MountStatus::Failing))
                .await
                .unwrap()
        );
        let m = repo.by_id(10).await.unwrap().unwrap();
        assert_eq!((m.status, m.last_test_at), (MountStatus::Failing, Some(5)));
        assert!(repo.set_cors_ok(11, Some(9)).await.unwrap());
        assert_eq!(repo.by_id(11).await.unwrap().unwrap().cors_ok_at, Some(9));
        let off = MountPatch {
            status: Some(MountStatus::Disabled),
            ..Default::default()
        };
        repo.update(11, off).await.unwrap();
        assert_eq!(repo.list_enabled().await.unwrap().len(), 1);
        assert_eq!(repo.list_all().await.unwrap().len(), 2);
        let s = SealedSecret::seal(&XorSealer(1), 10, &Secret::new("n")).unwrap();
        assert!(repo.set_sealed(10, s).await.unwrap());
        assert!(
            !repo
                .set_sealed(99, SealedSecret::from_bytes(vec![]))
                .await
                .unwrap()
        );
    }
}
