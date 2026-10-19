//! The model reader and animation playback on the user's Combat Arms
//! install, against the test cases of specs/combat_arms/models_ltb.md
//! and animation.md (Steam build 25595979) that unkeyed archives allow.
//! Cases on V2-keyed archives (models 8-11 and 13, animation 11-14) run
//! only when the config has a V2 key; without one they pass with a note.
//! Everything passes with a note without an install.

use std::collections::BTreeMap;

use lithtech_formats::{
    anim::{self, AnimList, Tracker},
    ltb::{self, Channel, Mesh, Model},
    rez::{RezArchive, RezKeys},
};
use source_formats::fs::FileSource;

use super::rez_install::{game_dir, keys};

fn open(archive: &str) -> Option<RezArchive> {
    let rez = RezArchive::open(game_dir()?.join(archive), &keys()).unwrap();
    if rez.locked() {
        eprintln!("skipping: {archive} needs the {:?} key", rez.header().key_scheme());
        return None;
    }
    Some(rez)
}

fn read(archive: &str, path: &str) -> Option<Vec<u8>> {
    Some(open(archive)?.read(path).unwrap().unwrap())
}

fn model(archive: &str, path: &str) -> Option<(usize, Model)> {
    let b = read(archive, path)?;
    let m = Model::parse(&b).unwrap_or_else(|e| panic!("{path}: {e}"));
    Some((b.len(), m))
}

fn close(a: &[f32], b: &[f32], tol: f32) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < tol)
}

fn parent_name(m: &Model, name: &str) -> Option<String> {
    let i = m.node(name)?;
    Some(m.nodes[m.nodes[i].parent?].name.clone())
}

fn geometry(m: &Model, piece: usize) -> &ltb::Geometry {
    m.pieces[piece].lods[0].mesh.geometry().unwrap()
}

fn vt(m: &Model, piece: usize) -> (usize, usize) {
    let g = geometry(m, piece);
    (g.positions.len(), g.triangles.len())
}

/// The unencrypted archives the census covers (models test case 1).
const CENSUS: &[&str] = &[
    "PROPS.rez",
    "MODELS.rez",
    "FX.rez",
    "GUNSMITH_NPC.rez",
    "GUNS_M_PV_ETC.rez",
    "GUNS_M_PV_LAUNCHER.rez",
    "GUNS_M_PV_MG.rez",
    "GUNS_M_PV_OPTION.rez",
    "GUNS_M_PV_Pistol.rez",
    "GUNS_M_PV_SG.rez",
    "GUNS_PT.rez",
    "GUNS_SC.rez",
    "PS.rez",
    "RS.rez",
];

