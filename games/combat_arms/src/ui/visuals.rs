//! Combat Arms' placeholder looks (wave 0), until its models load: the
//! local player's weapon in first person as a box (a long dark one for a
//! gun, a thin light one for a melee weapon). Other characters are the
//! shell's capsules (`client::hud`). Nothing here is Combat Arms' look.
//!
//! The seam: Combat Arms' models (players, view models, held weapons)
//! come from its install through its loader (specs/combat_arms/models_ltb.md,
//! characters.md; docs/plans/active/combat-arms.md) as `map` data the presentation already draws
//! (`map::bodies`, `map::view_model`); this module then goes. Choosing a
//! character's body and view-model animation by game data
//! (games/cs_source/src/game/player_anim.rs, view-model events in
//! games/cs_source/src/game/) is a lift candidate for Chimera.

use bevy::prelude::*;

use crate::{
    client::FirstPersonCamera,
    core::LocalPlayer,
    weapon::{Hitscan, Inventory, Melee},
};

/// The placeholder looks.
pub struct CaVisualsPlugin;

impl Plugin for CaVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, box_gun);
    }
}

/// The box drawn for the local player's weapon, and which weapon it shows.
#[derive(Component)]
pub struct BoxGun(pub Entity);

/// Keep a box in front of the first-person camera for the local player's
/// active weapon: a gun's or a melee weapon's shape, none for anything
/// else (or no weapon).
fn box_gun(
    player: Option<Single<&Inventory, With<LocalPlayer>>>,
    kinds: Query<(Has<Hitscan>, Has<Melee>)>,
    camera: Option<Single<Entity, With<FirstPersonCamera>>>,
    shown: Query<(Entity, &BoxGun)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let active = player.and_then(|inv| inv.active);
    if shown.iter().any(|(_, b)| Some(b.0) == active) {
        return;
    }
    for (e, _) in &shown {
        commands.entity(e).despawn();
    }
    let (Some(weapon), Some(camera)) = (active, camera) else {
        return;
    };
    let Ok((gun, melee)) = kinds.get(weapon) else {
        return;
    };
    // Metres, in the camera's space (-Z ahead, +X right, +Y up).
    let (size, colour, at) = if gun {
        (Vec3::new(0.04, 0.06, 0.45), Color::srgb(0.15, 0.15, 0.17), Vec3::new(0.16, -0.14, -0.45))
    } else if melee {
        (Vec3::new(0.02, 0.04, 0.3), Color::srgb(0.75, 0.75, 0.8), Vec3::new(0.2, -0.15, -0.35))
    } else {
        return;
    };
    let mesh = meshes.add(Cuboid::from_size(size));
    let material = materials.add(StandardMaterial {
        base_color: colour,
        perceptual_roughness: 0.6,
        ..default()
    });
    let entity = commands
        .spawn((
            BoxGun(weapon),
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_translation(at),
        ))
        .id();
    commands.entity(*camera).add_child(entity);
}
