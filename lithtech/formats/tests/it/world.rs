//! The world reader (`lithtech_formats::world`) on the synthetic fixtures
//! of specs/combat_arms/world_dat.md ("Synthetic fixtures": A, B and the
//! negative tests C1-C7), written here byte by byte from the spec's
//! tables (no game data), and the decoders on small inputs.

use lithtech_formats::world::{
    Location, Value, World, decode_intensities, decode_rgb_rle, decode_sub_lightmap, name_hash, rotation_axes,
    to_right_handed,
};

/// A little-endian writer that checks each field lands at the spec's
/// offset.
#[derive(Default)]
struct W(Vec<u8>);

impl W {
    fn at(&mut self, offset: usize) -> &mut Self {
        assert_eq!(self.0.len(), offset, "fixture layout");
        self
    }
    fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }
    fn i32(&mut self, v: i32) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }
    fn f32(&mut self, v: f32) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }
    fn vec3(&mut self, v: [f32; 3]) -> &mut Self {
        v.iter().for_each(|&x| {
            self.f32(x);
        });
        self
    }
    fn lstring(&mut self, s: &str) -> &mut Self {
        self.u16(s.len() as u16);
        self.0.extend(s.as_bytes());
        self
    }
    fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.0.extend(b);
        self
    }
}

/// Fixture A's bytes up to offset 352 (the shared start of A and B), with
/// the header's six offsets.
fn fixture_start(offsets: [u32; 6]) -> W {
    let mut w = W::default();
    w.at(0).u32(85);
    offsets.iter().for_each(|&o| {
        w.u32(o);
    });
    (0..8).for_each(|_| {
        w.u32(0);
    });
    w.at(60).u32(0);
    w.at(64).vec3([-64.0; 3]).vec3([64.0; 3]);
    w.at(88).vec3([0.0; 3]);
    // World tree.
    w.at(100).vec3([-64.0; 3]).vec3([64.0; 3]);
    w.at(124).u32(1).at(128).u32(0).at(132).u8(0);
    // World models: one, the physics BSP.
    w.at(133).u32(1).at(137).u32(0);
    w.at(141).u32(0x14).at(145).lstring("PhysicsBSP");
    w.at(157);
    for v in [3, 1, 1, 0, 1, 0, 3, 0, 0, 1] {
        w.u32(v);
    }
    w.at(197).vec3([0.0; 3]).vec3([32.0, 0.0, 32.0]).vec3([0.0; 3]);
    w.at(233).u32(8).u32(1);
    w.at(241).bytes(b"Default\0");
    w.at(249).u8(3);
    w.at(250).vec3([0.0, 1.0, 0.0]).f32(0.0);
    w.at(266).u32(0x1001).u16(0).u16(0);
    w.at(274).u32(0).u32(0).u32(0).u32(1).u32(2);
    w.at(294).u32(0).u16(0).i32(-2).i32(-1);
    w.at(308).vec3([0.0, 0.0, 0.0]).vec3([0.0, 0.0, 32.0]).vec3([32.0, 0.0, 0.0]);
    w.at(344).i32(0).at(348).u32(0);
    w
}

/// The light grid of Fixture A (44 bytes).
fn light_grid(w: &mut W) {
    w.vec3([-64.0; 3]).vec3([250.0; 3]);
    w.i32(1).i32(1).i32(1).u32(4).bytes(&[0x80, 40, 40, 40]);
}

/// Fixture A: the minimal valid file (432 bytes).
fn fixture_a() -> Vec<u8> {
    let mut w = fixture_start([352, 356, 360, 404, 412, 420]);
    w.at(352).u32(0).at(356).u32(0).at(360);
    light_grid(&mut w);
    w.at(404).u32(0).u32(0).at(412).u32(0).u32(0);
    w.at(420).u32(0).at(424).u32(0).at(428).u32(0).at(432);
    w.0
}