/// Models case 1 and animation cases 17 (install statistics): every
/// `.ltb` of the unencrypted archives.
#[test]
fn census() {
    let Some(game) = game_dir() else { return };
    let mut versions = BTreeMap::new();
    let mut kinds = BTreeMap::new();
    let mut refused = BTreeMap::new();
    let (mut models, mut pieces, mut lods, mut multi_lod) = (0, 0, 0, 0);
    let mut compressions = BTreeMap::new();
    let mut interps = BTreeMap::new();
    let (mut anims, mut with_children, mut with_sets, mut short) = (0, 0, 0, 0);
    let mut counts = BTreeMap::new();
    let mut vertex_nodes = 0;
    let mut vertex_files = std::collections::BTreeSet::new();
    for archive in CENSUS {
        let rez = RezArchive::open(game.join(archive), &RezKeys::new()).unwrap();
        assert!(rez.header().key_scheme().is_none(), "{archive} is keyed");
        for e in rez.entries().into_iter().filter(|e| e.path.ends_with(".ltb")) {
            let b = rez.read(&e.path).unwrap().unwrap();
            let m = match Model::parse(&b) {
                Ok(m) => m,
                Err(err) => {
                    let why = if b.first() == Some(&5) {
                        "render style"
                    } else if b.first() != Some(&1) {
                        "not a model"
                    } else {
                        "old version"
                    };
                    *refused.entry(why).or_insert(0) += 1;
                    if why == "old version" {
                        assert_eq!(e.path, "fx/waterfall2.ltb", "{err}");
                    }
                    continue;
                }
            };
            models += 1;
            assert_eq!(&b[..4], &[1, 0, 9, 0]);
            assert!(b[4..20].iter().all(|&x| x == 0));
            *versions.entry(m.version).or_insert(0) += 1;
            assert_eq!(m.counts.track_bytes as usize, m.track_bytes(), "{}", e.path);
            assert_eq!(m.counts.vertex_anim_bytes as usize, m.vertex_anim_bytes(), "{}", e.path);
            assert_eq!(m.counts.nodes as usize, m.nodes.len(), "{}", e.path);
            assert_eq!(m.counts.pieces as usize, m.pieces.len(), "{}", e.path);
            assert_eq!(m.counts.animations as usize, m.animations.len(), "{}", e.path);
            assert_eq!(m.counts.sockets as usize, m.sockets.len(), "{}", e.path);
            let keys: usize = m.animations.iter().map(|a| a.keys.len()).sum();
            assert_eq!(m.counts.keyframes as usize, keys, "{}", e.path);
            for p in &m.pieces {
                pieces += 1;
                lods += p.lods.len();
                multi_lod += usize::from(p.lods.len() > 1);
                for l in &p.lods {
                    assert_eq!(l.mesh.geometry().map_or([0; 4], |g| g.stream_flags), [0x13, 0, 0, 0]);
                    if let Mesh::Skinned { palette, .. } = &l.mesh {
                        assert!(palette.is_none(), "{}", e.path);
                    }
                    *kinds.entry(l.mesh.kind()).or_insert(0) += 1;
                }
            }
            with_children += usize::from(!m.children.is_empty());
            with_sets += usize::from(!m.weight_sets.is_empty());
            for a in &m.animations {
                anims += 1;
                short += usize::from(a.keys.len() <= 1);
                *compressions.entry(a.compression).or_insert(0) += 1;
                *interps.entry(a.interp_ms).or_insert(0) += 1;
                if let Some(k) = a.keys.first() {
                    assert_eq!(k.time, 0, "{} {}", e.path, a.name);
                }
                for c in &a.channels {
                    match c {
                        Channel::Transform { positions, rotations } => {
                            for n in [positions.len(), rotations.len()] {
                                let what = match n {
                                    0 => "0",
                                    1 => "1",
                                    n if n == a.keys.len() => "N",
                                    _ => "other",
                                };
                                *counts.entry(what).or_insert(0) += 1;
                            }
                        }
                        Channel::Vertices(_) => {
                            vertex_nodes += 1;
                            vertex_files.insert(e.path.clone());
                        }
                    }
                }
            }
        }
    }
    eprintln!(
        "models {models} {versions:?} pieces {pieces} lods {lods} kinds {kinds:?} refused {refused:?}; animations {anims} \
         compression {compressions:?} interpolation {interps:?} children {with_children} weight sets {with_sets} \
         0/1-key {short} channel counts {counts:?} vertex nodes {vertex_nodes}"
    );
    assert_eq!(models, 815);
    assert_eq!(versions, BTreeMap::from([(23, 161), (25, 654)]));
    assert_eq!((pieces, lods, multi_lod), (2713, 2713, 0));
    assert_eq!(kinds, BTreeMap::from([(4, 1733), (5, 858), (6, 122)]));
    assert_eq!(
        refused,
        BTreeMap::from([("render style", 65), ("not a model", 6), ("old version", 1)])
    );
    // Animation case 17.
    assert_eq!(anims, 2622);
    assert_eq!(compressions, BTreeMap::from([(0, 806), (1, 1), (2, 115), (3, 1700)]));
    assert_eq!(interps, BTreeMap::from([(0, 1955), (10, 27), (50, 3), (200, 637)]));
    assert_eq!((with_children, with_sets, short), (181, 26, 407));
    assert_eq!(counts.get("other"), None);
    // "0 only on the 244 vertex-animated nodes (38 files)": 122 vertex
    // channels, each counted as a position and a rotation channel of 0.
    assert_eq!((2 * vertex_nodes, vertex_files.len()), (244, 38));
}

