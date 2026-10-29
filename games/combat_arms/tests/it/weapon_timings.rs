//! Each install weapon fires, reloads and draws on its own view model's
//! animations (docs/backlog.md, "Combat Arms guns' own sounds, scopes and
//! models: audit", fix 3; docs/plans/active/ca-weapon-timings.md): copies
//! made from an install written here, their view models' lengths set in
//! the test (`weapons::timing::ViewTimes`), on the server and predicted;
//! ours keep their spec numbers; `FireAniRate` reaches every copy.

use std::time::Duration;

use bevy::prelude::*;
use chimera_core::core::{Health, Intent, SimClock};
use chimera_net::{
    harness::NetSim,
    net::{memory::LinkConditions, predict::NetGraph},
};
use chimera_sim::weapon::{self, Inventory, Magazine, ShellReload, Trigger, Weapon, data::WeaponDataAppExt};
use chimera_world::{greybox::GreyboxMapPlugin, harness::Sim};
use combat_arms::{
    game::{
        TICK_INTERVAL,
        attachments::CaAttachSlots,
        data::{attributes::WEAPONS_FILE, merge_weapon_docs, read_attributes},
        movement,
        weapons::timing::{AnimTimes, TIMING, ViewTimes},
    },
    harness::CaSim,
};
use toml::{Table, Value};

use super::attribute_data::meters_per_unit;

const INSTALL: &str = r#"
[Ammo0] Name="556" Type=1 MaxAmount=120
[Ammo1] Name="12ga" Type=1 MaxAmount=35
[Ammo2] Name="9mm" Type=1 MaxAmount=120
[Weapon0] Name="M4A1" Guntype=4 AmmoName0="556" ShotsPerClip=30 PVModelNormal="GUNS_M_PV_AR\PV_AR_M4A1.LTB"
[Weapon1] Name="Remington_870" Guntype=2 AmmoName0="12ga" ShotsPerClip=7 ReloadPerBullet=1 RapidFireCount=1
    PVModelNormal="GUNS_M_PV_SG\PV_SG_R870.LTB"
[Weapon2] Name="MP5A4" Guntype=3 AmmoName0="9mm" ShotsPerClip=30 PVModelNormal="GUNS_M_PV_SMG\PV_SMG_MP5.LTB"
[Weapon3] Name="Timing Rifle" Guntype=4 AmmoName0="556" ShotsPerClip=30 RapidFireCount=0 FireAniRate=0.25
    PVModelNormal="TEST\Timing\PV_Rifle.LTB"
[Weapon4] Name="Timing Shotgun" Guntype=2 AmmoName0="12ga" ShotsPerClip=7 ReloadPerBullet=1 RapidFireCount=1
    PVModelNormal="TEST\Timing\PV_Shotgun.LTB"
[Weapon5] Name="Timing SMG" Guntype=3 AmmoName0="9mm" ShotsPerClip=30 FireAniRate=0.5
    PVModelNormal="TEST\Timing\PV_SMG.LTB"
"#;

const RIFLE: &str = "combat_arms:timing_rifle";
const SHOTGUN: &str = "combat_arms:timing_shotgun";
const SMG: &str = "combat_arms:timing_smg";

/// The copies' view models, as the test writes them (s at rate 1).
fn models(app: &mut App) {
    let mut times = app.world_mut().get_resource_or_init::<ViewTimes>();
    times.set("test/timing/pv_rifle.ltb", AnimTimes {
        fire: Some(0.2),
        reload: Some(1.0),
        select: Some(0.5),
        ..default()
    });
    times.set("test/timing/pv_shotgun.ltb", AnimTimes {
        fire: Some(0.9),
        reload: Some(0.4),
        pre_reload: Some(0.25),
        post_reload: Some(0.6),
        select: Some(0.8),
    });
    // No select: the template's draw stays.
    times.set("test/timing/pv_smg.ltb", AnimTimes {
        fire: Some(0.12),
        reload: Some(1.5),
        ..default()
    });
}

fn merged() -> Vec<(String, Table)> {
    let (attrs, problems) = read_attributes(|name| match name {
        WEAPONS_FILE => Ok(INSTALL.as_bytes().to_vec()),
        _ => Ok(Vec::new()),
    });
    let weapons = attrs.weapons.as_ref().unwrap_or_else(|| panic!("{problems:?}"));
    let ours = [
        ("m4a1.toml", include_str!("../../data/weapons/m4a1.toml")),
        ("remington_870.toml", include_str!("../../data/weapons/remington_870.toml")),
        ("mp5a4.toml", include_str!("../../data/weapons/mp5a4.toml")),
    ];
    merge_weapon_docs(&ours, weapons, meters_per_unit())
        .unwrap()
        .into_iter()
        .map(|(n, t)| (n, t.parse().unwrap()))
        .collect()
}

