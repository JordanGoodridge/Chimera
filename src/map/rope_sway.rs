//! Ropes in the wind (specs/cs_source/ropes.md, "Wind"): a rope near the
//! camera keeps simulating after it settles, its free nodes pushed by
//! gusts (a map without env_wind; one with it isn't swayed, env_wind not
//! being simulated); away from the camera it runs on until it rests and
//! stops. The step is the settling one (`Verlet` with damping, three
//! max-length link sweeps, ends pinned) in engine space, and the drawn
//! points are the spline through the nodes, between the last two steps.
//!
//! Ropes in the wind (specs/cs_source/ropes.md "Wind"): a settled rope's
//! simulation (`RopeSway`) stepped again with the spec's gusts while the camera
//! is within `rope_wind_dist` (`RopeWind`), its mesh rebuilt from the spline
//! through its nodes; `spline` for the loaders.
//!
//! Known shortcuts (docs/tech-debt.md): Ropes (from specs/cs_source/ropes.md)
//! settle at load and sway in the spec's gusts while the camera is within 1000
//! units (`map::rope_sway`; the gusts are random, never compared with CS:S); on
//! maps with an env_wind (cs_italy, de_cbble, de_dust) they hang still,
//! env_wind's own wind not being simulated; no rope collision (`Collide`),
//! drawn points between the last two steps, their light not sampled again as
//! they move. Rope gravity 800 is measured, not specced; dust2's 10-node cable
//! still hangs ~11 px low in the sky_n view. Node light uses our prop probe
//! averaged over six directions (the engine's point-light query is not public).
//! Why it's OK for now: The 5-node cables match CS:S to 1-2 px.

use bevy::prelude::*;

use super::{MapRope, rope_material::rope_mesh};

/// Fixed simulation step, seconds.
pub const STEP: f32 = 1.0 / 50.0;
const DAMPING: f32 = 0.98;
const SWEEPS: usize = 3;
const MAX_SUBDIV: usize = 7;
const UNIT: f32 = 0.0254;
/// Wind only within this of the line between the ends (rope_wind_dist's
/// default).
pub const WIND_DIST: f32 = 1000.0 * UNIT;

/// `rope_wind_dist`: how near (units) the camera must be to a rope for
/// the wind to blow it; 0 keeps every rope still (refcmp's captures).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct RopeWind {
    pub dist: f32,
}

impl Default for RopeWind {
    fn default() -> Self {
        Self { dist: WIND_DIST / UNIT }
    }
}
/// A gust's strength before its envelope, m/s².
const GUST_BASE: f32 = 50.0 * UNIT;
/// A node is still moving while its squared step displacement exceeds this.
const REST_THRESHOLD: f32 = 0.03 * UNIT * UNIT;

/// The drawn points: every node, with `subdiv` (clamped to 0-7) uniform
/// Catmull-Rom points between neighbours. Also returns, per point, the
/// index of the node before it and how far it is toward the next one.
pub fn spline(nodes: &[Vec3], subdiv: usize) -> Vec<(Vec3, usize, f32)> {
    let s = subdiv.min(MAX_SUBDIV);
    let n = nodes.len();
    let mut out = vec![(nodes[0], 0, 0.0)];
    for i in 0..n - 1 {
        let p0 = nodes[i.saturating_sub(1)];
        let (p1, p2) = (nodes[i], nodes[i + 1]);
        let p3 = nodes[(i + 2).min(n - 1)];
        for k in 1..=s {
            let t = k as f32 / (s + 1) as f32;
            let c = p1
                + (p2 - p0) * (t / 2.0)
                + (p0 * 2.0 - p1 * 5.0 + p2 * 4.0 - p3) * (t * t / 2.0)
                + (-p0 + p1 * 3.0 - p2 * 3.0 + p3) * (t * t * t / 2.0);
            out.push((c, i, t));
        }
        out.push((p2, i + 1, 0.0));
    }
    out
}

/// A settled rope that can move again: its nodes (engine space, meters),
/// their positions a step ago, the link's maximum length and gravity.
#[derive(Clone, Debug)]
pub struct RopeSway {
    pub nodes: Vec<Vec3>,
    pub previous: Vec<Vec3>,
    pub link: f32,
    /// m/s².
    pub gravity: Vec3,
    pub subdiv: usize,
    /// Gusts push it (no NoWind key, no env_wind in the map).
    pub wind: bool,
}

