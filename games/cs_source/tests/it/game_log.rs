//! CS:S's game events and server log lines (`rules::events`, `game_log`):
//! the bomb dropped and picked up, a gun fired, a death and a round's end
//! give the specs' events and `triggered` lines, the players named
//! `"Name<userid><uniqueid><TEAM>"`.

use avian3d::prelude::Position;
use bevy::prelude::*;
use crate::stack::{
    console::Console,
    core::{Damage, Hitgroup, Team},
    game_log::{EventValue, GameEvent, ServerLog},
    games::cs_source::{TICK_INTERVAL, objectives::C4, weapons::{AK47, CsWeaponsPlugin}},
    greybox::GreyboxMapPlugin,
    harness::{CsSim, Sim},
    movement::placeholder,
    objectives::{MapObjectives, Zone, bomb::C4 as C4Part},
    weapon::{Weapon, drop::Loose, give},
};

#[derive(Resource, Default)]
struct Events(Vec<GameEvent>);

fn record(mut events: MessageReader<GameEvent>, mut log: ResMut<Events>) {
    log.0.extend(events.read().cloned());
}

fn sim() -> Sim {
    let mut sim = Sim::new(CsSim, (GreyboxMapPlugin, CsWeaponsPlugin));
    sim.set_tick_interval(TICK_INTERVAL);
    sim.app.init_resource::<Events>();
    sim.app.add_systems(Last, record);
    sim.app.insert_resource(MapObjectives {
        bomb_targets: vec![Zone::engine_box(Vec3::new(-3.0, -1.0, 8.0), Vec3::new(3.0, 3.0, 16.0))],
        ..default()
    });
    sim.app
        .world_mut()
        .resource_mut::<Console>()
        .submit("log on; sv_logfile 0");
    sim.ticks(2);
    sim
}

fn character(sim: &mut Sim, at: Vec3, team: u8, name: &str) -> Entity {
    let e = sim.spawn_character(at, placeholder::ID);
    sim.app.world_mut().entity_mut(e).insert((Team(team), Name::new(name.to_string())));
    e
}

fn kill(sim: &mut Sim, target: Entity, attacker: Option<Entity>) {
    sim.app.world_mut().write_message(Damage {
        force: Vec3::ZERO,
        target,
        attacker,
        amount: 10.0,
        point: Vec3::ZERO,
        dir: Vec3::X,
        hitgroup: Hitgroup::Head,
        kind: default(),
        weapon: None,
    });
    sim.ticks(3);
}

fn events(sim: &Sim, name: &str) -> Vec<GameEvent> {
    sim.app.world().resource::<Events>().0.iter().filter(|e| e.name == name).cloned().collect()
}

fn lines(sim: &Sim) -> Vec<String> {
    sim.app.world().resource::<ServerLog>().lines.iter().cloned().collect()
}

fn logged(sim: &Sim, text: &str) -> bool {
    lines(sim).iter().any(|l| l.starts_with("L ") && l.ends_with(text))
}

