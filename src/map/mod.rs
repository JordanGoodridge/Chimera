//! The Map slot: the neutral data any game's map importer produces and the
//! engine spawns, and the map's systems. Units are meters, Y up (README:
//! mounts own each game's units and axes; the engine sees only this).
//!
//! This file holds `MapPlugin` (what runs while a map is loaded),
//! unloading and changing maps, and the loaded map's resources. The rest:
//! `data` (`MapData` and what it holds), `state` (what the logic writes on
//! spawned parts), `spawn` (spawning a map, in phases), `bodies`
//! (character bodies and hitboxes), `props` (placed props after
//! spawning), `materials`, `images`, `collision` and `sky` (built from the
//! data), each re-exported here, plus one submodule per feature.

use std::{collections::HashMap, sync::Arc};

use avian3d::prelude::*;
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    mesh::Indices,
    prelude::*,
    render::render_resource::{Extent3d, PrimitiveTopology, TextureDimension, TextureFormat},
};

use crate::core::{MovingSolid, SpawnPoint, Team};
// Collision-world types live in `core` (the greybox map uses them too).
pub use crate::core::{
    BrushTreeNode, MapBrush, MapBrushCollider, MapBrushTree, MapBrushes, MapTerrain, MapTerrainCollider, MapWater, MapWaterVolume, PropSurface,
};

pub mod anim;
pub mod decal;
pub mod emitters;
pub mod entities;
pub mod fish;
pub mod fire;
pub mod fog;
pub use entities::{MapBrushEntity, MapEntities, MapEntity, MapHull};
pub mod beams;
pub mod breakables;
pub mod buoyancy;
pub mod color_correction;
pub mod contact_filter;
pub mod controllers;
pub mod copies;
pub use breakables::{
    BreakProp, BrushPanes, FallingPane, GlassImpact, GlassShatter, MapBreak, MapBreakPiece, PanePart, SpawnGibs,
};
pub mod detail;
pub(crate) mod dust;
pub mod hud;
pub mod interp;
pub(crate) mod light_styles;
pub mod loading;
pub mod lod;
pub mod loose;
pub mod material_fx;
pub use material_fx::{DetailMode, MapSelfIllum, MapUvTransform};
pub mod bot_profiles;
pub mod merge;
pub mod monitor;
pub mod nav;
pub mod particles;
pub mod placed_ragdoll;
pub mod probe_lit;
pub mod prop_material;
pub mod prop_physics;
pub mod psys;
pub mod radio;
pub mod ragdoll;
pub mod tint;
pub mod tracer;
pub use ragdoll::{MapCollisionHooks, MapRagdoll, MapRagdollBody, MapRagdollJoint, Ragdoll, RagdollBody, RagdollShot};
pub mod rope_material;
pub mod rope_sway;
pub mod shadows;
pub mod sound;
pub mod steam;
pub use sound::{
    GameSound, MapSoundClip, MapSoundEntry, MapSounds, MapSurface, PlaySound, PredictedSound, SoundControl, SoundKey,
    SoundLevel, StartSound,
};
pub mod shells;
pub mod sky_occluder;
pub mod sprite_material;
pub mod surface_color;
pub mod view_model;
pub mod vis;
pub mod water;
pub mod world_material;
mod data;
pub use data::*;
// Explicit: Bevy's prelude has a `LightProbe` too.
pub use data::LightProbe;
mod state;
pub use state::*;
mod images;
pub use images::*;
mod bodies;
pub use bodies::*;
mod props;
pub use props::*;
mod materials;
pub use materials::*;
mod collision;
use collision::*;
mod sky;
pub use sky::*;
mod spawn;
use spawn::*;
pub use spawn::{Draws, MapDrawer, MapSpawn, PropSpawn};
pub(crate) use spawn::spawn_map;
pub use view_model::{
    DynamicLight, EffectSettings, MapAttachment, MapViewModel, ViewAnimator, ViewModelAnchor, ViewModelCamera,
    ViewModelEvent, ViewModelEventKind, ViewModelOffset, ViewModelScreen, ViewModelSettings, ViewModelSource,
    ViewModels,
};

