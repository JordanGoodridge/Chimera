//! Combat Arms' data: the documents built into the game
//! (games/combat_arms/data/, parsed at run time by Chimera's
//! `weapon::data`, docs/weapon-data.md) and, when the install can be read,
//! its attribute files over them (specs/combat_arms/attribute_data.md,
//! "Defaults and overrides"; the records: `attributes`).
//!
//! Built in: a placeholder rifle (data/weapons/placeholder_*.toml, every
//! number marked PLACEHOLDER: ours, not the game's), the M9 knife
//! (m9.toml, inventory.md's numbers) and the movement numbers
//! (data/movement.toml, `movement::MovementData`).
//!
//! The install: with `[games.combat_arms] path` and a `V1` key in
//! chimera.local.toml, `install_attributes` opens the install's
//! `ATTRIBUTES.rez` and reads the weapons, server and models files
//! (`attributes`). Then:
//! - weapons (`merge_weapon_docs`): each of our documents whose ID (after
//!   `combat_arms:`) matches an install weapon's `Name` (case-insensitive,
//!   spaces as `_`: `slug`) takes the install's numbers for every field
//!   it maps (`weapon_values`): the record's value, or the spec's
//!   built-in default when the install lacks it, never ours. Install
//!   weapons that match none of ours are new documents, copied from our
//!   matched document of the same `Guntype` (its parts and hooks) with
//!   their own numbers, after ours in install order (ours keep their
//!   registry indices);
//! - movement (`merge_movement`): the body type `FigureType0` replaces
//!   `character.figure`.
//! Without an install or key, or when a document fails to parse (logged
//! with its name and line), the built-in data is used unchanged.
//! `weapon_set` and `movement::movement_data` are the merged data;
//! `use_builtin_data_only` (the test harness, `CaSim`) turns the install
//! off for the process.
//!
//! The other modules read data only through this one: `weapons` registers
//! the set, `inventory` names what a player starts with.
//!
//! Known shortcuts:
//! - A mapped value is written only where our document already has the
//!   table it goes in (the weapon's parts and hooks are ours; the install
//!   changes their numbers), and `trigger.per_press` only where the key
//!   is already there. Not mapped (no field in our data yet): `MovePenalty`
//!   and `ZoomedMovePenalty` (movement's `HeldMovePenalty`), `ZoomTime`,
//!   `ReloadPerBullet`, `InfiniteAmmo`, `TripleShot*` timing, sounds and
//!   sound radii, models, HUD fields, armour, `[Perturb]`, `[Player]`
//!   (the client's speeds are movement.md's: the spec's open question 2),
//!   `Model#`/`Hair#` and worn gear (attachments' spec).
//! - Ammo is patched through the weapon that uses it as its default
//!   (`ammo.<id>.max` = `MaxAmount` - `ShotsPerClip`, the clip being part
//!   of the total: weapon_firing.md, "Ammo"); our ammo IDs aren't matched
//!   to ammo records on their own.
//! - Lengths are converted with movement's `meters_per_unit` and the
//!   document's own `units.length` (not its base's).
//! - Only `FigureType0` (the male body, the reference character) is used:
//!   our movement data has one figure.
//! - Test case 29's install document has no `Ammo0`, which case 26 says
//!   fails to load; its test adds one.

pub mod attributes;

use std::{
    io,
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use bevy::prelude::*;
use lithtech_formats::attributes::Document;
use toml::{Table, Value};

use self::attributes::{AmmoRecord, MODELS_FILE, ModelsFile, SERVER_FILE, ServerPlayer, WEAPONS_FILE, WeaponRecord, WeaponsFile};
use super::movement::MovementData;
use crate::weapon::data::WeaponSet;

macro_rules! data_file {
    ($name:literal) => {
        ($name, include_str!(concat!("../../data/weapons/", $name)))
    };
}

/// Combat Arms' weapon documents (games/combat_arms/data/weapons), built
/// in, in registration order (the network names weapons by registry
/// index).
pub const WEAPON_DATA: &[(&str, &str)] = &[
    data_file!("m9.toml"),
    data_file!("placeholder_rifle.toml"),
];

/// The namespace of our weapon and ammo IDs.
const NS: &str = "combat_arms:";

static BUILTIN_ONLY: AtomicBool = AtomicBool::new(false);

/// Never read the install's attribute data in this process (scenario
/// tests: their numbers are the built-in ones). Call before the data is
/// first used.
pub fn use_builtin_data_only() {
    BUILTIN_ONLY.store(true, Ordering::Relaxed);
}

/// Combat Arms' weapons as data, parsed once per process (and findable by
/// `weapon::data::lookup`): the built-in documents, with the install's
/// attribute data over them when it can be read. Bad built-in data panics
/// (the tests load it); merged data that fails falls back to it.
pub fn weapon_set() -> &'static WeaponSet {
    static SET: OnceLock<&'static WeaponSet> = OnceLock::new();
    SET.get_or_init(|| {
        let builtin = || WeaponSet::parse(WEAPON_DATA, &[]).unwrap_or_else(|e| panic!("Combat Arms weapon data: {e}"));
        let merged = install_attributes().and_then(|a| a.weapons.as_ref()).and_then(|w| {
            let docs = merge_weapon_docs(WEAPON_DATA, w, super::movement::movement_data().meters_per_unit as f64)
                .map_err(|e| warn!("Combat Arms: the install's weapons weren't merged: {e}"))
                .ok()?;
            let refs: Vec<(&str, &str)> = docs.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
            WeaponSet::parse(&refs, &[])
                .map_err(|e| warn!("Combat Arms: the install's weapons don't load, using the built-in ones: {e}"))
                .ok()
        });
        let set: &'static WeaponSet = Box::leak(Box::new(merged.unwrap_or_else(builtin)));
        crate::weapon::data::publish(set);
        set
    })
}

