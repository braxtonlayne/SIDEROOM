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
        StatusRing => "status_ring",
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
    /// The channel a message case is about (the channel to mark age-restricted for misplaced adult content).
    #[serde(default, with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub target_channel_id: Option<i64>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub assignee_id: Option<i64>,
    pub report_count: i64,
    /// Holds on this case that are still in force.
    pub active_holds: i64,
    pub opened_at_ms: i64,
    pub updated_at_ms: i64,
    pub closed_at_ms: Option<i64>,
    /// The takedown notice this case was opened for (D-334), when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub takedown: Option<CaseTakedown>,
}

/// The notice behind a takedown case: its reference and kind, for the badge on the case list and page.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CaseTakedown {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub notice_id: i64,
    pub kind: TakedownKind,
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
    /// Library and cinema rows (series, chapters, titles, episodes) it freezes.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub catalog_rows: i64,
    /// Older than `moderation.hold_review_days`, so it is flagged for review (D-212).
    pub review_due: bool,
    /// A deletion request met this hold (MOD-DSR-005): the hold is flagged for a staff decision, release or retain
    /// with a stated legal basis. Cleared by either.
    #[serde(default)]
    pub erasure_flagged: bool,
    #[serde(default, with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub erasure_request_id: Option<i64>,
    /// Staff decided to retain the held rows after the request, with this basis.
    #[serde(default)]
    pub erasure_retained_basis: Option<String>,
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
    /// Only cases opened for a takedown notice (a source, not a category).
    pub takedown_only: bool,
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
    /// Most Persons that may share a sign-in address before it counts as a shared network (D-346).
    pub shared_ip_max_persons: i64,
    /// A new account counts as "shortly after a ban" within this many days (D-346).
    pub new_account_after_ban_days: i64,
    /// Counter-notices one staff member may record per day (RDY-SEC-006).
    pub counter_notices_per_day: i64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            reports_per_day: 20,
            hold_review_days: 90,
            case_reopen_days: 30,
            deleted_message_days: 30,
            shared_ip_max_persons: 4,
            new_account_after_ban_days: 14,
            counter_notices_per_day: 50,
        }
    }
}

/// Platform-setting keys behind [`Limits`].
pub mod keys {
    pub const REPORTS_PER_DAY: &str = "moderation.reports_per_day";
    pub const HOLD_REVIEW_DAYS: &str = "moderation.hold_review_days";
    pub const CASE_REOPEN_DAYS: &str = "moderation.case_reopen_days";
    pub const DELETED_MESSAGE_DAYS: &str = "retention.deleted_message_days";
    pub const SHARED_IP_MAX_PERSONS: &str = "moderation.shared_ip_max_persons";
    pub const NEW_ACCOUNT_AFTER_BAN_DAYS: &str = "moderation.new_account_after_ban_days";
    pub const COUNTER_NOTICES_PER_DAY: &str = "abuse.counter_notices_per_day";
}

fn is_zero(n: &i64) -> bool {
    *n == 0
}

// ---- the takedown register (step 2, spec 5) ----------------------------------------------------------------------

text_enum! {
    /// The two kinds of notice (spec 5).
    TakedownKind {
        Copyright => "copyright",
        IllegalContent => "illegal_content",
    }
}

text_enum! {
    /// Where a notice is in its life.
    NoticeState {
        Received => "received",
        Triage => "triage",
        Rejected => "rejected",
        Removed => "removed",
        CounterNoticed => "counter_noticed",
        Restored => "restored",
        Withdrawn => "withdrawn",
    }
}

text_enum! {
    /// What a notice item points at.
    ItemKind {
        Message => "message",
        Attachment => "attachment",
        LibrarySeries => "library_series",
        LibraryChapter => "library_chapter",
        CinemaTitle => "cinema_title",
        CinemaEpisode => "cinema_episode",
        // An object in a member's own bucket (BYOS-LEG-001): delisted, never deleted. `target_id` is the mount.
        StorageObject => "storage_object",
    }
}

text_enum! {
    /// Where one piece of content is: identified, held for evidence, or unavailable.
    ItemState {
        Identified => "identified",
        Held => "held",
        Disabled => "disabled",
        Restored => "restored",
    }
}

/// What staff enter for a notice (spec 5, step 1).
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct NewNotice {
    pub kind: TakedownKind,
    pub notifier_name: String,
    /// Email, postal address or phone, as the notifier gave it.
    pub notifier_contact: String,
    /// The work claimed (copyright) or the legal basis (illegal content).
    pub legal_basis: String,
    /// The exact URLs or ids the notifier named.
    #[serde(default)]
    pub locations: Vec<String>,
    /// The notice as received, kept verbatim.
    pub original_text: String,
    /// When it arrived (Unix ms); now when absent.
    #[serde(default)]
    pub received_at_ms: Option<i64>,
}