/// Fixture B: one object of every property type, one render block (916
/// bytes).
fn fixture_b() -> Vec<u8> {
    let mut w = fixture_start([352, 533, 537, 581, 589, 597]);
    w.at(352).u32(1).at(356).u16(175).at(358).lstring("Demo").at(364).u32(8);
    let prop = |w: &mut W, name: &str, code: u8, data: &[u8]| {
        w.lstring(name).u8(code).u32(0).u16(data.len() as u16).bytes(data);
    };
    let f = |v: &[f32]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
    w.at(368);
    prop(&mut w, "Name", 0, &[1, 0, b'A']);
    w.at(384);
    prop(&mut w, "Pos", 1, &f(&[1.0, 2.0, 3.0]));
    w.at(408);
    prop(&mut w, "Color", 2, &f(&[255.0, 128.0, 0.0]));
    w.at(434);
    prop(&mut w, "Real", 3, &f(&[0.5]));
    w.at(451);
    prop(&mut w, "Bits", 4, &f(&[6.0]));
    w.at(468);
    prop(&mut w, "Flag", 5, &[1]);
    w.at(482);
    prop(&mut w, "Count", 6, &f(&[7.0]));
    w.at(500);
    prop(&mut w, "Rotation", 7, &f(&[0.0, 1.570_796_4, 0.0, 0.0]));
    w.at(533).u32(0).at(537);
    light_grid(&mut w);
    w.at(581).u32(0).u32(0).at(589).u32(0).u32(0);
    // Render data: one block.
    w.at(597).u32(1).at(601).vec3([16.0, 0.0, 16.0]).vec3([16.0, 1.0, 16.0]);
    w.at(625).u32(1).at(629).lstring("Default").lstring("").u8(1).u32(1).lstring("");
    w.u32(0).u32(0).u32(0);
    w.at(659).u32(3).at(663);
    for p in [[0.0, 0.0, 0.0], [0.0, 0.0, 32.0], [32.0, 0.0, 0.0]] {
        w.vec3(p).f32(p[0] / 64.0).f32(p[2] / 64.0).f32(0.0).f32(0.0).u32(0xFFFF_FFFF);
        w.vec3([0.0, 1.0, 0.0]).vec3([1.0, 0.0, 0.0]).vec3([0.0, 0.0, 1.0]);
    }
    w.at(867).u32(1).at(871).u32(0).u32(1).u32(2).u32(0);
    w.at(887).u32(0).u32(0).u32(0).at(899).u8(0).at(900).u32(0).u32(0);
    w.at(908).u32(0).at(912).u32(0).at(916);
    w.0
}

#[test]
fn fixture_a_parses() {
    let bytes = fixture_a();
    assert_eq!(bytes.len(), 432);
    let w = World::parse(&bytes).unwrap();
    assert_eq!(w.header.version, 85);
    assert_eq!(w.models.len(), 1);
    let m = &w.models[0];
    assert!(m.is_physics() && w.physics().is_some());
    assert_eq!((m.name.as_str(), m.polygons.len()), ("PhysicsBSP", 1));
    assert_eq!(m.textures, ["Default"]);
    // (v1 − v0) × (v2 − v0) = (0, 1024, 0), along the plane normal.
    let v: Vec<[f32; 3]> = m.polygon_vertices(0).collect();
    let (e1, e2) = (
        [v[1][0] - v[0][0], v[1][1] - v[0][1], v[1][2] - v[0][2]],
        [v[2][0] - v[0][0], v[2][1] - v[0][1], v[2][2] - v[0][2]],
    );
    let cross = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ];
    assert_eq!(cross, [0.0, 1024.0, 0.0]);
    assert_eq!(m.polygon_plane(0).normal, [0.0, 1.0, 0.0]);
    assert_eq!(m.polygon_surface(0).flags, 0x1001);
    assert_eq!(m.locate([8.0, 1.0, 8.0]), Location::In);
    assert_eq!(m.locate([8.0, -1.0, 8.0]), Location::Out);
    // On the plane goes to the back.
    assert_eq!(m.locate([8.0, 0.0, 8.0]), Location::Out);
    assert_eq!(w.light_grid.samples, [40, 40, 40]);
    assert_eq!(w.light_grid.sample(0, 0, 0), [40, 40, 40]);
    assert_eq!(w.file_len, 432);
    assert!(w.objects.is_empty() && w.render.blocks.is_empty() && w.light_groups.is_empty());
    assert_eq!(w.tree.subdivided, [false]);
    assert_eq!(w.ambient_light(), [0.0; 3], "an empty info string");
}

