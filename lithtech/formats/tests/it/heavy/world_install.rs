//! The world reader on the user's Combat Arms install, against the test
//! cases of specs/combat_arms/world_dat.md (Steam build 25595979). Each
//! test passes with a note when there is no install; test case 7 also
//! needs a V2 key.

// The spec's yaws are values near pi as stored, not pi.
#![allow(clippy::approx_constant)]

use std::collections::BTreeMap;

use lithtech_formats::{
    rez::{RezArchive, RezKeys},
    world::{self, Location, World, shader},
};
use source_formats::fs::FileSource;

use super::rez_install::{game_dir, keys};

/// Every `.dat` in WORLDS2 and WORLDS3: (archive, path, bytes).
fn plain_maps() -> Option<Vec<(String, String, Vec<u8>)>> {
    let game = game_dir()?;
    let mut out = Vec::new();
    for archive in ["WORLDS2.rez", "WORLDS3.rez"] {
        let rez = RezArchive::open(game.join(archive), &RezKeys::new()).unwrap();
        let mut paths: Vec<String> = rez.entries().into_iter().map(|e| e.path).filter(|p| p.ends_with(".dat")).collect();
        paths.sort();
        for p in paths {
            let bytes = rez.read(&p).unwrap().unwrap();
            out.push((archive.to_string(), p, bytes));
        }
    }
    Some(out)
}

