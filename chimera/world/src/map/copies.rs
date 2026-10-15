//! Copies of map nodes made at run time (a point_template's brush and
//! prop members spawned again: specs/source/viewcontrol_and_templates.md
//! 2.2): the node and everything under it cloned, without the physics
//! engine's per-body bookkeeping (island and tree keys: the copy gets its
//! own, its body inserted again after the clone; a body that is its own
//! collider, a func_physbox, keeps its shape and is its colliders' body,
//! awake), visible wherever it is
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
    // A body that is its own collider (a func_physbox): the clone leaves
    // its shape out and its collider naming the source's body (a template
    // member's, taken out of the map and disabled): the copy had no shape
    // (no collision, and the physics flung it away). Its own shape, its own
    // body.
    let shape = world.get::<avian3d::prelude::Collider>(node).cloned();
    let mut e = world.entity_mut(copy);
    e.insert((MapCopy, pose, Visibility::Inherited));
    if let Some(shape) = shape
        && e.contains::<avian3d::prelude::RigidBody>()
    {
        if !e.contains::<avian3d::prelude::Collider>() {
            e.insert(shape);
        }
        e.insert(avian3d::prelude::ColliderOf { body: copy });
    }
    if let Some(mut p) = e.get_mut::<avian3d::prelude::Position>() {
        p.0 = pose.translation;
    }
    if let Some(mut r) = e.get_mut::<avian3d::prelude::Rotation>() {
        *r = avian3d::prelude::Rotation::from(pose.rotation);
    }
    // Its body inserted again: the clone added it before the rest, so
    // avian's insert hooks found nothing to set up and a moving copy (a
    // mover's, a physics brush's) had no solver body or island: a contact
    // between two such bodies panicked avian ("neither body is in an
    // island").
    let physics = world.contains_resource::<avian3d::dynamics::solver::islands::PhysicsIslands>();
    let mut stack: Vec<Entity> = vec![copy];
    while let Some(c) = stack.pop() {
        if let Some(rb) = world.get::<avian3d::prelude::RigidBody>(c).copied() {
            let mut e = world.entity_mut(c);
            e.remove::<avian3d::prelude::Sleeping>().insert(rb);
            if physics
                && !rb.is_static()
                && !e.contains::<avian3d::prelude::RigidBodyDisabled>()
                && !e.contains::<BodyIslandNode>()
            {
                e.insert(BodyIslandNode::default());
            }
        }
        stack.extend(world.get::<Children>(c).map(|c| c.to_vec()).unwrap_or_default());
    }
    world.flush();
    // Colliders under it that still name the source's body join the copy's.
    let mut stack: Vec<Entity> = world.get::<Children>(copy).map(|c| c.to_vec()).unwrap_or_default();
    while let Some(c) = stack.pop() {
        if world.get::<avian3d::prelude::ColliderOf>(c).is_some_and(|o| o.body == node) {
            world.entity_mut(c).insert(avian3d::prelude::ColliderOf { body: copy });
        }
        stack.extend(world.get::<Children>(c).map(|c| c.to_vec()).unwrap_or_default());
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
        let child_copy = world.get::<Children>(copy).unwrap()[0];
        assert_eq!(world.get::<ColliderOf>(child_copy).map(|o| o.body), Some(copy), "the child's collider joins the copy");
        world.entity_mut(body).insert(avian3d::prelude::Collider::sphere(1.0));
        let copy = copy_node(&mut world, body, Transform::default()).unwrap();
        assert_eq!(world.get::<ColliderOf>(copy).map(|o| o.body), Some(copy), "its own collider is its own");
        assert!(world.get::<avian3d::prelude::Collider>(copy).is_some(), "with its shape");
    }
}
