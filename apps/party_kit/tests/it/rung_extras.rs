//! The gun game's Combat Arms rungs' extras (`combat_arms::game::rungs`
//! on Chimera's `ladder::RungExtrasHook`): on the party's simulation, a
//! Combat Arms rung gives everyone on it the same fitted parts on its
//! gun, the same character and the same melee weapon (in place of the
//! kit's M9), again after a respawn; CS:S's rungs draw none and keep
//! their knife and the players' own looks; another seed draws others;
//! each option off gives today's (the M9, one's own look, the gun's own
//! parts); over the network a client's player wears the server's look
//! and parts.

use bevy::prelude::*;
use chimera_core::core::{Damage, DamageKind, Hitgroup};
use chimera_net::{harness::NetSim, net::memory::LinkConditions};
use chimera_sim::{
    minigame::{
        self, MinigameAppExt, Minigames,
        ladder::{self, Ladder, RungSeed},
    },
    testkit,
    weapon::{Inventory, Weapon, data::WeaponCatalog},
};
use chimera_world::{greybox::GreyboxMapPlugin, harness::Sim};
use combat_arms::{
    game::{
        TICK_INTERVAL,
        attachments::{CaFitted, CaModTable},
        characters::{CaRungLook, records::Selection},
        inventory::KNIFE as CA_KNIFE,
        rungs,
    },
    harness::CaSim,
};
use party_kit::{DEFAULT_MINIGAME, PartyPartsPlugin};

use super::gun_game::{alive, soldier};

/// The ladder's M4A1 rung (it accepts parts without the install).
const M4: usize = 7;

/// The gun game with `[rules]` lines `extra` added (as `kit:variant`),
/// started on seed `seed`.
fn party_with(extra: &str, seed: u64) -> Sim {
    let mut sim = Sim::new(CaSim, (GreyboxMapPlugin, PartyPartsPlugin));
    sim.app.insert_resource(RungSeed(seed));
    let text = sim.app.world().resource::<Minigames>().text(DEFAULT_MINIGAME).unwrap().to_string();
    let text = text
        .replace(&format!("id = \"{DEFAULT_MINIGAME}\""), "id = \"originals:variant\"")
        .replace("\nrung_extras = true\n", "\n")
        .replace("[rules]\n", &format!("[rules]\n{extra}\n"));
    sim.app.add_minigames(&[("variant.toml", &text)]).unwrap();
    minigame::start(sim.app.world_mut(), "originals:variant").expect("it starts");
    sim
}

fn ladder_of(sim: &Sim) -> Ladder {
    sim.app.world().resource::<Ladder>().clone()
}

/// Both players in, put on `rung`.
fn two_on(sim: &mut Sim, rung: usize) -> (Entity, Entity) {
    let a = soldier(sim);
    let b = soldier(sim);
    sim.seconds(1.5);
    let l = ladder_of(sim);
    for p in [a, b] {
        ladder::set_rung(sim.app.world_mut(), p, rung, &l);
    }
    sim.seconds(1.0);
    (a, b)
}

fn carried(sim: &Sim, p: Entity) -> Vec<&'static str> {
    let w = sim.app.world();
    w.get::<Inventory>(p).unwrap().weapons.iter().filter_map(|x| w.get::<Weapon>(*x)).map(|x| x.id).collect()
}

/// The melee weapon `p` carries.
fn melee(sim: &Sim, p: Entity) -> Option<&'static str> {
    let catalog = sim.app.world().resource::<WeaponCatalog>();
    carried(sim, p).into_iter().find(|id| catalog.get(id).is_some_and(|w| w.melee.is_some() || w.has_hook(combat_arms::game::inventory::KNIFE_HOOK)))
}

