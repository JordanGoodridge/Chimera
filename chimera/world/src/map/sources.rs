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
//! - Surfaces: a map of another game keeps its own surface names (its
//!   game's parts that stay, a gun of its game among them, use them), and
//!   the app's `SurfaceTable` for the pair (a data file the app owns, not
//!   either game) gives each its gameplay name (`MapData::surface_names`,
//!   spawned as `SurfaceNames`): by name, else by the Source game material
//!   letter the map's surface has (`sound::MapSurface::game_material`),
//!   else the table's default.
//! - Kept parts: the held and view models of the map's game whose keys
//!   start with one of `MapSources::kept` stay (another game's weapons an
//!   app offers in the gameplay's slots).
//!
//! The functions are plain `fn`s and the lists `'static` slices, so an
//! app's `MapSources` is a `static` and its `client::AppHooks::load_map`
//! a closure over it.

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use bevy::prelude::*;
use serde::Deserialize;

use super::MapData;
use crate::core::Team;

/// What a map load brings in (docs/plans/active/architecture-review.md,
/// F1): everything by default, less for a dedicated server or a test that
/// never draws the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapLoadOptions {
    /// Source's `mat_hdr_level` (0 LDR; games without HDR data ignore it).
    pub hdr_level: u8,
    /// Decode the textures. Off: each texture is a placeholder of the
    /// right size (`MapTexture::width`, `height`; no texels, no mip
    /// levels), so materials and decals still find them and the map's
    /// logic, collision and sounds are the same; a map loaded so can't be
    /// drawn. A game whose loader doesn't decode textures at load ignores it.
    pub textures: bool,
}

impl Default for MapLoadOptions {
    fn default() -> Self {
        Self { hdr_level: 0, textures: true }
    }
}

impl MapLoadOptions {
    /// Everything, at `mat_hdr_level` `hdr_level`.
    pub fn hdr(hdr_level: u8) -> Self {
        Self { hdr_level, ..Self::default() }
    }

    /// No textures (a dedicated server, a test that never draws).
    pub fn lean() -> Self {
        Self { textures: false, ..Self::default() }
    }
}

/// One game's maps (`game:name`) from its install.
#[derive(Debug)]
pub struct MapLoader {
    /// The game's id, the prefix of its map ids (`cs_source`).
    pub game: &'static str,
    /// Map `name` (without the prefix) with `options`.
    pub load: fn(&str, &MapLoadOptions) -> Result<MapData, String>,
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

/// One game's surface names as another's: a data file an app owns for
/// the pair (TOML: `[names]` the map's surface property names, lower-case,
/// to the gameplay's; `[materials]` Source game material letters to the
/// gameplay's, for names not listed; `default`, for the rest).
#[derive(Clone, Copy, Debug)]
pub struct SurfaceTable {
    /// The maps' game (`cs_source`).
    pub map: &'static str,
    /// The gameplay's game (`combat_arms`).
    pub gameplay: &'static str,
    /// The table's text (the app's data file, built in).
    pub text: &'static str,
}

/// A `SurfaceTable`'s rows.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceRows {
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub names: HashMap<String, String>,
    #[serde(default)]
    pub materials: HashMap<char, String>,
}

impl SurfaceTable {
    /// The table's rows (names lower-cased).
    pub fn rows(&self) -> Result<SurfaceRows, String> {
        let mut rows: SurfaceRows =
            toml::from_str(self.text).map_err(|e| format!("{} to {} surfaces: {e}", self.map, self.gameplay))?;
        rows.names = rows.names.into_iter().map(|(k, v)| (k.to_lowercase(), v)).collect();
        rows.materials = rows.materials.into_iter().map(|(k, v)| (k.to_ascii_uppercase(), v)).collect();
        Ok(rows)
    }
}

impl SurfaceRows {
    /// The gameplay's name for the map's surface `name` (lower-case),
    /// whose game material letter is `material` (None: the map doesn't
    /// say).
    pub fn get(&self, name: &str, material: Option<char>) -> Option<&str> {
        self.names
            .get(name)
            .or_else(|| self.materials.get(&material?.to_ascii_uppercase()))
            .or(self.default.as_ref())
            .map(String::as_str)
    }

