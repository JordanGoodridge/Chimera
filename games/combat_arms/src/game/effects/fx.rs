//! Effect groups played (specs/combat_arms/effects_sounds.md, "The effect
//! file"): a group of the install's effect file (`library::FxLibrary`)
//! started at a point with an effect frame (`effect_frame`: local +Z
//! along the surface normal or the shot), its keys created at their start
//! times, at the player's effect detail (`DetailLevel`):
//!
//! - `ParticleSystem`: bursts (`EmissionInterval` ≤ 0: one, on the key's
//!   first frame) or emissions every interval until the key ends, each
//!   `ParticlesPerEmission` particles of Chimera's pool
//!   (`map::particles`): start points by `Type` and radius, a per-axis
//!   velocity box in the effect's frame, life, size (scale keys s: a
//!   square of side 4 s), colour and alpha keys, gravity (positive up),
//!   friction per second, bounces (`PercentToBounce`, 75 % kept), spin;
//!   splats (`SplatPercent`, `SplatEffect`: the group at the world hit,
//!   flat against it, `KillOnSplat`) traced along each splatting
//!   particle's fall;
//! - `Sprite`: one particle facing the camera or lying along the normal,
//!   texture pixels × scale wide, for the key's length;
//! - `DynaLight`: a light (`FxLight`: radius and colour keys), drawn by
//!   the front end;
//! - `CamJitter`: an amplitude (scale keys) falling off with the camera's
//!   distance (`rules::jitter_falloff`), read by the front end;
//! - `CreateFX`: another group.
//!
//! A key drawn through an effect shader (`library::shaded`: the
//! refraction keys of heat and blasts) draws nothing.
//!
//! Several jitters add up (`jitter_at`, Open question 24).
//!
//! Known shortcuts (docs/tech-debt.md): keys are placed at the effect's
//! point (the attach modes, `rules::place_key`, aren't applied to the
//! pool's particles; the first-person muzzle flash is the front end's
//! own, at the view model's socket); multi-key scale and colour curves are
//! drawn as a ramp from the first key to the last (`rules::key_curve` is
//! the full curve), alpha as a fade-in/hold/fade-out window; cone and
//! cylinder emitters are circles (`rules::cone_start` is the cone); `AdditionalAcceleration` is left
//! out; a bouncing system's particles all bounce (Chimera's bounce planes
//! probed once, 75 % ± 10 % kept); friction is Chimera's linear damping at
//! the same per-second rate; `DisableZ` and `CastVisibleRay` sprites are
//! drawn depth-tested; no effect shaders (the refraction keys, whose
//! textures are distortion maps, are left out, not drawn as colour);
//! `LightningFX`, `LTBModel`, `PolyTrail`,
//! `PlaySound` and `Null` keys draw nothing; off-screen systems keep
//! ageing (Quirk 7 not reproduced).

use bevy::prelude::*;
use lithtech_formats::fxf::{FxKey, sample_keys};

use super::{
    library::{FxLibrary, additive, material_name, shaded},
    rules::jitter_falloff,
};
use crate::map::{
    ParticleRng,
    particles::{Bounce, Fade, MapParticles, Motion, Particle, ParticleGroup, Particles, Ramp, Shape, WorldTrace, probe_planes},
};

/// An effect's frame (columns: local X, Y, Z in world axes): local +Z
/// along `normal`, +Y from the world's up (world +X when the normal is
/// vertical).
pub fn effect_frame(normal: Vec3) -> Mat3 {
    let z = normal.normalize_or(Vec3::Y);
    let up = if z.cross(Vec3::Y).length_squared() < 1e-8 { Vec3::X } else { Vec3::Y };
    let y = (up - z * up.dot(z)).normalize_or(Vec3::X);
    let x = y.cross(z);
    Mat3::from_cols(x, y, z)
}

/// One key's progress.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct KeyRun {
    started: bool,
    next_emit: f32,
}

/// A group playing.
#[derive(Clone, Debug)]
pub struct FxInstance {
    pub group: String,
    /// Metres, world.
    pub at: Vec3,
    pub frame: Mat3,
    pub age: f32,
    keys: Vec<KeyRun>,
}

