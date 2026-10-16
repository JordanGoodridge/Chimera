//! `net_graph`'s numbers on a network client (`net::stats`,
//! `net::predict::NetGraph`) over `NetSim`'s link with latency and loss:
//! ping, packets and bytes in and out, command and update rates, loss,
//! the server's frame rate and tick rate, and choke when a client's
//! `rate` is below what its updates take.

use std::time::Duration;

use bevy::prelude::*;

use crate::stack::{
    games::cs_source::movement::{self as source, SourceMovementPlugin},
    greybox::GreyboxMapPlugin,
    harness::{CsSim, NetSim},
    net::{
        memory::LinkConditions,
        predict::{NetGraph, RateSettings},
    },
    slots::Loadout,
};

fn greybox(app: &mut App) {
    app.add_plugins((GreyboxMapPlugin, SourceMovementPlugin))
        .insert_resource(Loadout { movement: source::ID });
}

fn graph(sim: &NetSim, i: usize) -> NetGraph {
    sim.clients[i].app.world().resource::<NetGraph>().clone()
}

/// What the link carried to client `i` so far (lost packets included).
fn sent_to(sim: &NetSim, i: usize) -> u64 {
    let id = sim.client_id(i);
    sim.link.lock().unwrap().bytes_to.get(&id).copied().unwrap_or(0)
}

/// At 50 ms each way with 3 % loss: ping about the round trip, a
/// second's bytes in about what the link carried less the lost, a
/// command packet each tick out, the server's 64 Hz tick and frames and
/// 20 updates a second (`cl_updaterate` 20), some loss, no choke.
#[test]
fn a_clients_net_graph_has_its_connections_numbers() {
    let link = LinkConditions {
        latency: Duration::from_millis(50),
        jitter: Duration::ZERO,
        loss: 0.03,
    };
    let mut sim = NetSim::new(CsSim, link, 7, 1, greybox);
    sim.until_joined(600);
    sim.ticks(320);
    let before = sent_to(&sim, 0);
    // A second (64 frames of 1/64 s), then the readout.
    sim.ticks(64);
    let carried = (sent_to(&sim, 0) - before) as f64;
    let g = graph(&sim, 0);
    println!(
        "ping {:.1} ms, loss {:.1} %, in {:?} (link {carried} B/s), out {:?}, cmd {:.1}/s, server {:?}",
        g.ping_ms,
        g.loss * 100.0,
        g.incoming,
        g.outgoing,
        g.cmd_rate,
        g.server
    );
    assert!((80.0..=160.0).contains(&g.ping_ms), "ping {} ms", g.ping_ms);
    assert!(g.incoming.mean > 10.0 && g.outgoing.mean > 10.0, "packet sizes");
    let ratio = g.incoming.bytes_per_sec / carried;
    assert!((0.85..=1.05).contains(&ratio), "in {} B/s of the link's {carried}", g.incoming.bytes_per_sec);
    assert!(
        (20.0..=200.0).contains(&g.incoming.packets_per_sec),
        "in {} packets/s",
        g.incoming.packets_per_sec
    );
    // At least a packet a frame (renet's: one per channel with something
    // to send, acknowledgements), one command message a tick.
    assert!((60.0..=200.0).contains(&g.outgoing.packets_per_sec), "out {:?}", g.outgoing);
    assert!((60.0..=68.0).contains(&g.cmd_rate), "cmd {}/s", g.cmd_rate);
    assert!(g.outgoing.bytes_per_sec > 1000.0, "out {:?}", g.outgoing);
    let s = g.server.expect("the server's numbers arrive once a second");
    assert!((s.tick_hz - 64.0).abs() < 0.01, "{s:?}");
    assert!((s.fps - 64.0).abs() < 2.0, "{s:?}");
    assert!((s.updates - 20.0).abs() < 2.5, "{s:?}");
    assert_eq!(s.choke, 0.0, "{s:?}");
    assert!(g.loss < 0.2, "loss {}", g.loss);
}

/// A client whose `rate` is far below its updates' bytes is choked: the
/// server tells it the share held back, and it gets fewer updates.
#[test]
fn a_starved_client_sees_choke() {
    let mut sim = NetSim::new(CsSim, LinkConditions::default(), 9, 0, greybox);
    sim.server
        .app
        .world_mut()
        .resource_mut::<crate::stack::console::Console>()
        .submit("sv_minrate 0");
    sim.add_client(|w| {
        *w.resource_mut::<RateSettings>() = RateSettings {
            updaterate: 66.0,
            rate: 1000.0,
        };
    });
    sim.until_joined(600);
    sim.ticks(320);
    let s = graph(&sim, 0).server.expect("server numbers");
    println!("{s:?}");
    assert!(s.choke > 0.2, "choke {}", s.choke);
    assert!(s.updates < 50.0, "updates {}", s.updates);
}
