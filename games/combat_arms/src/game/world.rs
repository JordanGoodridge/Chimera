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
//!   visible (`model_look`), one mesh per section texture and blend;
//!   sections with shader codes other than 1, 2, 4, 8 and 9 never (Q17).
//!   An object's `Visible`, `StartHidden`, `Solid`, `Rayhit` and
//!   `BlendMode` properties are read as their names say: volumes such as
//!   `EndlessFall` are invisible, light rays are additive, windows
//!   translucent.
//! - Lighting ("Lightmaps", Q7, Q9, Q10, Q12): lightmapped surfaces
//!   (shaders 4 and 9) take their lightmap from the shader-2 pass of the
//!   same polygon (matched by polygon and position), packed into one atlas
//!   (`MapLightmap`) with a one-texel border copied from their edges; a
//!   triangle whose lightmap coordinates leave 0..1 is cut where they
//!   cross 0 and 1 and the parts outside take the edge (so it clamps per
//!   pixel, as the engine's sampler does). Vertex-lit sections (shaders 1
//!   and 8) light each vertex by its own colour: each triangle gets a
//!   2 x 2 atlas rectangle whose bilinear filtering is exactly the
//!   vertices' linear blend. Light groups start as the map's `LightGroup`
//!   objects say (`light_group_colours`: `StartColor` when `StartOn`,
//!   black when not, the stored colour without an object) and add
//!   min(255, base + ⌊colour × x⌋) to the lightmaps and vertex colours they
//!   cover (`add_light`). The light is the game's fixed-function multiply:
//!   2 (`Saturate`) × texture × light × `LightmapTFactor`² (`light_value`,
//!   the stored bytes as sRGB), with the map's look untonemapped (clamped,
//!   as the game's output).
//! - Textures come from the install's `.dtx` files (`lithtech_formats::
//!   dtx`, the level the shipped "DetailLevel" loads: `skipped_levels`;
//!   `alpharef` cuts out); the plain maps' world textures are in unkeyed
//!   archives. A texture that doesn't resolve (keyed archives without the
//!   V2 key) leaves a flat colour of its own per name.
//! - Sky (Q11): the last `DemoSkyWorldModel`'s world model is drawn by
//!   Chimera's sky camera (`MapMesh::skybox`, `MapSkyCamera`): it follows
//!   the player through the object's inner box (its position ±
//!   `SkyDims` × `InnerPercent`) as the player moves through the world's
//!   extents, behind the world. Fog (`WorldProperties`): `FogEnable`,
//!   `FogColor`, `FogNearZ`, `FogFarZ` as linear depth fog (`MapFog::
//!   linear`), the sky's with `SkyFogNearZ`/`SkyFogFarZ`.
//! - Collision (Q5, Q14): what players collide with
//!   (`collision_indices`): the physics BSP's polygons with the solid bit
//!   (the invisible bit plays no part) and those of the world models
//!   whose object is solid (`Solid`, or without it one of `SOLID_CLASSES`)
//!   where they stand at load. Collision sets (`MapCollisionSet`): the
//!   collision blockers, players only; the polygons without the solid bit
//!   (non-solid detail) and the world models that are ray-hit but not
//!   solid, traces only (shots, sight, cover, grenades).
//! - Volumes (`volumes`): `Water`, `EndlessFall` and the other volume
//!   classes go to the map's entities (class, properties, the world
//!   model's faces as one hull) for `volumes::CaVolume`.
//! - Props: objects that name a model (`props`: the capture-the-flag
//!   flags, whose model needs the V2 key).
//! - View models: the weapons' first-person models their data names
//!   (`view_model`), loaded with every map from the install (`load_from`).
//! - Spawns: `GameStartPoint` objects of one game type (`SPAWN_GAME_TYPE`
//!   when the map has it, else all), dropped to the floor below them,
//!   facing their yaw; `Teams` JSF is `Team(1)`, Creed `Team(2)`.
//! - No map file hash (Q15): the game checks none, a client loads its own
//!   copy by name, so the network handshake compares only the name.
//!
//! Known shortcuts (docs/tech-debt.md): second textures (dual-texture
//! sections draw texture 0; detail textures, environment maps and
//! fullbright masks aren't drawn: Chimera's world material has no second
//! UV set or LithTech's signed-add blend; the links are read,
//! `lithtech_formats::dtx::Dtx::link`). The sky world model is drawn with
//! depth (the game draws it in order without), from one scale for all
//! three axes (the game's inner box scales each), over the whole screen
//! (the game only inside the sky portals' rectangle); `FarZ`, the sky's
//! far plane and fog-disable flag, panning skies and volumetric fog aren't
//! read. Light groups stay as they start (no map scripts to switch them;
//! the light grid's fix-ups aren't applied). World models don't move
//! (doors and breakables stand as placed and block); volumes are their
//! world models' face planes.

