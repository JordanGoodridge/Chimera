//! Beams and glows from map entities, for any game (specs/source/
//! visual_entities.md): point_spotlight's shaft and halo (2), env_laser and
//! env_beam's beam between two points (3, 4.1), env_lightglow's
//! screen-sized glow (6). Beams are drawn by `BeamMaterial` (beam.wgsl);
//! halos and light glows are glow sprites (`GlowSprite`, faded by how
//! much of their occlusion proxy is seen) whose colour and size follow
//! the eye each frame. The logic switches them through `EntityPart`.

use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    mesh::Indices,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, PrimitiveTopology},
    shader::ShaderRef,
};

use super::{
    emitters::{BEAM_MAX_SEGMENTS, BEAM_NOISE_SAMPLES, beam_segments, fractal_noise},
    particles::ParticleRng,
    sprite_material::SpriteMaterial,
};

/// A beam between two points (engine space, meters).
#[derive(Clone, Debug, PartialEq)]
pub struct MapBeam {
    /// The entity (index into `MapData::entities`) the logic switches it
    /// by.
    pub entity: Option<usize>,
    pub start: Vec3,
    pub end: Vec3,
    /// Widths at the start and end, meters (the strip is twice as wide).
    pub width: f32,
    pub end_width: f32,
    /// Colour x brightness, 0..1.
    pub color: Vec3,
    /// Shading: 1 out, 2 in, 3 both, 0 none; over `fade` meters (0: the
    /// whole beam).
    pub shade: u8,
    pub fade: f32,
    /// Index into `MapData::textures`.
    pub texture: usize,
    /// A spotlight's: the near-axis fade width (meters) and its halo.
    pub spot: Option<SpotHalo>,
    pub start_on: bool,
    /// Noise amplitude A (0-64; 0: a straight beam): the strip's points
    /// move sideways by fractal noise x A x the length / 100, new every
    /// frame (1 step 5).
    pub noise: f32,
    /// TextureScroll s: the texture slides along the beam, its offset
    /// frac(now x s x s) (1 step 8: map beams scroll at s² lengths a
    /// second, a quirk kept).
    pub scroll: f32,
    /// The texture repeats every 100 units along the beam; false: once
    /// over its length ("no tile", spotlights).
    pub tile: bool,
    /// The sprite's frames (indices into `MapData::textures`; empty: a
    /// still `texture`), the start frame and frames a second (1 step 3).
    pub frames: Vec<usize>,
    pub frame_start: f32,
    pub frame_rate: f32,
}

/// One unit, meters.
const UNIT: f32 = 0.0254;

/// Fractal noise samples as the shader reads them (four to a vector).
pub const NOISE_VECTORS: usize = BEAM_NOISE_SAMPLES.div_ceil(4);

/// A point_spotlight's halo at the beam's start (2.3).
#[derive(Clone, Debug, PartialEq)]
pub struct SpotHalo {
    pub texture: usize,
    /// The spotlight's width W, meters.
    pub width: f32,
    /// rendercolor / 255.
    pub color: Vec3,
    /// The halo scale (60) in meters: half-size h x this.
    pub scale: f32,
    /// Occlusion proxy, meters.
    pub proxy: f32,
}

/// An env_lightglow (6): engine space, meters.
#[derive(Clone, Debug, PartialEq)]
pub struct MapGlow {
    pub entity: Option<usize>,
    pub position: Vec3,
    pub texture: usize,
    /// rendercolor / 255.
    pub color: Vec3,
    /// Half extents (horizontal, vertical) of the quad 100 units ahead,
    /// meters.
    pub half: Vec2,
    /// MinDist, MaxDist, OuterMaxDist, meters.
    pub min: f32,
    pub max: f32,
    pub outer: f32,
    /// "Visible only from front": its forward direction.
    pub front: Option<Vec3>,
    pub proxy: f32,
}

