//! Both games' parts in one app, on the engine's base added once
//! (docs/plans/active/architecture-review.md, A1 step 1): `CsSourceParts`
//! and `CombatArmsParts` build and tick together, and each game's guns
//! alone (`CsSourceGuns`, `CombatArmsGuns`) register both weapon sets.
//! Nothing is gated by the active game yet (A1 step 3), so every part
//! runs; this checks only that the plugins coexist.

use chimera_core::game_registry::{GameAppExt, Games};
use chimera_world::harness::Sim;

/// Both games' parts after one base, CS:S registered as a second game.
#[test]
fn both_games_parts_build_and_tick_on_one_base() {
    combat_arms::game::data::use_builtin_data_only();
    let mut sim = Sim::with(
        (chimera_sim::SimBasePlugins, chimera_core::metrics::TickMetricsPlugin),
        |app| {
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
    assert!(chimera_sim::weapon::data::lookup(combat_arms::game::inventory::STARTING_WEAPONS[0]).is_some());
}

/// Each game's weapons alone, on the base with no game registered.
#[test]
fn each_games_guns_register_without_its_rules() {
    combat_arms::game::data::use_builtin_data_only();
    let mut sim = Sim::with(chimera_sim::SimBasePlugins, |app| {
        app.add_plugins((combat_arms::CombatArmsGuns, cs_source::CsSourceGuns));
    });
    sim.ticks(2);
    assert!(
        sim.app.world().get_resource::<Games>().is_none_or(|g| g.all().next().is_none()),
        "no game registered"
    );
    for id in [cs_source::game::weapons::AK47, cs_source::game::weapons::USP] {
        assert!(chimera_sim::weapon::data::lookup(id).is_some(), "{id}");
    }
    for id in combat_arms::game::inventory::STARTING_WEAPONS {
        assert!(chimera_sim::weapon::data::lookup(id).is_some(), "{id}");
    }
}
