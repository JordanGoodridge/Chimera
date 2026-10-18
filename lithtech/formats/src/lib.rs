//! LithTech's file formats as plain data (the `lithtech/` loader layer,
//! beside `source/`; docs/ARCHITECTURE.md, "Crates"): bytes in, Rust
//! values out, no Bevy and no game code. A game crate (Combat Arms)
//! converts what these read into Chimera's neutral types, as `cs_source`
//! does with `source_formats`.
//!
//! - `attributes`: attribute files ("butes"), the INI-like text documents
//!   with a game's numbers: sections of typed values, strict typed reads.
//! - `dtx`: textures (`.dtx`, version -5): header, mip layout, 32-bit
//!   BGRA and DXT1/3/5 decoded to RGBA8.
//! - `rez`: `.rez` resource archives (header, directory, file data) as a
//!   mount layer (`source_formats::fs::FileSource`), and a folder of them
//!   as a `Mount`.
//! - `rez_cipher`: the payload cipher (Twofish-CBC with a key supplied at
//!   run time; keys never live in the repo).
//! - `world`: world files (the compiled map, `.dat`, version 85): BSPs,
//!   objects, the light grid, blockers and the render data with its
//!   lightmaps, bounds-checked.
//!
//! Each format follows its spec under specs/ (specs/README.md); comments
//! name the spec's sections.

pub mod attributes;
pub mod dtx;
pub mod rez;
pub mod rez_cipher;
pub mod world;
