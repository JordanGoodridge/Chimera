//! The `.rez` reader on the user's Combat Arms install, against the test
//! cases of specs/combat_arms/rez_archive.md. Each test passes with a note
//! when there is no install (`[games.combat_arms] path` in
//! chimera.local.toml) or, for decryption, no key for its scheme.

use std::{io::ErrorKind, path::PathBuf};

use lithtech_formats::rez::{RezArchive, RezCipher, RezKeys, open_dir};
use source_formats::fs::FileSource;

/// The local config's `[games.combat_arms]` table: `$CHIMERA_CONFIG` (or
/// `$MASHUP_CONFIG`), else chimera.local.toml (or mashup.local.toml) at the
/// repository root, as Chimera's `mount::config` looks.
pub(crate) fn config() -> Option<toml::Table> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let file = [std::env::var_os("CHIMERA_CONFIG"), std::env::var_os("MASHUP_CONFIG")]
        .into_iter()
        .flatten()
        .map(PathBuf::from)
        .chain([root.join("chimera.local.toml"), root.join("mashup.local.toml")])
        .find(|p| p.is_file())?;
    let table: toml::Table = std::fs::read_to_string(&file).ok()?.parse().ok()?;
    table.get("games")?.get("combat_arms")?.as_table().cloned()
}

/// The install's `Game/` folder, where the archives are.
pub(crate) fn game_dir() -> Option<PathBuf> {
    let Some(path) = config().and_then(|c| c.get("path")?.as_str().map(String::from)) else {
        eprintln!("skipping: no [games.combat_arms] path in chimera.local.toml");
        return None;
    };
    let path = match path.strip_prefix("~/") {
        Some(rest) => PathBuf::from(std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?).join(rest),
        None => PathBuf::from(path),
    };
    let game = path.join("Game");
    if game.is_dir() {
        Some(game)
    } else {
        eprintln!("skipping: no Combat Arms install at {}", path.display());
        None
    }
}

/// `[games.combat_arms.keys.<scheme>]` from the config (none in the repo).
pub(crate) fn keys() -> RezKeys {
    let mut out = RezKeys::new();
    let Some(keys) = config().and_then(|c| c.get("keys")?.as_table().cloned()) else {
        return out;
    };
    for (scheme, k) in keys {
        let text = |f: &str| k.get(f).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let cipher = RezCipher::from_hex(&text("key"), &text("iv"), &format!("combat_arms {scheme}")).unwrap();
        out.insert(scheme, cipher);
    }
    out
}

/// Test cases 1, 6 and 7: XML.rez's header fields and root block.
#[test]
fn xml_rez_header_and_root() {
    let Some(game) = game_dir() else { return };
    let path = game.join("XML.rez");
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(bytes.len(), 8_378_583);
    assert_eq!((bytes[0], bytes[1], bytes[0x3E], bytes[0x3F]), (0x26, 0x23, 0x21, 0x22));
    assert_eq!(&bytes[0x7C..0x7F], &[0x25, 0x27, 0x2A]);
    assert_eq!(&bytes[0x80..0x89], b"26508971\0");
    assert_eq!(&bytes[0xA2..0xAB], b"16726664\0");
    assert_eq!(
        &bytes[8_378_563..],
        &[
            1, 0, 0, 0, 0xB1, 0x61, 0x7F, 0, 0x89, 0x3B, 0, 0, 0xE2, 0xBC, 0x66, 0x6A, 0x58, 0x4D, 0x4C, 0
        ]
    );
    let rez = RezArchive::open(&path, &RezKeys::new()).unwrap();
    let h = rez.header();
    assert_eq!(h.title, "V1_RESOURCES_KEY");
    assert_eq!(h.version, 1);
    assert_eq!((h.root_offset, h.root_size, h.root_time), (8_378_563, 20, 0));
    assert_eq!(h.next_write, 8_348_081);
    assert_eq!(h.archive_time, 1_785_117_922);
    assert!(h.sorted);
    assert_eq!(h.check, 16_726_664);
    assert!(h.check_matches_fields());
    assert!(rez.files().all(|(p, _)| p.starts_with("xml/") && p.ends_with(".xml")));
    for p in [
        "xml/hud_sbf_frozenguage.xml",
        "xml/inventory_inventory.xml",
        "xml/pass_keyweapon_list.xml",
    ] {
        assert!(rez.entries().iter().any(|e| e.path == p), "missing {p}");
    }
    // Test case 9(e): the first payload starts right after the header.
    assert_eq!(rez.files().map(|(_, f)| f.offset).filter(|&o| o > 0).min(), Some(0xEC));
}

