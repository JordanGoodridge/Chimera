//! The keyed archives on the user's Combat Arms install, read with the
//! keys in the local config (`[games.combat_arms.keys.V1]` / `.V2`), against
//! specs/combat_arms/rez_archive.md's "Header checks after decryption":
//! V1 decrypts with Twofish, V2 with AES-256, and what comes out is each
//! type's own header and parses with our readers. Each test passes with a
//! note without an install or without its scheme's key. Nothing here
//! prints or writes key bytes or decrypted files.

use std::collections::BTreeMap;

use lithtech_formats::{
    attributes::Document,
    dtx::Dtx,
    ltb::Model,
    rez::{RezArchive, RezKeys, open_dir},
    world::World,
};
use source_formats::fs::FileSource;

use super::rez_install::{game_dir, keys};

/// The install's `Game/` folder and the keys, if `scheme`'s is there.
fn keyed(scheme: &str) -> Option<(std::path::PathBuf, RezKeys)> {
    let game = game_dir()?;
    let keys = keys();
    if !keys.contains_key(scheme) {
        eprintln!("skipping: no [games.combat_arms.keys.{scheme}] in chimera.local.toml");
        return None;
    }
    Some((game, keys))
}

/// What a type's decrypted payload starts with ("Header checks after
/// decryption"); `None` for types without a fixed signature.
fn signature(path: &str) -> Option<&'static [u8]> {
    let ext = path.rsplit('.').next()?;
    Some(match ext {
        // resType 0; the version -5 follows at 0x08 (`dtx_header_ok`).
        "dtx" => &[0, 0, 0, 0],
        "ltb" => &[0x01, 0x00, 0x09, 0x00],
        "dat" => &[0x55, 0x00, 0x00, 0x00],
        "pcx" => &[0x0A, 0x05, 0x01, 0x08],
        "bmp" => b"BM",
        "png" => b"\x89PNG\r\n\x1a\n",
        _ => return None,
    })
}

/// A `.dtx` starts `00 00 00 00 | xx xx xx xx | FB FF FF FF`: resType 0,
/// then the version -5 at 0x08.
fn dtx_header_ok(b: &[u8]) -> bool {
    b.len() >= 164 && b[..4] == [0; 4] && b[8..12] == [0xFB, 0xFF, 0xFF, 0xFF]
}

/// Every V2 archive: its first texture (by path) decrypts to a DTX
/// header and parses; per type, the first file of each signed type starts
/// with its signature.
#[test]
fn v2_archives_decrypt_to_valid_headers() {
    let Some((game, keys)) = keyed("V2") else { return };
    let (mount, _) = open_dir(&game, &keys).unwrap();
    let (mut archives, mut textures) = (0, 0);
    let mut by_type: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for layer in mount.layers() {
        let rez = RezArchive::open(layer.name(), &keys).unwrap();
        if rez.header().key_scheme() != Some("V2") {
            continue;
        }
        archives += 1;
        assert!(!rez.locked());
        let mut entries = rez.entries();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        if let Some(e) = entries.iter().find(|e| e.path.ends_with(".dtx") && e.size >= 164) {
            let b = rez.read(&e.path).unwrap().unwrap();
            assert!(dtx_header_ok(&b), "{}: {} is not a DTX header", layer.name(), e.path);
            Dtx::parse(b).unwrap_or_else(|err| panic!("{}: {err}", e.path));
            textures += 1;
        }
        // The first file of each signed type in this archive.
        let mut seen = std::collections::BTreeSet::new();
        for e in &entries {
            let Some(sig) = signature(&e.path) else { continue };
            let ext = e.path.rsplit('.').next().unwrap().to_string();
            if e.size < 16 || !seen.insert(ext.clone()) {
                continue;
            }
            let b = rez.read(&e.path).unwrap().unwrap();
            let ok = if ext == "dtx" { dtx_header_ok(&b) } else { b.starts_with(sig) };
            let t = by_type.entry(ext).or_default();
            t.0 += ok as usize;
            t.1 += 1;
            if !ok {
                eprintln!("{}: {} starts {:02X?}", layer.name(), e.path, &b[..8]);
            }
        }
    }
    eprintln!("{archives} V2 archives, {textures} with textures; first of each type (ok/seen): {by_type:?}");
    assert!(archives > 0 && textures > 0);
    for (ext, (ok, seen)) in &by_type {
        assert_eq!(ok, seen, "{ext}: first files that don't start with the type's header");
    }
    assert!(by_type.len() >= 6, "{by_type:?}");
}

/// A map from WORLDS and one from WORLDS4 parse with the world reader.
#[test]
fn v2_worlds_parse() {
    let Some((game, keys)) = keyed("V2") else { return };
    for archive in ["WORLDS.rez", "WORLDS4.rez"] {
        let rez = RezArchive::open(game.join(archive), &keys).unwrap();
        let mut maps: Vec<_> = rez.entries().into_iter().filter(|e| e.path.ends_with(".dat")).collect();
        maps.sort_by(|a, b| a.path.cmp(&b.path));
        let e = maps.first().expect("a map");
        let b = rez.read(&e.path).unwrap().unwrap();
        assert_eq!(b[..4], [0x55, 0, 0, 0], "{}", e.path);
        let w = World::parse(&b).unwrap_or_else(|err| panic!("{}: {err}", e.path));
        assert_eq!(w.header.version, 85);
        eprintln!("{archive}: {} parses", e.path);
    }
}

/// A character body and the M4A1's first-person model parse with the
/// model reader.
#[test]
fn v2_character_and_m4a1_models_parse() {
    let Some((game, keys)) = keyed("V2") else { return };
    for (archive, path) in [
        ("CHARS_M_BODY.rez", "chars_m_body/cm_body_alphateam_ki.ltb"),
        ("GUNS_M_PV_AR.rez", "guns_m_pv_ar/pv_ar_m4a1_sh.ltb"),
    ] {
        let rez = RezArchive::open(game.join(archive), &keys).unwrap();
        assert_eq!(rez.header().key_scheme(), Some("V2"), "{archive}");
        let b = rez.read(path).unwrap().unwrap();
        assert_eq!(b[..4], [0x01, 0x00, 0x09, 0x00], "{path}");
        let m = Model::parse(&b).unwrap_or_else(|err| panic!("{path}: {err}"));
        assert!(!m.pieces.is_empty() && !m.nodes.is_empty(), "{path}");
        eprintln!("{path}: {} pieces, {} nodes", m.pieces.len(), m.nodes.len());
    }
}

/// V1: ATTRIBUTES' weapons file decrypts (Twofish) to text the attribute
/// reader parses, with an M4A1 record.
#[test]
fn v1_weapons_attributes_parse() {
    let Some((game, keys)) = keyed("V1") else { return };
    let rez = RezArchive::open(game.join("ATTRIBUTES.rez"), &keys).unwrap();
    assert_eq!(rez.header().key_scheme(), Some("V1"));
    let b = rez.read("attributes/weapons.txt").unwrap().unwrap();
    assert_eq!(b.len(), 9_007_972);
    let doc = Document::parse(&b).unwrap_or_else(|err| panic!("weapons.txt: {err}"));
    let weapons = doc.records("Weapon");
    eprintln!("weapons.txt: {} sections, {} weapon records", doc.sections().len(), weapons.len());
    assert!(!weapons.is_empty());
}
