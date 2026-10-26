//! Wire and domain types of the platform admin backend. Ids are Snowflakes and
//! travel as decimal strings in JSON, like every other SIDE ROOM API.

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

/// `accounts.state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AccountState {
    Active,
    Deactivated,
    Deleted,
    PlatformBanned,
}

impl AccountState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Deactivated => "deactivated",
            Self::Deleted => "deleted",
            Self::PlatformBanned => "platform_banned",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "active" => Self::Active,
            "deactivated" => Self::Deactivated,
            "deleted" => Self::Deleted,
            "platform_banned" => Self::PlatformBanned,
            _ => return None,
        })
    }
}

/// Reason category of a platform ban (doc 06 section 5.4, doc 09). Only
/// `Serious` bans raise suspicion on the banned account's inviter (D-031).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BanCategory {
    Serious,
    Standard,
}

impl BanCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Serious => "serious",
            Self::Standard => "standard",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "serious" => Some(Self::Serious),
            "standard" => Some(Self::Standard),
            _ => None,
        }
    }
}

/// Why the staff member bans (D-211). Each has a default [`BanCategory`] the ban dialog starts
/// from; staff can raise or lower it before confirming, and the stored category is what the
/// inviter suspicion rule reads (D-031).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BanReason {
    IllegalContent,
    ChildSafety,
    Fraud,
    PlatformAbuse,
    Harassment,
    BanEvasion,
}

impl BanReason {
    pub const ALL: [Self; 6] = [
        Self::IllegalContent,
        Self::ChildSafety,
        Self::Fraud,
        Self::PlatformAbuse,
        Self::Harassment,
        Self::BanEvasion,
    ];

    /// Serious by default: illegal content, child safety, fraud and platform abuse (D-211).
    pub const fn default_category(self) -> BanCategory {
        match self {
            Self::IllegalContent | Self::ChildSafety | Self::Fraud | Self::PlatformAbuse => {
                BanCategory::Serious
            }
            Self::Harassment | Self::BanEvasion => BanCategory::Standard,
        }
    }
}

/// Why an access-log row exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AccessKind {
    /// A state-changing admin action.
    AdminAction,
    /// A read of account, Person or invite-graph detail.
    Read,
    /// A read of private content (DMs, private channels, deleted messages). A reason is required.
    PrivateRead,
    /// A staff member attempted something their roles do not allow.
    Denied,
}

impl AccessKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AdminAction => "admin_action",
            Self::Read => "read",
            Self::PrivateRead => "private_read",
            Self::Denied => "denied",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "admin_action" => Self::AdminAction,
            "read" => Self::Read,
            "private_read" => Self::PrivateRead,
            "denied" => Self::Denied,
            _ => return None,
        })
    }
}

/// One row to append to the access log (the chain fields are added by the repository).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    pub actor_id: i64,
    pub kind: AccessKind,
    pub action: String,
    pub target_kind: Option<String>,
    pub target_id: Option<i64>,
    pub reason: String,
    pub ip: Option<String>,
    /// Compact JSON (kept as text so the hash is reproducible).
    pub detail: String,
}

/// A stored, chained access-log row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AccessLogEntry {
    pub seq: i64,
    /// Microseconds since the Unix epoch.
    pub at_us: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub actor_id: i64,
    pub kind: AccessKind,
    pub action: String,
    pub target_kind: Option<String>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub target_id: Option<i64>,
    pub reason: String,
    pub ip: Option<String>,
    pub detail: String,
    /// Hex of the previous row's hash (all zeros for the first row).
    pub prev_hash: String,
    /// Hex of `sha256(prev_hash || canonical row)`.
    pub hash: String,
}

/// Result of verifying the whole chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ChainReport {
    pub ok: bool,
    pub rows_checked: i64,
    /// First sequence number that failed verification.
    pub first_bad_seq: Option<i64>,
    pub problem: Option<String>,
    /// Hash of the newest verified row; record it elsewhere to detect truncation.
    pub head_hash: Option<String>,
}