/// Models cases 2, 3, 15 (flagpole part) and 18.
#[test]
fn case_2_3_flagpole() {
    let Some((len, m)) = model("PROPS.rez", "props/headquaters/hq_flagpole01.ltb") else { return };
    assert_eq!((len, m.version), (4_873, 25));
    assert!(close(&[m.radius], &[270.872], 1e-3));
    assert_eq!(m.pieces.len(), 1);
    let p = &m.pieces[0];
    assert_eq!((p.name.as_str(), p.lods.len()), ("HQ_flagpole01", 1));
    let l = &p.lods[0];
    let Mesh::Rigid { node, .. } = &l.mesh else { panic!() };
    assert_eq!((*node, vt(&m, 0), l.body_size), (1, (116, 86), 4_264));
    assert_eq!(l.body_size as usize, 36 + 32 * 116 + 6 * 86);
    assert_eq!((l.used_textures(), l.render_style), (&[0][..], 0));
    assert_eq!(m.nodes.len(), 2);
    assert_eq!((m.nodes[0].name.as_str(), m.nodes[0].parent), ("Scene Root", None));
    assert_eq!((m.nodes[1].name.as_str(), m.nodes[1].parent, m.nodes[1].flags), ("flagpole01", Some(0), 2));
    assert_eq!(
        m.nodes[1].bind,
        [[1., 0., 0., 0.], [0., 0., 1., 0.], [0., -1., 0., 0.], [0., 0., 0., 1.]]
    );
    let names: Vec<_> = m.sockets.iter().map(|s| (s.name.as_str(), s.node)).collect();
    assert_eq!(names, [("HQ_pole_top", 1), ("HQ_pole_bottom", 1)]);
    assert!(close(&m.sockets[0].position, &[0., 0., 252.], 1e-3));
    assert!(close(&m.sockets[1].position, &[0., 0., 4.], 1e-3));
    assert!(close(&m.sockets[0].rotation, &[0.707, 0., 0., 0.707], 1e-3));
    assert_eq!(m.animations.len(), 1);
    let a = &m.animations[0];
    assert_eq!((a.name.as_str(), a.keys.len()), ("HQ_Flagpole01", 1));
    assert!(close(&m.animation_dims(0), &[10., 270., 10.], 1e-3));
    assert_eq!(m.animation_translation(0), [0.; 3]);
    let (lo, hi) = m.vertex_bounds().unwrap();
    assert!(close(&lo, &[-2.146, 0., -1.858], 1e-3) && close(&hi, &[2.146, 268.190, 1.858], 1e-3));
    // Case 3: sockets in model space at bind pose, before and after
    // conversion.
    for (s, want) in [(0, [0., 252., 0.]), (1, [0., 4., 0.])] {
        let b = m.socket_bind(s);
        let at = [b[0][3], b[1][3], b[2][3]];
        assert!(close(&at, &want, 1e-3), "{at:?}");
        assert!(close(&ltb::to_rh_vec(at), &want, 1e-3));
    }
    winding(&m);
}

/// Models case 15: the cross product of every triangle agrees with its
/// vertex normals, before and after conversion.
fn winding(m: &Model) -> usize {
    let mut n = 0;
    for p in &m.pieces {
        let g = p.lods[0].mesh.geometry().unwrap();
        for t in &g.triangles {
            for converted in [false, true] {
                let (t, f) = if converted {
                    (ltb::to_rh_triangle(*t), ltb::to_rh_vec as fn(_) -> _)
                } else {
                    (*t, (|v| v) as fn(_) -> _)
                };
                let [a, b, c] = t.map(|i| f(g.positions[i as usize]));
                let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                let x = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
                let ns = t.map(|i| f(g.normals[i as usize]));
                let sum: [f32; 3] = std::array::from_fn(|k| ns[0][k] + ns[1][k] + ns[2][k]);
                assert!(x[0] * sum[0] + x[1] * sum[1] + x[2] * sum[2] > 0.0);
            }
            n += 1;
        }
    }
    n
}

/// Models case 15: the other two models.
#[test]
fn case_15_winding() {
    let Some((_, m)) = model("GUNS_M_PV_Pistol.rez", "guns_m_pv_pistol/pv_pst_m92fs_sh.ltb") else { return };
    assert_eq!(winding(&m), 1080);
    let Some((_, m)) = model("MODELS.rez", "models/sphere.ltb") else { return };
    assert_eq!(winding(&m), 840);
}

