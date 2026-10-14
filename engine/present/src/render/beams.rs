//! Beams and glows from map entities, for any game (specs/source/
//! visual_entities.md): point_spotlight's shaft and halo (2), env_laser and
//! env_beam's beam between two points (3, 4.1), env_lightglow's
//! screen-sized glow (6). Beams are drawn by `BeamMaterial` (beam.wgsl);
//! halos and light glows are glow sprites (`GlowSprite`, faded by how
//! much of their occlusion proxy is seen) whose colour and size follow
//! the eye each frame. The logic switches them through `EntityPart`.
//!
//! Beams and glows for any game (specs/source/visual_entities.md): `MapBeam`
//! strips facing the camera (`BeamMaterial`: width, end width, shading over the
//! fade length, a spotlight's near-axis fade; added to the scene; drawn through
//! up to 129 points moved by fractal noise, the texture repeating and scrolling
//! along the beam and the sprite's frames playing, `animate_beams`), a
//! spotlight's halo and env_lightglow as glow sprites whose colour and size
//! follow the eye (`halo_look`, `glow_fade`), switched by `EntityPart`.
//!
//! Known shortcuts (docs/tech-debt.md): Beams
//! (specs/source/visual_entities.md): lasers and beams are drawn between their
//! ends as placed (targets that move aren't followed, the random target isn't
//! redrawn), with the spec's fractal noise (new each frame), segment count,
//! texture repeat per 100 units, TextureScroll as its quirk reads (offset
//! frac(now x s²): de_cbble's and de_inferno's chimney smoke at s 5 scroll 25
//! lengths a second, Q10 unconfirmed) and the sprite's frames at its framerate;
//! sine noise isn't drawn (no map entity sets it); strike-generator env_beams'
//! strikes (`map::emitters::StrikeEmitter`) are drawn through the particle pool
//! between the ends where they were at the strike (moving ends aren't
//! followed), with the spec's fractal noise and segment count but no scroll or
//! frame animation, a ring as nothing; their damage hits players only (not
//! props or breakables); env_spritetrail's strip is drawn with texture row 0
//! across its full width (texture resolution 0, spec Q6), its quads one per
//! pair of points (no smoothing), its rendermode only additive or blended;
//! env_spark draws the electric bullet-impact sparks (its magnitude, trail
//! length and glow flag aren't used); a spotlight's dynamic light isn't made;
//! halos and light glows fade by line tests to their proxy (as sprites) and add
//! in linear space; the beam's strip is twice its width wide (spec Q1). Why
//! it's OK for now: point_spotlight shafts are the common case (22 maps).

use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    mesh::Indices,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, PrimitiveTopology},
    shader::ShaderRef,
};

use super::sprite_material::SpriteMaterial;
use crate::map::{
    ParticleRng,
    beams::{MapBeam, MapGlow, SpotHalo},
    emitters::{BEAM_MAX_SEGMENTS, BEAM_NOISE_SAMPLES, fractal_noise},
};

/// One unit, meters.
const UNIT: f32 = 0.0254;

/// Fractal noise samples as the shader reads them (four to a vector).
pub const NOISE_VECTORS: usize = BEAM_NOISE_SAMPLES.div_ceil(4);

/// 100 units, the distance the glow's quad sits ahead of the eye.
pub const GLOW_DISTANCE_UNITS: f32 = 100.0;

#[derive(Clone, Copy, Debug, PartialEq, bevy::render::render_resource::ShaderType)]
pub struct BeamParams {
    pub start: Vec4,
    pub end: Vec4,
    pub color: Vec4,
    pub widths: Vec4,
    /// x: the points the strip is drawn through (N), y: noise sample to
    /// sideways meters, z: the texture's offset along the beam, w: its
    /// advance per segment.
    pub strip: Vec4,
    /// Fractal noise samples (`BEAM_NOISE_SAMPLES`, four to a vector).
    pub noise: [Vec4; NOISE_VECTORS],
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct BeamMaterial {
    #[uniform(0)]
    pub params: BeamParams,
    #[texture(1)]
    #[sampler(2)]
    pub texture: Option<Handle<Image>>,
}

impl Material for BeamMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://engine_present/render/beam.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "embedded://engine_present/render/beam.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        // Added to the scene (source + destination).
        let add = BlendComponent {
            src_factor: BlendFactor::One,
            dst_factor: BlendFactor::One,
            operation: BlendOperation::Add,
        };
        if let Some(fragment) = descriptor.fragment.as_mut() {
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = Some(BlendState { color: add, alpha: add });
            }
        }
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(false);
        }
        Ok(())
    }
}

