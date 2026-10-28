//! `party_kit`'s (Lucker Party's) integration tests, as one test binary (each file a
//! module): minigames on the party's simulation, headless
//! (`combat_arms::harness::CaSim` with `PartyPartsPlugin`); `heavy::`, the
//! user's installs. `cargo test -p party_kit --features dev --test it`;
//! where tests go: docs/OBSERVABILITY.md, section 1.

mod announcer;
mod controls;
mod cross_game;
mod cross_water_ladders;
mod cs_parts;
mod death_view;
mod grenade_looks;
mod gun_game;
mod hud;
mod identity;
mod identity_sounds;
mod kill_icons;
mod last_shot;
mod net_gun_game;
mod persist;
mod net_party;
mod results_hud;
mod rotation;
mod startup;
mod switches;

mod heavy {
    mod cs_decals;
    mod cross_hits;
    mod impacts;
}
