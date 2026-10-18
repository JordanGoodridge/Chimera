//! A guard for avian 0.7's contact bookkeeping: the narrow phase leaves a
//! contact pair's "started generating constraints" flag set when the pair
//! switches constraints on in a step where it isn't steadily touching.
//!
//! avian sets `ContactPairFlags::STARTED_GENERATING_CONSTRAINTS` when a
//! pair that made no constraints (created while a body was disabled, from a
//! collider proxy still marked "body disabled" after `RigidBodyDisabled`
//! went away, or a collider that had no body) now should. Its status pass
//! clears the flag only in the branch for a pair that was and still is
//! touching. In the step the pair starts touching it takes the "collision
//! started" branch instead (which adds the manifolds to the constraint graph
//! and the contact to an island) and the flag stays; it stays too while the
//! pair isn't touching. The next step whose manifold count changes then
//! takes the "started generating" branch: every manifold is added to the
//! constraint graph a second time (a handle past the pair's manifolds:
//! "index out of bounds" in avian's `prepare_contact_constraints`) and the
//! contact is linked into an island again (`contact.island.is_none()`
//! assertion). Both panicked our soak runs, at random, because which step a
//! pair starts touching in depends on timing.
//!
//! Any flag still set after the narrow phase is such a leftover: the pair's
//! constraints were already added (it started touching), or will be when it
//! starts touching. Clearing it there is what avian means to do.
//!
//! A soak run under load still panicked the same way once with only that,
//! on de_dust2 (a handle one past a pair's 7514 manifolds): some other path
//! leaves a pair's constraint handles out of step with its manifolds, not
//! found yet. So after clearing the flags, each pair's handles are matched
//! to what avian's status pass would have left (one per manifold for an
//! awake pair that touches and makes constraints, none otherwise), dropping
//! or adding the difference, with a warning naming the pair, its colliders
//! and bodies (the first few) to find that path by.
//!
//! Known shortcuts (docs/tech-debt.md): a workaround for avian 0.7.0's
//! contact bookkeeping, reaching into its contact pair flags and constraint
//! graph. Why it's OK for now: one system after the narrow phase;
//! `avian_still_needs_the_guard` fails once avian clears the flag itself,
//! and the warning says when the other path happens.

use avian3d::{
    collision::contact_types::{ContactId, ContactPairFlags},
    dynamics::solver::constraint_graph::ConstraintGraph,
    prelude::*,
};
use bevy::prelude::*;

/// Clear leftover "started generating constraints" flags and match each
/// pair's constraint handles to its manifolds after the narrow phase (see
/// the module docs).
pub fn plugin(app: &mut App) {
    app.add_systems(
        PhysicsSchedule,
        (clear_leftover_started_flags, match_constraint_handles)
            .chain()
            .after(PhysicsStepSystems::NarrowPhase)
            .before(PhysicsStepSystems::Solver)
            .ambiguous_with_all(),
    );
}

fn clear_leftover_started_flags(mut contacts: ResMut<ContactGraph>) {
    for pair in contacts.bypass_change_detection().active_pairs_mut() {
        if pair.flags.contains(ContactPairFlags::STARTED_GENERATING_CONSTRAINTS) {
            pair.flags.remove(ContactPairFlags::STARTED_GENERATING_CONSTRAINTS);
        }
    }
}

/// How many constraint handles `pair` should have after the narrow phase.
fn wanted_handles(pair: &ContactPair, awake: bool) -> usize {
    let makes = pair.generates_constraints() && pair.is_touching() && pair.body1.is_some() && pair.body2.is_some();
    if awake && makes { pair.manifolds.len() } else { 0 }
}

