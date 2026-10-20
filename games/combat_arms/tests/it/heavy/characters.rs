//! specs/combat_arms/characters.md's "install" test cases on the user's
//! Combat Arms install: the default male and female bodies with their
//! animation libraries (`CHARS_M_BODY`, V2-keyed), posed by `characters::
//! skeleton` and driven by `characters::driver`. The spec measured them on
//! the US reference client; the Steam libraries differ (Open question 8),
//! so a mismatch here is a number to re-measure, not necessarily a bug.
//! Passes with a note without an install or without the V2 key.

use std::sync::Arc;

use bevy::prelude::*;
use chimera_core::mount::config::LocalConfig;
use combat_arms::game::{
    GAME, INFO,
    characters::{
        driver::{Driver, Inputs},
        records,
        skeleton::{Posed, Rig},
    },
    model,
};
use lithtech_formats::anim::Tracker;

fn load(female: bool) -> Option<Rig> {
    let Some(install) = LocalConfig::load().ok().and_then(|c| c.game_path(GAME)) else {
        eprintln!("skipping: no Combat Arms install configured");
        return None;
    };
    if !install.join("Game").is_dir() {
        eprintln!("skipping: no Combat Arms install at {}", install.display());
        return None;
    }
    let mount = (INFO.open)(&install).unwrap();
    let file = |p: &str| mount.read(p).ok();
    let record = records::data().model(if female { 7 } else { 0 }).unwrap();
    match model::load(&record.file, &file) {
        Ok(l) => Some(Rig::new(l.model, l.list)),
        Err(e) => {
            eprintln!("skipping: {e} (CHARS_M_BODY needs the V2 key)");
            None
        }
    }
}

fn on(rig: &Rig, name: &str) -> Tracker {
    Tracker::on(rig.list.find(name).unwrap_or_else(|| panic!("no {name}")), true)
}

/// The drawn pose's frame: the rig's bind pose as `locals_rh` (what the
/// body's `Animator::fixed` gets) is the drawn skeleton's bind
/// (`model::bones`), node by node; and the standing pose stays near it
/// (no node more than a body height from its bind place).
#[test]
fn drawn_pose_matches_the_drawn_skeleton() {
    let Some(rig) = load(false) else { return };
    let bones = model::bones(&rig.model);
    let locals = rig.bind().locals_rh();
    assert_eq!(bones.len(), locals.len());
    for (j, (b, (q, p))) in bones.iter().zip(&locals).enumerate() {
        let dp = (b.position - *p).length();
        let dq = b.rotation.dot(*q).abs();
        assert!(dp < 0.01 && dq > 0.9999, "{} ({j}): bind {:?} {:?}, posed {p:?} {q:?}", b.name, b.position, b.rotation);
    }
    let globals = |l: &[(Quat, Vec3)]| -> Vec<Vec3> {
        let mut g: Vec<(Quat, Vec3)> = Vec::new();
        for (j, (q, p)) in l.iter().enumerate() {
            let x = match bones[j].parent {
                Some(par) => (g[par].0 * *q, g[par].1 + g[par].0 * *p),
                None => (*q, *p),
            };
            g.push(x);
        }
        g.into_iter().map(|x| x.1).collect()
    };
    let (bind, posed) = (globals(&locals), globals(&stand(&rig, 0.0).locals_rh()));
    for (j, (a, b)) in bind.iter().zip(&posed).enumerate() {
        assert!((*a - *b).length() < 80.0, "{}: bind {a}, standing {b}", bones[j].name);
    }
}

