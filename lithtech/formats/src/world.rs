//! LithTech world files (the compiled map, `.dat`) as Combat Arms ships
//! them, version 85, per specs/combat_arms/world_dat.md (section names
//! below are the spec's): a 60-byte header with six block offsets, the
//! info string, extents, the world tree's shape, the world models (BSPs:
//! planes, surfaces, polygons, nodes, points), then the blocks the header
//! points at: objects, blind object data, the light grid, collision and
//! particle blockers, and the render data (render blocks with sections,
//! vertices, triangles and RLE lightmaps; light groups).
//!
//! `World::parse` reads a whole file, bounds-checked: every count is
//! checked against the bytes left before anything is allocated, every
//! index against what it indexes ("Validation (engine)"), and every block
//! must end exactly where the next one starts and the last at the end of
//! the file ("Telling versions apart", rule 3). Anything else is an error
//! (`WorldError`, with the byte offset); a still-encrypted payload fails
//! the version check. Values stay in the file's frame (left-handed, +Y
//! up, world units); `to_right_handed` is "Coordinate conversion".
//!
//! Lightmaps, the light grid, the light groups' intensity streams and
//! their sub-lightmaps are decoded while parsing (each must decode to
//! exactly its size from exactly its bytes), so a parsed world holds
//! plain bytes.
//!
//! Malformed data ("Malformed records", Q16): objects are read in
//! sequence by their content, as the engine reads them; a record length R
//! that disagrees is a warning (`World::warnings`), not an error. A
//! property longer than its type uses its leading bytes; one shorter is
//! refused (the engine would read past it), as is a polygon of more than
//! 40 vertices (it would overrun the engine's buffer). The blockers'
//! trailer is reserved and kept as read (Q17); section shader codes are
//! kept as read, sections with codes other than 1, 2, 4, 8 and 9 never
//! drawn (the reader of `RenderWorld` skips them). Strings are 8-bit, no
//! byte above 0x7F in any shipped map: each byte becomes the char of the
//! same code (Latin-1), so nothing is lost.
//!
//! Known shortcuts (our reading where the spec leaves room):
//! - Strict where the engine only reads: a render block with more than
//!   65,535 vertices and a block light group whose section count differs
//!   from its block's are errors. The BSP's polygon-vertex total and
//!   texture-name length are checked (the spec calls them consistency
//!   checks). Not checked: surface texture indices against the texture
//!   list.
//! - Strings keep their full length (the engine's truncations are not
//!   applied, "Texture-name truncation").

use std::fmt;

/// "Header": the only version this reader takes.
pub const VERSION: u32 = 85;
/// "Header": its length; the info string starts here.
pub const HEADER_LEN: usize = 60;
/// "Header" rule 2: the smallest valid block offset.
const MIN_BLOCK_OFFSET: u32 = 64;
/// "BSP arrays": the engine's polygon vertex limit.
pub const MAX_POLYGON_VERTICES: u8 = 40;
/// "Render block": vertex indices are held as 16-bit.
pub const MAX_BLOCK_VERTICES: u32 = 65_535;
/// Decoded sizes above this are treated as corrupt (no allocation of
/// gigabytes from a bad count): 256 MiB.
const MAX_DECODED: usize = 1 << 28;
/// Nested render worlds deeper than this are corrupt (observed: one
/// level, the world models').
const MAX_RENDER_DEPTH: usize = 2;

/// Three f32 as stored: x, y, z (+Y up, +Z forward, left-handed).
pub type Vec3 = [f32; 3];

/// A world-model flag ("World-model flags").
pub mod model_flags {
    pub const MOVABLE: u32 = 0x02;
    pub const MAIN_WORLD: u32 = 0x04;
    pub const PHYSICS: u32 = 0x10;
    pub const VISIBILITY: u32 = 0x20;
}

/// Surface flag bits ("Surface flags"; the names are ours).
pub mod surface_flags {
    pub const SOLID: u32 = 1 << 0;
    pub const NONEXISTENT: u32 = 1 << 1;
    pub const INVISIBLE: u32 = 1 << 2;
    pub const TRANSLUCENT: u32 = 1 << 3;
    pub const SKY_PORTAL: u32 = 1 << 4;
    pub const FULLBRIGHT: u32 = 1 << 5;
    pub const FLAT_SHADED: u32 = 1 << 6;
    pub const LIGHTMAPPED: u32 = 1 << 7;
    pub const NO_SUBDIVIDE: u32 = 1 << 8;
    pub const HULL_MAKER: u32 = 1 << 9;
    pub const PARTICLE_BLOCKER: u32 = 1 << 10;
    pub const GLOBAL_LIGHT_ONLY: u32 = 1 << 11;
    pub const GOURAUD: u32 = 1 << 12;
    pub const OPENABLE_PORTAL: u32 = 1 << 13;
    pub const PANNING_SKY: u32 = 1 << 15;
    pub const PLAYER_BLOCKER: u32 = 1 << 17;
    pub const TERRAIN_OCCLUDER: u32 = 1 << 18;
    pub const SPLIT_OR_ADDITIVE: u32 = 1 << 19;
    pub const VISIBILITY_BLOCKER: u32 = 1 << 21;
    pub const NOT_A_STEP: u32 = 1 << 22;
    pub const MIRROR: u32 = 1 << 23;
    pub const RECEIVES_LIGHT: u32 = 1 << 24;
    pub const RECEIVES_SHADOWS: u32 = 1 << 25;
    pub const RECEIVES_SUNLIGHT: u32 = 1 << 26;
    pub const RECEIVES_SHADOW_MESH: u32 = 1 << 28;
    pub const CASTS_SHADOW_MESH: u32 = 1 << 29;
    pub const CLIPS_LIGHT: u32 = 1 << 30;
}

/// Section shader codes ("Shader codes").
pub mod shader {
    pub const NONE: u8 = 0;
    /// Textured, vertex lit: UV0 = texture.
    pub const GOURAUD: u8 = 1;
    /// The lightmap pass: UV0 = lightmap, texture 0 a light-animation name.
    pub const LIGHTMAP: u8 = 2;
    /// The texture pass of a lightmapped surface: UV0 = texture.
    pub const LIGHTMAP_TEXTURE: u8 = 4;
    pub const PANNING_SKY: u8 = 5;
    pub const SKY_PORTAL: u8 = 6;
    pub const OCCLUDER: u8 = 7;
    /// Two textures, vertex lit: UV0, UV1.
    pub const DUAL_TEXTURE: u8 = 8;
    /// The texture pass of a lightmapped dual-texture surface.
    pub const LIGHTMAP_DUAL_TEXTURE: u8 = 9;
    pub const SPLITTER: u8 = 10;
    pub const UNKNOWN: u8 = 11;
}

