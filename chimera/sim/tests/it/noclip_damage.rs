//! A noclipping player takes no damage (the engine's `core::Noclip`):
//! bullets still hit it and go on through, but neither health nor armour
//! moves; a suicide still kills it, and below the map it flies on. Moved
//! down from `cs_source`'s tests (which keep the HE blast, a CS:S grenade),
//! on the test kit's rifle.

use bevy::{ecs::message::Messages, prelude::*};
use chimera_core::core::{Damage, DamageKind, Health, Hitgroup, Noclip};
use chimera_sim::{
    SimBasePlugins,
    movement::{noclip, placeholder},
    testkit::{self, RIFLE},
    weapon::{Armor, WeaponEvent, WeaponEventKind},
};
use chimera_world::{
    greybox::{self, GreyboxMapPlugin},
    harness::Sim,
    map::KillHeight,
};

fn health(sim: &Sim, p: Entity) -> f32 {
    sim.app.world().get::<Health>(p).unwrap().current
}

fn armour(sim: &Sim, p: Entity) -> f32 {
    sim.app.world().get::<Armor>(p).unwrap().amount
}

/// A target `meters` ahead of the greybox spawn with full health and
/// armour, walking or noclipping.
fn target(sim: &mut Sim, meters: f32, noclipping: bool) -> Entity {
    let movement = if noclipping { noclip::ID } else { placeholder::ID };
    let t = sim.spawn_character(greybox::SPAWNS[0] - Vec3::Z * meters, movement);
    sim.app.world_mut().entity_mut(t).insert(Armor {
        amount: 1.0,
        helmet: true,
    });
    t
}

fn hits_on(sim: &Sim, who: Entity) -> usize {
    let messages = sim.app.world().resource::<Messages<WeaponEvent>>();
    let mut cursor = messages.get_cursor();
    cursor
        .read(messages)
        .filter(|e| matches!(e.kind, WeaponEventKind::Hit { target, .. } if target == who))
        .count()
}

/// One rifle shot at the chest of a target 5 m ahead, with a second
/// (walking) one 1 m behind it on the same line: (health and armour the
/// first lost, whether the shot hit it, health the second lost).
fn shot_through(noclipping: bool) -> (f32, f32, bool, f32) {
    let mut sim = testkit::kit_sim();
    let shooter = testkit::with_player(&mut sim, None, RIFLE);
    let first = target(&mut sim, 5.0, noclipping);
    let second = target(&mut sim, 6.0, false);
    sim.ticks(2);
    let point = sim.position(first) + Vec3::Y * (1.8 * 0.72 - 0.9);
    testkit::aim_at(&mut sim, shooter, point);
    sim.intent(shooter).fire = true;
    sim.ticks(1);
    sim.intent(shooter).fire = false;
    let hit = hits_on(&sim, first) > 0;
    sim.ticks(1);
    (
        1.0 - health(&sim, first),
        1.0 - armour(&sim, first),
        hit,
        1.0 - health(&sim, second),
    )
}

/// The bullet hits the noclipping player's body (an impact on it, its
/// damage handed on), goes on through as through a walking one, and takes
/// nothing from health or armour; walking, the same shot hurts.
#[test]
fn a_noclipping_player_is_shot_but_loses_neither_health_nor_armour() {
    let (health, armour, hit, behind) = shot_through(false);
    assert!(
        hit && health > 0.01 && armour > 0.0,
        "walking: hit {hit}, lost {health} and {armour} armour"
    );
    let (n_health, n_armour, n_hit, n_behind) = shot_through(true);
    assert!(n_hit, "the bullet still hits the noclipping body");
    assert_eq!((n_health, n_armour), (0.0, 0.0), "noclip takes nothing");
    assert!(
        (n_behind - behind).abs() < 1e-6,
        "the shot goes on through it as through a walking body: {n_behind} vs {behind}"
    );
}

/// The `kill` command (generic damage from oneself) still kills a
/// noclipping player; others' damage of the same kind doesn't.
#[test]
fn a_suicide_still_kills_in_noclip() {
    let mut sim = Sim::new(SimBasePlugins, GreyboxMapPlugin);
    let p = sim.spawn_character(Vec3::new(0.0, 5.0, 0.0), noclip::ID);
    let other = sim.spawn_character(Vec3::new(3.0, 1.0, 0.0), placeholder::ID);
    sim.ticks(1);
    assert!(sim.app.world().get::<Noclip>(p).is_some());
    let damage = |attacker| Damage {
        force: Vec3::ZERO,
        target: p,
        attacker: Some(attacker),
        amount: 10.0,
        point: Vec3::ZERO,
        dir: Vec3::NEG_Y,
        hitgroup: Hitgroup::Generic,
        kind: DamageKind::Generic,
        weapon: None,
    };
    sim.app.world_mut().write_message(damage(other));
    sim.ticks(1);
    assert_eq!(health(&sim, p), 1.0, "someone else's");
    sim.app.world_mut().write_message(damage(p));
    sim.ticks(1);
    assert_eq!(health(&sim, p), 0.0, "a suicide");
}

/// Below the map's kill height a noclipping player flies on (falling out
/// kills a walking one).
#[test]
fn noclip_flies_below_the_map() {
    let mut sim = Sim::new(SimBasePlugins, GreyboxMapPlugin);
    let p = sim.spawn_character(Vec3::new(0.0, -50.0, 0.0), noclip::ID);
    sim.app.insert_resource(KillHeight(-10.0));
    sim.ticks(5);
    assert_eq!(health(&sim, p), 1.0);
    assert!(sim.position(p).y < -49.0, "left where it is: {}", sim.position(p));
}
