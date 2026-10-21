//! Both games' parts in one app, on the engine's base added once
//! (docs/plans/active/architecture-review.md, A1 step 1): `CsSourceParts`
//! and `CombatArmsParts` build and tick together, and each game's guns
//! alone (`CsSourceGuns`, `CombatArmsGuns`) register both weapon sets;
//! each game's match-wide systems run only while it is the gameplay
//! (`chimera_core::gameplay`, A1 step 3), and a switch changes the tick.

use bevy::prelude::*;
use chimera_core::game_registry::{GameAppExt, Games};
use chimera_sim::weapon::data::WeaponCatalog;
use chimera_world::harness::Sim;

/// Both games' parts after one base, CS:S registered as a second game.
#[test]
fn both_games_parts_build_and_tick_on_one_base() {
    let mut sim = Sim::with(
        (chimera_sim::SimBasePlugins, chimera_core::metrics::TickMetricsPlugin),
        |app| {
            app.insert_resource(combat_arms::game::data::CaData::builtin());
            app.add_plugins(combat_arms::CombatArmsParts);
            // A second `RegisterGame` would replace the first's: register
            // through the app, as `MashupPartsPlugin` does.
            app.register_game(&cs_source::game::INFO);
            app.add_plugins(cs_source::CsSourceParts);
        },
    );
    sim.ticks(3);
    let ids: Vec<&str> = sim.app.world().resource::<Games>().all().map(|g| g.id).collect();
    assert_eq!(ids, [combat_arms::game::GAME, cs_source::game::GAME]);
    assert!(sim.app.world().resource::<WeaponCatalog>().get(combat_arms::game::inventory::STARTING_WEAPONS[0]).is_some());
}

/// Which games' rules ran in the next two ticks: CS:S's first rules step
/// stamps `RoundState::map_started`, Combat Arms' publishes its match
/// (`CaMatchView`) every tick.
fn rules_ran(sim: &mut Sim) -> (bool, bool) {
    let w = sim.app.world_mut();
    w.resource_mut::<cs_source::rules::rounds::RoundState>().map_started = None;
    w.insert_resource(combat_arms::game::rules::CaMatchView {
        mode: "probe".into(),
        ..Default::default()
    });
    sim.ticks(2);
    let w = sim.app.world();
    (
        w.resource::<cs_source::rules::rounds::RoundState>().map_started.is_some(),
        w.resource::<combat_arms::game::rules::CaMatchView>().mode != "probe",
    )
}

/// With both games' parts, only the gameplay's rules run (A1 step 3):
/// the first game's parts added are the gameplay (Combat Arms), and a
/// switch to CS:S runs CS:S's rules instead, at the new gameplay's tick.
#[test]
fn only_the_gameplays_rules_run_and_a_switch_changes_the_tick() {
    use chimera_core::gameplay::{Gameplay, switch_gameplay};
    let mut sim = Sim::with(
        (chimera_sim::SimBasePlugins, chimera_core::metrics::TickMetricsPlugin),
        |app| {
            app.insert_resource(combat_arms::game::data::CaData::builtin());
            app.add_plugins(combat_arms::CombatArmsParts);
            app.register_game(&cs_source::game::INFO);
            app.add_plugins(cs_source::CsSourceParts);
        },
    );
    sim.ticks(2);
    let gameplay = sim.app.world().resource::<Gameplay>().clone();
    assert_eq!(gameplay, Gameplay::of(&combat_arms::game::INFO), "the first game's parts play");
    assert_eq!(rules_ran(&mut sim), (false, true), "Combat Arms' rules only");

    let tick = std::time::Duration::from_secs_f64(1.0 / 128.0);
    switch_gameplay(
        sim.app.world_mut(),
        Gameplay {
            tick,
            ..Gameplay::of(&cs_source::game::INFO)
        },
    );
    assert_eq!(sim.app.world().resource::<Time<Fixed>>().timestep(), tick);
    assert_eq!(rules_ran(&mut sim), (true, false), "CS:S's rules only");

    switch_gameplay(sim.app.world_mut(), gameplay);
    assert_eq!(rules_ran(&mut sim), (false, true), "Combat Arms' again");
}

/// Each game's weapons alone, on the base with no game registered.
#[test]
fn each_games_guns_register_without_its_rules() {
    let mut sim = Sim::with(chimera_sim::SimBasePlugins, |app| {
        app.insert_resource(combat_arms::game::data::CaData::builtin());
        app.add_plugins((combat_arms::CombatArmsGuns, cs_source::CsSourceGuns));
    });
    sim.ticks(2);
    assert!(
        sim.app.world().get_resource::<Games>().is_none_or(|g| g.all().next().is_none()),
        "no game registered"
    );
    for id in [cs_source::game::weapons::AK47, cs_source::game::weapons::USP] {
        assert!(sim.app.world().resource::<WeaponCatalog>().get(id).is_some(), "{id}");
    }
    for id in combat_arms::game::inventory::STARTING_WEAPONS {
        assert!(sim.app.world().resource::<WeaponCatalog>().get(id).is_some(), "{id}");
    }
}
