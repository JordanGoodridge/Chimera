//! Knocked physics props settle and sleep on a real CS:S map, headless:
//! de_dust2's props, each given a bullet-like push, come to rest and go to
//! sleep within a few seconds (as in Source), rather than rocking on the
//! ground every tick. Skipped without an install.

use avian3d::prelude::*;
use bevy::prelude::*;
use crate::stack::{
    games::{
        self, cs_source,
        cs_source::movement::{SourceMovementPlugin, to_engine},
    },
    harness::{CsSim, Sim},
    map::{MapData, MapPlugin, PhysicsProp, PropIndex},
    mount::config::LocalConfig,
};

fn dust2() -> Option<MapData> {
    let installed = LocalConfig::load()
        .ok()?
        .game_path(cs_source::GAME)
        .is_some_and(|p| p.join("cstrike").is_dir());
    if !installed {
        eprintln!("skipping: no CS:S install configured");
        return None;
    }
    Some(games::load_map("cs_source:de_dust2").expect("load de_dust2"))
}

/// The dynamic physics props: (prop index, entity).
fn bodies(sim: &mut Sim) -> Vec<(usize, Entity)> {
    let world = sim.app.world_mut();
    let mut v: Vec<_> = world
        .query_filtered::<(Entity, &PropIndex, &RigidBody), With<PhysicsProp>>()
        .iter(world)
        .filter(|(_, _, rb)| rb.is_dynamic())
        .map(|(e, i, _)| (i.0, e))
        .collect();
    v.sort();
    v
}

/// A shot's push (`weapon::deliver`): an AK-47-sized bullet impulse
/// (damage 36 * 75 force scale, Source units) a little above the body's
/// centre, sideways and a bit down.
fn knock(sim: &mut Sim, body: Entity, dir: Vec3) {
    let world = sim.app.world_mut();
    let centre = world.get::<ComputedCenterOfMass>(body).map_or(Vec3::ZERO, |c| c.0);
    let at = world.get::<Transform>(body).unwrap().transform_point(centre) + Vec3::Y * 0.05;
    let impulse = to_engine(dir.normalize() * 36.0 * 75.0);
    let mut q = world.query::<Forces>();
    let mut forces = q.get_mut(world, body).unwrap();
    forces.apply_linear_impulse_at_point(impulse, at);
}

/// (linear m/s, angular rad/s, asleep)
fn motion(sim: &mut Sim, body: Entity) -> (f32, f32, bool) {
    let w = sim.app.world();
    (
        w.get::<LinearVelocity>(body).unwrap().0.length(),
        w.get::<AngularVelocity>(body).unwrap().0.length(),
        w.get::<Sleeping>(body).is_some(),
    )
}

/// Ticks until every one of `knocked` sleeps, up to `limit` seconds:
/// how long the last took (None if some are still awake) and those still
/// awake with their speeds (index, m/s, rad/s).
fn settle(sim: &mut Sim, knocked: &[(usize, Entity)], limit: f64) -> (Option<f64>, Vec<(usize, f32, f32)>) {
    let ticks = (limit / cs_source::TICK_INTERVAL).round() as u64;
    for t in 1..=ticks {
        sim.ticks(1);
        if knocked.iter().all(|(_, e)| motion(sim, *e).2) {
            return (Some(t as f64 * cs_source::TICK_INTERVAL), Vec::new());
        }
    }
    let awake = knocked
        .iter()
        .filter_map(|(i, e)| {
            let (l, a, asleep) = motion(sim, *e);
            (!asleep).then_some((*i, l, a))
        })
        .collect();
    (None, awake)
}

fn sim(map: MapData) -> Sim {
    let mut sim = Sim::new(CsSim, (MapPlugin::new(map), SourceMovementPlugin));
    sim.set_tick_interval(cs_source::TICK_INTERVAL);
    sim
}

/// de_dust2's prop 390, an oil drum standing on the crease between two
/// displacement triangles near (21, 0, -20) (engine units), which the
/// capacity test's bots knocked into rocking for good: pushed like a
/// bullet hit, from each side in turn, it sleeps within 4 s each time.
#[test]
fn a_knocked_oil_drum_settles_and_sleeps() {
    let Some(map) = dust2() else { return };
    let mut sim = sim(map);
    sim.seconds(4.0);
    let all = bodies(&mut sim);
    let &(_, body) = all.iter().find(|(i, _)| *i == 390).expect("prop 390 is a physics prop");
    for dir in [Vec3::X, Vec3::NEG_Y, Vec3::NEG_X, Vec3::Y] {
        knock(&mut sim, body, dir - Vec3::Z * 0.3);
        sim.ticks(1);
        assert!(!motion(&mut sim, body).2, "the push wakes it");
        let (took, awake) = settle(&mut sim, &[(390, body)], 4.0);
        let at = sim.app.world().get::<Transform>(body).unwrap().translation;
        eprintln!("pushed {dir}: asleep after {took:?} s at {at}");
        assert!(
            awake.is_empty(),
            "pushed {dir}, still awake 4 s later at {at}: {awake:?}"
        );
    }
}

/// Every de_dust2 physics prop, knocked like a bullet hit (all at once,
/// each a different way), sleeps within 8 s.
#[test]
fn every_knocked_dust2_prop_settles() {
    let Some(map) = dust2() else { return };
    let mut sim = sim(map);
    sim.seconds(4.0);
    let all = bodies(&mut sim);
    assert!(all.len() >= 60, "{} dynamic props", all.len());
    for (n, (_, e)) in all.iter().enumerate() {
        let a = n as f32 * 2.4;
        knock(&mut sim, *e, Vec3::new(a.cos(), a.sin(), -0.3));
    }
    sim.ticks(1);
    let (took, awake) = settle(&mut sim, &all, 8.0);
    eprintln!("{} props asleep after {took:?} s", all.len());
    assert!(
        awake.is_empty(),
        "still awake 8 s after the push (index, m/s, rad/s): {awake:?}"
    );
}
