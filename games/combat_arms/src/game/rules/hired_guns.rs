//! Hired Guns (`combat_arms:hired_guns`, room key `SQM`;
//! specs/combat_arms/game_modes_2.md M11): Alpha against Bravo in rounds
//! of gold collection (300 s) after a warm-up, respawning; G rounds played
//! make the match. Bullion bars lie on the map, players carry them in a
//! pocket that slows them down and bank them in their team's bank; bank
//! money hires AI mercenaries (`mercs`).
//!
//! - Bars (M11.3): at each real round's setup the mode's drop points
//!   (`ObjectSpawnPoint`s of `GameType` `SquadMatch`) are shuffled and a
//!   bar of `bullion_amount` (100) is placed on each of the first
//!   min(`drop_bullion_num`, points) (80); taken with the use key while
//!   the player's view ray meets it within `pickup_reach` (200 u) of the
//!   player. A death in a real round's live phase drops the whole pocket
//!   as a bag at the body (raised by its half-height, then to the floor),
//!   taken by touch within the same reach; a leaver's pocket drops the
//!   same way in any phase. Anyone takes any item; AI never do. The
//!   objects are `objectives::CaObjectives::bullion` (every machine's).
//! - The pocket (M11.4, `objectives::CaCarry::bars`, `money`): no limit;
//!   its weight is the carry penalty (`HiredGunsData::penalty`) as the
//!   movement's custom term (`movement::CustomMovePenalty`), 0 once
//!   banked, dropped or cleared at a round start.
//! - Banks (M11.5): every tick a player's box overlaps its own team's
//!   `BullionSaveTrigger` with money in the pocket (live, real round), the
//!   whole pocket is banked: the player's bank (`CaBank`) and mission
//!   points + its money, the team's banked bars + its bars. When the
//!   banked bars of both teams make all the bars placed and nothing is
//!   left on the ground, the round asks to end.
//! - Bank money (M11.6, not reset between rounds): + `base_bank` (1000)
//!   at each real round's start (+ `round_win_bonus` (500) for the
//!   previous round's winners) and on joining a live real round; +
//!   `kill_bonus` (100) for an enemy player's or an AI's kill (a
//!   mercenary's kill pays its hirer); + `death_bonus` (0) at one's own
//!   death; − a hire's cost.
//! - Deaths (M11.8's table, live real rounds only): every player death
//!   the victim's team's round deaths +1, victim deaths +1, the death
//!   bonus; an enemy kill (a player's or its mercenary's) killer kills +1,
//!   the kill bonus, the killer's team's round kills +1; a suicide victim
//!   suicides +1; a team kill takes the victim's death back, killer
//!   friendly kills +1. A mercenary killed by a player: an enemy kill and
//!   the bonus for that player, a round kill for its team, no death for
//!   anyone.
//! - Round winner (at the round over when none was set): more banked
//!   bars; else more round kills; else fewer round deaths; else none (no
//!   round win, no bonus). At the round over the winners' mission points
//!   + the round-win bonus; every bar, bag and AI removed.
//! - Match end: after real round G, or when a team is empty: the other
//!   team; else more round wins; else Bravo.
//! - Leaving (M11.9): the leaver's mercenaries go, its pocket drops; the
//!   last player of a team leaving asks for the round's end (the winner by
//!   the rule above; the match end test gives the other team the match);
//!   a living leaver with no teammate alive gives the round to the other
//!   team (Quirk 5).
//! - Scoreboard: mission points, kills, fewer deaths (S13).
//!
//! Known shortcuts: a bar's box is 10 u each way and a bag's 16 (the
//! props' sizes aren't in the spec), the view ray isn't stopped by walls;
//! the "saved" event's sounds and effect, the map's vault doors and
//! way-points (world objects) and the chat warning (the HUD's objective
//! line says it) aren't there; no headshot statistic for a mercenary
//! kill (`CaStats` has none); the per-map mission file isn't read (Blood
//! Money's values are data/modes.toml's `[hired_guns]`).

