//! The death camera's geometry (specs/combat_arms/death_killcam.md 3, 4):
//! the pull-back and the watch orbit, in the game's own frame (units u,
//! +Y up, left-handed: +X right, +Z forward; `to_world` and `from_world`
//! convert to Chimera's), with no engine in it: the world is a `Clip`,
//! a ray that says where it first hits geometry.

use bevy::prelude::*;

/// The anchor sits this far over a body (u).
pub const ANCHOR_LIFT: f32 = 20.0;
/// The pull-back's step (`DC_TV`), u per frame, before the pull-in.
pub const PULL_STEP: f32 = 5.0;
/// Every position is placed at this share of its distance from the anchor
/// (or from the wall hit).
pub const PULL_IN: f32 = 0.9;
/// The pull-back stops farther than this from its start (`DC_TL`).
pub const PULL_STOP: f32 = 220.0;
/// The orbit's offset (`DeadCamX`, `DeadCamY`) and base length L0.
pub const ORBIT_BACK: f32 = 165.0;
pub const ORBIT_UP: f32 = 90.0;
/// √(165² + 90²).
pub const ORBIT_LENGTH: f32 = 187.949_46;
/// One wheel notch zooms this far.
pub const ZOOM_STEP: f32 = 40.0;
/// The zoom's range, added to the orbit length.
pub const ZOOM_MIN: f32 = -ORBIT_LENGTH / 2.0;
pub const ZOOM_MAX: f32 = 800.0;
/// The dead player's pitch while not in first person: ±(45° − 0.1 rad).
pub const PITCH_LIMIT: f32 = std::f32::consts::FRAC_PI_4 - 0.1;
/// The look delta is halved before it is added to yaw and pitch.
pub const MOUSE_FACTOR: f32 = 0.5;

/// A camera pose in the game's frame: where, and where it looks (the
/// heading atan2(x, z) and the pitch, positive looking down).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    pub at: Vec3,
    pub heading: f32,
    pub pitch: f32,
}

/// The world as the camera meets it: the first hit of the ray from `from`
/// to `to` against world geometry (not players or props), if any.
pub trait Clip {
    fn hit(&self, from: Vec3, to: Vec3) -> Option<Vec3>;
}

impl<F: Fn(Vec3, Vec3) -> Option<Vec3>> Clip for F {
    fn hit(&self, from: Vec3, to: Vec3) -> Option<Vec3> {
        self(from, to)
    }
}

/// Open air: nothing is ever hit.
pub struct NoWalls;

impl Clip for NoWalls {
    fn hit(&self, _: Vec3, _: Vec3) -> Option<Vec3> {
        None
    }
}

/// `M(a, φ) v` with M = cos φ I + sin φ [a]× + (1 − cos φ) a aᵀ, the axis
/// used as it is (not normalised: Quirk 5).
pub fn rotate(a: Vec3, phi: f32, v: Vec3) -> Vec3 {
    let (s, c) = phi.sin_cos();
    v * c + a.cross(v) * s + a * (a.dot(v) * (1.0 - c))
}

/// The shared placement rule: from the anchor, a ray to the wanted
/// position; the camera goes to 0.9 of the way to the hit (or the wanted
/// position).
pub fn place(anchor: Vec3, wanted: Vec3, clip: &(impl Clip + ?Sized)) -> Vec3 {
    let h = clip.hit(anchor, wanted).unwrap_or(wanted);
    anchor + (h - anchor) * PULL_IN
}

/// Where a camera at `c` looks at `target`: the heading and the pitch
/// (positive looks down).
pub fn look_at(c: Vec3, target: Vec3) -> (f32, f32) {
    let d = target - c;
    let flat = (d.x * d.x + d.z * d.z).sqrt();
    (d.x.atan2(d.z), -d.y.atan2(flat))
}

/// The anchor over a body at `p`.
pub fn anchor(p: Vec3) -> Vec3 {
    p + Vec3::Y * ANCHOR_LIFT
}

