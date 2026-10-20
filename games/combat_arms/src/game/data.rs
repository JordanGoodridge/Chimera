//! Combat Arms' data: the documents built into the game
//! (games/combat_arms/data/, parsed at run time by Chimera's
//! `weapon::data`, docs/weapon-data.md) and, when the install can be read,
//! its attribute files over them (specs/combat_arms/attribute_data.md,
//! "Defaults and overrides"; the records: `attributes`).
//!
//! Built in: the M4A1 (data/weapons/m4a1.toml, the numbers of
//! weapon_firing.md's worked example), the M9 knife (m9.toml,
//! inventory.md's numbers), the grenades (m67.toml, xm84.toml, m18.toml,
//! m23f.toml, g7a.toml: grenades.md's), one weapon per class from
//! weapon_classes.md's "Typical weapons" (mp5a4, m60, beretta_m92fs,
//! remington_870, l96a1, dragunov_svd: an install weapon of a class we
//! have no document for is skipped, one of a class we have copies it) and
//! the movement numbers
//! (data/movement.toml, `movement::MovementData`).
//!
//! Built in too: the attachment records (data/attachments/mods.toml,
//! attachments.md's tables; `attachments::CaModTable`), each weapon's
//! accepted list and defaults in its `combat_arms:attachments` table.
//!
//! The install: with `[games.combat_arms] path` and a `V1` key in
//! chimera.local.toml, `install_attributes` opens the install's
//! `ATTRIBUTES.rez` and reads the weapons, server, models, marks,
//! character attachments and modification files (`attributes`; the
//! marks, goggles and backpacks go to `characters::records`). Then:
//! - weapons (`merge_weapon_docs`): each of our documents whose ID (after
//!   `combat_arms:`) matches an install weapon's `Name` (case-insensitive,
//!   spaces as `_`: `slug`; `INSTALL_NAMES` for the two of ours named
//!   otherwise, the knife `M9` and the `M-23F`) takes the install's
//!   numbers for every field it maps (`weapon_values`; the models,
//!   `model_values`: `PVModelNormal`, `PVSkin#` and `Pos` as `[models]
//!   view`, `view_skins`, `view_offset`, `HHModel` as `world`,
//!   `MuzzlePos` and `BreachOffset` in `combat_arms:view_model`; the
//!   third-person `AniType`, `LowerAniType` and `HHSkin#` go to
//!   `characters::records`): the record's value, or the spec's
//!   built-in default when the install lacks it, never ours, and the
//!   inventory class from `[WeaponOrder]` as its `slot` (the class - 1;
//!   in no list: ours). Install
//!   weapons that match none of ours are new documents, copied from our
//!   matched document of the same `Guntype` (its parts and hooks) with
//!   their own numbers, after ours in install order (ours keep their
//!   registry indices);
//! - movement (`merge_movement`): the body type `FigureType0` replaces
//!   `character.figure` (`StiffDuration` with it);
//! - attachments (`attachment_table`): the modification file's `Mod#`
//!   records over ours by name (ours not in it stay); a weapon document
//!   with a `combat_arms:attachments` table takes the install weapon's
//!   accepted list from the modification file's `Weapon#` record **of the
//!   same index** (attachments.md Q13: by position; a differing `Name` is
//!   logged; no record: an empty list, logged, Q2), its `Default*Attach`
//!   parts, `DefaultScopeVisible`, `FireSndRadius` and
//!   `SilencedFireRadius1..10` (`merge_weapon_docs_with_mods`).
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
//!   is already there. The zoom fields (`ZoomTime`, `CameraSway*`,
//!   `ReloadAfterFire`, `ZoomedMovePenalty`: weapon_classes.md) go in the
//!   `combat_arms:zoom` table, `AccumPreFire` in `combat_arms:spin`'s;
//!   `ReloadPerBullet` 0 removes our `magazine.shells` (1 keeps our shell
//!   timings: they are the view model's animations). Not mapped (no field
//!   in our data yet): the trigger's `latency` (the pre-fire's updates:
//!   ours, 3 or a primary shotgun's 2, copied with the class's document),
//!   the projectile's `FireOffset`, `TripleShot*` timing,

