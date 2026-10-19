//! Combat Arms' player movement, from specs/combat_arms/movement.md: the
//! movement slot's implementation `combat_arms:movement` (docs/DESIGN.md,
//! "Slots"), the loadout's default. LithTech "player physics": each tick
//! the horizontal velocity is clamped to the stance's speed limit, an
//! acceleration is built from the held keys (ramped from a start value),
//! and both are integrated in closed form with exponential ground friction
//! (gravity in the air, quarter-step displacement without friction).
//! Crouching blends the speed limit and the hull height by a crouch rate
//! (with the crouch-spam slowdown and lock-out); sprint drains a stamina
//! gauge; jumps set the vertical speed and leave a jump factor that weakens
//! the next jump and the air acceleration; landings from high enough deal
//! fall damage (`core::Damage`, crush, from the player).
//!
//! The maths runs in world units (u, +Y up, as the spec); the character's
//! `Transform` (engine meters) is the hull's centre, `meters_per_unit`
//! apart (data/movement.toml). The hull is an upright cylinder of radius
//! `max(halfX, halfZ)` and the crouch-blended half-height; the entity's
//! `Collider` follows it (`sync_collider`). Collision is avian shape casts
//! (`MoveAndSlide::cast_move`, keeping a 0.1 u skin): slide along up to 5
//! planes, stair-step up to 30.5 u, a 1 u (0.02 x halfY) standing probe
//! below the hull. Parry's cylinder casts stop short of flat faces on long
//! sweeps and report slightly tilted normals, so `Space::cast` sweeps a
//! hit again from where it stopped until it settles, and takes the
//! surface's normal from a short ray at the contact.
//!
//! Numbers: data/movement.toml (`MovementData`, spec values marked by
//! section). Per player: `CaCharacter` (figure and gear: sprint, stamina,
//! penalties; the reference character by default) and `HeldMovePenalty`
//! (the held weapon's `MovePenalty`, set by the weapon part; none: 0);
//! `StunPenalty` (P_stun, from a stun bullet: `attachments`). The HUD
//! reads the stamina gauge and the crouch lock-out from `CaMovement`.
//! The eye sits `eye_above_centre` over the hull's centre (83 u above the
//! feet standing, 60 crouched); `CameraRise` is the camera's upward
//! smoothing (the front end's `ui::effects::camera_motion` applies it to
//! the first-person camera each rendered frame). An infected character (`CaInfected`) hit by the other team
//! gets a `HitKnockback` from the weapon part (its ammo's
//! `DamageMovePenalty`): the push replaces the acceleration while it runs.
//! A spawn or respawn puts the player on the floor below and measures
//! falls from there; a teleport resets nothing.
//!
//! Known shortcuts (docs/tech-debt.md): Combat Arms movement
//! (specs/combat_arms/movement.md) runs once per engine tick, the game's
//! tick set to the spec's 15 ms (`game::TICK_INTERVAL`); the spec's own
//! frame accumulator (at most 6 ticks per frame, the rest dropped) is
//! `TickClock`, not wired to the engine's fixed clock. So what the spec
//! runs per rendered frame runs per tick here: the stamina gauge, and the
//! crouch release delay is checked against the tick's time, not the
//! frame's (the same totals; edges a frame apart). Sprint applies at once (the
//! spec's listen-server case). Not modelled: ladders, volumes (water,
//! viscosity, currents, gravity, jump pads, safety nets), explosion
//! pushers, movement disallowed (freeze), the infection stagger (nothing
//! sets `CaInfected` yet: no game modes), VIP accounts, kill-streak and
//! stamina perks, the ammo's own stun, the "falling" sound status, stamina kits,
//! jump stamina, maps' not-a-step surface flag (characters and moving
//! bodies are not steps). The shape cast reports where it touched, not
//! which cap: a stair-step drop or a standing probe that touches an edge
//! above the hull's flat bottom is taken as the curved side (a level
//! normal). Scale: the spec stays in world units and the game calls 40 u
//! a metre for its HUD readouts (`hud_metres`), but its bodies aren't
//! built to that (a 100 u hull would be 2.5 m), so `meters_per_unit`
//! scales by body size instead (open question 3; to be checked live).
//! Why it's OK for now: every spec test case is a scenario test on the
//! real collision (games/combat_arms/tests/it/movement.rs) and passes
//! within the stated tolerances; the missing pieces have nothing in
//! Chimera's Combat Arms to act on yet.

use std::sync::OnceLock;

use avian3d::{character_controller::move_and_slide::DepenetrationConfig, prelude::*};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    character::{CAPSULE_HEIGHT, CAPSULE_RADIUS},
    core::{
        CHARACTER_FILTER, Damage, DamageKind, Dead, FirstTimePredicted, Hitgroup, Intent, MovementState, Predict,
        PredictedAppExt, SimClock, Velocity,
    },
    slots::{Loadout, RegisterSlots},
    weapon::Zoomed,
};

/// The movement Combat Arms' players get.
pub const ID: &str = "combat_arms:movement";

/// Ours: how far below a spawn point the floor is looked for (u).
const SPAWN_DROP: f32 = 100_000.0;

/// data/movement.toml, built in.
pub const MOVEMENT_DATA: &str = include_str!("../../data/movement.toml");

/// The movement's numbers (data/movement.toml; spec "Constants").
#[derive(Resource, Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MovementData {
    pub meters_per_unit: f32,
    pub skin: f32,
    pub eye_offset: f32,
    pub eye_crouch_offset: f32,
    pub camera_rise_time: f32,
    pub camera_rise_min_speed: f32,
    pub camera_rise_max_lag: f32,
    pub hud_units_per_metre: f32,
    pub run_forward: f32,
    pub run_back: f32,
    pub run_strafe: f32,
    pub walk: f32,
    pub crouch: f32,
    pub jump: f32,
    pub ladder: f32,
    pub gravity: f32,
    pub friction: f32,
    pub base_accel: f32,
    pub start_accel: f32,
    pub max_accel: f32,
    pub accel_ramp: f32,
    pub crouch_accel_factor: f32,
    pub in_air_accel_factor: f32,
    pub slide_to_stop_time: f32,
    pub stop_snap_speed: f32,
    pub rest_threshold: f32,
    pub jump_factor_after_jump: f32,
    pub jump_factor_recovery: f32,
    pub jump_stance_tolerance: f32,
    pub walkable_normal: f32,
    pub stair_step: f32,
    pub standing_probe: f32,
    pub stand_half_extents: [f32; 3],
    pub crouch_half_extents: [f32; 3],
    pub crouch_release_delay: f64,
    pub fall_damage_start: f32,
    pub fall_damage_full: f32,
    pub fall_damage_at_full: f32,
    pub fall_minimum: f32,
    pub health_points: f32,
    pub crouch_spam: CrouchSpamData,
    pub character: CaCharacter,
}