use prop_material::{PropMaterial, PropParams};
use sprite_material::SpriteMaterial;
use world_material::WorldMaterial;

/// The loaded map's light field, when the game provides one.
#[derive(Resource, Clone)]
pub struct LightField(pub MapLightField);

/// The loaded map's screen overlay pictures (env_screenoverlay), by
/// lower-case material name; the client draws the one the logic shows
/// (`logic::ScreenFades::overlay`).
#[derive(Resource, Clone, Debug, Default)]
pub struct ScreenOverlayImages(pub HashMap<String, Handle<Image>>);

/// Spawns a loaded map: collision always, meshes only when rendering is
/// present (same pattern as the greybox, so maps load in headless tests).
pub struct MapPlugin {
    /// The map to spawn at startup; None starts without one (another map
    /// can be loaded later with `change_map`).
    pub data: Option<Arc<MapData>>,
    pub view: MapDebugView,
}

/// Debug renders, used to compare against reference screenshots.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum MapDebugView {
    #[default]
    Normal,
    /// White surfaces: only baked lighting (lightmaps, prop probes),
    /// multiplied by `scale` (e.g. 0.25 so overbright light fits in 0..1).
    Lighting { scale: f32 },
    /// Textures only, unlit.
    Albedo,
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

#[derive(Resource)]
pub(crate) struct PendingMap(Arc<MapData>, MapDebugView);

/// Characters below this height (meters) have fallen out of the map.
#[derive(Resource, Clone, Copy, Debug)]
pub struct KillHeight(pub f32);

/// Falling out of the map kills; characters that can't die (god mode, no
/// health) go back to a spawn point instead.
#[allow(clippy::type_complexity)]
fn fall_out_of_map(
    kill: Option<Res<KillHeight>>,
    mut characters: Query<
        (
            Entity,
            &mut Transform,
            Option<&mut crate::core::Velocity>,
            Option<&crate::core::Health>,
            Has<crate::core::God>,
            Option<&Team>,
        ),
        With<crate::core::Intent>,
    >,
    spawns: Query<(&Transform, &crate::core::SpawnPoint), Without<crate::core::Intent>>,
    mut damage: MessageWriter<crate::core::Damage>,
) {
    let Some(kill) = kill else { return };
    for (e, mut at, velocity, health, god, team) in &mut characters {
        if at.translation.y >= kill.0 {
            continue;
        }
        match health {
            Some(h) if !god => {
                if h.current > 0.0 {
                    damage.write(crate::core::Damage {
                        force: bevy::math::Vec3::ZERO,
                        target: e,
                        attacker: None,
                        amount: h.current.max(1.0) * 1000.0,
                        point: at.translation,
                        dir: Vec3::NEG_Y,
                        hitgroup: crate::core::Hitgroup::Generic,
                        kind: crate::core::DamageKind::Generic,
                        weapon: None,
                    });
                }
            }
            _ => {
                let spawn = spawns
                    .iter()
                    .find(|(_, s)| s.team.is_some() && s.team == team.copied())
                    .or_else(|| spawns.iter().next());
                if let Some((s, _)) = spawn {
                    at.translation = s.translation;
                    if let Some(mut v) = velocity {
                        v.0 = Vec3::ZERO;
                    }
                }
            }
        }
    }
}

/// The map's playable area (see `MapData::playable`).
#[derive(Resource)]
pub(crate) struct PlayableArea(pub(crate) (Vec3, Vec3));

/// The loaded map's presentation settings, for cameras to follow.
#[derive(Resource, Clone, Debug)]
pub struct ActiveMapLook(pub MapLook);

/// The loaded map's name (`MapData::name`), for reports; the client sets
/// it (from `--map` and the `map` command) so the simulation's entities
/// stay as they were (bot seeds come from entity ids).
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct LoadedMapName(pub String);

/// The loaded map's name (`MapData::name`) and file hash
/// (`MapData::file_hash`), set when it spawns, removed when it unloads.
/// The network handshake compares them with the server's.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct MapFile {
    pub name: String,
    pub hash: Option<[u8; 32]>,
}

/// Marks every entity belonging to the loaded map.
#[derive(Component)]
pub struct MapPart;

