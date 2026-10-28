//! An opt-in effect for a rung's swap (docs/DESIGN.md, "One HUD layer over
//! several games' HUDs"): where a character's game follows its gun
//! (`chimera_sim::identity`), its body, HUD and movement change on the tick,
//! a hard cut. Neither game has a swap, so parity is none: off
//! (`party_swap_effect 0`, archived; the party apps add the plugin).
//!
//! With `party_swap_effect 1`, when a living character's `GameIdentity`
//! changes to another game's (not its first one, not while dead): the
//! local player's screen flashes white and fades over `FLASH_SECONDS`
//! while the new gun draws (its own game's draw, which the swap already
//! plays); another character gets a pale puff at its body that grows and
//! fades over `PUFF_SECONDS`. `SwapEffectView` is the state, headless too
//! (tests read it). Generic, no game's look.
//!
//! Known shortcuts: no sound; the puff is one sphere, not particles.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::{
    core::{Dead, LocalPlayer},
    slots::GameIdentity,
};

/// Seconds the local flash takes to fade.
pub const FLASH_SECONDS: f32 = 0.3;
/// The flash's opacity at its start.
const FLASH_ALPHA: f32 = 0.45;
/// Seconds a puff lives, its size at the end (m) and its opacity.
pub const PUFF_SECONDS: f32 = 0.5;
const PUFF_SIZE: f32 = 1.4;
const PUFF_ALPHA: f32 = 0.5;

/// `party_swap_effect`: 1 plays the effect, 0 (the default) does not.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SwapEffectSetting {
    pub on: u8,
}

/// The effect now: the flash's seconds left (0: none), and how many local
/// swaps and others' puffs it has shown.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct SwapEffectView {
    pub flash: f32,
    pub swaps: u32,
    pub puffs: u32,
}

#[derive(Component)]
struct SwapFlash;

#[derive(Component)]
struct SwapPuff {
    age: f32,
}

/// The setting, the state, the flash's node and the puffs.
pub struct SwapEffectPlugin;

impl Plugin for SwapEffectPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SwapEffectSetting>()
            .init_resource::<SwapEffectView>()
            .add_systems(Startup, spawn)
            .add_systems(Update, (detect, (flash, puffs)).chain());
        if app.world().get_resource::<crate::console::Console>().is_some() {
            crate::console::resource_cvar::<SwapEffectSetting, u8>(
                app,
                "party_swap_effect",
                "1: a short flash when your gun changes game on a rung, a puff at others' bodies when theirs does; 0: off.",
                |s| &mut s.on,
            );
            app.world_mut().resource_mut::<crate::console::Console>().archive("party_swap_effect");
        }
    }
}

fn spawn(mut commands: Commands) {
    commands.spawn((
        SwapFlash,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100.0),
            height: percent(100.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
        Pickable::IGNORE,
        GlobalZIndex(41),
        Visibility::Hidden,
    ));
}

/// Swaps since the last frame: each character's identity against the one
/// it had.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn detect(
    changed: Query<(Entity, &GameIdentity, Has<LocalPlayer>, Has<Dead>, &GlobalTransform), Changed<GameIdentity>>,
    mut gone: RemovedComponents<GameIdentity>,
    mut last: Local<HashMap<Entity, String>>,
    setting: Res<SwapEffectSetting>,
    mut view: ResMut<SwapEffectView>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
    mut sphere: Local<Option<Handle<Mesh>>>,
    mut commands: Commands,
) {
    for e in gone.read() {
        last.remove(&e);
    }
    let (mut meshes, mut materials) = (meshes, materials);
    for (e, identity, local, dead, at) in &changed {
        let before = last.insert(e, identity.0.clone());
        let swapped = before.is_some_and(|b| !b.eq_ignore_ascii_case(&identity.0));
        if !swapped || dead || setting.on == 0 {
            continue;
        }
        if local {
            view.flash = FLASH_SECONDS;
            view.swaps += 1;
            continue;
        }
        view.puffs += 1;
        let look = match (meshes.as_deref_mut(), materials.as_deref_mut()) {
            (Some(meshes), Some(materials)) => Some((
                sphere.get_or_insert_with(|| meshes.add(Sphere::new(0.5))).clone(),
                materials.add(StandardMaterial {
                    base_color: Color::linear_rgba(0.9, 0.9, 0.95, PUFF_ALPHA),
                    unlit: true,
                    alpha_mode: AlphaMode::Blend,
                    ..default()
                }),
            )),
            _ => None,
        };
        let mut puff = commands.spawn((
            SwapPuff { age: 0.0 },
            Transform::from_translation(at.translation()).with_scale(Vec3::splat(0.1)),
        ));
        if let Some((mesh, material)) = look {
            puff.insert((Mesh3d(mesh), MeshMaterial3d(material)));
        }
    }
}

fn flash(
    time: Res<Time>,
    setting: Res<SwapEffectSetting>,
    mut view: ResMut<SwapEffectView>,
    node: Option<Single<(&mut Visibility, &mut BackgroundColor), With<SwapFlash>>>,
) {
    if setting.on == 0 {
        view.flash = 0.0;
    }
    let left = view.flash;
    view.flash = (view.flash - time.delta_secs()).max(0.0);
    if let Some(n) = node {
        let (mut vis, mut color) = n.into_inner();
        *vis = if left > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        color.0 = Color::srgba(1.0, 1.0, 1.0, FLASH_ALPHA * (left / FLASH_SECONDS).clamp(0.0, 1.0));
    }
}

/// Grow and fade each puff; despawn it at the end of its life.
fn puffs(
    mut puffs: Query<(Entity, &mut SwapPuff, &mut Transform, Option<&MeshMaterial3d<StandardMaterial>>)>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
    time: Res<Time>,
    mut commands: Commands,
) {
    let mut materials = materials;
    for (e, mut p, mut t, m) in &mut puffs {
        p.age += time.delta_secs();
        let f = p.age / PUFF_SECONDS;
        if f >= 1.0 {
            if let (Some(m), Some(materials)) = (m, materials.as_deref_mut()) {
                materials.remove(&m.0);
            }
            commands.entity(e).despawn();
            continue;
        }
        t.scale = Vec3::splat(0.1 + (PUFF_SIZE - 0.1) * f.sqrt());
        if let (Some(m), Some(materials)) = (m, materials.as_deref_mut())
            && let Some(mut mat) = materials.get_mut(&m.0)
        {
            mat.base_color.set_alpha(PUFF_ALPHA * (1.0 - f));
        }
    }
}
