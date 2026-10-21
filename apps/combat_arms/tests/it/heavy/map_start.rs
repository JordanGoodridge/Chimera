//! A map the app starts on (`--map combat_arms:<name>`: the map plugin
//! with the loaded map, no `map` command, so no `core::NewGame`) plays its
//! mission as a map loaded by the `map` command does: the install's
//! mission data is found when the match starts and its start triggers
//! reach the map's objects. SHORTFUSE in Elimination (TDM):
//! `Level01StartTrigger` fires and, through `Qrt_False`, removes the
//! dumpster lids (`RotatingDoor00`..`07`, Quarantine's). Passes with a note
//! without the install or without the V1 key (the mission archive).

use bevy::prelude::*;
use chimera_core::core::Team;
use chimera_world::{harness::Sim, map::MapSimPlugin};
use combat_arms::game::{
    missions::{self, CaMission},
    movement,
    objects::{CaObjects, runtime::Kind},
    rules::CaRuleSettings,
};

#[test]
fn a_map_started_on_plays_its_mission() {
    let data = match combat_arms_app::load_map("combat_arms:shortfuse", 0) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("skipping: {e}");
            return;
        }
    };
    if missions::install().is_none_or(|i| i.index.paths.is_empty()) {
        eprintln!("skipping: no mission data (GMS.rez needs the V1 key)");
        return;
    }
    let spawn = data.spawns.first().map(|s| s.0).unwrap_or(Vec3::Y);
    let mut sim = Sim::new(combat_arms_app::SimPlugin, MapSimPlugin::new(data));
    assert_eq!(sim.app.world().resource::<CaRuleSettings>().mode, "TDM", "the room's default mode");
    let p = sim.spawn_character(spawn + Vec3::Y, movement::ID);
    sim.app.world_mut().entity_mut(p).insert(Team(1));
    sim.seconds(1.0);

    let mission = sim.app.world().resource::<CaMission>().0.clone();
    assert_eq!(mission.map(|m| m.path.clone()).as_deref(), Some("gms/shortfuse_t.gms"), "the install's mission");
    let objects = sim.app.world().resource::<CaObjects>();
    let fired = |name: &str| match &objects.world.objects[objects.named(name).unwrap() as usize].kind {
        Kind::Trigger(t) => t.fired,
        _ => panic!("{name} is no trigger"),
    };
    assert_eq!(fired("Level01StartTrigger"), 1, "the mission's start trigger");
    assert_eq!(fired("Level04StartTrigger"), 0, "another mission's");
    for i in 0..8 {
        let id = objects.named(&format!("RotatingDoor0{i}")).unwrap();
        assert!(objects.world.objects[id as usize].removed, "RotatingDoor0{i}: Quarantine's, removed in TDM");
    }
    // `ca_object_props`: the state, where to stand and what the map stores.
    let props = combat_arms::game::objects::props(sim.app.world(), "Qrt_False");
    assert!(props.contains("centre: setpos ") && props.contains("msg RotatingDoor00 remove"), "{props}");
}
