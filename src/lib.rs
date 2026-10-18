//! `mashup`: the old paths. The CS:S app's glue (crate `cs_source_app`,
//! apps/cs_source: `SimPlugins`, `harness`, `greybox`, `games::load_map`,
//! `client`, re-exports of Chimera, Source's logic and CS:S) under the
//! names `dump` and its test (tests/it/) use. (`dump` reads Combat Arms'
//! archives with `lithtech_formats` directly.)
//!
//! Nothing new goes here: code belongs in its layer's crate
//! (docs/ARCHITECTURE.md, "Crates").

pub use cs_source_app::*;
