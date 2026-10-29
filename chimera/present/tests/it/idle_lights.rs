//! Bevy's directional-light shadow visibility, run only while a sun casts
//! shadows (`render::idle_lights`): the system is still in `PostUpdate`
//! once, in its place, and runs again when shadows go on.

use bevy::{
    camera::{CameraPlugin, visibility::CascadesVisibleEntities},
    light::LightPlugin,
    prelude::*,
};
use chimera_present::render::idle_lights::IdleLightsPlugin;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), CameraPlugin, LightPlugin))
        .init_asset::<Mesh>()
        .init_asset::<Image>()
        .init_asset::<bevy::gizmos::GizmoAsset>()
        .init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
        .add_plugins(IdleLightsPlugin);
    app.finish();
    app.cleanup();
    app
}

#[test]
fn the_dir_light_check_stays_in_place_once() {
    let mut app = app();
    let systems = chimera_world::harness::schedule_systems(app.world_mut(), PostUpdate);
    let n = systems.iter().filter(|s| s.contains("check_dir_light_mesh_visibility")).count();
    assert_eq!(n, 1, "{systems:#?}");
    app.update();
}

#[test]
fn it_runs_while_a_sun_casts_shadows() {
    let mut app = app();
    let camera = app.world_mut().spawn(Camera3d::default()).id();
    let sun = app
        .world_mut()
        .spawn((DirectionalLight { shadow_maps_enabled: true, ..default() }, Transform::default()))
        .id();
    app.update();
    app.update();
    // Bevy's system lists the camera's cascades for a shadow-casting sun.
    assert!(app.world().get::<CascadesVisibleEntities>(sun).unwrap().entities.contains_key(&camera));
    // Shadows off: it runs once more to clear them, then not at all.
    app.world_mut().get_mut::<DirectionalLight>(sun).unwrap().shadow_maps_enabled = false;
    app.update();
    assert!(app.world().get::<CascadesVisibleEntities>(sun).unwrap().entities.is_empty());
    app.update();
    assert!(app.world().get::<CascadesVisibleEntities>(sun).unwrap().entities.is_empty());
}