/// The crouch-spam penalty's parameters (spec "Crouch-spam penalty").
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrouchSpamData {
    pub duration: f32,
    pub tangents: [f32; 2],
    pub enable_slowdown: bool,
    pub slowdown_increase_duration: f32,
    pub slowdown_increase_tangents: [f32; 2],
    pub slowdown_decrease_amount: f32,
    pub max_slowdown_rate: f32,
    pub min_slowdown_sp: f32,
    pub enable_lockout: bool,
    pub lockout_increase_duration: f32,
    pub lockout_increase_tangents: [f32; 2],
    pub lockout_decrease_amount: f32,
    pub lockout_decrease_amount_in_penalty: f32,
    pub decrease_during_jumping_crouch: bool,
    pub decrease_by_keystroke: bool,
    pub decrease_exception_no_movement: bool,
    pub no_movement_time_limit: f32,
    pub no_movement_deviation_limit: f32,
}

/// A player's figure type and worn gear: what the speed limit, sprint and
/// stamina read (spec "Character and weapon data"). Absent on a player:
/// the data's (the reference character).
#[derive(Component, Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaCharacter {
    pub figure: Figure,
    #[serde(default)]
    pub gear: Vec<Gear>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Figure {
    /// `DashSpeed`: sprint multiplier on the run speed.
    pub dash_speed: f32,
    /// `DashGauge`: stamina drained per second sprinting.
    pub dash_gauge: f32,
    /// `GaugeRec`: stamina recovered per second.
    pub gauge_recovery: f32,
    /// `Maxstamina`.
    pub max_stamina: f32,
    /// `VariableSpeed`, `VariableWeight`, `VariableJump`.
    pub variable_speed: f32,
    pub variable_weight: f32,
    pub variable_jump: f32,
    /// `StiffDuration`: the hit knock-back lasts `1 + this` times longer.
    #[serde(default)]
    pub stiff_duration: f32,
}

/// One worn item (vest, head piece, goggles, backpack, gear mod).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gear {
    pub name: String,
    /// `DashSpeed`: added to the sprint bonus sum.
    pub dash_speed: f32,
    /// `MaxStamina`: added to the capacity bonus sum.
    pub max_stamina: f32,
    /// `MovePenalty`: added to the penalty.
    pub move_penalty: f32,
    /// Jump bonus (`J_model`).
    pub jump: f32,
}

impl CaCharacter {
    /// No gear, figure values as given (the spec's test cases' "no gear
    /// bonuses").
    pub fn without_gear(&self) -> Self {
        Self {
            figure: self.figure.clone(),
            gear: Vec::new(),
        }
    }

    fn gear_sum(&self, f: impl Fn(&Gear) -> f32) -> f32 {
        self.gear.iter().map(f).sum()
    }

    /// The stamina capacity: floor(Maxstamina x (1 + bonuses)).
    pub fn stamina_max(&self) -> f32 {
        (self.figure.max_stamina * (1.0 + self.gear_sum(|g| g.max_stamina))).floor()
    }
}

/// The held weapon's `MovePenalty` and its modification's factor
/// (`c_mod`), on the player. Set by the weapon part; absent: no penalty.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct HeldMovePenalty {
    pub penalty: f32,
    pub mod_factor: f32,
}

/// A stun's slowdown (spec "Penalty", P_stun): added to the penalty until
/// `until` (game time, s). Set by the shooter's stun bullet
/// (`attachments`); predicted with the victim's movement.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StunPenalty {
    pub penalty: f32,
    pub until: f64,
}

/// An infected character of the infection mode: hits from the other
/// team knock it back (spec "Other modifiers", `HitKnockback`). Nothing
/// sets it yet (no game modes).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct CaInfected;

/// A running hit knock-back (spec "Other modifiers", "Hit knock-back"):
/// set on an infected character by the weapon that hit it (its ammo's
/// `DamageMovePenalty` K and `DamageMovePenaltyDuration`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HitKnockback {
    /// The strength K: the push starts at K^2 u/s^2.
    pub strength: f32,
    /// The last hit's direction (the way the damage travelled), engine
    /// space; its horizontal part is pushed along.
    pub dir: Vec3,
    /// How long it lasts, s (the duration x (1 + `StiffDuration`)).
    pub duration: f32,
    /// When the movement first saw it (its t = 0); None until then.
    pub started: Option<f64>,
}

impl HitKnockback {
    /// A hit with knock-back `strength` for `duration` s on a figure with
    /// `stiff` (`StiffDuration`) at `now`, over the one running (`current`):
    /// the new timer replaces it only when longer than the time it has
    /// left; the direction is always the latest hit's. None: no knock-back
    /// (both values must be above 0).
    pub fn hit(current: Option<&Self>, strength: f32, duration: f32, stiff: f32, dir: Vec3, now: f64) -> Option<Self> {
        if strength <= 0.0 || duration <= 0.0 {
            return current.copied();
        }
        let new = Self {
            strength,
            dir,
            duration: duration * (1.0 + stiff),
            started: None,
        };
        Some(match current {
            Some(c) => {
                let left = c.started.map_or(c.duration as f64, |s| c.duration as f64 - (now - s)) as f32;
                if new.duration > left { new } else { Self { dir, ..*c } }
            }
            None => new,
        })
    }

    /// Seconds since it started at `now` while it runs; None when over.
    fn running(&self, now: f64) -> Option<f32> {
        let t = (now - self.started.unwrap_or(now)) as f32;
        (t < self.duration - 1e-6).then_some(t)
    }
}

/// The first-person camera's upward smoothing (spec "Eye (camera)
/// height"): rising more than 1 u (not in a jump) it moves at
/// `max(gap / 0.1 s, 140 u/s)`, at most 30 u behind, and snaps once
/// within 1 u; downward moves are immediate. Heights in u. The client's
/// camera runs it each rendered frame (`ui::effects::camera_motion`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CameraRise {
    pub height: Option<f32>,
}

impl CameraRise {
    /// One rendered frame of `dt` s toward the wanted height: the camera's.
    pub fn step(&mut self, wanted: f32, dt: f32, in_jump: bool, data: &MovementData) -> f32 {
        let h = match self.height {
            Some(h) if wanted - h > 1.0 && !in_jump => {
                let h = h.max(wanted - data.camera_rise_max_lag);
                let speed = ((wanted - h) / data.camera_rise_time).max(data.camera_rise_min_speed);
                let h = (h + speed * dt).min(wanted);
                if wanted - h <= 1.0 { wanted } else { h }
            }
            _ => wanted,
        };
        self.height = Some(h);
        h
    }
}

