//! Combat Arms' firing (specs/combat_arms/weapon_firing.md): how its guns
//! shoot, on Chimera's weapon frame (`weapon`) with the rest as
//! `combat_arms:*` weapon hooks and the systems around the frame in
//! `core::Predict::Weapons`, as CS:S's accuracy and recoil are.
//!
//! What the frame does from the data (data/weapons/*.toml, docs/weapon-data.md):
//! the trigger (`Trigger`: the fire cycle is the fire animation's length,
//! 105 ms on the M4A1, with `accumulate` timing for the spec's exact-time
//! mode and `set` for the frame-quantised one; `automatic = false` is
//! `RapidFireCount` 1, `per_press` a higher count), bursts (`[alt] kind =
//! "burst"`: three rounds a cycle apart that finish on their own, one burst
//! per pull, the lockout as the next pull's `refire`, the fire-mode toggle's
//! lockout as `step_blocks_primary`), the magazine (clip, the reserve as
//! total ammo minus the clip, partial reloads that lose nothing, refusals,
//! switching away abandoning a reload, `auto_reload`: an empty clip reloads
//! once the last fire cycle ends, trigger held or not) and the round's
//! `Fired` event (`Hitscan` without `DamageEffect`: the frame traces
//! nothing, this module delivers).
//!
//! What this module adds, by hook:
//!
//! - `combat_arms:damage` (`CaDamage`): delivery and damage. Each pellet
//!   draws its own uniform square offset (`square_dir`) of the cone the
//!   spread meters give before this tick's sub-steps, then walks the ray
//!   (`deliver`): characters by their hit spheres (`HitSpheres`: the box
//!   widened 2x on X and Z, hit only when the ray passes a sphere; the
//!   sphere with the smallest |p|^2 - r^2 wins, `pick_sphere`), walls by
//!   their surface (`CaSurfaces`, data/surfaces.toml: passed when no
//!   thicker than its limit, the range and damage dampened and the
//!   direction re-perturbed), at most 3 segments. Damage
//!   (`base_damage`, `armour_and_zone`): `InstDamage` x a uniform roll of
//!   +-`DamageRFacter` x 1.3 from behind (`is_behind`, the first,
//!   "client-reported" victim only) x the stepped range band
//!   (`band_scale`; beyond the last band: 1) x penetration dampening, then
//!   armour (`CaArmor`: gear protection by location, armour points) and
//!   the sphere's zone factor; the victim dies at 1 HP or less. The server
//!   spacing `fireDelay` drops a round that comes too soon after the last
//!   (spent all the same).
//! - `combat_arms:spread` (`CaSpread`): the firing and movement meters on
//!   fixed 1/80 s sub-steps (an exact accumulator keeping the remainder,
//!   Q2's recommendation; `SpreadOptions::quantised` discards it as the
//!   game does), the firing cap by stance, the movement target by run,
//!   jump and ladder, the turn term (Q11's snap), the airborne rise (Q12),
//!   the cone `n` in thousandths (`cone`), zoomed and burst values.
//! - `combat_arms:recoil` (`CaRecoil`): the view kick on the owner's
//!   `ViewPunch` (its pitch twice the accumulator, Q4), the side wag with
//!   its 50 % flips, and the return each tick before the frame, at the
//!   waiting rate for `JitterDecDelay` after a kick.
//! - `combat_arms:handling` (`CaHandling`): the held weapon's
//!   `MovePenalty`, set on its carrier as the movement's `HeldMovePenalty`
//!   when it is drawn (in the predicted tick, `core::Predict::Select`).
//!
//! A network client predicts `CaSpread` and `CaRecoil` with the weapon
//! (`weapon::NetPart`: the server's state goes with its player's;
//! `game::net`).
//!
//! The data's `combat_arms:inventory` table (sprint-out time) is
//! `inventory`'s hook.
//!
//! Per tick (the spec's "Per-tick order"): before the frame, the recoil
//! returns and the cone is set from the meters; the frame fires; after
//! it, each round is delivered with that cone, then the spread sub-steps
//! run (the first one a firing one when a round left), then each round
//! kicks the view. Every character gets the default armour
//! (`CaArmor::default`: 100 points, the default body gear) when it first
//! has health, and is hit through `HitSpheres` (the male skeleton when it
//! has none).
//!
//! Units: the data's (world units, hit points; `units` in each document);
//! the hit spheres are scaled to the character's box (`SKELETON_HEIGHT`).
//!
//! Known shortcuts (the spec's most literal reading where it is unclear,
//! listed for its open questions):
//! - 1 world unit is ASSUMED 1 cm (the data's `units`; movement.md Q3).
//! - The placeholder skeleton (`MALE`): sphere radii, locations and
//!   factors are the spec's, their places ours (Q9: no models yet), in a
//!   standing pose scaled to the character's box, crouched or not.
//! - The spec's client and server halves aren't split by process: the
//!   "client" test is the ray's first segment, the "server" walk the rest,
//!   both run where the round is delivered. In a network game the shooter's
//!   client predicts the round (its meters, kick and events; no damage) and
//!   the server walks it again against where the shooter saw the others
//!   (lag compensation, `weapon::lagcomp`, their boxes and looks then);
//!   nothing the client found is sent (no client-reported victim).
//! - Firing recovery waits while the time since the last shot is at most
//!   `DecDelay` (test case 16: 95 ms still waits), not "less than".
//! - Airborne from a jump: left the ground with the jump key held, until
//!   landing; a fall without a jump decays as on the ground. The airborne
//!   rise (Q12) grows by the increase step of the gap M - A.
//! - The run flag is "not walking" (`Intent::walk`), the moving flag any
//!   movement key; the stance's max speed for mu is movement.md's run,
//!   walk, crouch or ladder speed (no weapon `MovePenalty`).
//! - The turn term counts the view's change including its kick (recoil
//!   moves the real view), all of a tick's turn in its first sub-step.
//! - After a character, the ray goes on past it (no dampening) and may hurt
//!   a second one with the first one's banded damage banded again (the
//!   spec's edge case); hits behind a wall use the real distance (Q13's
//!   recommendation). World surfaces have no names yet (no Combat Arms
//!   maps): only colliders with a `PropSurface` named in the table pass.
//! - `fireDelay` counts from the previous round received, dropped or not.
//! - Armour: bullet protection only, groin counted as torso; the gear
//!   (which pieces a character wears, Q7) is the default body gear for
//!   everyone. Figure modifiers are 0 (both figure types).
//! - Not done: the back push (`BaseCamBackJitter`) and the view-model
//!   jiggle (presentation), continuous fire, prefire, per-round reloads,
//!   mod and perk bonuses (Q8: identity in a plain match), the fire-mode
//!   key (the toggle is attack2 until the binds come), the out-of-ammo
//!   voice line.
//!
//! Slots and switching are `inventory`'s, thrown weapons `grenades`',
//! attachments `attachments`'.

use std::sync::OnceLock;

use avian3d::prelude::*;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    core::{
        Damage, DamageKind, DamageRules, FirstTimePredicted, Health, Hitgroup, Intent, MovementState, NetRole, Predict,
        PropSurface, SOLID_LAYERS, SimClock, Velocity,
    },
    weapon::{
        AltModes, Burst, CarriedAppExt, Hitscan, Inventory, NetPart, SpreadShape, ViewPunch, Weapon, WeaponEvent,
        WeaponEventKind, WeaponFrame, Zoom, aim_of,
        data::{WeaponData, WeaponDataAppExt},
        lagcomp::{self, HitHistory, LagCompSettings, LagCompStats, ViewTick},
        random::{Rng, shared_random},
    },
};

/// The M4A1 (data/weapons/m4a1.toml): the rifle players start with.
pub const RIFLE: &str = "combat_arms:m4a1";

/// Hook names (docs/weapon-data.md, "Hooks").
pub const DAMAGE: &str = "combat_arms:damage";
pub const SPREAD: &str = "combat_arms:spread";
pub const RECOIL: &str = "combat_arms:recoil";
pub const HANDLING: &str = "combat_arms:handling";

