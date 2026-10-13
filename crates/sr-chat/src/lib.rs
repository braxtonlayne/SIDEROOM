//! sr-chat: domain types and pure service logic for guilds, channels, roles and
//! messages (docs/04, docs/07, docs/specs/lounge-chat.md).
//!
//! The rules modules are pure functions over loaded state: no database, no clock
//! (time is passed in). [`service::ChatService`] glues them to the repository
//! traits in [`repo`]; Postgres implementations live in `sr-db`.

pub mod admin_repo;
pub mod admin_rules;
pub mod admin_service;
pub mod channel_rules;
pub mod domain;
pub mod embed_rules;
pub mod error;
pub mod guild_rules;
pub mod limits;
pub mod message;
pub mod message_hooks;
pub mod message_rules;
pub mod perms;
pub mod poll_rules;
pub mod repo;
pub mod rich_repo;
pub mod role_rules;
pub mod service;
pub mod settings;
pub mod social;

#[cfg(test)]
mod admin_tests;
#[cfg(any(test, feature = "memory"))]
pub mod memory;
#[cfg(test)]
mod service_tests;

pub use admin_repo::{Ban, GuildAdminRepository};
pub use domain::*;
pub use error::{ChatError, ChatResult, RepoError};
pub use limits::ChatLimits;
pub use service::{ChatService, ForwardRequest, SendMessage};
