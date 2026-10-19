//! Combat Arms' placeholder looks (wave 0), until its models load: the
//! local player's weapon in first person as a box (a long dark one for a
//! gun, a thin light one for a melee weapon) when the map has no view
//! model for it (`game::view_model`: keyed ones without the V2 key);
//! grenades in flight as small
//! dark balls and smoke clouds as grey balls of the cloud's size
//! (`grenade_balls`, `smoke_balls`); the first-person muzzle flash as an
//! orange ball at the view model's `Flash` socket while
//! `game::view_model::CaViewModel` shows it (`muzzle_flash`, drawn in the
//! view-model pass) and first-person shells (`CaShell`) as small brass
//! boxes (`shell_boxes`). Other characters are the shell's capsules
//! (`client::hud`). Nothing here is Combat Arms' look (effects_sounds.md
//! is its own item).
//!
//! The seam: Combat Arms' models (players, view models, held weapons)
//! come from its install through its loader (specs/combat_arms/models_ltb.md,
//! characters.md; docs/plans/active/combat-arms.md) as `map` data the presentation already draws
//! (`map::bodies`, `map::view_model`); this module then goes. Choosing a
//! character's body and view-model animation by game data
//! (games/cs_source/src/game/player_anim.rs, view-model events in
//! games/cs_source/src/game/) is a lift candidate for Chimera.

use bevy::prelude::*;

use bevy::camera::visibility::RenderLayers;

use crate::{
    client::FirstPersonCamera,
    core::LocalPlayer,
    game::view_model::{CaShell, CaViewModel},
    map::{
        ViewModels,
        loose::ShownItem,
        view_model::{VIEW_MODEL_LAYER, ViewModelCamera},
    },
    weapon::{Hitscan, Inventory, Melee, Weapon, grenade::SmokeCloud},
};

/// The placeholder looks.
pub struct CaVisualsPlugin;

impl Plugin for CaVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                box_gun,
                grenade_balls,
                smoke_balls,
                muzzle_flash.after(crate::map::DriveAnimation),
                shell_boxes,
            ),
        );
    }
}

/// The box drawn for the local player's weapon, and which weapon it shows.
#[derive(Component)]
pub struct BoxGun(pub Entity);

/// Keep a box in front of the first-person camera for the local player's
/// active weapon: a gun's or a melee weapon's shape, none for anything
/// else (or no weapon, or one the map has a view model for).
#[allow(clippy::too_many_arguments)]
fn box_gun(
    player: Option<Single<&Inventory, With<LocalPlayer>>>,
    kinds: Query<(&Weapon, Has<Hitscan>, Has<Melee>)>,
    view_models: Option<Res<ViewModels>>,
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
    let Ok((w, gun, melee)) = kinds.get(weapon) else {
        return;
    };
    if view_models.is_some_and(|v| v.get(w.id).is_some()) {
        return;
    }
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

/// A Combat Arms grenade in flight (its `ShownItem`, on the server's world
/// or a client's) gets a small dark ball.
fn grenade_balls(
    new: Query<(Entity, &ShownItem), (Added<ShownItem>, Without<Mesh3d>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    for (e, item) in &new {
        if !item.0.starts_with(crate::game::GAME) {
            continue;
        }
        commands.entity(e).insert((
            Mesh3d(meshes.add(Sphere::new(0.04))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.2, 0.25, 0.15),
                perceptual_roughness: 0.7,
                ..default()
            })),
            Visibility::default(),
        ));
    }
}

/// A smoke cloud is a translucent grey ball of its radius and alpha.
fn smoke_balls(
    mut clouds: Query<(Entity, &SmokeCloud, Option<&mut Transform>, Has<Mesh3d>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    time: Res<Time>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for (e, cloud, transform, drawn) in &mut clouds {
        let t = (now - cloud.started) as f32;
        let r = cloud.radius(t).max(1e-3);
        let pose = Transform::from_translation(cloud.centre).with_scale(Vec3::splat(r));
        if !drawn {
            commands.entity(e).insert((
                Mesh3d(meshes.add(Sphere::new(1.0))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgba(0.7, 0.7, 0.7, 0.8),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    ..default()
                })),
                pose,
                Visibility::default(),
            ));
        } else if let Some(mut tr) = transform {
            *tr = pose;
        }
    }
}

/// The placeholder flash, a child of the view-model camera.
#[derive(Component)]
pub struct FlashBall;

/// Show the local player's first-person muzzle flash (view_models.md 9)
/// as a small orange ball where `CaViewModel::flash_at` says, in the
/// view-model pass.
fn muzzle_flash(
    player: Option<Single<&CaViewModel, With<LocalPlayer>>>,
    camera: Option<Single<Entity, With<ViewModelCamera>>>,
    mut balls: Query<(&mut Transform, &mut Visibility), With<FlashBall>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let at = player.and_then(|vm| vm.flash_at);
    let Ok((mut t, mut v)) = balls.single_mut() else {
        let Some(camera) = camera else { return };
        let ball = commands
            .spawn((
                FlashBall,
                Mesh3d(meshes.add(Sphere::new(0.02))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgba(1.0, 0.7, 0.25, 0.85),
                    emissive: LinearRgba::rgb(4.0, 2.2, 0.6),
                    alpha_mode: AlphaMode::Add,
                    unlit: true,
                    ..default()
                })),
                Transform::default(),
                Visibility::Hidden,
                RenderLayers::layer(VIEW_MODEL_LAYER),
                bevy::light::NotShadowCaster,
            ))
            .id();
        commands.entity(*camera).add_child(ball);
        return;
    };
    match at {
        Some(p) => {
            t.translation = p;
            *v = Visibility::Visible;
        }
        None => *v = Visibility::Hidden,
    }
}

/// First-person shells (view_models.md 10) as small brass boxes, grown by
/// their scale.
fn shell_boxes(
    mut shells: Query<(Entity, &CaShell, &mut Transform, Has<Mesh3d>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    for (e, shell, mut t, drawn) in &mut shells {
        t.scale = Vec3::splat(shell.scale);
        if !drawn {
            commands.entity(e).insert((
                Mesh3d(meshes.add(Cuboid::new(0.006, 0.006, 0.016))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(0.8, 0.62, 0.25),
                    metallic: 0.8,
                    perceptual_roughness: 0.35,
                    ..default()
                })),
                Visibility::default(),
            ));
        }
    }
}
