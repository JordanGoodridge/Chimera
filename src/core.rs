//! The shared vocabulary, from `chimera_core::core` (chimera/core/src/core.rs),
//! plus this build's `VERSION`: stamped here, in the app's crate, so a
//! commit (which changes the stamp) recompiles only this crate.

pub use chimera_core::core::*;

/// This build's version (`version` command, main menu, bug reports,
/// server banner): see `version::format_version`; stamped by build.rs.
pub const VERSION: &str = env!("MASHUP_VERSION");
