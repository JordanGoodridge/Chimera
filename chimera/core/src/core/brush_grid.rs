use bevy::prelude::*;

/// Physics layer of ragdoll bodies. They collide only with the default
/// layer (the world, props), not with characters or each other, and
/// gameplay queries (movement, bullets, sight) leave them out with
/// `SOLID_LAYERS` (specs/cs_source/ragdolls.md 4: ragdolls are debris).
pub const RAGDOLL_LAYER: avian3d::prelude::LayerMask = avian3d::prelude::LayerMask(1 << 1);
/// Every layer but ragdolls, physics shadows (and the loose items' copy
/// of the terrain): the mask for gameplay spatial queries.
pub const SOLID_LAYERS: avian3d::prelude::LayerMask = avian3d::prelude::LayerMask(
    !(1 << 1) & !(1 << 3) & !SHADOW_LAYER.0 & !PHYSICS_CLIP_LAYER.0 & !PLAYER_CLIP_LAYER.0,
);
/// Physics layer of map surfaces only characters' movement meets
/// (`MapCollisionSet` with `characters` alone: Combat Arms' collision
/// blockers). Spatial queries with `SOLID_LAYERS` (shots, sight, cover)
/// pass through; `CHARACTER_FILTER` meets them.
pub const PLAYER_CLIP_LAYER: avian3d::prelude::LayerMask = avian3d::prelude::LayerMask(1 << 6);
/// Physics layer of map surfaces only traces meet (`MapCollisionSet` with
/// `traces` alone: Combat Arms' non-solid detail and ray-hit-only world
/// models): in `SOLID_LAYERS`, not in `CHARACTER_FILTER`.
pub const TRACE_ONLY_LAYER: avian3d::prelude::LayerMask = avian3d::prelude::LayerMask(1 << 7);
/// Physics layer of brushes solid to physics objects only (Source's
/// func_clip_vphysics, `map::physics_clip`): props and physics brushes
/// collide with them; characters, ragdolls, loose items and every
/// spatial query (`SOLID_LAYERS`, `NOT_SHADOW`) pass through.
pub const PHYSICS_CLIP_LAYER: avian3d::prelude::LayerMask = avian3d::prelude::LayerMask(1 << 5);
/// Physics layer of players' physics shadows (specs/cs_source/physics_props.md
/// 4.1, `map::prop_physics::PhysicsShadow`): invisible bodies that only
/// touch the physics props players push. No spatial query should see
/// them: use `SOLID_LAYERS`, or `NOT_SHADOW` for queries of everything.
pub const SHADOW_LAYER: avian3d::prelude::LayerMask = avian3d::prelude::LayerMask(1 << 4);
/// Every layer but physics shadows (and physics clips, which only
/// physics objects meet).
pub const NOT_SHADOW: avian3d::prelude::LayerMask = avian3d::prelude::LayerMask(!SHADOW_LAYER.0 & !PHYSICS_CLIP_LAYER.0);
/// Physics layer of loose items (dropped weapons): they collide with the
/// world, props and each other but never with characters (Source's
/// weapon collision group), so a weapon thrown from inside its dropper
/// isn't shoved out of them. Shots and other gameplay queries still see
/// them.
pub const ITEM_LAYER: avian3d::prelude::LayerMask = avian3d::prelude::LayerMask(1 << 2);
/// What a character's body collides with: solid things but loose items
/// and traces-only surfaces, and players-only ones.
pub const CHARACTER_FILTER: avian3d::prelude::LayerMask =
    avian3d::prelude::LayerMask((SOLID_LAYERS.0 | PLAYER_CLIP_LAYER.0) & !ITEM_LAYER.0 & !TRACE_ONLY_LAYER.0);
/// The map's displacement surfaces as loose items collide with them (a
/// copy with its internal edges fixed, `map::spawn_map`); nothing else
/// touches or queries it.
pub const ITEM_GROUND_LAYER: avian3d::prelude::LayerMask = avian3d::prelude::LayerMask(1 << 3);

