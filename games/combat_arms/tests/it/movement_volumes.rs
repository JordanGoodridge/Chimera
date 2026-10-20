//! Combat Arms movement in volumes (specs/combat_arms/movement.md, "Other
//! modifiers", "Speed limit", "Jumping", "Gravity and air movement",
//! "Crouching", "Stopping", "Falling and fall damage"; the volume classes
//! of world_dat.md "Moving world models and volumes"): ladders, liquids,
//! viscosity, currents, friction and speed factors, gravity volumes, jump
//! pads, safety nets and explosion pushers, as scenario tests on `CaSim`
//! with `movement`'s test map and player (`movement::T`). The numbered
//! spec cases that touch them are named `case_<n>_...`; the rest check the
//! prose's rules with numbers worked out from the spec's formulas.
//!
//! Volumes are placed as boxes of their class with the class defaults
//! (`CaVolume::class_box`) and the properties a test sets. The stock
//! water of the maps (grenades.md section 7: viscosity 0.1, current
//! (0, 1500, 0) u/s) is `stock_water`.

use bevy::prelude::*;
use chimera_core::core::{DamageKind, SimClock, Velocity};
use combat_arms::game::{
    movement::{CaPusher, movement_data},
    volumes::CaVolume,
};

use crate::movement::{T, TOL, block, close, floor, on};

/// Engine meters per world unit.
fn s() -> f32 {
    movement_data().meters_per_unit
}

/// A volume of `class` from `min` to `max` (u), `with` changing it.
fn volume(t: &mut T, class: &str, min: Vec3, max: Vec3, with: impl FnOnce(&mut CaVolume)) {
    let mut v = CaVolume::class_box(class, min * s(), max * s()).expect(class);
    with(&mut v);
    t.sim.app.world_mut().spawn(v);
}

/// The maps' usual water: viscosity 0.1, current (0, 1500, 0).
fn stock_water(v: &mut CaVolume) {
    v.viscosity = 0.1;
    v.current = Vec3::new(0.0, 1500.0, 0.0);
}

fn set_vel(t: &mut T, v: Vec3) {
    t.sim.app.world_mut().get_mut::<Velocity>(t.p).unwrap().0 = v * s();
}

fn now(t: &T) -> f64 {
    t.sim.app.world().resource::<SimClock>().now
}

/// A wide box around the origin from height `y0` to `y1` (u).
fn slab(y0: f32, y1: f32) -> (Vec3, Vec3) {
    (Vec3::new(-5000.0, y0, -5000.0), Vec3::new(5000.0, y1, 5000.0))
}

const DT: f32 = 0.015;
/// e^(-k dt) with k = 5.
const E: f32 = 0.927_743_5;

// --- Ladders ---

/// Case 29 (ladder part): with a full gauge and sprint held, on a ladder:
/// no sprint.
#[test]
fn case_29_sprint_blocked_on_a_ladder() {
    let mut t = T::standing();
    t.insert(movement_data().character.clone());
    let (lo, hi) = slab(0.0, 1000.0);
    volume(&mut t, "Ladder", lo, hi, |_| {});
    t.forward(true);
    t.keys().sprint = true;
    t.ticks(5);
    assert!(t.mv().on_ladder, "on the ladder");
    assert!(!t.mv().sprint, "no sprint on a ladder");
    assert!(t.sim.state(t.p).on_ladder);
}

/// Case 31 (ladder and safety-net part): a landing from 400 u does 50
/// (case 31), but none on a ladder or in a safety-net volume.
#[test]
fn case_31_no_fall_damage_on_a_ladder_or_in_a_safety_net() {
    // A ladder from the floor up: looking down and holding forward the
    // player comes down it fast (no gravity, the viscosity only), lands,
    // and takes nothing.
    let mut t = T::standing();
    let (lo, hi) = slab(0.0, 1000.0);
    volume(&mut t, "Ladder", lo, hi, |_| {});
    t.place_in_air(on(0.0, 400.0, 0.0));
    t.keys().pitch = -std::f32::consts::FRAC_PI_2;
    t.forward(true);
    t.ticks(200);
    assert!(t.feet() < 1.0, "came down the ladder: feet at {}", t.feet());
    assert!(t.damage().is_empty(), "{:?}", t.damage());
    // A safety net at the floor: landing from 400 u in it does nothing.
    let mut t = T::standing();
    let (lo, hi) = slab(0.0, 100.0);
    volume(&mut t, "SafteyNet", lo, hi, |_| {});
    t.place_in_air(on(0.0, 400.0, 0.0));
    t.ticks(100);
    assert!(t.mv().on_ground);
    assert!(t.damage().is_empty(), "{:?}", t.damage());
}

