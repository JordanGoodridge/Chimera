//! Only the last camera drawing into a target copies the frame out to it
//! (`render::cameras::skip_covered_outputs`): the 3D sky's, the world's and
//! the view model's copies under the UI camera's are skipped
//! (`CameraOutputMode::Skip`); a camera turned off, one drawing into an
//! image (a capture), one with other HDR or MSAA settings (other
//! intermediate textures) or a part of the target (a viewport) changes who
//! copies; a camera with its own output mode keeps it. The camera left
//! copying for skipped ones replaces the target instead of Bevy's blend
//! over it (else the frame's alpha shows the clear colour: de_piranesi's
//! grey sky, 2026-10-28 playtest). And every camera is
//! batched on the CPU (`NoIndirectDrawing`, `render::cameras::CamerasPlugin`).

use bevy::{
    camera::{CameraOutputMode, Hdr, ImageRenderTarget, RenderTarget, Viewport},
    prelude::*,
    render::{render_resource::BlendState, view::NoIndirectDrawing},
    window::PrimaryWindow,
};
use chimera_present::render::cameras::{CamerasPlugin, skip_covered_outputs};

fn app() -> App {
    let mut app = App::new();
    app.add_systems(Update, skip_covered_outputs);
    app.world_mut().spawn((Window::default(), PrimaryWindow));
    app
}

fn camera(app: &mut App, order: isize) -> Entity {
    app.world_mut().spawn(Camera { order, ..default() }).id()
}

fn skipped(app: &App, camera: Entity) -> bool {
    matches!(
        app.world().get::<Camera>(camera).unwrap().output_mode,
        CameraOutputMode::Skip
    )
}

fn set(app: &mut App, camera: Entity, f: impl FnOnce(&mut Camera)) {
    f(&mut app.world_mut().get_mut::<Camera>(camera).unwrap());
}

#[test]
fn only_the_last_camera_on_the_window_copies_out() {
    let mut app = app();
    let (sky, world, view_model, ui) = (
        camera(&mut app, -1),
        camera(&mut app, 0),
        camera(&mut app, 1),
        camera(&mut app, 100),
    );
    app.update();
    assert!(skipped(&app, sky) && skipped(&app, world) && skipped(&app, view_model));
    assert!(!skipped(&app, ui), "the UI camera copies the whole frame out");
    // The UI camera off: the view model's copies, then the world's.
    set(&mut app, ui, |c| c.is_active = false);
    app.update();
    assert!(!skipped(&app, view_model) && skipped(&app, world) && skipped(&app, sky));
    set(&mut app, view_model, |c| c.is_active = false);
    app.update();
    assert!(!skipped(&app, world) && skipped(&app, sky));
    // Back on.
    set(&mut app, ui, |c| c.is_active = true);
    app.update();
    assert!(skipped(&app, world) && !skipped(&app, ui));
}

fn replaces(app: &App, camera: Entity) -> bool {
    matches!(
        app.world().get::<Camera>(camera).unwrap().output_mode,
        CameraOutputMode::Write { blend_state: Some(b), .. } if b == BlendState::REPLACE
    )
}

fn bevy_default(app: &App, camera: Entity) -> bool {
    matches!(
        app.world().get::<Camera>(camera).unwrap().output_mode,
        CameraOutputMode::Write {
            blend_state: None,
            clear_color: ClearColorConfig::Default
        }
    )
}

/// Bevy alpha-blends the copy of every camera but a target's bottom one
/// over what the target holds (its clear colour, the bottom copies being
/// skipped): the one copying must replace it.
#[test]
fn the_camera_copying_for_skipped_ones_replaces_the_target() {
    let mut app = app();
    let (sky, world, view_model, ui) = (
        camera(&mut app, -1),
        camera(&mut app, 0),
        camera(&mut app, 1),
        camera(&mut app, 100),
    );
    app.update();
    assert!(
        replaces(&app, ui),
        "the UI camera copies the sky's, world's and view model's frame"
    );
    set(&mut app, ui, |c| c.is_active = false);
    app.update();
    assert!(replaces(&app, view_model) && bevy_default(&app, ui));
    // The sky camera off (a leaf that sees no sky): the world's is the bottom.
    set(&mut app, sky, |c| c.is_active = false);
    set(&mut app, view_model, |c| c.is_active = false);
    app.update();
    assert!(
        bevy_default(&app, world) && bevy_default(&app, view_model) && bevy_default(&app, sky),
        "one camera: Bevy's own copy"
    );
    set(&mut app, sky, |c| c.is_active = true);
    set(&mut app, ui, |c| c.is_active = true);
    app.update();
    assert!(skipped(&app, sky) && skipped(&app, world) && replaces(&app, ui));
    // A lower camera on the target still copying its own (other textures):
    // the copy over it blends, as Bevy's.
    app.world_mut().entity_mut(sky).insert(Hdr);
    app.update();
    assert!(!skipped(&app, sky) && skipped(&app, world));
    assert!(bevy_default(&app, ui), "blends over the sky camera's copy");
}

#[test]
fn other_targets_textures_and_viewports_copy_their_own() {
    let mut app = app();
    let world = camera(&mut app, 0);
    let ui = camera(&mut app, 100);
    // A capture: the world drawn into an image, the UI still on the window.
    let image = RenderTarget::Image(ImageRenderTarget::from(Handle::<Image>::default()));
    app.world_mut().entity_mut(world).insert(image.clone());
    app.update();
    assert!(!skipped(&app, world) && !skipped(&app, ui), "one camera on each target");
    // On the window again, but in HDR without the UI camera: other textures.
    app.world_mut().entity_mut(world).remove::<RenderTarget>().insert(Hdr);
    app.update();
    assert!(!skipped(&app, world), "different intermediate textures");
    app.world_mut().entity_mut(ui).insert(Hdr);
    app.update();
    assert!(skipped(&app, world));
    app.world_mut().entity_mut(ui).insert(Msaa::Off);
    app.update();
    assert!(!skipped(&app, world), "other MSAA: other textures");
    app.world_mut().entity_mut(ui).insert(Msaa::default());
    // A camera drawing a part of the window covers nothing.
    set(&mut app, ui, |c| {
        c.viewport = Some(Viewport {
            physical_size: UVec2::new(10, 10),
            ..default()
        })
    });
    app.update();
    assert!(!skipped(&app, world));
}

#[test]
fn a_camera_with_its_own_output_mode_keeps_it() {
    let mut app = app();
    let blended = camera(&mut app, 0);
    let mode = CameraOutputMode::Write {
        blend_state: Some(BlendState::ALPHA_BLENDING),
        clear_color: ClearColorConfig::None,
    };
    set(&mut app, blended, |c| c.output_mode = mode);
    camera(&mut app, 100);
    app.update();
    assert!(matches!(
        app.world().get::<Camera>(blended).unwrap().output_mode,
        CameraOutputMode::Write {
            blend_state: Some(_),
            ..
        }
    ));
}

#[test]
fn every_camera_is_batched_on_the_cpu() {
    let mut app = App::new();
    app.add_plugins(CamerasPlugin);
    let camera = app.world_mut().spawn(Camera3d::default()).id();
    let ui = app.world_mut().spawn(Camera2d).id();
    assert!(app.world().get::<NoIndirectDrawing>(camera).is_some());
    assert!(app.world().get::<NoIndirectDrawing>(ui).is_some());
}
