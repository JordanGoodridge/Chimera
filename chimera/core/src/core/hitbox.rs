use bevy::prelude::*;

/// Where on a body a hit landed (Source hitgroups).
#[derive(Reflect, Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Hitgroup {
    #[default]
    Generic,
    Head,
    Chest,
    Stomach,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
}

/// Where on a body a hit landed: Chimera's body regions, the superset of
/// what the games tell apart (docs/DESIGN.md "Hits across games"). Source's
/// hitgroups are the coarse view (`hitgroup`), Combat Arms' locations
/// (`BodyPart`) the armour's; a game that tells fewer apart maps onto
/// these (`from_hitgroup`).
#[derive(Reflect, Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BodyRegion {
    #[default]
    Generic,
    Head,
    Neck,
    Chest,
    Stomach,
    Groin,
    LeftUpperArm,
    LeftForearm,
    LeftHand,
    RightUpperArm,
    RightForearm,
    RightHand,
    LeftThigh,
    LeftShin,
    LeftFoot,
    RightThigh,
    RightShin,
    RightFoot,
}

/// What armour covers a region as (Combat Arms' locations): the torso
/// takes in the neck, chest, stomach and anything generic.
#[derive(Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BodyPart {
    Head,
    Torso,
    Arm,
    Leg,
    Groin,
}

impl BodyRegion {
    /// Every region, in order (`index`).
    pub const ALL: [BodyRegion; 18] = [
        BodyRegion::Generic,
        BodyRegion::Head,
        BodyRegion::Neck,
        BodyRegion::Chest,
        BodyRegion::Stomach,
        BodyRegion::Groin,
        BodyRegion::LeftUpperArm,
        BodyRegion::LeftForearm,
        BodyRegion::LeftHand,
        BodyRegion::RightUpperArm,
        BodyRegion::RightForearm,
        BodyRegion::RightHand,
        BodyRegion::LeftThigh,
        BodyRegion::LeftShin,
        BodyRegion::LeftFoot,
        BodyRegion::RightThigh,
        BodyRegion::RightShin,
        BodyRegion::RightFoot,
    ];

    /// Its place in `ALL`.
    pub fn index(self) -> usize {
        self as usize
    }

    /// Source's hitgroup for it (what `Damage` and hit events carry): the
    /// neck counts as chest, the groin as stomach, every part of a limb as
    /// that limb.
    pub fn hitgroup(self) -> Hitgroup {
        use BodyRegion::*;
        match self {
            Generic => Hitgroup::Generic,
            Head => Hitgroup::Head,
            Neck | Chest => Hitgroup::Chest,
            Stomach | Groin => Hitgroup::Stomach,
            LeftUpperArm | LeftForearm | LeftHand => Hitgroup::LeftArm,
            RightUpperArm | RightForearm | RightHand => Hitgroup::RightArm,
            LeftThigh | LeftShin | LeftFoot => Hitgroup::LeftLeg,
            RightThigh | RightShin | RightFoot => Hitgroup::RightLeg,
        }
    }

    /// A hitgroup's region, for what knows only hitgroups: a limb's upper
    /// part.
    pub fn from_hitgroup(g: Hitgroup) -> Self {
        match g {
            Hitgroup::Generic => BodyRegion::Generic,
            Hitgroup::Head => BodyRegion::Head,
            Hitgroup::Chest => BodyRegion::Chest,
            Hitgroup::Stomach => BodyRegion::Stomach,
            Hitgroup::LeftArm => BodyRegion::LeftUpperArm,
            Hitgroup::RightArm => BodyRegion::RightUpperArm,
            Hitgroup::LeftLeg => BodyRegion::LeftThigh,
            Hitgroup::RightLeg => BodyRegion::RightThigh,
        }
    }

    /// The part armour covers it as.
    pub fn part(self) -> BodyPart {
        use BodyRegion::*;
        match self {
            Head => BodyPart::Head,
            Generic | Neck | Chest | Stomach => BodyPart::Torso,
            Groin => BodyPart::Groin,
            LeftUpperArm | LeftForearm | LeftHand | RightUpperArm | RightForearm | RightHand => BodyPart::Arm,
            LeftThigh | LeftShin | LeftFoot | RightThigh | RightShin | RightFoot => BodyPart::Leg,
        }
    }

    /// The hit priority of a shape that gives none (Combat Arms' server
    /// walk picks the highest, weapon_firing.md Corrections 5): its data's
    /// defaults by part (torso and groin 3, head 2, limbs 1).
    pub fn default_priority(self) -> f32 {
        match self.part() {
            BodyPart::Torso | BodyPart::Groin => 3.0,
            BodyPart::Head => 2.0,
            BodyPart::Arm | BodyPart::Leg => 1.0,
        }
    }
}

