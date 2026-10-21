//! The loadout (specs/combat_arms/lobby.md E, P): what a player equips in
//! the inventory screen, the rules for what may go where, the 1st Arms
//! check at READY and START, what the game server keeps of it in a match,
//! the fallbacks when an item expires, and how a set becomes what the
//! game's gear takes (`CaLoadout`, the character's `Selection`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{
    data::{ArmsKind, LobbyData},
    items::{BACKPACK_BASE, CHARACTER_BASE, Catalog, FACE_BASE, HEAD_BASE, Item, Kind, Owned},
};
use crate::game::{characters::records::Selection, gear::CaLoadout};

/// Which of the two sets (E, "loadout_pages").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetKind {
    Normal,
    PureCombat,
}

/// A place in a set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Slot {
    Primary,
    Secondary,
    Melee,
    Support,
    /// Backpack slot A-D, 0 to 3.
    Backpack(u8),
    Character,
    Head,
    Face,
    Uniform,
    Vest,
    BackpackGear,
    /// An ability slot.
    Ability(u8),
}

/// The refusal texts (E).
pub mod text {
    pub const TWO_SAME: &str = "You cannot equip two of the same item at the same time.";
    pub const CANT_TOGETHER: &str =
        "There is an item already equipped that can't be equipped together. Cannot equip this item.";
    pub const WRONG_SLOT: &str = "This item cannot be equipped in that slot.";
    pub const NOT_OWNED: &str = "You do not own this item.";
    pub const NO_SLOT: &str = "You have no backpack slot there.";
    pub const NEVER_EMPTY: &str = "A weapon slot cannot be empty.";
    pub const BASE_ITEM: &str = "A base item cannot be released.";
    pub const NOT_PURE: &str = "This item cannot be used in the Pure Combat set.";
    pub const NO_ABILITY_SLOT: &str = "You have no free ability slot.";
}

/// One loadout set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Set {
    pub primary: i32,
    pub secondary: i32,
    pub melee: i32,
    pub support: i32,
    pub bag: [Option<i32>; 4],
    pub character: i32,
    pub head: Option<i32>,
    pub face: Option<i32>,
    pub uniform: Option<i32>,
    pub vest: Option<i32>,
    pub backpack: Option<i32>,
    /// Ability items in their slots.
    pub abilities: Vec<i32>,
    /// Attachments (item numbers) by the weapon they are on.
    pub attachments: BTreeMap<i32, Vec<i32>>,
}

/// The player's loadout: both sets, the slot extender, and the default
/// weapons they chose for the expiry fallback (primary, secondary, melee,
/// support).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Loadout {
    pub normal: Set,
    pub pure_combat: Set,
    pub extender: bool,
    pub defaults: [Option<i32>; 4],
}

impl Loadout {
    /// A new player's (E, "Defaults for a new player"): M16A3, Beretta
    /// M92FS, M9, M67, Black Cap and the Standard Backpack; the Pure
    /// Combat set the same with no backpack (open question 9).
    pub fn new_player(catalog: &Catalog) -> Self {
        let pick = |kind: fn(Kind) -> bool| catalog.base_for(|i| kind(i.kind)).map_or(0, |i| i.no);
        let normal = Set {
            primary: pick(Kind::primary),
            secondary: pick(Kind::secondary),
            melee: pick(|k| k == Kind::Knife),
            support: pick(|k| k == Kind::Throwable),
            bag: [None; 4],
            character: CHARACTER_BASE,
            head: Some(catalog.base_head),
            face: None,
            uniform: None,
            vest: None,
            backpack: Some(catalog.base_backpack),
            abilities: Vec::new(),
            attachments: BTreeMap::new(),
        };
        let pure_combat = Set {
            backpack: None,
            ..normal.clone()
        };
        Self {
            normal,
            pure_combat,
            extender: false,
            defaults: [None; 4],
        }
    }

    pub fn set(&self, kind: SetKind) -> &Set {
        match kind {
            SetKind::Normal => &self.normal,
            SetKind::PureCombat => &self.pure_combat,
        }
    }

