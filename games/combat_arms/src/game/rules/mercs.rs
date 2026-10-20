//! Hired Guns' mercenaries (specs/combat_arms/game_modes_2.md M11.7,
//! bots_objectives.md 3): AI soldiers a player hires with bank money, on
//! the hirer's team, following, holding or fighting for it. They are
//! characters that aren't players (`core::Npc`: on no team's count, not
//! in the server's player list) whose `Intent` comes from the bot brain
//! (`bots::CaBot` with a `bots::MercMind`: the `Squad` goal set, the bot
//! brain's attack over the follow goal; `bots::brain::decide_merc`).
//!
//! - Hiring (`hire`, any phase; the console's `ca_hire`, a network
//!   client's `CaHireRequest`): refused, in this order, when the hirer
//!   already has `merc_cap` (2) living units, the unit id has no row, the
//!   bank is short of its cost; then the cost is taken from the bank
//!   (`hired_guns::CaBank`), and only then is the spawn object checked:
//!   exactly one object named `UNITSPAWN_ALPHA` (an Alpha hirer) or
//!   `UNITSPAWN_BRAVO` (Bravo), else the hire fails with the money spent
//!   (Quirk 4). The unit comes at that object, on the hirer's team, with
//!   the hirer as its leader and the hirer's current order.
//! - Orders (`set_order`, one per hirer for all its units; `ca_order`,
//!   `CaOrderRequest`): hold (the follow point where each unit stands
//!   now; a unit hired later under it: its spawn place), default (follow
//!   the leader), follow me (follow only: never fires, Quirk 3), and the
//!   attack move to a point the shipped client never sends. A follow point
//!   at the world origin means the leader (Quirk 4). The order goes back
//!   to default when the last unit is gone.
//! - Damage (`ai_damage`, before Chimera applies it): a unit's damage to
//!   anyone of its team, and anyone's of its team to it, is 0 whatever
//!   friendly fire says (Quirk 12); its damage is scaled by the mode's
//!   `ai_damage_factor` and one non-melee hit on a player takes at most
//!   `ai_damage.cap` (50 HP; 990 for a hit of 1000 or more).
//! - Credit (`killer_of`): a kill by a unit is its hirer's.
//! - Death (`died`): the unit leaves its hirer's list at once (the slot
//!   is free), the mode counts its killer (`hired_guns::merc_killed`), its
//!   body goes `body_lifetime` (2 s) later (`expire`); it never respawns
//!   and drops nothing.
//! - Removal: a hirer's units when it leaves (`remove_of`), every unit at
//!   each round over (`remove_all`).
//!
//! Known shortcuts (docs/tech-debt.md): the unit templates' brains
//! (`Squad_AR`, `_SMG`, `_MG`) aren't in the specs (bots_objectives.md
//! Open question 6): every unit fights with the bot data's Normal
//! template and its weapon's gun-type row; their weapons are ours (an
//! MP5A4 for the P90, an M4A1 for the M416, an M60 for the MG21E:
//! data/modes.toml); the attack-tower goal has nothing to shoot (no
//! turrets yet); a unit's box is a player's (not 24 x 53 x 24 u); the
//! follow distances are 3D (bots_objectives.md 3.4; game_modes_2.md M11.7
//! says horizontal: the two specs differ); the AI damage factor and cap
//! apply to the damage after armour (the weapon parts take armour first);
//! the kill feed doesn't mark squad kills; the order wheel's voice lines
//! aren't played.

use bevy::{ecs::message::MessageMutator, prelude::*};
use serde::{Deserialize, Serialize};

use super::{
    super::{
        bots::{CaBot, MercMind, Template, data::BotData, data::Level, roster::Kit},
        inventory::{self, HP},
        weapons::{CaArmor, ExactAim},
    },
    CaRuleSettings,
    objects::CaMapObjects,
    state::{ALPHA, BRAVO},
};
use crate::{
    character::spawn_character,
    core::{Damage, DamageKind, Died, Npc, SimClock, Team},
    spawning,
    weapon::{Inventory, OwnKit},
};