use std::collections::HashMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::{
    super::{inventory, movement::CustomMovePenalty},
    CaMode, CaRuleSettings, HIRED_GUNS, Hooks, LeftPlayer, Leaver, NO_HOOKS, Panel, RoundTime, ScoreOrder, Spawns,
    TEAM_POINTS, Teams, flow, mercs,
    objectives::{self, Bullion, CaObjectives},
    objects::{CaMapObjects, ObjBox},
    scoring::stats,
    state::{ALPHA, BRAVO, CaMatch, Death, DeathKind, Phase, enemy, side},
};
use crate::core::{Died, MovementState, Npc};

/// The drop points' and spawn points' game type (data/modes.toml's
/// `hired_guns.family`).
pub const FAMILY: &str = "SquadMatch";
/// A placed bar's and a bag's box half-extents (u).
pub const BAR_HALF: f32 = 10.0;
pub const BAG_HALF: f32 = 16.0;

/// The mode.
pub const MODE: CaMode = CaMode {
    module: HIRED_GUNS,
    key: "SQM",
    shown_as: "Hired Guns",
    description: "Hired Guns: Alpha against Bravo, respawning; carry bullion bars to your bank in 300 s rounds, hire AI \
                  mercenaries with bank money; G rounds played",
    respawns: true,
    warmup: true,
    warmup_revives: true,
    ff_scaled: true,
    assists: true,
    timer_shown: true,
    forced_setup: None,
    teams: Teams::Two,
    spawns: Spawns {
        family: Some(FAMILY),
        ..TEAM_POINTS
    },
    round_time: RoundTime::Mission,
    fire_ends_protection: false,
    death_drops: true,
    panel: Panel::RoundWins,
    order: ScoreOrder::MissionPoints,
    on_death,
    time_out,
    match_end,
    leaving,
    hooks: Hooks {
        setup,
        update,
        touches,
        round_over,
        died,
        left,
        ..NO_HOOKS
    },
};

/// A player's bank money (M11.6); replicated (the hire menu's and the
/// HUD's).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaBank(pub i32);

/// The real round a player was last paid its base bank money for.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaPaid(pub u32);

/// The server's Hired Guns state.
#[derive(Resource, Clone, Debug, Default)]
pub struct CaHiredGuns {
    /// The next item's id.
    pub next_id: u32,
    /// The previous real round's winner.
    pub last_winner: Option<u8>,
    /// Each player's use key at the last tick (a press is its edge).
    pub prev_use: HashMap<Entity, bool>,
}

/// Add `amount` to `e`'s bank (never below 0).
pub fn pay(world: &mut World, e: Entity, amount: i32) {
    let Ok(mut ent) = world.get_entity_mut(e) else { return };
    let now = ent.get::<CaBank>().map_or(0, |b| b.0);
    ent.insert(CaBank((now + amount).max(0)));
}

/// A bonus: bank and mission points.
fn bonus(world: &mut World, e: Entity, amount: i32) {
    if amount == 0 {
        return;
    }
    pay(world, e, amount);
    stats(world, e).mission_points += amount;
}

/// `e`'s pocket's weight as its custom move penalty.
fn weigh(world: &mut World, e: Entity) {
    let bars = objectives::carry(world, e).bars;
    let p = world.resource::<CaRuleSettings>().hired_guns.penalty(bars);
    let Ok(mut ent) = world.get_entity_mut(e) else { return };
    if p == 0.0 {
        ent.remove::<CustomMovePenalty>();
    } else if ent.get::<CustomMovePenalty>() != Some(&CustomMovePenalty(p)) {
        ent.insert(CustomMovePenalty(p));
    }
}

/// Every team's bars in pockets.
fn count_pockets(world: &mut World) {
    let mut n = [0u32; 2];
    for (e, t) in flow::players(world) {
        if let Some(i) = side(t) {
            n[i] += objectives::carry(world, e).bars;
        }
    }
    if world.resource::<CaObjectives>().in_pockets != n {
        world.resource_mut::<CaObjectives>().in_pockets = n;
    }
}

