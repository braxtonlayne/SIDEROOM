//! Social domain: relationships, DMs and group DMs, read states, notification
//! settings, profiles and presence preferences (docs/04 section 4,
//! docs/specs/lounge-chat.md sections 2-4, 10).
//!
//! The rules modules are pure functions; [`service::SocialService`] loads state
//! through the traits in [`repo`] and persists the resulting plans. No HTTP routes
//! live here (a later wave adds them).

pub mod dm;
pub mod memory;
pub mod notifications;
pub mod presence;
pub mod profile;
pub mod read_state;
pub mod relationships;
pub mod safety;
pub mod repo;
pub mod service;
pub mod user_settings;

#[cfg(test)]
mod tests;

pub use service::SocialService;

use crate::error::RepoError;
use sr_core::{ApiError, ErrorCode};

/// Private note length (Discord parity; the table CHECK matches). Open question 111.
pub const MAX_NOTE_CHARS: usize = 256;

/// Operator-configurable social limits (docs/specs/lounge-chat.md section 14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocialLimits {
    pub max_friends: usize,
    pub max_pending_outgoing: usize,
    pub max_blocked: usize,
    /// Total members of a group DM, owner included.
    pub max_group_size: usize,
    pub max_group_dms_per_user: usize,
    pub max_nickname_chars: usize,
    pub max_group_name_chars: usize,
    pub max_bio_chars: usize,
    pub max_pronouns_chars: usize,
    pub max_custom_status_chars: usize,
    pub max_connections: usize,
}

impl Default for SocialLimits {
    fn default() -> Self {
        Self {
            max_friends: 1000,
            max_pending_outgoing: 1000,
            max_blocked: 1000,
            max_group_size: 10,
            max_group_dms_per_user: 100,
            max_nickname_chars: 32,
            max_group_name_chars: 100,
            max_bio_chars: 190,
            max_pronouns_chars: 40,
            max_custom_status_chars: 128,
            max_connections: 50,
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum SocialError {
    #[error("unknown user")]
    UnknownUser,
    #[error("unknown channel")]
    UnknownChannel,
    #[error("you cannot do that to yourself")]
    SelfTarget,
    #[error("you have blocked this user")]
    BlockedByYou,
    #[error("this user cannot be contacted")]
    BlockedByThem,
    #[error("this user does not accept friend requests from you")]
    RequestsNotAccepted,
    #[error("already friends")]
    AlreadyFriends,
    #[error("no pending request from this user")]
    NoPendingRequest,
    #[error("not friends with this user")]
    NotFriends,
    #[error("maximum number of friends reached")]
    MaxFriends,
    #[error("maximum number of pending friend requests reached")]
    MaxPending,
    #[error("maximum number of blocked users reached")]
    MaxBlocked,
    #[error("cannot send messages to this user")]
    DmNotAllowed,
    #[error("maximum number of group DM recipients reached")]
    MaxGroupSize,
    #[error("maximum number of group DMs reached")]
    MaxGroupDms,
    #[error("not a participant of this conversation")]
    NotParticipant,
    #[error("only the group owner can do that")]
    NotOwner,
    #[error("action only valid in a group DM")]
    NotGroupDm,
    #[error("unknown invite")]
    UnknownInvite,
    #[error("your settings changed in another session; try again")]
    Conflict,
    #[error("invalid {field}: {message}")]
    Invalid {
        field: String,
        code: &'static str,
        message: String,
    },
    #[error(transparent)]
    Repo(#[from] RepoError),
}

pub type SocialResult<T> = Result<T, SocialError>;

impl SocialError {
    pub fn invalid(field: &str, code: &'static str, message: impl Into<String>) -> Self {
        Self::Invalid {
            field: field.to_owned(),
            code,
            message: message.into(),
        }
    }

    pub fn to_api_error(&self) -> ApiError {
        match self {
            Self::UnknownUser => ApiError::new(ErrorCode::UnknownUser),
            Self::UnknownChannel => ApiError::new(ErrorCode::UnknownChannel),
            Self::UnknownInvite => ApiError::new(ErrorCode::UnknownInvite),
            Self::DmNotAllowed | Self::BlockedByYou | Self::BlockedByThem => {
                ApiError::new(ErrorCode::CannotSendMessagesToUser)
            }
            Self::NotParticipant => ApiError::new(ErrorCode::MissingAccess),
            Self::NotOwner => ApiError::new(ErrorCode::MissingPermissions),
            Self::MaxFriends => ApiError::new(ErrorCode::MaxFriends),
            Self::MaxGroupDms => ApiError::new(ErrorCode::MaxGroupDms),
            Self::Invalid {
                field,
                code,
                message,
            } => ApiError::invalid_form().field(field.clone(), *code, message.clone()),
            Self::Repo(_) => ApiError::new(ErrorCode::SrInternal),
            other => ApiError::with_message(ErrorCode::General, other.to_string()),
        }
    }
}

/// Trim and bound a free-text field; empty after trimming becomes `None`.
pub(crate) fn clean_text(
    field: &str,
    value: Option<&str>,
    max_chars: usize,
) -> SocialResult<Option<String>> {
    let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    if v.chars().count() > max_chars {
        return Err(SocialError::invalid(
            field,
            "BASE_TYPE_MAX_LENGTH",
            format!("must be {max_chars} or fewer in length"),
        ));
    }
    Ok(Some(v.to_owned()))
}
