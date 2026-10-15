//! `cs_source`'s integration tests of its front end (feature `ui`), as one
//! test binary (each file a module): menus and HUD logic on the client
//! shell, and checks that need the presentation. `cargo test -p cs_source
//! --features dev,ui --test ui`. Where tests go: docs/OBSERVABILITY.md,
//! section 1.
//!
//! `heavy` holds the slow ones (real maps from the install): the fast
//! tier skips them with `-- --skip heavy::`.

#[path = "../common/mod.rs"]
mod common;
#[allow(unused_imports)]
use common::{load_map, load_map_level};

mod menus;

/// Slow tests: the fast tier (`-- --skip heavy::`) leaves them out.
mod heavy {
    mod hud_layout;
    mod lightmap_probe;
}
