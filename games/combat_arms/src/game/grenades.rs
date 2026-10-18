//! Combat Arms' thrown weapons (specs/combat_arms/grenades.md): the frag
//! (M67), flash (XM84), smoke (M18), incendiary (M-23F) and nerve gas
//! (G7A) grenades, on Chimera's thrown-weapon framework (`weapon::grenade`).
//!
//! The weapons are data (data/weapons/m67.toml and the others, the
//! `combat_arms:grenade` hook: `GrenadeParams`), built as Chimera's
//! `Throwable` (count, pin, release, post-fire and redraw, predicted with
//! the weapon frame) with `ThrowOptions`: the release when the pre-fire
//! ends, held or not (no cooking, 3); the spent weapon kept at total 0
//! (inventory.md 10: `inventory` skips it and switches away); no default
//! live drop on death (this module's `death_release`); and the projectile
//! made here (`launch`): from the eye along the view, perturbed by the
//! weapon's square spread, at `speed` with none of the thrower's velocity
//! (4). It is a Chimera `Projectile` marked `GameFlown` with this game's
//! state (`CaGrenade`), so the network draws it (`NetItem`), round
//! restarts clear it and `Detonated` tells the presentation and clients.
//!
//! On the server (authoritative), grenades, explosions and flash timers
//! run on server frames of their own (`CaServerFrames`: the shipped
//! server's 20 per second, each frame at the first tick at or after its
//! time, dt exactly 0.05 s), in the spec's per-tick order:
//!
//! 1. A grenade made since the last frame starts there (its fuse counts
//!    from that frame; its first physics step is the next frame's).
//! 2. Each grenade in creation order: the one-time nudge of a grenade
//!    that came to rest (6), the fuse (8: the first frame at or after
//!    start + `fuse`), else its physics (`step`: |v|² < 0.1 zeroed, water
//!    (7: `CaLiquid` volumes, the map's water at the defaults), the
//!    quarter-gravity move of 5 under 1600 u/s², a point against the world:
//!    a mirror bounce kept by the surface's hardness (`CaGrenadeSurfaces`,
//!    data/grenade_surfaces.toml) with the rest test on the horizontal
//!    speed, through invisible surfaces, gone on sky; characters touched by
//!    their hit spheres slow it to 65 % and are passed). A detonation (8)
//!    sends the effect (`Detonated`) and applies its instant part at once:
//!    a frag's blast (9: everything whose box touches the sphere, players
//!    twice (Quirk 3), d to the centre, two cover traces and the head-top
//!    retry, the linear falloff from a quarter of the radius, friendly fire
//!    by `core::DamageRules`, no armour against explosions, death at 1 HP),
//!    other grenades hit and chained in the same frame (13), a flash test
//!    (10: players once, protection `CaFlashProtection`, a hard facing
//!    split, a timer); a smoke's cloud is the clients' look only (11:
//!    `SmokeCloud` without a `SightBlocker`, so nothing in game logic sees
//!    it); fire and gas leave a `CaDamageSphere` (12).
//! 3. Damage spheres refresh the over-time entries of the players inside
//!    (`CaGrenadeEffects::over_time`).
//! 4. Flash timers that ran out send the flash (T ≥ 0.1 s) as Chimera's
//!    `core::Blinded` (white, fading by the end, T/2: its network message
//!    carries it to the victim's client) and `CaFlashed`; over-time
//!    entries deal their rate × dt (burn halved by body armour with armour
//!    points left, poison unprotected).
//!
//! `flash_white` is the client's screen curve (cos(π t / T) until T/2).
//! The server's "grenade in hand, pin pulled" flag (`CaPin`: on once the
//! pre-fire has ended while fire stays held, off when it is let go or the
//! weapon changes) releases a full-fuse grenade at 500 u/s along the body's
//! facing when its holder dies (14), ammo unchecked (Quirk 5).
//!
//! Units: the data's world units (1 u = 1 cm, `inventory::UNIT`) and hit
//! points inside; the world in metres and health (1.0 = 100 HP).
//!
//! Known shortcuts (the spec's most literal reading where it is unclear,
//! listed for its open questions):
//! - The release key time k (Q3) is the throw animation's end (67 ms): the
//!   grenade leaves 767 ms after the press, a switch before then keeps it;
//!   the throw starts on the press's tick (the spec's one or two client
//!   frames before the pre-fire are not modelled); the projectile appears
//!   a tick after the release (`Throwable` throws strictly after its time),
//!   then waits for the next server frame (Q6: its first step the frame
//!   after). The XM84's and M18's animations are the M67's (Q3); the
//!   re-draw after a throw with some left is the draw (Q5).
//! - The spread: n = n_min + floor((n_max − n_min) p) g with p the weapon's
//!   `CaSpread` meters (none on the stock grenades: p = 0, so n = n_min) and
//!   the limits not multiplied by g, as section 4 gives the flash's 0.06°
//!   with g = 0 (weapon_firing.md would make it 0). No throw perk (Q13).
//! - The bounce uses the formula's −0.1 n (twice the Constants' 0.05
//!   surface push), as the test cases do.
//! - The at-rest nudge uses the data's `half_height` (PLACEHOLDER, Q9).
//! - Characters touch a grenade only when its path enters one of their hit
//!   spheres (the placeholder skeleton), once until it leaves them (a touch
//!   a step would slow it again inside); the thrower never. Grenades never
//!   touch each other (points; `hits_same_kind` is read, unused). Moving
//!   brushes and props bounce it like the world (Q7: not solid in the
//!   reference). No bounce sound or bots' hearing of it (no assets, no
//!   bots); spin is drawn only.
//! - Water: `CaLiquid` boxes with their own gravity, viscosity and current
//!   (none on any map yet), the map's water volumes at the defaults (−200,
//!   no viscosity or current); half-volume sounds don't exist.
//! - Cover traces are blocked by static and kinematic bodies that aren't
//!   characters, prop entities or "invisible" surfaces. Breakables
//!   (`Damageable` bodies) take the blast without a cover test.
//! - Armour against blasts is skipped (no stock gear lists explode or
//!   flash); burn uses 0.5 per body gear (the stock body gear's burn entry,
//!   `burn_gear`), head gear none; flash protection is `CaFlashProtection`
//!   on the victim (goggles aren't in `weapons::Gear`, which knows bullets).
//! - The flash's facing is the body's yaw (Q11); W and C are the stock 2.0
//!   and 1.0 (Quirk 6: the victim's weapon's, all the defaults). The screen
//!   draws Chimera's white overlay over `Blinded` (a linear fade to T/2,
//!   no screen capture or first-frame HUD hiding); `flash_white` gives the
//!   spec's cosine for Combat Arms' own overlay when it is drawn.
//! - Bullets don't hit grenades (Q8); no drop-grenade perk (Q13); no debug
//!   bounce cap; the round's end doesn't detonate them (no rounds yet: a
//!   restart clears them, Chimera's); the M67's knock-back is the infection
//!   mode's (Q14).
//! - Placeholder looks (L10 not lifted): a ball in flight, a grey ball for
//!   the smoke (`ui::visuals`), no explosion or flash models.

use std::collections::HashMap;

use avian3d::prelude::*;
use bevy::prelude::*;
use serde::Deserialize;

