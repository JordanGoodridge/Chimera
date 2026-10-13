//! Switching weapons as CS:S players do besides the slot keys: `invnext`
//! / `invprev` (the mouse wheel's commands) step through every weapon,
//! slot by slot, and a pick names one weapon in a shared slot (the second
//! grenade); buying and picking up play the game's sounds.

use bevy::{
    ecs::message::{MessageCursor, Messages},
    prelude::*,
};
use mashup::{
    client::weapon_select::{WeaponMenu, WeaponSelectPlugin},
    console::Console,
    core::{Intent, LocalPlayer, Team},
    games::cs_source::{
        TICK_INTERVAL,
        grenades::{FLASHBANG, HEGRENADE},
        weapons::{AK47, CsWeaponsPlugin, ITEM_SOUND, KNIFE, PICKUP_SOUND},
    },
    greybox::{self, GreyboxMapPlugin},
    harness::Sim,
    map::GameSound,
    movement::placeholder,
    weapon::{Inventory, Weapon, economy::buy, give},
};

fn sim() -> (Sim, Entity) {
    let mut sim = Sim::with(|app| {
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .add_plugins((GreyboxMapPlugin, CsWeaponsPlugin, WeaponSelectPlugin));
    });
    sim.set_tick_interval(TICK_INTERVAL);
    let p = sim.spawn_character(greybox::SPAWNS[0], placeholder::ID);
    sim.app.world_mut().entity_mut(p).insert((LocalPlayer, Team(2)));
    sim.ticks(2);
    (sim, p)
}

fn active_id(sim: &Sim, p: Entity) -> &'static str {
    let w = sim.app.world();
    let a = w.get::<Inventory>(p).unwrap().active.expect("no active weapon");
    w.get::<Weapon>(a).unwrap().id
}

/// Run a console command, then pass the weapon selection's pick to the
/// intent (as the local input does) until the switch is done.
fn command(sim: &mut Sim, p: Entity, line: &str) {
    sim.app.world_mut().resource_mut::<Console>().submit(line);
    for _ in 0..60 {
        sim.ticks(1);
        let pick = sim.app.world().resource::<WeaponMenu>().selected_slot();
        sim.intent(p).select = pick;
    }
}

#[test]
fn invnext_and_invprev_step_through_every_weapon_by_slot() {
    let (mut sim, p) = sim();
    give(sim.app.world_mut(), p, HEGRENADE).unwrap();
    give(sim.app.world_mut(), p, FLASHBANG).unwrap();
    // The rifle in hand (a grenade given to a fresh character may be
    // drawn first).
    sim.intent(p).select = Some(0);
    sim.ticks(60);
    sim.intent(p).select = None;
    sim.ticks(2);
    assert_eq!(active_id(&sim, p), AK47);
    // Fast switch (no game HUD here): each step switches at once.
    command(&mut sim, p, "invprev");
    assert_eq!(active_id(&sim, p), FLASHBANG, "back from the first slot wraps to the last weapon");
    command(&mut sim, p, "invprev");
    assert_eq!(active_id(&sim, p), HEGRENADE, "the other weapon in the same slot");
    command(&mut sim, p, "invnext");
    assert_eq!(active_id(&sim, p), FLASHBANG);
    command(&mut sim, p, "invnext");
    assert_eq!(active_id(&sim, p), AK47, "wraps to the first slot");
    command(&mut sim, p, "invnext; invnext");
    assert_eq!(active_id(&sim, p), KNIFE, "two notches at once: pistol, then knife");
}

#[test]
fn a_pick_draws_the_second_weapon_in_a_shared_slot() {
    let (mut sim, p) = sim();
    give(sim.app.world_mut(), p, HEGRENADE).unwrap();
    give(sim.app.world_mut(), p, FLASHBANG).unwrap();
    sim.ticks(2);
    let slot = {
        let w = sim.app.world();
        let inv = w.get::<Inventory>(p).unwrap();
        let flash = inv.weapons.iter().find(|e| w.get::<Weapon>(**e).unwrap().id == FLASHBANG).unwrap();
        w.get::<Weapon>(*flash).unwrap().slot
    };
    // The slot key draws the first grenade; a pick names the second.
    sim.intent(p).select = Some(slot);
    sim.ticks(60);
    assert_eq!(active_id(&sim, p), HEGRENADE);
    sim.intent(p).select = None;
    sim.ticks(2);
    sim.intent(p).select = Some(Intent::pick(slot, 1));
    sim.ticks(60);
    assert_eq!(active_id(&sim, p), FLASHBANG);
}

/// The rules' sounds (`GameSound`: buying, picking up) sent over `n`
/// ticks, each once (`rules` keeps its place between calls).
fn heard(sim: &mut Sim, rules: &mut MessageCursor<GameSound>, n: u32) -> Vec<String> {
    let mut out = Vec::new();
    for _ in 0..n {
        sim.ticks(1);
        let w = sim.app.world();
        out.extend(rules.read(w.resource::<Messages<GameSound>>()).map(|s| s.0.entry.clone()));
    }
    out
}

#[test]
fn buying_and_picking_up_play_the_pickup_sounds() {
    let (mut sim, p) = sim();
    let mut rules = MessageCursor::<GameSound>::default();
    heard(&mut sim, &mut rules, 2);
    // A grenade: nothing is dropped for it.
    buy(sim.app.world_mut(), p, "hegrenade").unwrap();
    let after_gun = heard(&mut sim, &mut rules, 2);
    assert!(after_gun.iter().any(|e| e == PICKUP_SOUND), "{after_gun:?}");
    buy(sim.app.world_mut(), p, "vesthelm").unwrap();
    let after_vest = heard(&mut sim, &mut rules, 2);
    assert!(after_vest.iter().any(|e| e == ITEM_SOUND), "{after_vest:?}");
    assert!(!after_vest.iter().any(|e| e == PICKUP_SOUND), "{after_vest:?}");
    // A gun walked over: dropped, then taken back after the touch delay.
    let gun = mashup::weapon::drop::drop_weapon(sim.app.world_mut(), p, true).expect("dropped");
    let at = sim.position(p);
    let mut picked = Vec::new();
    for _ in 0..150 {
        if let Some(mut t) = sim.app.world_mut().get_mut::<Transform>(gun) {
            t.translation = at;
        }
        picked.extend(heard(&mut sim, &mut rules, 1));
    }
    assert!(sim.app.world().get_entity(gun).is_err(), "picked up");
    assert_eq!(picked.iter().filter(|e| *e == PICKUP_SOUND).count(), 1, "{picked:?}");
}