/// `combat_arms:handling`: what holding the weapon does to its carrier.
#[derive(Component, Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct CaHandling {
    /// `MovePenalty`: the fraction of speed lost while held (the
    /// movement's `HeldMovePenalty`).
    pub move_penalty: f32,
}

/// The weapon whose handling a character's `HeldMovePenalty` was last set
/// from.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
struct HandlingOf(Option<Entity>);

/// On drawing a weapon, its `MovePenalty` becomes the carrier's
/// `HeldMovePenalty` (none without `CaHandling`); set once per draw, so a
/// test or a mod may change it while the weapon is held.
fn apply_handling(
    owners: Query<(Entity, &Inventory, Option<&HandlingOf>)>,
    handling: Query<&CaHandling>,
    mut commands: Commands,
) {
    for (owner, inv, last) in &owners {
        if last.map(|l| l.0) == Some(inv.active) {
            continue;
        }
        let mut e = commands.entity(owner);
        e.insert(HandlingOf(inv.active));
        match inv.active.and_then(|w| handling.get(w).ok()) {
            Some(h) => {
                e.insert(super::movement::HeldMovePenalty {
                    penalty: h.move_penalty,
                    mod_factor: 0.0,
                });
            }
            None => {
                e.remove::<super::movement::HeldMovePenalty>();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Constants (the spec's "Constants": fixed in code)

/// Spread meters advance in steps of this, s.
pub const SPREAD_SUBSTEP: f64 = 1.0 / 80.0;
/// Most sub-steps in one tick.
pub const SPREAD_SUBSTEP_MAX: u32 = 100;
/// Damage multiplier from behind (client-reported hits only).
pub const BEHIND_BONUS: f32 = 1.3;
/// A character dies at this many hit points or fewer after a hit.
pub const DEATH_THRESHOLD: f32 = 1.0;
/// Horizontal widening of a character's box in the first ray test.
pub const HITBOX_WIDEN: f32 = 2.0;
/// Most ray segments a round walks.
pub const SERVER_TRACE_STEPS: u32 = 3;
/// `DamageRFacter` used when the data's is outside [0, 1].
pub const DAMAGE_ROLL_INVALID: f32 = 0.2;
/// The placeholder skeleton's standing height, world units (movement.md's
/// standing half-height 50): spheres scale by the character's box height
/// over this.
pub const SKELETON_HEIGHT: f32 = 100.0;
/// Stance speeds for the movement fraction mu (movement.md, u/s).
pub const RUN_SPEED: f32 = 285.0;
pub const WALK_SPEED: f32 = 70.0;
pub const CROUCH_SPEED: f32 = 50.0;
pub const LADDER_SPEED: f32 = 200.0;

/// The global spread scales (the weapons file's `[Perturb]` section, as the
/// client uses them after swapping firing and movement, Q1) and the turn
/// constants.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct SpreadScales {
    pub fire_inc: f32,
    pub fire_dec: f32,
    pub move_inc: f32,
    pub move_dec: f32,
    pub turn_effect: f32,
    pub fast_turn_rate: f32,
}

impl Default for SpreadScales {
    fn default() -> Self {
        Self {
            fire_inc: 3.0,
            fire_dec: 3.0,
            move_inc: 3.0,
            move_dec: 9.0,
            turn_effect: 3.0,
            fast_turn_rate: 2.3,
        }
    }
}

/// How the spread sub-steps keep time.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct SpreadOptions {
    /// Discard the remainder each tick, as the game does at its frame rate
    /// (Q2); off: an exact 80 Hz clock.
    pub quantised: bool,
}

// ---------------------------------------------------------------------------
// Hook parameters (data)

fn one() -> f32 {
    1.0
}

/// `combat_arms:damage`: damage per ray and the server's spacing.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageParams {
    /// `InstDamage` (ammo), hit points (the document's damage units).
    pub damage: f32,
    /// `InstDamageType` (ammo): 2 is bullet (the only one armour knows yet).
    #[serde(default = "bullet_type")]
    pub damage_type: i32,
    /// `DamageRFacter` (weapon): each hit x U(1 - roll, 1 + roll).
    #[serde(default = "default_roll")]
    pub roll: f32,
    /// `Effectrange0..2` (weapon): band edges, length units (truncated to
    /// whole units).
    #[serde(default)]
    pub effect_range: Vec<f32>,
    /// `Rangedamage0..2` (ammo): each band's multiplier.
    #[serde(default)]
    pub range_damage: Vec<f32>,
    /// `fireDelay` (weapon), ms.
    #[serde(default = "default_fire_delay")]
    pub fire_delay: f32,
}

fn bullet_type() -> i32 {
    2
}
fn default_roll() -> f32 {
    0.1
}
fn default_fire_delay() -> f32 {
    30.0
}

/// A cone's limits (spread units, thousandths) and the meter's speeds.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConeParams {
    /// `MinPerturb`, `MaxPerturb`.
    pub min: i32,
    pub max: i32,
    /// `PerturbIncSpeed`, `PerturbDecSpeed`.
    pub inc: f32,
    pub dec: f32,
}

/// The ammo's state values (`Fire*Perturb` caps, movement targets).
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct StateParams {
    pub fire: f32,
    pub fire_move: f32,
    pub fire_duck: f32,
    pub fire_move_duck: f32,
    pub walk: f32,
    pub run: f32,
    pub jump: f32,
}

/// `combat_arms:spread`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpreadParams {
    pub cone: ConeParams,
    /// Zoomed values (`Zoomed*`), used while zoomed where non-zero.
    #[serde(default)]
    pub zoomed_cone: Option<ConeParams>,
    /// Burst values (`TripleShot*Perturb*`), used in burst mode.
    #[serde(default)]
    pub burst_cone: Option<ConeParams>,
    /// `DynamicPerturbFactor`.
    #[serde(default = "one")]
    pub dynamic_factor: f32,
    pub state: StateParams,
    /// The ammo's `Zoomed*Perturb` values, used while zoomed where non-zero.
    #[serde(default)]
    pub zoomed_state: StateParams,
    /// `DecDelay` (ammo), ms.
    #[serde(default = "default_dec_delay")]
    pub dec_delay: f32,
}

fn default_dec_delay() -> f32 {
    150.0
}

/// `combat_arms:recoil` (degrees but the two random thresholds, radians;
/// delays ms; rates radians/s). Defaults: attribute_data.md's.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct RecoilParams {
    /// `BaseCamRecoilPitch`.
    pub pitch: f32,
    /// `BaseCamRecoilAccelPitch`, per radian accumulated.
    pub accel_pitch: f32,
    /// `MaxCamRecoilPitch`.
    pub max_pitch: f32,
    /// `MaxRandomPitch`, radians.
    pub random_pitch: f32,
    /// `BaseCamRecoilWagR`, `BaseCamRecoilWagL`.
    pub wag_r: f32,
    pub wag_l: f32,
    /// `MaxCamRecoilWag`.
    pub max_wag: f32,
    /// `MaxRandomwag`, radians.
    pub random_wag: f32,
    /// `BaseCamBackJitter`, `BackJitterRecover` (not used yet).
    pub back_jitter: f32,
    pub back_jitter_recover: f32,
    /// `TripleShotBaseCamRecoilPitch`, `TripleShotMaxCamRecoilPitch`: in
    /// burst mode when both are non-zero.
    pub burst_pitch: f32,
    pub burst_max_pitch: f32,
    /// `JitterDecDelay` (ammo), ms.
    pub jitter_dec_delay: f32,
    /// `JitterWaitRecover`, `JitterRecover` (ammo).
    pub jitter_wait_recover: f32,
    pub jitter_recover: f32,
}

impl Default for RecoilParams {
    fn default() -> Self {
        Self {
            pitch: 1.2,
            accel_pitch: 0.0,
            max_pitch: 10.0,
            random_pitch: 0.032,
            wag_r: 1.0,
            wag_l: -1.0,
            max_wag: 30.0,
            random_wag: 0.015,
            back_jitter: 0.5,
            back_jitter_recover: 3.0,
            burst_pitch: 0.0,
            burst_max_pitch: 0.0,
            jitter_dec_delay: 600.0,
            jitter_wait_recover: 0.1,
            jitter_recover: 0.3,
        }
    }
}

