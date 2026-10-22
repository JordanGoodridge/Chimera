//! The server plays the gun game on de_dust2 from the install, as its
//! command line asks (`+minigame originals:gun_game_dust2`: the pool's
//! first map, loaded without textures and dressed for Combat Arms):
//! soldiers come in at dust2's spawn points, the match goes live, one
//! shoots another with CS:S's Glock (its aim and trigger, the server's
//! hit) and climbs to Combat Arms' Beretta. Passes with a note without
//! both installs in chimera.local.toml.

use bevy::prelude::*;
use chimera_core::{
    core::{SpawnPoint, Team},
    gameplay::Gameplay,
    mount::config::LocalConfig,
};
use chimera_sim::{
    minigame::{self, MINIGAME, ladder::Rung},
    mode::{Match, Phase},
    testkit,
};
use chimera_world::map::LoadedMapName;

use crate::support::*;

fn installs() -> bool {
    let Ok(config) = LocalConfig::load() else { return false };
    let css = config
        .game_path_in(cs_source::game::GAME, cs_source::game::INFO.steam.as_ref())
        .filter(|p| p.join("cstrike").is_dir());
    let ca = config.game_path(combat_arms::game::GAME).filter(|p| p.join("Game").is_dir());
    css.is_some() && ca.is_some()
}

#[test]
fn the_server_runs_the_gun_game_on_dust2() {
    if !installs() {
        eprintln!("skipping: needs both the CS:S and the Combat Arms installs");
        return;
    }
    let Some(mut sim) = server(&["+minigame", "originals:gun_game_dust2", "+maxplayers", "4"]) else { return };
    assert_eq!(sim.app.world().resource::<LoadedMapName>().0, "cs_source:de_dust2");
    assert_eq!(sim.app.world().resource::<Gameplay>().rules, MINIGAME);
    let spawns: Vec<Vec3> = {
        let w = sim.app.world_mut();
        w.query_filtered::<&Transform, With<SpawnPoint>>().iter(w).map(|t| t.translation).collect()
    };
    assert!(spawns.len() >= 2, "dust2's spawn points: {}", spawns.len());
    let a = soldier(&mut sim, spawns[0]);
    let b = soldier(&mut sim, spawns[1]);
    sim.seconds(1.0);
    assert_eq!(sim.app.world().resource::<Match>().phase, Phase::Live);
    for p in [a, b] {
        let at = sim.position(p);
        assert!(
            spawns.iter().any(|s| s.xz().distance(at.xz()) < 0.5),
            "{p} came in at one of dust2's spawn points ({at})"
        );
        assert!(sim.app.world().get::<Team>(p).unwrap().0 >= chimera_sim::mode::FFA_FIRST, "a team of its own");
        assert_eq!(testkit::active_id(sim.app.world(), p), Some(cs_source::game::weapons::GLOCK));
    }
    // a at the first point, b at the nearest other one (the spawns of a
    // side stand together in the open), protection over.
    sim.seconds(1.0);
    let near = spawns[1..]
        .iter()
        .copied()
        .filter(|s| s.distance(spawns[0]) > 1.5)
        .min_by(|x, y| x.distance(spawns[0]).total_cmp(&y.distance(spawns[0])))
        .expect("another spawn point");
    place(&mut sim, a, spawns[0]);
    place(&mut sim, b, near);
    sim.seconds(0.5);
    assert!(shoot_until_dead(&mut sim, a, b, 10.0), "the Glock kills on dust2");
    assert_eq!(sim.app.world().get::<Rung>(a).map(|r| r.0), Some(1));
    sim.seconds(1.5);
    assert_eq!(testkit::active_id(sim.app.world(), a), Some("combat_arms:beretta_m92fs"), "Combat Arms' pistol next");
    let status = minigame::runtime::status(sim.app.world_mut()).unwrap();
    assert!(status.contains("rung 2/10"), "{status}");
}