/// The eye's height above the hull's centre (u) at crouch rate `r` (spec
/// "Eye (camera) height"): fixed from the standing box, plus the crouch
/// offset.
pub fn eye_above_centre(r: f32, data: &MovementData) -> f32 {
    data.eye_offset * data.stand_half_extents[1] + data.eye_crouch_offset * r
}

/// A distance as the game's HUD readouts print it: whole "metres" of 40 u
/// (spec "Units and conventions").
pub fn hud_metres(units: f32, data: &MovementData) -> u32 {
    (units / data.hud_units_per_metre).floor().max(0.0) as u32
}

/// The movement data, parsed once per process, with the install's
/// attribute data over it when it can be read (`data::with_install_movement`).
/// Bad data panics (the tests load it).
pub fn movement_data() -> &'static MovementData {
    static DATA: OnceLock<MovementData> = OnceLock::new();
    DATA.get_or_init(|| {
        super::data::with_install_movement(
            toml::from_str(MOVEMENT_DATA).unwrap_or_else(|e| panic!("data/movement.toml: {e}")),
        )
    })
}

/// Combat Arms' movement: registered as `ID` and made the loadout's
/// default.
pub struct CaMovementPlugin;

impl Plugin for CaMovementPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(movement_data().clone())
            .insert_resource(Loadout { movement: ID })
            .register_movement::<CaMovement>(ID)
            .add_systems(Predict::Movement, (step, sync_collider).chain())
            .predicted_net::<CaMovement>();
    }
}

/// The stamina gauge's state (spec "Sprint and stamina").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StaminaState {
    #[default]
    Recovering,
    Draining,
    /// Sprint held with an empty gauge: pinned at 0.
    Stopped,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
enum CrouchPhase {
    #[default]
    Idle,
    Down,
    Up,
}

/// What the player stands on (from the last move).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Standing {
    pub normal: Vec3,
    /// Another character (never ground for movement).
    pub character: bool,
}

/// Combat Arms' movement state on a player: the marker the slot inserts
/// and everything the movement keeps between ticks. Units are world units
/// (u, u/s). Public for the HUD (stamina, lock-out) and the tests.
#[derive(Component, Clone, Debug, Default, Serialize, Deserialize)]
pub struct CaMovement {
    // Control flags (spec "Input to control state"), this tick's.
    pub crouch: bool,
    pub run: bool,
    pub jump: bool,
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub moving: bool,
    pub sprint: bool,
    raw_crouch: bool,
    crouch_pressed_at: f64,
    jump_used: bool,
    /// In a jump (from the jump tick to the landing).
    pub in_jump: bool,
    /// The jump factor `j`, 1 at rest.
    pub jump_factor: f32,
    /// On ground for movement (walkable, not a character, not in a jump).
    pub on_ground: bool,
    /// What the hull rests on after the last move.
    pub standing: Option<Standing>,
    /// The crouch rate `r` (0 standing, 1 crouched).
    pub crouch_rate: f32,
    crouch_phase: CrouchPhase,
    crouch_elapsed: f32,
    /// The hull's half-height now (u; 0 before the first tick).
    pub half_height: f32,
    // Acceleration ramp.
    ramp_forward: f32,
    ramp_strafe: f32,
    /// The last direction speed `K`.
    pub direction_speed: f32,
    /// This tick's speed limit `V` and acceleration (u/s, u/s^2).
    pub speed_limit: f32,
    pub accel: Vec3,
    /// Stamina (spec "Sprint and stamina").
    pub stamina: f32,
    pub stamina_state: StaminaState,
    /// Crouch-spam values: slowdown and lock-out stamina, lock-out active.
    pub slowdown: f32,
    pub lockout: f32,
    pub crouch_locked: bool,
    spam_rising: bool,
    spam_elapsed: f32,
    /// Horizontal positions of the last ticks (time, x, z) for "still".
    history: Vec<(f64, Vec2)>,
    /// Highest Y while not standing (fall damage), u.
    fall_top: Option<f32>,
    /// Dead last tick (a respawn puts the player on the floor again).
    was_dead: bool,
    initialized: bool,
}

impl CaMovement {
    /// The stamina as the HUD shows it (whole points).
    pub fn stamina_points(&self) -> f32 {
        self.stamina.floor()
    }
}

/// The spec's crouch curve: the Hermite-like shape with tangents `t0`, `t1`.
pub fn crouch_shape(t: f32, [t0, t1]: [f32; 2]) -> f32 {
    (3.0 * t * t - 4.0 * t + 1.0) * t0 + (-6.0 * t * t + 6.0 * t) + (3.0 * t * t - 2.0 * t) * t1
}

/// The fall damage (HP) of a drop of `d` u, if any (spec "Falling and fall
/// damage").
pub fn fall_damage(d: f32, data: &MovementData) -> Option<f32> {
    (d > data.fall_minimum && d >= data.fall_damage_start).then(|| {
        data.fall_damage_at_full * (d - data.fall_damage_start) / (data.fall_damage_full - data.fall_damage_start)
    })
}

/// The spec's fixed movement clock (spec "Tick clock"): real time in, whole
/// ticks out, at most `max_per_frame` a frame (the rest dropped), and the
/// leftover fraction for drawing between ticks. Not the engine's clock
/// (see the module's known shortcuts).
#[derive(Clone, Copy, Debug)]
pub struct TickClock {
    pub tick_ms: u32,
    pub max_per_frame: u32,
    accumulator_ms: f64,
}

impl TickClock {
    /// The spec's: 15 ms ticks, at most `floor(100 ms / tick)` a frame.
    pub fn new(tick_ms: u32) -> Self {
        Self {
            tick_ms,
            max_per_frame: 100 / tick_ms,
            accumulator_ms: 0.0,
        }
    }

    pub fn with_remainder(mut self, ms: f64) -> Self {
        self.accumulator_ms = ms;
        self
    }

    /// Add a frame of `ms`: the ticks to run now.
    pub fn frame(&mut self, ms: f64) -> u32 {
        self.accumulator_ms += ms;
        let tick = self.tick_ms as f64;
        let due = (self.accumulator_ms / tick).floor() as u32;
        let run = due.min(self.max_per_frame);
        // Ticks beyond the cap are dropped with their time.
        self.accumulator_ms -= due as f64 * tick;
        run
    }

    /// The leftover fraction of a tick (interpolation factor).
    pub fn fraction(&self) -> f64 {
        self.accumulator_ms / self.tick_ms as f64
    }
}

/// The world as the movement sees it, in world units.
struct Space<'a, 'w, 's> {
    mover: &'a MoveAndSlide<'w, 's>,
    filter: SpatialQueryFilter,
    scale: f32,
    skin: f32,
    radius: f32,
    is_character: &'a dyn Fn(Entity) -> bool,
    /// Not a step (spec "Slopes and steps"): characters and moving
    /// objects (anything but a static body).
    not_a_step: &'a dyn Fn(Entity) -> bool,
}

