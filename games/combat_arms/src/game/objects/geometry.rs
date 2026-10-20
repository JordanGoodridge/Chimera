//! The world objects' maths in the world file's frame (world units, +X
//! right, +Y up, +Z forward; specs/combat_arms/world_objects.md "Units
//! and conventions"): the file's rotation R(p, y, r), the waveforms, the
//! boxes' overlap test, and the conversion to the engine's axes and
//! metres at the boundary.

use bevy::prelude::*;

/// R(p, y, r) (world_dat.md "Rotation to matrix": yaw, then pitch, then
/// roll; radians) as a quaternion acting on file-frame vectors.
pub fn rotation(pitch: f32, yaw: f32, roll: f32) -> Quat {
    Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch) * Quat::from_rotation_z(roll)
}

/// A file-frame point as the engine's (Z mirrored, `scale` metres per u).
pub fn to_engine(p: Vec3, scale: f32) -> Vec3 {
    Vec3::new(p.x, p.y, -p.z) * scale
}

/// An engine point (metres) in the file frame (u).
pub fn from_engine(p: Vec3, scale: f32) -> Vec3 {
    Vec3::new(p.x, p.y, -p.z) / scale
}

/// A file-frame rotation as the engine's (the mirror conjugates it: the
/// axis mirrored, the angle negated).
pub fn rotation_to_engine(q: Quat) -> Quat {
    Quat::from_xyzw(-q.x, -q.y, q.z, q.w)
}

/// An engine rotation in the file frame (its own inverse map).
pub fn rotation_from_engine(q: Quat) -> Quat {
    rotation_to_engine(q)
}

/// A waveform (world_objects.md 5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Waveform {
    #[default]
    Linear,
    Sine,
    SlowOff,
    SlowOn,
}

impl Waveform {
    /// By name (case-insensitive); unknown names keep `Linear`.
    pub fn named(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "sine" => Self::Sine,
            "slowoff" => Self::SlowOff,
            "slowon" => Self::SlowOn,
            _ => Self::Linear,
        }
    }

    /// The speed for the linear rate `r` at the fraction `p` done; `add`
    /// is the `SlowOn`/`SlowOff` additive term (world models 0.001, the
    /// keyframers' `SlowOn` 0.003 and no `SlowOff` term).
    pub fn speed(self, r: f32, p: f32, slow_on_add: f32, slow_off_add: f32) -> f32 {
        use std::f32::consts::{FRAC_PI_2, PI};
        match self {
            Self::Linear => r,
            Self::Sine => FRAC_PI_2 * r * (PI * p).sin(),
            Self::SlowOff => FRAC_PI_2 * r * (PI * p / 2.0).cos() - slow_off_add * r,
            Self::SlowOn => FRAC_PI_2 * r * (PI * p / 2.0).sin() + slow_on_add * r,
        }
    }
}

/// An oriented box: centre, rotation, half-extents (file frame).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Obb {
    pub centre: Vec3,
    pub rot: Quat,
    pub half: Vec3,
}

impl Obb {
    /// Whether it overlaps the axis-aligned box `min`..`max` (separating
    /// axes: the three world axes, the box's three, and their nine
    /// cross products). Touching faces don't count.
    pub fn overlaps(&self, min: Vec3, max: Vec3) -> bool {
        let c = (min + max) / 2.0;
        let h = (max - min) / 2.0;
        let axes = [self.rot * Vec3::X, self.rot * Vec3::Y, self.rot * Vec3::Z];
        let d = self.centre - c;
        let world = [Vec3::X, Vec3::Y, Vec3::Z];
        let mut tests: Vec<Vec3> = Vec::with_capacity(15);
        tests.extend(world);
        tests.extend(axes);
        for a in world {
            for b in axes {
                let x = a.cross(b);
                if x.length_squared() > 1e-8 {
                    tests.push(x.normalize());
                }
            }
        }
        const EPS: f32 = 1e-3;
        tests.iter().all(|&n| {
            let ra = h.x * n.x.abs() + h.y * n.y.abs() + h.z * n.z.abs();
            let rb = self.half.x * axes[0].dot(n).abs()
                + self.half.y * axes[1].dot(n).abs()
                + self.half.z * axes[2].dot(n).abs();
            d.dot(n).abs() < ra + rb - EPS
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_matches_world_dat() {
        // Yaw turns +Z toward +X; positive pitch looks down.
        let f = rotation(0.0, std::f32::consts::FRAC_PI_2, 0.0) * Vec3::Z;
        assert!(f.distance(Vec3::X) < 1e-6);
        let f = rotation(0.3, 0.0, 0.0) * Vec3::Z;
        assert!((f.y + 0.3f32.sin()).abs() < 1e-6);
        let axes = lithtech_formats::world::rotation_axes(0.4, -1.1, 0.7);
        let q = rotation(0.4, -1.1, 0.7);
        for (k, v) in [Vec3::X, Vec3::Y, Vec3::Z].into_iter().enumerate() {
            assert!((q * v).distance(Vec3::from(axes[k])) < 1e-5, "{k}");
        }
    }

    #[test]
    fn engine_rotation_conjugates_the_mirror() {
        let q = rotation(0.4, -1.1, 0.7);
        let v = Vec3::new(1.0, 2.0, 3.0);
        let a = to_engine(q * v, 1.0);
        let b = rotation_to_engine(q) * to_engine(v, 1.0);
        assert!(a.distance(b) < 1e-5);
    }

    #[test]
    fn boxes_overlap() {
        let o = Obb {
            centre: Vec3::ZERO,
            rot: Quat::from_rotation_y(0.785),
            half: Vec3::new(10.0, 10.0, 1.0),
        };
        assert!(!o.overlaps(Vec3::new(5.0, -1.0, 5.0), Vec3::new(7.0, 1.0, 7.0)));
        assert!(o.overlaps(Vec3::new(5.0, -1.0, -6.0), Vec3::new(7.0, 1.0, -4.0)));
        assert!(!o.overlaps(Vec3::new(20.0, -1.0, 0.0), Vec3::new(22.0, 1.0, 1.0)));
    }
}
