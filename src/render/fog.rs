//! Pushing the scene's fog (`map::fog::SceneFog`) to the materials that
//! draw after the world (ropes, runtime decals, particles) when it
//! changes (`apply_scene_fog`; not scheduled: those materials take the fog
//! when created).

use bevy::prelude::*;

use crate::map::fog::SceneFog;

/// Put the current fog on rope, decal and particle materials when it
/// changes.
pub fn apply_scene_fog(
    fog: Option<Res<SceneFog>>,
    ropes: Option<ResMut<Assets<crate::map::rope_material::RopeMaterial>>>,
    decals: Option<ResMut<Assets<crate::map::decal::DecalMaterial>>>,
    particles: Option<ResMut<Assets<super::particles::ParticleDrawMaterial>>>,
) {
    let Some(fog) = fog.filter(|f| f.is_changed()) else {
        return;
    };
    let f = fog.0;
    if let Some(mut ropes) = ropes {
        let ids: Vec<_> = ropes.ids().collect();
        for id in ids {
            if ropes.get(id).is_some_and(|m| m.params.fog != f)
                && let Some(mut m) = ropes.get_mut(id)
            {
                m.params.fog = f;
            }
        }
    }
    if let Some(mut decals) = decals {
        let ids: Vec<_> = decals.ids().collect();
        for id in ids {
            if decals.get(id).is_some_and(|m| m.fog != f)
                && let Some(mut m) = decals.get_mut(id)
            {
                m.fog = f;
            }
        }
    }
    if let Some(mut particles) = particles {
        let ids: Vec<_> = particles.ids().collect();
        for id in ids {
            if particles.get(id).is_some_and(|m| m.fog != f)
                && let Some(mut m) = particles.get_mut(id)
            {
                m.fog = f;
            }
        }
    }
}
