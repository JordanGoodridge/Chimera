//! Weapons as data (docs/weapon-data.md): a gun defined only in a data
//! file (apps/cs_source/tests/data/weapon_rail.toml, on CS:S's gun
//! template and hooks, with its own ammo type) is bought, fired and
//! replicated with no Rust naming it.

use std::path::Path;

use bevy::prelude::*;
use cs_source_app::{
    console::Console,
    core::{Health, LocalPlayer, Team},
    games::cs_source::{
        TICK_INTERVAL,
        hit_reaction::HitSlowdown,
        movement::{self as source, SourceMovementPlugin},
        weapons::CsWeaponsPlugin,
    },
    greybox::{self, GreyboxMapPlugin},
    harness::{NetSim, Sim},
    movement::placeholder,
    net::memory::LinkConditions,
    slots::Loadout,
    weapon::{
        Inventory, Magazine, SwitchWeight, Weapon, WeaponEvent, WeaponEventKind,
        data::{WeaponDataAppExt, WeaponSet},
        economy::{Money, Prices},
    },
};

const RAIL: &str = "mashup_test:weapon_rail";

/// Load the test's data directory into `app`, as an app loads a mod's.
fn add_rail(app: &mut App) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let docs = WeaponSet::read_dir(&dir).unwrap();
    let docs: Vec<(&str, &str)> = docs.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
    app.add_weapon_data(&docs).unwrap();
}

fn active(w: &World, p: Entity) -> Option<Entity> {
    w.get::<Inventory>(p)?.active
}

fn active_id(w: &World, p: Entity) -> Option<&'static str> {
    Some(w.get::<Weapon>(active(w, p)?)?.id)
}

#[test]
fn a_gun_from_data_is_bought_and_fires() {
    let mut sim = Sim::new((GreyboxMapPlugin, CsWeaponsPlugin));
    sim.set_tick_interval(TICK_INTERVAL);
    add_rail(&mut sim.app);
    let p = sim.spawn_character(greybox::SPAWNS[0], placeholder::ID);
    sim.app
        .world_mut()
        .entity_mut(p)
        .insert((LocalPlayer, Team(1), Money(5000)));
    sim.ticks(1);
    // Bought by its short name, at its price.
    sim.app.world_mut().resource_mut::<Console>().submit("buy rail");
    sim.ticks(2);
    let w = sim.app.world();
    assert_eq!(active_id(w, p), Some(RAIL));
    assert_eq!(w.get::<Money>(p).unwrap().0, 5000 - 1234);
    assert_eq!(w.resource::<Prices>().weapons.get(RAIL), Some(&1234));
    let rail = active(w, p).unwrap();
    let m = w.get::<Magazine>(rail).unwrap();
    // Bought full (CS:S's rule): the clip and its ammo type's 20.
    assert_eq!((m.clip, m.size, m.reserve, m.reserve_max), (5, 5, 20, 20));
    assert_eq!(w.get::<Weapon>(rail).unwrap().slot, 0);
    assert_eq!(w.get::<SwitchWeight>(rail), Some(&SwitchWeight(30)));
    // The CS:S hook it names is on it.
    assert_eq!(w.get::<HitSlowdown>(rail).unwrap().body, 0.4);
    // Drawn in its 0.5 s, it fires at a target 6 m ahead: 40 hit points,
    // no falloff.
    sim.seconds(0.55);
    let target = sim.spawn_character(greybox::SPAWNS[0] - Vec3::Z * 6.0, placeholder::ID);
    sim.app.world_mut().get_mut::<Health>(target).unwrap().current = 10.0;
    sim.ticks(2);
    let eye = sim.position(p) + sim.state(p).eye_offset;
    let d = sim.position(target) + Vec3::Y * (1.8 * 0.72 - 0.9) - eye;
    {
        let mut i = sim.intent(p);
        i.yaw = (-d.x).atan2(-d.z);
        i.pitch = d.y.atan2(d.xz().length());
    }
    let mut cursor = sim.app.world().resource::<Messages<WeaponEvent>>().get_cursor_current();
    sim.intent(p).fire = true;
    sim.ticks(1);
    sim.intent(p).fire = false;
    sim.ticks(1);
    let w = sim.app.world();
    let shots = cursor
        .read(w.resource::<Messages<WeaponEvent>>())
        .filter(|e| e.weapon == rail && matches!(e.kind, WeaponEventKind::Shot { .. }))
        .count();
    assert_eq!(shots, 1);
    assert_eq!(w.get::<Magazine>(rail).unwrap().clip, 4);
    let lost = ((10.0 - w.get::<Health>(target).unwrap().current) * 100.0).round() as i32;
    assert_eq!(lost, 40);
}

