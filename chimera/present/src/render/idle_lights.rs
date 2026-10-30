//! Bevy's directional-light shadow visibility only while a directional
//! light casts shadows (`IdleLightsPlugin`). Every frame Bevy's
//! `check_dir_light_mesh_visibility` queues a command that builds a new
//! query over `ViewVisibility` (`world.query`), which matches itself
//! against every archetype: ~0.13 ms a frame on the main thread in a party
//! match (2,000+ archetypes; docs/performance.md, "Party frame-rate pass",
//! third pass), even with no shadow-casting light, when the command has
//! nothing to mark. Our maps' sun casts no shadows (`map::spawn`: models
//! are probe lit), so the system now runs only while some directional
//! light has `shadow_maps_enabled` or still lists cascades' entities (it
//! clears those itself the frame shadows go off): the same results as
//! running it every frame. (A system's own set takes no condition, so the
//! plugin takes Bevy's out of `PostUpdate` and adds it back with the
//! condition, in Bevy's set and order: in `PreStartup`, once every plugin
//! is in. Taking a system out builds the schedule, which initializes every
//! system in it; done in the plugin's `finish` it froze `PostUpdate`'s
//! systems before later plugins' `finish` had run: replicon's
//! `collect_changes` then lacked read access to the components the
//! network's parts replicate, registered in `NetPlugin::finish`, and a
//! listen server panicked on its first send, playtest 2026-10-28.)
//!
//! Known shortcuts: the order constraints are copied from bevy_light's
//! `LightPlugin` as of Bevy 0.20; a Bevy update that moves the system
//! needs them checked (`tests/it/idle_lights.rs` checks it is there once).

use bevy::{
    camera::visibility::{CascadesVisibleEntities, VisibilitySystems},
    ecs::schedule::ScheduleCleanupPolicy,
    light::{SimulationLightSystems, check_dir_light_mesh_visibility},
    prelude::*,
};

#[derive(Clone, Copy)]
pub struct IdleLightsPlugin;

impl Plugin for IdleLightsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, swap_dir_light_check);
    }
}

/// Bevy's system out (a system's own set can't take a condition), then
/// back with the condition and the same place: Bevy's set and order
/// constraints (bevy_light's `LightPlugin`). At startup, not in a plugin's
/// `build` or `finish`: removing builds `PostUpdate`, initializing its
/// systems, which must see every plugin's registrations (the module doc).
fn swap_dir_light_check(world: &mut World) {
    let _ = world.try_schedule_scope(PostUpdate, |world, schedule| {
        let removed =
            schedule.remove_systems_in_set(check_dir_light_mesh_visibility, world, ScheduleCleanupPolicy::RemoveSystemsOnly);
        if removed.is_ok_and(|n| n > 0) {
            schedule.add_systems(
                check_dir_light_mesh_visibility
                    .run_if(dir_shadows_used)
                    .in_set(SimulationLightSystems::CheckLightVisibility)
                    .after(VisibilitySystems::CalculateBounds)
                    .after(TransformSystems::Propagate)
                    .after(SimulationLightSystems::UpdateLightFrusta)
                    .after(VisibilitySystems::CheckVisibility)
                    .before(VisibilitySystems::MarkNewlyHiddenEntitiesInvisible),
            );
        }
    });
}

/// Some directional light casts shadows, or has cascade entities left to
/// clear.
pub fn dir_shadows_used(lights: Query<(&DirectionalLight, &CascadesVisibleEntities)>) -> bool {
    lights.iter().any(|(l, v)| l.shadow_maps_enabled || !v.entities.is_empty())
}

#[cfg(test)]
mod tests {
    use bevy::{camera::visibility::VisibleMeshEntities, ecs::system::RunSystemOnce};

    use super::*;

    fn used(world: &mut World) -> bool {
        world.run_system_once(dir_shadows_used).unwrap()
    }

    #[test]
    fn runs_only_while_a_sun_casts_shadows_or_has_entities_to_clear() {
        let mut world = World::new();
        assert!(!used(&mut world), "no light");
        let sun = world
            .spawn((DirectionalLight { shadow_maps_enabled: false, ..default() }, CascadesVisibleEntities::default()))
            .id();
        assert!(!used(&mut world), "our maps' sun: no shadows, nothing listed");
        world.get_mut::<DirectionalLight>(sun).unwrap().shadow_maps_enabled = true;
        assert!(used(&mut world), "shadows on");
        world.get_mut::<DirectionalLight>(sun).unwrap().shadow_maps_enabled = false;
        world
            .get_mut::<CascadesVisibleEntities>(sun)
            .unwrap()
            .entities
            .insert(Entity::PLACEHOLDER, vec![VisibleMeshEntities::default()]);
        assert!(used(&mut world), "shadows just turned off: Bevy's system clears the list once more");
    }
}
