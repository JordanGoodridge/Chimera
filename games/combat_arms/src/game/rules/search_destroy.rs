//! Search & Destroy (`combat_arms:search_destroy`, room key `TMM`;
//! specs/combat_arms/game_modes.md M3): JSF (Alpha, `OffenceTeam`)
//! attacks with bombs, Creed (Bravo, `DefenceTeam`) defends two bomb
//! sites; rounds without respawn after a warm-up; G round wins take the
//! match.
//!
//! - Bombs: at each real round's setup every living JSF player gets the
//!   mission bomb (`objectives::BOMB`, class 6; once its loadout is
//!   there); never dropped on death; leftover bombs go at the round's end.
//!   No bombs in the warm-up (S9).
//! - Sites: the map's `BombZoneTrigger`s of `GameType` `Doobic` (`bomb`:
//!   plant, defuse, fuse).
//! - Deaths (S7's S&D column): enemy kill, killer kills +1 and victim
//!   deaths +1; team kill, killer friendly kills +1 (no death, Quirk 3);
//!   own damage, victim deaths and suicides +1; no killer, victim deaths
//!   +1. Assists as S7.
//! - Round win by elimination: a death leaving Creed with nobody alive
//!   gives the round to JSF, even with the bomb planted; one leaving JSF
//!   with nobody alive gives it to Creed only if no bomb is ticking; one
//!   leaving both empty to the victim's enemy team. A defuse wins for
//!   Creed, the detonation for JSF.
//! - Time-out (`TimeLimit` 180 s, a plant adds the fuse): Creed wins.
//! - Match end, after each real round: a team has G round wins, or a team
//!   is empty; the winner more round wins, else Bravo (Quirk 4, test case
//!   42).
//! - Leaving: a living leaver who was the last of its team alive gives the
//!   round to the other team (as Elimination Pro). Joiners are held while
//!   a bomb ticks (S1).
//! - Spawns: the `Doobic` points per team. Statistics: plants, detonations
//!   and defuses.
//!
//! Known shortcuts: sides never swap (as the stock files); the bomb's
//! hand-out waits for the loadout (the tick after the setup's respawn).

use bevy::prelude::*;

use super::{
    CaMode, CaRuleSettings, Hooks, NO_HOOKS, Panel, RoundTime, SEARCH_DESTROY, ScoreOrder, Spawns, Teams,
    bomb,
    flow::alive,
    objectives::{self, BOMB},
    scoring::stats,
    state::{ALPHA, BRAVO, CaMatch, Death, DeathKind, enemy},
};

/// The bomb sites' and points' game type.
pub const FAMILY: &str = "Doobic";
/// The attacking and defending teams (`OffenceTeam` JSF, `DefenceTeam`
/// Creed in every stock file).
pub const OFFENCE: u8 = ALPHA;
pub const DEFENCE: u8 = BRAVO;

/// The mode.
pub const MODE: CaMode = CaMode {
    module: SEARCH_DESTROY,
    key: "TMM",
    shown_as: "Search & Destroy",
    description: "Search & Destroy: JSF plants a bomb at one of two sites, Creed defends; rounds without respawn after a \
                  warm-up, G round wins take the match",
    respawns: false,
    warmup: true,
    warmup_revives: true,
    ff_scaled: true,
    assists: true,
    timer_shown: true,
    forced_setup: None,
    teams: Teams::Two,
    spawns: Spawns {
        family: Some(FAMILY),
        one_list: false,
        shuffled: false,
        locked_rounds: false,
    },
    round_time: RoundTime::Mission,
    fire_ends_protection: false,
    death_drops: true,
    panel: Panel::RoundWins,
    order: ScoreOrder::Kills,
    on_death,
    time_out: |w| w.resource_mut::<CaMatch>().winner = Some(DEFENCE),
    match_end,
    leaving: super::elimination::leaving,
    hooks: HOOKS,
};

/// S&D's hooks (Go4CA's too).
pub const HOOKS: Hooks = Hooks {
    setup,
    update,
    round_end: bomb::remove_all,
    holds_joiner: bomb::ticking,
    ..NO_HOOKS
};

/// A player waiting for the bomb (its loadout comes first).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct CaBombDue;

/// A real round's setup: every living JSF player is due a bomb.
fn setup(world: &mut World) {
    world.resource_mut::<bomb::CaPlanter>().0 = None;
    if world.resource::<CaMatch>().warmup {
        return;
    }
    for p in objectives::takers(world) {
        if p.team == OFFENCE {
            world.entity_mut(p.e).insert(CaBombDue);
        }
    }
}

/// The bombs handed out, then plants, defuses and the fuse.
fn update(world: &mut World) {
    let due: Vec<Entity> = world.query_filtered::<Entity, With<CaBombDue>>().iter(world).collect();
    for e in due {
        let ready = world
            .get::<crate::weapon::Inventory>(e)
            .is_some_and(|i| !i.weapons.is_empty());
        if ready {
            objectives::give_weapon(world, e, BOMB);
            world.entity_mut(e).remove::<CaBombDue>();
        }
    }
    bomb::update(world, FAMILY);
}

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
        }
        DeathKind::Suicide => {
            let mut s = stats(world, d.victim);
            s.deaths += 1;
            s.suicides += 1;
        }
        DeathKind::NoKiller => stats(world, d.victim).deaths += 1,
    }
    if world.resource::<CaMatch>().winner.is_some() {
        return;
    }
    let (a, b) = (alive(world, OFFENCE), alive(world, DEFENCE));
    let winner = match (a, b) {
        (0, 0) => Some(enemy(d.victim_team)),
        (_, 0) => Some(OFFENCE),
        (0, _) if !bomb::ticking(world) => Some(DEFENCE),
        _ => None,
    };
    if let Some(w) = winner {
        world.resource_mut::<CaMatch>().win(w);
    }
}

/// G round wins or an empty team; more round wins, else Bravo.
pub fn match_end(world: &World) -> Option<Option<u8>> {
    let goal = world.resource::<CaRuleSettings>().goal();
    let m = world.resource::<CaMatch>();
    let counts = world.resource::<super::CaTeamCounts>().0;
    if !counts.contains(&0) && m.round_wins.iter().all(|w| *w < goal) {
        return None;
    }
    Some(Some(if m.round_wins[0] > m.round_wins[1] { ALPHA } else { BRAVO }))
}