/// Node child index of the "in" leaf ("Nodes and point location").
pub const LEAF_IN: i32 = -1;
/// Node child index of the "out" leaf.
pub const LEAF_OUT: i32 = -2;

/// Why a world file was rejected: what, and at which byte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldError {
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for WorldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "world file, byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for WorldError {}

pub type Result<T> = std::result::Result<T, WorldError>;

/// A plane: n·p = d for points p on it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Plane {
    pub normal: Vec3,
    pub dist: f32,
}

impl Plane {
    /// n·p − d: positive in front.
    pub fn side(&self, p: Vec3) -> f32 {
        dot(self.normal, p) - self.dist
    }
}

/// "Header".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Header {
    pub version: u32,
    /// The six block offsets in header order: objects, blind object data,
    /// light grid, collision blockers, particle blockers, render data.
    pub offsets: [u32; 6],
    /// Eight reserved words (0 in every observed file).
    pub reserved: [u32; 8],
}

impl Header {
    pub fn objects(&self) -> u32 {
        self.offsets[0]
    }
    pub fn blind_data(&self) -> u32 {
        self.offsets[1]
    }
    pub fn light_grid(&self) -> u32 {
        self.offsets[2]
    }
    pub fn blockers(&self) -> u32 {
        self.offsets[3]
    }
    pub fn particle_blockers(&self) -> u32 {
        self.offsets[4]
    }
    pub fn render_data(&self) -> u32 {
        self.offsets[5]
    }
}

/// "World tree": the object quadtree's shape over X and Z.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldTree {
    pub min: Vec3,
    pub max: Vec3,
    /// N, nodes including the root.
    pub node_count: u32,
    pub terrain_depth: u32,
    /// Per node in pre-order: subdivided into four children.
    pub subdivided: Vec<bool>,
}

impl WorldTree {
    /// Every node's box in pre-order (child k as the spec's table).
    pub fn boxes(&self) -> Vec<(Vec3, Vec3)> {
        let mut out = Vec::with_capacity(self.subdivided.len());
        let mut stack = vec![(self.min, self.max)];
        let mut i = 0;
        while let Some((lo, hi)) = stack.pop() {
            out.push((lo, hi));
            if self.subdivided.get(i).copied().unwrap_or(false) {
                let c = [(lo[0] + hi[0]) / 2.0, (lo[2] + hi[2]) / 2.0];
                let child = |k: usize| {
                    let (x0, x1) = if k < 2 { (lo[0], c[0]) } else { (c[0], hi[0]) };
                    let (z0, z1) = if k.is_multiple_of(2) { (lo[2], c[1]) } else { (c[1], hi[2]) };
                    ([x0, lo[1], z0], [x1, hi[1], z1])
                };
                // Pushed in reverse so child 0 comes out first.
                for k in (0..4).rev() {
                    stack.push(child(k));
                }
            }
            i += 1;
        }
        out
    }
}

/// A BSP surface ("Surfaces").
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Surface {
    pub flags: u32,
    /// Index into the BSP's texture names.
    pub texture: u16,
    /// The game's surface type id (material), 0 unknown.
    pub surface_type: u16,
}

/// A BSP polygon ("Polygons"); its point indices are
/// `WorldModel::polygon_points`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BspPolygon {
    pub surface: u32,
    pub plane: u32,
    /// Start of its point indices in `WorldModel::point_indices`.
    pub first: u32,
    pub count: u8,
}

/// A BSP node ("Nodes"): splits by its polygon's plane.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BspNode {
    pub polygon: u32,
    /// Child index, or `LEAF_IN` / `LEAF_OUT`.
    pub back: i32,
    pub front: i32,
}

/// Where a point lies in a BSP ("Nodes and point location").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    /// The "in" leaf: open space in the physics BSP.
    In,
    /// The "out" leaf: solid or outside.
    Out,
}

/// A world model: one BSP ("World models (BSPs)").
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldModel {
    /// The record's leading word (0 everywhere; ignored by the engine).
    pub prefix: u32,
    pub flags: u32,
    pub name: String,
    pub leaf_count: u32,
    /// Sum of all polygon vertex counts, as stored.
    pub polygon_vertex_refs: u32,
    pub min: Vec3,
    pub max: Vec3,
    pub translation: Vec3,
    pub textures: Vec<String>,
    pub planes: Vec<Plane>,
    pub surfaces: Vec<Surface>,
    pub polygons: Vec<BspPolygon>,
    /// Every polygon's point indices, back to back.
    pub point_indices: Vec<u32>,
    pub nodes: Vec<BspNode>,
    pub points: Vec<Vec3>,
    pub root: i32,
}

impl WorldModel {
    /// The physics BSP (the static level).
    pub fn is_physics(&self) -> bool {
        self.flags & model_flags::PHYSICS != 0
    }

    /// Polygon `i`'s point indices.
    pub fn polygon_points(&self, i: usize) -> &[u32] {
        let p = &self.polygons[i];
        &self.point_indices[p.first as usize..p.first as usize + p.count as usize]
    }

    /// Polygon `i`'s points, in stored order.
    pub fn polygon_vertices(&self, i: usize) -> impl Iterator<Item = Vec3> + '_ {
        self.polygon_points(i).iter().map(|&k| self.points[k as usize])
    }

    /// Polygon `i`'s surface.
    pub fn polygon_surface(&self, i: usize) -> &Surface {
        &self.surfaces[self.polygons[i].surface as usize]
    }

    /// Polygon `i`'s plane.
    pub fn polygon_plane(&self, i: usize) -> &Plane {
        &self.planes[self.polygons[i].plane as usize]
    }

    /// Walk the nodes from the root: front when n·p − d > 0, else back,
    /// until a leaf. A model without nodes (root a leaf) answers its root.
    pub fn locate(&self, p: Vec3) -> Location {
        let mut node = self.root;
        // Indices were validated: each step is a node or a leaf; a cycle
        // can't run longer than the node count.
        for _ in 0..=self.nodes.len() {
            match node {
                LEAF_IN => return Location::In,
                LEAF_OUT => return Location::Out,
                _ => {}
            }
            let n = &self.nodes[node as usize];
            let plane = self.polygon_plane(n.polygon as usize);
            node = if plane.side(p) > 0.0 { n.front } else { n.back };
        }
        Location::Out
    }
}