use super::{
    inventory::{CaSelection, UNIT},
    weapons::{CaArmor, CaSpread, Gear, HitSpheres, Location, MALE, armour_and_zone, body_geometry, square_dir},
};
use crate::{
    core::{
        Blinded, Damage, DamageKind, DamageRules, Damageable, Died, Health, Hitgroup, Intent, MapWater,
        MovementState, Predict, PropSurface, SOLID_LAYERS, SimClock, SimSet,
    },
    map::PropEntity,
    weapon::{
        Inventory, Weapon, WeaponFrame, WeaponState,
        data::{WeaponData, WeaponDataAppExt},
        grenade::{
            Blast, Detonated, Flash, Flight, GameFlown, GrenadeEffect, GrenadeKind, Launch, Projectile, Smoke,
            SmokeCloud, ThrowOptions, ThrowRule, Throwable, spawn_projectile,
        },
        random::shared_random,
    },
};

/// The hook name (docs/weapon-data.md, "Hooks").
pub const HOOK: &str = "combat_arms:grenade";

/// The stock loadout's grenade (data/weapons/m67.toml).
pub const M67: &str = "combat_arms:m67";
pub const XM84: &str = "combat_arms:xm84";
pub const M18: &str = "combat_arms:m18";
pub const M23F: &str = "combat_arms:m23f";
pub const G7A: &str = "combat_arms:g7a";

// Constants (world units, seconds; the spec's "Constants").

/// The engine's gravity for every non-player object, u/s² (down).
pub const OBJECT_GRAVITY: f32 = 1600.0;
/// Displacement per step uses this × g dt², not ½ (Quirk 1).
pub const GRAVITY_INTEGRATION_FACTOR: f32 = 0.25;
/// A velocity with |v|² below this, (u/s)², is zeroed before the step.
pub const SLOW_STOP: f32 = 0.1;
/// The bounce's push off the surface, u/s (the formula's −0.1 n).
pub const SURFACE_PUSH: f32 = 0.1;
/// `GrenadeDampenPercent`: what a bounce loses on a surface not in the
/// table.
pub const DEFAULT_DAMPEN: f32 = 0.35;
/// `GrenadeMinVelMag`: a floor hit slower than this horizontally stops it,
/// u/s.
pub const REST_SPEED: f32 = 50.0;
/// A surface is a floor when its normal's up part is above this.
pub const REST_FLOOR_NORMAL: f32 = 0.75;
/// Blast and flash are full inside this fraction of the radius.
pub const BLAST_FULL_FRACTION: f32 = 0.25;
/// Objects whose dims vector is longer than this (u) have the excess taken
/// off their blast distance (not players).
pub const BIG_OBJECT_SLACK: f32 = 100.0;
/// `FlashBangDelay`: blast to blindness facing it, s; facing away a third.
pub const FLASH_DELAY: f64 = 0.25;
pub const FLASH_DELAY_BACK: f64 = FLASH_DELAY / 3.0;
/// Flash time facing away, as a fraction.
pub const FLASH_BACK_FACTOR: f32 = 0.25;
/// Shorter flashes (s) aren't sent.
pub const FLASH_MIN_TIME: f32 = 0.1;
/// A flash amount at most this does nothing.
pub const FLASH_MIN_AMOUNT: f32 = 0.001;
/// `WhiteScreenDecl` and `CapturedDecl` defaults (every stock weapon).
pub const WHITE_DECL: f32 = 2.0;
pub const CAPTURED_DECL: f32 = 1.0;
/// The live grenade released when its holder dies, u/s.
pub const DEATH_RELEASE_SPEED: f32 = 500.0;
/// A water volume's gravity unless the level sets another, u/s².
pub const LIQUID_GRAVITY: f32 = -200.0;
/// Speed lost per unit of a water volume's viscosity, u/s².
pub const LIQUID_VISCOSITY_SCALE: f32 = 5000.0;
/// A grenade at this many HP or fewer after damage goes off.
pub const DESTROYED_AT: f32 = 1.0;
/// A player at this many HP or fewer after damage dies.
pub const DEATH_AT: f32 = 1.0;
/// The shipped dedicated server's frame, s (20 frames a second).
pub const SERVER_FRAME: f64 = 0.05;
/// A detonation after a touch is backed off along the motion this far, u.
pub const DETONATION_BACKOFF: f32 = 0.1;
/// The over-time damage's cover traces start this far towards the
/// target, u.
pub const OVER_TIME_TRACE_START: f32 = 1.0;
/// Spin rates' ranges, rad/s (pitch; yaw and roll).
pub const SPIN_PITCH: f32 = std::f32::consts::PI;
pub const SPIN_YAW_ROLL: f32 = std::f32::consts::TAU;
/// Kept between a grenade and the surface it touched, u (ours: the next
/// step's trace starts outside it).
const SKIN: f32 = 1e-3;

// ---------------------------------------------------------------------------
// Data

/// What a grenade's detonation does at once (its ammo's `AreaDamageType`).
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AreaType {
    /// No instant part (fire, gas).
    None,
    /// A blast (type 7).
    Explode,
    /// A flash test (type 34).
    Flash,
    /// A smoke cloud on the clients; nothing on the server.
    Smoke,
}

/// The kind of an over-time damage sphere (`ProgDamageType`).
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OverTimeKind {
    /// Type 3: halved by body armour with points left.
    Burn,
    /// Type 9: unprotected.
    Poison,
}

/// A damage sphere's numbers (`ProgDamage*`): HP/s, u, s.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OverTimeParams {
    pub kind: OverTimeKind,
    pub rate: f32,
    pub radius: f32,
    pub lifetime: f32,
    pub duration: f32,
}

/// The `combat_arms:grenade` hook's parameters (data/weapons/m67.toml
/// says where each comes from). World units, seconds, hit points.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GrenadeParams {
    /// `SpawnedAmount`, `MaxAmount`.
    pub spawned: u32,
    pub max: u32,
    /// Pre-fire (pin pull), the release key time inside the throw, the
    /// post-fire.
    pub prefire: f32,
    pub release_key: f32,
    pub postfire: f32,
    /// `MinPerturb`, `MaxPerturb` (thousandths), `DynamicPerturbFactor`.
    pub perturb: [u32; 2],
    #[serde(default)]
    pub dynamic_perturb: f32,
    /// The projectile's `Lifetime` and `Velocity`; `DropVelocity`.
    pub fuse: f32,
    pub speed: f32,
    #[serde(default)]
    pub drop_speed: f32,
    /// The ammo's `Health`: the live grenade's hit points.
    pub health: f32,
    /// `CanHitSameKind`.
    #[serde(default)]
    pub hits_same_kind: bool,
    /// `AreaDamage`, `AreaDamageRadius`, `AreaDamageType`.
    #[serde(default)]
    pub area_damage: f32,
    #[serde(default)]
    pub area_radius: f32,
    pub area_type: AreaType,
    /// The lasting sphere (fire, gas).
    #[serde(default)]
    pub over_time: Option<OverTimeParams>,
    /// The world model's half-height, for the at-rest nudge.
    #[serde(default)]
    pub half_height: f32,
}

/// Weapon part: a Combat Arms grenade's numbers, and its document's units
/// (metres per length unit, health per damage point).
#[derive(Component, Clone, Debug)]
pub struct CaGrenadeData {
    pub params: GrenadeParams,
    pub unit: f32,
    pub hp: f32,
}

/// Weapon part (the server's): "grenade in hand, pin pulled" (3), from the
/// pin's start while fire stays held.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct CaPin {
    /// When the pin pull started (fire held since).
    pub since: Option<f64>,
    /// The third-person pre-fire has ended with fire still held.
    pub on: bool,
}

