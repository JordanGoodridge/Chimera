//! specs/combat_arms/inventory.md's test cases, one test each (`t01_` is
//! test case 1), on `CaSim` and the greybox with the spec's stock loadout:
//! the M16A3 and M92FS as fixtures (tests/data/: the spec's clip, total,
//! draw, reload and post-sprint times; fire rate and damage ours), the
//! game's M9 (data/weapons/m9.toml) and an M67 on Chimera's
//! `grenade::Throwable` (400 ms draw, hide-when-empty).
//!
//! Times count from the tick that first sees an input (t = 0); the sim
//! ticks at the game's tick (15 ms), the spec's frames are 60 fps, and "+-f" tolerances are
//! the spec's frames (1/60 s).

use bevy::{ecs::system::RunSystemOnce, prelude::*};
use chimera_core::core::{Damage, DamageKind, Health, Hitgroup, MovementState, SimTick};
use chimera_sim::weapon::{
    Inventory, Magazine, RegisterWeapons, StartingWeapons, Weapon, WeaponEvent, WeaponEventKind,
    data::WeaponDataAppExt,
    grenade::{Flight, GrenadeEffect, Smoke, ThrowRule, Throwable},
};
use chimera_world::{
    greybox::{self, GreyboxMapPlugin},
    harness::Sim,
    map::loose::LooseItem,
};
use combat_arms::{
    game::{
        inventory::{self, CaDeathDrops, CaItem, CaSelection, Dropped, Knife, KnifePhase, UNIT},
        movement,
    },
    harness::CaSim,
};

const RIFLE: &str = "combat_arms:test_m16a3";
const PISTOL: &str = "combat_arms:test_m92fs";
const GRENADE: &str = "combat_arms:test_m67";
const KNIFE: &str = inventory::KNIFE;
/// The stock loadout (Test cases: common setup), in loadout order.
const STOCK: &[&str] = &[RIFLE, PISTOL, KNIFE, GRENADE];
/// One of the spec's frames (60 fps).
const F: f64 = 1.0 / 60.0;

/// What weapons did, by tick.
#[derive(Resource, Default)]
struct Log(Vec<(u64, Entity, WeaponEventKind)>);

fn log_events(mut events: MessageReader<WeaponEvent>, tick: Res<SimTick>, mut log: ResMut<Log>) {
    for e in events.read() {
        log.0.push((tick.0, e.owner, e.kind.clone()));
    }
}

/// Damage dealt, by tick: (tick, target, points).
#[derive(Resource, Default)]
struct Hurt(Vec<(u64, Entity, f32)>);

fn log_damage(mut damage: MessageReader<Damage>, tick: Res<SimTick>, mut log: ResMut<Hurt>) {
    for d in damage.read() {
        log.0.push((tick.0, d.target, d.amount / 0.01));
    }
}

/// The M67 on Chimera's throwable part: 700 ms pin (pre-fire), the throw
/// 67 ms after the release, 433 ms of throw animation (67 + 366), drawn
/// in 400 ms; it hides when empty (inventory.md Constants).
fn grenade(e: &mut EntityWorldMut) {
    e.insert((
        Weapon {
            id: GRENADE,
            slot: 3,
            owner: None,
            draw_time: 0.4,
            max_speed: None,
        },
        CaItem {
            hide_when_empty: true,
            sprint_out: 0.267,
            ..default()
        },
        Throwable {
            count: 1,
            max: 1,
            pin_time: 0.7,
            throw_delay: 0.067,
            throw_time: 0.433,
            redraw_time: 0.4,
            fuse: 1.5,
            throw: ThrowRule {
                pitch_lift: 0.0,
                pitch_scale: 1.0,
                pitch_scale_up: 1.0,
                speed_per_deg: 0.1,
                speed_max: 10.0,
                forward: 0.2,
            },
            flight: Flight {
                half: 0.05,
                gravity: 9.8,
                elasticity: 0.45,
                player_elasticity: 0.3,
                elasticity_cap: 0.9,
                stop_speed: 0.5,
                floor_normal: 0.7,
                breakable_damage: 0.0,
                breakable_slowdown: 1.0,
                water_slowdown: 1.0,
                water_entry: 1.0,
                max_velocity: 50.0,
                check_interval: 0.1,
                spin_roll: 0.0,
                spin_pitch: 0.0,
            },
            effect: GrenadeEffect::Smoke(Smoke {
                max_radius: 1.0,
                expand_time: 1.0,
                fade_start: 2.0,
                fade_end: 3.0,
                pop_speed: 0.1,
                sight_radius: 0.75,
                sound: None,
            }),
            pin_sounds: Vec::new(),
            bounce_sound: None,
            pin: false,
            throw_at: None,
            redraw_at: None,
        },
    ));
}

fn sim_with(loadout: &[&'static str]) -> Sim {
    let loadout = loadout.to_vec();
    Sim::with(CaSim, move |app| {
        app.add_plugins(GreyboxMapPlugin);
        app.add_weapon_data(&[
            ("test_m16a3.toml", include_str!("../data/test_m16a3.toml")),
            ("test_m92fs.toml", include_str!("../data/test_m92fs.toml")),
        ])
        .unwrap();
        app.register_weapon(GRENADE, grenade);
        app.insert_resource(StartingWeapons {
            team: Vec::new(),
            all: loadout,
        });
        app.init_resource::<Log>()
            .init_resource::<Hurt>()
            .add_systems(FixedPostUpdate, (log_events, log_damage));
    })
}

/// A player with `loadout`, settled with its first draw done.
fn setup(loadout: &[&'static str]) -> (Sim, Entity) {
    let mut sim = sim_with(loadout);
    let p = player(&mut sim, greybox::SPAWNS[0]);
    (sim, p)
}

fn player(sim: &mut Sim, at: Vec3) -> Entity {
    let p = sim.spawn_character(at, movement::ID);
    sim.seconds(1.5);
    p
}

fn inv(sim: &Sim, p: Entity) -> Inventory {
    sim.app.world().get::<Inventory>(p).unwrap().clone()
}

fn sel(sim: &Sim, p: Entity) -> CaSelection {
    sim.app.world().get::<CaSelection>(p).unwrap().clone()
}

fn active(sim: &Sim, p: Entity) -> Option<Entity> {
    inv(sim, p).active
}

fn id(sim: &Sim, w: Entity) -> &'static str {
    sim.app.world().get::<Weapon>(w).unwrap().id
}

fn active_id(sim: &Sim, p: Entity) -> &'static str {
    id(sim, active(sim, p).expect("nothing in hand"))
}

