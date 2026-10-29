//! The characters' models from the install (specs/combat_arms/
//! characters.md section 2): the stock body records (`LOADED`), one
//! Chimera character model per skeleton (sex) named after its first
//! record (`body_name`), with its animation library (`game::model`): the
//! first record's body, then the other records' bodies, the faces of
//! that sex (each record's default and the loadout pool's) and the head
//! pieces they may wear merged onto its skeleton as body-part choices
//! (`BodyGroup`: a character's `map::BodyNumber` picks its body, face
//! and head piece, `drawn`); its texture slots filled per camouflage
//! index and team mark (skin family `family(c, teammate)`), a face's and
//! head piece's skins by camouflage index; each piece in its render
//! style (`render_style`: the body record's `RenderStyle#`, so the team
//! mark of an enemy draws through `Alpha.dtx`'s alpha, nothing); each
//! weapon's third-person model (`[models] world`, the weapons file's
//! `HHModel`, in its `HHRenderStyle#`, drawn in its own `Hand` pose:
//! `HELD_POSE`) as a held model on the `RightHand` socket; each grenade's
//! model in flight (its projectile record's `Model`, `Skin#` and
//! `ModelScale`, in its `World` pose: `THROWN_POSE`) as a held model in no
//! hand under `grenades::thrown_key`. The body's nodes, sockets, weight sets and
//! animation list are kept as a `Rig` (`rig`, every record of the group
//! posing on the first's) for posing.
//!
//! Converted once per session (`shared_character_models`, the first map
//! load's) and copied into each map after (`append_shared`: the textures
//! appended, the indices moved), the rigs and groups registered again.
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
    super::{
        data::{CaData, CaDataInner},
        grenades::{self, GrenadeParams},
        model,
        render_style::{self, Style},
        view_model::hands,
    },
    records::{self, ModelRecord, Selection},
    skeleton::Rig,
};
use crate::map::{MapCharacterModel, MapData, MapHeldModel, MapMesh, MapMeshLook, MapModel};

/// The character model of body record `id` (`map::BodyName`): the first
/// record of a body group (`BodyGroup`).
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

/// Whether a character shows the team mark (2): a teammate does, except
/// in Quarantine (the infection mode), where every mark is transparent
/// (Corrections 11).
pub fn shows_mark(teammate: bool, quarantine: bool) -> bool {
    teammate && !quarantine
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

/// The bodies loaded with a map: the three stock male bodies (Normal,
/// Light, Heavy: their vests) and their female twins, each sex one
/// character model (`BodyGroup`): the first record's body with its whole
/// animation library, the others' bodies merged onto its skeleton.
pub const LOADED: &[i32] = &[0, 1, 2, 7, 8, 9];

/// One character model's choices (`map::BodyNumber`, in this order of
/// body parts): which body record's pieces, which face, which head piece,
/// which goggles and which backpack (-1: none) show.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BodyGroup {
    pub skeleton: usize,
    pub records: Vec<i32>,
    pub faces: Vec<i32>,
    pub heads: Vec<i32>,
    pub goggles: Vec<i32>,
    pub backpacks: Vec<i32>,
}

impl BodyGroup {
    /// The model's name (its first record's, `body_name`).
    pub fn name(&self) -> String {
        body_name(self.records.first().copied().unwrap_or(0))
    }

    /// The body number showing record `record`, `face` and head piece
    /// `head` (a choice not in the group: its first).
    pub fn number(&self, record: i32, face: i32, head: i32) -> i32 {
        self.number_with(record, face, head, -1, -1)
    }

    /// `number` wearing goggles `goggle` and backpack `backpack` (-1:
    /// none; not in the group: its first, none).
    pub fn number_with(&self, record: i32, face: i32, head: i32, goggle: i32, backpack: i32) -> i32 {
        let at = |list: &[i32], v: i32| list.iter().position(|&x| x == v).unwrap_or(0) as i32;
        let n = |list: &[i32]| list.len().max(1) as i32;
        let (nr, nf, nh, ng) = (n(&self.records), n(&self.faces), n(&self.heads), n(&self.goggles));
        at(&self.records, record)
            + nr * (at(&self.faces, face)
                + nf * (at(&self.heads, head) + nh * (at(&self.goggles, goggle) + ng * at(&self.backpacks, backpack))))
    }

