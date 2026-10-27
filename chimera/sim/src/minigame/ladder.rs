//! `chimera:ladder`, the weapon ladder (Gun Game; Combat Arms' Arms Race
//! is its first user in shape, architecture-review.md D2): every player a
//! team of their own, each on a rung of the definition's ladder of weapons,
//! holding that rung's weapon (with the definition's kit). An enemy kill
//! moves the killer a rung up, its weapon swapped at once; a kill on the
//! last rung wins the match. With `demote_on = "melee_death"` a player
//! killed by an enemy's melee swing goes a rung down (not below the first):
//! melee damage, or any damage from a melee weapon (CS:S's knife deals
//! bullet damage).
//! The ladder gives the weapons, so nobody drops one at death
//! (`weapon::drop::NoDeathDrops`).
//! At the time limit the highest rung wins, then the most kills; a tie is
//! a draw.
//!
//! ```toml
//! [rules]
//! module = "chimera:ladder"
//! ladder = ["cs_source:weapon_glock", "combat_arms:m4a1"]   # the rungs, in order
//! advance_on = "kill"              # the only one so far
//! demote_on = "melee_death"        # or "none" (the default)
//! refill_ammo = true               # a rung's gun never runs dry (default: false)
//! win = { kind = "ladder_end" }    # the only one so far
//! ```
//!
//! Events: `rung_up` and `rung_down` (`MinigameEvent`, the new rung).

use bevy::prelude::*;
use serde::Deserialize;

use super::{ActiveMinigame, MinigameDef, MinigameEvent, RulesModule, hud::HudLine, runtime};
use crate::{
    core::{DamageKind, Died, Score, Team},
    mode::{Death, DeathKind, Hooks, Match, Mode, NO_HOOKS, Teams},
    weapon::{Inventory, Magazine, Weapon, data::WeaponCatalog, give},
};

/// The module's name.
pub const LADDER: &str = "chimera:ladder";

/// The rules module.
pub const RULES: RulesModule = RulesModule {
    mode: &MODE,
    check,
    start,
    loadout,
    status,
    notice,
};

/// The mode on the runtime.
pub static MODE: Mode = Mode {
    id: LADDER,
    key: LADDER,
    shown_as: "Weapon ladder",
    description: "every player for themselves up a ladder of weapons: a kill a rung up, the last rung's kill wins",
    respawns: true,
    warmup: false,
    warmup_revives: false,
    forced_setup: Some(0.0),
    teams: Teams::FreeForAll,
    on_death,
    time_out,
    match_end: |w| Some(w.resource::<Match>().winner),
    hooks: Hooks {
        died,
        before_spawns,
        update: refill,
        ..NO_HOOKS
    },
};

