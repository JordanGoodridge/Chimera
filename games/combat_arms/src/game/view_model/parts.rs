//! Fitted parts on the first-person view model (specs/combat_arms/
//! attachments.md "View model and world model"): for each socket slot
//! whose part has a `Socket` and a model for the weapon's variant index
//! (`attachments::CaMods::part_models`; a default sight with
//! `DefaultScopeVisible` 0 isn't drawn), the part model `AttachModel`c
//! with its skins (`AttachSkin`c`_`i, by texture index), render styles
//! (`AttachRenderStyle#`) and `AttachScale`, at that socket of the gun,
//! moved by the weapon's `MuzzlePVPosOff`/`RotOff` (muzzle parts) or
//! `ScopePVPosOff`/`RotOff` (sights) in the socket's frame. The part
//! follows the socket's node (its vertices skinned rigidly to it), so it
//! moves, hides and shows with the gun.
//!
//! A weapon with drawn parts shows a view model of its own: the key
//! `<weapon id>+<part>+<part>` (`view_key`), made on demand
//! (`on_demand::load_weapon`) from the weapon's view model with the parts'
//! meshes added (`attach`); until it is ready the bare gun's stands in.
//!
//! Known shortcuts (docs/plans/active/combat-arms.md, "Attachment
//! models"): the rotation offset is read as radians (yaw about the
//! socket's up axis, pitch about its right, roll about its forward;
//! every stock first-person value is zero); the muzzle flash stays at
//! the gun's `Flash` socket (not moved to a silencer's end); a part
//! model's own animations aren't played.

use std::f32::consts::PI;

use bevy::prelude::*;

use super::{VIEW_SCALE, load_part};
use super::super::{
    attachments::{AttachParams, CaMods, CaModTable, MUZZLE, ModRecord, PartOffset},
    render_style,
};
use crate::map::{MapData, MapViewModel};

/// What separates a weapon id from its parts in a view-model key.
pub const SEP: char = '+';

/// The view model to show for weapon `id` with fitted parts `mods`: its
/// own key, or `id+Part+Part` when parts with first-person models are
/// drawn on it.
pub fn view_key(id: &str, mods: Option<&CaMods>) -> String {
    let mut key = id.to_string();
    let Some(mods) = mods else { return key };
    for m in mods.part_models().into_iter().filter(|m| m.visible) {
        let has_model = mods.part(m.slot).is_some_and(|p| p.record.view_model(m.variant).is_some());
        if has_model {
            key.push(SEP);
            key.push_str(&m.name);
        }
    }
    key
}

/// A view-model key's weapon id and part names.
pub fn split_key(key: &str) -> (&str, Vec<&str>) {
    let mut it = key.split(SEP);
    let id = it.next().unwrap_or(key);
    (id, it.collect())
}

/// One part to put on a view model: its record, the weapon's variant
/// index for it, its slot's offset.
pub struct PartToDraw<'a> {
    pub record: &'a ModRecord,
    pub variant: usize,
    pub offset: PartOffset,
}

/// The parts of view-model key `key` as the weapon's data gives them:
/// each a known record with a socket, a variant in 0..9 from the weapon's
/// list and a first-person model there; unknown names are skipped.
pub fn parts_of<'a>(
    key: &str,
    table: &'a CaModTable,
    params: &AttachParams,
) -> Vec<PartToDraw<'a>> {
    let (_, names) = split_key(key);
    names
        .into_iter()
        .filter_map(|n| {
            let record = table.get(table.find(n)?)?;
            let variant = usize::try_from(params.variant_of(&record.name)?).ok().filter(|v| *v < 10)?;
            record.view_model(variant)?;
            let slot = record.slot()?;
            Some(PartToDraw {
                record,
                variant,
                offset: params.part_offsets.of(slot, true),
            })
        })
        .collect()
}

/// The socket transform `offset` adds, in the socket's frame (the game's
/// axes converted, units): the position, then yaw, pitch and roll.
pub fn offset_matrix(offset: &PartOffset) -> Mat4 {
    let pos = Vec3::from(lithtech_formats::ltb::to_rh_vec(offset.pos));
    // Z mirrored: turns about x and y change sign, about z keep it.
    let [yaw, pitch, roll] = offset.rot;
    let rot = Quat::from_euler(EulerRot::YXZ, -yaw, -pitch, roll);
    Mat4::from_rotation_translation(rot, pos)
}

