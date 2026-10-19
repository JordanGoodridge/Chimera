//! The character records (data/characters.toml: specs/combat_arms/
//! characters.md sections 1, 2, 3 and 11 as our data) and the model
//! choice: which body record a player's selection draws (1: cosmetics,
//! the face redirect, the default), its skins by camouflage index, the
//! team mark, head pieces and goggles by skeleton and figure type (2).

use std::{collections::HashMap, sync::OnceLock};

use serde::{Deserialize, Serialize};

/// A body record (`Model#`).
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ModelRecord {
    pub id: i32,
    pub name: String,
    /// The body file in the install's archives.
    pub file: String,
    /// The hit table and skeleton (`Skeleton`): index into `skeleton`.
    pub skeleton: usize,
    /// The body type (`FigureType`): sounds and stats.
    pub figure: usize,
    pub gear: i32,
    /// `FaceRelationModel#` by face index (-1: keep this record).
    #[serde(default)]
    pub faces: Vec<i32>,
    /// `UpperSkin#` and `LowerSkin#` by camouflage index.
    #[serde(default)]
    pub upper_skins: Vec<String>,
    #[serde(default)]
    pub lower_skins: Vec<String>,
}

/// A head piece (`Hair#`): its file per skeleton index.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct HeadRecord {
    pub id: i32,
    pub name: String,
    pub files: Vec<String>,
}

/// Goggles or a mask (`Goggle#`): its socket and file per figure type.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct GoggleRecord {
    pub id: i32,
    pub socket: String,
    pub files: Vec<String>,
}

/// The team-mark textures (`[TeamMark]`).
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Marks {
    pub enemy: String,
    pub teammate: String,
}

/// A weapon's third-person data (`AniType`, `LowerAniType`, `HHSkin#`,
/// `HHAniFrame_#`).
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct WeaponLook {
    pub ani_type: i32,
    #[serde(default)]
    pub lower_ani_type: i32,
    #[serde(default)]
    pub skins: Vec<String>,
    /// `HHAniFrame_0..9`, ms (0 or missing: none).
    #[serde(default)]
    pub frames: Vec<u32>,
}

/// A skeleton node the game uses, and where the spec measured it at `LST`
/// t = 0 (the fallback pose).
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct NodeRecord {
    pub name: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub at: Option<[f32; 3]>,
}

/// Where a socket sits: its parent node and offset in that node's space.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct SocketPlace {
    pub parent: String,
    pub offset: [f32; 3],
}

/// One hit-table entry (`Node#Name`, `Radius`, `Location`, `DamageFactor`).
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct SphereRecord {
    pub index: u32,
    pub name: String,
    pub radius: f32,
    /// 1 head, 2 torso, 3 arm, 4 leg, 5 groin.
    pub location: u8,
    pub factor: f32,
    /// On a socket of the stock body, not a node.
    #[serde(default)]
    pub socket: Option<SocketPlace>,
    /// No stock body has it.
    #[serde(default)]
    pub absent: bool,
    #[serde(default)]
    pub bind: Option<[f32; 3]>,
    /// Its centre at `LST` t = 0 (the fallback pose).
    #[serde(default)]
    pub at: Option<[f32; 3]>,
}

/// A socket the game uses.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct SocketRecord {
    pub name: String,
    pub parent: String,
    pub offset: [f32; 3],
    pub rotation: [f32; 4],
    pub bind: [f32; 3],
    /// Its position in the spec's "stand" pose (the fallback), when given.
    #[serde(default)]
    pub at: Option<[f32; 3]>,
}

/// A skeleton and its hit table (`Skeleton#`).
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct SkeletonRecord {
    pub name: String,
    /// Section 8's seven aim nodes, in order.
    pub aim: Vec<String>,
    pub nodes: Vec<NodeRecord>,
    pub spheres: Vec<SphereRecord>,
    pub sockets: Vec<SocketRecord>,
    /// The `Left_foot` node in the "stand" pose (the drop point).
    pub left_foot: [f32; 3],
}

impl SkeletonRecord {
    pub fn socket(&self, name: &str) -> Option<&SocketRecord> {
        self.sockets.iter().find(|s| s.name.eq_ignore_ascii_case(name))
    }

    pub fn sphere(&self, name: &str) -> Option<&SphereRecord> {
        self.spheres.iter().find(|s| s.name.eq_ignore_ascii_case(name))
    }
}

/// The whole file.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct CharacterData {
    pub model: Vec<ModelRecord>,
    pub head: Vec<HeadRecord>,
    pub goggle: Vec<GoggleRecord>,
    pub marks: Marks,
    pub weapons: HashMap<String, WeaponLook>,
    /// Seconds a body stays after its owner respawned, by mode key.
    pub body_removal: HashMap<String, f32>,
    pub skeleton: Vec<SkeletonRecord>,
}

/// Combat Arms' character data (data/characters.toml).
pub fn data() -> &'static CharacterData {
    static DATA: OnceLock<CharacterData> = OnceLock::new();
    DATA.get_or_init(|| {
        let mut data: CharacterData =
            toml::from_str(include_str!("../../../data/characters.toml")).expect("data/characters.toml");
        merge_install_weapons(&mut data);
        data
    })
}

/// The install's weapons file over our weapon looks: each weapon of the
/// set with an install record takes its `AniType`, `LowerAniType` and
/// `HHSkin#` (characters.md's "Third-person weapon model and animation
/// class"); `HHAniFrame_#` isn't read (our data has none either).
fn merge_install_weapons(data: &mut CharacterData) {
    use super::super::data as ca;
    if ca::install_attributes().is_none() {
        return;
    }
    for id in ca::builtin_weapon_ids() {
        let Some(r) = ca::install_weapon(id) else { continue };
        let look = data.weapons.entry(id.clone()).or_default();
        look.ani_type = r.ani_type;
        look.lower_ani_type = r.lower_ani_type;
        if !r.hh_skins.is_empty() {
            look.skins = ca::skin_paths(&r.hh_skins);
        }
    }
}