/// On a ladder: gravity off (a player at rest in it stays put), the speed
/// limit is the ladder's 200, the crouch rate is held at 0 and the jump
/// key does nothing.
#[test]
fn ladder_holds_the_player() {
    let mut t = T::standing();
    let (lo, hi) = slab(0.0, 1000.0);
    volume(&mut t, "Ladder", lo, hi, |_| {});
    t.place_in_air(on(0.0, 300.0, 0.0));
    let before = t.pos();
    t.ticks(50);
    assert!((t.pos() - before).length() < 1e-3, "{} -> {}", before, t.pos());
    assert!(t.mv().on_ladder && t.mv().standing.is_none());
    close(t.mv().speed_limit, 200.0, TOL, "ladder speed limit");
    // Crouch held: the rate stays 0 (and the hull standing).
    t.keys().crouch = true;
    t.ticks(10);
    assert_eq!(t.mv().crouch_rate, 0.0);
    close(t.mv().half_height, 50.0, TOL, "standing hull");
    // On the floor in the ladder: the jump key doesn't jump.
    let mut t = T::standing();
    volume(&mut t, "Ladder", lo, hi, |_| {});
    t.tick();
    t.keys().jump = true;
    t.ticks(3);
    assert!(!t.mv().in_jump && t.vel().y.abs() < 1e-3, "no jump: {}", t.vel());
}

/// Climbing: looking straight up and holding forward the acceleration
/// (3000, ramped from 500) is along the view; the viscosity (0.4 x 5000 =
/// 2000 u/s^2: 30 u/s a tick) holds it back until the ramp passes it. The
/// upward speed is capped at 200 at each tick's clamp, then the tick adds
/// 45: steady, 245 u/s at the end of each tick and 200 x dt + 3000 dt^2 / 4
/// = 3.16875 u up per tick (no friction off the ground).
#[test]
fn ladder_climb() {
    let mut t = T::standing();
    let (lo, hi) = slab(0.0, 5000.0);
    volume(&mut t, "Ladder", lo, hi, |_| {});
    t.place_in_air(on(0.0, 100.0, 0.0));
    t.keys().pitch = std::f32::consts::FRAC_PI_2;
    t.forward(true);
    t.tick();
    // Tick 1: the start acceleration, 500 (7.5 u/s), nothing to slow.
    close(t.mv().accel.length(), 500.0, TOL, "start accel");
    close(t.vel().y, 7.5, TOL, "speed after tick 1");
    t.ticks(80);
    let before = t.pos();
    t.tick();
    close(t.vel().y, 245.0, TOL, "end speed");
    close(t.pos().y - before.y, 3.16875, TOL, "climb per tick");
    close(t.vel().xz().length(), 0.0, TOL, "no horizontal speed");
}

/// Leaving the ladder at the top: climbing a ladder in front of a 300 u
/// ledge, looking up and toward it, the player climbs past the ledge's
/// edge and ends standing on it (the stair-step takes the last bit).
#[test]
fn ladder_dismount_at_the_top() {
    // The ledge: from z = -1000 to z = -20, top at 300.
    let ledge = block(Vec3::new(0.0, 150.0, -510.0), Vec3::new(500.0, 150.0, 490.0));
    let mut t = T::new(vec![floor(), ledge], on(0.0, 0.0, 0.0));
    // The ladder: in front of the ledge's face, up to 320.
    volume(&mut t, "Ladder", Vec3::new(-50.0, 0.0, -20.0), Vec3::new(50.0, 320.0, 20.0), |_| {});
    t.place_in_air(Vec3::new(0.0, 80.0, -4.0));
    t.keys().pitch = 1.2;
    t.forward(true);
    // Climbing, then off the ladder and on the ledge: keys released.
    let mut ticks = 0;
    while t.mv().on_ladder || !t.mv().on_ground {
        t.tick();
        ticks += 1;
        assert!(ticks < 400, "still climbing at {}", t.pos());
    }
    t.release_all();
    t.ticks(30);
    assert!(!t.mv().on_ladder);
    assert!(t.mv().on_ground, "standing: {:?} at {}", t.mv().standing, t.pos());
    close(t.feet(), 300.0, 0.5, "on the ledge");
    assert!(t.pos().z < -20.0, "over the ledge: {}", t.pos());
}

