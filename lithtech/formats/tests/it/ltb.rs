//! The model reader (`lithtech_formats::ltb`) on models written here from
//! the tables of specs/combat_arms/models_ltb.md (no game data): the
//! synthetic fixture (test case 19), its refusals (20), conversion (21),
//! LOD choice (14), and small skinned, palette and vertex-animated
//! variants for the mesh kinds and skinning rules (16, 18). The install
//! cases are `heavy::ltb_install`.

use lithtech_formats::{
    anim,
    ltb::{self, Channel, Mesh, Model},
};

/// A little-endian writer for fixtures.
#[derive(Default)]
pub struct W(pub Vec<u8>);

impl W {
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }
    pub fn i16(&mut self, v: i16) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }
    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }
    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }
    pub fn f32s(&mut self, v: &[f32]) -> &mut Self {
        for x in v {
            self.0.extend(x.to_le_bytes());
        }
        self
    }
    pub fn str(&mut self, s: &str) -> &mut Self {
        self.u16(s.len() as u16);
        self.0.extend(s.as_bytes());
        self
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

/// The parts of a fixture model that tests vary.
pub struct Fixture {
    pub version: u32,
    pub counts: [u32; 15],
    /// (piece name, LOD bytes after the distances and old offsets), one
    /// LOD each.
    pub pieces: Vec<(String, Vec<u8>)>,
    /// The node tree's bytes and its node count.
    pub nodes: (Vec<u8>, usize),
    pub weight_sets: Vec<(String, Vec<f32>)>,
    pub children: Vec<String>,
    /// Whole animation records (the section's count is added).
    pub animations: Vec<Vec<u8>>,
    /// Socket records' bytes and count.
    pub sockets: (Vec<u8>, u32),
    /// Per child slot, bindings (name, dims, translation).
    pub bindings: Vec<Vec<(String, [f32; 3], [f32; 3])>>,
}

/// A node record with `children` child records appended.
pub fn node(name: &str, index: u16, flags: u8, bind: [f32; 16], children: &[Vec<u8>]) -> Vec<u8> {
    let mut w = W::default();
    w.str(name).u16(index).u8(flags).f32s(&bind).u32(children.len() as u32);
    for c in children {
        w.0.extend(c);
    }
    w.0
}

pub const IDENTITY: [f32; 16] = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.];

/// A LOD record: texture slots, render style, priority, kind, body, used
/// nodes.
pub fn lod(kind: u32, body: &[u8], used: &[u8]) -> Vec<u8> {
    let mut w = W::default();
    w.u32(1).i32(0).i32(0).i32(0).i32(0).i32(0).u8(0).u32(kind).u32(body.len() as u32);
    w.0.extend(body);
    w.u8(used.len() as u8);
    w.0.extend(used);
    w.0
}

/// The synthetic fixture's rigid body: 3 vertices, 1 triangle, node 0.
pub fn rigid_body() -> Vec<u8> {
    let mut w = W::default();
    w.u32(3).u32(1).u32(1).u32(1).u32(0x13).u32(0).u32(0).u32(0).u32(0);
    w.f32s(&[0., 0., 0., 0., 0., 1., 0., 0.]);
    w.f32s(&[10., 0., 0., 0., 0., 1., 1., 0.]);
    w.f32s(&[0., 10., 0., 0., 0., 1., 0., 1.]);
    w.u16(0).u16(1).u16(2);
    w.0
}

/// The `LST` animation record of the synthetic fixture (compression 0,
/// one key, one node).
pub fn lst_record() -> Vec<u8> {
    let mut w = W::default();
    w.f32s(&[15., 50., 15.]).str("LST").u32(0).u32(200).u32(1).u32(0).str("");
    w.u8(0).f32s(&[0., 0., 0.]).f32s(&[0., 0., 0., 1.]);
    w.0
}

impl Fixture {
    /// The spec's "Synthetic fixture".
    pub fn spec() -> Fixture {
        let mut sock = W::default();
        sock.u32(0).str("Flash").f32s(&[0., 0., 0., 1.]).f32s(&[0., 0., 10.]).f32s(&[1., 1., 1.]);
        Fixture {
            version: 25,
            counts: [1, 1, 1, 1, 1, 1, 3, 3, 1, 1, 0, 4, 22, 0, 28],
            pieces: vec![("Body".into(), lod(4, &rigid_body(), &[0]))],
            nodes: (node("Scene Root", 0, 0, IDENTITY, &[]), 1),
            weight_sets: vec![],
            children: vec![],
            animations: vec![lst_record()],
            sockets: (sock.0, 1),
            bindings: vec![vec![("LST".into(), [15., 50., 15.], [0., -50., 0.])]],
        }
    }

