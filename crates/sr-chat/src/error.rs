//! Typed chat errors and their mapping to the API error taxonomy.

use sr_core::{ApiError, ErrorCode};

/// Failure inside a repository implementation.
#[derive(Debug, Clone, thiserror::Error)]
#[error("repository error: {0}")]
pub struct RepoError(pub String);

#[derive(Debug, Clone, thiserror::Error)]
pub enum ChatError {
    #[error("unknown guild")]
    UnknownGuild,
    #[error("unknown channel")]
    UnknownChannel,
    #[error("unknown message")]
    UnknownMessage,
    #[error("unknown member")]
    UnknownMember,
    #[error("unknown ban")]
    UnknownBan,
    #[error("unknown role")]
    UnknownRole,
    #[error("missing permissions")]
    MissingPermissions,
    #[error("missing access")]
    MissingAccess,
    #[error("cannot send an empty message")]
    EmptyMessage,
    #[error("cannot edit a message authored by another user")]
    NotAuthor,
    #[error("action not available in a DM channel")]
    NotInGuild,
    /// The account is in the most servers it may be in (D-659); carries the limit in force.
    #[error("maximum number of guilds reached ({0})")]
    MaxGuilds(i64),
    /// The server has the most members it may have (D-660).
    #[error("maximum number of server members reached")]
    MaxGuildMembers,
    #[error("maximum number of channels reached")]
    MaxChannels,
    #[error("maximum number of roles reached")]
    MaxRoles,
    #[error("maximum number of pins reached")]
    MaxPins,
    #[error("maximum number of reactions reached")]
    MaxReactions,
    #[error("thread is archived")]
    ThreadArchived,
    #[error("thread is locked")]
    ThreadLocked,
    #[error("a thread has already been created for this message")]
    ThreadAlreadyCreated,
    #[error("maximum number of active threads reached")]
    MaxActiveThreads,
    #[error("a tag is required to create a forum post")]
    TagRequired,
    #[error("slowmode: retry in {retry_after_ms} ms")]
    Slowmode { retry_after_ms: i64 },
    /// Invalid field; rendered as 50035 with a field path.
    #[error("invalid {field}: {message}")]
    Invalid {
        field: String,
        code: &'static str,
        message: String,
    },
    #[error(transparent)]
    Repo(#[from] RepoError),
}

impl ChatError {
    pub fn invalid(field: &str, code: &'static str, message: impl Into<String>) -> Self {
        Self::Invalid {
            field: field.to_owned(),
            code,
            message: message.into(),
        }
    }

