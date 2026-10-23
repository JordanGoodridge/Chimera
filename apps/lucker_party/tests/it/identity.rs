//! Each player is the game of the gun in hand (the gun game's `identity =
//! "weapon"`, `chimera_sim::identity`): a CS:S gun moves as CS:S, a
//! Combat Arms gun or the knife as Combat Arms. Headless on the party's
//! simulation (scripted soldiers, the rungs' swaps and the knife), and over
//! the network (`NetSim`, two clients): the switch is predicted on the
//! owning client's own tick with no prediction error.

use std::time::Duration;

use bevy::prelude::*;
use chimera_core::{
    core::{Intent, PredictedComponents, SimTick},
    slots::{FollowsWeapon, GameIdentity, MovementSlot},
};
use chimera_net::{
    harness::NetSim,
    net::{
        memory::LinkConditions,
        predict::{NetGraph, PredictionHistory},
    },
};
use chimera_sim::{
    minigame::{
        self,
        ladder::{self, Ladder, Rung},
    },
    testkit,
};
use chimera_world::greybox::GreyboxMapPlugin;
use combat_arms::{
    game::{TICK_INTERVAL, inventory::KNIFE, movement as ca_movement},
    harness::CaSim,
};
use cs_source::game::{movement as cs_movement, weapons as cs_weapons};
use lucker_party::{DEFAULT_MINIGAME, PartyPartsPlugin};

use crate::gun_game::{held, line_up, party, rung, shoot_until_dead, soldier};

fn movement(world: &World, e: Entity) -> Option<&'static str> {
    world.get::<MovementSlot>(e).map(|m| m.0)
}

fn identity(world: &World, e: Entity) -> Option<String> {
    world.get::<GameIdentity>(e).map(|i| i.0.clone())
}

/// `e`'s movement and identity are `game`'s.
fn assert_is(world: &World, e: Entity, game: &str, what: &str) {
    let want = if game == "cs_source" { cs_movement::ID } else { ca_movement::ID };
    assert_eq!(movement(world, e), Some(want), "{what}: {game}'s movement");
    assert_eq!(identity(world, e).as_deref(), Some(game), "{what}: {game}'s identity");
}

/// A rung-up's swap (CS:S's Glock to Combat Arms' Beretta) makes the
/// killer Combat Arms'; the next (the Desert Eagle) CS:S's again; the
/// knife is Combat Arms', and the gun back CS:S's.
#[test]
fn movement_follows_the_gun_on_a_rung_up_and_back() {
    let mut sim = party();
    let a = soldier(&mut sim);
    let b = soldier(&mut sim);
    sim.seconds(1.5);
    assert!(sim.app.world().get::<FollowsWeapon>(a).is_some(), "the gun game's players follow their gun");
    assert_eq!(held(&sim, a), Some(cs_weapons::GLOCK));
    assert_is(sim.app.world(), a, "cs_source", "the Glock");
    // A kill: the Beretta, Combat Arms'.
    line_up(&mut sim, a, b, 4.0);
    assert!(shoot_until_dead(&mut sim, a, b, 8.0), "the Glock kills");
    sim.ticks(2);
    assert_eq!(rung(&sim, a), 1);
    assert_eq!(held(&sim, a), Some("combat_arms:beretta_m92fs"));
    assert_is(sim.app.world(), a, "combat_arms", "the Beretta");
    // The victim comes back CS:S's, with its Glock.
    line_up(&mut sim, a, b, 4.0);
    assert_is(sim.app.world(), b, "cs_source", "b respawned with the Glock");
    // Another kill: the Desert Eagle, CS:S's again.
    assert!(shoot_until_dead(&mut sim, a, b, 8.0), "the Beretta kills");
    sim.ticks(2);
    assert_eq!(held(&sim, a), Some(cs_weapons::DEAGLE));
    assert_is(sim.app.world(), a, "cs_source", "the Desert Eagle");
    // The knife: Combat Arms'; Q back to the Desert Eagle: CS:S's.
    sim.intent(a).select = Some(2);
    sim.ticks(1);
    sim.intent(a).select = None;
    sim.ticks(2);
    assert_eq!(held(&sim, a), Some(KNIFE));
    assert_is(sim.app.world(), a, "combat_arms", "the knife");
    sim.intent(a).last_weapon = true;
    sim.ticks(1);
    sim.intent(a).last_weapon = false;
    sim.ticks(2);
    assert_eq!(held(&sim, a), Some(cs_weapons::DEAGLE));
    assert_is(sim.app.world(), a, "cs_source", "back to the Desert Eagle");
    // It still walks: CS:S's movement moves it on the greybox's floor.
    let from = sim.position(a);
    sim.intent(a).move_axis = Vec2::Y;
    sim.seconds(0.5);
    sim.intent(a).move_axis = Vec2::ZERO;
    assert!(sim.position(a).distance(from) > 1.0, "walked as CS:S: {from} -> {}", sim.position(a));
}

