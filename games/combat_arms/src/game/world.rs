//! Combat Arms' maps: a world file from the install
//! (`lithtech_formats::world`, specs/combat_arms/world_dat.md) converted
//! into Chimera's `MapData`, the way `cs_source` converts a BSP. A map is
//! `combat_arms:<name>`, the file `worlds*/<name>.dat` in the mounted
//! archives (`mount`); the 33 maps of WORLDS2 and WORLDS3 read without
//! keys, the rest need the V2 key.
//!
//! - Axes and units ("Coordinate conversion"): Z mirrored into a
//!   right-handed Y-up frame, triangle order reversed to keep the front
//!   faces, and world units scaled by movement's `meters_per_unit` (the
//!   same scale the movement and weapons use).
//! - Drawn: the render data only ("What is drawn and what is collision
//!   only"): the main render world and the world models whose object is
//!   visible (`model_look`), one mesh per section texture and blend.
//!   An object's `Visible`, `StartHidden`, `Solid` and `BlendMode`
//!   properties are read as their names say (their meaning belongs to
//!   the object classes' specs, not written yet): volumes such as
//!   `EndlessFall` are invisible, light rays are additive, windows
//!   translucent. Lightmapped surfaces (shaders 4 and 9) take their
//!   lightmap from the shader-2 pass of the same polygon (matched by
//!   polygon and position); the lightmaps are packed into one atlas
//!   (`MapLightmap`) with a one-texel border copied from their edges.
//!   Vertex-lit sections (shaders 1 and 8) get one atlas texel of their
//!   vertices' mean colour. Textures come from the install's `.dtx`
//!   files (`lithtech_formats::dtx`, level 0; `alpharef` cuts out); the
//!   plain maps' world textures are in unkeyed archives. A texture that
//!   doesn't resolve (keyed archives without the V2 key) leaves a flat
//!   colour of its own per name.
//! - Collision: the physics BSP's solid polygons (surface flag 0), the
//!   BSPs of world models whose object is solid (`Solid`, or without it
//!   one of `SOLID_CLASSES`: doors, moving world models, breakables)
//!   where they stand at load, and the collision blockers, as one
//!   triangle mesh.
//! - Props: objects that name a model (`props`: the capture-the-flag
//!   flags, whose model needs the V2 key).
//! - Spawns: the `GameStartPoint` objects that serve the team modes'
//!   family (`SPAWN_GAME_TYPE`, `Team Death Match`: game_modes.md S4,
//!   `qualifies`: unlocked, `Spawn` exactly 1), in the map's order (the
//!   rules' spawn cursors walk it), else the map's first start point;
//!   dropped to the floor below them, facing their yaw; `Teams` JSF is
//!   `Team(1)`, Creed `Team(2)`, `<none>` neither.
//!
//! Known shortcuts (docs/tech-debt.md): textures without detail
//! textures, environment maps, fullbright masks or second textures
//! (dual-texture sections draw texture 0); no
//! sky (the sky portals and the `DemoSkyWorldModel` scene are not drawn;
//! the sky's own world model is left out), no fog or `WorldProperties`;
//! light groups start as baked (their switchable parts aren't
//! modelled); vertex lighting is one colour per section; lightmap
//! coordinates beyond 0..1 (a few, up to 3.02) are clamped per vertex
//! (the engine clamps per texel); world models don't move (doors and
//! breakables stand as placed and block); collision blockers stop shots
//! too (they are player-only in the game); which surfaces collide is our
//! reading of open question 5 (solid bit set: collide); `EndlessFall`
//! and other volumes do nothing. No file hash for the network handshake.

use std::collections::HashMap;

use bevy::prelude::*;
use lithtech_formats::{
    dtx::Dtx,
    world::{self as lt, RenderWorld, World, shader, surface_flags},
};

use crate::{
    core::Team,
    map::{MapAlpha, MapData, MapLightmap, MapMesh, MapTexture},
    mount::Mount,
};

/// The prefix of Combat Arms' map ids.
pub const MAP_PREFIX: &str = "combat_arms:";