/// The `[rules]` keys.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    #[allow(dead_code)]
    module: String,
    ladder: Vec<String>,
    #[serde(default)]
    advance_on: Advance,
    #[serde(default)]
    demote_on: Demote,
    #[serde(default)]
    win: Win,
    #[serde(default)]
    refill_ammo: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Advance {
    #[default]
    Kill,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Demote {
    #[default]
    None,
    MeleeDeath,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Win {
    #[serde(default)]
    kind: WinKind,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WinKind {
    #[default]
    LadderEnd,
}

/// The ladder playing.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct Ladder {
    pub rungs: Vec<&'static str>,
    pub demote_on_melee: bool,
    /// The rungs' guns keep a full reserve (a ladder has no ammo to find).
    pub refill_ammo: bool,
}

/// A player's rung (0: the first). Replicated to clients (`minigame_net`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Rung(pub usize);

fn params(def: &MinigameDef) -> Result<Params, String> {
    let mut table = def.rules.params.clone();
    table.insert("module".into(), toml::Value::String(def.rules.module.clone()));
    let p: Params = table.try_into().map_err(|e| format!("[rules]: {e}"))?;
    let (Advance::Kill, WinKind::LadderEnd) = (p.advance_on, p.win.kind);
    Ok(p)
}

fn check(def: &MinigameDef, catalog: &WeaponCatalog) -> Result<(), String> {
    let p = params(def)?;
    if p.ladder.is_empty() {
        return Err("[rules] ladder: no rungs".into());
    }
    for id in &p.ladder {
        super::weapon(def, catalog, id)?;
    }
    Ok(())
}

fn start(world: &mut World, def: &MinigameDef) {
    let Ok(p) = params(def) else { return };
    let catalog = world.get_resource::<WeaponCatalog>().cloned().unwrap_or_default();
    let rungs = p.ladder.iter().filter_map(|id| super::weapon(def, &catalog, id).ok()).collect();
    world.insert_resource(crate::weapon::drop::NoDeathDrops);
    world.insert_resource(Ladder {
        rungs,
        demote_on_melee: p.demote_on == Demote::MeleeDeath,
        refill_ammo: p.refill_ammo,
    });
}

/// The rung's weapon.
fn loadout(world: &World, e: Entity) -> Vec<&'static str> {
    let Some(ladder) = world.get_resource::<Ladder>() else { return Vec::new() };
    let rung = world.get::<Rung>(e).map_or(0, |r| r.0);
    ladder.rungs.get(rung.min(ladder.rungs.len().saturating_sub(1))).copied().into_iter().collect()
}

/// With `refill_ammo`, a held rung gun's reserve is topped up every tick:
/// the ladder gives each gun once, and a 200-round machine gun used up
/// (misses count) would otherwise end the climb.
fn refill(world: &mut World) {
    let Some(ladder) = world.get_resource::<Ladder>() else { return };
    if !ladder.refill_ammo {
        return;
    }
    let rungs = ladder.rungs.clone();
    let mut guns = world.query::<(&Weapon, &mut Magazine)>();
    for (w, mut m) in guns.iter_mut(world) {
        if w.owner.is_some() && m.reserve < m.reserve_max && rungs.contains(&w.id) {
            m.reserve = m.reserve_max;
        }
    }
}

/// Everyone back to the first rung as the match starts.
fn before_spawns(world: &mut World) {
    for (e, _) in runtime::players(world) {
        world.entity_mut(e).insert(Rung(0));
    }
}

/// An enemy kill: the killer a rung up (its weapon swapped), or the match
/// won from the last rung.
fn on_death(world: &mut World, d: &Death) {
    if d.kind != DeathKind::Enemy {
        return;
    }
    let Some(k) = d.killer else { return };
    let Some(ladder) = world.get_resource::<Ladder>().cloned() else { return };
    let rung = world.get::<Rung>(k).map_or(0, |r| r.0) + 1;
    if rung >= ladder.rungs.len() {
        if let Some(team) = world.get::<Team>(k).map(|t| t.0) {
            world.resource_mut::<Match>().win(team);
        }
        return;
    }
    set_rung(world, k, rung, &ladder);
    world.write_message(MinigameEvent {
        name: "rung_up",
        player: Some(k),
        value: rung as i32,
    });
}

/// A melee death at an enemy's hands: a rung down.
fn died(world: &mut World, d: &Died) {
    let Some(ladder) = world.get_resource::<Ladder>().cloned() else { return };
    if !ladder.demote_on_melee || !world.resource::<Match>().counting() || !melee_kill(world, d) {
        return;
    }
    let team = |e: Entity| world.get::<Team>(e).map(|t| t.0);
    let Some(k) = d.attacker.filter(|k| *k != d.entity) else { return };
    if team(k) == team(d.entity) {
        return;
    }
    let rung = world.get::<Rung>(d.entity).map_or(0, |r| r.0);
    if rung == 0 {
        return;
    }
    // Dead: the weapon is the next life's.
    world.entity_mut(d.entity).insert(Rung(rung - 1));
    world.write_message(MinigameEvent {
        name: "rung_down",
        player: Some(d.entity),
        value: rung as i32 - 1,
    });
}

/// Whether `d` was a melee kill: melee damage, or a melee weapon's (the
/// damage's weapon, else what the attacker holds; CS:S's knife deals
/// bullet damage).
fn melee_kill(world: &World, d: &Died) -> bool {
    if d.damage.kind == DamageKind::Melee {
        return true;
    }
    let id = d.damage.weapon.or_else(|| {
        let inv = world.get::<Inventory>(d.attacker?)?;
        world.get::<Weapon>(inv.active?).map(|w| w.id)
    });
    let Some(id) = id else { return false };
    world
        .get_resource::<crate::weapon::data::WeaponCatalog>()
        .and_then(|c| c.get(id))
        .is_some_and(|w| w.melee.is_some())
}

/// `e` on `rung`, holding its weapon in place of the ladder's others.
pub fn set_rung(world: &mut World, e: Entity, rung: usize, ladder: &Ladder) {
    world.entity_mut(e).insert(Rung(rung));
    let before = carried_slots(world, e, ladder);
    let old: Vec<Entity> = world
        .get::<Inventory>(e)
        .map(|i| {
            i.weapons
                .iter()
                .copied()
                .filter(|w| world.get::<Weapon>(*w).is_some_and(|w| ladder.rungs.contains(&w.id)))
                .collect()
        })
        .unwrap_or_default();
    for w in old {
        runtime::take_weapon(world, e, w);
    }
    // A kit by game (the gun game's knife) goes with the new gun's game.
    runtime::refit_kit(world, e, ladder.rungs[rung]);
    give(world, e, ladder.rungs[rung]);
    keep_slot_order(world, e, &before, ladder);
    // No command of the player's did this: its client takes the state
    // as a restart, not a misprediction.
    crate::core::rebase(world, e);
}

/// What `e` carries as (weapon, slot, is one of the ladder's guns: else
/// the kit's, the knife).
fn carried_slots(world: &World, e: Entity, ladder: &Ladder) -> Vec<(Entity, u8, bool)> {
    let Some(inv) = world.get::<Inventory>(e) else { return Vec::new() };
    inv.weapons
        .iter()
        .filter_map(|w| {
            let weapon = world.get::<Weapon>(*w)?;
            Some((*w, weapon.slot, ladder.rungs.contains(&weapon.id)))
        })
        .collect()
}

/// `e`'s slot order for `lastinv` (`Inventory::slot_mru`) across a swap:
/// the weapons it no longer holds since `before` (the old gun, a knife of
/// the other game) hand their slot's place to the ones that came in (the
/// new gun, the other game's knife), a gun to a gun and a kit weapon to a
/// kit weapon, so Q still goes gun <-> knife whichever slots they are in.
fn keep_slot_order(world: &mut World, e: Entity, before: &[(Entity, u8, bool)], ladder: &Ladder) {
    let now = carried_slots(world, e, ladder);
    for gun in [true, false] {
        let gone = before.iter().filter(|(w, _, g)| *g == gun && !now.iter().any(|(n, ..)| n == w));
        let came = now.iter().filter(|(w, _, g)| *g == gun && !before.iter().any(|(b, ..)| b == w));
        let pairs: Vec<(u8, u8)> = gone.zip(came).map(|(g, c)| (g.1, c.1)).collect();
        if let Some(mut inv) = world.get_mut::<Inventory>(e) {
            // Through a spare number first: two slots may trade places.
            for (i, (old, _)) in pairs.iter().enumerate() {
                inv.rename_slot(*old, 200 + i as u8);
            }
            for (i, (_, new)) in pairs.iter().enumerate() {
                inv.rename_slot(200 + i as u8, *new);
            }
        }
    }
}

/// The time limit: the highest rung, then the most kills; a tie, a draw.
fn time_out(world: &mut World) {
    let mut best: Option<((usize, i32), u8)> = None;
    let mut tie = false;
    for (e, team) in runtime::players(world) {
        let key = (world.get::<Rung>(e).map_or(0, |r| r.0), world.get::<Score>(e).map_or(0, |s| s.kills));
        match best {
            Some((b, _)) if key == b => tie = true,
            Some((b, _)) if key < b => {}
            _ => {
                best = Some((key, team));
                tie = false;
            }
        }
    }
    if let Some((_, team)) = best.filter(|_| !tie) {
        world.resource_mut::<Match>().winner = Some(team);
    }
}

/// `e`'s place on the ladder, for `minigame_status`.
pub fn standing(world: &World, e: Entity) -> Option<String> {
    let active = world.get_resource::<ActiveMinigame>()?;
    if active.rules.mode.id != LADDER {
        return None;
    }
    let ladder = world.get_resource::<Ladder>()?;
    let rung = world.get::<Rung>(e).map_or(0, |r| r.0);
    Some(format!(", rung {}/{} ({})", rung + 1, ladder.rungs.len(), ladder.rungs.get(rung).unwrap_or(&"?")))
}

/// The HUD's lines for `viewer`: their rung and gun, the next one, who
/// leads (the highest rung, then the most kills; "tied" when level).
fn status(world: &mut World, viewer: Entity) -> Vec<HudLine> {
    let Some(ladder) = world.get_resource::<Ladder>().cloned() else { return Vec::new() };
    let total = ladder.rungs.len();
    let rung = world.get::<Rung>(viewer).map_or(0, |r| r.0).min(total.saturating_sub(1));
    let mut lines = Vec::new();
    if let Some(w) = ladder.rungs.get(rung) {
        lines.push(HudLine::new(format!("Rung {}/{total}: {{weapon}}", rung + 1)).weapon(w));
    }
    match ladder.rungs.get(rung + 1) {
        Some(w) => lines.push(HudLine::new("Next: {weapon}").weapon(w)),
        None => lines.push(HudLine::new("Last rung: a kill wins")),
    }
    let mut best: Option<((usize, i32), Entity)> = None;
    let mut tied = false;
    for (e, _) in runtime::players(world) {
        let key = (world.get::<Rung>(e).map_or(0, |r| r.0), world.get::<Score>(e).map_or(0, |s| s.kills));
        match best {
            Some((b, _)) if key == b => tied = true,
            Some((b, _)) if key < b => {}
            _ => {
                best = Some((key, e));
                tied = false;
            }
        }
    }
    if let Some(((r, _), who)) = best {
        lines.push(if tied {
            HudLine::new("Tied for the lead")
        } else if who == viewer {
            HudLine::new("You lead")
        } else {
            HudLine::new(format!("Leader: {{player}}, rung {}/{total}", r + 1)).player(who)
        });
    }
    lines
}

/// The viewer's own rung changes (`rung_up`, `rung_down`).
fn notice(world: &World, viewer: Entity, event: &MinigameEvent) -> Option<HudLine> {
    if event.player != Some(viewer) {
        return None;
    }
    let ladder = world.get_resource::<Ladder>()?;
    let total = ladder.rungs.len();
    let weapon = ladder.rungs.get(usize::try_from(event.value).ok()?).copied()?;
    let text = match event.name {
        "rung_up" => format!("Rung up! {}/{total}: {{weapon}}", event.value + 1),
        "rung_down" => format!("Knifed! Down to {}/{total}: {{weapon}}", event.value + 1),
        _ => return None,
    };
    Some(HudLine::new(text).weapon(weapon))
}