/// A shape on a character's body that shots test, in the character's
/// local space: feet at the origin, Y up, facing -Z (yaw 0), meters. An
/// oriented box (Source hitboxes: `half`, `rotation`) or a sphere (Combat
/// Arms' hit spheres: `radius`), with its body region, the hit priority
/// Combat Arms' server walk ranks shapes by and the body's own damage
/// factor for it (`scale`: Combat Arms' zone factor; guns whose damage
/// table says so use it). The body's game converts its own format into
/// these (docs/DESIGN.md "Hits across games"); every gun's rule works on
/// either shape.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub struct Hitbox {
    pub center: Vec3,
    pub half: Vec3,
    pub rotation: Quat,
    pub region: BodyRegion,
    /// A sphere of this radius (`half` and `rotation` unused); None: a box.
    pub radius: Option<f32>,
    pub priority: f32,
    pub scale: Option<f32>,
}

impl Hitbox {
    /// An oriented box of `region` (its region's default priority, no
    /// scale of its own).
    pub fn cuboid(center: Vec3, half: Vec3, rotation: Quat, region: BodyRegion) -> Self {
        Self {
            center,
            half,
            rotation,
            region,
            radius: None,
            priority: region.default_priority(),
            scale: None,
        }
    }

    /// A sphere of `region`.
    pub fn sphere(center: Vec3, radius: f32, region: BodyRegion) -> Self {
        Self {
            center,
            half: Vec3::splat(radius),
            rotation: Quat::IDENTITY,
            region,
            radius: Some(radius),
            priority: region.default_priority(),
            scale: None,
        }
    }

    /// Its Source hitgroup (`BodyRegion::hitgroup`).
    pub fn group(&self) -> Hitgroup {
        self.region.hitgroup()
    }

    /// Where a ray (local space) enters the shape, as a distance along
    /// `dir` (unit), if it does.
    pub fn ray_entry(&self, origin: Vec3, dir: Vec3) -> Option<f32> {
        self.ray_span(origin, dir).map(|(enter, _)| enter)
    }

    /// Where a ray (local space) enters and leaves the shape, as distances
    /// along `dir` (unit; the entry clamped at 0), if it crosses it.
    pub fn ray_span(&self, origin: Vec3, dir: Vec3) -> Option<(f32, f32)> {
        if let Some(r) = self.radius {
            if r <= 0.0 {
                return None;
            }
            let p = origin - self.center;
            let b = p.dot(dir);
            let c = p.length_squared() - r * r;
            let disc = b * b - c;
            if disc < 0.0 {
                return None;
            }
            let s = disc.sqrt();
            let (enter, leave) = (-b - s, -b + s);
            return (leave >= 0.0).then_some((enter.max(0.0), leave));
        }
        let inv = self.rotation.inverse();
        let o = inv * (origin - self.center);
        let d = inv * dir;
        let (mut enter, mut leave) = (f32::MIN, f32::MAX);
        for i in 0..3 {
            if d[i].abs() < 1e-9 {
                if o[i].abs() > self.half[i] {
                    return None;
                }
                continue;
            }
            let (a, b) = ((-self.half[i] - o[i]) / d[i], (self.half[i] - o[i]) / d[i]);
            enter = enter.max(a.min(b));
            leave = leave.min(a.max(b));
        }
        (enter <= leave && leave >= 0.0).then_some((enter.max(0.0), leave))
    }

    /// The radius it counts as for `power` and `line_passes`: a sphere's
    /// own, a box's bounding sphere's.
    pub fn reach(&self) -> f32 {
        self.radius.unwrap_or_else(|| self.half.length())
    }

    /// Whether the whole segment `start`..`end` passes the shape (Combat
    /// Arms' client test: a sphere when its closest point is within the
    /// radius; a box when the segment crosses it). A zero radius never.
    pub fn segment_passes(&self, start: Vec3, end: Vec3) -> bool {
        match self.radius {
            Some(r) => {
                if r <= 0.0 {
                    return false;
                }
                let seg = end - start;
                let len2 = seg.length_squared().max(1e-12);
                let t = ((self.center - start).dot(seg) / len2).clamp(0.0, 1.0);
                (start + seg * t).distance_squared(self.center) <= r * r
            }
            None => {
                let seg = end - start;
                let len = seg.length();
                len > 0.0 && self.ray_span(start, seg / len).is_some_and(|(enter, _)| enter <= len)
            }
        }
    }

