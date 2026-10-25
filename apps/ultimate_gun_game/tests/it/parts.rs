//! Each composed game's capability report (`chimera_sim::minigame::auto::
//! report`: CS:S's and Combat Arms' parts, checked against the app) and
//! the automatic gun game made from it: a ladder spread across both
//! games' guns, the same for the same seed, both games' maps in its pool,
//! played by the minigame runtime (each rung's gun makes its holder that
//! gun's game).

use bevy::prelude::*;
use chimera_core::slots::{GameIdentity, MovementSlot};
use chimera_sim::{
    minigame::{self, Minigames, auto, ladder::Ladder},
    testkit,
};
use chimera_world::{greybox::GreyboxMapPlugin, harness::Sim};
use combat_arms::{game::movement, harness::CaSim};
use party_kit::PartyPartsPlugin;
use ultimate_gun_game::{MINIGAME, RUNGS, SEED, UltimateGunGamePlugin};

pub(crate) fn ugg() -> Sim {
    Sim::new(CaSim, (GreyboxMapPlugin, PartyPartsPlugin, UltimateGunGamePlugin::default()))
}

/// The automatic gun game's ladder as `world` has it.
pub(crate) fn rungs(world: &World) -> Vec<String> {
    let def = world.resource::<Minigames>().get(MINIGAME).expect("the automatic gun game is made");
    def.rules.params["ladder"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

pub(crate) fn of(rungs: &[String], game: &str) -> usize {
    rungs.iter().filter(|id| id.starts_with(&format!("{game}:"))).count()
}

#[test]
fn each_game_reports_what_it_lends() {
    let sim = ugg();
    let report = auto::report(sim.app.world());
    let games: Vec<&str> = report.iter().map(|c| c.game).collect();
    assert_eq!(games, ["cs_source", "combat_arms"]);
    for c in &report {
        assert!(c.missing.is_empty(), "every claimed part is in the app: {}", c.line());
        assert!(c.playable(), "{}", c.line());
        assert!(c.knife.is_some() && c.hud.is_some() && c.characters.is_some(), "{}", c.line());
        assert!(!c.maps.is_empty(), "{}", c.line());
    }
    assert_eq!(report[0].guns.len(), cs_source_guns(), "every CS:S gun");
    assert_eq!(report[1].movement, Some(movement::ID));
}

fn cs_source_guns() -> usize {
    party_kit::GAME_PARTS[0].guns.len()
}

#[test]
fn the_ladder_spreads_across_both_games_and_keeps_its_seed() {
    let mut sim = ugg();
    let first = rungs(sim.app.world());
    assert_eq!(first.len(), RUNGS);
    assert_eq!((of(&first, "cs_source"), of(&first, "combat_arms")), (RUNGS / 2, RUNGS / 2), "{first:?}");
    let def = sim.app.world().resource::<Minigames>().get(MINIGAME).unwrap();
    assert_eq!(def.maps, ["cs_source:de_dust2", "combat_arms:warehouse", "cs_source:cs_office"], "every game's maps, in turn");
    assert_eq!(def.spawn.kit.for_game(Some("cs_source")), ["cs_source:weapon_knife"], "each gun's game's knife");
    assert_eq!(def.spawn.kit.for_game(Some("combat_arms")), ["combat_arms:m9_knife"]);
    // The same seed: the same ladder; another, another.
    ultimate_gun_game::generate(sim.app.world_mut(), SEED, RUNGS).unwrap();
    assert_eq!(rungs(sim.app.world()), first);
    ultimate_gun_game::generate(sim.app.world_mut(), SEED + 1, RUNGS).unwrap();
    assert_ne!(rungs(sim.app.world()), first);
    // `ugg_new` makes one and plays it.
    sim.app.world_mut().resource_mut::<chimera_core::console::Console>().submit("ugg_new 7 6 greybox");
    sim.ticks(2);
    assert_eq!(rungs(sim.app.world()).len(), 6);
    let active = sim.app.world().resource::<minigame::ActiveMinigame>();
    assert_eq!(active.def.id, MINIGAME);
}

#[test]
fn a_player_climbs_as_each_guns_game() {
    let mut sim = ugg();
    let ladder = rungs(sim.app.world());
    minigame::start(sim.app.world_mut(), MINIGAME).expect("the automatic gun game starts");
    let a = sim.spawn_character(Vec3::new(0.0, 1.0, 0.0), movement::ID);
    let _b = sim.spawn_character(Vec3::new(2.0, 1.0, 0.0), movement::ID);
    sim.seconds(1.5);
    let l = sim.app.world().resource::<Ladder>().clone();
    for (i, id) in ladder.iter().enumerate().take(4) {
        minigame::ladder::set_rung(sim.app.world_mut(), a, i, &l);
        sim.seconds(1.2);
        let w = sim.app.world();
        assert_eq!(testkit::active_id(w, a), Some(l.rungs[i]));
        assert_eq!(l.rungs[i], id.as_str());
        let game = id.split_once(':').unwrap().0;
        assert_eq!(w.get::<GameIdentity>(a).map(|g| g.0.as_str()), Some(game), "rung {i}: {id}");
        let want = if game == "cs_source" { "cs_source:movement" } else { movement::ID };
        assert_eq!(w.get::<MovementSlot>(a).map(|m| m.0), Some(want), "rung {i}: {id}");
    }
}
