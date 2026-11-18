# Observability Cheat Sheet

Quick ways to verify changes without guessing. Full historical reference: [docs/archive/observability-full.md](archive/observability-full.md).

## 1. Headless Scenario Tests

Place tests in the lowest crate whose API they need (`<crate>/tests/it/`). Slow tests (>1 s or full-map loads) go in `tests/it/heavy/`.

- **Chimera base**: `harness::Sim` with `SimBasePlugins` (`chimera_sim` / `chimera_world`).
- **CS:S**: `harness::CsSim` (`games/cs_source/tests/it/`).
- **Networking**: `harness::NetSim` (`chimera/net/`).

```bash
cargo test-fast                              # default-members fast tier (no heavy::)
cargo test -p cs_source --features dev --test it weapons::
cargo test -p repo_checks                    # architecture & layering rules
```

## 2. Visual Verification (`--screenshot` & `--map`)

Run any client binary with CLI flags to inspect a frame directly:

```bash
cargo dev -- --map de_dust2 --screenshot out.png
cargo dev -- --help                          # spawn, look, movement, and debug flags
```

## 3. Live ECS & Remote Inspection (BRP)

With `dev` features enabled (default for `cargo dev`), Bevy Remote Protocol listens on `http://127.0.0.1:15702`.

- Query entities/components or trigger console commands at runtime via BRP or the in-game console (`~`).
- Toggle the inspector overlay with `F11` (releases mouse capture).

## 4. Dedicated Server Smoke Test

```bash
cargo run -p cs_source_app --no-default-features --bin cs_source_server -- +map de_dust2
```