impl RopeSway {
    /// One fixed step with `accel` (gravity plus wind, m/s²) on every node
    /// (the ends are pinned again); returns the largest squared node
    /// displacement.
    pub fn step(&mut self, accel: Vec3) -> f32 {
        let (start, end) = (self.nodes[0], *self.nodes.last().unwrap());
        // Acceleration times step²/2, not step² (the spec's quirk).
        let push = accel * STEP * STEP / 2.0;
        for (p, q) in self.nodes.iter_mut().zip(self.previous.iter_mut()) {
            let next = *p + (*p - *q) * DAMPING + push;
            *q = *p;
            *p = next;
        }
        let last = self.nodes.len() - 1;
        for _ in 0..SWEEPS {
            for k in 0..last {
                let d = self.nodes[k] - self.nodes[k + 1];
                let len = d.length();
                if len > self.link {
                    let fix = d * (0.5 * (1.0 - self.link / len));
                    self.nodes[k] -= fix;
                    self.nodes[k + 1] += fix;
                }
            }
            self.nodes[0] = start;
            self.nodes[last] = end;
        }
        self.nodes
            .iter()
            .zip(&self.previous)
            .map(|(p, q)| p.distance_squared(*q))
            .fold(0.0, f32::max)
    }

    /// Whether `eye` is close enough for the wind to blow it: within
    /// `dist` (meters; `WIND_DIST` by default) of the line between its
    /// ends.
    pub fn in_wind(&self, eye: Vec3, dist: f32) -> bool {
        let (a, b) = (self.nodes[0], *self.nodes.last().unwrap());
        let d = b - a;
        let t = if d.length_squared() > 0.0 {
            ((eye - a).dot(d) / d.length_squared()).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.wind && eye.distance(a + d * t) < dist
    }
}

/// Gusts (no env_wind): the first 1-3 s after creation, each lasting
/// T = 2-3 s, the next starting 3-4 s after the last started; strength
/// 50 s u with s in -1..1 and u a random direction, shaped by
/// 1 - cos(π τ / T).
#[derive(Clone, Debug)]
pub struct Gusts {
    rng: u64,
    /// Seconds until the next gust starts.
    until_next: f32,
    /// The gust blowing: (seconds into it, its length, its vector).
    current: Option<(f32, f32, Vec3)>,
}

impl Gusts {
    pub fn new(seed: u64) -> Self {
        let mut g = Self {
            rng: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
            until_next: 0.0,
            current: None,
        };
        g.until_next = g.between(1.0, 3.0);
        g
    }

