//! The map objects the game modes read (specs/combat_arms/game_modes.md
//! S4, S12, M3, M4, M7, M9, M10; world_dat.md's object list): `game::world`
//! passes them through as map entities (`MapEntities`, every property as
//! text, the start points with their feet on the floor and their yaw), and
//! `read` turns them into `CaMapObjects` whenever a map loads:
//!
//! - start points (`GameStartPoint`): `GameType` (the spawn family),
//!   `Teams` (`JSF` Alpha, `Creed` Bravo, else none), `Spawn`, `Locked`,
//!   in map order;
//! - item drop points (`ObjectSpawnPoint`): `GameType` (S12; `Spawn` isn't
//!   read);
//! - bomb sites (`BombZoneTrigger`): `GameType`, `ZoneNumber`,
//!   `DefusableTeam`, `CanDefuse`, `DefuseTime`, `DetonateTime`, a box of
//!   `Dims` half-extents about `Pos`;
//! - flags (`FlagItem`): `Teams`, `GameType`, a box of `Dims` (10, 25, 10
//!   when absent), at `Pos` (home);
//! - flag zones (`FlagZoneTrigger`): `Team`, `LinkedFlag`, a box;
//! - use triggers (`ActivateTrigger`, Seize and Secure's
//!   `ActivateTrigger_A` and `_B` by `Name`): a box.
//!
//! Boxes are axis-aligned in the map's axes (so in ours), engine metres.
//! A test (or a tool) may put its own `CaMapObjects` in after a map loaded;
//! it is read again only when the map's entities change.
//!
//! Known shortcuts: a box's `Dims` is taken as its half-extents (as the
//! AI volumes'); `PlantedCommand` and the other command strings aren't run
//! (the bomb's effects on the sites are the mode's own: both sites close
//! after a plant); `TDMGameControl` and mission controllers are passed
//! through but nothing reads them.

use bevy::prelude::*;

use super::state::{ALPHA, BRAVO};
use crate::map::{MapEntities, MapEntity, entities::entity_to_engine};

/// The classes `game::world` passes through for the modes.
pub const MODE_CLASSES: &[&str] = &[
    "GameStartPoint",
    "ObjectSpawnPoint",
    "BombZoneTrigger",
    "FlagItem",
    "FlagZoneTrigger",
    "ActivateTrigger",
    "TDMGameControl",
];

/// Whether `game::world` passes objects of `class` through for the modes.
pub fn is_mode_object(class: &str) -> bool {
    MODE_CLASSES.contains(&class)
}

/// A flag's box half-extents when its object has none (M4), world units.
pub const FLAG_DIMS: Vec3 = Vec3::new(10.0, 25.0, 10.0);

/// An axis-aligned box (engine metres).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ObjBox {
    pub centre: Vec3,
    pub half: Vec3,
}

impl ObjBox {
    /// `p` inside the box widened by `margin` on each axis (inclusive).
    pub fn contains(&self, p: Vec3, margin: f32) -> bool {
        let d = (p - self.centre).abs();
        let r = self.half + Vec3::splat(margin);
        d.x <= r.x + 1e-5 && d.y <= r.y + 1e-5 && d.z <= r.z + 1e-5
    }

    /// Another box of half-extents `half` about `p` overlaps this one
    /// (strictly: touching isn't overlapping).
    pub fn touches(&self, p: Vec3, half: Vec3) -> bool {
        let d = (p - self.centre).abs();
        let r = self.half + half;
        d.x < r.x && d.y < r.y && d.z < r.z
    }
}

/// A start point (S4).
#[derive(Clone, Debug, PartialEq)]
pub struct StartPoint {
    /// `GameType`: the spawn family.
    pub family: String,
    /// Alpha, Bravo or none (`<none>`).
    pub team: Option<u8>,
    pub spawn: i32,
    pub locked: bool,
    /// Feet on the floor, facing its yaw (engine).
    pub at: Transform,
}

impl StartPoint {
    /// It serves `family` in the versus modes: `Spawn` exactly 1 and, unless
    /// `locked` lists are wanted (Hunted's rounds), unlocked.
    pub fn qualifies(&self, family: &str, locked: bool) -> bool {
        self.family == family && self.spawn == 1 && self.locked == locked
    }
}