struct Hit {
    /// Safe distance (u) along the cast.
    distance: f32,
    /// The contact on the surface hit (u).
    point: Vec3,
    normal: Vec3,
    entity: Entity,
}

impl Space<'_, '_, '_> {
    fn shape(&self, half_height: f32) -> Collider {
        Collider::cylinder(self.radius * self.scale, 2.0 * half_height * self.scale)
    }

    /// Sweep the hull from `centre` by `delta`: the first thing it would
    /// touch, if any. The shape cast stops short of flat faces by up to a
    /// few units on long sweeps (GJK's tolerance), so a hit is swept again
    /// from where it stopped until it stops moving; a hit that a closer
    /// sweep no longer finds was not there.
    fn cast(&self, centre: Vec3, half_height: f32, delta: Vec3) -> Option<Hit> {
        let len = delta.length();
        if len < 1e-6 {
            return None;
        }
        let dir = delta / len;
        let mut travelled = 0.0;
        let mut last = None;
        for _ in 0..6 {
            let rest = len - travelled;
            if rest <= 1e-4 {
                break;
            }
            let hit = self.cast_once(centre + dir * travelled, half_height, dir * rest)?;
            travelled += hit.distance.min(rest);
            let done = hit.distance < 1e-3;
            last = Some(hit);
            if done {
                break;
            }
        }
        last.map(|h| Hit { distance: travelled, ..h })
    }

    fn cast_once(&self, centre: Vec3, half_height: f32, delta: Vec3) -> Option<Hit> {
        let hit = self.mover.cast_move(
            &self.shape(half_height),
            centre * self.scale,
            Quat::IDENTITY,
            delta * self.scale,
            self.skin * self.scale,
            &self.filter,
        )?;
        Some(Hit {
            distance: hit.distance / self.scale,
            point: hit.point1 / self.scale,
            normal: self.face_normal(hit.point1, hit.normal1, hit.entity),
            entity: hit.entity,
        })
    }

    /// The surface's own normal at a cast's contact (meters): a short ray
    /// into the surface there. The shape cast's normal for a cylinder is
    /// off by up to a few hundredths, enough to tilt "flat" ground.
    fn face_normal(&self, point: Vec3, approx: Vec3, entity: Entity) -> Vec3 {
        let Ok(dir) = Dir3::new(-approx) else {
            return approx;
        };
        let back = 0.5 * self.scale;
        self.mover
            .spatial_query
            .cast_ray(point + approx * back, dir, 2.0 * back, true, &self.filter)
            .filter(|h| h.entity == entity && h.normal.dot(approx) > 0.5)
            .map_or(approx, |h| h.normal)
    }

    fn fits(&self, centre: Vec3, half_height: f32) -> bool {
        self.mover
            .spatial_query
            .shape_intersections(&self.shape(half_height), centre * self.scale, Quat::IDENTITY, &self.filter)
            .is_empty()
    }

    /// Push the hull out of anything it overlaps.
    fn depenetrate(&self, centre: Vec3, half_height: f32) -> Vec3 {
        if self.fits(centre, half_height) {
            return centre;
        }
        let config = DepenetrationConfig {
            skin_width: self.skin * self.scale,
            ..default()
        };
        let offset = self
            .mover
            .depenetrate(&self.shape(half_height), centre * self.scale, Quat::IDENTITY, &config, &self.filter);
        centre + offset / self.scale
    }

    /// Slide `delta` from `centre` along up to 5 planes: the end and what
    /// was hit (normal, entity).
    fn slide(&self, mut centre: Vec3, half_height: f32, delta: Vec3) -> (Vec3, Vec<(Vec3, Entity)>) {
        let mut planes: Vec<Dir3> = Vec::new();
        let mut normals = Vec::new();
        let mut remaining = delta;
        for _ in 0..5 {
            if remaining.length_squared() < 1e-10 {
                break;
            }
            match self.cast(centre, half_height, remaining) {
                None => {
                    centre += remaining;
                    break;
                }
                Some(hit) => {
                    let dir = remaining.normalize();
                    let moved = dir * hit.distance.min(remaining.length());
                    centre += moved;
                    normals.push((hit.normal, hit.entity));
                    if let Ok(n) = Dir3::new(hit.normal) {
                        planes.push(n);
                    }
                    remaining = MoveAndSlide::project_velocity(remaining - moved, &planes);
                }
            }
        }
        (centre, normals)
    }

    /// What the hull at `centre` stands on, within `probe` u below
    /// (spec "Slopes and steps", "What the player stands on"); `before`:
    /// what it stood on last.
    fn ground(
        &self,
        data: &MovementData,
        centre: Vec3,
        half_height: f32,
        probe: f32,
        before: Option<Standing>,
    ) -> Option<Standing> {
        let len = probe + self.skin;
        let hit = self.cast(centre, half_height, Vec3::NEG_Y * len)?;
        if hit.normal.y < 0.001 {
            return None;
        }
        let character = (self.is_character)(hit.entity);
        let steep = hit.normal.y < data.walkable_normal;
        if steep
            && let Some(old) = before.filter(|b| !b.character && b.normal.y >= data.walkable_normal)
        {
            // A slope beside the ground it stood on: it keeps standing on
            // that ground unless a probe from a step up, moved up-hill,
            // meets something.
            let ny = hit.normal.y;
            let t = (hit.distance / len).clamp(0.0, 1.0);
            let step = data.stair_step;
            let ahead = (0.5 * self.radius)
                .max(2.0 * (step + data.standing_probe * half_height * t) * (1.0 - ny * ny) + 0.11 * self.radius);
            let uphill = -Vec3::new(hit.normal.x, 0.0, hit.normal.z).normalize_or_zero();
            let from = centre + Vec3::Y * step;
            if uphill != Vec3::ZERO && self.cast(from, half_height, uphill * ahead).is_none() {
                return Some(old);
            }
        }
        Some(Standing {
            normal: hit.normal,
            character,
        })
    }

    /// Grow the hull from `old` to `new` half-height (spec "Crouching",
    /// "Growing"): down by up to the growth, then up by up to twice it,
    /// centred in the span found. Returns the centre and whether it fits
    /// (when not, it is forced around the old centre).
    fn grow(&self, centre: Vec3, old: f32, new: f32) -> (Vec3, bool) {
        let d = new - old;
        let down = self.cast(centre, old, Vec3::NEG_Y * d).map_or(d, |h| h.distance.min(d));
        let low = centre - Vec3::Y * down;
        let up = self.cast(low, old, Vec3::Y * (2.0 * d)).map_or(2.0 * d, |h| h.distance.min(2.0 * d));
        if old + up / 2.0 < new - 1e-4 {
            return (centre, false);
        }
        (low + Vec3::Y * (up / 2.0), true)
    }
}