/// A minigame without the option leaves movement to its `movement` slot.
#[test]
fn without_the_option_the_slots_movement_stays() {
    let mut sim = party();
    let text = originals::MINIGAMES.iter().find(|(f, _)| f.contains("gun_game_dust2")).unwrap().1;
    let text = text.replace("identity = \"weapon\"\n", "").replace("originals:gun_game_dust2", "test:gun_game_same");
    chimera_sim::minigame::MinigameAppExt::add_minigames(&mut sim.app, &[("same.toml", &text)]).unwrap();
    minigame::start(sim.app.world_mut(), "test:gun_game_same").unwrap();
    let a = soldier(&mut sim);
    let _b = soldier(&mut sim);
    sim.seconds(1.5);
    assert_eq!(held(&sim, a), Some(cs_weapons::GLOCK));
    assert_eq!(movement(sim.app.world(), a), Some(ca_movement::ID), "the slot's");
    assert!(sim.app.world().get::<FollowsWeapon>(a).is_none() && identity(sim.app.world(), a).is_none());
}

// Over the network.

fn link(latency_ms: u64) -> LinkConditions {
    LinkConditions {
        latency: Duration::from_millis(latency_ms),
        jitter: Duration::ZERO,
        loss: 0.0,
    }
}

/// The party's server (the gun game started) and two clients at
/// `latency_ms`, joined and in the game.
pub(crate) fn party_net(latency_ms: u64, seed: u64) -> NetSim {
    let mut sim = NetSim::new(CaSim, link(latency_ms), seed, 2, |app| {
        app.add_plugins((GreyboxMapPlugin, PartyPartsPlugin));
    });
    minigame::start(sim.server.app.world_mut(), DEFAULT_MINIGAME).expect("the gun game starts");
    sim.set_frame(TICK_INTERVAL);
    sim.until_joined(600);
    sim.ticks(300);
    sim
}

fn graph(sim: &NetSim, i: usize) -> NetGraph {
    sim.clients[i].app.world().resource::<NetGraph>().clone()
}

fn reset_graphs(sim: &mut NetSim) {
    for c in &mut sim.clients {
        *c.app.world_mut().resource_mut::<NetGraph>() = NetGraph::default();
    }
}

fn client_intent(sim: &mut NetSim, i: usize) -> Mut<'_, Intent> {
    let me = sim.local_player(i).unwrap();
    sim.clients[i].app.world_mut().get_mut::<Intent>(me).unwrap()
}

fn client_held(sim: &mut NetSim, i: usize) -> Option<&'static str> {
    let me = sim.local_player(i).unwrap();
    testkit::active_id(sim.clients[i].app.world(), me)
}

/// The server's character of client `i` and its own copy both move as
/// `game` does.
fn assert_moves_as(sim: &mut NetSim, i: usize, game: &str, what: &str) {
    let want = if game == "cs_source" { cs_movement::ID } else { ca_movement::ID };
    let theirs = sim.character_of(i).unwrap();
    let me = sim.local_player(i).unwrap();
    assert_eq!(movement(sim.server.app.world(), theirs), Some(want), "{what}: the server's");
    assert_eq!(movement(sim.clients[i].app.world(), me), Some(want), "{what}: client {i}'s prediction");
}