    pub fn set_mut(&mut self, kind: SetKind) -> &mut Set {
        match kind {
            SetKind::Normal => &mut self.normal,
            SetKind::PureCombat => &mut self.pure_combat,
        }
    }
}

impl Set {
    /// The item in a slot, if any.
    pub fn get(&self, slot: Slot) -> Option<i32> {
        match slot {
            Slot::Primary => Some(self.primary),
            Slot::Secondary => Some(self.secondary),
            Slot::Melee => Some(self.melee),
            Slot::Support => Some(self.support),
            Slot::Backpack(i) => self.bag.get(i as usize).copied().flatten(),
            Slot::Character => Some(self.character),
            Slot::Head => self.head,
            Slot::Face => self.face,
            Slot::Uniform => self.uniform,
            Slot::Vest => self.vest,
            Slot::BackpackGear => self.backpack,
            Slot::Ability(i) => self.abilities.get(i as usize).copied(),
        }
    }

    /// Every equipped item number (weapons, gear, abilities).
    pub fn equipped(&self) -> Vec<i32> {
        let mut v = vec![self.primary, self.secondary, self.melee, self.support];
        v.extend(self.bag.iter().flatten());
        v.push(self.character);
        v.extend([self.head, self.face, self.uniform, self.vest, self.backpack].into_iter().flatten());
        v
    }

    /// How many backpack slots are usable: the equipped backpack's `Slot`
    /// (E; the character's own are none here).
    pub fn usable_bag_slots(&self, catalog: &Catalog) -> usize {
        self.backpack.and_then(|b| catalog.get(b)).map_or(0, |i| i.slots as usize).min(4)
    }

    /// The weapons carried, in loadout order (primary, secondary, melee,
    /// support, then the backpack's usable slots).
    pub fn weapons(&self, catalog: &Catalog) -> Vec<i32> {
        let mut v = vec![self.primary, self.secondary, self.melee, self.support];
        let n = self.usable_bag_slots(catalog);
        v.extend(self.bag.iter().take(n).flatten());
        v
    }
}

/// Which item types a slot takes (E, "What may go where"): special and
/// assassin items only in a backpack slot.
pub fn slot_accepts(slot: Slot, k: Kind) -> bool {
    let ok = match slot {
        Slot::Primary => k.primary(),
        Slot::Secondary => k.secondary(),
        Slot::Melee => k == Kind::Knife,
        Slot::Support => k == Kind::Throwable,
        Slot::Backpack(_) => k.weapon() && k != Kind::Knife,
        Slot::Character => k == Kind::Character,
        Slot::Head => k == Kind::Head,
        Slot::Face => k == Kind::Face,
        Slot::Uniform => k == Kind::Uniform,
        Slot::Vest => k == Kind::Vest,
        Slot::BackpackGear => k == Kind::Backpack,
        Slot::Ability(_) => false,
    };
    ok && !(k.backpack_only() && !matches!(slot, Slot::Backpack(_)))
}

fn release_weapon_slot(slot: Slot) -> bool {
    matches!(slot, Slot::Primary | Slot::Secondary | Slot::Melee | Slot::Support)
}

