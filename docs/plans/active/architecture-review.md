# Plan: architecture review and refactoring (2026-10-13)

Status: proposal, waiting for the owner's pick of the first slices
(section 4). Nothing refactored yet. Measured on the dev box under load
(other agents building; load averages given with the times), so compare
ratios, not absolutes.

The question: the repository is "about 246,000 lines"; is there
architectural work that would make it smaller, faster to iterate on, and
easier to keep correct (CS:S parity)?

Short answer: the architecture is sound and mostly enforced; the code is
not bloated by dead code or wild duplication. What hurts is a handful of
very large units that everything touches (`map/mod.rs` and its
1,840-line `spawn_map`, `map` as a whole as the catch-all lowest layer,
`client` as a flat bag of 46 files, the docs' module table), a few
repeated patterns (VGUI dialog plumbing, KeyValues tokenizers, test
setup), and build/test cycle costs: every source edit recompiles the
whole 165k-line library, 7 unused tool binaries relink on every test
build, and the full suite has grown to ~70 minutes (under load), 84% of
it real-map tests. The biggest wins are cheap, move-only slices. The
crate split as drawn in engine-split.md would not speed up iteration
(60% of source commits touch `core` or `map`, at the bottom of it);
split finer, after sorting `map`, it would cut roughly a fifth to a
third of a test cycle.

## 1. Facts

### 1.1 Size

The 246k is every tracked line, not code:

| What | Lines |
|---|---|
| Rust, `src/` | 165,411 |
| of which unit tests inside `src/` (`#[cfg(test)]` modules, `*_tests.rs`) | ~20,400 |
| of which binaries (`src/bin/`: dump, mapsweep, movecmp, refcmp, ...) | 5,121 |
| Rust, `tests/it/` (one test crate) | 38,313 (heavy: 17,638) |
| Markdown: `specs/` | 18,755 |
| Markdown: `docs/` + README + CLAUDE.md | ~7,500 lines, **686 KB** (long lines) |
| `Cargo.lock` | 8,494 |
| `tools/` (probes, Python, SourceMod) | 6,685 |
| WGSL shaders | 1,777 |

So the game library proper is ~140k lines of non-test Rust; tests (unit
and integration) are ~59k. That is a healthy test ratio, not bloat.

`src/` by top-level module (lines including in-file tests):

| Module | Lines | Notes |
|---|---|---|
| `client` | 37,758 | 46 files flat in one folder + `game_menu/` |
| `map` | 32,385 | 54 files; `mod.rs` alone 5,824 |
| `games/cs_source` | 29,051 | 51 files: formats, CS:S behaviour, CS:S assets glue |
| `logic` | 24,053 | 32 files; 6,800 lines of them are `*_tests.rs` |
| `net` | 12,267 | 24 files |
| `weapon` | 6,767 | |
| `bot` | 6,650 | `mod.rs` 2,673 |
| `bin` | 5,121 | 8 tools |
| `objectives` | 2,190 | |
| `core.rs` | 1,717 | "keep this small" (its own doc) |
| `console.rs` | 1,137 | |
| `mount` | 1,092 | |
| others (`metrics`, `rules`, `harness`, `greybox`, `movement`, `slots`, `character`, `lib`, `main`) | ~3,700 | |

Largest files (non-test lines): `map/mod.rs` 5,619; `client/server_browser.rs`
3,000; `bot/mod.rs` 2,598; `logic/bridge.rs` 2,377; `cs_source/weapons.rs`
2,348; `client/console.rs` 2,314; `cs_source/movement.rs` 2,250;
`logic/movers.rs` 2,104; `cs_source/bsp.rs` 2,063; `cs_source/props.rs`
2,021; `client/widgets.rs` 1,925; `core.rs` 1,718.

### 1.2 Functions

4,049 non-test functions. 3,536 are under 50 lines (48k lines); 356 are
50-100; 126 are 100-200; 28 are 200-400; **3 are over 400 lines and hold
2,917 lines between them**:

| Function | Lines | Params |
|---|---|---|
| `map::spawn_map` (map/mod.rs:2303) | **1,840** | 14 (13 of them `Option<ResMut<Assets<_>>>`) |
| `bot::think` (bot/mod.rs:903) | 667 | 15 |
| `logic::bridge::apply_effects` | 410 | 3 |
| `logic::classes::class_input` | 391 | 6 |
| `cs_source::bsp::load_level_bytes` | 348 | 4 |
| `bot::walk_route` | 315 | 10 |
| `client::console::client_commands` | 313 | 1 (registers ~60 commands inline) |
| `cs_source::props::place_props` | 296 | 7 |
| `weapon::weapon_frame` | 289 | 5 |
| `client::overview::draw` | 273 | 15 |

Long parameter lists are Bevy systems: 129 functions take 8+ parameters,
28 take 12-16 (16 is Bevy's limit); 209 `#[allow(clippy::too_many_arguments
/ type_complexity)]`; only 8 `#[derive(SystemParam)]` bundles in the
whole crate.

