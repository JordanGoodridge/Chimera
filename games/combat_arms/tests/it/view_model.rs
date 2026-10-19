//! What Combat Arms' first-person view models play (`game::view_model`,
//! Chimera's `weapon::view_anim` on the weapon data's `[anim.sequences]`),
//! on the greybox with stand-in view models named as the install's
//! (one bone; the real M92FS is heavy::view_model's): the draw, then
//! idle; fire on a round (the M92FS has no last-round sequence, view_models.md
//! 7; a test weapon maps one); reload; a shell reload's start, loop and
//! end; zoom in and out (no stock weapon has zoom sequences: the data
//! actions on a test weapon). Not yet view_models.md 7's state machine
//! (game/view_model.rs).

use std::sync::Arc;

use bevy::prelude::*;
use chimera_core::core::Health;
use chimera_sim::weapon::{self, Inventory, Magazine, data::WeaponDataAppExt};
use chimera_world::{
    greybox::GreyboxMapPlugin,
    harness::Sim,
    map::{
        MapModel, MapViewModel, ViewAnimator, ViewModels,
        anim::{AnimSet, Animation, Sequence},
    },
};
use combat_arms::{
    game::{TICK_INTERVAL, movement},
    harness::CaSim,
};

const M92FS: &str = "combat_arms:beretta_m92fs";
const R870: &str = "combat_arms:remington_870";
const ZOOMING: &str = "test:m92fs_zoom";

/// The M92FS with a scope, zoom sequences and a last-round one (no such
/// stock weapon is unkeyed).
const ZOOM_DOC: &str = r#"
id = "test:m92fs_zoom"
base = "combat_arms:beretta_m92fs"
[hooks."combat_arms:zoom"]
zoom_time = 0.13
[hooks."combat_arms:attachments"]
accepts = ["DotSight_A_Pistol"]
defaults = { sight = "DotSight_A_Pistol" }
[anim.sequences]
zoom_in = ["zoomin"]
zoom_out = ["zoomout"]
fire_last = ["fire_last"]
"#;

