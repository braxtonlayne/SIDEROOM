//! Wire and domain types of the review pipeline. Ids are Snowflakes and travel as decimal
//! strings in JSON; timestamps are Unix milliseconds.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Serde adapter: `i64` id as a JSON string (accepts a number on input).
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

/// Serde adapter for `Option<i64>` ids.
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

macro_rules! str_enum {
    ($(#[$m:meta])* $name:ident { $($var:ident => $s:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
        pub enum $name {
            $(#[serde(rename = $s)] $var),+
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$var),+];

            pub fn as_str(self) -> &'static str {
                match self { $($name::$var => $s),+ }
            }

            pub fn parse(s: &str) -> Option<Self> {
                match s { $($s => Some($name::$var),)+ _ => None }
            }
        }
    };
}

str_enum! {
    /// Where a submission is in its life (spec section 2).
    SubmissionState {
        Draft => "draft",
        Submitted => "submitted",
        InReview => "in_review",
        ChangesRequested => "changes_requested",
        Approved => "approved",
        Rejected => "rejected",
        Published => "published",
        Withdrawn => "withdrawn",
    }
}

impl SubmissionState {
    /// No transition leaves a final state.
    pub fn is_final(self) -> bool {
        matches!(self, Self::Rejected | Self::Published | Self::Withdrawn)
    }

    /// Counts against the open-submissions limit of a member.
    pub fn is_open(self) -> bool {
        !self.is_final()
    }
}

str_enum! {
    /// Decision state of one item (a chapter, an episode).
    ItemState {
        Pending => "pending",
        Approved => "approved",
        Rejected => "rejected",
        Published => "published",
    }
}

/// A submission as stored.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Submission {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    /// Kind id, e.g. `library.series`.
    pub kind: String,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub submitter_id: Option<i64>,
    pub state: SubmissionState,
    pub title: String,
    pub summary: String,
    /// What the kind works on (the draft series for the Library kinds).
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub ref_id: Option<i64>,
    pub payload: serde_json::Value,
    pub flags: Vec<String>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub claimed_by: Option<i64>,
    /// When the reviewer's lease ends; a past value means the claim is free to take.
    pub claim_expires_at_ms: Option<i64>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub decided_by: Option<i64>,
    pub decision_reason: Option<String>,
    pub submitted_at_ms: Option<i64>,
    pub decided_at_ms: Option<i64>,
    pub published_at_ms: Option<i64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl Submission {
    /// The reviewer lease is held and not expired at `now_ms`.
    pub fn lease_live(&self, now_ms: i64) -> bool {
        self.claimed_by.is_some() && self.claim_expires_at_ms.is_some_and(|t| t > now_ms)
    }
}

/// One item of a submission with its own decision.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct SubmissionItem {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub submission_id: i64,
    /// The object the kind knows (a chapter id for the Library kinds).
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub ref_id: i64,
    pub label: String,
    pub position: i32,
    pub state: ItemState,
    pub note: Option<String>,
}

/// A row of the timeline.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct SubmissionEvent {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub submission_id: i64,
    /// `created`, `submitted`, `resubmitted`, `claimed`, `released`, `comment`,
    /// `changes_requested`, `approved`, `rejected`, `published`, `publish_failed`,
    /// `withdrawn`, `item_decision`, `edited`.
    pub kind: String,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub actor_id: Option<i64>,
    pub body: Option<String>,
    /// Reviewer-only note; never shown to the submitter.
    pub internal: bool,
    pub data: serde_json::Value,
    pub created_at_ms: i64,
}

/// A submission with its items and timeline.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct SubmissionDetail {
    pub submission: Submission,
    pub items: Vec<SubmissionItem>,
    pub events: Vec<SubmissionEvent>,
    /// Display label of the kind.
    pub kind_label: String,
}

/// A page of submissions.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct SubmissionPage {
    pub items: Vec<Submission>,
    pub total: i64,
    pub limit: u32,
    pub offset: u32,
}