/// An item drop point (S12).
#[derive(Clone, Debug, PartialEq)]
pub struct DropPoint {
    pub family: String,
    pub at: Vec3,
}

/// A bomb site (M3, M9).
#[derive(Clone, Debug, PartialEq)]
pub struct BombZone {
    pub family: String,
    /// `ZoneNumber`: 0 site A, 1 site B.
    pub number: i32,
    /// The team that may defuse a bomb planted here (`DefusableTeam`).
    pub defusers: u8,
    pub can_defuse: bool,
    /// `DefuseTime`, `DetonateTime`, s.
    pub defuse_time: f32,
    pub detonate_time: f32,
    pub area: ObjBox,
}

/// A team's flag (M4).
#[derive(Clone, Debug, PartialEq)]
pub struct FlagObject {
    pub name: String,
    pub family: String,
    pub team: u8,
    /// Its home and box.
    pub area: ObjBox,
}

/// A flag zone (M4): where a team captures.
#[derive(Clone, Debug, PartialEq)]
pub struct FlagZone {
    pub team: u8,
    pub linked_flag: String,
    pub area: ObjBox,
}

/// A use trigger (M10).
#[derive(Clone, Debug, PartialEq)]
pub struct UseTrigger {
    pub name: String,
    pub area: ObjBox,
}

/// The mode objects of the map loaded.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct CaMapObjects {
    pub starts: Vec<StartPoint>,
    pub drops: Vec<DropPoint>,
    pub bomb_zones: Vec<BombZone>,
    pub flags: Vec<FlagObject>,
    pub flag_zones: Vec<FlagZone>,
    pub triggers: Vec<UseTrigger>,
}

/// A team name (`JSF`, `Creed`, their ids or `Alpha`/`Bravo`) as a team id.
pub fn team_named(name: &str) -> Option<u8> {
    let n = name.trim();
    if n.eq_ignore_ascii_case("JSF") || n.eq_ignore_ascii_case("Alpha") || n == "1" {
        Some(ALPHA)
    } else if n.eq_ignore_ascii_case("Creed") || n.eq_ignore_ascii_case("Bravo") || n == "2" {
        Some(BRAVO)
    } else {
        None
    }
}

fn number(e: &MapEntity, key: &str) -> Option<f32> {
    e.get(key).and_then(|v| v.trim().parse().ok())
}

fn flag(e: &MapEntity, key: &str) -> bool {
    number(e, key).is_some_and(|v| v != 0.0) || e.get(key).is_some_and(|v| v.eq_ignore_ascii_case("true"))
}

fn area(e: &MapEntity, scale: f32, default_dims: Vec3) -> ObjBox {
    let dims = if e.get("Dims").is_some() {
        e.vector("Dims")
    } else {
        // Units in entity axes: x, z, y of the map's.
        Vec3::new(default_dims.x, default_dims.z, default_dims.y)
    };
    ObjBox {
        centre: entity_to_engine(e.origin(), scale),
        half: entity_to_engine(dims, scale).abs(),
    }
}

