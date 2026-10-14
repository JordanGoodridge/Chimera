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
//!
//! `MapPlugin` (what runs while a map is loaded), `unload_map`/`change_map`
//! (`MapHooks` for the layers above: what they reset at unload, the
//! spawner `change_map` runs), the loaded map's resources;
//! re-exports the files below and `core`'s collision types.

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
pub(crate) fn fall_out_of_map(
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
    /// built in `crate::map_plugin`, which adds `crate::audio`, then the
    /// systems of map and `crate::render` (in the order they always had),
    /// then spawns the map at startup with render's drawer (`spawn_map`).
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
    world.remove_resource::<ScreenOverlayImages>();
    world.remove_resource::<monitor::MonitorScreen>();
    world.insert_resource(MonitorCamera::default());
    if let Some(mut p) = world.get_resource_mut::<particles::Particles>() {
        p.groups.clear();
    }
    view_model::unload(world);
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
pub(crate) fn tag_moved_brush_entities(
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