/// Put an item on the ground (a bag falls to the floor below `at`).
fn place(world: &mut World, at: Vec3, bars: u32, money: i32, bag: bool) {
    let at = if bag { objectives::floor_below(world, at) } else { at };
    let id = {
        let mut h = world.resource_mut::<CaHiredGuns>();
        h.next_id += 1;
        h.next_id
    };
    world.resource_mut::<CaObjectives>().bullion.push(Bullion { id, at, bars, money, bag });
}

/// `e`'s whole pocket dropped as a bag at `at` (its centre; raised by the
/// half-height, then to the floor); the pocket emptied.
pub fn drop_pocket(world: &mut World, e: Option<Entity>, at: Vec3, carry: objectives::CaCarry) {
    if carry.money <= 0 {
        return;
    }
    let half = objectives::player_half().y;
    place(world, at + Vec3::Y * half, carry.bars, carry.money, true);
    if let Some(e) = e {
        let mut c = objectives::carry(world, e);
        c.bars = 0;
        c.money = 0;
        objectives::set_carry(world, e, c);
        weigh(world, e);
    }
    count_pockets(world);
}

/// The round's setup: pockets cleared (every round); in a real round the
/// bank pay and the bars.
fn setup(world: &mut World) {
    {
        let mut o = world.resource_mut::<CaObjectives>();
        o.bullion.clear();
        o.banked = [0; 2];
        o.placed = 0;
        o.in_pockets = [0; 2];
    }
    let weighed: Vec<Entity> = world.query_filtered::<Entity, With<CustomMovePenalty>>().iter(world).collect();
    for e in weighed {
        world.entity_mut(e).remove::<CustomMovePenalty>();
    }
    let (warmup, round) = {
        let m = world.resource::<CaMatch>();
        (m.warmup, m.round)
    };
    if warmup {
        return;
    }
    let h = world.resource::<CaRuleSettings>().hired_guns.clone();
    // The objective's `AIDamageFactor` becomes the console variable's.
    if let Some(mut ai) = world.get_resource_mut::<super::super::weapons::CaAiDamage>() {
        ai.factor = h.ai_damage_factor;
    }
    let winner = world.resource::<CaHiredGuns>().last_winner;

    for (e, team) in flow::players(world) {
        let win = if winner == Some(team) { h.round_win_bonus } else { 0 };
        pay(world, e, h.base_bank + win);
        world.entity_mut(e).insert(CaPaid(round));
    }
    let mut points: Vec<Vec3> = world
        .resource::<CaMapObjects>()
        .drops
        .iter()
        .filter(|d| d.family == h.family)
        .map(|d| d.at)
        .collect();
    objectives::shuffle(world, &mut points);
    let n = (h.drop_bullion_num as usize).min(points.len());
    for at in points.into_iter().take(n) {
        place(world, at, 1, h.bullion_amount, false);
    }
    world.resource_mut::<CaObjectives>().placed = n as u32;
}

/// Joining a live real round: the base pay.
fn update(world: &mut World) {
    if !world.resource::<CaMatch>().counting() {
        return;
    }
    let base = world.resource::<CaRuleSettings>().hired_guns.base_bank;
    let round = world.resource::<CaMatch>().round;
    for (e, _) in flow::players(world) {
        if world.get::<CaPaid>(e).is_none_or(|p| p.0 != round) {
            pay(world, e, base);
            world.entity_mut(e).insert(CaPaid(round));
        }
    }
}