/// A light an effect made (front end: a point light).
#[derive(Clone, Debug, PartialEq)]
pub struct FxLight {
    pub at: Vec3,
    pub age: f32,
    pub life: f32,
    /// (time 0..1, radius u) and (time, colour rgb + opacity).
    pub radius: Vec<(f32, f32)>,
    pub color: Vec<(f32, [f32; 4])>,
}

impl FxLight {
    /// Its radius (u) now.
    pub fn radius_now(&self) -> f32 {
        sample_keys(&self.radius, self.t(), 0.0)
    }

    pub fn color_now(&self) -> Vec3 {
        let t = self.t();
        let ch = |i: usize| {
            let keys: Vec<(f32, f32)> = self.color.iter().map(|(k, c)| (*k, c[i])).collect();
            sample_keys(&keys, t, 1.0)
        };
        Vec3::new(ch(0), ch(1), ch(2))
    }

    fn t(&self) -> f32 {
        if self.life > 0.0 { (self.age / self.life).clamp(0.0, 1.0) } else { 1.0 }
    }
}

/// A camera jitter an effect made.
#[derive(Clone, Debug, PartialEq)]
pub struct FxJitter {
    pub at: Vec3,
    pub age: f32,
    pub life: f32,
    pub scale: Vec<(f32, f32)>,
    /// u.
    pub inner: f32,
    pub outer: f32,
}

impl FxJitter {
    /// The amplitude a (u) for a camera `d` units away: scale × falloff.
    pub fn amplitude(&self, d: f32) -> f32 {
        let t = if self.life > 0.0 { (self.age / self.life).clamp(0.0, 1.0) } else { 1.0 };
        sample_keys(&self.scale, t, 0.0) * jitter_falloff(d, self.inner, self.outer)
    }
}

/// The groups playing, their lights and jitters.
#[derive(Resource, Debug)]
pub struct CaFx {
    pub instances: Vec<FxInstance>,
    pub lights: Vec<FxLight>,
    pub jitters: Vec<FxJitter>,
    /// The player's effect detail (0 low, 1 medium, 2 high).
    pub detail: u8,
    /// Groups to start later (a splat when its particle lands): (in
    /// seconds, name, point, normal).
    pending: Vec<(f32, String, Vec3, Vec3)>,
    rng: ParticleRng,
    /// Metres per unit.
    pub unit: f32,
}

impl Default for CaFx {
    fn default() -> Self {
        Self {
            instances: Vec::new(),
            lights: Vec::new(),
            jitters: Vec::new(),
            detail: super::rules::DEFAULT_FX_DETAIL,
            pending: Vec::new(),
            rng: ParticleRng::new(0xCAF0),
            unit: super::super::inventory::UNIT,
        }
    }
}

impl CaFx {
    /// Start group `name` at `at` (metres) with frame `frame`; false when
    /// the effect file has no such group (nothing plays).
    pub fn spawn(&mut self, lib: &FxLibrary, name: &str, at: Vec3, frame: Mat3) -> bool {
        let Some(g) = lib.group(name) else { return false };
        self.instances.push(FxInstance {
            group: g.name.clone(),
            at,
            frame,
            age: 0.0,
            keys: vec![KeyRun::default(); g.keys.len()],
        });
        true
    }

    /// The camera jitter's amplitude (u) for a camera at `cam` (metres).
    pub fn jitter_at(&self, cam: Vec3) -> f32 {
        self.jitters
            .iter()
            .map(|j| j.amplitude(j.at.distance(cam) / self.unit))
            .sum()
    }

