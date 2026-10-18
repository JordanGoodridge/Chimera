//! CS:S mount against a real install. Skipped (passes with a note) when
//! chimera.local.toml has no usable `[games.cs_source]` path.

use std::path::PathBuf;

use ::cs_source::game::{self as cs_source, vpk::Vpk};
use chimera_core::mount::{FileSource, config::LocalConfig};

fn install() -> Option<PathBuf> {
    let path = LocalConfig::load().ok()?.game_path(cs_source::GAME)?;
    if path.join("cstrike").is_dir() {
        Some(path)
    } else {
        eprintln!("skipping: no CS:S install at {}", path.display());
        None
    }
}

#[test]
fn reads_de_dust2_through_the_mount() {
    let Some(install) = install() else { return };
    let mount = cs_source::mount::open(&install).unwrap();
    let bsp = mount.read("maps/de_dust2.bsp").unwrap();
    assert_eq!(&bsp[0..4], b"VBSP");
    assert_eq!(i32::from_le_bytes(bsp[4..8].try_into().unwrap()), 20, "BSP version");
    // Case-insensitive, either slash, like the game.
    assert_eq!(mount.read("MAPS\\DE_DUST2.BSP").unwrap().len(), bsp.len());
}

#[test]
fn vpk_contents_match_their_crcs() {
    let Some(install) = install() else { return };
    let vpk = Vpk::open(install.join("cstrike/cstrike_pak_dir.vpk")).unwrap();
    assert!(vpk.len() > 10_000, "only {} entries", vpk.len());
    let mut entries = vpk.entries();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    // A spread sample across archives, including preload and in-dir data.
    for e in entries.iter().step_by(entries.len() / 300) {
        let data = vpk.read(&e.path).unwrap().unwrap();
        assert_eq!(data.len() as u64, e.size, "{}: size", e.path);
        assert_eq!(
            crc32fast::hash(&data),
            vpk.crc(&e.path).unwrap(),
            "{}: CRC mismatch",
            e.path
        );
    }
}

#[test]
fn mount_layers_shared_hl2_content() {
    let Some(install) = install() else { return };
    let mount = cs_source::mount::open(&install).unwrap();
    let entries = mount.entries();
    let from_hl2 = entries
        .iter()
        .filter(|(e, layer)| *layer >= 3 && e.path.ends_with(".vtf"))
        .count();
    assert!(
        from_hl2 > 1000,
        "expected HL2 textures behind CS:S content, got {from_hl2}"
    );
}
