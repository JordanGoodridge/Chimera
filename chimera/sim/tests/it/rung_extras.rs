//! The ladder's rung extras (`minigame::ladder::RungExtras`, a game's
//! `RungExtrasHook`) on the test kit, with a stand-in hook for the kit's
//! game: drawn once per ladder from the seeded generator, the same for
//! every player on a rung (its value applied to each, its melee weapon in
//! place of the kit's), again after a respawn and for a late joiner;
//! another seed draws others; the knife finale draws none; `rung_extras =
//! false` turns them off; the HUD's rung line carries the label.

use bevy::prelude::*;
use chimera_core::core::{Damage, DamageKind, Hitgroup, LocalPlayer};
use chimera_sim::{
    minigame::{
        self, MinigameAppExt, MinigamePlugin,
        auto::Rng,
        hud::MinigameHud,
        ladder::{Ladder, Rung, RungDraw, RungExtras, RungExtrasAppExt, RungExtrasHook, RungSeed},
    },
    movement::placeholder,
    testkit::{self, PISTOL, RIFLE},
    weapon::{Inventory, Weapon, data::WeaponDataAppExt},
};
use chimera_world::{greybox, harness::Sim};

const DEF: &str = r#"
id = "kit:gun_game"
name = "Kit gun game"
players = { min = 2, max = 4 }
maps = ["chimera:greybox"]
teams = "free_for_all"
time_limit = "60s"

[spawn]
respawn = { delay = "1s", protection = "0.5s" }
points = "map"
kit = ["kit:knife"]

[rules]
module = "chimera:ladder"
ladder = ["kit:pistol", "kit:rifle", "kit:shotgun"]
"#;

fn knife(id: &str) -> String {
    format!(
        "id = \"{id}\"\nbase = \"kit:base\"\nslot = 2\ndraw = 0.5\n\n[melee.primary]\nrange = 2.0\nhull = 0.5\n\
         facing_cos = 0.7\ndamage = 50.0\nhit_refire = 0.5\nmiss_refire = 0.4\nmiss_refire_other = 0.5\nforce = 0.0\n"
    )
}

/// What the stand-in hook put on a player: its rung's number.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
struct Drawn(String);

const MELEES: [&str; 3] = ["kit:knife_b", "kit:knife_c", "kit:knife_d"];

fn draw(_: &World, d: &RungDraw, rng: &mut Rng) -> Option<RungExtras> {
    let n = rng.next() % 1_000_000;
    Some(RungExtras {
        values: vec![("n".into(), n.to_string())],
        label: d.options.on("label").then(|| format!("L{n}")),
        melee: d.options.on("melee").then(|| MELEES[rng.below(MELEES.len())].to_string()),
        ..default()
    })
}

fn apply(world: &mut World, e: Entity, x: Option<&RungExtras>) {
    match x.and_then(|x| x.value("n")) {
        Some(n) => {
            world.entity_mut(e).insert(Drawn(n.to_string()));
        }
        None => {
            world.entity_mut(e).remove::<Drawn>();
        }
    }
}

const HOOK: RungExtrasHook = RungExtrasHook {
    game: "kit",
    options: &["label", "melee"],
    draw,
    apply,
};

fn sim_with(def: &str, seed: u64) -> Sim {
    let mut sim = Sim::with(chimera_sim::SimBasePlugins, |app| {
        app.add_plugins((greybox::GreyboxMapPlugin, MinigamePlugin));
        testkit::add_weapons(app);
        let docs: Vec<(String, String)> =
            ["kit:knife"].iter().chain(MELEES.iter()).map(|id| (format!("{id}.toml"), knife(id))).collect();
        let docs: Vec<(&str, &str)> = docs.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        app.add_weapon_data(&docs).expect("the knives load");
        app.add_rung_extras(HOOK);
        app.insert_resource(RungSeed(seed));
        app.add_minigames(&[("kit_gun_game.toml", def)]).expect("the definition reads");
    });
    minigame::start(sim.app.world_mut(), "kit:gun_game").expect("it starts");
    sim
}

fn player(sim: &mut Sim) -> Entity {
    sim.spawn_character(greybox::SPAWNS[0], placeholder::ID)
}

fn ladder(sim: &Sim) -> Ladder {
    sim.app.world().resource::<Ladder>().clone()
}

fn drawn(sim: &Sim, p: Entity) -> Option<String> {
    sim.app.world().get::<Drawn>(p).map(|d| d.0.clone())
}

fn carried(sim: &Sim, p: Entity) -> Vec<&'static str> {
    let w = sim.app.world();
    let inv = w.get::<Inventory>(p).unwrap();
    inv.weapons.iter().filter_map(|e| w.get::<Weapon>(*e).map(|x| x.id)).collect()
}

fn kill(sim: &mut Sim, p: Entity, by: Entity) {
    let at = sim.position(p);
    sim.app.world_mut().write_message(Damage {
        target: p,
        attacker: Some(by),
        amount: 500.0,
        point: at,
        dir: Vec3::NEG_Z,
        hitgroup: Hitgroup::Chest,
        kind: DamageKind::Bullet,
        weapon: None,
        force: Vec3::ZERO,
    });
    sim.ticks(2);
}

fn settle(sim: &mut Sim) {
    sim.seconds(1.7);
}