/// Spec "Slopes and steps": the collision move with stair stepping.
fn move_hull(
    space: &Space,
    data: &MovementData,
    centre: Vec3,
    half_height: f32,
    delta: Vec3,
    floor: Option<Vec3>,
    steep: Option<Vec3>,
) -> Vec3 {
    let mut delta = delta;
    // 1. Moving up a steep slope while standing on it: the up-slope part
    // goes.
    if let Some(n) = steep {
        let nh = Vec2::new(n.x, n.z).normalize_or_zero();
        let dh = Vec2::new(delta.x, delta.z);
        let into = dh.dot(nh);
        if into < 0.0 {
            let dh = dh - nh * into;
            delta.x = dh.x;
            delta.z = dh.y;
        }
    }
    // 2. The low path: along the floor plane when standing on walkable
    // ground. It is kept when it touched nothing, or anything that is not
    // a step.
    let low_delta = match floor {
        Some(n) if n.y > 0.0 => Vec3::new(delta.x, delta.y - n.dot(delta) / n.y, delta.z),
        _ => delta,
    };
    let (low, hits) = space.slide(centre, half_height, low_delta);
    let horizontal = Vec2::new(delta.x, delta.z);
    if horizontal.length_squared() < 1e-10 || hits.is_empty() || hits.iter().any(|(_, e)| (space.not_a_step)(*e)) {
        return low;
    }
    // 3. Lift (a low ceiling: keep the low result).
    let step = data.stair_step;
    if space.cast(centre, half_height, Vec3::Y * step).is_some() {
        return low;
    }
    // 4. The high path: slide from the lifted start, drop back at most the
    // lift.
    let lifted = centre + Vec3::Y * step;
    let (across, high_hits) = space.slide(lifted, half_height, delta);
    let high = match space.cast(across, half_height, Vec3::NEG_Y * step) {
        None => across - Vec3::Y * step,
        Some(hit) => {
            let landed = across - Vec3::Y * hit.distance;
            // What the flat bottom lands on carries its normal; a contact
            // higher up is an edge touching the curved side, whose normal
            // is level: steep, with no up-hill direction.
            let side = hit.point.y > landed.y - half_height + 2.0 * space.skin;
            let n = if side { Vec3::new(hit.normal.x, 0.0, hit.normal.z) } else { hit.normal };
            // 5. A steep top: refused when the high slide hit anything,
            // else unless a probe further up-hill is not steep.
            if n.y < data.walkable_normal {
                let uphill = -Vec3::new(n.x, 0.0, n.z).normalize_or_zero();
                if !high_hits.is_empty() || n.y < 0.001 || uphill == Vec3::ZERO {
                    return low;
                }
                let ny = n.y;
                let ahead = (0.5 * space.radius).max(2.0 * step * (1.0 - ny * ny) + 0.1);
                let target = across + uphill * ahead - lifted;
                let probe = lifted + space.cast(lifted, half_height, target).map_or(target, |h| target.normalize() * h.distance);
                let ok = space
                    .cast(probe, half_height, Vec3::NEG_Y * step)
                    .is_none_or(|h| h.normal.y >= data.walkable_normal);
                if !ok {
                    return low;
                }
            }
            landed
        }
    };
    // 6. Take the high result when it got further along the move.
    let o = Vec2::new(delta.x, delta.z);
    let along = |p: Vec3| Vec2::new(p.x - centre.x, p.z - centre.z).dot(o);
    let (l, h) = (along(low).max(0.0), along(high));
    let same_xz = Vec2::new(low.x - high.x, low.z - high.z).length() < 1e-4;
    if same_xz {
        if high.y >= low.y { high } else { low }
    } else if 1.01 * l < h {
        high
    } else {
        low
    }
}