impl Loadout {
    /// Equip `item` into `slot` of a set (E): double-click or the equip
    /// button. Refused (with the message the screen shows) when the slot
    /// does not take the item, the item is not owned, it is already
    /// equipped or forbidden by an equipped one, or the set is Pure
    /// Combat and the item is not on its list.
    pub fn equip(&mut self, catalog: &Catalog, owned: &Owned, kind: SetKind, slot: Slot, item: i32) -> Result<(), &'static str> {
        let it = catalog.get(item).ok_or(text::NOT_OWNED)?;
        if !owned.owns(catalog, item) {
            return Err(text::NOT_OWNED);
        }
        if kind == SetKind::PureCombat && !it.pure_combat {
            return Err(text::NOT_PURE);
        }
        let k = it.kind;
        if !slot_accepts(slot, k) {
            return Err(text::WRONG_SLOT);
        }
        let set = self.set(kind);
        if let Slot::Backpack(i) = slot
            && i as usize >= set.usable_bag_slots(catalog)
        {
            return Err(text::NO_SLOT);
        }
        // The same item twice, unless its type allows it; an item in the
        // slot being replaced doesn't count.
        let mut others = set.equipped();
        if let Some(cur) = set.get(slot)
            && let Some(p) = others.iter().position(|n| *n == cur)
        {
            others.remove(p);
        }
        if !it.duplicates && others.contains(&item) {
            return Err(text::TWO_SAME);
        }
        for o in &others {
            let Some(other) = catalog.get(*o) else { continue };
            if other.cant_with.contains(&item) || it.cant_with.contains(o) {
                return Err(text::CANT_TOGETHER);
            }
        }
        let set = self.set_mut(kind);
        match slot {
            Slot::Primary => set.primary = item,
            Slot::Secondary => set.secondary = item,
            Slot::Melee => set.melee = item,
            Slot::Support => set.support = item,
            Slot::Backpack(i) => set.bag[i as usize] = Some(item),
            Slot::Character => set.character = item,
            Slot::Head => set.head = Some(item),
            Slot::Face => set.face = Some(item),
            Slot::Uniform => set.uniform = Some(item),
            Slot::Vest => set.vest = Some(item),
            Slot::BackpackGear => {
                set.backpack = Some(item);
                // Weapons in slots the new backpack lacks are released.
                let n = catalog.get(item).map_or(0, |i| i.slots as usize);
                for b in set.bag.iter_mut().skip(n) {
                    *b = None;
                }
            }
            Slot::Ability(_) => unreachable!(),
        }
        Ok(())
    }

    /// Equip an ability (perk) item: it must fit the free ability slots
    /// (three, four with the extender).
    pub fn equip_ability(&mut self, gear: &crate::game::gear::records::GearData, kind: SetKind, item: i32) -> Result<(), &'static str> {
        let slots = if self.extender { 4 } else { 3 };
        let set = self.set_mut(kind);
        if set.abilities.contains(&item) {
            return Err(text::TWO_SAME);
        }
        let mut next = set.abilities.clone();
        next.push(item);
        if !crate::game::gear::perks_fit(gear, &next, slots) {
            return Err(text::NO_ABILITY_SLOT);
        }
        set.abilities = next;
        Ok(())
    }

    /// Release a slot: weapon slots can never be empty and a base item is
    /// never released; gear can be. Releasing the backpack also releases
    /// the backpack-slot weapons; releasing the character releases what
    /// depends on it (nothing here).
    pub fn release(&mut self, catalog: &Catalog, kind: SetKind, slot: Slot) -> Result<(), &'static str> {
        if release_weapon_slot(slot) {
            return Err(text::NEVER_EMPTY);
        }
        let set = self.set_mut(kind);
        if let Some(n) = set.get(slot)
            && catalog.get(n).is_some_and(|i| i.base)
            && !matches!(slot, Slot::Backpack(_) | Slot::BackpackGear)
        {
            return Err(text::BASE_ITEM);
        }
        match slot {
            Slot::Backpack(i) => set.bag[i as usize] = None,
            Slot::Head => set.head = None,
            Slot::Face => set.face = None,
            Slot::Uniform => set.uniform = None,
            Slot::Vest => set.vest = None,
            Slot::BackpackGear => {
                set.backpack = None;
                set.bag = [None; 4];
            }
            Slot::Ability(i) => {
                if (i as usize) < set.abilities.len() {
                    set.abilities.remove(i as usize);
                }
            }
            Slot::Character => return Err(text::BASE_ITEM),
            _ => unreachable!(),
        }
        Ok(())
    }

    /// An item expired (P): it leaves every set. A weapon slot falls back
    /// to the player's chosen default weapon for it, else the base item; a
    /// backpack-slot weapon is removed; the backpack falls back to the
    /// Standard Backpack (normal set) or none (Pure Combat), releasing the
    /// backpack-slot weapons that no longer fit; attachments leave the
    /// weapon; abilities bound to it are released. Returns the weapon
    /// slots that fell back, for the next inventory visit's auto-equip.
    pub fn expired(&mut self, catalog: &Catalog, gone: &[i32]) -> Vec<(SetKind, Slot)> {
        let mut fell = Vec::new();
        let defaults = self.defaults;
        for kind in [SetKind::Normal, SetKind::PureCombat] {
            let set = self.set_mut(kind);
            let base = |pick: fn(&Item) -> bool, idx: usize| -> i32 {
                defaults[idx]
                    .filter(|d| !gone.contains(d))
                    .or_else(|| catalog.base_for(pick).map(|i| i.no))
                    .unwrap_or(0)
            };
            if gone.contains(&set.primary) {
                set.primary = base(|i| i.kind.primary(), 0);
                fell.push((kind, Slot::Primary));
            }
            if gone.contains(&set.secondary) {
                set.secondary = base(|i| i.kind.secondary(), 1);
                fell.push((kind, Slot::Secondary));
            }
            if gone.contains(&set.melee) {
                set.melee = base(|i| i.kind == Kind::Knife, 2);
                fell.push((kind, Slot::Melee));
            }
            if gone.contains(&set.support) {
                set.support = base(|i| i.kind == Kind::Throwable, 3);
                fell.push((kind, Slot::Support));
            }
            for b in set.bag.iter_mut() {
                if b.is_some_and(|n| gone.contains(&n)) {
                    *b = None;
                }
            }
            if set.backpack.is_some_and(|b| gone.contains(&b)) {
                set.backpack = (kind == SetKind::Normal).then_some(catalog.base_backpack);
                let n = set.usable_bag_slots(catalog);
                for b in set.bag.iter_mut().skip(n) {
                    *b = None;
                }
            }
            for slot in [&mut set.head, &mut set.face, &mut set.uniform, &mut set.vest] {
                if slot.is_some_and(|n| gone.contains(&n)) {
                    *slot = None;
                }
            }
            if gone.contains(&set.character) {
                set.character = CHARACTER_BASE;
            }
            set.abilities.retain(|a| !gone.contains(a));
            for g in gone {
                set.attachments.remove(g);
            }
            for list in set.attachments.values_mut() {
                list.retain(|a| !gone.contains(a));
            }
        }
        fell
    }

    /// The next inventory visit re-equips an expired slot of the normal
    /// set with the best owned, unequipped item: first the same weapon
    /// type as the expired one, then the higher grade, then the type
    /// priority, then (the spec says at random) the lowest number.
    pub fn auto_equip(&mut self, catalog: &Catalog, owned: &Owned, expired: &[Item], fell: &[(SetKind, Slot)]) {
        for (kind, slot) in fell.iter().copied().filter(|(k, _)| *k == SetKind::Normal) {
            let was = expired.iter().find(|e| match slot {
                Slot::Primary => e.kind.primary(),
                Slot::Secondary => e.kind.secondary(),
                Slot::Melee => e.kind == Kind::Knife,
                Slot::Support => e.kind == Kind::Throwable,
                _ => false,
            });
            let set = self.set(kind);
            let equipped = set.equipped();
            let mut best: Option<&Item> = None;
            for c in catalog.items.values() {
                let fits = match slot {
                    Slot::Primary => c.kind.primary(),
                    Slot::Secondary => c.kind.secondary(),
                    Slot::Melee => c.kind == Kind::Knife,
                    Slot::Support => c.kind == Kind::Throwable,
                    _ => false,
                };
                if !fits || equipped.contains(&c.no) || !owned.owns(catalog, c.no) {
                    continue;
                }
                let rank = |i: &Item| (was.is_none_or(|w| w.kind != i.kind), std::cmp::Reverse(i.grade), i.kind.priority(), i.no);
                if best.is_none_or(|b| rank(c) < rank(b)) {
                    best = Some(c);
                }
            }
            if let Some(b) = best {
                let n = b.no;
                let set = self.set_mut(kind);
                match slot {
                    Slot::Primary => set.primary = n,
                    Slot::Secondary => set.secondary = n,
                    Slot::Melee => set.melee = n,
                    Slot::Support => set.support = n,
                    _ => {}
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The arms options

/// Whether an equipped weapon passes a "only" option's test in the READY
/// check, or an "except" option's (E, "The 1st Arms check").
fn ready_pass(id: u8, i: &Item) -> bool {
    match id {
        1 => i.kind == Kind::Pistol,
        // A knife: always true.
        2 => true,
        3 => i.kind.primary() && i.kind != Kind::Sniper,
        4 => i.kind.primary() && !matches!(i.kind, Kind::Rocket) && !i.explosive,
        // Outside the backpack: always true.
        5 => true,
        6 => i.kind == Kind::Sniper,
        7 => matches!(i.kind, Kind::Shotgun | Kind::Shotgun2),
        // No Shotguns' readiness ignores secondary-slot shotguns (quirk
        // 11): a primary-class weapon that is not a primary shotgun.
        8 => i.kind.primary() && i.kind != Kind::Shotgun,
        _ => true,
    }
}

/// The refusal for an arms option (E: "You do not have a pistol equipped
/// for 'Pistols Only' mode." and the like; the others are ours).
fn ready_message(data: &LobbyData, id: u8) -> String {
    let name = data.arms_option(id).map_or(String::new(), |a| a.name.clone());
    let what = match id {
        1 => "pistol",
        3 => "primary weapon other than a sniper rifle",
        4 => "primary weapon without explosives",
        6 => "sniper rifle",
        7 => "shotgun",
        8 => "primary weapon other than a shotgun",
        _ => "weapon",
    };
    format!("You do not have a {what} equipped for '{name}' mode.")
}

/// READY and START check the loadout against the room's arms (E): at
/// least one equipped weapon passes; with a 2nd Arms, "only" options pass
/// if either passes and "except" options need both.
pub fn ready_check(data: &LobbyData, catalog: &Catalog, set: &Set, arms: [u8; 2]) -> Result<(), String> {
    let options: Vec<u8> = arms.iter().copied().filter(|a| *a != 0).collect();
    if options.is_empty() {
        return Ok(());
    }
    let weapons: Vec<(&Item, bool)> = set
        .weapons(catalog)
        .iter()
        .enumerate()
        .filter_map(|(n, w)| catalog.get(*w).map(|i| (i, n >= 4)))
        .collect();
    let passes = |id: u8| weapons.iter().any(|(i, _)| ready_pass(id, i));
    let kind_of = |id: u8| data.arms_option(id).map_or(ArmsKind::None, |a| a.kind);
    let only = options.iter().filter(|a| kind_of(**a) == ArmsKind::Only);
    let except = options.iter().filter(|a| kind_of(**a) == ArmsKind::Except);
    let only_ok = only.clone().next().is_none() || only.clone().any(|a| passes(*a));
    let except_ok = except.clone().all(|a| passes(*a));
    if only_ok && except_ok {
        return Ok(());
    }
    let failed = options.iter().find(|a| !passes(**a)).copied().unwrap_or(options[0]);
    Err(ready_message(data, failed))
}

/// Whether the game server keeps a weapon under one arms option (E, "What
/// the game server keeps").
fn keeps(id: u8, i: &Item, in_bag: bool) -> bool {
    match id {
        1 => matches!(i.kind, Kind::Pistol | Kind::Knife),
        2 => i.kind == Kind::Knife,
        3 => i.kind != Kind::Sniper,
        4 => !(i.kind == Kind::Throwable || i.kind == Kind::Rocket || i.explosive),
        5 => !in_bag,
        6 => matches!(i.kind, Kind::Sniper | Kind::Knife),
        7 => matches!(i.kind, Kind::Shotgun | Kind::Shotgun2 | Kind::Knife),
        8 => !matches!(i.kind, Kind::Shotgun | Kind::Shotgun2),
        _ => true,
    }
}

/// The weapons (item numbers, in loadout order) the game server builds the
/// in-match inventory from: each dropped if the arms option forbids it;
/// with a 2nd Arms "only" options keep what either allows and "except"
/// options drop what either forbids. With nothing left the knife stays.
pub fn match_weapons(data: &LobbyData, catalog: &Catalog, set: &Set, arms: [u8; 2]) -> Vec<i32> {
    let options: Vec<u8> = arms.iter().copied().filter(|a| *a != 0).collect();
    let kind_of = |id: u8| data.arms_option(id).map_or(ArmsKind::None, |a| a.kind);
    let mut kept = Vec::new();
    for (n, w) in set.weapons(catalog).into_iter().enumerate() {
        let Some(i) = catalog.get(w) else { continue };
        let bag = n >= 4;
        let only: Vec<u8> = options.iter().copied().filter(|a| kind_of(*a) == ArmsKind::Only).collect();
        let except: Vec<u8> = options.iter().copied().filter(|a| kind_of(*a) == ArmsKind::Except).collect();
        let ok_only = only.is_empty() || only.iter().any(|a| keeps(*a, i, bag));
        let ok_except = except.iter().all(|a| keeps(*a, i, bag));
        if ok_only && ok_except {
            kept.push(w);
        }
    }
    if kept.is_empty() {
        kept.push(set.melee);
    }
    kept
}

impl Loadout {
    /// A loadout from a client, checked again as the lobby does: each
    /// slot replayed through `equip` on a new player's sets, what a rule
    /// refuses left at its default.
    pub fn sanitized(&self, catalog: &Catalog, owned: &Owned) -> Loadout {
        let mut out = Loadout::new_player(catalog);
        out.extender = self.extender;
        out.defaults = self.defaults;
        for kind in [SetKind::Normal, SetKind::PureCombat] {
            let from = self.set(kind).clone();
            let order = [
                (Slot::Primary, Some(from.primary)),
                (Slot::Secondary, Some(from.secondary)),
                (Slot::Melee, Some(from.melee)),
                (Slot::Support, Some(from.support)),
                (Slot::BackpackGear, from.backpack),
                (Slot::Backpack(0), from.bag[0]),
                (Slot::Backpack(1), from.bag[1]),
                (Slot::Backpack(2), from.bag[2]),
                (Slot::Backpack(3), from.bag[3]),
                (Slot::Character, Some(from.character)),
                (Slot::Head, from.head),
                (Slot::Face, from.face),
                (Slot::Uniform, from.uniform),
                (Slot::Vest, from.vest),
            ];
            for (slot, item) in order {
                if let Some(item) = item {
                    let _ = out.equip(catalog, owned, kind, slot, item);
                }
            }
            for a in &from.abilities {
                let _ = out.equip_ability(&catalog.gear, kind, *a);
            }
            out.set_mut(kind).attachments = from.attachments;
        }
        out
    }
}

// ---------------------------------------------------------------------------
// To the game's gear

/// A set as what the game's gear takes: the character's look and the
/// `CaLoadout`'s perks and equipped items (weapons, gear and
/// attachments, for the set tiers and the attachments).
pub fn to_gear(catalog: &Catalog, set: &Set, base: Selection, extender: bool) -> (Selection, CaLoadout) {
    let id = |item: Option<i32>, offset: i32| item.filter(|n| catalog.get(*n).is_some()).map(|n| n - offset);
    let look = Selection {
        model: set.character - CHARACTER_BASE,
        head: id(set.head, HEAD_BASE).unwrap_or(0),
        goggle: id(set.face, FACE_BASE).unwrap_or(-1),
        backpack: id(set.backpack, BACKPACK_BASE).unwrap_or(-1),
        ..base
    };
    let mut items = set.equipped();
    for list in set.attachments.values() {
        items.extend(list);
    }
    let loadout = CaLoadout {
        perks: set.abilities.clone(),
        items,
        extender,
        ..CaLoadout::default()
    };
    (look, loadout)
}