/// An object property's value ("Property types").
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    String(String),
    Vector(Vec3),
    /// r, g, b, each 0–255.
    Color(Vec3),
    Real(f32),
    /// Stored as f32 (`Value::int` truncates it).
    Flags(f32),
    Bool(bool),
    /// Stored as f32 (`Value::int` truncates it).
    Long(f32),
    /// Pitch, yaw, roll (radians) and a fourth, always-0 float.
    Rotation([f32; 4]),
    /// A type code the spec doesn't know: its bytes.
    Unknown { code: u8, data: Vec<u8> },
}

impl Value {
    /// The type code as stored.
    pub fn code(&self) -> u8 {
        match self {
            Value::String(_) => 0,
            Value::Vector(_) => 1,
            Value::Color(_) => 2,
            Value::Real(_) => 3,
            Value::Flags(_) => 4,
            Value::Bool(_) => 5,
            Value::Long(_) => 6,
            Value::Rotation(_) => 7,
            Value::Unknown { code, .. } => *code,
        }
    }

    /// A flags or long-integer value as the engine reads it: the f32
    /// truncated toward zero ("Integer properties stored as reals").
    pub fn int(&self) -> Option<i32> {
        match self {
            Value::Flags(f) | Value::Long(f) => Some(*f as i32),
            _ => None,
        }
    }
}

/// One property of an object.
#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    pub name: String,
    /// Editor flags (no runtime meaning).
    pub flags: u32,
    /// D, the stored data length.
    pub len: u16,
    pub value: Value,
}

/// An object ("Objects"): a class and its properties.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Object {
    pub class: String,
    pub properties: Vec<Property>,
}

impl Object {
    /// The first property of that name (names compared ignoring ASCII case).
    pub fn property(&self, name: &str) -> Option<&Property> {
        self.properties.iter().find(|p| p.name.eq_ignore_ascii_case(name))
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.property(name).map(|p| &p.value)
    }

    pub fn string(&self, name: &str) -> Option<&str> {
        match self.get(name)? {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn vector(&self, name: &str) -> Option<Vec3> {
        match self.get(name)? {
            Value::Vector(v) | Value::Color(v) => Some(*v),
            _ => None,
        }
    }

    pub fn real(&self, name: &str) -> Option<f32> {
        match self.get(name)? {
            Value::Real(r) => Some(*r),
            _ => None,
        }
    }

    pub fn bool(&self, name: &str) -> Option<bool> {
        match self.get(name)? {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn int(&self, name: &str) -> Option<i32> {
        self.get(name)?.int()
    }

    /// Pitch, yaw, roll of a rotation property.
    pub fn rotation(&self, name: &str) -> Option<[f32; 3]> {
        match self.get(name)? {
            Value::Rotation([p, y, r, _]) => Some([*p, *y, *r]),
            _ => None,
        }
    }

    /// `Name`.
    pub fn name(&self) -> Option<&str> {
        self.string("Name")
    }

    /// `Pos`.
    pub fn pos(&self) -> Option<Vec3> {
        self.vector("Pos")
    }
}

/// A blind object data chunk ("Blind object data").
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlindChunk {
    pub id: u32,
    pub data: Vec<u8>,
}

/// The light grid ("Light grid"), decoded.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LightGrid {
    pub origin: Vec3,
    pub cell: Vec3,
    /// X, Y, Z sample counts.
    pub counts: [u32; 3],
    /// C, the compressed byte count.
    pub compressed_len: u32,
    /// R, G, B per sample, at 3·(k·X·Y + j·X + i).
    pub samples: Vec<u8>,
}

impl LightGrid {
    /// Sample (i, j, k).
    pub fn sample(&self, i: u32, j: u32, k: u32) -> [u8; 3] {
        let [x, y, _] = self.counts;
        let at = 3 * (k as usize * x as usize * y as usize + j as usize * x as usize + i as usize);
        [self.samples[at], self.samples[at + 1], self.samples[at + 2]]
    }
}

/// A collision or particle blocker polygon, or a sky portal or occluder
/// ("geometry polygon").
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeometryPolygon {
    pub plane: Plane,
    pub vertices: Vec<Vec3>,
}

/// "Collision blockers and particle blockers".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Blockers {
    pub polygons: Vec<GeometryPolygon>,
    /// The trailing word (always 0).
    pub trailer: u32,
}

/// A render section ("Section").
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    /// Texture 0 and 1 (empty for none).
    pub textures: [String; 2],
    pub shader: u8,
    pub triangle_count: u32,
    /// The texture-effect script name (usually empty).
    pub effect: String,
    pub lightmap_width: u32,
    pub lightmap_height: u32,
    /// Z, the stored (compressed) lightmap bytes.
    pub lightmap_len: u32,
    /// The decoded lightmap: R, G, B per texel, rows from the top; empty
    /// when the section has none.
    pub lightmap: Vec<u8>,
}

/// A render vertex ("Vertex (68 bytes)").
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RenderVertex {
    pub pos: Vec3,
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
    /// A·2^24 + R·2^16 + G·2^8 + B.
    pub color: u32,
    pub normal: Vec3,
    pub tangent: Vec3,
    pub binormal: Vec3,
}

impl RenderVertex {
    /// The colour as R, G, B, A bytes.
    pub fn rgba(&self) -> [u8; 4] {
        let [b, g, r, a] = self.color.to_le_bytes();
        [r, g, b, a]
    }
}

/// A render triangle: three vertex indices and the BSP polygon it came
/// from.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Triangle {
    pub indices: [u32; 3],
    pub polygon: u32,
}

/// A rectangle of light-group intensities over a section's lightmap
/// ("Sub-lightmap"), decoded: w·h bytes, rows from the top.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubLightmap {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
    pub intensities: Vec<u8>,
}

/// A switchable light's share of one render block ("Block light group").
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockLightGroup {
    pub name: String,
    /// 0–1 per channel.
    pub color: Vec3,
    /// One intensity per block vertex.
    pub intensities: Vec<u8>,
    /// Per section of the block, its sub-lightmaps.
    pub sections: Vec<Vec<SubLightmap>>,
}

/// A render block ("Render block"): a node of the render tree.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenderBlock {
    pub center: Vec3,
    pub half_size: Vec3,
    pub sections: Vec<Section>,
    pub vertices: Vec<RenderVertex>,
    pub triangles: Vec<Triangle>,
    pub sky_portals: Vec<GeometryPolygon>,
    /// Occluder polygons and their ids.
    pub occluders: Vec<(GeometryPolygon, u32)>,
    pub light_groups: Vec<BlockLightGroup>,
    pub child_flags: u8,
    /// Child block indices as stored (meaningful where `child_flags` says).
    pub children: [u32; 2],
}

