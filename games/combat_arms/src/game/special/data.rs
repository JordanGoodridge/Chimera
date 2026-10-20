//! The special weapons' data: the parameters of the weapon hooks
//! (`combat_arms:projectile`, `:flame`, `:device`, `:bolt`) as the
//! documents in data/weapons/ give them. Every key says in the document
//! which spec section and which attribute-data field it is; lengths are
//! the document's world units (`units`), times seconds, damage hit points.

use serde::Deserialize;

use super::super::{grenades::OverTimeParams, weapons::DamageParams};

/// Hook names (docs/weapon-data.md, "Hooks").
pub const PROJECTILE: &str = "combat_arms:projectile";
pub const FLAME: &str = "combat_arms:flame";
pub const DEVICE: &str = "combat_arms:device";
pub const BOLT: &str = "combat_arms:bolt";

fn explode() -> u32 {
    7
}

/// `combat_arms:projectile`: an impact projectile the weapon fires (1.3):
/// rockets, launcher shells, flame puffs, the air strike's shells.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProjectileParams {
    /// The projectile record's `Velocity`, u/s.
    pub speed: f32,
    /// `Gravity` 1: the object gravity applies in flight.
    #[serde(default)]
    pub gravity: bool,
    /// `Lifetime`, s: it detonates in the air when this is over.
    pub lifetime: f32,
    /// The ammo's `Health`: hit points to shoot it down.
    pub health: f32,
    /// The box (half-extents, u): the model's first animation x `ModelScale`.
    pub box_half: [f32; 3],
    /// `CanHitSameKind`.
    #[serde(default)]
    pub hits_same_kind: bool,
    /// `MinPerturb`, `MaxPerturb` and `DynamicPerturbFactor` (the cone
    /// with the meters at 0: `grenades::throw_perturb`).
    pub perturb: [u32; 2],
    #[serde(default)]
    pub dynamic_perturb: f32,
    /// The ammo's `AreaDamage`, `AreaDamageRadius`, `AreaDamageType`.
    #[serde(default)]
    pub area_damage: f32,
    #[serde(default)]
    pub area_radius: f32,
    #[serde(default = "explode")]
    pub area_type: u32,
    /// The lasting sphere (`ProgDamage*`): the flames, the incendiary shell.
    #[serde(default)]
    pub over_time: Option<OverTimeParams>,
    /// The shooter never takes the sphere's burn (a `Guntype` 9 weapon's).
    #[serde(default)]
    pub shooter_exempt: bool,
}

/// `combat_arms:flame`: the flamethrower's fire keys (4): a pre-fire, then
/// a loop with `keys` fire keys `interval` apart.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FlameParams {
    pub prefire: f32,
    pub loop_len: f32,
    pub keys: u32,
    pub interval: f32,
    pub postfire: f32,
}

/// What kind of device (6).
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    LandMine,
    Hornet,
    Claymore,
    RemoteGrenade,
    Caltrops,
    NerveMine,
    Turret,
    FirstAid,
    Medkit,
    AirStrike,
}

/// The claymore's cone (6.3): `AreaArcDamage`, `AreaArcDamageRadius`,
/// `AreaArcDamageAngle` (half-angle, degrees).
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ArcParams {
    pub damage: f32,
    pub radius: f32,
    pub angle: f32,
}

/// The caltrops' cylinder and bleeding (6.5).
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CaltropsParams {
    /// `ProgDamage` HP/s for `ProgDamageDuration` s.
    pub rate: f32,
    pub duration: f32,
    /// `ProgDamageRadius`, `ProgDamageHeight`.
    pub radius: f32,
    pub height: f32,
    /// `DamageMaxCount`, and the time between counts of one victim.
    pub counts: u32,
    pub recount: f32,
    /// `StunnedMovePenalty`, `StunnedTime`.
    pub stun_penalty: f32,
    pub stun_time: f32,
}

/// The nerve mine's per-frame effect (6.6): damage type 44 with `amount`
/// for `duration` s on enemies within `radius`.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NerveParams {
    pub damage_type: i32,
    pub amount: f32,
    pub duration: f32,
    pub radius: f32,
}

/// The auto turret (6.7): the `Turret#` record and its gun.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct TurretParams {
    /// `Lower_set`, s: the time from landing to firing.
    pub setup: f32,
    /// `MaxAmmo`, `FireDelay`, `Perturb` (u, each axis), `SensorOffset`.
    pub ammo: u32,
    pub fire_delay: f32,
    pub perturb: f32,
    pub sensor_offset: [f32; 3],
    /// `ShootingAngle` (below, above), degrees.
    pub angles: [f32; 2],
    /// `SensorAreaRadius`, u.
    pub search_radius: f32,
    /// The top gun's ray (`Turret-TopGun`): damage, type, roll and range
    /// bands as a weapon's `combat_arms:damage`, and its range.
    pub gun: DamageParams,
    pub range: f32,
}

