//! How a thrown grenade turns in flight and how a client draws that turn
//! (specs/combat_arms/grenades.md, Corrections (2026-10-27) C2).
//!
//! - `Tumble`: pitch, yaw and roll accumulated from 0 at the grenade's
//!   creation (never reset or wrapped, whatever the throw direction), each
//!   update adding rate times the server frame; the rates are drawn when
//!   the grenade is made and again at every bounce. The rotation is
//!   *set* from the three angles each update: `R = Ry(y) Rx(p) Rz(r)` in
//!   the game's left-handed frame (+X right, +Y up, +Z forward), here
//!   mirrored in Z (`orientation`) into Bevy's right-handed one the models
//!   are converted to. A grenade at rest keeps its last orientation.
//! - `RotationBlend`: a client does not snap a new server rotation. On
//!   receiving one it takes a blend time T of 1.1 times its measured time
//!   between server updates; each rendered frame of length d, with
//!   `tau = min(T, d)` and `p = tau / T`, the drawn rotation moves to
//!   `slerp(drawn, server, p / 2)` and T drops by tau; at 0 it is the
//!   server's exactly.

use bevy::prelude::*;

use crate::{
    map::{interp::NetDrawn, loose::ShownItem},
    net::{NetItem, interp::Snapshots},
};

/// The blend time, as a share of the time between server updates.
pub const BLEND_SHARE: f32 = 1.1;

/// A grenade's accumulated angles (pitch, yaw, roll; rad) and spin rates
/// (rad/s).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tumble {
    pub angles: Vec3,
    pub rates: Vec3,
}

impl Tumble {
    /// Angles 0 with these rates.
    pub fn new(rates: Vec3) -> Self {
        Self {
            angles: Vec3::ZERO,
            rates,
        }
    }

    /// One update of `dt` seconds: the angles move on by the rates; the
    /// rotation to set.
    pub fn step(&mut self, dt: f32) -> Quat {
        self.angles += self.rates * dt;
        orientation(self.angles)
    }
}

/// The rotation of angles `(p, y, r)`: columns are where the model's
/// +X, +Y and +Z axes point, `R = Ry(y) Rx(p) Rz(r)`, then mirrored in Z
/// (`M R M`, `M = diag(1, 1, -1)`).
pub fn orientation(angles: Vec3) -> Quat {
    let (sp, cp) = angles.x.sin_cos();
    let (sy, cy) = angles.y.sin_cos();
    let (sr, cr) = angles.z.sin_cos();
    let rx = Mat3::from_cols(Vec3::X, Vec3::new(0.0, cp, sp), Vec3::new(0.0, -sp, cp));
    let ry = Mat3::from_cols(Vec3::new(cy, 0.0, -sy), Vec3::Y, Vec3::new(sy, 0.0, cy));
    let rz = Mat3::from_cols(Vec3::new(cr, sr, 0.0), Vec3::new(-sr, cr, 0.0), Vec3::Z);
    let r = ry * rx * rz;
    let m = |v: Vec3| Vec3::new(v.x, v.y, -v.z);
    Quat::from_mat3(&Mat3::from_cols(m(r.x_axis), m(r.y_axis), -m(r.z_axis))).normalize()
}

/// Where the model's forward (+Z of the game's frame) and up axes point,
/// in the game's frame, for a rotation set by `orientation`.
pub fn game_axes(q: Quat) -> (Vec3, Vec3) {
    let m = |v: Vec3| Vec3::new(v.x, v.y, -v.z);
    (m(q * Vec3::NEG_Z), m(q * Vec3::Y))
}

/// A client's drawn rotation of a grenade, moving towards the server's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RotationBlend {
    pub drawn: Quat,
    pub target: Quat,
    /// Blend time left, s.
    pub left: f32,
}

impl RotationBlend {
    /// A new object: snapped to `q`.
    pub fn snapped(q: Quat) -> Self {
        Self {
            drawn: q,
            target: q,
            left: 0.0,
        }
    }

    /// A new rotation `q` from the server, `interval` s after the last.
    pub fn receive(&mut self, q: Quat, interval: f32) {
        self.target = q;
        self.left = BLEND_SHARE * interval;
    }

    /// One rendered frame of `dt` s; the rotation to draw.
    pub fn frame(&mut self, dt: f32) -> Quat {
        if self.left <= 0.0 {
            self.drawn = self.target;
            return self.drawn;
        }
        let tau = self.left.min(dt);
        let p = tau / self.left;
        self.drawn = self.drawn.slerp(self.target, p / 2.0);
        self.left -= tau;
        if self.left <= 0.0 {
            self.left = 0.0;
            self.drawn = self.target;
        }
        self.drawn
    }
}

/// A thrown grenade's blend on a client, with the server rotation it
/// last heard and when.
#[derive(Component, Clone, Copy, Debug)]
pub struct ThrownBlend {
    pub blend: RotationBlend,
    heard: [f32; 4],
    at: f64,
}

/// The time between server updates taken when only one has been heard, s.
pub const FIRST_INTERVAL: f32 = 0.05;
/// Longest time between updates the measure believes, s (a stall is not
/// the server's rate).
pub const MAX_INTERVAL: f32 = 0.25;

