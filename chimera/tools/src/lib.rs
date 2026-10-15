//! Chimera's tools: what any game's tools use, knowing no game.
//!
//! - `perfgate`: the instructions-per-tick regression gate; a game or app
//!   names its scenarios (CS:S's: games/cs_source/tools).
//! - `imgcmp`: comparing a reference picture with ours (stats, detail
//!   correlation, side-by-side sheets), `refcmp`'s generic core.
//! - `tracesum` (src/bin/tracesum.rs): summarize a Chrome trace of a
//!   `--features profile` build.
//!
//! RenderDoc helpers live beside them as scripts (tools/renderdoc/).

use std::path::{Path, PathBuf};

pub mod imgcmp;
pub mod perfgate;

/// The repository's root from a crate folder `levels` deep in it
/// (`env!("CARGO_MANIFEST_DIR")`): where tools find their data files
/// (tools/perfgate/, tools/refcmp/, ...).
pub fn repo_root(manifest_dir: &str, levels: usize) -> PathBuf {
    let mut p = Path::new(manifest_dir).to_path_buf();
    for _ in 0..levels {
        p.pop();
    }
    p
}
