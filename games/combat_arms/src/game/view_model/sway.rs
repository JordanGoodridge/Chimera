//! The weapon sway (specs/combat_arms/view_models.md 6): a small
//! figure-eight of the view model while walking or running, the only
//! motion code adds to it (no camera bob or roll, no turn lag). Plain
//! data: `Sway::step` once per rendered frame, before the weapon update.

use std::f64::consts::{PI, TAU};

/// `BobSwayRunPaceAdjust`, `BobSwayWalkPaceAdjust` (cycles/s).
pub const PACE_RUN: f64 = 1.4;
pub const PACE_WALK: f64 = 0.6;
/// `BobDecayTime` (s): ramp-up and decay time of the amplitude.
pub const BOB_RAMP: f64 = 0.1;
/// `MaxBobAmp`.
pub const AMP_MAX: f64 = 10.0;
/// `SwayH`, `SwayV` (units per amplitude).
pub const SWAY_H: f64 = 0.002;
pub const SWAY_V: f64 = 0.0015;
/// The sideways sway's fixed extra factor and phase lag.
pub const SWAY_H_HALVING: f64 = 0.5;
pub const SWAY_H_PHASE: f64 = PI / 3.0;
/// Travel per frame (units) needed to count as moving.
pub const MOVE_THRESHOLD: f64 = 0.1;
/// The game's frame time clamp (s).
pub const FRAME_TIME_CAP: f64 = 0.1;

/// One frame's inputs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SwayInputs {
    /// Frame time (s; clamped to `FRAME_TIME_CAP` here) and client clock (s).
    pub dt: f64,
    pub clock: f64,
    /// The sway runs this frame (first person, can do footsteps, movement
    /// allowed; or spectating in first person). False freezes every value.
    pub active: bool,
    /// The run flag (off walking or crouched) and the moving flag (a
    /// movement key or jump held).
    pub run: bool,
    pub moving: bool,
    /// The player's speed (units/s).
    pub speed: f64,
    /// Spectating in first person: the distance test always passes.
    pub spectating: bool,
    /// A weapon in hand and not zoomed: the offsets are written.
    pub weapon: bool,
    pub zoomed: bool,
    /// `WeaponSway` (the profile option / 10).
    pub scale: f64,
}

/// The sway's state: phase, amplitude, ramp and decay start times, and the
/// offsets last written (units, camera axes).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sway {
    pub phase: f64,
    pub amp: f64,
    ramp_start: Option<f64>,
    decay_start: Option<f64>,
    /// Sideways (w) and vertical (h) offsets.
    pub w: f64,
    pub h: f64,
}

/// S(x) = (1 - cos pi x) / 2.
fn s(x: f64) -> f64 {
    (1.0 - (PI * x).cos()) / 2.0
}

fn on(x: f64) -> f64 {
    if x < 0.5 { s(x) } else { x }
}

fn off(x: f64) -> f64 {
    if x < 0.5 { x } else { s(x) }
}

impl Sway {
    /// One frame (6, "Steps").
    pub fn step(&mut self, i: &SwayInputs) {
        if !i.active {
            return;
        }
        let dt = i.dt.min(FRAME_TIME_CAP);
        let pace = TAU * if i.run { PACE_RUN } else { PACE_WALK };
        let moving = i.moving && (i.spectating || i.speed * dt > MOVE_THRESHOLD);
        // 1. Phase.
        if self.amp == 0.0 {
            self.phase = 0.0;
        } else {
            self.phase += pace * dt;
            if self.phase > TAU {
                self.phase -= TAU;
            }
        }
        // 3. Amplitude: start times back-dated from the current amplitude.
        let t = i.clock;
        if moving {
            let start = *self.ramp_start.get_or_insert(t - BOB_RAMP * self.amp / AMP_MAX);
            self.decay_start = None;
            let d = t - start;
            self.amp = if d <= BOB_RAMP { AMP_MAX * on(d / BOB_RAMP) } else { AMP_MAX }.min(AMP_MAX);
        } else {
            let start = *self.decay_start.get_or_insert(t - BOB_RAMP * (1.0 - self.amp / AMP_MAX));
            self.ramp_start = None;
            let d = t - start;
            self.amp = if d <= BOB_RAMP { AMP_MAX * (1.0 - off(d / BOB_RAMP)) } else { 0.0 }.max(0.0);
        }
        // 4. Offsets.
        if i.weapon && !i.zoomed {
            self.h = SWAY_V * self.amp * (2.0 * self.phase).sin() * i.scale;
            self.w = SWAY_H_HALVING * SWAY_H * self.amp * (self.phase - SWAY_H_PHASE).sin() * i.scale;
        }
    }
}
