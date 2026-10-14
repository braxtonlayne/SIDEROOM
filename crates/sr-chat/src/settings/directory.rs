//! The Server Directory (LNG-GLD-013, D-012): the platform-internal list of Community servers
//! that staff approved. A server asks to be listed with a category and a short blurb; staff with
//! `MANAGE_DIRECTORY` approve, reject or remove the listing. Only approved listings of servers
//! that still have Community on show up for logged-in members.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sr_core::{GuildId, UserId};

use super::error::{SettingsError, SettingsResult};
use crate::repo::RepoResult;

/// Directory categories, as slugs the web client translates.
pub const CATEGORIES: &[&str] = &[
    "gaming",
    "music",
    "entertainment",
    "science-tech",
    "education",
    "art",
    "anime",
    "books",
    "movies-tv",
    "sports",
    "community",
    "other",
];
/// Longest listing blurb.
pub const MAX_DESCRIPTION: usize = 300;
/// Longest staff note on a decision.
pub const MAX_NOTE: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryStatus {
    /// Waiting for staff.
    Pending,
    /// Listed.
    Approved,
    /// Staff said no.
    Rejected,
    /// Staff took an approved listing down.
    Removed,
}

impl DirectoryStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Removed => "removed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "pending" => Self::Pending,
            "approved" => Self::Approved,
            "rejected" => Self::Rejected,
            "removed" => Self::Removed,
            _ => return None,
        })
    }
}

/// One server's listing request and its state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryEntry {
    pub guild_id: GuildId,
    pub status: DirectoryStatus,
    pub category: String,
    /// Blurb shown on the card; falls back to the server description when empty.
    pub description: String,
    /// Staff pinned it to the top.
    pub featured: bool,
    pub requested_by: UserId,
    pub requested_at_ms: i64,
    pub decided_by: Option<UserId>,
    pub decided_at_ms: Option<i64>,
    /// Staff's note to the server's managers (why it was rejected, say).
    pub decision_note: Option<String>,
}

/// What staff decide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Approve,
    Reject,
    /// Take an approved listing down.
    Remove,
}

#[async_trait]
pub trait DirectoryRepository: Send + Sync {
    async fn get_listing(&self, guild: GuildId) -> RepoResult<Option<DirectoryEntry>>;
    /// Insert or replace the listing of `entry.guild_id`.
    async fn put_listing(&self, entry: &DirectoryEntry) -> RepoResult<()>;
    /// Withdraw a listing; `false` when there was none.
    async fn delete_listing(&self, guild: GuildId) -> RepoResult<bool>;
    /// Listings in `status` (any when `None`) of `category` (any when `None`), ordered by
    /// request time then guild id.
    async fn list_listings(
        &self,
        status: Option<DirectoryStatus>,
        category: Option<&str>,
    ) -> RepoResult<Vec<DirectoryEntry>>;
}

/// Check a category and blurb; returns them trimmed.
pub fn validate_listing(category: &str, description: &str) -> SettingsResult<(String, String)> {
    let category = category.trim().to_ascii_lowercase();
    if !CATEGORIES.contains(&category.as_str()) {
        return Err(SettingsError::invalid(
            "category",
            "ENUM_TYPE_BASE_TYPE_BAD_VALUE",
            "not one of the directory categories",
        ));
    }
    let description = description.trim().to_owned();
    if description.chars().count() > MAX_DESCRIPTION {
        return Err(SettingsError::invalid(
            "description",
            "BASE_TYPE_MAX_LENGTH",
            "must be 300 or fewer in length",
        ));
    }
    Ok((category, description))
}

/// A server asks to be listed, or edits its request (LNG-GLD-013). Needs Community on.
/// A first request, or one after a rejection or removal, waits for staff. Editing the
/// category or blurb of a listing that was approved sends it back for review, so an approved
/// card never shows text staff did not see; saving it unchanged leaves it alone.
pub fn request_listing(
    existing: Option<DirectoryEntry>,
    guild: GuildId,
    requester: UserId,
    category: &str,
    description: &str,
    community_enabled: bool,
    now_ms: i64,
) -> SettingsResult<DirectoryEntry> {
    if !community_enabled {
        return Err(SettingsError::invalid(
            "community_enabled",
            "COMMUNITY_REQUIRED",
            "enable Community before asking to be listed in the directory",
        ));
    }
    let (category, description) = validate_listing(category, description)?;
    let fresh = |featured| DirectoryEntry {
        guild_id: guild,
        status: DirectoryStatus::Pending,
        category: category.clone(),
        description: description.clone(),
        featured,
        requested_by: requester,
        requested_at_ms: now_ms,
        decided_by: None,
        decided_at_ms: None,
        decision_note: None,
    };
    Ok(match existing {
        None => fresh(false),
        Some(e)
            if matches!(
                e.status,
                DirectoryStatus::Rejected | DirectoryStatus::Removed
            ) =>
        {
            fresh(false)
        }
        Some(e) if e.category == category && e.description == description => e,
        Some(mut e) => {
            e.category = category;
            e.description = description;
            e.requested_by = requester;
            // A pending request keeps its place in the queue; an approved one is reviewed again.
            if e.status == DirectoryStatus::Approved {
                e.status = DirectoryStatus::Pending;
                e.requested_at_ms = now_ms;
                e.featured = false;
                e.decided_by = None;
                e.decided_at_ms = None;
                e.decision_note = None;
            }
            e
        }
    })
}