/// (name, seconds, looping)
type Seq = (&'static str, f32, bool);

fn view_model(key: &str, seqs: &[Seq]) -> MapViewModel {
    let defaults = vec![(Quat::IDENTITY, Vec3::ZERO)];
    let mut set = AnimSet {
        bases: vec![defaults.clone()],
        defaults,
        ..default()
    };
    for (i, (name, secs, looping)) in seqs.iter().enumerate() {
        set.animations.push(Animation {
            name: name.to_string(),
            fps: 30.0,
            frames: (secs * 30.0).round() as usize + 1,
            ..default()
        });
        set.sequences.push(Sequence {
            name: name.to_string(),
            looping: *looping,
            snap: true,
            grid: (1, 1),
            anims: vec![i],
            bone_weights: vec![1.0],
            ..default()
        });
    }
    MapViewModel {
        key: key.to_string(),
        model: MapModel::default(),
        bones: Vec::new(),
        root: Transform::default(),
        animations: Some(Arc::new(set)),
        right_handed: true,
        allow_flipping: false,
        attachments: Vec::new(),
        light_origin: Vec3::ZERO,
    }
}

const PISTOL: &[Seq] = &[
    ("idle_0", 2.0, true),
    ("select", 0.65, false),
    ("fire_0", 0.18, false),
    ("fire_last", 0.18, false),
    ("reload", 1.5, false),
    ("zoomin", 0.13, false),
    ("zoomout", 0.13, false),
];

const SHOTGUN: &[Seq] = &[
    ("idle_0", 2.0, true),
    ("select", 0.8, false),
    ("fire_0", 0.6, false),
    ("prereload", 0.4, false),
    ("reload", 0.5, false),
    ("postreload", 0.4, false),
];

fn sim() -> Sim {
    let mut sim = Sim::with(CaSim, |app| {
        app.add_plugins(GreyboxMapPlugin);
    });
    sim.set_tick_interval(TICK_INTERVAL);
    sim.app.add_weapon_data(&[("zoom", ZOOM_DOC)]).unwrap();
    sim.app.insert_resource(ViewModels(Arc::new(vec![
        view_model(M92FS, PISTOL),
        view_model(ZOOMING, PISTOL),
        view_model(R870, SHOTGUN),
    ])));
    sim
}

fn player(sim: &mut Sim) -> Entity {
    let p = sim.spawn_character(Vec3::new(30.0, 1.0, 30.0), movement::ID);
    sim.seconds(0.5);
    sim.app
        .world_mut()
        .entity_mut(p)
        .insert(Health { current: 1.0, max: 1.0 });
    p
}

fn playing(sim: &Sim, p: Entity) -> Option<String> {
    sim.app.world().get::<ViewAnimator>(p)?.sequence().map(str::to_string)
}

fn shown(sim: &Sim, p: Entity) -> Option<String> {
    sim.app.world().get::<ViewAnimator>(p)?.key.clone()
}

fn fire(sim: &mut Sim, p: Entity) {
    sim.intent(p).fire = true;
    sim.ticks(1);
    sim.intent(p).fire = false;
    sim.ticks(1);
}

/// Drawn: `select`, then `idle` once it has played (650 ms).
fn drawn(sim: &mut Sim, p: Entity, id: &str) -> Entity {
    let w = weapon::give(sim.app.world_mut(), p, id).unwrap();
    sim.ticks(3);
    assert_eq!(shown(sim, p).as_deref(), Some(id));
    assert_eq!(playing(sim, p).as_deref(), Some("select"));
    sim.seconds(1.0);
    assert_eq!(sim.app.world().get::<Inventory>(p).unwrap().active, Some(w));
    assert_eq!(playing(sim, p).as_deref(), Some("idle_0"));
    w
}

#[test]
fn pistol_draws_idles_fires_and_reloads() {
    let mut sim = sim();
    let p = player(&mut sim);
    let w = drawn(&mut sim, p, M92FS);
    fire(&mut sim, p);
    assert_eq!(playing(&sim, p).as_deref(), Some("fire_0"));
    sim.seconds(0.5);
    assert_eq!(playing(&sim, p).as_deref(), Some("idle_0"));
    // No last-round sequence in its data: `fire_0` again.
    sim.app.world_mut().get_mut::<Magazine>(w).unwrap().clip = 1;
    fire(&mut sim, p);
    assert_eq!(playing(&sim, p).as_deref(), Some("fire_0"));
    // An empty clip reloads on its own once the fire cycle ends.
    sim.seconds(0.3);
    assert_eq!(playing(&sim, p).as_deref(), Some("reload"));
    sim.seconds(2.0);
    assert_eq!(playing(&sim, p).as_deref(), Some("idle_0"));
    assert_eq!(sim.app.world().get::<Magazine>(w).unwrap().clip, 15);
    // R with a part-empty clip.
    fire(&mut sim, p);
    sim.seconds(0.5);
    sim.intent(p).reload = true;
    sim.ticks(1);
    sim.intent(p).reload = false;
    sim.ticks(1);
    assert_eq!(playing(&sim, p).as_deref(), Some("reload"));
}

#[test]
fn shotgun_reloads_shell_by_shell() {
    let mut sim = sim();
    let p = player(&mut sim);
    let w = drawn(&mut sim, p, R870);
    sim.app.world_mut().get_mut::<Magazine>(w).unwrap().clip = 5;
    sim.intent(p).reload = true;
    sim.ticks(1);
    sim.intent(p).reload = false;
    let mut seen: Vec<String> = Vec::new();
    for _ in 0..400 {
        sim.ticks(1);
        if let Some(s) = playing(&sim, p)
            && seen.last() != Some(&s)
        {
            seen.push(s);
        }
    }
    eprintln!("870 reload: {seen:?}");
    let at = |n: &str| seen.iter().position(|s| s == n);
    let (start, lp, end) = (at("prereload"), at("reload"), at("postreload"));
    assert!(start.is_some() && lp.is_some() && end.is_some(), "{seen:?}");
    assert!(start < lp && lp < end, "{seen:?}");
    assert_eq!(seen.last().map(String::as_str), Some("idle_0"));
}

#[test]
fn zoom_in_and_out_and_last_round() {
    let mut sim = sim();
    let p = player(&mut sim);
    drawn(&mut sim, p, ZOOMING);
    let press = |sim: &mut Sim| {
        sim.intent(p).secondary = true;
        sim.ticks(1);
        sim.intent(p).secondary = false;
        sim.ticks(1);
    };
    press(&mut sim);
    assert_eq!(playing(&sim, p).as_deref(), Some("zoomin"));
    sim.seconds(0.5);
    assert_eq!(playing(&sim, p).as_deref(), Some("idle_0"));
    // The zoom-out plays when the owner's zoom ends (after its transition).
    press(&mut sim);
    let mut saw = false;
    for _ in 0..60 {
        sim.ticks(1);
        saw |= playing(&sim, p).as_deref() == Some("zoomout");
    }
    assert!(saw);
    // The round that empties the clip plays `fire_last` when mapped.
    sim.seconds(1.0);
    let w = sim.app.world().get::<Inventory>(p).unwrap().active.unwrap();
    sim.app.world_mut().get_mut::<Magazine>(w).unwrap().clip = 1;
    fire(&mut sim, p);
    assert_eq!(playing(&sim, p).as_deref(), Some("fire_last"));
}