/// The mode objects among a map's entities (`scale` metres per entity
/// unit).
pub fn read(entities: &[MapEntity], scale: f32) -> CaMapObjects {
    let mut out = CaMapObjects::default();
    for e in entities {
        let family = e.get("GameType").unwrap_or("").to_string();
        match e.classname() {
            "GameStartPoint" => {
                let feet = if e.get(super::super::world::FEET_KEY).is_some() {
                    entity_to_engine(e.vector(super::super::world::FEET_KEY), scale)
                } else {
                    entity_to_engine(e.origin(), scale)
                };
                let yaw = number(e, super::super::world::YAW_KEY).unwrap_or(0.0);
                out.starts.push(StartPoint {
                    family,
                    team: e.get("Teams").and_then(team_named),
                    spawn: number(e, "Spawn").map_or(0, |s| s as i32),
                    locked: flag(e, "Locked"),
                    at: Transform::from_translation(feet + Vec3::Y * crate::map::SPAWN_LIFT)
                        .with_rotation(Quat::from_rotation_y(yaw)),
                });
            }
            "ObjectSpawnPoint" => out.drops.push(DropPoint {
                family,
                at: entity_to_engine(e.origin(), scale),
            }),
            "BombZoneTrigger" => out.bomb_zones.push(BombZone {
                family,
                number: number(e, "ZoneNumber").map_or(0, |n| n as i32),
                defusers: e.get("DefusableTeam").and_then(team_named).unwrap_or(BRAVO),
                can_defuse: e.get("CanDefuse").is_none() || flag(e, "CanDefuse"),
                defuse_time: number(e, "DefuseTime").unwrap_or(7.0),
                detonate_time: number(e, "DetonateTime").unwrap_or(40.0),
                area: area(e, scale, Vec3::ZERO),
            }),
            "FlagItem" => {
                let Some(team) = e.get("Teams").and_then(team_named) else { continue };
                out.flags.push(FlagObject {
                    name: e.get("targetname").unwrap_or("").to_string(),
                    family,
                    team,
                    area: area(e, scale, FLAG_DIMS),
                });
            }
            "FlagZoneTrigger" => {
                let Some(team) = e.get("Team").or(e.get("Teams")).and_then(team_named) else { continue };
                out.flag_zones.push(FlagZone {
                    team,
                    linked_flag: e.get("LinkedFlag").unwrap_or("").to_string(),
                    area: area(e, scale, Vec3::ZERO),
                });
            }
            "ActivateTrigger" => out.triggers.push(UseTrigger {
                name: e.get("targetname").unwrap_or("").to_string(),
                area: area(e, scale, Vec3::ZERO),
            }),
            _ => {}
        }
    }
    out
}

/// A map loaded (its entities changed): its objects read again.
pub(super) fn read_map_objects(entities: Option<Res<MapEntities>>, mut objects: ResMut<CaMapObjects>) {
    let Some(m) = entities else { return };
    if !m.is_changed() {
        return;
    }
    let next = read(&m.entities, m.scale);
    if *objects != next {
        *objects = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(pairs: &[(&str, &str)]) -> MapEntity {
        MapEntity {
            keyvalues: pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            ..default()
        }
    }

    #[test]
    fn objects_read_from_the_passed_through_entities() {
        let list = vec![
            ent(&[
                ("classname", "GameStartPoint"),
                ("origin", "100 0 66"),
                ("GameType", "Doobic"),
                ("Teams", "Creed"),
                ("Spawn", "1"),
                ("Locked", "0"),
                ("chimera_feet", "100 0 0"),
                ("chimera_yaw", "1.5"),
            ]),
            ent(&[
                ("classname", "BombZoneTrigger"),
                ("origin", "0 0 0"),
                ("Dims", "130 125 64"),
                ("GameType", "Doobic"),
                ("ZoneNumber", "1"),
                ("DefusableTeam", "Creed"),
                ("DefuseTime", "7"),
                ("DetonateTime", "40"),
            ]),
            ent(&[("classname", "FlagItem"), ("origin", "0 0 0"), ("Teams", "JSF"), ("targetname", "FlagA")]),
        ];
        let o = read(&list, 0.01);
        assert_eq!(o.starts.len(), 1);
        let s = &o.starts[0];
        assert_eq!((s.team, s.spawn, s.locked), (Some(BRAVO), 1, false));
        assert!(s.qualifies("Doobic", false));
        assert!((s.at.translation - Vec3::new(1.0, crate::map::SPAWN_LIFT, 0.0)).length() < 1e-5);
        let z = &o.bomb_zones[0];
        assert_eq!((z.number, z.defusers, z.defuse_time, z.detonate_time), (1, BRAVO, 7.0, 40.0));
        assert!((z.area.half - Vec3::new(1.30, 0.64, 1.25)).length() < 1e-5);
        assert!((o.flags[0].area.half - Vec3::new(0.10, 0.25, 0.10)).length() < 1e-5);
        assert_eq!(o.flags[0].name, "FlagA");
    }
}