fn doc<'a>(docs: &'a [(String, Table)], id: &str) -> &'a Table {
    &docs
        .iter()
        .find(|(_, t)| t.get("id").and_then(Value::as_str) == Some(id))
        .unwrap_or_else(|| panic!("{id}"))
        .1
}

fn hooks(t: &Table) -> &Table {
    t["hooks"].as_table().unwrap()
}

/// The copies' documents as text, to register.
fn copies() -> Vec<(String, String)> {
    let docs = merged();
    [RIFLE, SHOTGUN, SMG]
        .iter()
        .map(|id| (id.to_string(), toml::to_string(doc(&docs, id)).unwrap()))
        .collect()
}

fn register(app: &mut App, copies: &[(String, String)]) {
    models(app);
    let refs: Vec<(&str, &str)> = copies.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
    app.add_weapon_data(&refs).unwrap();
}

/// The merge writes the timing table on the copies of guns only, never
/// on ours, and `FireAniRate` on every weapon, a template without an
/// inventory table (the MP5A4) included.
#[test]
fn copies_get_a_timing_table_and_their_fire_rate() {
    let docs = merged();
    for id in [RIFLE, SHOTGUN, SMG] {
        assert!(hooks(doc(&docs, id)).contains_key(TIMING), "{id}");
    }
    for id in ["combat_arms:m4a1", "combat_arms:remington_870", "combat_arms:mp5a4"] {
        assert!(!hooks(doc(&docs, id)).contains_key(TIMING), "ours: {id}");
    }
    let rate = |id: &str| {
        hooks(doc(&docs, id))
            .get("combat_arms:inventory")
            .and_then(|t| t.get("fire_rate"))
            .and_then(Value::as_float)
    };
    assert_eq!(rate(RIFLE), Some(0.25));
    assert_eq!(rate(SMG), Some(0.5), "made where the MP5A4 has no inventory table");
    assert_eq!(rate("combat_arms:mp5a4"), Some(0.0));
}

fn sim() -> (Sim, Entity) {
    let copies = copies();
    let mut sim = Sim::with(CaSim, move |app| {
        app.add_plugins(GreyboxMapPlugin);
        register(app, &copies);
    });
    sim.set_tick_interval(TICK_INTERVAL);
    let p = sim.spawn_character(Vec3::new(30.0, 1.0, 30.0), movement::ID);
    sim.seconds(0.5);
    sim.app.world_mut().entity_mut(p).insert(Health {
        current: 1.0,
        max: 1.0,
    });
    (sim, p)
}

fn now(sim: &Sim) -> f64 {
    sim.app.world().resource::<SimClock>().now
}

/// The times the clip went down while fire was held for `seconds`.
fn shots(sim: &mut Sim, p: Entity, w: Entity, seconds: f64) -> Vec<f64> {
    let clip = |sim: &Sim| sim.app.world().get::<Magazine>(w).unwrap().clip;
    let mut out = Vec::new();
    sim.intent(p).fire = true;
    let end = now(sim) + seconds;
    let mut last = clip(sim);
    while now(sim) < end {
        sim.ticks(1);
        let c = clip(sim);
        if c < last {
            out.push(now(sim));
        }
        last = c;
    }
    sim.intent(p).fire = false;
    out
}

fn mean_gap(t: &[f64]) -> f64 {
    (t[t.len() - 1] - t[0]) / (t.len() - 1) as f64
}

/// A rifle copy fires at its own `fire_0` over 1 + `FireAniRate`
/// (0.2 / 1.25 = 160 ms), reloads in its `reload` and draws in its
/// `select`; with an attachments table these are its stock numbers.
#[test]
fn a_copy_fires_reloads_and_draws_on_its_own_animations() {
    let (mut sim, p) = sim();
    let w = weapon::give(sim.app.world_mut(), p, RIFLE).unwrap();
    {
        let world = sim.app.world();
        assert!((world.get::<Trigger>(w).unwrap().cycle - 0.16).abs() < 1e-6);
        assert_eq!(world.get::<Magazine>(w).unwrap().reload_time, 1.0);
        assert_eq!(world.get::<Weapon>(w).unwrap().draw_time, 0.5);
        let stock = world.get::<CaAttachSlots>(w).unwrap().stock;
        assert_eq!((stock.cycle, stock.reload, stock.draw), (0.2, 1.0, 0.5));
    }
    sim.seconds(1.0);
    assert_eq!(sim.app.world().get::<Inventory>(p).unwrap().active, Some(w));
    let t = shots(&mut sim, p, w, 2.0);
    assert!(t.len() >= 10, "{t:?}");
    let gap = mean_gap(&t);
    assert!((gap - 0.16).abs() < 0.01, "160 ms a round, not the M4A1's 105: {gap}");
    // The reload: its own 1 s.
    sim.seconds(0.5);
    sim.intent(p).reload = true;
    sim.ticks(1);
    sim.intent(p).reload = false;
    let start = now(&sim);
    while sim.app.world().get::<Magazine>(w).unwrap().clip < 30 && now(&sim) < start + 3.0 {
        sim.ticks(1);
    }
    let took = now(&sim) - start;
    assert!((took - 1.0).abs() < 0.05, "reloaded in {took} s");
    // Ours: the spec's numbers.
    let ours = weapon::give(sim.app.world_mut(), p, "combat_arms:m4a1").unwrap();
    let world = sim.app.world();
    assert!((world.get::<Trigger>(ours).unwrap().cycle - 0.105).abs() < 1e-6);
    assert_eq!(world.get::<Magazine>(ours).unwrap().reload_time, 2.3);
    assert_eq!(world.get::<Weapon>(ours).unwrap().draw_time, 1.2);
}