/// Grenade bounce hardness by surface name (data/grenade_surfaces.toml).
#[derive(Resource, Clone, Debug, Default)]
pub struct CaGrenadeSurfaces(pub Vec<(String, f32)>);

impl CaGrenadeSurfaces {
    /// The hardness of `name` (clamped to [0, 1]); unlisted surfaces keep
    /// 1 − `DEFAULT_DAMPEN`.
    pub fn hardness(&self, name: Option<&str>) -> f32 {
        let name = name.map(str::to_lowercase);
        name.and_then(|n| self.0.iter().find(|(s, _)| *s == n).map(|(_, h)| *h))
            .unwrap_or(1.0 - DEFAULT_DAMPEN)
            .clamp(0.0, 1.0)
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Doc {
            surface: Vec<Row>,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Row {
            names: Vec<String>,
            hardness: f32,
        }
        let doc: Doc = toml::from_str(text).map_err(|e| e.to_string())?;
        Ok(Self(
            doc.surface
                .into_iter()
                .flat_map(|r| r.names.into_iter().map(move |n| (n.to_lowercase(), r.hardness)))
                .collect(),
        ))
    }

    /// The built-in table (data/grenade_surfaces.toml).
    pub fn built_in() -> Self {
        Self::parse(include_str!("../../data/grenade_surfaces.toml")).expect("grenade_surfaces.toml")
    }
}

/// A water volume (a LithTech container) as a box, world metres, with its
/// gravity (u/s², the level's or `LIQUID_GRAVITY`), viscosity (0-1) and
/// current (u/s).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct CaLiquid {
    pub min: Vec3,
    pub max: Vec3,
    pub gravity: f32,
    pub viscosity: f32,
    pub current: Vec3,
}

/// A victim's flash protection: the sum of its gear's torso entries for
/// the flash type (anti-flash goggles: 1.0; others 0.3, 0.5, 0.7).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct CaFlashProtection(pub f32);

/// The last flash sent to this player: when it arrived (the server's
/// clock) and its flash time T, s.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct CaFlashed {
    pub at: f64,
    pub time: f32,
}

/// The clients' smoke cloud's look (11): a PLACEHOLDER sphere growing to
/// 3 m over 2 s, gone by 12 s (the 10 sprites of 200-300 u living 8-10 s
/// after a 2 s emission).
pub fn smoke_look() -> Smoke {
    Smoke {
        max_radius: 300.0 * UNIT,
        expand_time: 2.0,
        fade_start: 8.0,
        fade_end: 12.0,
        pop_speed: 0.0,
        sight_radius: 0.0,
        sound: None,
    }
}

/// The `combat_arms:grenade` hook: the weapon as Chimera's `Throwable`
/// with Combat Arms' throw (`ThrowOptions`, `launch`), its numbers
/// (`CaGrenadeData`) and the server's pin flag (`CaPin`).
pub fn throwable(e: &mut EntityWorldMut, d: &'static WeaponData, p: &GrenadeParams) {
    let unit = d.units.length;
    let hp = d.units.damage;
    // Read by the network's clients for a detonation's kind (and by
    // nothing else here: this module flies and detonates the grenade).
    let effect = match p.area_type {
        AreaType::Flash => GrenadeEffect::Flash(Flash {
            model: |_, _| None,
            sound: None,
            hearing: None,
            through_smoke: 1.0,
        }),
        AreaType::Smoke => GrenadeEffect::Smoke(smoke_look()),
        AreaType::Explode | AreaType::None => GrenadeEffect::Blast(Blast {
            damage: p.area_damage * hp,
            radius: p.area_radius * unit,
            bell_sigma: None,
            legacy: false,
            armor_ratio: None,
            quantum: 0.0,
            probe_up: 0.0,
            probe_down: 0.0,
            pull_out: 0.0,
            src_lift: 0.0,
            body_target: (0.5, 0.5),
            force: 0.0,
            force_jitter: (1.0, 1.0),
            max_push_speed: 0.0,
            ragdolls: None,
            scorch: None,
            sound: None,
            hearing: None,
            shake: None,
            shield: None,
            through_walls: false,
        }),
    };
    e.insert((
        Throwable {
            count: p.spawned.min(p.max.max(p.spawned)),
            max: p.max,
            // The release is the throw animation's fire event (3.4): the
            // pin's time runs to it.
            pin_time: p.prefire + p.release_key,
            throw_delay: 0.0,
            throw_time: p.postfire,
            redraw_time: d.draw,
            fuse: p.fuse,
            // Unused: `launch` makes the projectile and this module flies
            // it (`GameFlown`).
            throw: ThrowRule {
                pitch_lift: 0.0,
                pitch_scale: 1.0,
                pitch_scale_up: 1.0,
                speed_per_deg: 0.0,
                speed_max: p.speed * unit,
                forward: 0.0,
            },
            flight: Flight {
                half: 0.0,
                gravity: OBJECT_GRAVITY * unit,
                elasticity: 1.0 - DEFAULT_DAMPEN,
                player_elasticity: 1.0,
                elasticity_cap: 1.0,
                stop_speed: REST_SPEED * unit,
                floor_normal: REST_FLOOR_NORMAL,
                breakable_damage: 0.0,
                breakable_slowdown: 1.0,
                water_slowdown: 1.0,
                water_entry: 1.0,
                max_velocity: f32::MAX,
                check_interval: SERVER_FRAME as f32,
                spin_roll: 0.0,
                spin_pitch: 0.0,
            },
            effect,
            pin_sounds: Vec::new(),
            bounce_sound: None,
            pin: false,
            throw_at: None,
            redraw_at: None,
        },
        ThrowOptions {
            release_at_pin_end: true,
            keep_spent: true,
            drop_primed: false,
            launch: Some(launch),
        },
        CaGrenadeData {
            params: p.clone(),
            unit,
            hp,
        },
        CaPin::default(),
    ));
}

// ---------------------------------------------------------------------------
// The live grenade

/// One contact of a grenade's flight (for tests and tools): the step, the
/// point (u) and the velocity after it (u/s).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    pub step: u32,
    pub at: Vec3,
    pub velocity: Vec3,
    pub rest: bool,
}

/// A Combat Arms grenade in the world (the server's), on its `Projectile`.
/// Position and velocity in world units.
#[derive(Component, Clone, Debug)]
pub struct CaGrenade {
    pub weapon: &'static str,
    pub params: GrenadeParams,
    pub unit: f32,
    pub hp: f32,
    pub thrower: Option<Entity>,
    /// u, u/s.
    pub pos: Vec3,
    pub velocity: Vec3,
    /// The server frame it started on (None until the first after it was
    /// made).
    pub created: Option<u64>,
    /// Physics steps run.
    pub steps: u32,
    pub resting: bool,
    /// The one-time nudge of a grenade that came to rest, u.
    pub nudge: Option<Vec3>,
    /// It touched something in its last step, moving along `last_dir`.
    pub touched: bool,
    pub last_dir: Vec3,
    /// Hit points left.
    pub health: f32,
    /// Creation order (updates run in it).
    pub seq: u64,
    pub contacts: Vec<Contact>,
    /// The character it is passing through.
    pub inside: Option<Entity>,
    /// Visual spin, rad/s.
    pub spin: Vec3,
}

/// Server frames (`SERVER_FRAME` apart, the first tick at or after each
/// one's time runs it) and the grenades' creation counter.
#[derive(Resource, Clone, Copy, Debug)]
pub struct CaServerFrames {
    pub interval: f64,
    pub frame: u64,
    pub next_seq: u64,
}

