//! Where an app's maps come from, when the map and the gameplay may be two
//! games' (docs/DESIGN.md, "Slots": the map is a slot of its own): each
//! game offers its map loader (`MapLoader`: `game:name` from its install
//! to `MapData`) and the parts its gameplay brings to any map
//! (`MapDressing`: its view models, character bodies, effect looks); an
//! app composes them (`MapSources`), so a Combat Arms soldier plays on a
//! Counter-Strike: Source map with neither game knowing the other.
//!
//! - A map of the gameplay's own game loads as its loader makes it (the
//!   loader already brings the game's parts).
//! - A map of another game is the world only: what its game brings to
//!   every map (`MapData::take_game_parts`: bodies, held and view models,
//!   HUD, radio, bot names and navigation, round sounds, muzzle flash,
//!   shells, decals) is taken off, its spawn points' teams become the
//!   gameplay's (`SpawnTeams`; unlisted teams: any team), and the
//!   gameplay's dressing is added.
//! - Units: `MapData` is metres for every game (each loader applies its
//!   own game's world unit: Source's 0.0254 m inch, Combat Arms'
//!   `meters_per_unit`), the map's entity space keeps its own scale
//!   (`MapData::entity_scale`, `MapEntities::scale`), and a gameplay
//!   converts its own numbers with its own unit, so nothing is rescaled
//!   when the two differ.
//!
//! The functions are plain `fn`s and the lists `'static` slices, so an
//! app's `MapSources` is a `static` and its `client::AppHooks::load_map`
//! a closure over it.

use super::MapData;
use crate::core::Team;

/// One game's maps (`game:name`) from its install.
#[derive(Debug)]
pub struct MapLoader {
    /// The game's id, the prefix of its map ids (`cs_source`).
    pub game: &'static str,
    /// Map `name` (without the prefix) at a Source `mat_hdr_level` (0
    /// LDR; games without HDR data ignore it).
    pub load: fn(&str, u8) -> Result<MapData, String>,
}

/// What one game's gameplay brings to any map: its view models,
/// character bodies and held models, effect looks (added to a map of
/// another game after `MapData::take_game_parts`).
#[derive(Debug)]
pub struct MapDressing {
    /// The game's id (`combat_arms`).
    pub game: &'static str,
    /// Add the game's parts to `map`; an error (no install) leaves the
    /// map undressed, with a warning.
    pub dress: fn(&mut MapData) -> Result<(), String>,
}

/// Spawn teams of another game's maps as the gameplay's teams: map
/// `game`'s team `from` is the gameplay's `to` (CS:S's terrorists, 1, as
/// Combat Arms' Alpha, 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnTeams {
    pub game: &'static str,
    pub from: u8,
    pub to: u8,
}

/// An app's maps: the loaders it chose, its gameplay's dressing and the
/// team mapping of other games' spawns.
#[derive(Debug)]
pub struct MapSources {
    /// The loaders; a bare map name (no `game:`) is the first's.
    pub loaders: &'static [&'static MapLoader],
    /// The game whose gameplay runs on every map (None: maps load as
    /// their loaders make them).
    pub gameplay: Option<&'static MapDressing>,
    pub teams: &'static [SpawnTeams],
}

