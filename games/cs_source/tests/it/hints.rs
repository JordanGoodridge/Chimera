//! CS:S's automatic hints (`cl_autohelp`, `rules::hints`; every trigger
//! and rule is ours, guessed from the hints' wording: the module's docs):
//! a trigger shows its hint once per player per map, never again on that
//! map, not with `cl_autohelp 0` (and then later, once it's back on),
//! never to bots, and hints keep their gap. The network's is in
//! games/cs_source/tests/it/net.rs (`hints_reach_a_remote_player`).

use bevy::{ecs::message::Messages, prelude::*};

use crate::stack::{
    console::Console,
    core::{Bot, Damage, DamageKind, Hitgroup, LocalPlayer, MapLoads, Team},
    games::cs_source::{TICK_INTERVAL, weapons::CsWeaponsPlugin},
    greybox::GreyboxMapPlugin,
    harness::{CsSim, Sim},
    movement::placeholder,
    rules::hints::{AutoHint, HintRules},
};

const SPOTTED_ENEMY: &str = "Cstrike_TitlesTXT_Hint_spotted_an_enemy";
const BUY: &str = "Cstrike_TitlesTXT_Hint_press_buy_to_purchase";
const KILLED_ENEMY: &str = "Cstrike_TitlesTXT_Hint_win_round_by_killing_enemy";

/// A counter-terrorist local player looking down -Z at a terrorist 5 m
/// ahead on the greybox (no map: buying is open everywhere).
fn setup() -> (Sim, Entity, Entity) {
    let mut sim = Sim::new(CsSim, (GreyboxMapPlugin, CsWeaponsPlugin));
    sim.set_tick_interval(TICK_INTERVAL);
    let me = sim.spawn_character(Vec3::new(0.0, 1.0, 12.0), placeholder::ID);
    let enemy = sim.spawn_character(Vec3::new(0.0, 1.0, 7.0), placeholder::ID);
    sim.app.world_mut().entity_mut(me).insert((Team(2), LocalPlayer));
    sim.app.world_mut().entity_mut(enemy).insert(Team(1));
    (sim, me, enemy)
}

fn set_gap(sim: &mut Sim, gap: f32) {
    sim.app.world_mut().resource_mut::<HintRules>().gap = gap;
}

/// The hints written over `secs` seconds.
fn hints(sim: &mut Sim, secs: f64) -> Vec<AutoHint> {
    let ticks = (secs / TICK_INTERVAL).round() as usize;
    let mut cursor = sim.app.world().resource::<Messages<AutoHint>>().get_cursor_current();
    let mut out = Vec::new();
    for _ in 0..ticks {
        sim.ticks(1);
        out.extend(cursor.read(sim.app.world().resource::<Messages<AutoHint>>()).cloned());
    }
    out
}

fn count(hints: &[AutoHint], to: Entity, token: &str) -> usize {
    hints.iter().filter(|h| h.to == to && h.token == token).count()
}

#[test]
fn a_trigger_shows_its_hint_once_per_map() {
    let (mut sim, me, _) = setup();
    set_gap(&mut sim, 0.0);
    let first = hints(&mut sim, 1.0);
    assert_eq!(count(&first, me, SPOTTED_ENEMY), 1, "{first:?}");
    assert_eq!(count(&first, me, BUY), 1, "{first:?}");
    // Still seen, still in a buy zone: never again on this map.
    let later = hints(&mut sim, 2.0);
    assert!(later.is_empty(), "{later:?}");
    // A new map: once more.
    sim.app.world_mut().get_resource_or_insert_with(MapLoads::default).0 += 1;
    let again = hints(&mut sim, 1.0);
    assert_eq!(count(&again, me, SPOTTED_ENEMY), 1, "{again:?}");
}

#[test]
fn an_enemy_out_of_view_is_not_spotted() {
    let (mut sim, me, enemy) = setup();
    set_gap(&mut sim, 0.0);
    // Behind us.
    sim.app.world_mut().get_mut::<Transform>(enemy).unwrap().translation = Vec3::new(0.0, 1.0, 17.0);
    let seen = hints(&mut sim, 1.0);
    assert_eq!(count(&seen, me, SPOTTED_ENEMY), 0, "{seen:?}");
    // Turned round: now.
    sim.intent(me).yaw = std::f32::consts::PI;
    let seen = hints(&mut sim, 1.0);
    assert_eq!(count(&seen, me, SPOTTED_ENEMY), 1, "{seen:?}");
}

