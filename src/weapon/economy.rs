//! Money and buying for any game: prices by weapon ID and for armour
//! (`Prices`, filled by games), a character's `Money`, and whether buying
//! is open now (`BuyWindow`, set by the rules). Characters without `Money`
//! buy for free (deathmatch, testing).

use std::collections::HashMap;

use bevy::prelude::*;

use super::{
    Armor, Inventory, Weapon, WeaponRegistry, give,
    grenade::{GrenadeKind, Throwable},
};

/// A character's money.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[reflect(Component)]
pub struct Money(pub u32);

/// What things cost.
#[derive(Resource, Clone, Debug, Default)]
pub struct Prices {
    /// By full weapon ID.
    pub weapons: HashMap<&'static str, u32>,
    /// Weapons only one team may buy (weapon ID -> `core::Team` number).
    pub team_only: HashMap<&'static str, u8>,
    /// Kevlar; kevlar and helmet; the helmet alone (over full kevlar).
    pub vest: u32,
    pub vest_helmet: u32,
    pub helmet: u32,
    /// A bomb defusal kit (`DefuseKit`); 0: not sold. Team-only through
    /// `team_only["defuser"]`.
    pub defuser: u32,
    /// The buy menu as the game lays it out (empty: pistols, primaries and
    /// equipment by slot).
    pub menu: Vec<BuyCategory>,
    /// How much computer players like each primary when buying (unlisted:
    /// 1; 0: never).
    pub bot_weights: HashMap<&'static str, f32>,
    /// Ammo for sale: the box each weapon's ammo comes in, by weapon ID.
    /// A weapon listed here and bought for money comes with an empty
    /// reserve (you buy its ammo); unlisted weapons keep a full one.
    pub ammo: HashMap<&'static str, AmmoBox>,
    /// Rounds a starting weapon carries in reserve while ammo is sold
    /// (rounds with money), by weapon ID; unlisted: none.
    pub starting_reserve: HashMap<&'static str, u32>,
    /// Sound entry played when ammo is bought.
    pub ammo_sound: Option<String>,
    /// Sound entry played when a weapon is bought or picked up.
    pub pickup_sound: Option<String>,
    /// Sound entry played when armour or a defuse kit is bought.
    pub item_sound: Option<String>,
}

/// A box of ammo: its price and the rounds in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AmmoBox {
    pub price: u32,
    pub rounds: u32,
}

/// What buys ammo (`buy <name>`): the slot of the weapon it's for, and
/// whether it fills the reserve (else one box). CS:S's `primammo` and
/// `secammo` (the buy menu's ammo keys) fill; `buyammo1` and `buyammo2`
/// (its `,` and `.` keys) buy a box.
pub const AMMO_BUYS: [(&str, u8, bool); 4] = [
    ("primammo", 0, true),
    ("secammo", 1, true),
    ("buyammo1", 0, false),
    ("buyammo2", 1, false),
];

/// One buy-menu category: the number key that opens it, its name and its
/// items in order. A category with `direct` set buys that instead of
/// opening (CS:S's primary and secondary ammo keys).
#[derive(Clone, Debug, PartialEq)]
pub struct BuyCategory {
    pub key: u8,
    pub name: &'static str,
    pub items: Vec<BuyItem>,
    pub direct: Option<&'static str>,
}

/// One buy-menu line: what `buy` gets (a weapon ID, `vest`, `vesthelm`)
/// and its name.
#[derive(Clone, Debug, PartialEq)]
pub struct BuyItem {
    pub buy: &'static str,
    pub label: &'static str,
}

impl Prices {
    /// What `buy` (a weapon ID, `vest`, `vesthelm`) costs.
    pub fn of(&self, buy: &str) -> Option<u32> {
        match buy {
            "vest" => Some(self.vest),
            "vesthelm" => Some(self.vest_helmet),
            "defuser" => (self.defuser > 0).then_some(self.defuser),
            id => self.weapons.get(id).copied(),
        }
    }
}

/// A bomb defusal kit carried (CS:S): defusing takes the shorter time.
/// Kept until death, then dropped (`objectives::bomb`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component)]
pub struct DefuseKit;

/// Why buying fails outside a buy zone (CS:S's words).
pub const NOT_IN_BUY_ZONE: &str = "You are not in a buy zone.";

/// Whether buying is open: Ok, or why not (shown to the player).
#[derive(Resource, Clone, Debug)]
pub struct BuyWindow(pub Result<(), String>);

impl Default for BuyWindow {
    fn default() -> Self {
        Self(Ok(()))
    }
}

/// Why buying fails for a terrorist or a CT on a map whose
/// `info_map_parameters` bars them (CS:S's words, on one line).
pub const T_CANT_BUY: &str = "Terrorists aren't allowed to buy anything on this map!";
pub const CT_CANT_BUY: &str = "CTs aren't allowed to buy anything on this map!";

/// Why buying fails without the money (CS:S's words).
pub const INSUFFICIENT_FUNDS: &str = "You have insufficient funds.";
/// Why buying fails with no room left (CS:S's words).
pub const CANNOT_CARRY: &str = "You cannot carry any more.";

/// A network client's buy: what it asks the server for (`net` sends it,
/// the server runs `buy` and says why not).
#[derive(Message, Clone, Debug, PartialEq)]
pub struct BuyRequested(pub String);

