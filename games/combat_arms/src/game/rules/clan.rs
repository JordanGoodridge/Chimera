//! The clan modes (specs/combat_arms/game_modes_2.md M18): clan
//! Elimination (`combat_arms:clan_tdm`, `CDM`), clan Capture the Flag
//! (`combat_arms:clan_ctf`, `CCTF`), clan Search & Destroy
//! (`combat_arms:clan_search_destroy`, `CMM`) and clan Elimination Pro
//! (`combat_arms:clan_elimination_pro`, `CTSV`): the public rules (`tdm`,
//! `ctf`, `search_destroy`, `elimination`) with one clan a team, a warm-up
//! first and one half-time swap of team ids.
//!
//! - Shared (M18.1): a warm-up (10 s) before each half; friendly fire
//!   always off; no join in progress (`rules::no_join_in_progress`). At
//!   half time every player on team 1 goes to team 2 and back, and the
//!   team counters that decide the match (`CaMatch`'s score, round wins,
//!   kills, deaths, returns; S&D's mission completions) swap with them,
//!   so each clan keeps its score (`swap_teams`, at the next mission
//!   start: everyone respawned, then the warm-up, then the second half).
//!   Players' statistics are match totals and stay. The last player of a
//!   team leaving ends the match at once for the other team.
//! - `CDM` (M18.2): TDM's score, no time limit; the first death that
//!   brings a counter to ⌊G / 2⌋ (50) ends the first half's round (no
//!   winner); the second half's first counter at G (100) wins.
//! - `CCTF` (M18.3): CTF's rules in two halves of a flat `TimeLimit`
//!   (600 s); the first half ends at ⌊G / 2⌋ (3) captures of a team or at
//!   its time-out; in the second the first team at G (6) wins, its
//!   time-out goes by more captures, returns, enemy kills, fewer deaths,
//!   else a draw.
//! - `CMM`, `CTSV` (M18.4): exactly G (6) real rounds of S&D or
//!   Elimination Pro, no early end on round wins; the swap after round
//!   ⌊G / 2⌋ (3); the match after round G by clan: more round wins, (S&D)
//!   more mission completions, more enemy kills, fewer deaths, else a draw
//!   (Quirk 9: no extra rounds). A mission completion goes to the clan on
//!   team 1 at a round over when a bomb was planted that round and the
//!   round didn't end by its detonation or a defuse (Open question 7).
//!
//! Known shortcuts: clans are the teams (no clan records, marks or the
//! "left" record); a dropped flag at CTF half time goes home with the
//! next setup (Open question 8); the clan's enemy kills and deaths are
//! summed over the players in the match at its end (a leaver's go with
//! it).

use bevy::prelude::*;

use super::{
    CLAN_CTF, CLAN_ELIMINATION_PRO, CLAN_SEARCH_DESTROY, CLAN_TDM, CaMode, CaRuleSettings, Hooks, Leaver, RoundTime,
    bomb::CaPlanter,
    ctf, elimination,
    flow::players,
    objectives::CaObjectives,
    scoring::personal,
    search_destroy,
    state::{ALPHA, CaMatch, CaStats, Death, Phase, enemy, side},
    tdm,
};
use crate::core::Team;

/// The clan modes' state (the server's).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct CaClan {
    /// The second half is under way (or its warm-up).
    pub second_half: bool,
    /// The half-time swap at the next mission start.
    pub swap_due: bool,
    /// S&D's mission completions per team (they swap with the clans).
    pub completions: [u32; 2],
    /// Detonations and defuses counted when the round started.
    pub bomb_ends: u32,
}

/// Clan Elimination.
pub const TDM: CaMode = CaMode {
    module: CLAN_TDM,
    key: "CDM",
    shown_as: "Clan Elimination",
    description: "Clan Elimination: Team Death Match between two clans, sides swapped at G/2 kills after a warm-up; G \
                  kills win",
    warmup: true,
    warmup_revives: true,
    on_death: tdm_death,
    match_end: halves_match_end,
    leaving,
    hooks: Hooks {
        setup,
        update: |w| clan_update(w, |_| {}),
        before_spawns: swap_teams,
        after_round: half_after_round,
        ..super::NO_HOOKS
    },
    ..tdm::MODE
};

/// Clan Capture the Flag.
pub const CTF: CaMode = CaMode {
    module: CLAN_CTF,
    key: "CCTF",
    shown_as: "Clan Capture the Flag",
    description: "Clan Capture the Flag: two halves of 600 s, sides swapped at G/2 captures or the first half's end; G \
                  captures win",
    warmup: true,
    warmup_revives: true,
    round_time: RoundTime::Mission,
    time_out: ctf_time_out,
    match_end: halves_match_end,
    leaving,
    hooks: Hooks {
        setup: |w| {
            (ctf::MODE.hooks.setup)(w);
            setup(w);
        },
        update: |w| clan_update(w, ctf_update),
        before_spawns: swap_teams,
        after_round: half_after_round,
        ..ctf::MODE.hooks
    },
    ..ctf::MODE
};