//!   `ContinuousFire`, sounds and sound radii, render styles,
//!   `DotSightName`, armour, `[Perturb]`, `[Player]` (the client's
//!   speeds are movement.md's: the spec's open question 2), `Model#`/`Hair#`
//!   and worn gear, the modification file's gear lists and per-slot
//!   option fields.
//! - Special functions in the modification file are read by name
//!   (`InstDamage`, `Stun`, ...; targets `All`, `Player`, ...); a number
//!   there is skipped (the spec doesn't say how numbers map).
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
//! - The install adds some 2,000 weapons; a map loads the held
//!   (third-person) models of ours only (`builtin_weapon_ids`: what
//!   players are given), so an install weapon given by other means has
//!   none in third person (its view model loads on demand like any:
//!   `view_model::on_demand`).
//!   The skins' `Hands` placeholder and `PV_L_ARM` / `PV_R_ARM` tags (no
//!   `.dtx`) are empty slots (`skin_paths`).

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

use self::attributes::{
    ATTACHMENTS_FILE, AmmoRecord, PERKS_FILE, SETS_FILE, AttachmentsFile, MARKS_FILE, MOD_FILE, MODELS_FILE, MarksFile, ModFile, ModelsFile, SERVER_FILE, ServerPlayer, ServerTimeWeight, WEAPONS_FILE,
 WeaponRecord,
    WeaponsFile,
};
use super::{attachments::CaModTable, movement::MovementData};
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
    data_file!("m4a1.toml"),
    data_file!("m67.toml"),
    data_file!("xm84.toml"),
    data_file!("m18.toml"),
    data_file!("m23f.toml"),
    data_file!("g7a.toml"),
    // One weapon per class (weapon_classes.md, "Typical weapons").
    data_file!("mp5a4.toml"),
    data_file!("m60.toml"),
    data_file!("beretta_m92fs.toml"),
    data_file!("remington_870.toml"),
    data_file!("l96a1.toml"),
    data_file!("dragunov_svd.toml"),
    // The mission bomb (game_modes.md M3).
    data_file!("xk2.toml"),
    // Special weapons (special_weapons.md): rockets and launchers (2, 3), the
    // flamethrower (4), devices (6), crossbows (7) and the melee weapons beyond
    // the M9 (5).
    data_file!("law.toml"),
    data_file!("rpg-7.toml"),
    data_file!("m136_at4.toml"),
    data_file!("m32.toml"),
    data_file!("m32_incendiarybomb.toml"),
    data_file!("m32_stun.toml"),
    data_file!("m79.toml"),
    data_file!("mzp_1.toml"),
    data_file!("xm_25.toml"),
    data_file!("flamethrower.toml"),
    data_file!("m16a1_mine.toml"),
    data_file!("hornet.toml"),
    data_file!("claymore.toml"),
    data_file!("claymore_controller.toml"),
    data_file!("mobile_grenade.toml"),
    data_file!("mobile_grenade_controller.toml"),
    data_file!("caltrops.toml"),
    data_file!("nerve_confuse_mine.toml"),
    data_file!("auto-turret.toml"),
    data_file!("firstaidkit.toml"),
    data_file!("medkit.toml"),
    data_file!("lgb.toml"),
    data_file!("airstrike_napalm.toml"),
    data_file!("assassin_bowgun_nor.toml"),
    data_file!("assassin_bowgun_expl.toml"),
    data_file!("assassin_bowgun_expl_controller.toml"),
    data_file!("balisong_knife.toml"),
    data_file!("bowie_knife.toml"),
    data_file!("chainsaw.toml"),
    data_file!("dompedrosword.toml"),
    data_file!("gi_shovel.toml"),
    data_file!("guillotine.toml"),
    data_file!("knuckle.toml"),
    data_file!("kukuri.toml"),
    data_file!("m11-tactical_knife.toml"),
    data_file!("machete.toml"),
    data_file!("ninjato.toml"),
    data_file!("sickle.toml"),
    data_file!("sledgehammer.toml"),
    data_file!("strider_axe.toml"),
    data_file!("survival_machete.toml"),
    data_file!("tiger_strider.toml"),
    data_file!("tracker_knife.toml"),
    data_file!("uamc_ka_bar.toml"),
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

