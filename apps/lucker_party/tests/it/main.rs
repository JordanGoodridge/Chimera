//! `lucker_party`'s integration tests, as one test binary (each file a
//! module): minigames on the party's simulation, headless
//! (`combat_arms::harness::CaSim` with `PartyPartsPlugin`); `heavy::`, the
//! user's installs. `cargo test -p lucker_party --features dev --test it`;
//! where tests go: docs/OBSERVABILITY.md, section 1.

mod gun_game;
mod hud;
mod identity;
mod startup;