use std::collections::HashMap;

use bevy::prelude::*;
use lithtech_formats::{
    dtx::{DETAIL_LEVEL, Dtx},
    world::{self as lt, Object, RenderWorld, World, shader, surface_flags},
};

use crate::{
    core::Team,
    map::{
        MapAlpha, MapCollisionSet, MapData, MapEntity, MapFog, MapHull, MapLightmap, MapMesh, MapSkyCamera,
        MapTexture, entities::engine_to_entity,
    },
    mount::Mount,
};

/// The prefix of Combat Arms' map ids.
pub const MAP_PREFIX: &str = "combat_arms:";

/// The game type whose start points a map spawns at, when it has any
/// (the map's other modes' points otherwise; game modes are item 13's).
pub const SPAWN_GAME_TYPE: &str = "Team Death Match";

/// Object classes whose world models collide where they stand without a
/// `Solid` property ("How common things are expressed": doors, moving
/// geometry, breakables); other world models (volumes such as
/// `EndlessFall`) don't.
pub const SOLID_CLASSES: &[&str] = &[
    "WorldModel",
    "Breakable",
    "RotatingDoor",
    "SlidingDoor",
    "RotatingWorldModel",
    "SlidingWorldModel",
    "SpinningWorldModel",
];

/// The console's `Saturate` 1 (the default): the lighting multiply is
/// doubled ("Lightmaps", Q7).
pub const SATURATE: f32 = 2.0;

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
    map.warnings.extend(world.warnings.iter().map(|w| format!("{path}: {w}")));
    super::view_model::add_view_models(&mut map, &file);
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
    // The game's output is clamped, not tonemapped.
    map.look.tonemapping = false;

    let properties = world.objects_of("WorldProperties").map(|(_, o)| o).last();
    let t_factor = properties
        .and_then(|o| colour(o, "LightmapTFactor"))
        .unwrap_or([1.0; 3]);
    let groups = light_group_colours(world);
    let sky = sky_object(world);
    let mut worlds: Vec<(&RenderWorld, MapAlpha, bool)> = vec![(&world.render, MapAlpha::Opaque, false)];
    worlds.extend(world.render.models.iter().filter_map(|(n, w)| {
        if sky.and_then(Object::name) == Some(n.as_str()) {
            return Some((w, MapAlpha::Opaque, true));
        }
        Some((w, model_look(world, n)?, false))
    }));
    let mut atlas = Atlas::default();
    let mut meshes: HashMap<String, (MapMesh, Vec<u32>)> = HashMap::new();
    for (render, alpha, skybox) in &worlds {
        for block in &render.blocks {
            let lit = Lighting {
                groups: &groups,
                t_factor,
            };
            add_block(block, *alpha, *skybox, &lit, &mut atlas, &mut meshes, &to_engine);
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
    meshes.sort_by(|a, b| (a.skybox, &a.material).cmp(&(b.skybox, &b.material)));
    textures(&mut meshes, &mut map, file);
    map.meshes = meshes;
    map.lightmap = Some(lightmap.map);

    map.fog = properties.and_then(|o| world_fog(o, scale, false));
    map.sky_camera = sky.and_then(|o| sky_camera(world, o, properties, scale));
    collision(world, &mut map, &to_engine);
    volumes(world, &mut map, scale);
    spawns(world, &mut map, &to_engine);
    super::props::add_objects(world, &mut map, scale, file);
    if map.spawns.is_empty() {
        map.warnings.push("no GameStartPoint: spawning at the origin".into());
        map.spawns.push((Vec3::ZERO, None));
    }
    map
}

/// How world model `name` is drawn: None when its object is not visible
/// (`Visible` false or `StartHidden` true), else its `BlendMode`
/// (`Additive`, `Translucent`, opaque otherwise).
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

/// A colour property as 0..1 per channel (stored 0..255).
fn colour(o: &Object, key: &str) -> Option<[f32; 3]> {
    match o.get(key)? {
        lt::Value::Color(c) | lt::Value::Vector(c) => Some(c.map(|x| x / 255.0)),
        _ => None,
    }
}

/// Each light group's colour when the map has loaded (Q12): its
/// `LightGroup` object's `StartColor`/255 if `StartOn` (true unless set),
/// black if not; a group without an object keeps the colour stored with
/// it (the second argument).
pub fn light_group_colours(world: &World) -> impl Fn(&str, [f32; 3]) -> [f32; 3] + '_ {
    move |name: &str, stored: [f32; 3]| {
        let Some(o) = world
            .objects_of("LightGroup")
            .map(|(_, o)| o)
            .find(|o| o.name() == Some(name))
        else {
            return stored;
        };
        if o.bool("StartOn") == Some(false) {
            return [0.0; 3];
        }
        colour(o, "StartColor").unwrap_or([1.0; 3])
    }
}