fn read_map(archive: &str, path: &str) -> Option<(Vec<u8>, World)> {
    let game = game_dir()?;
    let rez = RezArchive::open(game.join(archive), &RezKeys::new()).unwrap();
    let bytes = rez.read(path).unwrap().unwrap();
    let w = World::parse(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
    Some((bytes, w))
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-3
}

fn close3(a: [f32; 3], b: [f32; 3]) -> bool {
    (0..3).all(|i| close(a[i], b[i]))
}

/// Polygons, points, render vertices and triangles, lightmaps (shader-2
/// sections), sections per shader, over a whole world.
#[derive(Debug, Default)]
struct Totals {
    polygons: usize,
    points: usize,
    render_vertices: usize,
    render_triangles: usize,
    sections: usize,
    lightmaps: usize,
    occluders: usize,
    sky_portals: usize,
    block_light_groups: usize,
    by_shader: BTreeMap<u8, usize>,
}

fn totals(w: &World) -> Totals {
    let mut t = Totals {
        polygons: w.models.iter().map(|m| m.polygons.len()).sum(),
        points: w.models.iter().map(|m| m.points.len()).sum(),
        ..Default::default()
    };
    for b in w.render.all_blocks() {
        t.render_vertices += b.vertices.len();
        t.render_triangles += b.triangles.len();
        t.sections += b.sections.len();
        t.occluders += b.occluders.len();
        t.sky_portals += b.sky_portals.len();
        t.block_light_groups += b.light_groups.len();
        for s in &b.sections {
            *t.by_shader.entry(s.shader).or_default() += 1;
            if s.shader == shader::LIGHTMAP {
                t.lightmaps += 1;
            }
        }
    }
    t
}

fn class_counts(w: &World) -> BTreeMap<&str, usize> {
    let mut out = BTreeMap::new();
    for o in &w.objects {
        *out.entry(o.class.as_str()).or_default() += 1;
    }
    out
}

fn start_points(w: &World) -> BTreeMap<(String, String), usize> {
    let mut out = BTreeMap::new();
    for (_, o) in w.objects_of("GameStartPoint") {
        let key = (
            o.string("GameType").unwrap_or("").to_string(),
            o.string("Teams").unwrap_or("").to_string(),
        );
        *out.entry(key).or_default() += 1;
    }
    out
}

/// A one-line summary per map (versions found, sizes), for the report.
#[test]
fn every_plain_map_summary() {
    let Some(maps) = plain_maps() else { return };
    for (archive, path, bytes) in &maps {
        let w = World::parse(bytes).unwrap_or_else(|e| panic!("{archive} {path}: {e}"));
        let t = totals(&w);
        eprintln!(
            "{archive} {path}: v{} {} bytes, {} models, {} objects, {} polys, {} blocks, {} lightmaps, {} spawns",
            w.header.version,
            bytes.len(),
            w.models.len(),
            w.objects.len(),
            t.polygons,
            w.render.blocks.len(),
            t.lightmaps,
            w.objects_of("GameStartPoint").count()
        );
    }
    assert_eq!(maps.len(), 33);
}

/// Test case 1: every plaintext map's header, block ends, world-model
/// flags and order, render world-model names and sky portals.
#[test]
fn case_1_every_plain_map() {
    let Some(maps) = plain_maps() else { return };
    for (_, path, bytes) in &maps {
        // The parse checks every block ends at the next offset and the
        // last at the end of the file.
        let w = World::parse(bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_eq!(w.header.version, 85, "{path}");
        assert_eq!(w.header.reserved, [0; 8], "{path}");
        let physics: Vec<usize> = (0..w.models.len()).filter(|&i| w.models[i].flags == 0x14).collect();
        assert_eq!(physics, [w.models.len() - 1], "{path}");
        assert_eq!(w.models.last().unwrap().name, "PhysicsBSP", "{path}");
        assert!(w.models[..w.models.len() - 1].iter().all(|m| m.flags == 0x02), "{path}");
        assert!(w.models.iter().all(|m| m.prefix == 0), "{path}");
        let names: Vec<&str> = w.render.models.iter().map(|(n, _)| n.as_str()).collect();
        let bsps: Vec<&str> = w.models[..w.models.len() - 1].iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, bsps, "{path}");
        assert!(w.render.models.iter().all(|(_, r)| r.models.is_empty()), "{path}");
        assert_eq!(totals(&w).sky_portals, 6, "{path}");
    }
    assert_eq!(maps.len(), 33);
}

/// Test case 2: WORLDS2\WAREHOUSE.DAT.
#[test]
fn case_2_warehouse() {
    let Some((bytes, w)) = read_map("WORLDS2.rez", "worlds2/warehouse.dat") else { return };
    assert_eq!(bytes.len(), 18_076_579);
    assert_eq!(w.header.version, 85);
    assert_eq!(w.header.offsets, [3_056_494, 3_303_416, 3_307_244, 3_333_968, 3_337_304, 3_337_312]);
    assert_eq!(w.info, "ambientlight 30 30 34; lmgridsize 10");
    assert_eq!(w.ambient_light(), [30.0, 30.0, 34.0]);
    assert!(close3(w.extents_min, [-9952.0, -10080.0, -10112.0]));
    assert!(close3(w.extents_max, [9952.0, 10080.0, 10112.0]));
    assert!(close3(w.world_offset, [-480.0, 5984.0, 1216.0]));
    assert_eq!((w.tree.node_count, w.tree_end), (5461, 851));
    assert_eq!(w.tree.boxes().len(), 5461);
    let t = totals(&w);
    assert_eq!((w.models.len(), t.polygons, t.points), (110, 38_404, 31_134));
    let m0 = &w.models[0];
    assert_eq!((m0.name.as_str(), m0.flags), ("EndlessFall00", 0x02));
    assert_eq!(
        (m0.points.len(), m0.planes.len(), m0.surfaces.len(), m0.polygons.len(), m0.nodes.len(), m0.textures.len()),
        (8, 6, 1, 6, 6, 2)
    );
    let p = &w.models[109];
    assert!(p.is_physics());
    assert_eq!(
        (p.points.len(), p.planes.len(), p.surfaces.len(), p.polygons.len()),
        (30_106, 33_628, 298, 37_648)
    );
    assert_eq!((p.polygon_vertex_refs, p.point_indices.len()), (114_984, 114_984));
    assert_eq!((p.nodes.len(), p.textures.len(), p.root), (90_899, 269, 0));
    assert_eq!(w.objects.len(), 370);
    assert_eq!(w.blind_data.len(), 6);
    assert!(w.blind_data.iter().all(|c| c.id == 0x73F5_3A84));
    let sizes: Vec<usize> = w.blind_data.iter().map(|c| c.data.len()).collect();
    assert_eq!(sizes, [230, 550, 902, 544, 472, 1078]);
    assert_eq!(w.light_grid.counts, [80, 81, 81]);
    assert!(close3(w.light_grid.cell, [250.0; 3]));
    assert_eq!(w.light_grid.compressed_len, 26_684);
    assert_eq!(
        (w.blockers.polygons.len(), w.particle_blockers.polygons.len(), w.light_groups.len()),
        (50, 0, 0)
    );
    assert_eq!((w.render.blocks.len(), w.render.models.len()), (145, 109));
    assert_eq!(
        (t.sections, t.lightmaps, t.render_vertices, t.render_triangles, t.occluders),
        (2_114, 305, 151_153, 69_385, 0)
    );
    assert_eq!(t.by_shader, BTreeMap::from([(1, 817), (2, 305), (4, 980), (8, 12)]));
    let root = &w.render.blocks[0];
    assert!(close3(root.center, [0.0; 3]) && close3(root.half_size, [9952.0, 10080.0, 10112.0]));
    assert_eq!((root.sections.len(), root.vertices.len(), root.triangles.len()), (50, 3_836, 1_956));
    assert_eq!((root.child_flags, root.children), (3, [66, 1]));
    let s0 = &root.sections[0];
    assert_eq!((s0.textures[0].as_str(), s0.shader, s0.triangle_count), ("LightAnim_BASE", 2, 2));
    assert_eq!((s0.lightmap_width, s0.lightmap_height, s0.lightmap_len), (256, 256, 7_422));
    assert_eq!(s0.lightmap.len(), 256 * 256 * 3);
    let top = w.render.blocks.iter().flat_map(|b| &b.triangles).map(|t| t.polygon).max();
    assert_eq!(top, Some(37_647));
    let classes = class_counts(&w);
    for (class, n) in [
        ("Light", 70),
        ("ObjectSpawnPoint", 60),
        ("EndlessFall", 55),
        ("WorldModel", 41),
        ("GameStartPoint", 40),
        ("DirLight", 38),
        ("SpecialFX", 24),
        ("Breakable", 12),
        ("TDMGameControl", 2),
        ("StaticSunLight", 1),
        ("DemoSkyWorldModel", 1),
        ("WorldProperties", 1),
    ] {
        assert_eq!(classes.get(class), Some(&n), "{class}");
    }
    let key = |a: &str, b: &str| (a.to_string(), b.to_string());
    assert_eq!(
        start_points(&w),
        BTreeMap::from([
            (key("Team Death Match", "JSF"), 12),
            (key("Team Death Match", "Creed"), 12),
            (key("FreeForAll", "<none>"), 16),
        ])
    );
    // Object 92.
    let o = &w.objects[92];
    assert_eq!((o.class.as_str(), o.properties.len()), ("GameStartPoint", 13));
    let names: Vec<&str> = o.properties.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Name",
            "Pos",
            "Rotation",
            "RenderGroup",
            "Template",
            "GameType",
            "Teams",
            "Spawn",
            "ModelTemplate",
            "PhysicsModel",
            "Command",
            "Locked",
            "SendCommandOnRespawn"
        ]
    );
    assert_eq!((o.name(), o.property("Name").unwrap().len), (Some("GameStartPoint11"), 18));
    assert!(close3(o.pos().unwrap(), [618.0, -5546.0, -2271.0002]));
    assert!(close3(o.rotation("Rotation").unwrap(), [0.0, -0.000001, 0.0]));
    assert_eq!(o.int("RenderGroup"), Some(0));
    assert_eq!(o.bool("Template"), Some(false));
    assert_eq!(o.string("GameType"), Some("Team Death Match"));
    assert_eq!((o.string("Teams"), o.property("Teams").unwrap().len), (Some("JSF"), 5));
    assert_eq!(o.int("Spawn"), Some(1));
    assert_eq!(o.string("ModelTemplate"), Some("Normal"));
    assert_eq!(o.string("PhysicsModel"), Some("Normal"));
    assert_eq!((o.string("Command"), o.property("Command").unwrap().len), (Some(""), 2));
    assert_eq!(o.bool("Locked"), Some(false));
    assert_eq!(o.bool("SendCommandOnRespawn"), Some(false));
    assert_eq!(o.property("Pos").unwrap().flags, 0x10_0000);
    assert_eq!(o.property("GameType").unwrap().flags, 0x2_0000);
    for (i, s) in w.objects_of("GameStartPoint") {
        assert_eq!(p.locate(s.pos().unwrap()), Location::In, "start point {i}");
    }
}