/// The first weapon `p` carries with this id.
fn carried(sim: &Sim, p: Entity, weapon: &str) -> Entity {
    inv(sim, p).weapons.into_iter().find(|w| id(sim, *w) == weapon).unwrap()
}

/// (clip, total) of a magazine weapon.
fn ammo(sim: &Sim, w: Entity) -> (u32, u32) {
    let m = sim.app.world().get::<Magazine>(w).unwrap();
    (m.clip, m.clip + m.reserve)
}

fn set_ammo(sim: &mut Sim, w: Entity, clip: u32, total: u32) {
    let mut m = sim.app.world_mut().get_mut::<Magazine>(w).unwrap();
    m.clip = clip;
    m.reserve = total - clip;
}

/// The tick that will see the next input: t = 0.
fn mark(sim: &Sim) -> u64 {
    sim.tick() + 1
}

fn dt(sim: &Sim) -> f64 {
    1.0 / sim.tick_hz()
}

/// The time of the last tick run, from `t0`.
fn now(sim: &Sim, t0: u64) -> f64 {
    (sim.tick() as f64 - t0 as f64) * dt(sim)
}

/// The time of `tick` from `t0`.
fn time_of(sim: &Sim, t0: u64, tick: u64) -> f64 {
    (tick as f64 - t0 as f64) * dt(sim)
}

/// Run until the next tick is the one at time `t` (so an input set now
/// is seen at `t`).
fn wait_until(sim: &mut Sim, t0: u64, t: f64) {
    let target = t0 + (t / dt(sim)).round() as u64;
    while sim.tick() + 1 < target {
        sim.ticks(1);
    }
}

/// Run tick by tick until `cond`; its time from `t0`.
fn until(sim: &mut Sim, t0: u64, limit: f64, cond: impl Fn(&Sim) -> bool) -> f64 {
    loop {
        sim.ticks(1);
        if cond(sim) {
            return now(sim, t0);
        }
        assert!(now(sim, t0) < limit, "nothing by {limit} s");
    }
}

/// Press class key `class` (1-6) for one tick.
/// The key is let go after its tick; the release is seen with the next
/// tick (so two presses in a row are two presses).
fn key(sim: &mut Sim, p: Entity, class: u8) {
    sim.intent(p).select = Some(class - 1);
    sim.ticks(1);
    sim.intent(p).select = None;
    release(sim, p);
}

/// Let the next tick see the release before another press.
fn release(sim: &mut Sim, p: Entity) {
    let _ = p;
    sim.ticks(1);
}

fn q(sim: &mut Sim, p: Entity) {
    sim.intent(p).last_weapon = true;
    sim.ticks(1);
    sim.intent(p).last_weapon = false;
    release(sim, p);
}

fn tap(sim: &mut Sim, p: Entity, f: impl Fn(&mut chimera_core::core::Intent, bool)) {
    f(&mut sim.intent(p), true);
    sim.ticks(1);
    f(&mut sim.intent(p), false);
}

/// Ticks at which `p`'s weapons logged events matching `pick`.
fn logged(sim: &Sim, p: Entity, pick: impl Fn(&WeaponEventKind) -> bool) -> Vec<u64> {
    sim.app
        .world()
        .resource::<Log>()
        .0
        .iter()
        .filter(|(_, o, k)| *o == p && pick(k))
        .map(|(t, ..)| *t)
        .collect()
}

fn swings(sim: &Sim, p: Entity) -> Vec<u64> {
    logged(sim, p, |k| matches!(k, WeaponEventKind::Swing { .. }))
}

fn knife_phase(sim: &Sim, p: Entity) -> KnifePhase {
    let k = carried(sim, p, KNIFE);
    sim.app.world().get::<Knife>(k).unwrap().state.phase
}

fn close(t: f64, want: f64, frames: f64) -> bool {
    (t - want).abs() <= frames * F + 1e-6
}

/// Hold fire until the active weapon's clip drops: the first shot's time.
fn first_shot(sim: &mut Sim, p: Entity, t0: u64, w: Entity) -> f64 {
    let before = ammo(sim, w).0;
    sim.intent(p).fire = true;
    until(sim, t0, 6.0, |s| ammo(s, w).0 < before)
}

// ---------------------------------------------------------------------------
// Draw times (1-5)

#[test]
fn t01_draw_rifle() {
    let (mut sim, p) = setup(STOCK);
    key(&mut sim, p, 2);
    sim.seconds(1.0);
    let pistol = carried(&sim, p, PISTOL);
    let pistol_ammo = ammo(&sim, pistol);
    let t0 = mark(&sim);
    sim.intent(p).fire = true;
    key(&mut sim, p, 1);
    assert_eq!(active_id(&sim, p), RIFLE, "in hand at t = 0");
    let rifle = carried(&sim, p, RIFLE);
    let t = first_shot(&mut sim, p, t0, rifle);
    assert!(close(t, 1.2, 2.0), "first shot at {t}");
    assert_eq!(ammo(&sim, pistol), pistol_ammo);
}

