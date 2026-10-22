//! The match's state (specs/combat_arms/game_modes.md S3, S7, S10): the
//! match itself is the mode runtime's (`chimera_sim::mode::Match`, lifted
//! from here: its phase and clocks, the team counters, the round results;
//! `CaMatch` names it), as are the team ids (Alpha and Bravo are the
//! runtime's two sides, S1) and a death's classification (S7's rows);
//! each player's statistics (`CaStats`, S7, with the modes' own) are
//! Combat Arms'. The server's; a network client sees it through
//! `game::net` (`CaMatchView`).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub use crate::mode::state::{
    ALPHA, BRAVO, Death, DeathKind, FFA_FIRST, FFA_LAST, Phase, RoundResult, classify, enemy, plays, side,
};

/// The match (the server's): the mode runtime's (`mode::Match`).
pub type CaMatch = crate::mode::Match;

/// A player's statistics for the whole match (S7): what the scoreboard
/// shows (kills, deaths, assists) and the rest the server keeps.
/// Replicated (`game::net`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaStats {
    /// Enemy kills.
    pub kills: u32,
    pub deaths: u32,
    pub suicides: u32,
    pub friendly_kills: u32,
    pub assists: u32,
    /// The modes' own (S7, S13's mode column): flag captures and returns,
    /// bomb plants, detonations and defuses, mission points (Quarantine's,
    /// VIP Elimination's, intel, flag raises) and round wins (the
    /// free-for-all round modes).
    pub captures: u32,
    pub returns: u32,
    pub plants: u32,
    pub detonations: u32,
    pub defuses: u32,
    pub mission_points: i32,
    pub round_wins: u32,
    /// Hunted: the time hunted, ms (M15).
    pub hunted_ms: u32,
    /// VIP Escort (game_modes_2.md M13.2): escapes as the VIP, kills of
    /// the VIP.
    pub vip_escapes: u32,
    pub vip_kills: u32,
    /// Hired Guns (M11.5): bars banked.
    pub bars_banked: u32,
}
