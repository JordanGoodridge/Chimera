//! `mashup_demo`'s integration tests, as one test binary (each file a
//! module): the mashup on the user's installs (`heavy::`: a CS:S map
//! played as Combat Arms, headless on Combat Arms' simulation,
//! `combat_arms::harness::CaSim`). `cargo test -p mashup_demo --features
//! dev --test it`; where tests go: docs/OBSERVABILITY.md, section 1.

mod two_games;

// Slow ones (the user's installs): the fast tier skips `heavy::`.
mod heavy {
    mod cs_maps;
    mod dust2;
}
