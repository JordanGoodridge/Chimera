# Architecture

A map of the code. Design rationale lives in [DESIGN.md](DESIGN.md); this
file says where things are and which way dependencies may point.

## Crates

A Cargo workspace (docs/plans/active/engine-split.md, "Crates and compile
time"); every member is a default member, so `cargo test` and `cargo
nextest run` at the root test them all (`checks/tests/it/architecture.rs`,
`workspace_crates`).

```
mashup (repository root: src/, tests/it/)   `dump`; `cs_source_app` at the old paths
  |    cs_source_server (apps/cs_source_server/)   the dedicated server: the app without `present`
  |   /
cs_source_app (apps/cs_source/)   the CS:S app: the game binary, SimPlugins (+ its version), games::load_map
  |
  |    cs_source[ui] (games/cs_source/src/ui/)   CS:S's front end: menus, panels, HUD (feature `ui`)
  |   /         \
  chimera_client  |  (chimera/client/)   client: the shell (input, camera, console UI, HUD base, widgets, debug)
  |    \        |
  |    cs_source (games/cs_source/)   CS:S's simulation: loaders, parts, rules, bots, SimPlugins, harness::CsSim
  |    |    source_logic (source/logic/)   logic: Source entity I/O
  |    |   /
  |    chimera_net (chimera/net/)   net: transport, replication, prediction; harness::NetSim
  |    |
chimera_present (chimera/present/)   render, audio, MapPlugin
  |    chimera_sim (chimera/sim/)   weapon, movement, rule_module; SimBasePlugins
  |    |
chimera_world (chimera/world/)   map: map data, collision, sim-side spawning; greybox, harness::Sim
  |                \
chimera_core          source_formats (source/formats/)   formats as plain data: no Bevy
 (chimera/core/)       core, console, slots, character, metrics, mount, game_registry
```

- `source_formats` turns bytes and text into Rust values (glam vectors,
  Source units); it never depends on Bevy, avian or `mashup` (checked).
  Conversion into engine types stays in the game (`cs_source`), which
  re-exports the readers at their old paths.
- An edit in `source_formats` rebuilds every crate (`mount` in
  `chimera_core` uses it); `cargo test -p source_formats` builds and runs
  only its own tests (seconds).
- `chimera_core` is the engine's stable base: the modules every layer
  uses and that rarely change (`core` but the build's `VERSION`,
  `console`, `slots`, `character`, `metrics`, and `mount`, which uses
  `source_formats`' VFS and KeyValues). It never depends on
  `mashup` or a game (checked); `mashup` re-exports its modules at their
  old paths (`mashup::core` adds `VERSION`, stamped in the app's crate
  so a commit doesn't rebuild the base). The layering table covers its
  modules too. `cargo test -p chimera_core --features dev` tests it alone.
- `chimera_world` is the simulation's map (`map`): the neutral map data,
  collision, spawning a map's simulated parts (the drawn ones through
  `map::MapDrawer`), the map's systems. It uses `chimera_core` and
  `source_formats` of ours and never what draws or plays the map, `mashup`
  or a game (checked). It registers the map's systems itself
  (`map::build`): alone as `map::MapSimPlugin` (the dedicated server's,
  nothing drawn or played), or with what draws and plays the map at
  fixed places among them (`map::MapLayers`, filled by `chimera_present`'s
  `MapPlugin`; the client's order). It also holds the greybox (`greybox`:
  the map built in code) and the headless harness (`harness::Sim`, on
  the simulation plugins its caller passes as the base), so every crate
  from here up tests on it at its own layer. `mashup::map` re-exports it, with
  `MapPlugin` in builds with the presentation.
  `cargo test -p chimera_world --features dev` tests it alone.
- `chimera_present` is the presentation beside the simulation: `render`,
  `audio` and `map_plugin` (`MapPlugin`: map's registration with audio's
  and render's systems at the places `map::MapLayers` keeps, in one
  order). It uses `chimera_world` and `chimera_core`, never `mashup` or a
  game (checked); `mashup` re-exports `render` and `audio` at their old
  paths. An edit in it rebuilds it and `mashup`, not the map; an edit in
  `mashup` rebuilds neither.
  `cargo test -p chimera_present --features dev` tests it alone.
- `chimera_client` is the client shell (`client`, re-exported by
  `mashup::client` at its old paths): local input and binds, the local
  player's camera, the console UI, chat, the fallback HUD and HUD text,
  fonts, the VGUI-like widgets and the drawing of a game's VGUI layouts,
  spectating's state and camera, picture and sound settings, captures,
  the debug UI and overlays, the network glue and the front end's
  interface (`front_end`). Its plugins are a group (`ClientPlugins`) a
  game's front end puts its HUD into (`add_after`). It uses the engine's
  crates, the presentation and the network included, and never Source's
  logic, `mashup` or a game (checked): a game says what the shell needs as
  data (`game_registry`, `core`, `debug_ui::GameDebug`), the map logic as
  `map::logic_view`, the app as `client::AppHooks`.
  `cargo test -p chimera_client --features dev` tests it alone.
- `chimera_sim` is the simulation's frameworks: `weapon` (the weapon
  system), `movement` (the movement slot's placeholder and noclip) and
  `rule_module` (the rule-module interface, `mashup::rules::module`).
  They name no game; games fill them. It uses `chimera_world` and
  `chimera_core`, never the presentation, `mashup` or a game (checked);
  `mashup` re-exports them at their old paths. `cargo test -p chimera_sim
  --features dev` tests it alone.
- `cs_source` is Counter-Strike: Source: `game` (its loaders, Source
  movement, weapons, effects, animation, `CsSourcePlugins`;
  `mashup::games::cs_source`), CS:S's rules, `rules` and `objectives`,
  its bots, `bot`, and its part of the network (`game::net`: its round,
  bomb and hostages replicated, its join flow and bots behind the
  network's hooks); with its feature `ui`, its front end (`ui`:
  `mashup::games::cs_source::ui`; the GameUI menus and dialogs, the join
  flow, the buy, team, radio and spectator menus, the scoreboard and its
  HUD, on the shell, `with_hud` putting the HUD among the shell's
  plugins). The simulation uses the engine crates (not `chimera_present`),
  Source's logic and `source_formats`, never `mashup` or another game
  (checked); only `ui` uses the presentation and the shell, as optional
  dependencies of the feature (checked: `workspace_crates`,
  `game_ui_only_with_its_feature`), and nothing else of the crate uses
  `ui`; its tests may use the presentation too (a dev-dependency:
  `TEST_ONLY`). `SimPlugins` is CS:S's simulation (Chimera's base,
  `chimera_sim::SimBasePlugins`, with its rules, objectives, bots,
  network part and Source's logic), which the app runs; `harness::CsSim`
  is that as the tests' and tools' base, with its test maps for network
  tests. `cargo test -p cs_source --features dev` tests the simulation
  (its scenario tests, heavy map tests among them), `--features dev,ui`
  the front end too.
- `source_logic` is Source's map logic (`logic`: entity I/O, logic
  classes, triggers, movers, breakables, props, fire, the bridge to the
  map's entities) with its part of the network (`logic::net`: movers,
  HUD messages). It uses `chimera_net`, `chimera_world` and `chimera_core`,
  never the presentation, `mashup` or a game (checked); `mashup::logic`
  re-exports it. `cargo test -p source_logic --features dev` tests it
  alone.
- `chimera_net` is network play (`net`): the transport, the join
  handshake, replication of what every game has, prediction and
  interpolation. A game and Source's logic register what they replicate
  of their own as parts (`net::NetAppExt::net_part`, applied in name
  order in `NetPlugin::finish`, after the engine's protocol) and fill
  its hooks (`net::parts`: `JoinRules`, `BotHooks`, `Holds`,
  `NetHooks`); `harness::NetSim` runs a server and clients over its
  in-memory link. It uses `chimera_sim`, `chimera_world` and `chimera_core`,
  never the presentation, Source's logic, `mashup` or a game (checked);
  `mashup::net` re-exports it with the parts' replicated types.
  `cargo test -p chimera_net --features dev` tests it alone.
- `cs_source_app` (apps/cs_source/) is the faithful CS:S app: the game
  binary (`mashup`, the workspace's `default-run`) and the app's glue:
  `SimPlugins` (`cs_source::SimPlugins` with the build's version),
  `games::load_map`, `client_hooks` and `client` (the shell with this app's hooks
  and CS:S's HUD), re-exports at the old paths, and build.rs's stamp. It
  uses `chimera_present` only with its default feature `present` (with
  the client shell and CS:S's front end, `cs_source/ui`).
- `cs_source_server` (apps/cs_source_server/) is the dedicated server
  (`mashup_server`): `cs_source_app` without `present`, so built alone it
  links no render or audio code (checked:
  `server_builds_without_presentation`, `workspace_crates`); in a
  workspace build features unify, so releases build it with `-p`.
- `mashup` (the repository root) is a thin shim: `cs_source_app`
  re-exported at the old `mashup::` paths (src/lib.rs) for `dump` (an
  install inspector for several games; its test in tests/it/) and Combat
  Arms' local loader (`games::combat_arms`, git-ignored). Nothing new
  goes in it.
- Each crate keeps its tests: unit tests beside the code, integration
  tests in one binary per crate (`<crate>/tests/it/`, `heavy::` for the
  slow ones; `cs_source`'s front end's in tests/ui/ with `ui`), in the
  lowest crate whose API they need (docs/OBSERVABILITY.md, section 1;
  checked: `tests_are_one_binary_per_crate`). Scenario tests run on the
  engine's harness (`chimera_world::harness::Sim`) with the base of
  their layer: the engine's (`chimera_sim::SimBasePlugins`) in Chimera's
  crates, CS:S's (`cs_source::harness::CsSim`) in `cs_source`'s, so
  the app keeps only its own.
- `repo_checks` (checks/) is the repository's own checks: the
  architecture tests (checks/tests/it/architecture.rs) and `affected`,
  the hooks' test selection (changed files to packages, then every
  package whose tests depend on them through cargo's crate graph). Plain
  crates only, no workspace crate or Bevy (checked), so it builds in
  seconds; its tests run on every selection.
- Tools live at the lowest layer whose knowledge they need, each layer's
  in its own crate, binaries behind `tools` (checked: `workspace_crates`):
  `chimera_tools` (chimera/tools/: the perf gate, `imgcmp`, `tracesum`;
  no Source, game or app), `source_tools` (source/tools/: RCON,
  `demodump`, `testmap`; no Bevy, Chimera or game) and `cs_source_tools`
  (games/cs_source/tools/: `refcmp`'s CS:S driver, `movecmp`, `demosim`, `mapsweep`,
  `perfgate`'s scenarios). CS:S's tools still depend on `cs_source_app`
  (its build stamp, map loading by id: tech-debt.md).
  An app's own tools only for app concerns (launch, packaging, budgets).
- `workspace-hack` (chimera/workspace-hack/, generated by `cargo hakari`,
  .config/hakari.toml) unifies third-party features: every member but
  `source_formats`, `source_tools`, `chimera_dev` and `repo_checks` depends on it
  (checked), so `cargo test -p <crate>` reuses the workspace's Bevy build.
  `chimera_dev` (chimera/dev/) carries Bevy's development features
  (`dev`: dynamic linking, the remote protocol; `profile`: Chrome traces),
  which every crate's `dev`/`profile` reaches through `chimera_core`'s;
  hakari leaves it out, so the hack never turns them on for release.

## Layers

```
            client            (window, input, camera, HUD, debug, agent tools)
              |
   harness    |    games/<name>   (one module per game)
      \       |       /    \
       net ---'      /      \       (net: transport, replication, join handshake)
        |           /        \
       bot   rules   logic  movement   greybox   mount   (mount: VFS, local config)
        |      |       |      |          |
        |  objectives  |      |          |      (objectives: bomb, hostages, zones)
        |      |       |      |          |
        |    weapon    |      |          |      (weapon: parts, inventory, weapon frame)
        |      |      /       |          |      (logic: entity I/O, triggers, movers, +use)
        |     map ---'        |          |      (map: neutral MapData + spawner, sound data)
         \     |       /          /
           character             (components every character has)
               |
             slots               (registries, loadout, swapping)
               |
            console              (cvars, commands, queue; games and client register)
               |
             core                (shared vocabulary)
```

Exact edges: net uses the engine's simulation (core, console, slots,
character, movement, map, weapon) and names no game or Source logic:
logic and games/<name> use it to register what they replicate (their
parts, `net::parts`), and harness and client use it; bot uses core, console,
slots, character, map, weapon (grenades
it carries and throws), objectives (planting, defusing); rules uses core,
console, weapon, objectives; objectives uses core, console, slots,
character, map, weapon (games use it too, to fill in their bomb and
hostages); weapon uses core, console, map; logic uses core, console,
map (games feed it through `MapData::entities`; harness and client may use
it); map uses only core. games/<name> uses core, console, slots,
character, movement, mount, map, weapon, objectives and rules (to register
its rule modules), bot and net (its part of the network); its `ui/` uses
those, client and the rest of the simulation.

Presentation sits beside the simulation, above `map`: `audio` (sound
playback, live sounds, room DSP, soundscapes, hearing) uses core and map,
and only `client` and `map_plugin` use it; `render` (drawing what the map
spawns and simulates: the drawn phases of spawning a map, what is made
in play, effects, culling, water, shadows, the sky, view models, the
materials and their shaders) uses core and map, and only `client`,
`map_plugin` and a game's `ui/` use it. `map_plugin` builds
`map::MapPlugin` (its type, re-exported into `map`): map's registration
(`map::build`) with audio's and render's systems at the places map keeps
for them (`map::MapLayers`), in one fixed order; the dedicated server
adds `map::MapSimPlugin` (map's alone). Animation evaluation stays on the
simulation's side (the server poses hitboxes); see
docs/plans/active/engine-split.md, "Crates and compile time".

Drawn-part requests: what the simulation makes during play says in plain
data what it looks like, and `render` fulfils it; the simulation never
builds a mesh or material. A character's team, `BodyName` and class pick
its body, `Held` names what it holds, a `Ragdoll` takes the dead's body
over, `LooseItem`/`ShownItem` name a held model to lie or move as;
a gib carries its `GibLook` (which model of which gib list, skin, glass,
fade distances) and a window its `BrushPanes`; render's systems react to
them (added, changed, removed, without their drawn marker). Without `render` (a dedicated server) the requests stay
unfulfilled and the simulation's parts (bodies, colliders) work alone
(`games/cs_source/tests/it/drawn_parts.rs`).

`metrics` (per-thread CPU time and hardware counters, distributions, the
tick's measurements) uses no other module of ours; any layer may use it
(so far harness and client; `SimPlugins` adds its tick plugin).

A module may use only the modules below it. Enforced by
`checks/tests/it/architecture.rs` (`ALLOWED`); update both together.

- `core` never depends on anything in this crate.
- Simulation code (everything except `client`) never depends on `client`, so
  it runs headless in tests and, later, on a dedicated server.
- `games/<a>` never uses `games/<b>`. Shared needs go into `core` once two or
  three games need them (README, "rule of three"), or through a bridge.
- A game's front end (its crate's `ui`, behind its `ui` feature: menus, panels, HUD) sits on
  `client` (the shell: input, camera, console UI, VGUI-like widgets, debug
  tools, net glue); the rest of the game never uses it. The shell reaches
  it only through `client::front_end` (hooks the game registers), and the
  app adds it (main.rs). The shell names no game (checked): what it needs
  of one (install, map cycle, tick, default binds, movement cvars) is data
  the game registers (`game_registry::GameInfo`, the first registered
  being the default game).
- Rule modules (`rules::module`): a game registers its modes by name
  (`cs_source:bomb`, ...), picked by the map and cvars; their code is
  still in `rules` and `objectives` (engine-split.md).

## Modules

One line per module or folder, grouped by layer. The detail lives in each
file's `//!` doc comment, which is the source of truth: when code changes
what a file does, change its `//!` doc in the same commit, and touch this
list only when a file is added, removed or changes its job. Keep each
entry to one short line (`checks/tests/it/architecture.rs` checks the length and
that the paths exist).

### Foundations (core, console, slots, character, movement)

- `chimera/core/src/core.rs`: the shared vocabulary: `Intent`, velocity and movement state, health and damage, teams, rounds, the tick sets (core)
- `apps/cs_source/src/core.rs`: `chimera_core::core` re-exported, plus the build's `VERSION` (stamped by build.rs) (core)
- `chimera/core/src/console.rs`: cvar and command registry, Source-style parsing, aliases, binds, exec and config files, built-ins, cvar scopes (console)
- `chimera/core/src/slots.rs`: `MovementRegistry`, `Loadout`, `set_movement` (slots)
- `chimera/core/src/character.rs`: `character_bundle`, `spawn_character`, capsule size (character)
- `chimera/sim/src/movement/`: `placeholder` (stand-in walking) and `noclip` movement implementations (movement)
- `chimera/world/src/greybox.rs`: a map slot built in code: ramps, crates, a ladder, a water tank (greybox)
- `chimera/core/src/mount/`: the VFS and KeyValues (re-exported from `source_formats`), `mashup.local.toml`, finding game installs (mount)
- `chimera/core/src/game_registry.rs`: `GameInfo` (what a game tells the shell), `Games`, `RegisterGame`; the default game (game_registry)
- `chimera/core/src/metrics/`: load-independent performance metrics: per-thread CPU time, hardware counters, distributions, tick metrics (metrics)

### Map (crate `chimera_world`, `chimera/world/`: neutral map data, spawning, effects as data)

- `chimera/world/src/lib.rs`, `chimera/world/src/map/mod.rs`: `unload_map`/`change_map`, `MapHooks`, resources (map)
- `chimera/world/src/map/registration.rs`: map's registration (`build`) with its presentation's places (`MapLayers`); `MapSimPlugin`, the `()` drawer (map)
- `chimera/world/src/map/data.rs`: `MapData` and everything it holds; plain data (map)
- `chimera/world/src/map/state.rs`: what the logic and replication write and the map and its presentation apply (map)
- `chimera/world/src/map/spawn.rs`: `spawn_map` in fixed phases, the drawn parts through a `MapDrawer` (map)
- `chimera/world/src/map/entities.rs`: `MapEntity`, entity-space helpers, anchors, prop-data keys (map)
- `chimera/world/src/map/loading.rs`: how far a map load has come, for a loading screen (map)
- `chimera/world/src/map/collision.rs`, `sky.rs`: colliders and the sky built from map data (map)
- `chimera/world/src/map/material_fx.rs`: what world and prop shaders share: UV transforms, detail modes, self-illumination (map)
- `chimera/world/src/map/vis.rs`: visibility data: PVS clusters, area portals, occluders, chunking (map)
- `chimera/world/src/map/lod.rs`: `MapLod`: a model's lower levels of detail as data (map)
- `chimera/world/src/map/logic_view.rs`: what map logic shows a player (HUD messages, fades, shakes) and tools (map)
- `chimera/world/src/map/surface_color.rs`: a surface's colour and light at a hit point, for tinted effects (map)
- `chimera/world/src/map/interp.rs`: drawing between simulation ticks (map)
- `chimera/world/src/map/bodies.rs`: which body and held model a character has, hitboxes (map)
- `chimera/world/src/map/props.rs`: placed props after spawning: physics props, sequences (map)
- `chimera/world/src/map/anim.rs`: skeletal animation: `AnimSet`, blending, layers, IK (map)
- `chimera/world/src/map/view_model.rs`: first-person view models' data, animator, events and placement maths (map)
- `chimera/world/src/map/ragdoll.rs`: ragdolls on character models (map)
- `chimera/world/src/map/placed_ragdoll.rs`: map-placed ragdolls (prop_ragdoll) (map)
- `chimera/world/src/map/loose.rs`: loose items: dropped weapons' bodies, shown items, resting on the ground (map)
- `chimera/world/src/map/shells.rs`: spent shells' types and flight step (map)
- `chimera/world/src/map/breakables.rs`: gibs, broken panes and falling pieces: physics, and gibs' looks (map)
- `chimera/world/src/map/prop_physics.rs`: physics props' motion as the logic asks (map)
- `chimera/world/src/map/controllers.rs`: physics controllers and joints the logic switches (map)
- `chimera/world/src/map/buoyancy.rs`: bodies in water float (map)
- `chimera/world/src/map/copies.rs`: copies of map nodes made at run time (map)
- `chimera/world/src/map/physics_clip.rs`: brushes solid to physics objects only (func_clip_vphysics) and their filters (map)
- `chimera/world/src/map/monitor.rs`: monitor screen cameras' markers, point_camera fog as a view fog (map)
- `chimera/world/src/map/fish.rs`: func_fish_pool's fish and where they swim next (map)
- `chimera/world/src/map/color_correction.rs`: color_correction entities as data (map)
- `chimera/world/src/map/water.rs`: water materials, surfaces, moving water, the view's water state (map)
- `chimera/world/src/map/particles.rs`: the CPU particle pool (map)
- `chimera/world/src/map/psys.rs`: data-driven particle systems (`.pcf` effects) as data (map)
- `chimera/world/src/map/emitters.rs`: sprite trails, smoke stacks, tesla arcs, embers, muzzle flashes, strikes as data (map)
- `chimera/world/src/map/beams.rs`: beams and glows as data (map)
- `chimera/world/src/map/steam.rs`: steam jets as data (map)
- `chimera/world/src/map/fire.rs`: the fires and burning entities the logic says burn now (map)
- `chimera/world/src/map/tracer.rs`: bullet tracer requests and muzzle points (map)
- `chimera/world/src/map/decal.rs`: runtime decals and players' sprays as data, impacts' lift (map)
- `chimera/world/src/map/rope_sway.rs`: ropes' settling step and spline (map)
- `chimera/world/src/map/nav.rs`: `NavMesh`: areas, links, ladders, hiding spots, A* (map)
- `chimera/world/src/map/sound.rs`: the simulation's side of sounds (map)
- `chimera/world/src/map/radio.rs`: a game's team radio data and chat line format (map)
- `chimera/world/src/map/hud.rs`: a game's own HUD, menus and GameUI look as data (map)
- `chimera/present/src/map_plugin.rs`: `MapPlugin`: map's registration with audio's and render's systems at its places (above map, audio, render)

### Render (crate `chimera_present`, `chimera/present/`: presentation beside the simulation)

- `chimera/present/src/render/mod.rs`, `chimera/present/src/render/spawn.rs`: the drawn phases of spawning a map (`MapAssets`, a `map::MapDrawer`) (render)
- `chimera/present/src/render/tint.rs`: entities' render colour, alpha and mode; brush texture frames (render)
- `chimera/present/src/render/texture_anim.rs`: animated textures (AnimatedTexture proxy) swapped into the materials binding them (render)
- `chimera/present/src/render/detail.rs`, `detail.wgsl`: detail sprites (grass and weeds) (render)
- `chimera/present/src/render/sky_occluder.rs`, `sky_occluder.wgsl`: opt-in depth-only sky faces (render)
- `chimera/present/src/render/beams.rs`, `beam.wgsl`: drawing beams, glows and spark bursts (render)
- `chimera/present/src/render/color_correction.rs`: color_correction volumes blended into one table (render)
- `chimera/present/src/render/blend_order.rs`: blended meshes at the same depth drawn in an order of their own (render)
- `chimera/present/src/render/light_styles.rs`: switchable and animated light styles at run time (render)
- `chimera/present/src/render/lod.rs`: props' levels of detail as drawn (render)
- `chimera/present/src/render/glow.rs`: glow sprites faded by how much of their proxy is seen (render)
- `chimera/present/src/render/psys.rs`: running and drawing the particle systems (render)
- `chimera/present/src/render/shells.rs`: spent shells thrown, flying and drawn (render)
- `chimera/present/src/render/rope_sway.rs`: ropes in the wind (render)
- `chimera/present/src/render/fish.rs`: func_fish_pool's fish swimming (render)
- `chimera/present/src/render/parts.rs`: drawn parts the logic switches on and off (render)
- `chimera/present/src/render/dust.rs`: dust motes; the effects' dynamic meshes (render)
- `chimera/present/src/render/steam.rs`: steam jets' puffs (render)
- `chimera/present/src/render/tracer.rs`: bullet tracers drawn from the muzzle (render)
- `chimera/present/src/render/emitters.rs`: trails, smoke stacks, teslas, embers, muzzle flashes, strikes through the pool (render)
- `chimera/present/src/render/particles.rs`, `particle.wgsl`: drawing the particle pool (render)
- `chimera/present/src/render/decal.rs`, `decal.wgsl`: runtime decals projected and drawn (render)
- `chimera/present/src/render/spray.rs`, `spray.wgsl`: players' sprays drawn from the install's logos (render)
- `chimera/present/src/render/monitor.rs`: func_monitor screens drawn from point_camera views (render)
- `chimera/present/src/render/shadows.rs`, `shadow.wgsl`: dynamic shadows as the shadow-detail setting asks (render)
- `chimera/present/src/render/sky.rs`: the sky on the cameras, the 3D skybox's camera, the sky resources (render)
- `chimera/present/src/render/world_material.rs`, `world.wgsl`: world surfaces: texture, baked light, bump, cubemap reflections (render)
- `chimera/present/src/render/prop_material.rs`, `prop.wgsl`: props lit by a light probe, plus point lights (render)
- `chimera/present/src/render/fog.rs`, `fog.wgsl`: Source's range and height fog shared by map shaders; the scene's fog (render)
- `chimera/present/src/render/probe_lit.rs`: moving models lit from the baked light field (render)
- `chimera/present/src/render/rope_material.rs`, `rope.wgsl`: ropes as camera-facing strips (render)
- `chimera/present/src/render/sprite_material.rs`, `sprite.wgsl`: sprites and glows added or blended in gamma space (render)
- `chimera/present/src/render/materials.rs`, `images.rs`: materials, meshes and images built from map data (render)
- `chimera/present/src/render/bodies.rs`: drawn character bodies, held models, a ragdoll's drawn body (render)
- `chimera/present/src/render/breakables.rs`: drawing gibs and windows' unbroken panes (render)
- `chimera/present/src/render/merge.rs`: brush entities drawn merged while at their spawn pose (render)
- `chimera/present/src/render/props.rs`: placed props' skins and body groups as drawn (render)
- `chimera/present/src/render/loose.rs`: drawn loose and shown items (render)
- `chimera/present/src/render/view_model.rs`: drawing view models, their screens and muzzle flashes (render)
- `chimera/present/src/render/vis.rs`: visibility culling, areaportal windows' fade, the culling cvars and stats (render)
- `chimera/present/src/render/water.rs`, `water.wgsl`, `screen_warp.wgsl`: drawing water: refraction, reflection, fog, overlays (render)

### Audio (crate `chimera_present`: presentation beside the simulation)

- `chimera/present/src/audio/mod.rs`, `chimera/present/src/audio/sound.rs`: `SoundPlugin`, one-shot playback, the listener (audio)
- `chimera/present/src/audio/live_sound.rs`: long-lived sounds started and stopped by `map::SoundControl` (audio)
- `chimera/present/src/audio/room.rs`: room DSP presets and the shared reverb bus (audio)
- `chimera/present/src/audio/soundscape.rs`: soundscape selection and playback (audio)
- `chimera/present/src/audio/hearing.rs`: blast and flash muffle and ringing, the underwater muffle (audio)

### Simulation above map (logic, weapon, objectives, rules, bot)

- `source/logic/src/logic/`: Source map logic (crate `source_logic`): entity I/O, triggers, movers, breakables, props, fire, the bridge (logic)
- `chimera/sim/src/weapon/`: weapons as parts, inventory, the weapon frame, economy, grenades, drops, lag compensation, sprays (weapon)
- `chimera/sim/src/weapon/data.rs`: weapons as TOML data built into the parts; hooks by name (docs/weapon-data.md) (weapon)
- `chimera/sim/src/weapon/economy_data.rs`: the shop as TOML data: items, buy menu, spawn kit, shop sounds (docs/economy-data.md) (weapon)
- `chimera/sim/src/weapon/pass_data.rs`: penetration by surface material as TOML data (`PassMaterials::parse`) (weapon)
- `games/cs_source/src/objectives/`: round objectives: bomb, hostages, zones, the use search (objectives)
- `games/cs_source/src/rules.rs`: deathmatch and round rules: death, respawn, rounds, teams, score (rules)
- `games/cs_source/src/rules/hints.rs`: automatic hints (`cl_autohelp`): data-named triggers, once per player per map (rules)
- `chimera/sim/src/rule_module.rs`: rule modules a game registers by name; `mashup_rules`; `rules::module` (rule_module)
- `games/cs_source/src/bot/`: bots: aim, navigation, team plans, radio, buying, grenades, objectives (bot)

### Network (net)

- `chimera/net/src/net/mod.rs`: `NetPlugin`, the protocol, replicated components (net)
- `chimera/net/src/net/parts.rs`: what games and Source's logic add: parts, join, bot and hold hooks (net)
- `chimera/net/src/net/server.rs`: the server: listening, the join handshake, update pacing, own states (net)
- `chimera/net/src/net/client.rs`: the client: connecting, joining, the welcome (net)
- `chimera/net/src/net/predict.rs`: the client's command clock and prediction (net)
- `chimera/net/src/net/interp.rs`: others drawn in the past from snapshots (net)
- `chimera/net/src/net/ordered.rs`: replicated values applied in tick order (net)
- `chimera/net/src/net/delta.rs`: byte deltas (net)
- `chimera/net/src/net/quant.rs`: `NetBody`'s, `NetProp`'s and `NetItem`'s quantized wire forms (net)
- `chimera/net/src/net/upnp.rs`: UPnP port mapping while hosting, `net_upnp` (net)
- `chimera/net/src/net/voice.rs`: voice chat's relay: who hears whom (a game's hook), `sv_voiceenable` (net)
- `source/logic/src/logic/net.rs`: Source's logic's part of the network: movers, HUD messages (logic)
- `games/cs_source/src/game/net.rs`: CS:S's part of the network: round, bomb, hostages, join and bot hooks (games)
- `apps/cs_source/src/net.rs`: `mashup::net`: `chimera_net` with the parts' replicated types (net)
- `chimera/net/src/net/props.rs`: physics props over the network (net)
- `chimera/net/src/net/ragdolls.rs`: map-placed ragdolls over the network (net)
- `chimera/net/src/net/anchors.rs`: placed weapons' anchor nodes over the network (net)
- `chimera/net/src/net/weapons.rs`: held weapons, loose items, grenades and weapon events over the network (net)
- `chimera/net/src/net/game.rs`: scoreboard, buying and the rules' sounds over the network (net)
- `chimera/net/src/net/chat.rs`: chat and radio over the network (net)
- `chimera/net/src/net/cheats.rs`: cheat commands from clients (net)
- `chimera/net/src/net/cvars.rs`: replicated cvars (net)
- `chimera/net/src/net/mapfx.rs`: map effects for clients (net)
- `chimera/net/src/net/decals.rs`: decals for late joiners, players' sprays (net)
- `chimera/net/src/net/motd.rs`: the server's message of the day (net)
- `chimera/net/src/net/udp.rs`: the UDP transports and fake network conditions (net)
- `chimera/net/src/net/query.rs`: server queries and LAN discovery (net)
- `chimera/net/src/net/memory.rs`: an in-memory transport for `NetSim` (net)
- `chimera/net/src/net/stats.rs`: `net_graph`'s numbers: traffic, the server's frame rate, choke (net)
- `chimera/net/src/net/vote.rs`: player votes: issues games register, who votes, quorum, timers, `callvote`/`vote` (net)

