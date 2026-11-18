# CLAUDE.md

Lucker Party / **Chimera** (`chimera/`) is a Rust + Bevy (0.20) + Avian3D (0.8) monorepo for rebuilding first-person games from runtime-loaded installs and composing their parts into minigames.

## Where things are

| Need | Read |
|---|---|
| Project overview & playing | [README.md](README.md) |
| Design rationale (slots, mounts, legal rules) | [docs/DESIGN.md](docs/DESIGN.md) |
| Crate map & layering rules | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| Tests, `--screenshot`, remote ECS queries | [docs/OBSERVABILITY.md](docs/OBSERVABILITY.md) |
| Known shortcuts | [docs/tech-debt.md](docs/tech-debt.md) |
| Behavior specs (reference only) | [specs/README.md](specs/README.md) |

## Hard rules

- **No game assets or keys**: Never commit extracted assets or encryption keys; load from user installs at runtime.
- **No third-party code**: Behavior enters only via `specs/` written in clean-room sessions.
- **Check pinned Bevy/Avian APIs**: Inspect `~/.cargo/registry/src/*/bevy_*-0.20.0/` or `avian3d` rather than guessing APIs.
- **Cross-platform**: Must build and run on Linux and Windows.
- **Architecture checks**: `cargo test -p repo_checks` enforces layering, asset/key guards, and links in this file.

## Token & Build Discipline

- **Do NOT read `docs/archive/`** unless explicitly asked for historical logs/plans.
- **Target single crates** when iterating (`cargo test -p <crate>`) instead of `--workspace`.
- **Keep files modular**: Avoid growing any `.rs` file past ~800 lines; split into focused submodules.
- **Keep docs lean**: Only update docs when architectural facts change.

## Commands

```bash
cargo dev                 # run CS:S client (dev features)
cargo party               # run Lucker Party client
cargo ugg                 # run Ultimate Gun Game client
cargo ca                  # run Combat Arms client
cargo mashup              # run Mashup Demo client
cargo test-fast           # fast unit/integration suite on default-members (skips heavy::)
cargo lint                # repo_checks (architecture, layering, asset guards)
cargo check-all           # check entire workspace (--workspace --all-targets)
cargo test -p cs_source --features dev --test it weapons::   # single test module
cargo run -p cs_source_app --no-default-features --bin cs_source_server -- +map de_dust2
```

- Dedicated servers build with `--no-default-features` on `cs_source_app`, `combat_arms_app`, `party_kit`, or `ultimate_gun_game`.
- After changing dependencies, run `cargo hakari generate` (`chimera/workspace-hack`).
- Enable git hooks once per clone: `git config core.hooksPath .githooks`.
