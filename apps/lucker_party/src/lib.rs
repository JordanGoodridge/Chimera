//! Lucker Party (apps/lucker_party, crate `lucker_party`, binary
//! `lucker_party`; docs/plans/active/architecture-review.md L2): minigames,
//! each a data file (`minigames/`, the `originals` crate) played on
//! Chimera's mode runtime (`chimera_sim::minigame`), with both games' parts:
//! Combat Arms' soldiers, movement, HUD and guns, CS:S's maps, map logic and
//! guns, on CS:S's or Combat Arms' maps. The games are libraries composed
//! here and nowhere else (checks/tests/it/architecture.rs), as in the
//! mashup demo.
//!
//! - `PartyPlugins`: the simulation: Combat Arms' (`combat_arms::
//!   SimPlugins`, `CombatArmsPlugins`: the default game, for bare map
//!   names), then `PartyPartsPlugin`: CS:S registered as a second game,
//!   Source's map logic, CS:S's guns, the minigame runtime
//!   (`MinigamePlugin`) and the original minigames (`OriginalsPlugin`).
//!   The app's gameplay is Combat Arms' until a minigame starts
//!   (`minigame <id>`, `+minigame originals:gun_game_dust2` on the command
//!   line), which takes the rules slot and names the others.
//! - `MAP_SOURCES`: both games' map loaders with Combat Arms' gameplay
//!   dressing, CS:S's terrorist and counter-terrorist spawns as Alpha's and
//!   Bravo's, CS:S's surfaces as Combat Arms' through the mashup's table
//!   (apps/mashup_demo/data/surfaces_cs_source_combat_arms.toml, read from
//!   there: one table for both apps) and CS:S's guns' models kept.
//! - `minigame_of`, `first_map`: the minigame a command line asks for
//!   (`DEFAULT_MINIGAME` without one) and its pool's first map, which the
//!   app and the server load unless the command line names a map.
//! - With `present` (the default; not the dedicated server's,
//!   apps/lucker_party_server): `ClientPlugin` (the shell with this app's
//!   hooks and Combat Arms' HUD) and `with_defaults` (the command line as
//!   the app plays it).
//!
//! - Bots: Combat Arms' (the gun game's `bots` slot), with CS:S's guns'
//!   types for their AI rows (data/bot_gun_types.toml); `MaxPlayer`.
//!
//! Known shortcuts: maps load by id only (no downloaded map files); the
//! minigame's map pool isn't rotated (its first map plays); the bots fight
//! with whatever their ladder gives but keep the reach of their first
//! life's weapons; Combat Arms' guns show their ids as names (no item
//! table yet: BERETTA_M92FS). The HUD shows the minigame's state
//! (`chimera_sim::minigame::hud`: rung, next gun, leader, time, notices),
//! CS:S's guns' names from CS:S's strings (`weapons::display_names`) and
//! their view models by Chimera's driver with CS:S's rules
//! (`PartyPartsPlugin`), on CS:S's maps only.

use bevy::{app::PluginGroupBuilder, prelude::*};
use chimera_core::game_registry::GameAppExt;
use chimera_sim::minigame::{MinigameDef, MinigamePlugin};
use chimera_world::map::{MapData, MapLoadOptions, MapSources, SpawnTeams, SurfaceTable};

/// This build's version (`version`, bug reports): stamped by build.rs.
pub const VERSION: &str = env!("CHIMERA_VERSION");

/// This build's commit (`git describe`): stamped by build.rs.
pub const BUILD: &str = env!("CHIMERA_GIT");

/// The minigame played without `+minigame`.
pub const DEFAULT_MINIGAME: &str = originals::GUN_GAME_DUST2;

/// CS:S's spawn teams as Combat Arms': terrorists (1) Alpha, counter-
/// terrorists (2) Bravo.
pub const TEAMS: &[SpawnTeams] = &[
    SpawnTeams {
        game: cs_source::game::GAME,
        from: 1,
        to: combat_arms::game::rules::state::ALPHA,
    },
    SpawnTeams {
        game: cs_source::game::GAME,
        from: 2,
        to: combat_arms::game::rules::state::BRAVO,
    },
];

/// CS:S's surfaces as Combat Arms' surface types: the mashup's table.
pub const SURFACES: &[SurfaceTable] = &[SurfaceTable {
    map: cs_source::game::GAME,
    gameplay: combat_arms::game::GAME,
    text: include_str!("../../mashup_demo/data/surfaces_cs_source_combat_arms.toml"),
}];

/// What of CS:S's parts stays on its maps: its guns' held and view models.
pub const KEPT: &[&str] = &["cs_source:weapon_"];

/// Where this app's maps come from and whose gameplay dresses them.
pub static MAP_SOURCES: MapSources = MapSources {
    loaders: &[&cs_source::game::MAP_LOADER, &combat_arms::game::world::MAP_LOADER],
    gameplay: Some(&combat_arms::game::world::DRESSING),
    teams: TEAMS,
    surfaces: SURFACES,
    kept: KEPT,
};

/// Map `id` (`cs_source:de_dust2`, `combat_arms:warehouse`) for the party.
pub fn load_map(id: &str, hdr_level: u8) -> Result<MapData, String> {
    MAP_SOURCES.load(id, hdr_level)
}

/// Map `id` with `options` (the dedicated server's: no textures).
pub fn load_map_with(id: &str, options: &MapLoadOptions) -> Result<MapData, String> {
    MAP_SOURCES.load_with(id, options)
}