/// The fitted set of `p`'s weapon `id`.
fn fitted(world: &World, p: Entity, id: &str) -> Option<CaFitted> {
    let inv = world.get::<Inventory>(p)?;
    inv.weapons.iter().find(|w| world.get::<Weapon>(**w).is_some_and(|x| x.id == id)).and_then(|w| world.get::<CaFitted>(*w)).copied()
}

/// The record IDs of the parts rung `rung` names, by slot.
fn rung_parts(sim: &Sim, rung: usize) -> Vec<u16> {
    let l = ladder_of(sim);
    let table = sim.app.world().resource::<CaModTable>();
    let parts = l.extras(rung).and_then(|x| x.value(rungs::PARTS)).unwrap_or_default();
    parts.split(',').filter(|p| !p.is_empty()).filter_map(|p| table.find(p)).collect()
}

fn look(sim: &Sim, p: Entity) -> Option<Selection> {
    sim.app.world().get::<Selection>(p).copied()
}

fn kill(sim: &mut Sim, p: Entity, by: Entity) {
    let at = sim.position(p);
    sim.app.world_mut().write_message(Damage {
        target: p,
        attacker: Some(by),
        amount: 1000.0,
        point: at,
        dir: Vec3::NEG_Z,
        hitgroup: Hitgroup::Chest,
        kind: DamageKind::Bullet,
        weapon: None,
        force: Vec3::ZERO,
    });
    sim.ticks(2);
}

#[test]
fn a_combat_arms_rung_gives_everyone_on_it_the_same_parts_character_and_melee() {
    let mut sim = party_with("", 3);
    let l = ladder_of(&sim);
    let x = l.extras(M4).expect("a Combat Arms rung draws").clone();
    let gun = l.rungs[M4];
    assert_eq!(gun, "combat_arms:m4a1");
    let model: i32 = x.value(rungs::MODEL).expect("a character").parse().unwrap();
    let knife = x.melee.clone().expect("a melee weapon");
    let parts = rung_parts(&sim, M4);
    assert!(!parts.is_empty(), "seed 3 fits some of the M4A1's parts: {x:?}");
    let (a, b) = two_on(&mut sim, M4);
    for p in [a, b] {
        assert_eq!(testkit::active_id(sim.app.world(), p), Some(gun));
        assert_eq!(look(&sim, p).map(|s| s.model), Some(model), "the rung's character");
        assert_eq!(melee(&sim, p), Some(knife.as_str()), "the rung's melee weapon: {:?}", carried(&sim, p));
        let f = fitted(sim.app.world(), p, gun).expect("fitted");
        for id in &parts {
            assert!(f.slots.contains(&Some(*id)), "part {id} fitted: {f:?}");
        }
    }
    let fa = fitted(sim.app.world(), a, gun).unwrap();
    assert_eq!(Some(fa), fitted(sim.app.world(), b, gun), "the same fitted set");
    assert_eq!(look(&sim, a), look(&sim, b), "the same look");
    // A respawn on the rung: the same again (a's kill takes it up one).
    kill(&mut sim, b, a);
    assert!(!alive(&sim, b));
    sim.seconds(4.0);
    assert!(alive(&sim, b));
    assert_eq!(sim.app.world().get::<ladder::Rung>(b).map(|r| r.0), Some(M4));
    assert_eq!(look(&sim, b).map(|s| s.model), Some(model));
    assert_eq!(melee(&sim, b), Some(knife.as_str()));
    assert_eq!(fitted(sim.app.world(), b, gun), Some(fa));
    // The killer's next rung (the M60) has its own.
    let next = l.extras(M4 + 1).unwrap();
    assert_eq!(melee(&sim, a), next.melee.as_deref());
}

#[test]
fn css_rungs_are_untouched() {
    let mut sim = party_with("", 3);
    let l = ladder_of(&sim);
    for (i, w) in l.rungs.iter().enumerate() {
        assert_eq!(l.extras(i).is_some(), w.starts_with("combat_arms:"), "rung {i} ({w})");
    }
    let (a, _b) = two_on(&mut sim, 0);
    assert!(carried(&sim, a).contains(&cs_source::game::weapons::KNIFE), "CS:S's knife: {:?}", carried(&sim, a));
    assert!(sim.app.world().get::<CaRungLook>(a).is_none(), "one's own look");
}

