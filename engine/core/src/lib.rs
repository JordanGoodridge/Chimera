//! The engine's stable base (docs/ARCHITECTURE.md, "Crates"): the shared
//! vocabulary (`core`), the console, the slot registries (`slots`), the
//! character's components (`character`) and the performance metrics. Code
//! everything else uses and that changes rarely, in its own crate so an
//! edit higher up doesn't recompile it.
//!
//! The game crate re-exports each module at its old path (`mashup::core`,
//! `mashup::console`, ...); `mashup::core` adds the build's `VERSION`.

pub mod character;
pub mod console;
pub mod core;
pub mod metrics;
pub mod slots;