/// Clan Search & Destroy.
pub const SEARCH_DESTROY: CaMode = CaMode {
    module: CLAN_SEARCH_DESTROY,
    key: "CMM",
    shown_as: "Clan Search & Destroy",
    description: "Clan Search & Destroy: exactly G rounds, sides swapped after G/2 with a warm-up",
    match_end: rounds_match_end,
    leaving: rounds_leaving,
    hooks: Hooks {
        setup: |w| {
            (search_destroy::HOOKS.setup)(w);
            setup(w);
        },
        update: |w| clan_update(w, search_destroy::HOOKS.update),
        round_over: completions,
        before_spawns: swap_teams,
        after_round: rounds_after_round,
        ..search_destroy::HOOKS
    },
    ..search_destroy::MODE
};

/// Clan Elimination Pro.
pub const ELIMINATION_PRO: CaMode = CaMode {
    module: CLAN_ELIMINATION_PRO,
    key: "CTSV",
    shown_as: "Clan Elimination Pro",
    description: "Clan Elimination Pro: exactly G rounds, sides swapped after G/2 with a warm-up",
    match_end: rounds_match_end,
    leaving: rounds_leaving,
    hooks: Hooks {
        setup,
        update: |w| clan_update(w, |_| {}),
        before_spawns: swap_teams,
        after_round: rounds_after_round,
        ..super::NO_HOOKS
    },
    ..elimination::MODE
};

/// The clan modes' room keys.
pub const KEYS: &[&str] = &["CDM", "CCTF", "CMM", "CTSV", "CESC"];

fn clan(world: &mut World) -> CaClan {
    *world.get_resource_or_init::<CaClan>()
}

/// The round's setup: the half shown, the bomb ends counted.
fn setup(world: &mut World) {
    let c = clan(world);
    world.resource_mut::<CaObjectives>().half = if c.second_half { 2 } else { 1 };
    let ends = bomb_ends(world);
    world.resource_mut::<CaClan>().bomb_ends = ends;
}

/// Every player's detonations and defuses.
fn bomb_ends(world: &mut World) -> u32 {
    world.query::<&CaStats>().iter(world).map(|s| s.detonations + s.defuses).sum()
}

/// The mode's own update, then: a match a leaver decided ends its round.
fn clan_update(world: &mut World, base: fn(&mut World)) {
    base(world);
    let mut m = world.resource_mut::<CaMatch>();
    if m.result.is_some() && m.phase == Phase::Live && !m.end_requested {
        m.end_requested = true;
    }
}

/// M18.1: at the mission start after half time, every player to the
/// other team and the counters with them.
fn swap_teams(world: &mut World) {
    if !clan(world).swap_due {
        return;
    }
    for (e, team) in players(world) {
        if side(team).is_some() {
            world.entity_mut(e).insert(Team(enemy(team)));
        }
    }
    {
        let mut m = world.resource_mut::<CaMatch>();
        m.score.swap(0, 1);
        m.round_wins.swap(0, 1);
        m.kills.swap(0, 1);
        m.deaths.swap(0, 1);
        m.returns.swap(0, 1);
    }
    let mut c = world.resource_mut::<CaClan>();
    c.completions.swap(0, 1);
    c.second_half = true;
    c.swap_due = false;
    info!("clan half time: teams swapped");
}

/// Ask for the swap and a warm-up at the next mission start.
fn ask_swap(world: &mut World) {
    world.resource_mut::<CaClan>().swap_due = true;
    world.resource_mut::<CaMatch>().warmup_next = true;
}

/// CDM, CCTF: the first half's real round over → the swap.
fn half_after_round(world: &mut World) {
    let c = clan(world);
    let m = world.resource::<CaMatch>();
    let real = m.results.last().is_some_and(|r| !r.warmup);
    let due = real && m.result.is_none() && !c.second_half && !c.swap_due;
    if due {
        ask_swap(world);
    }
}

/// CMM, CTSV: after round ⌊G / 2⌋ → the swap.
fn rounds_after_round(world: &mut World) {
    let c = clan(world);
    let half = world.resource::<CaRuleSettings>().goal() / 2;
    let m = world.resource::<CaMatch>();
    let real = m.results.last().is_some_and(|r| !r.warmup);
    let due = real && m.result.is_none() && !c.second_half && !c.swap_due && m.round == half;
    if due {
        ask_swap(world);
    }
}