/// Buy `name` for `owner` as a player asks for it: on a network client,
/// a request to the server (`BuyRequested`; Ok(None), the server answers
/// a refusal with a hint), else `buy`.
pub fn buy_or_ask(world: &mut World, owner: Entity, name: &str) -> Result<Option<String>, String> {
    if world.get_resource::<crate::core::NetRole>() == Some(&crate::core::NetRole::Client) {
        world.write_message(BuyRequested(name.to_string()));
        return Ok(None);
    }
    buy(world, owner, name).map(Some)
}

/// Buy `name` (a weapon ID, `weapon_ak47`, `ak47`, `vest`, `vesthelm`,
/// `defuser`, or ammo: `AMMO_BUYS`) for `owner`: checks the window and the
/// money, replaces a weapon in the same slot (dropped, as CS:S does).
/// Returns what happened.
pub fn buy(world: &mut World, owner: Entity, name: &str) -> Result<String, String> {
    let lower = name.to_lowercase();
    match lower.as_str() {
        "autobuy" => return autobuy_list(world, owner),
        "rebuy" => return rebuy(world, owner),
        _ => {}
    }
    let bought = buy_quietly(world, owner, name)?;
    // What rebuy buys again: everything carried after a buy.
    let snapshot = RebuySnapshot::of(world, owner);
    world.entity_mut(owner).insert(snapshot);
    let prices = world.resource::<Prices>();
    let sound = if AMMO_BUYS.iter().any(|(n, ..)| *n == lower) {
        prices.ammo_sound.clone()
    } else if matches!(lower.as_str(), "vest" | "vesthelm" | "defuser") {
        prices.item_sound.clone()
    } else {
        prices.pickup_sound.clone()
    };
    owner_sound(world, owner, sound);
    Ok(bought)
}

/// The userinfo keys of the player's autobuy and rebuy lists (CS:S's
/// client cvars, which its client fills from `cfg/autobuy.txt` and
/// `cfg/rebuy.txt`).
pub const AUTOBUY_KEY: &str = "cl_autobuy";
pub const REBUY_KEY: &str = "cl_rebuy";

/// The lists a player without its own use: CS:S's `cfg/autobuy_default.txt`
/// and `cfg/rebuy_default.txt` (their entries, in their order).
pub const AUTOBUY_DEFAULT: &str = "m4a1 ak47 famas galil p90 mp5 primammo secammo defuser vesthelm vest";
pub const REBUY_DEFAULT: &str =
    "PrimaryWeapon PrimaryAmmo Defuser Armor HEGrenade Flashbang SmokeGrenade SecondaryWeapon SecondaryAmmo NightVision";

/// The local player's `cl_autobuy` and `cl_rebuy` (sent as userinfo).
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct BuyLists {
    pub autobuy: String,
    pub rebuy: String,
}

impl Default for BuyLists {
    fn default() -> Self {
        Self {
            autobuy: AUTOBUY_DEFAULT.into(),
            rebuy: REBUY_DEFAULT.into(),
        }
    }
}

/// A player's list (`key`) from its userinfo, else the default.
fn user_list(world: &World, owner: Entity, key: &str, default: &str) -> Vec<String> {
    let list = world
        .get::<crate::core::UserInfo>(owner)
        .and_then(|u| u.0.get(key).cloned())
        .unwrap_or_else(|| default.to_string());
    list.split_whitespace().map(str::to_lowercase).take(32).collect()
}

/// CS:S's buy aliases that aren't the weapon's own name.
fn alias(name: &str) -> &str {
    match name {
        "mp5" => "mp5navy",
        other => other,
    }
}

/// The slot of the weapon `name` buys, if it names one.
fn weapon_slot(world: &World, name: &str) -> Option<u8> {
    let registry = world.resource::<WeaponRegistry>();
    let wanted = if name.contains(':') || name.starts_with("weapon_") {
        name.to_string()
    } else {
        format!("weapon_{name}")
    };
    let id = registry.find(&wanted)?.id;
    weapon_slots(registry).into_iter().find(|(w, _)| *w == id).map(|(_, s)| s)
}

/// The weapon `owner` carries in `slot`, if any.
fn carried_in(world: &World, owner: Entity, slot: u8) -> Option<&'static str> {
    let inv = world.get::<Inventory>(owner)?;
    inv.weapons
        .iter()
        .filter_map(|w| world.get::<Weapon>(*w))
        .find(|w| w.slot == slot)
        .map(|w| w.id)
}

/// `autobuy` (CS:S's F1): the player's `cl_autobuy` list in order, each
/// entry bought if it can be; once a primary (or secondary) weapon is
/// bought, later ones are skipped (the list file's rule). Primaries are
/// skipped too while one is carried (ours: buying would drop it; CS:S's
/// unmeasured).
fn autobuy_list(world: &mut World, owner: Entity) -> Result<String, String> {
    let list = user_list(world, owner, AUTOBUY_KEY, AUTOBUY_DEFAULT);
    let (mut got, mut why) = (Vec::new(), None);
    let mut primary = carried_in(world, owner, 0).is_some();
    let mut secondary = false;
    for entry in list {
        let name = alias(&entry).to_string();
        let slot = weapon_slot(world, &name);
        if (slot == Some(0) && primary) || (slot == Some(1) && secondary) {
            continue;
        }
        match buy(world, owner, &name) {
            Ok(_) => {
                primary |= slot == Some(0);
                secondary |= slot == Some(1);
                got.push(name);
            }
            Err(e) => why = Some(e),
        }
    }
    if got.is_empty() {
        return Err(why.unwrap_or_else(|| "Nothing to buy.".into()));
    }
    Ok(format!("autobuy: {}", got.join(" ")))
}

