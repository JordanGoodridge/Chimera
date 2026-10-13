//! Spawning a loaded map (`MapData`) into the world: collision always,
//! meshes, materials and effects only when rendering is present.

#[allow(unused_imports)]
use super::*;

/// Signs for normal maps' red and green channels in the world material.
/// Source's normal maps are DirectX-style (x along texture u, y along v,
/// image-down) and used unflipped (specs/cs_source/shaders.md). Overridable
/// with MASHUP_NORMAL_X_SIGN / MASHUP_NORMAL_G_SIGN for experiments.
pub(super) const NORMAL_G_SIGN: f32 = 1.0;
pub(super) const NORMAL_X_SIGN: f32 = 1.0;

/// How thick terrain surfaces are as solids (`MapTerrain`), meters: 2 units.
pub(super) const TERRAIN_THICKNESS: f32 = 2.0 * 0.0254;

/// How far below the map's lowest point (meters) falling ends.
pub(super) const KILL_MARGIN: f32 = 3.0;

pub(super) fn spawn_map(
    mut commands: Commands,
    pending: Res<PendingMap>,
    mut meshes: Option<ResMut<Assets<Mesh>>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    mut images: Option<ResMut<Assets<Image>>>,
    mut world_materials: Option<ResMut<Assets<WorldMaterial>>>,
    mut rope_materials: Option<ResMut<Assets<RopeMaterial>>>,
    mut sprite_materials: Option<ResMut<Assets<SpriteMaterial>>>,
    mut prop_materials: Option<ResMut<Assets<PropMaterial>>>,
    shadow_materials: Option<Res<Assets<shadows::ShadowMaterial>>>,
    mut water_materials: Option<ResMut<Assets<water::WaterMaterial>>>,
    mut bindposes: Option<ResMut<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>>,
    mut beam_materials: Option<ResMut<Assets<beams::BeamMaterial>>>,
    mut detail_materials: Option<ResMut<Assets<detail::DetailMaterial>>>,
) {
    let data = &pending.0;
    let view = pending.1;
    commands.insert_resource(MapFile {
        name: data.name.clone(),
        hash: data.file_hash,
    });
    info!(
        "spawning map: world material {}, lightmap {}",
        if world_materials.is_some() {
            "available"
        } else {
            "missing"
        },
        data.lightmap.is_some()
    );
    let root = commands
        .spawn((
            Name::new(format!("Map {}", data.name)),
            MapPart,
            Transform::default(),
            Visibility::default(),
        ))
        .id();
    // Drawn parts that never move (world chunks, static props, merged brush
    // entities, ropes, prop shadows) under a root of their own, beside the
    // map's (both at the origin): any transform written under a root (a
    // rotating brush, a physics prop) makes Bevy's transform propagation
    // recompute every child of that root, several times a frame
    // (surf_demise: 37k map parts, 9.6 ms a frame); a root whose tree
    // didn't change is skipped whole.
    let statics = commands
        .spawn((
            Name::new("Map static parts"),
            MapPart,
            Transform::default(),
            Visibility::default(),
        ))
        .id();

    if !data.collision_indices.is_empty() {
        commands.insert_resource(MapTerrain::from_triangles(
            data.collision_indices
                .iter()
                .map(|t| t.map(|i| Vec3::from(data.collision_positions[i as usize]))),
            TERRAIN_THICKNESS,
        ));
        let positions: Vec<Vec3> = data.collision_positions.iter().map(|p| Vec3::from(*p)).collect();
        commands.entity(root).with_child((
            Name::new("Map collision (surfaces)"),
            MapPart,
            MapTerrainCollider,
            RigidBody::Static,
            Collider::trimesh(positions.clone(), data.collision_indices.clone()),
            // Loose items slide on their own copy (below).
            CollisionLayers::new(LayerMask::DEFAULT, LayerMask::ALL & !crate::core::ITEM_LAYER),
            Transform::default(),
        ));
        if let Some(smooth) = smooth_terrain(positions, &data.collision_indices) {
            commands.entity(root).with_child((
                Name::new("Map collision (surfaces, for loose items)"),
                MapPart,
                MapTerrainCollider,
                RigidBody::Static,
                smooth,
                CollisionLayers::new(crate::core::ITEM_GROUND_LAYER, crate::core::ITEM_LAYER),
                Transform::default(),
            ));
        }
    }
    if !data.collision_hulls.is_empty() {
        let hulls: Vec<_> = data
            .collision_hulls
            .iter()
            .filter_map(|h| Collider::convex_hull(h.iter().map(|p| Vec3::from(*p)).collect()))
            .map(|c| (Vec3::ZERO, Quat::IDENTITY, c))
            .collect();
        commands.entity(root).with_child((
            Name::new("Map collision (solids)"),
            MapPart,
            MapBrushCollider,
            RigidBody::Static,
            Collider::compound(hulls),
            Transform::default(),
        ));
    }

    // Mover brush entities: a node each (placed by origin and angles), with
    // a kinematic collider; their meshes are spawned under it below.
    // Anchors (placed weapons with entities parented to them) get a node
    // too, which their props ride and the weapon layer moves.
    let anchors = entities::anchor_entities(&data.entities);
    let entity_nodes: Vec<Option<Entity>> = data
        .entities
        .iter()
        .enumerate()
        .map(|(i, e)| {
            if anchors[i] && !e.mover {
                let home = Transform::from_translation(entities::entity_to_engine(e.origin(), data.entity_scale))
                    .with_rotation(entities::rotation_to_engine(entities::entity_rotation(e.angles())));
                return Some(
                    commands
                        .spawn((
                            Name::new(format!("Anchor {i} ({})", e.classname())),
                            MapPart,
                            entities::MapAnchor(i),
                            home,
                            Visibility::default(),
                            interp::Interpolated::default(),
                            ChildOf(root),
                        ))
                        .id(),
                );
            }
            if !e.mover {
                return None;
            }
            let scale = data.entity_scale;
            let rotation = entities::entity_rotation(e.angles());
            let collider = entities::brush_collider(e, scale);
            let home = Transform::from_translation(entities::entity_to_engine(e.origin(), scale))
                .with_rotation(entities::rotation_to_engine(rotation));
            let mut node = commands.spawn((
                Name::new(format!("Brush entity {i} ({})", e.classname())),
                MapPart,
                MapBrushEntity(i),
                home,
                Visibility::default(),
                ChildOf(root),
            ));
            match (collider, &e.physics) {
                // A physics brush (func_physbox): a rigid body like a
                // physics prop, pinned while its motion is disabled.
                (Some(collider), Some(p)) => {
                    let points = e
                        .hulls
                        .iter()
                        .flat_map(|h| &h.points)
                        .map(|q| entities::entity_to_engine(*q, scale));
                    let bounds = points.fold((Vec3::MAX, Vec3::MIN), |(a, b), q| (a.min(q), b.max(q)));
                    node.insert((collider, PropEntity(i), PropHome(home)));
                    if p.frozen {
                        node.insert((
                            RigidBody::Static,
                            MapPropCollider,
                            prop_physics::FrozenBody {
                                physics: p.clone(),
                                bounds,
                            },
                        ));
                    } else {
                        node.insert(prop_physics::dynamic_body(p, bounds));
                        if p.asleep {
                            node.insert((prop_physics::StartAsleep, RigidBody::Static));
                        }
                    }
                }
                (Some(collider), None) => {
                    node.insert((
                        MapBrushCollider,
                        RigidBody::Kinematic,
                        collider,
                        contact_filter::hooks(),
                    ));
                }
                _ => {}
            }
            Some(node.id())
        })
        .collect();
    let parent_of = |m: &MapMesh| {
        m.entity
            .and_then(|i| entity_nodes.get(i).copied().flatten())
            .unwrap_or(root)
    };
    // Where the map places each mover node (merged brush entities draw
    // there, `merge`).
    let node_home = |i: usize| {
        let e = &data.entities[i];
        Transform::from_translation(entities::entity_to_engine(e.origin(), data.entity_scale))
            .with_rotation(entities::rotation_to_engine(entities::entity_rotation(e.angles())))
    };
    // Areaportal windows' brushes fade in with distance (`vis::fade_windows`).
    if let Some(v) = data.visibility.as_deref() {
        for (portal, p) in v.areas.portals.iter().enumerate() {
            let brush = p.fade.and_then(|f| f.brush);
            if let Some(node) = brush.and_then(|b| entity_nodes.get(b).copied().flatten()) {
                commands.entity(node).insert(vis::WindowBrush { portal, alpha: None });
            }
        }
    }
    if !data.entities.is_empty() {
        commands.insert_resource(MapEntities {
            entities: Arc::new(data.entities.clone()),
            scale: data.entity_scale,
        });
    }
    commands.insert_resource(breakables::prop_breaks(data));

    // Prop models: one collider per model (shared by its placements), and
    // render handles when rendering exists.
    let model_colliders: Vec<Option<Collider>> = data.models.iter().map(model_collider).collect();
    let mut model_parts: Vec<Vec<(Handle<Mesh>, Handle<StandardMaterial>)>> = Vec::new();
    // Per model mesh: [in the world (fogged), in the 3D skybox].
    let mut lit_model_materials: Vec<Vec<[Handle<PropMaterial>; 2]>> = Vec::new();
    // Per model with several skins: each skin's materials per mesh.
    let mut skin_model_materials: Vec<Vec<Vec<[Handle<PropMaterial>; 2]>>> = Vec::new();
    // Cubemap handles, and prop materials with the nearest one, made as
    // props need them: (model, mesh, sky layer, cubemap).
    let mut cubemap_handles: Vec<Handle<Image>> = Vec::new();
    let mut texture_handles: Vec<Handle<Image>> = Vec::new();
    let mut rig_bindposes: HashMap<usize, Handle<bevy::mesh::skinning::SkinnedMeshInverseBindposes>> = HashMap::new();
    let mut lod_parts: HashMap<usize, Vec<Vec<Handle<Mesh>>>> = HashMap::new();
    let mut envmap_variants: std::collections::HashMap<(usize, usize, bool, usize), Handle<PropMaterial>> =
        std::collections::HashMap::new();

    // Headless: dropped weapons and gibs still have their bodies, undrawn.
    if (meshes.is_none() || materials.is_none() || images.is_none() || bindposes.is_none())
        && let Some(root) = data.characters.first().map(|c| c.root)
    {
        let loose = data
            .held
            .iter()
            .map(|h| (h.key.clone(), loose::asset(h, root, Vec::new())))
            .collect();
        commands.insert_resource(loose::LooseAssets(loose));
    }
    if (meshes.is_none() || materials.is_none() || images.is_none() || prop_materials.is_none())
        && let Some(gibs) = breakables::build_assets(data, &[], view, None)
    {
        commands.insert_resource(gibs);
    }
    // Headless: steam jets still puff (undrawn), switched by the logic.
    if meshes.is_none() || materials.is_none() || images.is_none() {
        for (i, jet) in data.steam.iter().enumerate() {
            let mut e = commands.spawn((
                Name::new(format!("Steam {i}")),
                MapPart,
                steam::SteamEmitter::new(jet.clone(), Handle::default(), i as u64 + 1),
            ));
            if let Some(entity) = jet.entity {
                e.insert(EntityPart {
                    entity,
                    on: jet.start_on,
                    exists: true,
                });
            }
        }
    }
    if let (Some(meshes), Some(materials), Some(images)) = (meshes.as_mut(), materials.as_mut(), images.as_mut()) {
        // HUD pictures are clamped at their edges: wrapped, the scope
        // ring's outer edge blends with its clear centre and lets the
        // scene through at the screen's edges.
        let hud: std::collections::HashSet<usize> = data
            .hud
            .iter()
            .flat_map(|h| h.sprites.values().map(|s| s.texture))
            .collect();
        let textures: Vec<Handle<Image>> = data
            .textures
            .iter()
            .enumerate()
            .map(|(i, t)| {
                if Some(i) == data.camera_texture {
                    return monitor::screen_image(images);
                }
                let mut image = to_image(t, &data.look);
                if hud.contains(&i) {
                    clamp_edges(&mut image);
                }
                images.add(image)
            })
            .collect();
        if let Some(i) = data.camera_texture {
            commands.insert_resource(monitor::MonitorScreen(textures[i].clone()));
        }
        if !data.screen_overlays.is_empty() {
            commands.insert_resource(ScreenOverlayImages(
                data.screen_overlays
                    .iter()
                    .map(|(name, t)| (name.clone(), textures[*t].clone()))
                    .collect(),
            ));
        }
        let cubemaps: Vec<Handle<Image>> = data.cubemaps.iter().map(|c| images.add(cube_image(c))).collect();
        cubemap_handles = cubemaps.clone();
        texture_handles = textures.clone();
        // Character bodies, drawn by `attach_bodies`: skinned when the
        // model has a skeleton.
        if let Some(bindposes) = bindposes.as_mut() {
            // Bodies and what they hold are lit from the light field where
            // they are (probe_lit), as Source lights models.
            let Some(pm) = prop_materials.as_deref_mut() else {
                panic!("PropMaterial assets exist wherever StandardMaterial does");
            };
            let model_material = |m: &MapMesh| {
                let mut material = lit_prop_material(m, &textures, data, view, false);
                if !m.unlit && view != MapDebugView::Albedo && data.light_field.is_some() {
                    material.params.set_probe(&LightProbe::default(), 1.0);
                }
                material
            };
            let lighting_scale = if let MapDebugView::Lighting { scale } = view {
                scale
            } else {
                1.0
            };
            commands.insert_resource(probe_lit::ProbeLightScale(data.look.light_scale * lighting_scale));
            let bodies = data
                .characters
                .iter()
                .map(|c| body_assets(&c.model, &c.bones, c.root, meshes, pm, bindposes, &model_material))
                .collect();
            commands.insert_resource(CharacterBodies(bodies));
            let held = data
                .held
                .iter()
                .map(|h| {
                    let mut build = |model: &MapModel| -> Vec<(Handle<Mesh>, Handle<PropMaterial>)> {
                        model
                            .meshes
                            .iter()
                            .map(|m| (meshes.add(build_mesh(m, false)), pm.add(model_material(m))))
                            .collect()
                    };
                    let whole = build(&h.model);
                    let own = h.own.as_ref().map_or_else(|| whole.clone(), &mut build);
                    let merged = h.merged.iter().map(|(b, m)| (b.clone(), build(m))).collect();
                    (
                        h.key.clone(),
                        HeldParts {
                            bone: h.bone.clone(),
                            whole,
                            own,
                            muzzle: h.muzzle,
                            merged,
                        },
                    )
                })
                .collect::<HashMap<_, _>>();
            // Loose (dropped) forms, sized by the first character's scale.
            if let Some(root) = data.characters.first().map(|c| c.root) {
                let loose = data
                    .held
                    .iter()
                    .filter_map(|h| {
                        let parts = held.get(&h.key)?.whole.clone();
                        Some((h.key.clone(), loose::asset(h, root, parts)))
                    })
                    .collect();
                commands.insert_resource(loose::LooseAssets(loose));
            }
            commands.insert_resource(HeldAssets(held));
            if let Some(prop_materials) = prop_materials.as_mut() {
                commands.insert_resource(view_model::build_assets(
                    data,
                    &textures,
                    &cubemaps,
                    view,
                    meshes,
                    prop_materials,
                    bindposes,
                ));
            }
            if let Some(flash) =
                view_model::build_flash_assets(data, &textures, meshes, sprite_materials.as_deref_mut())
            {
                commands.insert_resource(flash);
            }
            if let Some(prop_materials) = prop_materials.as_mut()
                && let Some(shells) = shells::build_assets(data, &textures, view, meshes, prop_materials)
            {
                commands.insert_resource(shells);
            }
            if let Some(prop_materials) = prop_materials.as_mut()
                && let Some(gibs) = breakables::build_assets(data, &textures, view, Some((meshes, prop_materials)))
            {
                commands.insert_resource(gibs);
            }
        }
        let source_ldr = data.look.source_ldr_lightmaps;
        let built = data
            .lightmap
            .as_ref()
            .map(|l| LightmapLayers::build(&l.rgb, l.bumped.as_ref(), l.width, l.height, source_ldr));
        let lightmap = built.as_ref().map(|b| images.add(b.plain.clone()));
        let world_lightmap = built.as_ref().map(|b| images.add(b.world.clone()));
        let bumped_lightmaps: Option<[Handle<Image>; 3]> =
            built.and_then(|b| b.bumped).map(|pages| pages.map(|p| images.add(p)));
        // Switchable and animated light styles relight these images
        // (`light_styles`); the plain one only where surfaces sample it
        // (standard materials, used when the world material is missing).
        if let Some(l) = data.lightmap.as_ref().filter(|l| !l.styles.is_empty()) {
            commands.insert_resource(light_styles::StyledLightmaps::new(
                Arc::new(l.clone()),
                source_ldr,
                lightmap.clone().filter(|_| world_materials.is_none()),
                world_lightmap.clone(),
                bumped_lightmaps.clone(),
            ));
        }
        let mut sky_handle = None;
        if let Some(sky) = &data.sky
            && view == MapDebugView::Normal
        {
            let handle = images.add(sky_image(sky, &data.textures));
            sky_handle = Some(handle.clone());
            commands.insert_resource(MapSkybox(handle));
        }
        if let Some(bounds) = data.playable {
            commands.insert_resource(PlayableArea(bounds));
        }

        if let Some(cam) = &data.sky_camera
            && view == MapDebugView::Normal
        {
            commands.insert_resource(SkyCameraInfo(cam.clone()));
            commands.insert_resource(ActiveMapHas3dSky);
        }
        if let Some(vis) = &data.sky_vis
            && view == MapDebugView::Normal
        {
            commands.insert_resource(SkyVis(vis.clone()));
            // The sky is drawn by a sky camera even without a 3D skybox, so
            // it can be switched off where the map can't see it.
            if data.sky_camera.is_none() {
                commands.insert_resource(SkyCameraInfo(MapSkyCamera {
                    origin: Vec3::ZERO,
                    scale: 1.0,
                    fog: None,
                }));
            }
        }
        // Sky faces, to hide what lies behind them (depth only, when
        // `mashup_skyocclude` is on: `sky_occluder`).
        if let Some(mesh) = sky_occluder::mesh(&data.sky_surfaces) {
            commands.spawn((
                Name::new("Sky surfaces"),
                MapPart,
                sky_occluder::SkyOccluder,
                Mesh3d(meshes.add(mesh)),
                Transform::default(),
                ChildOf(statics),
            ));
        }
        // Water surfaces have their own material (map::water).
        let water_drawn = water_materials.is_some() && view == MapDebugView::Normal;
        if water_drawn && let Some(water_materials) = water_materials.as_mut() {
            water::spawn_surfaces(
                &mut commands,
                data,
                root,
                meshes,
                images,
                water_materials,
                &cubemaps,
                sky_handle.as_ref(),
                &|i| entity_nodes.get(i).copied().flatten(),
            );
        }
        // World meshes in chunks with tight bounds, tagged with the
        // clusters they touch (see `vis`). Meshes of entities with their own
        // node (movers, breakables) stay whole and untagged.
        let visibility = data.visibility.as_deref().filter(|_| !merged_world());
        let tag = |e: &mut EntityCommands, clusters: Vec<u32>| {
            if visibility.is_some() && !clusters.is_empty() {
                e.insert(vis::VisClusters::new(clusters));
            }
        };
        let chunk_size = vis::chunk_size();
        // One material asset per distinct material (brush entities have a
        // mesh per material each): meshes sharing one are drawn in batches
        // (one bind group, multi-draw) instead of one draw call apiece.
        // Keyed by the material's whole description, and by whether it is
        // in the 3D skybox (water fog skips skybox materials).
        // Areaportal windows' brushes keep materials of their own:
        // `vis::fade_windows` changes their alpha per brush.
        let window_brushes: std::collections::HashSet<usize> = data
            .visibility
            .as_deref()
            .map(|v| {
                v.areas
                    .portals
                    .iter()
                    .filter_map(|p| p.fade.and_then(|f| f.brush))
                    .collect()
            })
            .unwrap_or_default();
        let own = |m: &MapMesh| m.entity.filter(|e| window_brushes.contains(e));
        // Brush entities' opaque meshes are drawn merged per material and
        // chunk while they stay put (`merge`); their own meshes wait hidden.
        let merging = merge::enabled();
        // Windows with broken looks switch meshes when they break: never
        // merged.
        let paned: std::collections::HashSet<usize> = data
            .meshes
            .iter()
            .filter(|m| m.pane_look != PaneLook::Whole)
            .filter_map(|m| m.entity)
            .collect();
        let mut node_centres: HashMap<usize, (Vec3, Vec3)> = HashMap::new();
        for m in data.meshes.iter().filter(|m| !m.skybox && parent_of(m) != root) {
            let Some(i) = m.entity else { continue };
            let b = node_centres.entry(i).or_insert((Vec3::MAX, Vec3::MIN));
            for p in &m.positions {
                b.0 = b.0.min(Vec3::from(*p));
                b.1 = b.1.max(Vec3::from(*p));
            }
        }
        let merge_spot = |m: &MapMesh| -> Option<(usize, Entity, Transform, Vec3)> {
            let i = m.entity?;
            let node = parent_of(m);
            if !merging || node == root || own(m).is_some() || !merge::mergeable(m) || paned.contains(&i) {
                return None;
            }
            let home = node_home(i);
            let (lo, hi) = node_centres.get(&i)?;
            Some((i, node, home, home.transform_point((*lo + *hi) / 2.0)))
        };
        let mut world_merger: merge::Merger<(Handle<WorldMaterial>, bool)> = merge::Merger::new(chunk_size);
        let mut standard_merger: merge::Merger<(Handle<StandardMaterial>, bool)> = merge::Merger::new(chunk_size);
        let mut world_shared: HashMap<(bool, Option<usize>, String), Handle<WorldMaterial>> = HashMap::new();
        let mut standard_shared: HashMap<(bool, Option<usize>, String), Handle<StandardMaterial>> = HashMap::new();
        // Brush entity nodes' mesh bounds (node space), for culling them.
        let mut node_bounds: HashMap<Entity, (Vec3, Vec3)> = HashMap::new();
        for m in &data.meshes {
            if water_drawn && m.water.is_some() {
                continue;
            }
            let own_node = parent_of(m) != root;
            if own_node && visibility.is_some() && !m.skybox {
                let b = node_bounds
                    .entry(parent_of(m))
                    .or_insert((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)));
                for p in &m.positions {
                    b.0 = b.0.min(Vec3::from(*p));
                    b.1 = b.1.max(Vec3::from(*p));
                }
            }
            let mesh_vis = visibility.filter(|_| !own_node);
            // Chunks are placed at their centre (vertices relative to it,
            // small numbers), each a map part of its own.
            let chunks: Vec<(MapMesh, Vec<u32>, Vec3)> = vis::split_mesh(m, mesh_vis, chunk_size)
                .into_iter()
                .map(|(mut chunk, clusters)| {
                    let centre = if mesh_vis.is_some() {
                        vis::recentre(&mut chunk)
                    } else {
                        Vec3::ZERO
                    };
                    (chunk, clusters, centre)
                })
                .collect();
            let lit = lightmap.as_ref().filter(|_| m.lightmap_uvs.len() == m.positions.len());
            // Lightmapped world surfaces: Source-style texture x baked light.
            if let (Some(_), Some(lm), Some(world_materials)) = (lit, world_lightmap.as_ref(), world_materials.as_mut())
            {
                let [r, g, b] = m.color;
                // Calibration hook: MASHUP_NORMAL_G_SIGN overrides the sign;
                // 0 turns radiosity normal mapping off.
                let g_sign = std::env::var("MASHUP_NORMAL_G_SIGN")
                    .ok()
                    .and_then(|v| v.parse::<f32>().ok())
                    .unwrap_or(NORMAL_G_SIGN);
                let bumped = m.normal_map.is_some() && bumped_lightmaps.is_some() && g_sign != 0.0;
                // Two-texture blend: the weights ride in the vertex color's alpha.
                let blend = m.blend.unwrap_or_default();
                let blended = blend.texture.is_some() && m.blend_weights.len() == m.positions.len();
                let material = WorldMaterial {
                    params: WorldParams {
                        base_color: if m.texture.is_some() {
                            Vec4::ONE
                        } else {
                            Vec4::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
                        } * m.render_factor(),
                        light_scale: data.look.light_scale,
                        lightmap_scale: if source_ldr { SOURCE_LIGHTMAP_SCALE } else { 1.0 },
                        bicubic: if data.look.bicubic_lightmaps { 1.0 } else { 0.0 },
                        translucent: if m.modulate { 3.0 } else { m.alpha.shader_mode() },
                        blend: if blended { 1.0 } else { 0.0 },
                        blend_masked: if blend.mask.is_some() { 1.0 } else { 0.0 },
                        blend_normal: if bumped && blend.normal_map.is_some() { 1.0 } else { 0.0 },
                        detail: m.detail.map_or(0.0, |d| d.mode.shader_value()),
                        detail_factor: m.detail.map_or(0.0, |d| d.factor),
                        detail_scale: m.detail.map_or(Vec2::ONE, |d| Vec2::from_array(d.scale)),
                        detail_tint: m.detail.map_or(Vec4::ONE, |d| Vec3::from_array(d.tint).extend(1.0)),
                        base_uv_u: m.base_transform.shader_rows()[0],
                        base_uv_v: m.base_transform.shader_rows()[1],
                        base2_uv_u: blend.transform.shader_rows()[0],
                        base2_uv_v: blend.transform.shader_rows()[1],
                        selfillum: if m.selfillum.is_some() { 1.0 } else { 0.0 },
                        unlit: if m.unlit { 1.0 } else { 0.0 },
                        unlit_tint: m.tint.map_or(Vec4::ONE, |t| Vec3::from_array(t).extend(1.0)),
                        selfillum_tint: m.selfillum.map_or(Vec4::ONE, |s| Vec3::from_array(s.tint).extend(1.0)),
                        fog_color: fog_color(material_fog(data, view, m.skybox).as_ref()),
                        fog_range: fog_range(material_fog(data, view, m.skybox).as_ref()),
                        bumped: if bumped { 1.0 } else { 0.0 },
                        normal_g_sign: g_sign,
                        normal_x_sign: std::env::var("MASHUP_NORMAL_X_SIGN")
                            .ok()
                            .and_then(|v| v.parse::<f32>().ok())
                            .unwrap_or(NORMAL_X_SIGN),
                        alpha_cutoff: if let MapAlpha::Mask(c) = m.alpha { c } else { 0.0 },
                        debug_view: match view {
                            MapDebugView::Normal => 0.0,
                            MapDebugView::Lighting { .. } => 1.0,
                            MapDebugView::Albedo => 2.0,
                        },
                        envmap: match m.envmap {
                            Some(e) if e.cubemap.is_some() && e.imposter => 2.0,
                            Some(e) if e.cubemap.is_some() => 1.0,
                            _ => 0.0,
                        },
                        envmap_mask: match m.envmap.map(|e| e.mask) {
                            Some(EnvmapMask::NormalAlpha) if m.normal_map.is_some() => 1.0,
                            Some(EnvmapMask::BaseAlphaInverted) => 2.0,
                            Some(EnvmapMask::Texture(_)) => 3.0,
                            _ => 0.0,
                        },
                        has_normal: if m.normal_map.is_some() && g_sign != 0.0 {
                            1.0
                        } else {
                            0.0
                        },
                        envmap_contrast: m.envmap.map_or(0.0, |e| e.contrast),
                        envmap_saturation: m.envmap.map_or(1.0, |e| e.saturation),
                        envmap_fresnel: m.envmap.map_or(1.0, |e| e.fresnel),
                        envmap_tint: m.envmap.map_or(Vec4::ONE, |e| Vec3::from_array(e.tint).extend(1.0)),
                        ..WorldParams::identity_uv()
                    },
                    base: m.texture.map(|i| textures[i].clone()),
                    // Bound for radiosity bump lighting, and for reflections
                    // that follow the normal map.
                    normal: m
                        .normal_map
                        .filter(|_| bumped || (m.envmap.is_some() && g_sign != 0.0))
                        .map(|i| textures[i].clone()),
                    envmap: m.envmap.and_then(|e| e.cubemap).map(|c| cubemaps[c].clone()),
                    envmap_mask: match m.envmap.map(|e| e.mask) {
                        Some(EnvmapMask::Texture(i)) => Some(textures[i].clone()),
                        _ => None,
                    },
                    lightmap: Some(lm.clone()),
                    lightmap_b0: bumped_lightmaps.as_ref().map(|b| b[0].clone()),
                    lightmap_b1: bumped_lightmaps.as_ref().map(|b| b[1].clone()),
                    lightmap_b2: bumped_lightmaps.as_ref().map(|b| b[2].clone()),
                    base2: blend.texture.filter(|_| blended).map(|i| textures[i].clone()),
                    normal2: blend
                        .normal_map
                        .filter(|_| blended && bumped)
                        .map(|i| textures[i].clone()),
                    blend_mask: blend.mask.filter(|_| blended).map(|i| textures[i].clone()),
                    detail: m.detail.map(|d| textures[d.texture].clone()),
                    alpha_mode: m.alpha.shader_alpha_mode(),
                    double_sided: m.double_sided,
                    // Map overlays and infodecals (see WorldMaterial::decal).
                    decal: m.material.starts_with("decal:") && m.alpha != MapAlpha::Opaque,
                    modulate: m.modulate,
                };
                let material = world_shared
                    .entry((m.skybox, own(m), format!("{material:?}")))
                    .or_insert_with(|| world_materials.add(material))
                    .clone();
                let merged = merge_spot(m);
                if let Some((_, node, home, centre)) = &merged {
                    world_merger.add((material.clone(), blended), *node, home, *centre, m);
                }
                for (chunk, clusters, centre) in chunks {
                    let mut e = commands.spawn((
                        Name::new(chunk.material.clone()),
                        MapPart,
                        world_layer_of(chunk.skybox),
                        Mesh3d(meshes.add({
                            let mut mesh = build_mesh(&chunk, true);
                            if blended {
                                let colors: Vec<[f32; 4]> =
                                    chunk.blend_weights.iter().map(|w| [1.0, 1.0, 1.0, *w]).collect();
                                mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
                            }
                            mesh
                        })),
                        MeshMaterial3d(material.clone()),
                        Transform::from_translation(centre),
                        ChildOf(if own_node { parent_of(m) } else { statics }),
                    ));
                    if merged.is_some() {
                        e.insert((merge::MergedPiece, Visibility::Hidden));
                    }
                    if m.pane_look != PaneLook::Whole {
                        e.insert((PanePart(m.pane_look), Visibility::Hidden));
                    }
                    tag(&mut e, clusters);
                }
                continue;
            }
            let material = build_material(m, &textures, view, data.look.light_scale);
            let material = standard_shared
                .entry((m.skybox, own(m), format!("{material:?}")))
                .or_insert_with(|| materials.add(material))
                .clone();
            let merged = merge_spot(m);
            if let Some((_, node, home, centre)) = &merged {
                standard_merger.add((material.clone(), lit.is_some()), *node, home, *centre, m);
            }
            for (chunk, clusters, centre) in chunks {
                let mut part = commands.spawn((
                    Name::new(chunk.material.clone()),
                    MapPart,
                    world_layer_of(chunk.skybox),
                    Mesh3d(meshes.add(build_mesh(&chunk, lit.is_some()))),
                    MeshMaterial3d(material.clone()),
                    Transform::from_translation(centre),
                    ChildOf(if own_node { parent_of(m) } else { statics }),
                ));
                if let Some(image) = lit {
                    part.insert(bevy::pbr::Lightmap {
                        image: image.clone(),
                        uv_rect: Rect::new(0.0, 0.0, 1.0, 1.0),
                        bicubic_sampling: false,
                    });
                }
                if merged.is_some() {
                    part.insert((merge::MergedPiece, Visibility::Hidden));
                }
                if !m.frames.is_empty() {
                    part.insert(tint::TextureFrames(
                        m.frames.iter().map(|f| textures[*f].clone()).collect(),
                    ));
                }
                if m.pane_look != PaneLook::Whole {
                    part.insert((PanePart(m.pane_look), Visibility::Hidden));
                }
                tag(&mut part, clusters);
            }
        }
        // The combined meshes, under the map's root (world space).
        let mut node_chunks: HashMap<Entity, Vec<Entity>> = HashMap::new();
        let mut spawn_chunk = |commands: &mut Commands,
                               mesh: Handle<Mesh>,
                               lightmap: Option<bevy::pbr::Lightmap>,
                               name: &str,
                               centre: Vec3,
                               parts: Vec<(Entity, std::ops::Range<usize>)>,
                               indices: Vec<u32>,
                               clusters: Vec<u32>|
         -> Entity {
            let mut e = commands.spawn((
                Name::new(format!("Merged brush entities ({name})")),
                MapPart,
                world_layer_of(false),
                Mesh3d(mesh),
                Transform::from_translation(centre),
                Visibility::default(),
                ChildOf(statics),
            ));
            if let Some(l) = lightmap {
                e.insert(l);
            }
            let id = e.id();
            for (node, _) in &parts {
                let list = node_chunks.entry(*node).or_default();
                if list.last() != Some(&id) {
                    list.push(id);
                }
            }
            e.insert(merge::MergedChunk { parts, indices });
            if visibility.is_some() && !clusters.is_empty() {
                e.insert(vis::VisClusters::new(clusters));
            }
            id
        };
        for ((material, blended), chunk, centre, parts, clusters) in world_merger.finish(visibility) {
            let mut mesh = build_mesh(&chunk, true);
            if blended {
                let colors: Vec<[f32; 4]> = chunk.blend_weights.iter().map(|w| [1.0, 1.0, 1.0, *w]).collect();
                mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
            }
            let mesh = meshes.add(mesh);
            let e = spawn_chunk(
                &mut commands,
                mesh,
                None,
                &chunk.material,
                centre,
                parts,
                chunk.indices,
                clusters,
            );
            commands.entity(e).insert(MeshMaterial3d(material));
        }
        for ((material, lit), chunk, centre, parts, clusters) in standard_merger.finish(visibility) {
            let lm = lightmap.as_ref().filter(|_| lit).map(|image| bevy::pbr::Lightmap {
                image: image.clone(),
                uv_rect: Rect::new(0.0, 0.0, 1.0, 1.0),
                bicubic_sampling: false,
            });
            let mesh = meshes.add(build_mesh(&chunk, lit));
            let e = spawn_chunk(
                &mut commands,
                mesh,
                lm,
                &chunk.material,
                centre,
                parts,
                chunk.indices,
                clusters,
            );
            commands.entity(e).insert(MeshMaterial3d(material));
        }
        let node_index: HashMap<Entity, usize> = entity_nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| n.map(|n| (n, i)))
            .collect();
        for (node, chunks) in node_chunks {
            let Some(&i) = node_index.get(&node) else { continue };
            commands.entity(node).insert(merge::MergedBrush {
                home: node_home(i),
                chunks,
                merged: true,
            });
        }
        for (node, (min, max)) in node_bounds {
            commands
                .entity(node)
                .insert((BrushEntityBounds { min, max }, vis::VisClusters::new(Vec::new())));
        }
        model_parts = data
            .models
            .iter()
            .map(|model| {
                model
                    .meshes
                    .iter()
                    .map(|m| {
                        (
                            meshes.add(build_mesh(m, false)),
                            materials.add(build_material(m, &textures, view, data.look.light_scale)),
                        )
                    })
                    .collect()
            })
            .collect();
        if let Some(prop_materials) = prop_materials.as_mut() {
            lit_model_materials = data
                .models
                .iter()
                .map(|model| {
                    model
                        .meshes
                        .iter()
                        .map(|m| {
                            [false, true]
                                .map(|skybox| prop_materials.add(lit_prop_material(m, &textures, data, view, skybox)))
                        })
                        .collect()
                })
                .collect();
            skin_model_materials = data
                .models
                .iter()
                .map(|model| {
                    model
                        .skins
                        .iter()
                        .map(|looks| {
                            model
                                .meshes
                                .iter()
                                .zip(looks)
                                .map(|(m, look)| {
                                    let m = look.apply(m);
                                    [false, true].map(|skybox| {
                                        prop_materials.add(lit_prop_material(&m, &textures, data, view, skybox))
                                    })
                                })
                                .collect()
                        })
                        .collect()
                })
                .collect();
        }

        if view == MapDebugView::Normal
            && let Some(props) = &data.detail_props
            && let Some(detail_materials) = detail_materials.as_mut()
        {
            detail::spawn(
                &mut commands,
                meshes,
                detail_materials,
                props,
                textures[props.texture].clone(),
                fog::world_fog(data.fog.as_ref()),
                statics,
                |e, lo, hi| tag(e, visibility.map(|v| vis::box_clusters(v, lo, hi)).unwrap_or_default()),
            );
        }
        if view == MapDebugView::Normal {
            for (i, dust) in data.dust.iter().enumerate() {
                let mesh = meshes.add(dust::empty_mesh());
                // Culled by its bounds grown by its largest mote (9).
                let grow = Vec3::splat(if dust.world_size { dust.size.1 } else { 0.0 });
                let clusters = visibility
                    .map(|v| vis::box_clusters(v, dust.min - grow, dust.max + grow))
                    .unwrap_or_default();
                let mut e = commands.spawn((
                    Name::new(format!("Dust {i}")),
                    MapPart,
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color_texture: dust.texture.map(|t| textures[t].clone()),
                        unlit: true,
                        alpha_mode: AlphaMode::Blend,
                        double_sided: true,
                        cull_mode: None,
                        ..default()
                    })),
                    dust::DustEmitter::new(dust.clone(), mesh, i as u64 + 1),
                    // The mesh moves with the motes; don't cull it by stale bounds.
                    bevy::camera::visibility::NoFrustumCulling,
                    bevy::light::NotShadowCaster,
                    Transform::default(),
                    ChildOf(root),
                ));
                tag(&mut e, clusters);
                if let Some(entity) = dust.entity {
                    e.insert(EntityPart {
                        entity,
                        on: dust.start_on,
                        exists: true,
                    });
                }
            }
            // Steam jets: a puff mesh each, rebuilt per frame.
            for (i, jet) in data.steam.iter().enumerate() {
                let mesh = meshes.add(dust::empty_mesh());
                let end = jet.origin + jet.forward * jet.length;
                let reach = Vec3::splat(jet.length * 0.5 + jet.end_size * 2.0);
                let clusters = visibility
                    .map(|v| vis::box_clusters(v, jet.origin.min(end) - reach, jet.origin.max(end) + reach))
                    .unwrap_or_default();
                let mut e = commands.spawn((
                    Name::new(format!("Steam {i}")),
                    MapPart,
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color_texture: jet.texture.map(|t| textures[t].clone()),
                        unlit: true,
                        alpha_mode: AlphaMode::Blend,
                        double_sided: true,
                        cull_mode: None,
                        ..default()
                    })),
                    steam::SteamEmitter::new(jet.clone(), mesh, i as u64 + 1),
                    bevy::camera::visibility::NoFrustumCulling,
                    bevy::light::NotShadowCaster,
                    Transform::default(),
                    ChildOf(root),
                ));
                tag(&mut e, clusters);
                if let Some(entity) = jet.entity {
                    e.insert(EntityPart {
                        entity,
                        on: jet.start_on,
                        exists: true,
                    });
                }
            }
            // Sprite trails and smoke stacks: an entity each where its map
            // entity is (moved with it), drawn through the particle pool.
            for (i, t) in data.trails.iter().enumerate() {
                let mut e = commands.spawn((
                    Name::new(format!("Trail {i}")),
                    MapPart,
                    emitters::TrailEmitter::new(t.clone()),
                    emitters::FollowsEntity,
                    Transform::from_translation(t.position),
                    Visibility::default(),
                    ChildOf(root),
                ));
                if let Some(entity) = t.entity {
                    e.insert(EntityPart {
                        entity,
                        on: true,
                        exists: true,
                    });
                }
            }
            for (i, p) in data.particle_systems.placed.iter().enumerate() {
                let reach = Vec3::splat(256.0 * data.particle_systems.scale);
                let clusters = visibility
                    .map(|v| vis::box_clusters(v, p.position - reach, p.position + reach))
                    .unwrap_or_default();
                let mut e = commands.spawn((
                    Name::new(format!("Particle system {i}")),
                    MapPart,
                    psys::SystemEmitter::new(p.clone(), i as u64 + 1),
                    emitters::FollowsEntity,
                    Transform::from_translation(p.position).with_rotation(p.rotation),
                    Visibility::default(),
                    ChildOf(root),
                ));
                tag(&mut e, clusters);
                if let Some(entity) = p.entity {
                    e.insert(EntityPart {
                        entity,
                        on: p.start_on,
                        exists: true,
                    });
                }
            }
            if !data.particle_systems.defs.is_empty() {
                commands.insert_resource(psys::ParticleSystemDefs(data.particle_systems.clone()));
            }
            color_correction::spawn(&mut commands, data, root);
            for (i, m) in data.muzzle_flashes.iter().enumerate() {
                let mut e = commands.spawn((
                    Name::new(format!("Muzzle flash {i}")),
                    MapPart,
                    emitters::MuzzleFlashEmitter(m.clone()),
                    emitters::FollowsEntity,
                    Transform::from_translation(m.position).with_rotation(m.rotation),
                    Visibility::default(),
                    ChildOf(root),
                ));
                if let Some(entity) = m.entity {
                    e.insert(EntityPart {
                        entity,
                        on: true,
                        exists: true,
                    });
                }
            }
            for (i, m) in data.embers.iter().enumerate() {
                let clusters = visibility
                    .map(|v| vis::box_clusters(v, m.min, m.max))
                    .unwrap_or_default();
                let mut e = commands.spawn((
                    Name::new(format!("Embers {i}")),
                    MapPart,
                    emitters::EmbersEmitter::new(m.clone(), i as u64 + 1),
                    emitters::FollowsEntity,
                    Transform::from_translation(m.origin),
                    Visibility::default(),
                    ChildOf(root),
                ));
                tag(&mut e, clusters);
                if let Some(entity) = m.entity {
                    e.insert(EntityPart {
                        entity,
                        on: m.start_on,
                        exists: true,
                    });
                }
            }
            for (i, t) in data.teslas.iter().enumerate() {
                let mut e = commands.spawn((
                    Name::new(format!("Tesla {i}")),
                    MapPart,
                    emitters::TeslaEmitter::new(t.clone(), i as u64 + 1),
                    emitters::FollowsEntity,
                    Transform::from_translation(t.position),
                    Visibility::default(),
                    ChildOf(root),
                ));
                if let Some(entity) = t.entity {
                    e.insert(EntityPart {
                        entity,
                        on: true,
                        exists: true,
                    });
                }
            }
            for (i, s) in data.strikes.iter().enumerate() {
                commands.spawn((
                    Name::new(format!("Strike beams {i}")),
                    MapPart,
                    emitters::StrikeEmitter::new(s.clone(), i as u64 + 1),
                    Transform::default(),
                    Visibility::default(),
                    ChildOf(root),
                ));
            }
            for (i, s) in data.smokestacks.iter().enumerate() {
                let reach = s.length + s.base_spread + s.end_size * 2.0;
                let clusters = visibility
                    .map(|v| vis::box_clusters(v, s.position - Vec3::splat(reach), s.position + Vec3::splat(reach)))
                    .unwrap_or_default();
                let mut e = commands.spawn((
                    Name::new(format!("Smoke stack {i}")),
                    MapPart,
                    emitters::SmokeStackEmitter::new(s.clone(), i as u64 + 1),
                    emitters::FollowsEntity,
                    Transform::from_translation(s.position).with_rotation(s.rotation),
                    Visibility::default(),
                    ChildOf(root),
                ));
                tag(&mut e, clusters);
                if let Some(entity) = s.entity {
                    e.insert(EntityPart {
                        entity,
                        on: s.start_on,
                        exists: true,
                    });
                }
            }
        }
        if let Some(sprite_materials) = sprite_materials.as_mut()
            && view == MapDebugView::Normal
        {
            for (i, sprite) in data.sprites.iter().enumerate() {
                let t = &data.textures[sprite.texture];
                let mut e = commands.spawn((
                    Name::new(format!("Sprite {i}")),
                    MapPart,
                    Mesh3d(meshes.add(sprite_material::sprite_mesh(UVec2::new(t.width, t.height)))),
                    MeshMaterial3d(sprite_materials.add(SpriteMaterial {
                        params: SpriteParams {
                            color: sprite.color,
                            size: sprite.size,
                        },
                        texture: Some(textures[sprite.texture].clone()),
                        glow: sprite.glow,
                        blend: sprite.blend,
                    })),
                    bevy::light::NotShadowCaster,
                    // The quad is spread in the vertex shader, so the mesh's
                    // own bounds are a point: culling would drop the sprite
                    // as soon as its centre left the view.
                    bevy::camera::visibility::NoFrustumCulling,
                    emitters::FollowsEntity,
                    Transform::from_translation(sprite.position),
                    ChildOf(root),
                ));
                if let Some(v) = visibility {
                    let r = Vec3::splat(sprite.size.max_element() / 2.0);
                    tag(&mut e, vis::box_clusters(v, sprite.position - r, sprite.position + r));
                }
                if sprite.glow {
                    e.insert(GlowSprite {
                        color: sprite.color,
                        proxy: sprite.proxy,
                    });
                }
                if let Some(entity) = sprite.entity {
                    e.insert(EntityPart {
                        entity,
                        on: sprite.start_on,
                        exists: true,
                    });
                    if !sprite.start_on {
                        e.insert((vis::LogicHidden, Visibility::Hidden));
                    }
                }
            }
        }
        // Beams (point_spotlight, env_laser, env_beam), spotlights' halos
        // and light glows (`beams`).
        if let (Some(beam_materials), Some(sprite_materials)) = (beam_materials.as_mut(), sprite_materials.as_mut())
            && view == MapDebugView::Normal
        {
            let part = |e: &mut EntityCommands, entity: Option<usize>, on: bool| {
                if let Some(entity) = entity {
                    e.insert(EntityPart {
                        entity,
                        on,
                        exists: true,
                    });
                }
                if !on {
                    e.insert((vis::LogicHidden, Visibility::Hidden));
                }
            };
            let beam_mesh = meshes.add(beams::beam_mesh());
            // Hidden where the camera can't see them: the clusters along a
            // beam (a box every meter), around a glow.
            let segment_clusters = |a: Vec3, b: Vec3| {
                let Some(v) = visibility else { return Vec::new() };
                let steps = (a.distance(b).ceil() as usize).clamp(1, 4096);
                let mut out = Vec::new();
                for k in 0..=steps {
                    let p = a.lerp(b, k as f32 / steps as f32);
                    v.clusters_in_box(p - Vec3::splat(0.5), p + Vec3::splat(0.5), &mut out);
                }
                out.sort_unstable();
                out.dedup();
                out
            };
            for (i, b) in data.beams.iter().enumerate() {
                let mut e = commands.spawn((
                    Name::new(format!("Beam {i}")),
                    MapPart,
                    Mesh3d(beam_mesh.clone()),
                    MeshMaterial3d(beam_materials.add(beams::BeamMaterial {
                        params: b.params(),
                        texture: Some(textures[b.texture].clone()),
                    })),
                    bevy::light::NotShadowCaster,
                    // Placed in the vertex shader.
                    bevy::camera::visibility::NoFrustumCulling,
                    Transform::default(),
                    ChildOf(root),
                ));
                part(&mut e, b.entity, b.start_on);
                // Clusters along the beam, out to its noise's reach (fractal
                // samples stay within ±2).
                let reach = 2.0 * b.noise * b.start.distance(b.end) / 100.0;
                let mut clusters = segment_clusters(b.start, b.end);
                if reach > 0.0
                    && let Some(v) = visibility
                {
                    let (lo, hi) = (
                        b.start.min(b.end) - Vec3::splat(reach),
                        b.start.max(b.end) + Vec3::splat(reach),
                    );
                    v.clusters_in_box(lo, hi, &mut clusters);
                    clusters.sort_unstable();
                    clusters.dedup();
                }
                tag(&mut e, clusters);
                if b.animated() {
                    e.insert(beams::BeamMotion {
                        beam: b.clone(),
                        frames: b.frames.iter().map(|f| textures[*f].clone()).collect(),
                        rng: particles::ParticleRng::new(i as u64 + 1),
                    });
                }
                if let Some(h) = &b.spot {
                    let t = &data.textures[h.texture];
                    let mut e = commands.spawn((
                        Name::new(format!("Beam {i} halo")),
                        MapPart,
                        Mesh3d(meshes.add(sprite_material::sprite_mesh(UVec2::new(t.width, t.height)))),
                        MeshMaterial3d(sprite_materials.add(SpriteMaterial {
                            params: SpriteParams {
                                color: h.color.extend(1.0),
                                size: Vec2::splat(h.scale * 2.0),
                            },
                            texture: Some(textures[h.texture].clone()),
                            glow: true,
                            blend: crate::map::sprite_material::SpriteBlend::Add,
                        })),
                        bevy::light::NotShadowCaster,
                        bevy::camera::visibility::NoFrustumCulling,
                        Transform::from_translation(b.start),
                        GlowSprite {
                            color: h.color.extend(1.0),
                            proxy: h.proxy,
                        },
                        beams::EyeGlow::Halo {
                            halo: h.clone(),
                            dir: (b.end - b.start).normalize_or_zero(),
                        },
                        ChildOf(root),
                    ));
                    part(&mut e, b.entity, b.start_on);
                    tag(&mut e, segment_clusters(b.start, b.start));
                }
            }
            for (i, g) in data.glows.iter().enumerate() {
                let t = &data.textures[g.texture];
                let mut e = commands.spawn((
                    Name::new(format!("Light glow {i}")),
                    MapPart,
                    Mesh3d(meshes.add(sprite_material::sprite_mesh(UVec2::new(t.width, t.height)))),
                    MeshMaterial3d(sprite_materials.add(SpriteMaterial {
                        params: SpriteParams {
                            color: g.color.extend(1.0),
                            size: g.half * 2.0,
                        },
                        texture: Some(textures[g.texture].clone()),
                        glow: true,
                        blend: crate::map::sprite_material::SpriteBlend::Add,
                    })),
                    bevy::light::NotShadowCaster,
                    bevy::camera::visibility::NoFrustumCulling,
                    Transform::from_translation(g.position),
                    GlowSprite {
                        color: g.color.extend(1.0),
                        proxy: g.proxy,
                    },
                    beams::EyeGlow::Light(g.clone()),
                    ChildOf(root),
                ));
                part(&mut e, g.entity, true);
                tag(&mut e, segment_clusters(g.position, g.position));
            }
        }
        if let Some(rope_materials) = rope_materials.as_mut()
            && view == MapDebugView::Normal
        {
            for (i, rope) in data.ropes.iter().enumerate() {
                let mesh = meshes.add(rope_material::rope_mesh(rope));
                let clusters = match visibility {
                    Some(v) if !rope.points.is_empty() => {
                        let lo = rope.points.iter().fold(Vec3::MAX, |a, p| a.min(*p));
                        let hi = rope.points.iter().fold(Vec3::MIN, |a, p| a.max(*p));
                        let r = Vec3::splat(rope.width);
                        vis::box_clusters(v, lo - r, hi + r)
                    }
                    _ => Vec::new(),
                };
                let strips =
                    rope.back
                        .map(|(t, n)| (t, n, true))
                        .into_iter()
                        .chain([(rope.texture, rope.normal_map, false)]);
                let mut swaying = rope.sway.clone();
                for (texture, normal, back) in strips {
                    let mut e = commands.spawn((
                        Name::new(format!("Rope {i}{}", if back { " (back)" } else { "" })),
                        MapPart,
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(rope_materials.add(RopeMaterial {
                            params: RopeParams {
                                width: rope.width,
                                back: if back { 1.0 } else { 0.0 },
                                light_scale: data.look.light_scale,
                                has_normal_map: if normal.is_some() { 1.0 } else { 0.0 },
                                fog: fog::world_fog(data.fog.as_ref()),
                            },
                            base: texture.map(|t| textures[t].clone()),
                            normal: normal.map(|t| textures[t].clone()),
                            blend: back,
                        })),
                        bevy::light::NotShadowCaster,
                        Transform::default(),
                        ChildOf(statics),
                    ));
                    tag(&mut e, clusters.clone());
                    // The strips share the mesh: one of them moves it.
                    if let Some(sway) = swaying.take() {
                        e.insert(rope_sway::RopeMotion {
                            gusts: rope_sway::Gusts::new(i as u64 + 1),
                            sway,
                            look: rope.clone(),
                            mesh: mesh.clone(),
                            pending: 0.0,
                            resting: true,
                        });
                    }
                }
            }
        }
        // Dynamic shadows: built by `shadows::apply_shadow_detail` as
        // Video > Advanced's shadow detail asks (and again when it changes).
        if let (Some(settings), true) = (&data.shadows, shadow_materials.is_some())
            && view == MapDebugView::Normal
        {
            commands.insert_resource(shadows::ShadowSource {
                data: data.clone(),
                settings: settings.clone(),
                receivers: Arc::new(shadows::Receivers::new(data)),
                root: statics,
                fog_color: fog_color(data.fog.as_ref()),
                fog_range: fog_range(data.fog.as_ref()),
                tag: visibility.is_some(),
                built: None,
                blob_material: None,
            });
        }
    }

    // Exact brushes for movement that sweeps them (Source): the world's,
    // plus props that are (close to) convex, in world space.
    let mut brushes = data.collision_brushes.clone();
    // Comparison hook: MASHUP_NO_PROP_BRUSHES=1 sweeps props as meshes.
    let no_prop_brushes = std::env::var("MASHUP_NO_PROP_BRUSHES").is_ok_and(|v| v == "1");
    // Models without a collision model: their hull, when (nearly) convex.
    let model_hulls: Vec<Option<Vec<(Vec3, f32)>>> = data
        .models
        .iter()
        .map(|m| {
            m.collision
                .is_none()
                .then(|| convex_planes(m, CONVEX_SURFACE_SHARE))
                .flatten()
        })
        .collect();
    info!(
        "props: {} of {} models collide by their collision model, {} more as convex hulls; {} have levels of detail",
        data.models.iter().filter(|m| m.collision.is_some()).count(),
        data.models.len(),
        model_hulls.iter().filter(|h| h.is_some()).count(),
        data.models.iter().filter(|m| !m.lods.is_empty()).count(),
    );

    for (i, prop) in data.props.iter().enumerate() {
        // Convex pieces as planes: the model's collision model (each piece
        // exactly), else its hull when (nearly) convex, else the bounds box.
        let model = &data.models[prop.model];
        let pieces: Vec<Vec<(Vec3, f32)>> = match (prop.solid, &model.collision) {
            (PropSolid::Mesh, Some(c)) => c.pieces.iter().map(|p| p.planes.clone()).collect(),
            (PropSolid::Mesh, None) => model_hulls[prop.model].iter().cloned().collect(),
            (PropSolid::Box, _) => {
                let (lo, hi) = model.bounds;
                ((hi - lo).min_element() > 0.0)
                    .then(|| MapBrush::from_box(lo, hi).planes)
                    .into_iter()
                    .collect()
            }
            (PropSolid::None, _) => Vec::new(),
        };
        // Bodies that move are swept through physics queries, not as brushes.
        // Frozen ones stay put like static props until enabled
        // (`prop_physics::FrozenBody`).
        let dynamic = prop.physics.as_ref().filter(|p| !prop.skybox && !p.frozen);
        let prop_brush =
            (prop.parent.is_none() && !prop.skybox && !no_prop_brushes && !pieces.is_empty() && dynamic.is_none())
                .then_some(());
        // A prop entity's brushes go on its node (`MovingSolid`, still), so
        // they stop blocking once the logic removes it; others join the map's.
        let placed_brushes: Vec<MapBrush> = if prop_brush.is_some() {
            pieces
                .iter()
                .map(|planes| place_brush(planes, prop.translation, prop.rotation, model.surfaceprop.clone()))
                .collect()
        } else {
            Vec::new()
        };
        let own_solid = (prop.entity.is_some() && !placed_brushes.is_empty()).then(|| MovingSolid {
            brushes: placed_brushes.clone(),
            velocity: Vec3::ZERO,
            solid: true,
        });
        if own_solid.is_none() {
            brushes.extend(placed_brushes);
        }
        let placed = Transform::from_translation(prop.translation).with_rotation(prop.rotation);
        // Riding a mover: under its node, placed relative to it.
        let rider = prop
            .parent
            .and_then(|p| Some((entity_nodes.get(p).copied().flatten()?, data.entities.get(p)?)))
            .map(|(node, ent)| {
                let at = Transform::from_translation(entities::entity_to_engine(ent.origin(), data.entity_scale))
                    .with_rotation(entities::rotation_to_engine(entities::entity_rotation(ent.angles())));
                (
                    node,
                    Transform::from_matrix(at.to_matrix().inverse() * placed.to_matrix()),
                )
            });
        let mut e = commands.spawn((
            Name::new(format!("Prop {i}")),
            PropIndex(i),
            MapPart,
            rider.map_or(placed, |r| r.1),
            Visibility::default(),
            // Static props (no entity, no physics) never move.
            ChildOf(rider.map_or(
                if prop.entity.is_none() && prop.physics.is_none() {
                    statics
                } else {
                    root
                },
                |r| r.0,
            )),
        ));
        if let Some(solid) = own_solid {
            e.insert(solid);
        }
        // Props are hidden where the camera can't see them, and beyond
        // their fade distance; between the near and far fade distances
        // their meshes dither out (Bevy's visibility range: opaque passes,
        // no sorting). Props that stay put are tagged once; moving ones
        // (physics, riders, animated) follow their bounds as they move
        // (`tag_moved_brush_entities`), as Source culls every entity by
        // the view's PVS: other rooms' props must not show through the sky.
        let mut fade_range = None;
        let stays_put = dynamic.is_none() && rider.is_none() && model.rig.is_none();
        if !prop.skybox && !stays_put && !merged_world() && data.visibility.is_some() {
            let (min, max) = model.bounds;
            if min.x <= max.x {
                e.insert((BrushEntityBounds { min, max }, vis::VisClusters::new(Vec::new())));
            }
            if let Some((near, far)) = prop.fade {
                e.insert(vis::FadeDistance(far));
                fade_range = vis::fade_band(near, far);
            }
        }
        if !prop.skybox && stays_put && !merged_world() {
            let clusters = match data.visibility.as_deref() {
                Some(v) => {
                    let (lo, hi) = model.bounds;
                    let corners = (0..8).map(|k| {
                        let c = Vec3::new(
                            if k & 1 == 0 { lo.x } else { hi.x },
                            if k & 2 == 0 { lo.y } else { hi.y },
                            if k & 4 == 0 { lo.z } else { hi.z },
                        );
                        prop.translation + prop.rotation * c
                    });
                    let (min, max) = corners.fold((Vec3::MAX, Vec3::MIN), |(a, b), p| (a.min(p), b.max(p)));
                    // Hidden behind occluders by its bounds too.
                    if !v.occluders.is_empty() && min.x <= max.x {
                        e.insert(vis::Occludee { min, max });
                    }
                    vis::box_clusters(v, min, max)
                }
                None => Vec::new(),
            };
            if !clusters.is_empty() || prop.fade.is_some() {
                e.insert(vis::VisClusters::new(clusters));
            }
            if let Some((near, far)) = prop.fade {
                e.insert(vis::FadeDistance(far));
                fade_range = vis::fade_band(near, far);
            }
        }
        if let Some(index) = prop.entity {
            e.insert((PropEntity(index), PropHome(rider.map_or(placed, |r| r.1))));
            // func_fish_pool's fish swim about its origin (`fish`).
            if let Some(pool) = data
                .entities
                .get(index)
                .filter(|p| p.classname().eq_ignore_ascii_case("func_fish_pool"))
            {
                let range = pool
                    .get("max_range")
                    .map_or(150.0, |v| v.trim().parse().unwrap_or(150.0));
                e.insert(fish::Fish::new(
                    entities::entity_to_engine(pool.origin(), data.entity_scale),
                    range * data.entity_scale,
                    i as u64 + 1,
                ));
            }
        }
        let solid = if rider.is_some() { PropSolid::None } else { prop.solid };
        match (solid, &model_colliders[prop.model]) {
            (PropSolid::Mesh, Some(collider)) if dynamic.is_some() => {
                let p = dynamic.unwrap();
                e.insert((
                    collider.clone(),
                    prop_physics::dynamic_body(p, data.models[prop.model].bounds),
                ));
                if p.asleep && prop.entity.is_some() {
                    e.insert((prop_physics::StartAsleep, RigidBody::Static));
                }
                if let Some(s) = &model.surfaceprop {
                    e.insert(PropSurface(s.clone()));
                }
            }
            (PropSolid::Mesh, Some(collider)) => {
                e.insert((RigidBody::Static, collider.clone(), MapPropCollider));
                // A frozen physics prop can be set moving later.
                if let Some(p) = prop
                    .physics
                    .as_ref()
                    .filter(|p| p.frozen && !prop.skybox && prop.entity.is_some())
                {
                    e.insert(prop_physics::FrozenBody {
                        physics: p.clone(),
                        bounds: data.models[prop.model].bounds,
                    });
                }
                if let Some(s) = &model.surfaceprop {
                    e.insert(PropSurface(s.clone()));
                }
                if prop_brush.is_some() {
                    e.insert(MapBrushCollider);
                }
            }
            (PropSolid::Box, _) => {
                let (lo, hi) = data.models[prop.model].bounds;
                let size = hi - lo;
                if size.min_element() > 0.0 {
                    let child = (
                        Collider::cuboid(size.x, size.y, size.z),
                        Transform::from_translation((lo + hi) / 2.0),
                    );
                    e.insert(RigidBody::Static).with_children(|c| {
                        let mut ec = c.spawn((child, MapPropCollider));
                        if let Some(s) = &model.surfaceprop {
                            ec.insert(PropSurface(s.clone()));
                        }
                        if prop_brush.is_some() {
                            ec.insert(MapBrushCollider);
                        }
                    });
                }
            }
            _ => {}
        }
        let id = e.id();
        let probe_scale = data.look.light_scale
            * if let MapDebugView::Lighting { scale } = view {
                scale
            } else {
                1.0
            };
        let probe = prop.lighting.as_ref().filter(|_| view != MapDebugView::Albedo);
        let model = &data.models[prop.model];
        // An animated prop's skeleton: a root in the skeleton's space under
        // the node, then each bone under its parent, in the reference pose.
        let rig = model.rig.as_ref().filter(|_| meshes.is_some() && bindposes.is_some());
        let mut joints: Vec<Entity> = Vec::new();
        let mut inverse_bindposes = None;
        if let (Some(rig), Some(bindposes)) = (rig, bindposes.as_mut()) {
            let root = commands.spawn((rig.root, Visibility::Inherited, ChildOf(id))).id();
            for (i, b) in rig.bones.iter().enumerate() {
                let parent = b.parent.and_then(|p| joints.get(p).copied()).unwrap_or(root);
                let (rotation, position) = prop
                    .pose
                    .as_ref()
                    .and_then(|pose| pose.get(i).copied())
                    .unwrap_or((b.rotation, b.position));
                joints.push(
                    commands
                        .spawn((
                            Transform::from_translation(position).with_rotation(rotation),
                            Visibility::Inherited,
                            ChildOf(parent),
                        ))
                        .id(),
                );
            }
            inverse_bindposes = Some(
                rig_bindposes
                    .entry(prop.model)
                    .or_insert_with(|| bindposes.add(rig_inverse_bindposes(rig)))
                    .clone(),
            );
        }
        // Body parts: only the meshes of the chosen choice are drawn.
        let body_shown = |m: &MapMesh| {
            m.body
                .is_none_or(|(part, choice)| model.body_choice(prop.body, part as usize) == choice)
        };
        let mut spawned_materials: Vec<Handle<PropMaterial>> = Vec::new();
        // Levels of detail (`lod`): not for skinned models or the 3D
        // skybox's props.
        let leveled = !model.lods.is_empty() && model.rig.is_none() && !prop.skybox;
        if leveled {
            commands
                .entity(id)
                .insert(lod::LodGroup::new(model.bounds, &model.lods));
        }
        match (probe, meshes.as_mut(), lit_model_materials.get(prop.model)) {
            // Baked: own mesh copy with per-vertex light, unlit material
            // (texture x light, like the lightmapped world).
            (Some(probe), Some(meshes), Some(mats)) => {
                for (mesh_index, (m, material)) in model.meshes.iter().zip(mats).enumerate() {
                    let mut material = material[prop.skybox as usize].clone();
                    // `env_cubemap`: the cubemap baked nearest to the prop.
                    if let (Some(env), Some(prop_materials)) =
                        (m.envmap.filter(|e| e.cubemap.is_none()), prop_materials.as_mut())
                        && let Some(&(_, cube)) = data.cubemap_samples.iter().min_by(|a, b| {
                            a.0.distance(prop.translation)
                                .total_cmp(&b.0.distance(prop.translation))
                        })
                        && let Some(cube_handle) = cubemap_handles.get(cube)
                    {
                        let key = (prop.model, mesh_index, prop.skybox, cube);
                        material = envmap_variants
                            .entry(key)
                            .or_insert_with(|| {
                                let mut variant = prop_materials.get(&material).cloned().expect("prop material");
                                set_prop_envmap(&mut variant, &env, cube_handle.clone(), &texture_handles);
                                prop_materials.add(variant)
                            })
                            .clone();
                    }
                    spawned_materials.push(material.clone());
                    let layer = layer_of(prop.skybox);
                    // Light baked per vertex when the prop has it (and it
                    // covers the mesh), else the probe on each normal.
                    let colors = |m: &MapMesh| -> Vec<[f32; 4]> {
                        let baked = prop.vertex_light.as_ref().filter(|v| {
                            m.source_vertices.len() == m.normals.len()
                                && m.source_vertices.iter().all(|&i| (i as usize) < v.len())
                        });
                        m.normals
                            .iter()
                            .enumerate()
                            .map(|(i, n)| {
                                if m.unlit {
                                    return [1.0, 1.0, 1.0, 1.0];
                                }
                                let l = match baked {
                                    Some(v) => Vec3::from(v[m.source_vertices[i] as usize]),
                                    None => probe.eval(prop.rotation * Vec3::from(*n)),
                                } * probe_scale;
                                [l.x, l.y, l.z, 1.0]
                            })
                            .collect()
                    };
                    let mut mesh = build_mesh(m, false);
                    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors(m));
                    let skinned = inverse_bindposes.is_some() && m.joints.len() == m.positions.len();
                    if skinned {
                        mesh.insert_attribute(
                            Mesh::ATTRIBUTE_JOINT_INDEX,
                            bevy::mesh::VertexAttributeValues::Uint16x4(m.joints.clone()),
                        );
                        mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, m.joint_weights.clone());
                    }
                    let handle = meshes.add(mesh);
                    // Its lower levels, lit the same way (a level without
                    // this mesh keeps the finer one's).
                    let levels = (leveled && !skinned).then(|| {
                        let mut levels = vec![handle.clone()];
                        for l in &model.lods {
                            let h = match l.meshes.get(mesh_index).and_then(Option::as_ref) {
                                Some(lm) => {
                                    let mut mesh = build_mesh(lm, false);
                                    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors(lm));
                                    meshes.add(mesh)
                                }
                                None => levels.last().cloned().unwrap_or_default(),
                            };
                            levels.push(h);
                        }
                        lod::LodMeshes(levels)
                    });
                    let mut c = commands.spawn((
                        Mesh3d(handle),
                        MeshMaterial3d(material),
                        layer,
                        PropMeshSlot { mesh: mesh_index },
                        if body_shown(m) {
                            Visibility::Inherited
                        } else {
                            Visibility::Hidden
                        },
                        ChildOf(id),
                    ));
                    if let Some(range) = &fade_range {
                        c.insert(range.clone());
                    }
                    if let Some(levels) = levels {
                        c.insert(levels);
                    }
                    if let (true, Some(inverse)) = (skinned, inverse_bindposes.clone()) {
                        c.insert((
                            bevy::mesh::skinning::SkinnedMesh {
                                inverse_bindposes: inverse,
                                joints: joints.clone(),
                            },
                            // Bounds of the bind pose don't follow the bones.
                            bevy::camera::visibility::NoFrustumCulling,
                        ));
                    }
                }
            }
            (_, meshes_here, _) => {
                if let Some(parts) = model_parts.get(prop.model) {
                    // The model's levels, shared by its props (built once).
                    let levels = match (leveled, meshes_here) {
                        (true, Some(meshes)) => Some(
                            lod_parts
                                .entry(prop.model)
                                .or_insert_with(|| {
                                    parts
                                        .iter()
                                        .enumerate()
                                        .map(|(mesh_index, (mesh, _))| {
                                            let mut levels = vec![mesh.clone()];
                                            for l in &model.lods {
                                                let h = match l.meshes.get(mesh_index).and_then(Option::as_ref) {
                                                    Some(lm) => meshes.add(build_mesh(lm, false)),
                                                    None => levels.last().cloned().unwrap_or_default(),
                                                };
                                                levels.push(h);
                                            }
                                            levels
                                        })
                                        .collect()
                                })
                                .clone(),
                        ),
                        _ => None,
                    };
                    for (mesh_index, ((mesh, material), m)) in parts.iter().zip(&model.meshes).enumerate() {
                        let mut c = commands.spawn((
                            Mesh3d(mesh.clone()),
                            MeshMaterial3d(material.clone()),
                            layer_of(prop.skybox),
                            PropMeshSlot { mesh: mesh_index },
                            if body_shown(m) {
                                Visibility::Inherited
                            } else {
                                Visibility::Hidden
                            },
                            ChildOf(id),
                        ));
                        if let Some(range) = &fade_range {
                            c.insert(range.clone());
                        }
                        if let Some(levels) = levels.as_ref().and_then(|l| l.get(mesh_index)) {
                            c.insert(lod::LodMeshes(levels.clone()));
                        }
                    }
                }
            }
        }
        // What the logic may change: skin and body (entity props), sequences
        // (animated ones).
        if prop.entity.is_some() {
            let mut skins: Vec<Vec<Handle<PropMaterial>>> = skin_model_materials
                .get(prop.model)
                .map(|skins| {
                    skins
                        .iter()
                        .map(|mats| mats.iter().map(|m| m[prop.skybox as usize].clone()).collect())
                        .collect()
                })
                .unwrap_or_default();
            if let Some(own) = skins.get_mut(prop.skin.max(0) as usize)
                && own.len() == spawned_materials.len()
            {
                *own = spawned_materials;
            }
            commands.entity(id).insert(PropVariants {
                skins,
                body: prop.body,
                parts: model.body_parts.clone(),
                mesh_bodies: model.meshes.iter().map(|m| m.body).collect(),
                shown: (prop.skin, prop.body),
            });
            // A simulated ragdoll's bones follow its bodies.
            if let (Some(rig), Some(ragdoll)) = (rig.filter(|_| !joints.is_empty() && !prop.skybox), &prop.ragdoll) {
                let flags = prop
                    .entity
                    .and_then(|i| data.entities.get(i))
                    .and_then(|e| e.get("spawnflags"))
                    .map_or(0, |v| v.trim().parse().unwrap_or(0));
                commands.entity(id).insert(placed_ragdoll::PlacedRagdoll::new(
                    ragdoll.clone(),
                    rig,
                    prop.pose.clone(),
                    joints,
                    flags,
                    rider.map_or(placed, |r| r.1),
                ));
            // A posed ragdoll keeps its pose (no animation).
            } else if let Some(rig) =
                rig.filter(|_| !joints.is_empty() && prop.pose.is_none() && prop.ragdoll.is_none())
            {
                commands.entity(id).insert(PropRig {
                    animator: anim::Animator::new(rig.animations.clone()),
                    joints,
                    seen: None,
                    default: None,
                });
            }
        }
    }

    if let Some(v) = data.visibility.as_ref().filter(|_| !merged_world()) {
        commands.insert_resource(vis::ActiveVisibility(v.clone()));
    }
    commands.insert_resource(decal::DecalSurfaces::new(&data));
    commands.insert_resource(surface_color::SurfaceColors::new(data));
    commands.insert_resource(particles::ParticleMaterials(data.particles.clone()));
    if !texture_handles.is_empty() && !data.particles.materials.is_empty() {
        commands.insert_resource(particles::ParticleAssets::new(
            data.particles.clone(),
            texture_handles.clone(),
        ));
    }
    if let Some(h) = &data.hud {
        let images = h
            .sprites
            .values()
            .filter_map(|s| Some((s.texture, texture_handles.get(s.texture)?.clone())))
            .collect();
        commands.insert_resource(hud::ActiveHud(h.clone(), images));
    }
    if let Some(o) = &data.overview
        && let Some(image) = texture_handles.get(o.texture)
    {
        commands.insert_resource(hud::ActiveOverview(o.clone(), image.clone()));
    }
    if !texture_handles.is_empty() && !data.decals.groups.is_empty() {
        commands.insert_resource(decal::DecalAssets::new(data.decals.clone(), texture_handles.clone()));
        commands.insert_resource(decal::ImpactDecals);
    }
    commands.insert_resource(sound::SoundBank(data.sounds.clone()));
    if !data.characters.is_empty() {
        commands.insert_resource(CharacterModels(Arc::new(data.characters.clone())));
    }
    if !data.view_models.is_empty() {
        commands.insert_resource(ViewModels(Arc::new(data.view_models.clone())));
    }
    if let Some(field) = &data.light_field {
        commands.insert_resource(LightField(field.clone()));
    }
    if let Some(nav) = &data.nav {
        commands.insert_resource((**nav).clone());
    }
    commands.insert_resource(sound::SurfaceGrid::new(&data));
    if !brushes.is_empty() {
        let floor = data
            .playable
            .map(|(lo, _)| lo.y)
            .unwrap_or_else(|| brushes.iter().map(|b| b.min.y).fold(f32::MAX, f32::min));
        commands.insert_resource(KillHeight(floor - KILL_MARGIN));
        commands.insert_resource(MapBrushes(brushes));
        commands.insert_resource(MapTraceSkip(data.trace_skip.clone()));
        if let Some(tree) = &data.brush_tree {
            commands.insert_resource(tree.clone());
        }
        if let Some(g) = data.gravity {
            commands.insert_resource(Gravity(Vec3::NEG_Y * g));
        }
        commands.insert_resource(MapWater(data.water.clone()));
        commands.insert_resource(water::WaterMovers(
            data.water_movers
                .iter()
                .filter(|(v, e)| *v < data.water.len() && entity_nodes.get(*e).copied().flatten().is_some())
                .map(|&(v, e)| (v, e, data.water[v].brush.clone(), node_home(e)))
                .collect(),
        ));
        commands.insert_resource(fog::SceneFog(fog::world_fog(data.fog.as_ref())));
        commands.insert_resource(data.round_sounds.clone());
        if let Some(r) = &data.radio {
            commands.insert_resource(r.clone());
        }
        if let Some(p) = &data.bot_profiles {
            commands.insert_resource(p.clone());
        }
    }

    commands.spawn((
        Name::new("Sun"),
        MapPart,
        DirectionalLight {
            illuminance: 9000.0,
            // Models are lit from the baked light field (probe_lit), so no
            // shadow maps (they cost several ms a frame); the sun only lights
            // the few plain materials left.
            affects_lightmapped_mesh_diffuse: false,
            ..default()
        },
        Transform::default().looking_at(Vec3::new(-0.4, -1.0, -0.3), Vec3::Y),
    ));
    for (i, (feet, team)) in data.spawns.iter().enumerate() {
        commands.spawn((
            Name::new(format!("Spawn {i}")),
            MapPart,
            SpawnPoint { team: *team },
            Transform::from_translation(*feet + Vec3::Y * SPAWN_LIFT)
                .with_rotation(Quat::from_rotation_y(data.spawn_yaws.get(i).copied().unwrap_or(0.0))),
        ));
    }
}

/// Comparison hook: MASHUP_MERGED_WORLD=1 spawns the world as one mesh per
/// material with no visibility culling (as before chunking), for A/B
/// measurements against `vis`.
pub(crate) fn merged_world() -> bool {
    std::env::var("MASHUP_MERGED_WORLD").is_ok_and(|v| v == "1")
}