    /// The model's body parts' choice counts (`MapModel::body_parts`).
    pub fn parts(&self) -> Vec<u16> {
        [self.records.len(), self.faces.len(), self.heads.len(), self.goggles.len(), self.backpacks.len()]
            .map(|n| n.max(1) as u16)
            .to_vec()
    }
}

fn groups() -> &'static Mutex<Vec<BodyGroup>> {
    static GROUPS: OnceLock<Mutex<Vec<BodyGroup>>> = OnceLock::new();
    GROUPS.get_or_init(Default::default)
}

/// The body groups loaded (one per skeleton).
pub fn loaded_groups() -> Vec<BodyGroup> {
    groups().lock().map(|g| g.clone()).unwrap_or_default()
}

/// Keep `group` as its skeleton's (the loader's; tests register their
/// own).
pub fn register_group(group: BodyGroup) {
    if let Ok(mut g) = groups().lock() {
        g.retain(|x| x.skeleton != group.skeleton);
        g.push(group);
    }
}

/// What a selection draws (characters.md 1, 2): its effective body's
/// character model and body number (that body's pieces, the selection's
/// face, else the body's default, the head piece worn:
/// `CharacterData::worn_head`, its goggles and backpack); None when no
/// loaded group has its body.
pub fn drawn(ca: &CaData, look: &Selection) -> Option<(String, i32)> {
    let data = ca.characters();
    let (record, _) = data.effective(look);
    let g = groups().lock().ok()?;
    let group = g.iter().find(|g| g.records.contains(&record.id))?;
    let face = if group.faces.contains(&look.face) { look.face } else { data.default_face(record.id) };
    let head = data.worn_head(look.head, face, record.skeleton);
    let head = if data.head_file(head, record.skeleton).is_some() { head } else { -1 };
    Some((
        group.name(),
        group.number_with(record.id, face, head, look.goggle, look.backpack),
    ))
}

/// The loaded bodies (`LOADED`, one character model per skeleton) and
/// every third-person weapon of ours into `map` (when they read);
/// `shared_character_models` once per session.
pub fn add_character_models(map: &mut MapData, ca: &CaDataInner, file: &dyn Fn(&str) -> Option<Vec<u8>>) {
    let scale = super::super::movement::movement_data().meters_per_unit;
    let data = ca.characters();
    let mut failed = Vec::new();
    let mut skeletons: Vec<usize> = Vec::new();
    for id in LOADED {
        if let Some(r) = data.model(*id)
            && !skeletons.contains(&r.skeleton)
        {
            skeletons.push(r.skeleton);
        }
    }
    for skeleton in &skeletons {
        let records: Vec<&ModelRecord> = LOADED
            .iter()
            .filter_map(|id| data.model(*id))
            .filter(|r| r.skeleton == *skeleton)
            .collect();
        match body_group(&records, scale, map, file) {
            Ok(c) => map.kit.characters.push(c),
            Err(e) => failed.push(e),
        }
    }
    if !failed.is_empty() {
        map.warnings.push(format!(
            "character models: {} of {} bodies did not load (capsules instead; CHARS_* need the V2 key): {}",
            failed.len(),
            skeletons.len(),
            failed[0]
        ));
    }
    let Some(hand) = data.skeleton(0).socket("RightHand") else { return };
    let ours = ca.builtin_weapon_ids();
    for w in ca.weapon_set().weapons.iter().filter(|w| ours.contains(&w.id)) {
        let Some(path) = w.models.world.as_deref() else { continue };
        let look = data.weapon(Some(&w.id));
        match held(path, &w.id, &look, hand, map, file) {
            Ok(h) => map.kit.held.push(h),
            Err(e) => map.warnings.push(format!("{} third-person model: {e}", w.id)),
        }
    }
    // Grenades in flight: their projectile record's model (grenades.md 5,
    // 6), under `grenades::thrown_key`.
    for w in ca.weapon_set().weapons.iter().filter(|w| ours.contains(&w.id)) {
        let Some(p) = w.hook::<GrenadeParams>(grenades::HOOK) else { continue };
        let Some(path) = p.thrown_model.as_deref() else { continue };
        match thrown(path, &grenades::thrown_key(&w.id), &p.thrown_skins, p.thrown_scale, map, file) {
            Ok(h) => map.kit.held.push(h),
            Err(e) => map.warnings.push(format!("{} thrown model: {e}", w.id)),
        }
    }
}

