//! The shop as data (docs/economy-data.md; docs/plans/active/engine-split.md,
//! "Data-driven first"): a game's economy document gives what is for sale
//! besides weapons (`economy::ShopItem`: armour, or a part registered by
//! name, `economy::ItemParts`), the buy menu's layout (`BuyCategory`), the
//! spawn kit per team (`StartingWeapons`), the shop's sounds, whether a gun
//! bought where ammo is sold comes full, and buy-name aliases. Weapons'
//! own prices, teams and bots' weights are their weapon data's `[buy]`
//! (`weapon::data`). The rules of buying (refusals, armour over armour,
//! ammo by the box, bots' buying) stay `economy`'s code. In short:
//!
//! ```toml
//! [sounds]  ammo, pickup, item, night_vision_on, night_vision_off
//! [ammo]    bought_full = true
//! [aliases] mp5 = "mp5navy"
//! [[item]]  id = "game:vest", label = "Kevlar", price = 650,
//!           armor = { amount = 1.0 }, owned = "You already have Kevlar!"
//! [[menu]]  key = 1, name = "Pistols", items = [{ buy = "game:weapon_pistol", label = "Pistol" }]
//! [spawn]   kits = [{ team = 1, weapons = [...] }, { weapons = [...] }], all = [...]
//! ```
//!
//! Documents load after the weapons they name (`add_economy_data`); a
//! menu line names a weapon (as `WeaponRegistry::find` takes it) or an
//! item. Loading adds to what is there: an item or a menu category (by
//! key) of an ID already there replaces it in place, a new one comes
//! last; `[spawn]` replaces the kit. Unknown keys, an unknown weapon,
//! item or part, and an item giving nothing or two things are load errors
//! naming the document.

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::Deserialize;

use super::{
    StartingWeapons, WeaponRegistry,
    economy::{AMMO_BUYS, BuyCategory, BuyItem, ItemGives, ItemParts, Prices, ShopItem},
};

/// One economy document.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EconomyData {
    #[serde(default)]
    pub sounds: ShopSounds,
    #[serde(default)]
    pub ammo: Option<AmmoRules>,
    /// Another name -> the buy name it stands for (`autobuy` lists).
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
    #[serde(default)]
    pub item: Vec<ItemData>,
    #[serde(default)]
    pub menu: Vec<CategoryData>,
    #[serde(default)]
    pub spawn: Option<SpawnData>,
}

/// Sound entries (the game's sound scripts' names); each one given
/// replaces the app's.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ShopSounds {
    /// Ammo bought.
    pub ammo: Option<String>,
    /// A weapon bought or picked up.
    pub pickup: Option<String>,
    /// An item bought.
    pub item: Option<String>,
    /// Night vision turned on and off.
    pub night_vision_on: Option<String>,
    pub night_vision_off: Option<String>,
}

