//! Combat Arms' volumes (specs/combat_arms/world_dat.md, "Moving world
//! models and volumes"; grenades.md section 7): the maps' volume objects
//! (`Water`, `EndlessFall`, `PoisonGas`, ...), each tied to the world
//! model of its `Name`, as `CaVolume`s in the world.
//!
//! `world` hands them over as map entities (`MapEntities`: the class, its
//! `Viscosity`, `Current`, `Damage`, `PendingSec` and `Hidden`, and the
//! world model's polygons as one hull of planes). `load_volumes` makes a
//! `CaVolume` of each (class defaults where a property is missing) when a
//! map loads. A point is inside when it is behind every plane.
//!
//! What they do: viscosity and current slow and carry non-player objects
//! (grenades, `grenades::step`) and the liquids (`Water`,
//! `CorrosiveFluid`, `FreezingWater`) set their gravity to −200 u/s²;
//! players inside take `Damage` per second (`volume_damage`, on the
//! server), `EndlessFall`'s 10⁸ killing them, after `PendingSec` inside
//! when it is set.
//!
//! Known shortcuts: the volume is its world model's polygons' planes (the
//! exact shape for a convex brush; a concave one is taken as the
//! intersection of its faces' half-spaces); `Hidden` volumes stay off (no
//! map scripts to switch them); damage types, armour and the sound
//! filter, fog, tint, friction, ladder, wind, jump and player gravity
//! volumes aren't modelled (players' volume movement is movement.md's);
//! the pending time is checked every tick, not every 0.5 s.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::{
    core::{Damage, DamageKind, Health, Hitgroup, Intent, MovementState, SimClock, SimSet},
    map::{MapEntities, MapEntity, MapPart, entities::entity_to_engine},
};

/// Liquid gravity for non-player objects, u/s² (fixed in code).
pub const LIQUID_GRAVITY: f32 = -200.0;

/// The volume classes and their defaults: (class, liquid, viscosity,
/// damage per second (HP)).
pub const VOLUME_CLASSES: &[(&str, bool, f32, f32)] = &[
    ("Water", true, 0.035, 0.0),
    ("CorrosiveFluid", true, 0.035, 5.0),
    ("FreezingWater", true, 0.035, 10.0),
    ("VolumeBrush", false, 0.0, 0.0),
    ("PoisonGas", false, 0.0, 5.0),
    ("Electricity", false, 0.0, 20.0),
    ("Burn", false, 0.0, 20.0),
    ("ColdAir", false, 0.0, 5.0),
    ("EndlessFall", false, 0.0, 1.0e8),
    ("Ice", false, 0.0, 0.0),
    ("Wind", false, 0.0, 0.0),
    ("Filter", false, 0.0, 0.0),
    ("SafteyNet", false, 0.0, 0.0),
    ("Ladder", false, 0.4, 0.0),
    ("Weather", false, 0.0, 0.0),
    ("Gravity", false, 0.0, 0.0),
    ("DamageVolume", false, 0.0, 0.0),
    ("WorldVolume", false, 0.0, 0.0),
    ("JumpVolume", false, 0.0, 0.0),
];

/// Whether objects of `class` are volumes.
pub fn is_volume(class: &str) -> bool {
    VOLUME_CLASSES.iter().any(|(c, ..)| *c == class)
}

/// A volume in the world: engine axes, metres for its planes (outward
/// normals) and bounds; u/s for its current; HP per second.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct CaVolume {
    pub class: String,
    pub planes: Vec<(Vec3, f32)>,
    pub min: Vec3,
    pub max: Vec3,
    /// A liquid: −200 u/s² gravity for objects inside.
    pub liquid: bool,
    /// 0..1: speed lost, 5000 × this u/s².
    pub viscosity: f32,
    pub current: Vec3,
    /// Damage per second to players inside, HP.
    pub damage: f32,
    /// Seconds a player must be inside before it takes the damage.
    pub pending: f32,
    pub hidden: bool,
}

impl CaVolume {
    /// A `Water` box from `min` to `max` (m) with its viscosity and
    /// current (u/s).
    pub fn liquid_box(min: Vec3, max: Vec3, viscosity: f32, current: Vec3) -> Self {
        Self {
            class: "Water".into(),
            planes: Vec::new(),
            min,
            max,
            liquid: true,
            viscosity,
            current,
            damage: 0.0,
            pending: 0.0,
            hidden: false,
        }
    }