#[test]
fn another_seed_draws_other_extras() {
    let one = ladder_of(&party_with("", 3)).extras;
    assert_eq!(one, ladder_of(&party_with("", 3)).extras);
    assert_ne!(one, ladder_of(&party_with("", 4)).extras);
}

#[test]
fn each_option_off_is_todays_behaviour() {
    // All off: the M9, one's own look, the gun's own parts.
    let mut sim = party_with("rung_extras = false", 3);
    assert!(ladder_of(&sim).extras.is_empty());
    let (a, _) = two_on(&mut sim, M4);
    assert_eq!(melee(&sim, a), Some(CA_KNIFE));
    assert!(sim.app.world().get::<CaRungLook>(a).is_none());
    assert_eq!(look(&sim, a).map(|s| s.model), Some(0), "the default look");
    let stock = fitted(sim.app.world(), a, "combat_arms:m4a1").unwrap();
    // One at a time.
    for (key, check) in [("attachments", 0), ("character", 1), ("melee", 2)] {
        let mut sim = party_with(&format!("rung_extras = {{ {key} = false }}"), 3);
        let (a, _) = two_on(&mut sim, M4);
        let w = sim.app.world();
        let parts_stock = fitted(w, a, "combat_arms:m4a1") == Some(stock);
        let own_look = w.get::<CaRungLook>(a).is_none();
        let m9 = melee(&sim, a) == Some(CA_KNIFE);
        assert_eq!([parts_stock, own_look, m9][check], true, "{key} off");
        let others: Vec<bool> = [parts_stock, own_look, m9].iter().enumerate().filter(|(i, _)| *i != check).map(|(_, v)| *v).collect();
        assert!(others.iter().all(|v| !v), "{key} off leaves the others on: {others:?}");
    }
}

#[test]
fn a_network_client_wears_the_servers_look_and_parts() {
    let mut sim = NetSim::new(
        CaSim,
        LinkConditions {
            latency: std::time::Duration::from_millis(20),
            jitter: std::time::Duration::ZERO,
            loss: 0.0,
        },
        7,
        2,
        |app| {
            app.add_plugins((GreyboxMapPlugin, PartyPartsPlugin));
        },
    );
    sim.set_frame(TICK_INTERVAL);
    sim.server.app.insert_resource(RungSeed(3));
    minigame::start(sim.server.app.world_mut(), DEFAULT_MINIGAME).expect("the gun game starts");
    sim.until_joined(900);
    sim.ticks(300);
    let l = sim.server.app.world().resource::<Ladder>().clone();
    let (a, b) = (sim.character_of(0).unwrap(), sim.character_of(1).unwrap());
    for p in [a, b] {
        ladder::set_rung(sim.server.app.world_mut(), p, M4, &l);
    }
    sim.ticks(120);
    let gun = l.rungs[M4];
    let server_look = sim.server.app.world().get::<Selection>(a).copied();
    let server_fit = fitted(sim.server.app.world(), a, gun).map(|f| f.slots);
    assert!(server_look.is_some() && server_fit.is_some());
    assert_eq!(server_look, sim.server.app.world().get::<Selection>(b).copied());
    for i in 0..2 {
        let me = sim.local_player(i).unwrap();
        let w = sim.clients[i].app.world();
        assert_eq!(w.get::<Selection>(me).copied(), server_look, "client {i} wears the rung's look");
        assert_eq!(fitted(w, me, gun).map(|f| f.slots), server_fit, "client {i}'s gun has the rung's parts");
        assert_eq!(w.resource::<Ladder>().extras, l.extras, "client {i} has the server's rung extras");
    }
}
