//! Typed errors for the settings domain and their API mapping.

use crate::error::RepoError;
use sr_core::{ApiError, ErrorCode};

#[derive(Debug, Clone, thiserror::Error)]
pub enum SettingsError {
    #[error("unknown invite")]
    UnknownInvite,
    #[error("invite expired")]
    InviteExpired,
    #[error("invite has no uses left")]
    InviteExhausted,
    #[error("invites are paused in this server")]
    InvitesPaused,
    #[error("user is banned from this guild")]
    Banned,
    #[error("maximum number of invites reached")]
    MaxInvites,
    #[error("unknown emoji")]
    UnknownEmoji,
    #[error("unknown sticker")]
    UnknownSticker,
    #[error("unknown soundboard sound")]
    UnknownSound,
    #[error("unknown webhook")]
    UnknownWebhook,
    #[error("unknown automod rule")]
    UnknownRule,
    #[error("unknown template")]
    UnknownTemplate,
    #[error("a server limit was reached: {0}")]
    LimitReached(&'static str),
    #[error("missing permissions")]
    MissingPermissions,
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

impl SettingsError {
    pub fn invalid(field: &str, code: &'static str, message: impl Into<String>) -> Self {
        Self::Invalid {
            field: field.to_owned(),
            code,
            message: message.into(),
        }
    }

    pub fn to_api_error(&self) -> ApiError {
        match self {
            Self::UnknownInvite | Self::InviteExpired | Self::InviteExhausted => {
                ApiError::new(ErrorCode::UnknownInvite)
            }
            Self::InvitesPaused => {
                ApiError::with_message(ErrorCode::FeatureDisabled, "Invites are paused")
            }
            Self::Banned => ApiError::new(ErrorCode::UserBanned),
            Self::MaxInvites => ApiError::new(ErrorCode::MaxInvites),
            Self::UnknownEmoji | Self::UnknownSticker => ApiError::new(ErrorCode::UnknownEmoji),
            Self::UnknownSound => ApiError::new(ErrorCode::UnknownSound),
            Self::UnknownWebhook => ApiError::new(ErrorCode::UnknownWebhook),
            Self::UnknownRule | Self::UnknownTemplate => ApiError::new(ErrorCode::General),
            Self::LimitReached(what) => match *what {
                "static emoji" | "animated emoji" => ApiError::new(ErrorCode::MaxEmojis),
                "stickers" => ApiError::new(ErrorCode::MaxStickers),
                "soundboard sounds" => ApiError::new(ErrorCode::MaxSounds),
                _ => ApiError::with_message(
                    ErrorCode::InvalidFormBody,
                    format!("Maximum number of {what} reached"),
                ),
            },
            Self::MissingPermissions => ApiError::new(ErrorCode::MissingPermissions),
            Self::Invalid {
                field,
                code,
                message,
            } => ApiError::invalid_form().field(field.clone(), *code, message.clone()),
            Self::Repo(_) => ApiError::new(ErrorCode::SrInternal),
        }
    }
}

pub type SettingsResult<T> = Result<T, SettingsError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_to_api_codes() {
        assert_eq!(
            SettingsError::InviteExpired.to_api_error().code.code(),
            10006
        );
        assert_eq!(SettingsError::Banned.to_api_error().status(), 403);
        assert_eq!(SettingsError::MaxInvites.to_api_error().code.code(), 30016);
        let e = SettingsError::invalid("name", "BASE_TYPE_BAD_LENGTH", "bad").to_api_error();
        assert!(e.body()["errors"]["name"].is_object());
    }
}