/// A mercenary: its hirer (leader) and unit id; replicated (the HUD's
/// mercenary panel).
#[derive(Component, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[require(Npc)]
pub struct CaMerc {
    #[entities]
    pub leader: Entity,
    pub unit: u8,
    /// Dead since (its body goes `body_lifetime` s later).
    pub died: Option<f64>,
}

/// A squad's order (bots_objectives.md 3.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum MercOrder {
    #[default]
    Default,
    FollowMe,
    Hold,
    /// Follow the point (the server's; no client sends it).
    AttackMove(Vec3),
}

impl MercOrder {
    /// An order by name (`hold`, `default`, `follow`; the wheel's 1, 2, 3).
    pub fn named(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "hold" | "1" => Some(Self::Hold),
            "default" | "2" => Some(Self::Default),
            "follow" | "follow_me" | "3" => Some(Self::FollowMe),
            _ => None,
        }
    }
}

/// A hirer's current order (on the hirer; replicated for its HUD).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CaSquadOrder(pub MercOrder);

/// The living units `hirer` owns, oldest first.
pub fn living_units(world: &mut World, hirer: Entity) -> Vec<Entity> {
    let mut units: Vec<(Entity, u64)> = world
        .query::<(Entity, &CaMerc)>()
        .iter(world)
        .filter(|(_, m)| m.leader == hirer && m.died.is_none())
        .map(|(e, _)| (e, e.to_bits()))
        .collect();
    units.sort_by_key(|(_, k)| *k);
    units.into_iter().map(|(e, _)| e).collect()
}

/// Every mercenary, dead or alive.
pub fn all(world: &mut World) -> Vec<Entity> {
    world.query_filtered::<Entity, With<CaMerc>>().iter(world).collect()
}

/// The spawn object's name for a hirer on `team`.
fn spawn_name(world: &World, team: u8) -> Option<String> {
    let h = &world.resource::<CaRuleSettings>().hired_guns;
    match team {
        ALPHA => Some(h.unit_spawn_alpha.clone()),
        BRAVO => Some(h.unit_spawn_bravo.clone()),
        _ => None,
    }
}

/// Hire unit `unit` for `hirer` (M11.7): its new character, or why not.
pub fn hire(world: &mut World, hirer: Entity, unit: usize) -> Result<Entity, String> {
    let h = world.resource::<CaRuleSettings>().hired_guns.clone();
    if living_units(world, hirer).len() as u32 >= h.merc_cap {
        return Err(format!("already {} mercenaries", h.merc_cap));
    }
    let Some(row) = h.units.get(unit).cloned() else {
        return Err(format!("no unit {unit}"));
    };
    let bank = world.get::<super::hired_guns::CaBank>(hirer).map_or(0, |b| b.0);
    if bank < row.cost {
        return Err(format!("the bank holds {bank}, unit {unit} costs {}", row.cost));
    }
    super::hired_guns::pay(world, hirer, -row.cost);
    // Only now the spawn object (Quirk 4: the money is spent).
    let team = world.get::<Team>(hirer).map_or(0, |t| t.0);
    let Some(name) = spawn_name(world, team) else {
        return Err("the hirer is on no team".into());
    };
    let found: Vec<Transform> = world.resource::<CaMapObjects>().named(&name).map(|p| p.at).collect();
    if found.len() != 1 {
        return Err(format!("{} objects named {name}", found.len()));
    }
    let at = found[0];
    let e = spawn(world, hirer, unit as u8, team, at, &row.template, [&row.weapon, &row.grenade]);
    // The hirer's current order.
    let order = world.get::<CaSquadOrder>(hirer).copied().unwrap_or_default().0;
    apply_order(world, e, order);
    info!("{} hired unit {unit} ({})", hirer, row.template);
    Ok(e)
}

