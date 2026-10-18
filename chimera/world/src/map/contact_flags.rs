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
//! Known shortcuts (docs/tech-debt.md): a workaround for avian 0.7.0's
//! status pass, reaching into its contact pair flags. Why it's OK for now:
//! one system, and `avian_still_needs_the_guard` fails once avian clears
//! the flag itself; drop the module then.

use avian3d::{collision::contact_types::ContactPairFlags, prelude::*};
use bevy::prelude::*;

/// Clear leftover "started generating constraints" flags after the narrow
/// phase (see the module docs).
pub fn plugin(app: &mut App) {
    app.add_systems(
        PhysicsSchedule,
        clear_leftover_started_flags
            .after(PhysicsStepSystems::NarrowPhase)
            .before(PhysicsStepSystems::Solver)
            .ambiguous_with_all(),
    );
}

fn clear_leftover_started_flags(mut contacts: ResMut<ContactGraph>) {
    let contacts = contacts.bypass_change_detection();
    for pair in contacts.active_pairs_mut() {
        if pair.flags.contains(ContactPairFlags::STARTED_GENERATING_CONSTRAINTS) {
            pair.flags.remove(ContactPairFlags::STARTED_GENERATING_CONSTRAINTS);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use avian3d::dynamics::solver::constraint_graph::ConstraintGraph;
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
}
