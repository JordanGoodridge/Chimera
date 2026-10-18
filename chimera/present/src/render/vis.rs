//! Visibility sets for any game whose maps carry precomputed visibility
//! (Source's PVS, Quake-family maps): the map is divided into clusters, a
//! point query finds the camera's cluster, and each cluster lists the
//! clusters potentially visible from anywhere inside it. Map parts are
//! tagged with the clusters they touch (`VisClusters`) and hidden while
//! none of those is potentially visible from the camera's cluster.
//!
//! World meshes are merged per material by importers; `split_mesh` cuts
//! them into spatial chunks so each chunk has tight bounds (frustum
//! culling) and its own cluster list (visibility culling).
//!
//! Areaportals (Source's func_areaportal, func_areaportalwindow) divide a
//! map into areas: a closed portal (a shut door) hides the areas only
//! reachable through it, and an open one is looked through, so only what
//! its opening's screen rectangle covers is reached beyond it
//! (`MapAreas::flood`). Map parts are drawn when a camera's cluster can
//! potentially see one of their clusters and that cluster lies in an area
//! the camera reaches.
//!
//! `r_novis 1` turns culling off, `r_portalsopenall 1` area culling only.
//! Outside the map (a point in solid or in no cluster) everything is drawn.
//!
//! Visibility culling for any game with precomputed visibility: `MapVisibility`
//! (a point-to-cluster tree, per-cluster potentially visible sets),
//! `split_mesh` (world meshes cut into grid chunks with their clusters),
//! `MapAreas` (areas, areaportals with their openings, `flood`: areas reached
//! through open portals, narrowed by each opening's screen rectangle),
//! `AreaPortalStates` (portals logic closed), `WindowFade`/`fade_windows`
//! (areaportal windows' brushes fade in with distance; the portal closes behind
//! the opaque brush), `Occluder`/`OccluderStates`/`Occludee` (occluder
//! polygons, which logic turned off, props' bounds; `screen_occluders` and
//! `occluded`: a part hidden while its bounds lie behind an occluder's plane
//! and inside its projected outline; `r_occlusion 0` turns them off),
//! `VisClusters`/`FadeDistance` on map parts (brush entity nodes, moving props
//! (physics, riders, animated) and loose items by their bounds, re-found when
//! they move: `map::BrushEntityBounds`; water surfaces in chunks; beams and
//! glows along their length), `fade_band` (prop fade bands as Bevy visibility
//! ranges, dithered by `dither.wgsl` in the prop shaders), the `cull` system
//! (hide parts no camera's cluster can see or whose clusters lie in no area it
//! reaches, or beyond their fade distance; everything outside the map or with
//! `r_novis 1`; `r_portalsopenall 1` ignores areaportals; the water reflection
//! camera draws what its surface's clusters see, `water::ReflectionClusters`),
//! `VisStats`.
//!
//! Known shortcuts (docs/tech-debt.md): Visibility culling (`render::vis`, `map::vis`): no
//! per-leaf frustum culling; brush entities are culled by the clusters their
//! meshes' whole box touches (re-found whenever they move), the water
//! reflection by the clusters around its surface; the 3D skybox, physics props,
//! their shadows, characters, runtime decals and particles are never culled.
//! Areaportals follow the public BSP and entity documentation; open questions
//! taken as follows: a linked portal is closed exactly while every door named
//! by its `target` is fully closed (open from the first tick of opening to the
//! last of closing) and its own Open/Close/Toggle inputs only count when no
//! door matches; StartOpen absent = open; func_areaportalwindow: the distance
//! is from the eye to the opening's bounding box (the public docs don't say
//! from what point), the brush's alpha rises linearly from `TranslucencyLimit`
//! at `FadeStartDist` to 1 at `FadeDist` (a step when they are equal), blended
//! with Bevy's alpha blending (the game's exact blend and sort aren't known),
//! and the portal closes past `FadeDist` only when the window has a `target`
//! brush (de_port's and cs_compound's windows have none and stay open; the docs
//! leave what the game draws there open); a window brush logic disabled or
//! killed still closes it; `BackgroundBModel` isn't drawn; func_occluder:
//! occluders hide static props (and prop entities that stay put) only, not
//! world chunks, as the entity documentation describes them (hiding models;
//! whether the game also skips displacements or other world surfaces behind
//! them isn't stated); hiding world chunks too showed holes on cs_assault and
//! cs_compound, whose occluder brushes stick out of their walls and cover
//! windows (`refcmp vischeck`: up to 73% of a view), so props behind such an
//! occluder vanish here as they would in the game; occluders are used from both
//! faces, every polygon on its own (no merging of neighbours), with no minimum
//! screen area (`r_occluderminarea` and the like aren't public), not while the
//! water reflection draws, and the occluder lump's per-occluder flags and area
//! are ignored (all 0 or one area on stock maps); occluders without an entity
//! count as active; portal rectangles are the opening's projected bounds plus
//! 1% of the screen (the engine's exact clipping isn't public); a portal with a
//! non-opaque surface (glass, grates) within 0.25 m of its opening is never
//! closed (`AreaPortal::see_through`: closed, the game would show nothing
//! through it; ours is a guard, not the engine's rule). Prop fade bands are a
//! 4x4 dither by distance from the view to the prop's origin (Source blends
//! alpha; `fadescale` and `r_propsmaxdist` are ignored); a `fademindist` of 0
//! fades over the whole distance (taken literally), negative pops at
//! `fademaxdist`; physics, animated and riding props still never fade; no LOD
//! models. Why it's OK for now: Culls most of each map (docs/performance.md);
//! dithering keeps props in the opaque passes.