/// The movement data with the install's body type over it (called by
/// `movement::movement_data`).
pub fn with_install_movement(mut data: MovementData) -> MovementData {
    if let Some(models) = install_attributes().and_then(|a| a.models.as_ref()) {
        merge_movement(&mut data, models);
    }
    data
}

/// What was read from the install's attribute files: each `None` when its
/// file is missing, locked or fails to parse or load.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InstallAttributes {
    pub weapons: Option<WeaponsFile>,
    pub server: Option<ServerPlayer>,
    pub models: Option<ModelsFile>,
}

/// Parse one attribute document, errors naming it and the line.
pub fn parse_document(name: &str, bytes: &[u8]) -> Result<Document, String> {
    Document::parse(bytes).map_err(|e| format!("{name}: {e}"))
}

/// Read the attribute files through `read` (an in-archive path to its
/// bytes). Returns the records and, for each file that couldn't be used,
/// why (the caller logs them).
pub fn read_attributes(read: impl Fn(&str) -> io::Result<Vec<u8>>) -> (InstallAttributes, Vec<String>) {
    let mut problems = Vec::new();
    let mut doc = |name: &str| match read(name).map_err(|e| format!("{name}: {e}")).and_then(|b| parse_document(name, &b)) {
        Ok(d) => Some(d),
        Err(e) => {
            problems.push(e);
            None
        }
    };
    let weapons = doc(WEAPONS_FILE);
    let server = doc(SERVER_FILE);
    let models = doc(MODELS_FILE);
    let weapons = weapons.and_then(|d| {
        WeaponsFile::read(&d)
            .map_err(|e| problems.push(format!("{WEAPONS_FILE}: {e}")))
            .ok()
    });
    let attrs = InstallAttributes {
        weapons,
        server: server.and_then(|d| ServerPlayer::read(&d)),
        models: models.map(|d| ModelsFile::read(&d)),
    };
    (attrs, problems)
}

/// The install's attribute data, read once per process: `None` without
/// an install, with `use_builtin_data_only`, or when `ATTRIBUTES.rez`
/// can't be read (no V1 key: logged).
pub fn install_attributes() -> Option<&'static InstallAttributes> {
    static ATTRS: OnceLock<Option<InstallAttributes>> = OnceLock::new();
    ATTRS
        .get_or_init(|| {
            if BUILTIN_ONLY.load(Ordering::Relaxed) {
                return None;
            }
            let install = crate::mount::config::LocalConfig::load().ok()?.game_path(super::GAME)?;
            let path = install.join("Game").join("ATTRIBUTES.rez");
            if !path.is_file() {
                return None;
            }
            let attrs = open_attributes(&path)
                .map_err(|e| info!("Combat Arms: built-in data only: {e}"))
                .ok()?;
            Some(attrs)
        })
        .as_ref()
}

/// Read an install's `ATTRIBUTES.rez` with the configured keys. `Err`
/// when it can't be opened or is locked for want of its key.
pub fn open_attributes(path: &std::path::Path) -> Result<InstallAttributes, String> {
    let keys = super::mount::keys()?;
    let archive = lithtech_formats::rez::RezArchive::open(path, &keys).map_err(|e| e.to_string())?;
    if archive.locked() {
        return Err(format!(
            "{}: encrypted, and no key for it in chimera.local.toml",
            path.display()
        ));
    }
    let mut mount = crate::mount::Mount::default();
    mount.push(archive);
    let (attrs, problems) = read_attributes(|name| mount.read(name));
    for p in problems {
        warn!("Combat Arms: the install's {p}; using the built-in data for it");
    }
    Ok(attrs)
}

