# Architecture

Code layout and dependency direction. Design rationale lives in [DESIGN.md](DESIGN.md). Layering and repository rules are enforced by `cargo test -p repo_checks` ([checks/tests/it/architecture.rs](../checks/tests/it/architecture.rs)).

## Workspace Crates (25 Members)

The root `Cargo.toml` is a pure virtual workspace. `default-members` is `["apps/cs_source", "apps/party_kit"]` so day-to-day `cargo build`/`cargo test` stays fast.

### 1. Apps (`apps/*`) — Binaries & Composition
- `cs_source_app` (`apps/cs_source/`): Main CS:S client (`cs_source`) + headless dedicated server (`--no-default-features --bin cs_source_server`).
- `combat_arms_app` (`apps/combat_arms/`): Combat Arms client (`combat_arms`) + dedicated server (`--no-default-features --bin combat_arms_server`).
- `party_kit` (`apps/party_kit/`): Cross-game composition, minigames, `lucker_party` client, and `lucker_party_server` (`--no-default-features --bin lucker_party_server`).
- `ultimate_gun_game` (`apps/ultimate_gun_game/`): Gun-game client + `ultimate_gun_game_server` (`--no-default-features --bin ultimate_gun_game_server`).
- `mashup_demo` (`apps/mashup_demo/`): Demo app mixing Combat Arms character/weapons on CS:S maps.

### 2. Games (`games/*`) — Game Libraries
- `cs_source` (`games/cs_source/`): CS:S simulation, loaders, parts, rules, bots, `harness::CsSim`, and optional `ui` feature (menus/HUD).
- `combat_arms` (`games/combat_arms/`): Combat Arms simulation, loaders, weapons, modes, and optional `ui` feature.
- `originals` (`games/originals/`): Original minigame definitions and rules (depends on Chimera only, never on other games).

### 3. Format & Logic Loaders (`source/*`, `lithtech/*`)
- `source_formats` (`source/formats/`): Pure-data Source parsers (VPK, BSP, VTF, VMT, MDL, KeyValues). **No Bevy or Avian.**
- `lithtech_formats` (`lithtech/formats/`): Pure-data LithTech parsers (`.rez`, LTB, LTA, DTX, attribute files). **No Bevy or Avian.**
- `source_tools` (`source/tools/`): Offline Source inspection/conversion helpers.
- `source_logic` (`source/logic/`): Source entity I/O, triggers, movers (`logic::net`).

### 4. Chimera Engine (`chimera/*`)
- `chimera_vfs` (`chimera/vfs/`): Neutral virtual filesystem (`FileSource`, `Mount`, `LooseDir`), std-only.
- `chimera_core` (`chimera/core/`): Shared types, `core`, `console`, `slots`, `character`, `metrics`, `mount`, `game_registry`, `gameplay`, `game_parts`, `announcer`, `cues`, `query_cache`.
- `chimera_world` (`chimera/world/`): Neutral map data, collision, sim-side spawning, `greybox`, `music`, headless `harness::Sim`.
- `chimera_sim` (`chimera/sim/`): Simulation frameworks (`weapon`, `movement`, `rule_module`, `mode`, `minigame`, `identity`, `spawning`, `announce`, `server_docs`, `SimBasePlugins`, optional `testkit`).
- `chimera_net` (`chimera/net/`): Transport, replication, prediction, `harness::NetSim`.
- `chimera_present` (`chimera/present/`): Rendering, audio, `MapPlugin`, asset source installation (`<game>://`).
- `chimera_client` (`chimera/client/`): Client shell (window, input, camera, console UI, widgets, HUD base, spectating, debug UI).
- `chimera_tools` (`chimera/tools/`): Profiling/debug binaries (`profsum`, etc.).
- `workspace-hack` (`chimera/workspace-hack/`): Cargo-hakari feature unification crate.

### 5. Repo Tooling (`checks/`, `tools/dump/`)
- `repo_checks` (`checks/`): Architecture, layering, asset/key/spec guards, and `affected` crate selector.
- `dump` (`tools/dump/`): Standalone map/archive inspection CLI.

## Layering Rules

```
apps/* (present feature = client; --no-default-features = headless server)
  ├── games/* (cs_source, combat_arms, originals) [no cross-game deps]
  ├── source_logic
  └── chimera_client
        ├── chimera_present
        ├── chimera_net
        └── chimera_sim
              └── chimera_world
                    └── chimera_core
                          └── chimera_vfs / source_formats / lithtech_formats (no Bevy/Avian)
```

1. **Simulation vs Presentation**: `chimera_world`, `chimera_sim`, `chimera_net`, `source_logic`, and game simulation modules never import `chimera_present`, `chimera_client`, or Bevy rendering/audio types.
2. **Games Never Cross-Import**: `cs_source`, `combat_arms`, and `originals` do not depend on each other; composition happens only in `apps/*`.
3. **UI Separation**: Game simulation modules never import their own `ui` module (`games_ui`).
4. **No Third-Party Code or Assets**: Never commit extracted game assets, keys, or decompiled code. Behavior enters via `specs/` only.