/// A shell-by-shell copy: `PreReload`, `reload` per shell, `PostReload`
/// (weapon_classes.md 5.5), and its pump is its `fire_0`.
#[test]
fn a_shell_copy_takes_its_shell_timings() {
    let (mut sim, p) = sim();
    let w = weapon::give(sim.app.world_mut(), p, SHOTGUN).unwrap();
    let world = sim.app.world();
    let s = world.get::<ShellReload>(w).unwrap();
    assert_eq!((s.start, s.insert, s.finish), (0.25, 0.4, 0.6));
    assert!((world.get::<Trigger>(w).unwrap().cycle - 0.9).abs() < 1e-6);
    assert_eq!(world.get::<Weapon>(w).unwrap().draw_time, 0.8);
}

/// A copy of a template without an inventory table takes its
/// `FireAniRate` (0.12 / 1.5 = 80 ms) and keeps the template's draw where
/// its model has no `select`.
#[test]
fn a_copy_without_an_inventory_template_keeps_its_fire_rate() {
    let (mut sim, p) = sim();
    let w = weapon::give(sim.app.world_mut(), p, SMG).unwrap();
    let world = sim.app.world();
    assert!((world.get::<Trigger>(w).unwrap().cycle - 0.08).abs() < 1e-6);
    assert_eq!(world.get::<Magazine>(w).unwrap().reload_time, 1.5);
    assert_eq!(world.get::<Weapon>(w).unwrap().draw_time, 1.0, "the MP5A4's draw");
}

/// The server and the client build the copy with the same timing, so
/// its fire is predicted without error and the server's rounds come at
/// the copy's cycle.
#[test]
fn a_copys_timing_is_the_servers_and_predicted() {
    let copies = copies();
    let link = LinkConditions {
        latency: Duration::from_millis(50),
        jitter: Duration::ZERO,
        loss: 0.0,
    };
    let mut sim = NetSim::new(CaSim, link, 11, 1, move |app| {
        app.add_plugins(GreyboxMapPlugin);
        register(app, &copies);
    });
    sim.set_frame(TICK_INTERVAL);
    sim.until_joined(600);
    sim.ticks(240);
    let theirs = sim.character_of(0).unwrap();
    let sw = weapon::give(sim.server.app.world_mut(), theirs, RIFLE).unwrap();
    sim.ticks(150);
    let me = sim.local_player(0).unwrap();
    let cw = sim.clients[0].app.world().get::<Inventory>(me).unwrap().active.unwrap();
    assert_eq!(sim.clients[0].app.world().get::<Weapon>(cw).unwrap().id, RIFLE);
    assert!((sim.clients[0].app.world().get::<Trigger>(cw).unwrap().cycle - 0.16).abs() < 1e-6);
    *sim.clients[0].app.world_mut().resource_mut::<NetGraph>() = NetGraph::default();
    let intent = |sim: &mut NetSim, fire: bool| {
        let me = sim.local_player(0).unwrap();
        sim.clients[0].app.world_mut().get_mut::<Intent>(me).unwrap().fire = fire;
    };
    let clip = |w: &World, e: Entity| w.get::<Magazine>(e).unwrap().clip;
    let before = clip(sim.server.app.world(), sw);
    intent(&mut sim, true);
    // 1.6 s held: 1.6 / 0.16 + 1 = 11 rounds at the copy's cycle (16 at
    // the M4A1's), give or take one for the frame the press lands on.
    let ticks = (1.6 / TICK_INTERVAL).round() as u64;
    sim.ticks(ticks);
    intent(&mut sim, false);
    sim.ticks(60);
    let fired = before - clip(sim.server.app.world(), sw);
    assert!((10..=12).contains(&fired), "the server fired {fired} rounds");
    let g = sim.clients[0].app.world().resource::<NetGraph>().clone();
    println!("timings: {} states checked, {} errors ({:?})", g.checked, g.errors, g.last_differing);
    assert!(g.checked > 0);
    assert_eq!(g.errors, 0, "mispredicted ({:?})", g.last_differing);
    let me = sim.local_player(0).unwrap();
    let cw = sim.clients[0].app.world().get::<Inventory>(me).unwrap().active.unwrap();
    assert_eq!(clip(sim.clients[0].app.world(), cw), clip(sim.server.app.world(), sw));
}