/// A weapon's class when it has no record: the weapons file's `AniType`
/// default (attribute_data.md).
pub const DEFAULT_ANI_TYPE: i32 = 5;
/// The class with no weapon at all (5): the knife's.
pub const NO_WEAPON_ANI_TYPE: i32 = 14;

/// A player's model selection (1, "The selection"): the record a client
/// sends with its character info; -1 is "none".
#[derive(bevy::prelude::Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub model: i32,
    pub head: i32,
    pub face: i32,
    pub goggle: i32,
    pub backpack: i32,
    pub camouflage: i32,
    /// Cosmetic character and vest ids (-1: none).
    pub cosmetic_character: i32,
    pub cosmetic_vest: i32,
}

impl Default for Selection {
    /// "A fresh selection is model 0, head piece 0, face 0, no goggle, no
    /// backpack, camouflage 0."
    fn default() -> Self {
        Self {
            model: 0,
            head: 0,
            face: 0,
            goggle: -1,
            backpack: -1,
            camouflage: 0,
            cosmetic_character: -1,
            cosmetic_vest: -1,
        }
    }
}

impl CharacterData {
    pub fn model(&self, id: i32) -> Option<&ModelRecord> {
        self.model.iter().find(|m| m.id == id)
    }

    /// Model 0, the default.
    pub fn default_model(&self) -> &ModelRecord {
        self.model(0).unwrap_or(&self.model[0])
    }

    /// `record` after the face redirect (a valid face whose
    /// `FaceRelationModel` is set).
    fn redirect<'a>(&'a self, record: &'a ModelRecord, face: i32) -> &'a ModelRecord {
        usize::try_from(face)
            .ok()
            .and_then(|f| record.faces.get(f))
            .filter(|&&m| m >= 0)
            .and_then(|&m| self.model(m))
            .unwrap_or(record)
    }

    /// "Effective body": the record a selection draws (file, skins,
    /// skeleton) and its figure type (from the redirected base model).
    pub fn effective(&self, s: &Selection) -> (&ModelRecord, usize) {
        let start = if s.cosmetic_character >= 0 {
            s.cosmetic_character
        } else if s.cosmetic_vest >= 0 {
            s.cosmetic_vest
        } else {
            s.model
        };
        let base = self.model(s.model).unwrap_or(self.default_model());
        let own = self.model(start).unwrap_or(base);
        (self.redirect(own, s.face), self.redirect(base, s.face).figure)
    }

    /// The body's upper and leg skins for camouflage index `c` (index 0
    /// past the list).
    pub fn skins<'a>(&self, record: &'a ModelRecord, c: i32) -> (Option<&'a str>, Option<&'a str>) {
        let pick = |list: &'a [String]| {
            let i = usize::try_from(c).ok().filter(|&i| i < list.len()).unwrap_or(0);
            list.get(i).map(String::as_str)
        };
        (pick(&record.upper_skins), pick(&record.lower_skins))
    }

    /// The team mark on a body's slot 2 as a client sees it: a teammate's
    /// tie, else nothing (the transparent mark); in the infection mode
    /// always the latter.
    pub fn team_mark(&self, teammate: bool, infection: bool) -> &str {
        if teammate && !infection {
            &self.marks.teammate
        } else {
            &self.marks.enemy
        }
    }

    /// A head piece's file for `skeleton` (None: no head model).
    pub fn head_file(&self, head: i32, skeleton: usize) -> Option<&str> {
        let r = self.head.iter().find(|h| h.id == head)?;
        r.files.get(skeleton).map(String::as_str).filter(|f| !f.is_empty())
    }

    /// A goggle's socket and file for `figure` (None: none).
    pub fn goggle_file(&self, goggle: i32, figure: usize) -> Option<(&str, &str)> {
        let r = self.goggle.iter().find(|g| g.id == goggle)?;
        let file = r.files.get(figure).map(String::as_str).filter(|f| !f.is_empty())?;
        Some((r.socket.as_str(), file))
    }

    /// A weapon's third-person data (the attribute default without one).
    pub fn weapon(&self, id: Option<&str>) -> WeaponLook {
        match id {
            None => WeaponLook {
                ani_type: NO_WEAPON_ANI_TYPE,
                ..Default::default()
            },
            Some(id) => self.weapons.get(id).cloned().unwrap_or(WeaponLook {
                ani_type: DEFAULT_ANI_TYPE,
                ..Default::default()
            }),
        }
    }

    pub fn skeleton(&self, index: usize) -> &SkeletonRecord {
        self.skeleton.get(index).unwrap_or(&self.skeleton[0])
    }

    /// Seconds a body stays after its owner respawned in mode `mode`
    /// (None: until a new round).
    pub fn body_removal(&self, mode: &str) -> Option<f32> {
        self.body_removal.get(mode).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_reads() {
        let d = data();
        assert_eq!(d.skeleton.len(), 2);
        for s in &d.skeleton {
            assert_eq!(s.aim.len(), 7);
            for n in &s.nodes {
                if let Some(p) = &n.parent {
                    assert!(s.nodes.iter().any(|m| &m.name == p), "{}: parent {p}", s.name);
                }
            }
            for sp in &s.spheres {
                if let Some(sock) = &sp.socket {
                    assert!(s.nodes.iter().any(|m| m.name == sock.parent), "{}", sp.name);
                }
                if sp.radius > 0.0 && !sp.absent {
                    assert!(sp.at.is_some() && sp.bind.is_some(), "{}", sp.name);
                }
            }
        }
    }
}