    /// Every surface name `map` uses (its sound surfaces, its world's and
    /// its models' meshes, its models' and brushes' surface properties)
    /// with its gameplay name.
    pub fn translate(&self, map: &MapData) -> HashMap<String, String> {
        let mut names: BTreeSet<String> = map.sounds.surfaces.keys().cloned().collect();
        let meshes = map.meshes.iter().chain(map.models.iter().flat_map(|m| &m.meshes));
        names.extend(meshes.flat_map(|m| [&m.surface, &m.surface2]).flatten().cloned());
        names.extend(map.models.iter().filter_map(|m| m.surfaceprop.clone()));
        names
            .into_iter()
            .map(|n| n.to_lowercase())
            .filter_map(|n| {
                let material = map.sounds.surfaces.get(&n).map(|s| s.game_material);
                let to = self.get(&n, material)?.to_string();
                Some((n, to))
            })
            .collect()
    }
}

/// The loaded map's surface names as the gameplay's (`MapData::
/// surface_names`): empty when they are the gameplay's own.
#[derive(Resource, Clone, Debug, Default)]
pub struct SurfaceNames(pub Arc<HashMap<String, String>>);

impl SurfaceNames {
    /// The gameplay's name of the map's surface `name` (any case); None:
    /// no translation (the map's own name is the gameplay's, or unknown).
    pub fn get(&self, name: &str) -> Option<&str> {
        if self.0.is_empty() {
            return None;
        }
        self.0.get(&name.to_lowercase()).map(String::as_str)
    }

    /// `name` as the gameplay knows it: its translation, else itself.
    pub fn resolve<'a>(&'a self, name: &'a str) -> &'a str {
        self.get(name).unwrap_or(name)
    }
}

/// An app's maps: the loaders it chose, its gameplay's dressing, the
/// team mapping of other games' spawns, the surface tables and the map
/// game's parts it keeps.
#[derive(Debug)]
pub struct MapSources {
    /// The loaders; a bare map name (no `game:`) is the first's.
    pub loaders: &'static [&'static MapLoader],
    /// The game whose gameplay runs on every map (None: maps load as
    /// their loaders make them).
    pub gameplay: Option<&'static MapDressing>,
    pub teams: &'static [SpawnTeams],
    /// Other games' surface names as the gameplay's, one table per pair.
    pub surfaces: &'static [SurfaceTable],
    /// Key prefixes of the map game's held and view models that stay on
    /// its maps (`cs_source:weapon_`: CS:S's guns an app offers).
    pub kept: &'static [&'static str],
}