/// The default male body as the map carries it, skinned as the renderer
/// does (joints under the model's root, inverse binds from the same
/// chain): it stands on the body's origin, about a man tall, and in the
/// standing pose every vertex keeps its distance to the node that weighs
/// most on it (a mesh outside the root's frame swings about far-off
/// nodes: the body drawn torn apart, sunk to the waist).
#[test]
fn drawn_body_skins_in_its_frame() {
    use chimera_world::map::MapData;
    use combat_arms::game::characters::models;
    let Some(install) = LocalConfig::load().ok().and_then(|c| c.game_path(GAME)) else { return };
    if !install.join("Game").is_dir() {
        return;
    }
    let mount = (INFO.open)(&install).unwrap();
    let file = |p: &str| mount.read(p).ok();
    let mut map = MapData::default();
    models::add_character_models(&mut map, &file);
    let Some(body) = map.characters.iter().find(|c| c.name.as_deref() == Some(models::body_name(0).as_str())) else {
        eprintln!("skipping: no character bodies (V2 key)");
        return;
    };
    let Some(rig) = load(false) else { return };
    let chain = |l: &[(Quat, Vec3)]| -> Vec<Mat4> {
        let mut g: Vec<Mat4> = Vec::new();
        for (j, (q, p)) in l.iter().enumerate() {
            let local = Mat4::from_rotation_translation(*q, *p);
            g.push(body.bones[j].parent.map_or(body.root.to_matrix() * local, |par| g[par] * local));
        }
        g
    };
    let bind = chain(&body.bones.iter().map(|b| (b.rotation, b.position)).collect::<Vec<_>>());
    let posed = chain(&stand(&rig, 0.0).locals_rh());
    let (mut low, mut high, mut worst) = (f32::MAX, f32::MIN, (0.0f32, String::new()));
    for (k, mesh) in body.model.meshes.iter().enumerate() {
        for (v, p) in mesh.positions.iter().enumerate() {
            let p = Vec3::from(*p);
            (low, high) = (low.min(p.y), high.max(p.y));
            let j = mesh.joints[v]
                .iter()
                .zip(mesh.joint_weights[v])
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap()
                .0;
            let j = *j as usize;
            let skinned: Vec3 = mesh.joints[v]
                .iter()
                .zip(mesh.joint_weights[v])
                .map(|(&n, w)| (posed[n as usize] * bind[n as usize].inverse()).transform_point3(p) * w)
                .sum();
            let at_bind = (bind[j].w_axis.truncate() - p).length();
            let at_pose = (posed[j].w_axis.truncate() - skinned).length();
            if (at_pose - at_bind).abs() > worst.0 {
                worst = ((at_pose - at_bind).abs(), format!("piece {k} vertex {v} on {}", body.bones[j].name));
            }
        }
    }
    eprintln!("body from {low} to {high} m; worst drift {worst:?}");
    assert!(low.abs() < 0.15, "feet at {low} m");
    assert!((1.2..2.0).contains(&high), "top at {high} m");
    assert!(worst.0 < 0.05, "{} drifts {} m from its node", worst.1, worst.0);
}

/// The layered pose of section 4 at t = 0 (stand: crouch weight 0).
fn stand(rig: &Rig, r: f32) -> Posed {
    let mut crouch = on(rig, "LC");
    crouch.multiplier = r;
    let pose = rig.layered(&Tracker::on(0, true), &on(rig, "URFRAmS"), &on(rig, "LST"), &crouch, r);
    rig.posed(&pose, r)
}

/// One animation alone on every node at `t` ms.
fn single(rig: &Rig, name: &str, t: u32) -> Posed {
    let mut tr = on(rig, name);
    tr.set_time(&rig.list, t);
    let pose: Vec<_> = (0..rig.model.nodes.len()).map(|j| tr.pose(&rig.list, j)).collect();
    rig.posed(&pose, 0.0)
}

fn near(got: Option<Vec3>, want: [f32; 3], what: &str) {
    let got = got.unwrap_or_else(|| panic!("{what}: missing"));
    assert!((got - Vec3::from(want)).abs().max_element() < 0.01, "{what}: {got} vs {want:?}");
}

fn aimed(rig: &Rig, pitch: f32, twist: f32, server: bool) -> Posed {
    let mut p = stand(rig, 0.0);
    p.aim(&records::data().skeleton(0).aim, pitch.to_radians(), twist.to_radians(), server);
    p
}