    /// One frame: start and emit keys, age lights and jitters.
    pub fn step(
        &mut self,
        dt: f32,
        lib: &FxLibrary,
        particles: &mut Particles,
        materials: &MapParticles,
        world: &impl WorldTrace,
    ) {
        for p in &mut self.pending {
            p.0 -= dt;
        }
        let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending).into_iter().partition(|p| p.0 <= 0.0);
        self.pending = later;
        for (_, name, at, normal) in due {
            let spin = Mat3::from_axis_angle(normal.normalize_or(Vec3::Y), self.rng.float(0.0, std::f32::consts::TAU));
            self.spawn(lib, &name, at, spin * effect_frame(normal));
        }
        for l in &mut self.lights {
            l.age += dt;
        }
        self.lights.retain(|l| l.age <= l.life);
        for j in &mut self.jitters {
            j.age += dt;
        }
        self.jitters.retain(|j| j.age <= j.life);
        let mut instances = std::mem::take(&mut self.instances);
        for inst in &mut instances {
            inst.age += dt;
            let Some(g) = lib.group(&inst.group) else { continue };
            for (k, run) in g.keys.iter().zip(inst.keys.iter_mut()) {
                if !k.at_detail(self.detail) || inst.age < k.start || shaded(k) {
                    continue;
                }
                let first = !run.started;
                run.started = true;
                match k.kind.as_str() {
                    "ParticleSystem" => {
                        let interval = k.float("EmissionInterval").unwrap_or(-1.0);
                        if interval <= 0.0 {
                            if first {
                                self.emit(k, inst.at, inst.frame, lib, particles, materials, world);
                            }
                        } else {
                            if first {
                                run.next_emit = k.start;
                            }
                            while run.next_emit <= inst.age.min(k.end) {
                                self.emit(k, inst.at, inst.frame, lib, particles, materials, world);
                                run.next_emit += interval;
                            }
                        }
                    }
                    "Sprite" if first => self.sprite(k, inst.at, inst.frame, lib, particles, materials),
                    "DynaLight" if first => self.lights.push(FxLight {
                        at: inst.at + inst.frame * Vec3::from(k.vector("Offset").unwrap_or_default()) * self.unit,
                        age: inst.age - k.start,
                        life: (k.end - k.start).max(1e-3),
                        radius: k.scale_keys(),
                        color: k.color_keys(),
                    }),
                    "CamJitter" if first => self.jitters.push(FxJitter {
                        at: inst.at,
                        age: inst.age - k.start,
                        life: (k.end - k.start).max(1e-3),
                        scale: k.scale_keys(),
                        inner: k.float("InnerRadius").unwrap_or(0.0),
                        outer: k.float("OuterRadius").unwrap_or(0.0),
                    }),
                    "CreateFX" if first => {
                        if let Some(name) = k.string("FXName") {
                            self.pending.push((0.0, name.to_string(), inst.at, inst.frame.z_axis));
                        }
                    }
                    _ => {}
                }
            }
        }
        instances.retain(|i| lib.group(&i.group).is_some_and(|g| i.age <= g.length() + 0.05));
        // Groups started while stepping (none: `pending` holds those).
        instances.append(&mut self.instances);
        self.instances = instances;
    }

    /// One emission of a particle system.
    #[allow(clippy::too_many_arguments)]
    fn emit(
        &mut self,
        k: &FxKey,
        at: Vec3,
        frame: Mat3,
        lib: &FxLibrary,
        particles: &mut Particles,
        materials: &MapParticles,
        world: &impl WorldTrace,
    ) {
        let Some(texture) = lib.texture_of(k) else { return };
        let Some(material) = materials.find(&material_name(&texture, additive(k))) else { return };
        let u = self.unit;
        let n = k.int("ParticlesPerEmission").unwrap_or(1).max(0) as usize;
        let offset = Vec3::from(k.vector("Offset").unwrap_or_default());
        let (r0, r1) = (k.float("MinRadius").unwrap_or(0.0), k.float("MaxRadius").unwrap_or(0.0));
        let plane = Vec3::from(k.vector("EmissionPlane").unwrap_or([0.0, 0.0, 1.0])).normalize_or(Vec3::Z);
        let (v0, v1) = (
            Vec3::from(k.vector("MinParticleVelocity").unwrap_or_default()),
            Vec3::from(k.vector("MaxParticleVelocity").unwrap_or_default()),
        );
        let (l0, l1) = (
            k.float("MinParticleLifeSpan").unwrap_or(1.0),
            k.float("MaxParticleLifeSpan").unwrap_or(1.0),
        );
        let gravity = k.float("GravityAcceleration").unwrap_or(0.0);
        let friction = k.float("Friction").unwrap_or(0.0);
        let bounce = k.float("PercentToBounce").unwrap_or(0.0);
        let spin = k.combo("RotateParticles") == Some(1);
        let (w0, w1) = (
            k.float("MinAngularVelocity").unwrap_or(0.0).to_radians(),
            k.float("MaxAngularVelocity").unwrap_or(0.0).to_radians(),
        );
        let splat = k.float("SplatPercent").unwrap_or(0.0);
        let splat_fx = k.string("SplatEffect").filter(|s| !s.is_empty()).map(str::to_string);
        let kill_on_splat = k.combo("KillOnSplat") == Some(1);
        let sizes = k.scale_keys();
        let size = |s: f32| 2.0 * s * u;
        let ramp = match (sizes.first(), sizes.last()) {
            (Some(a), Some(b)) => Ramp::linear(size(a.1), size(b.1)),
            _ => Ramp::constant(size(1.0)),
        };
        let (color, color_fade, alpha, fade) = color_curve(&k.color_keys());
        let mut group = ParticleGroup::new(Motion {
            gravity: -gravity * u,
            damping: if friction > 0.0 && friction < 1.0 { -(1.0 - friction).ln() } else { 0.0 },
            ..default()
        });
        let mut max_speed: f32 = 0.0;
        for _ in 0..n {
            let r = self.rng.float(r0, r1);
            let local = offset
                + match k.combo("Type").unwrap_or(1) {
                    0 => random_dir(&mut self.rng) * r,
                    1 => Vec3::ZERO,
                    _ => {
                        let a = self.rng.float(0.0, std::f32::consts::TAU);
                        let (px, py) = plane.any_orthonormal_pair();
                        (px * a.cos() + py * a.sin()) * r
                    }
                };
            let v = Vec3::new(
                self.rng.float(v0.x, v1.x),
                self.rng.float(v0.y, v1.y),
                self.rng.float(v0.z, v1.z),
            );
            let position = at + frame * local * u;
            let velocity = frame * v * u;
            max_speed = max_speed.max(velocity.length());
            let mut p = Particle::new(position, self.rng.float(l0, l1).max(1e-3), material, 0.0);
            p.velocity = velocity;
            p.size = ramp;
            p.color = color;
            p.color_fade = color_fade;
            p.alpha = alpha;
            p.fade = fade;
            if spin {
                p.roll = self.rng.float(0.0, std::f32::consts::TAU);
                p.roll_speed = self.rng.float(w0, w1) * if self.rng.random() < 0.5 { -1.0 } else { 1.0 };
            }
            if splat > 0.0
                && let Some(fx) = &splat_fx
                && self.rng.random() * 100.0 < splat
                && let Some((t, hit, normal)) = fall(position, velocity, gravity * u, p.life, world)
            {
                self.pending.push((t, fx.clone(), hit, normal));
                if kill_on_splat {
                    p.life = t.max(1e-3);
                }
            }
            group.particles.push(p);
        }
        if bounce > 0.0 {
            group.motion.bounce = Some(Bounce {
                keep: 0.75,
                land_normal_y: 0.7,
                land_speed: 0.0,
                planes: probe_planes(at, None, max_speed, -gravity * u, world),
            });
        }
        particles.add(group);
    }

    /// A sprite key: one particle for the key's length.
    #[allow(clippy::too_many_arguments)]
    fn sprite(
        &mut self,
        k: &FxKey,
        at: Vec3,
        frame: Mat3,
        lib: &FxLibrary,
        particles: &mut Particles,
        materials: &MapParticles,
    ) {
        let Some(texture) = lib.texture_of(k) else { return };
        let Some(material) = materials.find(&material_name(&texture, additive(k))) else { return };
        let u = self.unit;
        let px = lib.texture_size(&texture).map_or(64.0, |s| s.x as f32);
        let offset = Vec3::from(k.vector("Offset").unwrap_or_default());
        let sizes = k.scale_keys();
        let along = k.combo("Facing") == Some(1);
        // Full width px · k units; a camera-facing sprite's size is half.
        let width = |s: f32| px * s * u * if along { 1.0 } else { 0.5 };
        let ramp = match (sizes.first(), sizes.last()) {
            (Some(a), Some(b)) => Ramp::linear(width(a.1), width(b.1)),
            _ => Ramp::constant(width(1.0)),
        };
        let (color, color_fade, alpha, fade) = color_curve(&k.color_keys());
        let mut p = Particle::new(at + frame * offset * u, (k.end - k.start).max(0.01), material, 0.0);
        p.size = ramp;
        p.color = color;
        p.color_fade = color_fade;
        p.alpha = alpha;
        p.fade = fade;
        if along {
            let normal = frame * Vec3::from(k.vector("Normal").unwrap_or([0.0, 0.0, 1.0]));
            p.shape = Shape::Flat {
                normal: normal.normalize_or(Vec3::Y),
                yaw: self.rng.float(0.0, 360.0),
                yaw_speed: 0.0,
            };
            // Just off the surface.
            p.position += normal.normalize_or(Vec3::Y) * 0.002;
        }
        let mut group = ParticleGroup::new(Motion::default());
        group.particles.push(p);
        particles.add(group);
    }
}

