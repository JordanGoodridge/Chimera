//! Combat Arms' game modes as rule modules (`rule_module`: registered by
//! name, picked by the map and cvars): their teams, rounds, scoring,
//! respawning and win conditions. Defined by specs/combat_arms/game_modes.md
//! (not written yet).
//!
//! Wave 0: one placeholder module, `combat_arms:placeholder` (it always
//! applies): no teams, rounds, score or respawning; whoever dies stays
//! dead.
//!
//! The seam: each mode registers here as a `RuleModule` with its
//! settings as data (`rule_module::parse_settings`), its systems in
//! `core::SimSet::Rules`. Deathmatch respawns, team joining, rounds and
//! the end of a game exist only in CS:S's crate (games/cs_source/src/
//! rules.rs, rules/), a lift candidate for Chimera before Combat Arms'
//! modes need them.

use bevy::prelude::*;

use crate::rule_module::{self, RuleModule, RuleModules};

/// The skeleton's only mode (PLACEHOLDER).
pub const PLACEHOLDER: &str = "combat_arms:placeholder";

pub const MODULES: [RuleModule; 1] = [RuleModule {
    name: PLACEHOLDER,
    description: "placeholder until specs/combat_arms/game_modes.md: no teams, rounds, score or respawning",
    settings: &[],
    applies: |_| true,
}];

/// Combat Arms' rule modules (and the rule-module interface, if no other
/// game added it).
pub struct CaRulesPlugin;

impl Plugin for CaRulesPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<RuleModules>() {
            rule_module::plugin(app);
        }
        for m in MODULES {
            RuleModules::register(app, m);
        }
    }
}
