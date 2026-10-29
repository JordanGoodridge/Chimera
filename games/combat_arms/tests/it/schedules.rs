//! Combat Arms' simulation schedules as its dedicated server and its
//! client compose them (docs/plans/active/architecture-review.md, E1
//! steps 2 and 3) as one ratchet (`chimera_world::harness::
//! check_schedule_ratchet`): per schedule, the ambiguous pairs
//! (conflicting systems with no order between them) of each, and how
//! many of those the server and the client run in opposite orders (where
//! a client computes otherwise than its server: a misprediction); each
//! count may only fall. Prediction's schedules have no such pair; the
//! fixed tick's still have some (the stored numbers), so a strict
//! "same order" test would first need them all ordered.
//!
//! Each composition adds the plugins in its binary's order
//! (apps/combat_arms_server/src/lib.rs's `ServerPlugin`,
//! apps/combat_arms/src/main.rs on the greybox), since the order systems
//! are added in decides how Bevy sorts the unordered ones; headless and
//! without what lies above the game. The client's map is the engine's
//! `MapSimPlugin` where the game adds the presentation's `MapPlugin`
//! (this crate's tests don't link the presentation), and the client shell
//! and Combat Arms' front end (`ui`) aren't in this binary, so their
//! systems in the fixed schedules are not compared.

use bevy::prelude::*;
use chimera_net::net::NetPlugin;
use chimera_world::{
    greybox::GreyboxMapPlugin,
    harness::{ScheduleRatchet, add_headless, check_schedule_ratchet},
    map::MapSimPlugin,
};
use combat_arms::game::CombatArmsPlugins;

/// Per schedule: ambiguous pairs (conflicting access, no order) on the
/// server and on the client, and how many of them the two run in
/// opposite orders. Each may fall (lower it here), never rise
/// (`check_schedule_ratchet` says which pairs to order). The prediction
/// schedules' opposite pairs stay at 0.
const RATCHET: &[ScheduleRatchet] = &[
    ScheduleRatchet {
        schedule: "FixedUpdate",
        server: 718,
        client: 718,
        opposite: 253,
    },
    ScheduleRatchet {
        schedule: "FixedPostUpdate",
        server: 13,
        client: 13,
        opposite: 4,
    },
    ScheduleRatchet {
        schedule: "FixedLast",
        server: 97,
        client: 97,
        opposite: 32,
    },
    ScheduleRatchet {
        schedule: "Predict::Select",
        server: 5,
        client: 5,
        opposite: 0,
    },
    ScheduleRatchet {
        schedule: "Predict::Movement",
        server: 4,
        client: 4,
        opposite: 0,
    },
    ScheduleRatchet {
        schedule: "Predict::Weapons",
        server: 12,
        client: 12,
        opposite: 0,
    },
];

/// The dedicated server's composition on the greybox (`ServerPlugin`).
fn server() -> App {
    let mut app = App::new();
    add_headless(&mut app);
    app.add_plugins(combat_arms::SimPlugins)
        .add_plugins((GreyboxMapPlugin, MapSimPlugin::empty()))
        .add_plugins((NetPlugin, CombatArmsPlugins));
    app.finish();
    app.cleanup();
    app
}

/// The game's (a client or listen server) on the greybox: the map first,
/// then the engine, the simulation (`combat_arms_app::SimPlugin`) and the
/// network.
fn client() -> App {
    let mut app = App::new();
    app.add_plugins((GreyboxMapPlugin, MapSimPlugin::empty()));
    add_headless(&mut app);
    app.add_plugins((combat_arms::SimPlugins, CombatArmsPlugins))
        .add_plugins(NetPlugin);
    app.finish();
    app.cleanup();
    app
}

#[test]
fn schedule_ratchet() {
    if let Err(report) = check_schedule_ratchet(&mut server(), &mut client(), RATCHET) {
        panic!("{report}");
    }
}
