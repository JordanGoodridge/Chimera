//! Character bodies: each character's drawn body (its team's model,
//! skinned to an animated skeleton), what it holds, and its hitboxes
//! following the skeleton (also headless, for the server's hit tests).
//!
//! Character bodies (by team or `BodyName`; the local one hidden unless
//! `ShowLocalBody`), held models, hitboxes on the skeleton (also headless).
//!
//! Known shortcuts (docs/tech-debt.md): Maps without character models fall back
//! to height bands on the collider for hitgroups; in single player hitboxes
//! follow the client-rate animation (a network server poses them per tick from
//! its own animation, `map::SimAnimator`), which takes other characters' look
//! as drawn (eased between ticks, `map::interp`), so their hitboxes trail the
//! simulated look by up to a tick; the server's per-tick animation runs CS:S's
//! player animation state a second time (twice the posing work per character on
//! a listen server) and its hitboxes for tick N come from the end of tick N-1.
//! Why it's OK for now: Matches what players see; lag compensation hits rewound
//! poses (tests/it/heavy/map_net_weapons.rs).
//!
//! Known shortcuts (docs/tech-debt.md): Frame-time shortcuts
//! (docs/performance.md, "Frame time pass"): bodies no camera drew last frame
//! are posed every 4th frame (`map::UNSEEN_POSE_INTERVAL`), so an unseen
//! shooter's muzzle (where its tracer starts) can be up to three frames behind
//! its animation; dust motes' and particles' meshes are padded to power-of-two
//! sizes with degenerate triangles; a physics prop's shadow is redrawn only
//! after it moves by a quarter of a shadow texel (a creeping prop's shadow lags
//! it by up to that); the per-frame schedules run on single-threaded executors
//! (`MASHUP_EXECUTOR=multi`: Bevy's). Why it's OK for now: Measured faster on
//! the dev box; hitboxes pose every tick on their own; the Windows PC is still
//! to be measured.

#[allow(unused_imports)]
use super::*;

/// What a character holds (a `MapHeldModel` key), drawn in its body's
/// hand. Games set it.
#[derive(Component, Clone, Default, Debug, PartialEq)]
pub struct Held(pub Option<String>);

/// Which character model a character uses (index in `CharacterModels`).
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub struct BodyModel(pub usize);
/// The loaded map's character models (see `MapCharacterModel`).
#[derive(Resource, Clone)]
pub struct CharacterModels(pub Arc<Vec<MapCharacterModel>>);

impl CharacterModels {
    /// The model for `team`: its own, else one without a team, else any.
    pub fn for_team(&self, team: Option<Team>) -> Option<(usize, &MapCharacterModel)> {
        let all = self
            .0
            .iter()
            .enumerate()
            .filter(|(_, m)| m.name.is_none() && m.class.is_none());
        all.clone()
            .find(|(_, m)| team.is_some() && m.team == team)
            .or_else(|| all.clone().find(|(_, m)| m.team.is_none()))
            .or_else(|| all.clone().next())
    }

    /// The model a character uses: the one its `BodyName` names, else its
    /// class's (`core::PlayerClass`), else its team's.
    pub fn for_character(
        &self,
        team: Option<Team>,
        name: Option<&BodyName>,
        class: Option<&crate::core::PlayerClass>,
    ) -> Option<(usize, &MapCharacterModel)> {
        match name {
            Some(n) => self
                .0
                .iter()
                .enumerate()
                .find(|(_, m)| m.name.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(&n.0))),
            None => class
                .filter(|c| c.0 != 0 && c.team() == team)
                .and_then(|c| self.0.iter().enumerate().find(|(_, m)| m.class == Some(c.0)))
                .or_else(|| self.for_team(team)),
        }
    }
}

/// A character drawn with a named model (`MapCharacterModel::name`)
/// instead of its team's: a hostage.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct BodyName(pub String);

/// Meshes and materials of each character model (index as in
/// `CharacterModels`).
#[derive(Resource)]
pub(crate) struct CharacterBodies(pub(crate) Vec<BodyAssets>);

/// Meshes and materials of each held model, by key.
#[derive(Resource)]
#[allow(clippy::type_complexity)]
pub(crate) struct HeldAssets(pub(crate) HashMap<String, HeldParts>);

