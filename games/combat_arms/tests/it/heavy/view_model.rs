//! Combat Arms' view models from the user's install (`game::view_model`):
//! every weapon whose data names one loads (the unkeyed pistol, shotgun
//! and machine-gun archives: M92FS, Remington 870, M60), skinned where
//! its texture archive is unkeyed (the 870); the M92FS's, on the greybox, plays
//! its draw then idle, fire on a shot and reload on a reload, by the
//! data's `[anim.sequences]` names. Passes with a note without
//! `[games.combat_arms] path` in chimera.local.toml.

use std::sync::Arc;

use bevy::prelude::*;
use chimera_core::{core::Health, mount::config::LocalConfig};
use chimera_sim::weapon::{self, Magazine, data};
use chimera_world::{
    greybox::GreyboxMapPlugin,
    harness::Sim,
    map::{MapData, ViewAnimator, ViewModels},
};
use combat_arms::{
    game::{GAME, INFO, TICK_INTERVAL, movement, view_model},
    harness::CaSim,
};

const M92FS: &str = "combat_arms:beretta_m92fs";

fn mount() -> Option<chimera_core::mount::Mount> {
    let Some(install) = LocalConfig::load().ok().and_then(|c| c.game_path(GAME)) else {
        eprintln!("skipping: no Combat Arms install configured");
        return None;
    };
    if !install.join("Game").is_dir() {
        eprintln!("skipping: no Combat Arms install at {}", install.display());
        return None;
    }
    Some((INFO.open)(&install).unwrap())
}

/// The data's view models, loaded as a map would.
fn loaded() -> Option<(chimera_core::mount::Mount, MapData)> {
    let mount = mount()?;
    combat_arms::game::data::use_builtin_data_only();
    let file = |p: &str| mount.read(p).ok();
    let mut map = MapData::default();
    view_model::add_view_models(&mut map, &file);
    Some((mount, map))
}

#[test]
fn data_view_models_load_skinned() {
    let Some((mount, map)) = loaded() else { return };
    for w in &map.warnings {
        eprintln!("warning: {w}");
    }
    for v in &map.view_models {
        let set = v.animations.as_ref().unwrap();
        eprintln!(
            "{}: {} meshes ({} textured), {} bones, sequences {:?}",
            v.key,
            v.model.meshes.len(),
            v.model.meshes.iter().filter(|m| m.texture.is_some()).count(),
            v.bones.len(),
            set.sequences
                .iter()
                .enumerate()
                .map(|(i, s)| format!("{} {:.2}s{}", s.name, set.duration(i), loops(s.looping)))
                .collect::<Vec<_>>()
        );
        // Every action the data maps names a sequence of the model.
        let d = combat_arms::game::data::weapon_set().get(&v.key).unwrap();
        for (action, names) in &d.anim.sequences {
            assert!(
                names.iter().all(|n| set.sequence(n).is_some()),
                "{}: {action} -> {names:?} not in the model",
                v.key
            );
        }
        // Textured where the skin reads (keyed texture archives need the
        // V2 key: the pistols' and machine guns').
        let skins = d.models.view_skins.iter().filter(|s| !s.is_empty());
        let readable = skins.into_iter().all(|s| mount.read(s).is_ok());
        let textured = v.model.meshes.iter().all(|m| m.texture.is_some());
        assert_eq!(textured, readable, "{}: skins", v.key);
    }
    for id in [M92FS, "combat_arms:remington_870", "combat_arms:m60"] {
        assert!(map.view_models.iter().any(|v| v.key == id), "{id}");
    }
}

fn playing(sim: &Sim, p: Entity) -> Option<String> {
    sim.app.world().get::<ViewAnimator>(p)?.sequence().map(str::to_string)
}

fn loops(looping: bool) -> &'static str {
    if looping { " loop" } else { "" }
}

/// The sequences the M92FS's data maps `action` to.
fn names(action: &str) -> Vec<String> {
    let d = data::lookup(M92FS).unwrap();
    d.anim.sequences.get(action).cloned().unwrap_or_default()
}

fn is(sim: &Sim, p: Entity, action: &str) -> bool {
    playing(sim, p).is_some_and(|s| names(action).iter().any(|n| n.eq_ignore_ascii_case(&s)))
}

#[test]
fn m92fs_draws_idles_fires_and_reloads() {
    let Some((_, map)) = loaded() else { return };
    let m92fs = map.view_models.iter().find(|v| v.key == M92FS).unwrap().clone();
    let mut sim = Sim::with(CaSim, |app| {
        app.add_plugins(GreyboxMapPlugin);
    });
    sim.set_tick_interval(TICK_INTERVAL);
    sim.app.insert_resource(ViewModels(Arc::new(vec![m92fs])));
    let p = sim.spawn_character(Vec3::new(30.0, 1.0, 30.0), movement::ID);
    sim.seconds(0.5);
    sim.app
        .world_mut()
        .entity_mut(p)
        .insert(Health { current: 1.0, max: 1.0 });
    let w = weapon::give(sim.app.world_mut(), p, M92FS).unwrap();
    sim.ticks(3);
    assert!(is(&sim, p, "draw"), "drawing: {:?}", playing(&sim, p));
    sim.seconds(1.0);
    assert!(is(&sim, p, "idle"), "after the draw: {:?}", playing(&sim, p));
    sim.intent(p).fire = true;
    sim.ticks(1);
    sim.intent(p).fire = false;
    sim.ticks(1);
    assert!(is(&sim, p, "fire"), "after a shot: {:?}", playing(&sim, p));
    sim.seconds(1.0);
    assert!(is(&sim, p, "idle"), "after the shot: {:?}", playing(&sim, p));
    sim.intent(p).reload = true;
    sim.ticks(1);
    sim.intent(p).reload = false;
    sim.ticks(1);
    assert!(is(&sim, p, "reload"), "reloading: {:?}", playing(&sim, p));
    sim.seconds(2.5);
    assert!(is(&sim, p, "idle"), "after the reload: {:?}", playing(&sim, p));
    assert_eq!(sim.app.world().get::<Magazine>(w).unwrap().clip, 15);
}
