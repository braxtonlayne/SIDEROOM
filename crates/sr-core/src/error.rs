//! Error taxonomy and Discord-style JSON error bodies (FND-CORE-002).
//!
//! Body shape: `{ "code": <int>, "message": "...", "errors": {...field tree...} }`.
//! Codes reuse Discord's JSON error codes where an equivalent exists; SIDE ROOM
//! specific codes start at 900000 (doc 05).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

macro_rules! error_codes {
    ($($variant:ident = $code:expr, $status:expr, $msg:expr;)*) => {
        /// Numeric API error code.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum ErrorCode { $($variant,)* }

        impl ErrorCode {
            pub const ALL: &'static [ErrorCode] = &[$(ErrorCode::$variant,)*];
            pub const fn code(self) -> u32 { match self { $(ErrorCode::$variant => $code,)* } }
            /// Default HTTP status for this code.
            pub const fn http_status(self) -> u16 { match self { $(ErrorCode::$variant => $status,)* } }
            /// Default user-facing message.
            pub const fn message(self) -> &'static str { match self { $(ErrorCode::$variant => $msg,)* } }
            pub fn from_code(code: u32) -> Option<Self> {
                match code { $($code => Some(ErrorCode::$variant),)* _ => None }
            }
        }
    };
}

error_codes! {
    General = 0, 400, "General error";
    UnknownAccount = 10001, 404, "Unknown Account";
    UnknownApplication = 10002, 404, "Unknown Application";
    UnknownChannel = 10003, 404, "Unknown Channel";
    UnknownGuild = 10004, 404, "Unknown Guild";
    UnknownInvite = 10006, 404, "Unknown Invite";
    UnknownMember = 10007, 404, "Unknown Member";
    UnknownMessage = 10008, 404, "Unknown Message";
    UnknownRole = 10011, 404, "Unknown Role";
    UnknownToken = 10012, 401, "Unknown Token";
    UnknownUser = 10013, 404, "Unknown User";
    UnknownEmoji = 10014, 404, "Unknown Emoji";
    UnknownWebhook = 10015, 404, "Unknown Webhook";
    UnknownSession = 10020, 404, "Unknown Session";
    UnknownBan = 10026, 404, "Unknown Ban";
    UnknownInteraction = 10062, 404, "Unknown interaction";
    UnknownApplicationCommand = 10063, 404, "Unknown application command";
    UnknownStageInstance = 10067, 404, "Unknown Stage Instance";
    UnknownScheduledEvent = 10070, 404, "Unknown Guild Scheduled Event";
    UnknownSound = 10097, 404, "Unknown Soundboard Sound";
    BotsCannotUseThisEndpoint = 20001, 403, "Bots cannot use this endpoint";
    OnlyBotsCanUseThisEndpoint = 20002, 403, "Only bots can use this endpoint";
    MaxGuilds = 30001, 400, "Maximum number of guilds reached";
    MaxRoles = 30005, 400, "Maximum number of guild roles reached";
    MaxReactions = 30010, 400, "Maximum number of reactions reached";
    MaxChannels = 30013, 400, "Maximum number of guild channels reached";
    MaxPins = 30003, 400, "Maximum number of pins reached for the channel";
    MaxInvites = 30016, 400, "Maximum number of invites reached";
    MaxFriends = 30002, 400, "Maximum number of friends reached (1000)";
    MaxGroupDms = 30011, 400, "Maximum number of group DMs reached (10)";
    MaxEmojis = 30008, 400, "Maximum number of emojis reached";
    MaxStickers = 30039, 400, "Maximum number of stickers reached";
    MaxSounds = 30045, 400, "Maximum number of soundboard sounds reached";
    SlowmodeRateLimit = 20016, 429, "This action cannot be performed due to slowmode rate limit";
    NoUsersWithDiscordTag = 80004, 400, "No users with DiscordTag exist";
    Unauthorized = 40001, 401, "Unauthorized";
    VerifyAccount = 40002, 403, "You need to verify your account";
    RequestEntityTooLarge = 40005, 413, "Request entity too large";
    FeatureDisabled = 40006, 403, "This feature has been temporarily disabled server-side";
    UserBanned = 40007, 403, "The user is banned from this guild";
    TargetNotInVoice = 40032, 400, "Target user is not connected to voice.";
    MessageAlreadyCrossposted = 40033, 400, "This message has already been crossposted.";
    InteractionAlreadyAcknowledged = 40060, 400, "Interaction has already been acknowledged.";
    TooManyRequests = 40062, 429, "You are being rate limited";
    TagRequired = 40067, 400, "A tag is required to create a forum post in this channel";
    MissingAccess = 50001, 403, "Missing Access";
    InvalidAccountType = 50002, 403, "Invalid Account Type";
    CannotExecuteOnDm = 50003, 403, "Cannot execute action on a DM channel";
    CannotEditMessageByOther = 50005, 403, "Cannot edit a message authored by another user";
    CannotExecuteOnChannelType = 50024, 400, "Cannot execute action on this channel type";
    CannotSendEmptyMessage = 50006, 400, "Cannot send an empty message";
    CannotSendMessagesToUser = 50007, 403, "Cannot send messages to this user";
    MissingPermissions = 50013, 403, "Missing Permissions";
    InvalidAuthenticationToken = 50014, 401, "Invalid authentication token";
    InvalidWebhookToken = 50027, 401, "Invalid webhook token";
    NoteTooLong = 50015, 400, "Note was too long";
    InvalidFormBody = 50035, 400, "Invalid Form Body";
    InvalidApiVersion = 50041, 400, "Invalid API version provided";
    ThreadArchived = 50083, 400, "Thread is archived";
    TwoFactorRequired = 60003, 403, "Two factor is required for this operation";
    ReactionBlocked = 90001, 403, "Reaction was blocked";
    ApiResourceOverloaded = 130000, 503, "API resource is currently overloaded. Try again a little later";
    ThreadAlreadyCreated = 160004, 400, "A thread has already been created for this message";
    ThreadLocked = 160005, 400, "Thread is locked";
    StageAlreadyOpen = 150006, 400, "A stage instance is already open for this channel";
    MaxActiveThreads = 160006, 400, "Maximum number of active threads reached";
    AutoModBlocked = 200000, 400, "Message was blocked by AutoMod";

    // SIDE ROOM specific (>= 900000).
    SrFeatureDisabled = 900001, 403, "This feature is disabled on this installation";
    SrStaffOnly = 900002, 403, "This action requires platform staff privileges";
    SrReasonRequired = 900003, 400, "A reason is required for this action";
    SrReauthRequired = 900004, 403, "Please re-confirm your identity to continue";
    SrInviteRequired = 900005, 403, "An invite is required to register";
    SrInviteExhausted = 900006, 403, "This invite can no longer be used";
    SrAccountDeactivated = 900007, 403, "This account has been deactivated";
    SrPlatformBanned = 900008, 403, "This account is banned from the platform";
    SrNotFound = 900009, 404, "Unknown resource";
    SrConflict = 900010, 409, "The current state does not allow this action";
    SrAgeConfirmationRequired = 900011, 403, "You need to confirm you are 18 or older to open this";
    SrSoundboardCooldown = 900012, 429, "Wait a moment before playing another sound";
    SrSecondFactorRequired = 900014, 403, "Set up a second factor (authenticator app or passkey) first";
    SrAppNotReleased = 900120, 403, "App not released";
    SrAppSuspended = 900121, 403, "This application is suspended";
    SrIgnoreListFull = 900101, 400, "Your ignore list is full";
    SrInternal = 900500, 500, "Internal server error";
}