### 1.3 Dependencies between modules

The top-level layering (docs/ARCHITECTURE.md, enforced by
`tests/it/architecture.rs`'s `ALLOWED`) holds: no violations, no cycles
between top-level modules. `crate::` references by edge (refs / files):

```
games/cs_source -> map 222/35   core 33/13   weapon 14/3   mount 7/4   console 5/4   objectives 4/2
client          -> map 173/33   console 105/36   net 68/14   core 65/22   weapon 33/10
                   rules 22/8   games/cs_source 18/7   metrics 18/4   ...
logic           -> map 141/24   core 28/11   console 5/2
map             -> core 89/17   (nothing else: its rule)
weapon          -> core 41/8    map 33/6    console 7/3
net             -> core 30/12   console 17/9   map 13/6   rules 11/6   bot, weapon, movement 6 each
bot             -> core 23/6    map 20/6    weapon 11/2   character 5/4   objectives 4/2
```

Findings:

- **`map` is the catch-all lowest layer.** It may use only `core`, and
  everything above may use it, so anything two upper layers share lands
  in `map`: rendering (11 of the crate's 15 custom materials, 17 of its 23
  WGSL files, culling, water, shadows, particles, decals, view models), audio
  (sound, live sounds, room DSP, soundscapes, hearing, radio), animation
  (anim, ragdolls), physics helpers (prop physics, buoyancy,
  controllers), HUD layouts (`map::hud`), bot data (`nav`,
  `bot_profiles`). 54 files, 32k lines. The engine-split plan already
  marks it "split".
- **Inside `map`, 41 of the 54 files form one dependency cycle**:
  `mod.rs` holds both the data types every submodule uses (`MapData`,
  `MapMesh`, `MapProp`, ... ~1,800 lines of types) and `spawn_map`,
  which uses 48 of the submodules. That cycle is what makes `map` hard to
  split into crates; splitting the data types out of `mod.rs` breaks it.
- Intra-module cycles elsewhere (client 40 of its 47 submodules, logic 20 of 32,
  net 20 of 24, cs_source 24 of 51) are mostly the usual "plugin in
  `mod.rs` registers submodules, submodules use `mod.rs`'s types" shape:
  harmless inside one crate, irrelevant to a split along top-level lines.
- Sim code reaches into render-ish `map` submodules for state:
  `logic` uses `map::vis` (`LogicHidden`, `VisClusters`), `map::tint`,
  `map::monitor`, `map::emitters`, `map::beams`, `map::color_correction`;
  `net` uses `map::tint`, `map::decal`, `map::beams`. The state these
  hold (hidden, tinted, on/off) is simulation; the drawing is not. A
  render/sim split needs the state components moved to the sim side.
- `client` uses `games::cs_source` in 7 files (18 refs: the game id, the
  tick interval, mounting, `import_map`, movement cvar names), and 8
  client files carry CS:S strings (`#Cstrike_*`, `cstrike/`). The rest of
  the engine has none: the "engine knows no game" rule mostly already
  holds in code.
- Game composition is written out three times: `main.rs` (CS:S movement,
  weapons, player and view animation, 11 material plugins listed one by
  one), `bin/mashup_server.rs` (movement, weapons; the server's hitbox
  animation comes in implicitly through `CsWeaponsPlugin`) and each
  test's `Sim::new(...)`. There is no `CsSourcePlugins`
  group.

### 1.4 Build and iteration times

One library crate (`mashup`, 165k lines) with Bevy linked dynamically
(`dev`), our code at `opt-level = 1`, 9 binaries, one integration-test
crate (`it`). Measured 2026-10-13 in a fresh worktree, `CARGO_BUILD_JOBS=4`,
sccache, `--timings`, **load average 30-50** (other agents' builds), so
absolute times are 2-4x what an idle box gives (docs/performance.md
measured a `lib.rs` touch at 26 s at load 14); the ratios are the point.

| `cargo test --features dev --no-run` after... | wall | `mashup` lib | lib unit tests (parallel) | `it` | 9 bins (4 at a time) |
|---|---|---|---|---|---|
| cold (all dependencies) | 85 min | 1,559 s | 649 s | 170 s | 2-57 s each |
| nothing (no-op) | 4 s | - | - | - | - |
| a new `pub fn` in `client/window_icon.rs` (leaf) | 318 s | 260 s | 140 s | 39 s | 1-27 s each |
| a new `pub fn` in `core.rs` (root) | 189 s | 153 s | 24 s | 21 s | 10-14 s each |
| both reverted | 273 s | 211 s | 31 s | 42 s | 1-18 s each |
| one test file (`tests/it/teams.rs`) | 39 s | - | - | 36 s | - |
| `cargo build --features dev --bin mashup` after a leaf body change | 83 s | 63 s | - | - | 19 s (`mashup`) |

What this says:

- **The library compile is 75-80% of a source-change test cycle**, and
  it costs the same whether the edit is in a leaf (`client`) or the root
  (`core`): one crate means every edit re-runs incremental compilation
  of all 165k lines. The spread between runs is load, not the edit.
- The tail after the library (37-62 s here) is the `it` crate plus
  **8 binaries relinked on every test build**: cargo builds all of a
  package's binaries for its integration tests. Only `dump` is used by a
  test (`mount_cs_source.rs`, `CARGO_BIN_EXE_dump`); `refcmp`, `movecmp`,
  `perfgate`, `tracesum`, `mapsweep`, `testmap` and `mashup_server`
  aren't, yet each relinks a ~1 GB debug binary every cycle.
- `cargo test` compiles the library twice (normal and with its 721 unit
  tests); they run in parallel, so it costs CPU more than wall time.
- A test-only change is cheap (39 s): the one-test-crate layout works.

Which edits a crate split would speed up, by what each commit touches
(424 commits since the history rewrite, 310 touching `src/`; the lowest
layer a commit touches decides what must rebuild):

| Lowest module touched | Commits | Share |
|---|---|---|
| `core` | 46 | 15% |
| `map` | 138 | 45% |
| `weapon`, `logic`, `bot`, `rules`, `net`, `console`, `mount`, others | 53 | 17% |
| `games` (cs_source) only | 35 | 11% |
| `client`, `bin`, `main` only | 38 | 12% |

60% of source commits touch `core` or `map`, which nearly everything
depends on: after a split those still rebuild almost every crate.

### 1.5 Tests

1,389 `#[test]` attributes (nextest counts 1,459 tests with the
macro-generated per-map ones): 668 integration attributes (440 in `tests/it`, 228 in
`tests/it/heavy`), 721 unit tests in `src/` (logic 216, client 178, map
128, cs_source 100, bot 28, net 26, weapon 18; none in core, rules,
slots, movement, harness). Integration lines by area: heavy (real maps,
long bot runs) 17.6k, net 5.9k, weapons 4.3k, map/logic 4.3k, movement
2.0k, other 3.6k.

Runtime, measured here (`cargo nextest run --features dev`, 2026-10-13,
load 25-60, all 1,440 passed, 19 skipped): **70 minutes wall**. The sum
of test times is 9,433 s: unit tests 108 s (721 tests), the fast
integration tests 1,418 s (439), **heavy 7,907 s (280 tests, 84%)**.
Because `.config/nextest.toml` runs at most 2 heavy tests at once
(memory), the wall time is about the heavy sum / 2. By file: map_courses
2,056 s (72 community maps, ~28 s each), map_community 651 s, de_nuke
613 s (2 tests), map_ladders 574 s (**one** test), net_soak 520 s (one),
de_dust2 512 s (62), map_surf 475 s (3); slowest fast-tier files cs_guns
225 s (30), net_rules 204 s, net_weapons 174 s. docs/performance.md's
"Test cycle" (200-270 s full suite, 2026-10-08) predates the
community-map suites: the full suite has grown about 5x in five days
(part of that is today's load; even at a third, a pre-push run is now
20+ minutes).

Shape problems:

- **No shared test-support module.** `tests/it/main.rs` declares only
  test files. Each file grows its own helpers: 27 `fn sim()`, 19 `fn
  load()`, 6 `fn installed()`, 4 `fn dust2()`, 5 `fn active_id()`,
  8 `fn feet()`. 35 files load real maps, each with its own "is the
  install there" check, and they disagree: some skip on a missing
  install, one requires `cstrike/` to exist, one also accepts a cached
  map, others `expect` and panic.
- Each heavy test loads and converts its map from scratch (nextest runs a
  process per test, so there is no sharing; textures decode on every
  load, tech-debt "no cache yet"). de_dust2's 62 tests pay 62 loads;
  how much of a heavy test's time is the load and how much the
  simulation is not measured anywhere.
- A few single tests loop over a whole map's features (every ladder,
  every surf map) and run 4-10 minutes alone, so they can't use the
  parallelism nextest has.

### 1.6 Duplicated code

Not much copy-paste in the simulation; most duplication is in UI
plumbing, file-format parsing, and tests:

- **VGUI dialogs.** `game_menu`, `server_browser`, `first_run` (and in a
  smaller way `buy_menu`, `team_menu`, `spectator_menu`, `chat`,
  `radio`) each define their own `Hit(Target)`, `TextHit`, `WheelList`
  marker components, a `pointer` system turning `Interaction` changes
  into the model's `Input`s (hover, click, caret from click with the
  same font/scale lookup, wheel over list or combo), and a
  "despawn the tree and redraw when the model changed or the window
  resized" system. The UI scale `(height / 720.0).clamp(0.6, 3.0)` is
  written 9 times in 6 files. The good part, a pure `handle(Input) ->
  Outcome` model unit-tested without a window, is already shared in
  spirit (`game_menu/model.rs` + `tests.rs`, `ServerBrowser::handle`).
