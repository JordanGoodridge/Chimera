# Plan: engine, Source and games as crates in one monorepo

Status: decided 2026-10-07: one private monorepo (`r0pi0x/LuckerParty`,
history rewritten to the new identity), restructured into the crates
below. Separate repos only if a good reason comes up later; crates keep
that a folder move. Nothing moved yet.

## Why

- Other people (a Discord server sharing game mashups) want to build on the
  engine: a Half-Life 2 group now, more games later. CS:S should be
  playable and buildable by others too.
- Lucker Party will move onto this engine, with friends writing original
  minigames.
- Before anything is shared, the work moves to an anonymous account
  (rewritten history; see "Order").

## Target layout

```
engine/              neutral: app shell, console, slots/mounts, physics,
                     movement framework, weapons/rounds/bots, HUD, net
source/              Source engine formats and shared Source behaviour:
                     BSP/VPK/VTF/VMT/MDL/PHY, entity I/O and logic classes,
                     soundscapes, Source materials and shaders
games/cs_source/     CS:S: library (plugins) + standalone app
games/combat_arms/   Combat Arms: library + standalone app (local, git-ignored for now)
games/originals/     Lucker Party's own games and assets
minigames/           minigame definitions + rule modules that mix games
apps/mashup/         dev sandbox: any game, any map, all debug tools
apps/lucker_party/   the party product
```

