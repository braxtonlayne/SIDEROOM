//! sr-moderation: the Phase 7 moderation backend (`MOD-*`, docs/specs/moderation-compliance.md).
//!
//! Step 1 (D-315): reports from members, cases for staff, holds, appeals.
//!
//! * [`Moderation`] is the handle over the database. Every call that changes something takes the actor as
//!   [`model::Who`] (platform staff with their bits, or a server moderator for one server) and checks what
//!   they may do with [`rules::access`]; platform staff changes also write the hash-chained access log in the
//!   same transaction.
//! * [`holds`] is the one hold implementation. The takedown register (step 2) and the data-request queue
//!   (step 3) call [`holds::place_hold_in`] / [`holds::release_hold_in`] inside their own transactions, or
//!   [`Moderation::place_hold`] / [`Moderation::release_hold`] on their own.
//! * [`rules`] holds everything that needs no database: routing (D-209), access, lifecycle, reviewer rule.

mod actions;
mod appeals;
pub mod blocklist;
mod cases;
pub mod error;
pub mod holds;
pub mod model;
mod reports;
pub mod rules;
pub mod service;
pub mod snapshot;
pub mod takedown_rules;
mod takedowns;

pub use actions::{ActionError, CaseActions, PerformRequest, PerformedAction};
pub use cases::{RECORDABLE_ACTIONS, SelfDeleteNote, Transition};
pub use error::{ModError, ModResult};
pub use holds::{HoldOutcome, HoldRequest, HoldScope, ReleaseOutcome};
pub use service::{
    GuildAudit, GuildAuditKind, GuildAuditRow, IdSource, Moderation, PolicySource, RemovalSink,
    SubjectNotifier,
};
pub use takedowns::register_csv;
