//! Wire and domain types of the moderation backend. Ids are snowflakes and travel as decimal strings in
//! JSON; times are Unix milliseconds.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sr_admin::model::{id, opt_id};
use utoipa::ToSchema;

macro_rules! text_enum {
    ($(#[$m:meta])* $name:ident { $($var:ident => $s:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($var),+ }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$var),+];

            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$var => $s),+ }
            }

            pub fn parse(s: &str) -> Option<Self> {
                match s { $($s => Some(Self::$var),)+ _ => None }
            }
        }
    };
}

text_enum! {
    /// Why something is reported or why a case exists: the ban reason vocabulary of spec section 8
    /// (`ban_reason_category`), which already includes "something else" as `other`.
    ReportCategory {
        Spam => "spam",
        Harassment => "harassment",
        IllegalContent => "illegal_content",
        ChildSafety => "child_safety",
        BanEvasion => "ban_evasion",
        Fraud => "fraud",
        PlatformAbuse => "platform_abuse",
        NsfwPolicy => "nsfw_policy",
        Other => "other",
    }
}

text_enum! {
    /// What a report or a case is about.
    TargetKind {
        Message => "message",
        User => "user",
        Channel => "channel",
        Guild => "guild",
        Library => "library",
        Cinema => "cinema",
        Person => "person",
        Other => "other",
    }
}

text_enum! {
    /// The lifecycle of a case (spec 4.2).
    CaseState {
        Open => "open",
        Triage => "triage",
        Investigating => "investigating",
        Actioned => "actioned",
        Dismissed => "dismissed",
        Closed => "closed",
    }
}

text_enum! {
    /// Who works a case (D-209).
    Routing {
        Guild => "guild",
        Platform => "platform",
        Reviewers => "reviewers",
    }
}

text_enum! {
    /// `standard` or `serious` (what drives the preset of a ban, D-211).
    CaseSeverity {
        Standard => "standard",
        Serious => "serious",
    }
}

text_enum! {
    /// What the reporter can read about their report.
    ReportState {
        Received => "received",
        UnderReview => "under_review",
        Closed => "closed",
    }
}

text_enum! {
    /// How a staff member decided an appeal.
    AppealDecision {
        Upheld => "upheld",
        Overturned => "overturned",
    }
}

impl ReportCategory {
    /// The severity a case of this category starts with (D-211).
    pub const fn default_severity(self) -> CaseSeverity {
        match self {
            Self::IllegalContent | Self::ChildSafety | Self::Fraud | Self::PlatformAbuse => {
                CaseSeverity::Serious
            }
            _ => CaseSeverity::Standard,
        }
    }

    /// Always handled by platform staff, wherever the report was filed (D-209).
    pub const fn always_escalated(self) -> bool {
        matches!(self, Self::IllegalContent | Self::ChildSafety)
    }
}

/// What a member sends to report something. The client sends ids and the note; the server snapshots the evidence.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct NewReport {
    pub target_kind: TargetKind,
    /// Snowflake of the message, account, channel, server, series or title.
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub target_id: i64,
    /// A message report names the channel the message is in.
    #[serde(default, with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub channel_id: Option<i64>,
    /// A user report may name the server it was filed from, so that server's moderators get it.
    #[serde(default, with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub guild_id: Option<i64>,
    pub category: ReportCategory,
    /// Up to 2000 characters.
    #[serde(default)]
    pub note: String,
    /// "Moderators may see my name" (off by default).
    #[serde(default)]
    pub moderators_may_see_name: bool,
}

/// What the reporter gets back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ReportReceipt {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub state: ReportState,
}

/// A report as its own author reads it: a status and a short outcome line, never the other party.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MyReport {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub target_kind: TargetKind,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub target_id: i64,
    pub category: ReportCategory,
    pub state: ReportState,
    pub outcome_line: String,
    pub created_at_ms: i64,
}

/// One row of a case list.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CaseSummary {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub state: CaseState,
    pub category: ReportCategory,
    pub severity: CaseSeverity,
    pub routing: Routing,
    /// Handed over from the server's moderators to platform staff.
    pub escalated: bool,
    pub summary: String,
    pub target_kind: TargetKind,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub target_id: Option<i64>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub guild_id: Option<i64>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub assignee_id: Option<i64>,
    pub report_count: i64,
    /// Holds on this case that are still in force.
    pub active_holds: i64,
    pub opened_at_ms: i64,
    pub updated_at_ms: i64,
    pub closed_at_ms: Option<i64>,
}