/// Models cases 4, 16 and 18: the headquarters flag (skinned, bone sets).
#[test]
fn case_4_flag() {
    let Some((len, m)) = model("PROPS.rez", "props/headquaters/hq_flag01.ltb") else { return };
    assert_eq!((len, m.version, m.radius), (19_572, 25, 96.0));
    assert_eq!(m.pieces.len(), 1);
    let l = &m.pieces[0].lods[0];
    let Mesh::Skinned { geometry: g, bone_sets, palette: None, .. } = &l.mesh else { panic!() };
    assert_eq!((m.pieces[0].name.as_str(), g.max_bones_per_triangle, g.max_bones_per_vertex), ("flag", 3, 3));
    assert_eq!((g.stride, g.positions.len(), g.triangles.len()), (40, 56, 60));
    let sets: Vec<_> = bone_sets
        .iter()
        .map(|s| (s.first_vertex, s.vertex_count, s.used().collect::<Vec<_>>(), s.index_end))
        .collect();
    assert_eq!(sets, [(0, 32, vec![2, 3, 4], 108), (32, 24, vec![1, 2, 3], 180)]);
    assert_eq!(l.body_size as usize, 34 + 56 * 40 + 6 * 60 + 4 + 12 * 2);
    assert_eq!(l.body_size, 2_662);
    let tree: Vec<_> = m.nodes.iter().map(|n| (n.name.as_str(), n.parent)).collect();
    assert_eq!(
        tree,
        [
            ("Scene Root", None),
            ("D_flag", Some(0)),
            ("Bone01", Some(1)),
            ("Bone02", Some(2)),
            ("Bone03", Some(3))
        ]
    );
    assert_eq!((m.sockets.len(), m.sockets[0].name.as_str(), m.sockets[0].node), (1, "HQ_flag_C", 1));
    assert!(close(&m.sockets[0].rotation, &[0., 0., 0., 0.707], 1e-3));
    let a = &m.animations[0];
    assert_eq!((a.name.as_str(), a.keys.len(), a.compression), ("HQ_flag01", 111, 0));
    assert!(close(&m.animation_dims(0), &[66.2, 32., 41.4], 1e-3));
    assert_eq!((m.counts.keyframes, m.counts.track_bytes), (111, 15_540));
    let (lo, hi) = m.vertex_bounds().unwrap();
    assert!(close(&lo, &[1.2, 252., 0.], 1e-3) && close(&hi, &[71.2, 300., 0.], 1e-3));
    // Case 16: stored weights sum to at most 1.
    for w in &g.weights {
        assert!(w.iter().sum::<f32>() <= 1.0 + 1e-5);
    }
    for inf in l.mesh.vertex_influences() {
        assert!((inf.iter().map(|x| x.1).sum::<f32>() - 1.0).abs() < 1e-5);
    }
}

/// Models cases 5 and 18, animation "Vertex animation": the laptop.
#[test]
fn case_5_laptop() {
    let Some((len, m)) = model("PROPS.rez", "props/sand_hog/sh_laptop.ltb") else { return };
    assert_eq!((len, m.version), (213_233, 25));
    assert!(close(&[m.radius], &[27.252], 1e-3));
    let names: Vec<_> = m.pieces.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        ["d_Normal_Screen", "d_HackScreen", "d_CopyScreen", "d_Gauge", "d_ConfirmScreen", "Laptop_Body"]
    );
    for (i, p) in m.pieces[..5].iter().enumerate() {
        let l = &p.lods[0];
        let Mesh::VertexAnimated { animated_vertices, anim_node, duplicates, .. } = &l.mesh else { panic!() };
        assert_eq!((vt(&m, i), *animated_vertices, duplicates.len()), ((4, 2), 4, 0));
        assert_eq!((l.body_size, *anim_node), (188, i as u32 + 1));
    }
    let Mesh::Rigid { node, .. } = &m.pieces[5].lods[0].mesh else { panic!() };
    assert_eq!((vt(&m, 5), *node), ((125, 60), 6));
    assert_eq!(m.nodes.len(), 7);
    assert_eq!((m.nodes[0].name.as_str(), m.nodes[0].flags), ("Scene Root", 2));
    assert!(m.nodes[1..].iter().all(|n| n.parent == Some(0)));
    assert_eq!((m.nodes[1].name.as_str(), m.nodes[6].name.as_str()), ("d_Normal_Screen", "Laptop_Bone"));
    let a = &m.animations[0];
    assert_eq!((a.name.as_str(), a.keys.len()), ("Base_Anim", 642));
    assert!(close(&m.animation_dims(0), &[14.5, 21.8, 13.9], 1e-3));
    assert_eq!((m.counts.vertex_anim_bytes, m.counts.track_bytes), (154_080, 35_952));
    assert_eq!(m.vertex_anim_bytes(), 5 * 4 * 642 * 12);
}

/// The M92FS pair (models 6, 7, 17; animation 1-10).
fn m92fs() -> Option<(usize, Model, usize, Model)> {
    let (a, pv) = model("GUNS_M_PV_Pistol.rez", "guns_m_pv_pistol/pv_pst_m92fs_sh.ltb")?;
    let (b, ani) = model("GUNS_M_PV_Pistol.rez", "guns_m_pv_pistol/ani_pst_m92fs_sh.ltb")?;
    Some((a, pv, b, ani))
}