// Collision world shared by maps and movement: exact brushes for swept-box
// movement, and water volumes.

/// A solid convex volume as planes: a point p is inside when n.p <= d for
/// every plane (n, d). Engine space, meters; `min`/`max` bound it. Games
/// whose movement sweeps boxes against brushes (Source) use these instead
/// of the physics engine's shape casts, which lose precision on large
/// volumes.
#[derive(Clone, Debug)]
pub struct MapBrush {
    pub planes: Vec<(Vec3, f32)>,
    pub min: Vec3,
    pub max: Vec3,
    /// Climbable (Source: ladder contents).
    pub ladder: bool,
    /// Surface property name (footsteps), lower-case, for brushes that
    /// carry their own (props). None: look it up from the world's faces.
    pub surface: Option<String>,
}

impl MapBrush {
    /// An axis-aligned box.
    pub fn from_box(min: Vec3, max: Vec3) -> Self {
        let planes = vec![
            (Vec3::X, max.x),
            (Vec3::NEG_X, -min.x),
            (Vec3::Y, max.y),
            (Vec3::NEG_Y, -min.y),
            (Vec3::Z, max.z),
            (Vec3::NEG_Z, -min.z),
        ];
        Self {
            planes,
            min,
            max,
            ladder: false,
            surface: None,
        }
    }

    /// Whether a point (engine space) is inside.
    pub fn contains(&self, p: Vec3) -> bool {
        p.cmpge(self.min).all() && p.cmple(self.max).all() && self.planes.iter().all(|(n, d)| n.dot(p) <= *d)
    }

    /// Whether an axis-aligned box (centre, half size) overlaps the brush
    /// by more than `skin` (any space, as long as the brush uses it too).
    pub fn overlaps_box(&self, centre: Vec3, half: Vec3, skin: f32) -> bool {
        self.max.cmpgt(centre - half).all()
            && self.min.cmplt(centre + half).all()
            && self
                .planes
                .iter()
                .all(|(n, d)| n.dot(centre) - (d + n.abs().dot(half)) < -skin)
    }

    /// Sweep an axis-aligned box (half size `half`) with its centre from
    /// `from` to `to`; hits stop `eps` short along the plane. Returns the
    /// fraction travelled and the hit plane's normal when the box hits
    /// (fraction 0 with a zero normal when it starts inside and stays in).
    pub fn sweep_box(&self, half: Vec3, from: Vec3, to: Vec3, eps: f32) -> Option<(f32, Vec3)> {
        let lo = from.min(to) - half - Vec3::splat(eps);
        let hi = from.max(to) + half + Vec3::splat(eps);
        if self.max.cmplt(lo).any() || self.min.cmpgt(hi).any() {
            return None;
        }
        let (mut enter, mut leave) = (-1.0f32, 1.0f32);
        let (mut starts_out, mut gets_out) = (false, false);
        let mut clip = Vec3::ZERO;
        for (n, d) in &self.planes {
            let dist = d + n.abs().dot(half);
            let d1 = n.dot(from) - dist;
            let d2 = n.dot(to) - dist;
            gets_out |= d2 > 0.0;
            starts_out |= d1 > 0.0;
            if d1 > 0.0 && (d2 >= eps || d2 >= d1) {
                return None;
            }
            if d1 <= 0.0 && d2 <= 0.0 {
                continue;
            }
            if d1 > d2 {
                let f = ((d1 - eps) / (d1 - d2)).max(0.0);
                if f > enter {
                    enter = f;
                    clip = *n;
                }
            } else {
                leave = leave.min(((d1 + eps) / (d1 - d2)).min(1.0));
            }
        }
        if !starts_out {
            return (!gets_out).then_some((0.0, Vec3::ZERO));
        }
        (enter < leave && enter > -1.0).then_some((enter.max(0.0), clip))
    }

