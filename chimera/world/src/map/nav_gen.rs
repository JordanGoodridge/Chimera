//! Navigation meshes generated from a map's collision, for maps that ship
//! none (`nav::NavMesh` is otherwise a game's file, CS:S's `.nav`): a
//! flood fill over a square grid from seed points (spawns), each cell's
//! floor found by a ray down, kept when a standing box fits above it and
//! nothing stands within the walker's radius, neighbours linked when a box
//! sweeps from one cell to the next at step height (up a step, or down a
//! drop as a one-way link); then the cells merged into axis-aligned
//! rectangles of nearly level, mutually linked cells (`NavArea`s with
//! their corner heights) and the cell links turned into area links
//! (`Via::Walk` across the side they leave by), marked
//! `NavMesh::generated`. Game-neutral: the walker's
//! sizes are `NavGenParams`, the colliders that count are the caller's
//! (`generate`'s `ignore`).
//!
//! Known shortcuts: no ladders, jumps, crouch-only areas, hiding spots or
//! visibility data; cells are tested at their centres only (a wall
//! thinner than a cell between two cell centres is found by the sweep,
//! a pillar between centres narrower than the radius is not); areas'
//! corner heights are the corner cells' floors (a ramp is a staircase of
//! areas no wider than `max_area` cells).

use std::collections::{HashMap, VecDeque};

use avian3d::prelude::*;
use bevy::prelude::*;

use super::nav::{NavArea, NavMesh, Side, Via};

/// The walker and the grid, metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NavGenParams {
    /// Grid cell edge.
    pub cell: f32,
    /// Highest step walked up without a jump.
    pub step: f32,
    /// Standing height (feet to head).
    pub height: f32,
    /// Horizontal half-size of the walker.
    pub radius: f32,
    /// Steepest walkable floor: the floor normal's least `y`.
    pub min_normal_y: f32,
    /// Deepest drop walked off (one way).
    pub max_drop: f32,
    /// Stop growing the grid at this many cells.
    pub max_cells: usize,
    /// Merged areas are at most this many cells a side.
    pub max_area: u32,
    /// The physics layers the walker meets (a game's characters' filter,
    /// e.g. `core::CHARACTER_FILTER`: players-only clips but not
    /// traces-only surfaces).
    pub mask: LayerMask,
}

impl Default for NavGenParams {
    fn default() -> Self {
        Self {
            cell: 0.5,
            step: 0.45,
            height: 1.8,
            radius: 0.3,
            min_normal_y: 0.7,
            max_drop: 2.0,
            max_cells: 300_000,
            max_area: 8,
            mask: crate::core::CHARACTER_FILTER,
        }
    }
}

/// One grid cell's floor.
#[derive(Clone, Copy, Debug)]
struct Cell {
    ix: i32,
    iz: i32,
    y: f32,
}

const DIRS: [(i32, i32, Side); 4] = [
    (1, 0, Side::MaxX),
    (-1, 0, Side::MinX),
    (0, 1, Side::MaxZ),
    (0, -1, Side::MinZ),
];

struct Grid<'a, 'w, 's> {
    q: &'a SpatialQuery<'w, 's>,
    filter: SpatialQueryFilter,
    ignore: &'a dyn Fn(Entity) -> bool,
    p: NavGenParams,
    cells: Vec<Cell>,
    /// Cells of each column (by index), lowest first not guaranteed.
    columns: HashMap<(i32, i32), Vec<usize>>,
    /// Outgoing links per cell: (cell, side).
    links: Vec<Vec<(usize, Side)>>,
}