    pub fn bytes(&self) -> Vec<u8> {
        let mut w = W::default();
        w.u8(1).u8(0).u16(9);
        w.0.extend([0u8; 16]);
        w.u32(self.version);
        for c in self.counts {
            w.u32(c);
        }
        w.str("").f32s(&[10.0]).u32(0).u32(self.pieces.len() as u32);
        for (name, lod) in &self.pieces {
            w.str(name).u32(1).f32s(&[0.0]).u32(0).u32(0);
            w.0.extend(lod);
        }
        w.0.extend(&self.nodes.0);
        w.u32(self.weight_sets.len() as u32);
        for (name, ws) in &self.weight_sets {
            w.str(name).u32(ws.len() as u32).f32s(ws);
        }
        w.u32(self.children.len() as u32 + 1);
        for c in &self.children {
            w.str(c);
        }
        w.u32(self.animations.len() as u32);
        for a in &self.animations {
            w.0.extend(a);
        }
        w.u32(self.sockets.1);
        w.0.extend(&self.sockets.0);
        for slot in &self.bindings {
            w.u32(slot.len() as u32);
            for (name, d, t) in slot {
                w.str(name).f32s(d).f32s(t);
            }
        }
        w.0
    }
}

fn close3(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3)
}

/// Test case 19: the synthetic fixture.
#[test]
fn case_19_synthetic_fixture() {
    let b = Fixture::spec().bytes();
    assert_eq!(b.len(), 540);
    let m = Model::parse(&b).unwrap();
    assert_eq!(m.version, 25);
    assert_eq!(m.counts.track_bytes, 28);
    assert_eq!(m.track_bytes(), 28);
    assert_eq!(m.pieces.len(), 1);
    let p = &m.pieces[0];
    assert_eq!((p.name.as_str(), p.lods.len()), ("Body", 1));
    let l = &p.lods[0];
    assert_eq!(l.body_size, 138);
    let Mesh::Rigid { node, geometry } = &l.mesh else { panic!("not rigid") };
    assert_eq!((*node, geometry.positions.len(), geometry.triangles.len()), (0, 3, 1));
    assert_eq!(geometry.stride, 32);
    assert_eq!(m.nodes.len(), 1);
    assert_eq!((m.nodes[0].name.as_str(), m.nodes[0].parent), ("Scene Root", None));
    assert!(m.weight_sets.is_empty() && m.children.is_empty());
    assert_eq!(m.animations[0].name, "LST");
    assert_eq!((m.animation_dims(0), m.animation_translation(0)), ([15., 50., 15.], [0., -50., 0.]));
    assert_eq!(m.sockets[0].name, "Flash");
    let s = m.socket_bind(0);
    assert!(close3([s[0][3], s[1][3], s[2][3]], [0., 0., 10.]));
    assert_eq!(ltb::to_rh_vec(m.sockets[0].position), [0., 0., -10.]);
    assert_eq!(m.vertex_bounds(), Some(([0., 0., 0.], [10., 10., 0.])));
    // The triangle's (p1 - p0) x (p2 - p0) = (0, 0, 100), along the normals.
    let g = geometry;
    let [a, b2, c] = g.triangles[0].map(|i| g.positions[i as usize]);
    let (e1, e2) = ([b2[0] - a[0], b2[1] - a[1], b2[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
    let cross = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
    assert_eq!(cross, [0., 0., 100.]);
    assert_eq!(ltb::to_rh_triangle(g.triangles[0]), [0, 2, 1]);
    assert_eq!(ltb::to_rh_vec(g.normals[0]), [0., 0., -1.]);
    // The combined list and the hull boxes: `LST` found, `LC` missing.
    let list = anim::AnimList::build(&m, &[]);
    assert_eq!(list.find("lst"), Some(0));
    assert_eq!(list.hull_boxes(), (ltb::FALLBACK_STAND, ltb::FALLBACK_CROUCH));
}

/// Test case 20: refusals, each a modified copy of the fixture.
#[test]
fn case_20_refusals() {
    let good = Fixture::spec().bytes();
    let refuse = |b: &[u8], what: &str| {
        assert!(Model::parse(b).is_err(), "{what} was accepted");
    };
    let mut b = good.clone();
    b[0] = 5;
    refuse(&b, "file type 5");
    let mut b = good.clone();
    b[20..24].copy_from_slice(&22u32.to_le_bytes());
    refuse(&b, "version 22");
    let mut b = good.clone();
    b[94..98].copy_from_slice(&65u32.to_le_bytes());
    refuse(&b, "65 pieces");
    let mut b = good.clone();
    b[145..149].copy_from_slice(&8u32.to_le_bytes());
    refuse(&b, "mesh kind 8");
    let mut b = good.clone();
    b[380..384].copy_from_slice(&0u32.to_le_bytes());
    refuse(&b, "child count 0");
    let mut f = Fixture::spec();
    f.weight_sets = vec![("upper".into(), vec![])];
    refuse(&f.bytes(), "a weight set with fewer weights than nodes");
    let mut b = good.clone();
    b[456..460].copy_from_slice(&1u32.to_le_bytes());
    refuse(&b, "a socket on node 1 of 1");
    let mut b = good.clone();
    b[2..4].copy_from_slice(&8u16.to_le_bytes());
    refuse(&b, "container version 8");
    for cut in 0..good.len() {
        refuse(&good[..cut], &format!("cut at {cut}"));
    }
    // Ours: trailing bytes, a wrong body size, a triangle index past V,
    // a rigid LOD whose stream flags aren't 0x13.
    let mut b = good.clone();
    b.push(0);
    refuse(&b, "a trailing byte");
    let mut b = good.clone();
    b[285..287].copy_from_slice(&3u16.to_le_bytes());
    refuse(&b, "triangle index 3 of 3 vertices");
    let mut b = good.clone();
    b[169..173].copy_from_slice(&0x17u32.to_le_bytes());
    refuse(&b, "stream flags 0x17");
    assert!(Model::parse(&good).is_ok());
}

/// Test case 21: conversion.
#[test]
fn case_21_conversion() {
    assert_eq!(ltb::to_rh_vec([1., 2., 3.]), [1., 2., -3.]);
    assert_eq!(ltb::to_rh_vec([0., 0., 1.]), [0., 0., -1.]);
    assert_eq!(ltb::to_rh_triangle([0, 1, 2]), [0, 2, 1]);
    assert_eq!(ltb::to_rh_quat([0.1, 0.2, 0.3, 0.927]), [-0.1, -0.2, 0.3, 0.927]);
    let t = [[1., 0., 0., 1.], [0., 1., 0., 2.], [0., 0., 1., 3.], [0., 0., 0., 1.]];
    let c = ltb::to_rh_mat(&t);
    assert_eq!([c[0][3], c[1][3], c[2][3]], [1., 2., -3.]);
    // Test case 2's `flagpole01` bind matrix.
    let f = [[1., 0., 0., 0.], [0., 0., 1., 0.], [0., -1., 0., 0.], [0., 0., 0., 1.]];
    assert_eq!(
        ltb::to_rh_mat(&f),
        [[1., 0., 0., 0.], [0., 0., -1., 0.], [0., 1., 0., 0.], [0., 0., 0., 1.]]
    );
    // The converted quaternion is the converted matrix's rotation (S R S).
    let q = [0.1f32, 0.2, 0.3, 0.927];
    let m = ltb::to_rh_mat(&anim::matrix([0.; 3], q));
    let r = anim::matrix([0.; 3], ltb::to_rh_quat(q));
    for i in 0..3 {
        assert!(close3(m[i][..3].try_into().unwrap(), r[i][..3].try_into().unwrap()));
    }
}

/// Test case 14: LOD choice.
#[test]
fn case_14_lod_choice() {
    let d = [0.0, 400.0, 800.0];
    for (dist, want) in [(0.0, 0), (399.9, 0), (400.0, 1), (799.9, 1), (800.0, 2), (5000.0, 2)] {
        assert_eq!(ltb::lod_index(&d, dist, 0), want, "d = {dist}");
    }
    assert_eq!(ltb::lod_index(&d, 0.0, 1), 1);
    assert_eq!(ltb::lod_index(&d, 800.0, 1), 2);
    assert_eq!(ltb::lod_index(&d, 400.0, -1), 0);
    assert_eq!(ltb::lod_index(&[0.0], 1000.0, 0), 0);
    assert_eq!(ltb::lod_index(&[0.0], 0.0, 1), 0);
}

/// A bone-set skinned body: 4 vertices in two sets (nodes 0, 1 and node
/// 1), 2 triangles, Bt = 2 (one stored weight).
fn skinned_body() -> Vec<u8> {
    let mut w = W::default();
    w.u32(4).u32(2).u32(2).u32(2).u8(0).u32(0x13).u32(0).u32(0).u32(0).u8(0);
    for (i, weight) in [0.25f32, 0.5, 0.0, 0.0].into_iter().enumerate() {
        w.f32s(&[i as f32, 0., 0.]).f32s(&[weight]).f32s(&[0., 0., 1., 0., 0.]);
    }
    w.u16(0).u16(1).u16(0).u16(1).u16(1).u16(0);
    w.u32(2);
    w.u16(0).u16(2).u8(0).u8(1).u8(0xFF).u8(0xFF).u32(3);
    w.u16(2).u16(2).u8(1).u8(0xFF).u8(0xFF).u8(0xFF).u32(6);
    // Second set's triangle (2, 3, 2) fixed below.
    let mut b = w.0;
    // Triangles start after 34 bytes of header and 4 x 36 of vertices.
    let t = 34 + 4 * 36;
    b[t + 6..t + 12].copy_from_slice(&[2, 0, 3, 0, 2, 0]);
    b
}

/// Test cases 16 and 18 on a fixture: bone sets tile, the body size rule,
/// the implied last weight.
#[test]
fn bone_set_mesh() {
    let mut f = Fixture::spec();
    let body = skinned_body();
    assert_eq!(body.len(), 34 + 4 * 36 + 6 * 2 + 4 + 12 * 2);
    f.pieces = vec![("skin".into(), lod(5, &body, &[0, 1]))];
    f.nodes = (node("Scene Root", 0, 0, IDENTITY, &[node("b", 1, 0, IDENTITY, &[])]), 2);
    let mut rec = W::default();
    rec.f32s(&[15., 50., 15.]).str("LST").u32(0).u32(200).u32(1).u32(0).str("");
    for _ in 0..2 {
        rec.u8(0).f32s(&[0., 0., 0.]).f32s(&[0., 0., 0., 1.]);
    }
    f.animations = vec![rec.0];
    let m = Model::parse(&f.bytes()).unwrap();
    assert_eq!(m.track_bytes(), 56);
    let mesh = &m.pieces[0].lods[0].mesh;
    let Mesh::Skinned { bone_sets, geometry, palette, .. } = mesh else { panic!() };
    assert!(palette.is_none());
    assert_eq!((bone_sets.len(), geometry.stride, geometry.weight_count), (2, 36, 1));
    let inf = mesh.vertex_influences();
    assert_eq!(inf[0][..2], [(0, 0.25), (1, 0.75)]);
    assert_eq!(inf[1][..2], [(0, 0.5), (1, 0.5)]);
    assert_eq!(inf[2][0], (1, 1.0));

    // A set that doesn't start where the last ended is refused.
    let mut bad = body.clone();
    let sets = 34 + 4 * 36 + 12 + 4;
    bad[sets + 12..sets + 14].copy_from_slice(&3u16.to_le_bytes());
    f.pieces = vec![("skin".into(), lod(5, &bad, &[0, 1]))];
    assert!(Model::parse(&f.bytes()).is_err());
    // An index outside its set's vertex range is refused.
    let mut bad = body.clone();
    let t = 34 + 4 * 36;
    bad[t..t + 2].copy_from_slice(&2u16.to_le_bytes());
    f.pieces = vec![("skin".into(), lod(5, &bad, &[0, 1]))];
    assert!(Model::parse(&f.bytes()).is_err());
}

/// A re-indexed palette mesh (Bv = 2): palette entries 0, 1 drive nodes
/// 1, 0.
#[test]
fn palette_mesh() {
    let mut w = W::default();
    w.u32(3).u32(1).u32(2).u32(2).u8(1).u32(0x13).u32(0).u32(0).u32(0).u8(1);
    w.u32(0).u32(1).u32(2).u32(1).u32(0);
    for i in 0..3 {
        w.f32s(&[i as f32, 0., 0.]).f32s(&[0.3]).u8(0).u8(1).u8(0).u8(0).f32s(&[0., 0., 1., 0., 0.]);
    }
    w.u16(0).u16(1).u16(2);
    assert_eq!(w.len(), 34 + 8 + 4 + 8 + 3 * 40 + 6);
    let mut f = Fixture::spec();
    f.pieces = vec![("p".into(), lod(5, &w.0, &[0, 1]))];
    f.nodes = (node("Scene Root", 0, 0, IDENTITY, &[node("b", 1, 0, IDENTITY, &[])]), 2);
    let mut rec = W::default();
    rec.f32s(&[15., 50., 15.]).str("LST").u32(0).u32(200).u32(1).u32(0).str("");
    for _ in 0..2 {
        rec.u8(0).f32s(&[0., 0., 0.]).f32s(&[0., 0., 0., 1.]);
    }
    f.animations = vec![rec.0];
    let m = Model::parse(&f.bytes()).unwrap();
    let mesh = &m.pieces[0].lods[0].mesh;
    let Mesh::Skinned { palette: Some(p), geometry, .. } = mesh else { panic!() };
    assert_eq!((p.remap.clone(), geometry.stride), (vec![1, 0], 40));
    let inf = mesh.vertex_influences();
    assert_eq!(inf[0][..2], [(1, 0.3), (0, 0.7)]);
}

/// A vertex-animated piece (body size rule of case 18) with its node's
/// vertex track (animation.md "Vertex animation").
#[test]
fn vertex_animated_mesh() {
    let mut w = W::default();
    w.u32(4).u32(3).u32(2).u32(0).u32(0).u32(0x13).u32(0).u32(0).u32(0).u32(1).u32(0);
    for i in 0..4 {
        w.f32s(&[i as f32, 0., 0., 0., 0., 1., 0., 0.]);
    }
    w.u16(0).u16(1).u16(2).u16(0).u16(2).u16(3);
    w.u32(1).u16(0).u16(3);
    assert_eq!(w.len(), 44 + 32 * 4 + 6 * 2 + 4 + 4);
    let mut f = Fixture::spec();
    f.pieces = vec![("screen".into(), lod(6, &w.0, &[]))];
    f.nodes = (node("Scene Root", 0, 0, IDENTITY, &[node("d", 1, 0, IDENTITY, &[])]), 2);
    let mut rec = W::default();
    rec.f32s(&[1., 1., 1.]).str("a").u32(0).u32(0).u32(2).u32(0).str("").u32(100).str("");
    rec.u8(0).f32s(&[0.; 6]).f32s(&[0., 0., 0., 1., 0., 0., 0., 1.]);
    rec.u8(1);
    for k in 0..2 {
        rec.u32(3);
        for v in 0..3 {
            rec.f32s(&[v as f32, k as f32 * 10.0, 0.]);
        }
    }
    f.animations = vec![rec.0];
    f.bindings = vec![vec![("a".into(), [1., 1., 1.], [0.; 3])]];
    let m = Model::parse(&f.bytes()).unwrap();
    assert_eq!((m.vertex_anim_bytes(), m.track_bytes()), (72, 56));
    let Mesh::VertexAnimated { animated_vertices, anim_node, duplicates, .. } = &m.pieces[0].lods[0].mesh else {
        panic!()
    };
    assert_eq!((*animated_vertices, *anim_node, duplicates.clone()), (3, 1, vec![(0, 3)]));
    assert!(matches!(m.animations[0].channels[1], Channel::Vertices(_)));
    let mid = anim::vertex_frame(&m.animations[0], 1, 0, 1, 0.5).unwrap();
    assert_eq!(mid, vec![[0., 5., 0.], [1., 5., 0.], [2., 5., 0.]]);
}

/// Child models and their slots' bindings ("Child models", "Animation
/// bindings"); a weight set of one weight per node.
#[test]
fn children_and_weight_sets() {
    let mut f = Fixture::spec();
    f.children = vec!["st_m_child.ltb".into()];
    f.weight_sets = vec![("upper".into(), vec![1.0])];
    f.bindings.push(vec![]);
    let m = Model::parse(&f.bytes()).unwrap();
    assert_eq!(m.children, ["st_m_child.ltb"]);
    assert_eq!(m.weight_sets[0].weights, [1.0]);
    assert_eq!(m.bindings.len(), 2);
}

/// "Model properties": the shadow command, and misspellings that don't
/// count ("Quirks").
#[test]
fn shadow_command() {
    let b = Fixture::spec().bytes();
    let with = |cmd: &str| {
        let mut out = b[..84].to_vec();
        out.extend((cmd.len() as u16).to_le_bytes());
        out.extend(cmd.as_bytes());
        out.extend(&b[86..]);
        Model::parse(&out).unwrap().shadow_enabled()
    };
    assert!(with("ShadowEnable"));
    assert!(with(" shadowenable "));
    assert!(!with("ShadoeEnable"));
    assert!(!with("ShadowEnable foo"));
}