impl RenderBlock {
    /// Each section with its triangles (section 0 owns the first t₀, ...).
    pub fn section_triangles(&self) -> impl Iterator<Item = (&Section, &[Triangle])> {
        let mut start = 0usize;
        self.sections.iter().map(move |s| {
            let tris = &self.triangles[start..start + s.triangle_count as usize];
            start += s.triangle_count as usize;
            (s, tris)
        })
    }

    /// Present children.
    pub fn child_blocks(&self) -> impl Iterator<Item = u32> + '_ {
        (0..2).filter(|k| self.child_flags & (1 << k) != 0).map(|k| self.children[k])
    }
}

/// A render world: blocks, and per world model a nested render world
/// ("Render data").
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenderWorld {
    pub blocks: Vec<RenderBlock>,
    pub models: Vec<(String, RenderWorld)>,
}

impl RenderWorld {
    /// This world's blocks and its world models' (one level down).
    pub fn all_blocks(&self) -> impl Iterator<Item = &RenderBlock> {
        self.blocks.iter().chain(self.models.iter().flat_map(|(_, w)| w.blocks.iter()))
    }
}

/// "World light group": a switchable light's light-grid fix-up.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldLightGroup {
    pub name: String,
    pub color: Vec3,
    pub offset: [u32; 3],
    pub size: [u32; 3],
    /// At k·sx·sy + j·sx + i.
    pub intensities: Vec<u8>,
}

/// A parsed world file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct World {
    pub header: Header,
    /// The info string ("World info string").
    pub info: String,
    pub extents_min: Vec3,
    pub extents_max: Vec3,
    /// What the compiler removed to recentre the map (editor = file + this).
    pub world_offset: Vec3,
    pub tree: WorldTree,
    /// Where the world tree ended (the world models start).
    pub tree_end: usize,
    pub models: Vec<WorldModel>,
    pub objects: Vec<Object>,
    pub blind_data: Vec<BlindChunk>,
    pub light_grid: LightGrid,
    pub blockers: Blockers,
    pub particle_blockers: Blockers,
    pub render: RenderWorld,
    pub light_groups: Vec<WorldLightGroup>,
    /// The file's length.
    pub file_len: usize,
    /// What the engine reads past without noticing (an object record
    /// whose length R disagrees with its content).
    pub warnings: Vec<String>,
}

impl World {
    /// Parse a whole world file (the bytes the archive returns).
    pub fn parse(bytes: &[u8]) -> Result<World> {
        let mut c = Cursor { b: bytes, pos: 0 };
        let mut w = World {
            file_len: bytes.len(),
            ..Default::default()
        };
        if bytes.len() < HEADER_LEN + 4 {
            return Err(c.err(format!("{} bytes: too short for a world file", bytes.len())));
        }
        // "Header" and "Telling versions apart", rules 1 and 2.
        let version = c.u32()?;
        if version != VERSION {
            return Err(WorldError {
                offset: 0,
                message: format!("version {version}, not {VERSION} (an unsupported version, or still encrypted)"),
            });
        }
        let mut offsets = [0u32; 6];
        for o in &mut offsets {
            *o = c.u32()?;
        }
        let mut reserved = [0u32; 8];
        for r in &mut reserved {
            *r = c.u32()?;
        }
        let mut last = MIN_BLOCK_OFFSET;
        for (i, &o) in offsets.iter().enumerate() {
            if o < last || o as usize > bytes.len() {
                return Err(WorldError {
                    offset: 4 + 4 * i,
                    message: format!(
                        "block offset {i} is {o}: offsets must be non-decreasing, at least {MIN_BLOCK_OFFSET} and at most the file size {}",
                        bytes.len()
                    ),
                });
            }
            last = o;
        }
        w.header = Header {
            version,
            offsets,
            reserved,
        };

        // "World info string, extents and offset".
        let len = c.u32()? as usize;
        w.info = latin1(c.take(len).map_err(|e| e.context("the info string"))?);
        w.extents_min = c.vec3()?;
        w.extents_max = c.vec3()?;
        w.world_offset = c.vec3()?;

        // "World tree".
        w.tree = parse_tree(&mut c)?;
        w.tree_end = c.pos;

        // "World models (BSPs)", up to the object list.
        let count = c.count(8, "world models")?;
        w.models = Vec::with_capacity(count);
        for i in 0..count {
            let prefix = c.u32()?;
            let mut m = parse_bsp(&mut c).map_err(|e| e.context(&format!("world model {i}")))?;
            m.prefix = prefix;
            w.models.push(m);
        }
        c.expect_end(offsets[0], "the world models", "the object list")?;

        // "Objects".
        let count = c.count(2, "objects")?;
        w.objects = Vec::with_capacity(count);
        for i in 0..count {
            let (o, warning) = parse_object(&mut c).map_err(|e| e.context(&format!("object {i}")))?;
            w.objects.push(o);
            w.warnings.extend(warning.map(|m| format!("object {i}: {m}")));
        }
        c.expect_end(offsets[1], "the object list", "the blind object data")?;

        // "Blind object data".
        let count = c.count(8, "blind chunks")?;
        for _ in 0..count {
            let size = c.u32()? as usize;
            let id = c.u32()?;
            let data = c.take(size)?.to_vec();
            w.blind_data.push(BlindChunk { id, data });
        }
        c.expect_end(offsets[2], "the blind object data", "the light grid")?;

        // "Light grid".
        w.light_grid = parse_light_grid(&mut c)?;
        c.expect_end(offsets[3], "the light grid", "the collision blockers")?;

        // "Collision blockers and particle blockers".
        w.blockers = parse_blockers(&mut c)?;
        c.expect_end(offsets[4], "the collision blockers", "the particle blockers")?;
        w.particle_blockers = parse_blockers(&mut c)?;
        c.expect_end(offsets[5], "the particle blockers", "the render data")?;

        // "Render data", then the world light groups to the end.
        w.render = parse_render_world(&mut c, 0)?;
        let count = c.count(2 + 12 + 24, "world light groups")?;
        for _ in 0..count {
            let name = c.lstring()?;
            let color = c.vec3()?;
            let offset = [c.u32()?, c.u32()?, c.u32()?];
            let size = [c.u32()?, c.u32()?, c.u32()?];
            let n = (size[0] as usize)
                .checked_mul(size[1] as usize)
                .and_then(|n| n.checked_mul(size[2] as usize))
                .filter(|&n| n <= MAX_DECODED)
                .ok_or_else(|| c.err(format!("world light group size {size:?} is too large")))?;
            let intensities = c.take(n)?.to_vec();
            w.light_groups.push(WorldLightGroup {
                name,
                color,
                offset,
                size,
                intensities,
            });
        }
        if c.pos != bytes.len() {
            return Err(c.err(format!(
                "the world light groups end at {}, not at the end of the file ({})",
                c.pos,
                bytes.len()
            )));
        }
        Ok(w)
    }