/// A client draws each grenade in flight at the rotation the blend gives
/// instead of the snapshots' (`RotationBlend`): the newest rotation the
/// server sent is the target, its measured spacing sets the blend time.
pub fn smooth(
    time: Res<Time<Real>>,
    mut commands: Commands,
    mut items: Query<
        (
            Entity,
            &Snapshots<NetItem>,
            &ShownItem,
            &mut Transform,
            Option<&mut ThrownBlend>,
        ),
        With<NetDrawn>,
    >,
) {
    let now = time.elapsed_secs_f64();
    for (e, snaps, shown, mut t, have) in &mut items {
        if !shown.0.ends_with(super::THROWN_SUFFIX) {
            continue;
        }
        let Some((_, newest)) = snaps.newest() else { continue };
        let target = Quat::from_array(newest.rotation).normalize();
        let Some(mut have) = have else {
            commands.entity(e).insert(ThrownBlend {
                blend: RotationBlend::snapped(target),
                heard: newest.rotation,
                at: now,
            });
            t.rotation = target;
            continue;
        };
        if have.heard != newest.rotation {
            let interval = ((now - have.at) as f32).clamp(0.0, MAX_INTERVAL);
            have.blend
                .receive(target, if interval > 0.0 { interval } else { FIRST_INTERVAL });
            have.heard = newest.rotation;
            have.at = now;
        }
        t.rotation = have.blend.frame(time.delta_secs());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI};

    fn near(a: Vec3, b: [f32; 3]) {
        assert!((a - Vec3::from(b)).abs().max_element() < 5e-4, "{a:?} vs {b:?}");
    }

    fn run(t: &mut Tumble, updates: usize) -> Quat {
        let mut q = Quat::IDENTITY;
        for _ in 0..updates {
            q = t.step(0.05);
        }
        q
    }

    // Case 68: rates (pi/2, pi, -pi).
    #[test]
    fn t68_tumble_numbers() {
        let mut t = Tumble::new(Vec3::new(FRAC_PI_2, PI, -PI));
        let q = run(&mut t, 1);
        near(t.angles, [0.0785, 0.1571, -0.1571]);
        let (f, u) = game_axes(q);
        near(f, [0.1560, -0.0785, 0.9846]);
        near(u, [0.1666, 0.9846, 0.0521]);
        let q = run(&mut t, 3);
        near(t.angles, [0.3142, 0.6283, -0.6283]);
        let (f, u) = game_axes(q);
        near(f, [0.5590, -0.3090, 0.7694]);
        near(u, [0.6225, 0.7694, -0.1432]);
        let q = run(&mut t, 16);
        let (f, u) = game_axes(q);
        near(f, [0.0, -1.0, 0.0]);
        near(u, [0.0, 0.0, 1.0]);
    }

    // Case 69: a bounce changes the rates only.
    #[test]
    fn t69_a_bounce_keeps_the_angles() {
        let mut t = Tumble::new(Vec3::new(FRAC_PI_2, PI, -PI));
        run(&mut t, 4);
        t.rates = Vec3::new(0.0, std::f32::consts::TAU, 0.0);
        let q = run(&mut t, 2);
        near(t.angles, [0.3142, 1.2566, -0.6283]);
        near(game_axes(q).0, [0.9045, -0.3090, 0.2939]);
    }

    #[test]
    fn the_rotation_is_set_from_the_angles() {
        let mut a = Tumble::new(Vec3::new(1.0, 2.0, 3.0));
        let q = run(&mut a, 7);
        near(a.angles, [0.35, 0.7, 1.05]);
        assert!(q.dot(orientation(a.angles)).abs() > 0.99999);
    }

    // Case 71: 60 fps, updates 50 ms apart, 30 degrees off.
    #[test]
    fn t71_client_smoothing_numbers() {
        let server = Quat::from_rotation_y(30f32.to_radians());
        let mut b = RotationBlend::snapped(Quat::IDENTITY);
        b.receive(server, 0.05);
        let left = |b: &RotationBlend| b.drawn.angle_between(server).to_degrees();
        let dt = 1.0 / 60.0;
        let mut seen = Vec::new();
        for _ in 0..4 {
            b.frame(dt);
            seen.push(left(&b));
        }
        for (got, want) in seen.iter().zip([25.5, 19.9, 12.3, 0.0]) {
            assert!((got - want).abs() < 0.06, "{seen:?}");
        }
    }

    #[test]
    fn a_new_rotation_restarts_the_blend() {
        let mut b = RotationBlend::snapped(Quat::IDENTITY);
        b.receive(Quat::from_rotation_y(0.5), 0.05);
        b.frame(1.0 / 60.0);
        let mid = b.drawn;
        b.receive(Quat::from_rotation_y(1.0), 0.05);
        assert!((b.left - 0.055).abs() < 1e-6);
        b.frame(1.0 / 60.0);
        assert!(b.drawn.angle_between(mid) > 0.0);
        assert!(
            b.drawn.angle_between(Quat::from_rotation_y(1.0)) > 0.0,
            "never quite catches it"
        );
    }
}