impl Grid<'_, '_, '_> {
    fn centre(&self, ix: i32, iz: i32) -> (f32, f32) {
        ((ix as f32 + 0.5) * self.p.cell, (iz as f32 + 0.5) * self.p.cell)
    }

    /// The walkable floor below `from_y` within `depth` at a column's
    /// centre (None: no floor, or too steep).
    fn floor(&self, ix: i32, iz: i32, from_y: f32, depth: f32) -> Option<f32> {
        let (x, z) = self.centre(ix, iz);
        let hit = self.q.cast_ray_predicate(
            Vec3::new(x, from_y, z),
            Dir3::NEG_Y,
            depth,
            true,
            &self.filter,
            &|e| !(self.ignore)(e),
        )?;
        (hit.normal.y >= self.p.min_normal_y).then_some(from_y - hit.distance)
    }

    /// Whether a walker stands on the floor `y` of a column: room above it
    /// (from the step up to the head) and no wall within the radius.
    fn stands(&self, ix: i32, iz: i32, y: f32) -> bool {
        let (x, z) = self.centre(ix, iz);
        let half = (self.p.height - self.p.step) * 0.5;
        let shape = Collider::cuboid(self.p.radius * 2.0, half * 2.0, self.p.radius * 2.0);
        let mut blocked = false;
        self.q.shape_intersections_callback(
            &shape,
            Vec3::new(x, y + self.p.step + half, z),
            Quat::IDENTITY,
            &self.filter,
            |e| {
                if (self.ignore)(e) {
                    true
                } else {
                    blocked = true;
                    false
                }
            },
        );
        !blocked
    }

    /// Whether a walker passes from one column's floor `y` to the next
    /// column (a box at step height swept the cell's length).
    fn passes(&self, ix: i32, iz: i32, y: f32, dx: i32, dz: i32) -> bool {
        let (x, z) = self.centre(ix, iz);
        let half = (self.p.height - self.p.step) * 0.5 - 0.02;
        let shape = Collider::cuboid(self.p.radius, half * 2.0, self.p.radius);
        let dir = Dir3::new(Vec3::new(dx as f32, 0.0, dz as f32)).unwrap();
        let config = ShapeCastConfig::from_max_distance(self.p.cell);
        self.q
            .cast_shape_predicate(
                &shape,
                Vec3::new(x, y + self.p.step + 0.02 + half, z),
                Quat::IDENTITY,
                dir,
                &config,
                &self.filter,
                &|e| !(self.ignore)(e),
            )
            .is_none()
    }

    /// The cell of column (ix, iz) whose floor is within a step of `y`.
    fn find(&self, ix: i32, iz: i32, y: f32) -> Option<usize> {
        self.columns
            .get(&(ix, iz))?
            .iter()
            .copied()
            .find(|&i| (self.cells[i].y - y).abs() < self.p.step * 0.5)
    }

    fn add(&mut self, ix: i32, iz: i32, y: f32) -> usize {
        self.cells.push(Cell { ix, iz, y });
        self.links.push(Vec::new());
        let i = self.cells.len() - 1;
        self.columns.entry((ix, iz)).or_default().push(i);
        i
    }
}

/// A navigation mesh over the floors reachable from `seeds` (points
/// standing on or just above a floor, metres), by the colliders the
/// spatial query sees but those `ignore` says to (characters, items).
pub fn generate(
    q: &SpatialQuery,
    ignore: &dyn Fn(Entity) -> bool,
    seeds: &[Vec3],
    p: &NavGenParams,
) -> NavMesh {
    let mut g = Grid {
        q,
        filter: SpatialQueryFilter::from_mask(p.mask),
        ignore,
        p: *p,
        cells: Vec::new(),
        columns: HashMap::new(),
        links: Vec::new(),
    };
    let mut open = VecDeque::new();
    for s in seeds {
        let (ix, iz) = ((s.x / p.cell).floor() as i32, (s.z / p.cell).floor() as i32);
        let Some(y) = g.floor(ix, iz, s.y + 0.5, 4.0) else { continue };
        if g.find(ix, iz, y).is_some() || !g.stands(ix, iz, y) {
            continue;
        }
        open.push_back(g.add(ix, iz, y));
    }
    while let Some(i) = open.pop_front() {
        let Cell { ix, iz, y } = g.cells[i];
        for (dx, dz, side) in DIRS {
            let (jx, jz) = (ix + dx, iz + dz);
            let top = y + p.step + 0.05;
            let Some(y1) = g.floor(jx, jz, top, p.step + 0.05 + p.max_drop) else { continue };
            if !g.passes(ix, iz, y, dx, dz) {
                continue;
            }
            let j = match g.find(jx, jz, y1) {
                Some(j) => j,
                None => {
                    if g.cells.len() >= p.max_cells || !g.stands(jx, jz, y1) {
                        continue;
                    }
                    let j = g.add(jx, jz, y1);
                    open.push_back(j);
                    j
                }
            };
            if !g.links[i].iter().any(|l| l.0 == j) {
                g.links[i].push((j, side));
            }
        }
    }
    merge(&g)
}

