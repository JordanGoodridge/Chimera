//! The characters' models from the install (specs/combat_arms/
//! characters.md section 2): the default male and female body records'
//! (`LOADED`) body files with their animation library (`game::model`), as Chimera's character model named
//! `body_name(id)`, its texture slots filled per camouflage index and
//! team mark (skin family `family(c, teammate)`), the default head piece
//! merged onto its skeleton when it has a file; each weapon's third-person
//! model (`[models] world`, the weapons file's `HHModel`) as a held model
//! on the `RightHand` socket. The body's nodes, sockets, weight sets and
//! animation list are kept as a `Rig` (`rig`) for posing.
//!
//! Every character archive (`CHARS_*`) and the third-person guns
//! (`GUNS_M_HH`) are V2-keyed: without the key nothing here loads (the
//! map's warnings say so) and characters stay the shell's capsules, posed
//! by the spec's skeleton (`skeleton::fallback`).

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use bevy::prelude::*;
use lithtech_formats::ltb;

use super::{
    super::{model, view_model::hands},
    records::{self, ModelRecord},
    skeleton::Rig,
};
use crate::map::{MapCharacterModel, MapData, MapHeldModel, MapMesh, MapMeshLook, MapModel};

/// The character model of body record `id` (`map::BodyName`).
pub fn body_name(id: i32) -> String {
    format!("combat_arms:body:{id}")
}

/// Skin families per body: camouflage index c and the team mark (2).
pub const CAMOUFLAGES: usize = 7;

/// The skin family of camouflage `c` (0 past the list) with a teammate's
/// or an enemy's mark.
pub fn family(c: i32, teammate: bool) -> i32 {
    let c = usize::try_from(c).ok().filter(|&c| c < CAMOUFLAGES).unwrap_or(0);
    (c * 2 + usize::from(teammate)) as i32
}

fn rigs() -> &'static Mutex<HashMap<String, Arc<Rig>>> {
    static RIGS: OnceLock<Mutex<HashMap<String, Arc<Rig>>>> = OnceLock::new();
    RIGS.get_or_init(Default::default)
}

/// The rig loaded for body file `path` (any case and slashes).
pub fn rig(path: &str) -> Option<Arc<Rig>> {
    rigs().lock().ok()?.get(&model::normalize(path)).cloned()
}

/// Keep `rig` as body file `path`'s (the loader's; tests register
/// stand-ins).
pub fn register(path: &str, rig: Arc<Rig>) {
    if let Ok(mut r) = rigs().lock() {
        r.insert(model::normalize(path), rig);
    }
}

/// Forget body file `path`'s rig.
pub fn unregister(path: &str) {
    if let Ok(mut r) = rigs().lock() {
        r.remove(&model::normalize(path));
    }
}

/// The bodies loaded with a map: the default male's and female's (what a
/// selection draws without gear, by its face). Each has its sex's whole
/// animation library (tens of megabytes), parsed per body.
pub const LOADED: &[i32] = &[0, 7];

/// The loaded bodies (`LOADED`) and every third-person weapon into `map`
/// (when they read).
pub fn add_character_models(map: &mut MapData, file: &dyn Fn(&str) -> Option<Vec<u8>>) {
    let scale = super::super::movement::movement_data().meters_per_unit;
    let data = records::data();
    let mut failed = Vec::new();
    for record in data.model.iter().filter(|r| LOADED.contains(&r.id)) {
        match body(record, scale, map, file) {
            Ok(c) => map.characters.push(c),
            Err(e) => failed.push(e),
        }
    }
    if !failed.is_empty() {
        map.warnings.push(format!(
            "character models: {} of {} bodies did not load (capsules instead; CHARS_* need the V2 key): {}",
            failed.len(),
            LOADED.len(),
            failed[0]
        ));
    }
    let Some(hand) = data.skeleton(0).socket("RightHand") else { return };
    for w in &super::super::data::weapon_set().weapons {
        let Some(path) = w.models.world.as_deref() else { continue };
        let look = data.weapon(Some(&w.id));
        match held(path, &w.id, &look.skins, hand, map, file) {
            Ok(h) => map.held.push(h),
            Err(e) => map.warnings.push(format!("{} third-person model: {e}", w.id)),
        }
    }
}

/// A texture slot's file for a family (2: 0 upper, 1 legs, 2 team mark,
/// 3 and 4 clan background and emblem: mark 0 without a clan).
fn slot(record: &ModelRecord, slot: i32, c: i32, teammate: bool) -> String {
    let data = records::data();
    let (upper, lower) = data.skins(record, c);
    match slot {
        0 => upper.unwrap_or_default().to_string(),
        1 => lower.unwrap_or_default().to_string(),
        2 => data.team_mark(teammate, false).to_string(),
        3 | 4 => data.marks.enemy.clone(),
        _ => String::new(),
    }
}

/// Textures by name, loaded once into `map`.
struct Textures<'a> {
    file: &'a dyn Fn(&str) -> Option<Vec<u8>>,
    loaded: HashMap<String, MapMesh>,
}

impl Textures<'_> {
    fn look(&mut self, mesh: &MapMesh, material: String, map: &mut MapData) -> MapMeshLook {
        let m = self.loaded.entry(material.clone()).or_insert_with(|| {
            let mut m = MapMesh {
                material: material.clone(),
                alpha: mesh.alpha,
                ..default()
            };
            super::super::world::textures(std::slice::from_mut(&mut m), map, self.file);
            m
        });
        MapMeshLook {
            material,
            texture: m.texture,
            alpha: m.alpha,
            ..default()
        }
    }
}

