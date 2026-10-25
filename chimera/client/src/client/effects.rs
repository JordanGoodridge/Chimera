//! Shot feedback until the games' own effects load: tracer streaks and
//! bullet marks on static surfaces. Placeholders, not CS:S's look (its
//! tracer sprites, impact decals by surface and particle puffs).
//!
//! `GenericImpactsPlugin`: Chimera's generic bullet impact, for impacts
//! no game in the app can draw on the map (`weapon::impact`): a mark in
//! the hit surface's class colour and a fading dust puff.

use std::collections::VecDeque;

use avian3d::prelude::RigidBody;
use bevy::prelude::*;

use crate::{
    core::LocalPlayer,
    weapon::{WeaponEvent, WeaponEventKind},
};

pub struct ShotEffectsPlugin;

impl Plugin for ShotEffectsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Marks>()
            .add_systems(Startup, setup)
            .add_systems(Update, (spawn_effects, move_tracers));
    }
}

/// Tracer speed and length, m/s and m (unmeasured; looks plausible).
const TRACER_SPEED: f32 = 250.0;
const TRACER_LENGTH: f32 = 2.5;
const TRACER_WIDTH: f32 = 0.012;
/// The local player's tracers start this far below and right of the eye
/// (roughly where a gun would be), and only every Nth shot shows one.
const LOCAL_OFFSET: Vec3 = Vec3::new(0.12, -0.12, 0.0);
const LOCAL_TRACER_EVERY: u32 = 3;
const MARK_SIZE: f32 = 0.05;
const MAX_MARKS: usize = 128;

#[derive(Resource)]
struct Assets3 {
    tracer: Handle<StandardMaterial>,
    mark: Handle<StandardMaterial>,
    unit_cube: Handle<Mesh>,
    quad: Handle<Mesh>,
}

#[derive(Resource, Default)]
struct Marks {
    spawned: VecDeque<Entity>,
    local_shots: u32,
}

#[derive(Component)]
struct Tracer {
    from: Vec3,
    dir: Vec3,
    length: f32,
    travelled: f32,
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(Assets3 {
        tracer: materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.85, 0.5, 0.8),
            unlit: true,
            alpha_mode: AlphaMode::Add,
            ..default()
        }),
        mark: materials.add(StandardMaterial {
            base_color: Color::srgba(0.05, 0.04, 0.03, 0.85),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            depth_bias: 50.0,
            ..default()
        }),
        unit_cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        quad: meshes.add(Circle::new(MARK_SIZE / 2.0)),
    });
}

fn spawn_effects(
    mut events: MessageReader<WeaponEvent>,
    local: Option<Single<Entity, With<LocalPlayer>>>,
    bodies: Query<&RigidBody>,
    assets: Res<Assets3>,
    mut marks: ResMut<Marks>,
    decals: Option<Res<crate::render::decal::ImpactDecals>>,
    mut commands: Commands,
) {
    let me = local.map(|l| *l);
    for e in events.read().filter(|e| e.shown()) {
        let WeaponEventKind::Shot { from, to, hit, normal } = &e.kind else {
            continue;
        };
        let mine = Some(e.owner) == me;
        let show = if mine {
            marks.local_shots += 1;
            marks.local_shots % LOCAL_TRACER_EVERY == 1
        } else {
            true
        };
        if show {
            let dir = (*to - *from).normalize_or_zero();
            // Offset the local player's start toward the gun side.
            let start = if mine {
                let right = dir.cross(Vec3::Y).normalize_or_zero();
                *from + right * LOCAL_OFFSET.x + Vec3::Y * LOCAL_OFFSET.y + dir * 0.5
            } else {
                *from + dir * 0.6
            };
            let length = (*to - start).length();
            commands.spawn((
                Tracer {
                    from: start,
                    dir: (*to - start).normalize_or_zero(),
                    length,
                    travelled: 0.0,
                },
                Mesh3d(assets.unit_cube.clone()),
                MeshMaterial3d(assets.tracer.clone()),
                Transform::from_translation(start).with_scale(Vec3::ZERO),
            ));
        }
        // Marks only on things that don't move, and only where the map
        // has no decals of its own.
        let (Some(hit), Some(n)) = (hit, normal) else { continue };
        if decals.is_some() {
            continue;
        }
        if bodies.get(*hit).is_ok_and(|b| !b.is_static()) {
            continue;
        }
        let mark = commands
            .spawn((
                Mesh3d(assets.quad.clone()),
                MeshMaterial3d(assets.mark.clone()),
                Transform::from_translation(*to + *n * 0.003).looking_to(-*n, any_up(*n)),
            ))
            .id();
        marks.spawned.push_back(mark);
        while marks.spawned.len() > MAX_MARKS {
            if let Some(old) = marks.spawned.pop_front() {
                commands.entity(old).despawn();
            }
        }
    }
}

fn any_up(n: Vec3) -> Vec3 {
    if n.y.abs() > 0.9 { Vec3::Z } else { Vec3::Y }
}