/// What a player carried after its last buy (CS:S's rebuy snapshot):
/// weapon IDs in the primary and secondary slots, armour (helmet),
/// grenades by weapon ID and count, a defuse kit.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct RebuySnapshot {
    pub primary: Option<&'static str>,
    pub secondary: Option<&'static str>,
    pub armor: Option<bool>,
    pub grenades: Vec<(&'static str, GrenadeKind, u32)>,
    pub defuser: bool,
}

impl RebuySnapshot {
    pub fn of(world: &World, owner: Entity) -> Self {
        let mut grenades = Vec::new();
        if let Some(inv) = world.get::<Inventory>(owner) {
            for w in &inv.weapons {
                if let (Some(weapon), Some(t)) = (world.get::<Weapon>(*w), world.get::<Throwable>(*w)) {
                    grenades.push((weapon.id, t.effect.kind(), t.count));
                }
            }
        }
        Self {
            primary: carried_in(world, owner, 0),
            secondary: carried_in(world, owner, 1),
            armor: world.get::<Armor>(owner).filter(|a| a.amount > 0.0).map(|a| a.helmet),
            grenades,
            defuser: world.get::<DefuseKit>(owner).is_some(),
        }
    }
}

/// `rebuy` (CS:S's F2): what the player carried after its last buy
/// (`RebuySnapshot`), bought again by the categories of its `cl_rebuy`
/// list in order (the rebuy list file's names); what is still carried is
/// skipped. Rebuying takes no new snapshot, so a round short of money
/// doesn't forget what it couldn't afford (ours; CS:S's unmeasured).
fn rebuy(world: &mut World, owner: Entity) -> Result<String, String> {
    let Some(snap) = world.get::<RebuySnapshot>(owner).cloned() else {
        return Err("Nothing to rebuy.".into());
    };
    let list = user_list(world, owner, REBUY_KEY, REBUY_DEFAULT);
    let mut wanted: Vec<String> = Vec::new();
    for category in list {
        let now = RebuySnapshot::of(world, owner);
        let grenade = |kind: GrenadeKind| -> Vec<String> {
            let had = snap.grenades.iter().find(|g| g.1 == kind);
            let has = now.grenades.iter().find(|g| g.1 == kind).map_or(0, |g| g.2);
            had.map(|(id, _, n)| vec![id.to_string(); n.saturating_sub(has) as usize])
                .unwrap_or_default()
        };
        let items: Vec<String> = match category.as_str() {
            "primaryweapon" => snap.primary.filter(|p| now.primary != Some(*p)).map(String::from).into_iter().collect(),
            "secondaryweapon" => snap
                .secondary
                .filter(|p| now.secondary != Some(*p))
                .map(String::from)
                .into_iter()
                .collect(),
            "primaryammo" => vec!["primammo".into()],
            "secondaryammo" => vec!["secammo".into()],
            "defuser" if snap.defuser && !now.defuser => vec!["defuser".into()],
            "armor" => match (snap.armor, now.armor) {
                (Some(true), Some(true)) | (Some(false), Some(_)) | (None, _) => Vec::new(),
                (Some(true), _) => vec!["vesthelm".into()],
                (Some(false), None) => vec!["vest".into()],
            },
            "hegrenade" => grenade(GrenadeKind::Blast),
            "flashbang" => grenade(GrenadeKind::Flash),
            "smokegrenade" => grenade(GrenadeKind::Smoke),
            _ => Vec::new(),
        };
        for item in items {
            // Ammo for a gun not carried, or a full reserve: nothing.
            if buy_without_snapshot(world, owner, &item).is_ok() {
                wanted.push(item);
            }
        }
    }
    if wanted.is_empty() {
        return Err("Nothing to rebuy.".into());
    }
    Ok(format!("rebuy: {}", wanted.join(" ")))
}

/// `buy` without taking a rebuy snapshot.
fn buy_without_snapshot(world: &mut World, owner: Entity, name: &str) -> Result<String, String> {
    let before = world.get::<RebuySnapshot>(owner).cloned();
    let result = buy(world, owner, name);
    match before {
        Some(s) => {
            world.entity_mut(owner).insert(s);
        }
        None => {
            world.entity_mut(owner).remove::<RebuySnapshot>();
        }
    }
    result
}

/// Play `entry` from `owner` as a sound of the rules (buying, picking
/// up): a network server's clients hear it too.
pub fn owner_sound(world: &mut World, owner: Entity, entry: Option<String>) {
    let Some(entry) = entry.filter(|_| world.contains_resource::<Messages<crate::map::PlaySound>>()) else {
        return;
    };
    let at = world.get::<Transform>(owner).map(|t| t.translation);
    let sound = crate::map::PlaySound {
        pitch: None,
        entry,
        at,
        volume: None,
        source: Some(owner),
        channel: None,
    };
    if world.contains_resource::<Messages<crate::map::GameSound>>() {
        world.write_message(crate::map::GameSound(sound));
    } else {
        world.write_message(sound);
    }
}

