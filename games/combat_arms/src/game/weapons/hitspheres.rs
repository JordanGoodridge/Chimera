use std::{
    collections::HashMap,
    sync::{LazyLock, OnceLock},
};

use avian3d::prelude::*;
use bevy::prelude::*;
use chimera_sim::server_docs::DocsAppExt;
use serde::{Deserialize, Serialize};

use super::super::attachments::{CaNpc, CaSpecialRoll, DamageMods, RecoilMods, SpecialKind, SpreadMods, Victim};
use super::super::characters;
use super::*;
use crate::{
    core::{
        Damage, DamageKind, DamageRules, Damageable, FirstTimePredicted, Health, Hitgroup, Intent, MovementState,
        NetRole, Predict, PredictedAppExt, PropSurface, SOLID_LAYERS, SimClock, Velocity,
    },
    map::{SurfaceNames, sound::SurfaceGrid},
    weapon::{
        AltModes, Armor, ArmorPieces, Burst, CarriedAppExt, Hitscan, Inventory, NetPart, ParamsAppExt, SpreadShape,
        ViewPunch, WeaponEvent, WeaponEventKind, WeaponFrame, Zoom, aim_of,
        data::{WeaponData, WeaponDataAppExt},
        hits,
        lagcomp::{self, HitHistory, LagCompSettings, LagCompStats, ViewTick},
        random::{Rng, shared_random},
    },
};

// ---------------------------------------------------------------------------
// Hit zones and armour

/// A hit sphere's location code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    Head,
    Torso,
    Arm,
    Leg,
    Groin,
}

/// One hit sphere of a skeleton (specs/combat_arms/characters.md 11):
/// its hit-table entry's node or socket name, radius, location and damage
/// factor, and `at`: its centre in the spec's measured standing pose (`LST`
/// at t = 0) in the character's frame (feet at the origin, +Y up, facing
/// -Z, +X its right), world units. Entries no stock body has, or with
/// radius 0, have radius 0 here (never hit).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitSphere {
    pub name: &'static str,
    pub at: Vec3,
    pub radius: f32,
    pub location: Location,
    pub factor: f32,
    /// The skeleton (`characters::records` index: 0 male, 1 female).
    pub skeleton: usize,
    /// `Node#Priority`: the server's own walk picks the highest
    /// (`priority_sphere`; data/characters.toml, ours by location).
    pub priority: f32,
}

use Location::{Arm, Groin, Head, Leg, Torso};

impl HitSphere {
    /// Its body region (`core::BodyRegion`, docs/DESIGN.md "Hits across games"):
    /// by location, the side by where it sits (`at.x`, as hits report it),
    /// the limb's part by its node's name (`*hand*`; `*arml*` the forearm;
    /// `*foot*`, `*toe*`; `*legl*` the shin; the rest of a limb, the
    /// pelvis spheres too, upper arm or thigh); the torso spheres chest
    /// but the neck (its hitgroup is the chest either way).
    pub fn region(&self) -> crate::core::BodyRegion {
        use crate::core::BodyRegion as R;
        let n = self.name.to_ascii_lowercase();
        let side = |l, r| if self.at.x < 0.0 { l } else { r };
        match self.location {
            Head => R::Head,
            Groin => R::Groin,
            Torso if n.contains("neck") => R::Neck,
            Torso => R::Chest,
            Arm if n.contains("hand") => side(R::LeftHand, R::RightHand),
            Arm if n.contains("arml") => side(R::LeftForearm, R::RightForearm),
            Arm => side(R::LeftUpperArm, R::RightUpperArm),
            Leg if n.contains("foot") || n.contains("toe") => side(R::LeftFoot, R::RightFoot),
            Leg if n.contains("legl") => side(R::LeftShin, R::RightShin),
            Leg => side(R::LeftThigh, R::RightThigh),
        }
    }
}