fn setup(app: &mut App) {
    app.add_plugins((GreyboxMapPlugin, SourceMovementPlugin, CsWeaponsPlugin))
        .insert_resource(Loadout { movement: source::ID });
    add_rail(app);
}

/// Over the network: the client asks, the server sells it, the client
/// predicts it (built from the same data on both sides) and its shots hit
/// on the server with the client's clip agreeing.
#[test]
fn a_gun_from_data_is_replicated() {
    let conditions = LinkConditions {
        latency: std::time::Duration::from_millis(50),
        jitter: std::time::Duration::ZERO,
        loss: 0.0,
    };
    let mut sim = NetSim::new(conditions, 61, 2, setup);
    sim.until_joined(600);
    sim.ticks(320);
    sim.clients[0]
        .app
        .world_mut()
        .resource_mut::<Console>()
        .submit("buy rail");
    sim.ticks(60);
    let me = sim.local_player(0).unwrap();
    let server_me = sim.character_of(0).unwrap();
    let other = sim.character_of(1).unwrap();
    assert_eq!(
        active_id(sim.server.app.world(), server_me),
        Some(RAIL),
        "the server sold it"
    );
    assert_eq!(
        active_id(sim.clients[0].app.world(), me),
        Some(RAIL),
        "the client predicts it"
    );
    // Face the other player down the greybox and shoot.
    for (i, z) in [(0, 12.0), (1, 4.0)] {
        let e = sim.character_of(i).unwrap();
        let world = sim.server.app.world_mut();
        world.get_mut::<Transform>(e).unwrap().translation = Vec3::new(0.0, 1.0, z);
        if let Some(mut p) = world.get_mut::<avian3d::prelude::Position>(e) {
            p.0 = Vec3::new(0.0, 1.0, z);
        }
    }
    sim.server.app.world_mut().get_mut::<Health>(other).unwrap().current = 10.0;
    let me = sim.local_player(0).unwrap();
    // Level, a little down: at the chest 8 m away.
    *sim.clients[0]
        .app
        .world_mut()
        .get_mut::<cs_source_app::core::Intent>(me)
        .unwrap() = cs_source_app::core::Intent {
        pitch: -0.06,
        ..default()
    };
    sim.ticks(60);
    let health = sim.server.app.world().get::<Health>(other).unwrap().current;
    let me = sim.local_player(0).unwrap();
    sim.clients[0]
        .app
        .world_mut()
        .get_mut::<cs_source_app::core::Intent>(me)
        .unwrap()
        .fire = true;
    sim.ticks(2);
    sim.clients[0]
        .app
        .world_mut()
        .get_mut::<cs_source_app::core::Intent>(me)
        .unwrap()
        .fire = false;
    sim.ticks(40);
    let after = sim.server.app.world().get::<Health>(other).unwrap().current;
    assert!(
        ((health - after) * 100.0).round() as i32 == 40,
        "the server dealt the data's 40: {health} -> {after}"
    );
    let clip = |w: &World, p: Entity| w.get::<Magazine>(active(w, p).unwrap()).unwrap().clip;
    assert_eq!(clip(sim.server.app.world(), server_me), 4);
    assert_eq!(clip(sim.clients[0].app.world(), me), 4, "the client's predicted clip");
}
