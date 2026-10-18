//! Combat Arms on the engine's headless harness
//! (`chimera_world::harness::Sim`), for its scenario tests and tools:
//! `CaSim`, the base they build on, as CS:S's `CsSim` is its tests'.
//! `Sim::new(CaSim, GreyboxMapPlugin)`.

use bevy::prelude::*;

/// Combat Arms' whole simulation (`crate::SimPlugins` with
/// `game::CombatArmsPlugins`: every part, placeholders included) as the
/// harness runs it, headless, at the game's tick (`game::TICK_INTERVAL`).
#[derive(Clone, Copy, Debug, Default)]
pub struct CaSim;

impl Plugin for CaSim {
    fn build(&self, app: &mut App) {
        app.add_plugins((crate::SimPlugins, crate::game::CombatArmsPlugins))
            .insert_resource(Time::<Fixed>::from_seconds(crate::game::TICK_INTERVAL));
    }
}