// ---------------------------------------------------------------------------
// Parts

/// A weapon's damage (`combat_arms:damage`), in engine units.
#[derive(Component, Clone, Debug)]
pub struct CaDamage {
    /// Per ray, health (1.0 = 100 HP).
    pub damage: f32,
    pub damage_type: i32,
    /// The random roll's half-width (already 0.2 when the data's was
    /// outside [0, 1]).
    pub roll: f32,
    /// (upper edge in metres, multiplier), three bands; none: no banding.
    pub bands: Option<[(f32, f32); 3]>,
    /// The server's spacing, s.
    pub fire_delay: f64,
    /// Metres per the data's length unit.
    pub unit: f32,
    /// When the last round arrived (for `fire_delay`).
    pub last_round: Option<f64>,
}

impl CaDamage {
    pub fn new(p: &DamageParams, unit: f32, health_per_point: f32) -> Self {
        let bands = (p.effect_range.len() >= 3 && p.range_damage.len() >= 3).then(|| {
            std::array::from_fn(|i| (p.effect_range[i].trunc() * unit, p.range_damage[i]))
        });
        Self {
            damage: p.damage * health_per_point,
            damage_type: p.damage_type,
            roll: if (0.0..=1.0).contains(&p.roll) { p.roll } else { DAMAGE_ROLL_INVALID },
            bands,
            fire_delay: p.fire_delay.max(0.0) as f64 / 1000.0,
            unit,
            last_round: None,
        }
    }
}

/// Which values a weapon's spread and recoil use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireMode {
    Normal,
    Zoomed,
    Burst,
}

/// The mode of a weapon from its attack2 mode (`AltModes`): burst when its
/// `Burst` is on, zoomed in a scope's modes.
pub fn fire_mode(mode: u8, burst: Option<&Burst>, zoom: Option<&Zoom>) -> FireMode {
    match (burst, zoom) {
        (Some(b), _) if b.mode == mode && b.count > 0 => FireMode::Burst,
        (_, Some(_)) if mode > 0 => FireMode::Zoomed,
        _ => FireMode::Normal,
    }
}

/// A weapon's spread (`combat_arms:spread`): the data and the two meters.
#[derive(Component, Clone, Debug)]
pub struct CaSpread {
    pub params: SpreadParams,
    /// Metres per the data's length unit (for speeds).
    pub unit: f32,
    /// The firing meter F and the movement meter M.
    pub fire: f32,
    pub movement: f32,
    /// When the last shot stamped the firing meter.
    pub last_shot: Option<f64>,
    /// Time not yet stepped, s.
    pub clock: f64,
    /// The view (pitch, yaw) at the last sub-step.
    pub prev_view: Option<Vec2>,
    /// Airborne after a jump (until landing).
    pub jumped: bool,
    pub was_on_ground: bool,
}

impl CaSpread {
    pub fn new(params: SpreadParams, unit: f32) -> Self {
        Self {
            params,
            unit,
            fire: 0.0,
            movement: 0.0,
            last_shot: None,
            clock: 0.0,
            prev_view: None,
            jumped: false,
            was_on_ground: true,
        }
    }

    /// The cone limits and speeds in `mode` (zoomed ones where non-zero).
    pub fn cone_params(&self, mode: FireMode) -> ConeParams {
        let base = self.params.cone;
        match mode {
            FireMode::Normal => base,
            FireMode::Burst => self.params.burst_cone.unwrap_or(base),
            FireMode::Zoomed => match self.params.zoomed_cone {
                Some(z) => ConeParams {
                    min: if z.min != 0 { z.min } else { base.min },
                    max: if z.max != 0 { z.max } else { base.max },
                    inc: if z.inc != 0.0 { z.inc } else { base.inc },
                    dec: if z.dec != 0.0 { z.dec } else { base.dec },
                },
                None => base,
            },
        }
    }

    /// The state values in `mode` (zoomed ones where non-zero).
    pub fn states(&self, mode: FireMode) -> StateParams {
        let (b, z) = (self.params.state, self.params.zoomed_state);
        if mode != FireMode::Zoomed {
            return b;
        }
        let pick = |z: f32, b: f32| if z != 0.0 { z } else { b };
        StateParams {
            fire: pick(z.fire, b.fire),
            fire_move: pick(z.fire_move, b.fire_move),
            fire_duck: pick(z.fire_duck, b.fire_duck),
            fire_move_duck: pick(z.fire_move_duck, b.fire_move_duck),
            walk: pick(z.walk, b.walk),
            run: pick(z.run, b.run),
            jump: pick(z.jump, b.jump),
        }
    }

    /// The spread fraction p = max(F, M) (0..1) a shot fired now uses: the
    /// HUD's r.
    pub fn fraction(&self) -> f32 {
        self.fire.max(self.movement)
    }

    /// The cone `n` (thousandths) a shot fired now gets in `mode`, from
    /// the meters as they stand: n_min + floor((n_max - n_min) p) g.
    pub fn cone(&self, mode: FireMode) -> f32 {
        let c = self.cone_params(mode);
        let g = self.params.dynamic_factor;
        let max = (c.max as f32 * g).trunc();
        let min = (c.min as f32 * g).trunc().min(max);
        if max <= 0.0 {
            return 0.0;
        }
        let p = self.fraction() as f64;
        min + (((max - min) as f64 * p).floor() as f32) * g
    }

    /// One firing-meter sub-step at `now`: a shot grows it toward `cap`
    /// and stamps the time; otherwise it waits `DecDelay` after the stamp,
    /// then recovers.
    pub fn fire_substep(&mut self, scales: &SpreadScales, cone: ConeParams, fired: bool, cap: f32, now: f64) {
        let dt = SPREAD_SUBSTEP as f32;
        if fired {
            self.fire = cap.min(self.fire + dt * scales.fire_inc * cone.inc);
            self.last_shot = Some(now);
        } else if self
            .last_shot
            .is_some_and(|t| now - t <= self.params.dec_delay as f64 / 1000.0 + 1e-9)
        {
        } else if self.fire > 0.0 {
            self.fire = (self.fire - dt * scales.fire_dec * cone.dec).max(0.0);
        }
    }

    /// One movement-meter sub-step toward `target` (S), with `mu_s` (mu S)
    /// and the turn term `rho`; `jumping`: airborne after a jump.
    #[allow(clippy::too_many_arguments)]
    pub fn move_substep(
        &mut self,
        scales: &SpreadScales,
        cone: ConeParams,
        target: f32,
        mu_s: f32,
        rho: f32,
        jumping: bool,
        jump_cap: f32,
    ) {
        let dt = SPREAD_SUBSTEP as f32;
        let a = rho.max(mu_s);
        let m = self.movement;
        if a > m {
            let grown = m + (dt * scales.move_inc).min(a - m) * cone.inc;
            self.movement = grown.min(target).min(mu_s);
        } else if a < m {
            let next = if jumping {
                // Q12: airborne after a jump it rises instead.
                m + (dt * scales.move_inc).min(m - a) * cone.inc
            } else {
                m - (dt * scales.move_dec).min(m - a) * cone.dec
            };
            self.movement = next.min(jump_cap).max(0.0);
        }
    }

    /// The turn term rho for a view change of (pitch, yaw) radians.
    pub fn turn(scales: &SpreadScales, d_pitch: f32, d_yaw: f32) -> f32 {
        let dt = SPREAD_SUBSTEP as f32;
        (scales.turn_effect * (d_pitch.abs() + d_yaw.abs()) / (2.0 + scales.fast_turn_rate * dt)).min(1.0)
    }
}

/// A weapon's view kick (`combat_arms:recoil`): the data, the vertical
/// accumulator J and the side one J_y (radians, half the view offset each
/// caused, Q4), the side direction and the last kick's time.
#[derive(Component, Clone, Debug)]
pub struct CaRecoil {
    pub params: RecoilParams,
    pub pitch: f32,
    pub yaw: f32,
    /// The side kick goes right (`wag_r`); None until the first one picks
    /// at random.
    pub right: Option<bool>,
    pub last_kick: Option<f64>,
}