/// `buy` without its sound.
fn buy_quietly(world: &mut World, owner: Entity, name: &str) -> Result<String, String> {
    world.resource::<BuyWindow>().0.clone()?;
    if world.get::<Money>(owner).is_some()
        && let Some(why) = cannot_buy_here(world, owner)
    {
        return Err(why);
    }
    let name = name.to_lowercase();
    let prices = world.resource::<Prices>().clone();
    let money = world.get::<Money>(owner).map(|m| m.0);
    let pay = |world: &mut World, cost: u32| -> Result<(), String> {
        if let Some(have) = money {
            if have < cost {
                return Err(INSUFFICIENT_FUNDS.into());
            }
            world.entity_mut(owner).insert(Money(have - cost));
        }
        Ok(())
    };
    if name == "vest" || name == "vesthelm" {
        let armor = world.get::<Armor>(owner).copied().unwrap_or_default();
        let full = armor.amount >= 1.0;
        let (cost, helmet) = match (name.as_str(), full, armor.helmet) {
            ("vest", true, _) => return Err("You already have Kevlar!".into()),
            ("vesthelm", true, true) => return Err("You already have Kevlar and a helmet!".into()),
            ("vest", false, h) => (prices.vest, h),
            (_, true, false) => (prices.helmet, true),
            _ => (prices.vest_helmet, true),
        };
        pay(world, cost)?;
        world.entity_mut(owner).insert(Armor { amount: 1.0, helmet });
        return Ok(format!("bought {name}"));
    }
    if let Some((_, slot, fill)) = AMMO_BUYS.iter().find(|(n, ..)| *n == name) {
        return buy_ammo(world, owner, *slot, *fill);
    }
    if name == "defuser" {
        if prices.defuser == 0 {
            return Err("no defuser for sale".into());
        }
        if let Some(team) = prices.team_only.get("defuser")
            && world.get::<crate::core::Team>(owner).is_some_and(|t| t.0 != *team)
        {
            return Err("Your team can't buy that.".into());
        }
        if world.get::<DefuseKit>(owner).is_some() {
            return Err("You already have a defuse kit.".into());
        }
        pay(world, prices.defuser)?;
        world.entity_mut(owner).insert(DefuseKit);
        return Ok("bought defuser".into());
    }
    let wanted = if name.contains(':') || name.starts_with("weapon_") {
        name
    } else {
        format!("weapon_{name}")
    };
    let id = world
        .resource::<WeaponRegistry>()
        .find(&wanted)
        .map(|d| d.id)
        .ok_or_else(|| format!("no weapon {wanted}"))?;
    let held: Vec<Entity> = world.get::<Inventory>(owner).map(|i| i.weapons.clone()).unwrap_or_default();
    if let Some(team) = prices.team_only.get(id)
        && world.get::<crate::core::Team>(owner).is_some_and(|t| t.0 != *team)
    {
        return Err("Your team can't buy that weapon.".into());
    }
    let cost = prices.weapons.get(id).copied().unwrap_or(0);
    if let Some(have) = held
        .iter()
        .copied()
        .find(|w| world.get::<Weapon>(*w).is_some_and(|w| w.id == id))
    {
        // Grenades stack up to their carry limit (spec grenades.md 1).
        let room = world.get::<Throwable>(have).is_some_and(|t| t.count < t.max);
        if world.get::<Throwable>(have).is_some() && !room {
            return Err(CANNOT_CARRY.into());
        }
        if !room {
            return Err("You already own that weapon.".into());
        }
        pay(world, cost)?;
        if let Some(mut t) = world.get_mut::<Throwable>(have) {
            t.count += 1;
        }
        return Ok(format!("bought {id}"));
    }
    pay(world, cost)?;
    let active = world.get::<Inventory>(owner).and_then(|i| i.active);
    let new = give(world, owner, id).ok_or("can't carry it")?;
    // Bought for money where ammo is sold: the clip only (spec
    // weapons.md 3.9: a new weapon has `default_clip` rounds, an equipped
    // clip weapon adds nothing to the reserve).
    if money.is_some()
        && prices.ammo.contains_key(id)
        && let Some(mut m) = world.get_mut::<super::Magazine>(new)
    {
        m.reserve = 0;
    }
    if world.get::<Throwable>(new).is_some() {
        // Grenades share their slot and don't take the hand.
        if active.is_some()
            && let Some(mut inv) = world.get_mut::<Inventory>(owner)
        {
            inv.wanted = None;
        }
        return Ok(format!("bought {id}"));
    }
    // One weapon per slot: the old one is dropped.
    let slot = world.get::<Weapon>(new).map(|w| w.slot);
    if let Some(old) = held
        .into_iter()
        .find(|w| world.get::<Weapon>(*w).map(|w| w.slot) == slot && world.get::<super::drop::Undroppable>(*w).is_none())
    {
        super::drop::drop_this(world, owner, old, false);
    }
    Ok(format!("bought {id}"))
}

