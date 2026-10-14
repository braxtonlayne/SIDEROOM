//! Domain types of the developer platform. Ids are snowflakes as `i64`, times are Unix
//! milliseconds (the HTTP layer renders ISO8601).

use serde_json::Value;
use sr_auth::session::TokenHash;

/// Privileged gateway intents (Discord bit positions).
pub mod intents {
    /// `GUILD_MEMBERS`.
    pub const GUILD_MEMBERS: u64 = 1 << 1;
    /// `GUILD_PRESENCES`.
    pub const GUILD_PRESENCES: u64 = 1 << 8;
    /// `MESSAGE_CONTENT`.
    pub const MESSAGE_CONTENT: u64 = 1 << 15;
    /// All privileged bits.
    pub const PRIVILEGED: u64 = GUILD_MEMBERS | GUILD_PRESENCES | MESSAGE_CONTENT;

    /// Wire names, in a stable order.
    pub const NAMES: [(&str, u64); 3] = [
        ("GUILD_MEMBERS", GUILD_MEMBERS),
        ("GUILD_PRESENCES", GUILD_PRESENCES),
        ("MESSAGE_CONTENT", MESSAGE_CONTENT),
    ];

    /// Bits to names.
    pub fn names(bits: u64) -> Vec<&'static str> {
        NAMES
            .iter()
            .filter(|(_, b)| bits & b != 0)
            .map(|(n, _)| *n)
            .collect()
    }

    /// Names to bits; `Err` holds the first unknown name.
    pub fn from_names<S: AsRef<str>>(names: &[S]) -> Result<u64, String> {
        let mut bits = 0;
        for n in names {
            let (_, b) = NAMES
                .iter()
                .find(|(x, _)| *x == n.as_ref())
                .ok_or_else(|| n.as_ref().to_owned())?;
            bits |= b;
        }
        Ok(bits)
    }
}

/// `accounts.flags` bit: the account may build apps (approved developer request).
pub const FLAG_APP_DEVELOPER: i64 = 1 << 1;

/// Where an application is in its life (doc 11 section 2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseState {
    /// Test servers only.
    Development,
    /// A version waits for staff.
    InReview,
    /// Installable anywhere (subject to `bot_public`).
    Released,
    /// Switched off by staff.
    Suspended,
}

impl ReleaseState {
    /// Database / wire value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::InReview => "in_review",
            Self::Released => "released",
            Self::Suspended => "suspended",
        }
    }

    /// Parse the database value.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "development" => Self::Development,
            "in_review" => Self::InReview,
            "released" => Self::Released,
            "suspended" => Self::Suspended,
            _ => return None,
        })
    }
}

/// An application and its bot account.
#[derive(Debug, Clone)]
pub struct Application {
    /// Application id (also the OAuth2 `client_id`).
    pub id: i64,
    /// Owner account.
    pub owner_id: i64,
    /// The bot's account id.
    pub bot_user_id: i64,
    /// Name (2-32).
    pub name: String,
    /// Description (0-400).
    pub description: String,
    /// Icon hash.
    pub icon_key: Option<String>,
    /// Up to 5 tags.
    pub tags: Vec<String>,
    /// Anyone may add the bot (else only the owner).
    pub bot_public: bool,
    /// Stored for parity.
    pub bot_require_code_grant: bool,
    /// Registered redirect URIs.
    pub redirect_uris: Vec<String>,
    /// Default install scopes.
    pub install_scopes: Vec<String>,
    /// Default install permissions (bits).
    pub install_permissions: u64,
    /// Terms of service URL.
    pub terms_url: Option<String>,
    /// Privacy policy URL.
    pub privacy_url: Option<String>,
    /// Release state.
    pub release_state: ReleaseState,
    /// Why it was suspended.
    pub suspended_reason: Option<String>,
    /// Privileged intents the developer asks for.
    pub intents_requested: u64,
    /// Privileged intents staff approved.
    pub intents_approved: u64,
    /// Digest of the client secret.
    pub client_secret_hash: TokenHash,
    /// Last four characters of the client secret.
    pub client_secret_hint: String,
    /// Creation time.
    pub created_at_ms: i64,
}

impl Application {
    /// Privileged intents the gateway lets the bot IDENTIFY with: the developer's set while
    /// the application is in test servers, the approved subset once released.
    pub fn allowed_intents(&self) -> u64 {
        let asked = self.intents_requested & intents::PRIVILEGED;
        match self.release_state {
            ReleaseState::Released => asked & self.intents_approved,
            ReleaseState::Suspended => 0,
            ReleaseState::Development | ReleaseState::InReview => asked,
        }
    }

    /// Discord application `flags` derived from the intents.
    pub fn flags(&self) -> u64 {
        let released = self.release_state == ReleaseState::Released;
        let on = self.allowed_intents();
        let mut f = 0u64;
        for (bit, full, limited) in [
            (intents::GUILD_PRESENCES, 1u64 << 12, 1u64 << 13),
            (intents::GUILD_MEMBERS, 1 << 14, 1 << 15),
            (intents::MESSAGE_CONTENT, 1 << 18, 1 << 19),
        ] {
            if on & bit != 0 {
                f |= if released { full } else { limited };
            }
        }
        f
    }

