//! A game's mount as the asset settings want it (docs/plans/active/
//! asset-service.md, M1): each game's `mount::open` goes through `open`
//! here, so every loader, test and tool reads the same way. With the
//! install in use (`[assets] use_install`, default true) the install's
//! layers come first and the store's `CachedFiles` (the game's manifest, if
//! the store has one) after them; with `use_install = false`, or when the
//! path is the store's stand-in for a missing install
//! (`LocalConfig::game_path_in`), the store alone. `CHIMERA_VFS_LOG=<file>`
//! wraps every layer in a `Recording`: each file read is logged with its
//! game, size and hash (`chimera_assets::recording`), which is how
//! `chimera_pack` gets a set.

use std::{io, path::Path};

use chimera_assets::{CachedFiles, Recorder, Recording};

use super::{FileSource, Mount, config::LocalConfig};

/// `game`'s mount: `open_install(install)` and/or the asset store, per
/// the local config; recorded when `CHIMERA_VFS_LOG` is set.
pub fn open(game: &str, install: &Path, open_install: fn(&Path) -> io::Result<Mount>) -> io::Result<Mount> {
    let config = LocalConfig::load().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let assets = config.assets.resolve();
    let cached = || -> io::Result<Option<CachedFiles>> {
        let (Some(store), Some(build)) = (assets.store.as_ref(), assets.build(game)) else { return Ok(None) };
        CachedFiles::open(store, game, &build).map(Some).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    };
    let stand_in = assets.stand_in_install(game).is_some_and(|p| p == install);
    let mut mount = if !assets.use_install || stand_in {
        let cached = cached()?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "{game}: the install is off (use_install = false) and the asset store ({}) has no manifest for it",
                    assets.store.as_ref().map_or("none".into(), |s| s.root.display().to_string())
                ),
            )
        })?;
        let mut mount = Mount::default();
        mount.push(cached);
        mount
    } else {
        let mut mount = open_install(install)?;
        if let Some(cached) = cached()? {
            mount.push(cached);
        }
        mount
    };
    if let Some(recorder) = Recorder::from_env() {
        mount = record(mount, game, &recorder);
    }
    Ok(mount)
}

/// `mount` with every layer's reads recorded under `game`.
pub fn record(mount: Mount, game: &str, recorder: &std::sync::Arc<Recorder>) -> Mount {
    mount.map_layers(|l| Box::new(Recording::new(l, game, recorder.clone())) as Box<dyn FileSource>)
}