/// Move each streak along its path; despawn at the end.
fn move_tracers(mut tracers: Query<(Entity, &mut Tracer, &mut Transform)>, time: Res<Time>, mut commands: Commands) {
    for (e, mut t, mut tf) in &mut tracers {
        t.travelled += TRACER_SPEED * time.delta_secs();
        let head = t.travelled.min(t.length);
        let tail = (t.travelled - TRACER_LENGTH).max(0.0);
        if tail >= t.length {
            commands.entity(e).despawn();
            continue;
        }
        let mid = t.from + t.dir * (head + tail) / 2.0;
        *tf = Transform::from_translation(mid)
            .looking_to(t.dir, any_up(t.dir))
            .with_scale(Vec3::new(TRACER_WIDTH, TRACER_WIDTH, (head - tail).max(0.001)));
    }
}

/// Chimera's generic impact (`weapon::impact::ImpactBy::Generic`: no game
/// in the app can draw this gun's impacts on this map): a dark mark in
/// the hit surface's class colour (`map::impact::SurfaceClass`) on what
/// doesn't move, and a puff of the class's dust that grows and fades.
/// No sound: Chimera ships no game's files.
pub struct GenericImpactsPlugin;

impl Plugin for GenericImpactsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GenericMarks>()
            .add_systems(Update, (generic_impacts, fade_puffs));
    }
}

/// Most generic marks kept; the oldest goes first.
const MAX_GENERIC_MARKS: usize = 256;
/// The puff's life, s, and its size at the end, m.
const PUFF_LIFE: f32 = 0.45;
const PUFF_SIZE: f32 = 0.25;

#[derive(Resource, Default)]
struct GenericMarks {
    spawned: VecDeque<Entity>,
    mesh: Option<(Handle<Mesh>, Handle<Mesh>)>,
    /// Each class's mark material, made on first use.
    looks: Vec<(crate::map::impact::SurfaceClass, Handle<StandardMaterial>)>,
}

#[derive(Component)]
struct Puff {
    age: f32,
}

#[allow(clippy::too_many_arguments)]
fn generic_impacts(
    mut events: MessageReader<WeaponEvent>,
    route: crate::weapon::impact::ImpactRoute,
    bodies: Query<&RigidBody>,
    characters: Query<(), With<crate::core::Intent>>,
    grid: Option<Res<crate::map::sound::SurfaceGrid>>,
    classes: Option<Res<crate::map::impact::SurfaceClasses>>,
    mut marks: ResMut<GenericMarks>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    use crate::map::impact::SurfaceClass;
    for e in events.read().filter(|e| e.shown()) {
        let (WeaponEventKind::Shot { to, hit: Some(hit), normal: Some(n), .. }
        | WeaponEventKind::ShotContinued { to, hit: Some(hit), normal: Some(n), .. }) = &e.kind
        else {
            continue;
        };
        if route.by(e) != crate::weapon::impact::ImpactBy::Generic {
            continue;
        }
        let class = if characters.contains(*hit) {
            SurfaceClass::Flesh
        } else {
            let name = grid.as_deref().and_then(|g| g.nearest(*to, 0.3)).unwrap_or("default");
            classes.as_deref().map_or_else(|| SurfaceClass::guess(name), |c| c.classify(route.map_game(), name))
        };
        let (quad, sphere) = marks
            .mesh
            .get_or_insert_with(|| (meshes.add(Circle::new(MARK_SIZE / 2.0)), meshes.add(Sphere::new(0.5))))
            .clone();
        let up = any_up(*n);
        if class != SurfaceClass::Flesh && !bodies.get(*hit).is_ok_and(|b| !b.is_static()) {
            let material = match marks.looks.iter().find(|(c, _)| *c == class) {
                Some((_, m)) => m.clone(),
                None => {
                    let c = class.mark_color();
                    let m = materials.add(StandardMaterial {
                        base_color: Color::linear_rgba(c.x, c.y, c.z, 0.9),
                        unlit: true,
                        alpha_mode: AlphaMode::Blend,
                        depth_bias: 50.0,
                        ..default()
                    });
                    marks.looks.push((class, m.clone()));
                    m
                }
            };
            let mark = commands
                .spawn((
                    Mesh3d(quad),
                    MeshMaterial3d(material),
                    Transform::from_translation(*to + *n * 0.003).looking_to(-*n, up),
                ))
                .id();
            marks.spawned.push_back(mark);
            while marks.spawned.len() > MAX_GENERIC_MARKS {
                if let Some(old) = marks.spawned.pop_front() {
                    commands.entity(old).despawn();
                }
            }
        }
        let d = class.dust_color();
        let material = materials.add(StandardMaterial {
            base_color: Color::linear_rgba(d.x, d.y, d.z, 0.6),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        });
        commands.spawn((
            Puff { age: 0.0 },
            Mesh3d(sphere),
            MeshMaterial3d(material),
            Transform::from_translation(*to + *n * 0.05).with_scale(Vec3::splat(0.02)),
        ));
    }
}

/// Grow and fade each puff; despawn it at the end of its life.
fn fade_puffs(
    mut puffs: Query<(Entity, &mut Puff, &mut Transform, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    time: Res<Time>,
    mut commands: Commands,
) {
    for (e, mut p, mut t, m) in &mut puffs {
        p.age += time.delta_secs();
        let f = p.age / PUFF_LIFE;
        if f >= 1.0 {
            materials.remove(&m.0);
            commands.entity(e).despawn();
            continue;
        }
        t.scale = Vec3::splat(0.02 + (PUFF_SIZE - 0.02) * f.sqrt());
        if let Some(mut mat) = materials.get_mut(&m.0) {
            mat.base_color.set_alpha(0.6 * (1.0 - f));
        }
    }
}
