//! Combat Arms' install as a mount: every `.rez` archive in its `Game/`
//! folder, in name order (`lithtech_formats::rez::open_dir`, per
//! specs/combat_arms/rez_archive.md), with the keys from the local config
//! (`[games.combat_arms.keys.<scheme>]` in chimera.local.toml, hex `key`
//! and `iv`; never in the repo). Without a key, a keyed archive's files
//! list but fail to read with `PermissionDenied`.

use std::{io, path::Path};

use lithtech_formats::rez::{self, RezCipher, RezKeys};

use crate::mount::{Mount, config::LocalConfig};

/// The keys the local config has for Combat Arms, by scheme (`V1`, `V2`).
/// Errors never include key text.
pub fn keys() -> Result<RezKeys, String> {
    let config = LocalConfig::load()?;
    let mut keys = RezKeys::new();
    for (scheme, k) in config.game_keys(super::GAME).into_iter().flatten() {
        keys.insert(
            scheme.clone(),
            RezCipher::from_hex(&k.key, &k.iv, &format!("{} {scheme}", super::GAME))?,
        );
    }
    Ok(keys)
}

/// Mount a Combat Arms install (the folder holding `Game/`).
pub fn open(install: &Path) -> io::Result<Mount> {
    let game = install.join("Game");
    if !game.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{}: no Game/ folder; is this a Combat Arms install?", install.display()),
        ));
    }
    let keys = keys().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(rez::open_dir(&game, &keys)?.0)
}