use bevy::prelude::*;

use crate::map::{AreaPortalStates, LogicHidden, OccluderStates, VisClusters, vis::*};

/// `r_portalsopenall`: 1 treats every areaportal as open and doesn't look
/// through them (PVS culling only).
#[derive(Resource, Default, Clone, Copy)]
pub struct PortalsOpenAll(pub u8);

/// `r_occlusion`: 0 turns occluders off.
#[derive(Resource, Clone, Copy)]
pub struct Occlusion(pub u8);

impl Default for Occlusion {
    fn default() -> Self {
        Self(1)
    }
}

/// `r_novis`: 1 draws everything (no visibility culling).
#[derive(Resource, Default, Clone, Copy)]
pub struct NoVis(pub u8);

/// What visibility culling did this frame, for `mashup_perf`.
#[derive(Resource, Default, Clone, Debug)]
pub struct VisStats {
    /// The camera's cluster (None: outside the map or culling off).
    pub cluster: Option<u32>,
    pub parts: usize,
    pub visible_parts: usize,
    /// Clusters drawn from: potentially visible from a camera's cluster
    /// and in an area it reaches.
    pub visible_clusters: usize,
    pub clusters: usize,
    /// The main camera's area (0: none) and how many areas it reaches.
    pub area: u16,
    pub visible_areas: usize,
    pub areas: usize,
    /// Areaportals closed by map logic.
    pub closed_portals: usize,
    /// Occluders active and the map's count; map parts they hide (of
    /// those otherwise drawn).
    pub active_occluders: usize,
    pub occluders: usize,
    pub occluded_parts: usize,
}