/// Page of access-log rows.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AccessLogPage {
    pub items: Vec<AccessLogEntry>,
    /// Pass as `before` to get the next (older) page.
    pub next_before: Option<i64>,
}

/// Filters of `GET /admin/access-log`.
#[derive(Debug, Clone, Default)]
pub struct AccessLogFilter {
    pub actor_id: Option<i64>,
    pub target_id: Option<i64>,
    pub action: Option<String>,
    pub kind: Option<AccessKind>,
    pub before: Option<i64>,
    pub limit: i64,
}

/// A staff role of the Staff Server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct StaffRole {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub name: String,
    pub position: i32,
    /// `PlatformPermissions` bits as a decimal string.
    pub platform_permissions: String,
}

impl StaffRole {
    pub fn permissions(&self) -> sr_core::PlatformPermissions {
        sr_core::PlatformPermissions::from_bits_truncate(
            self.platform_permissions.parse::<u64>().unwrap_or(0),
        )
    }
}

/// A current staff member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct StaffMember {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub account_id: i64,
    pub username: String,
    pub display_name: Option<String>,
    pub is_owner: bool,
    pub roles: Vec<StaffRole>,
    /// Union of the roles' platform bits (`OWNER` for the Owner), decimal string.
    pub platform_permissions: String,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub appointed_by: Option<i64>,
    pub appointed_at_ms: i64,
    pub reason: String,
}

/// A closed appointment (G-23): who stopped being staff and how it ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct FormerStaff {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub account_id: i64,
    pub username: String,
    /// `left`, `kicked` or `revoked`.
    pub revoked_reason: String,
    pub revoked_at_ms: i64,
    /// The note staff gave when revoking (or the system text for leaving).
    pub note: String,
}

/// The resolved platform standing of an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Standing {
    pub permissions: sr_core::PlatformPermissions,
    pub is_owner: bool,
    /// Set for a delegate: the most they may hand out (D-132).
    pub grant_ceiling: Option<sr_core::PlatformPermissions>,
}

/// The authenticated staff caller of a service method.
#[derive(Debug, Clone)]
pub struct Caller {
    pub user_id: i64,
    pub permissions: sr_core::PlatformPermissions,
    pub is_owner: bool,
    pub ip: Option<String>,
}

impl Caller {
    /// The operator CLI acts as the Owner under the system user.
    pub fn operator(system_user: i64) -> Self {
        Self {
            user_id: system_user,
            permissions: sr_core::PlatformPermissions::OWNER,
            is_owner: true,
            ip: Some("cli".to_owned()),
        }
    }
}

/// Account row for lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AccountSummary {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub person_id: i64,
    pub username: String,
    pub display_name: Option<String>,
    pub email: String,
    pub email_verified: bool,
    pub state: AccountState,
    pub flags: i64,
    pub created_at_ms: i64,
    pub state_changed_at_ms: i64,
}

/// A page of accounts; `next_after` is the cursor for the following page.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AccountPage {
    pub items: Vec<AccountSummary>,
    pub next_after: Option<String>,
}