#[test]
fn the_bomb_dropped_and_picked_up_is_logged() {
    let mut sim = sim();
    let t = character(&mut sim, Vec3::new(0.0, 1.0, 12.0), 1, "Tom");
    let ct = character(&mut sim, Vec3::new(-20.0, 1.0, 0.0), 2, "Cat");
    sim.ticks(2);
    give(sim.app.world_mut(), t, C4).unwrap();
    sim.seconds(1.2);
    kill(&mut sim, t, Some(ct));
    let tid = t.index().index();
    assert!(
        events(&sim, "bomb_dropped").iter().any(|e| e.player("userid") == Some(t)),
        "{:?}",
        sim.app.world().resource::<Events>().0
    );
    assert!(
        logged(&sim, &format!(": \"Tom<{tid}><STEAM_ID_LAN><TERRORIST>\" triggered \"Dropped_The_Bomb\"")),
        "{:#?}",
        lines(&sim)
    );
    // The death: by the CT, a headshot, with what he held (nothing).
    let death = events(&sim, "player_death");
    assert_eq!(death.len(), 1);
    assert_eq!(death[0].player("userid"), Some(t));
    assert_eq!(death[0].player("attacker"), Some(ct));
    assert_eq!(death[0].get("headshot"), Some(&EventValue::Bool(true)));
    // Its log line (CS:S, measured 2026-10-18, objectives.md Q17: `"A"
    // killed "B" with "usp"`).
    let cid = ct.index().index();
    let weapon = match death[0].get("weapon") {
        Some(EventValue::Str(w)) => w.clone(),
        other => panic!("weapon {other:?}"),
    };
    assert!(
        logged(
            &sim,
            &format!(
                ": \"Cat<{cid}><STEAM_ID_LAN><CT>\" killed \"Tom<{tid}><STEAM_ID_LAN><TERRORIST>\" with \"{weapon}\""
            )
        ),
        "{:#?}",
        lines(&sim)
    );
    // A death with no attacker is a suicide "with world".
    let lone = character(&mut sim, Vec3::new(20.0, 1.0, 0.0), 2, "Lou");
    sim.ticks(2);
    kill(&mut sim, lone, None);
    let lid = lone.index().index();
    assert!(
        logged(&sim, &format!(": \"Lou<{lid}><STEAM_ID_LAN><CT>\" committed suicide with \"world\"")),
        "{:#?}",
        lines(&sim)
    );
    // Another terrorist walks over it.
    let at = {
        let w = sim.app.world_mut();
        w.query::<(&Loose, &Transform)>()
            .iter(w)
            .find(|(l, _)| w.get::<C4Part>(l.weapon).is_some())
            .map(|(_, t)| t.translation)
            .expect("the bomb lies there")
    };
    let t2 = character(&mut sim, Vec3::new(-20.0, 1.0, 20.0), 1, "Ann");
    sim.ticks(80);
    {
        let w = sim.app.world_mut();
        let p = at + Vec3::Y * 0.5;
        w.get_mut::<Transform>(t2).unwrap().translation = p;
        w.get_mut::<Position>(t2).unwrap().0 = p;
    }
    sim.ticks(20);
    assert!(events(&sim, "bomb_pickup").iter().any(|e| e.player("userid") == Some(t2)));
    assert!(
        events(&sim, "item_pickup")
            .iter()
            .any(|e| e.player("userid") == Some(t2) && e.get("item") == Some(&EventValue::Str("c4".into())))
    );
    let aid = t2.index().index();
    assert!(
        logged(&sim, &format!(": \"Ann<{aid}><STEAM_ID_LAN><TERRORIST>\" triggered \"Got_The_Bomb\"")),
        "{:#?}",
        lines(&sim)
    );
    // Echoed to the console as well.
    let console = sim.app.world().resource::<Console>();
    assert!(console.output.iter().any(|l| l.text.ends_with("triggered \"Got_The_Bomb\"")));
    let w = sim.app.world_mut();
    let carried = w.query::<(&Weapon, &C4Part)>().iter(w).filter_map(|(w, _)| w.owner).collect::<Vec<_>>();
    assert_eq!(carried, vec![t2]);
}

#[test]
fn shots_and_a_rounds_end_are_events_and_lines() {
    let mut sim = sim();
    let t = character(&mut sim, Vec3::new(0.0, 1.0, 12.0), 1, "Tom");
    let ct = character(&mut sim, Vec3::new(5.0, 1.0, 12.0), 2, "Cat");
    sim.app
        .world_mut()
        .resource_mut::<Console>()
        .submit("mp_freezetime 0; mp_roundtime 1; mashup_rounds 1");
    sim.ticks(3);
    give(sim.app.world_mut(), t, AK47).unwrap();
    sim.seconds(1.2);
    // Down at the floor: every round hits something.
    sim.intent(t).pitch = -1.2;
    sim.intent(t).fire = true;
    sim.ticks(20);
    sim.intent(t).fire = false;
    sim.ticks(2);
    let fired = events(&sim, "weapon_fire");
    assert!(!fired.is_empty(), "the AK-47 fired");
    assert!(fired.iter().all(|e| e.get("weapon") == Some(&EventValue::Str("ak47".into()))));
    let impacts = events(&sim, "bullet_impact");
    assert!(impacts.len() >= fired.len(), "{} impacts for {} rounds", impacts.len(), fired.len());
    assert!(impacts.iter().all(|e| e.player("userid") == Some(t) && e.get("z").is_some()));
    // The last CT dies: the terrorists win by elimination.
    kill(&mut sim, ct, Some(t));
    sim.ticks(5);
    let death = events(&sim, "player_death");
    assert_eq!(death.last().and_then(|e| e.get("weapon")), Some(&EventValue::Str("ak47".into())));
    let end = events(&sim, "round_end");
    assert_eq!(end.len(), 1, "{:#?}", lines(&sim));
    assert_eq!(end[0].get("winner"), Some(&EventValue::Int(2)));
    assert_eq!(end[0].get("message"), Some(&EventValue::Str("#Terrorists_Win".into())));
    let all = lines(&sim);
    let win = all.iter().position(|l| l.ends_with(": Team \"TERRORIST\" triggered \"Terrorists_Win\""));
    let over = all.iter().position(|l| l.ends_with(": World triggered \"Round_End\""));
    assert!(win.is_some() && over > win, "{all:#?}");
}
