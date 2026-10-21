//! Combat Arms on the engine's headless harness
//! (`chimera_world::harness::Sim`), for its scenario tests and tools:
//! `CaSim`, the base they build on, as CS:S's `CsSim` is its tests'.
//! `Sim::new(CaSim, GreyboxMapPlugin)`.

use bevy::prelude::*;

use crate::game::rules::CaRuleSettings;

/// Combat Arms' whole simulation (`crate::SimPlugins` with
/// `game::CombatArmsPlugins`: every part, placeholders included) as the
/// harness runs it, headless, as the gameplay (its tick, `game::TICK_INTERVAL`),
/// on the built-in data only (`data::use_builtin_data_only`: never the
/// install's attribute data), with no game mode (`ca_mode none`: the
/// rules leave the test's characters alone; a mode's tests turn one on,
/// `with_mode`).
#[derive(Clone, Copy, Debug, Default)]
pub struct CaSim;

impl Plugin for CaSim {
    fn build(&self, app: &mut App) {
        crate::game::data::use_builtin_data_only();
        use crate::gameplay::{Gameplay, GameplayAppExt};
        app.add_plugins((crate::SimPlugins, crate::game::CombatArmsPlugins))
            .set_gameplay(Gameplay::of(&crate::game::INFO));
        app.world_mut().resource_mut::<CaRuleSettings>().mode = "none".into();
    }
}

/// Play `mode` (a room key: `TDM`, `TSV`) from the next server frame on,
/// the match from its start (`ca_mode` and `ca_restart`).
pub fn with_mode(world: &mut World, mode: &str) {
    world.resource_mut::<CaRuleSettings>().mode = mode.into();
    crate::game::rules::flow::restart(world);
}