/// Skeleton `index`'s hit table from data/characters.toml.
pub(super) fn hit_table(index: usize) -> &'static [HitSphere] {
    let skel = characters::records::builtin().skeleton(index);
    let table: Vec<HitSphere> = skel
        .spheres
        .iter()
        .map(|s| HitSphere {
            name: s.name.clone().leak(),
            at: s.at.map_or(Vec3::ZERO, |a| characters::skeleton::to_body(Vec3::from(a), characters::skeleton::STAND_FEET)),
            radius: if s.absent || s.at.is_none() { 0.0 } else { s.radius },
            location: match s.location {
                1 => Head,
                3 => Arm,
                4 => Leg,
                5 => Groin,
                _ => Torso,
            },
            factor: s.factor,
            skeleton: index,
            priority: s.priority.unwrap_or_else(|| skel.hit_priority.of(s.location)),
        })
        .collect();
    table.leak()
}

/// The male skeleton's hit table (table 0, the default male body).
pub static MALE: LazyLock<&'static [HitSphere]> = LazyLock::new(|| hit_table(0));
/// The female skeleton's (table 1).
pub static FEMALE: LazyLock<&'static [HitSphere]> = LazyLock::new(|| hit_table(1));

/// A character's hit table (`MALE` when it has none; `characters` sets
/// it from the body's skeleton).
#[derive(Component, Clone, Copy, Debug)]
pub struct HitSpheres(pub &'static [HitSphere]);

/// A named sphere of the male skeleton.
pub fn male(name: &str) -> &'static HitSphere {
    MALE.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no sphere {name}"))
}

/// Which sphere a segment from `start` to `end` hits: of those it passes
/// (radius > 0), the one with the smallest |p|^2 - r^2 (p the centre from
/// `start`). Spheres as (centre, radius).
pub fn pick_sphere(spheres: &[(Vec3, f32)], start: Vec3, end: Vec3) -> Option<usize> {
    let seg = end - start;
    let len2 = seg.length_squared().max(1e-12);
    spheres
        .iter()
        .enumerate()
        .filter(|(_, (c, r))| {
            if *r <= 0.0 {
                return false;
            }
            let t = ((*c - start).dot(seg) / len2).clamp(0.0, 1.0);
            (start + seg * t).distance_squared(*c) <= r * r
        })
        .map(|(i, (c, r))| (i, (*c - start).length_squared() - r * r))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Which sphere a character found by the server's own walk is hit in
/// (weapon_firing.md Corrections 5): of the spheres (centre, radius) whose
/// centre is within its radius of the infinite line through `at` along
/// the unit `dir`, wherever along the line, the one with the highest
/// `priority`; on a tie the later one. None: the line passes them all.
pub fn priority_sphere(spheres: &[(Vec3, f32)], data: &[HitSphere], at: Vec3, dir: Vec3) -> Option<usize> {
    spheres
        .iter()
        .enumerate()
        .filter(|(_, (c, r))| {
            let p = *c - at;
            *r > 0.0 && (p - dir * p.dot(dir)).length_squared() <= r * r
        })
        .map(|(i, _)| (i, data.get(i).map_or(0.0, |s| s.priority)))
        .fold(None, |best: Option<(usize, f32)>, (i, pr)| match best {
            Some((_, b)) if b > pr => best,
            _ => Some((i, pr)),
        })
        .map(|(i, _)| i)
}

/// The direction sector of a bullet travelling along `dir` at a victim
/// facing `forward` with `right` (0..11, pi/6 each).
pub fn sector(forward: Vec3, right: Vec3, dir: Vec3) -> u32 {
    let theta = std::f32::consts::PI + forward.dot(dir).atan2(right.dot(dir));
    ((theta / (std::f32::consts::PI / 6.0)).floor() as i32).clamp(0, 11) as u32
}

/// From behind: sectors 7 to 11 (Q5: lopsided).
pub fn is_behind(forward: Vec3, right: Vec3, dir: Vec3) -> bool {
    (7..=11).contains(&sector(forward, right, dir))
}

