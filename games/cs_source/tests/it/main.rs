//! `cs_source`'s integration tests of its simulation, as one test binary
//! (each file a module): CS:S's loaders and spec test cases on the user's
//! install, needing nothing above the game (no app, harness or front
//! end). `cargo test -p cs_source --features dev --test it`; the front
//! end's are tests/ui/ (feature `ui`). Where tests go:
//! docs/OBSERVABILITY.md, section 1.
//!
//! `heavy` holds the slow ones (real maps from the install): the fast
//! tier skips them with `-- --skip heavy::`.

#[path = "../common/mod.rs"]
mod common;
#[allow(unused_imports)]
use common::{load_map, load_map_level};

mod animation;
mod mount_cs_source;
mod mount_install;
mod nav;
mod phy_spec;
mod ragdoll_spec;
mod rope_spec;
mod view_model_spec;
mod wav_decode;

/// Slow tests: the fast tier (`-- --skip heavy::`) leaves them out.
mod heavy {
    mod map_hdr;
    mod map_stock;
    mod map_vis;
}
