//! CS:S's game modes as rule modules (`rules::module`): deathmatch
//! respawns (`mashup_rounds 0`), and with rounds (`mashup_rounds 1`) the
//! bomb scenario on maps with bomb targets, the hostage scenario on maps
//! with hostages, else plain elimination rounds. Tried in that order.
//!
//! The rules themselves still run from `rules` (deathmatch, the rounds
//! with money, game end) and `objectives` (bomb, hostages), as before:
//! the network, the bots and the movement name their types, so they move
//! under `cs_source` once those reach them through interfaces
//! (docs/plans/active/engine-split.md).

use bevy::prelude::*;

use crate::{
    objectives::{MapKind, MapObjectives},
    rules::{
        module::{RuleModule, RuleModules},
        rounds::RoundSettings,
    },
};

fn rounds(world: &World) -> bool {
    world.get_resource::<RoundSettings>().is_some_and(|r| r.enabled != 0)
}

fn map_kind(world: &World) -> MapKind {
    world
        .get_resource::<MapObjectives>()
        .map_or(MapKind::Neither, |o| o.kind())
}

/// The rounds' cvars, every round mode's.
const ROUND_CVARS: &[&str] = &[
    "mashup_rounds",
    "mp_freezetime",
    "mp_roundtime",
    "mp_buytime",
    "mp_startmoney",
    "mp_tkpunish",
    "mp_timelimit",
    "mp_maxrounds",
    "mp_winlimit",
    "mp_chattime",
];

pub const MODULES: [RuleModule; 4] = [
    RuleModule {
        name: "cs_source:deathmatch",
        description: "respawn after a delay, kills and deaths counted",
        settings: &[
            "mashup_rounds",
            "mp_respawn_delay",
            "mp_friendlyfire",
            "mp_fadetoblack",
            "mp_timelimit",
        ],
        applies: |w| !rounds(w),
    },
    RuleModule {
        name: "cs_source:bomb",
        description: "rounds with money: the terrorists plant the bomb, the counter-terrorists defend or defuse",
        settings: ROUND_CVARS,
        applies: |w| rounds(w) && map_kind(w) == MapKind::Bomb,
    },
    RuleModule {
        name: "cs_source:hostage",
        description: "rounds with money: the counter-terrorists rescue the hostages, the terrorists guard them",
        settings: ROUND_CVARS,
        applies: |w| rounds(w) && map_kind(w) == MapKind::Hostage,
    },
    RuleModule {
        name: "cs_source:elimination",
        description: "rounds with money, won by eliminating the other team (the defenders on time)",
        settings: ROUND_CVARS,
        applies: rounds,
    },
];

/// Registers CS:S's rule modules.
pub struct CsRulesPlugin;

impl Plugin for CsRulesPlugin {
    fn build(&self, app: &mut App) {
        for module in MODULES {
            RuleModules::register(app, module);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_map_and_mashup_rounds_pick_the_module() {
        let mut app = App::new();
        app.add_plugins(CsRulesPlugin).init_resource::<RoundSettings>();
        let pick = |app: &App| crate::rules::module::pick(app.world());
        assert_eq!(pick(&app), Some("cs_source:deathmatch"));
        app.world_mut().resource_mut::<RoundSettings>().enabled = 1;
        assert_eq!(pick(&app), Some("cs_source:elimination"), "no objectives");
        let zone = crate::objectives::Zone::engine_box(Vec3::ZERO, Vec3::ONE);
        app.insert_resource(MapObjectives {
            bomb_targets: vec![zone.clone()],
            ..default()
        });
        assert_eq!(pick(&app), Some("cs_source:bomb"));
        app.insert_resource(MapObjectives {
            hostages: vec![crate::objectives::HostageSpawn {
                feet: Vec3::ZERO,
                yaw: 0.0,
                kind: None,
                ..Default::default()
            }],
            ..default()
        });
        assert_eq!(pick(&app), Some("cs_source:hostage"));
    }
}