    fn random(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    fn between(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.random()
    }

    /// The wind's acceleration over the next `dt` seconds (m/s²), engine
    /// space, then the timers advance.
    pub fn advance(&mut self, dt: f32) -> Vec3 {
        if self.until_next <= 0.0 {
            let length = self.between(2.0, 3.0);
            let u = Vec3::new(self.between(-1.0, 1.0), self.between(-1.0, 1.0), self.between(-1.0, 1.0)).normalize_or_zero();
            let s = self.between(-1.0, 1.0);
            self.current = Some((0.0, length, u * GUST_BASE * s));
            self.until_next += self.between(3.0, 4.0);
        }
        let accel = match self.current {
            Some((tau, length, v)) if tau < length => v * (1.0 - (std::f32::consts::PI * tau / length).cos()),
            _ => Vec3::ZERO,
        };
        if let Some((tau, ..)) = self.current.as_mut() {
            *tau += dt;
        }
        self.until_next -= dt;
        accel
    }
}

/// A rope that sways: its simulation, gusts, the look it is drawn with and
/// the mesh its strips share.
#[derive(Component)]
pub(crate) struct RopeMotion {
    pub sway: RopeSway,
    pub gusts: Gusts,
    pub look: MapRope,
    pub mesh: Handle<Mesh>,
    /// Time not yet stepped, seconds.
    pub pending: f32,
    pub resting: bool,
}

/// Steps ropes the wind blows or that haven't come to rest, and redraws
/// them from their nodes between the last two steps.
pub(crate) fn sway_ropes(
    time: Res<Time>,
    cameras: Query<
        &GlobalTransform,
        (
            With<Camera3d>,
            Without<super::monitor::ScreenCamera>,
            Without<super::SkyboxCamera>,
            Without<super::ViewModelCamera>,
            Without<super::water::WaterReflectionCamera>,
        ),
    >,
    mut ropes: Query<&mut RopeMotion>,
    mut meshes: ResMut<Assets<Mesh>>,
    wind: Option<Res<RopeWind>>,
) {
    let Some(eye) = cameras.iter().next().map(|c| c.translation()) else { return };
    let dist = wind.map_or(WIND_DIST, |w| w.dist * UNIT);
    let dt = time.delta_secs().min(0.1);
    for mut r in &mut ropes {
        let windy = r.sway.in_wind(eye, dist);
        if !windy && r.resting {
            continue;
        }
        r.pending += dt;
        while r.pending >= STEP {
            r.pending -= STEP;
            let wind = if windy { r.gusts.advance(STEP) } else { Vec3::ZERO };
            let gravity = r.sway.gravity;
            let moved = r.sway.step(gravity + wind);
            r.resting = !windy && moved <= REST_THRESHOLD;
            if r.resting {
                r.pending = 0.0;
                break;
            }
        }
        // Drawn between the last step and the next.
        let alpha = (r.pending / STEP).clamp(0.0, 1.0);
        let nodes: Vec<Vec3> = r
            .sway
            .previous
            .iter()
            .zip(&r.sway.nodes)
            .map(|(q, p)| q.lerp(*p, if r.resting { 1.0 } else { alpha }))
            .collect();
        let points: Vec<Vec3> = spline(&nodes, r.sway.subdiv).into_iter().map(|(p, ..)| p).collect();
        if points.len() != r.look.points.len() {
            continue;
        }
        r.look.points = points;
        let mesh = rope_mesh(&r.look);
        let handle = r.mesh.clone();
        let _ = meshes.insert(&handle, mesh);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rope() -> RopeSway {
        // 256 units across, 4 links of 72 units (Slack 132): a sag.
        let a = Vec3::ZERO;
        let b = Vec3::X * 256.0 * UNIT;
        let nodes: Vec<Vec3> = (0..5).map(|i| a.lerp(b, i as f32 / 4.0)).collect();
        RopeSway {
            previous: nodes.clone(),
            nodes,
            link: 72.0 * UNIT,
            gravity: Vec3::NEG_Y * 800.0 * UNIT,
            subdiv: 2,
            wind: true,
        }
    }

    #[test]
    fn gusts_follow_the_spec_timing_and_envelope() {
        let mut g = Gusts::new(5);
        let first = g.until_next;
        assert!((1.0..=3.0).contains(&first), "{first}");
        // Nothing before the first gust.
        let mut t = 0.0;
        while t + STEP < first {
            assert_eq!(g.advance(STEP), Vec3::ZERO);
            t += STEP;
        }
        // Then a gust: at most 100 units/s² (twice the base), back to 0
        // after its 2-3 s.
        let mut peak: f32 = 0.0;
        let mut blew = 0.0;
        for _ in 0..(3.2 / STEP) as usize {
            let a = g.advance(STEP);
            peak = peak.max(a.length());
            if a != Vec3::ZERO {
                blew += STEP;
            }
        }
        assert!(peak > 0.0 && peak <= 2.0 * GUST_BASE + 1e-6, "{peak}");
        assert!((1.9..=3.1).contains(&blew), "{blew}");
    }

    #[test]
    fn a_rope_settles_and_the_wind_moves_it_sideways() {
        let mut r = rope();
        for _ in 0..2000 {
            r.step(r.gravity);
        }
        let settled = r.nodes.clone();
        assert!(settled[2].y < -0.1, "it sags: {}", settled[2]);
        // Still: a step changes nothing measurable.
        assert!(r.step(r.gravity) <= REST_THRESHOLD);
        // A steady sideways push (a gust's peak) moves the middle off the
        // plane of its ends, the ends stay pinned.
        for _ in 0..200 {
            r.step(r.gravity + Vec3::Z * 2.0 * GUST_BASE);
        }
        assert!(r.nodes[2].z > 0.01, "{}", r.nodes[2]);
        assert_eq!((r.nodes[0], r.nodes[4]), (settled[0], settled[4]));
    }

    #[test]
    fn wind_only_near_the_camera_and_unless_off() {
        let mut r = rope();
        assert!(r.in_wind(Vec3::new(1.0, 2.0, 0.0), WIND_DIST));
        assert!(!r.in_wind(Vec3::new(3.0, 0.0, WIND_DIST + 0.1), WIND_DIST));
        // rope_wind_dist 0: still.
        assert!(!r.in_wind(Vec3::new(1.0, 2.0, 0.0), 0.0));
        r.wind = false;
        assert!(!r.in_wind(Vec3::new(1.0, 2.0, 0.0), WIND_DIST));
    }
}