impl CaRecoil {
    pub fn new(params: RecoilParams) -> Self {
        Self {
            params,
            pitch: 0.0,
            yaw: 0.0,
            right: None,
            last_kick: None,
        }
    }

    /// (base kick, cap) radians in this mode.
    fn pitch_params(&self, burst: bool) -> (f32, f32) {
        let p = &self.params;
        if burst && p.burst_pitch != 0.0 && p.burst_max_pitch != 0.0 {
            (p.burst_pitch.to_radians(), p.burst_max_pitch.to_radians())
        } else {
            (p.pitch.to_radians(), p.max_pitch.to_radians())
        }
    }

    /// One shot's kick at `now`: returns how far the view goes up,
    /// radians (2h).
    pub fn kick(&mut self, burst: bool, now: f64) -> f32 {
        let (b, max) = self.pitch_params(burst);
        if max <= 0.0 {
            return 0.0;
        }
        let j = self.pitch.clamp(0.0, max);
        let k = b * (1.0 - j / max);
        let k = if k > 0.0 { k + self.params.accel_pitch * j } else { k };
        let h = (k / 2.0).min(max - j);
        self.pitch = j + h;
        self.last_kick = Some(now);
        2.0 * h
    }

    /// The side wag after a kick: returns the yaw change in the engine's
    /// sense (radians, + left), 0 without one. `roll` in [0, 1) picks the
    /// first direction and the flips.
    pub fn wag(&mut self, roll: f32) -> f32 {
        let p = self.params;
        if self.pitch.abs() <= p.random_pitch {
            return 0.0;
        }
        let right = *self.right.get_or_insert(roll < 0.5);
        let max = p.max_wag.to_radians();
        let q = if max != 0.0 { 1.0 - self.yaw.abs() / max } else { 1.0 };
        let w = q * if right { p.wag_r } else { p.wag_l }.to_radians();
        self.yaw += w / 2.0;
        if self.yaw.abs() > p.random_wag && (roll * 2.0).fract() < 0.5 {
            self.right = Some(!right);
        }
        // Combat Arms turns yaw by -w (+ right); the engine's yaw is + left.
        w
    }

    /// The return over `dt` seconds at `now`: (pitch down, yaw change in
    /// the engine's sense), radians.
    pub fn recover(&mut self, dt: f32, now: f64) -> (f32, f32) {
        let p = self.params;
        let waiting = self
            .last_kick
            .is_some_and(|t| now - t < p.jitter_dec_delay as f64 / 1000.0);
        let rate = if waiting { p.jitter_wait_recover } else { p.jitter_recover };
        let step = |acc: f32, max: f32| {
            let scale = if max > 0.0 { 1.0 + acc / max } else { 1.0 };
            acc.min(dt * rate * scale)
        };
        let mut down = 0.0;
        if self.pitch > 0.0 {
            let (_, max) = self.pitch_params(false);
            down = step(self.pitch, max);
            self.pitch -= down / 2.0;
        }
        let mut side = 0.0;
        if self.yaw != 0.0 {
            let v = step(self.yaw.abs(), p.max_wag.to_radians());
            let sign = self.yaw.signum();
            self.yaw -= sign * v / 2.0;
            if self.yaw.signum() != sign {
                self.yaw = 0.0;
            }
            side = -sign * v;
        }
        (down, side)
    }
}

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

/// One hit sphere of a skeleton: in the character's frame (feet at the
/// origin, +Y up, facing -Z, +X its right), world units of a standing
/// figure `SKELETON_HEIGHT` tall.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitSphere {
    pub name: &'static str,
    pub at: Vec3,
    pub radius: f32,
    pub location: Location,
    pub factor: f32,
}

const fn sphere(name: &'static str, at: [f32; 3], radius: f32, location: Location, factor: f32) -> HitSphere {
    HitSphere {
        name,
        at: Vec3::new(at[0], at[1], at[2]),
        radius,
        location,
        factor,
    }
}

use Location::{Arm, Groin, Head, Leg, Torso};

/// The male skeleton's hittable spheres (the spec's "Hit zones": radii,
/// locations and factors; the places are ours, a placeholder figure). The
/// right side is +X.
pub const MALE: &[HitSphere] = &[
    sphere("skull", [0.0, 97.0, 0.0], 1.0, Head, 6.0),
    sphere("jaw", [0.0, 87.5, -1.0], 3.0, Head, 6.0),
    sphere("upper face", [0.0, 93.0, -1.0], 4.6, Head, 6.0),
    sphere("neck", [0.0, 82.0, 0.0], 3.0, Torso, 1.0),
    sphere("upper chest", [0.0, 70.0, 0.0], 10.0, Torso, 1.0),
    sphere("chest", [0.0, 58.0, 0.0], 9.0, Torso, 1.0),
    sphere("groin", [0.0, 45.0, -2.0], 3.0, Groin, 1.0),
    sphere("hip left", [-10.0, 48.0, 2.0], 8.0, Leg, 0.9),
    sphere("hip right", [10.0, 48.0, 2.0], 8.5, Leg, 0.9),
    sphere("shoulder left", [-14.0, 77.0, 0.0], 5.5, Arm, 0.9),
    sphere("shoulder right", [14.0, 77.0, 0.0], 5.5, Arm, 0.9),
    sphere("shoulder left 2", [-14.0, 72.0, 1.0], 7.0, Arm, 0.9),
    sphere("shoulder right 2", [14.0, 72.0, 1.0], 7.0, Arm, 0.9),
    sphere("upper arm left", [-19.0, 64.0, 0.0], 4.5, Arm, 0.9),
    sphere("upper arm right", [19.0, 64.0, 0.0], 4.5, Arm, 0.9),
    sphere("upper arm left 2", [-19.0, 58.0, 0.0], 4.0, Arm, 0.9),
    sphere("upper arm right 2", [19.0, 58.0, 0.0], 4.0, Arm, 0.9),
    sphere("forearm left", [-20.0, 51.0, -1.0], 4.0, Arm, 0.9),
    sphere("forearm right", [20.0, 51.0, -1.0], 4.0, Arm, 0.9),
    sphere("forearm left 2", [-20.0, 44.0, -2.0], 4.0, Arm, 0.9),
    sphere("forearm right 2", [20.0, 44.0, -2.0], 4.0, Arm, 0.9),
    sphere("hand left", [-20.0, 36.0, -3.0], 3.2, Arm, 0.9),
    sphere("hand right", [20.0, 36.0, -3.0], 3.2, Arm, 0.9),
    sphere("thigh left", [-6.0, 36.0, 0.0], 8.0, Leg, 0.9),
    sphere("thigh right", [6.0, 36.0, 0.0], 8.0, Leg, 0.9),
    sphere("thigh left 2", [-6.0, 27.0, 0.0], 7.0, Leg, 0.9),
    sphere("thigh right 2", [6.0, 27.0, 0.0], 7.0, Leg, 0.9),
    sphere("shin left", [-6.0, 19.0, 0.0], 6.0, Leg, 0.75),
    sphere("shin right", [6.0, 19.0, 0.0], 6.0, Leg, 0.75),
    sphere("shin left 2", [-6.0, 12.0, 0.0], 6.0, Leg, 0.75),
    sphere("shin right 2", [6.0, 12.0, 0.0], 6.0, Leg, 0.75),
    sphere("foot left", [-6.0, 4.0, -3.0], 5.0, Leg, 0.5),
    sphere("foot right", [6.0, 4.0, -3.0], 5.0, Leg, 0.5),
    sphere("toe left", [-6.0, 2.0, -10.0], 3.0, Leg, 0.5),
    sphere("toe right", [6.0, 2.0, -10.0], 3.0, Leg, 0.5),
];

