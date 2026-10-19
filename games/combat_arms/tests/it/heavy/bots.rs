//! Combat Arms' bots on a real map from the user's install (WAREHOUSE):
//! the navigation areas are generated from its collision, and four bots a
//! team play Team Death Match for two minutes: they find and kill each
//! other, the kills are scored, the dead respawn, nothing panics. Passes
//! with a note without `[games.combat_arms] path` in chimera.local.toml.

use std::collections::HashSet;

use bevy::prelude::*;
use chimera_core::{core::Dead, mount::config::LocalConfig};
use chimera_world::{
    harness::Sim,
    map::{MapSimPlugin, nav::NavMesh},
};
use combat_arms::{
    game::{
        GAME, INFO,
        bots::{self, CaBot},
        rules::CaMatch,
        world,
    },
    harness::{CaSim, with_mode},
};

#[test]
fn bots_fight_on_warehouse() {
    let Some(install) = LocalConfig::load().ok().and_then(|c| c.game_path(GAME)) else {
        eprintln!("skipping: no Combat Arms install configured");
        return;
    };
    if !install.join("Game").is_dir() {
        eprintln!("skipping: no Combat Arms install at {}", install.display());
        return;
    }
    let mount = (INFO.open)(&install).unwrap();
    let map = world::load_from(&mount, "warehouse").unwrap();
    let mut sim = Sim::new(CaSim, MapSimPlugin::new(map));
    sim.ticks(2);
    with_mode(sim.app.world_mut(), "TDM");
    for team in [1, 2, 1, 2, 1, 2, 1, 2] {
        bots::add_bot(sim.app.world_mut(), Some(team)).unwrap();
    }
    let secs = 120.0;
    let mut deaths = 0;
    let mut dead_now: HashSet<Entity> = HashSet::new();
    let mut respawned = 0;
    for _ in 0..(secs / combat_arms::game::TICK_INTERVAL) as usize {
        sim.ticks(1);
        let w = sim.app.world_mut();
        let dead: HashSet<Entity> = w.query_filtered::<Entity, (With<CaBot>, With<Dead>)>().iter(w).collect();
        deaths += dead.difference(&dead_now).count();
        respawned += dead_now.difference(&dead).count();
        dead_now = dead;
    }
    let areas = sim.app.world().get_resource::<NavMesh>().map_or(0, |m| m.areas.len());
    let m = sim.app.world().resource::<CaMatch>().clone();
    eprintln!("warehouse: {areas} areas, {deaths} deaths, {respawned} respawns, score {:?}", m.score);
    assert!(areas > 50, "a navigation mesh: {areas} areas");
    assert!(deaths >= 2, "bots killed each other: {deaths}");
    assert!(m.score.iter().sum::<i32>() >= 2, "kills scored: {:?}", m.score);
    assert!(respawned >= 1, "the dead came back");
}
