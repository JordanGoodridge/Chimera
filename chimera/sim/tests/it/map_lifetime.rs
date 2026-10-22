//! Contract tests of a code-built map's lifetime (the greybox,
//! `chimera_world::greybox`): its spawn points exist for the simulation to
//! use, unloading removes everything it made, and spawning it again
//! restores that. The real maps' version of this is in
//! `chimera/present/tests/it/map_unload.rs`.

use bevy::prelude::*;
use chimera_core::core::{MapBrushes, SpawnPoint};
use chimera_sim::SimBasePlugins;
use chimera_world::{
    greybox::{self, GreyboxMapPlugin, GreyboxPart},
    harness::Sim,
};

fn spawn_points(sim: &mut Sim) -> Vec<Vec3> {
    let world = sim.app.world_mut();
    let mut points: Vec<Vec3> = world
        .query_filtered::<&Transform, With<SpawnPoint>>()
        .iter(world)
        .map(|t| t.translation)
        .collect();
    points.sort_by(|a, b| a.z.total_cmp(&b.z));
    points
}

#[test]
fn the_map_has_its_spawn_points() {
    let mut sim = Sim::new(SimBasePlugins, GreyboxMapPlugin);
    sim.ticks(1);
    let mut want = greybox::SPAWNS.to_vec();
    want.sort_by(|a, b| a.z.total_cmp(&b.z));
    assert_eq!(spawn_points(&mut sim), want);
    assert!(sim.app.world().contains_resource::<MapBrushes>());
}

#[test]
fn unloading_leaves_nothing_and_spawning_again_restores_it() {
    let mut sim = Sim::new(SimBasePlugins, GreyboxMapPlugin);
    sim.ticks(1);
    sim.app.world_mut().run_system_cached(greybox::unload).unwrap();
    let world = sim.app.world_mut();
    assert_eq!(world.query::<&GreyboxPart>().iter(world).count(), 0);
    assert!(!world.contains_resource::<MapBrushes>());
    assert!(spawn_points(&mut sim).is_empty());
    sim.app.world_mut().run_system_cached(greybox::respawn).unwrap();
    sim.ticks(1);
    assert_eq!(spawn_points(&mut sim).len(), greybox::SPAWNS.len());
    assert!(sim.app.world().contains_resource::<MapBrushes>());
}