/// The game type whose start points a map spawns at: the family of the
/// modes there are (TDM and Elimination Pro, game_modes.md S4).
pub const SPAWN_GAME_TYPE: &str = "Team Death Match";

/// Object classes whose world models collide where they stand
/// ("How common things are expressed": doors, moving geometry,
/// breakables); other world models (volumes such as `EndlessFall`) don't.
pub const SOLID_CLASSES: &[&str] = &[
    "WorldModel",
    "Breakable",
    "RotatingDoor",
    "SlidingDoor",
    "RotatingWorldModel",
    "SlidingWorldModel",
    "SpinningWorldModel",
];

/// Atlas texels between packed lightmaps' borders and its width.
const ATLAS_WIDTH: u32 = 4096;

/// Map `id` (`combat_arms:<name>`, or the bare name) from the install.
pub fn load(id: &str) -> Result<MapData, String> {
    let name = map_name(id);
    let mount = super::INFO.mount().ok_or_else(|| {
        "no Combat Arms install: set [games.combat_arms] path in chimera.local.toml".to_string()
    })?;
    load_from(&mount, &name)
}

/// A map id's file name: lower case, without the game prefix.
pub fn map_name(id: &str) -> String {
    id.strip_prefix(MAP_PREFIX).unwrap_or(id).to_lowercase()
}

/// The world file of map `name` in the mounted archives
/// (`worlds*/<name>.dat`, the first archive in name order).
pub fn find(mount: &Mount, name: &str) -> Option<String> {
    let file = format!("/{}.dat", name.to_lowercase());
    let mut paths: Vec<String> = mount
        .entries()
        .into_iter()
        .map(|(e, _)| e.path)
        .filter(|p| p.starts_with("worlds") && p.ends_with(&file))
        .collect();
    paths.sort();
    paths.into_iter().next()
}

