//! A noclipping player takes no damage (specs/cs_source/movement.md
//! "Noclip and spawning", measured on the probe 2026-10-17): a HE blast
//! reaches it, but neither health nor armour moves. The generic part
//! (bullets, a suicide, the kill height) is chimera_sim's
//! (chimera/sim/tests/it/noclip_damage.rs). trigger_hurt and falls after
//! noclip: tests/it/map_logic.rs.

use crate::{
    common::sim,
    stack::{
        core::Health,
        games::cs_source::grenades::HEGRENADE,
        harness::Sim,
        movement::{noclip, placeholder},
        weapon::{Armor, Inventory, Weapon, give, grenade::spawn_projectile},
    },
};
use bevy::prelude::*;

const UNIT: f32 = 0.0254;

fn health(sim: &Sim, p: Entity) -> f32 {
    sim.app.world().get::<Health>(p).unwrap().current
}

fn armour(sim: &Sim, p: Entity) -> f32 {
    sim.app.world().get::<Armor>(p).unwrap().amount
}

/// A character at `at` holding `id`, drawn.
fn holder(sim: &mut Sim, at: Vec3, id: &str) -> Entity {
    let p = sim.spawn_character(at, placeholder::ID);
    sim.ticks(1);
    give(sim.app.world_mut(), p, id).unwrap();
    sim.seconds(1.5);
    p
}

/// HE damage to a character 150 units from a grenade on the floor,
/// noclipping or not (health and armour lost).
fn blast(noclipping: bool) -> (f32, f32) {
    let mut sim = sim();
    let at = Vec3::new(0.0, 0.9, 10.0);
    let movement = if noclipping { noclip::ID } else { placeholder::ID };
    let victim = sim.spawn_character(at, movement);
    sim.app.world_mut().entity_mut(victim).insert(Armor {
        amount: 1.0,
        helmet: true,
    });
    sim.ticks(1);
    let thrower = holder(&mut sim, Vec3::new(36.0, 1.0, 36.0), HEGRENADE);
    let weapon = sim
        .app
        .world()
        .get::<Inventory>(thrower)
        .unwrap()
        .weapons
        .iter()
        .copied()
        .find(|w| sim.app.world().get::<Weapon>(*w).is_some_and(|x| x.id == HEGRENADE))
        .unwrap();
    let g = Vec3::new(0.0, 2.0 * UNIT, 10.0 - 150.0 * UNIT);
    spawn_projectile(sim.app.world_mut(), weapon, g, Vec3::ZERO).unwrap();
    sim.seconds(1.7);
    (1.0 - health(&sim, victim), 1.0 - armour(&sim, victim))
}

/// CS:S: a HE going off next to a noclipping player (its blast handed
/// on, 36 points) took nothing; walking, 37.
#[test]
fn a_noclipping_player_takes_nothing_from_a_blast() {
    let (health, armour) = blast(false);
    assert!(health > 0.1 && armour > 0.0, "walking: {health}, armour {armour}");
    assert_eq!(blast(true), (0.0, 0.0));
}