impl ShopSounds {
    /// Every entry given.
    pub fn entries(&self) -> impl Iterator<Item = &str> {
        [
            &self.ammo,
            &self.pickup,
            &self.item,
            &self.night_vision_on,
            &self.night_vision_off,
        ]
        .into_iter()
        .flatten()
        .map(String::as_str)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AmmoRules {
    /// A gun bought for money comes with a full reserve even where ammo is
    /// sold (`Prices::bought_full`).
    pub bought_full: bool,
}

/// Something for sale that isn't a weapon (`ShopItem`).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ItemData {
    /// Namespaced (`game:vest`).
    pub id: String,
    pub label: String,
    pub price: u32,
    #[serde(default)]
    pub team: Option<u8>,
    /// Armour it puts on (one of `armor` and `part`).
    #[serde(default)]
    pub armor: Option<ArmorData>,
    /// A part registered by name (`ItemParts`).
    #[serde(default)]
    pub part: Option<String>,
    /// Why buying it again is refused.
    pub owned: String,
    #[serde(default)]
    pub menu_greys_owned: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ArmorData {
    /// 1: full.
    pub amount: f32,
    #[serde(default)]
    pub helmet: bool,
    /// The helmet alone, over armour already worn.
    #[serde(default)]
    pub helmet_price: Option<u32>,
}

/// A buy-menu category (`BuyCategory`).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CategoryData {
    /// The number key that opens it.
    pub key: u8,
    pub name: String,
    #[serde(default)]
    pub items: Vec<LineData>,
    /// Bought at once instead of opening (an ammo buy, `AMMO_BUYS`, an
    /// item or a weapon).
    #[serde(default)]
    pub direct: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LineData {
    /// A weapon or item ID.
    pub buy: String,
    /// Its name in the menu (an item's own label when not given).
    #[serde(default)]
    pub label: Option<String>,
}

/// The spawn kit (`StartingWeapons`): per team (no team: everyone
/// else), then `all` for everyone; the last given is drawn.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SpawnData {
    #[serde(default)]
    pub kits: Vec<KitData>,
    #[serde(default)]
    pub all: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct KitData {
    #[serde(default)]
    pub team: Option<u8>,
    pub weapons: Vec<String>,
}

impl EconomyData {
    /// Parse one document (`name` for errors).
    pub fn parse(name: &str, text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| format!("{name}: {e}"))
    }
}

impl ItemData {
    /// The item, its part checked against `parts`.
    pub fn build(&self, parts: &ItemParts) -> Result<ShopItem, String> {
        let gives = match (&self.armor, &self.part) {
            (Some(a), None) => ItemGives::Armor {
                amount: a.amount,
                helmet: a.helmet,
                helmet_price: a.helmet_price,
            },
            (None, Some(p)) => {
                if parts.get(p).is_none() {
                    return Err(format!(
                        "item {}: unknown part {p} (registered: {})",
                        self.id,
                        parts.names().join(", ")
                    ));
                }
                ItemGives::Part(p.clone())
            }
            (None, None) => return Err(format!("item {}: gives nothing (`armor` or `part`)", self.id)),
            (Some(_), Some(_)) => return Err(format!("item {}: both `armor` and `part`", self.id)),
        };
        Ok(ShopItem {
            id: self.id.clone(),
            label: self.label.clone(),
            price: self.price,
            team: self.team,
            gives,
            owned: self.owned.clone(),
            menu_greys_owned: self.menu_greys_owned,
        })
    }
}

/// Apply `doc` to `prices` and `start`, resolving weapons in `registry`.
pub fn apply(
    doc: &EconomyData,
    prices: &mut Prices,
    start: &mut StartingWeapons,
    registry: &WeaponRegistry,
    parts: &ItemParts,
) -> Result<(), String> {
    let s = &doc.sounds;
    for (from, to) in [
        (&s.ammo, &mut prices.ammo_sound),
        (&s.pickup, &mut prices.pickup_sound),
        (&s.item, &mut prices.item_sound),
        (&s.night_vision_on, &mut prices.nightvision_on),
        (&s.night_vision_off, &mut prices.nightvision_off),
    ] {
        if from.is_some() {
            to.clone_from(from);
        }
    }
    if let Some(a) = &doc.ammo {
        prices.bought_full = a.bought_full;
    }
    for (from, to) in &doc.aliases {
        prices.aliases.insert(from.to_lowercase(), to.to_lowercase());
    }
    for item in &doc.item {
        if registry.find(&item.id).is_some() {
            return Err(format!("item {}: a weapon has that ID", item.id));
        }
        prices.add_item(item.build(parts)?);
    }
    let weapon = |name: &str| -> Result<&'static str, String> {
        registry
            .find(name)
            .map(|d| d.id)
            .ok_or_else(|| format!("unknown weapon {name}"))
    };
    for c in &doc.menu {
        let mut items = Vec::new();
        for line in &c.items {
            let (buy, label) = match prices.item(&line.buy) {
                Some(i) => (i.id.clone(), line.label.clone().unwrap_or_else(|| i.label.clone())),
                None => {
                    let id = weapon(&line.buy).map_err(|e| format!("menu {}: {e}", c.key))?;
                    let label = line
                        .label
                        .clone()
                        .ok_or_else(|| format!("menu {}: {id} has no label", c.key))?;
                    (id.to_string(), label)
                }
            };
            items.push(BuyItem { buy, label });
        }
        if let Some(d) = &c.direct
            && !AMMO_BUYS.iter().any(|(n, ..)| n.eq_ignore_ascii_case(d))
            && prices.item(d).is_none()
            && registry.find(d).is_none()
        {
            return Err(format!("menu {}: direct {d} buys nothing", c.key));
        }
        let category = BuyCategory {
            key: c.key,
            name: c.name.clone(),
            items,
            direct: c.direct.clone(),
        };
        match prices.menu.iter_mut().find(|m| m.key == c.key) {
            Some(old) => *old = category,
            None => prices.menu.push(category),
        }
    }
    if let Some(spawn) = &doc.spawn {
        let ids = |names: &[String]| names.iter().map(|n| weapon(n)).collect::<Result<Vec<_>, _>>();
        let mut kit = StartingWeapons {
            team: Vec::new(),
            all: ids(&spawn.all).map_err(|e| format!("spawn: {e}"))?,
        };
        for k in &spawn.kits {
            kit.team
                .push((k.team, ids(&k.weapons).map_err(|e| format!("spawn: {e}"))?));
        }
        *start = kit;
    }
    Ok(())
}

