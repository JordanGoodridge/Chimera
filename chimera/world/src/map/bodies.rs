//! Character bodies, the simulation's side: which model a character uses
//! (its team's, its class's or the one `BodyName` names), what it holds
//! (`Held`), its animations and its hitboxes following the skeleton (also
//! headless, for the server's hit tests). The drawn body, held model and
//! their posing are `render::bodies`, which draws what these say.
//!
//! `CharacterModels`, `BodyModel`, `BodyName`, `BodySkin`, `Held`,
//! `ShowLocalBody`;
//! hitboxes on the skeleton (also headless), `SimAnimator`/`SimPose`.
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
//! poses (games/cs_source/tests/it/heavy/map_net_weapons.rs).
//!
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

/// Which skin family a character's body shows (Source `skin`: a map's
/// hostage keeps its own); without it, the model's first.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BodySkin(pub i32);

/// Whether the local player's own body is drawn (third person). The
/// client sets it; the body is there either way, animated, and hidden
/// when this is false.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShowLocalBody(pub bool);

/// Game systems that decide what bodies play run in this set, before
/// the joints are posed.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct DriveAnimation;

/// Characters get their team's hitboxes from the loaded character models.
/// Characters also get the model's animations (an `Animator`, driven by
/// the game).
pub fn attach_hitboxes(
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
pub fn pose_hitboxes(
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
pub fn pose_sim_hitboxes(
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