/// The world model's animation a grenade in flight is drawn in (its
/// `Hand` first, then `World`: grenades.md 6).
pub const THROWN_POSE: &str = "World";

/// A grenade's model as drawn in flight (its projectile record's `Model`,
/// `Skin#` by texture index and `ModelScale`): its meshes in the `World`
/// pose, scaled, in the model's own space (the source's units); drawn as
/// a `map::loose::ShownItem` under `key`.
fn thrown(
    path: &str,
    key: &str,
    skins: &[String],
    scale: [f32; 3],
    map: &mut MapData,
    file: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<MapHeldModel, String> {
    let loaded = model::load(path, file)?;
    let mut m = model::convert(&loaded, 1.0, &|lod| {
        let i = lod.used_textures().first().copied().unwrap_or(-1);
        usize::try_from(i).ok().and_then(|i| skins.get(i)).cloned().unwrap_or_default()
    });
    super::super::world::textures(&mut m.meshes, map, file);
    if let Some(entry) = model::rest_entry(&loaded, THROWN_POSE) {
        model::pose_meshes(&loaded.model, entry, &mut m.meshes, 1.0);
    }
    m.rig = None;
    // The scale's axes as the conversion turned them.
    let s = Vec3::from(ltb::to_rh_vec(scale)).abs();
    for mesh in &mut m.meshes {
        for p in &mut mesh.positions {
            *p = (Vec3::from(*p) * s).to_array();
        }
        mesh.joints.clear();
        mesh.joint_weights.clear();
    }
    Ok(MapHeldModel {
        key: key.to_string(),
        model: m,
        bone: String::new(),
        muzzle: None,
        merged: Vec::new(),
        own: None,
    })
}

/// The character models every map carries (`add_character_models` into
/// an empty map: its own textures), their rigs and body groups
/// (`shared_character_models`).
pub struct SharedModels {
    data: MapData,
    rigs: Vec<(String, Arc<Rig>)>,
    groups: Vec<BodyGroup>,
}

/// `add_character_models`, converted once per session (the first map
/// load; the next ones copy it: `append_shared`).
pub fn shared_character_models(file: &dyn Fn(&str) -> Option<Vec<u8>>) -> Arc<SharedModels> {
    static SHARED: OnceLock<Mutex<Option<Arc<SharedModels>>>> = OnceLock::new();
    let mut cached = SHARED.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    cached
        .get_or_insert_with(|| {
            let mut data = MapData::default();
            add_character_models(&mut data, &CaData::install(), file);
            let rigs = CaData::install()
                .characters()
                .model
                .iter()
                .filter(|r| LOADED.contains(&r.id))
                .filter_map(|r| Some((r.file.clone(), rig(&r.file)?)))
                .collect();
            Arc::new(SharedModels {
                data,
                rigs,
                groups: loaded_groups(),
            })
        })
        .clone()
}

/// The shared character models into `map`: their textures appended to
/// the map's (the meshes' and skins' indices moved past the map's own),
/// their rigs and body groups registered again (a test may have replaced
/// one).
pub fn append_shared(map: &mut MapData, models: &SharedModels, kit: &crate::map::KitLoad) {
    for (path, r) in &models.rigs {
        register(path, r.clone());
    }
    for g in &models.groups {
        register_group(g.clone());
    }
    let base = map.textures.len();
    let shift = |model: &mut MapModel| {
        for t in model.meshes.iter_mut().filter_map(|m| m.texture.as_mut()) {
            *t += base;
        }
        for t in model.skins.iter_mut().flatten().filter_map(|l| l.texture.as_mut()) {
            *t += base;
        }
    };
    map.textures.extend(models.data.textures.iter().cloned());
    let bodies = models.data.kit.characters.iter().filter(|_| kit.bodies());
    map.kit.characters.extend(bodies.cloned().map(|mut c| {
        shift(&mut c.model);
        c
    }));
    let held = models.data.kit.held.iter().filter(|h| kit.wants(&h.key));
    map.kit.held.extend(held.cloned().map(|mut h| {
        shift(&mut h.model);
        h
    }));
    map.warnings.extend(models.data.warnings.iter().cloned());
}

/// A texture slot's file for a family (2: 0 upper, 1 legs, 2 team mark,
/// 3 and 4 clan background and emblem: mark 0 without a clan).
fn slot(record: &ModelRecord, slot: i32, c: i32, teammate: bool) -> String {
    let ca = CaData::install();
    let data = ca.characters();
    let (upper, lower) = data.skins(record, c);
    match slot {
        0 => upper.unwrap_or_default().to_string(),
        1 => lower.unwrap_or_default().to_string(),
        2 => data.team_mark(teammate, false).to_string(),
        3 | 4 => data.marks.enemy.clone(),
        _ => String::new(),
    }
}

/// Textures by name and render style, loaded once into `map`.
struct Textures<'a> {
    file: &'a dyn Fn(&str) -> Option<Vec<u8>>,
    loaded: HashMap<(String, Style, Style), MapMesh>,
}

