//! Shared mount plumbing: the read-only virtual filesystem over a game's
//! archives and loose files (`source_formats::fs`, re-exported here), plus
//! local configuration and finding the installs (`install`: config, saved
//! settings, Steam libraries). Archive formats live in `source_formats`
//! (VPK) or `games/<name>/`.

pub mod config;
pub mod install;

pub use source_formats::{
    fs::{Entry, FileSource, LooseDir, Mount, normalize},
    keyvalues,
};