/// The first-aid kit (6.8): `AreaDamageTickTime`, `AreaDamageTickDamage`,
/// `AreaDamageTotal`, `AreaDamageRadius`, the heal's damage type.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct KitParams {
    pub tick: f32,
    pub heal: f32,
    pub pool: f32,
    pub radius: f32,
}

/// The medkit's heal over time (6.9): HP/s for s, at most `total`.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MedkitParams {
    pub rate: f32,
    pub duration: f32,
    pub total: f32,
}

/// An air strike designator (6.10): the marker's flight (`speed`,
/// `lifetime` are the device's), the shells and where they drop.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StrikeParams {
    /// `AirDropHeight`, `AirDropDelay` (s between shells).
    pub height: f32,
    pub delay: f32,
    /// `AirDropPoint#`: (right, forward) offsets along the course, u.
    pub points: Vec<[f32; 2]>,
    /// The shell (the `AirStrikeID` weapon's projectile).
    pub shell: ProjectileParams,
}

/// `combat_arms:device`: a thrown or placed device (6) and how it is used.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct DeviceParams {
    pub kind: DeviceKind,
    /// `SpawnedAmount`, `MaxAmount`.
    pub spawned: u32,
    pub max: u32,
    /// The view-model animations (6.12): pre-fire, the throw play that
    /// repeats while fire is held (0: none), the fire key's time in the
    /// post-fire (or `fire_0`), the animation's length after the press,
    /// and the redraw.
    #[serde(default)]
    pub prefire: f32,
    #[serde(default)]
    pub throw_loop: f32,
    pub key: f32,
    pub postfire: f32,
    pub redraw: f32,
    /// The thrown cone (`MinPerturb`, `MaxPerturb`, the dynamic factor).
    #[serde(default)]
    pub perturb: [u32; 2],
    #[serde(default)]
    pub dynamic_perturb: f32,
    /// The projectile record's `Velocity` (0: placed, not thrown) and
    /// `Lifetime`, and the ammo's `Health`.
    pub speed: f32,
    pub lifetime: f32,
    pub health: f32,
    /// The box (half-extents, u).
    pub box_half: [f32; 3],
    /// `OnceActivate`: a new one replaces the old.
    #[serde(default)]
    pub once_only: bool,
    /// `SubControlWeaponName`: the detonator that goes with it.
    #[serde(default)]
    pub controller: Option<String>,
    /// The blast (`AreaDamage`, `AreaDamageRadius`, `AreaDamageType`).
    #[serde(default)]
    pub area_damage: f32,
    #[serde(default)]
    pub area_radius: f32,
    #[serde(default = "explode")]
    pub area_type: u32,
    /// The `active` animation between the trigger and the blast, s.
    #[serde(default)]
    pub active: f32,
    /// The hornet's `ready` animation, s, and `SensorAreaRadius`, u.
    #[serde(default)]
    pub ready: f32,
    #[serde(default)]
    pub sensor_radius: f32,
    /// The weak points: damage types it takes more of.
    #[serde(default)]
    pub weak: Vec<(i32, f32)>,
    #[serde(default)]
    pub arc: Option<ArcParams>,
    #[serde(default)]
    pub caltrops: Option<CaltropsParams>,
    #[serde(default)]
    pub nerve: Option<NerveParams>,
    #[serde(default)]
    pub turret: Option<TurretParams>,
    #[serde(default)]
    pub kit: Option<KitParams>,
    #[serde(default)]
    pub medkit: Option<MedkitParams>,
    #[serde(default)]
    pub strike: Option<StrikeParams>,
}

/// `combat_arms:bolt`: a crossbow's bolt (7).
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct BoltParams {
    /// `Velocity` (u/s), `Lifetime`.
    pub speed: f32,
    pub lifetime: f32,
    /// The ammo's `Health`.
    pub health: f32,
    /// The direct damage (`InstDamage`, its type, the roll, the range
    /// bands, measured from the map's origin: Quirk 4).
    pub damage: DamageParams,
    /// The cone (`MinPerturb`, `MaxPerturb`, the dynamic factor).
    pub perturb: [u32; 2],
    #[serde(default)]
    pub dynamic_perturb: f32,
    /// The explosive bolt: the delay after the detonator press, the blast
    /// and the weapon's `Range` (it goes off beyond it).
    #[serde(default)]
    pub explosive: Option<ExplosiveParams>,
    /// The detonator that goes with it (`SubControlWeaponName`).
    #[serde(default)]
    pub controller: Option<String>,
}

/// The explosive bolt (7): `ExplosionDelayTime`, `AreaDamage`,
/// `AreaDamageRadius`, `AreaDamageType`, `Range`.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ExplosiveParams {
    pub delay: f32,
    pub area_damage: f32,
    pub area_radius: f32,
    pub area_type: u32,
    pub range: f32,
}

/// `combat_arms:controller`: a detonator (a weapon whose fire detonates
/// the owner's devices of `of`).
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ControllerParams {
    /// The weapon ID of the devices it sets off.
    pub of: String,
}