#[test]
fn t02_draw_pistol() {
    let (mut sim, p) = setup(STOCK);
    let t0 = mark(&sim);
    key(&mut sim, p, 2);
    assert_eq!(active_id(&sim, p), PISTOL);
    let pistol = carried(&sim, p, PISTOL);
    let t = first_shot(&mut sim, p, t0, pistol);
    assert!(close(t, 0.65, 2.0), "ready at {t}");
}

#[test]
fn t03_draw_knife() {
    let (mut sim, p) = setup(STOCK);
    let t0 = mark(&sim);
    sim.intent(p).fire = true;
    key(&mut sim, p, 3);
    assert_eq!(active_id(&sim, p), KNIFE);
    let t = until(&mut sim, t0, 2.0, |s| !swings(s, p).is_empty());
    assert!(close(t, 0.45 + 0.155, 3.0), "first hit at {t}");
}

#[test]
fn t04_draw_grenade() {
    let (mut sim, p) = setup(STOCK);
    let t0 = mark(&sim);
    sim.intent(p).fire = true;
    key(&mut sim, p, 4);
    assert_eq!(active_id(&sim, p), GRENADE);
    let t = until(&mut sim, t0, 2.0, |s| {
        !logged(s, p, |k| matches!(k, WeaponEventKind::PinPulled)).is_empty()
    });
    assert!(close(t, 0.4, 2.0), "fire accepted at {t}");
}

#[test]
fn t05_no_redraw() {
    let (mut sim, p) = setup(STOCK);
    key(&mut sim, p, 2);
    sim.seconds(1.0);
    let t0 = mark(&sim);
    key(&mut sim, p, 1);
    let rifle = active(&sim, p).unwrap();
    wait_until(&mut sim, t0, 0.5);
    key(&mut sim, p, 1);
    assert_eq!(active(&sim, p), Some(rifle), "nothing changes");
    let t = first_shot(&mut sim, p, t0, rifle);
    assert!(close(t, 1.2, 2.0), "ready at {t}");
}

// ---------------------------------------------------------------------------
// Class keys and Q (6-10)

#[test]
fn t06_cycle_primaries() {
    let (mut sim, p) = setup(&[RIFLE, PISTOL, KNIFE, GRENADE, RIFLE]);
    let weapons = inv(&sim, p).weapons;
    let (a, b) = (weapons[0], weapons[4]);
    assert_eq!(active(&sim, p), Some(a), "A drawn at spawn");
    key(&mut sim, p, 1);
    assert_eq!(active(&sim, p), Some(b));
    key(&mut sim, p, 1);
    assert_eq!(active(&sim, p), Some(a));
    key(&mut sim, p, 2);
    assert_eq!(active_id(&sim, p), PISTOL);
    key(&mut sim, p, 1);
    assert_eq!(active(&sim, p), Some(a), "the last primary used");
}

#[test]
fn t07_q_toggle() {
    let (mut sim, p) = setup(STOCK);
    let rifle = active(&sim, p).unwrap();
    q(&mut sim, p);
    assert_eq!(active(&sim, p), Some(rifle), "previous empty: nothing");
    key(&mut sim, p, 2);
    q(&mut sim, p);
    assert_eq!(active_id(&sim, p), RIFLE);
    q(&mut sim, p);
    assert_eq!(active_id(&sim, p), PISTOL);
    q(&mut sim, p);
    assert_eq!(active_id(&sim, p), RIFLE);
}

/// Throw the grenade in hand: hold fire past the pin, then let go.
fn throw(sim: &mut Sim, p: Entity) {
    sim.intent(p).fire = true;
    sim.seconds(1.0);
    sim.intent(p).fire = false;
    sim.ticks(1);
    let g = carried(sim, p, GRENADE);
    assert_eq!(sim.app.world().get::<Throwable>(g).unwrap().count, 0, "thrown");
}

#[test]
fn t08_q_after_the_last_grenade() {
    let (mut sim, p) = setup(STOCK);
    key(&mut sim, p, 4);
    sim.seconds(0.5);
    throw(&mut sim, p);
    let t0 = mark(&sim);
    until(&mut sim, t0, 2.0, |s| active(s, p).is_some_and(|a| id(s, a) == RIFLE));
    assert_eq!(sel(&sim, p).previous, None, "Q cleared by the auto-switch");
    q(&mut sim, p);
    sim.ticks(2);
    assert_eq!(active_id(&sim, p), RIFLE, "Q does nothing");
}

#[test]
fn t09_dead_q() {
    let (mut sim, p) = setup(STOCK);
    key(&mut sim, p, 4);
    sim.seconds(0.5);
    throw(&mut sim, p);
    let g = carried(&sim, p, GRENADE);
    // Its ammo used, the throw animation still playing.
    assert!(sim.app.world().get::<Throwable>(g).unwrap().redraw_at.is_some());
    key(&mut sim, p, 2);
    assert_eq!(active_id(&sim, p), PISTOL, "no auto-switch: the grenade isn't in hand");
    assert_eq!(sel(&sim, p).previous, Some(g), "previous: the spent M67");
    let pistol = active(&sim, p).unwrap();
    q(&mut sim, p);
    assert_eq!(active(&sim, p), Some(pistol), "Q: nothing");
    assert_eq!(sel(&sim, p).previous, Some(pistol), "Q overwrote previous");
    q(&mut sim, p);
    assert_eq!(active(&sim, p), Some(pistol), "still nothing");
    key(&mut sim, p, 1);
    assert_eq!(active_id(&sim, p), RIFLE);
    q(&mut sim, p);
    assert_eq!(active(&sim, p), Some(pistol));
}

