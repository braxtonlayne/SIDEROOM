//! Shelf access (STU-ZK-001, P28-SHELF-1/2, P28-LEG-1; D-393, D-394): a shelf is a storage mount whose
//! credentials stay sealed on the server. A grantee reads one object through a presigned URL that lives
//! ten minutes ([`URL_TTL_SECS`]); the catalog is plaintext so a single object can be delisted.
//!
//! What this protects: other members (no grant, no listing, no URL) and grantees (they never hold a
//! credential; a leaked URL is one object for ten minutes). What it does not: the server, the operator or
//! an attacker who gets `sr-server` can read the credentials and the catalog. The mitigation is the
//! read-only, prefix-scoped key the owner supplies. See `docs/15-threat-model.md` T32.
//!
//! The database side is `sr_db::shelf_access_repo::PgShelfAccess`; this module is the contract.

use async_trait::async_trait;

use crate::scopes::FactsError;

/// The lifetime of a per-object URL, in seconds (P28-SHELF-1). Fixed: neither the settings nor a request
/// can raise it; a session with less left shortens it.
pub const URL_TTL_SECS: u64 = 600;

/// One cataloged object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    /// The shelf (mount) id.
    pub shelf_id: i64,
    /// Hex sha256 of the path.
    pub object_hash: String,
    /// Mount-relative key.
    pub path: String,
    /// Size in bytes.
    pub size: i64,
    /// `cinema`, `library`, `music` or none.
    pub kind: Option<String>,
    /// The classifier's class (`video`, `audio`, `reading`, `image`, ...).
    pub class: String,
    /// The owner's title override, empty when none.
    pub owner_title: String,
    /// The owner's cover override (a key in the same shelf), empty when none.
    pub owner_cover: String,
    /// The owner's note, empty when none.
    pub owner_note: String,
    /// Whether the object is listed (still in the bucket at its path).
    pub listed: bool,
    /// Whether staff delisted it (per-object takedown).
    pub delisted: bool,
    /// Last change (unix ms).
    pub updated_ms: i64,
}

/// A grant row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantRow {
    /// Grant id.
    pub id: i64,
    /// The shelf.
    pub shelf_id: i64,
    /// `friend` or `guild_role`.
    pub grantee_kind: String,
    /// The friend's account id, or the role id.
    pub grantee_id: i64,
    /// The role's guild.
    pub guild_id: Option<i64>,
    /// Who granted it.
    pub granted_by: i64,
    /// When (unix ms).
    pub created_ms: i64,
    /// When revoked (unix ms), `None` while live.
    pub revoked_ms: Option<i64>,
}

/// How a shelf reaches the viewer, for the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShelfLabel {
    /// The shelf (mount) id.
    pub shelf_id: i64,
    /// The mount's label.
    pub label: String,
    /// `user`, `guild` or `platform`.
    pub owner_kind: String,
    /// The owner's id (0 for the platform).
    pub owner_id: i64,
    /// The owner's username when a user owns it.
    pub owner_handle: Option<String>,
    /// Cataloged, listed, not delisted objects.
    pub object_count: i64,
}

/// One grant a viewer holds through a role or friendship, as the access decision reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShelfGrantFact {
    /// `true` for a guild role grant.
    pub guild_role: bool,
    /// The friend's id (always the viewer for a friend grant) or the role id.
    pub grantee_id: i64,
    /// The role's guild.
    pub guild_id: Option<i64>,
}

/// A grant could not be created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GrantError {
    /// The same grantee already holds a live grant.
    #[error("already granted")]
    Duplicate,
    /// No such shelf, role or account.
    #[error("not found")]
    Missing,
}

/// The shelf catalog and grants.
#[async_trait]
pub trait ShelfAccessStore: Send + Sync {
    /// Mounts that might reach `viewer` (own, shared, granted, server). Each is still decided by
    /// [`crate::scopes::access`]; this only narrows the scan.
    async fn candidate_shelves(&self, viewer: i64) -> Result<Vec<i64>, FactsError>;
    /// Labels, owners and counts for the shelves.
    async fn labels(&self, shelf_ids: &[i64]) -> Result<Vec<ShelfLabel>, FactsError>;
    /// Names of the guilds.
    async fn guild_names(&self, guild_ids: &[i64]) -> Result<Vec<(i64, String)>, FactsError>;
    /// One page of the catalog, ordered by `object_hash`: listed objects that are not delisted (neither by
    /// the shelf-object flag nor by an upheld takedown item).
    async fn catalog_page(
        &self,
        shelf_id: i64,
        after: Option<&str>,
        limit: i64,
    ) -> Result<Vec<CatalogEntry>, FactsError>;
    /// One object, delisted or not.
    async fn catalog_object(
        &self,
        shelf_id: i64,
        object_hash: &str,
    ) -> Result<Option<CatalogEntry>, FactsError>;
    /// Whether the takedown register delists this path of the shelf (an upheld `storage_object` item).
    async fn register_delisted(&self, shelf_id: i64, path: &str) -> Result<bool, FactsError>;
    /// Set the owner's title, cover and note of one object; `None` leaves a field. Whether the object exists.
    async fn set_owner_meta(
        &self,
        shelf_id: i64,
        object_hash: &str,
        title: Option<&str>,
        cover: Option<&str>,
        note: Option<&str>,
    ) -> Result<bool, FactsError>;
    /// Every grant of the shelf, newest first (live and revoked).
    async fn grants_of(&self, shelf_id: i64) -> Result<Vec<GrantRow>, FactsError>;
    /// Create a grant and append it to the hash-chained audit log, in one transaction.
    async fn create_grant(&self, g: &GrantRow) -> Result<GrantRow, GrantError>;
    /// Revoke a live grant of the shelf and append it to the audit log; whether there was one.
    async fn revoke_grant(&self, shelf_id: i64, grant_id: i64, by: i64)
    -> Result<bool, FactsError>;
    /// Whether the account exists and is active.
    async fn account_active(&self, id: i64) -> Result<bool, FactsError>;
}

/// Hex sha256 of a path: the `object_hash` of an object in the catalog.
pub fn path_hash(path: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut out = String::with_capacity(64);
    for b in Sha256::digest(path.as_bytes()) {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Whether `s` has the shape of an `object_hash`.
pub fn is_object_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_has_the_catalog_shape_and_is_stable() {
        let h = path_hash("Films/a.mkv");
        assert!(is_object_hash(&h));
        assert_eq!(h, path_hash("Films/a.mkv"));
        assert_ne!(h, path_hash("Films/b.mkv"));
        assert!(!is_object_hash("ABC"));
        assert!(!is_object_hash(&h.to_uppercase()));
    }

    #[test]
    fn the_url_lives_ten_minutes() {
        assert_eq!(URL_TTL_SECS, 600);
    }
}
