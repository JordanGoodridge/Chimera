//! `mashup`: the old paths. The CS:S app's glue (crate `cs_source_app`,
//! apps/cs_source: `SimPlugins`, `harness`, `greybox`, `games::load_map`,
//! `client`, re-exports of Chimera, Source's logic and CS:S) under the
//! names the integration tests (tests/it/) and `dump` use, plus Combat
//! Arms' local loader (`games::combat_arms`, src/games/combat_arms/,
//! git-ignored; the `combat_arms` feature).
//!
//! Nothing new goes here: code belongs in its layer's crate
//! (docs/ARCHITECTURE.md, "Crates").

pub use cs_source_app::*;

/// The app's games (`cs_source_app::games`: `load_map`, `cs_source`)
/// and, built with `combat_arms`, Combat Arms' local loader.
pub mod games {
    pub use cs_source_app::games::*;

    #[cfg(feature = "combat_arms")]
    pub mod combat_arms;
}
