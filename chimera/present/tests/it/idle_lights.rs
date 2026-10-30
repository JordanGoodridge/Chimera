//! Bevy's directional-light shadow visibility, run only while a sun casts
//! shadows (`render::idle_lights`): the system is still in `PostUpdate`
//! once, in its place, and runs again when shadows go on; and the swap
//! leaves `PostUpdate` unbuilt until every plugin is in (a system there
//! initializes with what a later plugin's `finish` registered: replicon's
//! replication query, a listen server's crash of 2026-10-28).

use bevy::{
    camera::{CameraPlugin, visibility::CascadesVisibleEntities},
    light::LightPlugin,
    prelude::*,
};
use chimera_present::render::idle_lights::IdleLightsPlugin;

fn app() -> App {
    app_with(|_| {})
}

/// The plugins, `more` after `IdleLightsPlugin`, finished.
fn app_with(more: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, AssetPlugin::default(), CameraPlugin, LightPlugin))
        .init_asset::<Mesh>()
        .init_asset::<Image>()
        .init_asset::<bevy::gizmos::GizmoAsset>()
        .init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
        .add_plugins(IdleLightsPlugin);
    more(&mut app);
    app.finish();
    app.cleanup();
    app
}

#[test]
fn the_dir_light_check_stays_in_place_once() {
    let mut app = app();
    app.update();
    let systems = chimera_world::harness::schedule_systems(app.world_mut(), PostUpdate);
    let n = systems.iter().filter(|s| s.contains("check_dir_light_mesh_visibility")).count();
    assert_eq!(n, 1, "{systems:#?}");
    app.update();
}

/// What a plugin's `finish` registers (net's parts, for replicon).
#[derive(Resource, Default)]
struct Registered(u32);

/// Read once, when its system is initialized (as replicon's query reads
/// the replication rules).
struct SeenAtInit(u32);

impl FromWorld for SeenAtInit {
    fn from_world(world: &mut World) -> Self {
        Self(world.get_resource::<Registered>().map_or(0, |r| r.0))
    }
}

#[derive(Resource, Default)]
struct Seen(u32);

struct LaterPlugin;

impl Plugin for LaterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Seen>()
            .add_systems(PostUpdate, |at_init: Local<SeenAtInit>, mut seen: ResMut<Seen>| seen.0 = at_init.0);
    }

    fn finish(&self, app: &mut App) {
        app.insert_resource(Registered(7));
    }
}

#[test]
fn post_update_initializes_after_every_plugins_finish() {
    let mut app = app_with(|app| {
        app.add_plugins(LaterPlugin);
    });
    app.update();
    assert_eq!(app.world().resource::<Seen>().0, 7, "a later plugin's finish seen");
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
