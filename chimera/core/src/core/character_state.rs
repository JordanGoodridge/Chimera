use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::brush_grid::MapBrush;

/// Character velocity in meters per second, owned by the active Movement
/// implementation.
#[derive(Component, Reflect, Default, Clone, Copy, Debug, Deref, DerefMut, Serialize, Deserialize)]
#[reflect(Component)]
pub struct Velocity(pub Vec3);

/// Published by the active Movement implementation every tick. Other slots
/// (weapons, HUD, animation) read this instead of depending on a specific
/// movement implementation.
#[derive(Component, Reflect, Default, Clone, Debug, Serialize, Deserialize)]
#[reflect(Component)]
pub struct MovementState {
    pub on_ground: bool,
    pub crouching: bool,
    pub sprinting: bool,
    /// Eye position relative to the character's origin, in meters.
    pub eye_offset: Vec3,
    /// The collision box relative to the character's origin (engine
    /// space, meters), when the movement has one (Source hulls); zero
    /// when it doesn't (the logic layer then uses the collider's bounds).
    pub hull_min: Vec3,
    pub hull_max: Vec3,
    /// The moving solid (`MovingSolid`) the character stands on, if any.
    /// Not sent to a predicting client (entity ids differ).
    #[serde(skip)]
    pub ground: Option<Entity>,
    /// Climbing a ladder (movements that have them).
    pub on_ladder: bool,
    /// The eye's roll from a view punch (radians; positive tilts the view
    /// clockwise, Source's roll): hard landings kick it.
    pub view_roll: f32,
}

/// A velocity that moves a character without being part of its own
/// velocity (Source base velocity: push triggers, leaving a moving
/// platform), engine space, m/s. Movement implementations that model it
/// read and consume it (specs/source/triggers.md, trigger_push).
#[derive(Component, Reflect, Default, Clone, Copy, Debug, Serialize, Deserialize)]
#[reflect(Component)]
pub struct BaseVelocity {
    pub velocity: Vec3,
    /// Set this tick by a push (Source: FL_BASEVELOCITY); the movement
    /// clears it at the start of its next tick.
    pub touched: bool,
    /// Taken off the ground by game logic (push, teleport) this tick: the
    /// movement must not snap it back down before it moves.
    pub unground: bool,
}

/// Per-character gravity multiplier (Source entity gravity, set by
/// trigger_gravity); 0 means normal.
#[derive(Component, Reflect, Clone, Copy, Debug)]
#[reflect(Component)]
pub struct EntityGravity(pub f32);

impl Default for EntityGravity {
    fn default() -> Self {
        Self(1.0)
    }
}

/// A solid that moves (doors, platforms): its brushes where they are this
/// tick (engine space) and its velocity (m/s), for movement that sweeps
/// brushes exactly. Kept up to date by whoever moves it (the logic layer).
#[derive(Component, Clone, Debug, Default)]
pub struct MovingSolid {
    pub brushes: Vec<MapBrush>,
    pub velocity: Vec3,
    /// Solid to characters right now (doors with "Passable", disabled
    /// func_brush: false).
    pub solid: bool,
}

// Serde: replicated to clients (`net`).
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[reflect(Component)]
pub struct Health {
    /// 1.0 is a standard player's full health (README normalization rules).
    pub current: f32,
    pub max: f32,
}

impl Default for Health {
    fn default() -> Self {
        Self { current: 1.0, max: 1.0 }
    }
}

/// A speed cap the character's equipment imposes (e.g. the held weapon),
/// meters per second. Movement implementations that model it read it.
#[derive(Component, Reflect, Clone, Copy, Debug, Serialize, Deserialize)]
#[reflect(Component)]
pub struct MaxSpeed(pub f32);

/// `chimera_noblock` (ours, a server setting sent to clients, off by
/// default as CS:S has nothing like it): 0 players block each other (CS:S),
/// 1 teammates pass through each other, 2 nobody blocks anybody.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NoBlock(pub u8);

impl NoBlock {
    /// Whether two characters on these teams block each other (None: not a
    /// player on a team, e.g. a hostage: always blocks).
    pub fn blocks(self, a: Option<u8>, b: Option<u8>) -> bool {
        match (self.0, a, b) {
            (_, None, _) | (_, _, None) | (0, ..) => true,
            (1, a, b) => a != b,
            _ => false,
        }
    }
}

/// Takes no damage (the `god` command).
#[derive(Component, Reflect, Default, Clone, Copy, Debug)]
#[reflect(Component)]
pub struct God;

/// Spawn protection's two windows, `SimClock` seconds (a minigame's,
/// `chimera_sim::minigame::runtime`; replicated to network clients on
/// their own clock by `chimera_net::minigame_net`), in shape Combat Arms'
/// (specs/combat_arms/game_modes.md S6) with the party's own look (user,
/// 2026-10-29): the body draws see-through on every machine
/// (`chimera_present`'s `render::tint::fade_protected`). Until
/// `bullets_until` the player isn't there for attacks (`passes_attacks`):
/// bullets, swings and thrown projectiles pass through and hit what is
/// behind, blasts leave it out; until `damage_until` no damage lands (the
/// rules hold `God` meanwhile). While either runs, a hit on the player
/// shows nothing on any machine (`hides_hits`): every delivery tells the
/// trace with no `hit` (no blood, puff, decal, impact sound, hit
/// indicator or name-tag flash: `chimera_sim::weapon::impact::
/// hide_protected`), so the server and every client agree.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct SpawnProtected {
    pub damage_until: f64,
    pub bullets_until: f64,
}