pub trait EconomyDataAppExt {
    /// Parse economy documents (`(name, text)`) and add them to the app's
    /// `Prices` and `StartingWeapons`, after the weapons they name. Nothing
    /// changes when one fails.
    fn add_economy_data(&mut self, docs: &[(&str, &str)]) -> Result<(), String>;
}

impl EconomyDataAppExt for App {
    fn add_economy_data(&mut self, docs: &[(&str, &str)]) -> Result<(), String> {
        let parsed = docs
            .iter()
            .map(|(n, t)| Ok((*n, EconomyData::parse(n, t)?)))
            .collect::<Result<Vec<_>, String>>()?;
        let world = self.world_mut();
        let mut prices = world.get_resource_or_init::<Prices>().clone();
        let mut start = world.get_resource_or_init::<StartingWeapons>().clone();
        let parts = world.get_resource_or_init::<ItemParts>().clone();
        let registry = world.get_resource_or_init::<WeaponRegistry>();
        for (name, doc) in &parsed {
            apply(doc, &mut prices, &mut start, &registry, &parts).map_err(|e| format!("{name}: {e}"))?;
        }
        world.insert_resource(prices);
        world.insert_resource(start);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.init_resource::<WeaponRegistry>();
        for id in ["g:weapon_pistol", "g:weapon_rifle"] {
            app.world_mut()
                .resource_mut::<WeaponRegistry>()
                .0
                .push(super::super::WeaponDef {
                    id,
                    build: std::sync::Arc::new(|_: &mut EntityWorldMut| {}),
                });
        }
        app
    }

    const DOC: &str = r#"
        [sounds]
        item = "Item.Bought"
        [aliases]
        gun = "pistol"
        [[item]]
        id = "g:vest"
        label = "Vest"
        price = 650
        armor = { amount = 1.0 }
        owned = "Got one."
        [[item]]
        id = "g:kit"
        label = "Kit"
        price = 200
        team = 2
        part = "defuse_kit"
        owned = "Got a kit."
        [[menu]]
        key = 1
        name = "Guns"
        items = [{ buy = "weapon_pistol", label = "Pistol" },{ buy = "g:weapon_rifle", label = "Rifle" }]
        [[menu]]
        key = 2
        name = "Gear"
        items = [{ buy = "vest" }, { buy = "g:kit", label = "Defuse kit" }]
        [[menu]]
        key = 6
        name = "Ammo"
        direct = "primammo"
        [spawn]
        kits = [{ team = 1, weapons = ["g:weapon_pistol"] }, { weapons = ["g:weapon_rifle"] }]
    "#;