/// Apply a staff decision. Approve and reject act on a pending (or earlier rejected) request;
/// remove acts on an approved listing. `featured` may only be set on an approved listing.
pub fn decide(
    mut entry: DirectoryEntry,
    decision: Decision,
    featured: Option<bool>,
    note: Option<&str>,
    staff: UserId,
    now_ms: i64,
) -> SettingsResult<DirectoryEntry> {
    let note = note.map(str::trim).filter(|n| !n.is_empty());
    if note.is_some_and(|n| n.chars().count() > MAX_NOTE) {
        return Err(SettingsError::invalid(
            "note",
            "BASE_TYPE_MAX_LENGTH",
            "must be 500 or fewer in length",
        ));
    }
    let conflict = |msg: &'static str| SettingsError::invalid("decision", "DIRECTORY_STATE", msg);
    entry.status = match (decision, entry.status) {
        (Decision::Approve, DirectoryStatus::Pending | DirectoryStatus::Rejected) => {
            DirectoryStatus::Approved
        }
        (Decision::Approve, DirectoryStatus::Approved) => DirectoryStatus::Approved,
        (Decision::Reject, DirectoryStatus::Pending) => DirectoryStatus::Rejected,
        (Decision::Remove, DirectoryStatus::Approved) => DirectoryStatus::Removed,
        (Decision::Approve, _) => {
            return Err(conflict("this listing was removed; it must ask again"));
        }
        (Decision::Reject, _) => return Err(conflict("only a pending request can be rejected")),
        (Decision::Remove, _) => return Err(conflict("only an approved listing can be removed")),
    };
    if let Some(f) = featured {
        if f && entry.status != DirectoryStatus::Approved {
            return Err(conflict("only an approved listing can be featured"));
        }
        entry.featured = f;
    }
    if entry.status != DirectoryStatus::Approved {
        entry.featured = false;
    }
    entry.decided_by = Some(staff);
    entry.decided_at_ms = Some(now_ms);
    entry.decision_note = note.map(str::to_owned);
    Ok(entry)
}

/// The listing shows to members only while it is approved and the server still is a Community.
pub fn is_visible(entry: &DirectoryEntry, community_enabled: bool) -> bool {
    entry.status == DirectoryStatus::Approved && community_enabled
}

/// In-memory [`DirectoryRepository`] for tests.
#[derive(Default)]
pub struct MemoryDirectory {
    entries: std::sync::Mutex<Vec<DirectoryEntry>>,
}

impl MemoryDirectory {
    fn with<R>(&self, f: impl FnOnce(&mut Vec<DirectoryEntry>) -> R) -> R {
        f(&mut self.entries.lock().unwrap_or_else(|p| p.into_inner()))
    }
}

#[async_trait]
impl DirectoryRepository for MemoryDirectory {
    async fn get_listing(&self, guild: GuildId) -> RepoResult<Option<DirectoryEntry>> {
        Ok(self.with(|v| v.iter().find(|e| e.guild_id == guild).cloned()))
    }

    async fn put_listing(&self, entry: &DirectoryEntry) -> RepoResult<()> {
        self.with(|v| {
            v.retain(|e| e.guild_id != entry.guild_id);
            v.push(entry.clone());
        });
        Ok(())
    }

    async fn delete_listing(&self, guild: GuildId) -> RepoResult<bool> {
        Ok(self.with(|v| {
            let before = v.len();
            v.retain(|e| e.guild_id != guild);
            v.len() != before
        }))
    }