/// Models cases 6, 7 and 17; animation cases 1 and 2.
#[test]
fn m92fs_files() {
    let Some((len, pv, alen, ani)) = m92fs() else { return };
    assert_eq!((len, pv.version), (60_570, 25));
    let pieces: Vec<_> = (0..pv.pieces.len())
        .map(|i| {
            let Mesh::Rigid { node, .. } = &pv.pieces[i].lods[0].mesh else { panic!() };
            (pv.pieces[i].name.as_str(), vt(&pv, i), *node)
        })
        .collect();
    assert_eq!(
        pieces,
        [
            ("Object04", (7, 5), 24),
            ("Object05", (42, 34), 26),
            ("Object02", (1001, 708), 24),
            ("Object06", (322, 254), 28),
            ("Object07", (108, 79), 31)
        ]
    );
    assert_eq!(pv.nodes.len(), 51);
    assert_eq!(parent_name(&pv, "Bip01").as_deref(), Some("Scene Root"));
    assert_eq!(parent_name(&pv, "Bip01 R Hand").as_deref(), Some("Bip01 R Forearm"));
    assert_eq!(parent_name(&pv, "Box06").as_deref(), Some("Box05"));
    let socks: Vec<_> = pv.sockets.iter().map(|s| (s.name.as_str(), s.node, s.position)).collect();
    assert_eq!(
        socks,
        [("flash", 27, [0., 0.078125, 0.640625]), ("breach", 27, [0.171875, 0.0625, -0.140625])]
    );
    assert_eq!(pv.nodes[27].name, "Box06");
    assert!(pv.weight_sets.is_empty());
    assert_eq!(pv.animations.len(), 1);
    let a = &pv.animations[0];
    assert_eq!((a.name.as_str(), a.keys.len(), a.compression, a.interp_ms), ("CM_HND_SP_DF_Beretta_M92F_FS_CH", 1, 0, 200));
    assert_eq!(a.keys[0].time, 0);
    for c in &a.channels {
        let Channel::Transform { positions, rotations } = c else { panic!() };
        assert_eq!((positions.len(), rotations.len()), (1, 1));
    }
    assert_eq!((pv.animation_dims(0), pv.animation_translation(0)), ([2., 2., 2.], [0.; 3]));
    assert_eq!(pv.children, ["Ani_PST_M92FS_SH.ltb"]);
    assert_eq!(pv.bindings.len(), 2);
    assert!(pv.bindings[1].is_empty());
    let (lo, hi) = pv.vertex_bounds().unwrap();
    assert!(close(&lo, &[4.131, 3.206, 0.049], 1e-3) && close(&hi, &[5.397, 3.968, 0.372], 1e-3));

    // Case 7 / animation case 2.
    assert_eq!((alen, ani.version), (191_067, 25));
    let pieces: Vec<_> = (0..ani.pieces.len())
        .map(|i| {
            assert!(matches!(ani.pieces[i].lods[0].mesh, Mesh::Skinned { .. }));
            (ani.pieces[i].name.as_str(), vt(&ani, i))
        })
        .collect();
    assert_eq!(pieces, [("arm_SH", (1650, 1332)), ("Object01", (510, 608)), ("PV_COLT", (1095, 754))]);
    assert_eq!(ani.nodes.len(), 51);
    assert_eq!(pv.parents(), ani.parents());
    let differ: Vec<_> = (0..51).filter(|&i| pv.nodes[i].name != ani.nodes[i].name).collect();
    assert!(differ.iter().all(|i| (24..=31).contains(i)), "{differ:?}");
    assert_eq!(differ.len(), 8);
    assert_eq!((ani.counts.keyframes, ani.counts.animations, ani.counts.track_bytes), (92, 13, 41_192));
    assert!(ani.sockets.is_empty());
    let names: Vec<_> = ani.animations.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "select", "idle_0", "idle_1", "prefire", "fire_0", "postfire", "reload", "dryfire", "PreReload",
            "PostReload", "PreRush", "rush", "PostRush"
        ]
    );
    for i in 0..13 {
        let a = &ani.animations[i];
        assert_eq!((a.compression, a.interp_ms), (3, 0));
        assert_eq!(ani.animation_dims(i), [2., 2., 2.]);
        assert!(close(&ani.animation_translation(i), &[-0.4, -3.6, -1.2], 1e-6));
    }
}

