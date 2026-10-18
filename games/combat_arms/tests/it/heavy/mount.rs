//! Combat Arms' install mounted as the shell mounts it (`INFO.open`,
//! `game::mount`). Passes with a note without `[games.combat_arms] path`
//! in chimera.local.toml.

use std::io::ErrorKind;

use chimera_core::mount::config::LocalConfig;
use combat_arms::game::{GAME, INFO, mount};

#[test]
fn install_mounts_its_archives() {
    let Some(install) = LocalConfig::load().ok().and_then(|c| c.game_path(GAME)) else {
        eprintln!("skipping: no Combat Arms install configured");
        return;
    };
    if !install.join("Game").is_dir() {
        eprintln!("skipping: no Combat Arms install at {}", install.display());
        return;
    }
    let mount = (INFO.open)(&install).unwrap();
    assert_eq!(mount.layers().count(), 108);
    assert!(mount.read("worlds2/warehouse.dat").unwrap().len() > 1_000_000);
    let keyed = mount.read("worlds/junk_flea.dat");
    if !mount::keys().unwrap().contains_key("V2") {
        assert_eq!(keyed.unwrap_err().kind(), ErrorKind::PermissionDenied);
    }
}
