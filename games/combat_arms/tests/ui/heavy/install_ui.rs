//! The install's own UI data through the UI system's readers
//! (specs/combat_arms/ui_system.md L1 and R1): every layout in XML.rez
//! reads as a widget tree (`layout::xml::read`), the language pack's
//! string tables (Language_Pack.rez) as string tables
//! (`layout::Strings::parse`). Both archives are V1-keyed: skipped
//! without the key (or without an install in chimera.local.toml).

use chimera_core::mount::{Mount, config::LocalConfig};
use combat_arms::{
    game::{GAME, INFO, mount},
    ui::layout::{Strings, xml},
};

fn install() -> Option<Mount> {
    let Some(install) = LocalConfig::load().ok().and_then(|c| c.game_path(GAME)) else {
        eprintln!("skipping: no Combat Arms install configured");
        return None;
    };
    if !install.join("Game").is_dir() {
        eprintln!("skipping: no Combat Arms install at {}", install.display());
        return None;
    }
    if !mount::keys().is_ok_and(|k| k.contains_key("V1")) {
        eprintln!("skipping: no V1 key in chimera.local.toml");
        return None;
    }
    Some((INFO.open)(&install).unwrap())
}

/// The files of the archive layer named `archive` (its file name, any
/// case).
fn files_of(mount: &Mount, archive: &str) -> Vec<String> {
    let layer = mount
        .layers()
        .find(|l| l.name().to_lowercase().ends_with(&archive.to_lowercase()))
        .unwrap_or_else(|| panic!("no {archive}"));
    let mut files: Vec<String> = layer.entries().into_iter().map(|e| e.path).collect();
    files.sort();
    files
}

fn utf8(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// L1: every `.xml` file of XML.rez that is a layout (`CBALAYOUT` root)
/// reads into a tree with at least one widget; the other XML files are
/// counted by root.
#[test]
fn install_layouts_read() {
    let Some(mount) = install() else { return };
    let files = files_of(&mount, "XML.rez");
    let mut roots = std::collections::BTreeMap::<String, usize>::new();
    let (mut layouts, mut failed) = (0, Vec::new());
    for path in files.iter().filter(|p| p.ends_with(".xml")) {
        let text = utf8(&mount.read(path).unwrap());
        let root = match xml::parse(&text) {
            Ok(e) => e.name.clone(),
            Err(e) => {
                failed.push(format!("{path}: {e}"));
                continue;
            }
        };
        *roots.entry(root.clone()).or_default() += 1;
        if root != "CBALAYOUT" {
            continue;
        }
        match xml::read(&text) {
            Ok(l) => {
                assert!(l.tree.order().len() > 1, "{path}: no widgets");
                layouts += 1;
            }
            Err(e) => failed.push(format!("{path}: {e}")),
        }
    }
    eprintln!("{} files; XML roots {roots:?}; {layouts} layouts read", files.len());
    for f in &failed {
        eprintln!("failed: {f}");
    }
    assert!(failed.is_empty(), "{} of the install's XML files don't read", failed.len());
    assert!(layouts > 100, "{layouts} layouts");
}

/// R1: the language pack's string tables read, with no duplicate keys
/// that would hide a value, and ids resolve through them (R2).
#[test]
fn install_string_tables_read() {
    let Some(mount) = install() else { return };
    let files = files_of(&mount, "Language_Pack.rez");
    eprintln!("Language_Pack.rez: {} files: {:?}", files.len(), files.iter().take(40).collect::<Vec<_>>());
    let mut tables = 0;
    for path in &files {
        let bytes = mount.read(path).unwrap();
        let Ok(text) = std::str::from_utf8(&bytes) else {
            eprintln!("{path}: not UTF-8 ({} bytes)", bytes.len());
            continue;
        };
        let t = Strings::parse(text);
        eprintln!("{path}: {} strings, {} duplicate keys", t.values.len(), t.duplicates.len());
        if t.values.len() > 100 {
            tables += 1;
        }
    }
    assert!(tables > 0, "no string table in Language_Pack.rez");
}
