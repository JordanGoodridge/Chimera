//! S&D for the Go4CA Cup (`combat_arms:go4ca_cup`, room key `ESL`;
//! specs/combat_arms/game_modes.md M17): Search & Destroy (M3, its
//! `search_destroy` rules and hooks) except that the match ends when both
//! teams' round wins together reach G (5): a fixed number of rounds.
//! Winner: more round wins, else Bravo.
//!
//! Known shortcuts: an empty team ends the match as in S&D (M17 changes
//! only the round total).

use bevy::prelude::*;

use super::{
    CaMode, CaRuleSettings, GO4CA_CUP, search_destroy,
    state::{ALPHA, BRAVO, CaMatch},
};

/// The mode.
pub const MODE: CaMode = CaMode {
    module: GO4CA_CUP,
    key: "ESL",
    shown_as: "S&D for Go4CA Cup",
    description: "S&D for the Go4CA Cup: Search & Destroy over a fixed number of rounds (both teams' wins reach G)",
    match_end,
    ..search_destroy::MODE
};

/// Both teams' wins reach G, or a team is empty: more wins, else Bravo.
fn match_end(world: &World) -> Option<Option<u8>> {
    let goal = world.resource::<CaRuleSettings>().goal();
    let m = world.resource::<CaMatch>();
    let counts = world.resource::<super::CaTeamCounts>().0;
    if !counts.contains(&0) && m.round_wins[0] + m.round_wins[1] < goal {
        return None;
    }
    Some(Some(if m.round_wins[0] > m.round_wins[1] { ALPHA } else { BRAVO }))
}