/// Cases 6-17's names and lengths on the install's library.
#[test]
fn install_states() {
    let Some(rig) = load(false) else { return };
    let mut d = Driver::new(Arc::new(rig.list.clone()));
    d.update(15, &Inputs::default());
    assert_eq!((d.name_of(&d.upper), d.name_of(&d.lower), d.name_of(&d.crouch)), ("URFRAmS", "LST", "LC"));
    for (name, len) in [("LST", 4000), ("LWF", 1000), ("LRF", 600), ("LCF", 1400), ("LSJ", 666), ("URFRFiS", 100), ("URFRSReS", 1866), ("D_HIT", 467), ("DT", 3033)] {
        let e = rig.list.find(name).unwrap_or_else(|| panic!("{name}"));
        assert_eq!(rig.list.entries[e].length(), len, "{name}");
    }
}

#[test]
fn t21_male_aim() {
    let Some(rig) = load(false) else { return };
    let p = aimed(&rig, 0.0, 0.0, false);
    near(p.sphere("Head"), [5.66, 29.72, 9.65], "Head");
    near(p.socket("RightHand").map(|s| s.0), [12.54, 22.90, 19.93], "RightHand");
    let p = aimed(&rig, 30.0, 0.0, false);
    near(p.sphere("Head"), [5.66, 24.68, 16.72], "Head down");
    near(p.socket("RightHand").map(|s| s.0), [12.54, 13.34, 22.19], "RightHand down");
    near(p.sphere("Face_Eyelid1"), [6.62, 28.50, 19.96], "Face_Eyelid1 down");
    near(aimed(&rig, -30.0, 0.0, false).sphere("Head"), [5.66, 30.98, 6.23], "Head up");
    let p = aimed(&rig, 0.0, 45.0, false);
    near(p.sphere("Head"), [10.30, 29.72, 3.59], "Head twisted");
    near(p.socket("RightHand").map(|s| s.0), [22.43, 22.90, 6.00], "RightHand twisted");
    near(aimed(&rig, 30.0, 0.0, true).sphere("Head"), [5.66, 27.45, 13.02], "server Head down");
}

#[test]
fn t22_female_aim() {
    let Some(rig) = load(true) else { return };
    let mut p = stand(&rig, 0.0);
    p.aim(&records::data().skeleton(1).aim, 30f32.to_radians(), 0.0, false);
    near(p.sphere("Bip01 Head"), [2.95, 24.74, 17.70], "Bip01 Head");
    near(p.sphere("Headfuck"), [4.07, 25.08, 20.87], "Headfuck");
}

#[test]
fn t23_to_27_sockets() {
    let Some(rig) = load(false) else { return };
    let bind = rig.bind();
    for s in &records::data().skeleton(0).sockets {
        near(bind.socket(&s.name).map(|p| p.0), s.bind, &s.name);
    }
    near(stand(&rig, 0.0).socket("RightHand").map(|s| s.0), [12.54, 22.90, 19.93], "RightHand stand");
    near(single(&rig, "LST", 0).socket("NAME_socket").map(|s| s.0), [9.70, 45.02, 13.52], "NAME_socket LST");
    near(stand(&rig, 0.0).socket("NAME_socket").map(|s| s.0), [8.63, 45.87, 11.51], "NAME_socket stand");
    near(stand(&rig, 1.0).socket("NAME_socket").map(|s| s.0), [6.13, 37.57, 15.21], "NAME_socket crouch");
    near(stand(&rig, 0.0).sphere("Left_foot"), [-10.02, -41.49, 13.52], "Left_foot stand");
    let Some(f) = load(true) else { return };
    near(stand(&f, 0.0).socket("NAME_socket").map(|s| s.0), [9.28, 43.71, 19.18], "female NAME_socket stand");
    near(stand(&f, 0.0).sphere("Left_foot"), [-7.73, -39.56, 12.91], "female Left_foot stand");
}

#[test]
fn t28_29_32_tables() {
    for female in [false, true] {
        let Some(rig) = load(female) else { return };
        let (bind, lst) = (rig.bind(), single(&rig, "LST", 0));
        let skel = records::data().skeleton(usize::from(female));
        for s in skel.spheres.iter().filter(|s| s.radius > 0.0 && !s.absent) {
            near(bind.sphere(&s.name), s.bind.unwrap(), &format!("{} bind", s.name));
            near(lst.sphere(&s.name), s.at.unwrap(), &format!("{} LST", s.name));
        }
    }
}