/// Every map in the mounted archives, by name.
pub fn map_names(mount: &Mount) -> Vec<String> {
    let mut names: Vec<String> = mount
        .entries()
        .into_iter()
        .filter_map(|(e, _)| {
            let (dir, file) = e.path.rsplit_once('/')?;
            (dir.starts_with("worlds") && !dir.contains('/')).then_some(())?;
            file.strip_suffix(".dat").map(str::to_string)
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Map `name` from a mounted install.
pub fn load_from(mount: &Mount, name: &str) -> Result<MapData, String> {
    let path = find(mount, name).ok_or_else(|| format!("{name}: no such map (worlds*/{name}.dat)"))?;
    let bytes = mount.read(&path).map_err(|e| format!("{path}: {e}"))?;
    let world = World::parse(&bytes).map_err(|e| format!("{path}: {e}"))?;
    let scale = super::movement::movement_data().meters_per_unit;
    let file = |path: &str| mount.read(path).ok();
    let mut map = convert(&world, name, scale, &file);
    if let Ok(spec) = std::env::var(super::props::DEV_PROP_VAR) {
        super::props::dev_prop(&world, &mut map, &spec, scale, &file);
    }
    Ok(map)
}

/// A world as Chimera's map, `scale` metres per world unit; `file` reads
/// a texture by its normalized path (lower case, `/`), None when missing.
pub fn convert(world: &World, name: &str, scale: f32, file: &dyn Fn(&str) -> Option<Vec<u8>>) -> MapData {
    let to_engine = |p: lt::Vec3| Vec3::from(lt::to_right_handed(p)) * scale;
    let mut map = MapData {
        name: format!("{MAP_PREFIX}{name}"),
        ..default()
    };

    let mut worlds: Vec<(&RenderWorld, MapAlpha)> = vec![(&world.render, MapAlpha::Opaque)];
    worlds.extend(
        world
            .render
            .models
            .iter()
            .filter_map(|(n, w)| Some((w, model_look(world, n)?))),
    );
    let mut atlas = Atlas::default();
    let mut meshes: HashMap<String, (MapMesh, Vec<u32>)> = HashMap::new();
    for (render, alpha) in &worlds {
        for block in &render.blocks {
            add_block(block, *alpha, &mut atlas, &mut meshes, &to_engine);
        }
    }
    let lightmap = atlas.finish();
    let mut meshes: Vec<MapMesh> = meshes
        .into_values()
        .map(|(mut m, rects)| {
            for (uv, rect) in m.lightmap_uvs.iter_mut().zip(rects) {
                *uv = lightmap.uv(rect, *uv);
            }
            m
        })
        .collect();
    meshes.sort_by(|a, b| a.material.cmp(&b.material));
    textures(&mut meshes, &mut map, file);
    map.meshes = meshes;
    map.lightmap = Some(lightmap.map);

    collision(world, &mut map, &to_engine);
    spawns(world, &mut map, &to_engine);
    super::props::add_objects(world, &mut map, scale, file);
    if map.spawns.is_empty() {
        map.warnings.push("no GameStartPoint: spawning at the origin".into());
        map.spawns.push((Vec3::ZERO, None));
    }
    map
}

/// How world model `name` is drawn: None when its object is not visible
/// (`Visible` false or `StartHidden` true) or is the sky scene's
/// (`DemoSkyWorldModel`, not drawn), else its `BlendMode` (`Additive`,
/// `Translucent`, opaque otherwise).
fn model_look(world: &World, name: &str) -> Option<MapAlpha> {
    let Some(o) = world.objects.iter().find(|o| o.name() == Some(name)) else {
        return Some(MapAlpha::Opaque);
    };
    if o.class == "DemoSkyWorldModel" || o.bool("Visible") == Some(false) || o.bool("StartHidden") == Some(true) {
        return None;
    }
    Some(match o.string("BlendMode") {
        Some(b) if b.eq_ignore_ascii_case("Additive") => MapAlpha::Add,
        Some(b) if b.eq_ignore_ascii_case("Translucent") => MapAlpha::Blend,
        _ => MapAlpha::Opaque,
    })
}

/// A lightmap rectangle's place in the atlas: index into `Atlas::rects`
/// and a coordinate in it (0..1, clamped).
type LightmapRef = (u32, [f32; 2]);

/// One render block's drawn sections into the meshes (by texture; each
/// with its vertices' lightmap rectangles, whose coordinates the mesh's
/// `lightmap_uvs` hold until the atlas is laid out).
fn add_block(
    block: &lt::RenderBlock,
    alpha: MapAlpha,
    atlas: &mut Atlas,
    meshes: &mut HashMap<String, (MapMesh, Vec<u32>)>,
    to_engine: &impl Fn(lt::Vec3) -> Vec3,
) {
    // The lightmap pass's coordinates, by polygon and position.
    let mut lightmap_at: HashMap<(u32, [u32; 3]), LightmapRef> = HashMap::new();
    for (s, tris) in block.section_triangles() {
        if s.shader != shader::LIGHTMAP || s.lightmap.is_empty() {
            continue;
        }
        let rect = atlas.add(s.lightmap_width, s.lightmap_height, srgb_texels(&s.lightmap));
        for t in tris {
            for &i in &t.indices {
                let v = &block.vertices[i as usize];
                lightmap_at.insert((t.polygon, v.pos.map(f32::to_bits)), (rect, v.uv0));
            }
        }
    }
    for (s, tris) in block.section_triangles() {
        let lightmapped = match s.shader {
            shader::LIGHTMAP_TEXTURE | shader::LIGHTMAP_DUAL_TEXTURE => true,
            shader::GOURAUD | shader::DUAL_TEXTURE => false,
            _ => continue,
        };
        // Vertex lighting: the section's mean colour as one texel.
        let flat = (!lightmapped).then(|| {
            let mut sum = [0.0f32; 3];
            let mut n = 0.0f32;
            for t in tris {
                for &i in &t.indices {
                    let [r, g, b, _] = block.vertices[i as usize].rgba();
                    sum = [sum[0] + r as f32, sum[1] + g as f32, sum[2] + b as f32];
                    n += 1.0;
                }
            }
            let mean = sum.map(|c| srgb_to_linear(c / n.max(1.0) / 255.0));
            (atlas.add_flat(mean), [0.5, 0.5])
        });
        let name = s.textures[0].to_lowercase();
        let (mesh, rects) = meshes.entry(format!("{name}|{alpha:?}")).or_insert_with(|| {
            let mesh = MapMesh {
                material: s.textures[0].clone(),
                color: flat_color(&name),
                alpha,
                ..default()
            };
            (mesh, Vec::new())
        });
        let mut index: HashMap<(u32, u32), u32> = HashMap::new();
        for t in tris {
            let mut tri = [0u32; 3];
            for (k, &i) in t.indices.iter().enumerate() {
                tri[k] = *index.entry((i, t.polygon)).or_insert_with(|| {
                    let v = &block.vertices[i as usize];
                    let (rect, uv) = flat
                        .or_else(|| lightmap_at.get(&(t.polygon, v.pos.map(f32::to_bits))).copied())
                        .unwrap_or_else(|| (atlas.add_flat([1.0; 3]), [0.5, 0.5]));
                    mesh.positions.push(to_engine(v.pos).to_array());
                    mesh.normals.push(lt::to_right_handed(v.normal));
                    mesh.uvs.push(v.uv0);
                    mesh.lightmap_uvs.push(uv);
                    rects.push(rect);
                    (mesh.positions.len() - 1) as u32
                });
            }
            // Mirroring reverses the winding: swap two corners.
            mesh.indices.extend([tri[0], tri[2], tri[1]]);
        }
    }
}

/// Each mesh's texture from the install (`.dtx`, level 0; an alpha-tested
/// one cut out at its `alpharef`); a name that doesn't resolve keeps the
/// mesh's flat colour, with a warning.
pub(crate) fn textures(meshes: &mut [MapMesh], map: &mut MapData, file: &dyn Fn(&str) -> Option<Vec<u8>>) {
    let mut missing = 0;
    for m in meshes {
        if m.material.is_empty() || m.material.eq_ignore_ascii_case("default") {
            continue;
        }
        let path = m.material.replace('\\', "/").to_lowercase();
        let Some(dtx) = file(&path).and_then(|b| Dtx::parse(b).ok()) else {
            missing += 1;
            continue;
        };
        if let (Some(r), MapAlpha::Opaque) = (dtx.alpha_ref(), m.alpha) {
            m.alpha = MapAlpha::Mask(r as f32 / 255.0);
        }
        map.textures.push(MapTexture {
            name: m.material.clone(),
            srgb: true,
            mips: Vec::new(),
            width: dtx.width,
            height: dtx.height,
            rgba8: dtx.rgba(0),
        });
        m.texture = Some(map.textures.len() - 1);
        m.color = [255; 3];
    }
    if missing > 0 {
        map.warnings
            .push(format!("{missing} textures missing or unreadable (keyed archives need the V2 key): flat colours"));
    }
}

/// A muted colour of its own per texture name.
fn flat_color(name: &str) -> [u8; 3] {
    let h = lt::name_hash(name.as_bytes());
    let hue = (h % 360) as f32;
    let c = Color::hsl(hue, 0.18, 0.62).to_srgba();
    [c.red, c.green, c.blue].map(|x| (x * 255.0).round() as u8)
}

fn srgb_to_linear(c: f32) -> f32 {
    Color::srgb(c, c, c).to_linear().red
}

/// A lightmap's RGB bytes as linear texels (the bytes are gamma-encoded:
/// the game modulates the texture by them as stored).
fn srgb_texels(rgb: &[u8]) -> Vec<[f32; 3]> {
    let lut: Vec<f32> = (0..=255).map(|b| srgb_to_linear(b as f32 / 255.0)).collect();
    rgb.as_chunks::<3>().0.iter().map(|t| t.map(|b| lut[b as usize])).collect()
}

/// Lightmaps gathered for one atlas.
#[derive(Default)]
struct Atlas {
    /// (width, height, texels) per rectangle.
    rects: Vec<(u32, u32, Vec<[f32; 3]>)>,
    flats: HashMap<[u32; 3], u32>,
}

impl Atlas {
    fn add(&mut self, width: u32, height: u32, texels: Vec<[f32; 3]>) -> u32 {
        self.rects.push((width, height, texels));
        (self.rects.len() - 1) as u32
    }

    /// One colour as a 2 x 2 rectangle (shared by equal colours).
    fn add_flat(&mut self, rgb: [f32; 3]) -> u32 {
        if let Some(&r) = self.flats.get(&rgb.map(f32::to_bits)) {
            return r;
        }
        let r = self.add(2, 2, vec![rgb; 4]);
        self.flats.insert(rgb.map(f32::to_bits), r);
        r
    }

    /// Lay the rectangles out in shelves (tallest first), each with a
    /// one-texel border copied from its edge.
    fn finish(self) -> LaidOut {
        let mut order: Vec<usize> = (0..self.rects.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse((self.rects[i].1, self.rects[i].0)));
        let mut places = vec![(0u32, 0u32); self.rects.len()];
        let (mut x, mut y, mut shelf) = (0u32, 0u32, 0u32);
        for &i in &order {
            let (w, h) = (self.rects[i].0 + 2, self.rects[i].1 + 2);
            if x + w > ATLAS_WIDTH {
                (x, y, shelf) = (0, y + shelf, 0);
            }
            places[i] = (x, y);
            x += w;
            shelf = shelf.max(h);
        }
        let width = ATLAS_WIDTH;
        let height = (y + shelf).max(1).next_multiple_of(4);
        let mut rgb = vec![[1.0f32; 3]; (width * height) as usize];
        for (i, (w, h, texels)) in self.rects.iter().enumerate() {
            let (px, py) = places[i];
            for ty in 0..h + 2 {
                for tx in 0..w + 2 {
                    let sx = tx.saturating_sub(1).min(w - 1);
                    let sy = ty.saturating_sub(1).min(h - 1);
                    rgb[((py + ty) * width + px + tx) as usize] = texels[(sy * w + sx) as usize];
                }
            }
        }
        LaidOut {
            places,
            sizes: self.rects.iter().map(|r| (r.0, r.1)).collect(),
            map: MapLightmap {
                width,
                height,
                rgb,
                ..default()
            },
        }
    }
}

struct LaidOut {
    places: Vec<(u32, u32)>,
    sizes: Vec<(u32, u32)>,
    map: MapLightmap,
}

impl LaidOut {
    /// A coordinate in rectangle `rect` (clamped to 0..1) as atlas
    /// coordinates.
    fn uv(&self, rect: u32, uv: [f32; 2]) -> [f32; 2] {
        let (x, y) = self.places[rect as usize];
        let (w, h) = self.sizes[rect as usize];
        let [u, v] = uv.map(|c| c.clamp(0.0, 1.0));
        [
            (x as f32 + 1.0 + u * w as f32) / self.map.width as f32,
            (y as f32 + 1.0 + v * h as f32) / self.map.height as f32,
        ]
    }
}

/// The collision mesh: solid polygons of the physics BSP and of solid
/// objects' world models, and the collision blockers.
fn collision(world: &World, map: &mut MapData, to_engine: &impl Fn(lt::Vec3) -> Vec3) {
    let solid_models: Vec<&str> = world
        .objects
        .iter()
        .filter(|o| o.bool("Solid").unwrap_or_else(|| SOLID_CLASSES.contains(&o.class.as_str())))
        .filter_map(|o| o.name())
        .collect();
    let (positions, indices) = (&mut map.collision_positions, &mut map.collision_indices);
    let mut fan = |points: &mut dyn Iterator<Item = lt::Vec3>| {
        let base = positions.len() as u32;
        let corners: Vec<Vec3> = points.map(to_engine).collect();
        for k in 1..corners.len().saturating_sub(1) {
            // Mirrored: the reversed order keeps the front face.
            let [a, b, c] = [0, k + 1, k];
            if (corners[b] - corners[a]).cross(corners[c] - corners[a]).length_squared() > 1e-12 {
                indices.push([base + a as u32, base + b as u32, base + c as u32]);
            }
        }
        positions.extend(corners.iter().map(|p| p.to_array()));
    };
    for m in &world.models {
        if !(m.is_physics() || solid_models.contains(&m.name.as_str())) {
            continue;
        }
        for i in 0..m.polygons.len() {
            if m.polygon_surface(i).flags & surface_flags::SOLID != 0 {
                fan(&mut m.polygon_vertices(i));
            }
        }
    }
    for p in &world.blockers.polygons {
        fan(&mut p.vertices.iter().copied());
    }
}

/// Whether a start point serves the versus modes' `family`
/// (game_modes.md S4): its `GameType`, not `Locked`, `Spawn` exactly 1.
pub fn qualifies(o: &lt::Object, family: &str) -> bool {
    o.string("GameType") == Some(family) && o.bool("Locked") != Some(true) && o.int("Spawn") == Some(1)
}

/// The start points of `SPAWN_GAME_TYPE` that qualify (`qualifies`), in
/// the map's order, feet on the floor below them; none: the map's first
/// start point (S4).
fn spawns(world: &World, map: &mut MapData, to_engine: &impl Fn(lt::Vec3) -> Vec3) {
    let all: Vec<&lt::Object> = world.objects_of("GameStartPoint").map(|(_, o)| o).collect();
    let mut starts: Vec<&lt::Object> = all.iter().copied().filter(|o| qualifies(o, SPAWN_GAME_TYPE)).collect();
    if starts.is_empty() {
        starts = all.into_iter().take(1).collect();
    }
    let floor = Floor::new(map);
    let (mut spawns, mut yaws) = (Vec::new(), Vec::new());
    for o in starts {
        let Some(pos) = o.pos() else { continue };
        let at = to_engine(pos);
        let feet = floor.below(at).unwrap_or(at);
        let team = match o.string("Teams") {
            Some(t) if t.eq_ignore_ascii_case("JSF") => Some(Team(1)),
            Some(t) if t.eq_ignore_ascii_case("Creed") => Some(Team(2)),
            _ => None,
        };
        // The facing is (sin yaw, 0, cos yaw) in file axes, so (sin yaw,
        // 0, -cos yaw) here; an `Intent` yaw turns the other way.
        let yaw = o.rotation("Rotation").map_or(0.0, |r| r[1]);
        spawns.push((feet, team));
        yaws.push(-yaw);
    }
    map.spawns = spawns;
    map.spawn_yaws = yaws;
}

/// Downward rays against the collision triangles.
struct Floor<'a> {
    positions: &'a [[f32; 3]],
    triangles: Vec<([u32; 3], Vec2, Vec2)>,
}

impl<'a> Floor<'a> {
    fn new(map: &'a MapData) -> Self {
        let p = |i: u32| Vec3::from(map.collision_positions[i as usize]);
        let triangles = map
            .collision_indices
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(p);
                (*t, a.xz().min(b.xz()).min(c.xz()), a.xz().max(b.xz()).max(c.xz()))
            })
            .collect();
        Self {
            positions: &map.collision_positions,
            triangles,
        }
    }

    /// The highest surface point at or below `at` (within 4 m).
    fn below(&self, at: Vec3) -> Option<Vec3> {
        let p = |i: u32| Vec3::from(self.positions[i as usize]);
        let mut best: Option<f32> = None;
        for (t, lo, hi) in &self.triangles {
            if at.x < lo.x || at.x > hi.x || at.z < lo.y || at.z > hi.y {
                continue;
            }
            let [a, b, c] = t.map(p);
            let n = (b - a).cross(c - a);
            if n.y.abs() < 1e-6 {
                continue;
            }
            // The plane's height at (x, z), then inside the triangle.
            let y = a.y - (n.x * (at.x - a.x) + n.z * (at.z - a.z)) / n.y;
            if y > at.y + 0.01 || y < at.y - 4.0 {
                continue;
            }
            let q = Vec3::new(at.x, y, at.z);
            let inside = [(a, b), (b, c), (c, a)]
                .iter()
                .map(|(u, v)| (*v - *u).cross(q - *u).dot(n))
                .all(|s| s >= -1e-6);
            if inside && best.is_none_or(|h| y > h) {
                best = Some(y);
            }
        }
        best.map(|y| Vec3::new(at.x, y, at.z))
    }
}