#[test]
fn t10_empty_grenade_key() {
    let (mut sim, p) = setup(STOCK);
    let g = carried(&sim, p, GRENADE);
    sim.app.world_mut().get_mut::<Throwable>(g).unwrap().count = 0;
    key(&mut sim, p, 4);
    sim.ticks(1);
    assert_eq!(active_id(&sim, p), RIFLE, "nothing happens");
}

// ---------------------------------------------------------------------------
// What interrupts what (11-16)

#[test]
fn t11_switch_mid_reload() {
    let (mut sim, p) = setup(STOCK);
    let rifle = carried(&sim, p, RIFLE);
    set_ammo(&mut sim, rifle, 10, 100);
    let t0 = mark(&sim);
    tap(&mut sim, p, |i, on| i.reload = on);
    wait_until(&mut sim, t0, 1.0);
    key(&mut sim, p, 2);
    wait_until(&mut sim, t0, 2.0);
    key(&mut sim, p, 1);
    assert_eq!(ammo(&sim, rifle), (10, 100), "the reload never completed");
    let t = first_shot(&mut sim, p, t0, rifle);
    assert!(close(t, 3.2, 2.0), "ready at {t}");
}

#[test]
fn t12_switch_to_an_empty_clip() {
    let (mut sim, p) = setup(STOCK);
    let rifle = carried(&sim, p, RIFLE);
    set_ammo(&mut sim, rifle, 0, 90);
    key(&mut sim, p, 2);
    sim.seconds(1.0);
    let t0 = mark(&sim);
    key(&mut sim, p, 1);
    assert_eq!(
        logged(&sim, p, |k| matches!(k, WeaponEventKind::ReloadStarted)),
        vec![t0],
        "the reload starts at once"
    );
    let t = until(&mut sim, t0, 4.0, |s| ammo(s, rifle).0 > 0);
    assert!(close(t, 2.3, 2.0), "reloaded at {t}");
    assert_eq!(ammo(&sim, rifle), (30, 90));
    let t1 = mark(&sim);
    let shot = first_shot(&mut sim, p, t1, rifle);
    assert!(shot < 1.5 * F, "ready: shot after {shot}");
}

#[test]
fn t13_reload_cancels_the_draw() {
    let (mut sim, p) = setup(STOCK);
    let rifle = carried(&sim, p, RIFLE);
    set_ammo(&mut sim, rifle, 20, 110);
    key(&mut sim, p, 2);
    sim.seconds(1.0);
    let t0 = mark(&sim);
    key(&mut sim, p, 1);
    wait_until(&mut sim, t0, 0.1);
    tap(&mut sim, p, |i, on| i.reload = on);
    let started = logged(&sim, p, |k| matches!(k, WeaponEventKind::ReloadStarted));
    assert_eq!(started.len(), 1);
    assert!(close(time_of(&sim, t0, started[0]), 0.1, 1.0));
    let t = until(&mut sim, t0, 4.0, |s| ammo(s, rifle).0 == 30);
    assert!(close(t, 2.4, 2.0), "ready at {t}");
    assert_eq!(ammo(&sim, rifle), (30, 110));
    let t1 = mark(&sim);
    assert!(first_shot(&mut sim, p, t1, rifle) < 1.5 * F);
}

#[test]
fn t14_fire_during_reload() {
    let (mut sim, p) = setup(STOCK);
    let rifle = carried(&sim, p, RIFLE);
    set_ammo(&mut sim, rifle, 5, 65);
    let t0 = mark(&sim);
    tap(&mut sim, p, |i, on| i.reload = on);
    wait_until(&mut sim, t0, 0.5);
    sim.intent(p).fire = true;
    let t = until(&mut sim, t0, 4.0, |s| ammo(s, rifle).0 != 5);
    assert!(close(t, 2.3, 1.0), "first shot at {t}");
    assert_eq!(ammo(&sim, rifle), (29, 64));
}

/// Sprint forward (the placeholder movement's sprint needs both).
fn sprint(sim: &mut Sim, p: Entity, on: bool) {
    let mut i = sim.intent(p);
    i.sprint = on;
    i.move_axis = if on { Vec2::Y } else { Vec2::ZERO };
}

#[test]
fn t15_sprint_during_reload() {
    let (mut sim, p) = setup(STOCK);
    let rifle = carried(&sim, p, RIFLE);
    set_ammo(&mut sim, rifle, 5, 65);
    let t0 = mark(&sim);
    tap(&mut sim, p, |i, on| i.reload = on);
    wait_until(&mut sim, t0, 1.0);
    sprint(&mut sim, p, true);
    sim.ticks(3);
    assert!(sim.state(p).sprinting, "sprinting");
    assert!(!sel(&sim, p).sprint_pose, "the reload goes on");
    let t = until(&mut sim, t0, 4.0, |s| ammo(s, rifle).0 == 30);
    assert!(close(t, 2.3, 1.0), "reloaded at {t}");
    assert_eq!(ammo(&sim, rifle), (30, 65));
    assert!(sel(&sim, p).sprint_pose, "then the sprint pose");
}

/// Sprint until the weapon in hand is in the sprint pose, then release
/// sprint holding fire: the time of the first shot (or knife hit).
fn leave_sprint(sim: &mut Sim, p: Entity, shot: impl Fn(&Sim) -> bool) -> f64 {
    sprint(sim, p, true);
    sim.seconds(0.5);
    assert!(sel(sim, p).sprint_pose, "in the sprint pose");
    sim.intent(p).fire = true;
    sim.ticks(2);
    assert!(!shot(sim), "fire ignored while sprinting");
    let t0 = mark(sim);
    sprint(sim, p, false);
    until(sim, t0, 2.0, shot)
}

