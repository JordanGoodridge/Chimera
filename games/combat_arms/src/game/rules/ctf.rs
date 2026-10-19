//! Capture the Flag (`combat_arms:capture_the_flag`, room key `CTF`;
//! specs/combat_arms/game_modes.md M4): Alpha against Bravo, respawning,
//! one round of `TimeLimit` x G (180 x G s); the first team to G captures
//! wins.
//!
//! - Objects: one flag per team (`FlagItem`, `GameType`
//!   `Capturetheflag`: its home and its (10, 25, 10) box) and one flag
//!   zone per team (`FlagZoneTrigger`) at the flag's base
//!   (`objects::CaMapObjects`). The flags' state is
//!   `objectives::CaObjectives::flags`, every machine's.
//! - Taking: a living player whose box touches the enemy flag (at home or
//!   dropped) takes it (`objectives::CaCarry::flag`); the flag leaves the
//!   world while carried.
//! - Returning: a living player touching their own flag while it is
//!   dropped sends it home at once: team returns +1, player returns +1.
//!   Touching your own flag at home does nothing.
//! - Dropping: a carrier's death drops it where the carrier was, moved
//!   down to the floor, going home after `flag_return` (30 s) untouched,
//!   no credit; a carrier killed by a kill zone (a volume's damage, which
//!   has no attacker, or a telefrag's crush) sends it home at once; a
//!   carrier who leaves drops it where they last were.
//! - Capture: a carrier of the enemy flag touching their own team's flag
//!   zone while their own flag is home, in the live phase: team captures
//!   (the team counter, `CaMatch::score`) +1, player captures +1, the
//!   carried flag home. The capture that brings a team to G wins.
//! - Deaths (S7's CTF column): enemy kill, killer kills +1, victim deaths
//!   +1, the killer's team's kills +1; team kill, killer friendly kills
//!   +1, victim deaths +1, the enemy team's kills +1; own damage, victim
//!   deaths, suicides and friendly kills +1, the enemy team's kills +1; no
//!   killer, victim deaths and suicides +1; every death, the victim's
//!   team's deaths +1 (`CaMatch::kills`, `deaths`: tie-breaks only).
//! - Time-out: more captures; else more returns; else more team kills;
//!   else fewer deaths; else a draw.
//! - Leaving: the last player of a team leaving gives the round to the
//!   other team at once. Spawns: the `Capturetheflag` points per team.
//! - The flags' props (`props`: the maps' `FlagItem` models) are shown
//!   only in capture the flag, at home or where dropped, hidden while
//!   carried (`show_flags`, every machine).
//!
//! Known shortcuts: a carried flag isn't drawn on the carrier's back (no
//! flag socket model is placed); taking and returning work in every phase
//! (only a capture is tied to the live phase); no assists (the mode isn't
//! in S7's assist list); no late-join closing.

use bevy::prelude::*;

use super::{
    CAPTURE_THE_FLAG, CaMode, CaRuleSettings, Hooks, NO_HOOKS, Panel, RoundTime, ScoreOrder, Spawns, Teams,
    objectives::{self, CaCarry, CaObjectives, Flag, FlagAt},
    objects::CaMapObjects,
    scoring::{personal, stats},
    state::{CaMatch, Death, DeathKind, enemy, side},
};
use crate::core::{DamageKind, Died, SimClock};

/// The flags' and points' game type.
pub const FAMILY: &str = "Capturetheflag";

/// The mode.
pub const MODE: CaMode = CaMode {
    module: CAPTURE_THE_FLAG,
    key: "CTF",
    shown_as: "Capture the Flag",
    description: "Capture the Flag: Alpha against Bravo, respawning; take the enemy flag to your zone while yours is \
                  home, the first team to G captures wins",
    respawns: true,
    warmup: false,
    warmup_revives: false,
    ff_scaled: true,
    assists: false,
    timer_shown: true,
    forced_setup: None,
    teams: Teams::Two,
    spawns: Spawns {
        family: Some(FAMILY),
        one_list: false,
        shuffled: false,
        locked_rounds: false,
    },
    round_time: RoundTime::MissionTimesGoal,
    fire_ends_protection: false,
    death_drops: true,
    panel: Panel::TeamScore,
    order: ScoreOrder::Captures,
    on_death,
    time_out,
    match_end: |w| Some(w.resource::<CaMatch>().winner),
    leaving: super::tdm::leaving,
    hooks: Hooks {
        setup,
        update,
        died,
        round_end: |w| {
            let carriers = w.resource::<CaFlagCarriers>().0;
            for (e, _) in carriers.into_iter().flatten() {
                let mut c = objectives::carry(w, e);
                c.flag = None;
                objectives::set_carry(w, e, c);
            }
            *w.resource_mut::<CaFlagCarriers>() = CaFlagCarriers::default();
        },
        ..NO_HOOKS
    },
};

