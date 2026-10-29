//! The gun game's Combat Arms rung characters (`combat_arms::game::rungs`,
//! docs/DESIGN.md "Gun game rules"): a drawn body plays with the baseline
//! vest (no correction for a network client at a swap to another body);
//! the infected's rung (Chimera's `ladder::RungCharacter`, with a
//! test-written infected's hand: the install's are the Quarantine hands)
//! turns everyone on it into the infected holding the hands alone, at
//! the baseline's health and movement, its moment once on entering and
//! once on leaving, and the next rung makes them themselves again; a
//! network client sees the same.

use std::time::Duration;

use bevy::prelude::*;
use chimera_core::core::{Damage, DamageKind, Health, Hitgroup, Intent};
use chimera_net::{
    harness::NetSim,
    net::{memory::LinkConditions, predict::NetGraph},
};
use chimera_sim::{
    minigame::{
        self, MinigameAppExt, Minigames,
        ladder::{self, Ladder, RungSeed},
    },
    weapon::{Inventory, Weapon},
};
use chimera_world::{greybox::GreyboxMapPlugin, harness::Sim};
use combat_arms::{
    game::{
        TICK_INTERVAL,
        characters::records::Selection,
        data::{CaData, WEAPON_DATA},
        gear::CaGearData,
        movement::{CaCharacter, CaInfected},
        rungs::{self, CaRungInfected},
    },
    harness::{CaSim, CaSimWith},
};
use party_kit::{DEFAULT_MINIGAME, PartyPartsPlugin};

use super::gun_game::{alive, soldier};
use super::identity::assert_no_errors;

/// The test's infected's hand: the M9 knife's numbers, marked infected.
const HANDS: &str = "combat_arms:test_infected_hands";

/// The built-in data with `HANDS`.
fn data() -> CaData {
    let mut docs: Vec<(String, String)> = WEAPON_DATA.iter().map(|(n, t)| (n.to_string(), t.to_string())).collect();
    let m9 = WEAPON_DATA.iter().find(|(n, _)| *n == "m9.toml").expect("the M9's document").1;
    let hands = m9.replace("id = \"combat_arms:m9_knife\"", &format!("id = \"{HANDS}\"")) + &format!("\n[hooks.\"{}\"]\n", rungs::INFECTED);
    docs.push(("test_infected_hands.toml".into(), hands));
    CaData::builtin_with_weapons(docs)
}