/// A held model's meshes on its bone, its muzzle there, and the parts
/// that follow other bones (`MapHeldModel::merged`).
#[allow(clippy::type_complexity)]
pub(crate) struct HeldParts {
    pub(crate) bone: String,
    /// The whole model on `bone` (loose items).
    pub(crate) whole: Vec<(Handle<Mesh>, Handle<PropMaterial>)>,
    /// What stays on `bone` when held.
    pub(crate) own: Vec<(Handle<Mesh>, Handle<PropMaterial>)>,
    pub(crate) muzzle: Option<Transform>,
    pub(crate) merged: Vec<(String, Vec<(Handle<Mesh>, Handle<PropMaterial>)>)>,
}

pub(crate) struct BodyAssets {
    pub(crate) parts: Vec<(Handle<Mesh>, Handle<PropMaterial>)>,
    pub(crate) bindposes: Handle<bevy::mesh::skinning::SkinnedMeshInverseBindposes>,
    pub(crate) bones: Vec<MapBone>,
    pub(crate) root: Transform,
}

/// A joint of a character body's skeleton: its bone index.
#[derive(Component)]
pub struct BodyJoint(pub usize);

/// A character's drawn body: which model, at its feet.
#[derive(Component)]
pub struct CharacterBody {
    pub(crate) model: usize,
    /// Joint entities by bone index.
    pub(crate) joints: Vec<Entity>,
    /// The held model shown and its key.
    pub(crate) held: Option<(Entity, String)>,
    /// Its bone-merged parts on other joints (`MapHeldModel::merged`).
    pub(crate) held_parts: Vec<Entity>,
}

/// Whether the local player's own body is drawn (third person). The
/// client sets it; the body is there either way, animated, and hidden
/// when this is false.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShowLocalBody(pub bool);

/// Give characters their team's body (a child at the feet); the local
/// player's is hidden unless `ShowLocalBody`.
#[allow(clippy::type_complexity)]
pub(crate) fn attach_bodies(
    models: Option<Res<CharacterModels>>,
    bodies: Option<Res<CharacterBodies>>,
    characters: Query<
        (
            Entity,
            (Option<&Team>, Option<&BodyName>, Option<&crate::core::PlayerClass>),
            &ColliderAabb,
            &GlobalTransform,
            Option<&Children>,
            Has<crate::core::LocalPlayer>,
        ),
        (With<crate::core::Intent>, Without<ragdoll::Ragdolled>),
    >,
    existing: Query<&CharacterBody>,
    show_local: Res<ShowLocalBody>,
    mut materials: Option<ResMut<Assets<PropMaterial>>>,
    mut commands: Commands,
) {
    let (Some(models), Some(bodies)) = (models, bodies) else {
        return;
    };
    for (e, (team, name, class), aabb, at, children, local) in &characters {
        let Some((index, _)) = models.for_character(team.copied(), name, class) else {
            continue;
        };
        let current = children
            .into_iter()
            .flatten()
            .find_map(|c| existing.get(*c).ok().map(|b| (*c, b.model)));
        match current {
            Some((_, m)) if m == index => continue,
            Some((old, _)) => commands.entity(old).despawn(),
            None => {}
        }
        let feet = aabb.min.y - at.translation().y;
        let assets = &bodies.0[index];
        let body = commands
            .spawn((
                Name::new("Body"),
                Transform::from_xyz(0.0, feet, 0.0),
                body_visibility(local, *show_local),
                ChildOf(e),
            ))
            .id();
        // The skeleton: a root in the source game's axes and units, then
        // each bone under its parent, in the reference pose.
        let root = commands.spawn((assets.root, Visibility::Inherited, ChildOf(body))).id();
        let mut joints: Vec<Entity> = Vec::with_capacity(assets.bones.len());
        for (i, b) in assets.bones.iter().enumerate() {
            let parent = b.parent.and_then(|p| joints.get(p).copied()).unwrap_or(root);
            joints.push(
                commands
                    .spawn((
                        BodyJoint(i),
                        Transform::from_translation(b.position).with_rotation(b.rotation),
                        Visibility::Inherited,
                        ChildOf(parent),
                    ))
                    .id(),
            );
        }
        commands.entity(body).insert(CharacterBody {
            model: index,
            joints: joints.clone(),
            held: None,
            held_parts: Vec::new(),
        });
        // Its own materials, lit where it stands (probe_lit), at its centre.
        let mut own = Vec::new();
        for (mesh, material) in &assets.parts {
            let material = match materials.as_mut() {
                Some(m) => probe_lit::instance(m, material),
                None => material.clone(),
            };
            own.push(material.clone());
            commands.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material),
                bevy::mesh::skinning::SkinnedMesh {
                    inverse_bindposes: assets.bindposes.clone(),
                    joints: joints.clone(),
                },
                ChildOf(body),
            ));
        }
        commands
            .entity(body)
            .insert(probe_lit::ProbeLit::new(own, Vec3::Y * BODY_LIGHT_HEIGHT));
    }
}