impl Textures<'_> {
    /// `material` drawn in `style`, then `over` (a team mark's own).
    fn look(&mut self, material: String, style: Style, over: Style, map: &mut MapData) -> MapMeshLook {
        let m = self.loaded.entry((material.clone(), style, over)).or_insert_with(|| {
            let mut m = MapMesh {
                material: material.clone(),
                ..default()
            };
            render_style::apply(&mut m, style);
            render_style::apply(&mut m, over);
            super::super::world::textures(std::slice::from_mut(&mut m), map, self.file);
            m
        });
        MapMeshLook {
            material,
            texture: m.texture,
            alpha: m.alpha,
            unlit: m.unlit,
            ..default()
        }
    }
}

/// Where a mesh's skins come from: a body record's texture slot, or its
/// own list by camouflage index (a face or head piece).
enum Source<'a> {
    Body(&'a ModelRecord, i32),
    Own(Vec<String>),
}

/// Each added mesh's skins' source and render style.
type Added<'a> = Vec<(Source<'a>, Style)>;

/// A body record's pieces: its texture slots, its render styles.
fn body_sources<'a>(record: &'a ModelRecord, lods: &[(i32, i32)]) -> Added<'a> {
    lods.iter()
        .map(|&(slot, style)| (Source::Body(record, slot), render_style::of_list(&record.render_styles, style)))
        .collect()
}

/// A face's or head piece's pieces: its own skins and render styles.
fn piece_sources<'a>(skins: &[String], styles: &[String], lods: &[(i32, i32)]) -> Added<'a> {
    lods.iter()
        .map(|&(_, style)| (Source::Own(skins.to_vec()), render_style::of_list(styles, style)))
        .collect()
}

