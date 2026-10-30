//! The voice jukebox (MUS-JB-002, wave 31): a collaborative queue per voice channel with vote-skip, a DJ
//! hand-over and a shared playback position read off the server clock.
//!
//! * [`model`]: the session, queue items, votes and listeners as one [`model::Doc`], the wire-neutral state.
//! * [`rules`]: pure functions over a doc (settle, enqueue, vote, skip, hand-over); every rule is tested
//!   without a database.
//! * [`store`]: the persistence ports. A transaction locks the channel's session row, so concurrent skips,
//!   ends and adds are serialised and a track change happens once.
//! * [`service`]: store + clock + ids + the rules, the surface the routes call.
//! * [`memory`]: an in-memory store for tests of dependent crates.
//! * [`ports`]: who may do what in the channel, and who is told (`JUKEBOX_UPDATE`).
//!
//! The server never streams audio and never presigns here. A queue row names a mount and a key; whether a
//! listener may read them is decided per listener, at play time, by the storage scopes (`sr-api`
//! `routes::voice::jukebox`). The jb-rooms job (MUS-JB-003) builds on the same model.

pub mod memory;
pub mod model;
pub mod ports;
pub mod rules;
pub mod service;
pub mod store;

pub use model::{Doc, Item, ItemState, JukeboxError, Mode, Session};
pub use service::JukeboxService;