/// The female skeleton: the male's body and limbs with its own head (the
/// upper face unhittable, two head spheres of 4.7 at 6.0 and one
/// head-located sphere of 3.0 at 1.0, Q9).
pub const FEMALE: &[HitSphere] = &[
    sphere("skull", [0.0, 97.0, 0.0], 1.0, Head, 6.0),
    sphere("jaw", [0.0, 87.5, -1.0], 3.0, Head, 1.0),
    sphere("head", [0.0, 92.0, -1.0], 4.7, Head, 6.0),
    sphere("head 2", [0.0, 95.0, 1.0], 4.7, Head, 6.0),
    sphere("upper face", [0.0, 93.0, -1.0], 0.0, Head, 6.0),
    sphere("neck", [0.0, 82.0, 0.0], 3.0, Torso, 1.0),
    sphere("upper chest", [0.0, 70.0, 0.0], 10.0, Torso, 1.0),
    sphere("chest", [0.0, 58.0, 0.0], 9.0, Torso, 1.0),
    sphere("groin", [0.0, 45.0, -2.0], 3.0, Groin, 1.0),
    sphere("hip left", [-10.0, 48.0, 2.0], 8.0, Leg, 0.9),
    sphere("hip right", [10.0, 48.0, 2.0], 8.5, Leg, 0.9),
    sphere("shoulder left", [-14.0, 77.0, 0.0], 5.5, Arm, 0.9),
    sphere("shoulder right", [14.0, 77.0, 0.0], 5.5, Arm, 0.9),
    sphere("upper arm left", [-19.0, 64.0, 0.0], 4.5, Arm, 0.9),
    sphere("upper arm right", [19.0, 64.0, 0.0], 4.5, Arm, 0.9),
    sphere("forearm left", [-20.0, 51.0, -1.0], 4.0, Arm, 0.9),
    sphere("forearm right", [20.0, 51.0, -1.0], 4.0, Arm, 0.9),
    sphere("hand left", [-20.0, 36.0, -3.0], 3.2, Arm, 0.9),
    sphere("hand right", [20.0, 36.0, -3.0], 3.2, Arm, 0.9),
    sphere("thigh left", [-6.0, 36.0, 0.0], 8.0, Leg, 0.9),
    sphere("thigh right", [6.0, 36.0, 0.0], 8.0, Leg, 0.9),
    sphere("shin left", [-6.0, 19.0, 0.0], 6.0, Leg, 0.75),
    sphere("shin right", [6.0, 19.0, 0.0], 6.0, Leg, 0.75),
    sphere("foot left", [-6.0, 4.0, -3.0], 5.0, Leg, 0.5),
    sphere("foot right", [6.0, 4.0, -3.0], 5.0, Leg, 0.5),
];

/// A character's hit spheres (`MALE` when it has none).
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

/// One piece of gear's bullet protection by location (fractions) and
/// whether it is used up.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Gear {
    pub head: f32,
    pub torso: f32,
    pub arm: f32,
    pub leg: f32,
    pub consumable: bool,
}

impl Gear {
    fn at(&self, location: Location) -> f32 {
        match location {
            Head => self.head,
            Torso | Groin => self.torso,
            Arm => self.arm,
            Leg => self.leg,
        }
    }
}

/// The spec's common gear ("Armour"): example values until the
/// characters spec gives the table (Q7).
pub const DEFAULT_BODY_GEAR: Gear = Gear {
    head: 0.0,
    torso: 0.15,
    arm: 0.15,
    leg: 0.15,
    consumable: true,
};
pub const LIGHT_BODY_GEAR: Gear = Gear {
    head: 0.0,
    torso: 0.05,
    arm: 0.05,
    leg: 0.05,
    consumable: true,
};
pub const HEAVY_BODY_GEAR: Gear = Gear {
    head: 0.0,
    torso: 0.30,
    arm: 0.30,
    leg: 0.30,
    consumable: true,
};
pub const DEFAULT_HEAD_GEAR: Gear = Gear {
    head: 0.25,
    torso: 0.0,
    arm: 0.0,
    leg: 0.0,
    consumable: true,
};

/// A character's armour: points (AP, hit points) and gear worn. The
/// server's; replicated to clients (`game::net`).
#[derive(Component, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CaArmor {
    pub points: f32,
    pub gear: Vec<Gear>,
}

impl Default for CaArmor {
    /// A model's `MaxArmor` 100 with the default body gear.
    fn default() -> Self {
        Self {
            points: 100.0,
            gear: vec![DEFAULT_BODY_GEAR],
        }
    }
}

/// Damage `d` (hit points) on `location` with zone factor `zone` after
/// armour: returns what health loses, and takes the armour's share. With
/// `lands` false (a refused hit) the armour is left whole.
pub fn armour_and_zone(d: f32, location: Location, zone: f32, armor: Option<&mut CaArmor>, lands: bool) -> f32 {
    let Some(armor) = armor else { return (d * zone).max(0.0) };
    let p: f32 = armor.gear.iter().map(|g| g.at(location)).sum();
    let consumable: f32 = armor.gear.iter().filter(|g| g.consumable).map(|g| g.at(location)).sum();
    let blocked = d * p.clamp(0.0, 1.0);
    let c = if p != 0.0 { consumable / p } else { 0.0 };
    let ap = armor.points;
    let out = if ap <= 0.0 {
        d * zone
    } else if ap - blocked > 0.0 || c <= 0.001 {
        (d - blocked) * zone
    } else {
        (d - ap) * zone
    };
    if ap > 0.0 && lands {
        armor.points = (ap - blocked * c).max(0.0);
    }
    out.max(0.0)
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
            Self::parse(include_str!("../../data/surfaces.toml"))
                .unwrap_or_else(|e| panic!("Combat Arms surfaces.toml: {e}"))
        })
    }
}

// ---------------------------------------------------------------------------
// Plugin

/// Combat Arms' weapons: its hooks, the systems around the weapon frame,
/// and the weapons registered from its data (`data::weapon_set`).
pub struct CaWeaponsPlugin;

impl Plugin for CaWeaponsPlugin {
    fn build(&self, app: &mut App) {
        app.register_weapon_hook::<DamageParams>(DAMAGE, |e, d: &'static WeaponData, p| {
            e.insert(CaDamage::new(p, d.units.length, d.units.damage));
        })
        .register_weapon_hook::<SpreadParams>(SPREAD, |e, d, p| {
            e.insert(CaSpread::new(p.clone(), d.units.length));
        })
        .register_weapon_hook::<RecoilParams>(RECOIL, |e, _, p| {
            e.insert(CaRecoil::new(*p));
        })
        .register_weapon_hook::<CaHandling>(HANDLING, |e, _, p| {
            e.insert(*p);
        });
        if let Err(e) = app.add_weapon_set(super::data::weapon_set()) {
            panic!("Combat Arms weapon data: {e}");
        }
        app.init_resource::<SpreadScales>()
            .init_resource::<SpreadOptions>()
            .insert_resource(CaSurfaces::built_in().clone())
            .add_message::<WeaponEvent>()
            .add_message::<Damage>()
            .add_systems(
                Predict::Weapons,
                (before_shots.before(WeaponFrame), after_shots.after(WeaponFrame)),
            )
            .add_systems(FixedUpdate, give_armour.run_if(crate::core::authoritative))
            // In the predicted tick: a client replaying a switch moves
            // with the penalty of the weapon it held then.
            .add_systems(Predict::Select, apply_handling.after(crate::weapon::SelectWeapons))
            // The meters and the kick are predicted with the weapon and
            // go to the client predicting it (`weapon::sync`).
            .predicted_part::<CaSpread>()
            .predicted_part::<CaRecoil>();
    }
}

impl NetPart for CaSpread {
    /// The meters, the last shot, the sub-step clock, the last view and
    /// the jump flags (the parameters are the data's).
    type Net = (f32, f32, Option<f64>, f64, Option<Vec2>, bool, bool);
    fn net(&self) -> Self::Net {
        (self.fire, self.movement, self.last_shot, self.clock, self.prev_view, self.jumped, self.was_on_ground)
    }
    fn set_net(&mut self, (fire, movement, last_shot, clock, prev_view, jumped, was_on_ground): Self::Net) {
        self.fire = fire;
        self.movement = movement;
        self.last_shot = last_shot;
        self.clock = clock;
        self.prev_view = prev_view;
        self.jumped = jumped;
        self.was_on_ground = was_on_ground;
    }
}