/// Our ID for an install name: lower case, anything but letters, digits,
/// `.`, `-` and `_` as `_` (`5.56mm M4A1` is `5.56mm_m4a1`).
pub fn slug(name: &str) -> String {
    name.chars()
        .map(|c| match c.to_ascii_lowercase() {
            c @ ('a'..='z' | '0'..='9' | '.' | '-' | '_') => c,
            _ => '_',
        })
        .collect()
}

/// When a mapped value is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum When {
    /// The table it goes in exists.
    Table,
    /// The key itself is already there.
    Key,
}

/// One mapped value: where it goes in a weapon document.
struct Set {
    path: Vec<String>,
    value: Value,
    when: When,
}

fn set(path: &[&str], value: impl Into<Value>) -> Set {
    Set {
        path: path.iter().map(|s| s.to_string()).collect(),
        value: value.into(),
        when: When::Table,
    }
}

fn floats(v: impl IntoIterator<Item = f64>) -> Value {
    Value::Array(v.into_iter().map(Value::Float).collect())
}

fn cone(c: &attributes::Cone) -> Value {
    let mut t = Table::new();
    t.insert("min".into(), Value::Integer(c.min.into()));
    t.insert("max".into(), Value::Integer(c.max.into()));
    t.insert("inc".into(), Value::Float(c.inc));
    t.insert("dec".into(), Value::Float(c.dec));
    Value::Table(t)
}

fn state(p: &attributes::AmmoPerturb) -> Value {
    let mut t = Table::new();
    for (k, v) in [
        ("fire", p.fire),
        ("fire_move", p.fire_move),
        ("fire_duck", p.fire_duck),
        ("fire_move_duck", p.fire_move_duck),
        ("walk", p.walk),
        ("run", p.run),
        ("jump", p.jump),
    ] {
        t.insert(k.into(), Value::Float(v));
    }
    Value::Table(t)
}

/// What an install weapon (and its default ammo, `ammo_id` in our
/// document) sets in a weapon document: our parts (`trigger`, `magazine`,
/// `hitscan`, `ammo`) in the document's units (`length_scale`: metres per
/// world unit over the document's metres per length unit), and the
/// parameters of Combat Arms' hooks (`combat_arms:damage`, `:spread`,
/// `:recoil`, games/combat_arms/src/game/weapons.rs) in the game's own
/// units (world units, ms, degrees, hit points).
fn weapon_values(w: &WeaponRecord, ammo: Option<(&str, &AmmoRecord)>, length_scale: f64) -> Vec<Set> {
    const DAMAGE: &str = "combat_arms:damage";
    const SPREAD: &str = "combat_arms:spread";
    const RECOIL: &str = "combat_arms:recoil";
    let mut out = vec![
        set(&["trigger", "automatic"], w.rapid_fire_count != 1),
        Set {
            when: When::Key,
            ..set(&["trigger", "per_press"], i64::from(w.rapid_fire_count.max(0)))
        },
        set(&["magazine", "clip"], i64::from(w.shots_per_clip)),
        set(&["hitscan", "range"], w.range as f64 * length_scale),
        set(&["hitscan", "pellets"], i64::from(w.vectors_per_round)),
        set(&["hooks", DAMAGE, "roll"], w.damage_roll()),
        set(&["hooks", DAMAGE, "effect_range"], floats(w.effect_range.iter().map(|&e| e as f64))),
        set(&["hooks", DAMAGE, "fire_delay"], w.fire_delay as f64),
        set(&["hooks", SPREAD, "cone"], cone(&w.perturb)),
        set(&["hooks", SPREAD, "zoomed_cone"], cone(&w.zoomed)),
        set(&["hooks", SPREAD, "dynamic_factor"], w.dynamic_perturb_factor),
        set(&["hooks", RECOIL, "pitch"], w.base_cam_recoil_pitch),
        set(&["hooks", RECOIL, "accel_pitch"], w.base_cam_recoil_accel_pitch),
        set(&["hooks", RECOIL, "max_pitch"], w.max_cam_recoil_pitch),
        set(&["hooks", RECOIL, "random_pitch"], w.max_random_pitch),
        set(&["hooks", RECOIL, "wag_r"], w.base_cam_recoil_wag_r),
        set(&["hooks", RECOIL, "wag_l"], w.base_cam_recoil_wag_l),
        set(&["hooks", RECOIL, "max_wag"], w.max_cam_recoil_wag),
        set(&["hooks", RECOIL, "random_wag"], w.max_random_wag),
        set(&["hooks", RECOIL, "back_jitter"], w.base_cam_back_jitter),
        set(&["hooks", RECOIL, "back_jitter_recover"], w.back_jitter_recover),
    ];
    if w.triple_shot_mode != 0 {
        out.extend([
            set(&["hooks", SPREAD, "burst_cone"], cone(&w.triple_shot)),
            set(&["hooks", RECOIL, "burst_pitch"], w.triple_shot_base_cam_recoil_pitch),
            set(&["hooks", RECOIL, "burst_max_pitch"], w.triple_shot_max_cam_recoil_pitch),
        ]);
    }
    if let Some((id, a)) = ammo {
        out.extend([
            set(&["ammo", id, "max"], i64::from((a.max_amount - w.shots_per_clip).max(0))),
            set(&["hooks", DAMAGE, "damage"], a.inst_damage as f64),
            set(&["hooks", DAMAGE, "damage_type"], i64::from(a.inst_damage_type)),
            set(&["hooks", DAMAGE, "range_damage"], floats(a.range_damage.iter().copied())),
            set(&["hooks", SPREAD, "state"], state(&a.perturb)),
            set(&["hooks", SPREAD, "zoomed_state"], state(&a.zoomed_perturb)),
            set(&["hooks", SPREAD, "dec_delay"], a.dec_delay as f64),
            set(&["hooks", RECOIL, "jitter_dec_delay"], a.jitter_dec_delay as f64),
            set(&["hooks", RECOIL, "jitter_wait_recover"], a.jitter_wait_recover),
            set(&["hooks", RECOIL, "jitter_recover"], a.jitter_recover),
        ]);
    }
    out
}

