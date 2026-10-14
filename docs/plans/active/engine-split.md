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
- [~] (2) `render` out of `map` (2026-10-13, one session, three
  commits, each with the spawn dump identical on the five maps modulo
  moved paths):
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
  - [ ] The rest, roughly in this order (each needs its data types left
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
    `MapDrawer` hooks where they spawn). `insert_resources` still
    inserts some drawing resources (decal and particle assets, fog) and
    `unload_map` still removes them: render's go through `MapHooks`
    when they move. Client post passes (`client::hdr`, `gamma`,
    `color_correction`) stay in client: they hang off
    `FirstPersonCamera`/`UiCamera`/`ClientArgs`.
  - A server without rendering: not yet. `mashup_server` adds no
    material plugins but links `render` through `MapPlugin` (its spawn
    drawer and the moved systems run headless with no asset stores, as
    before). Feasible once `build_rest` is split into map's systems and
    render's (an ordering change to make deliberately) and a headless
    `MapDrawer` (all `Draws` false, nothing spawned) stands in for
    `render::MapAssets`; then `render` can sit behind a feature.
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
- [ ] (2) CS:S UI out of `client`, rules behind a rule-module interface,
  game-registered replication.
- Order: (1) the `tools` package; (2) inside today's single crate, move
  code to its future home (the `map` sort, CS:S UI out of `client`,
  rules behind a rule-module interface, game-registered replication),
  each move-only with the architecture test updated; (3) cut crates
  bottom-up, one per commit with the full suite; (4) minigame
  definitions, the existing modes as the first rule modules.

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
| `src/core.rs`, `console.rs`, `slots.rs`, `harness.rs` | engine | |
| `src/logic` | source | Source entity I/O and classes (func_door, triggers, ...) |
| `src/map` | split | neutral runtime (MapData, culling, particles) vs Source materials/shaders (world.wgsl, water); audio already out (`src/audio`) |
| `src/games/cs_source` | split | formats (bsp, vpk, material, mdl, phy, wav, sound) to source; CS:S rules, weapons, HUD, bots' CS:S parts stay |
| `src/games/combat_arms` | games/combat_arms | local, git-ignored |
| `specs/` | split | `source/` to source (after review), `cs_source/` to cs_source, `combat_arms/` private |