/// Height of a body's lighting point above its feet, meters (about the
/// models' own illumination position, their middle).
pub(crate) const BODY_LIGHT_HEIGHT: f32 = 0.9;

pub(crate) fn body_visibility(local: bool, show_local: ShowLocalBody) -> Visibility {
    if local && !show_local.0 {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    }
}

/// Show or hide the local player's body when `ShowLocalBody` changes.
pub(crate) fn show_local_body(
    show: Res<ShowLocalBody>,
    local: Query<&Children, With<crate::core::LocalPlayer>>,
    mut bodies: Query<&mut Visibility, With<CharacterBody>>,
) {
    let want = body_visibility(true, *show);
    for children in &local {
        let mut it = bodies.iter_many_mut(children);
        while let Some(mut vis) = it.fetch_next() {
            if *vis != want {
                *vis = want;
            }
        }
    }
}

/// Turn bodies with their character's yaw (as drawn: `interp`).
pub(crate) fn turn_bodies(
    characters: Query<(
        &crate::core::Intent,
        &Children,
        Option<&anim::Animator>,
        Option<&interp::RenderedView>,
    )>,
    mut bodies: Query<&mut Transform, With<CharacterBody>>,
) {
    for (intent, children, animator, view) in &characters {
        let yaw = animator
            .and_then(|a| a.yaw)
            .unwrap_or(view.map_or(intent.yaw, |v| v.now.yaw));
        for c in children {
            let turned = Quat::from_rotation_y(yaw);
            if let Ok(mut t) = bodies.get_mut(*c)
                && t.rotation != turned
            {
                t.rotation = turned;
            }
        }
    }
}

/// Draw what each character holds in its body's hand.
pub(crate) fn attach_held(
    held: Option<Res<HeldAssets>>,
    bodies: Option<Res<CharacterBodies>>,
    characters: Query<(Entity, &Held, &Children)>,
    mut body_query: Query<&mut CharacterBody>,
    mut materials: Option<ResMut<Assets<PropMaterial>>>,
    mut commands: Commands,
) {
    let (Some(held), Some(bodies)) = (held, bodies) else {
        return;
    };
    for (character, want, children) in &characters {
        let Some(c) = children.iter().find(|c| body_query.contains(*c)) else {
            continue;
        };
        let mut body = body_query.get_mut(c).unwrap();
        if body.held.as_ref().map(|(_, k)| k.as_str()) == want.0.as_deref() {
            continue;
        }
        if let Some((e, _)) = body.held.take() {
            commands.entity(e).despawn();
        }
        for e in std::mem::take(&mut body.held_parts) {
            commands.entity(e).despawn();
        }
        let Some(key) = &want.0 else { continue };
        let Some(parts) = held.0.get(key) else {
            continue;
        };
        let joint_of = |bone: &str| {
            bodies.0[body.model]
                .bones
                .iter()
                .position(|b| b.name.eq_ignore_ascii_case(bone))
                .and_then(|i| body.joints.get(i).copied())
        };
        let Some(joint) = joint_of(&parts.bone) else {
            continue;
        };
        // Its own materials, lit where it is (probe_lit).
        let mut instance = |list: &[(Handle<Mesh>, Handle<PropMaterial>)]| -> Vec<Handle<PropMaterial>> {
            list.iter()
                .map(|(_, m)| match materials.as_mut() {
                    Some(assets) => probe_lit::instance(assets, m),
                    None => m.clone(),
                })
                .collect()
        };
        let own = instance(&parts.own);
        let model = commands
            .spawn((
                Transform::default(),
                Visibility::Inherited,
                ChildOf(joint),
                probe_lit::ProbeLit::new(own.clone(), Vec3::ZERO),
            ))
            .with_children(|m| {
                for ((mesh, _), material) in parts.own.iter().zip(own) {
                    m.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material)));
                }
                if let Some(t) = parts.muzzle {
                    m.spawn((t, view_model::HeldMuzzle { owner: character }));
                }
            })
            .id();
        // Bone-merged parts on their own joints, despawned with the rest.
        let mut merged = Vec::new();
        for (bone, list) in &parts.merged {
            let Some(at) = joint_of(bone) else { continue };
            let lit = instance(list);
            let part = commands
                .spawn((
                    Transform::default(),
                    Visibility::Inherited,
                    ChildOf(at),
                    probe_lit::ProbeLit::new(lit.clone(), Vec3::ZERO),
                ))
                .with_children(|m| {
                    for ((mesh, _), material) in list.iter().zip(lit) {
                        m.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material)));
                    }
                })
                .id();
            merged.push(part);
        }
        body.held_parts = merged;
        body.held = Some((model, key.clone()));
    }
}

