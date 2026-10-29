//! A bomb carrier that dies and is despawned (a client leaving) before the
//! queued drop of its objective items runs: no panic, no Bevy error. The
//! bomb is not dropped (the carrier is gone with it, as with any weapon of
//! a player who leaves); the next round's `round_start` removes it.

use crate::stack::{
    core::{Damage, Died, Team},
    games::cs_source::{TICK_INTERVAL, objectives::C4, weapons::CsWeaponsPlugin},
    greybox::GreyboxMapPlugin,
    harness::{CsSim, Sim},
    movement::placeholder,
    objectives::bomb::drop_objective_items,
    weapon::give,
};
use bevy::prelude::*;

fn sim() -> Sim {
    let mut sim = Sim::new(CsSim, (GreyboxMapPlugin, CsWeaponsPlugin));
    sim.set_tick_interval(TICK_INTERVAL);
    sim.ticks(2);
    sim
}

fn died(p: Entity) -> Died {
    Died {
        entity: p,
        attacker: None,
        damage: Damage {
            target: p,
            attacker: None,
            amount: 1.0,
            point: Vec3::ZERO,
            dir: Vec3::X,
            hitgroup: Default::default(),
            kind: Default::default(),
            weapon: None,
            force: Vec3::ZERO,
        },
    }
}

#[test]
fn a_carrier_despawned_with_its_death_queued_does_not_panic() {
    let mut sim = sim();
    let p = sim.spawn_character(Vec3::new(0.0, 1.0, 12.0), placeholder::ID);
    sim.app.world_mut().entity_mut(p).insert(Team(1));
    sim.ticks(2);
    give(sim.app.world_mut(), p, C4).unwrap();
    sim.ticks(2);
    // Died is announced and the player is gone before the drop applies.
    sim.app.world_mut().write_message(died(p));
    sim.app.world_mut().despawn(p);
    sim.ticks(4);
    // Called directly on a gone entity, too (the leave and spectate paths).
    drop_objective_items(sim.app.world_mut(), p);
    assert!(sim.app.world().get_entity(p).is_err());
}
