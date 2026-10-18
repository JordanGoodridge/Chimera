//! Placed props after spawning: their animated sequences and the
//! components that tag them (their looks as drawn: `render::props`;
//! their dynamic shadows: `render::shadows`).

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

/// An animated prop: what it plays, its joints by bone, the logic's last
/// word and the sequence it falls back to.
#[derive(Component)]
pub struct PropRig {
    pub animator: anim::Animator,
    pub joints: Vec<Entity>,
    pub seen: Option<PropSequence>,
    pub default: Option<usize>,
}

/// Inverse bind poses of a rig's bones in the reference pose under its
/// root.
pub fn rig_inverse_bindposes(rig: &MapRig) -> bevy::mesh::skinning::SkinnedMeshInverseBindposes {
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

/// Animated props play what the logic asks (`PropSequence`,
/// specs/source/prop_damage.md 9): a new serial restarts the named
/// sequence (sequence 0 when the model lacks it), switching at once; a
/// new round starts the default one (else the first); a non-looping
/// sequence that ends starts the default again, if there is one.
pub fn animate_props(
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

/// A prop's physics collider (sprite glows see through static props, as
/// the game's line test ignores them).
#[derive(Component)]
pub struct MapPropCollider;

/// A map collider light passes (glass, grates: `MapData::see_through_hulls`
/// and `see_through_entities`): solid to players and shots, not to a
/// flash's line (grenades.md Q12).
#[derive(Component)]
pub struct SeeThrough;

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