impl Default for CaServerFrames {
    fn default() -> Self {
        Self {
            interval: SERVER_FRAME,
            frame: 0,
            next_seq: 0,
        }
    }
}

impl CaServerFrames {
    pub fn time(&self) -> f64 {
        self.frame as f64 * self.interval
    }
}

/// A pending flash: the victim's server timer (10).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlashTimer {
    pub victim: Entity,
    pub due: f64,
    pub time: f32,
}

/// An over-time damage entry (12): HP/s for the seconds left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverTime {
    pub victim: Entity,
    pub kind: OverTimeKind,
    pub rate: f32,
    pub remaining: f32,
    pub attacker: Option<Entity>,
    pub weapon: &'static str,
}

/// The server's lasting grenade effects on players.
#[derive(Resource, Clone, Debug, Default)]
pub struct CaGrenadeEffects {
    pub flashes: Vec<FlashTimer>,
    pub over_time: Vec<OverTime>,
}

/// An incendiary's or gas grenade's invisible sphere (12), metres for the
/// centre, world units for the radius.
#[derive(Component, Clone, Debug)]
pub struct CaDamageSphere {
    pub centre: Vec3,
    pub params: OverTimeParams,
    pub unit: f32,
    pub hp: f32,
    pub thrower: Option<Entity>,
    pub weapon: &'static str,
    pub created: u64,
    pub until: f64,
}

/// A smoke cloud's end (the server removes it).
#[derive(Component, Clone, Copy, Debug)]
pub struct CaSmoke {
    pub until: f64,
}

/// Linear falloff (9): full within a quarter of `radius`, none at it.
pub fn falloff(radius: f32, d: f32) -> f32 {
    let r0 = BLAST_FULL_FRACTION * radius;
    if d <= r0 {
        1.0
    } else if d <= radius {
        1.0 - (d - r0) / (radius - r0)
    } else {
        0.0
    }
}

/// One step of flight (5) in world units: from the velocity at the start,
/// the move and the stored velocity, under `gravity` (u/s², signed).
pub fn flight_step(v: Vec3, gravity: f32, dt: f32) -> (Vec3, Vec3) {
    let g = Vec3::new(0.0, gravity, 0.0);
    (v * dt + GRAVITY_INTEGRATION_FACTOR * g * dt * dt, v + g * dt)
}

/// A bounce (6): the post-gravity `v` mirrored on `n`, pushed off and kept
/// by the hardness `h`; and whether it comes to rest.
pub fn bounce(v: Vec3, n: Vec3, h: f32) -> (Vec3, bool) {
    let out = h.clamp(0.0, 1.0) * (v - 2.0 * v.dot(n) * n - SURFACE_PUSH * n);
    if out.xz().length() < REST_SPEED && n.y > REST_FLOOR_NORMAL {
        (Vec3::ZERO, true)
    } else {
        (out, false)
    }
}

/// The screen's whiteness `t` seconds into a flash of `time` (10):
/// clamp(1 − W S(u)), S the half-cosine to u = 0.5, then u.
pub fn flash_white(t: f32, time: f32) -> f32 {
    if time <= 0.0 {
        return 0.0;
    }
    let u = t / time;
    let s = if u < 0.5 {
        0.5 * (1.0 - (std::f32::consts::PI * u).cos())
    } else {
        u
    };
    (1.0 - WHITE_DECL * s).clamp(0.0, 1.0)
}

/// The flash's whiteness now from Chimera's `Blinded` as this module
/// writes it (white from `fade_start`, gone at `end` = T/2 later).
pub fn blinded_white(b: &Blinded, now: f64) -> f32 {
    let time = (2.0 * (b.end - b.fade_start)) as f32;
    flash_white((now - b.fade_start) as f32, time)
}

/// The burn protection of one piece of gear: body gear 0.5 everywhere,
/// head gear none (the stock data: grenades.md 12, weapon_firing.md).
pub fn burn_gear(g: &Gear) -> Gear {
    let body = g.head == 0.0 && g.torso > 0.0;
    let p = if body { 0.5 } else { 0.0 };
    Gear {
        head: p,
        torso: p,
        arm: p,
        leg: p,
        consumable: g.consumable,
    }
}

// ---------------------------------------------------------------------------
// Plugin

/// Combat Arms' grenades: the hook (before the weapon data loads:
/// `CombatArmsPlugins` adds this before `weapons`), the server's frames
/// and effects, the throw's sprint gate and the death release.
pub struct CaGrenadesPlugin;

impl Plugin for CaGrenadesPlugin {
    fn build(&self, app: &mut App) {
        app.register_weapon_hook::<GrenadeParams>(HOOK, throwable)
            .init_resource::<CaServerFrames>()
            .init_resource::<CaGrenadeEffects>()
            .insert_resource(CaGrenadeSurfaces::built_in())
            .add_message::<Detonated>()
            .add_message::<Damage>()
            .add_message::<Died>()
            // Predicted with the throw: sprint holds a grenade's fire.
            .add_systems(Predict::Weapons, sprint_gate.before(WeaponFrame))
            .add_systems(
                FixedUpdate,
                (
                    (pin_flags, server_frames)
                        .chain()
                        .after(WeaponFrame)
                        .in_set(SimSet::Weapons),
                    death_release.after(crate::core::apply_damage),
                )
                    .run_if(crate::core::authoritative),
            );
    }
}

/// Fire is ignored while sprinting (3.1), in the sprint pose and its
/// post-sprint time: an idle grenade in hand can't start its throw.
fn sprint_gate(
    owners: Query<(&Inventory, &MovementState, Option<&CaSelection>)>,
    mut weapons: Query<(&Weapon, &Throwable, &mut WeaponState), With<CaGrenadeData>>,
    clock: Res<SimClock>,
) {
    let now = clock.now;
    let dt = clock.dt() as f64;
    for (weapon, t, mut st) in &mut weapons {
        let Some(owner) = weapon.owner else { continue };
        let Ok((inv, state, sel)) = owners.get(owner) else { continue };
        if t.primed() || t.redraw_at.is_some() || inv.active.is_none() {
            continue;
        }
        let held_until = if state.sprinting || sel.is_some_and(|s| s.sprint_pose) {
            now + dt
        } else {
            sel.map_or(0.0, |s| s.sprint_out_until)
        };
        if held_until > now + 1e-5 && st.next_primary < held_until {
            st.next_primary = held_until;
        }
    }
}

/// The server's "grenade in hand, pin pulled" flag (3).
fn pin_flags(
    owners: Query<(&Intent, &Inventory)>,
    mut weapons: Query<(Entity, &Weapon, &Throwable, &CaGrenadeData, &mut CaPin)>,
    clock: Res<SimClock>,
) {
    let now = clock.now;
    for (entity, weapon, t, data, mut pin) in &mut weapons {
        let held = weapon
            .owner
            .and_then(|o| owners.get(o).ok())
            .is_some_and(|(intent, inv)| inv.active == Some(entity) && intent.fire);
        let mut next = *pin;
        if !held {
            next = CaPin::default();
        } else {
            if next.since.is_none() && t.pin {
                next.since = Some(now);
            }
            if let Some(s) = next.since
                && now + 1e-5 >= s + data.params.prefire as f64
            {
                next.on = true;
            }
        }
        if *pin != next {
            *pin = next;
        }
    }
}