impl NetPart for CaRecoil {
    /// The accumulators, the side direction and the last kick's time.
    type Net = (f32, f32, Option<bool>, Option<f64>);
    fn net(&self) -> Self::Net {
        (self.pitch, self.yaw, self.right, self.last_kick)
    }
    fn set_net(&mut self, (pitch, yaw, right, last_kick): Self::Net) {
        self.pitch = pitch;
        self.yaw = yaw;
        self.right = right;
        self.last_kick = last_kick;
    }
}

/// Characters get the default armour when they first have health.
fn give_armour(new: Query<Entity, (With<Health>, With<Intent>, Without<CaArmor>)>, mut commands: Commands) {
    for e in &new {
        commands.entity(e).insert(CaArmor::default());
    }
}

/// The view a spread sub-step sees: the look angles with the kick.
fn view_of(intent: &Intent, punch: Option<&ViewPunch>) -> Vec2 {
    let p = punch.map_or(Vec3::ZERO, |p| p.0);
    Vec2::new(intent.pitch + p.x, intent.yaw + p.y)
}

/// Before the frame: the recoil returns, and each active weapon's cone is
/// what the HUD reads (`Hitscan::spread`, a disc of n / 1000).
#[allow(clippy::type_complexity)]
fn before_shots(
    mut owners: Query<(Entity, &Inventory, Option<&mut ViewPunch>)>,
    mut weapons: Query<(
        &Weapon,
        Option<&mut CaRecoil>,
        Option<&CaSpread>,
        Option<&mut Hitscan>,
        Option<&AltModes>,
        Option<&Burst>,
        Option<&Zoom>,
    )>,
    clock: Res<SimClock>,
    mut commands: Commands,
) {
    let (dt, now) = (clock.dt(), clock.now);
    for (owner, inv, mut punch) in &mut owners {
        for &w in &inv.weapons {
            let Ok((_, recoil, spread, scan, modes, burst, zoom)) = weapons.get_mut(w) else {
                continue;
            };
            if let Some(mut r) = recoil
                && (r.pitch > 0.0 || r.yaw != 0.0)
            {
                let (down, side) = r.recover(dt, now);
                let delta = Vec3::new(-down, side, 0.0);
                match punch.as_mut() {
                    Some(p) => p.0 += delta,
                    None => {
                        commands.entity(owner).insert(ViewPunch(delta));
                    }
                }
            }
            if Some(w) == inv.active
                && let (Some(s), Some(mut scan)) = (spread, scan)
            {
                let mode = fire_mode(modes.map_or(0, |m| m.current), burst, zoom);
                let n = s.cone(mode) / 1000.0;
                let want = SpreadShape::Disc {
                    inaccuracy: n,
                    spread: 0.0,
                };
                if scan.spread != want {
                    scan.spread = want;
                }
            }
        }
    }
}

/// One round to deliver (a `Fired` event of a Combat Arms weapon).
#[derive(Clone, Copy, Debug)]
struct Round {
    owner: Entity,
    weapon: Entity,
    origin: Vec3,
    yaw: f32,
    pitch: f32,
    seed: u32,
    mode: u8,
    replay: bool,
}

/// A character as the ray sees it.
struct Body {
    entity: Entity,
    /// World box (min, max).
    min: Vec3,
    max: Vec3,
    forward: Vec3,
    right: Vec3,
    /// World spheres (centre, radius) and their data.
    spheres: Vec<(Vec3, f32)>,
    data: &'static [HitSphere],
}

#[derive(bevy::ecs::system::SystemParam)]
struct Delivery<'w, 's> {
    spatial: SpatialQuery<'w, 's>,
    characters: Query<
        'w,
        's,
        (
            Entity,
            &'static Transform,
            &'static Intent,
            &'static MovementState,
            &'static Health,
            Option<&'static ColliderAabb>,
            Option<&'static HitSpheres>,
        ),
    >,
    armor: Query<'w, 's, &'static mut CaArmor>,
    surfaces: Query<'w, 's, &'static PropSurface>,
    colliders: Query<'w, 's, &'static ColliderOf>,
    bodies: Query<'w, 's, (&'static RigidBody, Option<&'static ColliderAabb>)>,
    table: Res<'w, CaSurfaces>,
    damage: MessageWriter<'w, Damage>,
    rules: DamageRules<'w, 's>,
    first: Res<'w, FirstTimePredicted>,
    role: Option<Res<'w, NetRole>>,
    playback: Option<Res<'w, crate::core::Playback>>,
    /// Lag compensation (a network server): each shooter's view tick,
    /// the characters' kept hit volumes, `sv_unlag`.
    lag: (
        Query<'w, 's, &'static ViewTick>,
        Query<'w, 's, (Entity, &'static HitHistory, &'static Health)>,
        Res<'w, LagCompSettings>,
        ResMut<'w, LagCompStats>,
        Res<'w, SimClock>,
    ),
}

impl Delivery<'_, '_> {
    fn authoritative(&self) -> bool {
        self.role.as_deref().is_none_or(|r| *r != NetRole::Client) && self.playback.is_none()
    }

    /// Where the other characters were when `owner` fired, as its client
    /// drew them (lag compensation, `weapon::lagcomp`): a network server's
    /// remote player's shot with `sv_unlag` on; else none (the present).
    fn rewind(&mut self, owner: Entity, server: bool) -> Vec<lagcomp::Rewound> {
        let (views, histories, settings, stats, clock) = &mut self.lag;
        let Some(view) = views.get(owner).ok().and_then(|v| v.0) else {
            return Vec::new();
        };
        if !server || settings.unlag == 0 {
            return Vec::new();
        }
        lagcomp::rewind(
            owner,
            view,
            clock.tick,
            clock.delta.as_secs_f64(),
            settings,
            histories.iter().filter(|(.., h)| h.current > 0.0).map(|(e, history, _)| (e, history)),
            stats,
        )
    }

    /// Every living character but `owner`, as the ray sees it: where it
    /// is, or where `rewound` puts it (its box and look then).
    fn bodies(&self, owner: Entity, rewound: &[lagcomp::Rewound]) -> Vec<Body> {
        self.characters
            .iter()
            .filter(|(e, .., h, _, _)| *e != owner && h.current > 0.0)
            .filter_map(|(entity, t, intent, state, _, aabb, spheres)| {
                let data = spheres.map_or(MALE, |s| s.0);
                let (min, max, spheres, turn) = match rewound.iter().find(|r| r.entity == entity) {
                    Some(r) => {
                        let b = r.aabb();
                        let turn = Quat::from_rotation_y(r.yaw);
                        let (min, max, spheres) = geometry_at(r.origin, turn, b.min, b.max, data);
                        (min, max, spheres, turn)
                    }
                    None => {
                        let (min, max, spheres) = body_geometry(t, intent, state, aabb, data)?;
                        (min, max, spheres, intent.yaw_rotation())
                    }
                };
                Some(Body {
                    entity,
                    min,
                    max,
                    forward: turn * Vec3::NEG_Z,
                    right: turn * Vec3::X,
                    spheres,
                    data,
                })
            })
            .collect()
    }

    fn body(&self, collider: Entity) -> Entity {
        self.colliders.get(collider).map_or(collider, |c| c.body)
    }
}

/// A character's box (world min, max: its movement's hull, else its
/// collider's bounds) and its spheres `data` in the world (centre,
/// radius), scaled to the box's height.
pub fn body_geometry(
    t: &Transform,
    intent: &Intent,
    state: &MovementState,
    aabb: Option<&ColliderAabb>,
    data: &[HitSphere],
) -> Option<(Vec3, Vec3, Vec<(Vec3, f32)>)> {
    let (min, max) = if state.hull_max != state.hull_min {
        (t.translation + state.hull_min, t.translation + state.hull_max)
    } else {
        let b = aabb?;
        (b.min, b.max)
    };
    Some(geometry_at(t.translation, intent.yaw_rotation(), min, max, data))
}