impl Serialize for ErrorCode {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u32(self.code())
    }
}

/// A field-level validation error: `{ "code": "...", "message": "..." }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldError {
    pub code: String,
    pub message: String,
}

/// Library-level error type. Services convert domain failures into this, and the
/// HTTP layer renders it with [`ApiError::body`] and [`ApiError::status`].
#[derive(Debug, Clone, thiserror::Error)]
#[error("{code:?} ({}): {message}", code.code())]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
    /// Dotted field path -> errors, rendered as Discord's nested `errors` tree.
    pub fields: Vec<(String, FieldError)>,
}

impl ApiError {
    pub fn new(code: ErrorCode) -> Self {
        Self {
            code,
            message: code.message().to_owned(),
            fields: Vec::new(),
        }
    }

    pub fn with_message(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            fields: Vec::new(),
        }
    }

    /// 50035 Invalid Form Body with no fields yet; chain [`ApiError::field`].
    pub fn invalid_form() -> Self {
        Self::new(ErrorCode::InvalidFormBody)
    }

    /// Add a field error; `path` is dotted, numeric segments become array indices
    /// in Discord's tree (`"embeds.0.title"`).
    pub fn field(
        mut self,
        path: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        self.fields.push((
            path.into(),
            FieldError {
                code: code.into(),
                message: message.into(),
            },
        ));
        self
    }

    pub fn status(&self) -> u16 {
        self.code.http_status()
    }

    /// JSON body: `{code, message}` plus `errors` when fields are present.
    pub fn body(&self) -> Value {
        let mut out = json!({ "code": self.code.code(), "message": self.message });
        if !self.fields.is_empty() {
            let mut root = Map::new();
            for (path, fe) in &self.fields {
                let segs: Vec<&str> = path.split('.').collect();
                insert_field(&mut root, &segs, fe);
            }
            out["errors"] = Value::Object(root);
        }
        out
    }
}