/// The reporter of a report, shown only to who may see it (spec 3.3).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ReporterView {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub account_id: i64,
    pub username: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CaseReport {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub category: ReportCategory,
    pub note: String,
    pub state: ReportState,
    /// Whether the reporter allowed server moderators to see their name.
    pub moderators_may_see_name: bool,
    /// `None` when the viewer may not see who reported.
    pub reporter: Option<ReporterView>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CaseEvidence {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub report_id: Option<i64>,
    /// `message_snapshot` or `target_summary`.
    pub kind: String,
    /// The immutable snapshot taken at report time.
    #[schema(value_type = Object)]
    pub payload: Value,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CaseNote {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub author_id: i64,
    pub body: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CaseEvent {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    /// `None` for the system.
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub actor_id: Option<i64>,
    pub kind: String,
    #[schema(value_type = Object)]
    pub detail: Value,
    pub created_at_ms: i64,
}

/// A hold, in force or released.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct HoldView {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub case_id: Option<i64>,
    pub scope_kind: Option<String>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub scope_id: Option<i64>,
    pub reason: String,
    /// `None` when the system placed it (a report on a message).
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub created_by: Option<i64>,
    pub created_at_ms: i64,
    pub released_at_ms: Option<i64>,
    pub release_reason: Option<String>,
    /// Messages, attachments and submissions the hold freezes right now.
    pub messages: i64,
    pub attachments: i64,
    pub submissions: i64,
    /// Older than `moderation.hold_review_days`, so it is flagged for review (D-212).
    pub review_due: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AppealView {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub ban_id: i64,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub case_id: Option<i64>,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub appellant_account_id: i64,
    pub message: String,
    /// `open`, `upheld` or `overturned`.
    pub state: String,
    /// The staff member who made the ban, who may not review its appeal (D-215).
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub banned_by: Option<i64>,
    /// The Person the ban is on.
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub person_id: i64,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub decided_by: Option<i64>,
    pub decision_reason: String,
    pub created_at_ms: i64,
    pub decided_at_ms: Option<i64>,
}

/// Everything about one case, as far as the viewer may see it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CaseDetail {
    pub case: CaseSummary,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub subject_person_id: Option<i64>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub subject_account_id: Option<i64>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub linked_case_id: Option<i64>,
    pub outcome: String,
    pub reports: Vec<CaseReport>,
    pub evidence: Vec<CaseEvidence>,
    pub notes: Vec<CaseNote>,
    pub events: Vec<CaseEvent>,
    pub holds: Vec<HoldView>,
    pub appeal: Option<AppealView>,
    /// What this viewer may do: `read`, `work` or `manage`.
    pub access: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CasePage {
    pub items: Vec<CaseSummary>,
    /// Pass as `cursor` for the next page.
    pub next_cursor: Option<String>,
}

/// Filters of the case queue.
#[derive(Debug, Clone, Default)]
pub struct CaseFilter {
    pub state: Option<CaseState>,
    /// Only cases that are not closed, dismissed or actioned.
    pub open_only: bool,
    pub category: Option<ReportCategory>,
    pub severity: Option<CaseSeverity>,
    pub assignee: Option<i64>,
    pub guild_id: Option<i64>,
    /// Only cases about this account.
    pub subject_account: Option<i64>,
    /// The id the previous page ended at.
    pub cursor: Option<i64>,
    /// Oldest first (the work queue); otherwise newest first.
    pub oldest_first: bool,
    pub limit: i64,
}

/// A case opened by staff (no report).
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct NewCase {
    pub category: ReportCategory,
    pub target_kind: TargetKind,
    #[serde(default, with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub target_id: Option<i64>,
    /// The account the case is about, when there is one; its Person becomes the subject.
    #[serde(default, with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub subject_account_id: Option<i64>,
    #[serde(default)]
    pub summary: String,
}

/// Optional extras of a staff-opened case. `NewCase` stays as it was for the callers that build one.
#[derive(Debug, Clone, Copy, Default)]
pub struct CaseExtras {
    /// The server the case is about (informational; the case is still platform business).
    pub guild_id: Option<i64>,
    /// Defaults to the preset of the category (D-211).
    pub severity: Option<CaseSeverity>,
}

/// Who acts on a case: platform staff with their bits, or a server moderator for one server.
#[derive(Debug, Clone)]
pub enum Who<'a> {
    Staff(&'a sr_admin::Caller),
    GuildMod { user_id: i64, guild_id: i64 },
}

impl Who<'_> {
    pub fn user_id(&self) -> i64 {
        match self {
            Who::Staff(c) => c.user_id,
            Who::GuildMod { user_id, .. } => *user_id,
        }
    }
}

/// What the viewer may do with a case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Access {
    None,
    /// Look, nothing else (reviewers of other kinds).
    Read,
    /// Notes, claim, state changes, escalate.
    Work,
    /// Everything, including holds, bans and appeals.
    Manage,
}

impl Access {
    pub const fn as_str(self) -> &'static str {
        match self {
            Access::None => "none",
            Access::Read => "read",
            Access::Work => "work",
            Access::Manage => "manage",
        }
    }
}

/// The few platform settings the backend reads.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub reports_per_day: i64,
    pub hold_review_days: i64,
    pub case_reopen_days: i64,
    pub deleted_message_days: i64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            reports_per_day: 20,
            hold_review_days: 90,
            case_reopen_days: 30,
            deleted_message_days: 30,
        }
    }
}

/// Platform-setting keys behind [`Limits`].
pub mod keys {
    pub const REPORTS_PER_DAY: &str = "moderation.reports_per_day";
    pub const HOLD_REVIEW_DAYS: &str = "moderation.hold_review_days";
    pub const CASE_REOPEN_DAYS: &str = "moderation.case_reopen_days";
    pub const DELETED_MESSAGE_DAYS: &str = "retention.deleted_message_days";
}