/// Game systems that decide what bodies play run in this set, before
/// the joints are posed.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct DriveAnimation;

/// Bodies no camera saw last frame are posed every this many frames
/// (staggered by entity), not every frame.
pub const UNSEEN_POSE_INTERVAL: u32 = 4;

/// Pose each animated body's joints from its character's `Animator`.
///
/// Only bodies some camera drew last frame (`ViewVisibility` of their
/// meshes) are posed every frame; the rest (bots out of view, the hidden
/// local body) every `UNSEEN_POSE_INTERVAL` frames, so their joints (and
/// what hangs on them: the held weapon and its muzzle) are never far
/// behind. Hitboxes don't read the joints (they pose from
/// `ragdoll::SkeletonPose`, every tick). Bodies aren't culled by their
/// joints (their bounds are the mesh's, at the body), so posing late
/// doesn't hide one. Without a camera (headless) every body is posed.
pub(crate) fn pose_bodies(
    time: Res<Time>,
    characters: Query<(&anim::Animator, &Children)>,
    bodies: Query<(Entity, &CharacterBody, Option<&Children>)>,
    seen: Query<&ViewVisibility, With<Mesh3d>>,
    cameras: Query<(), With<Camera>>,
    mut frame: Local<u32>,
    mut joints: Query<&mut Transform, With<BodyJoint>>,
) {
    let now = time.elapsed_secs_f64();
    *frame = frame.wrapping_add(1);
    let culling = !cameras.is_empty();
    for (animator, children) in &characters {
        let Some((body_entity, body, parts)) = children.iter().find_map(|c| bodies.get(c).ok()) else {
            continue;
        };
        if animator.main.is_none() {
            continue;
        }
        if culling
            && !body_seen(parts, &seen)
            && !frame
                .wrapping_add(body_entity.index_u32())
                .is_multiple_of(UNSEEN_POSE_INTERVAL)
        {
            continue;
        }
        let pose = animator.pose(now);
        for (joint, (q, p)) in body.joints.iter().zip(pose) {
            if let Ok(mut t) = joints.get_mut(*joint)
                && (t.rotation != q || t.translation != p)
            {
                t.rotation = q;
                t.translation = p;
            }
        }
    }
}

/// Whether any of a body's meshes was drawn by some camera last frame.
pub(crate) fn body_seen(parts: Option<&Children>, seen: &Query<&ViewVisibility, With<Mesh3d>>) -> bool {
    parts.into_iter().flatten().any(|p| seen.get(*p).is_ok_and(|v| v.get()))
}

/// Characters get their team's hitboxes from the loaded character models.
/// Characters also get the model's animations (an `Animator`, driven by
/// the game).
pub(crate) fn attach_hitboxes(
    models: Option<Res<CharacterModels>>,
    characters: Query<
        (
            Entity,
            Option<&Team>,
            Option<&BodyName>,
            Option<&crate::core::PlayerClass>,
            Option<&BodyModel>,
        ),
        With<crate::core::Intent>,
    >,
    mut commands: Commands,
) {
    let Some(models) = models else { return };
    for (e, team, name, class, current) in &characters {
        let Some((index, m)) = models.for_character(team.copied(), name, class) else {
            continue;
        };
        if current == Some(&BodyModel(index)) {
            continue;
        }
        let mut c = commands.entity(e);
        c.insert((BodyModel(index), crate::core::Hitboxes(m.hitboxes.clone())));
        match &m.animations {
            Some(set) => c.insert(anim::Animator::new(set.clone())),
            None => c.remove::<anim::Animator>(),
        };
    }
}