/// One body record's character model.
fn body(
    record: &ModelRecord,
    scale: f32,
    map: &mut MapData,
    file: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<MapCharacterModel, String> {
    let loaded = model::load(&record.file, file)?;
    // Each piece's first texture index, its slot.
    let slots: Vec<i32> = loaded
        .model
        .pieces
        .iter()
        .map(|p| p.lods.first().and_then(|l| l.used_textures().first().copied()).unwrap_or(-1))
        .collect();
    // Chimera's animation set only needs the body's own pose (the
    // driver lays the pose out itself, `Animator::fixed`): no resampling
    // of the library.
    let own = model::LoadedModel {
        path: loaded.path.clone(),
        model: loaded.model.clone(),
        list: lithtech_formats::anim::AnimList {
            entries: loaded.list.entries.iter().take(1).cloned().collect(),
            offsets: vec![Some(0)],
        },
    };
    let mut m = model::convert(&own, scale, &|lod| {
        let s = lod.used_textures().first().copied().unwrap_or(-1);
        slot(record, s, 0, false)
    });
    let rig_data = m.rig.take().ok_or("no skeleton")?;
    if let Some(head) = records::data().head_file(0, record.skeleton) {
        merge(&mut m, &rig_data.bones, head, scale, file)?;
    }
    let mut textures = Textures {
        file,
        loaded: HashMap::new(),
    };
    m.skins = (0..CAMOUFLAGES as i32 * 2)
        .map(|f| {
            let (c, teammate) = (f / 2, f % 2 == 1);
            m.meshes
                .iter()
                .enumerate()
                .map(|(k, mesh)| {
                    let s = slots.get(k).copied().unwrap_or(-1);
                    textures.look(mesh, slot(record, s, c, teammate), map)
                })
                .collect()
        })
        .collect();
    for (mesh, look) in m.meshes.iter_mut().zip(m.skins[0].clone()) {
        mesh.texture = look.texture;
        mesh.alpha = look.alpha;
        mesh.color = if look.texture.is_some() { [255; 3] } else { [150; 3] };
    }
    let rig = Rig::new(loaded.model, loaded.list);
    let feet = rig.half_heights.0;
    register(&record.file, Arc::new(rig));
    Ok(MapCharacterModel {
        team: None,
        name: Some(body_name(record.id)),
        class: None,
        hitboxes: Vec::new(),
        bones: rig_data.bones.clone(),
        root: Transform::from_translation(Vec3::Y * feet * scale).with_scale(Vec3::splat(scale)),
        animations: Some(rig_data.animations.clone()),
        boxes: Vec::new(),
        ragdoll: None,
        model: m,
    })
}

/// An additional model (a head piece) skinned onto the body's skeleton:
/// its bones matched to the body's nodes by name, its vertices moved into
/// the body's bind pose.
fn merge(body: &mut MapModel, bones: &[crate::map::MapBone], path: &str, scale: f32, file: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<(), String> {
    let loaded = model::load(path, file)?;
    let mut m = model::convert(&loaded, scale, &|_| String::new());
    let rig = m.rig.take().ok_or("head piece without a skeleton")?;
    let map = hands::match_bones(&rig.bones, bones)?;
    let (own, theirs) = (hands::globals(&rig.bones), hands::globals(bones));
    let to: Vec<Mat4> = map.iter().enumerate().map(|(h, &b)| theirs[b] * own[h].inverse()).collect();
    for mut mesh in m.meshes {
        for (v, p) in mesh.positions.iter_mut().enumerate() {
            let units = Vec3::from(*p) / scale;
            let (moved, normal) = hands::blend(&to, mesh.joints.get(v), mesh.joint_weights.get(v), units, mesh.normals.get(v));
            *p = (moved * scale).to_array();
            if let (Some(n), Some(out)) = (normal, mesh.normals.get_mut(v)) {
                *out = n.to_array();
            }
        }
        for j in mesh.joints.iter_mut().flat_map(|j| j.iter_mut()) {
            *j = map.get(*j as usize).copied().unwrap_or(0) as u16;
        }
        body.meshes.push(mesh);
    }
    Ok(())
}

/// A weapon's third-person model held at the `RightHand` socket: the
/// socket's offset and rotation baked into its meshes, in the frame of
/// the socket's parent node (`Right_hand`), the source's units.
fn held(
    path: &str,
    key: &str,
    skins: &[String],
    socket: &records::SocketRecord,
    map: &mut MapData,
    file: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<MapHeldModel, String> {
    let loaded = model::load(path, file)?;
    let mut m = model::convert(&loaded, 1.0, &|lod| {
        let i = lod.used_textures().first().copied().unwrap_or(-1);
        usize::try_from(i).ok().and_then(|i| skins.get(i)).cloned().unwrap_or_default()
    });
    super::super::world::textures(&mut m.meshes, map, file);
    m.rig = None;
    // The socket in the parent node's space, right-handed.
    let q = Quat::from_array(ltb::to_rh_quat(socket.rotation));
    let q = if q.length_squared() > 0.0 { q.normalize() } else { Quat::IDENTITY };
    let at = Mat4::from_rotation_translation(q, Vec3::from(ltb::to_rh_vec(socket.offset)));
    for mesh in &mut m.meshes {
        for p in &mut mesh.positions {
            *p = at.transform_point3(Vec3::from(*p)).to_array();
        }
        for n in &mut mesh.normals {
            *n = at.transform_vector3(Vec3::from(*n)).normalize_or_zero().to_array();
        }
        mesh.joints.clear();
        mesh.joint_weights.clear();
    }
    Ok(MapHeldModel {
        key: key.to_string(),
        model: m,
        bone: socket.parent.clone(),
        muzzle: None,
        merged: Vec::new(),
        own: None,
    })
}
