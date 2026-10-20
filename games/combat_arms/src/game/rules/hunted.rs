//! Hunted (`combat_arms:hunted`, room key `HG`;
//! specs/combat_arms/game_modes.md M15): Last Man Standing (M6, the
//! `last_man` rules) with its own spawns: the unlocked `HungerGame` points
//! in the warm-up, the locked ones in the rounds (one list each, shuffled
//! once a map load). Firing ends spawn protection.
//!
//! - Start (Open question 36): every player spawns holding only weapon 934
//!   (`fist`, the install's record with the V1 key; our knife stands in
//!   without it), with its normal ammunition (`CaFistKit`).
//! - Time hunted: each living player's time in a round's Live phase (until
//!   death, the time-out or the round's end) adds to `CaStats::hunted_ms`;
//!   the scoreboard ranks by mission points, then time hunted, then kills
//!   (`ScoreOrder::Hunted`).
//! - Rounds: Dark Hills' mission (270 s after a 20 s warm-up), else our
//!   data's same numbers.
//!
//! Known shortcuts: no scavenging (the mission's map item tables and the
//! map's `RandomMapItem` commands aren't run: no supply drops), no emotes
//! (characters.md 9), no system scan; the death camera's `DyingTime` hold
//! (3.5 s, a client's) only matters before a respawn, which Hunted's
//! rounds don't have.

use bevy::prelude::*;

use super::{
    CaMode, CaStats, HUNTED, Hooks, ScoreOrder, Spawns, last_man, objectives,
    state::{CaMatch, Phase},
};
use crate::core::{Dead, Health, SimClock};

/// The fist's weapon record (the start loadout).
pub const FIST: i32 = 934;

/// The mode.
pub const MODE: CaMode = CaMode {
    module: HUNTED,
    key: "HG",
    shown_as: "Hunted",
    description: "Hunted: Last Man Standing on the HungerGame points (unlocked in the warm-up, locked in rounds), \
                  starting with fists",
    spawns: Spawns {
        family: Some("HungerGame"),
        one_list: true,
        shuffled: true,
        locked_rounds: true,
    },
    order: ScoreOrder::Hunted,
    hooks: Hooks {
        setup,
        update,
        ..last_man::MODE.hooks
    },
    ..last_man::MODE
};

/// A player whose loadout becomes the fist once it is there.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct CaFistKit;

/// The start weapon: the install's fist, else our knife.
pub fn start_weapon() -> &'static str {
    super::super::data::weapon_for_index(FIST).unwrap_or(super::super::inventory::KNIFE)
}

fn setup(world: &mut World) {
    for (e, _) in super::flow::players(world) {
        world.entity_mut(e).insert(CaFistKit);
    }
}

fn update(world: &mut World) {
    // The fist only, once the loadout is there.
    let kits: Vec<Entity> = world.query_filtered::<Entity, With<CaFistKit>>().iter(world).collect();
    let fist = start_weapon();
    for e in kits {
        let ids: Vec<&'static str> = world
            .get::<crate::weapon::Inventory>(e)
            .map(|i| i.weapons.iter().filter_map(|w| world.get::<crate::weapon::Weapon>(*w).map(|w| w.id)).collect())
            .unwrap_or_default();
        if ids.is_empty() {
            continue;
        }
        if let Some(w) = objectives::give_weapon(world, e, fist)
            && let Some(mut inv) = world.get_mut::<crate::weapon::Inventory>(e)
        {
            inv.wanted = Some(w);
        }
        for id in ids.into_iter().filter(|id| *id != fist) {
            objectives::take_weapon(world, e, id);
        }
        world.entity_mut(e).remove::<CaFistKit>();
    }
    // Time hunted, in the Live phase of real rounds.
    let m = world.resource::<CaMatch>();
    if m.phase != Phase::Live || m.warmup {
        return;
    }
    let ms = (world.resource::<SimClock>().dt() * 1000.0).round() as u32;
    let living: Vec<Entity> = super::flow::players(world)
        .into_iter()
        .map(|(e, _)| e)
        .filter(|e| world.get::<Dead>(*e).is_none() && world.get::<Health>(*e).is_some_and(|h| h.current > 0.0))
        .collect();
    for e in living {
        if let Some(mut s) = world.get_mut::<CaStats>(e) {
            s.hunted_ms += ms;
        }
    }
}