/// Test case 3: WORLDS3\FREEZER.DAT.
#[test]
fn case_3_freezer() {
    let Some((bytes, w)) = read_map("WORLDS3.rez", "worlds3/freezer.dat") else { return };
    assert_eq!(bytes.len(), 11_343_910);
    assert_eq!(w.header.offsets, [2_118_069, 2_255_753, 2_255_757, 2_984_626, 2_988_314, 2_988_322]);
    assert_eq!(w.info, "ambientlight 40 40 40");
    assert!(close3(w.extents_min, [-32768.0, -28672.0, -32768.0]));
    assert!(close3(w.extents_max, [32768.0, 28672.0, 32768.0]));
    assert!(close3(w.world_offset, [0.0; 3]));
    assert_eq!((w.tree.node_count, w.tree_end), (21_845, 2_884));
    let t = totals(&w);
    assert_eq!((w.models.len(), t.polygons, t.points), (32, 26_198, 14_820));
    let p = &w.models[31];
    assert!(p.is_physics());
    assert_eq!(
        (p.points.len(), p.planes.len(), p.surfaces.len(), p.polygons.len(), p.nodes.len(), p.textures.len()),
        (13_704, 23_305, 54, 24_472, 65_717, 48)
    );
    let m3 = &w.models[3];
    assert_eq!((m3.name.as_str(), m3.flags), ("CMD_A_Radio", 0x02));
    assert_eq!((m3.points.len(), m3.planes.len(), m3.polygons.len(), m3.nodes.len()), (8, 6, 6, 6));
    let door = w.objects.iter().find(|o| o.name() == Some("CMD_A_Radio")).unwrap();
    assert_eq!(door.class, "RotatingDoor");
    assert!(close3(door.rotation("Rotation").unwrap(), [0.0, 3.141_593, 0.0]));
    assert_eq!((w.objects.len(), w.blind_data.len()), (275, 0));
    assert_eq!((w.light_grid.counts, w.light_grid.compressed_len), ([263, 230, 263], 728_829));
    assert_eq!(
        (w.blockers.polygons.len(), w.particle_blockers.polygons.len(), w.light_groups.len()),
        (64, 0, 0)
    );
    assert_eq!((w.render.blocks.len(), w.render.models.len()), (73, 31));
    assert_eq!((t.sections, t.lightmaps, t.render_vertices, t.render_triangles), (705, 349, 83_969, 31_729));
    assert_eq!(t.by_shader, BTreeMap::from([(1, 165), (2, 349), (4, 142), (8, 22), (9, 27)]));
    let root = &w.render.blocks[0];
    assert_eq!((root.sections.len(), root.vertices.len(), root.triangles.len()), (62, 2_613, 1_122));
    assert_eq!((root.child_flags, root.children), (3, [38, 1]));
    let classes = class_counts(&w);
    for (class, n) in [
        ("GameStartPoint", 80),
        ("ObjectSpawnPoint", 60),
        ("Light", 25),
        ("SpecialFX", 24),
        ("EndlessFall", 22),
        ("DirLight", 12),
        ("SoundFX", 10),
        ("WorldModel", 6),
        ("TDMGameControl", 5),
        ("FlagItem", 2),
        ("SlidingDoor", 2),
        ("RotatingDoor", 1),
    ] {
        assert_eq!(classes.get(class), Some(&n), "{class}");
    }
    let key = |a: &str, b: &str| (a.to_string(), b.to_string());
    assert_eq!(
        start_points(&w),
        BTreeMap::from([
            (key("Capturetheflag", "JSF"), 12),
            (key("Capturetheflag", "Creed"), 12),
            (key("Team Death Match", "JSF"), 12),
            (key("Team Death Match", "Creed"), 12),
            (key("Quarantine", "JSF"), 16),
            (key("FreeForAll", "<none>"), 16),
        ])
    );
    let o = &w.objects[0];
    assert_eq!((o.class.as_str(), o.name()), ("Light", Some("Light11")));
    assert!(close3(o.pos().unwrap(), [-256.0, 704.0, 1024.0]));
    assert!(close(o.real("LightRadius").unwrap(), 1200.0));
    assert!(close3(o.vector("LightColor").unwrap(), [196.0, 226.0, 255.0]));
    assert!(close(o.real("BrightScale").unwrap(), 0.3));
    assert_eq!(o.string("Attenuation"), Some("Quartic"));
    assert!(close3(o.vector("AttCoefs").unwrap(), [1.0, 0.0, 19.0]));
    assert!(close3(o.vector("AttExps").unwrap(), [0.0, 0.0, -2.0]));
    assert_eq!(o.string("LightGroup"), Some(""));
    assert_eq!(o.bool("FastLightObjects"), Some(true));
    let o = &w.objects[78];
    assert_eq!((o.class.as_str(), o.name()), ("GameStartPoint", Some("GameStartPoint23")));
    assert!(close3(o.pos().unwrap(), [684.424_8, 120.0, -1_434.06]));
    assert!(close(o.rotation("Rotation").unwrap()[1], -0.733_038));
    assert_eq!(o.string("Teams"), Some("Creed"));
}