impl SpawnProtected {
    /// Whether a hit on the player at `now` shows nothing (either window).
    pub fn hides_hits(&self, now: f64) -> bool {
        now + 1e-9 < self.damage_until.max(self.bullets_until)
    }

    /// Whether attacks pass through the player at `now` (bullets,
    /// swings, projectiles; blasts leave it out): as if it weren't there.
    pub fn passes_attacks(&self, now: f64) -> bool {
        now + 1e-9 < self.bullets_until
    }

    /// Whether both windows are over at `now`.
    pub fn over(&self, now: f64) -> bool {
        !self.hides_hits(now)
    }
}

/// Flies through walls: the noclip movement's marker (`chimera_sim`'s
/// `movement::noclip`; Source's noclip move type). Not `God`: CS:S still
/// traces a noclipping player's hitboxes (bullets hit it and go on through
/// as through any body) and hands its damage on, but it takes none, from
/// anything (bullets, blasts, trigger_hurt, the world), nor loses armour;
/// falls count again once it walks (specs/cs_source/movement.md "Noclip
/// and spawning"). `DamageRules` refuses it.
#[derive(Component, Reflect, Default, Clone, Copy, Debug)]
#[reflect(Component)]
pub struct Noclip;

/// The characters whose body is a woman's (Combat Arms' female
/// skeleton), as the game that knows keeps them: what picks a gendered
/// line (the announcer's groin-shot voice). A resource, not a marker, so
/// a body's sex never moves its character to another archetype.
#[derive(Resource, Default, Clone, Debug, PartialEq, Eq)]
pub struct FemaleBodies(pub std::collections::BTreeSet<Entity>);

/// What a blast or a flash does to a character's hearing for a while
/// (Source: a player DSP preset): what they hear is mixed `mix` toward a
/// muffled copy (low-passed at `cutoff` Hz, times `wet_gain`), and a
/// ringing tone (`ring_hz`, amplitude `ring_gain` times the mix) plays.
/// Full for `hold` seconds, then fading out over `fade` (linearly, or
/// exponentially down to 1 % by its end).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HearingEffect {
    pub hold: f32,
    pub fade: f32,
    pub exponential: bool,
    pub mix: f32,
    pub cutoff: f32,
    pub wet_gain: f32,
    pub ring_hz: f32,
    pub ring_gain: f32,
}

impl HearingEffect {
    /// Strength (0-1) `t` seconds after it started.
    pub fn level(&self, t: f32) -> f32 {
        if t < 0.0 || t >= self.hold + self.fade {
            0.0
        } else if t <= self.hold {
            1.0
        } else {
            let x = (t - self.hold) / self.fade;
            if self.exponential {
                // e^-4.6 = 1 %: then cut.
                (-4.6 * x).exp()
            } else {
                1.0 - x
            }
        }
    }

    /// Seconds until it is over.
    pub fn length(&self) -> f32 {
        self.hold + self.fade
    }
}

/// A character's hearing is hit (`HearingEffect`): clients apply it to
/// what the local player hears.
#[derive(Message, Clone, Copy, Debug)]
pub struct Deafened {
    pub target: Entity,
    pub effect: HearingEffect,
}

/// A character's blindness (a flashbang): the screen goes white at
/// `alpha` (0-1), holds until `fade_start` (seconds, the simulation
/// clock) and fades out linearly by `end`. Clients draw it; bots don't
/// see while it is strong.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Blinded {
    pub alpha: f32,
    pub fade_start: f64,
    pub end: f64,
}

impl Blinded {
    /// Whiteness at `now`.
    pub fn alpha_at(&self, now: f64) -> f32 {
        if now >= self.end {
            0.0
        } else if now <= self.fade_start || self.end <= self.fade_start {
            self.alpha
        } else {
            self.alpha * ((self.end - now) / (self.end - self.fade_start)) as f32
        }
    }

    /// `alpha_at` as a plain function (a default fade curve).
    pub fn alpha_at_now(b: &Blinded, now: f64) -> f32 {
        b.alpha_at(now)
    }

    /// The stronger of this blindness and `other` (a new flash only
    /// raises what is left of the current one).
    pub fn raised(self, other: Blinded, now: f64) -> Blinded {
        if self.alpha_at(now) >= other.alpha && self.end >= other.end {
            self
        } else if other.alpha >= self.alpha_at(now) && other.end >= self.end {
            other
        } else {
            // One is whiter, the other lasts longer: keep both maxima.
            Blinded {
                alpha: self.alpha_at(now).max(other.alpha),
                fade_start: self.fade_start.max(other.fade_start),
                end: self.end.max(other.end),
            }
        }
    }
}

/// Something sight doesn't pass (a smoke cloud): a sphere. A sight line
/// is blocked when more than `radius` of it lies inside (Counter-Strike
/// bots don't see through smoke; the exact rule isn't known, see
/// specs/cs_source/grenades.md Q16).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct SightBlocker {
    pub centre: Vec3,
    pub radius: f32,
}

impl SightBlocker {
    /// Length of the segment `from`-`to` inside the sphere.
    pub fn chord(&self, from: Vec3, to: Vec3) -> f32 {
        let d = to - from;
        let len = d.length();
        if len < 1e-6 || self.radius <= 0.0 {
            return 0.0;
        }
        let dir = d / len;
        let t = (self.centre - from).dot(dir);
        let closest = from + dir * t;
        let h2 = self.radius * self.radius - closest.distance_squared(self.centre);
        if h2 <= 0.0 {
            return 0.0;
        }
        let h = h2.sqrt();
        ((t + h).min(len) - (t - h).max(0.0)).max(0.0)
    }

    pub fn blocks(&self, from: Vec3, to: Vec3) -> bool {
        self.chord(from, to) > self.radius
    }
}