    /// The physics BSP (the one world model flagged physics).
    pub fn physics(&self) -> Option<&WorldModel> {
        self.models.iter().find(|m| m.is_physics())
    }

    /// A world model by name (exact).
    pub fn model(&self, name: &str) -> Option<(usize, &WorldModel)> {
        self.models.iter().enumerate().find(|(_, m)| m.name == name)
    }

    /// Objects of a class (exact name).
    pub fn objects_of<'a>(&'a self, class: &'a str) -> impl Iterator<Item = (usize, &'a Object)> + 'a {
        self.objects.iter().enumerate().filter(move |(_, o)| o.class == class)
    }

    /// A keyword's arguments in the info string (`ambientlight 30 30 34;
    /// lmgridsize 10`), keywords matched ignoring case.
    pub fn info_value(&self, keyword: &str) -> Option<Vec<&str>> {
        self.info.split(';').find_map(|stmt| {
            let mut words = stmt.split_whitespace();
            let k = words.next()?;
            k.eq_ignore_ascii_case(keyword).then(|| words.collect())
        })
    }

    /// `ambientlight r g b` clamped to 0..255; missing means 0, 0, 0.
    pub fn ambient_light(&self) -> [f32; 3] {
        let mut out = [0.0; 3];
        if let Some(args) = self.info_value("ambientlight") {
            for (o, a) in out.iter_mut().zip(args) {
                *o = a.parse::<f32>().unwrap_or(0.0).clamp(0.0, 255.0);
            }
        }
        out
    }
}

/// "Name hash": h = 31·h + b over the bytes, mod 2^32 (case-sensitive).
pub fn name_hash(name: &[u8]) -> u32 {
    name.iter().fold(0u32, |h, &b| h.wrapping_mul(31).wrapping_add(b as u32))
}

/// "Rotation to matrix": the right, up and forward columns of
/// R = R_y(yaw) · R_x(pitch) · R_z(roll), in the file's axes.
pub fn rotation_axes(pitch: f32, yaw: f32, roll: f32) -> [Vec3; 3] {
    let (sp, cp) = pitch.sin_cos();
    let (sy, cy) = yaw.sin_cos();
    let (sr, cr) = roll.sin_cos();
    [
        [cr * cy + sr * sp * sy, sr * cp, -cr * sy + sr * sp * cy],
        [-sr * cy + cr * sp * sy, cr * cp, sr * sy + cr * sp * cy],
        [cp * sy, -sp, cp * cy],
    ]
}

/// "Coordinate conversion": the file's left-handed frame to a right-handed
/// Y-up one, mirroring Z (positions, normals and directions alike; plane
/// distances don't change, polygon and triangle order must be reversed).
pub fn to_right_handed(v: Vec3) -> Vec3 {
    [v[0], v[1], -v[2]]
}

/// The RLE of the light grid and the lightmaps ("RLE"): tag byte t,
/// n = (t & 0x7F) + 1; t & 0x80: one RGB triple n times, else n triples.
/// `texels` triples must come out of exactly all of `data`.
pub fn decode_rgb_rle(data: &[u8], texels: usize) -> std::result::Result<Vec<u8>, String> {
    let want = texels * 3;
    let mut out = Vec::with_capacity(want);
    let mut i = 0;
    while out.len() < want {
        let t = *data.get(i).ok_or("the data ends before the texels are filled")?;
        i += 1;
        let n = (t & 0x7F) as usize + 1;
        if out.len() + 3 * n > want {
            return Err(format!("a run of {n} decodes past the {texels} texels"));
        }
        if t & 0x80 != 0 {
            let rgb = data.get(i..i + 3).ok_or("a run's colour is cut off")?;
            i += 3;
            for _ in 0..n {
                out.extend_from_slice(rgb);
            }
        } else {
            out.extend_from_slice(data.get(i..i + 3 * n).ok_or("a span is cut off")?);
            i += 3 * n;
        }
    }
    if i != data.len() {
        return Err(format!("{} bytes left after {texels} texels", data.len() - i));
    }
    Ok(out)
}

/// "Vertex intensity stream": x ≠ 0 is one vertex; 0, n is 1 + n zeros.
/// Exactly `count` entries from exactly all of `data`.
pub fn decode_intensities(data: &[u8], count: usize) -> std::result::Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(count);
    let mut i = 0;
    while i < data.len() {
        let x = data[i];
        i += 1;
        if x != 0 {
            out.push(x);
        } else {
            let n = *data.get(i).ok_or("a zero run's length is cut off")? as usize;
            i += 1;
            out.resize(out.len() + 1 + n, 0);
        }
        if out.len() > count {
            return Err(format!("decodes past {count} vertices"));
        }
    }
    if out.len() != count {
        return Err(format!("decodes to {} entries, not {count}", out.len()));
    }
    Ok(out)
}

/// "Sub-lightmap" run data: 0xFF, n, x is n + 1 texels of x; any other
/// byte one texel. Exactly `count` texels from exactly all of `data` (a
/// lone 0xFF, 0, 0 is one texel of 0: "Sub-lightmap escape").
pub fn decode_sub_lightmap(data: &[u8], count: usize) -> std::result::Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(count);
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        i += 1;
        if b == 0xFF {
            let [n, x] = data
                .get(i..i + 2)
                .and_then(|s| s.try_into().ok())
                .ok_or("a run is cut off")?;
            i += 2;
            out.resize(out.len() + n as usize + 1, x);
        } else {
            out.push(b);
        }
        if out.len() > count {
            return Err(format!("decodes past {count} texels"));
        }
    }
    if out.len() != count {
        return Err(format!("decodes to {} texels, not {count}", out.len()));
    }
    Ok(out)
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| c as char).collect()
}

impl WorldError {
    fn context(mut self, what: &str) -> Self {
        self.message = format!("{what}: {}", self.message);
        self
    }
}