#[test]
fn t30_lrf() {
    let Some(rig) = load(false) else { return };
    let p = single(&rig, "LRF", 300);
    near(p.sphere("Left_foot"), [-3.53, -33.27, -23.60], "Left_foot");
    near(p.sphere("Right_foot"), [3.44, -40.27, 18.95], "Right_foot");
    near(p.sphere("Head"), [2.70, 28.39, 13.53], "Head");
    near(p.sphere("Right_legl1"), [4.26, -30.65, 13.80], "Right_legl1");
}

#[test]
fn t31_33_stand_and_crouch() {
    if let Some(rig) = load(false) {
        let (s, c) = (stand(&rig, 0.0), stand(&rig, 1.0));
        for (name, a, b) in [
            ("Head", [5.66, 29.72, 9.65], [3.21, 21.61, 12.05]),
            ("Torso", [-0.15, 5.91, -0.22], [-0.69, -1.65, 0.15]),
            ("Face_Eyelid1", [6.62, 34.74, 9.89], [4.24, 26.57, 12.72]),
            ("left_hand", [8.84, 23.46, 28.27], [1.39, 14.69, 30.63]),
        ] {
            near(s.sphere(name), a, name);
            near(c.sphere(name), b, name);
        }
    }
    if let Some(rig) = load(true) {
        let (s, c) = (stand(&rig, 0.0), stand(&rig, 1.0));
        near(s.sphere("Bip01 Head"), [2.95, 30.63, 11.32], "female stand");
        near(c.sphere("Bip01 Head"), [2.48, 15.39, 15.58], "female crouch");
        // Above the feet: 80.63 -> 55.39 (Quirk 7).
        assert!((s.sphere("Bip01 Head").unwrap().y - s.feet - 80.63).abs() < 0.01);
        assert!((c.sphere("Bip01 Head").unwrap().y - c.feet - 55.39).abs() < 0.01);
    }
}

#[test]
fn t34_35_counts() {
    let Some(rig) = load(false) else { return };
    let bind = rig.bind();
    let skel = records::data().skeleton(0);
    let found = skel.spheres.iter().filter(|s| s.radius > 0.0 && bind.sphere(&s.name).is_some()).count();
    assert_eq!(found, 33);
    if let Some(f) = load(true) {
        let bind = f.bind();
        let skel = records::data().skeleton(1);
        assert_eq!(skel.spheres.iter().filter(|s| s.radius > 0.0 && bind.sphere(&s.name).is_some()).count(), 35);
    }
}

/// The census's unkeyed models on the stock skeletons: the infected ship's
/// dead-body props (53 and 60 nodes) and the zombie render-style bodies.
/// Their nodes carry the hit tables' names: the posing pipeline (rig,
/// bind pose, spheres on nodes) runs on them without the V2 key, and the
/// male ones' bind pose is case 28's (the female prop's differs by up to
/// 2 u: another body on the same tree).
#[test]
fn unkeyed_bodies_on_the_stock_skeletons() {
    let Some(install) = LocalConfig::load().ok().and_then(|c| c.game_path(GAME)) else { return };
    if !install.join("Game").is_dir() {
        return;
    }
    let mount = (INFO.open)(&install).unwrap();
    let file = |p: &str| mount.read(p).ok();
    for (path, skeleton, nodes) in [
        ("PROPS\\INFECTEDSHIP\\INFECTED_SHIP_DEAD_BODY_MAN01_KE.LTB", 0, 53),
        ("PROPS\\INFECTEDSHIP\\INFECTED_SHIP_DEAD_BODY_WOMAN_KE.LTB", 1, 60),
        ("RS\\CM_ZOMBIE_TURKEYARMY.LTB", 0, 53),
    ] {
        let loaded = match model::load(path, &file) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("skipping {path}: {e}");
                continue;
            }
        };
        assert_eq!(loaded.model.nodes.len(), nodes, "{path}");
        let rig = Rig::new(loaded.model, loaded.list);
        let bind = rig.bind();
        let skel = records::data().skeleton(skeleton);
        let mut far = 0.0f32;
        for s in skel.spheres.iter().filter(|s| s.radius > 0.0 && !s.absent && s.socket.is_none()) {
            let at = bind.sphere(&s.name).unwrap_or_else(|| panic!("{path}: no node {}", s.name));
            far = far.max((at - Vec3::from(s.bind.unwrap())).length());
        }
        if skeleton == 0 {
            // Case 28's node spheres: the male bind pose is the default
            // body's.
            assert!(far < 0.02, "{path}: {far}");
        }
        eprintln!("{path}: every node sphere's node there; farthest from the default body's bind: {far:.2} u; sockets {:?}", rig.model.sockets.iter().map(|s| &s.name).collect::<Vec<_>>());
    }
}