    #[test]
    fn a_document_fills_the_shop_and_the_spawn_kit() {
        let mut app = app();
        app.add_economy_data(&[("doc.toml", DOC)]).unwrap();
        let p = app.world().resource::<Prices>();
        assert_eq!(p.item_sound.as_deref(), Some("Item.Bought"));
        assert_eq!(p.aliases.get("gun").map(String::as_str), Some("pistol"));
        assert_eq!(p.of("vest"), Some(650));
        assert_eq!(p.team_of("kit"), Some(2));
        assert_eq!(p.menu[0].items[0].buy, "g:weapon_pistol");
        assert_eq!(p.menu[1].items[0].label, "Vest");
        assert_eq!(p.menu[1].items[1].buy, "g:kit");
        assert_eq!(p.menu[2].direct.as_deref(), Some("primammo"));
        let start = app.world().resource::<StartingWeapons>();
        assert_eq!(start.for_team(Some(1)), ["g:weapon_pistol"]);
        assert_eq!(start.for_team(Some(2)), ["g:weapon_rifle"]);
        // A second document replaces by ID and key, and adds.
        let more = r#"
            [[item]]
            id = "g:vest"
            label = "Vest"
            price = 500
            armor = { amount = 1.0 }
            owned = "Got one."
            [[menu]]
            key = 2
            name = "Gear"
            items = [{ buy = "g:vest" }]
            [[menu]]
            key = 9
            name = "More"
            items = [{ buy = "g:weapon_rifle", label = "Rifle" }]
        "#;
        app.add_economy_data(&[("more.toml", more)]).unwrap();
        let p = app.world().resource::<Prices>();
        assert_eq!((p.items.len(), p.of("vest")), (2, Some(500)));
        assert_eq!(p.menu.iter().map(|c| c.key).collect::<Vec<_>>(), [1, 2, 6, 9]);
        assert_eq!(p.menu[1].items.len(), 1);
    }

    #[test]
    fn mistakes_are_load_errors_naming_the_document() {
        let bad = [
            (
                "[[item]]\nid = \"g:x\"\nlabel = \"X\"\nprice = 1\nowned = \"\"\ncolour = 1",
                "unknown field",
            ),
            (
                "[[item]]\nid = \"g:x\"\nlabel = \"X\"\nprice = 1\nowned = \"\"",
                "gives nothing",
            ),
            (
                "[[item]]\nid = \"g:x\"\nlabel = \"X\"\nprice = 1\nowned = \"\"\npart = \"jetpack\"",
                "unknown part",
            ),
            (
                "[[menu]]\nkey = 1\nname = \"A\"\nitems = [{ buy = \"g:weapon_nope\", label = \"N\" }]",
                "unknown weapon",
            ),
            (
                "[[menu]]\nkey = 1\nname = \"A\"\nitems = [{ buy = \"g:weapon_rifle\" }]",
                "no label",
            ),
            (
                "[[menu]]\nkey = 6\nname = \"A\"\ndirect = \"thirdammo\"",
                "buys nothing",
            ),
            ("[spawn]\nall = [\"g:weapon_nope\"]", "unknown weapon"),
            ("[shop]\nopen = true", "unknown field"),
        ];
        for (text, why) in bad {
            let mut app = app();
            let e = app.add_economy_data(&[("bad.toml", text)]).unwrap_err();
            assert!(e.starts_with("bad.toml: ") && e.contains(why), "{text}: {e}");
            let items = app.world().get_resource::<Prices>().map_or(0, |p| p.items.len());
            assert_eq!(items, 0, "nothing changes");
        }
    }
}