/// Test case 4: WORLDS3\SNOWSTORM.DAT (light groups, occluders, particle
/// blockers).
#[test]
fn case_4_snowstorm() {
    let Some((bytes, w)) = read_map("WORLDS3.rez", "worlds3/snowstorm.dat") else { return };
    assert_eq!(bytes.len(), 20_681_275);
    assert_eq!(w.header.offsets, [3_050_093, 3_852_065, 3_852_177, 3_880_582, 3_887_254, 3_887_602]);
    assert_eq!(w.info, "");
    assert!(close3(w.extents_max, [10240.0; 3]) && close3(w.extents_min, [-10240.0; 3]));
    assert!(close3(w.world_offset, [0.0, 2048.0, 160.0]));
    assert_eq!((w.tree.node_count, w.tree_end), (5461, 815));
    let t = totals(&w);
    assert_eq!((w.models.len(), t.polygons, t.points), (212, 36_867, 40_135));
    let p = &w.models[211];
    assert_eq!(
        (p.points.len(), p.planes.len(), p.surfaces.len(), p.polygons.len(), p.nodes.len(), p.textures.len()),
        (32_885, 26_139, 250, 30_859, 67_885, 203)
    );
    let m0 = &w.models[0];
    assert_eq!((m0.name.as_str(), m0.flags), ("BOSS_SlidingDoor03", 0x02));
    assert_eq!(
        (m0.points.len(), m0.planes.len(), m0.surfaces.len(), m0.polygons.len(), m0.nodes.len()),
        (36, 10, 3, 40, 70)
    );
    assert!(close3(m0.translation, [84.0, -2964.0, -1046.0]));
    let door = w.objects.iter().find(|o| o.name() == Some("BOSS_SlidingDoor03")).unwrap();
    assert_eq!(door.class, "SlidingDoor");
    assert!(close3(door.pos().unwrap(), [126.0, -2898.26, -1044.0]));
    assert_eq!(w.objects.len(), 1_065);
    assert_eq!(w.blind_data.len(), 1);
    assert_eq!((w.blind_data[0].data.len(), w.blind_data[0].id), (100, 0x6AAF_0884));
    assert_eq!((w.light_grid.counts, w.light_grid.compressed_len), ([82, 82, 82], 28_365));
    assert_eq!((w.blockers.polygons.len(), w.particle_blockers.polygons.len()), (98, 5));
    assert_eq!((w.render.blocks.len(), w.render.models.len()), (133, 211));
    assert_eq!(
        (t.sections, t.lightmaps, t.render_vertices, t.render_triangles, t.occluders),
        (2_161, 280, 194_918, 94_662, 6)
    );
    // Every intensity stream and sub-lightmap decoded to its size (the
    // parse checks it); the groups by name.
    assert_eq!(t.block_light_groups, 34);
    let mut by_name = BTreeMap::new();
    for g in w.render.all_blocks().flat_map(|b| &b.light_groups) {
        *by_name.entry(g.name.as_str()).or_insert(0) += 1;
    }
    assert_eq!(by_name, BTreeMap::from([("LightGroup_H01", 14), ("LightGroup_H02", 20)]));
    assert_eq!(w.light_groups.len(), 2);
    let colour = [1.0, 0.9686275, 0.9019608];
    let (g1, g2) = (&w.light_groups[0], &w.light_groups[1]);
    assert_eq!((g1.name.as_str(), g1.offset, g1.size), ("LightGroup_H01", [40, 33, 45], [1, 1, 1]));
    assert_eq!((g2.name.as_str(), g2.offset, g2.size), ("LightGroup_H02", [39, 33, 42], [2, 1, 3]));
    assert!(close3(g1.color, colour) && close3(g2.color, colour));
    assert_eq!(world::name_hash(b"LightGroup_H01"), 1_196_487_219);
    let classes = class_counts(&w);
    for (class, n) in [
        ("AIVolume", 250),
        ("SpecialFX", 145),
        ("Light", 144),
        ("WorldModel", 48),
        ("SoundFX", 47),
        ("Breakable", 45),
        ("GameStartPoint", 16),
        ("SlidingDoor", 15),
        ("WeaponItem", 4),
        ("LightGroup", 2),
    ] {
        assert_eq!(classes.get(class), Some(&n), "{class}");
    }
    let key = |a: &str, b: &str| (a.to_string(), b.to_string());
    assert_eq!(start_points(&w), BTreeMap::from([(key("CabinFever2", "JSF"), 16)]));
    let o = &w.objects[43];
    assert_eq!(o.name(), Some("GameStartPoint_B06"));
    assert!(close3(o.pos().unwrap(), [136.0, -3012.0, -1898.0]));
    assert!(close(o.rotation("Rotation").unwrap()[1], 3.141_592));
    assert_eq!(o.bool("Locked"), Some(true));
    let portal = w
        .render
        .all_blocks()
        .flat_map(|b| &b.sky_portals)
        .find(|p| close3(p.plane.normal, [1.0, 0.0, 0.0]))
        .expect("a +X sky portal");
    assert!(close(portal.plane.dist, -10240.0));
}