    /// Whether the point `p` (m) is inside.
    pub fn contains(&self, p: Vec3) -> bool {
        !self.hidden
            && p.cmpge(self.min - 1e-4).all()
            && p.cmple(self.max + 1e-4).all()
            && self.planes.iter().all(|(n, d)| n.dot(p) <= d + 1e-4)
    }

    /// The volume of map entity `e` (`MapEntities` with `scale` metres per
    /// entity unit), if its class is a volume's and it has a hull.
    pub fn from_entity(e: &MapEntity, scale: f32) -> Option<Self> {
        let class = e.classname();
        let &(class, liquid, viscosity, damage) = VOLUME_CLASSES.iter().find(|(c, ..)| *c == class)?;
        let hull = e.hulls.first()?;
        let points: Vec<Vec3> = hull.points.iter().map(|p| entity_to_engine(*p, scale)).collect();
        let (min, max) = points
            .iter()
            .fold((Vec3::MAX, Vec3::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
        let real = |k: &str| e.get(k).and_then(|v| v.trim().parse::<f32>().ok());
        Some(Self {
            class: class.into(),
            planes: hull
                .planes
                .iter()
                .map(|(n, d)| (entity_to_engine(*n, 1.0), d * scale))
                .collect(),
            min,
            max,
            liquid,
            viscosity: real("Viscosity").unwrap_or(viscosity).clamp(0.0, 1.0),
            current: e.get("Current").map_or(Vec3::ZERO, |_| e.vector("Current")),
            damage: real("Damage").unwrap_or(damage),
            pending: real("PendingSec").unwrap_or(0.0),
            hidden: e.get("Hidden").is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true")),
        })
    }
}

/// The map's volumes, made when it loads; players' damage inside them.
pub struct CaVolumesPlugin;

impl Plugin for CaVolumesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TimeInside>().add_systems(
            FixedUpdate,
            (
                load_volumes.run_if(resource_exists_and_changed::<MapEntities>),
                volume_damage.run_if(crate::core::authoritative),
            )
                .chain()
                .in_set(SimSet::Weapons),
        );
    }
}

/// Each map entity of a volume class as a `CaVolume` (part of the map).
fn load_volumes(entities: Res<MapEntities>, old: Query<Entity, With<CaVolume>>, mut commands: Commands) {
    for e in &old {
        commands.entity(e).despawn();
    }
    for e in entities.entities.iter() {
        if let Some(v) = CaVolume::from_entity(e, entities.scale) {
            commands.spawn((Name::new(format!("Volume ({})", v.class)), v, MapPart));
        }
    }
}

/// Since when each player has been inside each damaging volume.
#[derive(Resource, Default)]
struct TimeInside(HashMap<(Entity, Entity), f64>);

/// Players inside a damaging volume take its damage × the tick (after its
/// pending time).
fn volume_damage(
    volumes: Query<(Entity, &CaVolume)>,
    players: Query<(Entity, &Transform, &MovementState, &Health), With<Intent>>,
    clock: Res<SimClock>,
    mut inside: ResMut<TimeInside>,
    mut damage: MessageWriter<Damage>,
) {
    let now = clock.now;
    let dt = clock.dt();
    let mut seen = HashMap::new();
    for (p, t, state, health) in &players {
        if health.current <= 0.0 {
            continue;
        }
        let centre = t.translation + (state.hull_min + state.hull_max) / 2.0;
        for (ve, v) in &volumes {
            if v.damage <= 0.0 || !v.contains(centre) {
                continue;
            }
            let since = *inside.0.get(&(p, ve)).unwrap_or(&now);
            seen.insert((p, ve), since);
            if now - since + 1e-9 < v.pending as f64 {
                continue;
            }
            damage.write(Damage {
                target: p,
                attacker: None,
                amount: v.damage * 0.01 * dt,
                point: centre,
                dir: Vec3::ZERO,
                hitgroup: Hitgroup::Generic,
                kind: DamageKind::Generic,
                weapon: None,
                force: Vec3::ZERO,
            });
        }
    }
    inside.0 = seen;
}
