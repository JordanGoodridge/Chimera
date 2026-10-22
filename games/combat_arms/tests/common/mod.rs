//! What `combat_arms`' test binaries share (`mod common;` in
//! tests/it/main.rs; a change here runs all of the crate's tests,
//! `repo_checks`' `affected`): the scenario tests' usual `Sim` and readers
//! of the weapon in hand.

use bevy::prelude::*;
use chimera_sim::weapon::{Inventory, Weapon};
use chimera_world::{greybox::GreyboxMapPlugin, harness::Sim};
use combat_arms::{game::TICK_INTERVAL, harness::CaSim};

/// Combat Arms' base (`CaSim`) on the greybox at the game's tick.
#[allow(dead_code)]
pub fn sim() -> Sim {
    let mut sim = Sim::new(CaSim, GreyboxMapPlugin);
    sim.set_tick_interval(TICK_INTERVAL);
    sim
}

/// The weapon `p` holds, if any.
#[allow(dead_code)]
pub fn active(sim: &Sim, p: Entity) -> Option<Entity> {
    sim.app.world().get::<Inventory>(p)?.active
}

/// The ID of the weapon `p` holds; panics with nothing in hand.
#[allow(dead_code)]
pub fn active_id(sim: &Sim, p: Entity) -> &'static str {
    let w = active(sim, p).expect("nothing in hand");
    sim.app.world().get::<Weapon>(w).unwrap().id
}