    /// Whether the application may do anything at all.
    pub fn is_live(&self) -> bool {
        self.release_state != ReleaseState::Suspended
    }
}

/// Fields of `PATCH /applications/{id}` after validation (`None` = unchanged).
#[derive(Debug, Clone, Default)]
pub struct AppPatch {
    /// New name.
    pub name: Option<String>,
    /// New description.
    pub description: Option<String>,
    /// New icon hash (`Some(None)` clears).
    pub icon_key: Option<Option<String>>,
    /// New tags.
    pub tags: Option<Vec<String>>,
    /// Public bot toggle.
    pub bot_public: Option<bool>,
    /// Code grant toggle.
    pub bot_require_code_grant: Option<bool>,
    /// New redirect URIs.
    pub redirect_uris: Option<Vec<String>>,
    /// New default install scopes.
    pub install_scopes: Option<Vec<String>>,
    /// New default install permissions.
    pub install_permissions: Option<u64>,
    /// Terms URL (`Some(None)` clears).
    pub terms_url: Option<Option<String>>,
    /// Privacy URL (`Some(None)` clears).
    pub privacy_url: Option<Option<String>>,
    /// Requested privileged intents.
    pub intents_requested: Option<u64>,
}

/// Everything needed to create an application and its bot in one transaction.
#[derive(Debug, Clone)]
pub struct NewApplication {
    /// Application id.
    pub id: i64,
    /// Owner.
    pub owner_id: i64,
    /// Bot account id.
    pub bot_user_id: i64,
    /// Person row id for the bot account.
    pub bot_person_id: i64,
    /// `bot_<id>`.
    pub bot_username: String,
    /// Name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Client secret digest.
    pub secret_hash: TokenHash,
    /// Client secret hint.
    pub secret_hint: String,
    /// First bot token row id.
    pub token_id: i64,
    /// First bot token digest.
    pub token_hash: TokenHash,
    /// Audit row id for `app.create`.
    pub audit_id: i64,
    /// Now.
    pub now_ms: i64,
}

/// A bot token resolved by its digest.
#[derive(Debug, Clone)]
pub struct BotRecord {
    /// Token row id.
    pub token_id: i64,
    /// The bot account.
    pub user_id: i64,
    /// Its application.
    pub application: Application,
    /// Digest stored for the token (compared again in constant time).
    pub token_hash: TokenHash,
    /// Whether the bot account is still active.
    pub account_active: bool,
}

/// An appended application audit row.
#[derive(Debug, Clone, PartialEq)]
pub struct AuditRow {
    /// Row id.
    pub id: i64,
    /// Application.
    pub application_id: i64,
    /// Acting account.
    pub actor_id: i64,
    /// `app.create`, `app.secret_reset`, ...
    pub action: String,
    /// Details (never secrets).
    pub details: Value,
    /// Time.
    pub created_at_ms: i64,
}

/// A request to be a developer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeveloperRequest {
    /// Id.
    pub id: i64,
    /// Applicant.
    pub account_id: i64,
    /// What they want to build.
    pub pitch: String,
    /// Prior work.
    pub prior_work: String,
    /// `pending`, `approved`, `rejected`, `suspended`.
    pub state: String,
    /// Reviewer's note.
    pub reviewer_note: Option<String>,
    /// Created.
    pub created_at_ms: i64,
    /// Decided.
    pub reviewed_at_ms: Option<i64>,
}

/// A release version.
#[derive(Debug, Clone, PartialEq)]
pub struct AppVersion {
    /// Id.
    pub id: i64,
    /// Application.
    pub application_id: i64,
    /// 1-based.
    pub version: i32,
    /// Manifest snapshot.
    pub manifest: Value,
    /// Why it needs review (`first_release`, `scopes`, `redirect_uris`, `intents`).
    pub needs_review_reasons: Vec<String>,
    /// Submitter's note.
    pub note: Option<String>,
    /// `pending`, `approved`, `rejected`, `changes_requested`, `withdrawn`.
    pub state: String,
    /// Reviewer's note.
    pub reviewer_note: Option<String>,
    /// Created.
    pub created_at_ms: i64,
    /// Decided.
    pub decided_at_ms: Option<i64>,
}

/// An install of an application in a guild.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Install {
    /// Application.
    pub application_id: i64,
    /// Guild.
    pub guild_id: i64,
    /// Who installed it.
    pub installed_by: i64,
    /// Scopes at install time.
    pub scopes: Vec<String>,
    /// Granted permission bits.
    pub permissions: u64,
    /// The managed role.
    pub role_id: Option<i64>,
    /// Time.
    pub created_at_ms: i64,
}