fn match_constraint_handles(
    mut contacts: ResMut<ContactGraph>,
    mut graph: ResMut<ConstraintGraph>,
    bodies: Query<(Option<&RigidBody>, Has<RigidBodyDisabled>, Has<Sleeping>)>,
    colliders: Query<(Option<&ColliderOf>, Has<Sensor>, Has<ColliderDisabled>)>,
    mut warned: Local<u32>,
) {
    let contacts = contacts.bypass_change_detection();
    let mut off: Vec<(ContactId, usize, usize)> = Vec::new();
    let pairs = contacts.active_pairs().iter().map(|p| (p, true));
    for (pair, awake) in pairs.chain(contacts.sleeping_pairs().iter().map(|p| (p, false))) {
        let want = wanted_handles(pair, awake);
        if let Some(edge) = contacts.get_edge_by_id(pair.contact_id)
            && edge.constraint_handles.len() != want
        {
            off.push((pair.contact_id, edge.constraint_handles.len(), want));
        }
    }
    for (id, have, want) in off {
        let Some((_, pair)) = contacts.get_by_id(id) else { continue };
        if *warned < 20 {
            *warned += 1;
            let body = |b: Option<Entity>| b.map(|b| (b, bodies.get(b).ok().map(|(r, d, s)| (r.copied(), d, s))));
            let collider = |c: Entity| (c, colliders.get(c).ok().map(|(o, s, d)| (o.map(|o| o.body), s, d)));
            warn!(
                "avian contact {id:?}: {have} constraint handles for {want} (manifolds {}, generates {}, \
                 touching {}, flags {:?}); colliders (entity, (body, sensor, disabled)) {:?} {:?}, \
                 bodies (entity, (type, disabled, sleeping)) {:?} {:?}",
                pair.manifolds.len(),
                pair.generates_constraints(),
                pair.is_touching(),
                pair.flags,
                collider(pair.collider1),
                collider(pair.collider2),
                body(pair.body1),
                body(pair.body2),
            );
        }
        let (Some(b1), Some(b2)) = (pair.body1, pair.body2) else { continue };
        if have > want {
            for _ in want..have {
                graph.pop_manifold(&mut contacts.edges, id, b1, b2);
            }
        } else if let Some((edge, pair)) = contacts.get_mut_by_id(id) {
            for _ in have..want {
                graph.push_manifold(edge, pair);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// What's wrong with avian's contact bookkeeping now: a constraint
    /// handle past its pair's manifolds, or a touching pair making
    /// constraints with another number of handles than manifolds.
    fn bookkeeping_errors(world: &World) -> Vec<String> {
        let contacts = world.resource::<ContactGraph>();
        let mut errors = Vec::new();
        for color in &world.resource::<ConstraintGraph>().colors {
            for h in &color.manifold_handles {
                match contacts.get_by_id(h.contact_id) {
                    None => errors.push(format!("handle to a removed pair: {h:?}")),
                    Some((_, pair)) if h.manifold_index >= pair.manifolds.len() => errors.push(format!(
                        "handle {h:?} past the pair's {} manifolds",
                        pair.manifolds.len()
                    )),
                    _ => {}
                }
            }
        }
        for pair in contacts.iter_active_touching().filter(|p| p.generates_constraints()) {
            let (edge, _) = contacts.get_by_id(pair.contact_id).unwrap();
            if edge.constraint_handles.len() != pair.manifolds.len() {
                errors.push(format!(
                    "pair {:?}: {} handles, {} manifolds",
                    pair.contact_id,
                    edge.constraint_handles.len(),
                    pair.manifolds.len()
                ));
            }
        }
        errors
    }

    /// A crate re-enabled above a floor of two slabs falls onto one (its
    /// collider's proxy still says "body disabled": its new pair with the
    /// floor starts out making no constraints and switches on before it
    /// touches), then is moved to span both slabs (one manifold to two).
    /// Returns the first bookkeeping error, or avian's panic.
    fn reenabled_crate_spans_two_slabs(guard: bool) -> Result<(), String> {
        let run = move || {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                TransformPlugin,
                AssetPlugin::default(),
                bevy::mesh::MeshPlugin,
                PhysicsPlugins::default().build().disable::<PhysicsInterpolationPlugin>(),
            ))
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
                1.0 / 64.0,
            )));
            if guard {
                plugin(&mut app);
            }
            app.finish();
            app.cleanup();
            app.update();
            let slab = |x: f32| (Vec3::new(x, -0.5, 0.0), Quat::IDENTITY, Collider::cuboid(2.0, 1.0, 4.0));
            app.world_mut().spawn((
                RigidBody::Static,
                Collider::compound(vec![slab(-1.5), slab(1.5)]),
                Transform::default(),
            ));
            let crate_ = app
                .world_mut()
                .spawn((
                    RigidBody::Dynamic,
                    Collider::cuboid(1.0, 1.0, 1.0),
                    SleepingDisabled,
                    RigidBodyDisabled,
                    Transform::from_xyz(-1.5, 2.0, 0.0),
                ))
                .id();
            let step = |app: &mut App, ticks: usize| -> Result<(), String> {
                for _ in 0..ticks {
                    app.update();
                    if let Some(e) = bookkeeping_errors(app.world()).into_iter().next() {
                        return Err(e);
                    }
                }
                Ok(())
            };
            let floor_manifolds = |app: &App| {
                let contacts = app.world().resource::<ContactGraph>();
                contacts.iter_active_touching().map(|p| p.manifolds.len()).sum::<usize>()
            };
            step(&mut app, 5)?;
            app.world_mut().entity_mut(crate_).remove::<RigidBodyDisabled>();
            step(&mut app, 60)?;
            assert_eq!(floor_manifolds(&app), 1, "resting on one slab");
            // Over the gap, on both slabs.
            app.world_mut().get_mut::<Position>(crate_).unwrap().0 = Vec3::new(0.0, 0.5, 0.0);
            step(&mut app, 5)?;
            assert_eq!(floor_manifolds(&app), 2, "resting on both slabs");
            Ok(())
        };
        std::panic::catch_unwind(run).unwrap_or_else(|e| {
            Err(e
                .downcast_ref::<String>()
                .cloned()
                .or(e.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default())
        })
    }

    #[test]
    fn a_pair_switching_constraints_on_as_it_touches_adds_them_once() {
        assert_eq!(reenabled_crate_spans_two_slabs(true), Ok(()));
    }

    /// The avian bug the guard works around: when this fails, avian clears
    /// the flag itself and the guard (and this test) can go.
    #[test]
    fn avian_still_needs_the_guard() {
        let without = reenabled_crate_spans_two_slabs(false);
        assert!(
            without.is_err(),
            "avian adds the constraints once without the guard now: drop `contact_flags`"
        );
    }

    /// A pair given a constraint handle past its manifolds (as the soak's
    /// unknown path left one) is put right before the solver reads it.
    #[test]
    fn a_stray_constraint_handle_is_dropped_before_the_solver() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            TransformPlugin,
            AssetPlugin::default(),
            bevy::mesh::MeshPlugin,
            PhysicsPlugins::default().build().disable::<PhysicsInterpolationPlugin>(),
        ))
        .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 64.0)));
        plugin(&mut app);
        app.finish();
        app.cleanup();
        app.update();
        app.world_mut()
            .spawn((RigidBody::Static, Collider::cuboid(4.0, 1.0, 4.0), Transform::from_xyz(0.0, -0.5, 0.0)));
        app.world_mut().spawn((
            RigidBody::Dynamic,
            Collider::cuboid(1.0, 1.0, 1.0),
            SleepingDisabled,
            Transform::from_xyz(0.0, 1.0, 0.0),
        ));
        for _ in 0..60 {
            app.update();
        }
        let world = app.world_mut();
        world.resource_scope(|world, mut graph: Mut<ConstraintGraph>| {
            let mut contacts = world.resource_mut::<ContactGraph>();
            let id = contacts.iter_active_touching().next().expect("the crate rests on the floor").contact_id;
            let (edge, pair) = contacts.get_mut_by_id(id).unwrap();
            graph.push_manifold(edge, pair);
        });
        assert!(!bookkeeping_errors(app.world()).is_empty(), "a stray handle");
        for _ in 0..10 {
            app.update();
            assert_eq!(bookkeeping_errors(app.world()), Vec::<String>::new());
        }
    }
}