/// The pull-back (3): starts 20 u above the dead player and backs away
/// from where the damage came from, looking that way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PullBack {
    /// The start S.
    pub start: Vec3,
    /// The camera now.
    pub at: Vec3,
    /// Stopped (a wall in the step's way, or too far).
    pub frozen: bool,
    /// The look, from the damage direction (None: no direction yet, the
    /// view the camera had is kept).
    pub look: Option<(f32, f32)>,
}

impl PullBack {
    /// The pull-back's first frame: the camera at S = P + (0, 20, 0).
    pub fn begin(p: Vec3) -> Self {
        let s = anchor(p);
        Self {
            start: s,
            at: s,
            frozen: false,
            look: None,
        }
    }

    /// One frame's step for a body at `p` hit in direction `d` (the way the
    /// damage travelled). A zero `d` moves nothing and keeps the look
    /// (Open question 5).
    pub fn step(&mut self, p: Vec3, d: Vec3, clip: &(impl Clip + ?Sized)) {
        if self.frozen {
            return;
        }
        let u = -d.normalize_or_zero();
        if u == Vec3::ZERO {
            return;
        }
        self.look = Some((u.x.atan2(u.z), -u.y.clamp(-1.0, 1.0).asin()));
        let next = self.at - u * PULL_STEP;
        if clip.hit(self.at, next).is_some() {
            self.frozen = true;
        }
        self.at = place(anchor(p), next, clip);
        // Quirk 2: never reached (the distance tested is at most 50 u).
        if (next - self.start).length() > PULL_STOP {
            self.frozen = true;
        }
    }
}

/// The watch camera's orbit offset for the dead player's yaw ψ and pitch
/// θ and the zoom z (4): `o3`, before the placement.
pub fn orbit_offset(psi: f32, theta: f32, zoom: f32) -> Vec3 {
    let o0 = Vec3::new(-ORBIT_BACK, ORBIT_UP, 0.0);
    let o1 = o0 * (1.0 + zoom / ORBIT_LENGTH);
    let o2 = rotate(Vec3::Y, -psi, o1);
    let n = o2.normalize_or_zero();
    let a = Vec3::new(n.z, 0.0, -n.x);
    rotate(a, -theta, o2)
}

/// The watch camera around a player at `p`: placed by the shared rule and
/// looking at the anchor.
pub fn orbit(p: Vec3, psi: f32, theta: f32, zoom: f32, clip: &(impl Clip + ?Sized)) -> Pose {
    let a = anchor(p);
    let at = place(a, a + orbit_offset(psi, theta, zoom), clip);
    let (heading, pitch) = look_at(at, a);
    Pose { at, heading, pitch }
}

/// A yaw and pitch after a mouse delta (radians, as the shell's `look_delta`
/// gives it: yaw left, pitch up): halved, +ψ turning right, θ positive
/// looking down, θ held to ±39.27°.
pub fn turn(psi: f32, theta: f32, look: Vec2) -> (f32, f32) {
    let psi = psi - look.x * MOUSE_FACTOR;
    let theta = (theta - look.y * MOUSE_FACTOR).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    (psi, theta)
}

/// The zoom after `notches` of the wheel (positive: rolled away from the
/// player).
pub fn zoom(z: f32, notches: f32) -> f32 {
    (z + ZOOM_STEP * notches).clamp(ZOOM_MIN, ZOOM_MAX)
}

/// The game's frame into Chimera's (Z mirrored), metres per unit `unit`.
pub fn to_world(p: Vec3, unit: f32) -> Vec3 {
    Vec3::new(p.x, p.y, -p.z) * unit
}

/// Chimera's frame into the game's.
pub fn from_world(p: Vec3, unit: f32) -> Vec3 {
    Vec3::new(p.x, p.y, -p.z) / unit
}

/// A direction (not scaled) into the game's frame and back.
pub fn mirror(v: Vec3) -> Vec3 {
    Vec3::new(v.x, v.y, -v.z)
}

/// The camera's rotation in Chimera's frame for a heading and pitch of the
/// game's (the heading turns +Z towards +X, i.e. right; Chimera's yaw
/// turns left).
pub fn to_rotation(heading: f32, pitch: f32) -> Quat {
    Quat::from_euler(EulerRot::YXZ, -heading, -pitch, 0.0)
}