    /// The brush moved: rotated by `rotation` about the origin, then
    /// moved by `offset` (planes and bounds; `points` are its corners
    /// before the move, for the new bounds).
    pub fn transformed(&self, points: &[Vec3], rotation: Quat, offset: Vec3) -> Self {
        let planes = self
            .planes
            .iter()
            .map(|(n, d)| {
                let n = rotation * *n;
                (n, d + n.dot(offset))
            })
            .collect();
        let (mut min, mut max) = (Vec3::MAX, Vec3::MIN);
        for p in points {
            let q = rotation * *p + offset;
            min = min.min(q);
            max = max.max(q);
        }
        if points.is_empty() {
            (min, max) = (self.min + offset, self.max + offset);
        }
        Self {
            planes,
            min,
            max,
            ladder: self.ladder,
            surface: self.surface.clone(),
        }
    }

    /// A convex solid from its face planes (`n . p <= d`, normals
    /// normalized), with the bevel planes a box sweep needs to be exact:
    /// the bounding box's planes and, through each edge, the planes along
    /// the edge and each axis that touch the solid only there (as vbsp
    /// adds to brushes). Without them a box swept by pushing the planes
    /// out stops short of a non-axial edge, in the air beside it: on a
    /// surf ramp built of pieces, a ghost wall at each seam.
    pub fn from_planes(mut planes: Vec<(Vec3, f32)>) -> Self {
        let n = planes.len();
        // Corners: where three planes meet inside all the others.
        let mut corners: Vec<Vec3> = Vec::new();
        for i in 0..n {
            for j in i + 1..n {
                for k in j + 1..n {
                    let ((n1, d1), (n2, d2), (n3, d3)) = (planes[i], planes[j], planes[k]);
                    let denom = n1.dot(n2.cross(n3));
                    if denom.abs() < 1e-6 {
                        continue;
                    }
                    let p = (n2.cross(n3) * d1 + n3.cross(n1) * d2 + n1.cross(n2) * d3) / denom;
                    if planes.iter().all(|(m, d)| m.dot(p) <= d + 1e-3) {
                        corners.push(p);
                    }
                }
            }
        }
        let on = |p: Vec3, (m, d): (Vec3, f32)| (m.dot(p) - d).abs() < 1e-3;
        let min = corners.iter().fold(Vec3::MAX, |a, c| a.min(*c));
        let max = corners.iter().fold(Vec3::MIN, |a, c| a.max(*c));
        let add = |planes: &mut Vec<(Vec3, f32)>, m: Vec3, d: f32| {
            if !planes.iter().any(|(q, e)| q.dot(m) > 0.9999 && (e - d).abs() < 1e-4) {
                planes.push((m, d));
            }
        };
        for (m, d) in Self::from_box(min, max).planes {
            add(&mut planes, m, d);
        }
        // Edges: two face planes that share at least two corners.
        for i in 0..n {
            for j in i + 1..n {
                let ends: Vec<Vec3> = corners
                    .iter()
                    .copied()
                    .filter(|p| on(*p, planes[i]) && on(*p, planes[j]))
                    .collect();
                let (Some(a), Some(b)) = (ends.first(), ends.iter().find(|p| p.distance(ends[0]) > 1e-4)) else {
                    continue;
                };
                let e = *b - *a;
                for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                    let Some(m) = e.cross(axis).try_normalize() else { continue };
                    for m in [m, -m] {
                        let d = m.dot(*a);
                        // A bevel touches the solid along the edge only.
                        if corners.iter().all(|c| m.dot(*c) <= d + 1e-3) {
                            add(&mut planes, m, d);
                        }
                    }
                }
            }
        }
        Self {
            planes,
            min,
            max,
            ladder: false,
            surface: None,
        }
    }

    /// A triangle as a thin solid: the triangle's face (normal `(b - a) x
    /// (c - a)`), a back face `thickness` behind it, its three sides, and
    /// the bevel planes a box sweep needs to be exact (the box's axes and
    /// each edge crossed with each axis), all as supporting planes.
    pub fn from_triangle(a: Vec3, b: Vec3, c: Vec3, thickness: f32) -> Option<Self> {
        let n = (b - a).cross(c - a).try_normalize()?;
        let back = n * -thickness;
        let corners = [a, b, c, a + back, b + back, c + back];
        let edges = [b - a, c - b, a - c, n];
        let mut axes: Vec<Vec3> = vec![Vec3::X, Vec3::Y, Vec3::Z, n];
        for e in &edges[..3] {
            axes.push(n.cross(*e));
        }
        for e in edges {
            for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                axes.push(e.cross(axis));
            }
        }
        let mut planes: Vec<(Vec3, f32)> = Vec::with_capacity(axes.len() * 2);
        for axis in axes {
            let Some(m) = axis.try_normalize() else { continue };
            for m in [m, -m] {
                let d = corners.iter().map(|p| m.dot(*p)).fold(f32::MIN, f32::max);
                // Only true repeats: a bevel a hair off another plane (an
                // edge across a gentle crease, tilted 0.07 degrees from the
                // axis) is a different plane through a different point. Kept
                // apart, a box rests on the crease where it really is (de_dust2's
                // T spawn ridge: 0.038 units lower, as CS:S) and lands on it
                // with the edge's normal, as CS:S does.
                if planes.iter().any(|(q, e)| q.dot(m) > 1.0 - 1e-6 && (e - d).abs() < 1e-5) {
                    continue;
                }
                planes.push((m, d));
            }
        }
        // The face first: sweeps that tie report the earliest plane.
        let face = planes.iter().position(|(m, _)| m.dot(n) > 1.0 - 1e-6)?;
        planes.swap(0, face);
        let min = corners.iter().fold(Vec3::MAX, |acc, p| acc.min(*p));
        let max = corners.iter().fold(Vec3::MIN, |acc, p| acc.max(*p));
        Some(Self {
            planes,
            min,
            max,
            ladder: false,
            surface: None,
        })
    }
}