/// Pick-ups, banking, the early end (after the tick's deaths).
fn touches(world: &mut World) {
    if !world.resource::<CaMatch>().counting() {
        world.resource_mut::<CaHiredGuns>().prev_use.clear();
        return;
    }
    let h = world.resource::<CaRuleSettings>().hired_guns.clone();
    let reach = objectives::units(h.pickup_reach);
    let half = objectives::player_half();
    let mut banked_now = false;
    for p in objectives::takers(world) {
        let pressed = {
            let mut g = world.resource_mut::<CaHiredGuns>();
            let before = g.prev_use.insert(p.e, p.intent.use_key).unwrap_or(false);
            p.intent.use_key && !before
        };
        // A bar by use: the nearest the view ray meets.
        if pressed {
            let eye = p.at + world.get::<MovementState>(p.e).map_or(Vec3::ZERO, |s| s.eye_offset);
            let dir = crate::weapon::aim_of(p.intent.yaw, p.intent.pitch) * Vec3::NEG_Z;
            let bar = objectives::units(BAR_HALF);
            let aimed = world
                .resource::<CaObjectives>()
                .bullion
                .iter()
                .filter(|b| !b.bag && b.at.distance(p.at) <= reach + 1e-5)
                .filter_map(|b| inventory::ray_box(eye, dir, b.at - Vec3::splat(bar), b.at + Vec3::splat(bar)).map(|d| (d, b.id)))
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, id)| id);
            if let Some(id) = aimed {
                take(world, p.e, id);
            }
        }
        // Bags by touch.
        let bag = objectives::units(BAG_HALF);
        let touched: Vec<u32> = world
            .resource::<CaObjectives>()
            .bullion
            .iter()
            .filter(|b| b.bag && b.at.distance(p.at) <= reach + 1e-5)
            .filter(|b| ObjBox { centre: b.at, half: Vec3::splat(bag) }.touches(p.at, half))
            .map(|b| b.id)
            .collect();
        for id in touched {
            take(world, p.e, id);
        }
        // The bank.
        let carry = objectives::carry(world, p.e);
        if carry.money > 0 {
            let in_bank = world
                .resource::<CaMapObjects>()
                .banks
                .iter()
                .any(|b| b.team.is_none_or(|t| t == p.team) && b.area.touches(p.at, half));
            if in_bank {
                bank(world, p.e, p.team, carry);
                banked_now = true;
            }
        }
    }
    count_pockets(world);
    if banked_now {
        let o = world.resource::<CaObjectives>();
        if o.banked[0] + o.banked[1] >= o.placed && o.bullion.is_empty() {
            let mut m = world.resource_mut::<CaMatch>();
            if !m.decided() {
                m.end_requested = true;
            }
        }
    }
}

/// `e` takes item `id` whole.
fn take(world: &mut World, e: Entity, id: u32) {
    let item = {
        let mut o = world.resource_mut::<CaObjectives>();
        let Some(i) = o.bullion.iter().position(|b| b.id == id) else { return };
        o.bullion.remove(i)
    };
    let mut c = objectives::carry(world, e);
    c.bars += item.bars;
    c.money += item.money;
    objectives::set_carry(world, e, c);
    weigh(world, e);
}

/// M11.5: the whole pocket into the bank.
fn bank(world: &mut World, e: Entity, team: u8, carry: objectives::CaCarry) {
    pay(world, e, carry.money);
    {
        let mut s = stats(world, e);
        s.mission_points += carry.money;
        s.bars_banked += carry.bars;
    }
    if let Some(i) = side(team) {
        world.resource_mut::<CaObjectives>().banked[i] += carry.bars;
    }
    let mut c = carry;
    c.bars = 0;
    c.money = 0;
    objectives::set_carry(world, e, c);
    weigh(world, e);
    info!("team {team} banked {} bars", carry.bars);
}

/// A death in any phase: the pocket drops as a bag in a real round's live
/// phase.
fn died(world: &mut World, d: &Died) {
    if world.get::<Npc>(d.entity).is_some() || !world.resource::<CaMatch>().counting() {
        return;
    }
    let carry = objectives::carry(world, d.entity);
    let at = world.get::<Transform>(d.entity).map_or(Vec3::ZERO, |t| t.translation);
    drop_pocket(world, Some(d.entity), at, carry);
}

/// M11.8's counts (live real rounds).
fn on_death(world: &mut World, d: &Death) {
    let h = world.resource::<CaRuleSettings>().hired_guns.clone();
    if let Some(i) = side(d.victim_team) {
        world.resource_mut::<CaMatch>().round_deaths[i] += 1;
    }
    stats(world, d.victim).deaths += 1;
    bonus(world, d.victim, h.death_bonus);
    match d.kind {
        DeathKind::Enemy => {
            if let Some(k) = d.killer {
                stats(world, k).kills += 1;
                bonus(world, k, h.kill_bonus);
            }
            if let Some(i) = side(enemy(d.victim_team)) {
                world.resource_mut::<CaMatch>().round_kills[i] += 1;
            }
        }
        DeathKind::Suicide => stats(world, d.victim).suicides += 1,
        DeathKind::TeamKill => {
            {
                let mut s = stats(world, d.victim);
                s.deaths = s.deaths.saturating_sub(1);
            }
            if let Some(k) = d.killer {
                stats(world, k).friendly_kills += 1;
            }
        }
        DeathKind::NoKiller => {}
    }
}

