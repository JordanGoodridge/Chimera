//! How the sound volumes scale gain (specs/combat_arms/options.md S2).
//! The effects volume `G` (0 to 100) is applied twice: as the sound
//! driver's master level and inside every sound's own level, so
//! loudness falls roughly with `(G/100)^2`. Music is a separate stream.
//!
//! Known shortcuts: whether a level above 1.0 amplifies in the sound
//! library is the spec's Open question 9; the levels here are numbers a
//! mixer reads (`CaSoundLevels`), and `ui::options` passes the master
//! and the music level on clamped to 0..1 (Chimera's mixer takes no
//! gain above 1).

/// A sound's class (S2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Weapons,
    Speech,
    Default,
}

/// The class multipliers, from the profile's hidden `[Sound]` values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Multipliers {
    pub weapons: f32,
    pub speech: f32,
    pub default: f32,
}

impl Default for Multipliers {
    fn default() -> Self {
        Self {
            weapons: 1.0,
            speech: 1.0,
            default: 0.75,
        }
    }
}

impl Multipliers {
    pub fn of(&self, class: Class) -> f32 {
        match class {
            Class::Weapons => self.weapons,
            Class::Speech => self.speech,
            Class::Default => self.default,
        }
    }
}

/// The effects master level: `G/100` with effects on (`G` clamped to
/// 0..100 after conversion to an unsigned 16-bit integer, so a negative
/// stored value becomes 100), 0 with effects off.
pub fn master(effects_on: bool, stored: i32) -> f32 {
    if !effects_on {
        return 0.0;
    }
    ((stored as u16).min(100)) as f32 / 100.0
}

/// A sound's own level: `v = floor(r c g)`, `v' = floor(127 v / 100)`,
/// level `v'/100`; `g = G/100` for the weapons and default classes and 1
/// for speech. `g` is single precision and the product is taken in double
/// precision before the truncation, which is how 100 x 0.7 ends at 69
/// (Quirk 6).
pub fn level(r: f32, class: Class, stored: i32, m: &Multipliers) -> f32 {
    let g = if class == Class::Speech {
        1.0
    } else {
        (stored as u16).min(100) as f32 / 100.0
    };
    let v = (r as f64 * m.of(class) as f64 * g as f64).floor();
    ((127.0 * v / 100.0).floor() / 100.0) as f32
}

/// The gain heard: the master times the level.
pub fn heard(effects_on: bool, stored: i32, r: f32, class: Class, m: &Multipliers) -> f32 {
    master(effects_on, stored) * level(r, class, stored, m)
}

/// The music stream's master level: `M/100` with music on, 0 off. The
/// number is passed on as stored in the profile (a value over 100 gives a
/// level over 1, Quirk 7).
pub fn music_level(music_on: bool, stored: i32) -> f32 {
    if music_on { stored as f32 / 100.0 } else { 0.0 }
}

/// The music volume as stored: clamped to 0..100.
pub fn music_stored(v: i32) -> i32 {
    v.clamp(0, 100)
}

/// What a mixer reads, kept as a resource by the options.
#[derive(bevy::prelude::Resource, Clone, Copy, Debug, PartialEq)]
pub struct CaSoundLevels {
    pub master: f32,
    pub music: f32,
    pub multipliers: Multipliers,
    /// The effects volume as stored (the sounds' own levels use it).
    pub stored: i32,
    pub mic: i32,
    pub speaker: i32,
}

impl Default for CaSoundLevels {
    fn default() -> Self {
        Self {
            master: 0.0,
            music: 0.0,
            multipliers: Multipliers::default(),
            stored: 0,
            mic: 0,
            speaker: 0,
        }
    }
}

impl CaSoundLevels {
    /// A sound of data volume `r` and `class`: its heard gain.
    pub fn heard(&self, r: f32, class: Class) -> f32 {
        self.master * level(r, class, self.stored, &self.multipliers)
    }
}
