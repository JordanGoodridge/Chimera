//! Placed props' looks as drawn: each prop node's mesh slots show the
//! skin and body group the logic gives it (`map::PropLook`), switching
//! their materials and showing the body parts' meshes. `PropVariants` and
//! `PropMeshSlot` are put on a prop's node and meshes when it spawns
//! (`render::spawn`).

use bevy::prelude::*;

use super::prop_material::PropMaterial;
use crate::map::{PropLook, body_choice};

/// A prop model's mesh (index into `MapModel::meshes`) drawn under the
/// prop's node.
#[derive(Component, Clone, Copy, Debug)]
pub struct PropMeshSlot {
    pub mesh: usize,
}

/// What a prop entity can switch to: its materials per skin family
/// (empty with one family), its body layout, and what it shows (skin,
/// body number).
#[derive(Component)]
pub struct PropVariants {
    pub skins: Vec<Vec<Handle<PropMaterial>>>,
    /// The body number it spawned with.
    pub body: i32,
    pub parts: Vec<u16>,
    pub mesh_bodies: Vec<Option<(u16, u16)>>,
    pub shown: (i32, i32),
}

/// Prop entities show the skin and body group the logic gives them
/// (`PropLook`). A skin the model doesn't have leaves it as it is.
pub fn apply_prop_looks(
    mut props: Query<(&PropLook, &mut PropVariants, &Children), Changed<PropLook>>,
    mut slots: Query<(
        &PropMeshSlot,
        Option<&mut MeshMaterial3d<PropMaterial>>,
        &mut Visibility,
    )>,
) {
    for (look, mut v, children) in &mut props {
        // SetBodyGroup sets the whole body number (specs/source/prop_damage.md 8).
        let body = look.body_group.unwrap_or(v.body);
        let skin = if (look.skin.max(0) as usize) < v.skins.len() {
            look.skin
        } else {
            v.shown.0
        };
        if (skin, body) == v.shown {
            continue;
        }
        v.shown = (skin, body);
        for child in children {
            let Ok((slot, material, mut visibility)) = slots.get_mut(*child) else {
                continue;
            };
            if let (Some(mut material), Some(want)) =
                (material, v.skins.get(skin as usize).and_then(|s| s.get(slot.mesh)))
                && material.0 != *want
            {
                material.0 = want.clone();
            }
            let shown = v
                .mesh_bodies
                .get(slot.mesh)
                .copied()
                .flatten()
                .is_none_or(|(part, choice)| body_choice(&v.parts, body, part as usize) == choice);
            visibility.set_if_neq(if shown {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
    }
}
