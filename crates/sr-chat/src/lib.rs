//! sr-chat: domain types and pure service logic for guilds, channels, roles and
//! messages (docs/04, docs/07, docs/specs/lounge-chat.md).
//!
//! The rules modules are pure functions over loaded state: no database, no clock
//! (time is passed in). [`service::ChatService`] glues them to the repository
//! traits in [`repo`]; Postgres implementations live in `sr-db`.

pub mod channel_rules;
pub mod domain;
pub mod error;
pub mod guild_rules;
pub mod limits;
pub mod message_rules;
pub mod perms;
pub mod repo;
pub mod role_rules;
pub mod service;
pub mod social;
pub mod settings;

#[cfg(test)]
mod memory;
#[cfg(test)]
mod service_tests;

pub use domain::*;
pub use error::{ChatError, ChatResult, RepoError};
pub use limits::ChatLimits;
pub use service::{ChatService, SendMessage};