#[test]
fn t16_leaving_sprint() {
    let (mut sim, p) = setup(STOCK);
    let rifle = carried(&sim, p, RIFLE);
    let t = leave_sprint(&mut sim, p, |s| ammo(s, rifle).0 < 30);
    assert!(close(t, 0.367, 2.0), "rifle: first shot at {t}");

    let (mut sim, p) = setup(STOCK);
    key(&mut sim, p, 2);
    sim.seconds(1.0);
    let pistol = carried(&sim, p, PISTOL);
    let t = leave_sprint(&mut sim, p, |s| ammo(s, pistol).0 < 15);
    assert!(close(t, 0.4, 2.0), "pistol: first shot at {t}");

    let (mut sim, p) = setup(STOCK);
    key(&mut sim, p, 3);
    sim.seconds(1.0);
    let t = leave_sprint(&mut sim, p, |s| !swings(s, p).is_empty());
    assert!(close(t, 0.167 + 0.001 + 0.155, 3.0), "knife: first hit at {t}");
}

// ---------------------------------------------------------------------------
// The knife (17-25)

fn knife_out(loadout: &[&'static str]) -> (Sim, Entity) {
    let (mut sim, p) = setup(loadout);
    key(&mut sim, p, 3);
    sim.seconds(1.0);
    assert_eq!(knife_phase(&sim, p), KnifePhase::Idle);
    (sim, p)
}

#[test]
fn t17_knife_primary_timing() {
    let (mut sim, p) = knife_out(STOCK);
    let t0 = mark(&sim);
    sim.intent(p).fire = true;
    wait_until(&mut sim, t0, 0.7);
    sim.intent(p).fire = false;
    let idle = until(&mut sim, t0, 3.0, |s| knife_phase(s, p) == KnifePhase::Idle);
    let hits: Vec<f64> = swings(&sim, p).into_iter().map(|t| time_of(&sim, t0, t)).collect();
    assert_eq!(hits.len(), 4, "hits at {hits:?}");
    for (t, want) in hits.iter().zip([0.156, 0.403, 0.806, 1.053]) {
        assert!(close(*t, want, 3.0), "hit at {t}, want {want}: {hits:?}");
    }
    assert!(close(idle, 1.302, 3.0), "idle at {idle}");
}

#[test]
fn t18_knife_secondary_timing() {
    let (mut sim, p) = knife_out(STOCK);
    let t0 = mark(&sim);
    tap(&mut sim, p, |i, on| i.secondary = on);
    let t = until(&mut sim, t0, 2.0, |s| !swings(s, p).is_empty());
    assert!(close(t, 0.368, 3.0), "hit at {t}");
    // Not again until the stab ends: a click at 1 s does nothing.
    wait_until(&mut sim, t0, 1.0);
    tap(&mut sim, p, |i, on| i.secondary = on);
    let idle = until(&mut sim, t0, 3.0, |s| knife_phase(s, p) == KnifePhase::Idle);
    assert!(close(idle, 1.101, 3.0), "fire possible again at {idle}");
    sim.seconds(1.0);
    assert_eq!(swings(&sim, p).len(), 1, "the second click was ignored");
}

#[test]
fn t19_both_buttons() {
    let (mut sim, p) = knife_out(STOCK);
    let t0 = mark(&sim);
    sim.intent(p).fire = true;
    sim.intent(p).secondary = true;
    let t = until(&mut sim, t0, 2.0, |s| !swings(s, p).is_empty());
    assert!(close(t, 0.156, 3.0), "hit at {t}");
    let k = carried(&sim, p, KNIFE);
    assert_eq!(sim.app.world().get::<Knife>(k).unwrap().state.attack, 0, "a primary swing");
    let secondary = logged(&sim, p, |k| matches!(k, WeaponEventKind::Swing { secondary: true, .. }));
    assert!(secondary.is_empty(), "not a stab");
}

/// A knife player looking down -Z and a target facing `yaw` whose
/// surface, at the attacker's eye height, is `surface` metres ahead of
/// the eye (health to spare: hits are read from the damage dealt).
fn knife_target(surface: f32, yaw: f32) -> (Sim, Entity, Entity) {
    let (mut sim, p) = knife_out(STOCK);
    // Where the knife's ray (from the eye, straight ahead) meets the
    // target's body, measured with the target 2 m ahead: then the target
    // is moved so that surface lies `surface` ahead.
    let probe = 2.0;
    let target = sim.spawn_character(sim.position(p) - Vec3::Z * probe, movement::ID);
    sim.intent(target).yaw = yaw;
    sim.app.world_mut().get_mut::<Health>(target).unwrap().current = 10.0;
    sim.seconds(0.5);
    let eye = sim.position(p) + sim.state(p).eye_offset;
    let hit = sim
        .app
        .world_mut()
        .run_system_once(move |q: avian3d::prelude::SpatialQuery| {
            let filter = avian3d::prelude::SpatialQueryFilter::from_excluded_entities([p]);
            q.cast_ray(eye, Dir3::NEG_Z, 10.0, true, &filter).map(|h| (h.entity, h.distance))
        })
        .unwrap()
        .expect("the ray meets the target");
    assert_eq!(hit.0, target);
    let depth = (eye.z - sim.position(target).z) - hit.1;
    let at = sim.position(target).with_z(eye.z - surface - depth);
    sim.app.world_mut().get_mut::<Transform>(target).unwrap().translation = at;
    sim.seconds(0.5);
    (sim, p, target)
}

fn health(sim: &Sim, e: Entity) -> f32 {
    sim.app.world().get::<Health>(e).unwrap().current
}

/// One attack (`secondary` or not) on the target: the damage dealt, in
/// points.
fn one_hit(sim: &mut Sim, p: Entity, target: Entity, secondary: bool) -> f32 {
    let n = swings(sim, p).len();
    let from = sim.tick();
    if secondary {
        tap(sim, p, |i, on| i.secondary = on);
    } else {
        tap(sim, p, |i, on| i.fire = on);
    }
    let t0 = mark(sim);
    until(sim, t0, 2.0, |s| swings(s, p).len() > n);
    sim.ticks(1);
    let hurt = &sim.app.world().resource::<Hurt>().0;
    let hits: Vec<f32> = hurt.iter().filter(|(t, e, _)| *t > from && *e == target).map(|h| h.2).collect();
    assert!(hits.len() <= 1, "one hit: {hits:?}");
    // Let the attack end before the next.
    sim.seconds(1.5);
    hits.first().copied().unwrap_or(0.0)
}

fn in_range(d: f32, lo: f32, hi: f32) -> bool {
    d >= lo - 1e-3 && d <= hi + 1e-3
}

#[test]
fn t20_knife_damage_front() {
    // Facing the attacker: yaw pi looks down +Z.
    let (mut sim, p, target) = knife_target(50.0 * UNIT, std::f32::consts::PI);
    let d = one_hit(&mut sim, p, target, false);
    assert!(in_range(d, 40.5, 49.5), "primary hit {d}");
    let d = one_hit(&mut sim, p, target, true);
    assert!(in_range(d, 67.5, 82.5), "secondary {d}");
}

#[test]
fn t21_knife_damage_behind() {
    // Facing away (down -Z, as the attacker).
    let (mut sim, p, target) = knife_target(50.0 * UNIT, 0.0);
    let d = one_hit(&mut sim, p, target, false);
    assert!(in_range(d, 52.65, 64.35), "primary hit {d}");
    let d = one_hit(&mut sim, p, target, true);
    assert!(in_range(d, 87.75, 107.25), "secondary {d}");
}

#[test]
fn t22_sector_edges() {
    let (f, r) = (Vec3::NEG_Z, Vec3::X);
    let d = |deg: f32| {
        let a = deg.to_radians();
        a.cos() * r + a.sin() * f
    };
    assert!(!inventory::behind(f, r, d(29.0)), "29: not behind");
    assert!(inventory::behind(f, r, d(31.0)), "31: behind");
    assert!(inventory::behind(f, r, d(179.0)), "179: behind");
    assert!(!inventory::behind(f, r, d(180.0)), "180: not behind");
    assert!(!inventory::behind(f, r, d(-30.0)), "-30: not behind");
}

#[test]
fn t23_knife_range() {
    for (surface, hits) in [(104.0, true), (106.0, false)] {
        let (mut sim, p, target) = knife_target(surface * UNIT, 0.0);
        let d = one_hit(&mut sim, p, target, false);
        assert_eq!(d > 0.0, hits, "surface at {surface} units: {d} damage");
    }
    // A wall at 60 units stops the ray before a target behind it.
    let (mut sim, p, target) = knife_target(90.0 * UNIT, 0.0);
    let eye = sim.position(p) + sim.state(p).eye_offset;
    sim.app.world_mut().spawn((
        avian3d::prelude::RigidBody::Static,
        avian3d::prelude::Collider::cuboid(2.0, 3.0, 0.02),
        Transform::from_translation(eye - Vec3::Z * (60.0 * UNIT + 0.01)),
    ));
    sim.ticks(2);
    let d = one_hit(&mut sim, p, target, false);
    assert_eq!(d, 0.0, "the wall took the hit");
}

#[test]
fn t24_swing_cut_by_a_switch() {
    let (mut sim, p) = knife_out(STOCK);
    let t0 = mark(&sim);
    tap(&mut sim, p, |i, on| i.fire = on);
    wait_until(&mut sim, t0, 0.25);
    key(&mut sim, p, 2);
    let pistol = carried(&sim, p, PISTOL);
    let t = first_shot(&mut sim, p, t0, pistol);
    let hits: Vec<f64> = swings(&sim, p).into_iter().map(|t| time_of(&sim, t0, t)).collect();
    assert_eq!(hits.len(), 1, "exactly one hit: {hits:?}");
    assert!(close(hits[0], 0.156, 3.0));
    assert!(close(t, 0.9, 2.0), "pistol ready at {t}");
}

#[test]
fn t25_sprint_knife() {
    let (mut sim, p) = knife_out(STOCK);
    let t0 = mark(&sim);
    sim.intent(p).fire = true;
    sim.ticks(1);
    sim.intent(p).fire = false;
    wait_until(&mut sim, t0, 0.1);
    sprint(&mut sim, p, true);
    let mut sprinting_hits = 0;
    let mut pose = None;
    while now(&sim, t0) < 1.0 {
        let n = swings(&sim, p).len();
        sim.ticks(1);
        if swings(&sim, p).len() > n && sim.state(p).sprinting {
            sprinting_hits += 1;
        }
        if pose.is_none() && sel(&sim, p).sprint_pose {
            pose = Some(now(&sim, t0));
        }
    }
    let hits: Vec<f64> = swings(&sim, p).into_iter().map(|t| time_of(&sim, t0, t)).collect();
    assert_eq!(hits.len(), 2, "both hits land: {hits:?}");
    assert!(close(hits[0], 0.156, 3.0) && close(hits[1], 0.403, 3.0), "{hits:?}");
    assert_eq!(sprinting_hits, 2, "both while sprinting");
    let pose = pose.expect("no sprint pose");
    assert!(close(pose, 0.651, 3.0), "sprint pose at {pose}");
}

// ---------------------------------------------------------------------------
// Drops and pickups (26-35)

fn kill(sim: &mut Sim, p: Entity) {
    let at = sim.position(p);
    sim.app.world_mut().write_message(Damage {
        target: p,
        attacker: None,
        amount: 10.0,
        point: at,
        dir: Vec3::NEG_Y,
        hitgroup: Hitgroup::Generic,
        kind: DamageKind::Generic,
        weapon: None,
        force: Vec3::ZERO,
    });
    sim.ticks(2);
    assert!(health(sim, p) <= 0.0);
}

fn items(sim: &mut Sim) -> Vec<(Entity, Dropped, String, Vec3)> {
    let w = sim.app.world_mut();
    w.query::<(Entity, &Dropped, &LooseItem, &Transform)>()
        .iter(w)
        .map(|(e, d, l, t)| (e, *d, l.0.clone(), t.translation))
        .collect()
}

/// The hull's bottom centre (the drop spot's height) and its width.
fn feet(sim: &Sim, p: Entity) -> (Vec3, f32) {
    let s: MovementState = sim.state(p);
    let (lo, hi) = if s.hull_max != s.hull_min {
        (s.hull_min, s.hull_max)
    } else {
        (Vec3::new(-0.4, -0.9, -0.4), Vec3::new(0.4, 0.9, 0.4))
    };
    (sim.position(p) + Vec3::Y * lo.y, hi.x - lo.x)
}

#[test]
fn t26_death_drop() {
    let (mut sim, p) = setup(STOCK);
    let rifle = carried(&sim, p, RIFLE);
    set_ammo(&mut sim, rifle, 12, 72);
    key(&mut sim, p, 2);
    sim.seconds(1.0);
    let (foot, width) = feet(&sim, p);
    kill(&mut sim, p);
    let t0 = mark(&sim);
    let dropped = items(&mut sim);
    assert_eq!(dropped.len(), 1, "one item: {dropped:?}");
    let (_, d, name, at) = &dropped[0];
    assert_eq!(name, RIFLE);
    assert_eq!((d.total, d.clip), (72, 12));
    assert_eq!(d.weapon, rifle);
    assert!((at - foot).xz().length() <= width / 2.0, "at the left foot: {at} vs {foot}");
    wait_until(&mut sim, t0, 19.9);
    sim.ticks(1);
    assert_eq!(items(&mut sim).len(), 1, "still there");
    sim.seconds(0.2);
    assert!(items(&mut sim).is_empty(), "gone after 20 s");
}

#[test]
fn t27_death_drop_exemptions() {
    let (mut sim, p) = setup(STOCK);
    sim.app.insert_resource(CaDeathDrops(false));
    kill(&mut sim, p);
    assert!(items(&mut sim).is_empty(), "snowball: no item");

    let (mut sim, p) = setup(STOCK);
    let rifle = carried(&sim, p, RIFLE);
    sim.app.world_mut().get_mut::<CaItem>(rifle).unwrap().drop_on_death = false;
    kill(&mut sim, p);
    assert!(items(&mut sim).is_empty(), "drop on death off: no item");
}

/// An item of `weapon` lying `dist` metres ahead of `p`'s eye, at eye
/// height (looked at).
fn item_ahead(sim: &mut Sim, p: Entity, weapon: &str, dist: f32, total: u32, clip: u32) -> Entity {
    let eye = sim.position(p) + sim.state(p).eye_offset;
    let item = inventory::spawn_item(sim.app.world_mut(), weapon, eye - Vec3::Z * dist, total, clip).unwrap();
    sim.ticks(1);
    item
}

fn press_use(sim: &mut Sim, p: Entity) {
    tap(sim, p, |i, on| i.use_key = on);
    sim.ticks(1);
}

#[test]
fn t28_pickup_swap() {
    let (mut sim, p) = setup(STOCK);
    let a = carried(&sim, p, RIFLE);
    set_ammo(&mut sim, a, 5, 35);
    let index = inv(&sim, p).weapons.iter().position(|w| *w == a).unwrap();
    let item = item_ahead(&mut sim, p, RIFLE, 80.0 * UNIT, 60, 30);
    let b = sim.app.world().get::<Dropped>(item).unwrap().weapon;
    assert_eq!(sel(&sim, p).pickup_target, Some(item), "the prompt");
    let (foot, width) = feet(&sim, p);
    let t0 = mark(&sim);
    sim.intent(p).use_key = true;
    sim.ticks(1);
    sim.intent(p).use_key = false;
    let dropped = items(&mut sim);
    assert_eq!(dropped.len(), 1, "A dropped, B removed: {dropped:?}");
    let (_, d, _, at) = &dropped[0];
    assert_eq!(d.weapon, a);
    assert_eq!((d.total, d.clip), (35, 5));
    let off = at - foot;
    assert!(off.x.abs() <= width && off.z.abs() <= width, "near the left foot: {off}");
    assert_eq!(inv(&sim, p).weapons[index], b, "B in A's equip index");
    assert_eq!(ammo(&sim, b), (30, 60));
    assert_eq!(active(&sim, p), Some(b), "B drawn");
    let t = first_shot(&mut sim, p, t0, b);
    assert!(close(t, 1.2, 2.0), "ready after B's draw: {t}");
}

#[test]
fn t29_pickup_while_holding_a_pistol() {
    let (mut sim, p) = setup(STOCK);
    let a = carried(&sim, p, RIFLE);
    set_ammo(&mut sim, a, 5, 35);
    key(&mut sim, p, 2);
    sim.seconds(1.0);
    let pistol = carried(&sim, p, PISTOL);
    let pistol_ammo = ammo(&sim, pistol);
    let item = item_ahead(&mut sim, p, RIFLE, 80.0 * UNIT, 60, 30);
    let b = sim.app.world().get::<Dropped>(item).unwrap().weapon;
    press_use(&mut sim, p);
    let dropped = items(&mut sim);
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0].1.weapon, a, "A dropped");
    assert_eq!(active(&sim, p), Some(b), "B drawn");
    assert_eq!(ammo(&sim, pistol), pistol_ammo);
}