### Games

- `apps/cs_source/src/games/mod.rs`: the `load_map("game:name")` dispatcher (games)
- `games/cs_source/src/game/rules.rs`: CS:S's rule modules (games)
- `games/cs_source/src/game/votes.rs`: CS:S's vote issues: kick, change level, restart game; how bots vote (games)
- `games/cs_source/src/game/`: Counter-Strike: Source from the install: BSP, materials, models, sounds, movement, weapons, HUD, menus (games)
- `games/cs_source/data/weapons/`: CS:S's weapons and ammo types as data (our numbers, built in) (games)
- `games/cs_source/data/`: CS:S's shop (`economy.toml`), penetration and round numbers as data (our numbers, built in) (games)

### Source formats (crate `source_formats`, `source/formats/`)

- `source/formats/src/fs.rs`: the VFS core: `FileSource`, `Mount` (layers in order), `LooseDir`, `normalize` (re-exported by `mount`)
- `source/formats/src/keyvalues.rs`: KeyValues: one tokenizer (`Escapes::On`/`Off`), the `Kv` tree, `parse`/`parse_res`/`parse_pc`
- `source/formats/src/localization.rs`: localization files (UTF-16/UTF-8, their own escapes)
- `source/formats/src/vpk.rs`: VPK archives (v1, v2) as a `FileSource`
- `source/formats/src/pak.rs`: a BSP's pakfile (zip; stored and LZMA entries)
- `source/formats/src/lumps.rs`: LZMA-compressed BSP lumps inflated before anything reads the file
- `source/formats/src/detail.rs`: the `dprp` detail prop lump
- `source/formats/src/vhv.rs`: static props' baked per-vertex light (`.vhv`)
- `source/formats/src/phy.rs`: collision models (`.phy`): solids, ragdolls, break blocks
- `source/formats/src/pcf.rs`: particle files (binary DMX) and particle manifests
- `source/formats/src/nav.rs`: `.nav` file records (linked into a `NavMesh` by `games/cs_source/src/game/nav.rs`)
- `source/formats/src/wav.rs`: WAV (PCM, MS-ADPCM) and MP3 decoding to `SoundClip`
- `source/formats/src/dem/`: Source demos (`.dem`): frames, messages, tables, events, sounds, props, user commands, playback, a test writer