/// The loaded map's terrain triangles (Source displacements) as thin
/// solids (`MapBrush::from_triangle`), with a grid to find the ones near a
/// box quickly. Movement that sweeps brushes exactly sweeps these the same
/// way instead of casting against the terrain's physics collider.
#[derive(Resource, Clone, Debug, Default)]
pub struct MapTerrain {
    pub brushes: Vec<MapBrush>,
    /// Grid over engine x and z: origin, cell size and counts.
    origin: Vec2,
    cell: f32,
    size: (usize, usize),
    cells: Vec<Vec<u32>>,
}

impl MapTerrain {
    /// Grid cell size, meters.
    const CELL: f32 = 2.0;

    pub fn from_triangles(triangles: impl IntoIterator<Item = [Vec3; 3]>, thickness: f32) -> Self {
        let brushes: Vec<MapBrush> = triangles
            .into_iter()
            .filter_map(|[a, b, c]| MapBrush::from_triangle(a, b, c, thickness))
            .collect();
        if brushes.is_empty() {
            return Self::default();
        }
        let lo = brushes.iter().fold(Vec2::MAX, |acc, b| acc.min(b.min.xz()));
        let hi = brushes.iter().fold(Vec2::MIN, |acc, b| acc.max(b.max.xz()));
        let size = (
            ((hi.x - lo.x) / Self::CELL).floor() as usize + 1,
            ((hi.y - lo.y) / Self::CELL).floor() as usize + 1,
        );
        let mut terrain = Self {
            brushes,
            origin: lo,
            cell: Self::CELL,
            size,
            cells: vec![Vec::new(); size.0 * size.1],
        };
        for i in 0..terrain.brushes.len() {
            let (x0, z0, x1, z1) = terrain.cell_range(terrain.brushes[i].min, terrain.brushes[i].max);
            for z in z0..=z1 {
                for x in x0..=x1 {
                    terrain.cells[z * size.0 + x].push(i as u32);
                }
            }
        }
        terrain
    }