fn insert_field(node: &mut Map<String, Value>, segs: &[&str], fe: &FieldError) {
    match segs.split_first() {
        None => {
            let list = node
                .entry("_errors".to_owned())
                .or_insert_with(|| Value::Array(Vec::new()));
            if let Value::Array(a) = list {
                a.push(json!({ "code": fe.code, "message": fe.message }));
            }
        }
        Some((head, rest)) => {
            let child = node
                .entry((*head).to_owned())
                .or_insert_with(|| Value::Object(Map::new()));
            if !child.is_object() {
                *child = Value::Object(Map::new());
            }
            if let Value::Object(m) = child {
                insert_field(m, rest, fe);
            }
        }
    }
}

impl From<ErrorCode> for ApiError {
    fn from(code: ErrorCode) -> Self {
        Self::new(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn codes_unique_and_roundtrip() {
        let mut seen = HashSet::new();
        for c in ErrorCode::ALL {
            assert!(seen.insert(c.code()), "duplicate {}", c.code());
            assert_eq!(ErrorCode::from_code(c.code()), Some(*c));
        }
        assert_eq!(ErrorCode::MissingPermissions.code(), 50013);
        assert_eq!(ErrorCode::UnknownChannel.code(), 10003);
    }

    #[test]
    fn discord_codes_verbatim() {
        assert_eq!(ErrorCode::SlowmodeRateLimit.code(), 20016);
        assert_eq!(ErrorCode::SlowmodeRateLimit.http_status(), 429);
        assert_eq!(ErrorCode::MaxEmojis.code(), 30008);
        assert_eq!(ErrorCode::MaxStickers.code(), 30039);
        assert_eq!(ErrorCode::MaxFriends.code(), 30002);
        assert_eq!(ErrorCode::MaxGroupDms.code(), 30011);
        assert_eq!(ErrorCode::NoUsersWithDiscordTag.code(), 80004);
        assert_eq!(ErrorCode::AutoModBlocked.code(), 200000);
        assert_eq!(ErrorCode::AutoModBlocked.http_status(), 400);
        assert_eq!(ErrorCode::SrIgnoreListFull.code(), 900101);
        assert_eq!(
            ErrorCode::SlowmodeRateLimit.message(),
            "This action cannot be performed due to slowmode rate limit"
        );
    }

    #[test]
    fn simple_body() {
        let e = ApiError::new(ErrorCode::MissingPermissions);
        assert_eq!(e.status(), 403);
        assert_eq!(
            e.body(),
            json!({"code": 50013, "message": "Missing Permissions"})
        );
    }

    #[test]
    fn form_body_tree() {
        let e = ApiError::invalid_form()
            .field("name", "BASE_TYPE_REQUIRED", "This field is required")
            .field("embeds.0.title", "BASE_TYPE_MAX_LENGTH", "Too long");
        let b = e.body();
        assert_eq!(b["code"], 50035);
        assert_eq!(
            b["errors"]["name"]["_errors"][0]["code"],
            "BASE_TYPE_REQUIRED"
        );
        assert_eq!(
            b["errors"]["embeds"]["0"]["title"]["_errors"][0]["message"],
            "Too long"
        );
    }

    #[test]
    fn serialize_code_as_int() {
        assert_eq!(
            serde_json::to_string(&ErrorCode::UnknownUser).unwrap(),
            "10013"
        );
    }
}
