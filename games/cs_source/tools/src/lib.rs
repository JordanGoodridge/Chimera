//! Counter-Strike: Source's tools (docs/OBSERVABILITY.md):
//!
//! - `refcmp` (src/bin/refcmp.rs): views and the HUD compared with the
//!   real CS:S (its reference driver over RCON; the comparison is
//!   `chimera_tools::imgcmp`, the HUD's `hudcmp`).
//! - `movecmp` (src/bin/movecmp.rs): Source movement compared tick for
//!   tick with CS:S's, through tools/css_probe's plugin.
//! - `mapsweep` (src/bin/mapsweep.rs): every cached map loaded headless,
//!   its problems reported.
//! - `perfgate` (src/bin/perfgate.rs): CS:S's scenarios on Chimera's
//!   instructions-per-tick gate (`chimera_tools::perfgate`).
//!
//! Known shortcuts (docs/tech-debt.md): These tools still reach CS:S's
//! simulation through the CS:S app (`cs_source_app`: the harness at its
//! old path, with CS:S's base `CsSim`; the build stamp `perfgate`'s
//! baselines name; map loading by id; `MapPlugin` through its
//! presentation), so this game-layer crate depends on the app;
//! `mapsweep` (and the root's `dump`) need CS:S's loaders
//! (`cs_source::game::bsp`, `mount`), so they aren't Source's tools yet.
//! Why it's OK for now: the crate is a leaf (nothing depends on it); the
//! harness is Chimera's and CS:S's now (`chimera_world::harness`,
//! `cs_source::harness`), so only the stamp and map loading keep it on
//! the app, and a Source map loader in `source/` would let them move
//! down.

pub mod hudcmp;

/// The repository's root (data files: tools/refcmp/, tools/perfgate/).
pub fn repo_root() -> std::path::PathBuf {
    chimera_tools::repo_root(env!("CARGO_MANIFEST_DIR"), 3)
}
