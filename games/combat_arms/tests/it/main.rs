//! `combat_arms`' integration tests, as one test binary (each file a
//! module): scenario tests of Combat Arms' simulation on the engine's
//! headless harness (`chimera_world::harness::Sim`) with its base
//! (`combat_arms::harness::CaSim`) and the greybox, with nothing above
//! the game. `cargo test -p combat_arms --features dev --test it`; where
//! tests go: docs/OBSERVABILITY.md, section 1.
//!
//! Wave 0 checks the skeleton: a player spawns, walks, fires the
//! placeholder rifle and switches to the placeholder knife. Spec test
//! cases (specs/combat_arms/) get their own files here as each subsystem
//! is implemented.

mod attribute_data;
mod game_modes;
mod grenades;
mod inventory;
mod model;
mod movement;
mod net;
mod net_modes;
mod skeleton;
mod weapon_classes;
mod weapon_firing;
mod world;

// Slow ones (the user's install): the fast tier skips `heavy::`.
mod heavy {
    mod attributes;
    mod model;
    mod mount;
    mod world;
}
