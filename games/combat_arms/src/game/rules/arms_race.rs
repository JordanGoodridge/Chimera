//! Arms Race (`combat_arms:arms_race`, room key `VWRM`;
//! specs/combat_arms/game_modes.md M14, missions.md F4.8): One Man Army
//! (M5, the `free_for_all` rules) with a weapon ladder; first to G kills
//! wins. No death drops (inventory.md 12); firing ends spawn protection.
//!
//! - The ladders (`missions::ladders`): the mission's own `Weapon_Step_Table`
//!   whose `Goal` is G, else its mode default file's (`GMS\VWRM.GMS`: every
//!   stock map), else our data's stock copy (data/modes.toml); none for a
//!   G no table names (every player keeps their own weapons).
//! - When it changes: at spawn (the step in force for the kill count) and
//!   each time a player's enemy kills equal a step's `Kill` exactly. A roll
//!   r in 0..=100: below the default ladder's `Percent` (80) the step's
//!   weapon (a weighted pick q in 0..=W among its candidates), drawn, in
//!   place of the player's weapon of its class; else the player keeps
//!   their own equipped weapon of that class (`missions::ladder_pick`).
//!   The weapon ids are the install's weapon records (V1), else our
//!   stand-ins (data/modes.toml `stand_ins`).
//! - `CaLadder` keeps each player's next step's kills (None: the last
//!   stage).
//!
//! Known shortcuts: no function-item ladders (no shop items are owned);
//! the "last stage" announcement and the next step are kept, not shown.

use bevy::prelude::*;

use super::{
    super::missions, ARMS_RACE, CaMode, CaStats, Hooks, NO_HOOKS, free_for_all, objectives,
    state::{Death, DeathKind},
};
use crate::weapon::{Inventory, Weapon};

/// The mode.
pub const MODE: CaMode = CaMode {
    rules: super::Mode {
        id: ARMS_RACE,
        key: "VWRM",
        shown_as: "Arms Race",
        description: "Arms Race: One Man Army with a weapon ladder (a step's weapon at its kill count), first to G kills wins",
        on_death,
        hooks: Hooks { update, ..NO_HOOKS },
        ..free_for_all::MODE.rules
    },
    death_drops: false,
    ..free_for_all::MODE
};

/// A player's place on the ladder: the weapon the ladder gave (and the
/// own weapon it replaced), the life it was given in, the next step's
/// kills.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct CaLadder {
    pub given: Option<&'static str>,
    pub replaced: Vec<&'static str>,
    /// The weapon entity the step was rolled for (a respawn rebuilds the
    /// loadout: a new first weapon).
    pub life: Option<Entity>,
    pub next: Option<i32>,
    pub last_stage: bool,
}

fn on_death(world: &mut World, d: &Death) {
    super::scoring::free_for_all(world, d);
    if d.kind != DeathKind::Enemy {
        return;
    }
    let Some(k) = d.killer else { return };
    free_for_all::test_win(world, k);
    let kills = world.get::<CaStats>(k).map_or(0, |s| s.kills) as i32;
    step(world, k, kills, true);
}

/// Roll `e`'s weapon for the step at `kills` (`exact`: only a step
/// starting there; else the one in force).
fn step(world: &mut World, e: Entity, kills: i32, exact: bool) {
    let ladders = missions::ladders(world);
    let Some(default) = ladders.iter().find(|t| t.item_no == -1).cloned() else { return };
    let at = if exact { default.step_at(kills) } else { default.step_for(kills) };
    let Some(s) = at.cloned() else { return };
    let r = objectives::draw(world, 101) as i32;
    let q = objectives::draw(world, (s.total() + 1).max(1) as usize) as i32;
    let pick = missions::ladder_pick(&ladders, &[], kills, r, q, exact).and_then(|id| missions::ladder_weapon(world, id));
    let mut ladder = world.get::<CaLadder>(e).cloned().unwrap_or_default();
    if let Some(old) = ladder.given.take() {
        objectives::take_weapon(world, e, old);
    }
    match pick {
        Some(id) => {
            let slot = world.resource::<crate::weapon::data::WeaponCatalog>().get(id).map(|d| d.slot);
            let own: Vec<&'static str> = world
                .get::<Inventory>(e)
                .map(|i| {
                    i.weapons
                        .iter()
                        .filter_map(|w| world.get::<Weapon>(*w))
                        .filter(|w| Some(w.slot) == slot && w.id != id)
                        .map(|w| w.id)
                        .collect()
                })
                .unwrap_or_default();
            for o in &own {
                objectives::take_weapon(world, e, o);
            }
            ladder.replaced.extend(own);
            // A weapon the player already owns stays theirs.
            let owned = objectives::has_weapon(world, e, id).is_some();
            if let Some(w) = objectives::give_weapon(world, e, id)
                && let Some(mut inv) = world.get_mut::<Inventory>(e)
            {
                inv.wanted = Some(w);
            }
            ladder.given = (!owned).then_some(id);
        }
        None => {
            // The player's own weapon of that class back.
            for o in std::mem::take(&mut ladder.replaced) {
                objectives::give_weapon(world, e, o);
            }
        }
    }
    // This life's mark: a weapon entity a respawn would despawn.
    ladder.life = ladder
        .given
        .and_then(|g| objectives::has_weapon(world, e, g))
        .or_else(|| world.get::<Inventory>(e).and_then(|i| i.weapons.first().copied()));
    ladder.next = default.next_kill(kills);
    ladder.last_stage = ladder.next.is_none();
    world.entity_mut(e).insert(ladder);
}

/// At each spawn (a rebuilt loadout), the step in force.
fn update(world: &mut World) {
    let players: Vec<Entity> = super::flow::players(world).into_iter().map(|(e, _)| e).collect();
    for e in players {
        if world.get::<Inventory>(e).is_none_or(|i| i.weapons.is_empty()) {
            continue;
        }
        let fresh = world
            .get::<CaLadder>(e)
            .is_none_or(|l| l.life.is_none_or(|w| world.get::<Weapon>(w).is_none()));
        if fresh {
            if let Some(mut l) = world.get_mut::<CaLadder>(e) {
                l.given = None;
                l.replaced.clear();
            }
            let kills = world.get::<CaStats>(e).map_or(0, |s| s.kills) as i32;
            step(world, e, kills, false);
        }
    }
}