/// The gun game on seed `seed`, `[rules]` lines `extra` added.
fn party<B>(base: impl bevy::app::Plugins<B>, extra: &str, seed: u64) -> Sim {
    let mut sim = Sim::new(base, (GreyboxMapPlugin, PartyPartsPlugin));
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

/// The infected's rung (exactly one).
fn infected_rung(l: &Ladder) -> usize {
    let rungs: Vec<usize> = (0..l.rungs.len()).filter(|i| l.character(*i).is_some()).collect();
    assert_eq!(rungs.len(), 1, "one infected's rung: {:?}", l.extras);
    rungs[0]
}

fn carried(world: &World, p: Entity) -> Vec<&'static str> {
    world.get::<Inventory>(p).unwrap().weapons.iter().filter_map(|x| world.get::<Weapon>(*x)).map(|x| x.id).collect()
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

/// The infection's moments as they came: (player, became or left).
#[derive(Resource, Default)]
struct Moments(Vec<(Entity, &'static str)>);

fn count_moments(app: &mut App) {
    app.init_resource::<Moments>()
        .add_observer(|on: On<Add<CaRungInfected>>, mut m: ResMut<Moments>| m.0.push((on.event_target(), "enter")))
        .add_observer(|on: On<Remove<CaRungInfected>>, mut m: ResMut<Moments>| m.0.push((on.event_target(), "leave")));
}

fn character(world: &World, p: Entity) -> String {
    format!("{:?}", world.get::<CaCharacter>(p).expect("a Combat Arms character"))
}

#[test]
fn the_infected_rung_makes_everyone_on_it_the_infected_with_the_hands_alone_until_the_next_rung() {
    let mut sim = party(CaSimWith(data()), "", 3);
    count_moments(&mut sim.app);
    let l = ladder_of(&sim);
    let z = infected_rung(&l);
    assert!(z >= l.rungs.len() * 2 / 3 && z + 1 < l.rungs.len(), "in the top third, not last: {z}");
    assert!(l.rungs[z].starts_with("combat_arms:"), "a Combat Arms rung: {}", l.rungs[z]);
    let x = l.extras(z).unwrap();
    assert_eq!(x.character.as_ref().map(|c| c.weapon.as_str()), Some(HANDS));
    let body: i32 = x.value(rungs::MODEL).expect("an infected body").parse().unwrap();
    // A player on the other Combat Arms gun rung for the baseline.
    let other = (0..l.rungs.len()).rev().find(|i| *i != z && l.rungs[*i].starts_with("combat_arms:") && l.character(*i).is_none()).unwrap();
    let (a, b, c) = (soldier(&mut sim), soldier(&mut sim), soldier(&mut sim));
    sim.seconds(1.5);
    ladder::set_rung(sim.app.world_mut(), c, other, &l);
    for p in [a, b] {
        ladder::set_rung(sim.app.world_mut(), p, z, &l);
    }
    sim.seconds(1.0);
    let w = sim.app.world();
    for p in [a, b] {
        assert_eq!(carried(w, p), [HANDS], "the hands alone: no gun, no kit");
        assert!(w.get::<CaRungInfected>(p).is_some(), "the infected");
        assert!(w.get::<CaInfected>(p).is_none(), "no infected rules (knock-back)");
        assert_eq!(w.get::<Selection>(p).map(|s| s.model), Some(body), "the infected's body");
        let h = w.get::<Health>(p).unwrap();
        assert_eq!((h.current, h.max), (1.0, 1.0), "the baseline's health");
        assert_eq!(character(w, p), character(w, c), "the baseline's movement");
    }
    assert_eq!(w.resource::<Moments>().0, [(a, "enter"), (b, "enter")], "once each");
    // b dies on the rung and comes back the infected: no new moment.
    kill(&mut sim, b, a);
    sim.seconds(4.0);
    assert!(alive(&sim, b));
    let w = sim.app.world();
    assert_eq!(carried(w, b), [HANDS]);
    assert!(w.get::<CaRungInfected>(b).is_some());
    // a's kill took it a rung up: itself again.
    assert_eq!(w.get::<ladder::Rung>(a).map(|r| r.0), Some(z + 1));
    let have = carried(w, a);
    assert!(!have.contains(&HANDS) && have.contains(&l.rungs[z + 1]), "the next rung's gun, the hands gone: {have:?}");
    assert!(have.len() >= 2, "the kit's knife back: {have:?}");
    assert!(w.get::<CaRungInfected>(a).is_none());
    assert_eq!(w.resource::<Moments>().0, [(a, "enter"), (b, "enter"), (a, "leave")]);
}

#[test]
fn the_infected_option_off_or_no_infected_hands_draws_no_infected_rung() {
    let off = ladder_of(&party(CaSimWith(data()), "rung_extras = { infected = false }", 3));
    assert!((0..off.rungs.len()).all(|i| off.character(i).is_none()));
    let plain = ladder_of(&party(CaSim, "", 3));
    assert!((0..plain.rungs.len()).all(|i| plain.character(i).is_none()), "built-in data: no hands");
    // No gun rung's melee weapon is ever a hand.
    for seed in 0..20 {
        let l = ladder_of(&party(CaSimWith(data()), "", seed));
        assert!(l.extras.iter().flatten().all(|x| x.melee.as_deref() != Some(HANDS)), "seed {seed}");
        infected_rung(&l);
    }
}

fn link() -> LinkConditions {
    LinkConditions {
        latency: Duration::from_millis(20),
        jitter: Duration::ZERO,
        loss: 0.0,
    }
}

fn net<B>(base: impl bevy::app::Plugins<B> + Clone + Send + Sync + 'static) -> NetSim {
    let mut sim = NetSim::new(base, link(), 7, 2, |app| {
        app.add_plugins((GreyboxMapPlugin, PartyPartsPlugin));
    });
    sim.set_frame(TICK_INTERVAL);
    sim.server.app.insert_resource(RungSeed(3));
    minigame::start(sim.server.app.world_mut(), DEFAULT_MINIGAME).expect("the gun game starts");
    sim.until_joined(900);
    sim.ticks(300);
    sim
}

fn reset_graphs(sim: &mut NetSim) {
    for c in &mut sim.clients {
        *c.app.world_mut().resource_mut::<NetGraph>() = NetGraph::default();
    }
}

fn all_on(sim: &mut NetSim, rung: usize) {
    let l = sim.server.app.world().resource::<Ladder>().clone();
    for i in 0..2 {
        let p = sim.character_of(i).unwrap();
        ladder::set_rung(sim.server.app.world_mut(), p, rung, &l);
    }
}

/// Both clients walk to and fro for `n` ticks, `at` (tick, what) done on
/// the way.
fn walk(sim: &mut NetSim, n: u32, mut at: impl FnMut(&mut NetSim, u32)) {
    for k in 0..n {
        for i in 0..2 {
            let me = sim.local_player(i).unwrap();
            let mut intent = sim.clients[i].app.world_mut().get_mut::<Intent>(me).unwrap();
            intent.move_axis = if (k / 20) % 2 == 0 { Vec2::Y } else { -Vec2::Y };
        }
        at(sim, k);
        sim.step();
    }
}

#[test]
fn a_network_client_predicts_a_swap_to_another_rung_body_without_a_correction() {
    let mut sim = net(CaSim);
    let l = sim.server.app.world().resource::<Ladder>().clone();
    let gear = sim.server.app.world().resource::<CaGearData>().0.clone();
    let ca: Vec<usize> = (0..l.rungs.len()).filter(|i| l.rungs[*i].starts_with("combat_arms:")).collect();
    // A rung body's own vest penalty (what it moved with before).
    let penalty = |i: usize| {
        let body: i32 = l.extras(i).and_then(|x| x.value(rungs::MODEL)?.parse().ok()).unwrap();
        gear.vests.get(&body).map(|v| v.move_penalty)
    };
    let (r1, r2) = ca
        .iter()
        .flat_map(|a| ca.iter().map(move |b| (*a, *b)))
        .find(|(a, b)| penalty(*a) != penalty(*b))
        .expect("two Combat Arms rungs whose bodies' vests move differently");
    all_on(&mut sim, r1);
    sim.ticks(120);
    reset_graphs(&mut sim);
    walk(&mut sim, 150, |sim, k| {
        if k == 40 {
            all_on(sim, r2);
        }
    });
    assert_no_errors(&sim, "a swap to another rung body, walking");
}

#[test]
fn a_network_client_sees_the_infected_and_their_return() {
    let mut sim = net(CaSimWith(data()));
    let l = sim.server.app.world().resource::<Ladder>().clone();
    let z = infected_rung(&l);
    all_on(&mut sim, z);
    sim.ticks(120);
    let first = sim.character_of(0).unwrap();
    let server_look = sim.server.app.world().get::<Selection>(first).copied();
    for i in 0..2 {
        let me = sim.local_player(i).unwrap();
        let w = sim.clients[i].app.world();
        assert!(w.get::<CaRungInfected>(me).is_some(), "client {i} sees its player infected");
        assert_eq!(carried(w, me), [HANDS], "client {i}: the hands alone");
        assert_eq!(w.get::<Health>(me).map(|h| h.max), Some(1.0), "client {i}: the baseline's health");
        assert_eq!(w.get::<Selection>(me).copied(), server_look);
    }
    reset_graphs(&mut sim);
    all_on(&mut sim, z + 1);
    sim.ticks(120);
    for i in 0..2 {
        let me = sim.local_player(i).unwrap();
        let w = sim.clients[i].app.world();
        assert!(w.get::<CaRungInfected>(me).is_none(), "client {i}: itself again");
        assert!(!carried(w, me).contains(&HANDS));
    }
    assert_no_errors(&sim, "leaving the infected's rung");
}
