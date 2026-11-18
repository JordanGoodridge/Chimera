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
use Location::*;
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

/// One piece of gear's protection (Q15): per damage type
/// (`InstDamageType`), a fraction for the head, torso, arm and leg, and
/// whether it is used up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gear {
    pub protects: &'static [Protection],
}

/// Gear whose only entry is a flash protection of `$v` on the head and
/// torso (grenades.md section 10).
macro_rules! flash_gear {
    ($v:expr) => {
        Gear {
            protects: &[Protection {
                damage_type: FLASH,
                head: $v,
                torso: $v,
                arm: 0.0,
                leg: 0.0,
                consumable: false,
            }],
        }
    };
}

/// Every piece of gear there is, by name (`Gear` goes over the network as
/// its place here).
pub const GEARS: &[(&str, Gear)] = &[
    ("default body", DEFAULT_BODY_GEAR),
    ("light body", LIGHT_BODY_GEAR),
    ("heavy body", HEAVY_BODY_GEAR),
    ("default head", DEFAULT_HEAD_GEAR),
    ("gas mask", GAS_MASK),
    // Gear with a flash entry (grenades.md section 10, its torso value,
    // which is its head value too), by data name; a name with two records
    // takes its first. Only the flash entries: their other protection
    // isn't in a spec here.
    ("Anti-Flash", flash_gear!(1.0)),
    ("Skull_head", flash_gear!(1.0)),
    ("Codename_Raven", flash_gear!(1.0)),
    ("Prison_Recon_S_grade", flash_gear!(1.0)),
    ("Assassin_F", flash_gear!(1.0)),
    ("Assassin_M", flash_gear!(1.0)),
    ("Protect_flash_100", flash_gear!(1.0)),
    ("Ballistic_Mask", flash_gear!(0.8)),
    ("SnowMan_Head", flash_gear!(0.8)),
    ("Davil_Mask", flash_gear!(0.8)),
    ("Guy_Fawkes_mask", flash_gear!(0.8)),
    ("Protect_flash_75", flash_gear!(0.75)),
    ("Anti-Flash_70", flash_gear!(0.7)),
    ("Recruit_Survival_Bandana", flash_gear!(0.7)),
    ("Protect_flash_45", flash_gear!(0.55)),
    ("Protect_flash_55", flash_gear!(0.55)),
    ("Protect_fire_24", flash_gear!(0.55)),
    ("Anti-Flash_50", flash_gear!(0.5)),
    ("Codename_Viper", flash_gear!(0.5)),
    ("Powered_Suit_Helmet_Extreme", flash_gear!(0.5)),
    ("Stek_Headgear", flash_gear!(0.5)),
    ("RoboSkull_HauserRider_G", flash_gear!(0.5)),
    ("Protect_flash_25", flash_gear!(0.45)),
    ("Protect_flash_35", flash_gear!(0.45)),
    ("Protect_fire_22", flash_gear!(0.45)),
    ("RoboSkull_HauserRider_S", flash_gear!(0.4)),
    ("Protect_fire_20", flash_gear!(0.35)),
    ("Anti-Flash_30", flash_gear!(0.3)),
    ("Eyepatch_Skull", flash_gear!(0.3)),
    ("Powered_Suit_Helmet_Hard", flash_gear!(0.3)),
    ("Protect_gas_20", flash_gear!(0.25)),
    ("Eyepatch", flash_gear!(0.15)),
    ("Protect_flash_15", flash_gear!(0.15)),
    ("Protect_fire_16", flash_gear!(0.15)),
    ("Protect_gas_16", flash_gear!(0.15)),
    ("Protect_flash_12", flash_gear!(0.12)),
    ("Protect_flash_9", flash_gear!(0.09)),
    ("Infected", flash_gear!(1.0)),
    ("Host_Infected", flash_gear!(1.0)),
];

/// A piece of gear by its name in `GEARS`.
pub fn gear(name: &str) -> Option<Gear> {
    GEARS.iter().find(|(n, _)| *n == name).map(|(_, g)| *g)
}

/// The sum of `pieces`' torso entries for `damage_type` (flash protection
/// sums them, grenades.md section 10), before any clamp. Only entries
/// made for that type count (not an every-class kit such as CS:S's
/// kevlar, which is no `ArmorPieces`).
pub fn gear_torso(pieces: &[hits::ArmorPiece], damage_type: i32) -> f32 {
    let class = Some(class_of(damage_type));
    pieces.iter().flat_map(|p| p.entries.iter()).filter(|e| e.class == class).map(|e| e.torso).sum()
}

impl Gear {
    /// A piece with these entries, interned: the same entries are always
    /// the same slice (a piece goes over the network by its entries, and
    /// a client decoding it must not grow memory each time).
    pub fn new(entries: &[Protection]) -> Gear {
        static INTERNED: std::sync::Mutex<Vec<&'static [Protection]>> = std::sync::Mutex::new(Vec::new());
        let mut list = INTERNED.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(found) = list.iter().find(|s| s.iter().eq(entries.iter())) {
            return Gear { protects: found };
        }
        let leaked: &'static [Protection] = Box::leak(entries.to_vec().into_boxed_slice());
        list.push(leaked);
        Gear { protects: leaked }
    }
}