/// A unit's character at `at`, armed with `weapons` (primary, grenade).
fn spawn(world: &mut World, hirer: Entity, unit: u8, team: u8, at: Transform, template: &str, weapons: [&String; 2]) -> Entity {
    let registry = world.resource::<crate::weapon::WeaponRegistry>();
    let ids: Vec<&'static str> = weapons.iter().filter_map(|id| registry.find(id).map(|d| d.id)).collect();
    let mut kit: Kit = [None; 6];
    kit[0] = ids.first().copied();
    kit[4] = ids.get(1).copied();
    let data = world.resource::<BotData>().clone();
    let seed = super::objectives::hash(hirer.to_bits() ^ ((unit as u64) << 40) ^ world.resource::<SimClock>().tick);
    let mut bot = CaBot::new(unit as usize, Template::new(Level::Normal, 0), seed, kit, &data);
    bot.merc = Some(MercMind::new(hirer));
    let e = {
        let mut commands = world.commands();
        spawn_character(&mut commands, at, Team(team), super::super::movement::ID)
    };
    world.flush();
    let hirer_name = world.get::<Name>(hirer).map_or_else(|| hirer.to_string(), |n| n.as_str().to_string());
    world.entity_mut(e).insert((
        Name::new(format!("{template} ({hirer_name})")),
        CaMerc {
            leader: hirer,
            unit,
            died: None,
        },
        OwnKit(ids),
        ExactAim,
        bot,
    ));
    // A mercenary is not a computer player (`core::Bot`): not counted or
    // kicked as one.
    world.entity_mut(e).remove::<crate::core::Bot>();
    inventory::respawn_inventory(world, e);
    spawning::revive(world, e, Some(at));
    world.entity_mut(e).insert(CaArmor::default());
    e
}

/// Give `hirer`'s squad `order` (bots_objectives.md 3.2).
pub fn set_order(world: &mut World, hirer: Entity, order: MercOrder) {
    if let Ok(mut h) = world.get_entity_mut(hirer) {
        h.insert(CaSquadOrder(order));
    }
    for e in living_units(world, hirer) {
        apply_order(world, e, order);
    }
}

/// One unit's follow point and follow goal for `order`.
fn apply_order(world: &mut World, e: Entity, order: MercOrder) {
    let feet = world.get::<Transform>(e).map_or(Vec3::ZERO, |t| t.translation - Vec3::Y * 0.9);
    let Some(mut bot) = world.get_mut::<CaBot>(e) else { return };
    let Some(mind) = &mut bot.merc else { return };
    let (point, only) = match order {
        MercOrder::Default => (None, false),
        MercOrder::FollowMe => (None, true),
        MercOrder::Hold => (Some(feet), false),
        MercOrder::AttackMove(p) => (Some(p), false),
    };
    mind.follow_point = point;
    mind.follow_only = only;
    mind.holding = false;
    mind.replan_at = 0.0;
}

/// Who a kill by `attacker` is credited to: a unit's hirer, else itself.
pub fn killer_of(world: &World, attacker: Entity) -> Entity {
    world
        .get::<CaMerc>(attacker)
        .map(|m| m.leader)
        .filter(|l| world.get_entity(*l).is_ok())
        .unwrap_or(attacker)
}

/// A unit died (`scoring` hands its deaths here): out of its hirer's
/// list, its killer counted by the mode, its body on a timer.
pub fn died(world: &mut World, d: &Died) {
    let now = world.resource::<SimClock>().now;
    let Some(m) = world.get::<CaMerc>(d.entity).copied() else { return };
    if m.died.is_some() {
        return;
    }
    if let Some(mut x) = world.get_mut::<CaMerc>(d.entity) {
        x.died = Some(now);
    }
    let (leader, team) = (m.leader, world.get::<Team>(d.entity).map_or(0, |t| t.0));
    if let Some(a) = d.attacker {
        // A player's kill (another AI's or the world's give nothing).
        let player = world.get::<Npc>(a).is_none() && world.get::<Team>(a).is_some_and(|t| t.0 != team);
        if player {
            let head = d.damage.hitgroup == crate::core::Hitgroup::Head;
            super::hired_guns::merc_killed(world, a, head);
        }
    }
    reset_order_if_empty(world, leader);
}

