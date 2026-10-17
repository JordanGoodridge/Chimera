//! A noclipping player takes no damage (specs/cs_source/movement.md
//! "Noclip and spawning", measured on the probe 2026-10-17): bullets still
//! hit it and go on through, a HE blast reaches it, but neither health nor
//! armour moves; a suicide still kills it, and below the map it flies on.
//! trigger_hurt and falls after noclip: tests/it/map_logic.rs.

use bevy::{ecs::message::Messages, prelude::*};
use crate::stack::{
    core::{Damage, DamageKind, Health, Hitgroup, Noclip},
    games::cs_source::{
        TICK_INTERVAL,
        grenades::HEGRENADE,
        weapons::{AK47, CsWeaponsPlugin},
    },
    greybox::{self, GreyboxMapPlugin},
    harness::{CsSim, Sim},
    map::KillHeight,
    movement::{noclip, placeholder},
    weapon::{Armor, Inventory, Weapon, WeaponEvent, WeaponEventKind, give, grenade::spawn_projectile},
};

const UNIT: f32 = 0.0254;

fn sim() -> Sim {
    let mut sim = Sim::new(CsSim, (GreyboxMapPlugin, CsWeaponsPlugin));
    sim.set_tick_interval(TICK_INTERVAL);
    sim
}

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

/// One AK-47 shot at the chest of a target 5 m ahead, with a second
/// (walking) one 1 m behind it on the same line: (health and armour the
/// first lost, whether the shot hit it, health the second lost).
fn shot_through(noclipping: bool) -> (f32, f32, bool, f32) {
    let mut sim = sim();
    let shooter = holder(&mut sim, greybox::SPAWNS[0], AK47);
    let first = target(&mut sim, 5.0, noclipping);
    let second = target(&mut sim, 6.0, false);
    sim.ticks(2);
    let point = sim.position(first) + Vec3::Y * (1.8 * 0.72 - 0.9);
    let eye = sim.position(shooter) + sim.state(shooter).eye_offset;
    let d = point - eye;
    {
        let mut i = sim.intent(shooter);
        i.yaw = (-d.x).atan2(-d.z);
        i.pitch = d.y.atan2(d.xz().length());
        i.fire = true;
    }
    sim.ticks(1);
    sim.intent(shooter).fire = false;
    let hit = hits_on(&sim, first) > 0;
    sim.ticks(1);
    (1.0 - health(&sim, first), 1.0 - armour(&sim, first), hit, 1.0 - health(&sim, second))
}

/// CS:S: the bullet hits the noclipping player's body (an impact on it,
/// its damage handed on), goes on through as through a walking one, and
/// takes nothing from health or armour; walking, the same shot hurts.
#[test]
fn a_noclipping_player_is_shot_but_loses_neither_health_nor_armour() {
    let (health, armour, hit, behind) = shot_through(false);
    assert!(hit && health > 0.1 && armour > 0.0, "walking: hit {hit}, lost {health} and {armour} armour");
    let (n_health, n_armour, n_hit, n_behind) = shot_through(true);
    assert!(n_hit, "the bullet still hits the noclipping body");
    assert_eq!((n_health, n_armour), (0.0, 0.0), "noclip takes nothing");
    assert!(
        (n_behind - behind).abs() < 1e-6,
        "the shot goes on through it as through a walking body: {n_behind} vs {behind}"
    );
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

/// The `kill` command (generic damage from oneself) still kills a
/// noclipping player (CS:S: death while noclipping); others' damage of
/// the same kind doesn't.
#[test]
fn a_suicide_still_kills_in_noclip() {
    let mut sim = sim();
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
    let mut sim = sim();
    let p = sim.spawn_character(Vec3::new(0.0, -50.0, 0.0), noclip::ID);
    sim.app.insert_resource(KillHeight(-10.0));
    sim.ticks(5);
    assert_eq!(health(&sim, p), 1.0);
    assert!(sim.position(p).y < -49.0, "left where it is: {}", sim.position(p));
}