/// Who carries each team's flag (Alpha's, Bravo's) and where they last
/// were alive (the server's).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct CaFlagCarriers(pub [Option<(Entity, Vec3)>; 2]);

/// The flags home at the round's setup.
fn setup(world: &mut World) {
    let objects = world.resource::<CaMapObjects>();
    let of_family: Vec<_> = objects.flags.iter().filter(|f| f.family == FAMILY).cloned().collect();
    let list = if of_family.is_empty() { objects.flags.clone() } else { of_family };
    let mut flags: Vec<Flag> = Vec::new();
    for f in list {
        if flags.iter().any(|x| x.team == f.team) {
            continue;
        }
        flags.push(Flag {
            team: f.team,
            home: f.area.centre,
            at: FlagAt::Home,
        });
    }
    flags.sort_by_key(|f| f.team);
    world.resource_mut::<CaObjectives>().flags = flags;
    *world.resource_mut::<CaFlagCarriers>() = CaFlagCarriers::default();
}

/// The flag's box half-extents (its object's, else the default).
fn flag_half(world: &World, team: u8) -> Vec3 {
    world
        .resource::<CaMapObjects>()
        .flags
        .iter()
        .find(|f| f.team == team)
        .map_or(super::objects::FLAG_DIMS * super::super::inventory::UNIT, |f| f.area.half)
}

/// Send `team`'s flag home.
fn send_home(world: &mut World, team: u8) {
    let mut o = world.resource_mut::<CaObjectives>();
    if let Some(f) = o.flags.iter_mut().find(|f| f.team == team) {
        f.at = FlagAt::Home;
    }
}

/// Drop `team`'s flag at `at` (moved down to the floor), or send it home
/// when `home`.
fn drop_flag(world: &mut World, team: u8, at: Vec3, home: bool) {
    if let Some(i) = side(team) {
        world.resource_mut::<CaFlagCarriers>().0[i] = None;
    }
    if home {
        send_home(world, team);
        return;
    }
    let floor = objectives::floor_below(world, at);
    let returns = world.resource::<SimClock>().now + world.resource::<CaRuleSettings>().objectives.flag_return as f64;
    let mut o = world.resource_mut::<CaObjectives>();
    if let Some(f) = o.flags.iter_mut().find(|f| f.team == team) {
        f.at = FlagAt::Dropped { at: floor, returns };
    }
}

/// A carrier's death drops the flag (a kill zone sends it home).
fn died(world: &mut World, d: &Died) {
    let c = objectives::carry(world, d.entity);
    let Some(team) = c.flag else { return };
    let kill_zone = (d.damage.kind == DamageKind::Generic && d.attacker.is_none())
        || (d.damage.kind == DamageKind::Crush && d.damage.weapon.is_some());
    let at = world.get::<Transform>(d.entity).map_or(Vec3::ZERO, |t| t.translation);
    world.entity_mut(d.entity).remove::<CaCarry>();
    drop_flag(world, team, at, kill_zone);
}

/// Carriers, return timers, touches and captures.
fn update(world: &mut World) {
    let now = world.resource::<SimClock>().now;
    // Carriers: their last living place; one gone (left) drops the flag.
    let carriers = world.resource::<CaFlagCarriers>().0;
    for (i, c) in carriers.iter().enumerate() {
        let Some((e, last)) = *c else { continue };
        let team = i as u8 + 1;
        match world.get::<Transform>(e) {
            Some(t) if world.get::<crate::core::Dead>(e).is_none() => {
                let at = t.translation;
                world.resource_mut::<CaFlagCarriers>().0[i] = Some((e, at));
            }
            Some(_) => {}
            None => drop_flag(world, team, last, false),
        }
    }
    // Return timers.
    let expired: Vec<u8> = world
        .resource::<CaObjectives>()
        .flags
        .iter()
        .filter(|f| matches!(f.at, FlagAt::Dropped { returns, .. } if now + 1e-9 >= returns))
        .map(|f| f.team)
        .collect();
    for team in expired {
        send_home(world, team);
    }
    // Touches.
    let half = objectives::player_half();
    for p in objectives::living(world) {
        let flags = world.resource::<CaObjectives>().flags.clone();
        let carry = objectives::carry(world, p.e);
        for f in &flags {
            let at = match f.at {
                FlagAt::Home => f.home,
                FlagAt::Dropped { at, .. } => at,
                FlagAt::Carried => continue,
            };
            let area = super::objects::ObjBox {
                centre: at,
                half: flag_half(world, f.team),
            };
            if !area.touches(p.at, half) {
                continue;
            }
            if f.team != p.team && carry.flag.is_none() {
                take(world, p.e, p.team, f.team, p.at);
                break;
            }
            if f.team == p.team && matches!(f.at, FlagAt::Dropped { .. }) {
                send_home(world, f.team);
                if let Some(i) = side(p.team) {
                    world.resource_mut::<CaMatch>().returns[i] += 1;
                }
                stats(world, p.e).returns += 1;
            }
        }
        capture(world, p.e, p.team, p.at);
    }
}