    fn cell_range(&self, lo: Vec3, hi: Vec3) -> (usize, usize, usize, usize) {
        let to = |v: f32, o: f32, n: usize| (((v - o) / self.cell).floor().max(0.0) as usize).min(n - 1);
        (
            to(lo.x, self.origin.x, self.size.0),
            to(lo.z, self.origin.y, self.size.1),
            to(hi.x, self.origin.x, self.size.0),
            to(hi.z, self.origin.y, self.size.1),
        )
    }

    /// Indices of brushes whose bounds may overlap `lo`..`hi` (sorted,
    /// no repeats), into `out`.
    pub fn near(&self, lo: Vec3, hi: Vec3, out: &mut Vec<u32>) {
        out.clear();
        if self.cells.is_empty()
            || hi.x < self.origin.x
            || hi.z < self.origin.y
            || lo.x > self.origin.x + self.cell * self.size.0 as f32
            || lo.z > self.origin.y + self.cell * self.size.1 as f32
        {
            return;
        }
        let (x0, z0, x1, z1) = self.cell_range(lo, hi);
        for z in z0..=z1 {
            for x in x0..=x1 {
                out.extend(self.cells[z * self.size.0 + x].iter().copied().filter(|&i| {
                    let b = &self.brushes[i as usize];
                    b.max.cmpge(lo).all() && b.min.cmple(hi).all()
                }));
            }
        }
        out.sort_unstable();
        out.dedup();
    }
}

/// Marks the physics collider built from the terrain triangles
/// (`MapTerrain`), so movement that sweeps them exactly can leave it out
/// of physics queries.
#[derive(Component, Debug)]
pub struct MapTerrainCollider;

/// A water (or slime) volume, for swimming.
#[derive(Clone, Debug)]
pub struct MapWaterVolume {
    pub brush: MapBrush,
    pub slime: bool,
}

/// The loaded map's water volumes (`MapData::water`).
#[derive(Resource, Clone, Debug, Default)]
pub struct MapWater(pub Vec<MapWaterVolume>);

/// A game's liquid volume, in the form any game's movement reads: swimming
/// follows it like a `MapWater` volume (a game's own water that isn't the
/// map's data, e.g. Combat Arms' `Water` volumes). The game keeps it in step
/// with its volume (moved, hidden: removed).
#[derive(Component, Clone, Debug)]
pub struct WaterZone(pub MapBrush);

/// A game's ladder volume, in the form any game's movement reads: climbed
/// like a map ladder brush (`MapBrush::ladder`). It is swept as a solid, so
/// a player climbs its faces rather than through it.
#[derive(Component, Clone, Debug)]
pub struct LadderZone(pub MapBrush);

/// The loaded map's brushes (`MapData::collision_brushes`).
#[derive(Resource, Clone, Debug, Default)]
pub struct MapBrushes(pub Vec<MapBrush>);

/// A grid over engine x and z of a list of brushes' bounds (a broad
/// phase): which brushes a box may overlap, without testing them all.
/// Built from `MapBrushes` by whoever sweeps against them (the Source
/// movement keeps one per map: a sweep tested every brush of the map's
/// bounds, ~75 us a player a tick on de_dust2; multiplayer.md
/// "Capacity").
#[derive(Clone, Debug, Default)]
pub struct BrushGrid {
    origin: Vec2,
    cell: f32,
    size: (usize, usize),
    cells: Vec<Vec<u32>>,
    /// Brushes over more than `MAX_CELLS` cells (the sky's walls, big
    /// floors): always candidates.
    big: Vec<u32>,
    /// How many brushes it was built from.
    pub len: usize,
}

impl BrushGrid {
    /// Grid cell size, meters (~157 units).
    const CELL: f32 = 4.0;
    const MAX_CELLS: usize = 64;

