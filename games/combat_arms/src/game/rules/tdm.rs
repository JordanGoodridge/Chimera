//! Team Death Match, shown as "Elimination" (`combat_arms:tdm`, room key
//! `TDM`; specs/combat_arms/game_modes.md M1): Alpha against Bravo,
//! respawning, one round with no time limit, won by the first team whose
//! kill counter reaches the goal G.
//!
//! - Score (S7's TDM column): every death adds 1 to the victim's enemy
//!   team, whoever caused it (team kills, suicides and falls feed the
//!   other side; Quirk 1). Enemy kill: killer kills +1, victim deaths +1.
//!   Team kill: killer friendly kills +1, victim deaths +1. Own damage:
//!   victim deaths, suicides and friendly kills +1 (Quirk 2). No killer:
//!   victim deaths and suicides +1. Assists as S7.
//! - Win: the first death that brings a counter to G or more asks for the
//!   round's end with the higher counter's team; a later death in the same
//!   frame (still live) counts and changes the winner only if the other
//!   counter is now strictly higher (Edge cases).
//! - No time-out (no timer); the match ends after the one round, the
//!   round's winner its winner.
//! - Leaving: the last player of a team leaving gives the round (and the
//!   match) to the other team at once.
//! - Setup: 0 s, forced (everyone frozen one frame). Friendly fire on:
//!   bullets and melee at a quarter.
//!
//! Known shortcuts: no revenge buff (the event customisation) and no
//! mutation boxes (the room's option, S12): neither exists yet.

use bevy::prelude::*;

use super::{
    CaMode, Leaver, TDM,
    scoring::stats,
    state::{CaMatch, Death, DeathKind, Phase, enemy, side},
};

/// The mode.
pub const MODE: CaMode = CaMode {
    module: TDM,
    key: "TDM",
    shown_as: "Elimination",
    description: "Team Death Match (shown as Elimination): Alpha against Bravo, respawning, the first team to G kills wins",
    respawns: true,
    warmup: false,
    warmup_revives: false,
    ff_scaled: true,
    assists: true,
    timer_shown: false,
    forced_setup: Some(0.0),
    on_death,
    time_out: |_| {},
    match_end: |w| Some(w.resource::<CaMatch>().winner),
    leaving,
};

fn on_death(world: &mut World, d: &Death) {
    match d.kind {
        DeathKind::Enemy => {
            if let Some(k) = d.killer {
                stats(world, k).kills += 1;
            }
            stats(world, d.victim).deaths += 1;
        }
        DeathKind::TeamKill => {
            if let Some(k) = d.killer {
                stats(world, k).friendly_kills += 1;
            }
            stats(world, d.victim).deaths += 1;
        }
        DeathKind::Suicide => {
            let mut s = stats(world, d.victim);
            s.deaths += 1;
            s.suicides += 1;
            s.friendly_kills += 1;
        }
        DeathKind::NoKiller => {
            let mut s = stats(world, d.victim);
            s.deaths += 1;
            s.suicides += 1;
        }
    }
    let goal = world.resource::<super::CaRuleSettings>().goal() as i32;
    let scorer = enemy(d.victim_team);
    let mut m = world.resource_mut::<CaMatch>();
    let Some(i) = side(scorer) else { return };
    m.score[i] += 1;
    if m.score.iter().all(|s| *s < goal) {
        return;
    }
    let leader = if m.score[0] > m.score[1] { 0 } else { 1 };
    let take = match m.winner.and_then(side) {
        None => true,
        Some(w) => m.score[leader] > m.score[w],
    };
    if take {
        m.win(leader as u8 + 1);
    }
}

/// The last player of a team leaving gives the round to the other team.
fn leaving(m: &mut CaMatch, l: &Leaver) {
    if m.phase == Phase::Live && l.team_players <= 1 {
        m.win(enemy(l.team));
    }
}