/// Write `s` into `doc` when its rule allows; whether it was written.
fn apply(doc: &mut Table, s: &Set) -> bool {
    let Some((last, parents)) = s.path.split_last() else {
        return false;
    };
    let mut t = doc;
    for p in parents {
        t = match t.get_mut(p) {
            Some(Value::Table(x)) => x,
            _ => return false,
        };
    }
    if s.when == When::Key && !t.contains_key(last) {
        return false;
    }
    t.insert(last.clone(), s.value.clone());
    true
}

fn doc_id(doc: &Table) -> Option<&str> {
    doc.get("id")?.as_str()
}

/// Patch one document with an install weapon (and its default ammo when
/// the document's magazine names an ammo).
fn patch(doc: &mut Table, w: &WeaponRecord, file: &WeaponsFile, meters_per_unit: f64) {
    let doc_length = doc
        .get("units")
        .and_then(|u| u.get("length"))
        .and_then(|l| l.as_float().or_else(|| l.as_integer().map(|i| i as f64)))
        .unwrap_or(1.0);
    let ammo_id = doc.get("magazine").and_then(|m| m.get("ammo")).and_then(|a| a.as_str()).map(String::from);
    let ammo = match (&ammo_id, file.default_ammo(w)) {
        (Some(id), Some(a)) => Some((id.as_str(), a)),
        _ => None,
    };
    for s in weapon_values(w, ammo, meters_per_unit / doc_length) {
        apply(doc, &s);
    }
}

