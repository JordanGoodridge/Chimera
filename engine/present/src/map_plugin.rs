//! `MapPlugin`: the map's own systems (`map`, crate `engine_world`), the
//! audio that plays its sounds (`audio`) and what draws it (`render`),
//! registered in one fixed order (the order decides how Bevy orders
//! systems that aren't ordered explicitly; resources are entities, so
//! their insert order decides their ids). The type lives here, beside its
//! `impl Plugin` (the orphan rule: `map` is another crate), and is
//! re-exported as `mashup::map::MapPlugin`, what tests and apps add. Map's own
//! registration first (`map::build_first`, render's ragdoll drawing,
//! `map::build_after_ragdolls`, render's light styles,
//! `map::build_after_light_styles`, render's monitor screens: where
//! map's plugins were added), audio's, then the systems of map and render
//! together (`build_rest`, the order they always had), then spawning the
//! map at startup (`spawn_map` with `render::MapAssets`, also as
//! `MapHooks::spawn` for `change_map`) and render's part of unloading
//! (`render::unload_map`, a `MapHooks::unload` hook).

use std::sync::Arc;

use avian3d::prelude::*;
use bevy::prelude::*;

#[allow(unused_imports)]
use crate::map::*;
use crate::{audio, render};

/// Spawns a loaded map: collision always, meshes only when rendering is
/// present (same pattern as the greybox, so maps load in headless tests).
pub struct MapPlugin {
    /// The map to spawn at startup; None starts without one (another map
    /// can be loaded later with `change_map`).
    pub data: Option<Arc<MapData>>,
    pub view: MapDebugView,
}

impl MapPlugin {
    pub fn new(data: MapData) -> Self {
        Self {
            data: Some(Arc::new(data)),
            view: MapDebugView::Normal,
        }
    }

    /// The map systems and materials without a map loaded yet.
    pub fn empty() -> Self {
        Self {
            data: None,
            view: MapDebugView::Normal,
        }
    }
}

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        build_first(app, self.data.as_ref(), self.view);
        app.add_systems(
            Update,
            (render::bodies::adopt_bodies, render::bodies::pose_ragdolls)
                .chain()
                .after(DriveAnimation)
                .run_if(resource_exists::<render::bodies::PlayModels>),
        );
        build_after_ragdolls(app);
        app.add_plugins(render::light_styles::LightStylesPlugin);
        build_after_light_styles(app);
        app.add_plugins(render::monitor::MonitorPlugin);
        app.add_plugins(audio::SoundPlugin);
        build_rest(app);
        app.add_systems(
            Startup,
            spawn_map::<render::MapAssets>.run_if(resource_exists::<PendingMap>),
        );
        app.init_resource::<MapHooks>();
        let mut hooks = app.world_mut().resource_mut::<MapHooks>();
        hooks.spawn = Some(spawn_pending_map);
        hooks.unload.push(render::unload_map);
        hooks.copied.push(render::merge::show_copied_pieces);
    }
}

/// `map::change_map`'s spawner: the pending map, drawn by `render`.
fn spawn_pending_map(world: &mut World) {
    if let Err(e) = world.run_system_cached(spawn_map::<render::MapAssets>) {
        error!("spawning the map: {e}");
    }
}