### Apps, binaries and the test harness

- `apps/cs_source/src/lib.rs`: `SimPlugins` (with the version), `client_hooks`, the harness's old paths (crate `cs_source_app`'s root)
- `apps/cs_source/build.rs`: stamps the build's commit and version (`core::VERSION`, `core::BUILD`)
- `src/lib.rs`: `mashup`: `cs_source_app` at the old paths, for `dump` and its test (crate root)
- `apps/cs_source/src/main.rs`: the game binary (top)
- `chimera/world/src/harness.rs`: `Sim`: a headless app on a given base stepped by exact ticks, for tests and tools (harness)
- `chimera/net/src/harness.rs`: `NetSim`: a server and clients, each a `Sim`, over the in-memory link (harness)
- `games/cs_source/src/harness.rs`: `CsSim` (CS:S's simulation as a `Sim`'s base) and maps for network tests (harness)
- `games/cs_source/src/lib.rs`: `SimPlugins` (CS:S's simulation), `swap_map` (crate `cs_source`'s root)
- `chimera/sim/src/lib.rs`: `SimBasePlugins`: the engine's simulation without a game (crate `chimera_sim`'s root)
- `checks/src/affected.rs`: which packages' tests a change can affect, through cargo's crate graph (repo_checks)
- `checks/src/bin/affected.rs`: dev tool: print the affected tests' `cargo nextest` arguments (the git hooks)
- `apps/cs_source_server/src/main.rs`: the dedicated server, built without the presentation (top)
- `src/bin/dump.rs`: dev tool: summarize, list and extract a game install's files (CS:S, local Combat Arms)