/// Each frame: find what each camera draws from (`camera_clusters`); when
/// that (or `r_novis`, or the set of tagged parts) changes, show the parts
/// in those clusters and hide the rest. Parts with a fade distance are
/// checked every frame, and so are parts with bounds (`Occludee`) while
/// the main view has active occluders in sight: those fully behind one
/// are hidden too (not while the water's reflection draws, which sees
/// from elsewhere).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn cull(
    vis: Option<Res<ActiveVisibility>>,
    novis: Res<NoVis>,
    open_all: Option<Res<PortalsOpenAll>>,
    portal_states: Option<Res<AreaPortalStates>>,
    (occlusion, occluder_states): (Option<Res<Occlusion>>, Option<Res<OccluderStates>>),
    cameras: Query<
        (
            &GlobalTransform,
            &Camera,
            Option<&Projection>,
            Has<crate::map::water::WaterReflectionCamera>,
            Option<&crate::map::water::ReflectionClusters>,
        ),
        (
            With<Camera3d>,
            Without<crate::map::monitor::ScreenCamera>,
            Without<crate::map::SkyboxCamera>,
            Without<crate::map::ViewModelCamera>,
        ),
    >,
    mut queries: ParamSet<(
        Query<(), Changed<VisClusters>>,
        Query<(
            &mut VisClusters,
            Option<&mut Visibility>,
            (Has<crate::map::GlowSprite>, Has<crate::map::water::WaterSurface>),
            Option<(&FadeDistance, &GlobalTransform)>,
            Has<LogicHidden>,
            Option<&Occludee>,
        )>,
    )>,
    mut stats: ResMut<VisStats>,
    mut last: Local<Option<Option<Vec<u64>>>>,
    mut was_occluding: Local<bool>,
) {
    // Every view that draws the map: the main one and, while it draws, the
    // water's mirrored reflection camera (whole areas: its projection is
    // mirrored).
    let active: Vec<(Vec3, Option<Mat4>, bool, Option<&[u32]>)> = cameras
        .iter()
        .filter(|(_, c, _, _, _)| c.is_active)
        .map(|(t, _, projection, reflection, surface)| {
            let clip = projection
                .filter(|_| !reflection)
                .map(|p| p.get_clip_from_view() * Mat4::from(t.affine().inverse()));
            (t.translation(), clip, reflection, surface.map(|s| &s.0[..]))
        })
        .collect();
    let eye = active.iter().find(|(_, _, r, _)| !r).map(|(p, _, _, _)| *p);
    let window_eye = eye.unwrap_or_else(|| active.first().map_or(Vec3::ZERO, |a| a.0));
    let default_states = AreaPortalStates::default();
    let states = portal_states.as_deref().unwrap_or(&default_states);
    let open_all = open_all.is_some_and(|o| o.0 != 0);
    // The union of the views' clusters; any view outside the map (or
    // r_novis 1) draws everything: None.
    let mut main = None;
    let drawn: Option<Vec<u64>> = match (&vis, novis.0) {
        (Some(v), 0) if !active.is_empty() => {
            let mut union = vec![0u64; v.0.cluster_count.div_ceil(64)];
            let mut inside = true;
            for (p, clip, reflection, surface) in &active {
                // A reflection shows what its water surface sees: the
                // clusters potentially visible from the surface's (its
                // mirrored eye is below the surface, often in solid).
                if let Some(surface) = surface.filter(|s| !s.is_empty()) {
                    for &c in surface {
                        if let Some(row) = v.0.visible.get(c as usize) {
                            for (u, w) in union.iter_mut().zip(row) {
                                *u |= w;
                            }
                        }
                    }
                    continue;
                }
                let Some((row, area, areas)) = camera_clusters(&v.0, *p, window_eye, *clip, states, open_all) else {
                    inside = false;
                    break;
                };
                if !reflection {
                    main = Some((v.0.cluster_at(*p), area, areas));
                }
                for (u, w) in union.iter_mut().zip(row) {
                    *u |= w;
                }
            }
            inside.then_some(union)
        }
        _ => None,
    };
    let changed = last.as_ref() != Some(&drawn) || !queries.p0().is_empty();
    stats.clusters = vis.as_ref().map_or(0, |v| v.0.cluster_count);
    stats.areas = vis.as_ref().map_or(0, |v| v.0.areas.area_count());
    stats.closed_portals = states.closed.len();
    let main = main.filter(|_| drawn.is_some());
    stats.cluster = main.and_then(|m| m.0);
    stats.area = main.map_or(0, |m| m.1);
    stats.visible_areas = main.map_or(stats.areas, |m| m.2);
    if changed {
        debug!(
            "visibility: camera at {eye:?}, area {}, {} areas reached",
            stats.area, stats.visible_areas
        );
        stats.visible_clusters = match &drawn {
            Some(d) => d.iter().map(|w| w.count_ones() as usize).sum(),
            None => stats.clusters,
        };
    }
    let in_set = |clusters: &[u32]| match &drawn {
        Some(d) => {
            clusters.is_empty()
                || clusters
                    .iter()
                    .any(|&c| d.get(c as usize / 64).is_some_and(|w| w >> (c % 64) & 1 != 0))
        }
        None => true,
    };
    // The main view's occluders in sight (none while a reflection draws).
    let occluders = vis.as_ref().map_or(&[][..], |v| &v.0.occluders[..]);
    let is_active = |o: &&Occluder| match &occluder_states {
        Some(s) => s.inactive.binary_search(&o.key).is_err(),
        None => o.start_active,
    };
    stats.occluders = occluders.len();
    stats.active_occluders = occluders.iter().filter(is_active).count();
    let main_clip = match active.as_slice() {
        [(p, Some(clip), false, _)] if occlusion.as_ref().is_none_or(|o| o.0 != 0) => Some((*p, *clip)),
        _ => None,
    };
    let in_sight: Vec<ScreenOccluder> = match main_clip {
        Some((p, clip)) if stats.active_occluders > 0 => screen_occluders(
            occluders
                .iter()
                .filter(is_active)
                .flat_map(|o| o.polygons.iter().map(Vec::as_slice)),
            p,
            clip,
        ),
        _ => Vec::new(),
    };
    let occluding = !in_sight.is_empty();
    let recheck_occludees = occluding || *was_occluding;
    *was_occluding = occluding;
    let (mut total, mut shown, mut hidden_by_occluders) = (0, 0, 0);
    for (mut part, visibility, (glow, water), fade, removed, occludee) in &mut queries.p1() {
        if !changed && fade.is_none() && !(recheck_occludees && occludee.is_some()) {
            total += 1;
            shown += part.potentially_visible as usize;
            continue;
        }
        let near = match (fade, eye) {
            (Some((FadeDistance(far), at)), Some(eye)) => at.translation().distance(eye) <= *far,
            _ => true,
        };
        let mut on = near && in_set(&part.clusters);
        if on
            && let (Some(b), Some((_, clip))) = (occludee, main_clip)
            && occluded(&in_sight, b.min, b.max, clip)
        {
            on = false;
            hidden_by_occluders += 1;
        }
        total += 1;
        shown += on as usize;
        if part.potentially_visible != on {
            part.potentially_visible = on;
        }
        if !glow
            && !water
            && let Some(mut v) = visibility
        {
            v.set_if_neq(if on && !removed {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
    }
    *last = Some(drawn);
    if changed {
        debug!("visibility: {shown} of {total} parts potentially visible");
    }
    stats.parts = total;
    stats.visible_parts = shown;
    if recheck_occludees || changed {
        stats.occluded_parts = hidden_by_occluders;
    }
}

/// A window brush material's own blending: alpha mode, `translucent`
/// and base alpha, kept while its fade changes them.
type OwnBlend = (AlphaMode, f32, f32);

/// Each frame: set each window brush's alpha from the main view's
/// distance to its opening (`WindowFade::alpha`). Fully transparent, its
/// meshes are hidden; partly, they blend; opaque, they draw as loaded.
#[allow(clippy::type_complexity)]
pub fn fade_windows(
    vis: Option<Res<ActiveVisibility>>,
    cameras: Query<
        (&GlobalTransform, &Camera),
        (
            With<Camera3d>,
            Without<crate::map::monitor::ScreenCamera>,
            Without<crate::map::SkyboxCamera>,
            Without<crate::map::ViewModelCamera>,
            Without<crate::map::water::WaterReflectionCamera>,
        ),
    >,
    mut brushes: Query<(&mut WindowBrush, Option<&Children>)>,
    mut panes: Query<(
        Option<&mut Visibility>,
        Option<&MeshMaterial3d<crate::render::world_material::WorldMaterial>>,
    )>,
    mut materials: Option<ResMut<Assets<crate::render::world_material::WorldMaterial>>>,
    mut own: Local<std::collections::HashMap<AssetId<crate::render::world_material::WorldMaterial>, OwnBlend>>,
) {
    let Some(vis) = vis else { return };
    let Some(eye) = cameras.iter().find(|(_, c)| c.is_active).map(|(t, _)| t.translation()) else {
        return;
    };
    for (mut brush, children) in &mut brushes {
        let Some(portal) = vis.0.areas.portals.get(brush.portal) else {
            continue;
        };
        let Some(fade) = portal.fade else { continue };
        let alpha = fade.alpha(portal.distance(eye));
        if brush.alpha == Some(alpha) {
            continue;
        }
        brush.alpha = Some(alpha);
        for &child in children.into_iter().flatten() {
            let Ok((visibility, material)) = panes.get_mut(child) else {
                continue;
            };
            if let Some(mut v) = visibility {
                v.set_if_neq(if alpha > 0.0 {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
            let (Some(handle), Some(materials)) = (material, materials.as_mut()) else {
                continue;
            };
            let Some(mut m) = materials.get_mut(&handle.0) else {
                continue;
            };
            let (mode, translucent, base_alpha) =
                *own.entry(handle.0.id())
                    .or_insert((m.alpha_mode, m.params.translucent, m.params.base_color.w));
            if alpha >= 1.0 {
                m.alpha_mode = mode;
                m.params.translucent = translucent;
                m.params.base_color.w = base_alpha;
            } else if alpha > 0.0 {
                if mode == AlphaMode::Opaque {
                    m.alpha_mode = AlphaMode::Blend;
                    m.params.translucent = 1.0;
                }
                m.params.base_color.w = base_alpha * alpha;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::MapMesh;

    /// Two rooms split by the plane x = 0: 0 <= x < 10 is cluster 0, x < 0
    /// cluster 1, x >= 10 is outside (solid). 0 sees 1, 1 sees only itself.
    fn two_rooms() -> MapVisibility {
        MapVisibility {
            planes: vec![(Vec3::X, 0.0), (Vec3::X, 10.0)],
            nodes: vec![(0, [1, -2]), (1, [-3, -1])],
            leaf_clusters: vec![0, 1, -1],
            cluster_count: 2,
            visible: vec![vec![0b11], vec![0b10]],
            audible: Vec::new(),
            areas: MapAreas::default(),
            occluders: Vec::new(),
        }
    }

    #[test]
    fn finds_clusters_and_sets() {
        let v = two_rooms();
        assert_eq!(v.cluster_at(Vec3::new(1.0, 0.0, 0.0)), Some(0));
        assert_eq!(v.cluster_at(Vec3::new(-1.0, 0.0, 0.0)), Some(1));
        assert_eq!(v.cluster_at(Vec3::new(20.0, 0.0, 0.0)), None);
        assert!(v.sees(0, 1) && !v.sees(1, 0));
        assert_eq!(box_clusters(&v, Vec3::splat(-1.0), Vec3::splat(1.0)), vec![0, 1]);
        assert_eq!(box_clusters(&v, Vec3::splat(0.5), Vec3::splat(1.0)), vec![0]);
    }

    #[test]
    fn segments_stop_at_solid_leaves() {
        let v = two_rooms();
        let at = |x: f32| Vec3::new(x, 0.0, 0.0);
        assert!(v.segment_clear(at(1.0), at(-5.0)));
        assert!(v.segment_clear(at(-5.0), at(9.0)));
        assert!(!v.segment_clear(at(1.0), at(12.0)));
        assert!(!v.segment_clear(at(12.0), at(15.0)), "starts in solid");
        assert!(MapVisibility::default().segment_clear(at(0.0), at(100.0)));
    }

    #[test]
    fn decompresses_run_length_rows() {
        // 20 clusters: 0x05 (clusters 0, 2), then one zero byte skipped,
        // then 0x08 (cluster 19).
        let row = decompress_row(&[0x05, 0x00, 0x01, 0x08], 20);
        let set: Vec<usize> = (0..20).filter(|&c| row[0] >> c & 1 != 0).collect();
        assert_eq!(set, vec![0, 2, 19]);
    }

    #[test]
    fn splits_meshes_by_cell_keeping_every_triangle() {
        let v = two_rooms();
        let mut mesh = MapMesh::default();
        for x in [-3.0f32, -1.0, 1.0, 3.0] {
            let base = mesh.positions.len() as u32;
            mesh.positions
                .extend([[x, 0.0, 0.0], [x + 0.5, 0.0, 0.0], [x, 0.5, 0.0]]);
            mesh.uvs.extend([[0.0, 0.0]; 3]);
            mesh.indices.extend([base, base + 1, base + 2]);
        }
        let chunks = split_mesh(&mesh, Some(&v), 2.0);
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks.iter().map(|(m, _)| m.indices.len()).sum::<usize>(), 12);
        for (m, clusters) in &chunks {
            assert_eq!(m.positions.len(), 3);
            assert_eq!(m.uvs.len(), 3);
            let x = m.positions[0][0];
            assert_eq!(clusters, &vec![if x < 0.0 { 1 } else { 0 }]);
        }
    }

    #[test]
    fn culling_follows_the_camera_and_shows_everything_outside() {
        let mut app = App::new();
        app.init_resource::<NoVis>()
            .init_resource::<VisStats>()
            .insert_resource(ActiveVisibility(std::sync::Arc::new(two_rooms())))
            .add_systems(Update, cull);
        let at = |x: f32| GlobalTransform::from_translation(Vec3::new(x, 0.0, 0.0));
        let camera = app.world_mut().spawn((Camera3d::default(), at(-1.0))).id();
        let in_0 = app
            .world_mut()
            .spawn((VisClusters::new(vec![0]), Visibility::default()))
            .id();
        let in_1 = app
            .world_mut()
            .spawn((VisClusters::new(vec![1]), Visibility::default()))
            .id();
        let shown = |app: &App| [in_0, in_1].map(|e| app.world().get::<Visibility>(e) == Some(&Visibility::Inherited));
        // From cluster 1 only cluster 1 is potentially visible.
        app.update();
        assert_eq!(shown(&app), [false, true]);
        // From cluster 0 both are.
        *app.world_mut().get_mut::<GlobalTransform>(camera).unwrap() = at(1.0);
        app.update();
        assert_eq!(shown(&app), [true, true]);
        // Back in 1, then outside the map: everything is drawn.
        *app.world_mut().get_mut::<GlobalTransform>(camera).unwrap() = at(-1.0);
        app.update();
        assert_eq!(shown(&app), [false, true]);
        *app.world_mut().get_mut::<GlobalTransform>(camera).unwrap() = at(20.0);
        app.update();
        assert_eq!(shown(&app), [true, true]);
        // r_novis 1 draws everything too.
        *app.world_mut().get_mut::<GlobalTransform>(camera).unwrap() = at(-1.0);
        app.update();
        assert_eq!(shown(&app), [false, true]);
        app.world_mut().resource_mut::<NoVis>().0 = 1;
        app.update();
        assert_eq!(shown(&app), [true, true]);
    }

    /// A square opening (side 2) centred at `c`, facing along z.
    fn opening(c: Vec3) -> Vec<Vec3> {
        [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
            .map(|(x, y)| c + Vec3::new(x, y, 0.0))
            .to_vec()
    }

    fn portal(key: u16, a: u16, b: u16, polygon: Vec<Vec3>) -> AreaPortal {
        AreaPortal {
            key,
            areas: [a, b],
            polygon,
            fade: None,
            see_through: false,
        }
    }

    /// Areas 1 - 2 - 3 in a row (portals 10 and 11) and 4 off area 1
    /// (portal 12); one leaf, and one cluster, per area 1..=4.
    fn row_of_areas(polygons: [Vec<Vec3>; 3]) -> MapAreas {
        let [a, b, c] = polygons;
        MapAreas::new(
            vec![1, 2, 3, 4],
            &[0, 1, 2, 3],
            4,
            vec![portal(10, 1, 2, a), portal(11, 2, 3, b), portal(12, 1, 4, c)],
        )
    }

    fn reached(r: &[bool]) -> Vec<usize> {
        (0..r.len()).filter(|&a| r[a]).collect()
    }

    #[test]
    fn flood_follows_open_portals() {
        let areas = row_of_areas([Vec::new(), Vec::new(), Vec::new()]);
        assert_eq!(areas.area_count(), 5);
        let all = |_: &AreaPortal| true;
        assert_eq!(reached(&areas.flood(1, &all, None)), vec![0, 1, 2, 3, 4]);
        // Closing 11 cuts off area 3 only; closing 10 cuts off 2 and 3.
        let shut = |keys: &'static [u16]| move |p: &AreaPortal| !keys.contains(&p.key);
        assert_eq!(reached(&areas.flood(1, &shut(&[11]), None)), vec![0, 1, 2, 4]);
        assert_eq!(reached(&areas.flood(1, &shut(&[10]), None)), vec![0, 1, 4]);
        assert_eq!(reached(&areas.flood(3, &shut(&[10]), None)), vec![0, 2, 3]);
        // Portals work both ways; area 0 (or none) reaches everything.
        assert_eq!(reached(&areas.flood(4, &all, None)), vec![0, 1, 2, 3, 4]);
        assert_eq!(
            reached(&areas.flood(0, &shut(&[10, 11, 12]), None)),
            vec![0, 1, 2, 3, 4]
        );
        assert_eq!(
            reached(&areas.flood(9, &shut(&[10, 11, 12]), None)),
            vec![0, 1, 2, 3, 4]
        );
        // Cluster k + 1 is in area k + 1.
        let mask = areas.cluster_mask(&areas.flood(1, &shut(&[11]), None), 4);
        assert_eq!(mask, vec![0b1011]);
    }

    #[test]
    fn flood_looks_through_portal_openings() {
        // Camera at the origin looking down -z, 90 degree field of view.
        let projection = PerspectiveProjection {
            fov: std::f32::consts::FRAC_PI_2,
            aspect_ratio: 1.0,
            ..default()
        };
        use bevy::camera::CameraProjection;
        let clip = projection.get_clip_from_view() * Mat4::IDENTITY;
        let all = |_: &AreaPortal| true;
        // In front: through 10 then 11 (straight ahead, further on);
        // portal 12 is behind the camera.
        let ahead = row_of_areas([
            opening(Vec3::new(0.0, 0.0, -5.0)),
            opening(Vec3::new(0.0, 0.0, -10.0)),
            opening(Vec3::new(0.0, 0.0, 5.0)),
        ]);
        assert_eq!(reached(&ahead.flood(1, &all, Some(clip))), vec![0, 1, 2, 3]);
        // 11 off to the side, outside the view: area 3 isn't seen.
        let aside = row_of_areas([
            opening(Vec3::new(0.0, 0.0, -5.0)),
            opening(Vec3::new(40.0, 0.0, -10.0)),
            opening(Vec3::new(0.0, 0.0, 5.0)),
        ]);
        assert_eq!(reached(&aside.flood(1, &all, Some(clip))), vec![0, 1, 2]);
        // 11 in view, but not through 10's opening (10 on the left, 11 on
        // the right): narrowed away.
        let narrowed = row_of_areas([
            opening(Vec3::new(-3.0, 0.0, -5.0)),
            opening(Vec3::new(6.0, 0.0, -10.0)),
            opening(Vec3::new(0.0, 0.0, 5.0)),
        ]);
        assert_eq!(reached(&narrowed.flood(1, &all, Some(clip))), vec![0, 1, 2]);
        // Without the projection, all of them.
        assert_eq!(reached(&narrowed.flood(1, &all, None)), vec![0, 1, 2, 3, 4]);
        // A portal straddling the eye's plane covers the whole screen.
        let straddle = row_of_areas([
            vec![
                Vec3::new(-1.0, -1.0, 1.0),
                Vec3::new(1.0, -1.0, -1.0),
                Vec3::new(0.0, 1.0, 0.0),
            ],
            opening(Vec3::new(0.0, 0.0, -10.0)),
            Vec::new(),
        ]);
        assert_eq!(reached(&straddle.flood(1, &all, Some(clip))), vec![0, 1, 2, 3, 4]);
    }

    /// `two_rooms` with areas: cluster 0 in area 1, cluster 1 in area 2,
    /// joined by portal 7 (an opening at x = 0).
    fn two_areas(fade: Option<WindowFade>) -> MapVisibility {
        let mut v = two_rooms();
        // Cluster 1 sees cluster 0 too here.
        v.visible = vec![vec![0b11], vec![0b11]];
        let mut p = portal(
            7,
            1,
            2,
            vec![
                Vec3::new(0.0, -1.0, -1.0),
                Vec3::new(0.0, 1.0, -1.0),
                Vec3::new(0.0, 1.0, 1.0),
            ],
        );
        p.fade = fade;
        v.areas = MapAreas::new(vec![1, 2, 0], &v.leaf_clusters, 2, vec![p]);
        v
    }

    #[test]
    fn closed_portals_cut_the_pvs() {
        let v = two_areas(None);
        let at = Vec3::new(1.0, 0.0, 0.0);
        assert_eq!(v.area_at(at), 1);
        let open = AreaPortalStates::default();
        let closed = AreaPortalStates { closed: vec![7] };
        assert_eq!(
            camera_clusters(&v, at, at, None, &open, false).unwrap(),
            (vec![0b11], 1, 3)
        );
        assert_eq!(
            camera_clusters(&v, at, at, None, &closed, false).unwrap(),
            (vec![0b01], 1, 2)
        );
        // r_portalsopenall: the PVS alone.
        assert_eq!(camera_clusters(&v, at, at, None, &closed, true).unwrap().0, vec![0b11]);
        // A window closes past its fade's end when it has a brush to draw
        // there (by the distance from the main view's eye), even with
        // glass in it; without a brush it stays open.
        let fade = WindowFade {
            start: 1.0,
            end: 2.0,
            limit: 0.0,
            brush: Some(3),
        };
        let mut w = two_areas(Some(fade));
        let far = Vec3::new(5.0, 0.0, 0.0);
        assert_eq!(camera_clusters(&w, far, far, None, &open, false).unwrap().0, vec![0b01]);
        assert_eq!(camera_clusters(&w, far, at, None, &open, false).unwrap().0, vec![0b11]);
        w.areas.portals[0].see_through = true;
        assert_eq!(camera_clusters(&w, far, far, None, &open, false).unwrap().0, vec![0b01]);
        let w = two_areas(Some(WindowFade { brush: None, ..fade }));
        assert_eq!(camera_clusters(&w, far, far, None, &open, false).unwrap().0, vec![0b11]);
        // A door with a window: seen through even when closed.
        let mut glass = two_areas(None);
        glass.areas.portals[0].see_through = true;
        assert_eq!(
            camera_clusters(&glass, at, at, None, &closed, false).unwrap().0,
            vec![0b11]
        );
        // Outside the map: None.
        let outside = Vec3::new(20.0, 0.0, 0.0);
        assert!(camera_clusters(&v, outside, outside, None, &open, false).is_none());
    }

    #[test]
    fn culling_hides_what_a_closed_portal_cuts_off() {
        let mut app = App::new();
        app.init_resource::<NoVis>()
            .init_resource::<VisStats>()
            .insert_resource(ActiveVisibility(std::sync::Arc::new(two_areas(None))))
            .add_systems(Update, cull);
        app.world_mut().spawn((
            Camera3d::default(),
            GlobalTransform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
        ));
        let far = app
            .world_mut()
            .spawn((VisClusters::new(vec![1]), Visibility::default()))
            .id();
        let shown = |app: &App| app.world().get::<Visibility>(far) == Some(&Visibility::Inherited);
        app.update();
        assert!(shown(&app), "no portal states: open");
        app.insert_resource(AreaPortalStates { closed: vec![7] });
        app.update();
        assert!(!shown(&app), "closed");
        assert_eq!(app.world().resource::<VisStats>().closed_portals, 1);
        app.insert_resource(PortalsOpenAll(1));
        app.update();
        assert!(shown(&app), "r_portalsopenall 1");
        app.insert_resource(PortalsOpenAll(0));
        app.update();
        assert!(!shown(&app));
        app.insert_resource(AreaPortalStates::default());
        app.update();
        assert!(shown(&app), "opened again");
    }

    #[test]
    fn glass_in_an_opening_keeps_its_portal_open() {
        let mut areas = row_of_areas([
            opening(Vec3::new(0.0, 0.0, -5.0)),
            opening(Vec3::new(0.0, 0.0, -10.0)),
            opening(Vec3::new(0.0, 0.0, 5.0)),
        ]);
        let pane = |z: f32, alpha: crate::map::MapAlpha| MapMesh {
            positions: vec![[-0.5, -0.5, z], [0.5, -0.5, z], [0.0, 0.5, z]],
            indices: vec![0, 1, 2],
            alpha,
            ..default()
        };
        // Glass just behind portal 11's opening; an opaque pane in 10's.
        areas.mark_see_through(&[
            pane(-10.1, crate::map::MapAlpha::Blend),
            pane(-5.0, crate::map::MapAlpha::Opaque),
        ]);
        let marked: Vec<u16> = areas.portals.iter().filter(|p| p.see_through).map(|p| p.key).collect();
        assert_eq!(marked, vec![11]);
    }

    #[test]
    fn fade_bands_need_a_near_distance_below_the_far_one() {
        let band = fade_band(10.0, 20.0).unwrap();
        assert_eq!(band.end_margin, 10.0..20.0);
        assert!(fade_band(-1.0, 20.0).is_none());
        assert!(fade_band(20.0, 20.0).is_none());
        assert!(fade_band(0.0, 20.0).is_some());
    }

    #[test]
    fn window_fades_rise_from_the_limit_to_opaque() {
        let f = WindowFade {
            start: 10.0,
            end: 20.0,
            limit: 0.2,
            brush: Some(0),
        };
        assert_eq!(f.alpha(0.0), 0.2);
        assert_eq!(f.alpha(10.0), 0.2);
        assert!((f.alpha(15.0) - 0.6).abs() < 1e-6);
        assert_eq!(f.alpha(25.0), 1.0);
        assert!(!f.closes(20.0) && f.closes(20.5));
        // Start and end equal (most stock windows): a switch at the end.
        let sharp = WindowFade {
            start: 20.0,
            limit: 0.0,
            ..f
        };
        assert_eq!(sharp.alpha(19.9), 0.0);
        assert_eq!(sharp.alpha(20.0), 1.0);
        // The distance is to the opening's bounds.
        let p = portal(1, 1, 2, opening(Vec3::new(0.0, 0.0, -5.0)));
        assert_eq!(p.distance(Vec3::new(0.0, 0.0, -5.0)), 0.0);
        assert_eq!(p.distance(Vec3::new(4.0, 0.0, -5.0)), 3.0);
        assert_eq!(p.distance(Vec3::new(0.0, 0.0, 5.0)), 10.0);
    }

    /// A camera at `eye` looking down -z, 90 degree field of view, square.
    fn looking_down_z(eye: Vec3) -> Mat4 {
        use bevy::camera::CameraProjection;
        let projection = PerspectiveProjection {
            fov: std::f32::consts::FRAC_PI_2,
            aspect_ratio: 1.0,
            ..default()
        };
        projection.get_clip_from_view() * Mat4::from_translation(-eye)
    }

    /// A square of side `2 * half` centred at `c`, facing along z.
    fn square(c: Vec3, half: f32) -> Vec<Vec3> {
        opening(Vec3::ZERO).into_iter().map(|p| c + p * half).collect()
    }

    #[test]
    fn occluders_hide_boxes_wholly_behind_them() {
        let clip = looking_down_z(Vec3::ZERO);
        let wall = square(Vec3::new(0.0, 0.0, -5.0), 2.0);
        let occ = screen_occluders([wall.as_slice()], Vec3::ZERO, clip);
        assert_eq!(occ.len(), 1);
        let hidden = |lo: [f32; 3], hi: [f32; 3]| occluded(&occ, Vec3::from(lo), Vec3::from(hi), clip);
        // Behind it and inside its outline.
        assert!(hidden([-1.0, -1.0, -11.0], [1.0, 1.0, -10.0]));
        // Straddling its plane, or in front of it.
        assert!(!hidden([-0.5, -0.5, -6.0], [0.5, 0.5, -4.0]));
        assert!(!hidden([-0.5, -0.5, -4.0], [0.5, 0.5, -3.0]));
        // Behind it but reaching past its edge (x 3..5 at 10 m is wider
        // than its 2 m half width at 5 m), or larger than it.
        assert!(!hidden([3.0, -1.0, -11.0], [5.0, 1.0, -10.0]));
        assert!(!hidden([-5.0, -5.0, -11.0], [5.0, 5.0, -10.0]));
        // Behind the eye.
        assert!(!hidden([-1.0, -1.0, 10.0], [1.0, 1.0, 11.0]));
        // Seen from either side (the winding doesn't matter).
        let reversed: Vec<Vec3> = wall.iter().rev().copied().collect();
        let occ_rev = screen_occluders([reversed.as_slice()], Vec3::ZERO, clip);
        assert!(occluded(
            &occ_rev,
            Vec3::new(-1.0, -1.0, -11.0),
            Vec3::new(1.0, 1.0, -10.0),
            clip
        ));
        // Unusable: crossing the eye's plane, edge on, off screen.
        let crossing = vec![
            Vec3::new(-1.0, -1.0, 1.0),
            Vec3::new(1.0, -1.0, -1.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        assert!(screen_occluders([crossing.as_slice()], Vec3::ZERO, clip).is_empty());
        let edge_on = vec![
            Vec3::new(0.0, -1.0, -4.0),
            Vec3::new(0.0, -1.0, -6.0),
            Vec3::new(0.0, 1.0, -5.0),
        ];
        assert!(screen_occluders([edge_on.as_slice()], Vec3::ZERO, clip).is_empty());
        let aside = square(Vec3::new(40.0, 0.0, -5.0), 2.0);
        assert!(screen_occluders([aside.as_slice()], Vec3::ZERO, clip).is_empty());
        // No occluders: nothing hidden.
        assert!(!occluded(
            &[],
            Vec3::new(-1.0, -1.0, -11.0),
            Vec3::new(1.0, 1.0, -10.0),
            clip
        ));
    }

    #[test]
    fn culling_hides_parts_behind_active_occluders() {
        let mut v = two_rooms();
        v.occluders = vec![Occluder {
            key: 4,
            polygons: vec![square(Vec3::new(1.0, 0.0, -5.0), 2.0)],
            start_active: true,
        }];
        let mut app = App::new();
        app.init_resource::<NoVis>()
            .init_resource::<VisStats>()
            .insert_resource(ActiveVisibility(std::sync::Arc::new(v)))
            .add_systems(Update, cull);
        let projection = Projection::Perspective(PerspectiveProjection {
            fov: std::f32::consts::FRAC_PI_2,
            aspect_ratio: 1.0,
            ..default()
        });
        app.world_mut().spawn((
            Camera3d::default(),
            projection,
            GlobalTransform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
        ));
        let part = |app: &mut App, z: f32| {
            app.world_mut()
                .spawn((
                    VisClusters::new(vec![0]),
                    Occludee {
                        min: Vec3::new(0.5, -0.5, z - 1.0),
                        max: Vec3::new(1.5, 0.5, z),
                    },
                    Visibility::default(),
                ))
                .id()
        };
        let behind = part(&mut app, -10.0);
        let before = part(&mut app, -3.0);
        let shown =
            |app: &App| [behind, before].map(|e| app.world().get::<Visibility>(e) == Some(&Visibility::Inherited));
        app.update();
        assert_eq!(shown(&app), [false, true], "active from the start");
        assert_eq!(app.world().resource::<VisStats>().occluded_parts, 1);
        app.insert_resource(OccluderStates { inactive: vec![4] });
        app.update();
        assert_eq!(shown(&app), [true, true], "deactivated");
        app.insert_resource(OccluderStates { inactive: vec![] });
        app.update();
        assert_eq!(shown(&app), [false, true], "activated again");
        app.insert_resource(Occlusion(0));
        app.update();
        assert_eq!(shown(&app), [true, true], "r_occlusion 0");
    }
}