/// Facts about a guild the install rules need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuildStats {
    /// The owner.
    pub owner_id: i64,
    /// Members including bots.
    pub member_count: i64,
}

/// One slash/user/message command.
#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    /// Id.
    pub id: i64,
    /// Application.
    pub application_id: i64,
    /// Guild, or `None` for global.
    pub guild_id: Option<i64>,
    /// 1 chat input, 2 user, 3 message.
    pub kind: i16,
    /// Name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Options.
    pub options: Value,
    /// Localized names.
    pub name_localizations: Option<Value>,
    /// Localized descriptions.
    pub description_localizations: Option<Value>,
    /// Default member permissions (bits) or `None` for everyone.
    pub default_member_permissions: Option<u64>,
    /// DM availability (stored).
    pub dm_permission: bool,
    /// Age-restricted.
    pub nsfw: bool,
    /// Contexts (stored).
    pub contexts: Option<Value>,
    /// Integration types (stored).
    pub integration_types: Option<Value>,
    /// Snowflake that changes with the command.
    pub version: i64,
}

/// A command as submitted, validated, before ids and versions are assigned.
#[derive(Debug, Clone, PartialEq)]
pub struct NewCommand {
    /// 1, 2 or 3.
    pub kind: i16,
    /// Name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Options.
    pub options: Value,
    /// Localized names.
    pub name_localizations: Option<Value>,
    /// Localized descriptions.
    pub description_localizations: Option<Value>,
    /// Default member permissions.
    pub default_member_permissions: Option<u64>,
    /// DM availability.
    pub dm_permission: bool,
    /// Age-restricted.
    pub nsfw: bool,
    /// Contexts.
    pub contexts: Option<Value>,
    /// Integration types.
    pub integration_types: Option<Value>,
}

/// One entry of a command's permission overwrites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandPermission {
    /// Role, user or channel id.
    pub target_id: i64,
    /// 1 role, 2 user, 3 channel.
    pub target_type: i16,
    /// Allowed?
    pub allowed: bool,
}

/// An OAuth2 grant (one live per user and application).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    /// Id.
    pub id: i64,
    /// Application.
    pub application_id: i64,
    /// User.
    pub user_id: i64,
    /// Scopes.
    pub scopes: Vec<String>,
    /// Created.
    pub created_at_ms: i64,
}

/// A stored authorization code.
#[derive(Debug, Clone)]
pub struct CodeRecord {
    /// Digest of the code.
    pub code_hash: TokenHash,
    /// Grant.
    pub grant: Grant,
    /// The redirect URI it was issued for.
    pub redirect_uri: String,
    /// Scopes of this code.
    pub scopes: Vec<String>,
    /// PKCE challenge.
    pub code_challenge: Option<String>,
    /// Expiry.
    pub expires_at_ms: i64,
    /// Used already?
    pub used: bool,
}

/// An access/refresh token pair.
#[derive(Debug, Clone)]
pub struct TokenRecord {
    /// Row id.
    pub id: i64,
    /// Grant.
    pub grant_id: i64,
    /// Application.
    pub application_id: i64,
    /// User.
    pub user_id: i64,
    /// Scopes.
    pub scopes: Vec<String>,
    /// Access digest.
    pub access_hash: TokenHash,
    /// Refresh digest.
    pub refresh_hash: TokenHash,
    /// Access expiry.
    pub access_expires_at_ms: i64,
    /// Refresh expiry.
    pub refresh_expires_at_ms: i64,
    /// The refresh token was already exchanged.
    pub refresh_used: bool,
    /// Revoked.
    pub revoked: bool,
}

/// A stored interaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionRow {
    /// Id.
    pub id: i64,
    /// Application.
    pub application_id: i64,
    /// 2 command, 3 component, 4 autocomplete, 5 modal submit.
    pub kind: i16,
    /// The invoking user.
    pub user_id: i64,
    /// Guild.
    pub guild_id: Option<i64>,
    /// Channel.
    pub channel_id: i64,
    /// Command.
    pub command_id: Option<i64>,
    /// Message a component sits on.
    pub source_message_id: Option<i64>,
    /// Client nonce.
    pub nonce: Option<String>,
    /// `created`, `responded` or `deferred`.
    pub state: String,
    /// Callback type used.
    pub response_type: Option<i16>,
    /// The (first) response is ephemeral.
    pub ephemeral: bool,
    /// The original response message.
    pub original_message_id: Option<i64>,
    /// Creation time.
    pub created_at_ms: i64,
    /// Token validity end.
    pub expires_at_ms: i64,
    /// Digest of the token.
    pub token_hash: [u8; 32],
}

/// A message that belongs to an interaction.
#[derive(Debug, Clone, PartialEq)]
pub struct InteractionMessage {
    /// Message id.
    pub message_id: i64,
    /// Interaction.
    pub interaction_id: i64,
    /// Ephemeral?
    pub ephemeral: bool,
    /// The original response?
    pub is_original: bool,
    /// Stored object of an ephemeral message.
    pub payload: Option<Value>,
}
