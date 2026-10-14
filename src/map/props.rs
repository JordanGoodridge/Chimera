//! Placed props after spawning: their looks (skin, body parts), animated
//! sequences and dynamic shadows, and the components that tag them.

#[allow(unused_imports)]
use super::*;

/// A simulated physics prop: how players interact with it, its mass (kg)
/// and its model-space bounds.
#[derive(Component, Debug, Clone, Copy)]
pub struct PhysicsProp {
    pub push: PushAway,
    pub mass: f32,
    pub bounds: (Vec3, Vec3),
}

/// A prop entity's index in `MapData::props`.
#[derive(Component, Debug, Clone, Copy)]
pub struct PropIndex(pub usize);

/// The map entity a prop node was placed by (index into
/// `MapData::entities`), so logic can find it (a sound playing from it).
#[derive(Component, Debug, Clone, Copy)]
pub struct PropEntity(pub usize);

/// Where an entity prop was placed (its spawn transform), so a round
/// restart can put a moved physics prop back.
#[derive(Component, Debug, Clone, Copy)]
pub struct PropHome(pub Transform);

/// What it takes to redraw prop shadows when props move (built by
/// `shadows::apply_shadow_detail`): render-to-texture, or blobs (`blob`:
/// one picture, only the meshes move).
#[derive(Resource)]
pub(crate) struct ShadowState {
    pub(crate) data: Arc<MapData>,
    pub(crate) settings: MapShadows,
    pub(crate) receivers: Arc<shadows::Receivers>,
    pub(crate) blob: bool,
    pub(crate) atlas: shadows::Atlas,
    pub(crate) atlas_image: Handle<Image>,
    pub(crate) material: Handle<shadows::ShadowMaterial>,
    pub(crate) cells: std::collections::HashMap<usize, shadows::Cell>,
    pub(crate) entities: std::collections::HashMap<usize, Entity>,
    pub(crate) root: Entity,
    /// Where each caster was when its shadow was last drawn.
    pub(crate) built: std::collections::HashMap<usize, (Vec3, Quat)>,
}