#[allow(clippy::type_complexity)]
fn step(
    mut q: Query<(
        Entity,
        &Intent,
        &mut CaMovement,
        &mut Transform,
        &mut Velocity,
        &mut MovementState,
        Option<&CaCharacter>,
        (Option<&HeldMovePenalty>, Option<&StunPenalty>),
        Option<&mut HitKnockback>,
        Has<Zoomed>,
        Has<Dead>,
    )>,
    characters: Query<(), With<Intent>>,
    bodies: Query<&RigidBody>,
    collider_of: Query<&ColliderOf>,
    move_and_slide: MoveAndSlide,
    data: Res<MovementData>,
    clock: Res<SimClock>,
    first: Option<Res<FirstTimePredicted>>,
    mut damage: MessageWriter<Damage>,
) {
    let dt = clock.dt();
    let d = &*data;
    let s = d.meters_per_unit;
    let first = first.is_none_or(|f| f.0);
    let is_character = |e: Entity| characters.contains(e);
    // Characters and moving objects are never steps.
    let not_a_step = |e: Entity| {
        let body = collider_of.get(e).map_or(e, |c| c.body);
        characters.contains(e) || characters.contains(body) || bodies.get(body).is_ok_and(|b| !b.is_static())
    };
    for (entity, intent, mut me, mut transform, mut vel, mut state, character, (held, stun), mut knock, zoomed, dead) in &mut q {
        let me = &mut *me;
        let character = character.unwrap_or(&d.character);
        let space = Space {
            mover: &move_and_slide,
            filter: SpatialQueryFilter::from_excluded_entities([entity]).with_mask(CHARACTER_FILTER),
            scale: s,
            skin: d.skin,
            radius: d.stand_half_extents[0].max(d.stand_half_extents[2]),
            is_character: &is_character,
            not_a_step: &not_a_step,
        };
        let stand_h = d.stand_half_extents[1];
        let crouch_h = d.crouch_half_extents[1];
        if !me.initialized {
            me.initialized = true;
            me.half_height = stand_h;
            me.jump_factor = 1.0;
            me.ramp_forward = d.start_accel;
            me.ramp_strafe = d.start_accel;
            me.direction_speed = d.run_forward;
            me.stamina = character.stamina_max();
            me.slowdown = 1.0;
            me.lockout = 1.0;
        }
        let mut pos = transform.translation / s;
        pos = space.depenetrate(pos, me.half_height);
        // Spawn and respawn (spec "Falling and fall damage"): the server
        // puts the player on the floor below the spawn point, and the fall
        // is measured from there.
        if me.fall_top.is_none() || (me.was_dead && !dead) {
            if let Some(hit) = space.cast(pos, me.half_height, Vec3::NEG_Y * SPAWN_DROP) {
                pos.y -= hit.distance;
            }
            me.fall_top = Some(pos.y);
        }
        me.was_dead = dead;
        // Where it was put since (a teleport) counts for the fall too.
        if me.standing.is_none() {
            me.fall_top = me.fall_top.map(|t| t.max(pos.y));
        }
        let mut v = vel.0 / s;
        let now = clock.now;
        let spam = &d.crouch_spam;
        let c0 = 1.0 / spam.duration;

        // 3. Control flags (spec "Input to control state").
        let was = (me.forward, me.back, me.left, me.right, me.moving, me.jump);
        let keys_on = !dead;
        let raw_crouch = keys_on && intent.crouch;
        let key_press = raw_crouch && !me.raw_crouch;
        me.raw_crouch = raw_crouch;
        let r_prev = me.crouch_rate;
        // 1. Crouch.
        if raw_crouch {
            if key_press {
                me.crouch_pressed_at = now;
            }
            me.crouch = true;
        } else {
            let delay = me.crouch && now - me.crouch_pressed_at < d.crouch_release_delay - 1e-9;
            let feet = pos.y - me.half_height;
            let no_room = (me.crouch || r_prev > 0.0) && !space.fits(Vec3::new(pos.x, feet + stand_h, pos.z), stand_h);
            me.crouch = delay || no_room;
        }
        if spam.enable_lockout && me.crouch_locked && r_prev == 0.0 {
            me.crouch = false;
        }
        // 2. Run: walk key XOR always-run (on); set at rest too.
        me.run = !me.crouch && keys_on && !intent.walk;
        // 3. Jump.
        me.jump = false;
        if !me.crouch && me.on_ground && !me.in_jump {
            if keys_on && intent.jump {
                if !me.jump_used {
                    me.jump_used = true;
                    me.jump = me.half_height >= stand_h - d.jump_stance_tolerance;
                }
            } else {
                me.jump_used = false;
            }
        }
        // 4. Crouch rate not 0: no jump, no sprint.
        let mut sprint_ok = true;
        if r_prev != 0.0 {
            me.jump = false;
            sprint_ok = false;
        }
        // 5. Forward and sprint. Opposing keys both set their flags.
        let axis = intent.move_axis;
        me.forward = keys_on && (axis.y > 0.0 || intent.opposed.y);
        me.back = keys_on && (axis.y < 0.0 || intent.opposed.y);
        me.left = keys_on && (axis.x < 0.0 || intent.opposed.x);
        me.right = keys_on && (axis.x > 0.0 || intent.opposed.x);
        // The hit knock-back: no run or sprint while it runs.
        let knocked = knock.as_mut().and_then(|k| {
            if k.started.is_none() {
                k.started = Some(now);
            }
            k.running(now)
        });
        if knocked.is_some() {
            me.run = false;
        }
        let can_sprint = me.run && r_prev == 0.0 && sprint_ok;
        if me.forward {
            if intent.sprint && me.stamina.floor() >= 1.0 && can_sprint {
                me.sprint = true;
                me.stamina_state = StaminaState::Draining;
            } else if intent.sprint && can_sprint {
                me.sprint = false;
                me.stamina_state = StaminaState::Stopped;
            } else {
                me.sprint = false;
                me.stamina_state = StaminaState::Recovering;
            }
        } else {
            me.sprint = false;
            me.stamina_state = StaminaState::Recovering;
        }
        // 6. Back ends the sprint, leaving the gauge's state (forward,
        // back and sprint drain it: Quirks).
        if me.back || knocked.is_some() {
            me.sprint = false;
        }
        // 7. Moving.
        me.moving = me.forward || me.back || me.left || me.right || me.jump;
        // 8. Jump request.
        let jump_request = me.jump && !was.5 && !me.in_jump;

        // 4. Crouch rate, then the crouch-spam values.
        let c = if spam.enable_lockout && me.crouch_locked {
            0.0
        } else if spam.enable_slowdown && spam.min_slowdown_sp.abs() > 1e-6 && me.slowdown <= spam.min_slowdown_sp {
            let k = me.slowdown.clamp(0.0, spam.min_slowdown_sp) / spam.min_slowdown_sp;
            c0 / (spam.max_slowdown_rate + (1.0 - spam.max_slowdown_rate) * k)
        } else {
            c0
        };
        if dead {
            me.crouch_rate = 0.0;
            me.crouch_phase = CrouchPhase::Idle;
        } else if me.crouch && c > 0.0 {
            if me.crouch_phase != CrouchPhase::Down {
                me.crouch_phase = CrouchPhase::Down;
                me.crouch_elapsed = 0.0;
            }
            let t = me.crouch_elapsed * c;
            me.crouch_rate += crouch_shape(t, spam.tangents) * dt * c;
            me.crouch_elapsed += dt;
            if me.crouch_rate >= 1.0 - 1e-6 {
                me.crouch_rate = 1.0;
            }
        } else if me.crouch_rate > 0.0 {
            if me.crouch_phase != CrouchPhase::Up {
                me.crouch_phase = CrouchPhase::Up;
                me.crouch_elapsed = 0.0;
            }
            let c_up = if c == 0.0 { c0 } else { c };
            let t = me.crouch_elapsed * c_up;
            me.crouch_rate -= crouch_shape(t, spam.tangents) * dt * c_up;
            me.crouch_elapsed += dt;
            if me.crouch_rate <= 1e-6 {
                me.crouch_rate = 0.0;
            }
        } else {
            me.crouch_phase = CrouchPhase::Idle;
            me.crouch_elapsed = 0.0;
        }
        let r = me.crouch_rate;
        crouch_spam(me, spam, r - r_prev, key_press, dt);

        // 5. Resize the hull (once: the spec's second resize changes
        // nothing). Shrinking keeps the feet when standing, else the
        // centre; growing finds its room below and above.
        let new_h = stand_h - r * (stand_h - crouch_h);
        if new_h < me.half_height {
            if me.standing.is_some() {
                pos.y -= me.half_height - new_h;
            }
            me.half_height = new_h;
        } else if new_h > me.half_height {
            // Not fitting: forced around the old centre.
            pos = space.grow(pos, me.half_height, new_h).0;
            me.half_height = new_h;
        }

        // 6. The last direction speed.
        if me.forward {
            me.direction_speed = d.run_forward;
        } else if me.back {
            me.direction_speed = d.run_back;
        } else if me.left || me.right {
            me.direction_speed = d.run_strafe;
        }

        // 7c. Ground from last tick's move; landing.
        if me.standing.is_some() && me.in_jump {
            me.in_jump = false;
        }
        me.on_ground = !me.in_jump
            && me.standing.is_some_and(|g| !g.character && g.normal.y >= d.walkable_normal);

        // 7d. Speed limit and clamp.
        let crouching = me.crouch || r == 1.0;
        let k = me.direction_speed;
        let fig = &character.figure;
        let v0 = if me.sprint && !crouching && !zoomed {
            k * fig.dash_speed * (1.0 + character.gear_sum(|g| g.dash_speed))
        } else if me.run && !crouching && !zoomed {
            k
        } else {
            d.walk
        };
        let v0 = v0.max(d.crouch);
        let v1 = v0 + (d.crouch - v0) * r;
        let held = held.copied().unwrap_or_default();
        let stun = stun.filter(|s| now < s.until).map_or(0.0, |s| s.penalty);
        let penalty = (held.penalty * (1.0 + held.mod_factor)
            + character.gear_sum(|g| g.move_penalty)
            + fig.variable_weight
            - fig.variable_speed
            + stun)
            .clamp(-1.0, 1.0);
        let limit = v1 * (1.0 - penalty);
        me.speed_limit = limit;
        let h = Vec2::new(v.x, v.z);
        if h.length() > limit {
            let h = h.normalize_or_zero() * limit;
            v.x = h.x;
            v.z = h.y;
        }

        // 7e. Acceleration.
        let yaw = intent.yaw_rotation();
        let fwd = (yaw * Vec3::NEG_Z).with_y(0.0).normalize_or_zero();
        let right = (yaw * Vec3::X).with_y(0.0).normalize_or_zero();
        let mut base = d.base_accel;
        if me.crouch {
            base *= d.crouch_accel_factor;
        }
        if me.in_jump {
            base *= d.in_air_accel_factor;
        }
        let start = me.moving && !was.4;
        // The jump-request tick and crouching skip the ramp: no start
        // rescale, the remembered ramp values kept.
        let bypass = jump_request || me.crouch;
        let (a_f, a_r) = if bypass {
            (base, base)
        } else {
            let inc = d.accel_ramp * dt;
            if start {
                me.ramp_forward = d.start_accel;
                me.ramp_strafe = d.start_accel;
            }
            let ramp = |l: &mut f32| {
                let n = *l + inc;
                *l = if n < d.max_accel { n } else { d.max_accel };
                *l
            };
            // A reversal: one key newly held while the other was (both held
            // on, the ramp goes on).
            let a_f = if (me.forward && was.1 && !was.0) || (me.back && was.0 && !was.1) {
                me.ramp_forward = d.start_accel;
                d.start_accel
            } else if me.forward || me.back {
                if start { me.ramp_forward } else { ramp(&mut me.ramp_forward) }
            } else {
                me.ramp_forward = d.start_accel;
                0.0
            };
            let a_r = if (me.right && was.2 && !was.3) || (me.left && was.3 && !was.2) {
                me.ramp_strafe = d.start_accel;
                d.start_accel
            } else if me.left || me.right {
                ramp(&mut me.ramp_strafe)
            } else {
                me.ramp_strafe = d.start_accel;
                0.0
            };
            (a_f, a_r)
        };
        let mut a = Vec3::ZERO;
        if me.forward {
            a += fwd * a_f;
        }
        if me.back {
            a -= fwd * a_f;
        }
        if me.left {
            a = (a - right * a_r).clamp_length_max(d.max_accel);
        }
        if me.right {
            a = (a + right * a_r).clamp_length_max(d.max_accel);
        }
        if start && !bypass && a != Vec3::ZERO {
            a = a.normalize() * d.start_accel;
        }
        if me.jump_factor < 1.0 {
            a *= me.jump_factor;
        }
        // The knock-back replaces it: K^2 (1 - t) along the hit's
        // horizontal direction; crouched, a quarter with a movement key,
        // none (and no velocity) without one.
        if let (Some(t), Some(k)) = (knocked, knock.as_deref()) {
            let dir = k.dir.with_y(0.0).normalize_or_zero();
            a = dir * k.strength * k.strength * (1.0 - t);
            if raw_crouch {
                if me.forward || me.back || me.left || me.right {
                    a *= 0.25;
                } else {
                    a = Vec3::ZERO;
                    v = Vec3::new(0.0, v.y, 0.0);
                }
            }
        }

        // 7f. Jump, or the jump factor's recovery.
        if jump_request && !dead && me.on_ground {
            let bonus = fig.variable_jump + character.gear_sum(|g| g.jump);
            v.y = d.jump * (1.0 + bonus) * me.jump_factor;
            me.in_jump = true;
            me.on_ground = false;
            me.jump_factor = if me.jump_factor < d.jump_factor_after_jump {
                me.jump_factor - 0.05 * dt
            } else {
                d.jump_factor_after_jump
            };
        } else if me.jump_factor < 1.0 {
            me.jump_factor += d.jump_factor_recovery * dt;
        } else {
            me.jump_factor = 1.0;
        }
        // 7g. Dying: no velocity, no acceleration.
        if dead {
            v = Vec3::ZERO;
            a = Vec3::ZERO;
        }
        me.accel = a;

        // 8. Braking with no movement keys (none under a knock-back).
        if !me.moving && me.on_ground && !me.in_jump && knocked.is_none() {
            let h = Vec2::new(v.x, v.z);
            let speed = h.length();
            if speed > d.stop_snap_speed {
                let slower = speed - dt * k / d.slide_to_stop_time;
                if slower <= 0.0 {
                    v = Vec3::ZERO;
                } else {
                    let h = h * (slower / speed);
                    v.x = h.x;
                    v.z = h.y;
                }
            } else {
                v = Vec3::ZERO;
            }
        }

        // 11. Integrate, move, stand, fix.
        let g = Vec3::Y * d.gravity;
        if v.length_squared() < d.rest_threshold {
            v = Vec3::ZERO;
        }
        if a.length_squared() < d.rest_threshold {
            a = Vec3::ZERO;
        }
        let mut friction = false;
        let mut floor = None;
        let mut steep = None;
        let mut rest = false;
        match me.standing {
            Some(_) if v == Vec3::ZERO && a == Vec3::ZERO => rest = true,
            Some(ground) if ground.normal.y < d.walkable_normal => {
                let n = ground.normal;
                let full = a + g;
                let mut along = full - n * full.dot(n);
                let down = (g - n * g.dot(n)).normalize_or_zero();
                let up_slope = along.dot(down);
                if up_slope < 0.0 {
                    along -= down * up_slope;
                }
                a = along.normalize_or_zero() * full.length();
                steep = Some(n);
            }
            Some(ground) => {
                let n = ground.normal;
                if (v + g * dt).dot(n) > 0.01 {
                    a += g;
                } else {
                    if a.dot(n) > 0.01 {
                        let len = a.length();
                        a = (a - n * (n.dot(a) + 1.0)).normalize_or_zero() * len;
                    }
                    friction = true;
                    floor = Some(n);
                }
            }
            None => a += g,
        }
        let (v_new, dx) = if rest {
            (Vec3::ZERO, Vec3::ZERO)
        } else if friction {
            let kf = d.friction;
            let e = (-kf * dt).exp();
            (
                a / kf + (v - a / kf) * e,
                a / kf * dt + (v * kf - a) * (1.0 - e) / (kf * kf),
            )
        } else {
            (v + a * dt, v * dt + a * (0.25 * dt * dt))
        };
        v = v_new;
        let before = pos;
        if dx != Vec3::ZERO {
            pos = move_hull(&space, d, pos, me.half_height, dx, floor, steep);
        }
        let probe = d.standing_probe * me.half_height;
        me.standing = space.ground(d, pos, me.half_height, probe, me.standing);
        let moved = pos - before;
        if dx.x.abs() > 0.01 && dx.z.abs() > 0.01 && me.standing.is_none() && moved == Vec3::ZERO {
            v.x = 0.0;
            v.z = 0.0;
        } else if dx.y.abs() > 0.01 && moved.y.abs() < 0.01 {
            v.y = 0.0;
        }

        // Fall damage (the server's, from positions).
        if me.standing.is_some() {
            if let Some(top) = me.fall_top {
                let drop = top - pos.y;
                if drop > 0.0
                    && let Some(hp) = fall_damage(drop, d)
                    && first
                    && !dead
                {
                    damage.write(Damage {
                        target: entity,
                        attacker: Some(entity),
                        amount: hp / d.health_points,
                        point: (pos - Vec3::Y * me.half_height) * s,
                        dir: Vec3::Y,
                        hitgroup: Hitgroup::Generic,
                        kind: DamageKind::Crush,
                        weapon: None,
                        force: Vec3::ZERO,
                    });
                }
            }
            me.fall_top = Some(pos.y);
        } else {
            me.fall_top = Some(me.fall_top.map_or(pos.y, |t| t.max(pos.y)));
        }

        // 13. Record for "still".
        me.history.push((now, Vec2::new(pos.x, pos.z)));
        let keep = now - spam.no_movement_time_limit as f64 - 0.1;
        me.history.retain(|(t, _)| *t >= keep);

        // Stamina (per tick here; the spec's per frame).
        let max = character.stamina_max();
        let ms = dt * 1000.0;
        match me.stamina_state {
            StaminaState::Draining => me.stamina = (me.stamina - fig.dash_gauge * ms / 1000.0).max(0.0),
            StaminaState::Recovering => me.stamina = (me.stamina + fig.gauge_recovery * ms / 1000.0).min(max),
            StaminaState::Stopped => me.stamina = 0.0,
        }

        transform.translation = pos * s;
        vel.0 = v * s;
        let r_xz = space.radius * s;
        let half = me.half_height * s;
        *state = MovementState {
            on_ground: !me.in_jump
                && me.standing.is_some_and(|g| !g.character && g.normal.y >= d.walkable_normal),
            crouching: r > 0.0,
            sprinting: me.sprint,
            eye_offset: Vec3::Y * eye_above_centre(r, d) * s,
            hull_min: Vec3::new(-r_xz, -half, -r_xz),
            hull_max: Vec3::new(r_xz, half, r_xz),
            ..default()
        };
    }
}

