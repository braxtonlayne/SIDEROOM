//! Threads and forum posts (LNG-THR-001..003, LNG-FRM-001): domain state, pure rules,
//! the repository surface and the service that ties them together.
//!
//! A thread is a channel of type 10/11/12 with a `parent_id` and thread metadata. Its
//! messages use the ordinary message paths; [`access`] makes permission checks on a
//! thread id inherit from the parent channel.

pub mod access;
pub mod hooks;
pub mod model;
pub mod repo;
pub mod rules;
pub mod service;

#[cfg(any(test, feature = "memory"))]
pub mod memory;
#[cfg(test)]
mod tests;

pub use model::{
    DefaultReaction, ForumConfig, ForumPatch, ForumTag, TagInput, ThreadInfo, ThreadMember,
    channel_flags,
};
pub use repo::{ActivityOutcome, ArchivedFilter, ThreadRepository};
pub use rules::{ForumPost, StartThread, ThreadPatch};
pub use service::{
    DeletedThread, MemberChange, StartedThread, ThreadListing, ThreadService, ThreadView,
};
