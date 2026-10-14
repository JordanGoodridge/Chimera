//! The material for sprites (`MapSprite`): every vertex sits at the
//! sprite's centre and the vertex shader spreads the corners along the
//! view's right and up axes, so the quad stays parallel to the view plane.
//! Added to the image (one, one); glows skip the depth test; sprites in
//! the colour, texture and solid render modes blend by their alpha, in
//! normal mode they are opaque (specs/cs_source/sprites_dust.md 3). The
//! shader reads the scene behind (the view transmission texture) so the
//! sum or blend is formed in gamma space, as in the game. See sprite.wgsl.
//!
//! Known shortcuts (docs/tech-debt.md): Sprites: glow visibility is 5 line
//! tests to the occlusion proxy (the game uses GPU occlusion queries) with no
//! fade-in; sprites add in gamma space by reading the scene behind them, so
//! they draw in Bevy's transmissive phase (before translucent surfaces, unlike
//! the game's glows) and overlapping sprites each add their own difference;
//! blended (1, 2, 4) and normal (0) sprites write the finished pixel from that
//! copy of the scene, so of two overlapping ones only the nearer shows; render
//! modes 7 and 8 aren't drawn. Why it's OK for now: dust2's lamp glows match
//! CS:S closely (refcmp glow_lamp).

use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    mesh::Indices,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{
        AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, CompareFunction, PrimitiveTopology,
    },
    shader::ShaderRef,
};

pub use crate::map::SpriteBlend;

#[derive(Clone, Copy, Debug, Default, bevy::render::render_resource::ShaderType)]
pub struct SpriteParams {
    /// RGB scale and alpha weight.
    pub color: Vec4,
    /// Full width and height, meters.
    pub size: Vec2,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[bind_group_data(SpriteMaterialKey)]
pub struct SpriteMaterial {
    #[uniform(0)]
    pub params: SpriteParams,
    #[texture(1)]
    #[sampler(2)]
    pub texture: Option<Handle<Image>>,
    /// Drawn over everything (glows); otherwise depth tested.
    pub glow: bool,
    /// How it meets what is behind.
    pub blend: SpriteBlend,
}

impl Material for SpriteMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://engine_present/render/sprite.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "embedded://engine_present/render/sprite.wgsl".into()
    }

    /// Opaque here only so Bevy draws sprites in the transmissive phase,
    /// which provides the scene behind them; `specialize` sets the additive
    /// blend and turns depth writes off. (Blended alpha modes always go to
    /// the transparent phase, which gets no copy of the scene.)
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }

    /// The shader reads the scene behind the sprite to add in gamma space,
    /// as the game does.
    fn reads_view_transmission_texture(&self) -> bool {
        true
    }

    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        // Added: the shader outputs what to add. Blended and opaque: it
        // outputs the finished pixel (it read the scene behind).
        let add = match key.bind_group_data.blend {
            SpriteBlend::Add => BlendComponent {
                src_factor: BlendFactor::One,
                dst_factor: BlendFactor::One,
                operation: BlendOperation::Add,
            },
            SpriteBlend::Blend | SpriteBlend::Opaque => BlendComponent::REPLACE,
        };
        if key.bind_group_data.blend != SpriteBlend::Add {
            if let Some(fragment) = descriptor.fragment.as_mut() {
                fragment.shader_defs.push("SPRITE_REPLACE".into());
                if key.bind_group_data.blend == SpriteBlend::Opaque {
                    fragment.shader_defs.push("SPRITE_OPAQUE".into());
                }
            }
        }
        if let Some(fragment) = descriptor.fragment.as_mut() {
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = Some(BlendState { color: add, alpha: add });
            }
        }
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(key.bind_group_data.blend == SpriteBlend::Opaque);
            if key.bind_group_data.glow {
                depth.depth_compare = Some(CompareFunction::Always);
            }
        }
        Ok(())
    }
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpriteMaterialKey {
    glow: bool,
    blend: SpriteBlend,
}

impl From<&SpriteMaterial> for SpriteMaterialKey {
    fn from(m: &SpriteMaterial) -> Self {
        Self {
            glow: m.glow,
            blend: m.blend,
        }
    }
}

/// A quad whose four vertices sit at the origin; UV_1 holds each corner
/// (-0.5..0.5) for the vertex shader.
pub fn sprite_mesh(texture_size: UVec2) -> Mesh {
    // Texture coordinates inset by half a texel, as the game does.
    let (du, dv) = (0.5 / texture_size.x.max(1) as f32, 0.5 / texture_size.y.max(1) as f32);
    let corners = [[-0.5, -0.5], [-0.5, 0.5], [0.5, 0.5], [0.5, -0.5]];
    let uvs = [[du, 1.0 - dv], [du, dv], [1.0 - du, dv], [1.0 - du, 1.0 - dv]];
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 4]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; 4]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs.to_vec());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, corners.to_vec());
    mesh.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    mesh
}

pub struct SpriteMaterialPlugin;

impl Plugin for SpriteMaterialPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "sprite.wgsl");
        app.add_plugins(MaterialPlugin::<SpriteMaterial>::default());
    }
}