/// Our weapon documents with the install's weapons merged in ("Defaults
/// and overrides", "Our loader"): `(name, TOML text)` in registration
/// order, ours first. `meters_per_unit`: metres per world unit.
pub fn merge_weapon_docs(
    ours: &[(&str, &str)],
    install: &WeaponsFile,
    meters_per_unit: f64,
) -> Result<Vec<(String, String)>, String> {
    let mut docs = Vec::new();
    for (name, text) in ours {
        let t: Table = text.parse().map_err(|e| format!("{name}: {e}"))?;
        docs.push((name.to_string(), t));
    }
    // Which install weapon each of ours is (`None`: no match).
    let matched: Vec<Option<usize>> = docs
        .iter()
        .map(|(_, t)| {
            let id = doc_id(t)?.strip_prefix(NS)?;
            install.weapons.iter().position(|w| !w.name.is_empty() && slug(&w.name) == id)
        })
        .collect();
    let mut ids: std::collections::HashSet<String> = docs.iter().filter_map(|(_, t)| doc_id(t).map(String::from)).collect();
    let mut ammo_ids: std::collections::HashSet<String> = docs
        .iter()
        .filter_map(|(_, t)| t.get("ammo")?.as_table())
        .flat_map(|a| a.keys().cloned())
        .collect();
    for ((_, t), m) in docs.iter_mut().zip(&matched) {
        if let Some(i) = m {
            patch(t, &install.weapons[*i], install, meters_per_unit);
        }
    }
    // New records: copies of our matched document of the same class.
    let mut new = Vec::new();
    let mut skipped = 0usize;
    for (i, w) in install.weapons.iter().enumerate() {
        if w.name.is_empty() || matched.contains(&Some(i)) {
            continue;
        }
        let id = format!("{NS}{}", slug(&w.name));
        let template = matched
            .iter()
            .zip(&docs)
            .find(|(m, _)| m.is_some_and(|j| install.weapons[j].gun_type == w.gun_type))
            .map(|(_, (_, t))| t);
        let (Some(template), Some(ammo)) = (template, file_default_ammo(install, w)) else {
            skipped += 1;
            continue;
        };
        if !ids.insert(id.clone()) {
            continue;
        }
        let mut t = template.clone();
        t.insert("id".into(), Value::String(id.clone()));
        t.remove("ammo");
        let ammo_id = format!("{NS}{}", slug(&ammo.name));
        if ammo_ids.insert(ammo_id.clone()) {
            let mut table = Table::new();
            let mut entry = Table::new();
            entry.insert("max".into(), Value::Integer(0));
            table.insert(ammo_id.clone(), Value::Table(entry));
            t.insert("ammo".into(), Value::Table(table));
        }
        if let Some(Value::Table(m)) = t.get_mut("magazine") {
            m.insert("ammo".into(), Value::String(ammo_id));
        }
        patch(&mut t, w, install, meters_per_unit);
        new.push((format!("install:{}", w.name), t));
    }
    if skipped > 0 {
        info!("Combat Arms: {skipped} install weapons skipped (no weapon of their class in our data, or no ammo)");
    }
    docs.extend(new);
    docs.into_iter()
        .map(|(n, t)| toml::to_string(&t).map(|s| (n.clone(), s)).map_err(|e| format!("{n}: {e}")))
        .collect()
}

/// A weapon's default ammo record; a weapon naming ammo none of which
/// resolves is logged ("Edge cases").
fn file_default_ammo<'a>(file: &'a WeaponsFile, w: &WeaponRecord) -> Option<&'a AmmoRecord> {
    let a = file.default_ammo(w);
    if a.is_none() && w.names_ammo {
        warn!("Combat Arms: install weapon \"{}\" names no ammo that exists", w.name);
    }
    a
}

/// The install's body type `FigureType0` over our movement data's
/// figure.
pub fn merge_movement(data: &mut MovementData, models: &ModelsFile) {
    let Some(f) = models.figure_types.first() else {
        return;
    };
    let fig = &mut data.character.figure;
    fig.dash_speed = f.dash_speed as f32;
    fig.dash_gauge = f.dash_gauge as f32;
    fig.gauge_recovery = f.gauge_rec as f32;
    fig.max_stamina = f.max_stamina as f32;
    fig.variable_speed = f.variable_speed as f32;
    fig.variable_weight = f.variable_weight as f32;
    fig.variable_jump = f.variable_jump as f32;
}

/// Combat Arms' data, loaded as the game starts (so a bad file fails at
/// once, not at the first weapon given).
pub struct CaDataPlugin;

impl Plugin for CaDataPlugin {
    fn build(&self, _app: &mut App) {
        weapon_set();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every file in data/weapons is built in (`WEAPON_DATA`), as it is
    /// on disk.
    #[test]
    fn every_data_file_is_built_in() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/weapons");
        let files = WeaponSet::read_dir(&dir).unwrap();
        for (name, text) in &files {
            let built_in = WEAPON_DATA.iter().find(|(n, _)| n == name);
            assert_eq!(built_in.map(|d| d.1), Some(text.as_str()), "{name} not in `WEAPON_DATA`");
        }
        assert_eq!(files.len(), WEAPON_DATA.len());
    }

    /// Every placeholder document says its numbers are placeholders until
    /// the specs replace them.
    #[test]
    fn placeholder_data_says_so() {
        use_builtin_data_only();
        for (name, text) in WEAPON_DATA.iter().filter(|(n, _)| n.starts_with("placeholder_")) {
            assert!(text.contains("PLACEHOLDER"), "{name}: no PLACEHOLDER note");
        }
        assert_eq!(weapon_set().weapons.len(), 2);
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("5.56mm M4A1"), "5.56mm_m4a1");
        assert_eq!(slug("M4A1"), "m4a1");
        assert_eq!(slug("K-2 (Gold)"), "k-2__gold_");
    }
}
