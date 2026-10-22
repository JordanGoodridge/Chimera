//! Scenario tests for bots' sight (`bot::think`'s first pass): the nearest living enemy in
//! sight is the target, a wall hides one, and a crowd of bots sees the same
//! whatever the executor and however many threads trace for them (the
//! traces run on the compute task pool; each bot's result is its own, so
//! nothing depends on the order the threads finish in).

use avian3d::prelude::*;
use bevy::prelude::*;
use crate::stack::{
    bot::{Bot, BotConfig},
    core::Team,
    games::cs_source::{TICK_INTERVAL, weapons::CsWeaponsPlugin},
    greybox::GreyboxMapPlugin,
    harness::{CsSim, Sim},
    movement::placeholder,
};

fn sim(multi: bool) -> Sim {
    let mut sim = Sim::new(CsSim, (GreyboxMapPlugin, CsWeaponsPlugin));
    sim.set_tick_interval(TICK_INTERVAL);
    if multi {
        sim.multi_threaded();
    }
    sim.app.world_mut().resource_mut::<BotConfig>().grenades = 0;
    sim.ticks(2);
    sim
}

fn wall(sim: &mut Sim, at: Vec3, size: Vec3) -> Entity {
    sim.app
        .world_mut()
        .spawn((Transform::from_translation(at), RigidBody::Static, Collider::cuboid(size.x, size.y, size.z)))
        .id()
}

fn character(sim: &mut Sim, at: Vec3, team: u8, bot: bool) -> Entity {
    let e = sim.spawn_character(at, placeholder::ID);
    sim.app.world_mut().entity_mut(e).insert(Team(team));
    if bot {
        sim.app.world_mut().entity_mut(e).insert(Bot::default());
    }
    e
}

fn target(sim: &Sim, bot: Entity) -> Option<Entity> {
    sim.app.world().get::<Bot>(bot).unwrap().target
}

/// The bot's target is the nearest enemy in sight: the nearer one hides
/// behind a wall, so the farther one in the open is it.
#[test]
fn the_nearest_enemy_in_sight_is_the_target() {
    let mut sim = sim(false);
    let me = character(&mut sim, Vec3::new(30.0, 0.9, 6.0), 1, true);
    // Straight ahead (-Z) 4 m, with a wall between; and 9 m to the side.
    let hidden = character(&mut sim, Vec3::new(30.0, 0.9, 2.0), 2, false);
    let open = character(&mut sim, Vec3::new(39.0, 0.9, 6.0), 2, false);
    wall(&mut sim, Vec3::new(30.0, 1.5, 4.0), Vec3::new(4.0, 3.0, 0.4));
    // A teammate in the open is nobody's target.
    let _mate = character(&mut sim, Vec3::new(28.0, 0.9, 6.0), 1, false);
    sim.ticks(3);
    assert_eq!(target(&sim, me), Some(open), "the wall hides {hidden:?}");
}

/// A wall between the only enemy and the bot: no target; once the wall is
/// gone, the enemy is it.
#[test]
fn a_wall_hides_the_enemy() {
    let mut sim = sim(false);
    let me = character(&mut sim, Vec3::new(30.0, 0.9, 6.0), 1, true);
    let enemy = character(&mut sim, Vec3::new(30.0, 0.9, 0.0), 2, false);
    let wall = wall(&mut sim, Vec3::new(30.0, 1.5, 3.0), Vec3::new(6.0, 3.0, 0.4));
    sim.ticks(3);
    assert_eq!(target(&sim, me), None);
    sim.app.world_mut().despawn(wall);
    sim.ticks(3);
    assert_eq!(target(&sim, me), Some(enemy));
}

/// Twelve bots of two teams around a pillar: what each targets and where
/// it looks after a second, by spawn order.
fn crowd(multi: bool) -> Vec<(Option<usize>, u32, u32)> {
    let mut sim = sim(multi);
    wall(&mut sim, Vec3::new(32.0, 1.5, 6.0), Vec3::new(1.0, 3.0, 6.0));
    let all: Vec<Entity> = (0..12)
        .map(|k| {
            let a = k as f32 / 12.0 * std::f32::consts::TAU;
            let at = Vec3::new(32.0 + 7.0 * a.cos(), 0.9, 6.0 + 7.0 * a.sin());
            character(&mut sim, at, 1 + (k % 2) as u8, true)
        })
        .collect();
    sim.seconds(1.0);
    all.iter()
        .map(|&e| {
            let t = target(&sim, e).map(|t| all.iter().position(|&o| o == t).unwrap());
            let i = sim.intent(e);
            (t, i.yaw.to_bits(), i.pitch.to_bits())
        })
        .collect()
}

#[test]
fn a_crowd_sees_the_same_on_any_executor() {
    let serial = crowd(false);
    assert!(serial.iter().filter(|b| b.0.is_some()).count() >= 4, "{serial:?}");
    assert_eq!(serial, crowd(false), "two runs on the same executor differ");
    assert_eq!(serial, crowd(true), "the multi-threaded executor changes what they see");
}