/// The projectile of a throw (4): from the eye along the view bent by the
/// weapon's square spread, at its speed, with none of the thrower's
/// velocity (Quirk 2).
pub fn launch(world: &mut World, l: Launch) {
    let Some(data) = world.get::<CaGrenadeData>(l.weapon).cloned() else {
        return;
    };
    let p = &data.params;
    let fraction = world.get::<CaSpread>(l.weapon).map_or(0.0, |s| s.fraction());
    let (lo, hi) = (p.perturb[0].min(p.perturb[1]), p.perturb[1]);
    let n = lo as f32 + ((hi - lo) as f32 * fraction).floor() * p.dynamic_perturb;
    let mut rng = shared_random(l.command, HOOK, 0);
    let (u1, u2) = (rng.next(), rng.next());
    let aim = Quat::from_euler(EulerRot::YXZ, l.yaw, l.pitch, 0.0);
    let dir = square_dir(aim, n / 1000.0, u1, u2);
    spawn_grenade(world, l.weapon, Some(l.thrower), l.eye, dir * p.speed * data.unit);
}

/// A live grenade of `weapon`'s kind (a Combat Arms `Throwable`) at `at`
/// (m) with `velocity` (m/s), credited to `thrower`, full fuse: Chimera's
/// `Projectile`, flown here. None for a weapon that isn't one.
pub fn spawn_grenade(
    world: &mut World,
    weapon: Entity,
    thrower: Option<Entity>,
    at: Vec3,
    velocity: Vec3,
) -> Option<Entity> {
    let data = world.get::<CaGrenadeData>(weapon)?.clone();
    let id = world.get::<Weapon>(weapon)?.id;
    let e = spawn_projectile(world, weapon, at, velocity)?;
    let seq = {
        let mut frames = world.resource_mut::<CaServerFrames>();
        frames.next_seq += 1;
        frames.next_seq
    };
    let mut rng = shared_random(seq as u32, "combat_arms:spin", 0);
    let spin = Vec3::new(
        (rng.next() * 2.0 - 1.0) * SPIN_PITCH,
        (rng.next() * 2.0 - 1.0) * SPIN_YAW_ROLL,
        (rng.next() * 2.0 - 1.0) * SPIN_YAW_ROLL,
    );
    let mut ent = world.entity_mut(e);
    if let Some(mut p) = ent.get_mut::<Projectile>() {
        p.thrower = thrower;
    }
    ent.insert((
        GameFlown,
        CaGrenade {
            weapon: id,
            params: data.params.clone(),
            unit: data.unit,
            hp: data.hp,
            thrower,
            pos: at / data.unit,
            velocity: velocity / data.unit,
            created: None,
            steps: 0,
            resting: false,
            nudge: None,
            touched: false,
            last_dir: Vec3::ZERO,
            health: data.params.health,
            seq,
            contacts: Vec::new(),
            inside: None,
            spin,
        },
    ));
    Some(e)
}

/// A player dying with the pin pulled releases a live grenade of that
/// weapon at its hull centre, 500 u/s along the body's facing, full fuse,
/// ammo unchecked (14, Quirk 5).
fn death_release(
    mut died: MessageReader<Died>,
    owners: Query<(&Inventory, &Transform, &MovementState, &Intent)>,
    mut weapons: Query<(&CaGrenadeData, &mut CaPin)>,
    mut commands: Commands,
) {
    for d in died.read() {
        let Ok((inv, t, state, intent)) = owners.get(d.entity) else {
            continue;
        };
        let Some(active) = inv.active else { continue };
        let Ok((data, mut pin)) = weapons.get_mut(active) else {
            continue;
        };
        if !pin.on {
            continue;
        }
        *pin = CaPin::default();
        let centre = t.translation + (state.hull_min + state.hull_max) / 2.0;
        let velocity = intent.yaw_rotation() * Vec3::NEG_Z * DEATH_RELEASE_SPEED * data.unit;
        let thrower = d.entity;
        commands.queue(move |w: &mut World| {
            spawn_grenade(w, active, Some(thrower), centre, velocity);
        });
    }
}

// ---------------------------------------------------------------------------
// The server's frames

/// The world as grenades see it (read only).
#[derive(bevy::ecs::system::SystemParam)]
struct GrenadeWorld<'w, 's> {
    spatial: SpatialQuery<'w, 's>,
    colliders: Query<'w, 's, &'static ColliderOf>,
    surfaces: Query<'w, 's, &'static PropSurface>,
    rigid: Query<'w, 's, &'static RigidBody>,
    props: Query<'w, 's, (), With<PropEntity>>,
    people: Query<'w, 's, (), With<Intent>>,
    #[allow(clippy::type_complexity)]
    characters: Query<
        'w,
        's,
        (
            Entity,
            &'static Transform,
            &'static Intent,
            &'static MovementState,
            Option<&'static ColliderAabb>,
            Option<&'static HitSpheres>,
            &'static Health,
            Option<&'static CaFlashProtection>,
        ),
        Without<CaGrenade>,
    >,
    breakables: Query<'w, 's, (Entity, &'static ColliderAabb), (With<Damageable>, Without<Intent>)>,
    liquids: Query<'w, 's, &'static CaLiquid>,
    water: Option<Res<'w, MapWater>>,
    table: Res<'w, CaGrenadeSurfaces>,
    entities: &'w bevy::ecs::entity::Entities,
}

/// What a character is to grenades: its box (m), centre (m), spheres (m)
/// and head top (m, y).
struct Body {
    entity: Entity,
    min: Vec3,
    max: Vec3,
    centre: Vec3,
    spheres: Vec<(Vec3, f32)>,
    head_top: f32,
    facing: Vec3,
    health: f32,
    flash_protection: f32,
}

impl GrenadeWorld<'_, '_> {
    fn body(&self, collider: Entity) -> Entity {
        self.colliders.get(collider).map_or(collider, |c| c.body)
    }

    fn surface(&self, collider: Entity) -> Option<String> {
        self.surfaces
            .get(collider)
            .or_else(|_| self.surfaces.get(self.body(collider)))
            .ok()
            .map(|s| s.0.to_lowercase())
    }

    fn invisible(&self, collider: Entity) -> bool {
        self.surface(collider).as_deref() == Some("invisible")
    }

    /// The first world surface on the segment (m): distance (m), normal,
    /// collider. Characters, `skip` and invisible surfaces are passed.
    fn cast(&self, from: Vec3, to: Vec3, skip: Option<Entity>) -> Option<(f32, Vec3, Entity)> {
        let d = to - from;
        let dir = Dir3::new(d).ok()?;
        let filter = SpatialQueryFilter::default().with_mask(SOLID_LAYERS);
        self.spatial
            .cast_ray_predicate(from, dir, d.length(), true, &filter, &|e| {
                let body = self.body(e);
                Some(body) != skip && !self.people.contains(body) && !self.invisible(e)
            })
            .map(|h| (h.distance, h.normal, h.entity))
    }

    /// Whether solid world geometry or a moving brush stops the segment
    /// (m) one way (cover, 9): static and kinematic bodies, not
    /// characters, prop entities or invisible surfaces.
    fn blocked_one_way(&self, from: Vec3, to: Vec3) -> bool {
        let d = to - from;
        let Ok(dir) = Dir3::new(d) else { return false };
        let filter = SpatialQueryFilter::default().with_mask(SOLID_LAYERS);
        self.spatial
            .cast_ray_predicate(from, dir, d.length(), true, &filter, &|e| {
                let body = self.body(e);
                let solid = self
                    .rigid
                    .get(e)
                    .or_else(|_| self.rigid.get(body))
                    .is_ok_and(|rb| !rb.is_dynamic());
                solid && !self.people.contains(body) && !self.props.contains(body) && !self.invisible(e)
            })
            .is_some()
    }

