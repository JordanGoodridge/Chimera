//! The shop as data (docs/economy-data.md): an item and a buy-menu
//! category defined only in a data file
//! (games/cs_source/tests/data/shop/specials.toml) are added on top of
//! CS:S's shop and bought, with no Rust naming them.

use std::path::Path;

use crate::stack::{
    core::Team,
    games::cs_source::{TICK_INTERVAL, weapons::CsWeaponsPlugin},
    greybox::{self, GreyboxMapPlugin},
    harness::{CsSim, Sim},
    movement::placeholder,
    weapon::{
        Armor, Inventory, Weapon,
        economy::{Money, Prices, buy},
        economy_data::EconomyDataAppExt,
    },
};

#[test]
fn an_item_and_a_menu_category_from_data_are_bought() {
    let mut sim = Sim::new(CsSim, (GreyboxMapPlugin, CsWeaponsPlugin));
    sim.set_tick_interval(TICK_INTERVAL);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/shop/specials.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    sim.app.add_economy_data(&[("specials.toml", &text)]).unwrap();
    // The new category comes after CS:S's eight, which stay.
    let prices = sim.app.world().resource::<Prices>().clone();
    let keys: Vec<u8> = prices.menu.iter().map(|c| c.key).collect();
    assert_eq!(keys, [1, 2, 3, 4, 5, 6, 7, 8, 9]);
    let specials = &prices.menu[8];
    assert_eq!(specials.name, "Specials");
    let lines: Vec<(&str, &str)> = specials
        .items
        .iter()
        .map(|i| (i.buy.as_str(), i.label.as_str()))
        .collect();
    assert_eq!(
        lines,
        [
            ("mashup_test:light_vest", "Light Vest"),
            ("cs_source:weapon_deagle", "Hand Cannon")
        ]
    );
    assert_eq!(prices.of("light_vest"), Some(300));
    // Bought from the menu's lines: half armour, then the pistol.
    let p = sim.spawn_character(greybox::SPAWNS[0], placeholder::ID);
    sim.app.world_mut().entity_mut(p).insert((Team(2), Money(3000)));
    sim.ticks(1);
    let w = sim.app.world_mut();
    for line in &specials.items {
        buy(w, p, &line.buy).unwrap();
    }
    assert_eq!(w.get::<Money>(p).unwrap().0, 3000 - 300 - 650);
    assert_eq!(
        w.get::<Armor>(p).copied(),
        Some(Armor {
            amount: 0.5,
            helmet: false
        })
    );
    let held: Vec<&str> = w
        .get::<Inventory>(p)
        .unwrap()
        .weapons
        .iter()
        .filter_map(|e| w.get::<Weapon>(*e).map(|x| x.id))
        .collect();
    assert!(held.contains(&"cs_source:weapon_deagle"), "{held:?}");
    // Owned: refused in the data's words; CS:S's kevlar still tops it up.
    assert_eq!(buy(w, p, "light_vest").unwrap_err(), "You already have a light vest.");
    buy(w, p, "vest").unwrap();
    assert_eq!(w.get::<Armor>(p).map(|a| a.amount), Some(1.0));
    assert_eq!(w.get::<Money>(p).unwrap().0, 3000 - 300 - 650 - 650);
}
