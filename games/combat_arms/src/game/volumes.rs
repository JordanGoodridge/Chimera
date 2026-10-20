//! Combat Arms' volumes (specs/combat_arms/world_dat.md, "Moving world
//! models and volumes"; grenades.md section 7): the maps' volume objects
//! (`Water`, `EndlessFall`, `PoisonGas`, ...), each tied to the world
//! model of its `Name`, as `CaVolume`s in the world.
//!
//! `world` hands them over as map entities (`MapEntities`: the class, its
//! `Viscosity`, `Current`, `Damage`, `PendingSec`, `Hidden`, `Friction`,
//! `PlayerSpeedFactor`, `PlayerGravity` and `Velocity`, and the world
//! model's polygons as one hull of planes). `load_volumes` makes a
//! `CaVolume` of each (class defaults where a property is missing) when a
//! map loads. A point is inside when it is behind every plane.
//!
//! What they do: viscosity and current slow and carry non-player objects
//! (grenades, `grenades::step`) and the liquids (`Water`,
//! `CorrosiveFluid`, `FreezingWater`) set their gravity to −200 u/s²
//! (`in_liquid`: also the effects' under-water tests and the first-person
//! muzzle flash's); players' movement in them (ladders, liquids,
//! viscosity, currents, friction, speed factors, player gravity, jump
//! pads, safety nets) is movement.md's (`movement`);
//! players inside take `Damage` per second (`volume_damage`, on the
//! server; burn for `CorrosiveFluid` and `Burn`), `EndlessFall`'s 10⁸
//! killing them, after `PendingSec` inside when it is set.
//! `ca_volumes_here [radius]` lists the volumes at the player's centre
//! (`volumes_near`).
//!
//! Known shortcuts: the volume is its world model's polygons' planes (the
//! exact shape for a convex brush; a concave one is taken as the
//! intersection of its faces' half-spaces); volumes stay as the map's
//! start-up commands leave them (`Hidden`, or switched off by a
//! `StartupCommand`: `world`; no other map scripts switch them); the
//! `DamageType` property, armour, the sound filter, fog and tint aren't
//! read (freezing damage is generic);
//! the class defaults of `Friction` (1, `Ice` 0) and `PlayerSpeedFactor`
//! (1) are ours (world_dat.md gives Ice's and Wind's only); the pending
//! time is checked every tick, not every 0.5 s.

use std::collections::HashMap;

/// What an `EndlessFall` volume's damage names as its weapon: the
/// `EndlessFall` damage kind (characters.md 10).
pub const ENDLESS_FALL: &str = "combat_arms:endless_fall";

use bevy::prelude::*;

use crate::{
    console::ConsoleAppExt,
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

/// A `Gravity` volume's `PlayerGravity` by default, u/s^2.
pub const DEFAULT_PLAYER_GRAVITY: f32 = -2000.0;

/// The classes whose damage burns (corrosive 5 burn/s, `Burn`).
const BURNING: &[&str] = &["CorrosiveFluid", "Burn"];

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
    /// `Friction`: the player's ground friction is 5 x the least of the
    /// volumes' it is in (movement.md "Constants"); 1, `Ice` 0.
    pub friction: f32,
    /// `PlayerSpeedFactor`: multiplies the player's speed limit; 1.
    pub speed_factor: f32,
    /// A `Gravity` volume's `PlayerGravity` (u/s^2 along Y, −2000): the
    /// players' gravity inside is the sum of these.
    pub player_gravity: Option<f32>,
    /// A `JumpVolume`'s `Velocity` (u/s, a number in the maps, taken as
    /// straight up: ours): added to a player's velocity by a jump inside,
    /// in place of the jump speed.
    pub jump_velocity: Option<Vec3>,
    /// The damage's kind: burn for the burning classes, else generic.
    pub kind: DamageKind,
}

impl CaVolume {
    /// A `Water` box from `min` to `max` (m) with its viscosity and
    /// current (u/s).
    pub fn liquid_box(min: Vec3, max: Vec3, viscosity: f32, current: Vec3) -> Self {
        Self {
            viscosity,
            current,
            ..Self::class_box("Water", min, max).expect("a volume class")
        }
    }

    /// A box of volume class `class` from `min` to `max` (m) with the
    /// class's defaults; None for a class that isn't a volume's.
    pub fn class_box(class: &str, min: Vec3, max: Vec3) -> Option<Self> {
        let &(class, liquid, viscosity, damage) = VOLUME_CLASSES.iter().find(|(c, ..)| *c == class)?;
        Some(Self {
            class: class.into(),
            planes: Vec::new(),
            min,
            max,
            liquid,
            viscosity,
            current: Vec3::ZERO,
            damage,
            pending: 0.0,
            hidden: false,
            friction: if class == "Ice" { 0.0 } else { 1.0 },
            speed_factor: 1.0,
            player_gravity: (class == "Gravity").then_some(DEFAULT_PLAYER_GRAVITY),
            jump_velocity: (class == "JumpVolume").then_some(Vec3::ZERO),
            kind: if BURNING.contains(&class) { DamageKind::Burn } else { DamageKind::Generic },
        })
    }

    /// A ladder (`Ladder`): characters inside climb (movement.md "Other
    /// modifiers").
    pub fn is_ladder(&self) -> bool {
        self.class == "Ladder"
    }

