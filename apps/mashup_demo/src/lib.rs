//! The first mashup app (apps/mashup_demo, crate `mashup_demo`, binary
//! `mashup_demo`): Counter-Strike: Source's maps played as Combat Arms
//! (docs/DESIGN.md, "Goal": a Combat Arms character on a CS:S map). Two
//! games as libraries, composed here and nowhere else: CS:S gives the map
//! (`cs_source::game::MAP_LOADER`: the world, its collision, entities,
//! sounds, spawn points), Combat Arms everything played on it (its movement, the
//! M4A1, knife and grenades with their view models, its soldiers' bodies,
//! its HUD, its bots on navigation generated from the map's collision,
//! Team Death Match). Neither game knows the other
//! (checks/tests/it/architecture.rs).
//!
//! - `MAP_SOURCES`: Chimera's `map::MapSources` with both games' loaders
//!   and Combat Arms as the gameplay: a CS:S map loses CS:S's parts
//!   (bodies, view models, HUD, radio, navigation, round sounds) and gets
//!   Combat Arms' (`combat_arms::game::world::DRESSING`); its terrorist
//!   spawns are Alpha's, its counter-terrorist spawns Bravo's (`TEAMS`).
//!   Combat Arms' own maps load as in its app.
//! - Units: every loader gives metres (CS:S's inch, Combat Arms'
//!   `meters_per_unit`), and each game's numbers stay in its own unit, so
//!   the soldier is Combat Arms' size on de_dust2 with nothing rescaled
//!   (`map::sources`).
//! - `MashupPlugins`: Combat Arms' simulation (`combat_arms::SimPlugins`,
//!   `CombatArmsPlugins`: the default game, so its tick, binds and HUD) and
//!   CS:S registered after it for its install (`map` lists the default
//!   game's maps; `map cs_source:<name>` loads a CS:S one).
//! - `ClientPlugin`: the shell with this app's hooks (`client_hooks`) and
//!   Combat Arms' HUD.
//!
//! Known shortcuts: maps load by id only (no downloaded map files, no map
//! offer to network clients); CS:S's map logic (`source_logic`: doors,
//! buttons, breakables, triggers) isn't run, so brush entities stand as
//! placed; CS:S's surfaces are unknown to Combat Arms' effects (its
//! default impacts and footsteps); CS:S's weapons aren't offered (their
//! data and hooks are CS:S's simulation, which this app doesn't run).

use bevy::{app::PluginGroupBuilder, prelude::*};
use chimera_client::client::{AppHooks, Args, ClientPlugins};
use chimera_core::game_registry::RegisterGame;
use chimera_world::{
    greybox,
    map::{MapData, MapSources, SpawnTeams},
};

/// This build's version (`version` command, bug reports); stamped by
/// build.rs.
pub const VERSION: &str = env!("CHIMERA_VERSION");

/// This build's commit (`git describe`: client hooks, bug reports);
/// stamped by build.rs.
pub const BUILD: &str = env!("CHIMERA_GIT");

/// The map the app starts on without `--map`.
pub const DEFAULT_MAP: &str = "cs_source:de_dust2";

/// Bots the app keeps without a `MaxPlayer` of the user's (Combat Arms'
/// Bot Deathmatch quota): two a side with the player.
pub const DEFAULT_BOTS: u32 = 3;

/// CS:S's spawn teams as Combat Arms': terrorists (1) Alpha, counter-
/// terrorists (2) Bravo (`combat_arms::game::rules::state`).
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

/// Where this app's maps come from and whose gameplay they get.
pub static MAP_SOURCES: MapSources = MapSources {
    loaders: &[&cs_source::game::MAP_LOADER, &combat_arms::game::world::MAP_LOADER],
    gameplay: Some(&combat_arms::game::world::DRESSING),
    teams: TEAMS,
};

/// Map `id` (`cs_source:de_dust2`, `combat_arms:warehouse`; a bare name is
/// CS:S's) for Combat Arms' gameplay.
pub fn load_map(id: &str, hdr_level: u8) -> Result<MapData, String> {
    MAP_SOURCES.load(id, hdr_level)
}

/// What this app tells the client shell: its commit, its maps
/// (`MAP_SOURCES`) and the greybox's own hooks.
pub fn client_hooks() -> AppHooks {
    AppHooks {
        build: BUILD,
        load_map,
        load_map_file: |id, _, _| Err(format!("{id}: the mashup loads maps by id only, not from a file")),
        map_file_bytes: |_| None,
        unload_greybox: greybox::unload,
        respawn_greybox: greybox::respawn,
    }
}

/// The mashup's simulation: Combat Arms' (the engine's base, then its
/// parts, registering it as the default game), then CS:S registered as
/// a game whose maps this app plays.
pub struct MashupPlugins;

impl PluginGroup for MashupPlugins {
    fn build(self) -> PluginGroupBuilder {
        combat_arms::game::CombatArmsPlugins
            .build()
            .add_before::<combat_arms::game::data::CaDataPlugin>(combat_arms::SimPlugins)
            .add(RegisterGame(&cs_source::game::INFO))
    }
}

/// The client as this app runs it: the shell's plugins with the app's
/// hooks and Combat Arms' HUD.
pub struct ClientPlugin {
    pub args: Args,
}

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(combat_arms::ui::with_hud(
            ClientPlugins {
                args: self.args.clone(),
                hooks: client_hooks(),
            }
            .build(),
        ));
    }
}

/// The command line as the app plays it: `DEFAULT_MAP` without `--map`,
/// `DEFAULT_BOTS` bots unless the command line sets `MaxPlayer`.
pub fn with_defaults(mut args: Args) -> Args {
    args.map.get_or_insert_with(|| DEFAULT_MAP.into());
    if !args.console.iter().any(|c| c.to_lowercase().starts_with("maxplayer")) {
        args.console.insert(0, format!("MaxPlayer {DEFAULT_BOTS}"));
    }
    args
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_build_is_stamped() {
        assert!(!super::BUILD.is_empty() && !super::VERSION.is_empty());
        assert_eq!(super::client_hooks().build, super::BUILD);
    }

    #[test]
    fn it_starts_on_dust2_with_bots() {
        let args = super::with_defaults(chimera_client::client::Args::default());
        assert_eq!(args.map.as_deref(), Some(super::DEFAULT_MAP));
        assert_eq!(args.console, ["MaxPlayer 3"]);
        let mut own = chimera_client::client::Args::default();
        own.console.push("maxplayer 9".into());
        own.map = Some("combat_arms:warehouse".into());
        let own = super::with_defaults(own);
        assert_eq!((own.map.as_deref(), own.console.len()), (Some("combat_arms:warehouse"), 1));
    }

    #[test]
    fn both_games_have_loaders_and_combat_arms_plays() {
        let s = &super::MAP_SOURCES;
        assert!(s.loader("cs_source").is_some() && s.loader("combat_arms").is_some());
        assert_eq!(s.gameplay.map(|g| g.game), Some(combat_arms::game::GAME));
    }
}