/// Animation cases 3-8: the combined list, key times, strings, channel
/// counts and decoded values.
#[test]
fn m92fs_animations() {
    let Some((_, pv, _, ani)) = m92fs() else { return };
    let list = AnimList::build(&pv, &[Some(&ani)]);
    assert_eq!(list.entries.len(), 14);
    assert_eq!(list.entries[0].name(), "CM_HND_SP_DF_Beretta_M92F_FS_CH");
    let want = [
        ("select", 19, 650),
        ("idle_0", 3, 1566),
        ("idle_1", 3, 1566),
        ("prefire", 2, 0),
        ("fire_0", 5, 180),
        ("postfire", 2, 150),
        ("reload", 29, 1500),
        ("dryfire", 6, 300),
        ("PreReload", 6, 300),
        ("PostReload", 6, 300),
        ("PreRush", 4, 250),
        ("rush", 3, 1500),
        ("PostRush", 4, 400),
    ];
    for (i, (name, keys, len)) in want.into_iter().enumerate() {
        let e = &list.entries[i + 1];
        assert_eq!((e.name(), e.animation.keys.len(), e.length()), (name, keys, len));
    }
    assert_eq!((list.find("SELECT"), list.find("dryFire")), (Some(1), Some(8)));
    assert_eq!((list.find("deselect"), list.find("alt_fire_0")), (None, None));
    let times = |n: &str| -> Vec<u32> { list.entries[list.find(n).unwrap()].animation.keys.iter().map(|k| k.time).collect() };
    assert_eq!(
        times("select"),
        [0, 98, 157, 197, 216, 236, 275, 296, 335, 354, 393, 433, 453, 492, 532, 552, 591, 610, 650]
    );
    assert_eq!(times("fire_0"), [0, 29, 52, 103, 180]);
    assert_eq!(times("PostRush"), [0, 167, 267, 400]);
    // Case 5: string events.
    let strings = |n: &str| -> Vec<(u32, String)> {
        list.entries[list.find(n).unwrap()]
            .animation
            .keys
            .iter()
            .filter(|k| !k.string.is_empty())
            .map(|k| (k.time, k.string.clone()))
            .collect()
    };
    assert_eq!(strings("select"), [(296, "sound_key 3".to_string())]);
    assert_eq!(strings("fire_0"), [(0, "FIRE_KEY".to_string())]);
    assert_eq!(
        strings("reload"),
        [(226, "sound_key 2".into()), (453, "sound_key 1".into()), (962, "sound_key 3".to_string())]
    );
    for n in ["dryfire", "PreReload", "PostReload"] {
        assert_eq!(strings(n), [(0, "dryfire_key".to_string())]);
    }
    let all: usize = list.entries.iter().map(|e| e.animation.keys.iter().filter(|k| !k.string.is_empty()).count()).sum();
    assert_eq!(all, 1 + 1 + 3 + 3);
    // Case 6: channel counts in `select`.
    let select = &list.entries[1].animation;
    let counts = |j: usize| match &select.channels[j] {
        Channel::Transform { positions, rotations } => (positions.len(), rotations.len()),
        Channel::Vertices(_) => panic!(),
    };
    assert_eq!(ani.nodes[6].name, "Bip01 R UpperArm");
    assert_eq!((counts(0), counts(6), counts(7), counts(9)), ((1, 1), (1, 19), (19, 19), (19, 1)));
    assert_eq!(select.key_value(0, 5), ([0.; 3], [0., 0., 0., 1.]));
    let (p, q) = select.key_value(1, 0);
    assert!(close(&p, &[0., 0.3553, 0.], 1e-4));
    assert!(close(&q, &[-0.013916, -0.510971, -0.023408, 0.859157], 1e-4));
    // Case 7: node 7 `Bip01 R Forearm`.
    assert_eq!(ani.nodes[7].name, "Bip01 R Forearm");
    let (p, q) = select.key_value(7, 1);
    assert!(close(&p, &[1.4803, 0., 0.], 1e-4) && close(&q, &[0., 0.5656, 0., 0.824671], 1e-4));
    let (_, q) = select.key_value(7, 2);
    assert!(close(&q, &[0., 0.594409, 0., 0.804132], 1e-4));
    let (a, b, u) = anim::key_span(select, 120.0);
    assert_eq!((a, b), (1, 2));
    assert!(close(&[u], &[22.0 / 59.0], 1e-6));
    let (p, q) = anim::sample(&list.entries[1], 7, 120.0);
    assert!(close(&p, &[1.4803, 0., 0.], 1e-4) && close(&q, &[0., 0.576431, 0., 0.817138], 1e-4));
    let (_, q) = anim::sample(&list.entries[1], 7, 650.0);
    assert!(close(&q, &[0., 0.685263, 0., 0.728263], 1e-4));
    // Case 8: node 29 `BOP06` in `reload`.
    let reload = list.find("reload").unwrap();
    let r = &list.entries[reload];
    let Channel::Transform { positions, rotations } = &r.animation.channels[29] else { panic!() };
    assert_eq!((ani.nodes[29].name.as_str(), positions.len(), rotations.len()), ("BOP06", 29, 1));
    assert_eq!(rotations[0], [0., 0., 0., 1.]);
    assert_eq!((r.animation.keys[5].time, r.animation.keys[6].time), (311, 425));
    assert!(close(&positions[5], &[0., -0.9404, -0.5573], 1e-4));
    assert!(close(&positions[6], &[0., -1.0358, -0.5743], 1e-4));
    let (p, _) = anim::sample(r, 29, 368.0);
    assert!(close(&p, &[0., -0.9881, -0.5658], 1e-4));
    for k in (0..4).chain(9..29) {
        assert!(close(&positions[k], &[0., -0.0432, -0.2202], 1e-4), "key {k}");
    }
    for t in [0.0, 400.0, 1500.0] {
        assert!(close(&anim::sample(r, 0, t).0, &[-0.4, -3.6, -1.2], 1e-4));
    }
}