#[test]
fn t30_pickup_out_of_range() {
    let (mut sim, p) = setup(STOCK);
    // Its surface 140 units away, its centre 150 (> 135).
    let item = item_ahead(&mut sim, p, RIFLE, 150.0 * UNIT, 60, 30);
    assert_eq!(sel(&sim, p).pickup_target, None, "no prompt");
    let rifle = carried(&sim, p, RIFLE);
    press_use(&mut sim, p);
    assert!(sim.app.world().get::<Dropped>(item).is_some(), "E did nothing");
    assert_eq!(active(&sim, p), Some(rifle));
}

#[test]
fn t31_pickup_clamp() {
    let (mut sim, p) = setup(STOCK);
    let item = item_ahead(&mut sim, p, RIFLE, 80.0 * UNIT, 150, 30);
    let b = sim.app.world().get::<Dropped>(item).unwrap().weapon;
    press_use(&mut sim, p);
    assert_eq!(ammo(&sim, b).1, 120);
}

#[test]
fn t32_pickup_at_total_0() {
    let (mut sim, p) = setup(STOCK);
    let item = item_ahead(&mut sim, p, RIFLE, 80.0 * UNIT, 0, 7);
    let b = sim.app.world().get::<Dropped>(item).unwrap().weapon;
    press_use(&mut sim, p);
    assert_eq!(active(&sim, p), Some(b));
    assert_eq!(ammo(&sim, b), (0, 0));
}