    pub fn to_api_error(&self) -> ApiError {
        match self {
            Self::UnknownGuild => ApiError::new(ErrorCode::UnknownGuild),
            Self::UnknownChannel => ApiError::new(ErrorCode::UnknownChannel),
            Self::UnknownMessage => ApiError::new(ErrorCode::UnknownMessage),
            Self::UnknownMember => ApiError::new(ErrorCode::UnknownMember),
            Self::UnknownRole => ApiError::new(ErrorCode::UnknownRole),
            Self::UnknownBan => ApiError::new(ErrorCode::UnknownBan),
            Self::MissingPermissions => ApiError::new(ErrorCode::MissingPermissions),
            Self::MissingAccess => ApiError::new(ErrorCode::MissingAccess),
            Self::EmptyMessage => ApiError::new(ErrorCode::CannotSendEmptyMessage),
            Self::NotAuthor => ApiError::new(ErrorCode::CannotEditMessageByOther),
            Self::NotInGuild => ApiError::new(ErrorCode::CannotExecuteOnDm),
            Self::MaxGuilds(n) => ApiError::with_message(
                ErrorCode::MaxGuilds,
                format!("Maximum number of guilds reached ({n})"),
            ),
            Self::MaxGuildMembers => ApiError::new(ErrorCode::MaxGuildMembers),
            Self::MaxChannels => ApiError::new(ErrorCode::MaxChannels),
            Self::MaxRoles => ApiError::new(ErrorCode::MaxRoles),
            Self::MaxPins => ApiError::new(ErrorCode::MaxPins),
            Self::MaxReactions => ApiError::new(ErrorCode::MaxReactions),
            Self::Slowmode { .. } => ApiError::new(ErrorCode::SlowmodeRateLimit),
            Self::ThreadArchived => ApiError::new(ErrorCode::ThreadArchived),
            Self::ThreadLocked => ApiError::new(ErrorCode::ThreadLocked),
            Self::ThreadAlreadyCreated => ApiError::new(ErrorCode::ThreadAlreadyCreated),
            Self::MaxActiveThreads => ApiError::new(ErrorCode::MaxActiveThreads),
            Self::TagRequired => ApiError::new(ErrorCode::TagRequired),
            Self::Invalid {
                field,
                code,
                message,
            } => ApiError::invalid_form().field(field.clone(), *code, message.clone()),
            Self::Repo(_) => ApiError::new(ErrorCode::SrInternal),
        }
    }
}

pub type ChatResult<T> = Result<T, ChatError>;

/// Marker texts of the `RepoError` a repository answers when a server limit stops an action (D-659,
/// D-660), for repositories whose error type is `RepoError`. The number in force follows the colon.
const CAP_USER_MARKER: &str = "server limit: account is in the most servers: ";
const CAP_GUILD_MARKER: &str = "server limit: server has the most members: ";

/// The `RepoError` for a limit hit.
pub fn cap_hit_to_repo(hit: sr_core::guild_caps::CapHit) -> RepoError {
    use sr_core::guild_caps::CapHit;
    match hit {
        CapHit::UserServers(n) => RepoError(format!("{CAP_USER_MARKER}{n}")),
        CapHit::GuildMembers(n) => RepoError(format!("{CAP_GUILD_MARKER}{n}")),
    }
}

/// The limit hit a `RepoError` carries, if it is one of the markers.
pub fn cap_hit_from_repo(e: &RepoError) -> Option<sr_core::guild_caps::CapHit> {
    use sr_core::guild_caps::CapHit;
    if let Some(n) = e.0.strip_prefix(CAP_USER_MARKER) {
        return Some(CapHit::UserServers(n.parse().unwrap_or(0)));
    }
    e.0.strip_prefix(CAP_GUILD_MARKER)
        .map(|n| CapHit::GuildMembers(n.parse().unwrap_or(0)))
}

impl From<sr_core::guild_caps::CapHit> for ChatError {
    fn from(h: sr_core::guild_caps::CapHit) -> Self {
        match h {
            sr_core::guild_caps::CapHit::UserServers(n) => Self::MaxGuilds(n),
            sr_core::guild_caps::CapHit::GuildMembers(_) => Self::MaxGuildMembers,
        }
    }
}

impl ChatError {
    /// A repository error as the typed error: a limit marker becomes its case, anything else stays a repo error.
    pub fn from_repo(e: RepoError) -> Self {
        cap_hit_from_repo(&e).map_or(Self::Repo(e), Self::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_to_api_codes() {
        assert_eq!(ChatError::UnknownChannel.to_api_error().code.code(), 10003);
        assert_eq!(ChatError::MissingPermissions.to_api_error().status(), 403);
        let e = ChatError::invalid("name", "BASE_TYPE_REQUIRED", "required").to_api_error();
        assert_eq!(e.code.code(), 50035);
        assert!(e.body()["errors"]["name"].is_object());
        assert_eq!(
            ChatError::Repo(RepoError("x".into()))
                .to_api_error()
                .status(),
            500
        );
    }
}

#[cfg(test)]
mod cap_tests {
    use super::*;
    use sr_core::guild_caps::CapHit;

    /// D-659, D-660: the two limit errors answer 400 with Discord's codes; 30001 carries the number in force.
    #[test]
    fn limit_errors_map_to_30001_with_the_number_and_30019() {
        let a = ChatError::MaxGuilds(200).to_api_error();
        assert_eq!((a.status(), a.code.code()), (400, 30001));
        assert_eq!(
            a.body()["message"],
            "Maximum number of guilds reached (200)"
        );
        let a = ChatError::MaxGuildMembers.to_api_error();
        assert_eq!((a.status(), a.code.code()), (400, 30019));
        assert_eq!(
            a.body()["message"],
            "Maximum number of server members reached"
        );
    }

    /// The marker a repository without a typed error returns round-trips to the typed error.
    #[test]
    fn a_limit_marker_becomes_the_typed_error() {
        let e = ChatError::from_repo(cap_hit_to_repo(CapHit::UserServers(3)));
        assert!(matches!(e, ChatError::MaxGuilds(3)), "{e:?}");
        let e = ChatError::from_repo(cap_hit_to_repo(CapHit::GuildMembers(9)));
        assert!(matches!(e, ChatError::MaxGuildMembers), "{e:?}");
        let e = ChatError::from_repo(RepoError("boom".into()));
        assert!(matches!(e, ChatError::Repo(_)), "{e:?}");
    }
}
