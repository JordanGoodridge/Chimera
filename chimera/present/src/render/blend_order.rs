//! Blended meshes drawn in the same order whatever was drawn before.
//!
//! Bevy sorts each view's blended (`Transparent3d`) items by their mesh
//! centre's depth along the view direction, with a stable sort over a list
//! it keeps from frame to frame. Two items at the same depth (de_nuke's
//! door windows: two glass faces 5 cm apart, seen edge-on, have the same
//! view depth) then keep whatever order earlier frames left, which depends
//! on the views before and on what was hidden or shown since: the same
//! view drew its glass differently after another view (`refcmp ordercheck`,
//! docs/performance.md "Checks"). Map overlays and infodecals drew in the
//! alpha-mask phase, whose order between pipelines follows the order the
//! pipelines were made in (what the camera saw first), so overlapping
//! decals of different kinds stacked differently from run to run.
//!
//! `settle_ties` runs after Bevy's sort: map decals first, in the map's
//! order (`DecalOrder`, set at spawn: overlays and infodecals as the map
//! lists them; their render order already lifts them apart), then the rest
//! in Bevy's order with items at the same depth ordered by their mesh
//! centre's distance from the eye (farther first, as back-to-front drawing
//! wants), then by main-world entity, so a view's order depends only on
//! that view.

use std::{cmp::Ordering, collections::HashMap};

use bevy::{
    core_pipeline::core_3d::{Transparent3d, TransparentSortingInfo3d},
    math::FloatOrd,
    prelude::*,
    render::{
        Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
        render_phase::{ViewSortedRenderPhases, sort_phase_system},
        sync_world::MainEntity,
        view::ExtractedView,
    },
};

/// A map decal's (overlay's or infodecal's) place in the map's drawing
/// order: lower draws first, so later ones lie on top.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DecalOrder(pub u32);

/// The render world's copy of every drawn decal's `DecalOrder`.
#[derive(Resource, Default)]
struct DecalOrders(HashMap<MainEntity, u32>);

/// Adds `settle_ties` to the render world (in `finish`: the map plugin may
/// be added before the renderer).
pub struct BlendOrderPlugin;

impl Plugin for BlendOrderPlugin {
    fn build(&self, _app: &mut App) {}

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<DecalOrders>()
            .add_systems(ExtractSchedule, extract_decal_orders)
            .add_systems(
                Render,
                settle_ties
                    .in_set(RenderSystems::PhaseSort)
                    .after(sort_phase_system::<Transparent3d>),
            );
    }
}

fn extract_decal_orders(decals: Extract<Query<(Entity, &DecalOrder)>>, mut orders: ResMut<DecalOrders>) {
    orders.0.clear();
    orders.0.extend(decals.iter().map(|(e, o)| (MainEntity::from(e), o.0)));
}

/// Sorts each view's blended items again: decals first in their order,
/// ties in Bevy's sort settled.
fn settle_ties(
    views: Query<&ExtractedView>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    decals: Res<DecalOrders>,
) {
    for view in &views {
        let Some(phase) = phases.get_mut(&view.retained_view_entity) else {
            continue;
        };
        let items = &phase.items;
        let decal = |i: &Transparent3d| decals.0.get(&i.entity.1).copied();
        let unsettled = items.values().zip(items.values().skip(1)).any(|(a, b)| {
            a.distance == b.distance || decal(a).is_some() || decal(b).is_some()
        });
        if !unsettled {
            continue;
        }
        let eye = view.world_from_view.translation();
        phase.items.sort_by(|_, a, _, b| {
            order(
                Key {
                    decal: decal(a),
                    depth: a.distance,
                    centre: centre(&a.sorting_info),
                    entity: a.entity.1.id(),
                },
                Key {
                    decal: decal(b),
                    depth: b.distance,
                    centre: centre(&b.sorting_info),
                    entity: b.entity.1.id(),
                },
                eye,
            )
        });
    }
}

fn centre(info: &TransparentSortingInfo3d) -> Option<Vec3> {
    match info {
        TransparentSortingInfo3d::Sorted { mesh_center, .. } => Some(*mesh_center),
        TransparentSortingInfo3d::AlwaysOnTop => None,
    }
}

/// What orders a blended item.
#[derive(Clone, Copy)]
struct Key {
    /// A map decal's `DecalOrder`.
    decal: Option<u32>,
    /// View depth as Bevy computes it (larger is nearer).
    depth: f32,
    centre: Option<Vec3>,
    /// Main-world entity.
    entity: Entity,
}

/// Draw order of two blended items: map decals first, in their order;
/// then Bevy's (ascending depth), farther from the eye first, by entity.
fn order(a: Key, b: Key, eye: Vec3) -> Ordering {
    let far = |c: Option<Vec3>| FloatOrd(-c.map_or(0.0, |c| c.distance_squared(eye)));
    match (a.decal, b.decal) {
        (Some(x), Some(y)) => return x.cmp(&y).then_with(|| a.entity.cmp(&b.entity)),
        (Some(_), None) => return Ordering::Less,
        (None, Some(_)) => return Ordering::Greater,
        (None, None) => {}
    }
    FloatOrd(a.depth)
        .cmp(&FloatOrd(b.depth))
        .then_with(|| far(a.centre).cmp(&far(b.centre)))
        .then_with(|| a.entity.cmp(&b.entity))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(depth: f32, centre: Vec3, entity: u32, decal: Option<u32>) -> Key {
        Key {
            decal,
            depth,
            centre: Some(centre),
            entity: Entity::from_raw_u32(entity).unwrap(),
        }
    }

    #[test]
    fn ties_go_farther_first_then_by_entity() {
        let eye = Vec3::ZERO;
        // Depth decides first.
        let near = key(-1.0, Vec3::new(0.0, 0.0, -1.0), 1, None);
        let far = key(-5.0, Vec3::new(0.0, 0.0, -5.0), 2, None);
        assert_eq!(order(far, near, eye), Ordering::Less);
        // Same depth, seen edge-on: the pane farther from the eye first,
        // whichever way they are compared.
        let pane_a = key(-3.0, Vec3::new(3.5, 0.0, -3.0), 7, None);
        let pane_b = key(-3.0, Vec3::new(3.55, 0.0, -3.0), 3, None);
        assert_eq!(order(pane_b, pane_a, eye), Ordering::Less);
        assert_eq!(order(pane_a, pane_b, eye), Ordering::Greater);
        // Same depth and distance: by entity, never equal.
        let twin = key(-3.0, Vec3::new(3.5, 0.0, -3.0), 2, None);
        assert_ne!(order(twin, pane_a, eye), Ordering::Equal);
        assert_eq!(order(twin, pane_a, eye), order(pane_a, twin, eye).reverse());
    }

    #[test]
    fn decals_first_in_map_order() {
        let eye = Vec3::ZERO;
        // A decal near the eye still draws before a far window.
        let decal = key(-1.0, Vec3::new(0.0, 0.0, -1.0), 5, Some(3));
        let window = key(-9.0, Vec3::new(0.0, 0.0, -9.0), 6, None);
        assert_eq!(order(decal, window, eye), Ordering::Less);
        assert_eq!(order(window, decal, eye), Ordering::Greater);
        // Between decals the map's order decides, not depth.
        let later = key(-9.0, Vec3::new(0.0, 0.0, -9.0), 4, Some(7));
        assert_eq!(order(decal, later, eye), Ordering::Less);
        assert_eq!(order(later, decal, eye), Ordering::Greater);
    }
}