/// Redraw the shadows of physics props that moved (silhouette and mesh);
/// a prop the logic hid (broken, killed) hides its shadow until it shows
/// again (a round restart). Props that only jitter in place (resting
/// physics props that never fall asleep: seven on cs_office, every frame)
/// keep their shadow until they have moved by a quarter of a shadow texel
/// (`shadows::moved_visibly`), and a redrawn cell updates only its part of
/// the atlas image: with seven props rebuilt and the whole atlas converted
/// every frame this cost 16 ms a frame there.
#[allow(clippy::type_complexity)]
pub(crate) fn update_prop_shadows(
    mut commands: Commands,
    state: Option<ResMut<ShadowState>>,
    moved: Query<(&PropIndex, &Transform), (With<PhysicsProp>, Changed<Transform>)>,
    hidden: Query<&PropIndex, Added<LogicHidden>>,
    mut shown: RemovedComponents<LogicHidden>,
    props: Query<&PropIndex, Without<LogicHidden>>,
    mut shadow_vis: Query<Option<&mut VisClusters>, With<PropShadow>>,
    vis: Option<Res<vis::ActiveVisibility>>,
    shadow_meshes: Query<&Mesh3d, With<PropShadow>>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(mut state) = state else {
        shown.clear();
        return;
    };
    let state = &mut *state;
    for index in &hidden {
        if let Some(e) = state.entities.get(&index.0) {
            commands.entity(*e).insert((LogicHidden, Visibility::Hidden));
        }
    }
    for node in shown.read() {
        let Ok(index) = props.get(node) else { continue };
        if let Some(e) = state.entities.get(&index.0).copied() {
            let on = shadow_vis.get(e).ok().flatten().is_none_or(|v| v.potentially_visible);
            commands.entity(e).remove::<LogicHidden>().insert(if on {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
    }
    let mut redrawn: Vec<shadows::Cell> = Vec::new();
    for (index, t) in &moved {
        let Some(cell) = state.cells.get(&index.0).copied() else {
            continue;
        };
        let model = &state.data.models[state.data.props[index.0].model];
        if let Some(&from) = state.built.get(&index.0)
            && !shadows::moved_visibly(model.bounds, cell.size, from, (t.translation, t.rotation))
        {
            continue;
        }
        state.built.insert(index.0, (t.translation, t.rotation));
        let mesh = if state.blob {
            let frame = shadows::ShadowFrame::blob(
                model.bounds,
                t.translation,
                t.rotation,
                shadows::DOWN,
                state.settings.distance,
            );
            shadows::shadow_mesh(&frame, &state.receivers, shadows::BLOB_RECT)
        } else {
            redrawn.push(cell);
            shadows::rebuild(
                &state.data,
                &state.settings,
                &state.receivers,
                &mut state.atlas,
                &cell,
                t.translation,
                t.rotation,
            )
        };
        match (mesh, state.entities.get(&index.0).copied()) {
            (Some(mesh), Some(e)) => match shadow_meshes.get(e) {
                // The same mesh asset, rewritten: a new handle every
                // frame left the shadow undrawn while its prop moved
                // (Bevy re-specializes an entity whose `Mesh3d` changed
                // and skipped it while the new mesh wasn't prepared yet).
                // (The handle keeps the asset alive: the id stays valid.)
                Ok(handle) => {
                    // Hidden where the camera can't see where it now lies.
                    if let Some(v) = vis.as_ref()
                        && let Some(aabb) = bevy::camera::primitives::MeshAabb::compute_aabb(&mesh)
                        && let Ok(Some(mut tag)) = shadow_vis.get_mut(e)
                    {
                        let clusters = vis::box_clusters(&v.0, Vec3::from(aabb.min()), Vec3::from(aabb.max()));
                        if !clusters.is_empty() && *tag.clusters != clusters[..] {
                            tag.clusters = clusters.into_boxed_slice();
                        }
                    }
                    let _ = meshes.insert(handle.id(), mesh);
                }
                Err(_) => {
                    commands.entity(e).insert(Mesh3d(meshes.add(mesh)));
                }
            },
            (Some(mesh), None) => {
                let e = commands
                    .spawn((
                        Name::new(format!("Shadow of prop {}", index.0)),
                        MapPart,
                        PropShadow { prop: index.0 },
                        Mesh3d(meshes.add(mesh)),
                        MeshMaterial3d(state.material.clone()),
                        bevy::light::NotShadowCaster,
                        Transform::default(),
                        ChildOf(state.root),
                    ))
                    .id();
                state.entities.insert(index.0, e);
            }
            (None, Some(e)) => {
                commands.entity(e).despawn();
                state.entities.remove(&index.0);
            }
            (None, None) => {}
        }
    }
    if !redrawn.is_empty()
        && let Some(mut image) = images.get_mut(&state.atlas_image)
    {
        match image.data.as_mut() {
            Some(bytes) => {
                for cell in &redrawn {
                    state.atlas.write_bytes(bytes, cell.at, cell.size);
                }
            }
            None => image.data = Some(state.atlas.bytes()),
        }
    }
}

/// A prop's dynamic shadow on the world (index into `MapData::props`).
#[derive(Component, Debug)]
pub struct PropShadow {
    pub prop: usize,
}

/// A prop model's mesh (index into `MapModel::meshes`) drawn under the
/// prop's node.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct PropMeshSlot {
    pub(crate) mesh: usize,
}

/// What a prop entity can switch to: its materials per skin family
/// (empty with one family), its body layout, and what it shows (skin,
/// body number).
#[derive(Component)]
pub(crate) struct PropVariants {
    pub(crate) skins: Vec<Vec<Handle<PropMaterial>>>,
    /// The body number it spawned with.
    pub(crate) body: i32,
    pub(crate) parts: Vec<u16>,
    pub(crate) mesh_bodies: Vec<Option<(u16, u16)>>,
    pub(crate) shown: (i32, i32),
}

/// An animated prop: what it plays, its joints by bone, the logic's last
/// word and the sequence it falls back to.
#[derive(Component)]
pub(crate) struct PropRig {
    pub(crate) animator: anim::Animator,
    pub(crate) joints: Vec<Entity>,
    pub(crate) seen: Option<PropSequence>,
    pub(crate) default: Option<usize>,
}

/// Inverse bind poses of a rig's bones in the reference pose under its
/// root.
pub(crate) fn rig_inverse_bindposes(rig: &MapRig) -> bevy::mesh::skinning::SkinnedMeshInverseBindposes {
    let mut global: Vec<Mat4> = Vec::with_capacity(rig.bones.len());
    for b in &rig.bones {
        let local = Mat4::from_rotation_translation(b.rotation, b.position);
        let parent = b
            .parent
            .and_then(|p| global.get(p).copied())
            .unwrap_or(rig.root.to_matrix());
        global.push(parent * local);
    }
    let inverse: Vec<Mat4> = global.iter().map(|m| m.inverse()).collect();
    bevy::mesh::skinning::SkinnedMeshInverseBindposes::from(inverse)
}

/// Prop entities show the skin and body group the logic gives them
/// (`PropLook`). A skin the model doesn't have leaves it as it is.
pub(crate) fn apply_prop_looks(
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

/// Animated props play what the logic asks (`PropSequence`,
/// specs/source/prop_damage.md 9): a new serial restarts the named
/// sequence (sequence 0 when the model lacks it), switching at once; a
/// new round starts the default one (else the first); a non-looping
/// sequence that ends starts the default again, if there is one.
pub(crate) fn animate_props(
    time: Res<Time>,
    mut props: Query<(&mut PropRig, Option<&PropSequence>)>,
    mut joints: Query<&mut Transform>,
) {
    let (now, dt) = (time.elapsed_secs_f64(), time.delta_secs());
    for (mut rig, seq) in &mut props {
        let rig = &mut *rig;
        if let Some(seq) = seq
            && rig.seen.as_ref() != Some(seq)
        {
            let old = rig.seen.replace(seq.clone());
            rig.default = Some(seq.default.as_str())
                .filter(|d| !d.is_empty())
                .and_then(|d| rig.animator.set.sequence(d));
            let new_round = old.as_ref().is_none_or(|o| o.round != seq.round);
            let old_serial = old.as_ref().and_then(|o| o.play.as_ref()).map(|p| p.1);
            match &seq.play {
                Some((name, serial)) if new_round || old_serial != Some(*serial) => {
                    let s = rig.animator.set.sequence(name).unwrap_or_else(|| {
                        warn!("prop: no sequence {name}");
                        0
                    });
                    rig.animator.restart(s, now);
                    rig.animator.fading.clear();
                }
                // No default: its first sequence (the model's idle).
                None if new_round => {
                    rig.animator.restart(rig.default.unwrap_or(0), now);
                    rig.animator.fading.clear();
                }
                _ => {}
            }
        }
        if rig.animator.main.is_none() {
            continue;
        }
        rig.animator.advance(dt, now);
        // A non-looping default restarts too, forever.
        if rig.animator.finished()
            && let Some(d) = rig.default
        {
            rig.animator.restart(d, now);
            rig.animator.fading.clear();
        }
        let pose = rig.animator.pose(now);
        for (joint, (q, p)) in rig.joints.iter().zip(pose) {
            if let Ok(mut t) = joints.get_mut(*joint) {
                t.rotation = q;
                t.translation = p;
            }
        }
    }
}

/// Sprites and dust volumes follow their entity (`EntityPart`): a sprite
/// is drawn while on; a dust volume while it exists (off, it only stops
/// spawning motes).
#[allow(clippy::type_complexity)]
pub(crate) fn switch_parts(
    parts: Query<
        (
            Entity,
            &EntityPart,
            Option<&VisClusters>,
            Has<LogicHidden>,
            Has<dust::DustEmitter>,
            Has<steam::SteamEmitter>,
        ),
        Changed<EntityPart>,
    >,
    mut commands: Commands,
) {
    for (e, part, clusters, hidden, dust, steam) in &parts {
        // Dust and steam stay drawn while off: what they made lives on.
        let dust = dust || steam;
        let shown = part.exists && (dust || part.on);
        if shown != hidden {
            continue;
        }
        if shown {
            let mut c = commands.entity(e);
            c.remove::<LogicHidden>();
            if clusters.is_none_or(|c| c.potentially_visible) {
                c.insert(Visibility::Inherited);
            }
        } else {
            commands.entity(e).insert((LogicHidden, Visibility::Hidden));
        }
    }
}

/// A prop's physics collider (sprite glows see through static props, as
/// the game's line test ignores them).
#[derive(Component)]
pub struct MapPropCollider;

#[cfg(test)]
mod switch_tests {
    use super::*;

    #[test]
    fn body_numbers_pick_one_choice_per_part() {
        // Parts with 1, 3 and 2 choices: body = c0 + 1 * c1 + 3 * c2.
        let parts = [1, 3, 2];
        assert_eq!(body_choice(&parts, 5, 1), 2);
        assert_eq!(body_choice(&parts, 5, 2), 1);
    }

    #[test]
    fn switched_parts_hide_and_show() {
        let mut app = App::new();
        app.add_systems(Update, switch_parts);
        let sprite = app
            .world_mut()
            .spawn((
                EntityPart {
                    entity: 3,
                    on: true,
                    exists: true,
                },
                Visibility::Inherited,
            ))
            .id();
        app.update();
        assert!(!app.world().entity(sprite).contains::<LogicHidden>());
        app.world_mut().get_mut::<EntityPart>(sprite).unwrap().on = false;
        app.update();
        assert!(app.world().entity(sprite).contains::<LogicHidden>());
        assert_eq!(app.world().get::<Visibility>(sprite), Some(&Visibility::Hidden));
        app.world_mut().get_mut::<EntityPart>(sprite).unwrap().on = true;
        app.update();
        assert!(!app.world().entity(sprite).contains::<LogicHidden>());
        assert_eq!(app.world().get::<Visibility>(sprite), Some(&Visibility::Inherited));
    }

    #[test]
    fn light_styles_add_and_remove_their_share() {
        let l = MapLightmap {
            width: 2,
            height: 1,
            rgb: vec![[1.0; 3], [0.5; 3]],
            bumped: None,
            styles: vec![
                MapLightStyle {
                    style: 32,
                    on: true,
                    texels: vec![0],
                    rgb: vec![[0.75; 3]],
                    ..default()
                },
                MapLightStyle {
                    style: 33,
                    on: false,
                    texels: vec![1],
                    rgb: vec![[0.25; 3]],
                    ..default()
                },
            ],
        };
        let lit = |on: u8| move |s: &MapLightStyle| if s.style == on { 1.0 } else { 0.0 };
        assert_eq!(l.relit(&lit(32)).0, l.rgb);
        assert_eq!(l.relit(&lit(33)).0, vec![[0.25; 3], [0.75; 3]]);
    }
}
