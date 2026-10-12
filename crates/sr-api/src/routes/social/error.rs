//! Error mapping for the social routes. Handlers reuse the message routes' error
//! type and extractors so every endpoint answers with the same Discord shapes.

use crate::routes::messages::error::MsgError;
use sr_chat::social::SocialError;

pub use crate::routes::messages::error::{ApiJson, MsgResult, invalid, parse_id};

impl From<SocialError> for MsgError {
    fn from(e: SocialError) -> Self {
        if let SocialError::Repo(ref r) = e {
            tracing::error!(error = %r, "social repository failure");
        }
        Self::Api(e.to_api_error())
    }
}