### Tools (each layer's, `--features tools`)

- `chimera/tools/src/perfgate.rs`: the instructions-per-tick regression gate; games name the scenarios
- `chimera/tools/src/imgcmp.rs`: a reference picture and ours compared: stats, detail correlation, sheets
- `chimera/tools/src/bin/tracesum.rs`: dev tool: summarize a Chrome trace
- `source/tools/src/rcon.rs`: Source RCON client, for tools driving a real Source server
- `source/tools/src/bin/demodump.rs`: dev tool: print a Source demo's events, user messages, sounds and entity props
- `source/tools/src/bin/testmap.rs`: dev tool: write the entity test map
- `games/cs_source/tools/src/bin/refcmp.rs`: dev tool: compare views against real CS:S (its reference driver)
- `games/cs_source/tools/src/hudcmp.rs`: `refcmp hudcmp`: the HUD compared panel by panel with CS:S's
- `games/cs_source/tools/src/bin/movecmp.rs`: dev tool: Source movement compared tick for tick with CS:S's
- `games/cs_source/tools/src/bin/demosim.rs`: dev tool: a client demo's recorded input re-simulated and compared (`game/resim.rs`)
- `games/cs_source/tools/src/bin/mapsweep.rs`: dev tool: load every cached map headless and report problems
- `games/cs_source/tools/src/bin/perfgate.rs`: dev tool: CS:S's scenarios on the perf gate

