//! `lucker_party`'s integration tests, as one test binary (each file a
//! module): minigames on the party's simulation, headless
//! (`combat_arms::harness::CaSim` with `PartyPartsPlugin`); `heavy::`, the
//! user's installs. `cargo test -p lucker_party --features dev --test it`;
//! where tests go: docs/OBSERVABILITY.md, section 1.

mod controls;
mod cross_game;
mod gun_game;
mod hud;
mod identity;
mod net_gun_game;
mod persist;
mod net_party;
mod results_hud;
mod rotation;
mod startup;
mod switches;

mod heavy {
    mod impacts;
}