/// A light group's addition to one channel: min(255, base + ⌊colour × x⌋).
pub fn add_light(base: u8, colour: f32, x: u8) -> u8 {
    (base as u32 + (colour * x as f32).floor().max(0.0) as u32).min(255) as u8
}

/// The light a stored lightmap or vertex-colour byte gives, linear, for a
/// texture multiplied by it: the game's 2 x texture x light x T²
/// (`Saturate`, the `LightmapTFactor` channel T on both passes), the
/// product taken as sRGB.
pub fn light_value(byte: u8, t: f32) -> f32 {
    let c = SATURATE * byte as f32 / 255.0 * t * t;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// How light groups and the lightmap factor light a block.
struct Lighting<'a> {
    groups: &'a dyn Fn(&str, [f32; 3]) -> [f32; 3],
    t_factor: [f32; 3],
}

impl Lighting<'_> {
    /// Each of the block's light groups with its colour, if on (a colour
    /// that is 0 in all three channels at 8 bits adds nothing).
    fn on<'b>(&self, block: &'b lt::RenderBlock) -> Vec<(&'b lt::BlockLightGroup, [f32; 3])> {
        block
            .light_groups
            .iter()
            .map(|g| (g, (self.groups)(&g.name, g.color)))
            .filter(|(_, c)| c.iter().any(|x| (x * 255.0) as u32 > 0))
            .collect()
    }

    fn texel(&self, rgb: [u8; 3]) -> [f32; 3] {
        std::array::from_fn(|k| light_value(rgb[k], self.t_factor[k]))
    }
}

/// A lightmap rectangle's place in the atlas: index into `Atlas::rects`
/// and a coordinate in it (0..1).
type LightmapRef = (u32, [f32; 2]);

/// A corner of a drawn triangle (file axes).
#[derive(Clone, Copy, Debug)]
struct Corner {
    pos: lt::Vec3,
    normal: lt::Vec3,
    uv: [f32; 2],
    lm: [f32; 2],
}

impl Corner {
    fn lerp(a: &Corner, b: &Corner, t: f32) -> Corner {
        let l3 = |x: lt::Vec3, y: lt::Vec3| std::array::from_fn(|k| x[k] + (y[k] - x[k]) * t);
        let l2 = |x: [f32; 2], y: [f32; 2]| std::array::from_fn(|k| x[k] + (y[k] - x[k]) * t);
        Corner {
            pos: l3(a.pos, b.pos),
            normal: l3(a.normal, b.normal),
            uv: l2(a.uv, b.uv),
            lm: l2(a.lm, b.lm),
        }
    }
}

/// A convex polygon cut by the line lm[axis] = at: (the part at or below,
/// the part at or above), each empty when it has nothing.
fn cut(poly: &[Corner], axis: usize, at: f32) -> (Vec<Corner>, Vec<Corner>) {
    let (mut below, mut above) = (Vec::new(), Vec::new());
    for i in 0..poly.len() {
        let (a, b) = (&poly[i], &poly[(i + 1) % poly.len()]);
        let (da, db) = (a.lm[axis] - at, b.lm[axis] - at);
        if da <= 0.0 {
            below.push(*a);
        }
        if da >= 0.0 {
            above.push(*a);
        }
        if (da < 0.0 && db > 0.0) || (da > 0.0 && db < 0.0) {
            let mid = Corner::lerp(a, b, da / (da - db));
            below.push(mid);
            above.push(mid);
        }
    }
    let keep = |p: Vec<Corner>| if p.len() >= 3 { p } else { Vec::new() };
    (keep(below), keep(above))
}