#[test]
fn cl_autohelp_0_shows_none_and_they_come_once_it_is_back_on() {
    let (mut sim, me, _) = setup();
    set_gap(&mut sim, 0.0);
    sim.app.world_mut().resource_mut::<Console>().submit("cl_autohelp 0");
    let off = hints(&mut sim, 1.5);
    assert!(off.is_empty(), "{off:?}");
    sim.app.world_mut().resource_mut::<Console>().submit("cl_autohelp 1");
    let on = hints(&mut sim, 1.0);
    assert_eq!(count(&on, me, SPOTTED_ENEMY), 1, "{on:?}");
}

#[test]
fn bots_get_no_hints_and_hints_keep_their_gap() {
    let (mut sim, me, enemy) = setup();
    sim.app.world_mut().entity_mut(enemy).insert(Bot);
    let gap = sim.app.world().resource::<HintRules>().gap;
    assert!(gap > 1.0, "CS:S's data has a gap: {gap}");
    // Two at once (the buy zone, the enemy in view): the second waits.
    let mut seen = Vec::new();
    let mut at = Vec::new();
    let step = 0.25;
    let mut t = 0.0;
    while t < gap as f64 + 1.0 {
        for h in hints(&mut sim, step) {
            at.push(t);
            seen.push(h);
        }
        t += step;
    }
    assert!(seen.iter().all(|h| h.to == me), "a bot's: {seen:?}");
    assert_eq!(seen.len(), 2, "{seen:?}");
    assert!(at[1] - at[0] >= gap as f64 - step, "{at:?}");
}

#[test]
fn killing_an_enemy_shows_its_hint() {
    let (mut sim, me, enemy) = setup();
    set_gap(&mut sim, 0.0);
    hints(&mut sim, 0.5);
    sim.app.world_mut().write_message(Damage {
        target: enemy,
        attacker: Some(me),
        amount: 5.0,
        point: Vec3::new(0.0, 1.0, 7.0),
        dir: Vec3::NEG_Z,
        hitgroup: Hitgroup::Generic,
        kind: DamageKind::Generic,
        weapon: None,
        force: Vec3::ZERO,
    });
    let seen = hints(&mut sim, 0.5);
    assert_eq!(count(&seen, me, KILLED_ENEMY), 1, "{seen:?}");
}

#[test]
fn the_bomb_carrier_hears_of_the_bomb_then_of_the_target() {
    use avian3d::prelude::Position;
    use crate::stack::objectives::{MapObjectives, Zone};
    const HAVE_BOMB: &str = "Cstrike_TitlesTXT_Hint_you_have_the_bomb";
    const IN_TARGET: &str = "Cstrike_TitlesTXT_Hint_you_are_in_targetzone";
    let mut sim = Sim::new(CsSim, (GreyboxMapPlugin, CsWeaponsPlugin));
    sim.set_tick_interval(TICK_INTERVAL);
    sim.app.insert_resource(MapObjectives {
        bomb_targets: vec![Zone::engine_box(Vec3::new(-8.0, -1.0, 0.0), Vec3::new(-4.0, 3.0, 4.0))],
        ..default()
    });
    let me = sim.spawn_character(Vec3::new(0.0, 1.0, -20.0), placeholder::ID);
    sim.app.world_mut().entity_mut(me).insert((Team(1), LocalPlayer));
    set_gap(&mut sim, 0.0);
    sim.app
        .world_mut()
        .resource_mut::<Console>()
        .submit("mp_freezetime 1; mp_roundtime 1; chimera_rounds 1");
    let given = hints(&mut sim, 2.0);
    assert_eq!(count(&given, me, HAVE_BOMB), 1, "{given:?}");
    assert_eq!(count(&given, me, IN_TARGET), 0, "{given:?}");
    let at = Vec3::new(-6.0, 1.0, 2.0);
    sim.app.world_mut().get_mut::<Transform>(me).unwrap().translation = at;
    if let Some(mut p) = sim.app.world_mut().get_mut::<Position>(me) {
        p.0 = at;
    }
    let inside = hints(&mut sim, 1.0);
    assert_eq!(count(&inside, me, IN_TARGET), 1, "{inside:?}");
}