    pub fn new(brushes: &[MapBrush]) -> Self {
        if brushes.is_empty() {
            return Self::default();
        }
        let lo = brushes.iter().fold(Vec2::MAX, |acc, b| acc.min(b.min.xz()));
        let hi = brushes.iter().fold(Vec2::MIN, |acc, b| acc.max(b.max.xz()));
        if !lo.is_finite() || !hi.is_finite() {
            return Self {
                big: (0..brushes.len() as u32).collect(),
                len: brushes.len(),
                ..default()
            };
        }
        let size = (
            (((hi.x - lo.x) / Self::CELL).floor() as usize + 1).min(4096),
            (((hi.y - lo.y) / Self::CELL).floor() as usize + 1).min(4096),
        );
        let mut grid = Self {
            origin: lo,
            cell: Self::CELL,
            size,
            cells: vec![Vec::new(); size.0 * size.1],
            big: Vec::new(),
            len: brushes.len(),
        };
        for (i, b) in brushes.iter().enumerate() {
            let (x0, z0, x1, z1) = grid.cell_range(b.min, b.max);
            // Empty or inverted bounds (min above max, NaN): in the always
            // list, where the callers' own bounds tests decide.
            let inverted = x1 < x0 || z1 < z0 || !b.min.is_finite() || !b.max.is_finite();
            if inverted || (x1 - x0 + 1) * (z1 - z0 + 1) > Self::MAX_CELLS {
                grid.big.push(i as u32);
                continue;
            }
            for z in z0..=z1 {
                for x in x0..=x1 {
                    grid.cells[z * size.0 + x].push(i as u32);
                }
            }
        }
        grid
    }

    fn cell_range(&self, lo: Vec3, hi: Vec3) -> (usize, usize, usize, usize) {
        let to = |v: f32, o: f32, n: usize| (((v - o) / self.cell).floor().max(0.0) as usize).min(n - 1);
        (
            to(lo.x, self.origin.x, self.size.0),
            to(lo.z, self.origin.y, self.size.1),
            to(hi.x, self.origin.x, self.size.0),
            to(hi.z, self.origin.y, self.size.1),
        )
    }

    /// Indices of `brushes` (the list it was built from) whose bounds
    /// overlap `lo`..`hi`, bounds included: sorted, no repeats, into
    /// `out`. Every brush the box touches is there, so a sweep testing
    /// these in order gets what testing all of them would.
    pub fn near(&self, brushes: &[MapBrush], lo: Vec3, hi: Vec3, out: &mut Vec<u32>) {
        out.clear();
        let overlaps = |i: &u32| {
            let b = &brushes[*i as usize];
            b.max.cmpge(lo).all() && b.min.cmple(hi).all()
        };
        out.extend(self.big.iter().copied().filter(overlaps));
        // Out of the grid: its edge cells (clamped), which hold whatever
        // lies past them.
        if !self.cells.is_empty() && lo.is_finite() && hi.is_finite() {
            let (x0, z0, x1, z1) = self.cell_range(lo, hi);
            for z in z0..=z1 {
                for x in x0..=x1 {
                    out.extend(self.cells[z * self.size.0 + x].iter().copied().filter(overlaps));
                }
            }
        }
        out.sort_unstable();
        out.dedup();
    }
}

/// The map's BSP tree over the leading `MapBrushes` (Source maps; the
/// world's brushes). A Source trace tests brushes leaf by leaf down this
/// tree, so of two faces hit at the same distance (a ladder flush with a
/// player clip) the one reached first wins. Engine space.
#[derive(Resource, Clone, Debug, Default)]
pub struct MapBrushTree {
    pub nodes: Vec<BrushTreeNode>,
    /// Each leaf's brushes (indices into `MapBrushes`), in stored order.
    pub leaves: Vec<Vec<u32>>,
    /// Root: a node index (>= 0) or leaf -1-index.
    pub root: i32,
}