- **KeyValues text.** Two tokenizers: `mount::keyvalues::tokens` (handles
  `\` escapes) and `cs_source::surfaceprops::tokens` (no escapes), the
  latter used by 15 call sites (material, props, bsp, gameui, vgui, hud,
  pcf, decals, soundscape, water, ...), each walking the token list with
  its own brace-depth loop. Two `rgba()` colour parsers
  (`cs_source/gameui.rs`, `cs_source/vgui.rs`). Escape handling differs
  between files: a parity question (which files does the game read with
  escapes on?) hidden in which tokenizer a module happened to import.
- **Binary readers.** Five little-endian cursor types with their own
  `u8/u16/u32/i32/f32/vec3` (`nav`, `pcf`, `anim`, `vpk`,
  `net::query`), plus raw `from_le_bytes` in `bsp` (16), `ambient` (12),
  `detail`, `overlays`, `material`, ... ~200 lines in all.
- `fn local_player(w: &mut World) -> Result<Entity, String>` three times
  (client console, debug UI, weapon).
- Cvar plumbing is fine: `resource_cvar::<R, T>(app, name, help, field)`
  covers the common case (200 of 315 registrations).
- Net replication is fine: one registration list in `net/mod.rs`,
  `Snapshots<T>` generic interpolation buffers; each mirrored thing has a
  server write and a client draw system by design.

### 1.7 Dead code, unsafe, errors, features

- Dead code is negligible: 23 `pub` items referenced nowhere else (most
  are spawnflag constants documenting a format: `map::nav` flags,
  `SF_FIRE_*`), 2 `#[allow(dead_code)]`. rustc already warns on private
  dead code.
- `unsafe`: 6 sites, all FFI (thread CPU clocks in `metrics`, the Windows
  registry in `mount::install`). Fine.
- Errors: `Result<_, String>` everywhere (140 signatures), 21
  `io::Result`, 2 error enums; 241 `.unwrap()` and 19 `.expect()` in
  non-test code. Consistent enough for a game; no change proposed.
- Features: `dev`, `profile`, `combat_arms` (git-ignored folder);
  13 `cfg(feature)` sites. Clean.
- Visibility: 3,962 `pub` items against 578 `pub(crate)`/`pub(super)`.
  In one crate that is free; at a crate split every `pub` becomes API.

### 1.8 Systems and ordering

104 plugins, 279 `add_systems` calls, 316 resources, 308 components, 85
messages/events, 14 system sets. Ordering is mostly explicit pairwise:
198 `.before/.after` against 55 `.in_set`. The core sets (`SimSet::
{Rules, Commands, Movement, Weapons}`, `LogicSet`, `InterpSystems`, the
net send/receive sets) carry most of it, and ARCHITECTURE.md's "Data
flow per tick" documents it well. The pairwise edges between individual
systems in other modules (`.after(crate::core::apply_damage)` 6 times,
`.after(super::interp::draw_others)`, ...) are where ordering bugs would
hide; there are few of them.

### 1.9 Docs as a cost

For an agent-driven repo the docs are part of the build: CLAUDE.md sends
every session to them.

| File | Size | Commits touching it (since 2026-10-03: 582 commits with merges) |
|---|---|---|
| docs/ARCHITECTURE.md | 129 KB (~32k tokens; module-table rows up to 11k characters) | 176 |
| docs/tech-debt.md ("Small, current, honest") | 102 KB | 160 |
| docs/plans/active/multiplayer.md | 96 KB | 23 |
| docs/OBSERVABILITY.md | 75 KB | 115 |
| docs/backlog.md | 44 KB | 230 |

The module table repeats, at paragraph length, what each file's `//!`
doc already says (compare `client/widgets.rs`' header with its row), so
every feature edits it twice, and parallel agents conflict on it more
than on any source file. The code hotspots by the same count:
`map/mod.rs` 131, `cs_source/bsp.rs` 100, `client/mod.rs` 63,
`cs_source/props.rs` 48, `cs_source/mod.rs` 47, `core.rs` 46.

## 2. Assessment

### Keep (working well)

- **The layer rule and its test.** `ALLOWED` plus failure messages that
  say how to fix it; zero violations at 165k lines is the best evidence
  the structure works. Crates later only turn this into compiler errors.
- **Pure models under thin Bevy shells**: `logic::world::LogicWorld`
  (entity I/O as plain data, `Effect`s applied by `bridge`), the menus'
  `handle(Input) -> Outcome` models, `widgets`' control state machines,
  `game_hud::layout`. These are why 721 unit tests run without a window.
  New code should keep this shape.
- **Headless `harness::Sim`** and one integration-test crate with a
  fast and a heavy tier.
- **Data in, behaviour out between engine and game**: games fill
  `MapData`, `GameUi`, `MapSounds`; the engine has no CS:S strings
  outside 8 client files.
- **Slots by marker components**, `resource_cvar`, `Snapshots<T>`.
- No dead-code or `unsafe` problem; errors consistent.

### Hurting (with evidence)

1. `map/mod.rs`: the most-edited source file (131 commits in 10 days),
   5,800 lines, one 1,840-line function with 13 optional asset
   parameters, and the root of `map`'s 41-file cycle. Every map feature
   lands in `spawn_map`, so parallel agents conflict there.
2. `map` as the catch-all: render, audio, animation, physics and HUD
   data in one module that may only see `core`. It hides which code is
   simulation (must run headless, on the server) and which is drawing,
   and it is the main obstacle in the engine-split plan.
3. Docs size and churn (1.9): a direct tax on every agent session and the
   top merge-conflict source.
4. Repeated dialog plumbing (1.6) and 27 ad-hoc test setups (1.5):
   moderate line counts, but they make behaviour drift (UI scale clamp,
   install-skip semantics) easy.
5. A few very large systems (`bot::think`, `apply_effects`,
   `class_input`, `client_commands`, `overview::draw`) that are hard to
   review; `bot` is parked now, so it hurts least.
6. Two KeyValues tokenizers with different escape rules: a parity risk,
   not a size one.

### Risks of refactoring here

- **Merge conflicts with ongoing work** are the main cost: several agents
  work from `main` at once (playtest fixes, community maps). Moves of hot
  files must land fast, alone, and be announced; one big refactor branch
  would conflict with everything.
- **Behaviour changes from reordering**: `spawn_map` spawns in a fixed
  order and some tests check entity ids or iteration order (bot radio's
  entity-id checks, logic's node order). Extracting functions must keep
  the order.
- **Test churn**: module moves change `use` paths in tests; `pub use`
  shims keep old paths compiling until a sweep.
- **System order**: moving systems between plugins can change the order
  Bevy picks for unordered systems. Moves must keep each system's sets
  and edges, and the full suite (heavy included) must pass.
- **Parity**: anything that touches parsing (KeyValues) or effect
  application can change what CS:S content looks like; guard with
  refcmp/hudcmp captures and mapsweep before/after.

## 3. Proposals, ranked

Ranked by payoff over cost and risk. Session counts are rough
(one agent session each).

### P1. Split `map/mod.rs` (data types, bodies, spawn phases)

- **Problem**: 1.3, 2.1 above.
- **Change**:
  1. Move the format types (`MapData` and the ~60 types it holds,
     lines ~100-1900) into `map/data.rs`, re-exported from `map` so no
     path changes outside. No submodule then needs `mod.rs` for types.
  2. Move character bodies, held models and hitboxes (lines ~700-1170)
     into `map/bodies.rs`; prop shadows, prop looks and animation
     (~4,600-5,000) into `map/props.rs`; sky (~5,280-5,500) into
     `map/sky.rs`; the image builders into `map/images.rs`.
  3. Break `spawn_map` into phase functions in `map/spawn/` (world
     chunks, brush entities and movers, props, ropes, sky, water,
     decals and overlays, sounds, ...), each taking a `SpawnCtx` (the
     commands and the optional asset stores, one `SystemParam`) and the
     map data, called in today's order.
- **Payoff**: `mod.rs` from 5,800 to a few hundred lines; the biggest
  function becomes ~10 of 100-250; map features stop colliding in one
  function; breaks the `map` cycle (prerequisite for P3 and the crate
  split). No lines removed to speak of; clarity and fewer conflicts.
- **Cost**: 2 sessions (types and file moves: 1; `spawn_map` phases: 1).
- **Risk**: low for moves; medium for the phase split (spawn order, local
  state threaded between phases). Mitigation: keep phase order; compare
  a dump of each map's spawned entities (names, components, transforms,
  in order) before and after on dust2, nuke and two community maps (a
  small test helper, kept); the heavy map suite; refcmp screenshots.
- **Slices**: (a) `data.rs` move, (b) bodies/props/sky/images moves,
  (c) `SpawnCtx` + extract two phases, (d) the rest of the phases. Each
  lands alone, quickly, when no other branch is mid-edit in `map/mod.rs`.

### P2. Docs diet: module table to one line per module

- **Problem**: 1.9.
- **Change**: ARCHITECTURE.md keeps the layers, the rules, the data flow
  per tick and one line per module or folder ("`client/widgets.rs`:
  VGUI control state machines and drawing"); the detail moves to (or
  already is in) each file's `//!` doc, which becomes the source of
  truth. CLAUDE.md's "keep docs true" then means: change the `//!` doc
  with the code. tech-debt.md: one line per debt with a link to the spec
  or plan that holds the detail; finished plans' history out of
  `active/`. A test (in `architecture.rs`) can cap line length or row
  count of the module table, so it stays short.
- **Payoff**: ~100 KB less for every session that reads the map; the top
  conflict file becomes rarely edited. No code risk.
- **Cost**: 1 session. **Risk**: losing information: move, don't delete
  (check each row's facts are in the file's `//!` doc first).

### P3. Sort `map` (and `client`) into top-level modules in the crate

This is engine-split step 2's "sort every module first", done without
Cargo changes.

- **Problem**: 1.3 (`map` catch-all), 2.2.
- **Change**: new top-level modules beside `map`, each with its own row
  in `ALLOWED`:
  - `render` (materials and their plugins, WGSL, `vis`, `water`'s
    drawing, `shadows`, particle drawing, `decal`, `view_model`, sky
    drawing, `lod`, `fog`, `tint`'s drawing...): may use core, map;
    nothing in the simulation may use it.
  - `audio` (`sound`, `live_sound`, `room`, `soundscape`, `hearing`):
    sound events stay as sim data in `map`/`core`; playback here.
  - `anim` (`anim`, `ragdoll`, `placed_ragdoll`, bodies).
  - `map` keeps `MapData`, spawning, entities, collision, prop
    physics, nav and the sim-side state components that `logic`/`net`
    toggle today inside render files (`LogicHidden`, `VisClusters`,
    `RenderLook`, `BrushTint`, beam/monitor/emitter on-off state):
    the state moves to the sim side, the drawing reads it.
  - `client` grouped into folders: `hud/`, `ui/` (dialogs, widgets,
    vgui, menus), `view/` (camera, interp, spectate, freeze cam,
    senses), `post/` (hdr, gamma, colour correction), `debug/`.
- **Payoff**: the layer test then proves the simulation never depends on
  rendering (the dedicated server and headless tests stop needing
  `Option<Assets<_>>` workarounds over time: 54 such parameters in 24
  files today); the crate split becomes folder moves. Clarity for
  agents: "where does X go" has an answer.
- **Cost**: 3-4 sessions (`render` 2, `audio`/`anim` 1, `client`
  folders 1). **Risk**: import churn (`pub use` shims, then a sweep);
  system-order changes when plugins move (keep sets/edges, full suite).
  Do it in slices, one submodule family per commit.

### P4. Test support module

- **Problem**: 1.5.
- **Change**: `tests/it/support/` (declared in `main.rs`, not a test
  file): `install()` / a `require_install!()` skip macro with one rule,
  `cs_map(name) -> Option<MapData>`, `cs_sim(plugins)` (greybox + CS:S
  weapons + tick), `active_weapon_id`, `feet`/`to_source` helpers,
  `NetSim` builders. Migrate files as they are touched, plus one sweep
  of the 35 map-loading files.
- **Payoff**: ~600-1,000 test lines, one skip rule (no silently
  skipped vs panicking difference), new tests shorter to write.
- **Cost**: 1-2 sessions. **Risk**: low (tests only); guard by
  comparing the list of passing/skipped tests before and after.

### P4b. The heavy tier's time

- **Problem**: 1.5: the full suite is now ~70 minutes under load, 84%
  of it heavy tests capped at 2 at a time; every push waits for it.
- **Change**, measuring first:
  1. Log each heavy test's map-load time and simulation time (a line
     from `games::load_map` under a test env var is enough) to see
     which dominates.
  2. If loading: a headless load path that skips what tests don't use
     (texture decode, image building, mip chains) and/or a converted-map
     cache keyed by the BSP's hash and a format version.
  3. Split the 4-10 minute single tests (every ladder, every surf map,
     the soak) into one test per map or case so they spread over the
     heavy slots.
  4. Measure a heavy test's peak memory and raise the heavy group's
     `max-threads` where the box allows, or give the community sweeps
     (`map_courses`, `map_community`: 95 tests, ~2,700 s) a third tier
     run before merges that touch `logic`/`map` and nightly, not every
     push.
- **Payoff**: the push gate back toward the 5-minute range; also faster
  `mapsweep`. **Cost**: 1-2 sessions. **Risk**: low; a test tier split
  needs a written rule for when it must run, or regressions on
  community maps slip through.

### P5. Game composition as plugin groups

- **Problem**: 1.3 last point.
- **Change**: `games::cs_source::CsSourcePlugins` (movement, weapons,
  player/view animation where rendering exists, tick interval, loadout)
  and `map::MapRenderPlugins` (the 11 material plugins), used by
  `main.rs`, `mashup_server` and full-game tests. Today what a game is
  is partly implicit (`CsWeaponsPlugin` adds the server's
  `player_anim::SimAnimPlugin`; the drawn animation plugins are listed
  only in `main.rs`); the group makes the client/server difference one
  visible `if rendering`.
- **Payoff**: one place says what "CS:S" is; client and server can't
  drift; the `cargo run -p cs_source` app of the engine-split plan
  starts here. **Cost**: half a session. **Risk**: low.

### P6. One VGUI dialog layer

- **Problem**: 1.6 (dialogs).
- **Change**: in `client/widgets` (or `client/ui/dialog.rs`): generic
  `Hit<T>`, `TextHit<T>`, `WheelList` components, a `UiScale` helper
  (the one place for `/ 720` and its clamp), and a `Dialog` trait
  (`type Target; type Input; fn handle(&mut self, Input) -> Outcome;
  fn draw(&self, &mut DialogDraw)`) with generic `pointer::<D>` and
  `redraw::<D>` systems. Port `first_run` first (smallest), then
  `server_browser`, then `game_menu`.
- **Payoff**: ~600-1,000 lines; the same caret/scroll/double-click
  behaviour in every dialog (a parity win: VGUI behaves the same in all
  of them). **Cost**: 2 sessions. **Risk**: medium (UI behaviour);
  guard with the models' unit tests, `menus` tests, `hud_layout`, and
  hudcmp captures of each dialog before/after.

### P7. Source text and binary readers

- **Problem**: 1.6 (KeyValues, readers).
- **Change**: one KeyValues module (tokenizer with an explicit
  `escapes: bool`, a tree parser, and a tiny cursor for the token-walk
  loops) in `mount` (or `source` later); every parser picks escapes
  explicitly, with the choice per file type written in the spec (ask a
  spec session if unknown). One `LeReader` for binary formats.
- **Payoff**: ~300-500 lines; the escape behaviour becomes a visible,
  tested decision per file type. **Cost**: 1 session. **Risk**: parsing
  changes on community content; guard with mapsweep over the cached maps
  (warnings and entity counts identical) and the mount tests.

### P8. Split the remaining giant systems

`bot::think` (667 lines), `logic::bridge::apply_effects` (410: one arm
group per effect family into functions), `logic::classes::class_input`
(391: per-class functions), `client::console::client_commands` (313:
commands registered next to their feature, as other modules do),
`overview::draw`, `radar::update`. Introduce `SystemParam` bundles for
the recurring query groups (characters, the local player, UI fonts and
window scale). **Payoff**: reviewability; fewer 16-parameter systems.
**Cost**: 2-3 sessions, can be done opportunistically when a feature
touches one. **Risk**: low-medium; existing tests cover them. `bot` is
parked: leave it until bots are picked up again.

### P9. The crate split (engine-split step 2)

- **Problem**: every source edit recompiles all 165k lines (1.4); the
  layer rules are a text-scanning test, not the compiler.
- **What the measurements say about the plan as written**: the target
  layout (`engine` / `source` / `games/*` / `apps/*`) puts `core`,
  `map`, `client` and `net` in one `engine` crate at the bottom. Edits
  there (about 85% of source commits) would rebuild `engine`, then
  `source`, the games and the apps: no faster than today, possibly
  slower (a longer chain, less cross-crate inlining). Only CS:S-only
  edits (11%) get the full win. **As drawn, the split is not an
  iteration-speed win**; its value is the publishing/sharing goal of
  the plan and compiler-enforced boundaries.
- **To make it a speed win**, the engine has to be several crates whose
  hot parts sit high in the graph: `engine_core` (core, console, slots,
  character, metrics), `map` (data, spawning, collision), `sim`
  (weapon, objectives, rules, bot, movement, logic or `source`),
  `render`, `audio`, `net`, `client`, then games and apps. Rough
  estimate, by the commit mix above and assuming incremental time
  scales with the rebuilt crates' size: core edits about as slow as
  today, `map` data edits ~0.9x, sim edits ~0.5x, client/game edits
  ~0.3x of today's library time: about **25-30% off the library time,
  ~20% off a test cycle**, close to the earlier 15-25% estimate. After
  P1 and P3 move most of what `map` commits touch today (spawning,
  materials, drawing) into `render`/spawn code above the simulation,
  the blended gain grows toward ~35-40%: P1/P3 roughly double the
  split's payoff, so they come first.
- **Costs**: each crate's unit tests become their own test binary (more
  links, ~1-3 s each idle); `pub` items become cross-crate API (3,962
  of them, many only `pub` by habit); Windows build to re-verify; every
  open branch rebased over moved paths. 4-6 sessions after P1/P3, done
  one crate at a time from the bottom, each a move-only commit with the
  full suite.
- **Cheap wins to take first** (half a session, ~10% of a cycle):
  `required-features = ["tools"]` on the six debugging binaries no test
  uses (and on `mashup_server` if no test needs it), so `cargo test`
  stops relinking them; the pre-push `cargo check --lib --bins` adds
  `--features tools` so they keep compiling. Then measure, on an idle
  box, mold (documented opt-in) and `debug = "line-tables-only"` for
  dependencies (docs/performance.md lists both, unmeasured on a full
  cycle).
- **Recommendation**: do the cheap wins now, P1 and P3 next, then the
  split along the finer lines above, starting with `games/cs_source` as
  its own crate (the cleanest boundary: it uses only core, console,
  mount, map, weapon, objectives, and 7 client files reach into it) and
  `client` (the top). Don't do the split as one big move.

### Not worth doing

- **Rewriting net replication** or the logic world: they are already
  the cleanest parts.
- **A typed error overhaul**: `Result<_, String>` is consistent and the
  messages reach the console as they are.
- **Hunting dead code**: there is almost none.
- **Changing UI toolkit** (e.g. everything to egui): the VGUI parity
  work depends on Bevy UI layout matching the game's pixels.
- **A macro or proc-macro for cvars or replication**: the existing
  generic helpers already remove the boilerplate; a macro would hide it
  from agents reading the code.
- **Splitting files just for size** where the file is one concern
  (`cs_source/weapons.rs` is mostly the 24 guns' tables;
  `cs_source/movement.rs` is one spec).

## 4. Recommended order

For approval. Each slice is one session or less, lands on `main` alone
and quickly (announce it so other agents merge right after), passes the
full suite, and changes no behaviour.

1. **Tool binaries out of the test build** (P9 cheap wins): `required-
   features` on the unused binaries; the hook, the release workflow
   and scripts that build them pass the feature; docs updated
   (CLAUDE.md commands, OBSERVABILITY.md, performance.md "Test cycle"
   with before/after on an idle box). Half a session; ~10% off every
   test cycle for every agent from then on.
2. **Heavy tier: measure, then cut** (P4b steps 1 and 3 first: the
   load/simulate timing line and splitting the 4-10 minute tests). One
   session; the full suite is the slowest loop every agent waits on.
3. **Docs diet** (P2): ARCHITECTURE.md module table to one line per
   module, details checked into `//!` docs; tech-debt.md to one line per
   entry with links. One session; removes the top conflict file and
   ~100 KB from each session's reading.
4. **`map/mod.rs` types and helpers out** (P1 slices a and b):
   `map/data.rs`, `bodies.rs`, `props.rs`, `sky.rs`, `images.rs`, with
   re-exports so no path changes outside `map`. One session; land when
   no branch has `map/mod.rs` changes in flight.
5. **`spawn_map` in phases** (P1 slices c and d) with a spawned-entity
   dump compared before/after on dust2, nuke and two community maps.
   One to two sessions.
6. **Test support module** (P4) and **game plugin groups** (P5):
   independent of the above, small, can run in parallel with 4-5.

Then, in order of value: P3 (sort `map`/`client` into top-level
modules), P6 (dialog layer), P7 (KeyValues/readers, with a spec answer
on escapes), the crate split along P9's finer lines, and P8
opportunistically. Re-measure the build after slice 1 and after P3 on
an idle box, and update this plan with the numbers.

## Measurement scripts

The numbers above came from small throwaway scripts (line counts per
module, function lengths and parameter counts, `crate::` edges and
cycles per folder, unused `pub` names, cargo `--timings` parsing,
commits by lowest module touched). They were not kept: `cargo
--timings`, `git log --name-only` and the layering test cover what a
future pass needs; if this review is repeated, keeping them under
`tools/` would be worth it then.