/// The order back to default when `hirer` has no unit left.
fn reset_order_if_empty(world: &mut World, hirer: Entity) {
    if living_units(world, hirer).is_empty()
        && let Ok(mut h) = world.get_entity_mut(hirer)
        && h.get::<CaSquadOrder>().is_some_and(|o| o.0 != MercOrder::Default)
    {
        h.insert(CaSquadOrder::default());
    }
}

/// Remove unit `e` (its weapons too).
pub fn remove(world: &mut World, e: Entity) {
    let leader = world.get::<CaMerc>(e).map(|m| m.leader);
    let weapons = world.get::<Inventory>(e).map(|i| i.weapons.clone()).unwrap_or_default();
    for w in weapons {
        if let Ok(w) = world.get_entity_mut(w) {
            w.despawn();
        }
    }
    if let Ok(x) = world.get_entity_mut(e) {
        x.despawn();
    }
    if let Some(l) = leader {
        reset_order_if_empty(world, l);
    }
}

/// Every unit of `hirer` removed (it left).
pub fn remove_of(world: &mut World, hirer: Entity) {
    let units: Vec<Entity> = world
        .query::<(Entity, &CaMerc)>()
        .iter(world)
        .filter(|(_, m)| m.leader == hirer)
        .map(|(e, _)| e)
        .collect();
    for e in units {
        remove(world, e);
    }
}

/// Every unit removed (a round over).
pub fn remove_all(world: &mut World) {
    for e in all(world) {
        remove(world, e);
    }
}

/// Dead units' bodies go `body_lifetime` s after the death; units whose
/// hirer is gone go at once.
pub(super) fn expire(world: &mut World) {
    let now = world.resource::<SimClock>().now;
    let life = world.resource::<CaRuleSettings>().hired_guns.body_lifetime as f64;
    let gone: Vec<Entity> = world
        .query::<(Entity, &CaMerc)>()
        .iter(world)
        .filter(|(_, m)| m.died.is_some_and(|t| now + 1e-9 >= t + life))
        .map(|(e, _)| e)
        .collect();
    for e in gone {
        remove(world, e);
    }
    let orphans: Vec<Entity> = world
        .query::<(Entity, &CaMerc)>()
        .iter(world)
        .filter(|(_, m)| world.get_entity(m.leader).is_err())
        .map(|(e, _)| e)
        .collect();
    for e in orphans {
        remove(world, e);
    }
}

/// M11.7's damage rules for AI: same-team damage to or from a unit is 0;
/// a unit's damage scaled by the AI damage factor and, on a player, one
/// non-melee hit capped (game_modes_2.md Constants).
pub(super) fn ai_damage(
    mut damage: MessageMutator<Damage>,
    mercs: Query<&CaMerc>,
    teams: Query<&Team>,
    npcs: Query<(), With<Npc>>,
    settings: Res<CaRuleSettings>,
) {
    let factor = settings.hired_guns.ai_damage_factor;
    for d in damage.read() {
        let Some(a) = d.attacker.filter(|a| *a != d.target) else { continue };
        let (from_ai, to_ai) = (mercs.contains(a), mercs.contains(d.target));
        if !from_ai && !to_ai {
            continue;
        }
        let mates = matches!((teams.get(a), teams.get(d.target)), (Ok(x), Ok(y)) if x == y);
        if mates {
            d.amount = 0.0;
            continue;
        }
        if from_ai {
            d.amount *= factor;
            if !npcs.contains(d.target) {
                let hp = d.amount / HP;
                d.amount = settings.ai_damage.capped(hp, d.kind == DamageKind::Melee) * HP;
            }
        }
    }
}
