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
    /// Staff approved the submitted entry document; the proxy serves it.
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

/// An application's Activity settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ConfigView {
    pub application_id: String,
    pub entry_url: Option<String>,
    pub url_mappings: Vec<UrlMapping>,
    pub review_state: ReviewState,
    /// SHA-256 (hex) of the entry document the owner submitted.
    pub pinned_sha256: Option<String>,
    /// SHA-256 (hex) of the entry document staff approved; the proxy serves only this one.
    pub reviewed_sha256: Option<String>,
    pub review_note: Option<String>,
    pub updated_at_ms: i64,
}

/// What the owner may change.
#[derive(Debug, Clone, PartialEq, Deserialize, ToSchema)]
pub struct ConfigInput {
    /// `null` or empty clears it.
    pub entry_url: Option<String>,
    #[serde(default)]
    pub url_mappings: Vec<UrlMapping>,
}

/// A submitted Activity in the staff queue.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct PendingView {
    pub application_name: String,
    pub owner_id: String,
    pub config: ConfigView,
}

/// An approved Activity in the launcher's shelf.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ShelfItem {
    pub application_id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ParticipantView {
    pub user_id: String,
    pub joined_at_ms: i64,
}

/// A running activity in a voice channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct InstanceView {
    pub id: String,
    pub application_id: String,
    pub application_name: String,
    pub guild_id: String,
    pub channel_id: String,
    pub started_by: String,
    pub created_at_ms: i64,
    pub participants: Vec<ParticipantView>,
}