/// One body group's character model (the `LOADED` records of one
/// skeleton): the first record's body and library; the other records'
/// bodies, the faces of this skeleton (each record's default and the
/// loadout pool's) and the head pieces they may wear (none, the pool's,
/// each face's `HairIndex`) merged onto its skeleton as body-part choices
/// (`BodyGroup`), each mesh drawn in its render style (`render_style`).
fn body_group(
    records: &[&ModelRecord],
    scale: f32,
    map: &mut MapData,
    file: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<MapCharacterModel, String> {
    let base = *records.first().ok_or("no body")?;
    let ca = CaData::install();
    let data = ca.characters();
    let loaded = model::load(&base.file, file)?;
    let base_lods: Vec<(i32, i32)> = model::drawn_lods(&loaded.model).into_iter().map(model::slot_and_style).collect();
    // Chimera's animation set only needs the body's own pose (the
    // driver lays the pose out itself, `Animator::fixed`): no resampling
    // of the library.
    let own = model::LoadedModel {
        path: loaded.path.clone(),
        model: loaded.model.clone(),
        list: lithtech_formats::anim::AnimList {
            entries: loaded.list.entries.iter().take(1).cloned().collect(),
            offsets: vec![Some(0)],
            ..Default::default()
        },
    };
    let mut m = model::convert(&own, scale, &|_| String::new());
    let rig_data = m.rig.take().ok_or("no skeleton")?;
    let mut sources: Added = body_sources(base, &base_lods);
    let mut bodies: Vec<Option<(u16, u16)>> = vec![Some((0, 0)); m.meshes.len()];
    let mut group = BodyGroup {
        skeleton: base.skeleton,
        records: vec![base.id],
        ..default()
    };
    let mut warn = Vec::new();
    // The other records' bodies (part 0).
    for r in &records[1..] {
        match merge(&mut m, &rig_data.bones, &r.file, scale, file) {
            Ok(lods) => {
                let choice = group.records.len() as u16;
                bodies.extend(std::iter::repeat_n(Some((0, choice)), lods.len()));
                sources.extend(body_sources(r, &lods));
                group.records.push(r.id);
            }
            Err(e) => warn.push(format!("body {}: {e}", r.file)),
        }
    }
    // Faces (part 1): this skeleton's (by model 0's face redirect), each
    // record's default first.
    let mut face_ids: Vec<i32> = Vec::new();
    for f in group.records.iter().map(|&id| data.default_face(id)).chain(data.loadouts.faces.iter().copied()) {
        if !face_ids.contains(&f) {
            face_ids.push(f);
        }
    }
    for f in face_ids {
        let look = Selection {
            face: f,
            ..Selection::default()
        };
        if data.effective(&look).0.skeleton != base.skeleton {
            continue;
        }
        let Some(rec) = data.face(f) else { continue };
        let Some(path) = &rec.file else { continue };
        match merge(&mut m, &rig_data.bones, path, scale, file) {
            Ok(lods) => {
                let choice = group.faces.len() as u16;
                bodies.extend(std::iter::repeat_n(Some((1, choice)), lods.len()));
                sources.extend(piece_sources(&rec.skins, &rec.render_styles, &lods));
                group.faces.push(f);
            }
            Err(e) => warn.push(format!("face {path}: {e}")),
        }
    }
    // Head pieces (part 2): none first, then the pool's and the faces'.
    group.heads.push(-1);
    let mut head_ids: Vec<i32> = data.loadouts.heads.clone();
    head_ids.extend(group.faces.iter().filter_map(|&f| data.face(f).and_then(|r| r.hair)));
    for h in head_ids {
        if group.heads.contains(&h) {
            continue;
        }
        let Some(path) = data.head_file(h, base.skeleton) else { continue };
        let Some(rec) = data.head.iter().find(|x| x.id == h) else { continue };
        match merge(&mut m, &rig_data.bones, path, scale, file) {
            Ok(lods) => {
                let choice = group.heads.len() as u16;
                bodies.extend(std::iter::repeat_n(Some((2, choice)), lods.len()));
                sources.extend(piece_sources(&rec.skins, &rec.render_styles, &lods));
                group.heads.push(h);
            }
            Err(e) => warn.push(format!("head piece {path}: {e}")),
        }
    }
    // Goggles (part 3) and backpacks (part 4): socket attachments
    // (rigid on the body's socket), none first, then the pool's, the
    // files of the body's figure type.
    group.goggles.push(-1);
    group.backpacks.push(-1);
    for (part, pool, records) in [(3u16, &data.loadouts.goggles, &data.goggle), (4, &data.loadouts.backpacks, &data.backpack)] {
        for &id in pool {
            let chosen = if part == 3 { &group.goggles } else { &group.backpacks };
            if chosen.contains(&id) {
                continue;
            }
            let Some(rec) = records.iter().find(|r| r.id == id) else { continue };
            let Some(path) = rec.files.get(base.figure).filter(|f| !f.is_empty()) else { continue };
            match attach(&mut m, &rig_data.bones, &loaded.model, &rec.socket, path, scale, file) {
                Ok(lods) => {
                    let list = if part == 3 { &mut group.goggles } else { &mut group.backpacks };
                    let choice = list.len() as u16;
                    bodies.extend(std::iter::repeat_n(Some((part, choice)), lods.len()));
                    sources.extend(piece_sources(&rec.skins, &rec.render_styles, &lods));
                    list.push(id);
                }
                Err(e) => warn.push(format!("attachment {path}: {e}")),
            }
        }
    }
    if !warn.is_empty() {
        map.warnings.push(format!(
            "character model {}: {} pieces did not load: {}",
            base.id,
            warn.len(),
            warn.join("; ")
        ));
    }
    let mut textures = Textures {
        file,
        loaded: HashMap::new(),
    };
    m.skins = (0..CAMOUFLAGES as i32 * 2)
        .map(|f| {
            let (c, teammate) = (f / 2, f % 2 == 1);
            sources
                .iter()
                .map(|(source, style)| {
                    let (name, over) = match source {
                        Source::Own(list) => {
                            let i = usize::try_from(c).ok().filter(|&i| i < list.len()).unwrap_or(0);
                            (list.get(i).cloned().unwrap_or_default(), Style::Opaque)
                        }
                        // The team mark in its mark's own style too
                        // (`[TeamMark]`'s `RenderStyle#`).
                        Source::Body(record, 2) => (
                            slot(record, 2, c, teammate),
                            render_style::of_list(&data.marks.render_styles, i32::from(teammate)),
                        ),
                        Source::Body(record, s) => (slot(record, *s, c, teammate), Style::Opaque),
                    };
                    textures.look(name, *style, over, map)
                })
                .collect()
        })
        .collect();
    for ((mesh, look), body) in m.meshes.iter_mut().zip(m.skins[0].clone()).zip(&bodies) {
        mesh.material = look.material.clone();
        mesh.texture = look.texture;
        mesh.alpha = look.alpha;
        mesh.unlit = look.unlit;
        mesh.color = if look.texture.is_some() { [255; 3] } else { [150; 3] };
        mesh.body = *body;
    }
    m.body_parts = group.parts();
    let rig = Arc::new(Rig::new(loaded.model, loaded.list));
    let feet = rig.half_heights.0;
    // The meshes in the body's frame (feet at the origin), as the root
    // places the skeleton: in model space they would sit the root's lift
    // below their nodes, and every turned node would swing its vertices
    // about a point that far off.
    for mesh in &mut m.meshes {
        for p in &mut mesh.positions {
            p[1] += feet * scale;
        }
    }
    // Every record of the group poses on the first's skeleton and
    // library (one tree shape per sex, characters.md 2).
    for r in records {
        register(&r.file, rig.clone());
    }
    let name = group.name();
    register_group(group);
    Ok(MapCharacterModel {
        game: Some(crate::game::GAME),
        team: None,
        name: Some(name),
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

/// An additional model (another body, a face or head piece) skinned onto
/// the body's skeleton: its bones matched to the body's nodes by name,
/// its vertices moved into the body's bind pose. The added meshes' skin
/// slots and render-style indices (`model::slot_and_style`).
fn merge(
    body: &mut MapModel,
    bones: &[crate::map::MapBone],
    path: &str,
    scale: f32,
    file: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<Vec<(i32, i32)>, String> {
    let loaded = model::load_own(path, file)?;
    let lods: Vec<(i32, i32)> = model::drawn_lods(&loaded.model).into_iter().map(model::slot_and_style).collect();
    let mut m = model::convert(&loaded, scale, &|_| String::new());
    let rig = m.rig.take().ok_or("a piece without a skeleton")?;
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
    Ok(lods)
}

/// A socket attachment (characters.md 2: goggles, masks, backpacks)
/// fixed to the body's socket `socket` (`owner`'s, by name, any case):
/// its meshes placed by the socket's bind transform (its parent node's
/// bind, then the socket's offset and rotation; the attachment's own
/// origin and axes on the socket) and bound wholly to the parent node,
/// so it follows that node's animation rigidly. Its meshes' skin slots
/// and render-style indices; an error without that socket ("not
/// attached").
fn attach(
    body: &mut MapModel,
    bones: &[crate::map::MapBone],
    owner: &ltb::Model,
    socket: &str,
    path: &str,
    scale: f32,
    file: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<Vec<(i32, i32)>, String> {
    let s = owner
        .sockets
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(socket))
        .ok_or_else(|| format!("the body has no socket {socket}"))?;
    let node = s.node as usize;
    let parent = *hands::globals(bones).get(node).ok_or("the socket's node is not in the skeleton")?;
    let loaded = model::load_own(path, file)?;
    let lods: Vec<(i32, i32)> = model::drawn_lods(&loaded.model).into_iter().map(model::slot_and_style).collect();
    let m = model::convert(&loaded, scale, &|_| String::new());
    let at = parent * socket_local(s.rotation, s.position);
    for mut mesh in m.meshes {
        place_rigid(&mut mesh, at, node, scale);
        body.meshes.push(mesh);
    }
    Ok(lods)
}

/// A rigid mesh (metres) moved by `at` (world units) and bound wholly to
/// node `node`.
pub fn place_rigid(mesh: &mut MapMesh, at: Mat4, node: usize, scale: f32) {
    for p in &mut mesh.positions {
        *p = (at.transform_point3(Vec3::from(*p) / scale) * scale).to_array();
    }
    for n in &mut mesh.normals {
        *n = at.transform_vector3(Vec3::from(*n)).normalize_or_zero().to_array();
    }
    let count = mesh.positions.len();
    mesh.joints = vec![[node as u16, 0, 0, 0]; count];
    mesh.joint_weights = vec![[1.0, 0.0, 0.0, 0.0]; count];
}

/// A socket's offset and rotation (stored, left-handed) as a
/// right-handed transform in its parent node's space.
pub fn socket_local(rotation: ltb::Quat, offset: ltb::Vec3) -> Mat4 {
    let q = Quat::from_array(ltb::to_rh_quat(rotation));
    let q = if q.length_squared() > 0.0 { q.normalize() } else { Quat::IDENTITY };
    Mat4::from_rotation_translation(q, Vec3::from(ltb::to_rh_vec(offset)))
}

/// The third-person weapon models' own animation they are held in (each
/// has `Hand` and `World`; `Hand` turns the root half a turn from the
/// bind pose for almost every gun).
pub const HELD_POSE: &str = "Hand";

/// A weapon's third-person model held at the `RightHand` socket: its
/// meshes in its `Hand` pose (`HELD_POSE`), the socket's offset and
/// rotation baked into them, in the frame of the socket's parent node
/// (`Right_hand`), the source's units.
fn held(
    path: &str,
    key: &str,
    look: &records::WeaponLook,
    socket: &records::SocketRecord,
    map: &mut MapData,
    file: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<MapHeldModel, String> {
    let loaded = model::load(path, file)?;
    let styles: Vec<Style> = model::drawn_lods(&loaded.model)
        .into_iter()
        .map(|l| render_style::of_list(&look.render_styles, l.render_style))
        .collect();
    let mut m = model::convert(&loaded, 1.0, &|lod| {
        let i = lod.used_textures().first().copied().unwrap_or(-1);
        usize::try_from(i).ok().and_then(|i| look.skins.get(i)).cloned().unwrap_or_default()
    });
    for (mesh, style) in m.meshes.iter_mut().zip(styles) {
        render_style::apply(mesh, style);
    }
    super::super::world::textures(&mut m.meshes, map, file);
    // Drawn in its own `Hand` pose, not the bind pose (`model::rest_entry`).
    if let Some(entry) = model::rest_entry(&loaded, HELD_POSE) {
        model::pose_meshes(&loaded.model, entry, &mut m.meshes, 1.0);
    }
    m.rig = None;
    // The socket in the parent node's space, right-handed.
    let at = socket_local(socket.rotation, socket.offset);
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