/// CDM's deaths: TDM's; the first half ends at ⌊G / 2⌋.
fn tdm_death(world: &mut World, d: &Death) {
    if clan(world).second_half {
        personal(world, d);
        tdm::add_and_test(world, d.victim_team, 1);
        return;
    }
    personal(world, d);
    let half = (world.resource::<CaRuleSettings>().goal() / 2) as i32;
    let mut m = world.resource_mut::<CaMatch>();
    if let Some(i) = side(enemy(d.victim_team)) {
        m.score[i] += 1;
    }
    if m.score.iter().any(|s| *s >= half) && !m.decided() {
        m.end_without_winner();
    }
}

/// CCTF: CTF's update; the first half ends at ⌊G / 2⌋ captures.
fn ctf_update(world: &mut World) {
    (ctf::MODE.hooks.update)(world);
    if clan(world).second_half {
        return;
    }
    let half = (world.resource::<CaRuleSettings>().goal() / 2) as i32;
    let mut m = world.resource_mut::<CaMatch>();
    if m.counting() && !m.decided() && m.score.iter().any(|s| *s >= half) {
        m.end_without_winner();
    }
}

/// CCTF's time-out: the first half's ends without a winner.
fn ctf_time_out(world: &mut World) {
    if clan(world).second_half {
        (ctf::MODE.time_out)(world);
    }
}

/// CDM, CCTF: an empty team loses; else the second half's round decides.
fn halves_match_end(world: &World) -> Option<Option<u8>> {
    if let Some(r) = empty_team(world) {
        return Some(r);
    }
    let second = world.get_resource::<CaClan>().is_some_and(|c| c.second_half);
    second.then(|| world.resource::<CaMatch>().winner)
}

/// An empty team: the other wins.
fn empty_team(world: &World) -> Option<Option<u8>> {
    let counts = world.resource::<super::CaTeamCounts>().0;
    match counts {
        [0, b] if b > 0 => Some(Some(super::state::BRAVO)),
        [a, 0] if a > 0 => Some(Some(ALPHA)),
        [0, 0] => Some(None),
        _ => None,
    }
}

/// CMM, CTSV: after round G by clan: round wins, (S&D) mission
/// completions, enemy kills, fewer deaths, else a draw.
fn rounds_match_end(world: &World) -> Option<Option<u8>> {
    if let Some(r) = empty_team(world) {
        return Some(r);
    }
    let goal = world.resource::<CaRuleSettings>().goal();
    let m = world.resource::<CaMatch>();
    if m.round < goal {
        return None;
    }
    let completions = world.get_resource::<CaClan>().map_or([0; 2], |c| c.completions);
    let sd = super::current_mode(world).is_some_and(|x| x.key == SEARCH_DESTROY.key);
    let (kills, deaths) = clan_totals(world);
    let pick = ctf::pick;
    let winner = pick(m.round_wins[0] as i64, m.round_wins[1] as i64)
        .or_else(|| if sd { pick(completions[0] as i64, completions[1] as i64) } else { None })
        .or_else(|| pick(kills[0], kills[1]))
        .or_else(|| pick(deaths[1], deaths[0]));
    Some(winner)
}

/// Each team's players' enemy kills and deaths over the match.
pub fn clan_totals(world: &World) -> ([i64; 2], [i64; 2]) {
    let (mut kills, mut deaths) = ([0i64; 2], [0i64; 2]);
    let Some(mut q) = world.try_query_filtered::<(&Team, &CaStats), (
        With<crate::core::Intent>,
        Without<crate::core::Npc>,
        Without<crate::core::Spectating>,
    )>() else {
        return (kills, deaths);
    };
    for (t, s) in q.iter(world) {
        if let Some(i) = side(t.0) {
            kills[i] += s.kills as i64;
            deaths[i] += s.deaths as i64;
        }
    }
    (kills, deaths)
}

/// CMM's round over: a mission completion for team 1 when a bomb was
/// planted and the round didn't end by its detonation or a defuse.
fn completions(world: &mut World) {
    if world.resource::<CaMatch>().warmup || world.resource::<CaPlanter>().0.is_none() {
        return;
    }
    let ends = bomb_ends(world);
    let mut c = world.resource_mut::<CaClan>();
    if ends == c.bomb_ends {
        c.completions[0] += 1;
    }
    c.bomb_ends = ends;
}

/// M18.1: the last player of a team leaving ends the match for the other
/// team at once.
fn leaving(m: &mut CaMatch, l: &Leaver) {
    if l.team_players <= 1 && m.result.is_none() && !matches!(m.phase, Phase::Waiting | Phase::MatchOver) {
        m.result = Some(Some(enemy(l.team)));
        if m.phase == Phase::Live {
            m.end_requested = true;
        }
    }
}

/// The round modes': the clans' rule, then Elimination Pro's (a living
/// leaver, the last of its team alive, gives the round away).
fn rounds_leaving(m: &mut CaMatch, l: &Leaver) {
    leaving(m, l);
    if m.result.is_none() {
        elimination::leaving(m, l);
    }
}

/// A fresh match: the first half.
pub fn reset(world: &mut World) {
    world.insert_resource(CaClan::default());
}
