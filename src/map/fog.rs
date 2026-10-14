//! Source's fog for map shaders that draw after the world (ropes, runtime
//! decals, particles): the world's range fog and, under or at the water,
//! the water's fog (specs/cs_source/water.md sections 8 and 9). The maths
//! is `fog.wgsl` (`mashup::fog::source_fog`); world and prop materials
//! carry the same four values in their own uniforms.
//!
//! `SceneFog` holds the current values: the map sets the world part when
//! it spawns, `water::underwater_fog` the water part as the eye moves.
//! Materials take it when created (`crate::render::fog::apply_scene_fog`
//! would update them when it changes).
//!
//! Source's fog shared by map shaders (`mashup::fog::source_fog`): world range
//! fog (in the 3D skybox the sky_camera's own, `map::material_fog`), the
//! water's range fog under water and height fog at the surface; `SceneFog` (set
//! by the map and `water::underwater_fog`) taken by rope, decal and particle
//! materials.

use bevy::prelude::*;

/// Fog uniforms (see fog.wgsl): world fog linear colour (w = 1 when on)
/// and range (start, end meters, max density); water fog linear colour
/// (w: 0 off, 1 range fog under water, 2 height fog at the surface) and
/// range (start, end meters, max density, plane height meters).
#[derive(Clone, Copy, Debug, Default, PartialEq, bevy::render::render_resource::ShaderType)]
pub struct FogUniform {
    pub color: Vec4,
    pub range: Vec4,
    pub water_color: Vec4,
    pub water_range: Vec4,
}

/// Water fog mode in `FogUniform::water_color.w`.
pub const WATER_FOG_OFF: f32 = 0.0;
pub const WATER_FOG_RANGE: f32 = 1.0;
pub const WATER_FOG_HEIGHT: f32 = 2.0;

/// The fog every view-space effect should use now.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct SceneFog(pub FogUniform);

/// The world part of the fog, from the map's fog.
pub fn world_fog(fog: Option<&super::MapFog>) -> FogUniform {
    FogUniform {
        color: super::fog_color(fog),
        range: super::fog_range(fog),
        ..default()
    }
}

/// How much fog a point gets (the shader's `source_fog`, for tests):
/// (colour, amount). `eye` and `point` in meters, `depth` the point's view
/// depth.
pub fn fog_at(f: &FogUniform, eye: Vec3, point: Vec3, depth: f32) -> (Vec3, f32) {
    let range_amount = |r: Vec4| {
        let x = ((depth - r.x) / (r.y - r.x)).min(r.z).clamp(0.0, 1.0);
        x * x
    };
    if f.water_color.w > 0.5 && point.y < f.water_range.w {
        let amount = if f.water_color.w > 1.5 {
            super::water::height_fog(
                eye.y,
                point.y,
                f.water_range.w,
                depth,
                1.0 / (f.water_range.y - f.water_range.x).max(1e-4),
            )
        } else {
            range_amount(f.water_range)
        };
        return (f.water_color.truncate(), amount);
    }
    if f.color.w < 0.5 {
        return (Vec3::ZERO, 0.0);
    }
    (f.color.truncate(), range_amount(f.range))
}

#[derive(Resource)]
struct FogShaderLoaded;

/// Loads `mashup::fog` for the map's shaders. Needs rendering.
pub struct FogShaderPlugin;

impl Plugin for FogShaderPlugin {
    fn build(&self, app: &mut App) {
        // Several material plugins add this one; load the library once.
        if app.world().contains_resource::<FogShaderLoaded>() {
            return;
        }
        app.insert_resource(FogShaderLoaded);
        bevy::shader::load_shader_library!(app, "fog.wgsl");
    }

    fn is_unique(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn water(mode: f32) -> FogUniform {
        FogUniform {
            color: Vec4::new(0.5, 0.5, 0.5, 1.0),
            range: Vec4::new(0.0, 10.0, 1.0, 0.0),
            water_color: Vec4::new(0.0, 0.1, 0.2, mode),
            water_range: Vec4::new(0.0, 1.0, 1.0, 0.0),
        }
    }

    #[test]
    fn world_fog_above_the_plane_water_fog_below() {
        let f = water(WATER_FOG_RANGE);
        // Above the plane: world range fog, f^2 at 5 of 10 m.
        let (c, a) = fog_at(&f, Vec3::new(0.0, -1.0, 0.0), Vec3::new(0.0, 1.0, -5.0), 5.0);
        assert_eq!(c, Vec3::splat(0.5));
        assert!((a - 0.25).abs() < 1e-6);
        // Below: the water's, 0.5 of its 1 m range -> 0.25.
        let (c, a) = fog_at(&f, Vec3::new(0.0, -1.0, 0.0), Vec3::new(0.0, -1.0, -0.5), 0.5);
        assert_eq!(c, Vec3::new(0.0, 0.1, 0.2));
        assert!((a - 0.25).abs() < 1e-6);
        // Off: world fog everywhere.
        let (_, a) = fog_at(&water(WATER_FOG_OFF), Vec3::ZERO, Vec3::new(0.0, -1.0, -5.0), 5.0);
        assert!((a - 0.25).abs() < 1e-6);
    }

    #[test]
    fn height_fog_at_the_surface() {
        // Eye 0.1 m above the plane, a point 0.4 m below and 0.5 m away
        // (depth): 0.8 of the path under water, 0.5 m x K (1/m) -> 0.4,
        // not squared.
        let f = water(WATER_FOG_HEIGHT);
        let (c, a) = fog_at(&f, Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.0, -0.4, -0.5), 0.5);
        assert_eq!(c, Vec3::new(0.0, 0.1, 0.2));
        assert!((a - 0.4).abs() < 1e-5, "{a}");
    }
}