/// Test case 5: every lightmap, every plaintext map.
#[test]
fn case_5_lightmaps() {
    let Some(maps) = plain_maps() else { return };
    let (mut with, mut without) = (0, 0);
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for (_, path, bytes) in &maps {
        let w = World::parse(bytes).unwrap();
        for b in w.render.all_blocks() {
            for (s, tris) in b.section_triangles() {
                if s.shader != shader::LIGHTMAP {
                    assert_eq!(s.lightmap_len, 0, "{path}");
                    without += 1;
                    continue;
                }
                with += 1;
                assert!(s.lightmap_len > 0, "{path}");
                for d in [s.lightmap_width, s.lightmap_height] {
                    assert!(d.is_power_of_two() && (16..=256).contains(&d), "{path}: {d}");
                }
                // Decoded to exactly w·h·3 from exactly Z bytes (the parse).
                assert_eq!(s.lightmap.len() as u32, s.lightmap_width * s.lightmap_height * 3);
                for t in tris {
                    for &i in &t.indices {
                        let uv = b.vertices[i as usize].uv0;
                        lo = lo.min(uv[0].min(uv[1]));
                        hi = hi.max(uv[0].max(uv[1]));
                    }
                }
            }
        }
    }
    eprintln!("{with} lightmap sections, {without} others; UV0 {lo}..{hi}");
    assert_eq!((with, without), (13_191, 65_734));
    assert!(lo >= 0.00195 - 1e-5 && hi <= 3.022 + 1e-3, "{lo}..{hi}");
}