#[test]
fn fixture_b_parses() {
    let bytes = fixture_b();
    assert_eq!(bytes.len(), 916);
    let w = World::parse(&bytes).unwrap();
    assert_eq!(w.header.offsets, [352, 533, 537, 581, 589, 597]);
    assert_eq!(w.objects.len(), 1);
    let o = &w.objects[0];
    assert_eq!(o.class, "Demo");
    assert_eq!(o.properties.len(), 8);
    assert_eq!(o.name(), Some("A"));
    assert_eq!(o.pos(), Some([1.0, 2.0, 3.0]));
    assert_eq!(o.get("Color"), Some(&Value::Color([255.0, 128.0, 0.0])));
    assert_eq!(o.real("Real"), Some(0.5));
    assert_eq!((o.get("Bits").unwrap().code(), o.int("Bits")), (4, Some(6)));
    assert_eq!(o.bool("Flag"), Some(true));
    assert_eq!((o.get("Count").unwrap().code(), o.int("Count")), (6, Some(7)));
    let [pitch, yaw, roll] = o.rotation("Rotation").unwrap();
    assert_eq!(yaw, 1.570_796_4);
    // Yaw 1.5707964 faces (1, 0, 0) in file axes.
    let forward = rotation_axes(pitch, yaw, roll)[2];
    assert!((forward[0] - 1.0).abs() < 1e-6 && forward[1].abs() < 1e-6 && forward[2].abs() < 1e-6);
    let b = &w.render.blocks[0];
    assert_eq!((w.render.blocks.len(), b.vertices.len(), b.triangles.len()), (1, 3, 1));
    assert_eq!(b.triangles[0].polygon, 0);
    assert_eq!(b.sections[0].textures[0], "Default");
    assert_eq!((b.sections[0].shader, b.sections[0].lightmap.len()), (1, 0));
    assert_eq!(b.vertices[1].uv0, [0.0, 0.5]);
    assert_eq!(b.vertices[0].rgba(), [255; 4]);
    assert_eq!(b.child_blocks().count(), 0);
    assert_eq!(name_hash(b"A"), 65);
}

/// Fixture C: each change must be rejected.
#[test]
fn fixture_c_negative_cases() {
    let a = fixture_a();
    let patch = |offset: usize, bytes: &[u8]| {
        let mut f = a.clone();
        f[offset..offset + bytes.len()].copy_from_slice(bytes);
        f
    };
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("1: version 84", patch(0, &84u32.to_le_bytes())),
        // Counts at 157: points, planes, surfaces, user portals (169), ...
        ("2: a user portal", patch(169, &1u32.to_le_bytes())),
        ("3: a terrain section", patch(348, &1u32.to_le_bytes())),
        // Polygon 0 at 274: surface, plane, points 0, 1, 2 (the last at 290).
        ("4: point index 3 of 3", patch(290, &3u32.to_le_bytes())),
        ("5: tree count 5, bits 0", patch(124, &5u32.to_le_bytes())),
        ("6: truncated", a[..431].to_vec()),
        // Light grid RLE at 400: a run of 2 into a 1-sample grid.
        ("7: grid RLE past its size", patch(400, &[0x81])),
    ];
    for (what, bytes) in cases {
        let e = World::parse(&bytes).expect_err(what);
        eprintln!("{what}: {e}");
    }
}

/// "Telling versions apart" rule 2, and blocks that don't meet.
#[test]
fn header_offsets_and_block_ends_are_checked() {
    let a = fixture_a();
    let patch = |offset: usize, v: u32| {
        let mut f = a.clone();
        f[offset..offset + 4].copy_from_slice(&v.to_le_bytes());
        f
    };
    // Decreasing, below 64, past the end, and a block that ends early.
    for bytes in [patch(8, 300), patch(4, 63), patch(24, 433), patch(8, 360)] {
        assert!(World::parse(&bytes).is_err());
    }
    // An info string longer than the file.
    assert!(World::parse(&patch(60, 10_000)).is_err());
    // A still-encrypted payload: random first bytes.
    assert!(World::parse(&patch(0, 0x9E37_79B9)).is_err());
    // Nothing at all.
    assert!(World::parse(&[]).is_err());
}

/// Every prefix of both fixtures is rejected without a panic.
#[test]
fn every_truncation_is_an_error() {
    for f in [fixture_a(), fixture_b()] {
        for n in 0..f.len() {
            assert!(World::parse(&f[..n]).is_err(), "{n} bytes");
        }
    }
}

#[test]
fn decoders() {
    // RLE: a run of 2, then a span of 1.
    assert_eq!(
        decode_rgb_rle(&[0x81, 1, 2, 3, 0x00, 4, 5, 6], 3).unwrap(),
        [1, 2, 3, 1, 2, 3, 4, 5, 6]
    );
    assert!(decode_rgb_rle(&[0x82, 1, 2, 3], 2).is_err(), "past the end");
    assert!(decode_rgb_rle(&[0x81, 1, 2, 3, 0], 2).is_err(), "bytes left");
    assert!(decode_rgb_rle(&[0x81, 1, 2], 2).is_err(), "cut off");
    // Intensities: 7, then 1 + 2 zeros, then 9.
    assert_eq!(decode_intensities(&[7, 0, 2, 9], 5).unwrap(), [7, 0, 0, 0, 9]);
    assert!(decode_intensities(&[7, 0, 2, 9], 4).is_err());
    assert!(decode_intensities(&[7, 0], 2).is_err());
    // Sub-lightmaps: 0xFF, n, x is n + 1 texels; the writer's lone-255
    // escape decodes as one texel of 0 ("Sub-lightmap escape").
    assert_eq!(decode_sub_lightmap(&[5, 0xFF, 2, 8], 4).unwrap(), [5, 8, 8, 8]);
    assert_eq!(decode_sub_lightmap(&[0xFF, 0, 0], 1).unwrap(), [0]);
    assert!(decode_sub_lightmap(&[0xFF, 3, 1], 2).is_err());
}

