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
//! (characters.md 9), no system scan. The death camera's hold, pull-back
//! and free cameras are the client's (`game::death_cam`; its hold rides
//! the body's place, not the `Eyes` socket).

use bevy::prelude::*;

use super::{
    CaMode, CaStats, HUNTED, Hooks, ScoreOrder, Spawns, last_man,
    objectives::{self, Roster},
    state::{CaMatch, Phase},
};
use crate::{
    core::SimClock,
    weapon::{Inventory, Weapon},
};

/// The fist's weapon record (the start loadout).
pub const FIST: i32 = 934;

/// The mode.
pub const MODE: CaMode = CaMode {
    rules: super::Mode {
        id: HUNTED,
        key: "HG",
        shown_as: "Hunted",
        description: "Hunted: Last Man Standing on the HungerGame points (unlocked in the warm-up, locked in rounds), \
                      starting with fists",
        hooks: Hooks {
            setup,
            ..last_man::MODE.rules.hooks
        },
        ..last_man::MODE.rules
    },
    spawns: Spawns {
        family: Some("HungerGame"),
        one_list: true,
        shuffled: true,
        locked_rounds: true,
    },
    order: ScoreOrder::Hunted,
    ..last_man::MODE
};

/// A player whose loadout becomes the fist once it is there.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct CaFistKit;

/// The start weapon: the install's fist, else our knife.
pub fn start_weapon(data: &super::super::data::CaDataInner) -> &'static str {
    data.weapon_for_index(FIST).unwrap_or(super::super::inventory::KNIFE)
}

fn setup(world: &mut World) {
    for (e, _) in super::flow::players(world) {
        world.entity_mut(e).insert(CaFistKit);
    }
}

/// The mode's per-tick system (`rules::mode_updates`): a fist kit's
/// loadout becomes the fist once it is there (giving and taking weapons:
/// world work, queued); in the live phase of real rounds, the living's
/// time hunted.
pub(super) fn update(
    kits: Query<(Entity, Option<&Inventory>), With<CaFistKit>>,
    weapons: Query<&Weapon>,
    data: Res<super::super::data::CaData>,
    m: Res<CaMatch>,
    clock: Res<SimClock>,
    roster: Roster,
    mut stats: Query<&mut CaStats>,
    mut commands: Commands,
) {
    // The fist only, once the loadout is there.
    let fist = start_weapon(&data);
    for (e, inv) in &kits {
        let ids: Vec<&'static str> = inv
            .map(|i| i.weapons.iter().filter_map(|w| weapons.get(*w).ok().map(|w| w.id)).collect())
            .unwrap_or_default();
        if ids.is_empty() {
            continue;
        }
        commands.queue(move |world: &mut World| {
            if let Some(w) = objectives::give_weapon(world, e, fist)
                && let Some(mut inv) = world.get_mut::<Inventory>(e)
            {
                inv.wanted = Some(w);
            }
            for id in ids.into_iter().filter(|id| *id != fist) {
                objectives::take_weapon(world, e, id);
            }
            world.entity_mut(e).remove::<CaFistKit>();
        });
    }
    // Time hunted, in the Live phase of real rounds.
    if m.phase != Phase::Live || m.warmup {
        return;
    }
    let ms = (clock.dt() * 1000.0).round() as u32;
    for (e, _) in roster.players() {
        if roster.breathing(e)
            && let Ok(mut s) = stats.get_mut(e)
        {
            s.hunted_ms += ms;
        }
    }
}