// --- Liquids ---

/// The maps' water lifts a player: in it at rest, the viscosity (0.1 x
/// 5000 = 500 u/s^2: 7.5 u/s a tick) first stops what speed there is,
/// then the current adds 1500 x dt = 22.5 u/s up and gravity takes 16.5:
/// 6 u/s at the end of each tick and 22.5 dt - 1100 dt^2 / 4 = 0.275625 u
/// up per tick. At the top the centre leaves the water, which stops the
/// player; it falls back in and bobs there.
#[test]
fn water_lifts_the_player() {
    let mut t = T::standing();
    let (lo, hi) = slab(0.0, 1000.0);
    volume(&mut t, "Water", lo, hi, stock_water);
    t.place_in_air(Vec3::new(0.0, 900.0, 0.0));
    for tick in 1..=3 {
        let before = t.pos();
        t.tick();
        close(t.vel().y, 6.0, TOL, &format!("speed after tick {tick}"));
        close(t.pos().y - before.y, 0.275625, TOL, &format!("rise on tick {tick}"));
        assert!(t.mv().in_liquid);
    }
    // To the surface (100 u at 18.4 u/s) and bobbing there.
    t.ticks(500);
    for _ in 0..200 {
        t.tick();
        let y = t.pos().y;
        assert!(y > 997.0 && y < 1001.0, "bobbing at the surface: centre {y}");
    }
}

/// Leaving a liquid (its centre out of the water) zeroes the velocity:
/// still water, a player moving up and sideways through its top stops,
/// then falls (-16.5 u/s after that tick).
#[test]
fn leaving_water_stops_the_player() {
    let mut t = T::standing();
    let (lo, hi) = slab(0.0, 1000.0);
    volume(&mut t, "Water", lo, hi, |v| v.viscosity = 0.0);
    t.place_in_air(Vec3::new(0.0, 998.0, 0.0));
    set_vel(&mut t, Vec3::new(100.0, 300.0, 0.0));
    t.tick();
    assert!(t.pos().y > 1000.0 && t.mv().in_liquid, "out on this tick: {}", t.pos());
    t.tick();
    assert!(!t.mv().in_liquid);
    let v = t.vel();
    close(v.x, 0.0, TOL, "stopped");
    close(v.y, -16.5, TOL, "falling from rest");
}

/// Free-movement volumes (liquids) hold the crouch rate at 0.
#[test]
fn no_crouch_in_water() {
    let mut t = T::standing();
    let (lo, hi) = slab(0.0, 1000.0);
    volume(&mut t, "Water", lo, hi, |v| v.viscosity = 0.0);
    t.keys().crouch = true;
    t.ticks(10);
    assert!(t.mv().in_liquid);
    assert_eq!(t.mv().crouch_rate, 0.0);
}

/// The corrosive liquid burns: 5 HP per second, as burn damage.
#[test]
fn corrosive_fluid_burns() {
    let mut t = T::standing();
    let (lo, hi) = slab(0.0, 1000.0);
    volume(&mut t, "CorrosiveFluid", lo, hi, |v| v.viscosity = 0.0);
    t.ticks(10);
    let d = t.damage();
    assert!(!d.is_empty());
    for d in &d {
        assert_eq!(d.kind, DamageKind::Burn);
        close(d.amount * movement_data().health_points, 5.0 * DT, 1e-5, "burn per tick");
    }
}

// --- Viscosity, currents, friction, speed factors ---

/// Viscosity takes 5000 x c x dt off the speed, and stops the braking:
/// on the floor in a volume of viscosity 0.1, sliding at 200 u/s with no
/// key: 200 - 7.5 = 192.5, then the friction alone: 192.5 e^(-k dt) =
/// 178.5906 u/s, 192.5 (1 - e^(-k dt)) / k = 2.781876 u. (Without the
/// volume the braking would take 42.75 first.)
#[test]
fn viscosity_slows_and_stops_the_braking() {
    let mut t = T::standing();
    let (lo, hi) = slab(-10.0, 1000.0);
    volume(&mut t, "VolumeBrush", lo, hi, |v| v.viscosity = 0.1);
    t.tick();
    set_vel(&mut t, Vec3::new(200.0, 0.0, 0.0));
    let before = t.pos();
    t.tick();
    close(t.vel().x, 192.5 * E, TOL, "end speed");
    close(t.pos().x - before.x, 192.5 * (1.0 - E) / 5.0, TOL, "distance");
}

