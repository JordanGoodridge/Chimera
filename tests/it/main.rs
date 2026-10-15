//! The root package's (`mashup`) integration tests, as one test binary:
//! `dump` (this package's install inspector, `CARGO_BIN_EXE_dump`) and
//! Combat Arms' local, git-ignored tests. Every other test lives in the
//! crate whose API it needs (docs/OBSERVABILITY.md, section 1).

mod dump;

// Combat Arms: local, git-ignored files (.gitignore), present where
// src/games/combat_arms/ is.
#[cfg(feature = "combat_arms")]
mod mount_combat_arms;
#[cfg(feature = "combat_arms")]
mod rez_format;