/// Every texture-pass triangle (shaders 4 and 9) of every plaintext map
/// has a lightmap-pass triangle (shader 2) in its block with the same
/// corners in the same winding (perhaps starting at another corner): the
/// pair the engine's equal-depth multiply puts together, and what Combat
/// Arms' map conversion matches them by. Their polygon indices often
/// differ (1,040,084 of 1,113,880: the reason it doesn't match by
/// polygon); three triangles of WARHEAD have no twin.
#[test]
fn texture_pass_triangles_have_lightmap_twins() {
    let Some(maps) = plain_maps() else { return };
    let key = |b: &world::RenderBlock, t: &world::Triangle| {
        let p = t.indices.map(|i| b.vertices[i as usize].pos.map(f32::to_bits));
        let r = (0..3).min_by_key(|&k| p[k]).unwrap();
        [p[r], p[(r + 1) % 3], p[(r + 2) % 3]]
    };
    let (mut textured, mut other_polygon, mut missing) = (0usize, 0usize, Vec::new());
    for (_, path, bytes) in &maps {
        let w = World::parse(bytes).unwrap();
        for b in w.render.all_blocks() {
            let mut lit: BTreeMap<_, Vec<u32>> = BTreeMap::new();
            for (s, tris) in b.section_triangles() {
                if s.shader == shader::LIGHTMAP {
                    for t in tris {
                        lit.entry(key(b, t)).or_default().push(t.polygon);
                    }
                }
            }
            for (s, tris) in b.section_triangles() {
                if !matches!(s.shader, shader::LIGHTMAP_TEXTURE | shader::LIGHTMAP_DUAL_TEXTURE) {
                    continue;
                }
                for t in tris {
                    textured += 1;
                    let Some(twins) = lit.get(&key(b, t)) else {
                        missing.push(format!("{path} polygon {}", t.polygon));
                        continue;
                    };
                    if !twins.contains(&t.polygon) {
                        other_polygon += 1;
                    }
                }
            }
        }
    }
    eprintln!(
        "{textured} texture-pass triangles, {other_polygon} with a twin of another polygon index; without: {missing:?}"
    );
    assert!(other_polygon > textured / 2, "{other_polygon} of {textured}");
    // Three in WARHEAD have none (no shader-2 triangle of their polygon in
    // the block either): they draw unlit by the lightmap (white).
    assert_eq!(missing.len(), 3, "{missing:?}");
    assert!(missing.iter().all(|m| m.starts_with("worlds2/warhead.dat")), "{missing:?}");
}