#[derive(Clone, Debug)]
pub struct BrushTreeNode {
    /// Splitting plane n.p = dist; children[0] is in front of it.
    pub normal: Vec3,
    pub dist: f32,
    pub children: [i32; 2],
}

impl MapBrushTree {
    /// The order a box (half size `half`) swept from centre `from` to `to`
    /// meets the brushes: leaves near the box's path, front to back along
    /// the move (in front of a split plane first when moving along it),
    /// each brush at its first leaf. `slop` widens the box when deciding
    /// which side of a plane it touches.
    pub fn sweep_order(&self, half: Vec3, from: Vec3, to: Vec3, slop: f32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut stack = vec![self.root];
        while let Some(child) = stack.pop() {
            if child < 0 {
                for &b in self.leaves.get((-child - 1) as usize).map_or(&[][..], |l| &l[..]) {
                    if seen.insert(b) {
                        out.push(b);
                    }
                }
                continue;
            }
            let Some(node) = self.nodes.get(child as usize) else { continue };
            let offset = node.normal.abs().dot(half) + slop;
            let (t1, t2) = (node.normal.dot(from) - node.dist, node.normal.dot(to) - node.dist);
            if t1 >= offset && t2 >= offset {
                stack.push(node.children[0]);
            } else if t1 < -offset && t2 < -offset {
                stack.push(node.children[1]);
            } else {
                // The side the move starts on first: the back when moving
                // toward the front; the front when moving along the plane.
                let first = usize::from(t1 < t2);
                stack.push(node.children[first ^ 1]);
                stack.push(node.children[first]);
            }
        }
        out
    }
}

/// Marks the physics collider built from the same brushes, so movement that
/// sweeps `MapBrushes` itself can leave it out of physics queries.
#[derive(Component, Debug)]
pub struct MapBrushCollider;

/// A prop collider's surface property name (lower-case), reported by traces
/// that hit it (footsteps).
#[derive(Component, Clone, Debug)]
pub struct PropSurface(pub String);

#[cfg(test)]
mod brush_grid_tests {
    use super::*;

    /// The grid's candidates are exactly the brushes whose bounds overlap
    /// the box (what testing every brush finds), in index order.
    #[test]
    fn the_grid_finds_what_testing_every_brush_finds() {
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut rand = move |lo: f32, hi: f32| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            lo + (hi - lo) * (seed >> 40) as f32 / (1u64 << 24) as f32
        };
        let mut brushes = Vec::new();
        for i in 0..2000 {
            let at = Vec3::new(rand(-300.0, 300.0), rand(-50.0, 50.0), rand(-300.0, 300.0));
            // Some long walls and floors (the grid's "big" list), and an
            // empty brush (bounds inverted, as a brush with no volume has).
            if i % 97 == 0 {
                brushes.push(MapBrush::from_box(Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)));
                continue;
            }
            let size = if i % 50 == 0 {
                Vec3::new(rand(50.0, 600.0), rand(0.1, 10.0), rand(50.0, 600.0))
            } else {
                Vec3::new(rand(0.1, 8.0), rand(0.1, 8.0), rand(0.1, 8.0))
            };
            brushes.push(MapBrush::from_box(at, at + size));
        }
        let grid = BrushGrid::new(&brushes);
        let mut near = Vec::new();
        for _ in 0..500 {
            // Boxes inside and outside the brushes' extent.
            let lo = Vec3::new(rand(-1000.0, 1000.0), rand(-60.0, 60.0), rand(-1000.0, 1000.0));
            let hi = lo + Vec3::new(rand(0.0, 20.0), rand(0.0, 5.0), rand(0.0, 20.0));
            grid.near(&brushes, lo, hi, &mut near);
            let all: Vec<u32> = brushes
                .iter()
                .enumerate()
                .filter(|(_, b)| !(b.max.cmplt(lo).any() || b.min.cmpgt(hi).any()))
                .map(|(i, _)| i as u32)
                .collect();
            assert_eq!(near, all);
        }
    }
}