impl Default for BeamParams {
    fn default() -> Self {
        Self {
            start: Vec4::ZERO,
            end: Vec4::ZERO,
            color: Vec4::ZERO,
            widths: Vec4::ZERO,
            strip: Vec4::ZERO,
            noise: [Vec4::ZERO; NOISE_VECTORS],
        }
    }
}

/// A beam's strip: two vertices for each of the most points a beam is
/// drawn through (uv: the side, the point's number), placed by the vertex
/// shader; points past a beam's own count fold onto its end.
pub fn beam_mesh() -> Mesh {
    let points = BEAM_MAX_SEGMENTS + 1;
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; points * 2]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; points * 2]);
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_UV_0,
        (0..points).flat_map(|j| [[0.0f32, j as f32], [1.0, j as f32]]).collect::<Vec<_>>(),
    );
    // The shader writes the vertex colour (the shading along the beam).
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0f32; 4]; points * 2]);
    let indices = (0..points as u32 - 1)
        .flat_map(|j| {
            let (a, b, c, d) = (j * 2, j * 2 + 1, j * 2 + 2, j * 2 + 3);
            [a, c, b, b, c, d]
        })
        .collect();
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// A beam's shader parameters at `now` seconds, with this frame's noise
/// samples (zeros: straight).
pub fn beam_params_at(beam: &MapBeam, now: f32, noise: &[f32; BEAM_NOISE_SAMPLES]) -> BeamParams {
    let n = beam.points();
    let length = beam.length_units();
    let step = if beam.tile { length / 100.0 } else { 1.0 } / (n - 1) as f32;
    let mut packed = [Vec4::ZERO; NOISE_VECTORS];
    for (i, v) in noise.iter().enumerate() {
        packed[i / 4][i % 4] = *v;
    }
    BeamParams {
        start: beam.start.extend(1.0),
        end: beam.end.extend(1.0),
        color: beam.color.extend(beam.shade as f32),
        widths: Vec4::new(
            beam.width,
            beam.end_width,
            beam.fade,
            beam.spot.as_ref().map_or(0.0, |s| s.width),
        ),
        strip: Vec4::new(n as f32, beam.noise * length / 100.0 * UNIT, beam.scroll_offset(now), step),
        noise: packed,
    }
}

/// A beam's shader parameters, straight, at time zero.
pub fn beam_params(beam: &MapBeam) -> BeamParams {
    beam_params_at(beam, 0.0, &[0.0; BEAM_NOISE_SAMPLES])
}

/// A beam whose look moves: its noise, scroll and sprite frames, redrawn
/// every frame (`animate_beams`).
#[derive(Component)]
pub struct BeamMotion {
    pub beam: MapBeam,
    /// The sprite's frames (empty: a still texture).
    pub frames: Vec<Handle<Image>>,
    pub rng: ParticleRng,
}

/// Moves animated beams: new noise every frame, the texture's scroll and
/// the sprite's frame (specs/source/visual_entities.md 1 steps 3, 5, 8).
pub fn animate_beams(
    time: Res<Time>,
    mut beams: Query<(&mut BeamMotion, &MeshMaterial3d<BeamMaterial>, &InheritedVisibility)>,
    materials: Option<ResMut<Assets<BeamMaterial>>>,
) {
    let Some(mut materials) = materials else { return };
    let now = time.elapsed_secs();
    for (mut m, material, visible) in &mut beams {
        if !visible.get() {
            continue;
        }
        let noise = if m.beam.noise != 0.0 {
            fractal_noise(&mut m.rng)
        } else {
            [0.0; BEAM_NOISE_SAMPLES]
        };
        let params = beam_params_at(&m.beam, now, &noise);
        let texture = (m.frames.len() > 1).then(|| m.frames[m.beam.frame(now)].clone());
        let Some(current) = materials.get(&material.0) else { continue };
        let texture_changed = texture.as_ref().is_some_and(|t| current.texture.as_ref() != Some(t));
        if current.params == params && !texture_changed {
            continue;
        }
        if let Some(mut mat) = materials.get_mut(&material.0) {
            mat.params = params;
            if let Some(t) = texture {
                mat.texture = Some(t);
            }
        }
    }
}