/// A bounds-checked little-endian reader.
struct Cursor<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn err(&self, message: impl Into<String>) -> WorldError {
        WorldError {
            offset: self.pos,
            message: message.into(),
        }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.b.len())
            .ok_or_else(|| self.err(format!("{n} bytes wanted, {} left", self.b.len() - self.pos)))?;
        let s = &self.b[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.take(N)?.try_into().unwrap())
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.array()?))
    }

    fn vec3(&mut self) -> Result<Vec3> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    fn plane(&mut self) -> Result<Plane> {
        Ok(Plane {
            normal: self.vec3()?,
            dist: self.f32()?,
        })
    }

    /// "lstring": u16 byte count, the bytes, no terminator.
    fn lstring(&mut self) -> Result<String> {
        let n = self.u16()? as usize;
        Ok(latin1(self.take(n)?))
    }

    /// A u32 count of elements at least `min_size` bytes each, checked
    /// against the bytes left.
    fn count(&mut self, min_size: usize, what: &str) -> Result<usize> {
        let at = self.pos;
        let n = self.u32()? as usize;
        self.check_count(n, min_size, what, at)
    }

    fn check_count(&self, n: usize, min_size: usize, what: &str, at: usize) -> Result<usize> {
        let left = self.b.len() - self.pos;
        if n.saturating_mul(min_size) > left {
            return Err(WorldError {
                offset: at,
                message: format!("{n} {what} can't fit in the {left} bytes left"),
            });
        }
        Ok(n)
    }

    /// "File layout": a block's sequential parse ends where the next starts.
    fn expect_end(&mut self, next: u32, block: &str, next_block: &str) -> Result<()> {
        if self.pos != next as usize {
            return Err(self.err(format!("{block} end at {}, but {next_block} starts at {next}", self.pos)));
        }
        Ok(())
    }
}

fn parse_tree(c: &mut Cursor) -> Result<WorldTree> {
    let min = c.vec3()?;
    let max = c.vec3()?;
    let at = c.pos;
    let n = c.u32()? as usize;
    let terrain_depth = c.u32()?;
    if n == 0 {
        return Err(WorldError {
            offset: at,
            message: "the world tree has no nodes (the root counts)".into(),
        });
    }
    let bits = c.take(n.div_ceil(8)).map_err(|e| e.context("the world tree's bits"))?;
    let bit = |i: usize| bits[i / 8] & (1 << (i % 8)) != 0;
    // Pre-order: each node read takes one bit; a 1 adds four nodes to read.
    let (mut produced, mut pending) = (0usize, 1usize);
    let mut subdivided = Vec::with_capacity(n);
    while pending > 0 {
        if produced >= n {
            return Err(WorldError {
                offset: at,
                message: format!("the world tree's bits describe more than its {n} nodes"),
            });
        }
        let b = bit(produced);
        subdivided.push(b);
        produced += 1;
        pending = pending - 1 + if b { 4 } else { 0 };
    }
    if produced != n {
        return Err(WorldError {
            offset: at,
            message: format!("the world tree's bits describe {produced} nodes, not {n}"),
        });
    }
    Ok(WorldTree {
        min,
        max,
        node_count: n as u32,
        terrain_depth,
        subdivided,
    })
}

fn parse_bsp(c: &mut Cursor) -> Result<WorldModel> {
    let mut m = WorldModel {
        flags: c.u32()?,
        name: c.lstring()?,
        ..Default::default()
    };
    let p = c.u32()? as usize;
    let q = c.u32()? as usize;
    let s = c.u32()? as usize;
    let at = c.pos;
    let user_portals = c.u32()?;
    if user_portals != 0 {
        return Err(WorldError {
            offset: at,
            message: format!("{user_portals} user portals (must be 0)"),
        });
    }
    let g = c.u32()? as usize;
    m.leaf_count = c.u32()?;
    m.polygon_vertex_refs = c.u32()?;
    let _vis_list_size = c.u32()?;
    let _leaf_lists = c.u32()?;
    let k = c.u32()? as usize;
    m.min = c.vec3()?;
    m.max = c.vec3()?;
    m.translation = c.vec3()?;
    let t_len = c.u32()? as usize;
    let t = c.u32()? as usize;

    // Texture names: T NUL-terminated strings in exactly T_len bytes.
    let at = c.pos;
    let names = c.take(t_len).map_err(|e| e.context("the texture names"))?;
    let mut rest = names;
    for _ in 0..t {
        let end = rest.iter().position(|&b| b == 0).ok_or_else(|| WorldError {
            offset: at,
            message: format!("{t} texture names don't fit in {t_len} bytes"),
        })?;
        m.textures.push(latin1(&rest[..end]));
        rest = &rest[end + 1..];
    }
    if !rest.is_empty() {
        return Err(WorldError {
            offset: at,
            message: format!("{t} texture names leave {} of their {t_len} bytes", rest.len()),
        });
    }

    let at = c.pos;
    c.check_count(g, 1 + 8 + 12, "polygons", at)?;
    let counts = c.take(g)?.to_vec();
    if let Some(i) = counts.iter().position(|&v| v > MAX_POLYGON_VERTICES) {
        return Err(WorldError {
            offset: at + i,
            message: format!("polygon {i} has {} vertices (at most {MAX_POLYGON_VERTICES})", counts[i]),
        });
    }
    let refs: u64 = counts.iter().map(|&v| v as u64).sum();
    if refs != m.polygon_vertex_refs as u64 {
        return Err(WorldError {
            offset: at,
            message: format!(
                "polygon vertex counts sum to {refs}, the header says {}",
                m.polygon_vertex_refs
            ),
        });
    }

    // "Leaves" (Edge cases): skipped.
    for _ in 0..m.leaf_count {
        let n = c.u16()?;
        if n == 0xFFFF {
            c.u16()?;
        } else {
            for _ in 0..n {
                let _portal = c.u16()?;
                let len = c.u16()? as usize;
                c.take(len)?;
            }
        }
    }

    let at = c.pos;
    c.check_count(q, 16, "planes", at)?;
    m.planes = (0..q).map(|_| c.plane()).collect::<Result<_>>()?;
    let at = c.pos;
    c.check_count(s, 8, "surfaces", at)?;
    m.surfaces = (0..s)
        .map(|_| {
            Ok(Surface {
                flags: c.u32()?,
                texture: c.u16()?,
                surface_type: c.u16()?,
            })
        })
        .collect::<Result<_>>()?;

    m.polygons.reserve(g);
    m.point_indices.reserve(refs as usize);
    for (i, &v) in counts.iter().enumerate() {
        let at = c.pos;
        let surface = c.u32()?;
        let plane = c.u32()?;
        if surface as usize >= s || plane as usize >= q {
            return Err(WorldError {
                offset: at,
                message: format!("polygon {i}: surface {surface} of {s}, plane {plane} of {q}"),
            });
        }
        let first = m.point_indices.len() as u32;
        for _ in 0..v {
            let at = c.pos;
            let k = c.u32()?;
            if k as usize >= p {
                return Err(WorldError {
                    offset: at,
                    message: format!("polygon {i}: point index {k}, but there are {p} points"),
                });
            }
            m.point_indices.push(k);
        }
        m.polygons.push(BspPolygon {
            surface,
            plane,
            first,
            count: v,
        });
    }

    let child_ok = |i: i32| i == LEAF_IN || i == LEAF_OUT || (i >= 0 && (i as usize) < k);
    let at = c.pos;
    c.check_count(k, 14, "nodes", at)?;
    m.nodes.reserve(k);
    for i in 0..k {
        let at = c.pos;
        let polygon = c.u32()?;
        let _unused = c.u16()?;
        let back = c.i32()?;
        let front = c.i32()?;
        if polygon as usize >= g || !child_ok(back) || !child_ok(front) {
            return Err(WorldError {
                offset: at,
                message: format!("node {i}: polygon {polygon} of {g}, children {back} and {front} of {k}"),
            });
        }
        m.nodes.push(BspNode { polygon, back, front });
    }
    let at = c.pos;
    c.check_count(p, 12, "points", at)?;
    m.points = (0..p).map(|_| c.vec3()).collect::<Result<_>>()?;
    let at = c.pos;
    m.root = c.i32()?;
    if !child_ok(m.root) {
        return Err(WorldError {
            offset: at,
            message: format!("root node {} of {k}", m.root),
        });
    }
    let at = c.pos;
    let sections = c.u32()?;
    if sections != 0 {
        return Err(WorldError {
            offset: at,
            message: format!("{sections} terrain sections (must be 0)"),
        });
    }
    Ok(m)
}