/// Buy ammo for the weapon `owner` carries in `slot`: one box, or
/// (`fill`) boxes until the reserve is full or the money runs out. A box
/// that doesn't fit whole costs its full price (CS:S's rule as commonly
/// described; unmeasured).
fn buy_ammo(world: &mut World, owner: Entity, slot: u8, fill: bool) -> Result<String, String> {
    let prices = world.resource::<Prices>();
    let held = world.get::<Inventory>(owner).map(|i| i.weapons.clone()).unwrap_or_default();
    let Some((weapon, ammo)) = held.into_iter().find_map(|w| {
        let id = world.get::<Weapon>(w).filter(|x| x.slot == slot)?.id;
        world.get::<super::Magazine>(w)?;
        Some((w, *prices.ammo.get(id)?))
    }) else {
        return Err("You have no weapon for that ammo.".into());
    };
    let mut bought = 0;
    loop {
        let m = world.get::<super::Magazine>(weapon).expect("checked above");
        let room = m.reserve_max.saturating_sub(m.reserve);
        let why = if room == 0 {
            CANNOT_CARRY
        } else if world.get::<Money>(owner).is_some_and(|m| m.0 < ammo.price) {
            INSUFFICIENT_FUNDS
        } else {
            ""
        };
        if !why.is_empty() {
            if bought == 0 {
                return Err(why.into());
            }
            break;
        }
        if let Some(mut money) = world.get_mut::<Money>(owner) {
            money.0 -= ammo.price;
        }
        world.get_mut::<super::Magazine>(weapon).expect("checked above").reserve += ammo.rounds.min(room);
        bought += 1;
        if !fill {
            break;
        }
    }
    Ok(format!("bought {bought} box{}", if bought == 1 { "" } else { "es" }))
}

/// Set a starting weapon's reserve for a game where ammo is bought:
/// `Prices::starting_reserve` (no change where ammo isn't sold).
pub fn starting_reserve(world: &mut World, weapon: Entity) {
    let Some(id) = world.get::<Weapon>(weapon).map(|w| w.id) else {
        return;
    };
    let Some(prices) = world.get_resource::<Prices>() else {
        return;
    };
    if !prices.ammo.contains_key(id) {
        return;
    }
    let rounds = prices.starting_reserve.get(id).copied().unwrap_or(0);
    if let Some(mut m) = world.get_mut::<super::Magazine>(weapon) {
        m.reserve = rounds.min(m.reserve_max);
    }
}

/// Why `owner` can't buy where it stands, if it can't: its team may not
/// buy on this map (`map_buying`), or it is out of its buy zones.
pub fn cannot_buy_here(world: &World, owner: Entity) -> Option<String> {
    let team = world.get::<crate::core::Team>(owner).map(|t| t.0);
    let barred = match map_buying(world) {
        3 => true,
        1 => team == Some(1),
        2 => team == Some(2),
        _ => false,
    };
    if barred {
        // CS:S says it by the player's team (a CT when not a terrorist).
        return Some(if team == Some(1) { T_CANT_BUY } else { CT_CANT_BUY }.into());
    }
    (!in_buy_zone(world, owner)).then(|| NOT_IN_BUY_ZONE.into())
}

/// The map's `info_map_parameters` `buying` (CS:S): 0 everyone may buy,
/// 1 CTs only, 2 terrorists only, 3 nobody. 0 without one.
pub fn map_buying(world: &World) -> u8 {
    let Some(map) = world.get_resource::<crate::map::MapEntities>() else {
        return 0;
    };
    map.entities
        .iter()
        .find(|e| e.classname().eq_ignore_ascii_case("info_map_parameters"))
        .and_then(|e| e.get("buying"))
        .and_then(|v| v.trim().parse().ok())
        .filter(|b| *b <= 3)
        .unwrap_or(0)
}

/// Whether `owner` stands in one of its team's buy zones (Source
/// `func_buyzone`, `TeamNum` 2 terrorists, 3 CTs, else anyone). A map
/// without zones, or no map, lets everyone buy anywhere. CS:S makes zones
/// around each team's spawns then: not modelled yet, it needs a spec
/// (the zones' size; docs/backlog.md).
pub fn in_buy_zone(world: &World, owner: Entity) -> bool {
    use crate::map::entities::{engine_to_entity, entity_rotation};
    let Some(map) = world.get_resource::<crate::map::MapEntities>() else {
        return true;
    };
    let zones: Vec<_> = map
        .entities
        .iter()
        .filter(|e| e.classname().eq_ignore_ascii_case("func_buyzone") && !e.hulls.is_empty())
        .collect();
    if zones.is_empty() {
        return true;
    }
    let Some(at) = world.get::<Transform>(owner).map(|t| t.translation) else {
        return false;
    };
    // Our teams 1 and 2 are Source's 2 and 3.
    let team = world.get::<crate::core::Team>(owner).map(|t| t.0 as i32 + 1);
    let p = engine_to_entity(at, map.scale);
    zones.iter().any(|z| {
        let zone_team = z.get("TeamNum").and_then(|v| v.trim().parse::<i32>().ok()).unwrap_or(0);
        if (zone_team == 2 || zone_team == 3) && team.is_some_and(|t| t != zone_team) {
            return false;
        }
        let local = entity_rotation(z.angles()).inverse() * (p - z.origin());
        z.hulls
            .iter()
            .any(|h| h.planes.iter().all(|(n, d)| n.dot(local) <= *d + 1.0))
    })
}

/// Each registered weapon's slot (built once on a scratch world).
pub fn weapon_slots(registry: &WeaponRegistry) -> Vec<(&'static str, u8)> {
    let mut scratch = World::new();
    registry
        .0
        .iter()
        .filter_map(|d| {
            let mut e = scratch.spawn_empty();
            (d.build)(&mut e);
            e.get::<Weapon>().map(|w| (d.id, w.slot))
        })
        .collect()
}

