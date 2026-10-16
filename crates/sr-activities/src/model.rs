//! Wire types of Activities. Snowflake ids are strings on the wire.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One URL mapping: requests to `<activity origin><prefix>/...` are fetched from `target`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UrlMapping {
    /// `/api`: a path of one or more segments, never `/`.
    pub prefix: String,
    /// `https://api.example.com/v1`: https, no query.
    pub target: String,
}

/// Where an application's Activity is in review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewState {
    /// Being edited; not submitted.
    Draft,
    /// Submitted: staff has not decided.
    Pending,
    /// Staff approved a bundle; the proxy serves it.
    Approved,
    /// Staff turned it down; the owner edits and submits again.
    Rejected,
}

impl ReviewState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "approved" => Self::Approved,
            "rejected" => Self::Rejected,
            _ => Self::Draft,
        }
    }
}

/// Where a bundle is in review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BundleState {
    /// Uploaded; not submitted.
    Draft,
    /// Submitted: staff has not decided.
    Pending,
    /// Staff approved this bundle; it is the one the proxy serves (once the application is released).
    Approved,
    /// Staff turned it down; the owner uploads a new bundle.
    Rejected,
    /// Replaced by a newer bundle; its files are deleted by `activities.gc`.
    Superseded,
}

impl BundleState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Superseded => "superseded",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "approved" => Self::Approved,
            "rejected" => Self::Rejected,
            "superseded" => Self::Superseded,
            _ => Self::Draft,
        }
    }
}

/// One uploaded bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BundleView {
    pub id: String,
    /// SHA-256 (hex) of the uploaded zip: what staff approve.
    pub sha256: String,
    /// Total unpacked size in bytes.
    pub byte_size: i64,
    pub entry_count: i32,
    pub state: BundleState,
    pub uploaded_at_ms: i64,
    pub reviewed_at_ms: Option<i64>,
    pub note: Option<String>,
}

/// One file of a bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BundleEntry {
    pub path: String,
    pub size: i64,
    pub content_type: String,
}

/// A bundle with the files it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BundleDetail {
    pub bundle: BundleView,
    pub entries: Vec<BundleEntry>,
}

/// An application's Activity settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ConfigView {
    pub application_id: String,
    /// Live URL served only while the application is in development (never reviewed).
    pub dev_entry_url: Option<String>,
    pub url_mappings: Vec<UrlMapping>,
    /// Summary of the review: `pending` while a bundle waits, `approved` while a bundle is reviewed.
    pub review_state: ReviewState,
    /// The bundle staff approved; the proxy serves only this one.
    pub reviewed_bundle: Option<BundleView>,
    /// Recent bundles (the review history), newest first, superseded ones included.
    pub bundles: Vec<BundleView>,
    pub review_note: Option<String>,
    pub updated_at_ms: i64,
}

/// What the owner may change.
#[derive(Debug, Clone, PartialEq, Deserialize, ToSchema)]
pub struct ConfigInput {
    /// `null` or empty clears it. Only an application in development may set one.
    pub dev_entry_url: Option<String>,
    #[serde(default)]
    pub url_mappings: Vec<UrlMapping>,
}

/// A submitted Activity in the staff queue.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct PendingView {
    pub application_name: String,
    pub owner_id: String,
    pub config: ConfigView,
    /// The bundle waiting for a decision.
    pub bundle: BundleDetail,
}

/// An Activity in the launcher's shelf.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ShelfItem {
    pub application_id: String,
    pub name: String,
    pub description: String,
    /// A development build only the owner and testers see.
    pub development: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ParticipantView {
    pub user_id: String,
    pub joined_at_ms: i64,
}

/// A running activity in a voice or text channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct InstanceView {
    pub id: String,
    pub application_id: String,
    pub application_name: String,
    pub guild_id: String,
    pub channel_id: String,
    /// `voice` or `text`.
    pub channel_type: String,
    /// Running a development build (live entry URL).
    pub development: bool,
    pub started_by: String,
    pub created_at_ms: i64,
    pub participants: Vec<ParticipantView>,
}