/// A random unit vector.
fn random_dir(rng: &mut ParticleRng) -> Vec3 {
    loop {
        let v = Vec3::new(rng.float(-1.0, 1.0), rng.float(-1.0, 1.0), rng.float(-1.0, 1.0));
        let l = v.length_squared();
        if l > 1e-4 && l <= 1.0 {
            return v / l.sqrt();
        }
    }
}

/// Colour keys as a particle's colour, colour fade, base alpha and alpha
/// fade: colour from the first key to the last; alpha its largest value,
/// faded in up to the first key that reaches it and out after the last.
pub fn color_curve(keys: &[(f32, [f32; 4])]) -> (Vec3, Option<(Vec3, f32, f32)>, f32, Fade) {
    let Some((t0, c0)) = keys.first().copied() else {
        return (Vec3::ONE, None, 1.0, Fade::Ramp(Ramp::constant(1.0)));
    };
    let (t1, c1) = keys.last().copied().unwrap_or((t0, c0));
    let color = Vec3::new(c0[0], c0[1], c0[2]);
    let end = Vec3::new(c1[0], c1[1], c1[2]);
    let fade_color = (color.distance(end) > 1e-3).then_some((end, t0, t1));
    let amax = keys.iter().map(|k| k.1[3]).fold(0.0, f32::max);
    if amax <= 0.0 {
        return (color, fade_color, 0.0, Fade::Ramp(Ramp::constant(0.0)));
    }
    let first_max = keys.iter().find(|k| k.1[3] >= amax - 1e-4).map_or(t0, |k| k.0);
    let last_max = keys.iter().rev().find(|k| k.1[3] >= amax - 1e-4).map_or(t1, |k| k.0);
    let start_alpha = c0[3] / amax;
    let end_alpha = c1[3] / amax;
    let fade = if end_alpha < 1.0 - 1e-4 && last_max < t1 {
        if end_alpha <= 1e-4 {
            Fade::Windows {
                start_alpha,
                fade_in: (t0, first_max),
                fade_out: (last_max, t1),
            }
        } else {
            Fade::Ramp(Ramp::linear(1.0, end_alpha).between(last_max, t1))
        }
    } else if start_alpha < 1.0 - 1e-4 {
        Fade::Windows {
            start_alpha,
            fade_in: (t0, first_max),
            fade_out: (2.0, 2.0),
        }
    } else {
        Fade::Ramp(Ramp::constant(1.0))
    };
    (color, fade_color, amax, fade)
}

/// Where a particle starting at `p` with velocity `v` (m/s) under gravity
/// `g` (m/s², positive up) first meets the world within `life` seconds:
/// (time, point, normal).
fn fall(p: Vec3, v: Vec3, g: f32, life: f32, world: &impl WorldTrace) -> Option<(f32, Vec3, Vec3)> {
    const STEP: f32 = 1.0 / 30.0;
    let (mut at, mut vel, mut t) = (p, v, 0.0);
    while t < life {
        let next = at + vel * STEP;
        if let Some((f, n)) = world.trace(at, next) {
            return Some((t + STEP * f, at.lerp(next, f), n));
        }
        at = next;
        vel.y += g * STEP;
        t += STEP;
    }
    None
}
