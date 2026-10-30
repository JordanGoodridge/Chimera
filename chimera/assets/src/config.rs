//! Where the store is and whether the install is used: `[assets]` in
//! chimera.local.toml (`store`, `use_install`, `builds.<game>`), with the
//! environment over it (`CHIMERA_ASSET_STORE`, `CHIMERA_ASSET_USE_INSTALL`
//! = 0/1). The default store is per user and shared by every checkout and
//! slot: `~/.cache/chimera/assets` (Linux), `%LOCALAPPDATA%\chimera\assets`
//! (Windows).

use std::{collections::BTreeMap, path::PathBuf};

use serde::Deserialize;

use crate::Store;

pub const STORE_ENV: &str = "CHIMERA_ASSET_STORE";
pub const USE_INSTALL_ENV: &str = "CHIMERA_ASSET_USE_INSTALL";

/// `[assets]` as written.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct AssetsConfig {
    /// The store folder (`~` expanded); default: the user's cache.
    pub store: Option<PathBuf>,
    /// False: games with a manifest in the store load from it alone, the
    /// install hidden. Default true: the store is layered under the
    /// install (and stands in for a missing one).
    pub use_install: Option<bool>,
    /// The build to mount per game (default: the newest in the store).
    #[serde(default)]
    pub builds: BTreeMap<String, String>,
}

/// The settings in effect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetSettings {
    pub store: Option<Store>,
    pub use_install: bool,
    pub builds: BTreeMap<String, String>,
}

/// The user-wide store folder.
pub fn default_store() -> Option<PathBuf> {
    Some(dirs::cache_dir()?.join("chimera").join("assets"))
}

fn expand_home(path: PathBuf) -> PathBuf {
    match (path.strip_prefix("~"), dirs::home_dir()) {
        (Ok(rest), Some(home)) => home.join(rest),
        _ => path,
    }
}

impl AssetsConfig {
    /// With the environment's overrides.
    pub fn resolve(&self) -> AssetSettings {
        self.resolve_with(std::env::var_os(STORE_ENV).map(PathBuf::from), std::env::var(USE_INSTALL_ENV).ok())
    }

    pub fn resolve_with(&self, store_env: Option<PathBuf>, use_install_env: Option<String>) -> AssetSettings {
        let store = store_env
            .filter(|p| !p.as_os_str().is_empty())
            .or_else(|| self.store.clone())
            .map(expand_home)
            .or_else(default_store)
            .map(Store::new);
        let use_install = match use_install_env.as_deref().map(str::trim) {
            Some("0" | "false" | "no" | "off") => false,
            Some("1" | "true" | "yes" | "on") => true,
            _ => self.use_install.unwrap_or(true),
        };
        AssetSettings { store, use_install, builds: self.builds.clone() }
    }
}

impl AssetSettings {
    /// The build of `game` to mount: the pinned one, else the store's
    /// newest; None when the store has no manifest for it.
    pub fn build(&self, game: &str) -> Option<String> {
        let store = self.store.as_ref()?;
        match self.builds.get(game) {
            Some(b) => store.manifest_path(game, b).is_file().then(|| b.clone()),
            None => store.latest_build(game),
        }
    }

    /// Whether the store can serve `game`.
    pub fn serves(&self, game: &str) -> bool {
        self.build(game).is_some()
    }

    /// The path that stands for `game`'s install when it is served from
    /// the store alone (its manifest folder): `served_only` recognizes it.
    pub fn stand_in_install(&self, game: &str) -> Option<PathBuf> {
        Some(self.store.as_ref()?.manifest_dir(game))
    }
}
