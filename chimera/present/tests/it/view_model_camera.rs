//! The view-model camera draws only when something is on its layer
//! (`render::view_model::view_model_cameras_draw`): with no view model, no
//! shell, no under-water overlay (spectating, dead, menus,
//! `r_drawviewmodel 0`) it is off, so the frame skips its passes and its
//! MSAA copy; it is on while its anchor's camera draws and something on
//! `VIEW_MODEL_LAYER` is visible, and always in HDR.

use bevy::{camera::{Hdr, visibility::RenderLayers}, prelude::*};
use chimera_present::render::view_model::view_model_cameras_draw;
use chimera_world::map::{ViewModelAnchor, ViewModelCamera, view_model::VIEW_MODEL_LAYER};

fn app() -> (App, Entity, Entity) {
    let mut app = App::new();
    app.add_systems(Update, view_model_cameras_draw);
    let anchor = app.world_mut().spawn((Camera::default(), ViewModelAnchor)).id();
    let camera = app
        .world_mut()
        .spawn((Camera::default(), ViewModelCamera, ChildOf(anchor)))
        .id();
    (app, anchor, camera)
}

fn active(app: &App, camera: Entity) -> bool {
    app.world().get::<Camera>(camera).unwrap().is_active
}

fn drawn(app: &mut App, layers: RenderLayers, visible: bool) -> Entity {
    let v = if visible {
        InheritedVisibility::VISIBLE
    } else {
        InheritedVisibility::HIDDEN
    };
    app.world_mut().spawn((Mesh3d(Handle::default()), layers, v)).id()
}

#[test]
fn off_with_nothing_on_its_layer() {
    let (mut app, _, camera) = app();
    app.update();
    assert!(!active(&app, camera), "nothing to draw");
    // A hidden view model (r_drawviewmodel 0, dead) and the world's meshes
    // don't count.
    drawn(&mut app, RenderLayers::layer(VIEW_MODEL_LAYER), false);
    drawn(&mut app, RenderLayers::layer(0), true);
    app.update();
    assert!(!active(&app, camera));
}

#[test]
fn on_while_something_there_is_visible() {
    let (mut app, _, camera) = app();
    let model = drawn(&mut app, RenderLayers::layer(VIEW_MODEL_LAYER), true);
    app.update();
    assert!(active(&app, camera), "a view model, shell or overlay");
    app.world_mut().entity_mut(model).insert(InheritedVisibility::HIDDEN);
    app.update();
    assert!(!active(&app, camera), "hidden again");
    // On both layers (a muzzle flash's sprite).
    drawn(&mut app, RenderLayers::from_layers(&[0, VIEW_MODEL_LAYER]), true);
    app.update();
    assert!(active(&app, camera));
}

#[test]
fn off_with_its_anchor_and_on_in_hdr() {
    let (mut app, anchor, camera) = app();
    drawn(&mut app, RenderLayers::layer(VIEW_MODEL_LAYER), true);
    app.world_mut().get_mut::<Camera>(anchor).unwrap().is_active = false;
    app.update();
    assert!(!active(&app, camera), "its anchor doesn't draw");
    app.world_mut().get_mut::<Camera>(anchor).unwrap().is_active = true;
    app.update();
    assert!(active(&app, camera));
    // In HDR it stays on with nothing to draw (auto exposure's state).
    let (mut app, _, camera) = self::app();
    app.world_mut().entity_mut(camera).insert(Hdr);
    app.update();
    assert!(active(&app, camera), "HDR");
}
