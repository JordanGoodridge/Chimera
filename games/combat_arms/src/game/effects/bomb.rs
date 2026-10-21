//! The mission bomb on every machine (specs/combat_arms/game_modes.md M3
//! "The planted bomb on the clients", S15; effects_sounds.md 9 and case
//! 57): what each client hears and sees of the planted bomb, from the
//! objectives every machine has (`rules::objectives::CaObjectives`).
//!
//! - Planted: the interface sound `UI_SND\BombPlanted.wav`, the same for
//!   both teams; then one interface beep each time the whole-second count
//!   of the time left changes, the file by the time left at that moment
//!   (`beep`: above 30 s `T9COUNT1`, above 20 s `T9COUNT2`, above 10 s
//!   `T9COUNT3`, above 5 s `T9COUNT4`, above 0 `T9COUNT5`).
//! - Defused: the site's "defused" sound (`GUNS_SND_ETC\T9\DEFUSED.WAV`,
//!   radius 500).
//! - Detonated (`CaObjectives::detonation`): the site's explosion show:
//!   six effects at the site's centre at about 0.4, 0.5, 2.7, 2.8, 5.2 and
//!   5.3 s (`SHOW`: `Explode_S` and `Explode_L` in turn); nobody is hurt.
//!
//! Known shortcuts: the show is S15's timings at the site's centre (the
//! map's own explosion trigger, its effect objects and the camera points
//! aren't run); the map camera doesn't take the views; the defusing loop
//! isn't played; the forced dynamic lights (Quirk 12) aren't.

use bevy::prelude::*;

use super::{CaFxRequest, FxKind, sounds::CaSound};
use super::super::rules::objectives::CaObjectives;
use crate::core::SimClock;

/// The planted sound and the countdown's beeps (`BombTimerSound1..5`).
pub const PLANTED: &str = "UI_SND\\BombPlanted.wav";
pub const DEFUSED: &str = "GUNS_SND_ETC\\T9\\DEFUSED.WAV";
pub const DEFUSED_RADIUS: f32 = 500.0;

/// The explosion show: (seconds after the detonation, effect group).
pub const SHOW: [(f64, &str); 6] = [
    (0.4, "Explode_S"),
    (0.5, "Explode_L"),
    (2.7, "Explode_S"),
    (2.8, "Explode_L"),
    (5.2, "Explode_S"),
    (5.3, "Explode_L"),
];

/// The beep for the whole-second count changing with `left` seconds left
/// (case 67: 25.99 s → `T9COUNT2`, 5.99 → `T9COUNT4`, 4.99 → `T9COUNT5`).
pub fn beep(left: f64) -> Option<&'static str> {
    Some(if left > 30.0 {
        "UI_SND\\T9COUNT1.wav"
    } else if left > 20.0 {
        "UI_SND\\T9COUNT2.wav"
    } else if left > 10.0 {
        "UI_SND\\T9COUNT3.wav"
    } else if left > 5.0 {
        "UI_SND\\T9COUNT4.wav"
    } else if left > 0.0 {
        "UI_SND\\T9COUNT5.wav"
    } else {
        return None;
    })
}

/// What this machine last saw of the bomb.
#[derive(Default)]
pub struct BombSeen {
    /// The planted bomb's detonation time, and the whole seconds left at
    /// the last update.
    planted: Option<(f64, i64)>,
    defused: Option<Vec3>,
    /// The detonation shown and how many of the show's effects started.
    show: Option<(f64, usize)>,
}

/// Each frame: the planted sound, the beeps, the defuse sound, the show.
pub fn bomb_effects(
    objectives: Option<Res<CaObjectives>>,
    clock: Res<SimClock>,
    mut seen: Local<BombSeen>,
    mut sounds: MessageWriter<CaSound>,
    mut fx: MessageWriter<CaFxRequest>,
) {
    let Some(o) = objectives else { return };
    let now = clock.now;
    match o.planted {
        Some(p) => {
            let whole = (p.detonates - now).max(0.0).ceil() as i64;
            match seen.planted {
                None => {
                    sounds.write(CaSound::local(PLANTED));
                }
                Some((_, last)) if whole != last => {
                    if let Some(file) = beep(p.detonates - now) {
                        sounds.write(CaSound::local(file));
                    }
                }
                _ => {}
            }
            seen.planted = Some((p.detonates, whole));
        }
        None => seen.planted = None,
    }
    if o.defused != seen.defused {
        if let Some(at) = o.defused {
            sounds.write(CaSound::at(DEFUSED, at, DEFUSED_RADIUS));
        }
        seen.defused = o.defused;
    }
    match o.detonation {
        Some((site, when)) => {
            let (_, done) = seen.show.filter(|(t, _)| *t == when).unwrap_or((when, 0));
            let mut started = done;
            for (i, (t, group)) in SHOW.iter().enumerate().skip(done) {
                if now + 1e-9 < when + t {
                    break;
                }
                fx.write(CaFxRequest {
                    group: group.to_string(),
                    at: site,
                    normal: Vec3::Y,
                    kind: FxKind::Grenade,
                });
                started = i + 1;
            }
            seen.show = Some((when, started));
        }
        None => seen.show = None,
    }
}