/// A piece of content found for a notice.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct NewItem {
    pub kind: ItemKind,
    /// The content id; for `storage_object` the mount id.
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    /// For `storage_object` only: the mount-relative object key. Required there, refused for any other kind.
    #[serde(default)]
    pub key: Option<String>,
}

/// One content item of a notice.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TakedownItem {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub kind: ItemKind,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub target_id: i64,
    pub label: String,
    /// For `storage_object`: the mount-relative key that was delisted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_key: Option<String>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub subject_account_id: Option<i64>,
    pub state: ItemState,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub hold_id: Option<i64>,
    pub disabled_at_ms: Option<i64>,
    pub restored_at_ms: Option<i64>,
}

/// A row of the register.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct NoticeSummary {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub kind: TakedownKind,
    pub state: NoticeState,
    pub notifier_name: String,
    pub received_at_ms: i64,
    pub triage_due_at_ms: i64,
    /// Received or in triage and past the one-business-day target.
    pub triage_overdue: bool,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub case_id: Option<i64>,
    pub items: i64,
    pub removed_at_ms: Option<i64>,
    pub restore_after_ms: Option<i64>,
    pub court_filing_noted: bool,
}

/// One entry of a notice's timeline.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct NoticeEvent {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub actor_id: Option<i64>,
    pub kind: String,
    #[schema(value_type = Object)]
    pub detail: Value,
    pub created_at_ms: i64,
}

/// One notice that counts as a strike.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CountedNotice {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub kind: TakedownKind,
    pub state: NoticeState,
    pub removed_at_ms: i64,
    /// A counter-notice is pending.
    pub contested: bool,
}

/// The strikes of one Person named by a notice.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct StrikeCount {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub person_id: i64,
    /// Upheld takedowns inside the window, this one included once it is upheld.
    pub strikes: i64,
    /// Of those, the notices whose content is down while a counter-notice is pending.
    pub contested: i64,
    /// The notices counted, newest first.
    pub counted: Vec<CountedNotice>,
    pub window_months: i64,
    pub threshold: i64,
    /// The review case the counter opened for this Person, when it did.
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub review_case_id: Option<i64>,
}

/// A notice with everything staff need on its page.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct NoticeDetail {
    #[serde(flatten)]
    pub summary: NoticeSummary,
    pub notifier_contact: String,
    pub legal_basis: String,
    pub locations: Vec<String>,
    pub original_text: String,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub entered_by: i64,
    pub rejection_reason: String,
    pub counter_notice_text: String,
    pub counter_noticed_at_ms: Option<i64>,
    pub restored_at_ms: Option<i64>,
    pub restore_reason: String,
    pub items: Vec<TakedownItem>,
    pub events: Vec<NoticeEvent>,
    pub strikes: Vec<StrikeCount>,
}

/// What upholding a notice did.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UpholdOutcome {
    pub items_disabled: i64,
    /// Uploaders or authors who got the system DM.
    pub notified: i64,
    /// The review case opened because a Person reached the threshold.
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub review_case_id: Option<i64>,
}

/// The reply text for an incomplete notice.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RejectOutcome {
    /// The templated reply staff send to the notifier.
    pub reply: String,
}

/// What the item pages may show about a piece of content (spec 5; `MOD-TKD-011`).
#[derive(Debug, Clone, Serialize)]
pub struct ItemStatus {
    /// `none`, `held` (frozen for evidence, still reachable by staff) or `taken_down`.
    pub state: String,
}

/// The settings the takedown backend reads: the strike policy (D-293) and the operator's details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TakedownPolicy {
    pub strike_review_threshold: u32,
    pub strike_window_months: u32,
    pub operator_country: Option<String>,
    pub operator_contact: Option<String>,
}

impl Default for TakedownPolicy {
    fn default() -> Self {
        Self {
            strike_review_threshold: 3,
            strike_window_months: 12,
            operator_country: None,
            operator_contact: None,
        }
    }
}

// ---- inviter suspicion and evasion signals (Phase 7 step 5; spec section 10) ---------------------------------------

text_enum! {
    /// What an evasion signal says about a pair of Persons. A signal is a review item and never acts.
    SignalKind {
        SharedLoginIp => "shared_login_ip",
        SharedInviteLineage => "shared_invite_lineage",
        NewAccountAfterBan => "new_account_after_ban",
    }
}

text_enum! {
    /// Where a signal is in review.
    SignalState {
        Open => "open",
        Dismissed => "dismissed",
        Confirmed => "confirmed",
    }
}