/// On WAREHOUSE with the install's bodies (when they read): the map
/// carries the default male and female bodies as character models, a player wears the
/// default one (`BodyName`), its hit spheres come from the model's pose
/// (not the spec's skeleton), and its drawn pose is the driver's.
#[test]
fn warehouse_players_wear_their_bodies() {
    use chimera_world::{
        harness::Sim,
        map::{BodyName, MapSimPlugin, anim::Animator},
    };
    use combat_arms::{
        game::{
            characters::{CaAnim, CaPose, models},
            movement, world,
        },
        harness::CaSim,
    };
    let Some(install) = LocalConfig::load().ok().and_then(|c| c.game_path(GAME)) else { return };
    if !install.join("Game").is_dir() {
        return;
    }
    let mount = (INFO.open)(&install).unwrap();
    let map = world::load_from(&mount, "warehouse").unwrap();
    let names: Vec<_> = map.characters.iter().filter_map(|c| c.name.clone()).collect();
    if !names.contains(&models::body_name(0)) {
        eprintln!("skipping: no character bodies ({:?})", map.warnings.iter().find(|w| w.contains("character")));
        return;
    }
    assert_eq!(names, [models::body_name(0), models::body_name(7)]);
    let spawn = map.spawns[0].0;
    let mut sim = Sim::new(CaSim, MapSimPlugin::new(map));
    sim.ticks(2);
    let p = sim.spawn_character(spawn + Vec3::Y, movement::ID);
    sim.seconds(1.0);
    let world = sim.app.world();
    assert_eq!(world.get::<BodyName>(p).map(|b| b.0.as_str()), Some(models::body_name(0).as_str()));
    assert!(world.get::<CaAnim>(p).unwrap().rig.is_some());
    let pose = world.get::<CaPose>(p).unwrap();
    assert!(pose.modelled);
    let found = pose.spheres.iter().filter(|s| s.is_some()).count();
    assert_eq!(found, 33, "the default male's 33 spheres");
    let animator = world.get::<Animator>(p).expect("the body's animator");
    assert!(animator.fixed.is_some(), "drawn from the driver's pose");
}

