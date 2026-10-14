//! Materials and meshes built from map meshes: the standard and prop
//! materials, and the fog parameters map shaders take.

use bevy::{asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology};

use super::prop_material::{PropMaterial, PropParams};
#[allow(unused_imports)]
use crate::map::*;

/// Bevy adds `lightmap * lightmap_exposure` as light, then applies the
/// camera's exposure. This cancels the default camera exposure (EV100 9.7)
/// so a lightmap value of 1.0 shows a texture at its own brightness.
pub(crate) const LIGHTMAP_EXPOSURE: f32 = 1.2 * 831.746_4; // 1.2 * 2^9.7

pub(crate) fn build_mesh(m: &MapMesh, with_lightmap: bool) -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, m.positions.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, m.normals.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, m.uvs.clone());
    if with_lightmap {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, m.lightmap_uvs.clone());
    }
    mesh.insert_indices(Indices::U32(m.indices.clone()));
    mesh
}

pub(crate) fn build_material(
    m: &MapMesh,
    textures: &[Handle<Image>],
    view: MapDebugView,
    light_scale: f32,
) -> StandardMaterial {
    let [r, g, b] = m.color;
    let lighting_only = matches!(view, MapDebugView::Lighting { .. });
    let scale = if let MapDebugView::Lighting { scale } = view {
        scale
    } else {
        1.0
    };
    let tint = m.tint.map_or(LinearRgba::WHITE, |[r, g, b]| LinearRgba::rgb(r, g, b));
    StandardMaterial {
        base_color: if m.texture.is_some() || lighting_only {
            tint.into()
        } else {
            {
                let c = Color::srgb_u8(r, g, b).to_linear();
                LinearRgba::rgb(c.red * tint.red, c.green * tint.green, c.blue * tint.blue).into()
            }
        },
        base_color_texture: m.texture.filter(|_| !lighting_only).map(|i| textures[i].clone()),
        unlit: view == MapDebugView::Albedo,
        perceptual_roughness: 0.95,
        reflectance: 0.2,
        alpha_mode: match m.alpha {
            MapAlpha::Opaque => AlphaMode::Opaque,
            MapAlpha::Mask(cutoff) => AlphaMode::Mask(cutoff),
            MapAlpha::Blend => AlphaMode::Blend,
            MapAlpha::Add => AlphaMode::Add,
        },
        double_sided: m.double_sided,
        cull_mode: if m.double_sided {
            None
        } else {
            Some(bevy::render::render_resource::Face::Back)
        },
        lightmap_exposure: LIGHTMAP_EXPOSURE * scale * light_scale,
        ..default()
    }
}

/// The material of a prop mesh lit by a light probe (`PropMaterial`),
/// without its cubemap; `skybox`: in the 3D skybox (the sky_camera's fog).
pub(crate) fn lit_prop_material(
    m: &MapMesh,
    textures: &[Handle<Image>],
    data: &MapData,
    view: MapDebugView,
    skybox: bool,
) -> PropMaterial {
    let [r, g, b] = m.color;
    let lighting_only = matches!(view, MapDebugView::Lighting { .. });
    let tint = m.tint.map_or(Vec4::ONE, |[r, g, b]| Vec4::new(r, g, b, 1.0));
    PropMaterial {
        params: PropParams {
            base_color: if m.texture.is_some() || lighting_only {
                tint
            } else {
                Color::srgb_u8(r, g, b).to_linear().to_vec4() * tint
            },
            alpha_cutoff: if let MapAlpha::Mask(c) = m.alpha { c } else { 0.0 },
            fog_color: fog_color(material_fog(data, view, skybox).as_ref()),
            fog_range: fog_range(material_fog(data, view, skybox).as_ref()),
            translucent: m.alpha.shader_mode(),
            dynamic: if m.unlit || view != MapDebugView::Normal {
                0.0
            } else {
                1.0
            },
            base_uv_u: m.base_transform.shader_rows()[0],
            base_uv_v: m.base_transform.shader_rows()[1],
            detail: m
                .detail
                .filter(|_| !lighting_only)
                .map_or(0.0, |d| d.mode.shader_value()),
            detail_factor: m.detail.map_or(0.0, |d| d.factor),
            detail_scale: m.detail.map_or(Vec2::ONE, |d| Vec2::from_array(d.scale)),
            detail_tint: m.detail.map_or(Vec4::ONE, |d| Vec3::from_array(d.tint).extend(1.0)),
            selfillum: match m.selfillum {
                Some(MapSelfIllum { mask: Some(_), .. }) => 2.0,
                Some(_) => 1.0,
                None => 0.0,
            },
            selfillum_tint: m.selfillum.map_or(Vec4::ONE, |s| Vec3::from_array(s.tint).extend(1.0)),
            ..default()
        },
        base: m.texture.filter(|_| !lighting_only).map(|i| textures[i].clone()),
        envmap: None,
        envmap_mask: None,
        detail: m.detail.filter(|_| !lighting_only).map(|d| textures[d.texture].clone()),
        selfillum_mask: m.selfillum.and_then(|s| s.mask).map(|i| textures[i].clone()),
        alpha_mode: m.alpha.shader_alpha_mode(),
        double_sided: m.double_sided,
        cull_front: false,
    }
}