Rules:
- `engine` knows no game. `source` depends on `engine` only. Games depend
  on `engine` (and `source` if they're Source games), never on each other.
- Apps pick games with cargo features. A game that needs something the
  engine can't express gets a new engine abstraction, not a special case.
- Each game crate is both a library and a program: `cargo run -p
  cs_source` is a faithful CS:S (its menus, HUD, modes, loaded from the
  player's install); the same library supplies parts to minigames. A
  game's own modes are minigame definitions, so both uses run the same
  code.
- `tests/it/architecture.rs` already enforces most of this as layers;
  crates make the compiler enforce it.

## Crates and compile time (agreed 2026-10-13)

The layout above stays the outer structure (ownership and reuse); inside
it, crates are cut so that hot code sits high in the dependency graph
(architecture-review.md measured why: one crate means every edit
recompiles all 165k lines, and one `engine` crate at the bottom wouldn't
help).

```
engine/   game-agnostic, several crates with stable interfaces
  core      core, console, slots/registries, version, metrics
  world     map data, collision, sim-side spawning
  sim       frameworks: movement slot, weapon system, rule-module interface, bots framework
  present   render, audio, animation (a dedicated server can leave it out)
  net       replication and protocol; games register what replicates
  client    app shell, input, console UI, a VGUI-like widget layer, debug tools
source/
  formats   VPK/BSP/MDL/PHY/PCF/KeyValues/.res: pure data, no Bevy
  source    entity I/O and logic classes, Source materials/shaders, soundscapes, Source movement
games/cs_source/   parts (weapons, rule modules, HUD, GameUI menus, bot behaviour)
                   + CsSourcePlugins (the faithful game) + its app
minigames/, games/originals/, apps/*, tools/ (debug binaries: not built by `cargo test`)
```

- Parts depend on engine (and source) interfaces only, never on each
  other: CS:S weapons don't assume the CS:S HUD or rounds. Each part
  plugs into a slot (movement, weapons, character, HUD, rules, menus).
  The architecture test enforces it.
- Today's engine-level modules that are really CS:S parts move to
  `games/cs_source` first: the GameUI menus, team/buy menus and HUD
  pieces in `client` (8 files carry `#Cstrike_*`/`cstrike/`), the round,
  bomb and hostage rules in `rules`/`objectives` (behind a rule-module
  interface), and CS:S round state in `net` (game-registered
  replication). This is also the compile win: the hot code moves up.
- **Data-driven first** (user, 2026-10-13): where a thing can be data,
  make it data loaded at run time, with Rust only for behaviour data
  can't express (then as a hook the data names). First target: weapons
  (stats, model, sounds, animations, fire modes as data; special
  behaviour as registered hooks), then rule modules' settings, HUD
  layouts, minigame definitions (below), bots' buy tables.
- Example of what this must support later (not planned now): a game
  crate built on top of `cs_source` that adds original guns and modes
  ("CS:S, our way") as its own ids and data, without touching
  `cs_source`; the faithful game stays the parity baseline and its tests
  run against `cs_source` alone.

Progress (2026-10-13, one session; architecture-review.md has the
measurements):

- [x] (1) Tools out of the test build: no `tools/` package yet; the
  binaries no test runs are behind `required-features` (`tools`: refcmp,
  movecmp, perfgate, tracesum, mapsweep; `server`: mashup_server). A
  leaf edit's test build went from ~228 s to ~127 s (medians, load
  16-36). A `tools/` package comes with the crate split.
- [x] (2) `map/mod.rs` split (P1): `data` (MapData, plain data), `state`
  (what the logic writes), `spawn` (`spawn_map` as ~25 phase functions),
  `bodies`, `props`, `materials`, `images`, `collision`, `sky`; paths
  outside `map` unchanged. `tests/it/heavy/map_spawn_dump.rs` dumps
  spawned maps for before/after comparisons.
- [x] (2) `audio` out of `map` (P3, first slice): playback, live sounds,
  room DSP, soundscapes, hearing; `map::sound` keeps the simulation's
  side. `map_plugin.rs` builds `map::MapPlugin` (map, then audio, in the
  old order); the architecture test keeps map, logic and net off audio.
- [x] (2) `anim`: decided (coordinator, 2026-10-13): animation
  evaluation is simulation (the server poses hitboxes, as Source's
  server-side bones), so `map::anim`, `bodies::SimAnimator` and the
  ragdolls stay on the sim side; only drawing and skinning is
  presentation and moves with `render`.
- [x] (2) `render` out of `map` (2026-10-13 and 14, three sessions,
  each commit with the spawn dump identical on the five maps modulo
  moved paths; left: `render` behind a feature for the server, and the
  crate cut, both below):
  - [x] the state `logic`/`net`/`weapon` write moved to `map::state`
    (`VisClusters`, `LogicHidden`, areaportal/occluder states,
    `RenderLook`/`BrushTint`, `MonitorCamera`/`MonitorFog`, the
    effects' messages, `PlaceDecal`, `ParticleRng`): those layers use no
    drawing module of map's.
  - [x] `render/` with the drawn phases of spawning: `map::spawn_map`
    is generic over `map::MapDrawer` (a system parameter called at the
    old places: drawn parts, each prop's drawn parts, textures for the
    resources), `render::MapAssets` implements it; `map_plugin` adds
    `spawn_map::<render::MapAssets>` and `MapHooks::spawn` (for
    `change_map`; was `MapUnloadHooks`).
  - [x] map's system registration (`build_rest`) lives in `map_plugin`
    (order kept); `tint`, `detail`, `sky_occluder` moved whole; `beams`,
    `color_correction`, `lod` split (data stays in map, drawing in
    render).
  - [~] The rest, roughly in this order (each needs its data types left
    in map and its users in map's sim side cut first): glow sprites
    (`GlowSprite`, `glow_visibility`: `vis` and `beams` use them),
    `psys`, `shells` and `rope_sway` (data split), `fish`, `dust`,
    `steam`, `tracer`, `emitters`, `particles` drawing, `decal`
    drawing, `monitor`, `water` drawing, `shadows` (`props.rs`'s
    `update_prop_shadows` with it), `vis` culling (`cull`,
    `fade_windows`, chunking; `copies`, `merge`, `breakables`,
    `prop_physics` name its tags), `sky` drawing, `light_styles`,
    `view_model` drawing, then the materials and WGSL last
    (`world_material`, `prop_material`, `sprite_material`,
    `rope_material`, `fog`, `probe_lit`, `materials.rs`/`images.rs`
    helpers: `breakables`, `loose`, `bodies`, `ragdoll`, `view_model`
    use them, so their drawn parts have to move first, through more
    `MapDrawer` hooks where they spawn). Render's resources and entities
    go at unload through `render::unload_map` (a `MapHooks::unload`
    hook, so they now go after map's own). Done (2026-10-14, one commit
    per group, the spawn dump identical modulo moved paths):
    - [x] `glow_visibility` (`render::glow`; `GlowSprite` stays in map
      for `vis`), `psys` (definitions stay; `Instance`, `SystemEmitter`,
      `update_systems` moved), `shells` (types and `shell_step` stay;
      assets, `eject`, `fly` moved), `rope_sway` (`RopeSway`, `spline`
      stay; `Gusts`, `RopeMotion`, `RopeWind`, `sway_ropes` moved),
      `fish` (`Fish` stays; `swim` moved).
    - [x] `dust` (whole), `steam` (`MapSteam` stays; the headless
      stand-ins through `MapDrawer::spawn_undrawn`), `switch_parts`
      (`render::parts`), `tracer` (`Tracer`, `MuzzleCache` stay),
      `emitters` (the `Map*` data and beam noise stay), `particles`
      drawing (the pool, its stepping and `ParticleMaterials` stay;
      `ParticleAssets` inserted at its old place through
      `MapDrawer::insert_drawn`), `fog::apply_scene_fog` (names the
      particle material; never scheduled).
    - [x] `decal` drawing (`MapDecals`, `LIFT`, `DEPTH_DROP` stay;
      `DecalSurfaces`, `DecalAssets` and `ImpactDecals` inserted at their
      places through `MapDrawer::insert_drawn`), `monitor` (the screen
      cameras' markers and `MonitorFog`'s view fog stay; `MonitorCamera`
      is initialized by map where `MonitorPlugin` was added, the plugin's
      system by render right after).
    - [x] `water` drawing (`MapWaterMaterial`, `WaterSurface`,
      `move_water` and `WaterMovers`, `WaterView` (hearing reads it),
      the reflection camera's markers, the layers and `height_fog` stay;
      the material, cameras, fog, overlays and their systems moved).
    - [x] `shadows` whole, with `props.rs`'s `ShadowState`,
      `update_prop_shadows` and `PropShadow`.
    - [x] `vis` culling: `cull`, `fade_windows` and the culling cvars
      and stats (`NoVis`, `PortalsOpenAll`, `Occlusion`, `VisStats`);
      the visibility data, chunking and the tags (`VisClusters`,
      `FadeDistance`, `Occludee`, `WindowBrush`, `ActiveVisibility`:
      bots and hearing read it) stay.
    - [x] `sky` drawing: `attach_sky`, `follow_sky_camera`,
      `show_skybox_in_place` and the sky resources render inserts; the
      images, `SkyboxCamera` and the layers stay.
    - [x] `light_styles` whole (its texel encoders to `map::images`);
      `MapPlugin::build_first` split at the place its plugin was added
      (`build_after_light_styles`), so registration keeps its order.
    - [x] `view_model` drawing: assets, `draw_view_models`, screens,
      muzzle flashes and their lights, `unload`; the games' types, the
      animator, events, markers (`HeldMuzzle`, which `bodies` spawns) and
      the placement maths (`shells` uses it) stay.
    - [x] `architecture::simulation_uses_no_drawing_module_of_map`: the
      simulation's layers name none of the materials left in map
      (`MAP_DRAWING`; one exception, `SpriteBlend`).
    - [x] Drawer hooks that run after spawning (2026-10-14, one commit
      per group, the spawn dump identical modulo moved paths; component
      ids and the change tick masked where a system or component was
      added): drawn-part requests (docs/ARCHITECTURE.md): what the
      simulation makes in play says in plain data what it looks like
      and render's systems fulfil it; headless nothing is drawn
      (`tests/it/drawn_parts.rs`). `render::bodies` (bodies, held
      models, the ragdoll's adoption of the dead's body), `render::loose`
      (`LooseItem`, `ShownItem`), `render::breakables` (a gib's
      `GibLook`, windows' `BrushPanes` clipped and made whole),
      `render::props` (`PropLook` on skins and body groups). Resource
      order kept: the gibs' meshes live in `render::bodies::PlayModels`
      (was `CharacterBodies`), built in the old order and inserted at
      the bodies' place; `MapPlugin::build_first` split where the
      ragdoll's drawn systems were added.
    - [x] The materials and WGSL: `world_material`, `prop_material`,
      `sprite_material`, `rope_material`, `probe_lit`, `fog` (merged
      with `render::fog`), `materials.rs`/`images.rs` and their shaders
      are render's; `SpriteBlend` went to `map::data`,
      `parameter_gamma_to_linear` to `map::material_fx`, the water fog
      modes to `map::water`; `SceneFog` is inserted through
      `MapDrawer::insert_drawn` (`DrawnResource::Fog`) at its old place.
      `architecture::simulation_builds_no_drawn_parts` replaces the
      `MAP_DRAWING` check: the simulation's layers name no drawing type
      (`Mesh3d`, `Assets<Mesh>`, materials) and `src/map` holds no
      shader.
    - [x] `merge` whole (a node copied at run time is finished by a
      `MapHooks::copied` hook: render shows its pieces, never merged)
      and the placed ragdoll's never-culled meshes
      (`render::bodies::uncull_placed_ragdolls`): the simulation's
      layers name no drawing type, with no exceptions.
    Client post passes (`client::hdr`, `gamma`,
    `color_correction`) stay in client: they hang off
    `FirstPersonCamera`/`UiCamera`/`ClientArgs`.
  - A server without rendering: not yet. `mashup_server` adds no
    material plugins but links `render` through `MapPlugin` (its spawn
    drawer and the moved systems run headless with no asset stores, as
    before). Feasible once `build_rest` is split into map's systems and
    render's (an ordering change to make deliberately) and a headless
    `MapDrawer` (all `Draws` false, nothing spawned) stands in for
    `render::MapAssets`; then `render` can sit behind a feature.
    The drawn-part requests already leave such a server correct: with
    no renderer they stay unfulfilled (`tests/it/drawn_parts.rs`).
  - [x] Cutting the crates (`map` to `engine/world`, `render` to
    `engine/present`). Done (2026-10-14): `engine/world` (package
    `engine_world`, module `map`): `MapPlugin`'s type moved up into
    `map_plugin` beside its `impl Plugin` (re-exported as
    `map::MapPlugin`; map's part is `map::build_first`,
    `build_after_ragdolls`, `build_after_light_styles`), map's
    `pub(crate)` items became `pub`, `impl MapBeam`'s drawing maths split
    (the data's in map, `render::beams::beam_params` in render), a
    `#[cfg(test)]` stand-in inlined into render's test; spawn dump
    identical on the five maps modulo moved paths (no masking needed).
    Then `engine/present` (package `engine_present`): `render`, `audio`
    and `map_plugin` (`MapPlugin`, `pub use`d by `mashup::map`) moved
    whole; their `pub(crate)` items became `pub`, the embedded shaders'
    paths are `embedded://engine_present/render/...`; `mashup`
    re-exports `render` and `audio`. Spawn dump identical again (five
    maps, modulo paths), `tests/it/drawn_parts.rs` passes, de_dust2 and
    gg_tbr_water_basin screenshots draw as before. Measured
    (docs/performance.md, "`engine_world` and `engine_present`"): a
    leaf edit in render or map, `cargo test --features dev --no-run`,
    30-72 s (was 210 s); in a game file still 127-176 s (mashup's 134k
    lines); `cargo test -p engine_world` 9 s, `-p engine_present` 12 s.
    Left: `render` behind a feature for the server (above), the
    simulation's other layers (`engine/sim`), `mashup` itself.
    What it needed (2026-10-14): (a)
    `impl Plugin for MapPlugin` lives in `map_plugin` (root crate) for a
    type in `map`: the orphan rule forbids that across crates, so the
    type moves up beside its registration (e.g. `present::MapPlugin`,
    re-exported as `map::MapPlugin`), or `world` gets its own plugin
    with registration hooks that `present` fills in the same order;
    (b) about 50 `pub(crate)` items of `map` (and more `pub(crate)`
    fields: `LooseAsset`, `GibAssets`, `FlyingGib`, `Ragdoll`,
    `PendingMap`, `PlayableArea`, the props' and bodies' types) that
    `render`/`map_plugin`/`audio` use become `pub`; (c) `map` uses only
    `core` (`engine_core`) of ours, plus avian, replicon and Bevy's
    image and camera types (`map::sky`'s images, the camera markers):
    no blocker; (d) the root crate re-exports `world::map` and
    `present::render` so `crate::map`/`mashup::map` paths keep working;
    (e) `audio` (above `map`, beside `render`) goes with `present`.
  - Measured: a leaf edit's test build (`client/window_icon.rs`, a new
    `pub fn`, `cargo test --features dev --no-run`) 239 s before
    (load 14-26), 114 s after (load 17-18): one crate either way, so
    the difference is load and cache, not the move; the win comes with
    the crate cut.
- [x] Docs diet (P2): docs/ARCHITECTURE.md's module table became one
  line per module (133 KB to 22 KB) and docs/tech-debt.md one line per
  entry (109 KB to 15 KB); the detail moved into the files' `//!` docs
  (debt under "Known shortcuts"). `architecture::docs_stay_one_line_per_entry`
  caps the line lengths and checks the paths.
- [x] Heavy tier (P4b): map-looping heavy tests split per map, nextest
  tuned to 5 heavy slots weighted by memory; full suite 1,812 s to
  1,009 s (docs/performance.md, "The heavy tier").
- [~] (2) CS:S UI out of `client`, rules behind a rule-module interface,
  game-registered replication (2026-10-13, one session):
  - [x] `games/cs_source/ui/` (`CsSourceUiPlugin`, added by main.rs
    after `ClientPlugin`): the GameUI menus and dialogs (`game_menu/`,
    `options` (its rows; the video cvars stay in `client::video`),
    `first_run`, `server_browser`), the join flow (`join_menus`), the
    buy, team, radio and spectator menus, the freeze cam panel
    (`freeze_cam`), `objectives_hud`, `bomb_fx`,
    `target_id`, `scoreboard`, `window_icon`. The shell reaches them
    through `client::front_end`: `FrontEnd` hooks (entered/left a game,
    joining, a failure, a failed map load, starting a game) and the state
    it reads, `MenuStatus` and `Panels`, which the menus copy in
    `FrontEndSync` (First, Update after their systems, PostUpdate; the
    shell's readers saw the menus' resources directly before, so a
    reader in `Update` may see a change a frame later than it did).
    `bot::DIFFICULTIES` moved out of Create Server (the debug UI uses
    it). Architecture test: `games_ui` may use `client`; the rest of a
    game may not use its `ui`; `client` names a game only in
    `CLIENT_GAME_EXCEPTIONS` (console, binds, net glue: the install,
    maps, map cycle and tick of CS:S, waiting for a game registry; the
    debug UI's Source movement cvars).
  - Left in `client`, CS:S-flavoured but on generic data (`map::hud`'s
    layouts): `game_hud`, `hud_sprites`, `win_panel` (CS:S's
    `win_round.res` and strings; `game_hud` spawns it), `weapon_select`
    (its `#Cstrike_WPNHUD_*` table), `spectate` (draws CS:S's
    `spectator.res` bars), `radar`, `overview`, `vgui`
    (its `VguiOpen`/`VguiMenu` name CS:S's menus), `hud_text`, `fonts`.
    Next: the HUD as a game part (a HUD slot), then these.
  - [~] Rule modules: `rules::module` (`RuleModule`: name, description,
    its cvars as data, when it applies; `RuleModules` registry; `pick`;
    `mashup_rules`), CS:S's registered by `games/cs_source/rules.rs`
    (`cs_source:deathmatch`, `:bomb`, `:hostage`, `:elimination`,
    picked by `mashup_rounds` and the map's objectives as the code
    already does). The code did not move: `rules::rounds` and
    `objectives` are named by `net` (`RoundState`, `RoundEnded`,
    `frozen_intent`, `Hostage`, `BombHold`), `bot` (bomb and hostage
    goals, `RoundOpen`), `cs_source::movement`/`player_anim`/`pain`
    (`Hostage`), `rules.rs` itself (deathmatch respawns read the round
    phase) and the client's HUD; each needs an interface first (round
    state as replicated data the module registers; objectives behind a
    bot-goal and use-target interface). Not started: game-registered
    replication (`net` still names the round and objective types).
  - [x] (3) `cs_source::CsSourcePlugins` (movement, weapons with the
    bomb and hostages, rule modules, player and view model animation):
    main.rs and `mashup_server` (without the animation, as before) use
    it; tests still pick plugins one by one.
- [x] (3) First crate (2026-10-14): the repository is a Cargo workspace
  (`mashup` at the root, every member a default member) and
  `source/formats` (package `source_formats`, pure data, no Bevy:
  checked by `architecture::workspace_crates`) holds the VFS core, VPK,
  pakfile, LZMA lumps, `.vhv`, `.phy`, WAV/MP3, the detail lump, DMX/PCF,
  `.nav` records, KeyValues (one tokenizer, explicit escapes: P7) and
  localization files; `mashup` re-exports them at their old paths. A
  format edit is checked with `cargo test -p source_formats` in about a
  second (docs/performance.md, "The first crate"). Not moved yet: BSP
  and MDL conversion (they build engine types; vbsp/vmdl do the
  parsing), surface properties, sound scripts and soundscapes (their
  parsing is mixed with the loading), `.res` layout conversion.
- [x] (3) `engine/core` (package `engine_core`, 2026-10-14): `core`,
  `console`, `slots`, `character`, `metrics` and `core::version`
  (`format_version`) moved whole; they used nothing above each other,
  so no interface was needed. `mashup` re-exports them at their old
  paths; `mashup::core` (src/core.rs) adds `VERSION`, stamped by
  build.rs in the app's crate so a commit doesn't rebuild the base.
  `movement` (placeholder, noclip) stays: it is a slot implementation,
  `engine/sim` material. The layering table covers engine/core/src too;
  `workspace_crates` keeps it off `mashup` and the games. `dev` forwards
  to `engine_core/dev` (same Bevy build for `cargo test -p engine_core
  --features dev`). Measured (a new `pub fn`, load 10-40): an edit in
  `engine_core` checked with `cargo test -p engine_core --features dev`
  (build and run) in 4-7 s; a leaf edit in `mashup`, `cargo test
  --features dev --no-run`, 127-160 s as before (one crate below it
  less, but `mashup` is still ~160k lines); after an `engine_core` edit
  the same command took 28-81 s (`mashup` recompiles incrementally).
- [x] (3) `mashup`'s module graph measured (2026-10-14, after the
  engine crates): every `crate::`/`mashup::` path (grouped imports
  split) from each top-level module to another, comments stripped.
  Lines are the module's own; `cs_ui` is `games/cs_source/ui/`, `engine`
  the modules re-exported from the engine crates (core, console, slots,
  character, metrics, map, render, audio).

  | Module (lines) | Uses (references) | Used by |
  |---|---|---|
  | `logic` (24.6k) | engine only (map 141, core 28, console 5) | net 8, client 8, cs_ui 2, harness 1 |
  | `movement` (0.2k) | engine only | net 6, client 1 |
  | `mount` (0.8k) | nothing of ours (source_formats) | cs_source 10, client 6, cs_ui 6, games 1, bins |
  | `greybox` (0.3k) | engine only | client 3, cs_ui 1, bins |
  | `weapon` (7.8k) | engine only (core 64, map 48, console 13) | cs_source 28, client 32, cs_ui 17, bot 15, net 12, rules 11, objectives 8 |
  | `objectives` (2.2k) | weapon 8, engine | cs_source 11, rules 8, bot 7, cs_ui 7, client 5, net 4 |
  | `rules` (2.0k) | objectives 8, weapon 11, engine | client 33, net 28, cs_ui 22, cs_source 2 |
  | `cs_source` (28.3k) | weapon 28, objectives 11, mount 10, rules 2, engine (map 244) | client 13 (4 files, `CLIENT_GAME_EXCEPTIONS`), cs_ui 9, main, bins |
  | `bot` (7.0k) | weapon 15, objectives 7, engine | net 6, cs_ui 5, client 6 |
  | `net` (13.8k) | rules 28, weapon 12, logic 8, bot 6, movement 6, objectives 4, engine | client 41, cs_ui 35, harness 21 |
  | `client` (23.3k) | everything (map 101, console 92, core 77, net 41, rules 33, weapon 32, cs_source 13) | cs_ui 138, main |
  | `cs_ui` (17.9k) | client 138, map 71, console 41, net 35, rules 22, weapon 17, cs_source 9 | main |
  | `harness` (0.5k) | net 21, lib, engine | tests |

  Self-contained layers on the engine crates (no back-reference to
  cut): `logic`, `movement`, `weapon`, `mount`, `greybox`; then
  `objectives` (on weapon) and `rules` (on objectives, weapon). The
  simulation of CS:S (`cs_source` without `ui/`) uses only those and
  the engine: nothing of `net`, `bot`, `client` or `logic`. So, bottom
  first: `logic` to `source/logic` (needs nothing); `weapon` and
  `movement` to `engine/sim` (nothing); `mount` down to the engine
  (its config's repository path); then `cs_source`'s simulation with
  the CS:S rules (`objectives`, `rules`) to `games/cs_source` (its
  `ui/` stays in `mashup` with the client, re-exported beside it).
  `net` (to `engine/net`) is blocked by naming CS:S's rules (round
  state, bomb, hostages: game-registered replication) and `logic`
  (Source); `bot` by its bomb and hostage goals; `client` by
  `CLIENT_GAME_EXCEPTIONS` (a game registry).
- [x] (3) `source/logic` (package `source_logic`, 2026-10-14): `logic`
  moved whole (it uses only `core`, `console` and `map`); one
  `pub(crate)` item became `pub`; `mashup` re-exports it as
  `mashup::logic`. The layering table covers source/logic/src;
  `workspace_crates` keeps it off `engine_present`, `mashup` and the
  games. Spawn dump identical on the five maps modulo moved paths.
- [x] (3) `engine/sim` (package `engine_sim`, 2026-10-14): `weapon`,
  `movement` (placeholder, noclip) and `rules::module` (as
  `rule_module`: the `RuleModule` registry, `pick`, `mashup_rules`)
  moved whole; they use only the engine crates. `mashup` re-exports
  `weapon` and `movement` at their old paths and `rules::module` as
  before; `rule_module::plugin` became `pub` (rules adds it). Spawn dump
  identical on the five maps modulo moved paths.
- [x] (3) `mount` down into `engine_core` (2026-10-14): the VFS and
  KeyValues re-exports, `mashup.local.toml` and finding installs, so a
  game crate can use it; `mashup::mount` re-exports it. `engine_core`
  now depends on `source_formats` (the VFS, KeyValues) and on `dirs`,
  `toml` and `windows-sys` (moved from `mashup`); the config's
  checkout fallback is the repository root (`CARGO_MANIFEST_DIR/../..`).
  No ECS types, so no spawn dump.
- Order: (1) the `tools` package; (2) inside today's single crate, move
  code to its future home (the `map` sort, CS:S UI out of `client`,
  rules behind a rule-module interface, game-registered replication),
  each move-only with the architecture test updated; (3) cut crates
  bottom-up, one per commit with the full suite; (4) minigame
  definitions, the existing modes as the first rule modules.

Test selection (user, 2026-10-14): no coverage-based test selection;
the crate split (with each crate's own tests and cargo's crate graph to
pick affected tests) is the way to shorter pushes. Until then pushes run
the full suite, and the coordinator batches merges.

## Minigames (the middle layer)

- A minigame is a data file picking pieces by id from any game (map,
  character, movement, weapons) plus a rule module by name:

  ```toml
  name = "Gun Game: Dust II"
  map = "cs_source:de_dust2"
  movement = "cs_source:movement"
  weapons = ["cs_source:weapon_*"]
  rules = "originals:gungame"
  players = "2-16"
  ```

- Rule modules (round flow, scoring, win conditions) are Rust code that
  games register by name; definitions load at run time, so new minigames
  need no rebuild. The existing modes (deathmatch, bomb and hostage
  rounds) become the first rule modules.
- No native dynamic plugins (Rust has no stable ABI; a Bevy dylib must
  match the compiler and Bevy version exactly). If rule code must ship
  without a rebuild later, scripted rule modules (WASM or Lua) behind the
  same interface.

## Repo

- One monorepo holding everything, developed in the open: engine,
  Source, games, minigames, both apps, specs and docs.
- Combat Arms (loader and specs) is developed locally and git-ignored for
  now, and was removed from the history (2026-10-07). It builds with
  `--features combat_arms` where the folder exists.

## Developing in the open

- Never commit game assets, extracted files, keys, or personal traces
  (names, personal emails, machine paths, Discord ids); commit as
  `r0pi0x` in UTC (CLAUDE.md, "Commit identity").
- Licence: MIT OR Apache-2.0 for the code (LICENSE-MIT, LICENSE-APACHE);
  original game assets may get their own licence per folder later.

## Order

1. [x] New account and identity: history rewritten to `r0pi0x` (noreply
   email, UTC timestamps), private `r0pi0x/LuckerParty` created,
   `origin` re-pointed (2026-10-07). Left: Windows re-clones; the old
   repo is deleted by the owner when satisfied.
2. [ ] Cargo workspace, one crate at a time, tests passing at each step:
   `engine` out of `src/` first, then `source` (the Source formats and
   logic out of `games/cs_source`, `logic`, parts of `map`), then
   `games/*` and `apps/*`. Sort every module first (table below), then
   move. [architecture-review.md](architecture-review.md) (P9) measured
   it: one `engine` crate at the bottom gives no iteration gain (most
   commits touch `core` or `map`); a finer engine split after sorting
   `map` does.
3. [ ] Minigame definitions and rule modules; the existing modes as the
   first definitions.
4. [ ] Networking ([multiplayer.md](multiplayer.md)); the Lucker Party app (lobby, party flow, rotation,
   launcher).

## Module sorting (to fill in at step 3)

| Module | Goes to | Notes |
|---|---|---|
| `src/core.rs`, `console.rs`, `slots.rs`, `harness.rs` | engine | done but `harness.rs`: `engine/core` |
| `src/logic` | source | done: `source/logic` (`source_logic`) |
| `src/map` | split | neutral runtime (MapData, culling, particles) vs Source materials/shaders (world.wgsl, water); audio already out (`src/audio`) |
| `src/games/cs_source` | split | formats (bsp, vpk, material, mdl, phy, wav, sound) to source; CS:S rules, weapons, HUD, bots' CS:S parts stay |
| `src/games/combat_arms` | games/combat_arms | local, git-ignored |
| `specs/` | split | `source/` to source (after review), `cs_source/` to cs_source, `combat_arms/` private |
