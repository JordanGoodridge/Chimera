//! Game files as content-addressed records (docs/plans/active/asset-service.md,
//! M1): a `Store` holds each file a game's search path shows as one record,
//! named by the SHA-256 of its plain bytes and stored zstd-compressed
//! (`records/<ab>/<id>.zst`); a `Manifest` per game build maps paths to
//! records and names sets of them (`de_dust2`, `heavy-tests`;
//! `manifests/<game>/<build>.txt`). `CachedFiles` mounts a manifest as a
//! read-only `chimera_vfs::FileSource`, checking each record's hash as it
//! reads; `Recording` wraps a mount's layers and notes every file a load
//! reads (`CHIMERA_VFS_LOG=<file>`: game, path, size, hash per line), which
//! is how sets are recorded; `pack` fills a store from a mount. Settings
//! (`[assets]` in chimera.local.toml, `CHIMERA_ASSET_STORE`,
//! `CHIMERA_ASSET_USE_INSTALL`) are `AssetsConfig`; Chimera's `mount`
//! (chimera_core) layers the store under or instead of an install. The
//! store is per user (`~/.cache/chimera/assets`,
//! `%LOCALAPPDATA%\chimera\assets`), shared by every checkout, never in the
//! repo. Keys never reach it: Combat Arms' files are stored decrypted by
//! the host's packer.
//!
//! Known shortcuts: a mount layered under an install takes the newest
//! build's manifest (or the one `[assets] builds` pins), not the install's
//! own build; a read of a record that isn't in the store fails (no fetcher
//! until M2); `verify` and `gc` read the whole store single-threaded.

pub mod cached;
pub mod config;
pub mod manifest;
pub mod pack;
pub mod recording;
pub mod store;

pub use cached::CachedFiles;
pub use config::{AssetSettings, AssetsConfig};
pub use manifest::{FileRecord, Manifest};
pub use pack::{PackReport, pack};
pub use recording::{LogLine, Recorder, Recording, read_log};
pub use store::{Store, sha256_hex};