// A piece goes over the network by its entries: the install's pieces
// are not in `GEARS`.
impl Serialize for Gear {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.protects.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Gear {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let entries = Vec::<Protection>::deserialize(d)?;
        Ok(Gear::new(&entries))
    }
}

/// What a piece of gear does against one damage type.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Protection {
    pub damage_type: i32,
    pub head: f32,
    pub torso: f32,
    pub arm: f32,
    pub leg: f32,
    pub consumable: bool,
}

impl Gear {
    /// The piece as Chimera's armour coverage (`weapon::hits`): its first
    /// entry per damage type, by class (`class_of`), the groin uncovered,
    /// worn by what it blocks when consumable.
    pub fn coverage(&self) -> impl Iterator<Item = hits::Coverage> + '_ {
        self.protects
            .iter()
            .enumerate()
            .filter(|(i, p)| self.protects[..*i].iter().all(|o| o.damage_type != p.damage_type))
            .map(|(_, p)| hits::Coverage {
                class: Some(class_of(p.damage_type)),
                head: p.head,
                torso: p.torso,
                arm: p.arm,
                leg: p.leg,
                groin: 0.0,
                wear: if p.consumable { 1.0 } else { 0.0 },
            })
    }
}

pub(super) const fn bullet(head: f32, body: f32) -> Protection {
    Protection {
        damage_type: BULLET,
        head,
        torso: body,
        arm: body,
        leg: body,
        consumable: true,
    }
}

/// Body gear's burn protection: 0.5 everywhere, the head too.
pub(super) const BODY_BURN: Protection = Protection {
    damage_type: BURN,
    head: 0.5,
    torso: 0.5,
    arm: 0.5,
    leg: 0.5,
    consumable: true,
};

/// The spec's common gear ("Armour"): example values until the
/// characters spec gives the table (Q7). Body gear covers bullets and
/// burn, head gear bullets, the gas mask poison (not used up); nothing
/// covers melee or explosions.
pub const DEFAULT_BODY_GEAR: Gear = Gear {
    protects: &[bullet(0.0, 0.15), BODY_BURN],
};
pub const LIGHT_BODY_GEAR: Gear = Gear {
    protects: &[bullet(0.0, 0.05), BODY_BURN],
};
pub const HEAVY_BODY_GEAR: Gear = Gear {
    protects: &[bullet(0.0, 0.30), BODY_BURN],
};
pub const DEFAULT_HEAD_GEAR: Gear = Gear {
    protects: &[bullet(0.25, 0.0)],
};
pub const GAS_MASK: Gear = Gear {
    protects: &[Protection {
        damage_type: POISON,
        head: 1.0,
        torso: 1.0,
        arm: 1.0,
        leg: 1.0,
        consumable: false,
    }],
};

/// A shooter whose rounds go exactly where it looks: no spread cone, one
/// ray per shot (no extra pellets) and no recoil kick. Combat Arms' bots
/// (bots.md 7.3, Quirk 7), whose brain picks each shot's aim point.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExactAim;

/// Combat Arms' armour: Chimera's `weapon::Armor` (its amount: the
/// points, AP, over `HIT_POINTS`) with the gear worn as `ArmorPieces`
/// (`pieces_of`). The server's; replicated to clients (`game::net`).
pub fn gear_armour(points: f32, gear: &[Gear]) -> (Armor, ArmorPieces) {
    (
        Armor {
            amount: points / HIT_POINTS,
            helmet: false,
        },
        pieces_of(gear),
    )
}

/// The gear worn as Chimera's armour pieces (`Gear::coverage`), running
/// out by what they block.
pub fn pieces_of(gear: &[Gear]) -> ArmorPieces {
    ArmorPieces {
        pieces: gear.iter().map(|g| hits::ArmorPiece { entries: g.coverage().collect() }).collect(),
        depletion: hits::Depletion::Blocked,
    }
}

/// A model's `MaxArmor` 100 with the default body gear.
pub fn default_armour() -> (Armor, ArmorPieces) {
    gear_armour(100.0, &[DEFAULT_BODY_GEAR])
}

/// Armour points (AP) of `armor`.
pub fn points(armor: &Armor) -> f32 {
    armor.amount * HIT_POINTS
}

/// What the armour step needs of a hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArmourHit {
    /// `InstDamageType`.
    pub damage_type: i32,
    /// The hit's location (torso for types without zones).
    pub location: Location,
    /// The zone factor (1 for types without zones).
    pub zone: f32,
    /// An infected victim: its groin counts as torso.
    pub infected: bool,
}