/// What a member sends to start a submission.
#[derive(Debug, Clone, PartialEq, Deserialize, ToSchema)]
pub struct CreateSubmission {
    /// Kind id, e.g. `library.series`.
    pub kind: String,
    /// Kind specific input (the series fields for `library.series`, the target series for
    /// `library.items`).
    #[serde(default)]
    pub payload: serde_json::Value,
    /// Defaults to what the kind derives from the payload.
    pub title: Option<String>,
    #[serde(default)]
    pub summary: String,
}

/// Editable fields of an open submission.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, ToSchema)]
pub struct UpdateSubmission {
    pub title: Option<String>,
    pub summary: Option<String>,
}

/// A note on the timeline.
#[derive(Debug, Clone, PartialEq, Deserialize, ToSchema)]
pub struct CommentInput {
    pub body: String,
    /// Reviewer-only (ignored for the submitter).
    #[serde(default)]
    pub internal: bool,
}

/// A decision with its reason (request changes, reject, approve).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, ToSchema)]
pub struct DecisionInput {
    /// Required for request changes and reject; optional for approve.
    pub reason: Option<String>,
}

/// A decision on one item.
#[derive(Debug, Clone, PartialEq, Deserialize, ToSchema)]
pub struct ItemDecisionInput {
    /// `approved`, `rejected` or `pending` (undo).
    pub state: ItemState,
    pub note: Option<String>,
}

/// Listing filter and paging, shared by "my submissions" and the review queue.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SubmissionQuery {
    pub kinds: Vec<String>,
    pub states: Vec<SubmissionState>,
    pub submitter: Option<i64>,
    /// Only claimed (with a live lease) by this reviewer.
    pub claimed_by: Option<i64>,
    /// Only submissions nobody holds a live lease on.
    pub unclaimed: bool,
    /// Submitted at or before this time (age filter).
    pub submitted_before_ms: Option<i64>,
    pub title_contains: Option<String>,
    pub sort: SortKey,
    pub descending: bool,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SortKey {
    /// Oldest waiting first by default; newest first with `descending`.
    #[default]
    Submitted,
    Updated,
    Title,
}

impl SortKey {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "submitted" => Some(Self::Submitted),
            "updated" => Some(Self::Updated),
            "title" => Some(Self::Title),
            _ => None,
        }
    }
}

/// A kind available for creation.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct KindInfo {
    pub id: String,
    pub label: String,
}

// ---- requests --------------------------------------------------------

str_enum! {
    RequestStatus {
        Open => "open",
        Fulfilled => "fulfilled",
        Declined => "declined",
    }
}

/// A content request (Library or Cinema board).
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ContentRequest {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub section: String,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub requester_id: Option<i64>,
    pub title: String,
    pub creator: Option<String>,
    pub year: Option<i16>,
    pub link: Option<String>,
    pub note: String,
    pub status: RequestStatus,
    pub vote_count: i32,
    /// The caller already upvoted.
    pub voted: bool,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub submission_id: Option<i64>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, ToSchema)]
pub struct RequestInput {
    pub title: String,
    pub creator: Option<String>,
    pub year: Option<i16>,
    pub link: Option<String>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct RequestPage {
    pub items: Vec<ContentRequest>,
    pub total: i64,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RequestQuery {
    pub section: String,
    pub status: Option<RequestStatus>,
    /// Newest first instead of most votes first.
    pub newest: bool,
    pub limit: u32,
    pub offset: u32,
}

/// A contribution credit (D-074): one per new published item, credited to the submitter.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Credit {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    /// `library` or `cinema`.
    pub section: String,
    /// The published item (chapter, episode, movie).
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub ref_id: i64,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub user_id: Option<i64>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub submission_id: Option<i64>,
    pub label: String,
    pub granted_at_ms: i64,
    pub revoked_at_ms: Option<i64>,
    pub revoke_reason: Option<String>,
}

/// A credit to insert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCredit {
    pub id: i64,
    pub ref_id: i64,
    pub label: String,
}

/// What a member has left of today's upload allowance (`None` limits for staff).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct UploadAllowance {
    pub limit_bytes: Option<i64>,
    pub used_bytes: i64,
    pub remaining_bytes: Option<i64>,
}

/// What one housekeeping pass did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct SweepReport {
    pub warned: u32,
    pub abandoned: u32,
    pub purged: u32,
    pub failed: u32,
}