/// Rung `rung`'s drawn value and melee weapon.
fn rung_set(l: &Ladder, rung: usize) -> (String, &'static str) {
    let x = l.extras(rung).expect("drawn");
    let melee = MELEES.iter().find(|m| Some(**m) == x.melee.as_deref()).expect("a melee weapon");
    (x.value("n").unwrap().to_string(), melee)
}

#[test]
fn every_player_on_a_rung_gets_its_extras_again_after_a_respawn_and_when_joining_late() {
    let mut sim = sim_with(DEF, 1);
    let l = ladder(&sim);
    assert_eq!(l.extras.len(), 3, "one per rung");
    let a = player(&mut sim);
    let b = player(&mut sim);
    settle(&mut sim);
    let (n0, melee0) = rung_set(&l, 0);
    for p in [a, b] {
        assert_eq!(drawn(&sim, p).as_deref(), Some(n0.as_str()), "the rung's value on everyone");
        assert_eq!(carried(&sim, p), [melee0, PISTOL], "the rung's melee weapon in place of the kit's knife");
    }
    // A respawn on the rung: the same.
    kill(&mut sim, b, a);
    settle(&mut sim);
    assert_eq!(sim.app.world().get::<Rung>(a).map(|r| r.0), Some(1));
    let (n1, melee1) = rung_set(&l, 1);
    assert_eq!(drawn(&sim, a), Some(n1.clone()), "the next rung's, at the swap");
    let have = carried(&sim, a);
    assert!(have.contains(&melee1) && have.contains(&RIFLE), "{have:?}");
    assert!(melee0 == melee1 || !have.contains(&melee0), "the last rung's melee weapon gone: {have:?}");
    assert_eq!(drawn(&sim, b), Some(n0.clone()), "back on rung 1 after the respawn");
    assert_eq!(carried(&sim, b), [melee0, PISTOL]);
    // A late joiner on the lowest rung (b's, the first).
    let c = player(&mut sim);
    settle(&mut sim);
    assert_eq!(drawn(&sim, c), Some(n0), "the late joiner gets the rung's");
    assert_eq!(carried(&sim, c), [melee0, PISTOL]);
}

#[test]
fn another_seed_or_another_ladder_draws_other_extras() {
    let one = ladder(&sim_with(DEF, 1)).extras;
    assert_eq!(one, ladder(&sim_with(DEF, 1)).extras, "the same seed, the same draws");
    assert_ne!(one, ladder(&sim_with(DEF, 2)).extras, "another seed, others");
    // The next ladder this host makes (the next map's) draws anew.
    let mut sim = sim_with(DEF, 1);
    minigame::start(sim.app.world_mut(), "kit:gun_game").unwrap();
    assert_ne!(one, ladder(&sim).extras);
}

#[test]
fn the_knife_finale_draws_nothing_and_keeps_its_knife() {
    let def = DEF.replace("[rules]", "[rules]\nknife_finale = true");
    let mut sim = sim_with(&def, 1);
    let l = ladder(&sim);
    assert_eq!(l.rungs.last(), Some(&"kit:knife"));
    assert!(l.extras(3).is_none(), "the finale's rung draws none");
    let a = player(&mut sim);
    let b = player(&mut sim);
    settle(&mut sim);
    for _ in 0..3 {
        kill(&mut sim, b, a);
        settle(&mut sim);
    }
    assert_eq!(sim.app.world().get::<Rung>(a).map(|r| r.0), Some(3));
    assert_eq!(carried(&sim, a), ["kit:knife"], "the finale's own knife alone");
    assert_eq!(drawn(&sim, a), None, "nothing of the hook's on it");
}

#[test]
fn rung_extras_off_draws_none_and_an_option_off_leaves_its_part_out() {
    let def = DEF.replace("[rules]", "[rules]\nrung_extras = false");
    let mut sim = sim_with(&def, 1);
    assert!(ladder(&sim).extras.is_empty());
    let a = player(&mut sim);
    let _b = player(&mut sim);
    settle(&mut sim);
    assert_eq!(drawn(&sim, a), None);
    assert_eq!(carried(&sim, a), ["kit:knife", PISTOL], "the kit's knife, as without extras");

    let def = DEF.replace("[rules]", "[rules]\nrung_extras = { melee = false }");
    let mut sim = sim_with(&def, 1);
    let l = ladder(&sim);
    assert!(l.extras(0).is_some_and(|x| x.melee.is_none() && x.value("n").is_some()), "{:?}", l.extras(0));
    let a = player(&mut sim);
    let _b = player(&mut sim);
    settle(&mut sim);
    assert!(drawn(&sim, a).is_some());
    assert_eq!(carried(&sim, a), ["kit:knife", PISTOL]);
}

#[test]
fn the_huds_rung_line_carries_the_label() {
    let mut sim = sim_with(DEF, 1);
    let a = player(&mut sim);
    let _b = player(&mut sim);
    sim.app.world_mut().entity_mut(a).insert(LocalPlayer);
    settle(&mut sim);
    let n = rung_set(&ladder(&sim), 0).0;
    let lines: Vec<String> = sim.app.world().resource::<MinigameHud>().lines.iter().map(|l| l.text.clone()).collect();
    assert!(lines.iter().any(|l| l.starts_with("Rung 1/3") && l.ends_with(&format!("(L{n})"))), "{lines:?}");
}