/// A burst of sparks (env_spark; visual_entities.md 8): where (engine
/// space), its direction (zero: none) and magnitude; a game draws it.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct SparkBurst {
    pub at: Vec3,
    pub dir: Vec3,
    pub magnitude: f32,
}

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
        "embedded://mashup/map/beam.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "embedded://mashup/map/beam.wgsl".into()
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

impl MapBeam {
    /// Its length, units.
    fn length_units(&self) -> f32 {
        self.start.distance(self.end) / UNIT
    }

    /// The points its strip is drawn through (1 step 4; a spotlight's
    /// beam: one straight segment, 2.3).
    pub fn points(&self) -> usize {
        if self.spot.is_some() {
            return 2;
        }
        beam_segments(self.length_units(), self.width / UNIT, self.end_width / UNIT, self.noise).max(2)
    }

    /// The texture's offset along the beam at `now` seconds (1 step 8).
    pub fn scroll_offset(&self, now: f32) -> f32 {
        (now as f64 * self.scroll as f64 * self.scroll as f64).rem_euclid(1.0) as f32
    }

    /// The sprite frame shown at `now` (1 step 3), an index into `frames`.
    pub fn frame(&self, now: f32) -> usize {
        let count = self.frames.len().max(1);
        ((self.frame_start + now * self.frame_rate).max(0.0) as usize) % count
    }

    /// Whether its look changes from frame to frame (noise, scroll or an
    /// animated sprite).
    pub fn animated(&self) -> bool {
        self.noise != 0.0 || self.scroll != 0.0 || (self.frames.len() > 1 && self.frame_rate != 0.0)
    }

    /// Shader parameters at `now` seconds, with this frame's noise samples
    /// (zeros: straight).
    pub fn params_at(&self, now: f32, noise: &[f32; BEAM_NOISE_SAMPLES]) -> BeamParams {
        let n = self.points();
        let length = self.length_units();
        let step = if self.tile { length / 100.0 } else { 1.0 } / (n - 1) as f32;
        let mut packed = [Vec4::ZERO; NOISE_VECTORS];
        for (i, v) in noise.iter().enumerate() {
            packed[i / 4][i % 4] = *v;
        }
        BeamParams {
            start: self.start.extend(1.0),
            end: self.end.extend(1.0),
            color: self.color.extend(self.shade as f32),
            widths: Vec4::new(
                self.width,
                self.end_width,
                self.fade,
                self.spot.as_ref().map_or(0.0, |s| s.width),
            ),
            strip: Vec4::new(n as f32, self.noise * length / 100.0 * UNIT, self.scroll_offset(now), step),
            noise: packed,
        }
    }

    pub fn params(&self) -> BeamParams {
        self.params_at(0.0, &[0.0; BEAM_NOISE_SAMPLES])
    }
}

/// A beam whose look moves: its noise, scroll and sprite frames, redrawn
/// every frame (`animate_beams`).
#[derive(Component)]
pub(super) struct BeamMotion {
    pub beam: MapBeam,
    /// The sprite's frames (empty: a still texture).
    pub frames: Vec<Handle<Image>>,
    pub rng: ParticleRng,
}

/// Moves animated beams: new noise every frame, the texture's scroll and
/// the sprite's frame (specs/source/visual_entities.md 1 steps 3, 5, 8).
pub(super) fn animate_beams(
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
        let params = m.beam.params_at(now, &noise);
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
pub(super) enum EyeGlow {
    Halo { halo: SpotHalo, dir: Vec3 },
    Light(MapGlow),
}

/// Halos and light glows follow the eye: their base colour (before the
/// occlusion fade, `GlowSprite`) and their size.
pub(super) fn follow_eye(
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
    mut glows: Query<(&GlobalTransform, &EyeGlow, &mut super::GlowSprite, &MeshMaterial3d<SpriteMaterial>)>,
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
        let p = b.params_at(0.0, &[0.0; BEAM_NOISE_SAMPLES]);
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
        let q = noisy.params_at(0.0, &samples);
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
        assert_eq!(spot.params().strip.w, 1.0);
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