    /// Both cover traces (blast to target and back) clear.
    fn clear(&self, a: Vec3, b: Vec3) -> bool {
        !self.blocked_one_way(a, b) && !self.blocked_one_way(b, a)
    }

    /// The living characters, in no particular order.
    fn bodies(&self) -> Vec<Body> {
        let mut out = Vec::new();
        for (entity, t, intent, state, aabb, spheres, health, flash) in &self.characters {
            if health.current <= 0.0 {
                continue;
            }
            let data = spheres.map_or(MALE, |s| s.0);
            let Some((min, max, spheres)) = body_geometry(t, intent, state, aabb, data) else {
                continue;
            };
            let head_top = data
                .iter()
                .zip(&spheres)
                .filter(|(s, _)| s.location == Location::Head)
                .map(|(_, (c, r))| c.y + r)
                .fold(f32::MIN, f32::max);
            out.push(Body {
                entity,
                min,
                max,
                centre: (min + max) / 2.0,
                spheres,
                head_top: if head_top == f32::MIN { max.y } else { head_top },
                facing: intent.yaw_rotation() * Vec3::NEG_Z,
                health: health.current,
                flash_protection: flash.map_or(0.0, |f| f.0),
            });
        }
        out
    }

    /// The water at `p` (m): (gravity u/s², viscosity, current u/s).
    fn liquid(&self, p: Vec3) -> Option<(f32, f32, Vec3)> {
        if let Some(l) = self.liquids.iter().find(|l| p.cmpge(l.min).all() && p.cmple(l.max).all()) {
            return Some((l.gravity, l.viscosity, l.current));
        }
        let wet = self.water.as_ref().is_some_and(|w| {
            w.0.iter()
                .any(|v| v.brush.planes.iter().all(|(n, d)| n.dot(p) - d <= 0.0))
        });
        wet.then_some((LIQUID_GRAVITY, 0.0, Vec3::ZERO))
    }
}

/// Where a segment first enters a sphere: the fraction along it.
fn sphere_entry(from: Vec3, to: Vec3, centre: Vec3, r: f32) -> Option<f32> {
    let d = to - from;
    let a = d.length_squared();
    if a <= 0.0 || r <= 0.0 {
        return None;
    }
    let m = from - centre;
    let c = m.length_squared() - r * r;
    if c <= 0.0 {
        return Some(0.0);
    }
    let b = m.dot(d);
    let disc = b * b - a * c;
    if disc < 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()) / a;
    (0.0..=1.0).contains(&t).then_some(t)
}

/// What a frame changes besides the grenades: damage dealt (with each
/// target's health as it goes), detonations, effects.
struct Frame {
    time: f64,
    now: f64,
    dt: f32,
    frame: u64,
    health: HashMap<Entity, f32>,
    gone: Vec<Entity>,
    queue: Vec<Entity>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct Out<'w, 's> {
    damage: MessageWriter<'w, Damage>,
    detonated: MessageWriter<'w, Detonated>,
    rules: DamageRules<'w, 's>,
    armour: Query<'w, 's, &'static mut CaArmor>,
    effects: ResMut<'w, CaGrenadeEffects>,
    commands: Commands<'w, 's>,
}

impl Out<'_, '_> {
    /// Deal `amount` (health units) to `target` by `attacker`: refused by
    /// the team rules, death at `DEATH_AT` HP or less (the rest of its
    /// health), counted against its health this frame.
    #[allow(clippy::too_many_arguments)]
    fn deal(
        &mut self,
        f: &mut Frame,
        target: Entity,
        health: f32,
        amount: f32,
        kind: DamageKind,
        attacker: Option<Entity>,
        weapon: &'static str,
        point: Vec3,
        hp: f32,
    ) {
        let mut damage = Damage {
            target,
            attacker,
            amount,
            point,
            dir: Vec3::ZERO,
            hitgroup: Hitgroup::Generic,
            kind,
            weapon: Some(weapon),
            force: Vec3::ZERO,
        };
        if amount <= 0.0 || self.rules.refuse(&damage) {
            return;
        }
        let left = f.health.entry(target).or_insert(health);
        if *left <= 0.0 {
            return;
        }
        if *left - amount <= DEATH_AT * hp + 1e-6 {
            damage.amount = amount.max(*left);
        }
        *left -= damage.amount;
        self.damage.write(damage);
    }