/// Make `material` reflect `cube` as `env` says (Source `$envmap`).
pub(crate) fn set_prop_envmap(
    material: &mut PropMaterial,
    env: &MapEnvmap,
    cube: Handle<Image>,
    textures: &[Handle<Image>],
) {
    material.envmap = Some(cube);
    material.params.envmap = 1.0;
    material.params.envmap_mask = match env.mask {
        EnvmapMask::BaseAlphaInverted => 2.0,
        EnvmapMask::Texture(_) => 3.0,
        _ => 0.0,
    };
    if let EnvmapMask::Texture(t) = env.mask {
        material.envmap_mask = textures.get(t).cloned();
    }
    material.params.envmap_contrast = env.contrast;
    material.params.envmap_saturation = env.saturation;
    material.params.envmap_tint = Vec3::from_array(env.tint.map(parameter_gamma_to_linear)).extend(1.0);
}

/// Fog color for shaders: linear RGB, w = 1 when fog is on.
pub(crate) fn fog_color(fog: Option<&MapFog>) -> Vec4 {
    fog.map_or(Vec4::ZERO, |f| {
        let c = Color::srgb(f.color[0], f.color[1], f.color[2]).to_linear();
        Vec4::new(c.red, c.green, c.blue, 1.0)
    })
}

/// The fog a map material gets: the world's, or in the 3D skybox the
/// sky_camera's own (specs/cs_source/shadows_sky.md: the skybox pass draws
/// with it), its distances from world scale to the skybox's (divided by
/// the scale: the sky camera sees the skybox from `eye / scale`). None in
/// debug views.
pub fn material_fog(data: &MapData, view: MapDebugView, skybox: bool) -> Option<MapFog> {
    if view != MapDebugView::Normal {
        return None;
    }
    if !skybox {
        return data.fog.clone();
    }
    let cam = data.sky_camera.as_ref()?;
    cam.fog.as_ref().map(|f| MapFog {
        start: f.start / cam.scale,
        end: f.end / cam.scale,
        ..f.clone()
    })
}

/// Fog start, end (meters) and max density for shaders.
pub(crate) fn fog_range(fog: Option<&MapFog>) -> Vec4 {
    fog.map_or(Vec4::new(0.0, 1.0, 0.0, 0.0), |f| {
        Vec4::new(f.start, f.end.max(f.start + 0.01), f.max_density, 0.0)
    })
}

#[cfg(test)]
mod fog_tests {
    use super::*;

    fn fog(start: f32, end: f32) -> MapFog {
        MapFog {
            color: [0.5, 0.6, 0.7],
            start,
            end,
            max_density: 0.8,
        }
    }

    /// The world's materials take the world's fog; the 3D skybox's take
    /// the sky_camera's, its distances in skybox space (divided by the
    /// scale); debug views none.
    #[test]
    fn skybox_materials_take_the_sky_cameras_fog() {
        let data = MapData {
            fog: Some(fog(10.0, 100.0)),
            sky_camera: Some(MapSkyCamera {
                origin: Vec3::ZERO,
                scale: 4.0,
                fog: Some(fog(12.0, 380.0)),
            }),
            ..default()
        };
        let world = material_fog(&data, MapDebugView::Normal, false).unwrap();
        assert_eq!((world.start, world.end), (10.0, 100.0));
        let sky = material_fog(&data, MapDebugView::Normal, true).unwrap();
        assert_eq!((sky.start, sky.end, sky.max_density), (3.0, 95.0, 0.8));
        assert_eq!(fog_color(Some(&sky)).w, 1.0, "on");
        assert!(material_fog(&data, MapDebugView::Albedo, true).is_none());
        // No sky_camera fog: the skybox is unfogged, whatever the world's.
        let unfogged = MapData {
            sky_camera: Some(MapSkyCamera {
                origin: Vec3::ZERO,
                scale: 16.0,
                fog: None,
            }),
            ..data
        };
        assert!(material_fog(&unfogged, MapDebugView::Normal, true).is_none());
        assert!(material_fog(&unfogged, MapDebugView::Normal, false).is_some());
    }
}
