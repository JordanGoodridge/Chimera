//! Copies of map nodes made at run time (a point_template's brush and
//! prop members spawned again: specs/source/viewcontrol_and_templates.md
//! 2.2): the node and everything under it cloned, without the physics
//! engine's per-body bookkeeping (island and tree keys: the copy gets its
//! own when its body and colliders are added), visible wherever it is
//! (the original's visibility clusters don't apply), and not replicated (a
//! network client doesn't see copies yet). `MapCopy` marks them so a round
//! restart removes them. The layers above finish a copy through
//! `MapHooks::copied` (render draws it by its own meshes, never merged).

use avian3d::collider_tree::ColliderTreeProxyKey;
use avian3d::dynamics::solver::islands::BodyIslandNode;
use bevy::prelude::*;

use super::{MapHooks, VisClusters};
use super::vis::{FadeDistance, Occludee};

/// A node copied at run time.
#[derive(Component, Clone, Copy, Debug)]
pub struct MapCopy;

/// Copy `node` (and its children) at `pose`; returns the copy.
pub fn copy_node(world: &mut World, node: Entity, pose: Transform) -> Option<Entity> {
    world.get_entity(node).ok()?;
    let copy = world.entity_mut(node).clone_and_spawn_with_opt_out(|b| {
        b.linked_cloning(true)
            // A body's collider list is a linked relationship that holds
            // the body itself when the body has its own collider (a
            // func_physbox): followed, cloning never ends. The colliders
            // under it are cloned as children and join the copy's body.
            .deny::<(BodyIslandNode, ColliderTreeProxyKey, avian3d::prelude::RigidBodyColliders)>()
            .deny::<(VisClusters, Occludee, FadeDistance)>()
            .deny::<bevy_replicon::prelude::Replicated>();
    });
    let mut e = world.entity_mut(copy);
    e.insert((MapCopy, pose, Visibility::Inherited));
    if let Some(mut p) = e.get_mut::<avian3d::prelude::Position>() {
        p.0 = pose.translation;
    }
    if let Some(mut r) = e.get_mut::<avian3d::prelude::Rotation>() {
        *r = avian3d::prelude::Rotation::from(pose.rotation);
    }
    let hooks = world.get_resource::<MapHooks>().map(|h| h.copied.clone()).unwrap_or_default();
    for hook in hooks {
        hook(world, copy);
    }
    Some(copy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use avian3d::prelude::{ColliderOf, RigidBody, RigidBodyColliders};

    #[test]
    fn a_body_with_its_own_collider_copies_once() {
        // mg_creative_multigames_v8_ns's boats: a func_physbox is a body
        // that is its own collider, so its collider list holds itself;
        // copying one hung the game (cloning without end).
        let mut world = World::new();
        let body = world.spawn((RigidBody::Dynamic, Transform::default())).id();
        world.entity_mut(body).insert(ColliderOf { body });
        let child = world.spawn((Transform::default(), ChildOf(body))).id();
        world.entity_mut(child).insert(ColliderOf { body });
        assert_eq!(world.get::<RigidBodyColliders>(body).map(|c| c.len()), Some(2));
        let copy = copy_node(&mut world, body, Transform::from_xyz(1.0, 0.0, 0.0)).unwrap();
        assert_ne!(copy, body);
        assert!(world.get::<MapCopy>(copy).is_some());
        assert_eq!(world.get::<Children>(copy).map(|c| c.len()), Some(1), "its child copied once");
    }
}