/// Animation cases 9 and 10: frame-stepped playback of `select` and the
/// looping wrap of `idle_0`.
#[test]
fn m92fs_playback() {
    let Some((_, pv, _, ani)) = m92fs() else { return };
    let list = AnimList::build(&pv, &[Some(&ani)]);
    for (delta, sound_update, sound_t, done_update) in [(16, 19, 304, 41), (10, 30, 300, 66)] {
        let mut t = Tracker::on(0, true);
        t.looping = false;
        t.start(list.find("select").unwrap());
        let mut fired_at = None;
        let mut done_at = None;
        for update in 1..=100 {
            let mut fired = vec![];
            t.update(&list, delta, &mut |s| fired.push(s.to_string()));
            if fired.contains(&"sound_key 3".to_string()) {
                fired_at = Some((update, t.time));
            }
            if done_at.is_none() && t.done(&list) {
                done_at = Some((update, t.time));
            }
        }
        assert_eq!(fired_at, Some((sound_update, sound_t)), "delta {delta}");
        assert_eq!(done_at, Some((done_update, 651)), "delta {delta}");
    }
    let idle = list.find("idle_0").unwrap();
    let mut t = Tracker::on(idle, true);
    t.update(&list, 1000, &mut |_| {});
    assert_eq!((t.time, t.a, t.b), (1000, 1, 2));
    assert!(close(&[t.u], &[400.0 / 966.0], 1e-6));
    t.update(&list, 1000, &mut |_| {});
    assert_eq!((t.time, t.a, t.b), (434, 2, 2));
    for j in 1..ani.nodes.len() {
        let (p, q) = t.pose(&list, j);
        let (wp, wq) = list.entries[idle].animation.key_value(j, 2);
        // The same rotation (slerp of a non-unit quaternion with itself
        // scales it; R(q) normalises).
        let (r, wr) = (anim::rotation(q), anim::rotation(wq));
        assert!(close(&p, &wp, 1e-5) && (0..3).all(|i| close(&r[i], &wr[i], 1e-5)), "node {j}");
    }
    t.update(&list, 1000, &mut |_| {});
    assert_eq!((t.time, t.a, t.b), (1434, 1, 2));
    assert!(close(&[t.u], &[834.0 / 966.0], 1e-6));
    let mut t = Tracker::on(idle, true);
    t.set_rate(0.5);
    let steps: Vec<u32> = (0..4)
        .map(|_| {
            let before = t.time;
            t.update(&list, 3, &mut |_| {});
            t.time - before
        })
        .collect();
    assert_eq!(steps, [1, 2, 1, 2]);
}

/// Models case 12: the gunsmith NPC and its animation child.
#[test]
fn case_12_gunsmith() {
    let Some((len, m)) = model("GUNSMITH_NPC.rez", "gunsmith_npc/npc_model.ltb") else { return };
    assert_eq!((len, m.version), (543_330, 25));
    let pieces: Vec<_> = (0..m.pieces.len()).map(|i| (m.pieces[i].name.as_str(), m.pieces[i].lods[0].mesh.kind(), vt(&m, i))).collect();
    assert_eq!(
        pieces,
        [
            ("cap", 5, (812, 822)),
            ("base", 5, (3501, 3320)),
            ("eye_1", 4, (140, 168)),
            ("Cylinder02", 5, (1370, 1710)),
            ("eye_el", 4, (109, 133)),
            ("Plane05", 4, (9, 5)),
            ("table", 4, (1689, 1202)),
            ("Plane02", 4, (4, 2)),
            ("Plane06", 5, (4, 2))
        ]
    );
    assert_eq!(m.nodes.len(), 69);
    assert_eq!(parent_name(&m, "Point01").as_deref(), Some("Scene Root"));
    assert_eq!(parent_name(&m, "bone_03").as_deref(), Some("Point01"));
    assert_eq!(parent_name(&m, "Bip01 Pelvis").as_deref(), Some("Bip01"));
    assert!(m.sockets.is_empty());
    let a = &m.animations[0];
    assert_eq!((m.animations.len(), a.name.as_str(), a.keys.len()), (1, "npc_model", 92));
    assert_eq!(m.animation_dims(0), [15., 50., 15.]);
    assert_eq!(m.children, ["npc_ani.ltb"]);
    let Some((clen, c)) = model("GUNSMITH_NPC.rez", "gunsmith_npc/npc_ani.ltb") else { return };
    assert_eq!((clen, c.nodes.len(), c.animations.len()), (2_954_119, 69, 11));
    let names: Vec<_> = c.animations[..3].iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["start", "idle_1", "idle_2"]);
    // Case 17: equal depth-first parent lists.
    assert_eq!(m.parents(), c.parents());
}

