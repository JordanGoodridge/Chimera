//! What every camera's frame costs the render world, cut where nothing
//! shows it (`CamerasPlugin`): mesh instances batched on the CPU for each
//! view instead of Bevy's GPU preprocessing (`NoIndirectDrawing` on every
//! camera), and only the last camera drawing into a target copies the
//! frame out to it (`skip_covered_outputs`).
//!
//! GPU preprocessing (Bevy's default where compute shaders run) builds
//! each view's instance data and indirect draw parameters on the GPU:
//! per view and per frame, bind groups made again and several compute
//! passes (bin unpacking, uniform allocation, culling, indirect
//! parameters) over the mesh instances. A party match draws four cameras
//! (the 3D sky, the world, the view model, the UI); batching each one's
//! visible meshes on the CPU instead cut the render world's instructions
//! a frame by 44-57% with a few hundred meshes drawn (docs/performance.md,
//! "Party frame-rate pass", second pass). For A/B:
//! `CHIMERA_GPU_PREPROCESS=1` keeps Bevy's way, `CHIMERA_SKIP_OUTPUTS=0`
//! every camera's copy.
//!
//! Every camera that draws into a window or image renders into the
//! target's intermediate ("main") textures, shared by all the cameras on
//! that target with the same HDR and MSAA settings, and then by default
//! copies them out to the target (Bevy's `upscaling` pass, a full-screen
//! blit). A party match draws four (the 3D sky, the world, the view model,
//! the UI over them): the first three's copies are overwritten by the next
//! camera's. `CameraOutputMode::Skip` leaves the copy to the last one.
//!
//! Known shortcuts: CPU batching was measured on stock maps (a few hundred
//! meshes drawn); on a map with thousands of meshes in view the GPU path
//! may win again, unmeasured (`refcmp bench` with and without
//! `CHIMERA_GPU_PREPROCESS=1`). Why it's OK for now: the party plays stock
//! maps, and the switch is one variable.

use bevy::{
    camera::{CameraOutputMode, Hdr, NormalizedRenderTarget, RenderTarget},
    prelude::*,
    render::view::NoIndirectDrawing,
    window::PrimaryWindow,
};

/// Every camera batched on the CPU (unless `CHIMERA_GPU_PREPROCESS=1`) and
/// only the last one on each target copying out to it.
pub struct CamerasPlugin;

impl Plugin for CamerasPlugin {
    fn build(&self, app: &mut App) {
        if !gpu_preprocessing() {
            // At spawn (Bevy reads it then; added later it is unspecified).
            app.register_required_components::<Camera, NoIndirectDrawing>();
        }
        // After everything that turns cameras on or off (PostUpdate's view
        // model and sky cameras), before extraction.
        if chimera_core::env::var("CHIMERA_SKIP_OUTPUTS").is_ok_and(|v| v == "0") {
            return;
        }
        app.add_systems(Last, skip_covered_outputs);
    }
}

/// `CHIMERA_GPU_PREPROCESS=1`: Bevy's GPU mesh preprocessing and indirect
/// draws for every camera (the default before; for comparisons).
pub fn gpu_preprocessing() -> bool {
    chimera_core::env::var("CHIMERA_GPU_PREPROCESS").is_ok_and(|v| v == "1")
}

/// Set on a camera whose copy to its target `skip_covered_outputs` turned
/// off, so it can be turned back on (other cameras' output modes are left
/// as they are).
#[derive(Component)]
pub struct OutputSkipped;

/// Which cameras share intermediate textures: the same target, HDR or
/// not, the same MSAA (Bevy's `main_texture_key`, less the texture usages,
/// which no camera here changes).
fn texture_key(
    target: Option<&RenderTarget>,
    window: Option<Entity>,
    hdr: bool,
    msaa: Option<&Msaa>,
) -> Option<(NormalizedRenderTarget, bool, u32)> {
    let target = target.cloned().unwrap_or_default().normalize(window)?;
    Some((target, hdr, msaa.copied().unwrap_or_default().samples()))
}

/// Turn off the copy out to the target of every active camera that a
/// later active camera on the same intermediate textures, drawing the
/// whole target, copies out after it (`CameraOutputMode::Skip`); turn it
/// back on once nothing later does (the view model's or the UI's camera
/// off, a capture drawing the world into an image). Runs last in the frame,
/// after everything that turns cameras on and off, so the frame extracted
/// always has one copy per target. Writes only on change.
#[allow(clippy::type_complexity)]
pub fn skip_covered_outputs(
    window: Query<Entity, With<PrimaryWindow>>,
    mut cameras: Query<(
        Entity,
        &mut Camera,
        Option<&RenderTarget>,
        Has<Hdr>,
        Option<&Msaa>,
        Has<OutputSkipped>,
    )>,
    mut commands: Commands,
) {
    let window = window.iter().next();
    // The highest order of a whole-target camera, per texture key.
    let mut last: Vec<((NormalizedRenderTarget, bool, u32), isize)> = Vec::new();
    for (_, camera, target, hdr, msaa, _) in &cameras {
        if !camera.is_active || camera.viewport.is_some() {
            continue;
        }
        let Some(key) = texture_key(target, window, hdr, msaa) else { continue };
        match last.iter_mut().find(|(k, _)| *k == key) {
            Some((_, order)) => *order = (*order).max(camera.order),
            None => last.push((key, camera.order)),
        }
    }
    for (entity, mut camera, target, hdr, msaa, skipped) in &mut cameras {
        let covered = camera.is_active
            && texture_key(target, window, hdr, msaa)
                .and_then(|key| last.iter().find(|(k, _)| *k == key))
                .is_some_and(|(_, order)| *order > camera.order);
        if covered && !skipped {
            // Only cameras still copying the default way: one with its own
            // output mode (a blend) keeps it.
            if matches!(
                camera.output_mode,
                CameraOutputMode::Write {
                    blend_state: None,
                    clear_color: ClearColorConfig::Default
                }
            ) {
                camera.output_mode = CameraOutputMode::Skip;
                commands.entity(entity).insert(OutputSkipped);
            }
        } else if !covered && skipped {
            camera.output_mode = CameraOutputMode::default();
            commands.entity(entity).remove::<OutputSkipped>();
        }
    }
}