#[test]
fn conventions() {
    // "Name hash" and "Coordinate conversion".
    assert_eq!(name_hash(b"LightGroup_H01"), 0x4750_F233);
    assert_eq!(to_right_handed([1.0, 2.0, 3.0]), [1.0, 2.0, -3.0]);
    // Yaw 0 faces +Z; positive pitch looks down.
    assert_eq!(rotation_axes(0.0, 0.0, 0.0), [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
    assert!(rotation_axes(0.3, 0.0, 0.0)[2][1] < 0.0);
}

/// Fixture B with its object's properties replaced by `props` (name,
/// type code, stored bytes) and its record length R set to the content's
/// plus `r_off`; the later blocks moved to follow.
fn fixture_b_with(props: &[(&str, u8, Vec<u8>)], r_off: i32) -> Vec<u8> {
    let b = fixture_b();
    let mut obj = W::default();
    obj.lstring("Demo").u32(props.len() as u32);
    for (name, code, data) in props {
        obj.lstring(name).u8(*code).u32(0).u16(data.len() as u16).bytes(data);
    }
    let mut block = W::default();
    block.u32(1).u16((obj.0.len() as i32 + r_off) as u16).bytes(&obj.0);
    let delta = block.0.len() as i64 - (533 - 352);
    let mut out = b[..352].to_vec();
    for i in 1..6 {
        let at = 4 + 4 * i;
        let o = u32::from_le_bytes(out[at..at + 4].try_into().unwrap()) as i64 + delta;
        out[at..at + 4].copy_from_slice(&(o as u32).to_le_bytes());
    }
    out.extend(block.0);
    out.extend(&b[533..]);
    out
}

/// Test case 17: an unknown shader code (Fixture B's section, byte 640,
/// from 1 to 3) parses exactly as before and is kept (the drawing skips
/// it).
#[test]
fn t17_unknown_shader_code() {
    let mut b = fixture_b();
    assert_eq!(b[640], 1);
    b[640] = 3;
    let w = World::parse(&b).unwrap();
    let mut want = World::parse(&fixture_b()).unwrap();
    want.render.blocks[0].sections[0].shader = 3;
    assert_eq!(w, want);
}

/// Test case 18: the collision blockers' trailer (Fixture A, offset 408)
/// is read and ignored.
#[test]
fn t18_blocker_trailer() {
    let mut a = fixture_a();
    a[408..412].copy_from_slice(&7u32.to_le_bytes());
    let w = World::parse(&a).unwrap();
    assert!(w.blockers.polygons.is_empty() && w.warnings.is_empty());
}

/// Test case 19: a real stored in 8 bytes uses its first four and the
/// next property starts after all 8; one in 2 bytes is refused; a
/// polygon of 41 vertices is refused. A record length that disagrees
/// with the content is a warning, the list read on by content.
#[test]
fn t19_property_lengths() {
    let real8 = [0.5f32.to_le_bytes(), 9.0f32.to_le_bytes()].concat();
    let props = [("Real", 3, real8), ("Flag", 5, vec![1])];
    let w = World::parse(&fixture_b_with(&props, 0)).unwrap();
    let o = &w.objects[0];
    assert_eq!((o.real("Real"), o.bool("Flag")), (Some(0.5), Some(true)));
    assert!(w.warnings.is_empty());
    assert!(World::parse(&fixture_b_with(&[("Real", 3, vec![0, 0])], 0)).is_err(), "D = 2");
    for r_off in [-3, 5] {
        let w = World::parse(&fixture_b_with(&props, r_off)).unwrap();
        assert_eq!(w.objects[0].real("Real"), Some(0.5));
        assert_eq!(w.warnings.len(), 1, "{:?}", w.warnings);
    }
    // Fixture A's polygon 0 has its vertex count byte at 249.
    let mut a = fixture_a();
    assert_eq!(a[249], 3);
    a[249] = 41;
    assert!(World::parse(&a).is_err());
}