/// Hitboxes follow the animated skeleton (as the server places them),
/// except where a network server animates the body in the fixed tick
/// (`SimAnimator`).
#[allow(clippy::type_complexity)]
pub(crate) fn pose_hitboxes(
    models: Option<Res<CharacterModels>>,
    mut characters: Query<
        (
            &anim::Animator,
            &BodyModel,
            &crate::core::Intent,
            &ragdoll::SkeletonPose,
            &mut crate::core::Hitboxes,
        ),
        Without<SimAnimator>,
    >,
) {
    let Some(models) = models else { return };
    for (animator, model, intent, pose, mut hitboxes) in &mut characters {
        let Some(m) = models.0.get(model.0) else { continue };
        if animator.main.is_none() || m.boxes.len() != hitboxes.0.len() {
            continue;
        }
        // Bones in the skeleton's space (this tick's pose), then to the
        // character's frame (which the trace turns by the look yaw; the
        // body turns by its own yaw).
        let Some(frame) = pose.frames.back() else { continue };
        let global = &frame.bones;
        let turn = Quat::from_rotation_y(animator.yaw.unwrap_or(intent.yaw) - intent.yaw) * m.root.rotation;
        let scale = m.root.scale.x;
        for (h, b) in hitboxes.0.iter_mut().zip(&m.boxes) {
            let Some((q, p)) = global.get(b.bone) else { continue };
            h.center = turn * (*p + *q * b.center) * scale;
            h.half = b.half * scale;
            h.rotation = turn * *q;
        }
    }
}

/// A network server's own animation of a character's body, driven in the
/// fixed tick from simulation values (games drive it before `SimPose`,
/// e.g. CS:S's player animation state): the character's hitboxes are
/// posed from it after every tick (`pose_sim_hitboxes`), not from the
/// drawn animation (which runs at the frame rate and eases the look), so
/// what a tick traces and what lag compensation keeps is the tick's own
/// pose (docs/plans/active/multiplayer.md, slice 4).
#[derive(Component, Clone, Debug)]
pub struct SimAnimator(pub anim::Animator);

/// After each tick (`FixedLast`): hitboxes posed from `SimAnimator`s.
/// Games drive the animators before it; lag compensation records after.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SimPose;

/// Hitboxes from the server's own animation of each body, at the tick's
/// time (as `pose_hitboxes` does from the drawn one).
pub(crate) fn pose_sim_hitboxes(
    models: Option<Res<CharacterModels>>,
    clock: Res<crate::core::SimClock>,
    mut characters: Query<(
        &SimAnimator,
        &BodyModel,
        &crate::core::Intent,
        &mut crate::core::Hitboxes,
    )>,
) {
    let Some(models) = models else { return };
    for (animator, model, intent, mut hitboxes) in &mut characters {
        let animator = &animator.0;
        let Some(m) = models.0.get(model.0) else { continue };
        if animator.main.is_none() || m.boxes.len() != hitboxes.0.len() {
            continue;
        }
        let global = ragdoll::globals(m, &animator.pose(clock.now));
        let turn = Quat::from_rotation_y(animator.yaw.unwrap_or(intent.yaw) - intent.yaw) * m.root.rotation;
        let scale = m.root.scale.x;
        for (h, b) in hitboxes.0.iter_mut().zip(&m.boxes) {
            let Some((q, p)) = global.get(b.bone) else { continue };
            h.center = turn * (*p + *q * b.center) * scale;
            h.half = b.half * scale;
            h.rotation = turn * *q;
        }
    }
}