### Client (window, input, camera, HUD, menus, debug)

- `chimera/client/src/client/`: the shell's plugins (`ClientPlugins`), command line, local player's camera (client)
- `apps/cs_source/src/client.rs`: the shell at its old path, the app's `ClientPlugin` with CS:S's HUD (crate root)
- `chimera/client/src/client/front_end.rs`: how the shell reaches a game's menus: `FrontEnd` hooks, `MenuStatus`, `Panels` (client)
- `games/cs_source/src/ui/`: CS:S's front end on the shell (feature `ui`), `CsSourceUiPlugin`, `with_hud` (games_ui)
- `chimera/client/src/client/net.rs`: the game's side of the network: joining, dropping, hosting (client)
- `chimera/client/src/client/view.rs`: third-person, free and chase cameras (client)
- `chimera/client/src/client/spectate.rs`: spectating while dead or on the spectator team: state and camera (client)
- `games/cs_source/src/ui/spectator_hud.rs`: the spectator's bars and panel (games_ui)
- `games/cs_source/src/ui/freeze_cam.rs`: the freeze cam's picture (games_ui)
- `chimera/client/src/client/senses.rs`, `flash.wgsl`: flashes and blasts on the local player's senses (client)
- `chimera/client/src/client/hdr.rs`, `hdr_tonemap.wgsl`: `mat_hdr_level` and the tonemapper (client)
- `chimera/client/src/client/gamma.rs`, `gamma.wgsl`: `mat_monitorgamma` (client)
- `chimera/client/src/client/night_vision.rs`, `night_vision.wgsl`: night vision goggles turned on and their green view (client)
- `chimera/client/src/client/color_correction.rs`, `color_correction.wgsl`: `mat_colorcorrection` on the finished frame (client)
- `chimera/client/src/client/audio.rs`: the options' sound settings (client)
- `chimera/client/src/client/binds.rs`: every game key is a bind; default binds (the default game's) (client)
- `chimera/client/src/client/video.rs`: field of view, video and texture settings (client)
- `chimera/client/src/client/voice.rs`: voice chat: `+voicerecord`, microphone, Opus, playback (client)
- `games/cs_source/src/ui/options.rs`: the options dialogs' settings (games_ui)
- `chimera/client/src/client/console.rs`: the in-game console UI (client)
- `chimera/client/src/client/debug_ui.rs`: the debug UI (F8) (client)
- `chimera/client/src/client/debug_views.rs`: gizmo debug overlays: nav, ragdolls (client)
- `games/cs_source/src/ui/debug.rs`: CS:S in the debug tools: bot overlays, the debug UI's bots and rounds (games_ui)
- `chimera/client/src/client/perf.rs`: the `mashup_perf` overlay (client)
- `chimera/client/src/client/net_graph.rs`: `net_graph`: Source's network readout and graph (client)
- `chimera/client/src/client/frame_metrics.rs`: per-frame metrics of both worlds (client)
- `games/cs_source/src/ui/window_icon.rs`: the window and taskbar icon (games_ui)
- `games/cs_source/src/ui/first_run.rs`: finding CS:S on first run (games_ui)
- `chimera/client/src/client/fonts.rs`: the one font service (client)
- `chimera/client/src/client/widgets.rs`: VGUI control state machines and drawing (client)
- `chimera/client/src/client/vgui.rs`: drawing a game's own menu layouts; the mouse free while one is open (client)
- `games/cs_source/src/ui/game_menu/`: the main menu and game menu dialogs, the vote setup dialog (games_ui)
- `games/cs_source/src/ui/server_browser.rs`: the server browser (games_ui)
- `games/cs_source/src/ui/team_menu.rs`: the team menu (games_ui)
- `games/cs_source/src/ui/join_menus.rs`: CS:S's join flow: MOTD, team and class menus (games_ui)
- `games/cs_source/src/ui/buy_menu.rs`: the buy menu (games_ui)
- `games/cs_source/src/ui/radio.rs`: the team radio menus and calls (games_ui)
- `chimera/client/src/client/chat.rs`: the chat area and hint text (client)
- `games/cs_source/src/ui/scoreboard.rs`: the scoreboard (games_ui)
- `games/cs_source/src/ui/win_panel.rs`: the round's win panel (games_ui)
- `games/cs_source/src/ui/vote_hud.rs`: the vote panels: the running vote, its outcome, a refused call (games_ui)
- `games/cs_source/src/ui/spectator_menu.rs`: the spectator menu (games_ui)
- `games/cs_source/src/ui/overview.rs`: the spectators' map overview (games_ui)
- `chimera/client/src/client/hud.rs`: the fallback HUD: crosshair, scope, tints, health, ammo, killfeed (client)
- `games/cs_source/src/ui/game_hud.rs`: the game's own HUD panels (games_ui)
- `chimera/client/src/client/hud_text.rs`, `hud_text.wgsl`: HUD text drawn as Source draws it (client)
- `games/cs_source/src/ui/hud_sprites.rs`: HUD sprite icons and damage indicators (games_ui)
- `games/cs_source/src/ui/weapon_select.rs`: weapon selection and pickup history (games_ui)
- `chimera/client/src/client/weapon_menu.rs`: a HUD weapon selection's state, which input reads (client)
- `games/cs_source/src/ui/objectives_hud.rs`: bomb, kit, hostage and zone icons, centre prints on the HUD (games_ui)
- `games/cs_source/src/ui/hints.rs`: the automatic hints' text (the install's strings) and sound (games_ui)
- `games/cs_source/src/ui/bomb_fx.rs`: what the bomb shows: the code on the C4 screen, the LED glow (games_ui)
- `games/cs_source/src/ui/radar.rs`, `radar.wgsl`: the radar (games_ui)
- `games/cs_source/src/ui/demo.rs`: `playdemo`: a demo's map loaded, then played with the spectator camera (games_ui)
- `games/cs_source/src/ui/target_id.rs`: the target ID under the crosshair (games_ui)
- `games/cs_source/src/ui/voice_hud.rs`: who is talking: the voice icon and list (games_ui)
- `chimera/client/src/client/game_text.rs`: map HUD messages (game_text) (client)
- `chimera/client/src/client/map_screen.rs`: map fades, hints and screen overlays (client)

