//! `CaData`: the data one app plays Combat Arms with, a `Resource` its
//! plugins read (architecture review E2, X3). It was process-wide: a
//! switch (`use_builtin_data_only`) and a pile of `OnceLock` caches
//! (weapon set, attachments, character records, gear, movement,
//! missions), so every app in a process (a `NetSim`'s server and clients,
//! the tests of one binary) shared whichever data loaded first.
//!
//! A `CaData` holds the install's attribute files (or none) and what is
//! derived from them, each derived table filled on first use *inside this
//! value*, so two apps with different data never see each other's:
//!
//! - `CaData::builtin()`: the built-in documents only, never the install
//!   (the harness's `CaSim`, scenario tests);
//! - `CaData::install()`: the install's attribute data over the built-in
//!   when it can be read (the games' default, `CaData::default()`);
//! - `CaData::builtin_with_weapons(docs)`: the built-in data with other
//!   weapon documents (tests, minigame tuning).
//!
//! The first two are process-wide singletons (the install's
//! `ATTRIBUTES.rez` takes seconds to read and decrypt, and is the same
//! for every app, so it is read once; the weapon set built from it is
//! leaked once: weapon IDs are `&'static str`). They are immutable once
//! built, and the app's choice between them is the resource, not a
//! switch.
//!
//! Known shortcuts:
//! - Map loaders (`map::MapLoader`, `MapDressing`) are plain function
//!   pointers with no app to ask: they use `CaData::install()`, as they
//!   read the install anyway.

use std::{
    collections::BTreeSet,
    ops::Deref,
    sync::{Arc, OnceLock},
};

use bevy::prelude::*;
use toml::Table;

use super::{
    InstallAttributes, WEAPON_DATA, WeaponRecord, doc_id, install_weapon_in, is_install_name, merge_mods,
    merge_weapon_docs_with_mods, open_attributes, with_install_movement,
};
use crate::{mount::Mount, weapon::data::WeaponSet};

use super::super::{
    attachments::CaModTable,
    characters::records::CharacterData,
    gear::records::GearData,
    missions::InstallMissions,
    movement::{MovementData, movement_data},
};

/// The data an app plays Combat Arms with (module doc). Cheap to clone;
/// derefs to `CaDataInner`.
#[derive(Resource, Clone)]
pub struct CaData(Arc<CaDataInner>);

impl Default for CaData {
    fn default() -> Self {
        Self::install()
    }
}

impl Deref for CaData {
    type Target = CaDataInner;
    fn deref(&self) -> &CaDataInner {
        &self.0
    }
}

/// What a `CaData` holds.
pub struct CaDataInner {
    builtin_only: bool,
    attributes: Option<InstallAttributes>,
    /// The built-in weapon documents of this data.
    docs: Vec<(String, String)>,
    weapon_set: OnceLock<&'static WeaponSet>,
    /// The documents `weapon_set` was parsed from (merged or built in).
    weapon_docs: OnceLock<Vec<(String, String)>>,
    attachments: OnceLock<CaModTable>,
    movement: OnceLock<MovementData>,
    gear: OnceLock<GearData>,
    characters: OnceLock<CharacterData>,
    missions: OnceLock<Option<InstallMissions>>,
    builtin_ids: OnceLock<BTreeSet<String>>,
}

impl CaData {
    fn new(builtin_only: bool, attributes: Option<InstallAttributes>, docs: Vec<(String, String)>) -> Self {
        Self(Arc::new(CaDataInner {
            builtin_only,
            attributes,
            docs,
            weapon_set: OnceLock::new(),
            weapon_docs: OnceLock::new(),
            attachments: OnceLock::new(),
            movement: OnceLock::new(),
            gear: OnceLock::new(),
            characters: OnceLock::new(),
            missions: OnceLock::new(),
            builtin_ids: OnceLock::new(),
        }))
    }

    fn built_in_docs() -> Vec<(String, String)> {
        WEAPON_DATA.iter().map(|(n, t)| (n.to_string(), t.to_string())).collect()
    }

    /// The built-in data only: never the install's attribute files, its
    /// mount or its sounds (scenario tests: their numbers are ours).
    pub fn builtin() -> Self {
        Self::builtin_static().clone()
    }

