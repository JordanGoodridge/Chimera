//! The attribute data of the user's Combat Arms install
//! (specs/combat_arms/attribute_data.md, "Where the data sits"), read
//! through our own `.rez` reader. Passes with a note without an install;
//! without a V1 key in chimera.local.toml only the plaintext directory is
//! checked and the loader must fall back to the built-in data. Nothing
//! read here is written anywhere.

use std::collections::BTreeSet;

use chimera_core::mount::config::LocalConfig;
use chimera_sim::weapon::data::WeaponSet;
use combat_arms::game::data::{WEAPON_DATA, attributes::*, merge_weapon_docs, open_attributes, read_attributes};
use combat_arms::game::{GAME, mount};
use lithtech_formats::attributes::Document;
use lithtech_formats::rez::RezArchive;
use chimera_core::mount::FileSource;

fn attributes_rez() -> Option<std::path::PathBuf> {
    let Some(install) = LocalConfig::load().ok().and_then(|c| c.game_path(GAME)) else {
        eprintln!("skipping: no Combat Arms install configured");
        return None;
    };
    let path = install.join("Game").join("ATTRIBUTES.rez");
    if !path.is_file() {
        eprintln!("skipping: no {}", path.display());
        return None;
    }
    Some(path)
}

/// Every key the spec's field tables name for weapons and ammo (lower
/// case, list keys without their index), for open question 1.
fn known(key: &str) -> bool {
    const KNOWN: &[&str] = &[
        "name", "guntype", "clientweapontype", "anitype", "loweranitype", "shotsperclip", "ammoname",
        "infiniteammo", "reloadperbullet", "vectorsperround", "range", "effectrange", "alt_effectrange",
        "damagerfacter", "firedelay", "fireanirate", "rapidfirecount", "continuousfire", "hudfiremode",
        "tripleshotmode", "tripleshotroundnum", "tripleshotdelay", "tripleshotselectdelay", "tripleshotminperturb",
        "tripleshotmaxperturb", "tripleshotperturbincspeed", "tripleshotperturbdecspeed",
        "tripleshotmaxcamrecoilpitch", "tripleshotbasecamrecoilpitch", "minperturb", "maxperturb", "duckperturb",
        "moveduckperturb", "zoomedminperturb", "zoomedmaxperturb", "perturbincspeed", "perturbdecspeed",
        "zoomedperturbincspeed", "zoomedperturbdecspeed", "dynamicperturbfactor", "dynnamicperturbfactorrun",
        "dynnamicperturbfactorwalk", "dynnamicperturbfactorjump", "zoomtime", "movepenalty", "zoomedmovepenalty",
        "maxcamrecoilpitch", "basecamrecoilpitch", "basecamrecoilaccelpitch", "maxcamrecoilyaw", "basecamrecoilyaw",
        "maxcamrecoilwag", "basecamrecoilwagl", "basecamrecoilwagr", "maxrandompitch", "maxrandomwag",
        "basecambackjitter", "backjitterrecover", "cameraswayxspeed", "cameraswayyspeed", "cameraswayxfreq",
        "cameraswayyfreq", "disablecrosshair", "crosshairtype", "noselect", "grenadespeed", "grenadeattach",
        "widefiremaxnum", "widefireangledegree", "firesndradius", "reloadsndradius", "selectsndradius",
        "silencedfireradius", "dropbyplayerdead", "damageweapon", "checkreloadanitime", "hidden", "pos", "muzzlepos",
        "breachoffset", "hhmodelscale", "recoil", "muzzlepvposoff", "muzzlepvrotoff", "muzzlehhposoff",
        "muzzlehhrotoff", "scopepvposoff", "scopepvrotoff", "scopehhposoff", "scopehhrotoff", "pvskin",
        "pvrenderstyle", "hhmodel", "hhskin", "hhrenderstyle", "firesnd", "alt_firesnd", "dryfiresnd", "selectsnd",
        "deselectsnd", "reloadsnd", "silencedfiresnd", "pvmuzzlefxname", "hhmuzzlefxname", "hudactiveimage",
        "hudnoneactiveimage", "hudweaponnameimage", "weaponkillimage", "dotsightname",
    ];
    let base = key.trim_end_matches(|c: char| c.is_ascii_digit());
    KNOWN.contains(&base) || key.starts_with("pvmodel")
}

#[test]
fn install_attribute_data() {
    let Some(path) = attributes_rez() else { return };
    let keys = mount::keys().unwrap();
    let archive = RezArchive::open(&path, &keys).unwrap();
    // The plaintext directory: one ATTRIBUTES folder with the files by role.
    let entries = archive.entries();
    let size = |p: &str| entries.iter().find(|e| e.path == p).map(|e| e.size);
    assert_eq!(size(WEAPONS_FILE), Some(9_007_972));
    assert_eq!(size(SERVER_FILE), Some(2_176));
    assert!(size(MODELS_FILE).is_some());
    assert!(entries.iter().all(|e| e.path.starts_with("attributes/")), "a file outside ATTRIBUTES/");
    eprintln!("ATTRIBUTES.rez: {} files", entries.len());

    if archive.locked() {
        // No V1 key: the loader keeps the built-in data.
        assert_eq!(archive.read(WEAPONS_FILE).unwrap().unwrap_err().kind(), std::io::ErrorKind::PermissionDenied);
        let e = open_attributes(&path).unwrap_err();
        assert!(e.contains("encrypted"), "{e}");
        eprintln!("no V1 key: only the directory was checked");
        return;
    }

    let (attrs, problems) = read_attributes(|name| archive.read(name).unwrap());
    assert!(problems.is_empty(), "{problems:?}");
    let w = attrs.weapons.as_ref().unwrap();
    eprintln!("weapons file: {} weapons, {} ammo, {} gear", w.weapons.len(), w.ammo.len(), w.gear.len());
    let m4 = w.weapon("M4A1").expect("an M4A1 record");
    assert_eq!(m4.gun_type, 4);
    assert!(w.default_ammo(m4).is_some());
    let server = attrs.server.as_ref().unwrap();
    eprintln!("[Player]: {server:?}");
    let models = attrs.models.as_ref().unwrap();
    assert!(models.figure_types.len() >= 2);

    // Open question 1: keys of weapon records that no field table names.
    let doc = Document::parse(&archive.read(WEAPONS_FILE).unwrap().unwrap()).unwrap();
    let unknown: BTreeSet<_> = doc
        .records("Weapon")
        .iter()
        .flat_map(|s| s.iter().map(|(k, _)| k.to_string()))
        .filter(|k| !known(k))
        .collect();
    eprintln!("weapon keys in no field table ({}): {unknown:?}", unknown.len());

    // The merge loads with our documents.
    let docs = merge_weapon_docs(WEAPON_DATA, w, 0.01).unwrap();
    let refs: Vec<(&str, &str)> = docs.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
    let set = WeaponSet::parse(&refs, &[]).unwrap();
    eprintln!("merged: {} weapons", set.weapons.len());
}