/// Whether this process reads nothing of the install at run time
/// (`use_builtin_data_only`; the effects' sounds go silent too).
pub fn builtin_only() -> bool {
    BUILTIN_ONLY.load(Ordering::Relaxed)
}

/// Combat Arms' weapons as data, parsed once per process (and findable by
/// `weapon::data::lookup`): the built-in documents, with the install's
/// attribute data over them when it can be read. Bad built-in data panics
/// (the tests load it); merged data that fails falls back to it.
pub fn weapon_set() -> &'static WeaponSet {
    static SET: OnceLock<&'static WeaponSet> = OnceLock::new();
    SET.get_or_init(|| {
        let builtin = || WeaponSet::parse(WEAPON_DATA, &[]).unwrap_or_else(|e| panic!("Combat Arms weapon data: {e}"));
        let mods = install_attributes().and_then(|a| a.mods.as_ref());
        let merged = install_attributes().and_then(|a| a.weapons.as_ref()).and_then(|w| {
            let mpu = super::movement::movement_data().meters_per_unit as f64;
            let docs = merge_weapon_docs_with_mods(WEAPON_DATA, w, mods, mpu)
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

/// The attachment records: the built-in ones (data/attachments/mods.toml)
/// with the install's modification file over them by name.
pub fn attachment_table() -> &'static CaModTable {
    static TABLE: OnceLock<CaModTable> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = CaModTable::built_in().clone();
        if let Some(mods) = install_attributes().and_then(|a| a.mods.as_ref()) {
            merge_mods(&mut table, mods);
        }
        table
    })
}

/// The modification file's records over `table` by name (ours not in it
/// stay).
pub fn merge_mods(table: &mut CaModTable, mods: &ModFile) {
    for r in &mods.mods {
        if !r.name.is_empty() {
            table.insert(r.clone());
        }
    }
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
    /// The server file's `[TimeWeight]` (`ReloadCheckTime`).
    pub time_weight: Option<ServerTimeWeight>,
    pub models: Option<ModelsFile>,
    /// The modification file (attachments).
    pub mods: Option<ModFile>,
    /// The team marks (`[TeamMark]`: `TeamName#`, `RenderStyle#`).
    pub marks: Option<MarksFile>,
    /// The character attachments (`Goggle#`, `Backpack#`).
    pub attachments: Option<AttachmentsFile>,
    /// The gear and perk records (gear_perks.md), read from the weapons,
    /// models, attachments, modification, perk and set files.
    pub gear: Option<super::gear::records::GearInstall>,
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
    let mods = doc(MOD_FILE);
    let marks = doc(MARKS_FILE);
    let attachments = doc(ATTACHMENTS_FILE);
    let perks = doc(PERKS_FILE);
    let sets = doc(SETS_FILE);
    let any_gear_file = [&weapons, &models, &attachments, &mods, &perks, &sets].iter().any(|d| d.is_some());
    let gear = any_gear_file.then(|| {
        super::gear::records::GearInstall::read(
            weapons.as_ref(),
            models.as_ref(),
            attachments.as_ref(),
            mods.as_ref(),
            perks.as_ref(),
            sets.as_ref(),
        )
    });
    let weapons = weapons.and_then(|d| {
        WeaponsFile::read(&d)
            .map_err(|e| problems.push(format!("{WEAPONS_FILE}: {e}")))
            .ok()
    });
    let attrs = InstallAttributes {
        weapons,
        time_weight: server.as_ref().map(ServerTimeWeight::read),
        server: server.and_then(|d| ServerPlayer::read(&d)),
        models: models.map(|d| ModelsFile::read(&d)),
        mods: mods.map(|d| ModFile::read(&d)),
        marks: marks.and_then(|d| MarksFile::read(&d)),
        attachments: attachments.map(|d| AttachmentsFile::read(&d)),
        gear,
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

/// The server's `ReloadCheckTime` (attribute_data.md Corrections 3): the
/// install's `[TimeWeight]`, else the built-in 0.8.
pub fn reload_check_time() -> f64 {
    install_attributes()
        .and_then(|a| a.time_weight)
        .map_or(ServerTimeWeight::BUILT_IN, |t| t.reload_check_time)
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

/// `CameraSway*` as the zoom hook's `sway`.
fn sway(s: &[f64; 4]) -> Value {
    let mut t = Table::new();
    for (k, v) in ["x_speed", "y_speed", "x_freq", "y_freq"].iter().zip(s) {
        t.insert((*k).into(), Value::Float(*v));
    }
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
/// `:recoil`, `:handling` in game/weapons.rs; `:inventory` in
/// game/inventory.rs; `:hud` in game/hud.rs) in the game's own
/// units (world units, ms, degrees, hit points).
fn weapon_values(w: &WeaponRecord, ammo: Option<(&str, &AmmoRecord)>, length_scale: f64) -> Vec<Set> {
    const DAMAGE: &str = "combat_arms:damage";
    const SPREAD: &str = "combat_arms:spread";
    const RECOIL: &str = "combat_arms:recoil";
    const HANDLING: &str = "combat_arms:handling";
    const INVENTORY: &str = "combat_arms:inventory";
    const HUD: &str = "combat_arms:hud";
    const ZOOM: &str = "combat_arms:zoom";
    const SPIN: &str = "combat_arms:spin";
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
        set(&["hooks", HANDLING, "move_penalty"], w.move_penalty),
        set(&["hooks", INVENTORY, "infinite"], w.infinite_ammo != 0),
        set(&["hooks", INVENTORY, "drop_on_death"], w.drop_by_player_dead),
        set(&["hooks", HUD, "crosshair"], i64::from(w.cross_hair_type.clamp(0, 255))),
        set(&["hooks", HUD, "disable_crosshair"], w.disable_crosshair != 0),
        set(&["hooks", HUD, "duck_perturb"], i64::from(w.duck_perturb)),
        set(&["hooks", HUD, "move_duck_perturb"], i64::from(w.move_duck_perturb)),
        set(&["hooks", HUD, "wide_count"], i64::from(w.wide_fire_max_num)),
        set(&["hooks", HUD, "bomb"], w.gun_type == 8),
        // weapon_classes.md: the zoom fields, the bolt's zoom-out, the
        // minigun's reversible spin.
        set(&["hooks", ZOOM, "zoom_time"], w.zoom_time),
        set(&["hooks", ZOOM, "sway"], sway(&w.camera_sway)),
        set(&["hooks", ZOOM, "reload_after_fire"], w.reload_after_fire != 0),
        set(&["hooks", ZOOM, "zoomed_move_penalty"], w.zoomed_move_penalty),
        set(&["hooks", SPIN, "reversible"], w.accum_pre_fire != 0),
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
            set(&["hooks", DAMAGE, "move_penalty"], a.damage_move_penalty),
            set(&["hooks", DAMAGE, "move_penalty_duration"], a.damage_move_penalty_duration),
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

/// What an install weapon's default ammo sets in the special weapons'
/// hooks (special_weapons.md, "Where the numbers live"): the blast, the
/// over-time sphere, the hit points, the counts and the knife's reach and
/// damages. The projectile records (`FX.TXT`) aren't read: speeds,
/// lifetimes and boxes are ours.
fn special_values(w: &WeaponRecord, a: &AmmoRecord, length_scale: f64) -> Vec<Set> {
    use super::special::{BOLT, DEVICE, PROJECTILE};
    let mut out = Vec::new();
    let area = |hook: &str, out: &mut Vec<Set>| {
        out.extend([
            set(&["hooks", hook, "area_damage"], a.area_damage as f64),
            set(&["hooks", hook, "area_radius"], a.area_damage_radius as f64),
        ]);
        if a.area_damage_type >= 0 {
            out.push(set(&["hooks", hook, "area_type"], i64::from(a.area_damage_type)));
        }
    };
    area(PROJECTILE, &mut out);
    area(DEVICE, &mut out);
    out.push(set(&["hooks", PROJECTILE, "health"], a.health));
    out.push(set(&["hooks", DEVICE, "health"], a.health));
    out.push(set(&["hooks", BOLT, "health"], a.health));
    for hook in [PROJECTILE] {
        out.extend([
            set(&["hooks", hook, "over_time", "rate"], a.prog_damage),
            set(&["hooks", hook, "over_time", "radius"], a.prog_damage_radius),
            set(&["hooks", hook, "over_time", "lifetime"], a.prog_damage_lifetime),
            set(&["hooks", hook, "over_time", "duration"], a.prog_damage_duration),
        ]);
    }
    out.extend([
        set(&["hooks", DEVICE, "spawned"], i64::from(a.spawned_amount.max(0))),
        set(&["hooks", DEVICE, "max"], i64::from(a.max_amount.max(0))),
        set(&["hooks", DEVICE, "caltrops", "rate"], a.prog_damage),
        set(&["hooks", DEVICE, "caltrops", "duration"], a.prog_damage_duration),
        set(&["hooks", DEVICE, "caltrops", "radius"], a.prog_damage_radius),
        set(&["hooks", DEVICE, "caltrops", "stun_penalty"], a.stunned_move_penalty),
        set(&["hooks", DEVICE, "caltrops", "stun_time"], a.stunned_time),
        set(&["hooks", DEVICE, "nerve", "amount"], a.area_damage as f64),
        set(&["hooks", DEVICE, "nerve", "duration"], a.area_damage as f64),
        set(&["hooks", DEVICE, "nerve", "radius"], a.area_damage_radius as f64),
        set(&["hooks", BOLT, "damage", "damage"], a.inst_damage as f64),
        set(&["hooks", BOLT, "damage", "damage_type"], i64::from(a.inst_damage_type)),
    ]);
    // The melee weapons: `Range` and the two buttons' damages.
    const KNIFE: &str = super::inventory::KNIFE_HOOK;
    out.extend([
        set(&["hooks", KNIFE, "range"], w.range as f64 * length_scale),
        set(&["hooks", KNIFE, "primary", "damage"], a.inst_damage as f64),
        set(&["hooks", KNIFE, "secondary", "damage"], a.alt_inst_damage as f64),
    ]);
    out
}

/// An attribute file's skin list as archive paths: entries that aren't
/// texture files (the stock data's `Hands` placeholder and the
/// `PV_L_ARM` / `PV_R_ARM` tags, view_models.md 2) are empty.
pub fn skin_paths(skins: &[String]) -> Vec<String> {
    skins
        .iter()
        .map(|s| {
            let p = super::model::normalize(s);
            if p.ends_with(".dtx") { p } else { String::new() }
        })
        .collect()
}

/// The install weapon's models and their placement: `PVModelNormal`
/// with `PVSkin#` and `Pos` as the view model, `HHModel` as the held one,
/// `MuzzlePos` and `BreachOffset` in our `combat_arms:view_model` table
/// (when the document has one).
fn model_values(w: &WeaponRecord) -> Vec<Set> {
    let vec3 = |v: [f64; 3]| floats(v);
    let mut out = vec![
        set(&["hooks", super::view_model::VIEW, "muzzle_pos"], vec3(w.muzzle_pos)),
        set(&["hooks", super::view_model::VIEW, "breach_offset"], vec3(w.breach_offset)),
    ];
    if !w.pv_model.is_empty() {
        let skins = skin_paths(&w.pv_skins).into_iter().map(Value::String).collect::<Vec<_>>();
        out.extend([
            set(&["models", "view"], super::model::normalize(&w.pv_model)),
            set(&["models", "view_skins"], Value::Array(skins)),
            set(&["models", "view_offset"], vec3(w.pos)),
        ]);
    }
    if !w.hh_model.is_empty() {
        out.push(set(&["models", "world"], super::model::normalize(&w.hh_model)));
    }
    out
}

/// Our IDs (after `combat_arms:`) whose install `Name` is not their
/// slug: the knife is the install's `M9`, the incendiary grenade its
/// `M-23F`.
const INSTALL_NAMES: &[(&str, &str)] = &[("m9_knife", "M9"), ("m23f", "M-23F")];

/// Whether install weapon `name` is our document `id` (without
/// `combat_arms:`): case-insensitively by name ("Our loader"), through
/// its slug or `INSTALL_NAMES`.
fn is_install_name(name: &str, id: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    match INSTALL_NAMES.iter().find(|(ours, _)| *ours == id) {
        Some((_, install)) => name.eq_ignore_ascii_case(install),
        None => slug(name) == id,
    }
}

/// The IDs of our own weapon documents (the weapons a player is given:
/// `inventory`, the modes, the bots). The models a map loads are theirs
/// only: the install adds some 2,000 weapons, too many to load per map.
pub fn builtin_weapon_ids() -> &'static std::collections::HashSet<String> {
    static IDS: OnceLock<std::collections::HashSet<String>> = OnceLock::new();
    IDS.get_or_init(|| {
        WEAPON_DATA
            .iter()
            .filter_map(|(_, text)| {
                let t = text.parse::<Table>().ok()?;
                // A detonator is never seen in a hand (no held model).
                if t.get("hooks").and_then(|h| h.get(super::special::CONTROLLER)).is_some() {
                    return None;
                }
                doc_id(&t).map(String::from)
            })
            .collect()
    })
}

/// The install's record of weapon `id` (ours or one merged from the
/// install: `combat_arms:<slug of its Name>`), when the install's
/// weapons file was read.
pub fn install_weapon(id: &str) -> Option<&'static WeaponRecord> {
    install_weapon_in(install_attributes()?.weapons.as_ref()?, id)
}

/// Weapon `id`'s record in `file` (the merge's match: `combat_arms:` and
/// the slug of its `Name`).
pub fn install_weapon_in<'a>(file: &'a WeaponsFile, id: &str) -> Option<&'a WeaponRecord> {
    let id = id.strip_prefix(NS)?;
    file.weapons.iter().find(|w| is_install_name(&w.name, id))
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

/// What an install weapon (the weapons file's record `w`, index `index`)
/// sets in a document's `combat_arms:attachments` table: its defaults,
/// sound radii and, with the modification file, the accepted list of its
/// record of the same index (attachments.md Q13; none: empty, Q2).
fn attachment_values(w: &WeaponRecord, index: usize, mods: Option<&ModFile>) -> Vec<Set> {
    const ATTACH: &str = super::attachments::ATTACHMENTS;
    let mut defaults = Table::new();
    for (k, v) in [
        ("muzzle", &w.default_muzzle_attach),
        ("sight", &w.default_scope_attach),
        ("magazine", &w.default_magazine_attach),
        ("bullet", &w.default_bullet_attach),
        ("mech_sight", &w.default_mech_sight_attach),
    ] {
        if !v.is_empty() {
            defaults.insert(k.into(), Value::String(v.clone()));
        }
    }
    let mut out = vec![
        set(&["hooks", ATTACH, "defaults"], Value::Table(defaults)),
        set(&["hooks", ATTACH, "default_sight_visible"], w.default_scope_visible),
        set(&["hooks", ATTACH, "fire_sound_radius"], i64::from(w.fire_snd_radius)),
        set(
            &["hooks", ATTACH, "silenced_radius"],
            Value::Array(w.silenced_fire_radius.iter().map(|r| Value::Integer((*r).into())).collect()),
        ),
    ];
    if let Some(mods) = mods {
        let accepts = match mods.weapons.get(index) {
            Some(r) => {
                if !r.name.eq_ignore_ascii_case(&w.name) {
                    info!(
                        "Combat Arms: modification record Weapon{index} is named \"{}\", weapon {index} \"{}\" (linked by position)",
                        r.name, w.name
                    );
                }
                r.accepts
                    .iter()
                    .map(|(name, variant)| {
                        let mut t = Table::new();
                        t.insert("name".into(), Value::String(name.clone()));
                        t.insert("variant".into(), Value::Integer((*variant).into()));
                        Value::Table(t)
                    })
                    .collect()
            }
            None => {
                warn!("Combat Arms: no modification record for weapon {index} \"{}\": it accepts no attachments", w.name);
                Vec::new()
            }
        };
        out.push(set(&["hooks", ATTACH, "accepts"], Value::Array(accepts)));
    }
    out
}

/// Patch one document with an install weapon (and its default ammo when
/// the document's magazine names an ammo).
fn patch(doc: &mut Table, w: &WeaponRecord, file: &WeaponsFile, mods: Option<&ModFile>, meters_per_unit: f64) {
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
    // In f32, as both scales are read: equal scales give exactly 1.
    let length_scale = (meters_per_unit as f32 / doc_length as f32) as f64;
    for s in weapon_values(w, ammo, length_scale) {
        apply(doc, &s);
    }
    if let Some(a) = file.default_ammo(w) {
        for s in special_values(w, a, length_scale) {
            apply(doc, &s);
        }
    }
    // The models (view_models.md 1, 2, 4; characters.md 2): the install's
    // files replace ours, its placement with them.
    if !(w.pv_model.is_empty() && w.hh_model.is_empty()) && !doc.contains_key("models") {
        doc.insert("models".into(), Value::Table(Table::new()));
    }
    for s in model_values(w) {
        apply(doc, &s);
    }
    // `ReloadPerBullet` 0: a magazine reload (our shell timings stay when
    // it is 1: they are the animations').
    if w.reload_per_bullet == 0
        && let Some(Value::Table(m)) = doc.get_mut("magazine")
    {
        m.remove("shells");
    }
    // The inventory class: the order list it is in (our `slot` is the
    // class - 1; in none, ours stays).
    let id = file.weapons.iter().position(|x| std::ptr::eq(x, w));
    if let Some(i) = id {
        for s in attachment_values(w, i, mods) {
            apply(doc, &s);
        }
    }
    if let Some(class) = id.map(|i| file.inventory_class(i)).filter(|c| *c > 0) {
        apply(doc, &set(&["slot"], i64::from(class - 1)));
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
    merge_weapon_docs_with_mods(ours, install, None, meters_per_unit)
}

/// `merge_weapon_docs` with the install's modification file: each
/// document's accepted list from the record of its install weapon's index.
pub fn merge_weapon_docs_with_mods(
    ours: &[(&str, &str)],
    install: &WeaponsFile,
    mods: Option<&ModFile>,
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
            install.weapons.iter().position(|w| is_install_name(&w.name, id))
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
            patch(t, &install.weapons[*i], install, mods, meters_per_unit);
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
        patch(&mut t, w, install, mods, meters_per_unit);
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
    fig.stiff_duration = f.stiff_duration as f32;
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
        for (name, text) in WEAPON_DATA.iter().filter(|(n, _)| n.starts_with("placeholder_")) {
            assert!(text.contains("PLACEHOLDER"), "{name}: no PLACEHOLDER note");
        }
        // The built-in documents parse on their own; `weapon_set()` is
        // process-wide and may already hold the install's merged set
        // (another test in this binary loaded it first).
        let builtin = WeaponSet::parse(WEAPON_DATA, &[]).unwrap();
        assert_eq!(builtin.weapons.len(), WEAPON_DATA.len());
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("5.56mm M4A1"), "5.56mm_m4a1");
        assert_eq!(slug("M4A1"), "m4a1");
        assert_eq!(slug("K-2 (Gold)"), "k-2__gold_");
    }
}
