//! Colliders and brushes built from map models: convex hulls as brush
//! planes, smoothed terrain for loose items, a model's collider.

#[allow(unused_imports)]
use super::*;

/// Share of a prop's surface that must lie on its convex hull for the hull
/// to stand in for it in exact (brush) collision. Crates and boxes with
/// shallow panel insets pass; arches and other concave props don't.
pub(super) const CONVEX_SURFACE_SHARE: f32 = 0.75;
pub(super) const MAX_HULL_PLANES: usize = 48;
/// How close (meters) a triangle must be to a hull face to count as on it.
pub(super) const ON_HULL: f32 = 0.01;

/// A model's convex hull as planes (model space), if at least `share` of
/// its surface area lies on the hull.
pub(super) fn convex_planes(model: &MapModel, share: f32) -> Option<Vec<(Vec3, f32)>> {
    let points: Vec<Vec3> = model
        .meshes
        .iter()
        .flat_map(|m| m.positions.iter().map(|p| Vec3::from(*p)))
        .collect();
    if points.len() < 4 {
        return None;
    }
    let (verts, tris) = avian3d::parry::transformation::convex_hull(&points);
    let mut planes: Vec<(Vec3, f32)> = Vec::new();
    for t in &tris {
        let [a, b, c] = t.map(|i| verts[i as usize]);
        let n = (b - a).cross(c - a);
        if n.length_squared() < 1e-12 {
            continue;
        }
        let n = n.normalize();
        let d = n.dot(a);
        if !planes.iter().any(|(m, e)| m.dot(n) > 0.9999 && (e - d).abs() < 1e-4) {
            planes.push((n, d));
        }
    }
    // Rounded props (many faces) stay meshes: little to gain, slow to place.
    if !(4..=MAX_HULL_PLANES).contains(&planes.len()) {
        return None;
    }
    let (mut on, mut total) = (0.0f32, 0.0f32);
    for m in &model.meshes {
        for t in m.indices.as_chunks::<3>().0 {
            let [a, b, c] = t.map(|i| Vec3::from(m.positions[i as usize]));
            let area = (b - a).cross(c - a).length() / 2.0;
            total += area;
            if planes
                .iter()
                .any(|(n, d)| [a, b, c].iter().all(|p| (n.dot(*p) - d).abs() < ON_HULL))
            {
                on += area;
            }
        }
    }
    (total > 0.0 && on >= share * total).then_some(planes)
}

/// Model-space planes placed in the world, with bevel planes added (so box
/// sweeps stop at edges and corners like Source's brushes:
/// `MapBrush::from_planes`).
pub(super) fn place_brush(
    planes: &[(Vec3, f32)],
    translation: Vec3,
    rotation: Quat,
    surface: Option<String>,
) -> MapBrush {
    let world: Vec<(Vec3, f32)> = planes
        .iter()
        .map(|(n, d)| {
            let n2 = rotation * *n;
            (n2, d + n2.dot(translation))
        })
        .collect();
    MapBrush {
        surface,
        ..MapBrush::from_planes(world)
    }
}

/// The collision triangles (displacements) as loose items see them: each
/// triangle comes with its own corners, so they are merged and the mesh's
/// internal edges fixed. On the plain mesh a small box sliding over it (a
/// shot weapon) catches on every edge between two triangles as on a step
/// and stops dead. Only loose items use it: physics props resting on it
/// can sink through (a de_dust2 oil drum did), so they keep the plain one.
pub(super) fn smooth_terrain(positions: Vec<Vec3>, indices: &[[u32; 3]]) -> Option<Collider> {
    let flags = TrimeshFlags::FIX_INTERNAL_EDGES
        | TrimeshFlags::DELETE_DEGENERATE_TRIANGLES
        | TrimeshFlags::DELETE_DUPLICATE_TRIANGLES;
    Collider::try_trimesh_with_config(positions, indices.to_vec(), flags)
        .inspect_err(|e| warn!("no smooth terrain for loose items: {e:?}"))
        .ok()
}

/// A model's collider in model space: its collision model's convex pieces,
/// else all its triangles.
pub(super) fn model_collider(model: &MapModel) -> Option<Collider> {
    if let Some(c) = &model.collision {
        let parts: Vec<(Vec3, Quat, Collider)> = c
            .pieces
            .iter()
            .filter_map(|p| Collider::convex_hull(p.points.clone()))
            .map(|hull| (Vec3::ZERO, Quat::IDENTITY, hull))
            .collect();
        if !parts.is_empty() {
            return Some(Collider::compound(parts));
        }
    }
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for m in &model.meshes {
        let base = positions.len() as u32;
        positions.extend(m.positions.iter().map(|p| Vec3::from(*p)));
        indices.extend(
            m.indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|t| [base + t[0], base + t[1], base + t[2]]),
        );
    }
    (!indices.is_empty()).then(|| Collider::trimesh(positions, indices))
}
