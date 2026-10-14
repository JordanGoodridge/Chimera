//! Drawn loose items: a dropped weapon's meshes (`map::loose::LooseItem`)
//! and an item that moves itself (`map::loose::ShownItem`, e.g. a thrown
//! grenade or the planted bomb, `SitOnOrigin`), each lit where it is
//! (`probe_lit`, its own material instances). The item components are the
//! simulation's requests (a held model's key); the body is
//! `map::loose::attach_loose`'s, the meshes these systems add, from the
//! held models' whole meshes (`render::bodies::HeldAssets`) placed by the
//! loose form's frame (`map::loose::LooseAssets`).

use bevy::prelude::*;

use super::bodies::HeldAssets;
use crate::map::{
    MapPart, probe_lit,
    loose::{LooseAssets, LooseItem, ShownItem, SitOnOrigin},
};

/// Marks a `LooseItem` whose meshes were added.
#[derive(Component)]
pub(crate) struct LooseItemDrawn;

/// Give new loose items their meshes, lit where they lie.
pub(crate) fn draw_loose(
    items: Query<(Entity, &LooseItem), Without<LooseItemDrawn>>,
    assets: Option<Res<LooseAssets>>,
    held: Option<Res<HeldAssets>>,
    mut materials: Option<ResMut<Assets<crate::map::prop_material::PropMaterial>>>,
    mut commands: Commands,
) {
    let (Some(assets), Some(held)) = (assets, held) else { return };
    for (e, item) in &items {
        // `try_`: an item can be gone by the time this applies (picked up,
        // or a network client's copy despawned, in the same frame).
        commands.entity(e).try_insert(LooseItemDrawn);
        let (Some(a), Some(parts)) = (assets.0.get(&item.0), held.0.get(&item.0)) else {
            continue;
        };
        // Its own materials, lit where it lies (probe_lit).
        let own: Vec<_> = parts
            .whole
            .iter()
            .map(|(_, m)| match materials.as_mut() {
                Some(assets) => probe_lit::instance(assets, m),
                None => m.clone(),
            })
            .collect();
        commands
            .entity(e)
            .try_insert(probe_lit::ProbeLit::new(own.clone(), Vec3::ZERO));
        let parts: Vec<_> = parts.whole.iter().map(|(mesh, _)| mesh.clone()).zip(own).collect();
        let frame = a.frame;
        commands.queue(move |w: &mut World| {
            if w.get_entity(e).is_err() {
                return;
            }
            for (mesh, material) in parts {
                w.spawn((Mesh3d(mesh), MeshMaterial3d(material), frame, ChildOf(e)));
            }
        });
    }
}

/// Marks a `ShownItem` whose meshes were added.
#[derive(Component)]
pub(crate) struct ShownItemDrawn;

/// Give new shown items their meshes.
pub(crate) fn attach_shown(
    items: Query<(Entity, &ShownItem, Option<&SitOnOrigin>), Without<ShownItemDrawn>>,
    assets: Option<Res<LooseAssets>>,
    held: Option<Res<HeldAssets>>,
    mut materials: Option<ResMut<Assets<crate::map::prop_material::PropMaterial>>>,
    mut commands: Commands,
) {
    let (Some(assets), Some(held)) = (assets, held) else { return };
    for (e, item, sit) in &items {
        let mut ent = commands.entity(e);
        ent.insert((ShownItemDrawn, MapPart, Visibility::default()));
        if let (Some(a), Some(parts)) = (assets.0.get(&item.0), held.0.get(&item.0)) {
            let mut frame = a.frame;
            if let Some(SitOnOrigin(turn)) = sit {
                let m = Mat3::from_quat(*turn);
                let half = m.x_axis.abs() * a.half.x + m.y_axis.abs() * a.half.y + m.z_axis.abs() * a.half.z;
                frame = Transform::from_rotation(*turn) * frame;
                frame.translation.y += half.y;
            }
            // Its own materials, lit where it is (probe_lit).
            let own: Vec<_> = parts
                .whole
                .iter()
                .map(|(_, m)| match materials.as_mut() {
                    Some(assets) => probe_lit::instance(assets, m),
                    None => m.clone(),
                })
                .collect();
            ent.insert(probe_lit::ProbeLit::new(own.clone(), Vec3::ZERO));
            ent.with_children(|c| {
                for ((mesh, _), material) in parts.whole.iter().zip(own) {
                    c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material), frame));
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::bodies::HeldParts;
    use std::collections::HashMap;

    /// An item despawned before its meshes are put in place (picked up, a
    /// network client's copy gone, the same frame) is no error.
    #[test]
    fn an_item_gone_in_the_same_frame_is_no_error() {
        let mut app = App::new();
        app.set_error_handler(bevy::ecs::error::panic);
        let mut assets = HashMap::new();
        assets.insert("gun".to_string(), crate::map::loose::LooseAsset::stand_in());
        app.insert_resource(LooseAssets(assets));
        let mut held = HashMap::new();
        held.insert(
            "gun".to_string(),
            HeldParts {
                bone: String::new(),
                whole: vec![(Handle::default(), Handle::default())],
                own: Vec::new(),
                muzzle: None,
                merged: Vec::new(),
            },
        );
        app.insert_resource(HeldAssets(held));
        fn take(items: Query<Entity, With<LooseItem>>, mut commands: Commands) {
            for e in &items {
                commands.entity(e).despawn();
            }
        }
        // Both queue their commands; the despawn applies first.
        app.add_systems(Update, (take, draw_loose).chain_ignore_deferred());
        app.world_mut().spawn(LooseItem("gun".into()));
        app.update();
        let w = app.world_mut();
        assert_eq!(w.query::<&LooseItem>().iter(w).count(), 0);
        assert_eq!(w.query::<&Mesh3d>().iter(w).count(), 0, "no orphan parts");
    }
}