/// A spotlight halo's look from `eye` (2.3): half-size h x scale, h from 1
/// (eye 4W or more from the axis) to 2 (within W/2); colour x (2k)²
/// clamped, k the cosine between the beam and the eye; none from behind.
pub fn halo_look(halo: &SpotHalo, origin: Vec3, dir: Vec3, eye: Vec3) -> (f32, Vec3) {
    let rel = eye - origin;
    let e = (rel - dir * rel.dot(dir)).length();
    let w = halo.width.max(1e-6);
    let h = (1.0 + (4.0 * w - e) / (3.5 * w)).clamp(1.0, 2.0);
    let k = dir.dot(rel.normalize_or_zero());
    if k <= 0.0 {
        return (h * halo.scale, Vec3::ZERO);
    }
    (h * halo.scale, halo.color * (2.0 * k).powi(2).clamp(0.0, 1.0))
}

/// A light glow's distance fade (6): from MinDist (0) to MaxDist (1),
/// then to 0 at OuterMaxDist when that is past MaxDist.
pub fn glow_fade(glow: &MapGlow, dist: f32) -> f32 {
    let remap = |x: f32, a: f32, b: f32| {
        if b == a {
            if x >= b { 1.0 } else { 0.0 }
        } else {
            ((x - a) / (b - a)).clamp(0.0, 1.0)
        }
    };
    if glow.outer > glow.max && dist > glow.max {
        1.0 - remap(dist, glow.max, glow.outer)
    } else {
        remap(dist, glow.min, glow.max)
    }
}

/// What follows the eye on a halo or glow sprite.
#[derive(Component, Clone, Debug)]
pub enum EyeGlow {
    Halo { halo: SpotHalo, dir: Vec3 },
    Light(MapGlow),
}

/// Halos and light glows follow the eye: their base colour (before the
/// occlusion fade, `GlowSprite`) and their size.
pub fn follow_eye(
    cameras: Query<
        &GlobalTransform,
        (
            With<Camera3d>,
            Without<crate::map::monitor::ScreenCamera>,
            Without<crate::map::SkyboxCamera>,
            Without<crate::map::ViewModelCamera>,
            Without<crate::map::water::WaterReflectionCamera>,
        ),
    >,
    mut glows: Query<(&GlobalTransform, &EyeGlow, &mut crate::map::GlowSprite, &MeshMaterial3d<SpriteMaterial>)>,
    materials: Option<ResMut<Assets<SpriteMaterial>>>,
) {
    let (Some(eye), Some(mut materials)) = (cameras.iter().next(), materials) else {
        return;
    };
    let eye = eye.translation();
    for (at, look, mut sprite, material) in &mut glows {
        let p = at.translation();
        let (size, color) = match look {
            EyeGlow::Halo { halo, dir } => {
                let (half, color) = halo_look(halo, p, *dir, eye);
                (Vec2::splat(half * 2.0), color)
            }
            EyeGlow::Light(g) => {
                let dist = eye.distance(p);
                let mut color = g.color * glow_fade(g, dist);
                if let Some(front) = g.front
                    && (eye - p).normalize_or_zero().dot(front) < 0.0
                {
                    color = Vec3::ZERO;
                }
                // The same size on screen at any distance: the quad 100
                // units ahead, scaled out to where the glow is.
                let ahead = GLOW_DISTANCE_UNITS * 0.0254;
                (g.half * 2.0 * dist / ahead, color)
            }
        };
        let base = color.extend(1.0);
        if sprite.color != base {
            sprite.color = base;
        }
        if materials.get(&material.0).is_some_and(|m| m.params.size != size)
            && let Some(mut m) = materials.get_mut(&material.0)
        {
            m.params.size = size;
        }
    }
}

pub struct BeamMaterialPlugin;