/// Damage `d` (hit points) of `hit` after armour: returns what health
/// loses, and takes the armour's share. With `lands` false (a refused hit)
/// the armour is left whole. The shared armour step
/// (`weapon::hits::absorb`, ratio 1, fractions kept, `Depletion::Blocked`)
/// with the gear as coverage (`Gear::coverage`), then the zone factor.
pub fn armour_and_zone(d: f32, hit: ArmourHit, armor: Option<&mut Armor>, pieces: Option<&ArmorPieces>, lands: bool) -> f32 {
    let zone = hit.zone;
    let Some(armor) = armor else { return (d * zone).max(0.0) };
    let location = if hit.infected && hit.location == Groin { Torso } else { hit.location };
    let ap = points(armor);
    let (entries, depletion) = crate::weapon::coverage(armor, pieces);
    let a = hits::absorb(d, part_of(location), class_of(hit.damage_type), Some(1.0), 0.0, ap, entries, depletion);
    if a.covered && lands {
        armor.amount = (ap - a.taken).max(0.0) / HIT_POINTS;
    }
    (a.to_health * zone).max(0.0)
}

/// Chimera's damage class of a damage type (`InstDamageType`): bullet,
/// burn, explosion (blast), poison, flash and melee as Chimera's, any
/// other as a class of Combat Arms' own (`DamageClass::OWN` + the type).
pub fn class_of(damage_type: i32) -> hits::DamageClass {
    match damage_type {
        BULLET => hits::DamageClass::BULLET,
        BURN => hits::DamageClass::BURN,
        EXPLOSION => hits::DamageClass::BLAST,
        POISON => hits::DamageClass::POISON,
        FLASH => hits::DamageClass::FLASH,
        MELEE => hits::DamageClass::MELEE,
        t => hits::DamageClass(hits::DamageClass::OWN.saturating_add(t.clamp(0, i32::from(u16::MAX)) as u16)),
    }
}

/// An armour location as Chimera's body part.
pub fn part_of(location: Location) -> crate::core::BodyPart {
    match location {
        Head => crate::core::BodyPart::Head,
        Torso => crate::core::BodyPart::Torso,
        Arm => crate::core::BodyPart::Arm,
        Leg => crate::core::BodyPart::Leg,
        Groin => crate::core::BodyPart::Groin,
    }
}

/// Steps 1 to 4 of "Damage per ray" before penetration and the victim:
/// base x the roll (`u` in [0, 1] picks within it) x `behind` x the band
/// at `distance` metres. Health units.
pub fn base_damage(d: &CaDamage, u: f32, behind: bool, distance: f32) -> f32 {
    let roll = 1.0 - d.roll + 2.0 * d.roll * u;
    let back = if behind { BEHIND_BONUS } else { 1.0 };
    d.damage * roll * back * band_scale(d, distance)
}

/// The range band's multiplier at `distance` metres (edges inclusive at
/// the top; beyond the last, or without bands, 1: Q8).
pub fn band_scale(d: &CaDamage, distance: f32) -> f32 {
    let Some(bands) = d.bands else { return 1.0 };
    // Edges are whole units: compare in units, a hair of float slack.
    let units = distance / d.unit;
    bands
        .iter()
        .find(|(edge, _)| units <= edge / d.unit + 1e-3)
        .map_or(1.0, |(_, m)| *m)
}

/// A uniform square spread: the aim's forward bent by (2 u1 - 1) `half`
/// right and (2 u2 - 1) `half` up, normalised (`half` = n / 1000).
pub fn square_dir(aim: Quat, half: f32, u1: f32, u2: f32) -> Vec3 {
    let f = aim * Vec3::NEG_Z;
    let r = aim * Vec3::X;
    let up = aim * Vec3::Y;
    (f + r * ((2.0 * u1 - 1.0) * half) + up * ((2.0 * u2 - 1.0) * half)).normalize()
}

// ---------------------------------------------------------------------------
// Surfaces

/// One shoot-through surface (data/surfaces.toml), metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfacePass {
    /// Most material passed (0: always).
    pub thickness: f32,
    /// Exit direction offset, thousandths per axis.
    pub perturb: f32,
    pub range: f32,
    pub damage: f32,
}

/// Shoot-through surfaces by name (lower case).
#[derive(Resource, Clone, Debug, Default)]
pub struct CaSurfaces(pub Vec<(String, SurfacePass)>);

impl CaSurfaces {
    pub fn get(&self, name: &str) -> Option<SurfacePass> {
        let name = name.to_lowercase();
        self.0.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
    }

    /// Parse a surfaces document (data/surfaces.toml's format).
    pub fn parse(text: &str) -> Result<Self, String> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Units {
            length: f32,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Row {
            names: Vec<String>,
            thickness: f32,
            perturb: f32,
            range: f32,
            damage: f32,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Doc {
            units: Units,
            surface: Vec<Row>,
        }
        let doc: Doc = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in doc.surface {
            for n in row.names {
                out.push((
                    n.to_lowercase(),
                    SurfacePass {
                        thickness: row.thickness * doc.units.length,
                        perturb: row.perturb,
                        range: row.range,
                        damage: row.damage,
                    },
                ));
            }
        }
        Ok(Self(out))
    }

    /// The built-in table (data/surfaces.toml).
    pub fn built_in() -> &'static Self {
        static SET: OnceLock<CaSurfaces> = OnceLock::new();
        SET.get_or_init(|| {
            Self::parse(include_str!("../../../data/surfaces.toml"))
                .unwrap_or_else(|e| panic!("Combat Arms surfaces.toml: {e}"))
        })
    }
}