/// The map's and render's registration after the sound plugin's
/// (`map::build_first`).
fn build_rest(app: &mut App) {
    app.init_resource::<ShowLocalBody>()
        .init_resource::<render::vis::NoVis>()
        .init_resource::<render::vis::PortalsOpenAll>()
        .init_resource::<render::vis::Occlusion>()
        .init_resource::<render::sky_occluder::SkyOcclusion>()
        .init_resource::<render::vis::VisStats>()
        .add_message::<PlaceDecal>()
        .add_message::<render::decal::ClearDecals>()
        .init_resource::<render::decal::DecalSettings>()
        .add_message::<ViewModelEvent>()
        .add_message::<SpawnGibs>()
        .add_message::<breakables::BreakProp>()
        .add_message::<prop_physics::PropAwakened>()
        .add_message::<GlassShatter>()
        .add_message::<TeslaSpark>()
        .add_message::<BeamStrike>()
        .add_message::<MuzzleFlashFire>()
        .add_message::<ViewKick>()
        .add_message::<GlassImpact>()
        .add_message::<breakables::FallingPane>()
        .add_message::<CollisionStart>()
        .init_resource::<particles::Particles>()
        .configure_sets(Update, particles::ParticleSet::Step.before(particles::ParticleSet::Draw))
        .init_resource::<tracer::MuzzleCache>()
        .init_resource::<render::tracer::PendingTracers>()
        .add_message::<tracer::Tracer>()
        .insert_resource(GlobalAmbientLight {
            brightness: 600.0,
            // Baked lighting already includes the map's ambient light.
            affects_lightmapped_meshes: false,
            ..default()
        })
        .add_systems(
            FixedUpdate,
            (attach_hitboxes, ragdoll::record_poses, pose_hitboxes)
                .chain()
                .before(crate::core::SimSet::Movement),
        )
        .add_systems(FixedUpdate, fall_out_of_map.after(crate::core::SimSet::Movement))
        .add_systems(FixedLast, pose_sim_hitboxes.in_set(SimPose))
        .add_systems(
            FixedUpdate,
            breakables::round_restart
                .after(crate::core::SimSet::Rules)
                .before(crate::core::SimSet::Movement),
        )
        .add_systems(FixedPostUpdate, breakables::update_panes)
        .add_systems(
            FixedPostUpdate,
            (render::breakables::restore_panes, render::breakables::clip_panes)
                .chain()
                .after(breakables::update_panes),
        )
        .add_systems(FixedPostUpdate, prop_physics::wake_on_contact.after(PhysicsSystems::Writeback))
        // Moving water follows its brush entity (before movement and
        // buoyancy read it).
        .add_systems(FixedUpdate, water::move_water.before(crate::core::SimSet::Movement))
        // What the logic switches: sprites and dust, lights, prop
        // skins, bodies and sequences.
        .add_systems(
            Update,
            (
                render::parts::switch_parts
                    .before(render::glow::glow_visibility)
                    .before(render::dust::update_dust)
                    .before(render::steam::update_steam),
                (render::light_styles::relight, render::light_styles::animate).chain(),
                render::props::apply_prop_looks,
                animate_props,
                render::beams::follow_eye.after(render::parts::switch_parts).before(render::glow::glow_visibility),
                render::beams::animate_beams.after(render::parts::switch_parts),
                render::rope_sway::sway_ropes,
            ),
        )
        .add_systems(
            Update,
            (
                breakables::break_props,
                breakables::fall_panes,
                breakables::spawn_gibs,
                render::breakables::draw_gibs,
                breakables::fly_gibs,
                render::breakables::fade_gibs,
            )
                .chain(),
        )
        .add_systems(
            FixedPostUpdate,
            breakables::shatter_pane_pieces.after(PhysicsSystems::Writeback),
        )
        // Before the physics step reads it (headless too).
        .add_systems(FixedUpdate, loose::attach_loose.before(crate::core::SimSet::Movement))
        .add_systems(
            Update,
            (
                render::sky::attach_sky,
                render::glow::glow_visibility,
                render::dust::update_dust,
                render::steam::update_steam,
                (
                    render::tracer::draw_tracers,
                    particles::step_particles.in_set(particles::ParticleSet::Step),
                    (
                        render::emitters::update_trails,
                        render::emitters::update_smokestacks,
                        render::emitters::update_teslas,
                        render::emitters::update_strikes,
                        render::color_correction::update,
                        render::emitters::update_embers,
                        render::emitters::update_muzzle_flashes,
                        render::fish::swim,
                        render::psys::update_systems,
                    )
                        .after(particles::ParticleSet::Step)
                        .before(particles::ParticleSet::Draw),
                    render::particles::draw_particles
                        .run_if(
                            resource_exists::<Assets<Mesh>>
                                .and_then(resource_exists::<Assets<render::particles::ParticleDrawMaterial>>),
                        )
                        .in_set(particles::ParticleSet::Draw),
                )
                    .chain(),
                render::sky::show_skybox_in_place,
                (
                    render::decal::place_decals,
                    render::decal::drop_brush_decals,
                    render::decal::clear_decals,
                    render::spray::draw_sprays,
                )
                    .chain(),
                (
                    render::bodies::attach_bodies,
                    render::bodies::show_local_body,
                    render::bodies::turn_bodies.after(DriveAnimation),
                    render::bodies::pose_bodies.after(DriveAnimation),
                    render::bodies::attach_held,
                    render::probe_lit::relight,
                    render::loose::attach_shown,
                    render::loose::draw_loose,
                )
                    .run_if(resource_exists::<render::bodies::PlayModels>),
                (render::view_model::draw_view_models, render::view_model::view_model_screens)
                    .chain()
                    .after(DriveAnimation)
                    .run_if(resource_exists::<render::view_model::ViewModelAssets>),
                (
                    render::view_model::muzzle_flashes,
                    render::view_model::size_flash_sprites,
                    render::view_model::age_effects,
                    render::shells::eject,
                    render::shells::fly,
                )
                    .chain()
                    .after(DriveAnimation)
                    .after(render::view_model::draw_view_models),
            ),
        )
        .add_systems(
            PostUpdate,
            (
                render::shadows::apply_shadow_detail,
                render::shadows::update_prop_shadows,
                render::shadows::update_character_blobs,
            )
                .chain()
                .run_if(resource_exists::<Assets<Mesh>>.and_then(resource_exists::<Assets<Image>>))
                .after(bevy::transform::TransformSystems::Propagate)
                // A rewritten shadow mesh is announced this frame:
                // announced a frame late, the render world (which
                // takes a RENDER_WORLD-only mesh's data) took the
                // rewrite on an earlier announcement and then found the
                // late one empty ("cannot be extracted: already
                // extracted", logged each time a moving prop stopped).
                .before(bevy::asset::AssetEventSystems),
        )
        .add_systems(
            PostUpdate,
            // Props' levels of detail from where the eye is this frame.
            render::lod::switch_lods.after(bevy::transform::TransformSystems::Propagate),
        )
        .add_systems(Update, render::bodies::uncull_placed_ragdolls)
        .add_systems(
            PostUpdate,
            // After propagation, so it sees this frame's (interpolated)
            // eye rather than the last physics tick's; before frusta.
            render::sky::follow_sky_camera
                .after(bevy::transform::TransformSystems::Propagate)
                .before(bevy::camera::visibility::VisibilitySystems::UpdateFrusta),
        )
        .add_systems(
            PostUpdate,
            // From this frame's camera, before visibility propagates.
            (
                render::tint::apply_brush_tints,
                render::tint::apply_character_looks,
                render::merge::sync_merged_brushes,
                tag_moved_brush_entities,
                render::vis::cull,
                render::vis::fade_windows,
            )
                .chain()
                .after(bevy::transform::TransformSystems::Propagate)
                .before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
        );
}