impl MapSources {
    /// The loader of `game`, if the app has one.
    pub fn loader(&self, game: &str) -> Option<&'static MapLoader> {
        self.loaders.iter().copied().find(|l| l.game.eq_ignore_ascii_case(game))
    }

    /// Map `id` (`game:name`, or a bare name of the first loader's game)
    /// for this app's gameplay.
    pub fn load(&self, id: &str, hdr_level: u8) -> Result<MapData, String> {
        self.load_with(id, &MapLoadOptions::hdr(hdr_level))
    }

    /// `load` with `options` (a server's: `MapLoadOptions::lean`).
    pub fn load_with(&self, id: &str, options: &MapLoadOptions) -> Result<MapData, String> {
        let (loader, name) = match id.split_once(':') {
            Some((game, name)) => (
                self.loader(game)
                    .ok_or_else(|| format!("{id}: no map loader for `{game}` in this app"))?,
                name,
            ),
            None => (*self.loaders.first().ok_or("this app loads no maps")?, id),
        };
        let mut map = (loader.load)(name, options)?;
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
        map.take_game_parts_but(self.kept);
        if let Some(table) = self
            .surfaces
            .iter()
            .find(|t| t.map.eq_ignore_ascii_case(game) && t.gameplay.eq_ignore_ascii_case(gameplay.game))
        {
            match table.rows() {
                Ok(rows) => map.surface_names = rows.translate(map),
                Err(e) => map.warnings.push(e),
            }
        }
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
        self.take_game_parts_but(&[]);
    }

    /// `take_game_parts`, but the held and view models whose keys start
    /// with one of `kept` stay.
    pub fn take_game_parts_but(&mut self, kept: &[&str]) {
        let keep = |key: &str| kept.iter().any(|k| key.starts_with(k));
        self.characters.clear();
        self.held.retain(|h| keep(&h.key));
        self.view_models.retain(|v| keep(&v.key));
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

    fn a_map(name: &str, _: &MapLoadOptions) -> Result<MapData, String> {
        let mut sounds = crate::map::sound::MapSounds::default();
        for (name, letter) in [("sand", 'N'), ("rock", 'C'), ("weird", '?')] {
            let surface = crate::map::sound::MapSurface {
                game_material: letter,
                ..default()
            };
            sounds.surfaces.insert(name.into(), surface);
        }
        let mut map = MapData {
            name: format!("a:{name}"),
            spawns: vec![(Vec3::ZERO, Some(Team(1))), (Vec3::X, Some(Team(2))), (Vec3::Y, Some(Team(7)))],
            sounds: Arc::new(sounds),
            ..default()
        };
        map.meshes.push(crate::map::MapMesh {
            surface: Some("wood_crate".into()),
            ..default()
        });
        for key in ["a:gun", "a:knife"] {
            map.held.push(crate::map::MapHeldModel {
                key: key.into(),
                model: default(),
                bone: String::new(),
                muzzle: None,
                merged: Vec::new(),
                own: None,
            });
        }
        map.nav = Some(Default::default());
        Ok(map)
    }

    fn b_map(name: &str, _: &MapLoadOptions) -> Result<MapData, String> {
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
        surfaces: &[SurfaceTable {
            map: "a",
            gameplay: "b",
            text: "default = \"Stone\"\n[names]\nWood_Crate = \"Wood\"\n[materials]\nN = \"Sand\"\n",
        }],
        kept: &["a:gun"],
    };

    #[test]
    fn another_games_map_is_dressed_for_the_gameplay() {
        let map = SOURCES.load("a:x", 0).unwrap();
        assert_eq!(map.name, "a:x");
        assert!(map.nav.is_none(), "a's navigation taken off");
        assert_eq!(map.warnings, ["b's parts"]);
        let teams: Vec<_> = map.spawns.iter().map(|(_, t)| t.map(|t| t.0)).collect();
        assert_eq!(teams, [Some(2), Some(1), None], "mapped; unlisted teams spawn anyone");
        let kept: Vec<&str> = map.held.iter().map(|h| h.key.as_str()).collect();
        assert_eq!(kept, ["a:gun"], "the kept prefix stays");
    }

    #[test]
    fn options_reach_the_loader() {
        fn lean_only(name: &str, o: &MapLoadOptions) -> Result<MapData, String> {
            assert!(!o.textures && o.hdr_level == 0, "{o:?}");
            Ok(MapData { name: name.into(), ..default() })
        }
        static L: MapLoader = MapLoader { game: "l", load: lean_only };
        static S: MapSources = MapSources { loaders: &[&L], gameplay: None, teams: &[], surfaces: &[], kept: &[] };
        S.load_with("l:x", &MapLoadOptions::lean()).unwrap();
        assert!(MapLoadOptions::default().textures, "everything by default");
    }

    #[test]
    fn surfaces_get_the_gameplays_names() {
        let map = SOURCES.load("a:x", 0).unwrap();
        let names = SurfaceNames(Arc::new(map.surface_names.clone()));
        assert_eq!(names.get("WOOD_CRATE"), Some("Wood"), "by name");
        assert_eq!(names.get("sand"), Some("Sand"), "by game material letter");
        assert_eq!(names.get("weird"), Some("Stone"), "else the default");
        assert_eq!(names.resolve("unseen"), "unseen");
        let own = SOURCES.load("b:y", 0).unwrap();
        assert!(own.surface_names.is_empty(), "the gameplay's own map keeps its names");
    }

    #[test]
    fn the_gameplays_own_map_loads_as_its_loader_makes_it() {
        let map = SOURCES.load("b:y", 0).unwrap();
        assert!(map.nav.is_some() && map.warnings.is_empty());
        assert_eq!(SOURCES.load("x", 0).unwrap().name, "a:x", "a bare name is the first loader's");
        assert!(SOURCES.load("c:z", 0).unwrap_err().contains("no map loader for `c`"));
    }
}