/// A triangle cut where its lightmap coordinates cross 0 and 1, each part
/// clamped to 0..1: interpolating a part's clamped corners gives the
/// clamp of the interpolated coordinate (the sampler's per-pixel clamp).
fn clamp_cut(tri: [Corner; 3]) -> Vec<Vec<Corner>> {
    let mut parts = vec![tri.to_vec()];
    for (axis, at) in [(0, 0.0), (0, 1.0), (1, 0.0), (1, 1.0)] {
        parts = parts
            .into_iter()
            .flat_map(|p| {
                let (a, b) = cut(&p, axis, at);
                [a, b]
            })
            .filter(|p| !p.is_empty())
            .collect();
    }
    for p in &mut parts {
        for c in p.iter_mut() {
            c.lm = c.lm.map(|x| x.clamp(0.0, 1.0));
        }
    }
    parts
}

/// One render block's drawn sections into the meshes (by texture; each
/// with its vertices' lightmap rectangles, whose coordinates the mesh's
/// `lightmap_uvs` hold until the atlas is laid out).
fn add_block(
    block: &lt::RenderBlock,
    alpha: MapAlpha,
    skybox: bool,
    lit: &Lighting,
    atlas: &mut Atlas,
    meshes: &mut HashMap<String, (MapMesh, Vec<u32>)>,
    to_engine: &impl Fn(lt::Vec3) -> Vec3,
) {
    let groups = lit.on(block);
    // Vertex colours with the light groups that are on.
    let colours: Vec<[u8; 3]> = block
        .vertices
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let [r, g, b, _] = v.rgba();
            let mut c = [r, g, b];
            for (group, colour) in &groups {
                let x = group.intensities.get(i).copied().unwrap_or(0);
                c = std::array::from_fn(|k| add_light(c[k], colour[k], x));
            }
            c
        })
        .collect();
    // The lightmap pass's coordinates, by polygon and position.
    let mut lightmap_at: HashMap<(u32, [u32; 3]), LightmapRef> = HashMap::new();
    for (si, (s, tris)) in block.section_triangles().enumerate() {
        if s.shader != shader::LIGHTMAP || s.lightmap.is_empty() {
            continue;
        }
        let mut rgb = s.lightmap.clone();
        let w = s.lightmap_width;
        for (group, colour) in &groups {
            for sub in group.sections.get(si).into_iter().flatten() {
                for y in 0..sub.height {
                    for x in 0..sub.width {
                        let (tx, ty) = (sub.left + x, sub.top + y);
                        let Some(&i) = sub.intensities.get((y * sub.width + x) as usize) else { continue };
                        if tx >= w || ty >= s.lightmap_height {
                            continue;
                        }
                        let at = ((ty * w + tx) * 3) as usize;
                        for k in 0..3 {
                            rgb[at + k] = add_light(rgb[at + k], colour[k], i);
                        }
                    }
                }
            }
        }
        let texels = rgb.as_chunks::<3>().0.iter().map(|t| lit.texel(*t)).collect();
        let rect = atlas.add(s.lightmap_width, s.lightmap_height, texels);
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
            // The lightmap pass, sky portals, occluders, and codes the
            // engine treats as invalid: not drawn.
            _ => continue,
        };
        let name = s.textures[0].to_lowercase();
        let key = format!("{name}|{alpha:?}|{skybox}");
        let (mesh, rects) = meshes.entry(key).or_insert_with(|| {
            let mesh = MapMesh {
                material: s.textures[0].clone(),
                color: flat_color(&name),
                alpha,
                skybox,
                ..default()
            };
            (mesh, Vec::new())
        });
        let push = |mesh: &mut MapMesh, rects: &mut Vec<u32>, c: &Corner, rect: u32| {
            mesh.positions.push(to_engine(c.pos).to_array());
            mesh.normals.push(Vec3::from(lt::to_right_handed(c.normal)).normalize_or_zero().to_array());
            mesh.uvs.push(c.uv);
            mesh.lightmap_uvs.push(c.lm);
            rects.push(rect);
            (mesh.positions.len() - 1) as u32
        };
        let mut shared: HashMap<(u32, u32), u32> = HashMap::new();
        for t in tris {
            let corners = t.indices.map(|i| {
                let v = &block.vertices[i as usize];
                Corner {
                    pos: v.pos,
                    normal: v.normal,
                    uv: v.uv0,
                    lm: [0.5, 0.5],
                }
            });
            if !lightmapped {
                // Vertex lighting: a 2 x 2 rectangle whose bilinear blend
                // over the triangle is the corners' linear one.
                let [l0, l1, l2] = t.indices.map(|i| Vec3::from(lit.texel(colours[i as usize])));
                let (rect, lms) = if l0 == l1 && l1 == l2 {
                    (atlas.add_flat(l0.to_array()), [[0.5, 0.5]; 3])
                } else {
                    let l3 = l1 + l2 - l0;
                    let rect = atlas.add(2, 2, vec![l0.to_array(), l1.to_array(), l2.to_array(), l3.to_array()]);
                    (rect, [[0.25, 0.25], [0.75, 0.25], [0.25, 0.75]])
                };
                let mut tri = [0u32; 3];
                for k in 0..3 {
                    let c = Corner { lm: lms[k], ..corners[k] };
                    tri[k] = push(mesh, rects, &c, rect);
                }
                // Mirroring reverses the winding: swap two corners.
                mesh.indices.extend([tri[0], tri[2], tri[1]]);
                continue;
            }
            let found = t.indices.map(|i| {
                let v = &block.vertices[i as usize];
                lightmap_at.get(&(t.polygon, v.pos.map(f32::to_bits))).copied()
            });
            let rect = match found {
                [Some(a), Some(b), Some(c)] if a.0 == b.0 && b.0 == c.0 => Some(a.0),
                _ => None,
            };
            let outside = found.iter().flatten().any(|(_, uv)| uv.iter().any(|x| !(0.0..=1.0).contains(x)));
            if let (Some(rect), true) = (rect, outside) {
                // Cut where the coordinates leave 0..1 (the per-pixel
                // clamp).
                let corners: [Corner; 3] = std::array::from_fn(|k| Corner {
                    lm: found[k].unwrap().1,
                    ..corners[k]
                });
                for part in clamp_cut(corners) {
                    let ids: Vec<u32> = part.iter().map(|c| push(mesh, rects, c, rect)).collect();
                    for k in 1..ids.len() - 1 {
                        mesh.indices.extend([ids[0], ids[k + 1], ids[k]]);
                    }
                }
                continue;
            }
            let mut tri = [0u32; 3];
            for (k, &i) in t.indices.iter().enumerate() {
                tri[k] = *shared.entry((i, t.polygon)).or_insert_with(|| {
                    let (rect, lm) = found[k].unwrap_or_else(|| (atlas.add_flat([1.0; 3]), [0.5, 0.5]));
                    let c = Corner {
                        lm: lm.map(|x| x.clamp(0.0, 1.0)),
                        ..corners[k]
                    };
                    push(mesh, rects, &c, rect)
                });
            }
            // Mirroring reverses the winding: swap two corners.
            mesh.indices.extend([tri[0], tri[2], tri[1]]);
        }
    }
}