impl Plugin for BeamMaterialPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "beam.wgsl");
        app.add_plugins(MaterialPlugin::<BeamMaterial>::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halo_by_eye() {
        let halo = SpotHalo {
            texture: 0,
            width: 50.0,
            color: Vec3::ONE,
            scale: 60.0,
            proxy: 2.0,
        };
        // Units here: the spec's case (W 50).
        let (half, c) = halo_look(&halo, Vec3::ZERO, Vec3::X, Vec3::new(100.0, 25.0, 0.0));
        // 25 from the axis: h 2; in front, k ≈ 0.97 → (2k)² clamps to 1.
        assert!((half - 120.0).abs() < 1e-3);
        assert_eq!(c, Vec3::ONE);
        let (_, behind) = halo_look(&halo, Vec3::ZERO, Vec3::X, Vec3::new(-100.0, 0.0, 0.0));
        assert_eq!(behind, Vec3::ZERO);
        // k = 0.25: (2k)² = 0.25.
        let eye = Vec3::new(0.25, (1.0f32 - 0.0625).sqrt(), 0.0) * 400.0;
        let (_, quarter) = halo_look(&halo, Vec3::ZERO, Vec3::X, eye);
        assert!((quarter.x - 0.25).abs() < 1e-3, "{quarter}");
    }

    /// de_cbble's chimney smoke (env_beam: 208 units, BoltWidth 25,
    /// TextureScroll 5, framerate 20, no noise).
    fn smoke() -> MapBeam {
        MapBeam {
            entity: None,
            start: Vec3::ZERO,
            end: Vec3::Y * 208.0 * UNIT,
            width: 25.0 * UNIT,
            end_width: 25.0 * UNIT,
            color: Vec3::ONE,
            shade: 2,
            fade: 0.0,
            texture: 0,
            spot: None,
            start_on: true,
            noise: 0.0,
            scroll: 5.0,
            tile: true,
            frames: vec![10, 11, 12, 13],
            frame_start: 0.0,
            frame_rate: 20.0,
        }
    }

    #[test]
    fn beam_strip_segments_tiling_scroll_and_frames() {
        let b = smoke();
        // 1 step 4: int(208 x 0.075 + 3) = 18 points, but segments
        // shorter than 1.414 x the half width 12.5 make it
        // int(208 / 17.675) + 1 = 12.
        assert_eq!(b.points(), 12);
        let p = beam_params_at(&b, 0.0, &[0.0; BEAM_NOISE_SAMPLES]);
        // One repeat per 100 units: 2.08 over the beam's 11 segments.
        assert!((p.strip.w * 11.0 - 2.08).abs() < 1e-4, "{}", p.strip.w);
        // Scroll offset frac(now x s²): 0.1 s at s 5 is 2.5 lengths.
        assert!((b.scroll_offset(0.1) - 0.5).abs() < 1e-4);
        // Frame int(start + now x rate) mod count: 0.27 s at 20/s is 5 -> 1.
        assert_eq!(b.frame(0.27), 1);
        assert!(b.animated());
        // With noise: int(L x 0.25 + 3) points (capped at 128) and the
        // sample scale A x L / 100.
        let noisy = MapBeam {
            noise: 10.0,
            scroll: 0.0,
            frames: Vec::new(),
            width: UNIT,
            end_width: UNIT,
            ..smoke()
        };
        assert_eq!(noisy.points(), 55);
        let mut samples = [0.0; BEAM_NOISE_SAMPLES];
        samples[64] = 0.5;
        samples[128] = -1.0;
        let q = beam_params_at(&noisy, 0.0, &samples);
        assert!((q.strip.y - 10.0 * 2.08 * UNIT).abs() < 1e-6);
        assert_eq!((q.noise[16].x, q.noise[32].x), (0.5, -1.0));
        // A spotlight's: one straight segment, the texture once along it.
        let spot = MapBeam {
            spot: Some(SpotHalo {
                texture: 0,
                width: 0.25,
                color: Vec3::ONE,
                scale: 1.5,
                proxy: 0.05,
            }),
            scroll: 0.0,
            frames: Vec::new(),
            tile: false,
            ..smoke()
        };
        assert_eq!(spot.points(), 2);
        assert_eq!(beam_params(&spot).strip.w, 1.0);
        assert!(!spot.animated());
    }

    #[test]
    fn glow_distance_fade() {
        let g = MapGlow {
            entity: None,
            position: Vec3::ZERO,
            texture: 0,
            color: Vec3::ONE,
            half: Vec2::ONE,
            min: 100.0,
            max: 500.0,
            outer: 0.0,
            front: None,
            proxy: 2.0,
        };
        assert!((glow_fade(&g, 300.0) - 0.5).abs() < 1e-6);
        let outer = MapGlow { outer: 1000.0, ..g.clone() };
        assert!((glow_fade(&outer, 750.0) - 0.5).abs() < 1e-6);
        assert_eq!(glow_fade(&outer, 1200.0), 0.0);
    }
}