/// Test case 6: BSP polygon winding against the plane, every plaintext map.
#[test]
fn case_6_polygon_winding() {
    let Some(maps) = plain_maps() else { return };
    let (mut along, mut against, mut degenerate) = (0, 0, 0);
    for (_, _, bytes) in &maps {
        let w = World::parse(bytes).unwrap();
        for m in &w.models {
            for i in 0..m.polygons.len() {
                let v: Vec<[f64; 3]> = m.polygon_vertices(i).map(|p| p.map(f64::from)).collect();
                let e1 = [v[1][0] - v[0][0], v[1][1] - v[0][1], v[1][2] - v[0][2]];
                let e2 = [v[2][0] - v[0][0], v[2][1] - v[0][1], v[2][2] - v[0][2]];
                let cross = [
                    e1[1] * e2[2] - e1[2] * e2[1],
                    e1[2] * e2[0] - e1[0] * e2[2],
                    e1[0] * e2[1] - e1[1] * e2[0],
                ];
                let n = m.polygon_plane(i).normal.map(f64::from);
                let d = cross[0] * n[0] + cross[1] * n[1] + cross[2] * n[2];
                if d.abs() <= 1e-6 {
                    degenerate += 1;
                } else if d > 0.0 {
                    along += 1;
                } else {
                    against += 1;
                }
            }
        }
    }
    eprintln!("{along} along, {against} against, {degenerate} degenerate");
    assert_eq!((along, against, degenerate), (2_003_330, 22, 39));
}

