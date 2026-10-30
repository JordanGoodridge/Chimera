//! Taking characters out of play and bringing them back, for any game's
//! rules (lifted from CS:S's deathmatch for Combat Arms' modes,
//! docs/plans/active/combat-arms.md L15): what every game's respawn does
//! whatever its rules say about when and where.
//!
//! - `hold_the_dead`: the dead (`core::Dead`) neither move nor shoot;
//!   their intent is cleared each tick but the look. A game's rules add
//!   it to their own set (CS:S's in `SimSet::Rules`, after its respawns).
//! - `revive`: back in play: no longer `Dead` or non-solid, full health,
//!   still, out of a noclip the `noclip` command turned on (games spawn
//!   their players walking), and, given a place, there and facing its way.
//!   Which place, the weapons and whatever else a spawn gives are the
//!   game's (CS:S's `rules::put_at_spawn`, Combat Arms' `rules::spawns`).

use avian3d::prelude::{ColliderDisabled, Position};
use bevy::prelude::*;

use crate::core::{Dead, Health, Intent, Velocity};

/// The dead neither move nor shoot (their intent is cleared each tick,
/// the look kept).
pub fn hold_the_dead(mut dead: Query<&mut Intent, With<Dead>>) {
    for mut intent in &mut dead {
        let (yaw, pitch) = (intent.yaw, intent.pitch);
        *intent = Intent {
            yaw,
            pitch,
            ..default()
        };
    }
}

/// Bring `e` back into play: out of a toggled noclip, no longer `Dead` or
/// non-solid (`ColliderDisabled`), at full health, with no velocity, and,
/// with `at`, placed there (the transform and the physics' position: spawn
/// points are already lifted to a character's centre) looking along its
/// yaw with a level pitch. False when `e` doesn't exist.
pub fn revive(world: &mut World, e: Entity, at: Option<Transform>) -> bool {
    crate::movement::noclip::end_toggled(world, e);
    let Ok(mut ent) = world.get_entity_mut(e) else { return false };
    ent.remove::<(Dead, ColliderDisabled)>();
    if let Some(mut h) = ent.get_mut::<Health>() {
        h.current = h.max;
    }
    if let Some(mut v) = ent.get_mut::<Velocity>() {
        v.0 = Vec3::ZERO;
    }
    if let Some(at) = at {
        if let Some(mut t) = ent.get_mut::<Transform>() {
            t.translation = at.translation;
        }
        if let Some(mut p) = ent.get_mut::<Position>() {
            p.0 = at.translation;
        }
        let (yaw, pitch, _) = at.rotation.to_euler(EulerRot::YXZ);
        chimera_core::core::set_view(world, e, yaw, pitch);
    }
    true
}
