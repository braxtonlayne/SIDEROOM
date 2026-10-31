//! The whole-guild mirror (BRG-SYN-001, wave 35b; design `p35b-design.md`).
//!
//! The bridge is a state reconciler, not an event replayer (design section 2.1). For every mapped object it keeps
//! the last state both sides agreed on ([`SyncedState`]); on any change from either side it compares SIDE ROOM's
//! current value, Discord's current value and that state field by field, and applies the authority rule of the
//! field ([`reconcile`]). Events only say which object to look at ([`SyncOp`]).
//!
//! Layout:
//!
//! * [`types`]: the shared vocabulary, [`SyncMode`], [`SyncState`], [`ObjectKind`], [`SyncOp`], [`SyncedState`].
//! * [`patch`]: [`SrStructuralPatch`], the only thing the Discord-to-SIDE ROOM applier accepts.
//! * [`reconcile`]: the per-field authority table and the three-way decision.
//!
//! # The authorization guarantee
//!
//! Nothing that arrives from Discord can raise a SIDE ROOM member's permissions (the IMMUTABLE RULE). The
//! Discord-to-SIDE ROOM path takes an [`SrStructuralPatch`], which has no permission word, no overwrite and no role
//! position field at all, so the compiler keeps them out. The two paths that do write permissions here are an
//! owner-confirmed plan import and an SR actor's "Apply on SIDE ROOM", both through the ordinary SIDE ROOM
//! services, never through this module.
//!
//! Permission translation (the bit masks, overwrite pushes, reverts, conflicts) is BRG-SYN-002: [] (role
//! words, Discord-only bits, import, role-deletion guard), [] (channel overwrites, SIDE ROOM to Discord,
//! trimming Discord-only targets) and [] (the conflict log and audit rows 220 to 224);
//!  runs random Discord event sequences against them. Every Discord HTTP call is BRG-SYN-003
//! and goes through [](crate::DiscordApi).

pub mod conflicts;
pub mod eligible;
pub mod engine;
pub mod holds;
pub mod jobs;
pub mod ops;
pub mod overwrites;
pub mod patch;
pub mod perms;
pub mod plan;
pub mod port;
pub mod reconcile;
pub mod reconcile_run;
pub mod store;
pub mod teardown_opts;
pub mod types;

pub use eligible::{Mirrored, Refusal, bridgeable};
pub use engine::{PlanEdit, PlanView, SeedReport, StoredPlan, SyncEngine};
pub use jobs::PassReport;
pub use patch::{ChannelPatch, RoleStructurePatch, SrStructuralPatch, ThreadPatch};
pub use plan::{Action, Blocker, DANGEROUS_BITS, ItemKind, Plan, PlanItem};
pub use port::{BridgeOrigin, NoOverwrites, OverwritePush, SrNewChannel, SrNewRole, SrStructure};
pub use reconcile::{Authority, Decision, FieldView, decide, field_authority};
pub use reconcile_run::ReconcileReport;
pub use teardown_opts::TeardownPreview;
pub use types::{
    FieldState, ObjectKind, OpDirection, OpPriority, OpState, SyncMode, SyncOp, SyncState,
    SyncedState,
};