/// Test case 7: with a V2 key, every map of the keyed archives is
/// version 85 and parses whole. Runs only with a V2 key in the local
/// config.
#[test]
fn case_7_keyed_maps() {
    let Some(game) = game_dir() else { return };
    let keys = keys();
    if !keys.contains_key("V2") {
        eprintln!("skipping: no V2 key in chimera.local.toml");
        return;
    }
    let mut n = 0;
    for archive in ["WORLDS.rez", "WORLDS4.rez", "WORLDS5.rez"] {
        let rez = RezArchive::open(game.join(archive), &keys).unwrap();
        for e in rez.entries().into_iter().filter(|e| e.path.ends_with(".dat")) {
            let bytes = rez.read(&e.path).unwrap().unwrap();
            let w = World::parse(&bytes).unwrap_or_else(|err| panic!("{}: {err}", e.path));
            assert_eq!(w.header.version, 85);
            let t = totals(&w);
            eprintln!(
                "{archive} {}: {} models, {} objects, {} polys, {} render triangles in {} blocks ({} world models), \
                 {} lightmaps, shaders {:?}, {} spawns",
                e.path,
                w.models.len(),
                w.objects.len(),
                t.polygons,
                t.render_triangles,
                w.render.blocks.len(),
                w.render.models.len(),
                t.lightmaps,
                t.by_shader,
                w.objects_of("GameStartPoint").count()
            );
            if t.render_triangles < 1000 {
                eprintln!("    objects: {:?}", class_counts(&w));
                for o in w.objects.iter().filter(|o| o.string("Filename").is_some()).take(20) {
                    eprintln!("    {} {:?}: {:?}", o.class, o.string("Name"), o.string("Filename"));
                }
            }
            if e.path == "worlds/piazza.dat" {
                assert_eq!(bytes.len(), 35_895_268);
            }
            n += 1;
        }
    }
    assert_eq!(n, 72);
}

/// Test cases 9 and 10 on the Steam copy of WAREHOUSE (the older
/// client's has the same size): 4,960 of its 37,648 physics-BSP polygons
/// lack the solid bit; 50 collision-blocker polygons.
#[test]
fn case_9_10_warehouse_solid_bits_and_blockers() {
    let Some((_, w)) = read_map("WORLDS2.rez", "worlds2/warehouse.dat") else { return };
    let physics = w.physics().unwrap();
    let non_solid = (0..physics.polygons.len())
        .filter(|&i| physics.polygon_surface(i).flags & lithtech_formats::world::surface_flags::SOLID == 0)
        .count();
    assert_eq!((non_solid, physics.polygons.len()), (4_960, 37_648));
    assert_eq!(w.blockers.polygons.len(), 50);
}

/// Test case 21 on the 33 plain maps: every byte of every class name,
/// property name, string property, world-model name, BSP texture name and
/// render section name is below 0x80; and no object record's length
/// disagrees with its content.
#[test]
fn case_21_eight_bit_text() {
    let Some(maps) = plain_maps() else { return };
    for (_, path, bytes) in maps {
        let w = World::parse(&bytes).unwrap();
        assert!(w.warnings.is_empty(), "{path}: {:?}", w.warnings);
        let mut texts: Vec<&str> = Vec::new();
        for o in &w.objects {
            texts.push(&o.class);
            for p in &o.properties {
                texts.push(&p.name);
                if let lithtech_formats::world::Value::String(s) = &p.value {
                    texts.push(s);
                }
            }
        }
        for m in &w.models {
            texts.push(&m.name);
            texts.extend(m.textures.iter().map(String::as_str));
        }
        for b in w.render.all_blocks() {
            for s in &b.sections {
                texts.extend(s.textures.iter().map(String::as_str));
            }
        }
        assert!(texts.iter().all(|t| t.chars().all(|c| (c as u32) < 0x80)), "{path}");
    }
}