pub(crate) fn assert_no_errors(sim: &NetSim, what: &str) {
    for i in 0..sim.clients.len() {
        let g = graph(sim, i);
        println!(
            "{what}, client {i}: {} states checked, {} errors (last differing: {:?}), {} resyncs",
            g.checked, g.errors, g.last_differing, g.resyncs
        );
        assert!(g.checked > 0, "{what}: client {i} compared no state");
        assert_eq!(g.errors, 0, "{what}: client {i} mispredicted ({:?})", g.last_differing);
    }
}

/// 30 more ticks: every tick client `i` predicted that the server ran is
/// the server's state, bit for bit.
fn assert_agrees(sim: &mut NetSim, i: usize, what: &str) {
    let theirs = sim.character_of(i).unwrap();
    let mut server = std::collections::BTreeMap::new();
    for _ in 0..30 {
        sim.step();
        let world = sim.server.app.world();
        let tick = world.resource::<SimTick>().0;
        server.insert(tick, world.resource::<PredictedComponents>().encode(world, theirs));
    }
    let history = &sim.clients[i].app.world().resource::<PredictionHistory>().0;
    let compared = history
        .iter()
        .filter(|p| !p.state.is_empty())
        .filter_map(|p| server.get(&p.tick).map(|s| (p, s)))
        .inspect(|(p, s)| assert!(**s == p.state, "{what}: the prediction for tick {} isn't the server's", p.tick))
        .count();
    assert!(compared > 0, "{what}: no predicted tick to compare");
}

/// Client 0's own knife (Combat Arms') and back (Q, CS:S's Glock), then
/// the server's rung-ups (the Beretta, Combat Arms'; the Desert Eagle,
/// CS:S's): its movement switches on its own predicted tick, as the
/// server's does, with no prediction error while it walks.
#[test]
fn a_switch_is_predicted_without_error() {
    let mut sim = party_net(80, 7);
    assert_eq!(client_held(&mut sim, 0), Some(cs_weapons::GLOCK), "the first rung's Glock");
    assert_moves_as(&mut sim, 0, "cs_source", "the Glock");
    let me = sim.local_player(0).unwrap();
    assert!(sim.clients[0].app.world().get::<FollowsWeapon>(me).is_some(), "the client knows its player follows");
    reset_graphs(&mut sim);
    let walk = |sim: &mut NetSim, n: u32| {
        for k in 0..n {
            let mut i = client_intent(sim, 0);
            i.move_axis = if (k / 20) % 2 == 0 { Vec2::Y } else { -Vec2::Y };
            i.yaw = 0.3 + k as f32 * 0.01;
            sim.step();
        }
        client_intent(sim, 0).move_axis = Vec2::ZERO;
    };
    walk(&mut sim, 30);
    // The knife: class key 3, walking on.
    client_intent(&mut sim, 0).select = Some(2);
    sim.step();
    client_intent(&mut sim, 0).select = None;
    walk(&mut sim, 60);
    assert_eq!(client_held(&mut sim, 0), Some(KNIFE));
    assert_moves_as(&mut sim, 0, "combat_arms", "the knife");
    // Q: back to the Glock.
    client_intent(&mut sim, 0).last_weapon = true;
    sim.step();
    client_intent(&mut sim, 0).last_weapon = false;
    walk(&mut sim, 60);
    assert_eq!(client_held(&mut sim, 0), Some(cs_weapons::GLOCK));
    assert_moves_as(&mut sim, 0, "cs_source", "the Glock again");
    // The server's rung-ups, while it walks.
    for (rung, gun, game) in [(1, "combat_arms:beretta_m92fs", "combat_arms"), (2, cs_weapons::DEAGLE, "cs_source")] {
        let theirs = sim.character_of(0).unwrap();
        let w = sim.server.app.world_mut();
        let l = w.resource::<Ladder>().clone();
        ladder::set_rung(w, theirs, rung, &l);
        walk(&mut sim, 80);
        assert_eq!(sim.server.app.world().get::<Rung>(theirs).map(|r| r.0), Some(rung));
        assert_eq!(client_held(&mut sim, 0), Some(gun), "rung {rung}");
        assert_moves_as(&mut sim, 0, game, gun);
    }
    sim.ticks(60);
    assert_no_errors(&sim, "switches");
    assert_agrees(&mut sim, 0, "switches");
    assert!(graph(&sim, 0).resyncs >= 2, "the server's swaps were restarts: {:?}", graph(&sim, 0));
}