/// Spec "Crouch-spam penalty": update the slowdown and lock-out values
/// after this tick's crouch rate change `dr`.
fn crouch_spam(me: &mut CaMovement, spam: &CrouchSpamData, dr: f32, key_press: bool, dt: f32) {
    let moving_change = dr.abs() > 1e-6;
    let crouch_start = moving_change && dr > 0.0 && !me.spam_rising;
    if moving_change {
        me.spam_rising = dr > 0.0;
    }
    if crouch_start {
        me.spam_elapsed = 0.0;
    } else {
        me.spam_elapsed += dt;
    }
    // "Still": horizontal distance over the last 0.2 s, up to last tick.
    let still = match me.history.last() {
        None => true,
        Some(&(newest, _)) => {
            let window: Vec<Vec2> = me
                .history
                .iter()
                .filter(|(t, _)| newest - *t <= spam.no_movement_time_limit as f64 + 1e-6)
                .map(|(_, p)| *p)
                .collect();
            let moved: f32 = window.windows(2).map(|w| (w[1] - w[0]).length()).sum();
            moved < spam.no_movement_deviation_limit
        }
    };
    let ground = me.on_ground;
    let charge = (!still || !spam.decrease_exception_no_movement) && (ground || spam.decrease_during_jumping_crouch);
    let charged = if spam.decrease_by_keystroke { key_press } else { crouch_start };

    if spam.enable_slowdown {
        if !crouch_start && !key_press && !moving_change {
            let t = me.spam_elapsed / spam.slowdown_increase_duration;
            me.slowdown += crouch_shape(t, spam.slowdown_increase_tangents) * dt / spam.slowdown_increase_duration;
        } else if charge && charged {
            me.slowdown -= spam.slowdown_decrease_amount;
        }
        me.slowdown = me.slowdown.clamp(0.0, 1.0);
    } else {
        me.slowdown = 1.0;
    }

    if spam.enable_lockout {
        let was_locked = me.crouch_locked;
        if !crouch_start && !key_press && ground {
            let t = me.spam_elapsed / spam.lockout_increase_duration;
            me.lockout += crouch_shape(t, spam.lockout_increase_tangents) * dt / spam.lockout_increase_duration;
            if me.lockout >= 1.0 {
                me.crouch_locked = false;
            }
        } else if charge && charged {
            me.lockout -= if was_locked {
                spam.lockout_decrease_amount_in_penalty
            } else {
                spam.lockout_decrease_amount
            };
            if me.lockout <= 0.0 {
                me.crouch_locked = true;
            }
        }
        me.lockout = me.lockout.clamp(0.0, 1.0);
    } else {
        me.lockout = 1.0;
        me.crouch_locked = false;
    }
}

/// The entity's collider follows the hull (an upright cylinder); a player
/// that leaves this movement gets the standard capsule back.
fn sync_collider(
    mut q: Query<(&CaMovement, &mut Collider), Changed<CaMovement>>,
    mut removed: RemovedComponents<CaMovement>,
    mut others: Query<&mut Collider, Without<CaMovement>>,
    data: Res<MovementData>,
) {
    let s = data.meters_per_unit;
    let radius = data.stand_half_extents[0].max(data.stand_half_extents[2]) * s;
    for e in removed.read() {
        if let Ok(mut c) = others.get_mut(e) {
            *c = Collider::capsule(CAPSULE_RADIUS, CAPSULE_HEIGHT - 2.0 * CAPSULE_RADIUS);
        }
    }
    for (me, mut collider) in &mut q {
        if me.half_height <= 0.0 {
            continue;
        }
        let half = me.half_height * s;
        let wanted = Collider::cylinder(radius, 2.0 * half);
        let same = collider
            .shape_scaled()
            .as_cylinder()
            .is_some_and(|c| (c.half_height - half).abs() < 1e-6 && (c.radius - radius).abs() < 1e-6);
        if !same {
            *collider = wanted;
        }
    }
}
