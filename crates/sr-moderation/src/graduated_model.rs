//! Wire types of the graduated toolset (MOD-GRD-001). Ids travel as decimal strings, times as Unix milliseconds.

use serde::{Deserialize, Serialize};
use sr_admin::model::{id, opt_id};
use utoipa::ToSchema;

/// One per-member restriction or warning (what staff see; the member sees [`MyRestriction`]).
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct RestrictionView {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub account_id: i64,
    /// `warn`, `timeout`, `invite_freeze`, `case_freeze` or `sterilization`.
    pub kind: String,
    pub reason: String,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub case_id: Option<i64>,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub imposed_by: i64,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub approved_by: Option<i64>,
    /// 1.0 times the account's accountability multiplier when the act was recorded.
    pub weight: f32,
    pub starts_at_ms: i64,
    pub ends_at_ms: i64,
    /// The freeze ends in the account's removal unless cleared.
    pub removes_account: bool,
    /// `active`, `lifted`, `overturned`, `expired` or `removed`.
    pub state: String,
    pub appeal_message: Option<String>,
    /// `open`, `upheld` or `overturned`.
    pub appeal_state: Option<String>,
}

/// What a member sees of a restriction on their own account: no case, no staff member, no tree.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct MyRestriction {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    /// `warn`, `timeout`, `invite_freeze`, `case_freeze` (shown as a freeze) or `sterilization` (shown as
    /// "you cannot invite right now").
    pub kind: String,
    /// What the member is told it covers, in words.
    pub effect: String,
    /// The reason staff wrote for the warning or timeout; case freezes and sterilizations say only that the
    /// account is under review.
    pub reason: String,
    pub ends_at_ms: i64,
    pub state: String,
    pub can_appeal: bool,
    pub appeal_state: Option<String>,
}

/// Result of an act that names no row (revoking unused codes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct RevokedInvites {
    pub codes_revoked: i64,
}

/// A subtree case in the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct SubtreeCaseSummary {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub root_account_id: i64,
    pub root_username: String,
    pub category: String,
    /// `open` or `closed`.
    pub state: String,
    pub opened_at_ms: i64,
    pub nodes: i64,
    pub auto_frozen: i64,
    pub waiting_review: i64,
}

/// One node of a case with its activity stats and lineage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct SubtreeNode {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub account_id: i64,
    pub username: String,
    /// `root`, `descendant`, `inviter` or `inviter2`.
    pub role: String,
    /// 0 root, 1.. below it, -1 and -2 the inviters.
    pub depth: i32,
    /// The account that issued the invite this one redeemed.
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub parent_account_id: Option<i64>,
    pub registered_at_ms: i64,
    pub messages: i64,
    pub voice_seconds: i64,
    /// D-392 when the case opened.
    pub unactivated: bool,
    /// `auto_frozen`, `review`, `frozen`, `banned`, `sterilized`, `cleared`, `removed` or `not_active`.
    pub status: String,
    /// Frozen or sterilized right now.
    pub restricted: bool,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct SubtreeCaseDetail {
    pub case: SubtreeCaseSummary,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub opened_by: i64,
    pub nodes_truncated: bool,
    pub nodes: Vec<SubtreeNode>,
    /// Requests waiting for a second staff member, or approved and waiting for the Operator.
    pub requests: Vec<ModRequest>,
}

/// A sterilization or branch-action request (the two-person step).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ModRequest {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    /// `sterilize` or `branch_action`.
    pub kind: String,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub case_id: i64,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub target_account_id: Option<i64>,
    /// `freeze` or `ban` (branch action only).
    pub action: Option<String>,
    pub reason: String,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub proposed_by: i64,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub approved_by: Option<i64>,
    /// `pending`, `approved`, `executed`, `rejected` or `cancelled`.
    pub state: String,
    pub created_at_ms: i64,
    pub nodes_actioned: Option<i32>,
}

/// What a branch action did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct BranchOutcome {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub request_id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub case_id: i64,
    /// `freeze` or `ban`.
    pub action: String,
    pub nodes_actioned: i64,
    pub already_banned_or_gone: i64,
    /// The anonymised notice it emitted (counts and category only).
    pub notice: NoticeView,
}

/// The anonymised notice: a category and counts, by construction nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct NoticeView {
    /// `serious_ban` or `branch_action`.
    pub kind: String,
    pub category: String,
    pub title: String,
    pub body: String,
    pub accounts_actioned: i64,
    pub accounts_frozen: i64,
    pub created_at_ms: i64,
}

/// One month of the transparency page: counts only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct TransparencyMonth {
    /// `YYYY-MM`.
    pub month: String,
    pub serious_bans: i64,
    pub branch_actions: i64,
    pub accounts_actioned: i64,
    pub accounts_frozen: i64,
    pub sterilizations: i64,
    pub appeals_filed: i64,
    pub appeals_overturned: i64,
    /// By offense category: `(category label, notices)`.
    pub by_category: Vec<CategoryCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct CategoryCount {
    pub category: String,
    pub notices: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct Transparency {
    /// Newest month first, at most 24.
    pub months: Vec<TransparencyMonth>,
}

/// What `sr-cli moderation drill --synthetic` reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct DrillReport {
    /// Accounts in the synthetic tree (all rolled back with it).
    pub tree_size: i64,
    pub auto_frozen: i64,
    pub waiting_review: i64,
    pub notice: NoticeView,
    /// Rows of the real tables the drill left behind (always 0: it ran in a transaction that was rolled back).
    pub rows_left_behind: i64,
}