    /// The axis-aligned box around the shape (in its own space: min, max).
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let extent = match self.radius {
            Some(r) => Vec3::splat(r.max(0.0)),
            None => {
                let m = Mat3::from_quat(self.rotation);
                Vec3::new(
                    m.row(0).abs().dot(self.half),
                    m.row(1).abs().dot(self.half),
                    m.row(2).abs().dot(self.half),
                )
            }
        };
        (self.center - extent, self.center + extent)
    }

    /// |p|^2 - r^2, p its centre from `from` (Combat Arms' client test
    /// takes the smallest of the shapes a segment passes; a box by its
    /// bounding sphere).
    pub fn power(&self, from: Vec3) -> f32 {
        let r = self.reach();
        (self.center - from).length_squared() - r * r
    }

    /// Whether the infinite line through `at` along the unit `dir` passes
    /// within the shape's reach of its centre (Combat Arms' server walk;
    /// a box by its bounding sphere). A zero radius never.
    pub fn line_passes(&self, at: Vec3, dir: Vec3) -> bool {
        let r = self.reach();
        let p = self.center - at;
        r > 0.0 && (p - dir * p.dot(dir)).length_squared() <= r * r
    }

    /// Between `self` and `other` (the same shape later) by `f`.
    pub fn lerp(&self, other: &Hitbox, f: f32) -> Hitbox {
        Hitbox {
            center: self.center.lerp(other.center, f),
            half: self.half.lerp(other.half, f),
            rotation: self.rotation.slerp(other.rotation, f),
            radius: match (self.radius, other.radius) {
                (Some(a), Some(b)) => Some(a + (b - a) * f),
                (r, _) => r,
            },
            ..*self
        }
    }
}

/// A character's hitboxes (see `Hitbox`).
#[derive(Component, Clone, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct Hitboxes(pub Vec<Hitbox>);

impl Hitboxes {
    /// The axis-aligned box around every shape (min, max; in their
    /// space), if there are any. A gun whose rule first tests a body's
    /// box (Combat Arms') widens that box by these, so shapes poking out
    /// of the character's hull (another game's taller model's head) still
    /// count, as in Source.
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        self.0.iter().map(Hitbox::bounds).reduce(|(a0, a1), (b0, b1)| (a0.min(b0), a1.max(b1)))
    }

    /// The shape a ray (local space) enters first within `limit`: its
    /// index, where it enters and leaves it (CS:S's rule).
    pub fn nearest(&self, origin: Vec3, dir: Vec3, limit: f32) -> Option<(usize, f32, f32)> {
        self.0
            .iter()
            .enumerate()
            .filter_map(|(i, b)| b.ray_span(origin, dir).map(|(enter, leave)| (i, enter, leave)))
            .filter(|(_, t, _)| *t < limit)
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// Of the shapes the whole segment passes, the one with the smallest
    /// `Hitbox::power` from `start` (Combat Arms' client rule).
    pub fn by_power(&self, start: Vec3, end: Vec3) -> Option<usize> {
        self.0
            .iter()
            .enumerate()
            .filter(|(_, b)| b.segment_passes(start, end))
            .map(|(i, b)| (i, b.power(start)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// Of the shapes the line through `at` along `dir` passes, the one
    /// of the highest priority; on a tie the later one (Combat Arms'
    /// server walk).
    pub fn by_priority(&self, at: Vec3, dir: Vec3) -> Option<usize> {
        self.0
            .iter()
            .enumerate()
            .filter(|(_, b)| b.line_passes(at, dir))
            .fold(None, |best: Option<(usize, f32)>, (i, b)| match best {
                Some((_, p)) if p > b.priority => best,
                _ => Some((i, b.priority)),
            })
            .map(|(i, _)| i)
    }
}

#[cfg(test)]
mod hitbox_bounds_tests {
    use super::*;

    /// The bounds hold every shape: a sphere by its radius, a turned box
    /// by its corners.
    #[test]
    fn bounds_hold_spheres_and_turned_boxes() {
        let turned = Quat::from_rotation_z(std::f32::consts::FRAC_PI_4);
        let shapes = Hitboxes(vec![
            Hitbox::sphere(Vec3::new(0.0, 1.0, 0.0), 0.1, BodyRegion::Chest),
            Hitbox::cuboid(Vec3::new(0.0, 2.0, 0.0), Vec3::new(0.1, 0.1, 0.1), turned, BodyRegion::Head),
        ]);
        let (min, max) = shapes.bounds().unwrap();
        let r = 0.1 * std::f32::consts::SQRT_2;
        assert!((max.y - (2.0 + r)).abs() < 1e-5 && (min.y - 0.9).abs() < 1e-5, "{min} {max}");
        assert!((max.x - r).abs() < 1e-5 && (max.z - 0.1).abs() < 1e-5, "{min} {max}");
        assert!(Hitboxes::default().bounds().is_none());
    }
}
