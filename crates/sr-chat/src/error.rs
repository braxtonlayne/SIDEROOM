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
    #[error("maximum number of channels reached")]
    MaxChannels,
    #[error("maximum number of roles reached")]
    MaxRoles,
    #[error("maximum number of pins reached")]
    MaxPins,
    #[error("maximum number of reactions reached")]
    MaxReactions,
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
            Self::MaxChannels => ApiError::new(ErrorCode::MaxChannels),
            Self::MaxRoles => ApiError::new(ErrorCode::MaxRoles),
            Self::MaxPins => ApiError::new(ErrorCode::MaxPins),
            Self::MaxReactions => ApiError::new(ErrorCode::MaxReactions),
            Self::Slowmode { .. } => ApiError::with_message(
                ErrorCode::TooManyRequests,
                "Slowmode is enabled in this channel",
            ),
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
