# The first mashup app

`apps/mashup_demo` (crate and binary `mashup_demo`): Counter-Strike:
Source's maps played as Combat Arms, the engine's premise (docs/DESIGN.md,
"Goal") shown end to end. The app composes `cs_source` (its map loader
only) and `combat_arms` (its whole simulation and front end) as libraries;
neither game knows the other (checks/tests/it/architecture.rs).

## Done (2026-10-19)

- Chimera's `map::sources` (chimera/world/src/map/sources.rs): an app's
  maps as `MapSources`, each game's `MapLoader` (`game:name` to
  `MapData`), the gameplay's `MapDressing` (its parts on any map) and
  `SpawnTeams` (another game's spawn teams as the gameplay's).
  `MapData::take_game_parts` takes off what a map's game brings to every
  map (bodies, held and view models, HUD, radio, bot names, navigation,
  round sounds, muzzle flash, shells, decals).
- The games' sides: `cs_source::game::MAP_LOADER` (the CS:S app's
  `games::load_map_level` uses it too), `combat_arms::game::world::
  MAP_LOADER` and `DRESSING` (`world::dress`: effect looks, view models,
  character bodies; `effects::library::add_looks` split from
  `add_to_map`).
- Scale: nothing to rescale. `MapData` is metres for every game (each
  loader applies its own unit: Source's 0.0254 m inch, Combat Arms'
  `meters_per_unit`), entity space keeps its own (`MapData::
  entity_scale`), and a gameplay converts its own numbers with its own
  unit. The heavy test checks the soldier's eye height on dust2.
- The shell's map change runs at the gameplay's tick (the default game's),
  not the map's game's (chimera/client/src/client/console.rs).
- The app: de_dust2 by default, three bots (`MaxPlayer`), Team Death
  Match, T spawns Alpha's and CT spawns Bravo's, Combat Arms' HUD; CS:S
  registered as a second game for its install. Bots navigate areas
  generated from dust2's collision (dust2's own `.nav` is CS:S's part and
  is taken off).
- Tests: unit tests in `map::sources` and the app; `heavy::dust2` (both
  installs): the map dressed, a soldier on the floor at Combat Arms'
  size, walking, firing the M4A1, bots fighting and respawning.

## Left

- CS:S's map logic (`source_logic`: doors, buttons, breakables,
  triggers) isn't run; brush entities stand as placed.
- CS:S's surfaces aren't translated into Combat Arms' surface types
  (effects and footsteps take the unknown surface's): a table of Source
  surface properties to Combat Arms' types in the dressing.
- CS:S's weapons per slot: their data and hooks belong to CS:S's
  simulation plugins; offering them needs those hooks registrable alone
  (a weapons-only plugin in `cs_source`) and the view models kept.
- Network: no map offer of CS:S maps to clients (`map_file_bytes`), no
  dedicated mashup server.
- Chimera still has Source-inch constants in presentation code
  (`client::view`'s third-person distances, `render::shadows`, audio's
  distance units); a game-level unit in `GameInfo` would make them the
  gameplay's.
- Menus: the app starts straight in game (Combat Arms has no front-end
  menus yet).