/// Each registered grenade: (weapon ID, kind, carry limit).
pub fn grenade_kinds(registry: &WeaponRegistry) -> Vec<(&'static str, GrenadeKind, u32)> {
    let mut scratch = World::new();
    registry
        .0
        .iter()
        .filter_map(|d| {
            let mut e = scratch.spawn_empty();
            (d.build)(&mut e);
            e.get::<Throwable>().map(|t| (d.id, t.effect.kind(), t.max))
        })
        .collect()
}

/// Chance a bot buys each grenade it can afford after its gun and armour
/// when it has no `BotBuying` of its own (ours; CS:S's bot buying isn't
/// public): an HE, a flash (a second one at half the chance), a smoke.
pub const BOT_GRENADE_CHANCE: [(GrenadeKind, f32); 3] = [
    (GrenadeKind::Blast, 0.6),
    (GrenadeKind::Flash, 0.5),
    (GrenadeKind::Smoke, 0.3),
];

/// What a computer player wants from the shop (set by its brain: `bot::
/// buy`): preferred primaries in order (empty: the game's
/// `Prices::bot_weights`), the chance of each grenade (HE, flash, smoke),
/// and whether it wants a defusal kit.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct BotBuying {
    pub prefs: Vec<&'static str>,
    pub grenades: [f32; 3],
    pub kit: bool,
}

/// Bots' buying rules (ours, after CS:S bots as seen in play; their code
/// isn't public): a primary is bought only with this much left for
/// kevlar after it (else the bot saves: an eco round), money...
pub const BOT_ARMOUR_RESERVE: u32 = 650;
/// ...unless it has no more than this (pistol-round money): then kevlar, pistol ammo
/// and grenades with the rest.
pub const BOT_PISTOL_MONEY: u32 = 1000;
/// Saving, it still buys a kit (if it wants one) with at least this.
pub const BOT_ECO_KIT_MONEY: u32 = 1500;

/// What a computer player buys with its money (`autobuy_rolling`), with a
/// roll per bot and round.
pub fn autobuy(world: &mut World, owner: Entity) {
    let round = world
        .get_resource::<crate::core::RoundRestarts>()
        .map_or(0, |r| r.0 as u64);
    let seed = crate::core::Seed::of(world.get::<crate::core::Seed>(owner), owner);
    let mut rng = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ round.wrapping_mul(0xD1B5_4A32_D192_ED03) | 1;
    autobuy_rolling(world, owner, &mut || {
        rng ^= rng >> 12;
        rng ^= rng << 25;
        rng ^= rng >> 27;
        (rng.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32 / (1u64 << 24) as f32
    });
}

/// How a bot's round of buying went (`autobuy_rolling`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuyKind {
    /// It had a primary already: armour, ammo, kit, grenades.
    Kept,
    /// Bought a primary, then the rest.
    Full,
    /// Pistol-round money: kevlar, pistol ammo and grenades.
    Pistol,
    /// Saved its money (an eco round).
    Eco,
}

/// What a computer player buys (`BotBuying`, else plain defaults): with a
/// primary already, armour, ammo, a kit and grenades; else its first
/// preferred primary (or one picked by `Prices::bot_weights` among the
/// dearer half) it can afford with `BOT_ARMOUR_RESERVE` left, then ammo, a
/// helmet with kevlar when it can (else kevlar), a kit if it wants one,
/// and grenades by the chances. Short of any primary it saves: nothing but
/// a kit with `BOT_ECO_KIT_MONEY`; with pistol-round money
/// (`BOT_PISTOL_MONEY`), kevlar, pistol ammo and grenades. Each roll in 0..1.
pub fn autobuy_rolling(world: &mut World, owner: Entity, roll: &mut dyn FnMut() -> f32) -> BuyKind {
    let wish = world.get::<BotBuying>(owner).cloned();
    let kind = autobuy_gun(world, owner, wish.as_ref(), roll);
    let money = |world: &World| world.get::<Money>(owner).map_or(u32::MAX, |m| m.0);
    let kit = wish.as_ref().is_some_and(|w| w.kit);
    if kind == BuyKind::Eco {
        if kit && money(world) >= BOT_ECO_KIT_MONEY {
            let _ = buy(world, owner, "defuser");
        }
        return kind;
    }
    // Ammo for what it carries, before armour (a gun is no use empty);
    // a pistol round's money goes on kevlar first.
    if kind == BuyKind::Pistol {
        let _ = buy(world, owner, "vest");
        let _ = buy(world, owner, "secammo");
    } else {
        let _ = buy(world, owner, "primammo");
        let _ = buy(world, owner, "secammo");
        let _ = buy(world, owner, "vesthelm").or_else(|_| buy(world, owner, "vest"));
        if kit {
            let _ = buy(world, owner, "defuser");
        }
    }
    let grenades = grenade_kinds(world.resource::<WeaponRegistry>());
    let chances: [(GrenadeKind, f32); 3] = match &wish {
        Some(w) => [
            (GrenadeKind::Blast, w.grenades[0]),
            (GrenadeKind::Flash, w.grenades[1]),
            (GrenadeKind::Smoke, w.grenades[2]),
        ],
        None => BOT_GRENADE_CHANCE,
    };
    for (kind, chance) in chances {
        let Some((id, _, max)) = grenades.iter().find(|g| g.1 == kind) else {
            continue;
        };
        for n in 0..*max {
            let chance = if n == 0 { chance } else { chance / 2.0 };
            if roll() >= chance || buy(world, owner, id).is_err() {
                break;
            }
        }
    }
    kind
}

