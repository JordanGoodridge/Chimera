//! Combat Arms on the engine's headless harness
//! (`chimera_world::harness::Sim`), for its scenario tests and tools:
//! `CaSim`, the base they build on, as CS:S's `CsSim` is its tests'.
//! `Sim::new(CaSim, GreyboxMapPlugin)`.

use bevy::prelude::*;

use crate::game::rules::CaRuleSettings;

/// Combat Arms' whole simulation (`crate::SimPlugins` with
/// `game::CombatArmsPlugins`: every part, placeholders included) as the
/// harness runs it, headless, as the gameplay (its tick, `game::TICK_INTERVAL`),
/// on the built-in data only (`data::CaData::builtin`: never the
/// install's attribute data; an app's own `CaData` is its resource), with
/// no game mode (`ca_mode none`: the rules leave the test's characters
/// alone; a mode's tests turn one on, `with_mode`).
#[derive(Clone, Copy, Debug, Default)]
pub struct CaSim;

impl Plugin for CaSim {
    fn build(&self, app: &mut App) {
        CaSimWith(crate::game::data::CaData::builtin()).build(app);
    }
}

/// `CaSim` on `data` instead of the built-in data (tests of an app's own
/// data: weapon tuning, an install merged by hand). Each app has its own,
/// so two `Sim`s in one process can differ.
#[derive(Clone)]
pub struct CaSimWith(pub crate::game::data::CaData);

impl Plugin for CaSimWith {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.0.clone());
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