/// Put `part`'s model (read through `file`) onto `view` at its socket,
/// its textures added to `textures`. Err: no such socket on the gun, or
/// the model doesn't read.
pub fn attach(
    view: &mut MapViewModel,
    part: &PartToDraw,
    textures: &mut MapData,
    file: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<(), String> {
    let record = part.record;
    let socket_name = record.socket.as_deref().unwrap_or_default();
    let socket = view
        .attachments
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case(socket_name))
        .ok_or_else(|| format!("{}: the gun has no `{socket_name}` socket", record.name))?
        .clone();
    let (path, skins) = record
        .view_model(part.variant)
        .ok_or_else(|| format!("{}: no model for variant {}", record.name, part.variant))?;
    let (mut meshes, lods) = load_part(path, skins, textures, file)?;
    // The socket's frame at the reference pose, in the eye's space.
    let globals = super::hands::globals(&view.bones);
    let bone = globals.get(socket.bone).copied().unwrap_or(Mat4::IDENTITY);
    let scale = Mat4::from_scale(Vec3::from(record.attach_scale.unwrap_or([1.0; 3])));
    let socket_m = view.root.to_matrix() * bone * socket.local.to_matrix();
    let offset = offset_matrix(&part.offset);
    let turn = if record.slot() == Some(MUZZLE) {
        muzzle_turn(view, socket_m, offset * scale, &meshes)
    } else {
        Mat4::IDENTITY
    };
    let to_eye = socket_m * offset * turn * scale;
    for (mesh, style) in meshes.iter_mut().zip(lods) {
        render_style::apply(mesh, render_style::of_list(&record.attach_render_styles, style));
        for p in &mut mesh.positions {
            *p = to_eye.transform_point3(Vec3::from(*p) / VIEW_SCALE).to_array();
        }
        for n in &mut mesh.normals {
            *n = to_eye.transform_vector3(Vec3::from(*n)).normalize_or_zero().to_array();
        }
        let count = mesh.positions.len();
        mesh.joints = vec![[socket.bone as u16, 0, 0, 0]; count];
        mesh.joint_weights = vec![[1.0, 0.0, 0.0, 0.0]; count];
        view.model.meshes.push(mesh.clone());
    }
    Ok(())
}

/// How a muzzle part's model turns at its socket (ours, pending the
/// spec: docs/plans/active/combat-arms.md "Attachment models"): with the
/// spec's reading (the model in the socket's frame) the stock rifles'
/// silencers sit backwards inside the gun while the M24's and the
/// pistols' fit, so of the model as it is and turned half a turn about
/// each axis, the one that reaches farthest ahead of the muzzle without
/// leaving it (its back end within `MUZZLE_GAP` units of the socket),
/// ahead meaning away from the gun's middle.
fn muzzle_turn(view: &MapViewModel, socket: Mat4, offset: Mat4, meshes: &[crate::map::MapMesh]) -> Mat4 {
    let gun: Vec<Vec3> = view.model.meshes.iter().flat_map(|m| m.positions.iter()).map(|p| Vec3::from(*p)).collect();
    let part: Vec<Vec3> = meshes.iter().flat_map(|m| m.positions.iter()).map(|p| Vec3::from(*p) / VIEW_SCALE).collect();
    turn_ahead(socket, &gun, offset, &part)
}

/// `muzzle_turn` for any model: `gun`'s points (where the socket matrix
/// `socket` maps to), the part's `points` in its own frame, placed by
/// `offset` in the socket's frame.
pub fn turn_ahead(socket: Mat4, gun: &[Vec3], offset: Mat4, points: &[Vec3]) -> Mat4 {
    if gun.is_empty() {
        return Mat4::IDENTITY;
    }
    let inv = socket.inverse();
    let middle = gun.iter().fold(Vec3::ZERO, |a, p| a + inv.transform_point3(*p)) / gun.len() as f32;
    let ahead = -middle.normalize_or_zero();
    let mut best = (Mat4::IDENTITY, f32::MIN);
    for turn in [Mat4::IDENTITY, Mat4::from_rotation_x(PI), Mat4::from_rotation_y(PI), Mat4::from_rotation_z(PI)] {
        let m = offset * turn;
        let along = points.iter().map(|p| m.transform_point3(*p).dot(ahead));
        let (lo, hi) = along.fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)));
        let reach = if lo <= MUZZLE_GAP { hi } else { hi - 1.0e4 };
        if reach > best.1 + 1e-3 {
            best = (turn, reach);
        }
    }
    best.0
}

/// How far ahead of the muzzle (units) a muzzle part may start
/// (`muzzle_turn`).
const MUZZLE_GAP: f32 = 0.3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_split_into_the_weapon_and_its_parts() {
        assert_eq!(split_key("combat_arms:m4a1"), ("combat_arms:m4a1", vec![]));
        assert_eq!(
            split_key("combat_arms:m4a1+Silencer_A_AR+DotSight_A_AR"),
            ("combat_arms:m4a1", vec!["Silencer_A_AR", "DotSight_A_AR"])
        );
    }

    #[test]
    fn a_zero_offset_is_the_socket_itself() {
        assert_eq!(offset_matrix(&PartOffset::default()), Mat4::IDENTITY);
    }
}