/// Animation case 15: vertex animation in the mine's indicator.
#[test]
fn anim_case_15_mine() {
    let Some((len, m)) = model("FX.rez", "fx/weapons/m16mine/mine_indi_sh.ltb") else { return };
    assert_eq!((len, m.version, m.nodes.len()), (21_107, 25, 4));
    let a = &m.animations[0];
    assert_eq!((a.name.as_str(), a.compression, a.interp_ms, a.keys.len()), ("Exp_FAK_SH2", 0, 0, 121));
    let t: Vec<u32> = a.keys[..4].iter().map(|k| k.time).collect();
    assert_eq!((t, a.length()), (vec![0, 33, 67, 100], 4000));
    for j in 0..2 {
        let Channel::Transform { positions, rotations } = &a.channels[j] else { panic!() };
        assert_eq!((positions.len(), rotations.len()), (121, 121));
    }
    for j in 2..4 {
        let Channel::Vertices(frames) = &a.channels[j] else { panic!() };
        assert!(frames.iter().all(|f| f.len() == 4));
    }
    let Channel::Vertices(frames) = &a.channels[2] else { panic!() };
    assert!(close(&frames[0][0], &[-7.8755, 47.1020, -0.0143], 1e-4));
    assert!(close(&frames[1][0], &[-7.8644, 47.0233, -0.4265], 1e-4));
    assert_eq!((m.counts.keyframes, m.counts.animations), (121, 1));
    assert_eq!((m.counts.vertex_anim_bytes, m.counts.track_bytes), (11_616, 6_776));
}

/// Animation case 16: compression 2 in the default placeholder model.
#[test]
fn anim_case_16_default() {
    let Some((len, m)) = model("MODELS.rez", "models/default.ltb") else { return };
    assert_eq!((len, m.version, m.nodes.len(), m.nodes[0].name.as_str()), (1_720, 23, 1, "null"));
    let a = &m.animations[0];
    assert_eq!((a.name.as_str(), a.compression, a.interp_ms), ("x", 2, 200));
    assert_eq!(a.keys.iter().map(|k| k.time).collect::<Vec<_>>(), [0, 6566]);
    assert_eq!(m.animation_dims(0), [3., 3., 3.]);
    assert_eq!(a.key_value(0, 1), ([0.; 3], [0., 0., 0., 1.]));
    assert_eq!(m.counts.track_bytes, 14);
}

/// Models cases 8-11 and 13, animation 11-14: V2-keyed archives. They
/// run only with a V2 key in the config (the box has none; the spec's
/// values were measured on unencrypted copies and need re-measuring).
#[test]
fn v2_keyed_cases() {
    let Some((len, m)) = model("GUNS_M_PV_AR.rez", "guns_m_pv_ar/pv_ar_m4a1_sh.ltb") else { return };
    assert_eq!((len, m.version, m.pieces.len(), m.nodes.len()), (112_806, 25, 1, 50));
    assert_eq!(vt(&m, 0), (2690, 1617));
    let Some((_, ani)) = model("GUNS_M_PV_AR.rez", "guns_m_pv_ar/ani_ar_m4a1_sh.ltb") else { return };
    assert_eq!(ani.animations.len(), 13);
    assert_eq!(ani.parents(), m.parents());
    let Some((len, body)) = model("CHARS_M_BODY.rez", "chars_m_body/cm_body_alphateam_ki.ltb") else { return };
    assert_eq!((len, body.pieces.len(), body.nodes.len(), body.sockets.len()), (276_818, 8, 53, 18));
    let Some((_, child)) = model("CHARS_M_BODY.rez", "chars_m_body/st_m_child.ltb") else { return };
    let list = AnimList::build(&body, &[Some(&child)]);
    assert_eq!(list.hull_boxes(), ([15., 50., 15.], [15., 40., 15.]));
    let Some((len, b)) = model("CHARS_M_BODY.rez", "chars_m_body/cm_hv_ur_brandon_js.ltb") else { return };
    assert_eq!(len, 392_166);
    let leg = b.pieces.iter().find(|p| p.name == "Urban_Leg").unwrap();
    assert_eq!((leg.lod_distances.clone(), leg.old_lod_offsets), (vec![0., 400., 800.], (0, 2)));
}
