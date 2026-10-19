//! The match's state (specs/combat_arms/game_modes.md S3, S7, S10): its
//! phase and clocks, the team counters, the round results, and each
//! player's statistics. The server's; a network client sees it through
//! `game::net` (`CaMatchView`).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Alpha (`JSF`) and Bravo (`Creed`) (S1).
pub const ALPHA: u8 = 1;
pub const BRAVO: u8 = 2;

/// A team id's index in the two-team counters (Alpha 0, Bravo 1); None
/// for anything else (no team, observers).
pub fn side(team: u8) -> Option<usize> {
    match team {
        ALPHA => Some(0),
        BRAVO => Some(1),
        _ => None,
    }
}

/// The other team of Alpha or Bravo.
pub fn enemy(team: u8) -> u8 {
    if team == ALPHA { BRAVO } else { ALPHA }
}

/// The match's phases (S3; ours names). Mission start and round end are
/// steps between them, not phases of their own.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Phase {
    /// The map is loaded, nobody has come in yet.
    #[default]
    Waiting,
    /// The first player came in: the mission starts at the next frame.
    Starting,
    /// Round setup: everyone respawned and frozen until `until`.
    Setup { until: f64 },
    /// Kills count; the round timer runs (`CaMatch::round_ends`).
    Live,
    /// A winner (or the time-out) ended the round; players keep playing
    /// but nothing counts until `until`.
    RoundOver { until: f64 },
    /// After a round's end, until the next mission start or the match's
    /// result.
    Intermission { until: f64 },
    /// The match's result is in.
    MatchOver,
}

/// One round's result as the server sends it (S3 step 5).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoundResult {
    /// The round's number (rounds played before it, the warm-up not
    /// counted).
    pub round: u32,
    pub warmup: bool,
    /// The winner's team id; None: no winner (a draw, the warm-up).
    pub winner: Option<u8>,
    /// The team counters when it ended (Alpha, Bravo).
    pub score: [i32; 2],
}

/// The match (the server's).
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct CaMatch {
    pub phase: Phase,
    /// The round under way is the warm-up (S9).
    pub warmup: bool,
    /// Rounds played (the warm-up not counted).
    pub round: u32,
    /// Live: when the round timer reaches 0 (game seconds); None: no
    /// timer.
    pub round_ends: Option<f64>,
    /// The round's winner so far (team id).
    pub winner: Option<u8>,
    /// A rule asked for the round's end (the next frame's mission update
    /// enters round over).
    pub end_requested: bool,
    /// The team counter the mode decides by (TDM: kills), Alpha and Bravo.
    pub score: [i32; 2],
    /// Round wins.
    pub round_wins: [u32; 2],
    /// This round's enemy kills and deaths per team (Elimination Pro's
    /// time-out).
    pub round_kills: [u32; 2],
    pub round_deaths: [u32; 2],
    /// The match's result, once decided: Some(winner), Some(None) a draw.
    pub result: Option<Option<u8>>,
    /// The round results so far, in order.
    pub results: Vec<RoundResult>,
    /// The warm-up's next revive (S9).
    pub next_revive: f64,
}

impl CaMatch {
    /// Kills count now (S7: only in the live phase, never in a warm-up).
    pub fn counting(&self) -> bool {
        self.phase == Phase::Live && !self.warmup
    }

    /// Ask for the round's end with `winner` (a win condition): the next
    /// frame's mission update enters round over.
    pub fn win(&mut self, winner: u8) {
        self.winner = Some(winner);
        self.end_requested = true;
    }
}

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
}

/// How a death came about (S7's rows).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeathKind {
    /// An enemy's damage.
    Enemy,
    /// A teammate's.
    TeamKill,
    /// The victim's own damage (its grenade, a telefrag).
    Suicide,
    /// No player's (a fall, the world).
    NoKiller,
}

/// A death the mode counts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Death {
    pub victim: Entity,
    pub victim_team: u8,
    pub killer: Option<Entity>,
    pub kind: DeathKind,
}

/// Classify a death of a player on `victim_team` by `killer` (on
/// `killer_team`): `own` when it was the victim's own damage but not a
/// fall's (falls have no killer).
pub fn classify(victim: Entity, killer: Option<Entity>, killer_team: Option<u8>, victim_team: u8, fall: bool) -> DeathKind {
    match killer {
        None => DeathKind::NoKiller,
        Some(k) if k == victim => {
            if fall {
                DeathKind::NoKiller
            } else {
                DeathKind::Suicide
            }
        }
        Some(_) if killer_team == Some(victim_team) => DeathKind::TeamKill,
        Some(_) => DeathKind::Enemy,
    }
}