/// Filters of the account search.
#[derive(Debug, Clone, Default)]
pub struct AccountQuery {
    /// Matches username, display name, email (substring) or an exact id.
    pub q: Option<String>,
    pub state: Option<AccountState>,
    pub person_id: Option<i64>,
    /// Accounts linked to this Discord user id.
    pub discord_id: Option<String>,
    /// Accounts that signed in from this IP address.
    pub ip: Option<String>,
    pub after: Option<i64>,
    pub limit: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LoginEvent {
    pub ip: Option<String>,
    pub success: bool,
    pub at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AccountRef {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub username: String,
}

/// Everything the account drawer shows.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AccountDetail {
    pub account: AccountSummary,
    pub invited_by: Option<AccountRef>,
    pub sessions_active: i64,
    pub mfa_enabled: bool,
    pub credit_balance: i64,
    pub invites_issued: i64,
    pub invites_redeemed: i64,
    pub suspicion_level: i32,
    pub person_banned: bool,
    pub recent_logins: Vec<LoginEvent>,
    /// Staff froze this account's invite creation by hand (ADM-GRAPH-004).
    #[serde(default)]
    pub invites_frozen: bool,
}

/// What `reset 2FA` removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, Default)]
pub struct Reset2faReport {
    pub totp: u64,
    pub recovery_codes: u64,
    pub passkeys: u64,
    pub trusted_devices: u64,
    pub sessions_revoked: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DeleteRequest {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub account_id: i64,
    pub state: String,
    pub created_at_ms: i64,
}

/// A Person and its accounts.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PersonDetail {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub notes: String,
    pub suspicion_level: i32,
    pub accounts: Vec<AccountSummary>,
    /// Discord identities of the accounts of this Person, live and moved (D-262).
    #[serde(default)]
    pub identities: Vec<IdentitySummary>,
    pub ban: Option<BanRecord>,
}

/// A Discord identity row of one of a Person's accounts. A moved one is history: `unlinked_at_ms` and
/// `moved_to_account_id` are set and a newer live row holds the identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IdentitySummary {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub account_id: i64,
    pub provider: String,
    pub provider_user_id: String,
    pub linked_at_ms: i64,
    pub unlinked_at_ms: Option<i64>,
    #[serde(with = "opt_id", default)]
    #[schema(value_type = Option<String>)]
    pub moved_to_account_id: Option<i64>,
}

/// What a completed identity move changed, for the notification to the old account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityMoved {
    pub provider: String,
    pub provider_user_id: String,
    pub from_account_id: i64,
    pub to_account_id: i64,
    pub to_username: String,
    /// The target account was in another Person and was linked into this one by the move.
    pub linked_target: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BanRecord {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub person_id: i64,
    pub category: BanCategory,
    pub note: String,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub banned_by: i64,
    pub created_at_ms: i64,
    pub lifted_at_ms: Option<i64>,
    /// Why (spec 8, `ban_reason_category`), stamped when the ban was made through a case; `None` for older bans.
    #[serde(default)]
    pub reason_category: Option<String>,
}

/// Filters of the bans list (MOD-BAN-003). Every field narrows; none set lists all bans.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BanFilter {
    pub active_only: bool,
    pub category: Option<BanCategory>,
    /// A `ban_reason_category` value, or `none` for bans made without one.
    pub reason_category: Option<String>,
    /// Bans made at least this many days ago.
    pub older_than_days: Option<i32>,
    /// Bans made at most this many days ago.
    pub newer_than_days: Option<i32>,
}

/// The values of `ban_reason_category` (spec 8) a filter may name, plus `none`.
pub const BAN_REASON_FILTERS: [&str; 10] = [
    "spam",
    "harassment",
    "illegal_content",
    "child_safety",
    "ban_evasion",
    "fraud",
    "platform_abuse",
    "nsfw_policy",
    "other",
    "none",
];

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BanPage {
    pub items: Vec<BanRecord>,
    pub next_after: Option<String>,
}

