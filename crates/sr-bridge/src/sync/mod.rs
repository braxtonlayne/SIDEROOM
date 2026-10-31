//! The whole-guild mirror (wave 35b, BRG-SYN-001 to 004).
//!
//! This file declares the modules of BRG-SYN-002 (permissions: SIDE ROOM is the master copy). sync-core's
//! reconciler, plan, seed and ops modules are declared next to these when the branches meet.
//!
//! * [`perms`]: role permission words, the Discord-only bits, the import and the role-deletion guard.
//! * [`overwrites`]: channel overwrites, SIDE ROOM to Discord, with the trim of Discord-only targets.
//! * [`conflicts`]: the conflict log (also the proposal list) and the bridge's audit rows 220 to 224.
//!
//! The guarantee (design section 5.2): nothing that arrives from Discord changes a SIDE ROOM role's permission
//! word, an overwrite's allow set or a role's position. The functions here are pure over `sr-core` types: they
//! take SIDE ROOM's state by shared reference and return what to send, or what to add here (a role with no
//! permissions, a channel with its parent's overwrites). `tests/authz_property.rs` runs random Discord event
//! sequences against them and checks that no member's computed permissions ever grow.

pub mod conflicts;
pub mod overwrites;
pub mod perms;