## Data flow per tick

```
PreUpdate:    (server) UserCmds from clients ──> CommandBuffer (by tick), DropRequest ──> drop;
              (client) replication ──> NetBody, NetMover, NetProp, NetItem and their Snapshots (by server
              tick); ticks heard ──> InterpClock, held snapshots; our NetBody ──> Transform (until
              predicted); OwnState ──> pending; NetHeld ──> others' held weapon; FireBullets ──>
              RemoteShot; WeaponFx, Killed, HitConfirm, Detonation, Senses ──> weapon events, Died,
              Detonated, Blinded, Deafened; NetSmoke ──> SmokeCloud
Update:       keyboard/mouse ──> Intent (local player)
FixedFirst:   (client) drawn transforms ──> the simulation's (map::interp::restore); command tick + SimClock;
              server state vs. predicted: on a mismatch restore it, re-run later commands (core::predict)
FixedUpdate:  bot brains, (server) each player's command for this tick ──> Intent, its view tick ──>
              lagcomp::ViewTick (before the rules, which hold it); (client) local Intent normalized
              (+ the render tick) ──> this tick's NetCmd
              SimSet::Rules     rounds (new round ──> RoundRestarts, objectives::round_start;
                                bomb/hostage state ──> round end; FreezeTime, RoundOpen),
                                respawn; the dead's and (freeze time) everyone's Intent held
              objectives        arming/defusing hold still; hostages follow ──> Intent
              MapControls ──> Intent (taken buttons, at controls, frozen; the buttons kept for game_ui)
              weapon selection/deploy ──> MaxSpeed           (core::Predict::Select)
              RoundRestarts changed ──> logic world re-created, nodes back; gibs, panes reset
              LogicSet::Pre     thinks, movers push/carry characters, +use (Intent.use_key);
                                (client) movers stepped to the command tick, our player carried
                                (logic::net::place, also in each replayed command)
              SimSet::Commands  Intent.command stamped (tick + Seed)
              SimSet::Movement  Intent (+ MaxSpeed) ──> Transform, Velocity, MovementState  (Predict::Movement)
              AnchorSet         placed weapons (loose or carried) ──> EntityAnchors, anchor nodes
              LogicSet::Post    trigger touches (push ──> BaseVelocity, teleport), untouch,
                                event queue (outputs ──> inputs), removals; effects ──> Damage, HUD text
              SimSet::Weapons   Intent + MovementState ──> traces, Damage, WeaponEvent, PlaySound
                                (inaccuracy, frame, recoil, timed sounds, zoom, grenade throws:
                                Predict::Weapons; (server) a remote player's traces against others
                                rewound to its view tick from HitHistory: weapon::lagcomp)
                                then the bomb: arming, plant, defuse (use search), beeps,
                                Explosion, FireEntityOutput; +use on hostages ──> ObjectiveEvent
              LogicSet::Damage  Damage on breakable and prop nodes ──> break, panes, gibs, sounds,
                                Explosion (next tick, in SimSet::Weapons)
              then              Damage ──> Health, Died ──> Score, Dead
                                WeaponEvent, Damage ──> impact sounds, decals, Particles (game effects)
FixedPostUpdate: physics step; new contacts of props ──> impact Damage (crush, next tick)
FixedLast:    (client) Transform, eye, look, punch ──> interpolation samples (last two ticks); predicted
              state ──> PredictionHistory; (server) characters ──> NetBody, NetHeld, OwnState (with
              what they carry); bodies animated from the tick's values (SimAnimator) ──> Hitboxes
              (map::SimPose) ──> HitHistory; movers ──> NetMover; physics props ──> NetProp; loose
              weapons, grenades ──> NetItem
after fixed loop: (client) samples + overstep fraction ──> drawn Transform, RenderedView; others,
              physics props and items (NetDrawn) from their snapshots at the render tick (net::interp,
              net::props, net::weapons)
Update:       (client) RemoteShot ──> others' rounds traced again (no damage) ──> WeaponEvent, PlaySound
PostUpdate:   (server) changes replicated (a mutate message with each client's updates: UpdatePacing),
              OwnStateDelta to each player due one;
              WeaponEvent ──> FireBullets, WeaponFx (to the others), HitConfirm (to the shooter);
              Detonated, Died, Blinded/Deafened ──> Detonation, Killed, Senses;
              (client) new NetCmds + backups ──> UserCmds; drop ──> DropRequest
              PlaySound ──> one-shots; SoundControl ──> LiveSounds (ambient loops) ──> audio
Update:       camera <── Intent (local look) + drawn Transform + RenderedView (eye offset, roll, punch)
              Particles stepped (frame time, at most 0.1 s) and drawn
              DriveAnimation  WeaponEvent ──> Animator (bodies), ViewAnimator (view models) ──> joints
                              view-model animation events ──> ViewModelEvent ──> flash, light, shells
                              Intent + Velocity ──> ViewModelOffset (bob, sway)
```

Intent buttons are levels, not edges. Movement runs at a fixed tick
(`DEFAULT_TICK_HZ` until a spec sets one); rendering draws between the last
two ticks (`map::interp`): gameplay, traces and shots use the simulation's
values, cameras, bodies and their animation the drawn ones.

## Slots

An implementation registers under a namespaced ID (`mashup:noclip`,
`combat_arms:movement`) with a marker component. Its systems act only on
entities with that marker, so swapping is removing one marker and inserting
another (`slots::set_movement`). Only Movement exists so far; the other slots
follow the same pattern when they arrive.