/// Outcome of banning a Person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BanOutcome {
    pub ban: BanRecord,
    pub accounts_banned: u64,
    pub sessions_revoked: u64,
    /// Set when a serious ban raised suspicion on the inviter's Person.
    pub inviter_suspicion: Option<SuspicionChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SuspicionChange {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub person_id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub inviter_account_id: i64,
    pub level: i32,
    /// Level reached the freeze threshold: the Person's invite creation is frozen.
    pub frozen: bool,
    /// Level reached the escalation threshold.
    pub escalated: bool,
    /// This very ban moved the level across the freeze threshold (it was below before).
    #[serde(default)]
    pub freeze_crossed: bool,
    /// This very ban moved the level across the escalation threshold.
    #[serde(default)]
    pub escalate_crossed: bool,
    /// The Person whose ban caused it (the invitee). Staff only; never shown to the inviter.
    #[serde(default, with = "id")]
    #[schema(value_type = String)]
    pub banned_person_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UnbanOutcome {
    pub accounts_reactivated: u64,
    /// Accounts left deactivated because the account cap was reached.
    pub accounts_left_deactivated: u64,
}

/// A node of the invite graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct InviteNode {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub account_id: i64,
    pub username: String,
    pub state: AccountState,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub person_id: i64,
    pub suspicion_level: i32,
    /// The inviter's account id (`None` for the root: the Owner).
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub parent_id: Option<i64>,
    /// Hops from the requested account (0 = itself, negative = ancestors).
    pub depth: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, Default)]
pub struct InviteStats {
    pub issued: i64,
    pub open: i64,
    pub redeemed: i64,
    pub revoked: i64,
    pub descendants: i64,
    pub banned_descendants: i64,
}

/// The inviter chain of an account plus its stats.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InviteTree {
    pub account: InviteNode,
    /// Root first, ending with the account's direct inviter.
    pub ancestors: Vec<InviteNode>,
    pub stats: InviteStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InviteDescendants {
    pub nodes: Vec<InviteNode>,
    /// True when the node cap cut the result short.
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LedgerEntry {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub delta: i32,
    pub reason: String,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub ref_id: Option<i64>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreditLedger {
    pub balance: i64,
    pub entries: Vec<LedgerEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct InviteRow {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub code: String,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub issuer_account_id: i64,
    pub state: String,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub redeemed_by_account_id: Option<i64>,
    pub note: String,
    pub emergency: bool,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InvitePage {
    pub items: Vec<InviteRow>,
    pub next_after: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct InviteQuery {
    pub issuer_id: Option<i64>,
    pub state: Option<String>,
    pub after: Option<i64>,
    pub limit: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SuspectPerson {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub person_id: i64,
    pub suspicion_level: i32,
    pub frozen: bool,
    pub accounts: Vec<AccountRef>,
}

/// A stored platform setting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredSetting {
    pub value: serde_json::Value,
    pub updated_by: Option<i64>,
    pub updated_at_ms: i64,
}

/// One bucket measured by the daily `storage.measure` job: the object store, Library media or Cinema media.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct StorageFigure {
    pub bucket: String,
    pub bytes: i64,
    pub objects: i64,
    /// The walk hit its object cap or time budget, so the figures are a lower bound.
    pub truncated: bool,
    pub measured_at_ms: i64,
}

/// Counts for the dashboard and system page.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SystemCounts {
    pub accounts_active: i64,
    pub accounts_deactivated: i64,
    pub accounts_banned: i64,
    pub accounts_deleted: i64,
    pub sessions_active: i64,
    pub jobs_queued: i64,
    pub jobs_running: i64,
    pub jobs_dead: i64,
    pub staff: i64,
    /// The account cap (`accounts.cap`), for the gauge next to `accounts_active`.
    #[serde(default)]
    pub accounts_cap: i64,
    /// Submissions waiting for a reviewer (submitted or in review).
    #[serde(default)]
    pub review_pending: i64,
    /// Unused invite codes.
    #[serde(default)]
    pub invites_open: i64,
    /// Staff alerts nobody acknowledged yet.
    #[serde(default)]
    pub alerts_unacked: i64,
    /// Bytes of stored attachments (live rows).
    #[serde(default)]
    pub storage_bytes: i64,
    /// The daily `storage.measure` figures (RDY-OBS-002); empty before its first run.
    #[serde(default)]
    pub storage_figures: Vec<StorageFigure>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct JobRow {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub kind: String,
    pub state: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub last_error: Option<String>,
    pub run_at_ms: i64,
    pub finished_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct JobPage {
    pub items: Vec<JobRow>,
    pub next_after: Option<String>,
}