/// Test case 8: WORLDS.rez's signed check numbers.
#[test]
fn worlds_rez_signed_check() {
    let Some(game) = game_dir() else { return };
    let rez = RezArchive::open(game.join("WORLDS.rez"), &RezKeys::new()).unwrap();
    let h = rez.header();
    assert_eq!(h.title, "V2_RESOURCES_KEY");
    assert_eq!(
        (h.root_offset, h.root_size, h.next_write),
        (1_094_698_128, 23, 1_094_695_292)
    );
    assert_eq!(h.check as i32, -2_105_573_853);
    assert!(h.check_matches_fields());
    for p in ["worlds/dredge.dat", "worlds/oilrig.dat", "worlds/piazza.dat"] {
        assert!(rez.entries().iter().any(|e| e.path == p), "missing {p}");
    }
}

/// Every archive opens and its check number is the writer's sum; the
/// keyed ones are those titled `V1`/`V2_RESOURCES_KEY`.
#[test]
fn every_archive_opens() {
    let Some(game) = game_dir() else { return };
    let (mount, locked) = open_dir(&game, &RezKeys::new()).unwrap();
    let mut files = 0;
    let mut keyed = 0;
    for layer in mount.layers() {
        let rez = RezArchive::open(layer.name(), &RezKeys::new()).unwrap();
        assert!(rez.header().check_matches_fields(), "{}: check sum", layer.name());
        keyed += rez.locked() as usize;
        files += rez.entries().len();
    }
    eprintln!(
        "{} archives, {files} files, {} locked without keys",
        mount.layers().count(),
        locked.len()
    );
    assert_eq!(mount.layers().count(), 108);
    assert_eq!(files, 32_024);
    assert_eq!(locked.len(), keyed);
    assert_eq!(locked.len(), 40);
}

/// Plain archives read as stored: maps in WORLDS2/WORLDS3 are readable
/// without keys; a keyed archive's files say they need one.
#[test]
fn plain_worlds_read_and_keyed_ones_are_locked() {
    let Some(game) = game_dir() else { return };
    let (mount, _) = open_dir(&game, &RezKeys::new()).unwrap();
    let mut plain_maps = 0;
    for layer in mount.layers() {
        let name = layer.name().to_lowercase();
        if !(name.ends_with("worlds2.rez") || name.ends_with("worlds3.rez")) {
            continue;
        }
        for e in layer.entries().iter().filter(|e| e.path.ends_with(".dat")) {
            let data = layer.read(&e.path).unwrap().unwrap();
            assert_eq!(data.len() as u64, e.size);
            plain_maps += 1;
        }
    }
    eprintln!("{plain_maps} maps readable without keys");
    assert_eq!(plain_maps, 33);
    assert!(mount.read("worlds2/warehouse.dat").unwrap().len() > 1_000_000);
    assert_eq!(
        mount.read("worlds/junk_flea.dat").unwrap_err().kind(),
        ErrorKind::PermissionDenied
    );
}

/// Test case 9(d): with a V1 key, the XML files that share their first 64
/// ciphertext bytes decrypt to the same readable text. Runs only with a
/// V1 key in the local config.
#[test]
fn v1_key_decrypts_xml() {
    let Some(game) = game_dir() else { return };
    let keys = keys();
    if !keys.contains_key("V1") {
        eprintln!("skipping: no [games.combat_arms.keys.V1] in chimera.local.toml");
        return;
    }
    let rez = RezArchive::open(game.join("XML.rez"), &keys).unwrap();
    let mut entries = rez.entries();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let mut readable = 0;
    for e in entries.iter().filter(|e| e.size >= 64).take(50) {
        let data = rez.read(&e.path).unwrap().unwrap();
        if data[..16]
            .iter()
            .all(|b| b.is_ascii() && (*b >= 0x20 || b.is_ascii_whitespace()))
        {
            readable += 1;
        }
    }
    assert!(
        readable >= 45,
        "only {readable}/50 files start with text: wrong V1 key or IV?"
    );
}
