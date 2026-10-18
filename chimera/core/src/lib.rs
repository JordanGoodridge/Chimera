//! The engine's stable base (docs/ARCHITECTURE.md, "Crates"): the shared
//! vocabulary (`core`), the console, the slot registries (`slots`), the
//! character's components (`character`), the performance metrics, the
//! mounts (`mount`: the VFS over a game's files, local config, finding
//! installs), game events and the server log (`game_log`) and the games an app plays (`game_registry`: what a game
//! tells the shell about itself). Code
//! everything else uses and that changes rarely, in its own crate so an
//! edit higher up doesn't recompile it.
//!
//! The game crate re-exports each module at its old path (`mashup::core`,
//! `mashup::console`, ...); `mashup::core` adds the build's `VERSION`.

pub mod character;
pub mod console;
pub mod core;
pub mod game_log;
pub mod game_registry;
pub mod metrics;
pub mod mount;
pub mod slots;