/// An account as staff see it in the panel.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PersonAccountRef {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub username: String,
}

/// One step of the suspicion history (`suspicion_events`); a clear is a step with a negative delta and the
/// reason the staff member gave. Steps are never deleted.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PanelEvent {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub delta: i32,
    pub level_after: i32,
    /// `raised`, `cleared` or `reregistration_refused` (a banned Person's registration was refused).
    pub kind: String,
    pub reason: String,
    /// The banned account that raised it.
    pub source_account: Option<PersonAccountRef>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub actor_id: Option<i64>,
    /// The case this step opened or fed.
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub case_id: Option<i64>,
    pub created_at_ms: i64,
}

/// A banned account somewhere below the Person in the invite tree.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BannedDescendant {
    pub account: PersonAccountRef,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub person_id: i64,
    /// 1 for an account the Person invited directly.
    pub depth: i32,
    /// `serious` raises the inviter's suspicion; `standard` does not (D-031).
    pub ban_category: String,
    /// The why (spec section 8), when the ban carries one.
    pub reason_category: Option<String>,
    pub banned_at_ms: i64,
    /// The ban was lifted.
    pub lifted: bool,
    /// The newest case about that Person.
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub case_id: Option<i64>,
}

/// The chain of inviters from the root down to one of the Person's accounts.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct InvitePath {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub account_id: i64,
    /// Root first; the last entry is the account itself.
    pub nodes: Vec<PersonAccountRef>,
}

/// An automatic case about the Person, open or not.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SuspicionCaseRef {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub state: CaseState,
    /// `suspicion_freeze` or `suspicion_escalation`; absent for a case a Person link demoted (D-351).
    pub auto_kind: Option<String>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub assignee_id: Option<i64>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub linked_case_id: Option<i64>,
    pub opened_at_ms: i64,
    /// Set when a Person link demoted this case: the surviving automatic case it was folded into (D-351).
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub folded_into_case_id: Option<i64>,
}

/// An evasion signal: a review item about a pair of Persons. It carries counts and words, never an address.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SignalView {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub person_id: i64,
    pub account: Option<PersonAccountRef>,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub related_person_id: i64,
    pub related_account: Option<PersonAccountRef>,
    pub kind: SignalKind,
    #[schema(value_type = Object)]
    pub detail: Value,
    pub state: SignalState,
    pub created_at_ms: i64,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub reviewed_by: Option<i64>,
    pub reviewed_at_ms: Option<i64>,
    pub review_reason: Option<String>,
    /// The case opened from this signal.
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub case_id: Option<i64>,
    /// Set when a Person link folded this signal: the Person that was merged away (D-349, D-350).
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub folded_from_person_id: Option<i64>,
}

/// One address both Persons signed in from inside the window (staff with `MANAGE_ACCOUNTS`; the read is logged).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SharedAddress {
    pub ip: String,
    pub person_last_seen_ms: i64,
    pub related_last_seen_ms: i64,
    /// How many Persons signed in from it in the window.
    pub persons: i64,
}

/// A signal with the addresses behind it, read live: nothing is stored, so what retention or an account deletion
/// removed is gone from here too.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SignalDetail {
    pub signal: SignalView,
    pub window_days: i64,
    pub shared_addresses: Vec<SharedAddress>,
    /// What the reviewer looked at when they confirmed the signal (never an address); stays when the rows are gone.
    #[schema(value_type = Option<Object>)]
    pub snapshot: Option<Value>,
}

/// The suspicion panel of a Person page (MOD-SUS-001).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SuspicionPanel {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub person_id: i64,
    pub level: i32,
    pub freeze_threshold: i64,
    pub escalate_threshold: i64,
    /// The level has reached the freeze threshold: the Person's invite creation is paused.
    pub invites_paused: bool,
    pub accounts: Vec<PersonAccountRef>,
    pub events: Vec<PanelEvent>,
    pub banned_descendants: Vec<BannedDescendant>,
    pub invite_paths: Vec<InvitePath>,
    pub cases: Vec<SuspicionCaseRef>,
    /// Evasion signals about the Person; absent when the viewer lacks `MANAGE_ACCOUNTS` (they are IP-derived).
    pub signals: Option<Vec<SignalView>>,
}

/// What a scan of one Person raised.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ScanOutcome {
    /// Signals newly raised by this scan.
    pub raised: i64,
}

/// Query of the signal list.
#[derive(Debug, Clone, Default)]
pub struct SignalFilter {
    pub state: Option<SignalState>,
    pub person_id: Option<i64>,
    pub limit: i64,
}
