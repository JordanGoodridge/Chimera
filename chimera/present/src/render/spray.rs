//! Drawing players' sprays (`map::decal::Spray`, placed by the simulation's
//! `weapon::spray`): each logo (`map::decal::MapDecals::logos`, from this
//! install) projected onto the world triangles around its point like a
//! runtime decal (`decal::project`), alpha blended and lit by the baked
//! light where it lies (`map::surface_color::SurfaceColors`), as the game
//! draws a lightmapped translucent decal. The mesh is the spray entity's
//! child, so a replaced or removed spray takes it along. Another player's
//! logo named `map::decal::CUSTOM_LOGO` (its own import) isn't drawn: this
//! install's file of that name is a different picture.
//!
//! Players' sprays drawn (`SprayMaterial`, `spray.wgsl`): the logo projected
//! onto the world near the spray, alpha blended, times the baked light at its
//! centre; a child of the `map::decal::Spray` entity; others' imported logos
//! (`CUSTOM_LOGO`) left out.
//!
//! Known shortcuts (docs/tech-debt.md): Sprays drawn: one light value (the
//! lightmap at the centre) for the whole logo, no fog, no dynamic lights;
//! orientation as `weapon::spray` gives it. Why it's OK for now: Sprays read
//! right on lit walls; compare with the game in capture C23.

use bevy::{
    asset::embedded_asset, prelude::*, reflect::TypePath, render::render_resource::AsBindGroup, shader::ShaderRef,
};

use super::decal::{DecalAssets, DecalSurfaces, project};
use crate::{
    core::LocalPlayer,
    map::{
        MapPart,
        decal::{CUSTOM_LOGO, Spray},
        surface_color::SurfaceColors,
    },
};

/// A spray's mesh, under its `Spray` entity.
#[derive(Component)]
pub struct SprayDrawn;

/// Draw sprays that are new or moved (once the map's logos and surfaces
/// are there).
#[allow(clippy::too_many_arguments)]
pub fn draw_sprays(
    sprays: Query<(Entity, Ref<Spray>, Option<&Children>)>,
    drawn: Query<(), With<SprayDrawn>>,
    local: Query<(), With<LocalPlayer>>,
    surfaces: Option<Res<DecalSurfaces>>,
    assets: Option<Res<DecalAssets>>,
    colors: Option<Res<SurfaceColors>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<SprayMaterial>>>,
    mut done: Local<Vec<Entity>>,
    mut commands: Commands,
) {
    let (Some(surfaces), Some(assets), Some(mut meshes), Some(mut materials)) = (surfaces, assets, meshes, materials)
    else {
        done.clear();
        return;
    };
    done.retain(|e| sprays.contains(*e));
    for (e, spray, children) in &sprays {
        if done.contains(&e) && !spray.is_changed() {
            continue;
        }
        if !done.contains(&e) {
            done.push(e);
        }
        for child in children.into_iter().flatten().filter(|c| drawn.contains(**c)) {
            commands.entity(*child).despawn();
        }
        let logo = spray.logo.to_lowercase();
        if logo == CUSTOM_LOGO && !local.contains(spray.owner) {
            continue;
        }
        let Some(&index) = assets.data.logos.get(&logo) else {
            continue;
        };
        let decal = assets.data.decals[index].clone();
        let Some(texture) = assets.textures.get(decal.texture).cloned() else {
            continue;
        };
        let normal = spray.normal.normalize_or_zero();
        let up = spray.up.normalize_or_zero();
        let right = up.cross(normal).normalize_or_zero();
        let down = -up;
        let Some(mesh) = project(&surfaces.world, &decal, spray.point, normal, right, down) else {
            continue;
        };
        let light = colors
            .as_ref()
            .and_then(|c| c.world(spray.point, normal))
            .map_or(Vec3::ONE, |l| l.light);
        let material = materials.add(SprayMaterial {
            light: light.extend(1.0),
            texture,
        });
        // A client's replicated spray has no transform of its own.
        commands
            .entity(e)
            .insert_if_new((Transform::default(), Visibility::default()))
            .with_child((
                Name::new("Spray drawn"),
                SprayDrawn,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material),
                Transform::default(),
                Visibility::default(),
                MapPart,
            ));
    }
}

/// A spray's look: its logo, alpha blended, times the light where it lies.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct SprayMaterial {
    /// rgb: the baked light at the spray (linear, 1 = the logo's own
    /// colour).
    #[uniform(0)]
    pub light: Vec4,
    #[texture(1)]
    #[sampler(2)]
    pub texture: Handle<Image>,
}

impl Material for SprayMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://chimera_present/render/spray.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }
}

pub struct SprayMaterialPlugin;

impl Plugin for SprayMaterialPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "spray.wgsl");
        app.add_plugins(MaterialPlugin::<SprayMaterial>::default());
    }
}