    /// A safety net (`SafteyNet`): no fall damage inside.
    pub fn is_safety_net(&self) -> bool {
        self.class == "SafteyNet"
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
        let defaults = Self::class_box(e.classname(), Vec3::ZERO, Vec3::ZERO)?;
        let hull = e.hulls.first()?;
        let points: Vec<Vec3> = hull.points.iter().map(|p| entity_to_engine(*p, scale)).collect();
        let (min, max) = points
            .iter()
            .fold((Vec3::MAX, Vec3::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
        let real = |k: &str| e.get(k).and_then(|v| v.trim().parse::<f32>().ok());
        Some(Self {
            planes: hull
                .planes
                .iter()
                .map(|(n, d)| (entity_to_engine(*n, 1.0), d * scale))
                .collect(),
            min,
            max,
            viscosity: real("Viscosity").unwrap_or(defaults.viscosity).clamp(0.0, 1.0),
            current: e.get("Current").map_or(Vec3::ZERO, |_| e.vector("Current")),
            damage: real("Damage").unwrap_or(defaults.damage),
            pending: real("PendingSec").unwrap_or(0.0),
            hidden: e.get("Hidden").is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true")),
            friction: real("Friction").unwrap_or(defaults.friction),
            speed_factor: real("PlayerSpeedFactor").unwrap_or(defaults.speed_factor),
            player_gravity: defaults.player_gravity.map(|g| real("PlayerGravity").unwrap_or(g)),
            jump_velocity: defaults.jump_velocity.map(|v| real("Velocity").map_or(v, |up| Vec3::Y * up)),
            ..defaults
        })
    }
}

/// The objects a map's start-up command line (a `StartupCommand`
/// object's `Command`) switches at once, in order: `msg <name> on` (true)
/// or `off` (false). Commands are separated by `;`; delayed ones
/// (`delay <s> (...)`) and other messages are skipped.
pub fn startup_switches(command: &str) -> Vec<(String, bool)> {
    command
        .split(';')
        .filter_map(|c| {
            let mut words = c.split_whitespace();
            if !words.next()?.eq_ignore_ascii_case("msg") {
                return None;
            }
            let name = words.next()?;
            let on = match words.next()? {
                w if w.eq_ignore_ascii_case("on") => true,
                w if w.eq_ignore_ascii_case("off") => false,
                _ => return None,
            };
            words.next().is_none().then(|| (name.to_string(), on))
        })
        .collect()
}

/// Whether the point `p` (m) is inside a liquid volume (`Water`,
/// `CorrosiveFluid`, `FreezingWater`): effects_sounds.md's under-water
/// tests, view_models.md's muzzle flash.
pub fn in_liquid<'a>(volumes: impl IntoIterator<Item = &'a CaVolume>, p: Vec3) -> bool {
    volumes.into_iter().any(|v| v.liquid && v.contains(p))
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
        )
        .console_command(
            "ca_volumes_here",
            "ca_volumes_here [radius u = 0]: the map's volumes at your centre (or within the radius of it): name, class, bounds (u, engine axes), damage, pending, hidden, whether you are inside.",
            |w, a| {
                let radius = a.first().and_then(|r| r.parse::<f32>().ok()).unwrap_or(0.0).max(0.0);
                let p = crate::console::command_player(w)?;
                let t = w.get::<Transform>(p).ok_or("no transform")?.translation;
                let centre = w.get::<MovementState>(p).map_or(t, |s| t + (s.hull_min + s.hull_max) / 2.0);
                Ok(Some(volumes_near(w, centre, radius)))
            },
        );
    }
}

/// `ca_volumes_here`'s listing: the volumes whose bounds come within
/// `radius_u` (u) of `centre` (m).
pub fn volumes_near(w: &mut World, centre: Vec3, radius_u: f32) -> String {
    let unit = super::movement::movement_data().meters_per_unit;
    let r = radius_u * unit;
    let mut lines = vec![format!("centre {:.1} (u, engine axes)", centre / unit)];
    let mut q = w.query::<(Option<&Name>, &CaVolume)>();
    for (n, v) in q.iter(w) {
        if !(centre.cmpge(v.min - r).all() && centre.cmple(v.max + r).all()) {
            continue;
        }
        lines.push(format!(
            "{}: {} {:.1}..{:.1} damage {}/s{} pending {} hidden {} friction {} viscosity {}{}",
            n.map_or("?", |n| n.as_str()),
            v.class,
            v.min / unit,
            v.max / unit,
            v.damage,
            if v.damage > 0.0 { format!(" ({:?})", v.kind) } else { String::new() },
            v.pending,
            v.hidden,
            v.friction,
            v.viscosity,
            if v.contains(centre) { " INSIDE" } else { "" }
        ));
    }
    if lines.len() == 1 {
        lines.push("no volumes".into());
    }
    lines.join("\n")
}

/// Each map entity of a volume class as a `CaVolume` (part of the map).
fn load_volumes(entities: Res<MapEntities>, old: Query<Entity, With<CaVolume>>, mut commands: Commands) {
    for e in &old {
        commands.entity(e).despawn();
    }
    for e in entities.entities.iter() {
        if let Some(v) = CaVolume::from_entity(e, entities.scale) {
            let name = match e.get("targetname") {
                Some(n) => format!("Volume {n} ({})", v.class),
                None => format!("Volume ({})", v.class),
            };
            commands.spawn((Name::new(name), v, MapPart));
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
                kind: v.kind,
                // The endless fall's own damage kind (characters.md 10:
                // its death animation `D_W`).
                weapon: (v.class == "EndlessFall").then_some(ENDLESS_FALL),
                force: Vec3::ZERO,
            });
        }
    }
    inside.0 = seen;
}