/// characters.md section 2 with the keys: each sex's character model
/// (the default male, record 0, and its female twin, record 7) has the
/// stock bodies (Normal, Light, Heavy) as its first body part, the faces
/// as the second (its default face first: 0, and 4 for the female) and
/// the head pieces as the third (none first; the female face's hair
/// among them), merged on its skeleton: every piece is in the drawn
/// model, textured, and the faces and head pieces reach the head (above
/// 1.3 m). The team-mark pieces (texture slot 2) draw in the transparent
/// mark style, blended: an enemy's mark, `Alpha.dtx`, shows nothing.
/// Skips without the V1 and V2 keys.
#[test]
fn default_bodies_wear_face_and_head() {
    use chimera_world::map::{MapAlpha, MapData};
    use combat_arms::game::characters::models;
    let Some(install) = LocalConfig::load().ok().and_then(|c| c.game_path(GAME)) else { return };
    if !install.join("Game").is_dir() {
        return;
    }
    let keys = combat_arms::game::mount::keys().unwrap_or_default();
    if !(keys.contains_key("V1") && keys.contains_key("V2")) {
        eprintln!("skipping: needs the V1 and V2 keys");
        return;
    }
    let mount = (INFO.open)(&install).unwrap();
    let file = |p: &str| mount.read(p).ok();
    let data = records::data();
    assert!(data.face(0).and_then(|f| f.file.as_ref()).is_some(), "the install's Face0 has a file");
    let mut map = MapData::default();
    models::add_character_models(&mut map, &file);
    eprintln!("warnings: {:?}", map.warnings);
    let groups = models::loaded_groups();
    for (id, face_id) in [(0, 0), (7, 4)] {
        assert_eq!(data.default_face(id), face_id);
        let group = groups.iter().find(|g| g.records.first() == Some(&id)).expect("the group");
        eprintln!("group {id}: {group:?}");
        assert_eq!(group.records.len(), 3, "the three stock bodies");
        assert_eq!(group.faces.first(), Some(&face_id));
        // The sex's four faces (the Steam install lacks one female face's
        // file: a warning).
        assert!(group.faces.len() >= 3, "the sex's faces");
        assert_eq!(group.heads.first(), Some(&-1));
        let body = map
            .characters
            .iter()
            .find(|c| c.name.as_deref() == Some(models::body_name(id).as_str()))
            .unwrap_or_else(|| panic!("body {id} not loaded: {:?}", map.warnings));
        let meshes = &body.model.meshes;
        assert_eq!(body.model.body_parts, group.parts());
        for part in 0..3u16 {
            let n = group.parts()[part as usize];
            for choice in 0..n {
                // Head piece choice 0 is none: no meshes.
                let has = meshes.iter().any(|m| m.body == Some((part, choice)));
                assert_eq!(has, !(part == 2 && choice == 0), "part {part} choice {choice}");
            }
        }
        for (k, mesh) in meshes.iter().enumerate() {
            assert!(mesh.texture.is_some() || mesh.material.is_empty(), "body {id}: mesh {k} ({}) untextured", mesh.material);
            if matches!(mesh.body, Some((1 | 2, _))) {
                let top = mesh.positions.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
                assert!(top > 1.3, "body {id}: face or head mesh {k} tops out at {top} m");
            }
        }
        assert!(body.model.skins.iter().all(|s| s.len() == meshes.len()));
        // The team marks: blended in both families, the enemy's texture
        // (mark 0) clear, the teammate's (mark 1) not.
        let enemy = &body.model.skins[models::family(0, false) as usize];
        let mate = &body.model.skins[models::family(0, true) as usize];
        let marks: Vec<usize> = (0..meshes.len())
            .filter(|&k| mate[k].material.eq_ignore_ascii_case(&data.marks.teammate) && meshes[k].body == Some((0, 0)))
            .collect();
        assert!(!marks.is_empty(), "body {id}: no team-mark piece");
        for &k in &marks {
            assert_eq!(enemy[k].alpha, MapAlpha::Blend, "body {id}: mark {k}");
            assert_eq!(mate[k].alpha, MapAlpha::Blend);
            assert!(enemy[k].material.eq_ignore_ascii_case(&data.marks.enemy));
            let opacity = |t: Option<usize>| {
                let t = &map.textures[t.expect("textured")];
                t.rgba8.chunks(4).map(|p| p[3] as f32).sum::<f32>() / (t.rgba8.len() / 4) as f32
            };
            let (clear, shown) = (opacity(enemy[k].texture), opacity(mate[k].texture));
            eprintln!("body {id}: mark {k}: enemy alpha {clear:.1}, teammate {shown:.1}");
            assert!(clear < 8.0 && shown > 64.0);
        }
    }
    // The female face comes with a head piece (its hair).
    assert!(data.face(4).and_then(|f| f.hair).is_some());
    // Other players hold their weapon's own third-person model (`HHModel`
    // with its `HHSkin#`), textured, for every weapon players are given.
    let ours = combat_arms::game::data::builtin_weapon_ids();
    let with_model: Vec<_> = combat_arms::game::data::weapon_set()
        .weapons
        .iter()
        .filter(|w| ours.contains(&w.id) && w.models.world.is_some())
        .map(|w| w.id.clone())
        .collect();
    assert!(with_model.len() >= 10, "{with_model:?}");
    for id in &with_model {
        let held = map.held.iter().find(|h| &h.key == id).unwrap_or_else(|| panic!("{id}: no held model: {:?}", map.warnings));
        assert!(!held.model.meshes.is_empty(), "{id}");
        assert!(held.model.meshes.iter().any(|m| m.texture.is_some()), "{id}: untextured (the grey box)");
    }
}
