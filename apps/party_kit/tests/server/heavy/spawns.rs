//! The gun game's `points = "random_hidden"` on the real maps (needs both
//! installs in chimera.local.toml, else passes with a note): de_dust2 (its
//! own navigation mesh) and Combat Arms' warehouse (a mesh generated from
//! its collision). Fifty respawns of one soldier with another standing
//! somewhere: spread over many areas rather than the two team spawns, out
//! of the other's sight, at the least distance, on spots a soldier can
//! stand on and walk from. Prints the figures (`--nocapture`).

use std::collections::BTreeSet;

use avian3d::prelude::*;
use bevy::{ecs::system::SystemState, prelude::*};
use chimera_core::{
    core::{Damage, DamageKind, Dead, Hitgroup, Intent, SOLID_LAYERS, SpawnPoint},
    mount::config::LocalConfig,
};
use chimera_sim::minigame::spawns::SpawnSpots;
use chimera_world::harness::Sim;

use crate::support::*;

fn installs() -> bool {
    let Ok(config) = LocalConfig::load() else { return false };
    let css = config
        .game_path_in(cs_source::game::GAME, cs_source::game::INFO.steam.as_ref())
        .filter(|p| p.join("cstrike").is_dir());
    let ca = config.game_path(combat_arms::game::GAME).filter(|p| p.join("Game").is_dir());
    css.is_some() && ca.is_some()
}

fn die(sim: &mut Sim, p: Entity) {
    let at = sim.position(p);
    sim.app.world_mut().write_message(Damage {
        target: p,
        attacker: None,
        amount: 5000.0,
        point: at,
        dir: Vec3::NEG_Z,
        hitgroup: Hitgroup::Chest,
        kind: DamageKind::Bullet,
        weapon: None,
        force: Vec3::ZERO,
    });
    sim.ticks(2);
    assert!(sim.app.world().get::<Dead>(p).is_some(), "dead");
}

fn blocked(sim: &mut Sim, from: Vec3, to: Vec3) -> bool {
    let w = sim.app.world_mut();
    let characters: Vec<Entity> = w.query_filtered::<Entity, With<Intent>>().iter(w).collect();
    let mut state = SystemState::<SpatialQuery>::new(w);
    let q = state.get(w).unwrap();
    let filter = SpatialQueryFilter::from_mask(SOLID_LAYERS).with_excluded_entities(characters);
    let d = to - from;
    q.cast_ray(from, Dir3::new(d).unwrap(), d.length() - 0.05, true, &filter).is_some()
}

fn cells(points: impl Iterator<Item = Vec3>) -> BTreeSet<(i32, i32)> {
    points.map(|p| ((p.x / 10.0).floor() as i32, (p.z / 10.0).floor() as i32)).collect()
}

fn play(map: &str, min_cells: usize, max_near_points: usize) {
    let Some(mut sim) = server(&["+minigame", "originals:gun_game_dust2", "+map", map, "+maxplayers", "4"]) else {
        return;
    };
    let points: Vec<Vec3> = {
        let w = sim.app.world_mut();
        w.query_filtered::<&Transform, With<SpawnPoint>>().iter(w).map(|t| t.translation).collect()
    };
    let a = soldier(&mut sim, points[0]);
    let b = soldier(&mut sim, points[1]);
    sim.seconds(2.0);
    let mut at = Vec::new();
    let (mut hidden, mut far, mut moved) = (0, 0, 0);
    for i in 0..50 {
        // The enemy moves about: another of the map's spawn points every five.
        if i % 5 == 0 {
            place(&mut sim, a, points[(i / 5 * 7) % points.len()]);
            sim.seconds(0.3);
        }
        die(&mut sim, b);
        sim.seconds(4.0);
        assert!(sim.app.world().get::<Dead>(b).is_none(), "back");
        let p = sim.position(b);
        let (ea, pa) = (sim.position(a) + Vec3::Y * 0.7, sim.position(a));
        if blocked(&mut sim, ea, p + Vec3::Y * 0.7) && blocked(&mut sim, ea, p - Vec3::Y * 0.8) {
            hidden += 1;
        }
        if p.distance(pa) >= 20.0 {
            far += 1;
        }
        // Stands where it came, and walks away (some direction a metre).
        let still = sim.position(b);
        let mut best = 0.0f32;
        for yaw in [0.0f32, 1.57, 3.14, 4.71] {
            place(&mut sim, b, still);
            sim.intent(b).yaw = yaw;
            sim.intent(b).move_axis = Vec2::new(0.0, 1.0);
            sim.seconds(0.5);
            best = best.max(sim.position(b).xz().distance(still.xz()));
        }
        sim.intent(b).move_axis = Vec2::ZERO;
        if best > 1.0 {
            moved += 1;
        }
        place(&mut sim, b, still);
        at.push(p);
    }
    let spots = sim.app.world().resource::<SpawnSpots>().clone();
    let spawn_cells = cells(points.iter().copied());
    let got = cells(at.iter().copied());
    let near_a_point = at.iter().filter(|p| points.iter().any(|s| s.xz().distance(p.xz()) < 3.0)).count();
    let (lo, hi) = spots.spots.iter().fold((Vec3::MAX, Vec3::MIN), |(l, h), s| (l.min(*s), h.max(*s)));
    let (plo, phi) = points.iter().fold((Vec3::MAX, Vec3::MIN), |(l, h), s| (l.min(*s), h.max(*s)));
    let nav_areas = sim.app.world().get_resource::<chimera_world::map::nav::NavMesh>().map_or(0, |n| n.areas.len());
    eprintln!("spots span {lo} .. {hi}; spawn points span {plo} .. {phi}; spots in {} cells; map nav areas {nav_areas}", cells(spots.spots.iter().copied()).len());
    eprintln!(
        "{map}: {} spots (nav {}), map spawn points {} in {} cells; 50 respawns in {} cells, {} near a map spawn \
         point; hidden {hidden}/50, 20 m away {far}/50, walked {moved}/50",
        spots.spots.len(),
        spots.from_nav,
        points.len(),
        spawn_cells.len(),
        got.len(),
        near_a_point
    );
    assert!(spots.from_nav && spots.spots.len() > 80);
    assert!(got.len() >= min_cells, "spread over {} cells", got.len());
    assert!(near_a_point <= max_near_points, "{near_a_point} at the team spawns");
    assert!(hidden >= 40, "{hidden}/50 hidden");
    assert!(moved >= 48, "{moved}/50 could walk");
}

#[test]
fn the_gun_game_spawns_spread_over_dust2() {
    if !installs() {
        eprintln!("skipping: needs both the CS:S and the Combat Arms installs");
        return;
    }
    play("cs_source:de_dust2", 15, 10);
}

#[test]
fn the_gun_game_spawns_spread_over_the_warehouse() {
    if !installs() {
        eprintln!("skipping: needs both the CS:S and the Combat Arms installs");
        return;
    }
    play("combat_arms:warehouse", 8, 50);
}