#[test]
fn t33_same_id_ammo_pickup() {
    let (mut sim, p) = setup(STOCK);
    let pistol = carried(&sim, p, PISTOL);
    set_ammo(&mut sim, pistol, 5, 20);
    let item = item_ahead(&mut sim, p, PISTOL, 80.0 * UNIT, 30, 15);
    let rifle = active(&sim, p).unwrap();
    press_use(&mut sim, p);
    assert_eq!(ammo(&sim, pistol), (15, 45), "total min(20 + 30, 45), clip refilled");
    assert!(sim.app.world().get_entity(item).is_err(), "the item is removed");
    assert_eq!(active(&sim, p), Some(rifle), "the pistol isn't drawn");
}

#[test]
fn t34_auto_switch_fallback() {
    // No primary at all; the pistol is chosen at spawn.
    let (mut sim, p) = setup(&[PISTOL, KNIFE, GRENADE]);
    assert_eq!(active_id(&sim, p), PISTOL);
    assert_eq!(sel(&sim, p).last_chosen[inventory::PRIMARY as usize], None);
    key(&mut sim, p, 4);
    sim.seconds(0.5);
    throw(&mut sim, p);
    let g = carried(&sim, p, GRENADE);
    let end = sim.app.world().get::<Throwable>(g).unwrap().redraw_at.unwrap();
    let t0 = mark(&sim);
    let t = until(&mut sim, t0, 2.0, |s| active(s, p).is_some_and(|a| id(s, a) == PISTOL));
    let when = sim.app.world().resource::<chimera_core::core::SimClock>().now;
    assert!((when - end).abs() <= 2.0 * dt(&sim), "drawn {t} s on, at the throw's end");
}