/// The simulation: Combat Arms' (the engine's base, then its parts, the
/// default game), then `PartyPartsPlugin`.
pub struct PartyPlugins;

impl PluginGroup for PartyPlugins {
    fn build(self) -> PluginGroupBuilder {
        combat_arms::game::CombatArmsPlugins
            .build()
            .add_before::<combat_arms::game::data::CaDataPlugin>(combat_arms::SimPlugins)
            .add(PartyPartsPlugin)
    }
}

/// Combat Arms' bots' gun types for CS:S's guns (data/bot_gun_types.toml).
pub const BOT_GUN_TYPES: &str = include_str!("../data/bot_gun_types.toml");

/// Bots the app keeps without a `MaxPlayer` of the user's: a gun game of
/// four with the player.
pub const DEFAULT_BOTS: u32 = 3;

/// What the party adds to Combat Arms' simulation (the headless tests run
/// it on `combat_arms::harness::CaSim`), after Combat Arms' parts: CS:S
/// registered as a second game (its install and maps), Source's map
/// logic, CS:S's guns, the minigame runtime and the original minigames;
/// Combat Arms' modes off (`ca_mode none`: its rules leave the players
/// to the minigame, a joiner comes in at a free spawn point) and its bots
/// given CS:S's guns' types (`BOT_GUN_TYPES`).
pub struct PartyPartsPlugin;

impl Plugin for PartyPartsPlugin {
    fn build(&self, app: &mut App) {
        app.register_game(&cs_source::game::INFO);
        // CS:S's guns' view models: Chimera's generic driver with CS:S's
        // rules plays them (Combat Arms' driver lets go of a character
        // holding another game's gun).
        app.insert_resource(cs_source::game::view_anim::rules());
        app.add_plugins((
            source_logic::logic::LogicPlugin,
            cs_source::game::weapons::CsGunsPlugin,
            MinigamePlugin,
            originals::OriginalsPlugin,
        ));
        if let Some(mut s) = app.world_mut().get_resource_mut::<combat_arms::game::rules::CaRuleSettings>() {
            s.mode = "none".into();
        }
        if let Some(mut bots) = app.world_mut().get_resource_mut::<combat_arms::game::bots::BotData>() {
            bots.gun_types.extend(bot_gun_types());
        }
    }
}

/// `BOT_GUN_TYPES` read: weapon id to Combat Arms' gun type.
pub fn bot_gun_types() -> Vec<(String, String)> {
    let table: toml::Table = toml::from_str(BOT_GUN_TYPES).expect("data/bot_gun_types.toml reads");
    table
        .into_iter()
        .map(|(id, kind)| (id, kind.as_str().expect("a gun type's name").to_string()))
        .collect()
}

/// The minigame a command line's console lines ask for (`minigame <id>`,
/// from `+minigame <id>`), if any.
pub fn minigame_of(console: &[String]) -> Option<String> {
    console.iter().find_map(|line| {
        let mut words = line.split_whitespace();
        let first = words.next()?;
        first.eq_ignore_ascii_case("minigame").then(|| words.next().map(|w| w.trim_matches('"').to_string()))?
    })
}

/// The first map of built-in minigame `id`'s pool.
pub fn first_map(id: &str) -> Result<String, String> {
    originals::MINIGAMES
        .iter()
        .filter_map(|(file, text)| MinigameDef::parse(file, text).ok())
        .find(|d| d.id == id)
        .map(|d| d.maps[0].clone())
        .ok_or_else(|| format!("no minigame `{id}` (the originals: {})", ids().join(", ")))
}

/// The original minigames' ids.
pub fn ids() -> Vec<String> {
    originals::MINIGAMES
        .iter()
        .filter_map(|(file, text)| MinigameDef::parse(file, text).ok())
        .map(|d| d.id)
        .collect()
}

#[cfg(feature = "present")]
mod client;
#[cfg(feature = "present")]
pub use client::{ClientPlugin, client_hooks, with_defaults};

#[cfg(test)]
mod tests {
    #[test]
    fn the_build_is_stamped() {
        assert!(!super::BUILD.is_empty() && !super::VERSION.is_empty());
    }

    #[test]
    fn a_command_line_names_its_minigame_and_its_first_map() {
        let lines = ["MaxPlayer 3".to_string(), "minigame originals:gun_game_dust2".to_string()];
        assert_eq!(super::minigame_of(&lines).as_deref(), Some("originals:gun_game_dust2"));
        assert_eq!(super::minigame_of(&lines[..1]), None);
        assert_eq!(super::first_map(super::DEFAULT_MINIGAME).unwrap(), "cs_source:de_dust2");
        assert!(super::first_map("originals:nothing").is_err());
    }

    #[test]
    fn every_cs_source_gun_has_a_bot_gun_type_combat_arms_knows() {
        let ca = combat_arms::game::bots::BotData::built_in();
        let types = super::bot_gun_types();
        for gun in cs_source::game::weapons::guns() {
            let row = types.iter().find(|(id, _)| *id == gun.id);
            let (_, kind) = row.unwrap_or_else(|| panic!("{} has no bot gun type", gun.id));
            assert!(ca.guns.contains_key(kind.as_str()), "{}: {kind} isn't one of Combat Arms' gun types", gun.id);
        }
    }

    #[test]
    fn both_games_have_loaders_and_combat_arms_dresses() {
        let s = &super::MAP_SOURCES;
        assert!(s.loader("cs_source").is_some() && s.loader("combat_arms").is_some());
        assert_eq!(s.gameplay.map(|g| g.game), Some(combat_arms::game::GAME));
    }
}