    async fn list_listings(
        &self,
        status: Option<DirectoryStatus>,
        category: Option<&str>,
    ) -> RepoResult<Vec<DirectoryEntry>> {
        Ok(self.with(|v| {
            let mut out: Vec<_> = v
                .iter()
                .filter(|e| status.is_none_or(|s| e.status == s))
                .filter(|e| category.is_none_or(|c| e.category == c))
                .cloned()
                .collect();
            out.sort_by_key(|e| (e.requested_at_ms, e.guild_id));
            out
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: GuildId = GuildId::new(1);
    const OWNER: UserId = UserId::new(2);
    const STAFF: UserId = UserId::new(3);

    fn ask(existing: Option<DirectoryEntry>, cat: &str, desc: &str, now: i64) -> DirectoryEntry {
        request_listing(existing, G, OWNER, cat, desc, true, now).unwrap()
    }

    #[test]
    fn needs_community_and_a_known_category() {
        assert!(request_listing(None, G, OWNER, "gaming", "", false, 1).is_err());
        assert!(request_listing(None, G, OWNER, "nope", "", true, 1).is_err());
        assert!(request_listing(None, G, OWNER, "gaming", &"x".repeat(301), true, 1).is_err());
        let e = ask(None, " Gaming ", "  Fun  ", 10);
        assert_eq!(
            (e.status, e.category.as_str(), e.description.as_str()),
            (DirectoryStatus::Pending, "gaming", "Fun")
        );
        assert_eq!(e.requested_at_ms, 10);
    }

    #[test]
    fn approval_flow_and_re_review() {
        let pending = ask(None, "music", "tunes", 10);
        let approved = decide(
            pending.clone(),
            Decision::Approve,
            Some(true),
            Some(" welcome "),
            STAFF,
            20,
        )
        .unwrap();
        assert_eq!(approved.status, DirectoryStatus::Approved);
        assert!(approved.featured);
        assert_eq!(approved.decision_note.as_deref(), Some("welcome"));
        assert_eq!(
            (approved.decided_by, approved.decided_at_ms),
            (Some(STAFF), Some(20))
        );
        assert!(is_visible(&approved, true));
        assert!(
            !is_visible(&approved, false),
            "community switched off hides it"
        );

        // Saving the same text changes nothing; new text sends it back for review.
        let same = ask(Some(approved.clone()), "music", "tunes", 30);
        assert_eq!(same, approved);
        let edited = ask(Some(approved), "music", "more tunes", 40);
        assert_eq!(edited.status, DirectoryStatus::Pending);
        assert!(!edited.featured);
        assert_eq!(edited.requested_at_ms, 40);
        assert!(edited.decided_by.is_none());
    }

    #[test]
    fn rejection_removal_and_asking_again() {
        let pending = ask(None, "art", "", 10);
        assert!(decide(pending.clone(), Decision::Remove, None, None, STAFF, 11).is_err());
        let rejected =
            decide(pending, Decision::Reject, None, Some("too thin"), STAFF, 12).unwrap();
        assert_eq!(rejected.status, DirectoryStatus::Rejected);
        assert!(decide(rejected.clone(), Decision::Reject, None, None, STAFF, 13).is_err());
        // A rejection can be reconsidered by staff.
        let approved = decide(rejected.clone(), Decision::Approve, None, None, STAFF, 14).unwrap();
        let removed = decide(
            approved.clone(),
            Decision::Remove,
            None,
            Some("spam"),
            STAFF,
            15,
        )
        .unwrap();
        assert_eq!(removed.status, DirectoryStatus::Removed);
        assert!(!removed.featured);
        assert!(decide(removed.clone(), Decision::Approve, None, None, STAFF, 16).is_err());
        // The server may ask again after a rejection or removal.
        assert_eq!(
            ask(Some(removed), "art", "", 50).status,
            DirectoryStatus::Pending
        );
        assert_eq!(
            ask(Some(rejected), "art", "", 51).status,
            DirectoryStatus::Pending
        );
        // Only approved listings feature; long notes are refused.
        assert!(
            decide(
                approved,
                Decision::Approve,
                Some(true),
                Some(&"n".repeat(501)),
                STAFF,
                17
            )
            .is_err()
        );
        let p = ask(None, "art", "", 1);
        assert!(decide(p, Decision::Reject, Some(true), None, STAFF, 2).is_err());
    }

    #[tokio::test]
    async fn memory_repo_filters_and_orders() {
        let repo = MemoryDirectory::default();
        for (g, cat, at) in [(5u64, "music", 30), (4, "art", 10), (6, "music", 20)] {
            let mut e = ask(None, cat, "", at);
            e.guild_id = GuildId::new(g);
            repo.put_listing(&e).await.unwrap();
        }
        let all = repo.list_listings(None, None).await.unwrap();
        assert_eq!(
            all.iter().map(|e| e.guild_id.get()).collect::<Vec<_>>(),
            vec![4, 6, 5]
        );
        let music = repo
            .list_listings(Some(DirectoryStatus::Pending), Some("music"))
            .await
            .unwrap();
        assert_eq!(music.len(), 2);
        assert!(
            repo.list_listings(Some(DirectoryStatus::Approved), None)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(repo.delete_listing(GuildId::new(4)).await.unwrap());
        assert!(!repo.delete_listing(GuildId::new(4)).await.unwrap());
        assert_eq!(
            DirectoryStatus::parse("approved"),
            Some(DirectoryStatus::Approved)
        );
        assert_eq!(DirectoryStatus::parse("x"), None);
    }
}