/// The tick after a jump skips the viscosity: jumping in a volume of
/// viscosity 0.1, the vertical speed after ticks 1-3 is 313.5 (case 13),
/// 297.0 (no viscosity) and 297 - 7.5 - 16.5 = 273.0.
#[test]
fn viscosity_skips_the_tick_after_a_jump() {
    let mut t = T::standing();
    let (lo, hi) = slab(-10.0, 1000.0);
    volume(&mut t, "VolumeBrush", lo, hi, |v| v.viscosity = 0.1);
    t.tick();
    t.keys().jump = true;
    for want in [313.5, 297.0, 273.0] {
        t.tick();
        close(t.vel().y, want, TOL, "vertical speed");
    }
}

/// A current adds current x dt to the velocity each tick and stops the
/// braking: standing in a (0-viscosity) volume with current (100, 0, 0):
/// 1.5 u/s, then the friction: 1.5 e^(-k dt) = 1.391615 u/s, moving
/// 1.5 (1 - e^(-k dt)) / k = 0.0216770 u.
#[test]
fn current_carries_the_player() {
    let mut t = T::standing();
    let (lo, hi) = slab(-10.0, 1000.0);
    volume(&mut t, "Wind", lo, hi, |v| v.current = Vec3::new(100.0, 0.0, 0.0));
    let before = t.pos();
    t.tick();
    close(t.vel().x, 1.5 * E, TOL, "end speed");
    close(t.pos().x - before.x, 1.5 * (1.0 - E) / 5.0, 1e-4, "distance");
}

/// `PlayerSpeedFactor` multiplies the speed limit (f_container): 0.5 in
/// a volume: run 142.5, and two such volumes 71.25.
#[test]
fn speed_factor_scales_the_limit() {
    let mut t = T::standing();
    let (lo, hi) = slab(-10.0, 1000.0);
    volume(&mut t, "VolumeBrush", lo, hi, |v| v.speed_factor = 0.5);
    t.steady_forward();
    close(t.mv().speed_limit, 142.5, TOL, "one volume");
    volume(&mut t, "VolumeBrush", lo, hi, |v| v.speed_factor = 0.5);
    t.tick();
    close(t.mv().speed_limit, 71.25, TOL, "two volumes");
}

/// Ice (friction 0): k = 5 x 0 = 0, no friction: a steady run on it ends
/// each tick at 285 + 3000 dt = 330 u/s and moves 285 dt + 3000 dt^2 / 2 =
/// 4.6125 u (the closed form's limit).
#[test]
fn ice_has_no_friction() {
    let mut t = T::standing();
    let (lo, hi) = slab(-10.0, 1000.0);
    volume(&mut t, "Ice", lo, hi, |_| {});
    t.steady_forward();
    let before = t.pos();
    t.tick();
    close(t.hspeed(), 330.0, TOL, "end speed");
    close(before.z - t.pos().z, 4.6125, TOL, "distance per tick");
}

// --- Gravity volumes, jump pads ---

/// A gravity volume replaces the player's gravity (-2000 by default):
/// at rest in the air, -30 u/s and -2000 dt^2 / 4 = -0.1125 u after a
/// tick; two of them add up (-4000: -60 u/s).
#[test]
fn gravity_volume_replaces_gravity() {
    let mut t = T::standing();
    let (lo, hi) = slab(0.0, 1000.0);
    volume(&mut t, "Gravity", lo, hi, |_| {});
    t.place_in_air(Vec3::new(0.0, 500.0, 0.0));
    let before = t.pos();
    t.tick();
    close(t.vel().y, -30.0, TOL, "speed");
    close(t.pos().y - before.y, -0.1125, TOL, "drop");
    volume(&mut t, "Gravity", lo, hi, |_| {});
    t.place_in_air(Vec3::new(0.0, 500.0, 0.0));
    t.tick();
    close(t.vel().y, -60.0, TOL, "two volumes");
}