    /// Whether the team rules spare `victim` from `attacker`'s grenade.
    fn spared(&self, victim: Entity, attacker: Option<Entity>) -> bool {
        self.rules.refuse(&Damage {
            target: victim,
            attacker,
            amount: 1.0,
            point: Vec3::ZERO,
            dir: Vec3::ZERO,
            hitgroup: Hitgroup::Generic,
            kind: DamageKind::Blast,
            weapon: None,
            force: Vec3::ZERO,
        })
    }
}

type Grenades<'w, 's> = Query<'w, 's, (Entity, &'static mut CaGrenade, &'static mut Transform, &'static mut Projectile)>;

/// Run the server frames due by this tick.
#[allow(clippy::too_many_arguments)]
fn server_frames(
    mut frames: ResMut<CaServerFrames>,
    clock: Res<SimClock>,
    mut grenades: Grenades,
    mut spheres: Query<(Entity, &CaDamageSphere)>,
    smokes: Query<(Entity, &CaSmoke)>,
    world: GrenadeWorld,
    mut out: Out,
) {
    let due = ((clock.now + 1e-9) / frames.interval).floor() as u64;
    while frames.frame < due {
        frames.frame += 1;
        let mut f = Frame {
            time: frames.time(),
            now: clock.now,
            dt: frames.interval as f32,
            frame: frames.frame,
            health: HashMap::new(),
            gone: Vec::new(),
            queue: Vec::new(),
        };
        run_frame(&mut f, &mut grenades, &mut spheres, &world, &mut out);
    }
    for (e, s) in &smokes {
        if clock.now >= s.until {
            out.commands.entity(e).try_despawn();
        }
    }
}

fn run_frame(
    f: &mut Frame,
    grenades: &mut Grenades,
    spheres: &mut Query<(Entity, &CaDamageSphere)>,
    world: &GrenadeWorld,
    out: &mut Out,
) {
    // 1-2. Grenades in creation order.
    let mut order: Vec<(u64, Entity)> = grenades.iter().map(|(e, g, ..)| (g.seq, e)).collect();
    order.sort_unstable_by_key(|(s, _)| *s);
    for (_, e) in order {
        if f.gone.contains(&e) {
            continue;
        }
        let fused = {
            let Ok((_, mut g, mut transform, _)) = grenades.get_mut(e) else {
                continue;
            };
            let created = match g.created {
                None => {
                    g.created = Some(f.frame);
                    continue;
                }
                Some(c) => c,
            };
            if let Some(n) = g.nudge.take() {
                g.pos += n;
                transform.translation = g.pos * g.unit;
            }
            let elapsed = (f.frame - created) as f64 * f.dt as f64;
            elapsed + 1e-9 >= g.params.fuse as f64
        };
        if fused {
            f.queue.push(e);
            detonations(f, grenades, world, out);
            continue;
        }
        let Ok((_, mut g, mut transform, mut projectile)) = grenades.get_mut(e) else {
            continue;
        };
        if g.resting {
            continue;
        }
        if !step(f, &mut g, world) {
            // Into the sky: gone without a trace.
            f.gone.push(e);
            out.commands.entity(e).try_despawn();
            continue;
        }
        transform.translation = g.pos * g.unit;
        let spin = g.spin * f.dt;
        transform.rotation *= Quat::from_euler(EulerRot::YXZ, spin.y, spin.x, spin.z);
        projectile.velocity = g.velocity * g.unit;
    }
    // 3. Lasting explosions refresh the entries of the players inside.
    let bodies = world.bodies();
    for (e, s) in spheres.iter() {
        if s.thrower.is_some_and(|t| !world.entities.contains(t)) || f.time + 1e-9 >= s.until {
            out.commands.entity(e).try_despawn();
            continue;
        }
        if s.created == f.frame {
            continue;
        }
        let r = s.params.radius * s.unit;
        for b in &bodies {
            let to = b.centre;
            if to.distance(s.centre) > r || out.spared(b.entity, s.thrower) {
                continue;
            }
            let start = s.centre + (to - s.centre).normalize_or_zero() * OVER_TIME_TRACE_START * s.unit;
            if !world.clear(start, to) && !world.clear(start, to.with_y(b.head_top)) {
                continue;
            }
            let entry = OverTime {
                victim: b.entity,
                kind: s.params.kind,
                rate: s.params.rate * s.hp,
                remaining: s.params.duration,
                attacker: s.thrower,
                weapon: s.weapon,
            };
            let list = &mut out.effects.over_time;
            match list.iter_mut().find(|o| o.victim == b.entity && o.kind == s.params.kind) {
                Some(o) => *o = entry,
                None => list.push(entry),
            }
        }
    }
    // 4. Each player's flash timer and over-time damage.
    let timers = std::mem::take(&mut out.effects.flashes);
    for t in timers {
        if f.time + 1e-9 < t.due {
            out.effects.flashes.push(t);
            continue;
        }
        if t.time >= FLASH_MIN_TIME
            && let Ok(mut ent) = out.commands.get_entity(t.victim)
        {
            ent.insert((
                Blinded {
                    alpha: 1.0,
                    fade_start: f.now,
                    end: f.now + (t.time / WHITE_DECL) as f64,
                },
                CaFlashed { at: f.now, time: t.time },
            ));
        }
    }
    let entries = std::mem::take(&mut out.effects.over_time);
    for mut o in entries {
        let Some(b) = bodies.iter().find(|b| b.entity == o.victim) else {
            continue;
        };
        let mut amount = o.rate * f.dt;
        let hp = 0.01;
        let lands = !out.spared(o.victim, o.attacker);
        if o.kind == OverTimeKind::Burn
            && let Ok(mut armour) = out.armour.get_mut(o.victim)
        {
            let mut burn = CaArmor {
                points: armour.points,
                gear: armour.gear.iter().map(burn_gear).collect(),
            };
            amount = armour_and_zone(amount / hp, Location::Torso, 1.0, Some(&mut burn), lands) * hp;
            if armour.points != burn.points {
                armour.points = burn.points;
            }
        }
        let kind = match o.kind {
            OverTimeKind::Burn => DamageKind::Burn,
            OverTimeKind::Poison => DamageKind::Generic,
        };
        out.deal(f, o.victim, b.health, amount, kind, o.attacker, o.weapon, b.centre, hp);
        o.remaining -= f.dt;
        if o.remaining > 1e-4 {
            out.effects.over_time.push(o);
        }
    }
}

/// One physics step (5-7) of a grenade not at rest. False when it went
/// into the sky.
fn step(f: &Frame, g: &mut CaGrenade, world: &GrenadeWorld) -> bool {
    let dt = f.dt;
    let unit = g.unit;
    g.touched = false;
    let mut v = g.velocity;
    if v.length_squared() < SLOW_STOP {
        v = Vec3::ZERO;
    }
    let mut gravity = -OBJECT_GRAVITY;
    if let Some((lg, c, current)) = world.liquid(g.pos * unit) {
        gravity = lg;
        if (0.0..=1.0).contains(&c) {
            let speed = v.length();
            let loss = LIQUID_VISCOSITY_SCALE * c * dt;
            v = if loss >= speed { Vec3::ZERO } else { v * (speed - loss) / speed };
        }
        v += current * dt;
    }
    let (delta, after) = flight_step(v, gravity, dt);
    g.velocity = after;
    g.steps += 1;
    if delta == Vec3::ZERO {
        return true;
    }
    let from = g.pos;
    let to = from + delta;
    let dir = delta.normalize();
    // The world.
    let wall = world.cast(from * unit, to * unit, g.thrower);
    let wall_t = wall.map_or(f32::MAX, |(d, ..)| d / (delta.length() * unit));
    // Characters, by their hit spheres (not the thrower, not one it is
    // already passing through).
    let mut touch: Option<(f32, Entity)> = None;
    let mut still_inside = false;
    for b in world.bodies() {
        if Some(b.entity) == g.thrower {
            continue;
        }
        let entry = b
            .spheres
            .iter()
            .filter_map(|(c, r)| sphere_entry(from * unit, to * unit, *c, *r))
            .fold(f32::MAX, f32::min);
        if entry == f32::MAX {
            continue;
        }
        if g.inside == Some(b.entity) {
            still_inside = true;
            continue;
        }
        if touch.is_none_or(|(t, _)| entry < t) {
            touch = Some((entry, b.entity));
        }
    }
    if !still_inside {
        g.inside = None;
    }
    if let Some((t, who)) = touch
        && t < wall_t
    {
        // Snapped to the contact, slowed by flesh, no mirror; on through.
        g.pos = from + delta * t;
        g.velocity *= world.table.hardness(Some("flesh"));
        g.touched = true;
        g.last_dir = dir;
        g.inside = Some(who);
        g.contacts.push(Contact {
            step: g.steps,
            at: g.pos,
            velocity: g.velocity,
            rest: false,
        });
        return true;
    }
    let Some((distance, n, collider)) = wall else {
        g.pos = to;
        return true;
    };
    let surface = world.surface(collider);
    if surface.as_deref() == Some("sky") {
        return false;
    }
    let n = n.normalize_or(Vec3::Y);
    let at = from + dir * (distance / unit);
    let (v2, rest) = bounce(g.velocity, n, world.table.hardness(surface.as_deref()));
    let h = world.table.hardness(surface.as_deref());
    g.pos = at + n * SKIN;
    g.velocity = v2;
    g.touched = true;
    g.last_dir = dir;
    g.contacts.push(Contact {
        step: g.steps,
        at,
        velocity: v2,
        rest,
    });
    // A new spin at every bounce.
    let mut rng = shared_random(g.seq as u32, "combat_arms:spin", g.steps as i32);
    g.spin = Vec3::new(
        (rng.next() * 2.0 - 1.0) * SPIN_PITCH,
        (rng.next() * 2.0 - 1.0) * SPIN_YAW_ROLL,
        (rng.next() * 2.0 - 1.0) * SPIN_YAW_ROLL,
    );
    if rest {
        g.resting = true;
        g.spin = Vec3::ZERO;
        g.nudge = Some(-dir * g.params.half_height * h.max(0.1));
    }
    true
}

/// Detonate the queued grenades (8), and those their blasts set off (13).
fn detonations(f: &mut Frame, grenades: &mut Grenades, world: &GrenadeWorld, out: &mut Out) {
    while let Some(e) = f.queue.pop() {
        if f.gone.contains(&e) {
            continue;
        }
        let Ok((_, g, ..)) = grenades.get(e) else { continue };
        let g = g.clone();
        f.gone.push(e);
        out.commands.entity(e).try_despawn();
        let at_u = if g.touched { g.pos - g.last_dir * DETONATION_BACKOFF } else { g.pos };
        let at = at_u * g.unit;
        let p = &g.params;
        out.detonated.write(Detonated {
            kind: match p.area_type {
                AreaType::Flash => GrenadeKind::Flash,
                AreaType::Smoke => GrenadeKind::Smoke,
                AreaType::Explode | AreaType::None => GrenadeKind::Blast,
            },
            weapon: g.weapon,
            thrower: g.thrower,
            at,
            ground: None,
            projectile: e,
            shake: None,
        });
        if p.area_damage > 0.0 && p.area_radius > 0.0 {
            match p.area_type {
                AreaType::Explode => blast(f, e, &g, at, grenades, world, out),
                AreaType::Flash => flash(f, &g, at, world, out),
                AreaType::Smoke | AreaType::None => {}
            }
        }
        if p.area_type == AreaType::Smoke {
            let look = smoke_look();
            out.commands.spawn((
                Name::new("Smoke cloud"),
                SmokeCloud {
                    centre: at,
                    started: f.now,
                    smoke: look.clone(),
                    grenade: None,
                    weapon: Some(g.weapon),
                },
                CaSmoke {
                    until: f.now + look.fade_end as f64,
                },
                Transform::from_translation(at),
            ));
        }
        if let Some(o) = p.over_time
            && o.lifetime > 0.0
        {
            out.commands.spawn((
                Name::new("Damage sphere"),
                Transform::from_translation(at),
                CaDamageSphere {
                    centre: at,
                    params: o,
                    unit: g.unit,
                    hp: g.hp,
                    thrower: g.thrower,
                    weapon: g.weapon,
                    created: f.frame,
                    until: f.time + o.lifetime as f64,
                },
            ));
        }
    }
}

/// The closest point of a box to `p`.
fn box_distance(min: Vec3, max: Vec3, p: Vec3) -> f32 {
    p.clamp(min, max).distance(p)
}

/// A frag's blast (9) at `at` (m).
#[allow(clippy::too_many_arguments)]
fn blast(
    f: &mut Frame,
    me: Entity,
    g: &CaGrenade,
    at: Vec3,
    grenades: &mut Grenades,
    world: &GrenadeWorld,
    out: &mut Out,
) {
    let p = &g.params;
    let (unit, hp) = (g.unit, g.hp);
    let r = p.area_radius;
    let amount = |d: f32| p.area_damage * falloff(r, d);
    // Players and their hitboxes: the same blast twice (Quirk 3).
    for b in world.bodies() {
        if box_distance(b.min, b.max, at) > r * unit {
            continue;
        }
        let d = b.centre.distance(at) / unit;
        if d > r {
            continue;
        }
        if !world.clear(at, b.centre) && !world.clear(at, b.centre.with_y(b.head_top)) {
            continue;
        }
        // Explode: no stock gear protects (armour points untouched).
        let damage = amount(d) * hp;
        for _ in 0..2 {
            let health = b.health;
            out.deal(f, b.entity, health, damage, DamageKind::Blast, g.thrower, g.weapon, b.centre, hp);
        }
    }
    // Other live grenades (13).
    let others: Vec<Entity> = grenades
        .iter()
        .filter(|(e, ..)| *e != me && !f.gone.contains(e))
        .map(|(e, ..)| e)
        .collect();
    for o in others {
        let Ok((_, mut other, ..)) = grenades.get_mut(o) else { continue };
        let pos = other.pos * other.unit;
        let d = pos.distance(at) / unit;
        if d > r || !world.clear(at, pos) {
            continue;
        }
        other.health -= amount(d);
        if other.health <= DESTROYED_AT {
            f.queue.push(o);
        }
    }
    // Breakables and other damageable objects: no cover test.
    for (e, aabb) in &world.breakables {
        if box_distance(aabb.min, aabb.max, at) > r * unit {
            continue;
        }
        let centre = (aabb.min + aabb.max) / 2.0;
        let dims = (aabb.max - aabb.min) / 2.0 / unit;
        let slack = (dims.length() - BIG_OBJECT_SLACK).max(0.0);
        let d = (centre.distance(at) / unit - slack).max(0.0);
        if d > r {
            continue;
        }
        out.damage.write(Damage {
            target: e,
            attacker: g.thrower,
            amount: amount(d) * hp,
            point: centre,
            dir: (centre - at).normalize_or_zero(),
            hitgroup: Hitgroup::Generic,
            kind: DamageKind::Blast,
            weapon: Some(g.weapon),
            force: Vec3::ZERO,
        });
    }
}

/// A flash test (10) at `at` (m): each player in sight gets a timer.
fn flash(f: &mut Frame, g: &CaGrenade, at: Vec3, world: &GrenadeWorld, out: &mut Out) {
    let p = &g.params;
    let unit = g.unit;
    let r = p.area_radius;
    for b in world.bodies() {
        if box_distance(b.min, b.max, at) > r * unit {
            continue;
        }
        let d = b.centre.distance(at) / unit;
        if d > r {
            continue;
        }
        if !world.clear(at, b.centre) && !world.clear(at, b.centre.with_y(b.head_top)) {
            continue;
        }
        if out.spared(b.entity, g.thrower) {
            continue;
        }
        let amount = p.area_damage * falloff(r, d) * (1.0 - b.flash_protection.clamp(0.0, 1.0));
        if amount <= FLASH_MIN_AMOUNT {
            continue;
        }
        let facing = b.facing.dot(b.centre - at) < 0.0;
        let (delay, time) = if facing {
            (FLASH_DELAY, amount)
        } else {
            (FLASH_DELAY_BACK, amount * FLASH_BACK_FACTOR)
        };
        let timer = FlashTimer {
            victim: b.entity,
            due: f.time + delay,
            time,
        };
        let list = &mut out.effects.flashes;
        match list.iter_mut().find(|t| t.victim == b.entity) {
            Some(t) => *t = timer,
            None => list.push(timer),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_surface_table_parses() {
        let t = CaGrenadeSurfaces::built_in();
        assert_eq!(t.hardness(Some("Desert Sand")), 0.1);
        assert_eq!(t.hardness(Some("chain-link fence")), 0.5);
        assert_eq!(t.hardness(Some("concrete")), 0.65);
        assert_eq!(t.hardness(None), 0.65);
        assert_eq!(t.hardness(Some("sky")), 0.0);
    }

    #[test]
    fn falloff_is_full_to_a_quarter() {
        assert_eq!(falloff(450.0, 100.0), 1.0);
        assert!((falloff(450.0, 300.0) - 0.4444).abs() < 1e-4);
        assert_eq!(falloff(450.0, 450.0), 0.0);
        assert_eq!(falloff(450.0, 460.0), 0.0);
    }

    #[test]
    fn the_flash_curve_is_a_half_cosine() {
        assert_eq!(flash_white(0.0, 25.0), 1.0);
        assert!((flash_white(6.25, 25.0) - 0.7071).abs() < 1e-4);
        assert!((flash_white(8.333, 25.0) - 0.5).abs() < 1e-3);
        assert_eq!(flash_white(12.5, 25.0), 0.0);
    }
}