impl MapSources {
    /// The loader of `game`, if the app has one.
    pub fn loader(&self, game: &str) -> Option<&'static MapLoader> {
        self.loaders.iter().copied().find(|l| l.game.eq_ignore_ascii_case(game))
    }

    /// Map `id` (`game:name`, or a bare name of the first loader's game)
    /// for this app's gameplay.
    pub fn load(&self, id: &str, hdr_level: u8) -> Result<MapData, String> {
        let (loader, name) = match id.split_once(':') {
            Some((game, name)) => (
                self.loader(game)
                    .ok_or_else(|| format!("{id}: no map loader for `{game}` in this app"))?,
                name,
            ),
            None => (*self.loaders.first().ok_or("this app loads no maps")?, id),
        };
        let mut map = (loader.load)(name, hdr_level)?;
        self.dress(loader.game, &mut map);
        Ok(map)
    }

    /// `map` (of `game`) made ready for the gameplay: unchanged when the
    /// gameplay is the map's own game (or there is none); else its game's
    /// parts taken off, its spawn teams mapped (`teams`) and the
    /// gameplay's parts added.
    pub fn dress(&self, game: &str, map: &mut MapData) {
        let Some(gameplay) = self.gameplay.filter(|g| !g.game.eq_ignore_ascii_case(game)) else {
            return;
        };
        map.take_game_parts();
        for (_, team) in &mut map.spawns {
            *team = team.and_then(|t| {
                self.teams
                    .iter()
                    .find(|s| s.game.eq_ignore_ascii_case(game) && s.from == t.0)
                    .map(|s| Team(s.to))
            });
        }
        if let Err(e) = (gameplay.dress)(map) {
            map.warnings.push(format!("{}'s parts: {e}", gameplay.game));
        }
    }
}

impl MapData {
    /// Take off what the map's game brings to every map rather than the
    /// map itself: character bodies and held models, view models, the
    /// HUD look, the radio, bot names and the navigation mesh, the round
    /// sounds, the muzzle flash, spent shells and impact decals. The
    /// world, its entities, sounds, particles and gibs stay.
    pub fn take_game_parts(&mut self) {
        self.characters.clear();
        self.held.clear();
        self.view_models.clear();
        self.hud = None;
        self.radio = None;
        self.bot_profiles = None;
        self.nav = None;
        self.round_sounds = Default::default();
        self.muzzle_flash = None;
        self.shells.clear();
        self.shell_physics = None;
        self.decals = Default::default();
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    use super::*;

    fn a_map(name: &str, _: u8) -> Result<MapData, String> {
        let mut map = MapData {
            name: format!("a:{name}"),
            spawns: vec![(Vec3::ZERO, Some(Team(1))), (Vec3::X, Some(Team(2))), (Vec3::Y, Some(Team(7)))],
            ..default()
        };
        map.nav = Some(Default::default());
        Ok(map)
    }

    fn b_map(name: &str, _: u8) -> Result<MapData, String> {
        let mut map = MapData {
            name: format!("b:{name}"),
            ..default()
        };
        map.nav = Some(Default::default());
        Ok(map)
    }

    fn b_parts(map: &mut MapData) -> Result<(), String> {
        map.warnings.push("b's parts".into());
        Ok(())
    }

    static A: MapLoader = MapLoader { game: "a", load: a_map };
    static B: MapLoader = MapLoader { game: "b", load: b_map };
    static B_PARTS: MapDressing = MapDressing { game: "b", dress: b_parts };
    static SOURCES: MapSources = MapSources {
        loaders: &[&A, &B],
        gameplay: Some(&B_PARTS),
        teams: &[SpawnTeams { game: "a", from: 1, to: 2 }, SpawnTeams { game: "a", from: 2, to: 1 }],
    };

    #[test]
    fn another_games_map_is_dressed_for_the_gameplay() {
        let map = SOURCES.load("a:x", 0).unwrap();
        assert_eq!(map.name, "a:x");
        assert!(map.nav.is_none(), "a's navigation taken off");
        assert_eq!(map.warnings, ["b's parts"]);
        let teams: Vec<_> = map.spawns.iter().map(|(_, t)| t.map(|t| t.0)).collect();
        assert_eq!(teams, [Some(2), Some(1), None], "mapped; unlisted teams spawn anyone");
    }

    #[test]
    fn the_gameplays_own_map_loads_as_its_loader_makes_it() {
        let map = SOURCES.load("b:y", 0).unwrap();
        assert!(map.nav.is_some() && map.warnings.is_empty());
        assert_eq!(SOURCES.load("x", 0).unwrap().name, "a:x", "a bare name is the first loader's");
        assert!(SOURCES.load("c:z", 0).unwrap_err().contains("no map loader for `c`"));
    }
}