/// Cells into rectangles and their links into area links.
fn merge(g: &Grid<'_, '_, '_>) -> NavMesh {
    let n = g.cells.len();
    let linked = |a: usize, b: usize| g.links[a].iter().any(|l| l.0 == b) && g.links[b].iter().any(|l| l.0 == a);
    let level = |a: usize, b: usize| (g.cells[a].y - g.cells[b].y).abs() <= g.p.step * 0.5;
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| (g.cells[i].iz, g.cells[i].ix));
    let mut area_of = vec![usize::MAX; n];
    let mut rects: Vec<(i32, i32, i32, i32, [usize; 4])> = Vec::new();
    let cell_at = |ix: i32, iz: i32, near: usize, area_of: &[usize]| -> Option<usize> {
        let c = g.find(ix, iz, g.cells[near].y)?;
        (area_of[c] == usize::MAX).then_some(c)
    };
    for &a in &order {
        if area_of[a] != usize::MAX {
            continue;
        }
        let Cell { ix, iz, .. } = g.cells[a];
        // Grow along +x.
        let mut row = vec![a];
        while (row.len() as u32) < g.p.max_area {
            let last = *row.last().unwrap();
            let Some(c) = cell_at(ix + row.len() as i32, iz, last, &area_of) else { break };
            if !linked(last, c) || !level(a, c) || row.contains(&c) {
                break;
            }
            row.push(c);
        }
        // Then rows along +z while every cell joins.
        let mut rows = vec![row];
        while (rows.len() as u32) < g.p.max_area {
            let prev = rows.last().unwrap();
            let mut next = Vec::with_capacity(prev.len());
            for (k, &above) in prev.iter().enumerate() {
                let Some(c) = cell_at(ix + k as i32, iz + rows.len() as i32, above, &area_of) else { break };
                if !linked(above, c) || !level(a, c) || (k > 0 && !linked(next[k - 1], c)) {
                    break;
                }
                next.push(c);
            }
            if next.len() != prev.len() {
                break;
            }
            rows.push(next);
        }
        let id = rects.len();
        for r in &rows {
            for &c in r {
                area_of[c] = id;
            }
        }
        let (w, h) = (rows[0].len(), rows.len());
        let corners = [rows[0][0], rows[0][w - 1], rows[h - 1][w - 1], rows[h - 1][0]];
        rects.push((ix, iz, ix + w as i32, iz + h as i32, corners));
    }
    let s = g.p.cell;
    let mut areas: Vec<NavArea> = rects
        .iter()
        .enumerate()
        .map(|(id, &(x0, z0, x1, z1, corners))| {
            let heights = corners.map(|c| g.cells[c].y);
            let min = Vec2::new(x0 as f32 * s, z0 as f32 * s);
            let max = Vec2::new(x1 as f32 * s, z1 as f32 * s);
            let mid = (min + max) * 0.5;
            NavArea {
                id: id as u32,
                flags: 0,
                min,
                max,
                heights,
                center: Vec3::new(mid.x, (heights[0] + heights[2]) * 0.5, mid.y),
                links: Vec::new(),
                place: None,
                hiding: Vec::new(),
                visible: Vec::new(),
                encounters: Vec::new(),
            }
        })
        .collect();
    for (i, links) in g.links.iter().enumerate() {
        for &(j, side) in links {
            let (a, b) = (area_of[i], area_of[j]);
            if a == b || a == usize::MAX || b == usize::MAX {
                continue;
            }
            let link = (b, Via::Walk(side));
            if !areas[a].links.contains(&link) {
                areas[a].links.push(link);
            }
        }
    }
    NavMesh {
        areas,
        ladders: Vec::new(),
        places: Vec::new(),
        generated: true,
    }
}
