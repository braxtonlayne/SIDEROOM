//! The utility drawer's pure logic (wave 32, UTL-*).
//!
//! One `pub mod` per module of the wave; each module has one owner and the others must not edit it:
//! `search` (utl-search), `calc`, `units`, `percent` (utl-calc), `dice`, `diff`, `hash`
//! (utl-dev), `gif` and `palette` (the image agent, per the roadmap Split). Each owner also adds its own wasm-bindgen
//! exports inside its module file, so this file only ever changes to add a module.

pub mod calc;
pub mod dice;
pub mod diff;
pub mod gif;
pub mod hash;
pub mod palette;
pub mod percent;
pub mod search;
pub mod units;