/// A mercenary killed by player `killer` (M11.7): an enemy kill, the kill
/// bonus, a round kill for its team (live real rounds).
pub fn merc_killed(world: &mut World, killer: Entity, _head: bool) {
    if !world.resource::<CaMatch>().counting() || super::current_mode(world).is_none_or(|m| m.key != MODE.key) {
        return;
    }
    let kill_bonus = world.resource::<CaRuleSettings>().hired_guns.kill_bonus;
    stats(world, killer).kills += 1;
    bonus(world, killer, kill_bonus);
    super::scoring::sync_score(world, killer);
    let team = world.get::<crate::core::Team>(killer).map_or(0, |t| t.0);
    if let Some(i) = side(team) {
        world.resource_mut::<CaMatch>().round_kills[i] += 1;
    }
}

/// Alpha when `a` is greater, Bravo when `b` is, None when equal.
fn pick(a: i64, b: i64) -> Option<u8> {
    super::ctf::pick(a, b)
}

/// More banked bars; more round kills; fewer round deaths; else none.
fn time_out(world: &mut World) {
    let banked = world.resource::<CaObjectives>().banked;
    let mut m = world.resource_mut::<CaMatch>();
    m.winner = pick(banked[0] as i64, banked[1] as i64)
        .or_else(|| pick(m.round_kills[0] as i64, m.round_kills[1] as i64))
        .or_else(|| pick(m.round_deaths[1] as i64, m.round_deaths[0] as i64));
}

/// The round over: the winners' bonus, every item and AI removed.
fn round_over(world: &mut World) {
    let (winner, warmup) = {
        let m = world.resource::<CaMatch>();
        (m.winner, m.warmup)
    };
    world.resource_mut::<CaHiredGuns>().last_winner = if warmup { None } else { winner };
    if !warmup && let Some(w) = winner {
        let bonus_amount = world.resource::<CaRuleSettings>().hired_guns.round_win_bonus;
        for (e, t) in flow::players(world) {
            if t == w {
                stats(world, e).mission_points += bonus_amount;
            }
        }
    }
    world.resource_mut::<CaObjectives>().bullion.clear();
    mercs::remove_all(world);
}

/// After real round G, or a team empty: the other team; more round wins;
/// else Bravo.
fn match_end(world: &World) -> Option<Option<u8>> {
    let goal = world.resource::<CaRuleSettings>().goal();
    let m = world.resource::<CaMatch>();
    let counts = world.resource::<super::CaTeamCounts>().0;
    let empty = counts.contains(&0);
    if !empty && m.round < goal {
        return None;
    }
    let winner = if counts[0] == 0 && counts[1] > 0 {
        BRAVO
    } else if counts[1] == 0 && counts[0] > 0 {
        ALPHA
    } else if m.round_wins[0] > m.round_wins[1] {
        ALPHA
    } else {
        BRAVO
    };
    Some(Some(winner))
}

/// M11.9: the last of a team asks for the round's end; a living leaver
/// with no teammate alive gives the round away.
fn leaving(m: &mut CaMatch, l: &Leaver) {
    if m.phase != Phase::Live || m.warmup || m.decided() {
        return;
    }
    if l.team_players <= 1 {
        m.end_requested = true;
    } else if l.alive && l.team_alive <= 1 {
        m.win(enemy(l.team));
    }
}

/// A leaver's pocket drops (any phase) and its mercenaries go.
fn left(world: &mut World, l: &LeftPlayer) {
    mercs::remove_of(world, l.entity);
    drop_pocket(world, None, l.at, l.carry);
    world.resource_mut::<CaHiredGuns>().prev_use.remove(&l.entity);
}
