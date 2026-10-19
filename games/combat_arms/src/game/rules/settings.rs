//! The modes' settings (specs/combat_arms/game_modes.md S2): the room's
//! choices (the mode, the goal G, friendly fire, join in progress), the
//! dedicated server's file (round setup, round-over and intermission
//! lengths), the shared constants (respawn waits, spawn protection, the
//! warm-up's revive period) and each mode's goal range and mission data
//! (its round `TimeLimit` and warm-up `TrainingTimeLimit`), as data
//! (data/modes.toml, `rule_module::parse_settings`) the `ca_*` cvars set
//! after.
//!
//! Known shortcuts: the per-map mission files aren't read (every map has
//! the stock values: no TDM time limit, 180 s Elimination Pro rounds, a
//! 10 s warm-up); `ca_goal` takes any value (the lobby's range and step
//! aren't enforced: 0 is the mode's default); the respawn waits and the
//! protection run on the game's clock, not the OS millisecond clocks.

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::Deserialize;

use crate::{console::resource_cvar, rule_module};

/// data/modes.toml, built in.
pub const MODES_DATA: &str = include_str!("../../../data/modes.toml");

/// A mode's own numbers (S2's goal table, the mission files).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModeData {
    pub goal_min: u32,
    pub goal_max: u32,
    pub goal_step: u32,
    pub goal_default: u32,
    /// The mission's `TimeLimit` (s); None: the round has no timer.
    pub time_limit: Option<f32>,
    /// The mission's `TrainingTimeLimit` (s): the warm-up round's length,
    /// for the modes that have one.
    pub training_time_limit: Option<f32>,
}

/// The settings the rules read (data/modes.toml, then the cvars).
#[derive(Resource, Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CaRuleSettings {
    /// The room's mode by its room key (`TDM`, `TSV`), or `none`.
    pub mode: String,
    /// The goal G; 0: the mode's default.
    pub goal: u32,
    /// Friendly fire (S8): 1 on.
    pub friendly_fire: u8,
    /// Join in progress (S1): 1 on.
    pub join_in_progress: u8,
    /// `ObjectStart`, `ObjectEnd`, `MissionEnd` (S3), s.
    pub object_start: f32,
    pub object_end: f32,
    pub mission_end: f32,
    /// S5: the client's wait before it asks, the server's least wait.
    pub respawn_wait_client: f32,
    pub respawn_wait_server: f32,
    /// S6: shots pass through, all damage blocked (s).
    pub protect_bullets: f32,
    pub protect_all: f32,
    /// S9: the warm-up revives the dead this often (s).
    pub warmup_revive: f32,
    /// S1: round modes hold a joiner with at least this many players in.
    pub late_join_wait: u32,
    /// Each mode's numbers, by room key.
    pub modes: BTreeMap<String, ModeData>,
}

impl Default for CaRuleSettings {
    fn default() -> Self {
        rule_module::parse_settings("data/modes.toml", MODES_DATA).unwrap_or_else(|e| panic!("{e}"))
    }
}

impl CaRuleSettings {
    /// The running mode's numbers (the defaults for a mode without any).
    pub fn mode_data(&self) -> ModeData {
        self.modes.get(&self.mode).cloned().unwrap_or_default()
    }

    /// G: the room's goal, else the mode's default.
    pub fn goal(&self) -> u32 {
        if self.goal > 0 { self.goal } else { self.mode_data().goal_default }
    }
}

/// The settings and their cvars.
pub(super) fn plugin(app: &mut App) {
    app.init_resource::<CaRuleSettings>();
    resource_cvar::<CaRuleSettings, String>(
        app,
        "ca_mode",
        "The game mode by its room key: TDM (Elimination), TSV (Elimination Pro), none. Takes effect at the next \
         map load or ca_restart.",
        |s| &mut s.mode,
    );
    resource_cvar::<CaRuleSettings, u32>(app, "ca_goal", "The goal G (kills, round wins); 0: the mode's default.", |s| {
        &mut s.goal
    });
    resource_cvar::<CaRuleSettings, u8>(
        app,
        "ca_friendlyfire",
        "1: teammates hurt each other (NetFriendlyFire; bullets and melee at a quarter in the listed modes).",
        |s| &mut s.friendly_fire,
    );
    resource_cvar::<CaRuleSettings, u8>(
        app,
        "ca_join_in_progress",
        "1: players may join a running match.",
        |s| &mut s.join_in_progress,
    );
    resource_cvar::<CaRuleSettings, f32>(app, "ca_object_start", "Round setup length, s (ObjectStart).", |s| {
        &mut s.object_start
    });
    resource_cvar::<CaRuleSettings, f32>(app, "ca_object_end", "Round-over length, s (ObjectEnd).", |s| {
        &mut s.object_end
    });
    resource_cvar::<CaRuleSettings, f32>(app, "ca_mission_end", "Intermission length, s (MissionEnd).", |s| {
        &mut s.mission_end
    });
    resource_cvar::<CaRuleSettings, f32>(
        app,
        "ca_respawn_wait",
        "The server refuses a respawn sooner than this after a death, s.",
        |s| &mut s.respawn_wait_server,
    );
    resource_cvar::<CaRuleSettings, f32>(
        app,
        "ca_respawn_wait_client",
        "A dead player asks to respawn after this, s.",
        |s| &mut s.respawn_wait_client,
    );
    resource_cvar::<CaRuleSettings, f32>(
        app,
        "ca_respawn_invulnerability",
        "After a respawn, no damage at all for this long, s (RespawnInvulnerabilityTime).",
        |s| &mut s.protect_all,
    );
    resource_cvar::<CaRuleSettings, f32>(
        app,
        "ca_respawn_bullet_protection",
        "After a respawn, instant-hit shots pass through for this long, s.",
        |s| &mut s.protect_bullets,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_data_reads() {
        let s = CaRuleSettings::default();
        assert_eq!(s.mode, "TDM");
        assert_eq!(s.goal(), 100);
        assert_eq!(s.mode_data().time_limit, None);
        let tsv = &s.modes["TSV"];
        assert_eq!((tsv.goal_default, tsv.time_limit, tsv.training_time_limit), (7, Some(180.0), Some(10.0)));
        assert_eq!((s.object_start, s.object_end, s.mission_end), (0.0, 3.0, 3.0));
    }
}
