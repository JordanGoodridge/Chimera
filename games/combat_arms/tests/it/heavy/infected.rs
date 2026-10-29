//! The gun game's infected from the user's install (`game::rungs`,
//! docs/DESIGN.md "Gun game rules"): the Quarantine hands are marked as
//! an infected's weapons (`rungs::INFECTED`, from their `LowerAniType`)
//! and are knives, no gun rung's melee pool has them, the stock body's
//! infection override names a face the data has, and the infected
//! bodies are rung bodies. Needs the V1 key; passes with a note without.

use combat_arms::game::{data::CaData, inventory::KNIFE_HOOK, rungs};

#[test]
fn the_install_marks_its_infected_hands_and_overrides() {
    let ca = CaData::install();
    if ca.attributes().and_then(|a| a.models.as_ref()).is_none() {
        eprintln!("skipping: the install's attribute data isn't readable (the V1 key)");
        return;
    }
    let set = ca.weapon_set();
    let infected: Vec<&str> = set.weapons.iter().filter(|w| w.has_hook(rungs::INFECTED)).map(|w| w.id.as_str()).collect();
    eprintln!("{} infected's weapons: {infected:?}", infected.len());
    for (index, want) in rungs::STOCK_HANDS.iter().zip(["combat_arms:quarantine_virus_hand", "combat_arms:quarantine_host_hand"]) {
        assert_eq!(ca.weapon_for_index(*index), Some(want), "record {index}");
        assert!(infected.contains(&want), "{want} marked infected");
    }
    assert!(set.weapons.iter().filter(|w| w.has_hook(rungs::INFECTED)).all(|w| w.has_hook(KNIFE_HOOK)), "every hand a knife");
    assert!(!set.weapons.iter().any(|w| w.id == "combat_arms:m9_knife" && w.has_hook(rungs::INFECTED)));
    let ch = ca.characters();
    let o = ch.infection_of(0).expect("Normal's override");
    assert!(ch.face.iter().any(|f| f.id == o.face), "its face {} is in the data", o.face);
    assert_eq!(ch.infected_look(0).face, o.face);
    let bodies = ch.infected_bodies();
    let pool = ch.rung_bodies();
    assert!(!bodies.is_empty() && bodies.iter().all(|b| pool.contains(b)));
    eprintln!("{} of {} rung bodies have an infection override", bodies.len(), pool.len());
}
