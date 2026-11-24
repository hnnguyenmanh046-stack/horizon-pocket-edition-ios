//! Accurate Block Placement: the logic is in `runtime.rs` and the mixins; no Hooks are needed.
pub mod runtime;

use mod_api::{self, Hooks};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {}

