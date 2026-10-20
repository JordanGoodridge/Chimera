//! Hunted (`combat_arms:hunted`, room key `HG`;
//! specs/combat_arms/game_modes.md M15): Last Man Standing (M6, the
//! `last_man` rules) with its own spawns: the unlocked `HungerGame` points
//! in the warm-up, the locked ones in the rounds (one list each, shuffled
//! once a map load). Firing ends spawn protection.
//!
//! Known shortcuts: no scavenging (players start with the usual loadout;
//! no supply drops on the drop points, S12), no emotes and no time hunted:
//! the scoreboard ranks by mission points (round wins) then kills. The
//! death camera's hold, pull-back and free cameras are the client's
//! (`game::death_cam`; its hold rides the body's place, not the `Eyes`
//! socket).

use super::{CaMode, HUNTED, Spawns, last_man};

/// The mode.
pub const MODE: CaMode = CaMode {
    module: HUNTED,
    key: "HG",
    shown_as: "Hunted",
    description: "Hunted: Last Man Standing on the HungerGame points (unlocked in the warm-up, locked in rounds)",
    spawns: Spawns {
        family: Some("HungerGame"),
        one_list: true,
        shuffled: true,
        locked_rounds: true,
    },
    ..last_man::MODE
};
