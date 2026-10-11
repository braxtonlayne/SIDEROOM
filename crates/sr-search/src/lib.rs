//! sr-search: Tantivy indexes (docs/02 section 3, docs/specs/lounge-chat.md section 9).
//!
//! * [`MessageIndex`]: the message index (LNG-SRC-001/002/003). Documents are
//!   built from [`MessageEvent`]s; deleted messages are removed, so they are
//!   never searchable. Permission filtering happens at query time: the caller
//!   passes the set of channels the searcher may read ([`Access`]) and nothing
//!   outside it can be returned.
//! * [`query`]: parser for Discord-style filters (`from:`, `mentions:`, `has:`,
//!   `before:`, `during:`, `after:`, `in:`, `pinned:`, `authorType:`). Filter
//!   values are ids; resolving names to ids is the API layer's job.
//! * [`MemberIndex`]: prefix search over guild members.
//! * [`ViewableChannelCache`]: per-(user, guild) allow-list cache (60 s TTL) and
//!   [`apply_nsfw_gate`] for NSFW channels.

pub mod access;
pub mod error;
pub mod members;
pub mod messages;
pub mod query;

pub use access::{ACCESS_TTL, ViewableChannelCache, apply_nsfw_gate};
pub use error::{SearchError, SearchResult};
pub use members::{IndexedMember, MemberHit, MemberIndex, MemberQuery};
pub use messages::{
    Access, AuthorType, HasKind, Hit, IndexedMessage, MessageEvent, MessageIndex, Page,
    SearchRequest, Snippet, Sort,
};
pub use query::{ParsedQuery, QueryError, parse_query};