/// A character's spheres `data` in the world for its box `min..max`
/// (world) around `origin`, turned by `turn`: (min, max, spheres).
fn geometry_at(origin: Vec3, turn: Quat, min: Vec3, max: Vec3, data: &[HitSphere]) -> (Vec3, Vec3, Vec<(Vec3, f32)>) {
    let scale = (max.y - min.y) / SKELETON_HEIGHT;
    let feet = origin.with_y(min.y);
    let spheres = data.iter().map(|s| (feet + turn * (s.at * scale), s.radius * scale)).collect();
    (min, max, spheres)
}

/// Where a ray enters an axis-aligned box (distance along the unit `dir`,
/// clamped at 0), if within `limit`.
pub fn box_entry(min: Vec3, max: Vec3, from: Vec3, dir: Vec3, limit: f32) -> Option<f32> {
    let (mut enter, mut leave) = (f32::MIN, f32::MAX);
    for i in 0..3 {
        if dir[i].abs() < 1e-9 {
            if from[i] < min[i] || from[i] > max[i] {
                return None;
            }
            continue;
        }
        let (a, b) = ((min[i] - from[i]) / dir[i], (max[i] - from[i]) / dir[i]);
        enter = enter.max(a.min(b));
        leave = leave.min(a.max(b));
    }
    (enter <= leave && leave >= 0.0 && enter.max(0.0) <= limit).then_some(enter.max(0.0))
}

/// After the frame: deliver each round, step the spread, kick the view.
#[allow(clippy::type_complexity)]
fn after_shots(
    mut events: ParamSet<(MessageReader<WeaponEvent>, MessageWriter<WeaponEvent>)>,
    mut owners: Query<
        (Entity, &Inventory, &Intent, &MovementState, &Velocity, Option<&mut ViewPunch>),

    >,
    mut weapons: Query<(
        Option<&mut CaDamage>,
        Option<&mut CaSpread>,
        Option<&mut CaRecoil>,
        Option<&Hitscan>,
        Option<&AltModes>,
        Option<&Burst>,
        Option<&Zoom>,
    )>,
    mut delivery: Delivery,
    clock: Res<SimClock>,
    scales: Res<SpreadScales>,
    options: Res<SpreadOptions>,
    mut commands: Commands,
) {
    let now = clock.now;
    let rounds: Vec<Round> = events
        .p0()
        .read()
        .filter_map(|e| match e.kind {
            WeaponEventKind::Fired {
                origin,
                yaw,
                pitch,
                seed,
                mode,
                ..
            } => Some(Round {
                owner: e.owner,
                weapon: e.weapon,
                origin,
                yaw,
                pitch,
                seed,
                mode,
                replay: e.replay,
            }),
            _ => None,
        })
        .collect();

    // Deliver, with the cone the meters give before this tick's steps.
    let mut out: Vec<WeaponEvent> = Vec::new();
    for r in &rounds {
        let Ok((Some(mut dmg), spread, _, scan, _, burst, zoom)) = weapons.get_mut(r.weapon) else {
            continue;
        };
        let mode = fire_mode(r.mode, burst, zoom);
        let half = spread.as_ref().map_or(0.0, |s| s.cone(mode)) / 1000.0;
        let pellets = scan.map_or(1, |s| s.pellets.max(1));
        let range = scan.map_or(0.0, |s| s.range);
        let server = delivery.authoritative() && delivery.first.0 && !r.replay;
        if server {
            // The server's spacing: a round too soon after the last one is
            // dropped (spent all the same).
            let early = dmg.last_round.is_some_and(|t| now - t < dmg.fire_delay - 1e-9);
            dmg.last_round = Some(now);
            if early {
                continue;
            }
        }
        let aim = aim_of(r.yaw, r.pitch);
        let rewound = delivery.rewind(r.owner, server);
        for pellet in 0..pellets {
            let mut rng = Rng::new(r.seed.wrapping_add(1 + pellet));
            let (u1, u2) = (rng.next(), rng.next());
            let dir = square_dir(aim, half, u1, u2);
            walk(&mut delivery, &dmg, r, dir, range, &rewound, &mut rng, server, &mut out);
        }
    }

    // Spread sub-steps for each owner's active weapon, then the kicks.
    for (owner, inv, intent, state, vel, mut punch) in &mut owners {
        let Some(active) = inv.active else { continue };
        let Ok((_, spread, recoil, _, modes, burst, zoom)) = weapons.get_mut(active) else {
            continue;
        };
        let mine: Vec<&Round> = rounds.iter().filter(|r| r.owner == owner && r.weapon == active).collect();
        let mode = fire_mode(modes.map_or(0, |m| m.current), burst, zoom);
        if let Some(mut s) = spread {
            step_spread(&mut s, &scales, &options, intent, state, vel.0, punch.as_deref(), mode, !mine.is_empty(), now, clock.dt());
        }
        let Some(mut recoil) = recoil else { continue };
        for (i, r) in mine.iter().enumerate() {
            let up = recoil.kick(fire_mode(r.mode, burst, zoom) == FireMode::Burst, now);
            let roll = shared_random(intent.command, "combat_arms:wag", i as i32).next();
            let side = recoil.wag(roll);
            let delta = Vec3::new(up, side, 0.0);
            match punch.as_mut() {
                Some(p) => p.0 += delta,
                None => {
                    commands.entity(owner).insert(ViewPunch(delta));
                }
            }
        }
    }
    let mut writer = events.p1();
    for e in out {
        writer.write(e);
    }
}

/// One tick of a weapon's spread meters (the spec's step 8).
#[allow(clippy::too_many_arguments)]
fn step_spread(
    s: &mut CaSpread,
    scales: &SpreadScales,
    options: &SpreadOptions,
    intent: &Intent,
    state: &MovementState,
    velocity: Vec3,
    punch: Option<&ViewPunch>,
    mode: FireMode,
    fired: bool,
    now: f64,
    dt: f32,
) {
    // Airborne after a jump: left the ground with the jump key held.
    if state.on_ground {
        s.jumped = false;
    } else if s.was_on_ground {
        s.jumped = intent.jump;
    }
    s.was_on_ground = state.on_ground;
    let cone = s.cone_params(mode);
    let st = s.states(mode);
    if cone.max == 0 {
        s.fire = 0.0;
        s.movement = 0.0;
        return;
    }
    let moving = intent.move_axis != Vec2::ZERO;
    let cap = match (moving, state.crouching) {
        (true, true) => st.fire_move_duck,
        (true, false) => st.fire_move,
        (false, true) => st.fire_duck,
        (false, false) => st.fire,
    };
    let jumping = s.jumped && !state.on_ground;
    let run = !intent.walk;
    let (target, vmax) = if state.on_ladder {
        (st.run, LADDER_SPEED)
    } else if jumping {
        (st.jump, if run { RUN_SPEED } else { WALK_SPEED })
    } else if state.crouching {
        (if run { st.run } else { st.walk }, CROUCH_SPEED)
    } else if run {
        (st.run, RUN_SPEED)
    } else {
        (st.walk, WALK_SPEED)
    };
    let mu = (velocity.length() / (vmax * s.unit)).min(1.0);
    let mu_s = if state.on_ladder { target } else { mu * target };
    s.clock += dt as f64;
    let mut n = ((s.clock / SPREAD_SUBSTEP + 1e-9).floor() as u32).min(SPREAD_SUBSTEP_MAX);
    if n > 0 {
        s.clock = if options.quantised { 0.0 } else { (s.clock - n as f64 * SPREAD_SUBSTEP).max(0.0) };
    } else if fired {
        n = 1;
    }
    let view = view_of(intent, punch);
    for i in 0..n {
        let prev = s.prev_view.unwrap_or(view);
        let rho = CaSpread::turn(scales, view.x - prev.x, view.y - prev.y);
        s.prev_view = Some(view);
        s.move_substep(scales, cone, target, mu_s, rho, jumping, st.jump);
        s.fire_substep(scales, cone, fired && i == 0, cap, now);
    }
}