/// One object record ("Objects"), read in sequence as the engine does: by
/// its content, the record length R only compared (a mismatch is returned
/// as a warning, the next record read right after the last property).
fn parse_object(c: &mut Cursor) -> Result<(Object, Option<String>)> {
    let len = c.u16()? as usize;
    let start = c.pos;
    let end = start + len;
    let class = c.lstring()?;
    let count = c.count(2 + 1 + 4 + 2, "properties")?;
    let mut properties = Vec::with_capacity(count);
    for _ in 0..count {
        let name = c.lstring()?;
        let code = c.u8()?;
        let flags = c.u32()?;
        let d = c.u16()?;
        let at = c.pos;
        let data = c.take(d as usize)?;
        let bad = || WorldError {
            offset: at,
            message: format!("property {name}: {d} bytes don't hold a value of type {code}"),
        };
        let f = |i: usize| f32::from_le_bytes(data[4 * i..4 * i + 4].try_into().unwrap());
        // "Malformed records": a value longer than its type uses its
        // leading bytes; a shorter one would be read past its data.
        let fixed = |size: usize| if data.len() >= size { Ok(()) } else { Err(bad()) };
        let value = match code {
            0 => {
                let n = data
                    .get(..2)
                    .map(|b| u16::from_le_bytes([b[0], b[1]]) as usize)
                    .ok_or_else(bad)?;
                Value::String(latin1(data.get(2..2 + n).ok_or_else(bad)?))
            }
            1 => fixed(12).map(|_| Value::Vector([f(0), f(1), f(2)]))?,
            2 => fixed(12).map(|_| Value::Color([f(0), f(1), f(2)]))?,
            3 => fixed(4).map(|_| Value::Real(f(0)))?,
            4 => fixed(4).map(|_| Value::Flags(f(0)))?,
            5 => fixed(1).map(|_| Value::Bool(data[0] != 0))?,
            6 => fixed(4).map(|_| Value::Long(f(0)))?,
            7 => fixed(16).map(|_| Value::Rotation([f(0), f(1), f(2), f(3)]))?,
            // "Unknown type codes": skipped by their length, kept as bytes.
            _ => Value::Unknown {
                code,
                data: data.to_vec(),
            },
        };
        properties.push(Property {
            name,
            flags,
            len: d,
            value,
        });
    }
    let warning = (c.pos != end).then(|| {
        format!(
            "{class}: the record says {len} bytes, its properties take {}",
            c.pos - start
        )
    });
    Ok((Object { class, properties }, warning))
}

fn parse_light_grid(c: &mut Cursor) -> Result<LightGrid> {
    let origin = c.vec3()?;
    let cell = c.vec3()?;
    let at = c.pos;
    let raw = [c.i32()?, c.i32()?, c.i32()?];
    let bad = |m: String| WorldError { offset: at, message: m };
    if raw.iter().any(|&n| n < 0) {
        return Err(bad(format!("light grid sample counts {raw:?}")));
    }
    let counts = raw.map(|n| n as u32);
    let texels = (counts[0] as usize)
        .checked_mul(counts[1] as usize)
        .and_then(|n| n.checked_mul(counts[2] as usize))
        .filter(|&n| n * 3 <= MAX_DECODED)
        .ok_or_else(|| bad(format!("light grid sample counts {counts:?} are too large")))?;
    let compressed_len = c.u32()?;
    let data = c.take(compressed_len as usize)?;
    let samples = decode_rgb_rle(data, texels).map_err(|m| bad(format!("light grid: {m}")))?;
    Ok(LightGrid {
        origin,
        cell,
        counts,
        compressed_len,
        samples,
    })
}

fn parse_geometry_polygon(c: &mut Cursor, count: usize) -> Result<GeometryPolygon> {
    c.check_count(count, 12, "vertices", c.pos)?;
    let vertices = (0..count).map(|_| c.vec3()).collect::<Result<_>>()?;
    Ok(GeometryPolygon {
        plane: Plane::default(),
        vertices,
    })
}

fn parse_blockers(c: &mut Cursor) -> Result<Blockers> {
    let count = c.count(20, "blocker polygons")?;
    let mut polygons = Vec::with_capacity(count);
    for _ in 0..count {
        let plane = c.plane()?;
        let v = c.u32()? as usize;
        let mut poly = parse_geometry_polygon(c, v)?;
        poly.plane = plane;
        polygons.push(poly);
    }
    Ok(Blockers {
        polygons,
        trailer: c.u32()?,
    })
}

/// "Geometry polygon": u8 v, v points, then the plane.
fn parse_render_polygon(c: &mut Cursor) -> Result<GeometryPolygon> {
    let v = c.u8()? as usize;
    let mut poly = parse_geometry_polygon(c, v)?;
    poly.plane = c.plane()?;
    Ok(poly)
}

