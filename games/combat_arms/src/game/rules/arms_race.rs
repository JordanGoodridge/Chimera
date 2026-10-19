//! Arms Race (`combat_arms:arms_race`, room key `VWRM`;
//! specs/combat_arms/game_modes.md M14): One Man Army (M5, the
//! `free_for_all` rules) with a weapon ladder: every `ladder_kills` kills a
//! player's weapon is replaced by the next of the ladder (`ladder`, the
//! last kept at its end), drawn; first to G kills wins. No death drops
//! (inventory.md 12); firing ends spawn protection.
//!
//! Known shortcuts: the map's weapon table and its kill counts aren't in
//! the spec: the ladder and the step are data (PLACEHOLDER: our weapons,
//! a step every 2 kills, data/modes.toml); the random roll between the
//! table and the player's own equipment isn't made (always the table);
//! the ladder's weapon replaces the primary class (and the ladder's
//! earlier weapons), the rest of the loadout stays, and it is given again
//! after a respawn rebuilt the loadout.

use bevy::prelude::*;

use super::{
    ARMS_RACE, CaMode, CaRuleSettings, CaStats, Hooks, NO_HOOKS, free_for_all,
    objectives,
    state::{Death, DeathKind},
};
use crate::weapon::{Inventory, Weapon};

/// The mode.
pub const MODE: CaMode = CaMode {
    module: ARMS_RACE,
    key: "VWRM",
    shown_as: "Arms Race",
    description: "Arms Race: One Man Army with a weapon ladder (the next weapon every few kills), first to G kills wins",
    death_drops: false,
    on_death,
    hooks: Hooks { update, ..NO_HOOKS },
    ..free_for_all::MODE
};

/// The ladder step `kills` reach.
pub fn step(kills: u32, per_step: u32, len: usize) -> usize {
    if per_step == 0 || len == 0 {
        return 0;
    }
    ((kills / per_step) as usize).min(len - 1)
}

fn on_death(world: &mut World, d: &Death) {
    super::scoring::free_for_all(world, d);
    if d.kind != DeathKind::Enemy {
        return;
    }
    let Some(k) = d.killer else { return };
    free_for_all::test_win(world, k);
    apply(world, k);
}

/// `e`'s weapon for its ladder step, when it hasn't got it (its loadout
/// there).
fn apply(world: &mut World, e: Entity) {
    let o = world.resource::<CaRuleSettings>().objectives.clone();
    let kills = world.get::<CaStats>(e).map_or(0, |s| s.kills);
    if kills < o.ladder_kills || o.ladder.is_empty() {
        return;
    }
    let want = o.ladder[step(kills, o.ladder_kills, o.ladder.len())].clone();
    let Some(inv) = world.get::<Inventory>(e) else { return };
    if inv.weapons.is_empty() || objectives::has_weapon(world, e, &want).is_some() {
        return;
    }
    let old: Vec<&'static str> = inv
        .weapons
        .iter()
        .filter_map(|w| world.get::<Weapon>(*w))
        .filter(|w| w.slot == 0 || o.ladder.iter().any(|l| l == w.id))
        .map(|w| w.id)
        .collect();
    for id in old {
        objectives::take_weapon(world, e, id);
    }
    crate::weapon::give(world, e, &want);
}

/// The ladder's weapons kept through respawns.
fn update(world: &mut World) {
    let players: Vec<Entity> = super::flow::players(world).into_iter().map(|(e, _)| e).collect();
    for e in players {
        apply(world, e);
    }
}