#[test]
fn t35_respawn() {
    let (mut sim, p) = setup(STOCK);
    let a = carried(&sim, p, RIFLE);
    set_ammo(&mut sim, a, 5, 35);
    let item = item_ahead(&mut sim, p, RIFLE, 80.0 * UNIT, 60, 30);
    let b = sim.app.world().get::<Dropped>(item).unwrap().weapon;
    press_use(&mut sim, p);
    assert_eq!(active(&sim, p), Some(b));
    kill(&mut sim, p);
    sim.app.world_mut().get_mut::<Health>(p).unwrap().current = 1.0;
    inventory::respawn_inventory(sim.app.world_mut(), p);
    sim.seconds(1.5);
    let weapons: Vec<&str> = inv(&sim, p).weapons.iter().map(|w| id(&sim, *w)).collect();
    assert_eq!(weapons, STOCK, "the loadout again");
    let rifle = active(&sim, p).unwrap();
    assert_eq!(id(&sim, rifle), RIFLE);
    assert_ne!(rifle, a);
    assert_ne!(rifle, b);
    assert_eq!(ammo(&sim, rifle), (30, 120), "full ammo");
    assert_eq!(sel(&sim, p).previous, None);
    q(&mut sim, p);
    sim.ticks(1);
    assert_eq!(active(&sim, p), Some(rifle), "Q does nothing");
}

#[test]
fn t36_rate_cap() {
    assert_eq!(inventory::played_rate(2.1), 1.0);
    assert_eq!(inventory::played_rate(1.9), 1.9);
    let mut sim = sim_with(STOCK);
    let doc = |rate: f32| {
        include_str!("../data/test_m16a3.toml")
            .replace("combat_arms:test_m16a3", &format!("combat_arms:rate_{}", (rate * 10.0) as u32))
            .replace("[ammo.\"combat_arms:test_556\"]\nmax = 90\n", "")
            .replace("sprint_out = 0.367", &format!("sprint_out = 0.367\nselect_rate = {rate}"))
    };
    let (fast, slow) = (doc(1.1), doc(0.9));
    sim.app.add_weapon_data(&[("rate_11.toml", &fast), ("rate_9.toml", &slow)]).unwrap();
    let p = player(&mut sim, greybox::SPAWNS[0]);
    for (weapon, draw) in [("combat_arms:rate_11", 1.2), ("combat_arms:rate_9", 1.2 / 1.9)] {
        let w = chimera_sim::weapon::give(sim.app.world_mut(), p, weapon).unwrap();
        let got = sim.app.world().get::<Weapon>(w).unwrap().draw_time;
        assert!((got - draw).abs() < 1e-4, "{weapon}: draw {got}, want {draw}");
    }
}