    /// `builtin()` by reference: its tables are read-only tables for code
    /// with no app to ask (`characters::records::builtin`).
    pub fn builtin_static() -> &'static Self {
        static BUILTIN: OnceLock<CaData> = OnceLock::new();
        BUILTIN.get_or_init(|| Self::new(true, None, Self::built_in_docs()))
    }

    /// The install's attribute data over the built-in when it can be read
    /// (`[games.combat_arms] path` and the `V1` key in the local config);
    /// else the built-in numbers, with the install's files still
    /// mountable. Read once per process.
    pub fn install() -> Self {
        Self::install_static().clone()
    }

    /// `install()` by reference (the lobby's tables).
    pub fn install_static() -> &'static Self {
        static INSTALL: OnceLock<CaData> = OnceLock::new();
        INSTALL
            .get_or_init(|| {
                let attributes = (|| {
                    let install = crate::mount::config::LocalConfig::load().ok()?.game_path(super::super::GAME)?;
                    let path = install.join("Game").join("ATTRIBUTES.rez");
                    if !path.is_file() {
                        return None;
                    }
                    open_attributes(&path)
                        .map_err(|e| info!("Combat Arms: built-in data only: {e}"))
                        .ok()
                })();
                Self::new(false, attributes, Self::built_in_docs())
            })
    }

    /// The built-in data with `docs` as the weapon documents (name, TOML
    /// text) instead of `WEAPON_DATA`: a test's or a minigame's own
    /// weapon tuning. Not shared with any other app.
    pub fn builtin_with_weapons(docs: Vec<(String, String)>) -> Self {
        Self::new(true, None, docs)
    }

    /// An install's attribute files, as read, over the built-in data
    /// (tests that build an install by hand). Not shared.
    pub fn with_attributes(attributes: InstallAttributes) -> Self {
        Self::new(false, Some(attributes), Self::built_in_docs())
    }

    /// The app's data: its resource if a plugin or the game's setup put
    /// one there, else the default (inserted now, so every plugin of the
    /// app agrees whatever order they build in).
    pub fn of(app: &mut App) -> Self {
        app.init_resource::<CaData>();
        app.world().resource::<CaData>().clone()
    }
}

impl CaDataInner {
    /// Whether this data reads nothing of the install at run time (its
    /// numbers, mount and sounds: the effects go silent too).
    pub fn builtin_only(&self) -> bool {
        self.builtin_only
    }

    /// The install's mount (its archives' directories read once per
    /// process: `world::mount`); `None` without an install, or in
    /// `builtin()` data (no map, sound or texture reads the install).
    pub fn mount(&self) -> Option<Arc<Mount>> {
        if self.builtin_only {
            return None;
        }
        super::super::world::mount()
    }

    /// The install's attribute data: `None` without an install, with
    /// `builtin()`, or when `ATTRIBUTES.rez` can't be read (no V1 key:
    /// logged).
    pub fn attributes(&self) -> Option<&InstallAttributes> {
        self.attributes.as_ref()
    }

    /// The server's `ReloadCheckTime` (attribute_data.md Corrections 3):
    /// the install's `[TimeWeight]`, else the built-in 0.8.
    pub fn reload_check_time(&self) -> f64 {
        use super::attributes::ServerTimeWeight;
        self.attributes()
            .and_then(|a| a.time_weight)
            .map_or(ServerTimeWeight::BUILT_IN, |t| t.reload_check_time)
    }