fn autobuy_gun(world: &mut World, owner: Entity, wish: Option<&BotBuying>, roll: &mut dyn FnMut() -> f32) -> BuyKind {
    let prices = world.resource::<Prices>().clone();
    let slots = weapon_slots(world.resource::<WeaponRegistry>());
    let team = world.get::<crate::core::Team>(owner).map(|t| t.0);
    let has_money = world.get::<Money>(owner).is_some();
    let money = world.get::<Money>(owner).map_or(u32::MAX, |m| m.0);
    let held: Vec<u8> = world
        .get::<Inventory>(owner)
        .map(|i| i.weapons.iter().filter_map(|w| world.get::<Weapon>(*w)).map(|w| w.slot).collect())
        .unwrap_or_default();
    if held.contains(&0) {
        return BuyKind::Kept;
    }
    // Free buying (no money) keeps nothing back.
    let reserve = if has_money { BOT_ARMOUR_RESERVE.min(prices.vest) } else { 0 };
    let ok = |id: &str, price: u32| {
        price.saturating_add(reserve) <= money && prices.team_only.get(id).is_none_or(|t| Some(*t) == team)
    };
    let primaries: Vec<(&'static str, u32)> = slots
        .iter()
        .filter(|(_, slot)| *slot == 0)
        .filter_map(|(id, _)| Some((*id, *prices.weapons.get(id)?)))
        .collect();
    // The profile's preferences in order: the first it can afford.
    let preferred = wish.and_then(|w| {
        w.prefs
            .iter()
            .find_map(|p| primaries.iter().find(|(id, price)| id == p && ok(id, *price)))
    });
    let chosen = preferred.copied().or_else(|| {
        let weight = |id: &str| prices.bot_weights.get(id).copied().unwrap_or(1.0);
        let mut can: Vec<(&'static str, u32)> = primaries
            .iter()
            .copied()
            .filter(|(id, price)| ok(id, *price) && weight(id) > 0.0)
            .collect();
        // The dearer half (rounded up): spend the money, but not always on
        // the single dearest thing.
        can.sort_by_key(|(id, price)| (std::cmp::Reverse(*price), *id));
        can.truncate(can.len().div_ceil(2));
        let total: f32 = can.iter().map(|(id, _)| weight(id)).sum();
        let mut pick = roll() * total;
        can.iter()
            .find(|(id, _)| {
                pick -= weight(id);
                pick < 0.0
            })
            .or(can.last())
            .copied()
    });
    match chosen {
        Some((id, _)) => {
            let _ = buy(world, owner, id);
            BuyKind::Full
        }
        None if money <= BOT_PISTOL_MONEY => BuyKind::Pistol,
        None => BuyKind::Eco,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> (World, Entity) {
        let mut w = World::new();
        w.init_resource::<Time>();
        w.init_resource::<WeaponRegistry>();
        w.init_resource::<BuyWindow>();
        let mut prices = Prices {
            vest: 650,
            vest_helmet: 1000,
            helmet: 350,
            ..default()
        };
        prices.weapons.insert("g:weapon_rifle", 2500);
        w.insert_resource(prices);
        w.resource_mut::<WeaponRegistry>().0.push(super::super::WeaponDef {
            id: "g:weapon_rifle",
            build: |e| {
                e.insert(Weapon {
                    id: "g:weapon_rifle",
                    slot: 0,
                    owner: None,
                    draw_time: 0.0,
                    max_speed: None,
                });
            },
        });
        let p = w.spawn((Transform::default(), Inventory::default(), Money(3000))).id();
        (w, p)
    }

    /// `info_map_parameters` `buying`: 0 everyone, 1 CTs only, 2
    /// terrorists only, 3 nobody; refused in CS:S's words.
    #[test]
    fn map_parameters_bar_teams_from_buying() {
        use crate::map::entities::{MapEntities, MapEntity};
        let (mut w, p) = world();
        let set = |w: &mut World, buying: &str| {
            w.insert_resource(MapEntities {
                entities: std::sync::Arc::new(vec![MapEntity {
                    keyvalues: vec![
                        ("classname".into(), "info_map_parameters".into()),
                        ("buying".into(), buying.into()),
                    ],
                    hulls: vec![],
                    mover: false,
                    physics: None,
                }]),
                scale: 0.0254,
            });
        };
        // Terrorist (ours 1) and CT (ours 2).
        for (buying, t_ok, ct_ok) in [("0", true, true), ("1", false, true), ("2", true, false), ("3", false, false)] {
            set(&mut w, buying);
            for (team, ok, words) in [(1, t_ok, T_CANT_BUY), (2, ct_ok, CT_CANT_BUY)] {
                w.entity_mut(p).insert((crate::core::Team(team), Money(16000)));
                let got = buy(&mut w, p, "vest");
                w.entity_mut(p).remove::<Armor>();
                if ok {
                    assert!(got.is_ok(), "buying {buying}, team {team}: {got:?}");
                } else {
                    assert_eq!(got, Err(words.to_string()), "buying {buying}, team {team}");
                }
            }
        }
        // Free buying (no money) ignores it, as it does buy zones.
        w.entity_mut(p).remove::<Money>();
        assert!(buy(&mut w, p, "vest").is_ok());
    }

    #[test]
    fn buying_costs_money_and_checks_funds_and_the_window() {
        let (mut w, p) = world();
        w.resource_mut::<Prices>().team_only.insert("g:weapon_rifle", 1);
        w.entity_mut(p).insert(crate::core::Team(2));
        assert!(buy(&mut w, p, "rifle").unwrap_err().contains("team"));
        w.entity_mut(p).insert(crate::core::Team(1));
        assert!(buy(&mut w, p, "rifle").is_ok());
        assert_eq!(w.get::<Money>(p), Some(&Money(500)));
        assert!(buy(&mut w, p, "rifle").unwrap_err().contains("already"));
        assert!(buy(&mut w, p, "vesthelm").unwrap_err().contains("insufficient"));
        assert!(buy(&mut w, p, "vest").is_err(), "650 > 500");
        w.entity_mut(p).insert(Money(2000));
        buy(&mut w, p, "vest").unwrap();
        // Helmet over full kevlar: 350.
        buy(&mut w, p, "vesthelm").unwrap();
        assert_eq!(w.get::<Money>(p), Some(&Money(1000)));
        assert_eq!(w.get::<Armor>(p).map(|a| a.helmet), Some(true));
        // A bot with 2700: saves (the rifle would leave nothing for
        // kevlar); with 3200 the rifle (2500) and kevlar (650).
        let rifle = w.get::<Inventory>(p).unwrap().weapons.clone();
        for r in rifle {
            w.get_mut::<Inventory>(p).unwrap().weapons.retain(|x| *x != r);
        }
        w.entity_mut(p).remove::<Armor>();
        w.entity_mut(p).insert(Money(2700));
        assert_eq!(autobuy_rolling(&mut w, p, &mut || 0.0), BuyKind::Eco);
        assert_eq!(w.get::<Money>(p), Some(&Money(2700)));
        w.entity_mut(p).insert(Money(3200));
        assert_eq!(autobuy_rolling(&mut w, p, &mut || 0.0), BuyKind::Full);
        assert_eq!(w.get::<Money>(p), Some(&Money(50)));
        assert_eq!(w.get::<Armor>(p).map(|a| a.helmet), Some(false));
        w.insert_resource(BuyWindow(Err("buy time is over".into())));
        assert_eq!(buy(&mut w, p, "vest").unwrap_err(), "buy time is over");
    }

    #[test]
    fn ammo_by_the_box_up_to_the_reserve_max() {
        let (mut w, p) = world();
        w.resource_mut::<WeaponRegistry>().0.push(super::super::WeaponDef {
            id: "g:weapon_smg",
            build: |e| {
                e.insert((
                    Weapon {
                        id: "g:weapon_smg",
                        slot: 0,
                        owner: None,
                        draw_time: 0.0,
                        max_speed: None,
                    },
                    super::super::Magazine {
                        clip: 30,
                        size: 30,
                        reserve: 90,
                        reserve_max: 90,
                        reload_time: 1.0,
                        reload_while_held: false,
                    },
                ));
            },
        });
        {
            let mut prices = w.resource_mut::<Prices>();
            prices.weapons.insert("g:weapon_smg", 1500);
            prices.ammo.insert("g:weapon_smg", AmmoBox { price: 80, rounds: 30 });
            prices.starting_reserve.insert("g:weapon_smg", 40);
        }
        let reserve = |w: &mut World| {
            let gun = w.get::<Inventory>(p).unwrap().weapons[0];
            w.get::<super::super::Magazine>(gun).unwrap().reserve
        };
        let set_reserve = |w: &mut World, n: u32| {
            let gun = w.get::<Inventory>(p).unwrap().weapons[0];
            w.get_mut::<super::super::Magazine>(gun).unwrap().reserve = n;
        };
        // Bought for money: the clip only.
        buy(&mut w, p, "smg").unwrap();
        assert_eq!((reserve(&mut w), w.get::<Money>(p).unwrap().0), (0, 1500));
        // One box, then fill: two more boxes reach the max.
        buy(&mut w, p, "buyammo1").unwrap();
        assert_eq!((reserve(&mut w), w.get::<Money>(p).unwrap().0), (30, 1420));
        buy(&mut w, p, "primammo").unwrap();
        assert_eq!((reserve(&mut w), w.get::<Money>(p).unwrap().0), (90, 1260));
        assert_eq!(buy(&mut w, p, "primammo").unwrap_err(), CANNOT_CARRY);
        // A box that doesn't fit whole tops up for the full price.
        set_reserve(&mut w, 75);
        buy(&mut w, p, "buyammo1").unwrap();
        assert_eq!((reserve(&mut w), w.get::<Money>(p).unwrap().0), (90, 1180));
        // Out of money: refused; fill stops when the money runs out.
        set_reserve(&mut w, 0);
        w.entity_mut(p).insert(Money(50));
        assert_eq!(buy(&mut w, p, "primammo").unwrap_err(), INSUFFICIENT_FUNDS);
        w.entity_mut(p).insert(Money(170));
        buy(&mut w, p, "primammo").unwrap();
        assert_eq!((reserve(&mut w), w.get::<Money>(p).unwrap().0), (60, 10));
        // No pistol: no secondary ammo.
        assert!(buy(&mut w, p, "secammo").is_err());
        // Starting weapons carry the game's starting reserve.
        let gun = w.get::<Inventory>(p).unwrap().weapons[0];
        starting_reserve(&mut w, gun);
        assert_eq!(reserve(&mut w), 40);
        // Free buying (no money) fills too.
        w.entity_mut(p).remove::<Money>();
        buy(&mut w, p, "primammo").unwrap();
        assert_eq!(reserve(&mut w), 90);
    }
}
