//! The mouse (specs/combat_arms/options.md M2, M3): the sensitivity's
//! axis scale, what a frame's mouse counts turn the view by, the legacy
//! smoothing path, and the in-match sensitivity keys. Arithmetic only;
//! `ui::options` hands the result to the shell's mouse look.

use std::f64::consts::TAU;

/// Axis scale at sensitivity 0, rad per count.
pub const SENS_BASE: f64 = 0.00001;
/// Added per sensitivity point.
pub const SENS_STEP: f64 = 0.000225;
/// When the profile has no `Sensitivity`.
pub const SENS_DEFAULT: i32 = 5;
/// The in-match keys' limits.
pub const SENS_KEY_MIN: i32 = 1;
pub const SENS_KEY_MAX: i32 = 99;
/// The stock (no smoothing) path's count multiplier.
pub const NO_SMOOTHING_GAIN: f64 = 3.0;
/// The final factor on mouse yaw and pitch.
pub const LOOK_HALF: f64 = 0.5;
/// `InputRate`, used only by the legacy smoothing path, ms.
pub const INPUT_RATE: f64 = 12.0;
/// How long the chat line after an in-match change shows, ms.
pub const SENS_MESSAGE_TIME: f64 = 5000.0;

/// `k(S)`: the engine's scale on both mouse axes, rad per count.
pub fn axis_scale(sensitivity: f64) -> f64 {
    SENS_BASE + SENS_STEP * sensitivity
}

/// The zoom divisor `1 + 3L` for zoom level `L` (0, 1, 2).
pub fn zoom_divisor(level: u8) -> f64 {
    1.0 + 3.0 * level as f64
}

/// The turn of one frame on the stock path: the frame's counts
/// `(right, down)` become `a = 3 k c`, and the view turns by
/// `0.5 a / (1 + 3L)` yaw (right is positive) and `0.5 a / (1 + 3L)`
/// pitch (down is positive; `invert` flips its sign). A full turn or more
/// in one frame is reduced modulo 2 pi. All zero while a popup blocks
/// the look (`blocked`).
pub fn view_delta(counts: (f64, f64), sensitivity: f64, level: u8, invert: bool, blocked: bool) -> (f64, f64) {
    if blocked {
        return (0.0, 0.0);
    }
    let k = axis_scale(sensitivity);
    let d = zoom_divisor(level);
    let yaw = LOOK_HALF * NO_SMOOTHING_GAIN * k * counts.0 / d;
    let pitch = LOOK_HALF * NO_SMOOTHING_GAIN * k * counts.1 / d;
    let wrap = |v: f64| if v.abs() >= TAU { v % TAU } else { v };
    (wrap(yaw), wrap(if invert { -pitch } else { pitch }))
}

/// Counts for a full turn at unzoomed sensitivity `s` (test case 3).
pub fn counts_per_turn(sensitivity: f64) -> f64 {
    TAU / (1.5 * axis_scale(sensitivity))
}

/// The legacy smoothing path (`MouseSmoothingEnable` 1) for one axis:
/// the newest three samples `s0`, `s1`, `s2` give
/// `s0 + (InputRate/100) ((s0 + s1 + s2)/3 - s0)`.
pub fn smooth(samples: [f64; 3]) -> f64 {
    let [s0, s1, s2] = samples;
    s0 + (INPUT_RATE / 100.0) * ((s0 + s1 + s2) / 3.0 - s0)
}

/// M3: `SensitivityInc` and `SensitivityDec`: S moves by one, staying
/// within 1 to 99 (a slider value of 0 or 100 stays when a key can't move
/// it further).
pub fn sensitivity_key(s: i32, inc: bool) -> i32 {
    if inc {
        if s + 1 < 100 { s + 1 } else { s }
    } else if s - 1 > 0 {
        s - 1
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scale_and_a_full_turn() {
        assert!((axis_scale(5.0) - 0.001135).abs() < 1e-12);
        assert!((counts_per_turn(5.0) - 3690.6).abs() < 0.1);
    }

    #[test]
    fn keys_stop_at_the_ends() {
        assert_eq!(sensitivity_key(99, true), 99);
        assert_eq!(sensitivity_key(1, false), 1);
        assert_eq!(sensitivity_key(5, true), 6);
    }
}