/// Each mesh's texture from the install (`.dtx`, the level the shipped
/// "DetailLevel" loads for its group; an alpha-tested one cut out at its
/// `alpharef`); a name that doesn't resolve keeps the mesh's flat colour,
/// with a warning.
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
        let level = dtx.skipped_levels(DETAIL_LEVEL, 0);
        let (width, height) = dtx.level_dims(level);
        map.textures.push(MapTexture {
            name: m.material.clone(),
            srgb: true,
            mips: Vec::new(),
            width,
            height,
            rgba8: dtx.rgba(level),
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

/// The sky object: the last `DemoSkyWorldModel` (they all take sky slot
/// 0, so the last one made is the sky).
fn sky_object(world: &World) -> Option<&Object> {
    world.objects_of("DemoSkyWorldModel").map(|(_, o)| o).last()
}

/// The sky camera (Q11): its position follows the player's through the
/// sky object's inner box V (position ± `SkyDims` × `InnerPercent`) as the
/// player moves through the world's extents E, c = V_min + (V_max −
/// V_min) ⊙ (p − E_min) / (E_max − E_min); Chimera's sky camera is
/// `origin + p / scale`, one scale for all axes (their mean). Its fog is
/// the world's with the sky's distances.
fn sky_camera(world: &World, sky: &Object, properties: Option<&Object>, scale: f32) -> Option<MapSkyCamera> {
    let pos = Vec3::from(sky.pos()?);
    let dims = sky.vector("SkyDims").map_or(Vec3::ZERO, Vec3::from);
    let inner = Vec3::new(
        sky.real("InnerPercentX").unwrap_or(0.0),
        sky.real("InnerPercentY").unwrap_or(0.0),
        sky.real("InnerPercentZ").unwrap_or(0.0),
    );
    let half_v = dims * inner;
    let (e_min, e_max) = (Vec3::from(world.extents_min), Vec3::from(world.extents_max));
    let ratios = (e_max - e_min) / (2.0 * half_v).max(Vec3::splat(1e-6));
    let sky_scale = ((ratios.x + ratios.y + ratios.z) / 3.0).min(1e6);
    let engine = |p: Vec3| Vec3::from(lt::to_right_handed(p.to_array())) * scale;
    let e_centre = (e_min + e_max) / 2.0;
    let fog = properties.and_then(|o| world_fog(o, scale, true)).map(|f| MapFog {
        // `material_fog` divides by the scale: the sky's own distances.
        start: f.start * sky_scale,
        end: f.end * sky_scale,
        ..f
    });
    Some(MapSkyCamera {
        origin: engine(pos) - engine(e_centre) / sky_scale,
        scale: sky_scale,
        fog,
    })
}

/// The world properties' fog (`FogEnable`, true unless set; `FogColor`
/// (127, 127, 127); `FogNearZ` 1, `FogFarZ` 5000 u, or for the sky
/// `SkyFogNearZ` 100, `SkyFogFarZ` 1000), linear in depth.
fn world_fog(o: &Object, scale: f32, sky: bool) -> Option<MapFog> {
    if o.bool("FogEnable") == Some(false) {
        return None;
    }
    let (near, far) = if sky {
        (
            o.real("SkyFogNearZ").unwrap_or(100.0),
            o.real("SkyFogFarZ").unwrap_or(1000.0),
        )
    } else {
        (o.real("FogNearZ").unwrap_or(1.0), o.real("FogFarZ").unwrap_or(5000.0))
    };
    Some(MapFog {
        color: colour(o, "FogColor").unwrap_or([127.0 / 255.0; 3]),
        start: near * scale,
        end: far * scale,
        max_density: 1.0,
        linear: true,
    })
}

/// Whether the object named `name` makes its world model solid, and
/// whether rays hit it (Q14): `Solid` (without it, `SOLID_CLASSES`) and
/// `Rayhit` (false without it).
fn model_flags(world: &World, name: &str) -> (bool, bool) {
    let Some(o) = world.objects.iter().find(|o| o.name() == Some(name)) else {
        return (false, false);
    };
    let solid = o.bool("Solid").unwrap_or_else(|| SOLID_CLASSES.contains(&o.class.as_str()));
    (solid, o.bool("Rayhit").unwrap_or(false))
}

/// The collision (Q5, Q14): solid polygons of the physics BSP and of
/// solid objects' world models for everything; their other polygons and
/// ray-hit-only world models for traces only; the collision blockers for
/// players only.
fn collision(world: &World, map: &mut MapData, to_engine: &impl Fn(lt::Vec3) -> Vec3) {
    let positions = &mut map.collision_positions;
    let mut fan = |points: &mut dyn Iterator<Item = lt::Vec3>, out: &mut Vec<[u32; 3]>| {
        let base = positions.len() as u32;
        let corners: Vec<Vec3> = points.map(to_engine).collect();
        for k in 1..corners.len().saturating_sub(1) {
            // Mirrored: the reversed order keeps the front face.
            let [a, b, c] = [0, k + 1, k];
            if (corners[b] - corners[a]).cross(corners[c] - corners[a]).length_squared() > 1e-12 {
                out.push([base + a as u32, base + b as u32, base + c as u32]);
            }
        }
        positions.extend(corners.iter().map(|p| p.to_array()));
    };
    let (mut solid, mut traces, mut blockers) = (Vec::new(), Vec::new(), Vec::new());
    for m in &world.models {
        let (is_solid, rayhit) = if m.is_physics() { (true, true) } else { model_flags(world, &m.name) };
        if !(is_solid || rayhit) {
            continue;
        }
        for i in 0..m.polygons.len() {
            let out = if is_solid && m.polygon_surface(i).flags & surface_flags::SOLID != 0 {
                &mut solid
            } else {
                &mut traces
            };
            fan(&mut m.polygon_vertices(i), out);
        }
    }
    for p in &world.blockers.polygons {
        fan(&mut p.vertices.iter().copied(), &mut blockers);
    }
    map.collision_indices = solid;
    map.collision_sets = vec![
        MapCollisionSet {
            name: "collision blockers".into(),
            indices: blockers,
            characters: true,
            traces: false,
        },
        MapCollisionSet {
            name: "non-solid detail".into(),
            indices: traces,
            characters: false,
            traces: true,
        },
    ];
}

/// The volume objects (`volumes::is_volume`) as map entities: their class
/// and properties (engine axes: `Current` with Z mirrored) and their world
/// model's faces as one hull (planes facing out), in entity space.
fn volumes(world: &World, map: &mut MapData, scale: f32) {
    let mut any = false;
    for o in &world.objects {
        if !super::volumes::is_volume(&o.class) {
            continue;
        }
        let Some((_, m)) = o.name().and_then(|n| world.model(n)) else { continue };
        let points: Vec<Vec3> = (0..m.polygons.len())
            .flat_map(|i| m.polygon_vertices(i).collect::<Vec<_>>())
            .map(|p| Vec3::from(lt::to_right_handed(p)) * scale)
            .collect();
        if points.is_empty() {
            continue;
        }
        let centre = points.iter().sum::<Vec3>() / points.len() as f32;
        let planes = (0..m.polygons.len())
            .map(|i| {
                let p = m.polygon_plane(i);
                let n = Vec3::from(lt::to_right_handed(p.normal));
                let d = p.dist * scale;
                // Facing out of the volume.
                if n.dot(centre) > d { (-n, -d) } else { (n, d) }
            })
            .map(|(n, d)| (engine_to_entity(n, 1.0), d / scale))
            .collect();
        let mut keyvalues = vec![("classname".to_string(), o.class.clone())];
        if let Some(n) = o.name() {
            keyvalues.push(("targetname".into(), n.into()));
        }
        for key in ["Viscosity", "Damage", "PendingSec"] {
            if let Some(v) = o.real(key) {
                keyvalues.push((key.into(), v.to_string()));
            }
        }
        if let Some(c) = o.vector("Current") {
            let [x, y, z] = lt::to_right_handed(c);
            keyvalues.push(("Current".into(), format!("{x} {y} {z}")));
        }
        if let Some(h) = o.bool("Hidden") {
            keyvalues.push(("Hidden".into(), (h as u8).to_string()));
        }
        map.entities.push(MapEntity {
            keyvalues,
            hulls: vec![MapHull {
                planes,
                points: points.iter().map(|p| engine_to_entity(*p, scale)).collect(),
            }],
            ..default()
        });
        any = true;
    }
    if any {
        map.entity_scale = scale;
    }
}

/// Start points of one game type, feet on the floor below them.
fn spawns(world: &World, map: &mut MapData, to_engine: &impl Fn(lt::Vec3) -> Vec3) {
    let starts: Vec<&lt::Object> = world.objects_of("GameStartPoint").map(|(_, o)| o).collect();
    let mode = starts
        .iter()
        .any(|o| o.string("GameType") == Some(SPAWN_GAME_TYPE))
        .then_some(SPAWN_GAME_TYPE);
    let floor = Floor::new(map);
    let (mut spawns, mut yaws) = (Vec::new(), Vec::new());
    for o in starts {
        if mode.is_some() && o.string("GameType") != mode {
            continue;
        }
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

/// Downward rays against what players stand on (the collision triangles
/// and the players-only sets).
struct Floor<'a> {
    positions: &'a [[f32; 3]],
    triangles: Vec<([u32; 3], Vec2, Vec2)>,
}

impl<'a> Floor<'a> {
    fn new(map: &'a MapData) -> Self {
        let p = |i: u32| Vec3::from(map.collision_positions[i as usize]);
        let players = map.collision_sets.iter().filter(|s| s.characters).flat_map(|s| &s.indices);
        let triangles = map
            .collision_indices
            .iter()
            .chain(players)
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