fn parse_render_world(c: &mut Cursor, depth: usize) -> Result<RenderWorld> {
    let count = c.count(24 + 4 * 6 + 9, "render blocks")?;
    let mut world = RenderWorld {
        blocks: Vec::with_capacity(count),
        models: Vec::new(),
    };
    for i in 0..count {
        world
            .blocks
            .push(parse_render_block(c).map_err(|e| e.context(&format!("render block {i}")))?);
    }
    for (i, b) in world.blocks.iter().enumerate() {
        if let Some(child) = b.child_blocks().find(|&k| k as usize >= count) {
            return Err(c.err(format!("render block {i}'s child {child} of {count}")));
        }
    }
    let at = c.pos;
    let models = c.count(2 + 8, "render world models")?;
    if models > 0 && depth >= MAX_RENDER_DEPTH {
        return Err(WorldError {
            offset: at,
            message: "render worlds nested too deep".into(),
        });
    }
    for _ in 0..models {
        let name = c.lstring()?;
        let nested = parse_render_world(c, depth + 1).map_err(|e| e.context(&format!("render world {name}")))?;
        world.models.push((name, nested));
    }
    Ok(world)
}

fn parse_section(c: &mut Cursor) -> Result<Section> {
    let mut s = Section {
        textures: [c.lstring()?, c.lstring()?],
        shader: c.u8()?,
        triangle_count: c.u32()?,
        effect: c.lstring()?,
        ..Default::default()
    };
    let at = c.pos;
    s.lightmap_width = c.u32()?;
    s.lightmap_height = c.u32()?;
    s.lightmap_len = c.u32()?;
    let data = c.take(s.lightmap_len as usize)?;
    if !data.is_empty() {
        let texels = (s.lightmap_width as usize)
            .checked_mul(s.lightmap_height as usize)
            .filter(|&n| n * 3 <= MAX_DECODED)
            .ok_or_else(|| WorldError {
                offset: at,
                message: format!("lightmap {} x {} is too large", s.lightmap_width, s.lightmap_height),
            })?;
        s.lightmap = decode_rgb_rle(data, texels).map_err(|m| WorldError {
            offset: at,
            message: format!("lightmap {} x {}: {m}", s.lightmap_width, s.lightmap_height),
        })?;
    }
    Ok(s)
}

fn parse_render_block(c: &mut Cursor) -> Result<RenderBlock> {
    let mut b = RenderBlock {
        center: c.vec3()?,
        half_size: c.vec3()?,
        ..Default::default()
    };
    let n = c.count(2 + 2 + 1 + 4 + 2 + 12, "sections")?;
    b.sections = (0..n)
        .map(|i| parse_section(c).map_err(|e| e.context(&format!("section {i}"))))
        .collect::<Result<_>>()?;

    let at = c.pos;
    let v = c.count(68, "vertices")?;
    if v as u32 > MAX_BLOCK_VERTICES {
        return Err(WorldError {
            offset: at,
            message: format!("{v} vertices (at most {MAX_BLOCK_VERTICES})"),
        });
    }
    b.vertices.reserve(v);
    for _ in 0..v {
        b.vertices.push(RenderVertex {
            pos: c.vec3()?,
            uv0: [c.f32()?, c.f32()?],
            uv1: [c.f32()?, c.f32()?],
            color: c.u32()?,
            normal: c.vec3()?,
            tangent: c.vec3()?,
            binormal: c.vec3()?,
        });
    }

    let at = c.pos;
    let f = c.count(16, "triangles")?;
    let sum: u64 = b.sections.iter().map(|s| s.triangle_count as u64).sum();
    if sum != f as u64 {
        return Err(WorldError {
            offset: at,
            message: format!("{f} triangles, but the sections own {sum}"),
        });
    }
    b.triangles.reserve(f);
    for i in 0..f {
        let at = c.pos;
        let indices = [c.u32()?, c.u32()?, c.u32()?];
        if indices.iter().any(|&k| k as usize >= v) {
            return Err(WorldError {
                offset: at,
                message: format!("triangle {i}: vertex indices {indices:?} of {v}"),
            });
        }
        b.triangles.push(Triangle {
            indices,
            polygon: c.u32()?,
        });
    }

    let n = c.count(1 + 16, "sky portals")?;
    b.sky_portals = (0..n).map(|_| parse_render_polygon(c)).collect::<Result<_>>()?;
    let n = c.count(1 + 16 + 4, "occluders")?;
    b.occluders = (0..n)
        .map(|_| Ok((parse_render_polygon(c)?, c.u32()?)))
        .collect::<Result<_>>()?;

    let n = c.count(2 + 12 + 4 + 4, "light groups")?;
    for _ in 0..n {
        let name = c.lstring()?;
        let color = c.vec3()?;
        let len = c.u32()? as usize;
        let at = c.pos;
        let stream = c.take(len)?;
        let intensities = decode_intensities(stream, v).map_err(|m| WorldError {
            offset: at,
            message: format!("light group {name}'s intensities: {m}"),
        })?;
        let at = c.pos;
        let sections = c.count(4, "light group sections")?;
        if sections != b.sections.len() {
            return Err(WorldError {
                offset: at,
                message: format!(
                    "light group {name} has {sections} sections, its block {}",
                    b.sections.len()
                ),
            });
        }
        let mut per_section = Vec::with_capacity(sections);
        for _ in 0..sections {
            let s = c.count(20, "sub-lightmaps")?;
            let mut subs = Vec::with_capacity(s);
            for _ in 0..s {
                let at = c.pos;
                let (left, top, width, height) = (c.u32()?, c.u32()?, c.u32()?, c.u32()?);
                let len = c.u32()? as usize;
                let data = c.take(len)?;
                let texels = (width as usize)
                    .checked_mul(height as usize)
                    .filter(|&n| n <= MAX_DECODED)
                    .ok_or_else(|| WorldError {
                        offset: at,
                        message: format!("sub-lightmap {width} x {height} is too large"),
                    })?;
                let intensities = decode_sub_lightmap(data, texels).map_err(|m| WorldError {
                    offset: at,
                    message: format!("light group {name}'s sub-lightmap: {m}"),
                })?;
                subs.push(SubLightmap {
                    left,
                    top,
                    width,
                    height,
                    intensities,
                });
            }
            per_section.push(subs);
        }
        b.light_groups.push(BlockLightGroup {
            name,
            color,
            intensities,
            sections: per_section,
        });
    }
    b.child_flags = c.u8()?;
    b.children = [c.u32()?, c.u32()?];
    Ok(b)
}