    /// Combat Arms' weapons as data, parsed once per `CaData`: the
    /// built-in documents, with the install's attribute data over them
    /// when it can be read. Bad built-in data panics (the tests load it);
    /// merged data that fails falls back to it. Leaked (`&'static`, as
    /// every weapon set).
    pub fn weapon_set(&self) -> &'static WeaponSet {
        self.weapon_set.get_or_init(|| {
            let docs: Vec<(&str, &str)> = self.docs.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
            let builtin = || WeaponSet::parse(&docs, &[]).unwrap_or_else(|e| panic!("Combat Arms weapon data: {e}"));
            let mods = self.attributes().and_then(|a| a.mods.as_ref());
            let merged = self.attributes().and_then(|a| a.weapons.as_ref()).and_then(|w| {
                let mpu = movement_data().meters_per_unit as f64;
                let merged = merge_weapon_docs_with_mods(&docs, w, mods, mpu)
                    .map_err(|e| warn!("Combat Arms: the install's weapons weren't merged: {e}"))
                    .ok()?;
                let refs: Vec<(&str, &str)> = merged.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
                let set = WeaponSet::parse(&refs, &[])
                    .map_err(|e| warn!("Combat Arms: the install's weapons don't load, using the built-in ones: {e}"))
                    .ok()?;
                Some((set, merged))
            });
            let (set, used) = merged.unwrap_or_else(|| (builtin(), self.docs.clone()));
            let _ = self.weapon_docs.set(used);
            Box::leak(Box::new(set))
        })
    }

    /// The weapon documents (name, TOML text) `weapon_set` is parsed
    /// from, in registry order: the install's merged over ours, or ours
    /// (what a server sends a client whose own differ: `served`).
    pub fn weapon_docs(&self) -> &[(String, String)] {
        self.weapon_set();
        self.weapon_docs.get().expect("set with the weapon set")
    }

    /// The attachment records: the built-in ones (data/attachments/mods.toml)
    /// with the install's modification file over them by name.
    pub fn attachments(&self) -> &CaModTable {
        self.attachments.get_or_init(|| {
            let mut table = CaModTable::built_in().clone();
            if let Some(mods) = self.attributes().and_then(|a| a.mods.as_ref()) {
                merge_mods(&mut table, mods);
            }
            table
        })
    }

    /// The movement data with the install's body type over it.
    pub fn movement(&self) -> &MovementData {
        self.movement
            .get_or_init(|| with_install_movement(movement_data().clone(), self.attributes()))
    }

    /// The gear data: ours with the install's records over them.
    pub fn gear(&self) -> &GearData {
        self.gear.get_or_init(|| {
            let data = GearData::built_in();
            match self.attributes().and_then(|a| a.gear.as_ref()) {
                Some(install) => data.with_install(install),
                None => data,
            }
        })
    }

    /// The character data (data/characters.toml) with the install's
    /// marks, models and weapon looks over it.
    pub fn characters(&self) -> &CharacterData {
        self.characters.get_or_init(|| CharacterData::load(self))
    }

    /// The install's missions (the mounted archives); `None` in
    /// `builtin()` data or without them.
    pub fn missions(&self) -> Option<&InstallMissions> {
        self.missions
            .get_or_init(|| {
                if self.builtin_only {
                    return None;
                }
                let mount = super::super::world::mount()?;
                Some(super::super::missions::read_install(&|p| mount.read(p).ok()))
            })
            .as_ref()
    }

    /// The IDs of our own weapon documents (the weapons a player is
    /// given: `inventory`, the modes, the bots). The models a map loads
    /// are theirs only: the install adds some 2,000 weapons, too many to
    /// load per map.
    pub fn builtin_weapon_ids(&self) -> &BTreeSet<String> {
        self.builtin_ids.get_or_init(|| {
            self.docs
                .iter()
                .filter_map(|(_, text)| {
                    let t = text.parse::<Table>().ok()?;
                    // A detonator is never seen in a hand (no held model).
                    if t.get("hooks").and_then(|h| h.get(super::super::special::CONTROLLER)).is_some() {
                        return None;
                    }
                    doc_id(&t).map(String::from)
                })
                .collect()
        })
    }

    /// Our weapon document for an install weapon `Name` (`is_install_name`:
    /// the mission files' objective weapons, missions.md F3), if the set
    /// has it.
    pub fn weapon_for_name(&self, name: &str) -> Option<&'static str> {
        self.weapon_set()
            .weapons
            .iter()
            .find(|w| w.id.strip_prefix("combat_arms:").is_some_and(|id| is_install_name(name, id)))
            .map(|w| w.id.as_str())
    }

    /// Our weapon document for an install weapon record's index (the
    /// mission files' weapon ids: Arms Race ladders, the infected's and
    /// the agent's loadouts), with the V1 key; None without the install
    /// or a document for it.
    pub fn weapon_for_index(&self, index: i32) -> Option<&'static str> {
        let weapons = self.attributes()?.weapons.as_ref()?;
        let name = &weapons.weapons.get(usize::try_from(index).ok()?)?.name;
        self.weapon_for_name(name)
    }

    /// The install's record of weapon `id` (ours or one merged from the
    /// install: `combat_arms:<slug of its Name>`), when the install's
    /// weapons file was read.
    pub fn install_weapon(&self, id: &str) -> Option<&WeaponRecord> {
        install_weapon_in(self.attributes()?.weapons.as_ref()?, id)
    }
}