/// Walk one pellet's ray from the round's origin (the spec's "Hitscan",
/// "Penetration" and "Damage per ray").
#[allow(clippy::too_many_arguments)]
fn walk(
    w: &mut Delivery,
    dmg: &CaDamage,
    r: &Round,
    dir: Vec3,
    range: f32,
    rewound: &[lagcomp::Rewound],
    rng: &mut Rng,
    server: bool,
    out: &mut Vec<WeaponEvent>,
) {
    let bodies = w.bodies(r.owner, rewound);
    let excluded: Vec<Entity> = std::iter::once(r.owner)
        .chain(w.characters.iter().map(|(e, ..)| e))
        .collect();
    let filter = SpatialQueryFilter::from_excluded_entities(excluded).with_mask(SOLID_LAYERS);
    let unit = dmg.unit;
    let mut from = r.origin;
    let mut dir = dir;
    let mut left = range;
    // Penetration dampening so far; the first victim's damage (after its
    // band) for one behind it; characters passed.
    let mut dampen = 1.0;
    let mut first: Option<f32> = None;
    let mut skip: Vec<Entity> = Vec::new();
    for segment in 0..SERVER_TRACE_STEPS {
        if left < unit {
            break;
        }
        let Ok(d3) = Dir3::new(dir) else { break };
        let world = w.spatial.cast_ray(from, d3, left, true, &filter);
        let wall = world.as_ref().map_or(left, |h| h.distance);
        // The nearest character whose widened box the ray enters before
        // the wall and whose spheres it passes.
        let mut best: Option<(f32, usize, usize)> = None;
        for (bi, b) in bodies.iter().enumerate() {
            if skip.contains(&b.entity) {
                continue;
            }
            let c = (b.min + b.max) / 2.0;
            let half = (b.max - b.min) / 2.0 * Vec3::new(HITBOX_WIDEN, 1.0, HITBOX_WIDEN);
            let Some(t) = box_entry(c - half, c + half, from, dir, wall) else {
                continue;
            };
            if best.is_some_and(|(bt, ..)| bt <= t) {
                continue;
            }
            let Some(si) = pick_sphere(&b.spheres, from, from + dir * left) else {
                continue;
            };
            best = Some((t, bi, si));
        }
        let kind = |from: Vec3, to: Vec3, hit: Option<Entity>, normal: Option<Vec3>| {
            if segment == 0 {
                WeaponEventKind::Shot { from, to, hit, normal }
            } else {
                WeaponEventKind::ShotContinued { from, to, hit, normal }
            }
        };
        let event = |kind| WeaponEvent {
            owner: r.owner,
            weapon: r.weapon,
            kind,
            replay: r.replay,
        };
        if let Some((t, bi, si)) = best {
            let b = &bodies[bi];
            // The reported point: where the ray enters the plain box.
            let t_point = box_entry(b.min, b.max, from, dir, left).unwrap_or(t);
            let point = from + dir * t_point;
            out.push(event(kind(from, point, Some(b.entity), Some(-dir))));
            let distance = point.distance(r.origin);
            let d = match first {
                // One behind the first victim: its damage banded again.
                Some(prev) => prev * band_scale(dmg, distance),
                None => {
                    // The first segment is the shooter's own test: the
                    // roll, from behind, the band.
                    let behind = segment == 0 && is_behind(b.forward, b.right, dir);
                    base_damage(dmg, rng.next(), behind, distance)
                }
            };
            first.get_or_insert(d);
            if server {
                hurt(w, dmg, r, b, si, d * dampen, point, dir, out);
            }
            skip.push(b.entity);
            left -= t_point;
            from = point;
            continue;
        }
        let Some(hit) = world else {
            out.push(event(kind(from, from + dir * left, None, None)));
            break;
        };
        let point = from + dir * hit.distance;
        let body = w.body(hit.entity);
        out.push(event(kind(from, point, Some(body), Some(hit.normal))));
        // Through it?
        let name = w.surfaces.get(hit.entity).or_else(|_| w.surfaces.get(body)).ok();
        let Some(pass) = name.and_then(|n| w.table.get(&n.0)) else { break };
        let exit = if pass.thickness <= 0.0 {
            point
        } else {
            // Cast back from the limit (plus a unit) beyond the entry.
            let reach = pass.thickness + unit;
            let far = point + dir * reach;
            let back = Dir3::new(-dir).unwrap_or(Dir3::NEG_Z);
            let hit_back = w.spatial.cast_ray_predicate(far, back, reach, true, &SpatialQueryFilter::default(), &|e| {
                e == hit.entity
            });
            let exit = match hit_back {
                Some(h) if h.distance <= 0.0 => break,
                Some(h) => far - dir * h.distance,
                None => point,
            };
            if exit.distance(point) > pass.thickness + 1e-4 {
                break;
            }
            // An object (not the world) must be thinner than the limit on
            // some axis too.
            if let Ok((rb, aabb)) = w.bodies.get(body)
                && !rb.is_static()
                && aabb.is_some_and(|b| (b.max - b.min).min_element() >= pass.thickness)
            {
                break;
            }
            exit
        };
        left = (left - exit.distance(from)) * pass.range;
        dampen *= pass.damage;
        let q = Quat::from_rotation_arc(Vec3::NEG_Z, dir);
        let p = pass.perturb / 1000.0;
        dir = (dir + q * Vec3::X * rng.range(-p, p) + q * Vec3::Y * rng.range(-p, p)).normalize();
        from = exit + dir * 1e-3;
    }
}

/// Damage `d` (health units, before the victim) to the sphere `si` of
/// `b`: armour and zone, the death threshold.
#[allow(clippy::too_many_arguments)]
fn hurt(
    w: &mut Delivery,
    dmg: &CaDamage,
    r: &Round,
    b: &Body,
    si: usize,
    d: f32,
    point: Vec3,
    dir: Vec3,
    out: &mut Vec<WeaponEvent>,
) {
    let sphere = b.data[si];
    // Hit points from here on (armour points are).
    let hp = 0.01;
    let side = |l, r| if sphere.at.x < 0.0 { l } else { r };
    let hitgroup = match sphere.location {
        Head => Hitgroup::Head,
        Torso => Hitgroup::Chest,
        Groin => Hitgroup::Stomach,
        Arm => side(Hitgroup::LeftArm, Hitgroup::RightArm),
        Leg => side(Hitgroup::LeftLeg, Hitgroup::RightLeg),
    };
    let mut damage = Damage {
        target: b.entity,
        attacker: Some(r.owner),
        amount: 0.0,
        point,
        dir,
        hitgroup,
        kind: DamageKind::Bullet,
        weapon: None,
        force: Vec3::ZERO,
    };
    let lands = w.rules.lands(&damage);
    // Armour knows bullets only (gear's other damage types: not yet).
    let armor = if dmg.damage_type == bullet_type() { w.armor.get_mut(b.entity).ok() } else { None };
    let points = armour_and_zone(d / hp, sphere.location, sphere.factor, armor.map(|a| a.into_inner()), lands) * hp;
    let health = w.characters.get(b.entity).map_or(0.0, |(.., h, _, _)| h.current);
    // Dead at 1 HP or less.
    damage.amount = if points > 0.0 && health - points <= DEATH_THRESHOLD * hp + 1e-6 {
        points.max(health)
    } else {
        points
    };
    out.push(WeaponEvent {
        owner: r.owner,
        weapon: r.weapon,
        kind: WeaponEventKind::Hit {
            target: b.entity,
            amount: damage.amount,
            hitgroup,
        },
        replay: r.replay,
    });
    w.damage.write(damage);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_surface_table_parses() {
        let t = CaSurfaces::built_in();
        assert_eq!(t.get("Wood").map(|s| s.damage), Some(0.5));
        assert_eq!(t.get("plywood").map(|s| s.damage), Some(0.6));
        assert_eq!(t.get("water").map(|s| s.thickness), Some(0.0));
        assert!(t.get("concrete").is_none());
    }

    #[test]
    fn spheres_are_named_once() {
        for set in [MALE, FEMALE] {
            for (i, s) in set.iter().enumerate() {
                assert!(set[..i].iter().all(|o| o.name != s.name), "{}", s.name);
            }
        }
    }
}