impl MapPlugin {
    /// The map's registration before the sound plugin's: `MapPlugin` is
    /// built in `crate::map_plugin`, which adds `crate::audio` between
    /// this and `build_rest`, in the order it always had, then spawns the
    /// map at startup with `crate::render`'s drawer (`spawn_map`).
    pub(crate) fn build_first(&self, app: &mut App) {
        if let Some(data) = &self.data {
            app.insert_resource(PendingMap(data.clone(), self.view))
                .insert_resource(ActiveMapLook(data.look.clone()));
        }
        ragdoll::plugin(app);
        placed_ragdoll::plugin(app);
        app.add_plugins(light_styles::LightStylesPlugin);
        app.add_plugins((controllers::ControllersPlugin, buoyancy::BuoyancyPlugin, monitor::MonitorPlugin));
    }

    /// The map's registration after the sound plugin's (`build_first`).
    pub(crate) fn build_rest(&self, app: &mut App) {
        app.init_resource::<ShowLocalBody>()
            .init_resource::<vis::NoVis>()
            .init_resource::<vis::PortalsOpenAll>()
            .init_resource::<vis::Occlusion>()
            .init_resource::<sky_occluder::SkyOcclusion>()
            .init_resource::<vis::VisStats>()
            .add_message::<PlaceDecal>()
            .add_message::<decal::ClearDecals>()
            .init_resource::<decal::DecalSettings>()
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
            .init_resource::<tracer::PendingTracers>()
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
            .add_systems(FixedPostUpdate, prop_physics::wake_on_contact.after(PhysicsSystems::Writeback))
            // Moving water follows its brush entity (before movement and
            // buoyancy read it).
            .add_systems(FixedUpdate, water::move_water.before(crate::core::SimSet::Movement))
            // What the logic switches: sprites and dust, lights, prop
            // skins, bodies and sequences.
            .add_systems(
                Update,
                (
                    switch_parts
                        .before(glow_visibility)
                        .before(dust::update_dust)
                        .before(steam::update_steam),
                    (light_styles::relight, light_styles::animate).chain(),
                    apply_prop_looks,
                    animate_props,
                    beams::follow_eye.after(switch_parts).before(glow_visibility),
                    beams::animate_beams.after(switch_parts),
                    rope_sway::sway_ropes,
                ),
            )
            .add_systems(
                Update,
                (
                    breakables::break_props,
                    breakables::fall_panes,
                    breakables::spawn_gibs,
                    breakables::fly_gibs,
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
                    attach_sky,
                    glow_visibility,
                    dust::update_dust,
                    steam::update_steam,
                    (
                        tracer::draw_tracers,
                        particles::step_particles.in_set(particles::ParticleSet::Step),
                        (
                            emitters::update_trails,
                            emitters::update_smokestacks,
                            emitters::update_teslas,
                            emitters::update_strikes,
                            color_correction::update,
                            emitters::update_embers,
                            emitters::update_muzzle_flashes,
                            fish::swim,
                            psys::update_systems,
                        )
                            .after(particles::ParticleSet::Step)
                            .before(particles::ParticleSet::Draw),
                        particles::draw_particles
                            .run_if(
                                resource_exists::<Assets<Mesh>>
                                    .and_then(resource_exists::<Assets<particles::ParticleDrawMaterial>>),
                            )
                            .in_set(particles::ParticleSet::Draw),
                    )
                        .chain(),
                    show_skybox_in_place,
                    (decal::place_decals, decal::drop_brush_decals, decal::clear_decals).chain(),
                    (
                        attach_bodies,
                        show_local_body,
                        turn_bodies.after(DriveAnimation),
                        pose_bodies.after(DriveAnimation),
                        attach_held,
                        probe_lit::relight,
                        loose::attach_shown,
                    )
                        .run_if(resource_exists::<CharacterBodies>),
                    (view_model::draw_view_models, view_model::view_model_screens)
                        .chain()
                        .after(DriveAnimation)
                        .run_if(resource_exists::<view_model::ViewModelAssets>),
                    (
                        view_model::muzzle_flashes,
                        view_model::size_flash_sprites,
                        view_model::age_effects,
                        shells::eject,
                        shells::fly,
                    )
                        .chain()
                        .after(DriveAnimation)
                        .after(view_model::draw_view_models),
                ),
            )
            .add_systems(
                PostUpdate,
                (
                    shadows::apply_shadow_detail,
                    update_prop_shadows,
                    shadows::update_character_blobs,
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
                lod::switch_lods.after(bevy::transform::TransformSystems::Propagate),
            )
            .add_systems(
                PostUpdate,
                // After propagation, so it sees this frame's (interpolated)
                // eye rather than the last physics tick's; before frusta.
                follow_sky_camera
                    .after(bevy::transform::TransformSystems::Propagate)
                    .before(bevy::camera::visibility::VisibilitySystems::UpdateFrusta),
            )
            .add_systems(
                PostUpdate,
                // From this frame's camera, before visibility propagates.
                (
                    tint::apply_brush_tints,
                    tint::apply_character_looks,
                    merge::sync_merged_brushes,
                    tag_moved_brush_entities,
                    vis::cull,
                    vis::fade_windows,
                )
                    .chain()
                    .after(bevy::transform::TransformSystems::Propagate)
                    .before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
            );
    }
}

/// Bevy adds `lightmap * lightmap_exposure` as light, then applies the
/// camera's exposure. This cancels the default camera exposure (EV100 9.7)
/// so a lightmap value of 1.0 shows a texture at its own brightness.
pub(crate) const LIGHTMAP_EXPOSURE: f32 = 1.2 * 831.746_4; // 1.2 * 2^9.7

/// Height of the capsule center above the feet, so spawns start standing.
pub const SPAWN_LIFT: f32 = 1.0;

/// Remove the loaded map: its entities, its resources and the sky on
/// cameras. Characters stay.
pub fn unload_map(world: &mut World) {
    let parts: Vec<Entity> = world.query_filtered::<Entity, With<MapPart>>().iter(world).collect();
    for e in parts {
        if let Ok(e) = world.get_entity_mut(e) {
            e.despawn();
        }
    }
    let cams: Vec<Entity> = world
        .query_filtered::<Entity, With<bevy::light::Skybox>>()
        .iter(world)
        .collect();
    for c in cams {
        world.entity_mut(c).remove::<bevy::light::Skybox>();
    }
    world.remove_resource::<PendingMap>();
    world.remove_resource::<MapFile>();
    world.remove_resource::<light_styles::StyledLightmaps>();
    world.remove_resource::<MapSkybox>();
    world.remove_resource::<PlayableArea>();
    world.remove_resource::<SkyCameraInfo>();
    world.remove_resource::<ActiveMapHas3dSky>();
    world.remove_resource::<SkyVis>();
    world.remove_resource::<vis::ActiveVisibility>();
    world.remove_resource::<ShadowState>();
    world.remove_resource::<shadows::ShadowSource>();
    world.remove_resource::<sound::SoundBank>();
    world.remove_resource::<CharacterModels>();
    world.remove_resource::<CharacterBodies>();
    world.remove_resource::<HeldAssets>();
    world.remove_resource::<decal::DecalSurfaces>();
    world.remove_resource::<decal::DecalAssets>();
    world.remove_resource::<decal::ImpactDecals>();
    world.remove_resource::<hud::ActiveHud>();
    world.remove_resource::<hud::ActiveOverview>();
    world.remove_resource::<surface_color::SurfaceColors>();
    world.remove_resource::<particles::ParticleAssets>();
    world.remove_resource::<particles::ParticleMaterials>();
    world.remove_resource::<psys::ParticleSystemDefs>();
    world.remove_resource::<ScreenOverlayImages>();
    world.remove_resource::<monitor::MonitorScreen>();
    world.insert_resource(MonitorCamera::default());
    if let Some(mut p) = world.get_resource_mut::<particles::Particles>() {
        p.groups.clear();
    }
    view_model::unload(world);
    shells::unload(world);
    breakables::unload(world);
    world.remove_resource::<breakables::PropBreaks>();
    let bodies: Vec<Entity> = world
        .query_filtered::<Entity, With<CharacterBody>>()
        .iter(world)
        .collect();
    for b in bodies {
        world.entity_mut(b).despawn();
    }
    let animated: Vec<Entity> = world
        .query_filtered::<Entity, With<anim::Animator>>()
        .iter(world)
        .collect();
    for e in animated {
        world.entity_mut(e).remove::<anim::Animator>();
    }
    let modelled: Vec<Entity> = world.query_filtered::<Entity, With<BodyModel>>().iter(world).collect();
    for e in modelled {
        world.entity_mut(e).remove::<(BodyModel, crate::core::Hitboxes)>();
    }
    world.remove_resource::<nav::NavMesh>();
    world.remove_resource::<sound::SurfaceGrid>();
    world.remove_resource::<MapBrushes>();
    world.remove_resource::<MapBrushTree>();
    world.remove_resource::<KillHeight>();
    world.remove_resource::<MapWater>();
    world.remove_resource::<water::WaterMovers>();
    world.remove_resource::<RoundSounds>();
    world.remove_resource::<radio::RadioCommands>();
    world.remove_resource::<bot_profiles::BotProfiles>();
    world.remove_resource::<MapEntities>();
    world.remove_resource::<water::MapWaterRender>();
    world.remove_resource::<water::WaterView>();
    world.remove_resource::<fog::SceneFog>();
    world.remove_resource::<MapTerrain>();
    world.insert_resource(Gravity::default());
    let hooks = world.get_resource::<MapHooks>().map(|h| h.unload.clone()).unwrap_or_default();
    for hook in hooks {
        hook(world);
    }
}

/// What the layers above the map add to loading and unloading maps.
#[derive(Resource, Default)]
pub struct MapHooks {
    /// What they reset when a map is unloaded: `unload_map` runs them
    /// last, in the order they were added (`crate::audio` stops the
    /// soundscape and the long-lived sounds).
    pub unload: Vec<fn(&mut World)>,
    /// Spawns the pending map (`spawn_map` with the drawer
    /// `crate::map_plugin` picks), for `change_map`.
    pub spawn: Option<fn(&mut World)>,
}

/// Replace the loaded map with `data` (spawned now, as at startup).
pub fn change_map(world: &mut World, data: MapData, view: MapDebugView) {
    unload_map(world);
    let data = Arc::new(data);
    world.insert_resource(ActiveMapLook(data.look.clone()));
    world.insert_resource(PendingMap(data, view));
    match world.get_resource::<MapHooks>().and_then(|h| h.spawn) {
        Some(spawn) => spawn(world),
        None => error!("spawning the map: no map spawner (MapPlugin missing)"),
    }
}

/// A brush entity node's (mover, breakable) or a moving prop's mesh
/// bounds in its own space: its `VisClusters` follow it as it moves
/// (`tag_moved_brush_entities`).
#[derive(Component, Clone, Copy, Debug)]
pub struct BrushEntityBounds {
    pub min: Vec3,
    pub max: Vec3,
}

/// Brush entities that moved (and every one on its first frame): the
/// clusters their world bounds touch, for visibility culling. A door or
/// breakable is then hidden like the world around it.
fn tag_moved_brush_entities(
    vis: Option<Res<vis::ActiveVisibility>>,
    mut nodes: Query<(&GlobalTransform, &BrushEntityBounds, &mut VisClusters), Changed<GlobalTransform>>,
) {
    let Some(vis) = vis else { return };
    let mut clusters = Vec::new();
    for (at, b, mut tag) in &mut nodes {
        let aabb = bevy::camera::primitives::Aabb::from_min_max(b.min, b.max);
        let centre = at.transform_point(aabb.center.into());
        let m = at.affine().matrix3;
        let half = Vec3::from(aabb.half_extents);
        let extent = Vec3::new(
            m.row(0).abs().dot(half.into()),
            m.row(1).abs().dot(half.into()),
            m.row(2).abs().dot(half.into()),
        );
        clusters.clear();
        vis.0.clusters_in_box(centre - extent, centre + extent, &mut clusters);
        clusters.sort_unstable();
        clusters.dedup();
        // A box in solid only (no clusters) would be drawn always; keep
        // what it had.
        if !clusters.is_empty() && *tag.clusters != clusters[..] {
            tag.clusters = clusters.clone().into_boxed_slice();
        }
    }
}

/// A glow sprite: drawn over everything, faded by how much of its
/// occlusion proxy is visible.
#[derive(Component)]
pub(crate) struct GlowSprite {
    pub(crate) color: Vec4,
    pub(crate) proxy: f32,
}

/// Glow visibility (specs/cs_source/sprites_dust.md, 5a): the game measures
/// the visible fraction of a view-facing square (half-diagonal = proxy
/// size) pulled the proxy size toward the eye, with occlusion queries. We
/// approximate it with lines to the square's centre and corners, and fade
/// the glow by the fraction that get through. Static props don't occlude.
#[allow(clippy::type_complexity)]
fn glow_visibility(
    query: SpatialQuery,
    cameras: Query<(&GlobalTransform, &Camera), (With<Camera3d>, Without<monitor::ScreenCamera>, Without<SkyboxCamera>, Without<ViewModelCamera>, Without<water::WaterReflectionCamera>)>,
    ignored: Query<Entity, With<crate::core::Intent>>,
    mut glows: Query<
        (
            &GlobalTransform,
            &GlowSprite,
            &MeshMaterial3d<SpriteMaterial>,
            &mut Visibility,
            Option<&VisClusters>,
        ),
        Without<LogicHidden>,
    >,
    materials: Option<ResMut<Assets<SpriteMaterial>>>,
) {
    let (Some((eye, camera)), Some(mut materials)) = (cameras.iter().next(), materials) else {
        return;
    };
    let filter = SpatialQueryFilter::from_excluded_entities(ignored.iter()).with_mask(crate::core::NOT_SHADOW);
    let from = eye.translation();
    let (right, up) = (eye.right().as_vec3(), eye.up().as_vec3());
    for (glow, sprite, material, mut visibility, clusters) in &mut glows {
        // Not potentially visible: hidden, no occlusion tests.
        if clusters.is_some_and(|c| !c.potentially_visible) {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }
        let p = glow.translation();
        let centre = p + (from - p).normalize_or_zero() * sprite.proxy;
        let r = sprite.proxy / std::f32::consts::SQRT_2;
        let points = [
            centre,
            centre + (right + up) * r,
            centre + (right - up) * r,
            centre - (right + up) * r,
            centre - (right - up) * r,
        ];
        let seen = points
            .iter()
            .filter(|to| {
                Dir3::new(**to - from).map_or(true, |dir| {
                    query.cast_ray(from, dir, from.distance(**to), true, &filter).is_none()
                })
            })
            .count() as f32
            / points.len() as f32;
        // Times the fraction of the proxy's screen rectangle inside the
        // viewport (0 if any corner is behind the eye), as the game's
        // occlusion queries are clipped to the screen.
        let seen = seen * clip_fraction(camera, eye, &points[1..]);
        visibility.set_if_neq(if seen > 0.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        // Only touch the material when the colour changes: a mutable
        // borrow alone re-prepares it for the GPU.
        let color = (sprite.color.truncate() * seen).extend(sprite.color.w);
        if materials.get(&material.0).is_some_and(|m| m.params.color != color)
            && let Some(mut m) = materials.get_mut(&material.0)
        {
            m.params.color = color;
        }
    }
}

/// How much of the screen rectangle around `corners` lies in the viewport.
fn clip_fraction(camera: &Camera, eye: &GlobalTransform, corners: &[Vec3]) -> f32 {
    let mut lo = Vec2::splat(f32::MAX);
    let mut hi = Vec2::splat(f32::MIN);
    for c in corners {
        let Some(ndc) = camera.world_to_ndc(eye, *c) else {
            return 0.0;
        };
        if ndc.z < 0.0 || ndc.z > 1.0 {
            return 0.0;
        }
        lo = lo.min(ndc.truncate());
        hi = hi.max(ndc.truncate());
    }
    let area = (hi - lo).max(Vec2::splat(1e-6));
    let inside = (hi.min(Vec2::ONE) - lo.max(Vec2::NEG_ONE)).max(Vec2::ZERO);
    (inside.x * inside.y / (area.x * area.y)).clamp(0.0, 1.0)
}