/// A jump in a jump pad (`JumpVolume`) adds the pad's `Velocity` to the
/// whole velocity instead of setting the jump speed: with (0, 800, 0),
/// 800 - 16.5 = 783.5 u/s after the jump tick, 800 dt - 1100 dt^2 / 4 =
/// 11.938125 u up; it is a jump (jump factor 0.3).
#[test]
fn jump_pad_launches() {
    let mut t = T::standing();
    let (lo, hi) = slab(-10.0, 1000.0);
    volume(&mut t, "JumpVolume", lo, hi, |v| v.jump_velocity = Some(Vec3::new(0.0, 800.0, 0.0)));
    t.tick();
    t.keys().jump = true;
    let before = t.pos();
    t.tick();
    close(t.vel().y, 783.5, TOL, "speed");
    close(t.pos().y - before.y, 11.938125, TOL, "rise");
    assert!(t.mv().in_jump);
    close(t.mv().jump_factor, 0.3, TOL, "jump factor");
}

// --- Explosion pushers ---

fn pusher(t: &mut T, origin: Vec3, radius: f32, strength: f32) {
    let at = now(t) + DT as f64;
    t.sim.app.world_mut().spawn(CaPusher {
        origin: origin * s(),
        radius,
        strength,
        at,
    });
}

/// On the ground a pusher 100 u away (radius 200, strength 400) adds
/// 400 x (1 - 100/200) = 200 u/s away from it, after the braking; the
/// friction then: 200 e^(-k dt) = 185.5487 u/s, 200 (1 - e^(-k dt)) / k =
/// 2.890261 u. Only on its tick.
#[test]
fn pusher_on_the_ground() {
    let mut t = T::standing();
    let at = t.pos();
    pusher(&mut t, at - Vec3::new(100.0, 0.0, 0.0), 200.0, 400.0);
    t.tick();
    close(t.vel().x, 200.0 * E, TOL, "pushed");
    close(t.pos().x - at.x, 200.0 * (1.0 - E) / 5.0, TOL, "distance");
    t.tick();
    assert!(t.vel().x < 200.0 * E * E, "pushed once: {}", t.vel());
}

/// In the air the push is a tenth, and the push lifts the upward speed to
/// at most 100: a pusher 100 u below (radius 200, strength 4000) gives
/// 200 up, a tenth 200 ... capped at 100, then gravity: 83.5 u/s. From
/// the side, 4000 x 0.5 x 0.1 = 200 u/s.
#[test]
fn pusher_in_the_air() {
    let mut t = T::standing();
    t.place_in_air(Vec3::new(0.0, 500.0, 0.0));
    pusher(&mut t, Vec3::new(0.0, 400.0, 0.0), 200.0, 4000.0);
    t.tick();
    close(t.vel().y, 100.0 - 16.5, TOL, "upward capped");
    let mut t = T::standing();
    t.place_in_air(Vec3::new(0.0, 500.0, 0.0));
    pusher(&mut t, Vec3::new(-100.0, 500.0, 0.0), 200.0, 4000.0);
    t.tick();
    close(t.vel().x, 200.0, TOL, "a tenth");
}

/// A wall between the pusher and the player: no push.
#[test]
fn pusher_needs_a_clear_line() {
    let wall = block(Vec3::new(-50.0, 100.0, 0.0), Vec3::new(5.0, 100.0, 100.0));
    let mut t = T::new(vec![floor(), wall], on(0.0, 0.0, 0.0));
    t.tick();
    let at = t.pos();
    pusher(&mut t, at - Vec3::new(100.0, 0.0, 0.0), 200.0, 400.0);
    t.tick();
    close(t.vel().x, 0.0, TOL, "not pushed");
}

/// A map's start-up command line: the `msg <name> on|off` switches that
/// run at once, in order; delayed ones and other messages are not.
#[test]
fn startup_switches() {
    let line = "msg Collision_Box01 (solid 0);\r\nmsg Box_Kill01 off;\r\nmsg Sub_Box_Kill01 OFF;\r\n\
                delay 7 (msg Box_Kill01 on);msg Lift on";
    assert_eq!(
        combat_arms::game::volumes::startup_switches(line),
        [("Box_Kill01".to_string(), false), ("Sub_Box_Kill01".to_string(), false), ("Lift".to_string(), true)]
    );
}