/// Render assets of a skinned model: meshes with joint attributes (when
/// it has a skeleton), materials, and inverse bind poses from the bones'
/// reference pose under `root`.
pub(crate) fn body_assets(
    model: &MapModel,
    bones: &[MapBone],
    root: Transform,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<PropMaterial>,
    bindposes: &mut Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>,
    material: &dyn Fn(&MapMesh) -> PropMaterial,
) -> BodyAssets {
    let parts = model
        .meshes
        .iter()
        .map(|m| {
            let mut mesh = build_mesh(m, false);
            if m.joints.len() == m.positions.len() && !bones.is_empty() {
                mesh.insert_attribute(
                    Mesh::ATTRIBUTE_JOINT_INDEX,
                    bevy::mesh::VertexAttributeValues::Uint16x4(m.joints.clone()),
                );
                mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, m.joint_weights.clone());
            }
            (meshes.add(mesh), materials.add(material(m)))
        })
        .collect();
    // Each bone's reference pose in body space, inverted.
    let mut global: Vec<Mat4> = Vec::with_capacity(bones.len());
    for b in bones {
        let local = Mat4::from_rotation_translation(b.rotation, b.position);
        let parent = b
            .parent
            .and_then(|p| global.get(p).copied())
            .unwrap_or(root.to_matrix());
        global.push(parent * local);
    }
    let inverse: Vec<Mat4> = global.iter().map(|m| m.inverse()).collect();
    BodyAssets {
        parts,
        bindposes: bindposes.add(bevy::mesh::skinning::SkinnedMeshInverseBindposes::from(inverse)),
        bones: bones.to_vec(),
        root,
    }
}

#[cfg(test)]
mod pose_tests {
    use super::*;
    use std::sync::Arc;

    /// One bone whose pose is always (identity, (1, 2, 3)).
    fn animator() -> anim::Animator {
        let pose = (Quat::IDENTITY, Vec3::new(1.0, 2.0, 3.0));
        let set = anim::AnimSet {
            defaults: vec![pose],
            bases: vec![vec![pose]],
            animations: vec![anim::Animation {
                frames: 1,
                fps: 30.0,
                ..default()
            }],
            sequences: vec![anim::Sequence {
                grid: (1, 1),
                anims: vec![0],
                looping: true,
                bone_weights: vec![1.0],
                ..default()
            }],
            ..default()
        };
        let mut a = anim::Animator::new(Arc::new(set));
        a.play(0, 0.0);
        a
    }

    /// A character with a one-joint body and one mesh part drawn or not;
    /// returns the joint.
    fn spawn_character(app: &mut App, drawn: bool) -> Entity {
        let world = app.world_mut();
        let character = world.spawn(animator()).id();
        let body = world.spawn(ChildOf(character)).id();
        let joint = world.spawn((BodyJoint(0), Transform::default(), ChildOf(body))).id();
        let seen = if drawn {
            ViewVisibility::VISIBLE
        } else {
            ViewVisibility::HIDDEN
        };
        world.spawn((Mesh3d::default(), seen, ChildOf(body)));
        world.entity_mut(body).insert(CharacterBody {
            model: 0,
            joints: vec![joint],
            held: None,
            held_parts: Vec::new(),
        });
        joint
    }

    /// Frames out of `n` in which the joint was posed.
    fn posed_frames(app: &mut App, joint: Entity, n: u32) -> u32 {
        let mut posed = 0;
        for _ in 0..n {
            app.world_mut().get_mut::<Transform>(joint).unwrap().translation = Vec3::ZERO;
            app.update();
            posed += (app.world().get::<Transform>(joint).unwrap().translation == Vec3::new(1.0, 2.0, 3.0)) as u32;
        }
        posed
    }

    fn app(camera: bool) -> App {
        let mut app = App::new();
        app.init_resource::<Time>().add_systems(Update, pose_bodies);
        if camera {
            app.world_mut().spawn(Camera::default());
        }
        app
    }

    #[test]
    fn unseen_bodies_are_not_posed_every_frame() {
        let mut app = app(true);
        let seen = spawn_character(&mut app, true);
        let unseen = spawn_character(&mut app, false);
        let n = 8 * UNSEEN_POSE_INTERVAL;
        assert_eq!(posed_frames(&mut app, seen, n), n, "a drawn body is posed every frame");
        assert_eq!(
            posed_frames(&mut app, unseen, n),
            n / UNSEEN_POSE_INTERVAL,
            "a body no camera drew is posed every {UNSEEN_POSE_INTERVAL} frames"
        );
    }

    #[test]
    fn without_cameras_every_body_is_posed() {
        let mut app = app(false);
        let unseen = spawn_character(&mut app, false);
        assert_eq!(posed_frames(&mut app, unseen, 8), 8);
    }
}