/// `e` (on `team`) takes `flag`'s flag.
fn take(world: &mut World, e: Entity, _team: u8, flag: u8, at: Vec3) {
    {
        let mut o = world.resource_mut::<CaObjectives>();
        let Some(f) = o.flags.iter_mut().find(|f| f.team == flag) else { return };
        f.at = FlagAt::Carried;
    }
    if let Some(i) = side(flag) {
        world.resource_mut::<CaFlagCarriers>().0[i] = Some((e, at));
    }
    let mut c = objectives::carry(world, e);
    c.flag = Some(flag);
    objectives::set_carry(world, e, c);
}

/// `e` (on `team`), carrying the enemy flag, at its own flag zone with its
/// own flag home, captures.
fn capture(world: &mut World, e: Entity, team: u8, at: Vec3) {
    let c = objectives::carry(world, e);
    let Some(flag) = c.flag else { return };
    if flag == team || !world.resource::<CaMatch>().counting() {
        return;
    }
    let own_home = world
        .resource::<CaObjectives>()
        .flag(team)
        .is_none_or(|f| f.at == FlagAt::Home);
    let half = objectives::player_half();
    let in_zone = world
        .resource::<CaMapObjects>()
        .flag_zones
        .iter()
        .any(|z| z.team == team && z.area.touches(at, half));
    if !own_home || !in_zone {
        return;
    }
    let goal = world.resource::<CaRuleSettings>().goal() as i32;
    let mut cleared = c;
    cleared.flag = None;
    objectives::set_carry(world, e, cleared);
    if let Some(i) = side(flag) {
        world.resource_mut::<CaFlagCarriers>().0[i] = None;
    }
    send_home(world, flag);
    stats(world, e).captures += 1;
    let Some(i) = side(team) else { return };
    let mut m = world.resource_mut::<CaMatch>();
    m.score[i] += 1;
    if m.score[i] >= goal && m.winner.is_none() {
        m.win(team);
    }
    info!("flag captured by team {team}: {:?}", m.score);
}

fn on_death(world: &mut World, d: &Death) {
    personal(world, d);
    let scorer = match d.kind {
        DeathKind::Enemy => Some(enemy(d.victim_team)),
        DeathKind::TeamKill | DeathKind::Suicide => Some(enemy(d.victim_team)),
        DeathKind::NoKiller => None,
    };
    let mut m = world.resource_mut::<CaMatch>();
    if let Some(i) = scorer.and_then(side) {
        m.kills[i] += 1;
    }
    if let Some(i) = side(d.victim_team) {
        m.deaths[i] += 1;
    }
}

/// More captures; more returns; more kills; fewer deaths; else none.
fn time_out(world: &mut World) {
    let mut m = world.resource_mut::<CaMatch>();
    m.winner = pick(m.score[0] as i64, m.score[1] as i64)
        .or_else(|| pick(m.returns[0] as i64, m.returns[1] as i64))
        .or_else(|| pick(m.kills[0] as i64, m.kills[1] as i64))
        .or_else(|| pick(m.deaths[1] as i64, m.deaths[0] as i64));
}

/// Alpha when `a` is greater, Bravo when `b` is, None when equal.
pub fn pick(a: i64, b: i64) -> Option<u8> {
    match a.cmp(&b) {
        std::cmp::Ordering::Greater => Some(super::state::ALPHA),
        std::cmp::Ordering::Less => Some(super::state::BRAVO),
        std::cmp::Ordering::Equal => None,
    }
}

/// The flags' props: shown in capture the flag only, at home or where
/// dropped, hidden while carried (every machine, from the objectives it
/// sees).
pub fn show_flags(
    view: Res<super::CaMatchView>,
    objectives: Res<CaObjectives>,
    entities: Option<Res<crate::map::MapEntities>>,
    mut props: Query<(&crate::map::PropEntity, &crate::map::PropHome, &mut Transform, &mut Visibility)>,
) {
    let Some(entities) = entities else { return };
    let ctf = view.mode == MODE.key;
    for (index, home, mut t, mut vis) in &mut props {
        let Some(e) = entities.entities.get(index.0) else { continue };
        if e.classname() != "FlagItem" {
            continue;
        }
        let team = e.get("Teams").and_then(super::objects::team_named);
        let flag = team.and_then(|t| objectives.flag(t));
        let (shown, at) = match flag.map(|f| f.at) {
            _ if !ctf => (false, home.0.translation),
            Some(FlagAt::Carried) => (false, home.0.translation),
            Some(FlagAt::Dropped { at, .. }) => (true, at),
            _ => (true, home.0.translation),
        };
        let want = if shown { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if t.translation != at {
            t.translation = at;
        }
    }
}
