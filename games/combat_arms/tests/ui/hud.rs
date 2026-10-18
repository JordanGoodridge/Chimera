//! specs/combat_arms/hud_crosshair.md's test cases, one test each
//! (`t01_` is test case 1): the behaviour (`ui::hud::model`) for the
//! numbers, and scenario tests on `CaSim` and the greybox with the HUD's
//! state (`CaHudStatePlugin`, no window) for what the world drives: the
//! crosshair from the held weapon, the hit indicator, damage taken, the
//! health block, the weapon panel, the kill feed, name tags. The rifle is
//! a fixture (tests/data/test_hud_rifle.toml: the spec's M4A1 at rest);
//! the knife is the game's M9.
//!
//! Tolerances: positions to 0.01 px (0.5 px where the spec gives "about"),
//! times to one HUD frame.

use bevy::prelude::*;
use chimera_core::core::{Damage, DamageKind, Died, Health, Hitgroup, LocalPlayer, RoundRestarts, Team};
use chimera_sim::weapon::{
    Inventory, Magazine, StartingWeapons, Weapon, WeaponEvent, WeaponEventKind, data::WeaponDataAppExt,
};
use chimera_world::{
    greybox::{self, GreyboxMapPlugin},
    harness::Sim,
};
use combat_arms::{
    game::{
        hud::CaHudData,
        inventory, movement,
    },
    harness::CaSim,
    ui::hud::{CaHud, CaHudStatePlugin, HudOptions, HudScreen, model, model::*, rapid_fire_count, spread_of},
};

const RIFLE: &str = "combat_arms:test_hud_rifle";
const KNIFE: &str = inventory::KNIFE;
const EPS: f32 = 0.01;

fn screen() -> Screen {
    Screen::default()
}

/// The M4A1's perturb fields (section B's example).
fn m4() -> Perturb {
    Perturb {
        min: 3,
        max: 100,
        zoomed_min: 2,
        zoomed_max: 30,
        duck: 2,
        move_duck: 3,
        ..default()
    }
}

/// The crosshair of shape `shape` (the weapon's own `weapon_shape`) with
/// limits (min, max), r, the seconds since the shot, an offset, on
/// `screen`.
fn cross(shape: u8, weapon_shape: u8, (min, max): (i32, i32), r: f32, since: Option<f32>, offset: i32, s: Screen) -> Vec<Element> {
    crosshair_elements(
        &CrosshairInputs {
            weapon_shape,
            shape,
            min,
            max,
            r,
            since_fire: since,
            since_alt: None,
            offset,
            wide: 4,
        },
        s,
    )
    .1
}

fn plus(min: i32, max: i32, r: f32, since: Option<f32>, offset: i32, s: Screen) -> Vec<PxRect> {
    cross(0, 0, (min, max), r, since, offset, s)
        .into_iter()
        .map(|e| match e {
            Element::Rect(r) => r,
            e => panic!("not a rect: {e:?}"),
        })
        .collect()
}

fn near(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

/// A horizontal arm: x range on row `row`.
fn h(r: &PxRect, x0: f32, x1: f32, row: f32) {
    assert!(
        near(r.min.x, x0, EPS) && near(r.max.x, x1, EPS) && near(r.min.y, row, EPS) && near(r.max.y, row + 1.0, EPS),
        "{r:?} != x [{x0}, {x1}) row {row}"
    );
}

/// A vertical arm: y range on column `col`.
fn v(r: &PxRect, y0: f32, y1: f32, col: f32) {
    assert!(
        near(r.min.y, y0, EPS) && near(r.max.y, y1, EPS) && near(r.min.x, col, EPS) && near(r.max.x, col + 1.0, EPS),
        "{r:?} != y [{y0}, {y1}) column {col}"
    );
}

fn close(a: Vec2, b: Vec2, tol: f32) {
    assert!(a.distance(b) <= tol, "{a} != {b}");
}

fn dot_at(e: &[Element], x: f32, y: f32) -> bool {
    e.iter().any(|e| matches!(e, Element::Rect(r) if r.min == Vec2::new(x, y) && r.max == Vec2::new(x + 1.0, y + 1.0)))
}

// ---------------------------------------------------------------------------
// Scenario setup

fn sim_with(loadout: &[&'static str]) -> Sim {
    let loadout = loadout.to_vec();
    Sim::with(CaSim, move |app| {
        app.add_plugins((GreyboxMapPlugin, CaHudStatePlugin));
        app.add_weapon_data(&[("test_hud_rifle.toml", include_str!("../data/test_hud_rifle.toml"))])
            .unwrap();
        app.insert_resource(StartingWeapons {
            team: Vec::new(),
            all: loadout,
        });
    })
}

/// The local player (team 1) with `loadout`, settled with its draw done.
fn setup(loadout: &[&'static str]) -> (Sim, Entity) {
    let mut sim = sim_with(loadout);
    let me = sim.spawn_character(greybox::SPAWNS[0], movement::ID);
    sim.app.world_mut().entity_mut(me).insert((LocalPlayer, Team(1), Name::new("Me")));
    sim.seconds(1.0);
    (sim, me)
}

fn hud(sim: &Sim) -> CaHud {
    sim.app.world().resource::<CaHud>().clone()
}

fn active(sim: &Sim, p: Entity) -> Entity {
    sim.app.world().get::<Inventory>(p).unwrap().active.unwrap()
}

/// Another character `ahead` m in front of `me` (yaw 0 looks down -Z).
fn other(sim: &mut Sim, me: Entity, ahead: f32, team: u8, name: &str) -> Entity {
    let at = sim.position(me) + Vec3::new(0.0, 0.0, -ahead);
    let e = sim.spawn_character(at, movement::ID);
    sim.app.world_mut().entity_mut(e).insert((Team(team), Name::new(name.to_string())));
    sim.ticks(10);
    e
}

/// Turn `me` to look level at `target`'s middle.
fn look_at(sim: &mut Sim, me: Entity, target: Entity) {
    let d = sim.position(target) - sim.position(me);
    let mut i = sim.intent(me);
    i.yaw = (-d.x).atan2(-d.z);
    i.pitch = -0.1;
    sim.ticks(2);
}

fn hurt(sim: &mut Sim, target: Entity, attacker: Option<Entity>, amount: f32, dir: Vec3) {
    sim.app.world_mut().write_message(Damage {
        target,
        attacker,
        amount,
        point: Vec3::ZERO,
        dir,
        hitgroup: Hitgroup::Chest,
        kind: DamageKind::Bullet,
        weapon: None,
        force: Vec3::ZERO,
    });
}

fn now_ms(sim: &Sim) -> f64 {
    sim.app.world().resource::<Time>().elapsed_secs_f64() * 1000.0
}

// ---------------------------------------------------------------------------
// Crosshair geometry (A1 to A5)

#[test]
fn t01_plus_at_rest() {
    let r = plus(3, 100, 0.0, None, 0, screen());
    assert_eq!(r.len(), 4, "four arms, no dot");
    h(&r[0], 500.0, 507.0, 383.0);
    h(&r[1], 516.0, 523.0, 383.0);
    v(&r[2], 372.0, 379.0, 511.0);
    v(&r[3], 388.0, 395.0, 511.0);
    assert_eq!(choose_colour(ColourInputs { weapon: Some(None), ..default() }, [0; 4]), [0, 255, 0, 255]);
    // In the game: the fixture's cone 3/1000 is the gap; green.
    let (sim, _) = setup(&[RIFLE]);
    let c = hud(&sim).crosshair;
    assert!(c.draws.lines);
    assert_eq!(c.colour, [0, 255, 0, 255]);
    assert_eq!(c.elements, cross(0, 0, (3, 3), 0.0, None, 0, screen()));
}

#[test]
fn t02_full_spread() {
    let r = plus(3, 100, 1.0, None, 0, screen());
    h(&r[0], 403.0, 410.0, 383.0);
    h(&r[1], 613.0, 620.0, 383.0);
    v(&r[2], 275.0, 282.0, 511.0);
    v(&r[3], 485.0, 492.0, 511.0);
}

#[test]
fn t03_half_spread() {
    let r = plus(3, 100, 0.5, None, 0, screen());
    h(&r[0], 451.5, 458.5, 383.0);
    h(&r[1], 564.5, 571.5, 383.0);
}

#[test]
fn t04_1920x1080() {
    let s = Screen::new(1920.0, 1080.0);
    let m = Measures::new(3, 100, 0.0, None, 0, s);
    assert!(near(m.g, 5.625, EPS) && near(m.l, 13.125, EPS));
    assert_eq!(m.centre, Vec2::new(960.0, 540.0));
    let r = plus(3, 100, 0.0, None, 0, s);
    h(&r[0], 939.25, 952.375, 539.0);
    h(&r[1], 966.625, 979.75, 539.0);
    v(&r[2], 519.25, 532.375, 959.0);
    v(&r[3], 546.625, 559.75, 959.0);
}

#[test]
fn t05_1024x600_not_squashed() {
    let r = plus(3, 100, 0.0, None, 0, Screen::new(1024.0, 600.0));
    h(&r[0], 500.0, 507.0, 299.0);
    v(&r[2], 288.0, 295.0, 511.0);
    v(&r[3], 304.0, 311.0, 511.0);
}

#[test]
fn t06_just_fired() {
    let m = Measures::new(3, 100, 0.0, Some(0.0), 0, screen());
    assert!(near(m.d, 7.0, EPS) && near(m.l, 13.0, EPS));
    let r = plus(3, 100, 0.0, Some(0.0), 0, screen());
    h(&r[0], 490.0, 503.0, 383.0);
    h(&r[1], 520.0, 533.0, 383.0);
    v(&r[2], 362.0, 375.0, 511.0);
    v(&r[3], 392.0, 405.0, 511.0);
    // In the game: a shot (Fired) kicks the crosshair at once.
    let (mut sim, me) = setup(&[RIFLE]);
    let w = active(&sim, me);
    sim.app.world_mut().write_message(WeaponEvent {
        owner: me,
        weapon: w,
        kind: WeaponEventKind::Fired {
            origin: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            seed: 0,
            spread: chimera_sim::weapon::SpreadShape::Disc {
                inaccuracy: 0.003,
                spread: 0.0,
            },
            mode: 0,
        },
        replay: false,
    });
    sim.ticks(1);
    let m = hud(&sim).crosshair.measures.unwrap();
    assert!(near(m.d, 7.0, EPS) && near(m.l, 13.0, EPS), "{m:?}");
    sim.seconds(0.2);
    let m = hud(&sim).crosshair.measures.unwrap();
    assert!(near(m.d, 3.0, EPS) && near(m.l, 7.0, EPS), "{m:?}");
}

#[test]
fn t07_kick_half_way() {
    let m = Measures::new(3, 100, 0.0, Some(0.05), 0, screen());
    assert!(near(m.d, 5.0, EPS) && near(m.l, 10.0, EPS));
    let r = plus(3, 100, 0.0, Some(0.05), 0, screen());
    h(&r[0], 495.0, 505.0, 383.0);
    h(&r[1], 518.0, 528.0, 383.0);
}

#[test]
fn t08_kick_over() {
    assert_eq!(plus(3, 100, 0.0, Some(0.1), 0, screen()), plus(3, 100, 0.0, None, 0, screen()));
}

fn zoomed() -> Stance {
    Stance {
        zoomed: true,
        ..default()
    }
}

#[test]
fn t09_zoomed() {
    assert_eq!(perturb_limits(&m4(), &PerturbMods::default(), zoomed()), (2, 30));
    let r = plus(2, 30, 0.0, None, 0, screen());
    h(&r[0], 501.0, 508.0, 383.0);
    h(&r[1], 515.0, 522.0, 383.0);
}

#[test]
fn t10_zoomed_half() {
    let (min, max) = perturb_limits(&m4(), &PerturbMods::default(), zoomed());
    assert!(near(gap(min, max, 0.5, 1.0), 16.0, EPS));
    let r = plus(min, max, 0.5, None, 0, screen());
    h(&r[0], 487.0, 494.0, 383.0);
    h(&r[1], 529.0, 536.0, 383.0);
}

#[test]
fn t11_crouched_still() {
    let s = Stance {
        crouch: true,
        ..default()
    };
    let (min, max) = perturb_limits(&m4(), &PerturbMods::default(), s);
    assert_eq!((min, max), (2, 100));
    assert_eq!(plus(min, max, 0.0, None, 0, screen()), plus(2, 30, 0.0, None, 0, screen()));
    // Only with both crouch fields (edge case).
    let one = Perturb { move_duck: 0, ..m4() };
    assert_eq!(perturb_limits(&one, &PerturbMods::default(), s), (3, 100));
}

#[test]
fn t12_crouched_moving() {
    let s = Stance {
        crouch: true,
        moving: true,
        ..default()
    };
    let (min, max) = perturb_limits(&m4(), &PerturbMods::default(), s);
    assert_eq!((min, max), (3, 100));
    assert_eq!(plus(min, max, 0.0, None, 0, screen()), plus(3, 100, 0.0, None, 0, screen()));
}

#[test]
fn t13_offset_and_colour() {
    let m = Measures::new(3, 100, 0.0, None, 45, screen());
    assert!(near(m.d, 48.0, EPS));
    let r = plus(3, 100, 0.0, None, 45, screen());
    h(&r[0], 455.0, 462.0, 383.0);
    h(&r[1], 561.0, 568.0, 383.0);
    let c = choose_colour(
        ColourInputs {
            weapon: Some(Some([110, 191, 228])),
            ..default()
        },
        DEFAULT_CROSSHAIR,
    );
    assert_eq!(c, [110, 191, 228, 255]);
}

#[test]
fn t14_offset_unscaled() {
    let s = Screen::new(2048.0, 1536.0);
    let m = Measures::new(3, 100, 0.0, None, 45, s);
    assert!(near(m.g, 6.0, EPS) && near(m.d, 51.0, EPS) && near(m.l, 14.0, EPS));
    let r = plus(3, 100, 0.0, None, 45, s);
    h(&r[0], 957.0, 971.0, 767.0);
}

#[test]
fn t15_plus_with_dot() {
    let e = cross(1, 1, (3, 100), 0.0, None, 0, screen());
    assert_eq!(e[..4], cross(0, 0, (3, 100), 0.0, None, 0, screen())[..]);
    assert_eq!(e.len(), 5);
    assert!(dot_at(&e, 511.0, 383.0));
}

#[test]
fn t16_no_top_arm() {
    let e = cross(2, 2, (3, 100), 0.0, None, 0, screen());
    let plain = cross(0, 0, (3, 100), 0.0, None, 0, screen());
    assert_eq!(e.len(), 4);
    assert!(e.contains(&plain[0]) && e.contains(&plain[1]) && e.contains(&plain[3]));
    assert!(!e.contains(&plain[2]), "no top arm");
    assert!(dot_at(&e, 511.0, 383.0));
}

#[test]
fn t17_x() {
    let e = cross(3, 3, (3, 100), 0.0, None, 0, screen());
    assert!(!e.iter().any(|e| matches!(e, Element::Rect(r) if r.size().x > 1.0 || r.size().y > 1.0)), "no bars");
    let lines: Vec<_> = e
        .iter()
        .filter_map(|e| match e {
            Element::Line(a, b) => Some((*a, *b)),
            _ => None,
        })
        .collect();
    assert_eq!(lines.len(), 4);
    let c = Vec2::new(512.0, 384.0);
    for (a, b) in &lines {
        assert!(near(a.distance(c), 5.0, EPS) && near(b.distance(c), 12.0, EPS));
    }
    close(lines[0].0, Vec2::new(508.46, 380.46), 0.01);
    close(lines[0].1, Vec2::new(503.51, 375.51), 0.01);
    assert!(dot_at(&e, 511.0, 383.0));
}

fn circles(e: &[Element]) -> Vec<f32> {
    e.iter()
        .filter_map(|e| match e {
            Element::Circle { centre, radius } => {
                assert_eq!(*centre, Vec2::new(512.0, 384.0));
                Some(*radius)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn t18_circle() {
    let e = cross(4, 4, (3, 100), 0.0, None, 0, screen());
    assert_eq!(circles(&e), vec![4.0]);
    let v = circle_vertices(Vec2::new(512.0, 384.0), 4.0);
    assert_eq!(v.len(), 32);
    close(v[0], Vec2::new(512.0, 380.0), 1e-4);
    assert!(dot_at(&e, 511.0, 383.0));
}

#[test]
fn t19_circle_just_fired() {
    let e = cross(4, 4, (3, 100), 0.0, Some(0.0), 0, screen());
    assert_eq!(circles(&e), vec![14.0]);
}

#[test]
fn t20_brackets() {
    let e = cross(6, 6, (3, 100), 0.0, None, 0, screen());
    let lines: Vec<_> = e
        .iter()
        .filter_map(|e| match e {
            Element::Line(a, b) => Some((*a, *b)),
            _ => None,
        })
        .collect();
    assert_eq!(lines.len(), 4);
    let ends = |apex: Vec2| -> Vec<Vec2> { lines.iter().filter(|(a, _)| *a == apex).map(|(_, b)| *b).collect() };
    let left = ends(Vec2::new(494.0, 384.0));
    let right = ends(Vec2::new(530.0, 384.0));
    assert_eq!((left.len(), right.len()), (2, 2));
    close(left[0], Vec2::new(502.49, 375.51), 0.01);
    close(left[1], Vec2::new(502.49, 392.49), 0.01);
    close(right[0], Vec2::new(521.51, 375.51), 0.01);
    close(right[1], Vec2::new(521.51, 392.49), 0.01);
    for (a, b) in &lines {
        assert!(near(a.distance(*b), 12.0, EPS));
    }
    assert!(dot_at(&e, 511.0, 383.0));
}

#[test]
fn t21_double_circle() {
    let e = cross(5, 5, (3, 100), 0.0, None, 0, screen());
    assert_eq!(circles(&e), vec![4.0, 16.0], "inner 4; outer m' 15, M' 150, D' 15");
    assert!(!e.iter().any(|e| matches!(e, Element::Rect(_))), "no dot");
}

#[test]
fn t22_item_circle() {
    let shape = choose_shape(0, Some(5), true);
    assert_eq!(shape, 5);
    let e = cross(shape, 0, (3, 100), 0.0, None, 0, screen());
    assert_eq!(circles(&e), vec![4.0], "one circle");
    assert_eq!(e.len(), 1, "no dot");
    // An item outside a tournament room's list is ignored.
    assert_eq!(choose_shape(0, Some(5), false), 0);
}

#[test]
fn t23_weapon_shape_wins() {
    assert_eq!(choose_shape(6, Some(3), true), 6);
    // In the game: the item set on the HUD.
    let (mut sim, _) = setup(&[RIFLE]);
    sim.app.world_mut().resource_mut::<HudOptions>().item_shape = Some(3);
    sim.ticks(1);
    assert_eq!(hud(&sim).crosshair.shape, 3, "the fixture has no shape of its own");
}

#[test]
fn t24_item_and_dot_sight_colours() {
    let yellow = [255, 255, 0, 255];
    let base = ColourInputs {
        item: Some(yellow),
        weapon: Some(None),
        ..default()
    };
    assert_eq!(choose_colour(base, DEFAULT_CROSSHAIR), yellow);
    let magenta = [255, 0, 255, 255];
    assert_eq!(
        choose_colour(
            ColourInputs {
                dot_sight: Some(magenta),
                ..base
            },
            DEFAULT_CROSSHAIR
        ),
        magenta
    );
    // A zero item colour counts as none; no weapon data keeps the last.
    assert_eq!(
        choose_colour(
            ColourInputs {
                item: Some([0, 0, 0, 255]),
                weapon: Some(None),
                ..default()
            },
            HIT_CROSSHAIR
        ),
        DEFAULT_CROSSHAIR
    );
    assert_eq!(choose_colour(ColourInputs::default(), HIT_CROSSHAIR), HIT_CROSSHAIR);
    // In the game.
    let (mut sim, _) = setup(&[RIFLE]);
    sim.app.world_mut().resource_mut::<HudOptions>().item_colour = Some(yellow);
    sim.ticks(1);
    assert_eq!(hud(&sim).crosshair.colour, yellow);
}

// ---------------------------------------------------------------------------
// Hit indicator (A10)

#[test]
fn t25_hit_indicator() {
    let mut h = HitIndicator::default();
    h.hit(0.0, true);
    for t in [0.0, 100.0, 499.0, 500.0] {
        assert!(h.frame(t), "{t}");
    }
    assert!(!h.frame(516.7));
    // In the game: damage dealt to an enemy turns the crosshair red for
    // 500 ms.
    let (mut sim, me) = setup(&[RIFLE]);
    let foe = other(&mut sim, me, 5.0, 2, "Foe");
    hurt(&mut sim, foe, Some(me), 0.01, Vec3::NEG_Z);
    sim.ticks(1);
    let t_hit = now_ms(&sim);
    assert_eq!(hud(&sim).crosshair.colour, HIT_CROSSHAIR);
    while now_ms(&sim) - t_hit <= 500.0 {
        assert_eq!(hud(&sim).crosshair.colour, HIT_CROSSHAIR, "at {}", now_ms(&sim) - t_hit);
        sim.ticks(1);
    }
    assert_eq!(hud(&sim).crosshair.colour, DEFAULT_CROSSHAIR, "green on the first frame past 500 ms");
}

#[test]
fn t26_second_hit_extends() {
    let mut h = HitIndicator::default();
    h.hit(0.0, true);
    assert!(h.frame(400.0));
    h.hit(400.0, true);
    assert!(h.frame(600.0) && h.frame(900.0));
    assert!(!h.frame(916.7));
}

#[test]
fn t27_teammate_hit() {
    let mut h = HitIndicator::default();
    h.hit(0.0, false);
    assert!(!h.frame(0.0));
    let (mut sim, me) = setup(&[RIFLE]);
    let mate = other(&mut sim, me, 5.0, 1, "Mate");
    hurt(&mut sim, mate, Some(me), 0.01, Vec3::NEG_Z);
    sim.ticks(1);
    assert_eq!(hud(&sim).crosshair.colour, DEFAULT_CROSSHAIR);
}

#[test]
fn t28_round_end_clears() {
    let mut h = HitIndicator::default();
    h.hit(0.0, true);
    h.reset();
    assert!(!h.frame(10.0));
    let (mut sim, me) = setup(&[RIFLE]);
    sim.app.world_mut().init_resource::<RoundRestarts>();
    sim.ticks(1);
    let foe = other(&mut sim, me, 5.0, 2, "Foe");
    hurt(&mut sim, foe, Some(me), 0.01, Vec3::NEG_Z);
    sim.ticks(1);
    assert_eq!(hud(&sim).crosshair.colour, HIT_CROSSHAIR);
    sim.app.world_mut().resource_mut::<RoundRestarts>().0 += 1;
    sim.ticks(1);
    assert_eq!(hud(&sim).crosshair.colour, DEFAULT_CROSSHAIR, "the next frame");
}

// ---------------------------------------------------------------------------
// What draws when (A7, A8, A9)

fn shown() -> model::Visibility {
    model::Visibility {
        ok: true,
        weapon: true,
        ..default()
    }
}

#[test]
fn t29_scope_overlay() {
    let d = draws(model::Visibility {
        scope_overlay: true,
        ..shown()
    });
    assert_eq!(
        d,
        Draws {
            damage: true,
            ..default()
        },
        "damage feedback only"
    );
    // In the game: a sniper scope's overlay hides the lines.
    let (mut sim, me) = setup(&[RIFLE]);
    let w = active(&sim, me);
    let mut modes = chimera_sim::weapon::AltModes::new(2, 0.3, false);
    modes.current = 1;
    sim.app.world_mut().entity_mut(w).insert((
        modes,
        chimera_sim::weapon::Zoom {
            fov: vec![20.0],
            max_speed: None,
            unzoom_after_shot: false,
            scope: true,
        },
    ));
    sim.ticks(2);
    assert!(sim.app.world().get::<chimera_sim::weapon::Zoomed>(me).is_some_and(|z| z.scope));
    let c = hud(&sim).crosshair;
    assert!(!c.draws.lines && c.elements.is_empty());
}

#[test]
fn t30_disabled_with_dot_sight() {
    let d = draws(model::Visibility {
        disabled: true,
        dot_sight: true,
        ..shown()
    });
    assert!(!d.lines);
    assert_eq!(d.dot_sight, 1);
    assert_eq!(dot_sight_rect(screen()), PxRect::new(448.0, 448.0 + 129.0, 320.0, 320.0 + 129.0));
    // In the game: `disable_crosshair` and `dot_sight` from the hook.
    let (mut sim, me) = setup(&[RIFLE]);
    let w = active(&sim, me);
    sim.app.world_mut().entity_mut(w).insert(CaHudData {
        disable_crosshair: true,
        dot_sight: Some("reticle".into()),
        ..default()
    });
    sim.ticks(1);
    let c = hud(&sim).crosshair;
    assert!(c.elements.is_empty());
    assert_eq!((c.draws.dot_sight, c.dot_sight), (1, Some(dot_sight_rect(screen()))));
}

#[test]
fn t31_dot_sight_twice() {
    let d = draws(model::Visibility {
        dot_sight: true,
        ..shown()
    });
    assert!(d.lines);
    assert_eq!(d.dot_sight, 2);
}

#[test]
fn t32_range_in() {
    assert_eq!(range_readout(Some(2000.0), 3500.0, 0.0), ("50.0".into(), [255, 198, 0, 255]));
    assert_eq!(RANGE_AT, Vec2::new(490.0, 400.0));
    // With the overlay up it still draws (A9); after the lines otherwise.
    let d = draws(model::Visibility {
        scope_overlay: true,
        range_readout: true,
        ..shown()
    });
    assert!(d.range_readout && !d.lines);
}

#[test]
fn t33_range_out() {
    assert_eq!(range_readout(Some(4000.0), 3500.0, 0.0), ("100.0".into(), [255, 0, 0, 255]));
    // An attachment's range bonus widens it: 3500 + trunc(3500 x 0.2).
    assert_eq!(range_readout(Some(4000.0), 3500.0, 0.2).1, RANGE_IN);
}

#[test]
fn t34_range_nothing() {
    assert_eq!(range_readout(None, 3500.0, 0.0).0, "");
}

// ---------------------------------------------------------------------------
// Damage taken (C)

const F: Vec2 = Vec2::new(0.0, -1.0);
const RT: Vec2 = Vec2::new(1.0, 0.0);

#[test]
fn t35_from_ahead() {
    let d = DamageFeedback::hit(0.0, 1.0, F, RT, -F, 0.0).unwrap();
    assert!(near(d.s, 3.0, 1e-4));
    let p = d.corners(0.0, screen());
    close(p[0], Vec2::new(681.0, 215.0), 0.5);
    close(p[1], Vec2::new(343.0, 215.0), 0.5);
    close(p[2], Vec2::new(343.0, 553.0), 0.5);
    close(p[3], Vec2::new(681.0, 553.0), 0.5);
    // In the game: damage travelling back along the view (a shooter
    // ahead), the same corners.
    let (mut sim, me) = setup(&[RIFLE]);
    hurt(&mut sim, me, None, 0.01, Vec3::Z);
    sim.ticks(1);
    let v = hud(&sim).damage.expect("damage shown");
    close(v.corners[0], Vec2::new(681.0, 215.0), 0.5);
    close(v.corners[1], Vec2::new(343.0, 215.0), 0.5);
}

#[test]
fn t36_from_behind() {
    let d = DamageFeedback::hit(0.0, 1.0, F, RT, F, 0.0).unwrap();
    assert!(near(d.s, 9.0, 1e-4));
    let p = d.corners(0.0, screen());
    close(p[0], Vec2::new(342.8, 552.8), 0.5);
    close(p[1], Vec2::new(680.8, 553.2), 0.5);
}

#[test]
fn t37_from_the_right() {
    let d = DamageFeedback::hit(0.0, 1.0, F, RT, -RT, 0.0).unwrap();
    assert!(near(d.s, 12.0, 1e-4));
    let p = d.corners(0.0, screen());
    close(p[0], Vec2::new(680.7, 553.3), 0.5);
    close(p[1], Vec2::new(681.3, 215.3), 0.5);
}

#[test]
fn t38_turning_keeps_pointing() {
    let d = DamageFeedback::hit(0.0, 1.0, F, RT, -F, 0.0).unwrap();
    assert!(near(d.phi(std::f32::consts::FRAC_PI_2), 0.75 * std::f32::consts::PI, 1e-4));
    let p = d.corners(std::f32::consts::FRAC_PI_2, screen());
    close(p[0], Vec2::new(343.0, 215.0), 0.5);
    close(p[1], Vec2::new(343.0, 553.0), 0.5);
    // In the game: turning right (our yaw falls) by pi/2 after the hit.
    let (mut sim, me) = setup(&[RIFLE]);
    hurt(&mut sim, me, None, 0.01, Vec3::Z);
    sim.ticks(1);
    sim.intent(me).yaw -= std::f32::consts::FRAC_PI_2;
    sim.ticks(1);
    let v = hud(&sim).damage.unwrap();
    close(v.corners[0], Vec2::new(343.0, 215.0), 0.5);
    close(v.corners[1], Vec2::new(343.0, 553.0), 0.5);
}

#[test]
fn t39_fade_per_frame() {
    let mut d = DamageFeedback::hit(0.0, 1.0, F, RT, -F, 0.0).unwrap();
    let frame = |n: u32| n as f64 * 1000.0 / 60.0;
    for n in 1..=30 {
        assert!(d.frame(frame(n)));
        assert_eq!(d.alpha, (255 - 2 * n) as u8);
    }
    assert_eq!(d.alpha, 195);
    for n in 31..60 {
        assert!(d.frame(frame(n)), "{n}");
    }
    assert!(!d.frame(frame(60)), "gone at t_dmg + 1000 ms");
    // In the game: gone on the first frame 1000 ms after.
    let (mut sim, me) = setup(&[RIFLE]);
    hurt(&mut sim, me, None, 0.01, Vec3::Z);
    sim.ticks(1);
    let t = now_ms(&sim);
    assert_eq!(hud(&sim).damage.unwrap().alpha, 255);
    sim.ticks(1);
    assert_eq!(hud(&sim).damage.unwrap().alpha, 253);
    while now_ms(&sim) - t < 1000.0 {
        assert!(hud(&sim).damage.is_some());
        sim.ticks(1);
    }
    assert!(hud(&sim).damage.is_none());
}

#[test]
fn t40_absorbed_hit() {
    assert!(DamageFeedback::hit(0.0, 0.0, F, RT, -F, 0.0).is_none());
    let (mut sim, me) = setup(&[RIFLE]);
    hurt(&mut sim, me, None, 0.0, Vec3::Z);
    sim.ticks(1);
    assert!(hud(&sim).damage.is_none());
}

#[test]
fn t41_overlay_rect() {
    assert_eq!(hit_overlay_rect(Screen::new(1920.0, 1080.0)), PxRect::new(0.0, 1920.0, 0.0, 1440.0));
}

// ---------------------------------------------------------------------------
// Health, armour, stamina (D2, D3)

fn set_health(sim: &mut Sim, p: Entity, hp: f32) {
    sim.app.world_mut().get_mut::<Health>(p).unwrap().current = hp / 100.0;
    sim.ticks(1);
}

#[test]
fn t42_low_health() {
    assert_eq!(hp_shown(30.0, 100.0, 100.0), 30);
    assert!(low_health(30.0, 100.0, false));
    let c = health_colours(true);
    assert_eq!((c.values, c.hp_ap_labels, c.silhouette), (LOW_HEALTH, LOW_HEALTH, Some(LOW_HEALTH)));
    assert_eq!(c.other_labels, [216, 216, 216, 255]);
    let mut beat = Heartbeat::default();
    let beats: Vec<u32> = (0..=180).filter(|n| beat.frame(*n as f64 / 60.0, true)).collect();
    assert_eq!(beats, vec![0, 60, 120, 180], "at least every 1.0 s");
    // In the game.
    let (mut sim, me) = setup(&[RIFLE]);
    set_health(&mut sim, me, 30.0);
    let s = hud(&sim).status.unwrap();
    assert_eq!((s.hp.as_str(), s.low, s.colours.values), ("30", true, LOW_HEALTH));
    let before = hud(&sim).heartbeats;
    sim.seconds(2.05);
    assert_eq!(hud(&sim).heartbeats - before, 2);
}

#[test]
fn t43_not_low() {
    assert!(!low_health(31.0, 100.0, false));
    let c = health_colours(false);
    assert_eq!((c.values, c.hp_ap_labels, c.silhouette), (VALUE_WHITE, LABEL_GREY, None));
    let mut beat = Heartbeat::default();
    assert!(!beat.frame(0.0, false));
    let (mut sim, me) = setup(&[RIFLE]);
    set_health(&mut sim, me, 31.0);
    let s = hud(&sim).status.unwrap();
    assert_eq!((s.hp.as_str(), s.low), ("31", false));
    let before = hud(&sim).heartbeats;
    sim.seconds(1.5);
    assert_eq!(hud(&sim).heartbeats, before);
}

#[test]
fn t44_interface_max_health() {
    assert_eq!(hp_shown(1.0, 200.0, 100.0), 1);
    assert_eq!(hp_shown(199.0, 200.0, 100.0), 99);
}

#[test]
fn t45_zero_health() {
    assert_eq!(hp_shown(0.0, 100.0, 100.0), 0);
    assert!(low_health(0.0, 100.0, false));
    assert_eq!(hp_shown(-5.0, 100.0, 100.0), 0);
}

#[test]
fn t46_infected() {
    assert!(!low_health(30.0, 100.0, true));
}

#[test]
fn t47_armour() {
    assert_eq!(ap_shown(75.0), 75);
    assert_eq!(ap_shown(-3.0), 0);
    let (mut sim, me) = setup(&[RIFLE]);
    sim.app.world_mut().entity_mut(me).insert(chimera_sim::weapon::Armor {
        amount: 0.75,
        helmet: false,
    });
    sim.ticks(1);
    assert_eq!(hud(&sim).status.unwrap().ap, "75");
}

#[test]
fn t48_stamina() {
    assert_eq!(stamina_bar(50.0, 100.0), 60.0);
    assert_eq!(stamina_bar(50.0, 100.0) * Screen::new(1920.0, 1080.0).xr(), 112.5);
    assert_eq!(stamina_bar(100.0, 100.0) * Screen::new(1920.0, 1080.0).xr(), 225.0, "the full bar at 1920");
    assert_eq!(stamina_bar(0.0, 100.0), 0.0);
    assert_eq!(stamina_bar(100.0, 100.0), 120.0);
    // In the game: a rested player's full gauge.
    let (sim, _) = setup(&[RIFLE]);
    assert_eq!(hud(&sim).status.unwrap().stamina_bar, 120.0);
}

// ---------------------------------------------------------------------------
// The weapon panel (D4)

fn set_ammo(sim: &mut Sim, w: Entity, clip: u32, reserve: u32) {
    let mut m = sim.app.world_mut().get_mut::<Magazine>(w).unwrap();
    m.clip = clip;
    m.reserve = reserve;
    sim.ticks(1);
}

#[test]
fn t49_ammo_text() {
    let (mut sim, me) = setup(&[RIFLE]);
    let w = active(&sim, me);
    set_ammo(&mut sim, w, 7, 90);
    let p = hud(&sim).weapon.unwrap();
    assert_eq!(p.name, "M4A1");
    assert_eq!(p.ammo, Some(("007".into(), "090".into())));
    assert_eq!((panel::CLIP_AT, panel::SLASH_AT, panel::RESERVE_AT), (
        Vec2::new(910.0, 742.0),
        Vec2::new(962.0, 742.0),
        Vec2::new(974.0, 741.0)
    ));
    assert_eq!((panel::CLIP, panel::RESERVE), ([240, 240, 240, 255], [180, 180, 180, 255]));
}

#[test]
fn t50_digits() {
    assert_eq!((three_digits(30), three_digits(1200), three_digits(7)), ("030".into(), "1200".into(), "007".into()));
}

#[test]
fn t51_empty_no_warning() {
    let (mut sim, me) = setup(&[RIFLE]);
    let w = active(&sim, me);
    set_ammo(&mut sim, w, 0, 0);
    assert_eq!(hud(&sim).weapon.unwrap().ammo, Some(("000".into(), "000".into())));
}

#[test]
fn t52_knife() {
    let (sim, me) = setup(&[KNIFE]);
    assert_eq!(sim.app.world().get::<Weapon>(active(&sim, me)).unwrap().id, KNIFE);
    let p = hud(&sim).weapon.expect("the name stays");
    assert!(!p.name.is_empty());
    assert_eq!((p.ammo, p.icon), (None, None));
}

/// As corrected (case 77): only the mission bomb and a weapon in no order
/// list hide the panel.
#[test]
fn t53_not_a_gun() {
    let p = PanelInputs {
        name: "C4".into(),
        class: 0,
        ..default()
    };
    assert_eq!(weapon_panel(&p, true), None);
    let bomb = PanelInputs {
        class: 6,
        bomb: true,
        ..p.clone()
    };
    assert_eq!(weapon_panel(&bomb, true), None);
    // The bomb flag hides a gun's panel too.
    let (mut sim, me) = setup(&[RIFLE]);
    let w = active(&sim, me);
    sim.app.world_mut().entity_mut(w).insert(CaHudData {
        bomb: true,
        ..default()
    });
    sim.ticks(1);
    assert_eq!(hud(&sim).weapon, None);
}

/// As corrected (case 76): a sniper rifle is a primary and gets an icon
/// (single shot).
#[test]
fn t54_fire_mode_icon() {
    assert_eq!(fire_icon(1, None, 0, false), Some(FireIcon::Auto));
    assert_eq!(fire_icon(1, None, 1, false), Some(FireIcon::Single));
    assert_eq!(fire_icon(1, Some(2), 0, false), Some(FireIcon::Burst2));
    assert_eq!(fire_icon(1, Some(3), 0, false), Some(FireIcon::Burst3));
    assert_eq!(fire_icon(1, None, 1, false), Some(FireIcon::Single), "sniper rifle");
    assert_eq!(rapid_fire_count(None), 1);
    assert_eq!(panel::ICON, (Vec2::new(926.0, 731.0), Vec2::new(128.0, 16.0)));
    let (sim, _) = setup(&[RIFLE]);
    assert_eq!(hud(&sim).weapon.unwrap().icon, Some(FireIcon::Auto));
}

#[test]
fn t55_name_box() {
    let (at, size) = panel::NAME_BOX;
    assert_eq!(at.x + size.x, 1019.0, "right edge");
    assert_eq!(at.y + size.y / 2.0, 721.0, "centred on y");
}

// ---------------------------------------------------------------------------
// Kill feed (D5)

fn kill(killer: &str, victim: &str) -> Kill {
    Kill {
        killer: Some((killer.into(), true)),
        victim: (victim.into(), false),
        icon: KillIcon::Weapon(Some("m4a1".into())),
        badges: Vec::new(),
    }
}

#[test]
fn t56_feed_expiry() {
    let mut f = KillFeed::default();
    for (t, v) in [(0.0, "A"), (1000.0, "B"), (2000.0, "C")] {
        f.push(t, kill("K", v));
    }
    let ys: Vec<f32> = (0..f.lines.len()).map(KillFeed::line_y).collect();
    assert_eq!(ys, vec![45.0, 65.0, 85.0]);
    f.frame(30000.0 + 1000.0 / 60.0);
    let left: Vec<&str> = f.lines.iter().map(|(_, k)| k.victim.0.as_str()).collect();
    assert_eq!(left, vec!["B", "C"], "A gone, B at 45, C at 65");
    // In the game: Died messages become lines.
    let (mut sim, me) = setup(&[RIFLE]);
    let foe = other(&mut sim, me, 5.0, 2, "Foe");
    let damage = Damage {
        target: foe,
        attacker: Some(me),
        amount: 1.0,
        point: Vec3::ZERO,
        dir: Vec3::NEG_Z,
        hitgroup: Hitgroup::Head,
        kind: DamageKind::Bullet,
        weapon: None,
        force: Vec3::ZERO,
    };
    sim.app.world_mut().write_message(Died {
        entity: foe,
        attacker: Some(me),
        damage,
    });
    sim.ticks(1);
    let feed = hud(&sim).feed;
    assert_eq!(feed.len(), 1);
    let k = &feed[0].kill;
    assert_eq!(feed[0].y, 45.0);
    assert_eq!(k.killer, Some(("Me".into(), true)));
    assert_eq!(k.victim, ("Foe".into(), false));
    assert_eq!(k.icon, KillIcon::Weapon(Some(RIFLE.into())));
    assert_eq!(k.badges, vec![Badge::Headshot]);
    sim.seconds(30.05);
    assert!(hud(&sim).feed.is_empty());
}

#[test]
fn t57_six_kills() {
    let mut f = KillFeed::default();
    for i in 0..6 {
        f.push(i as f64 * 100.0, kill("K", &format!("V{i}")));
    }
    assert_eq!(f.lines.len(), 5);
    assert_eq!(f.lines[0].1.victim.0, "V1", "the first kill is gone");
    assert_eq!(f.lines[4].1.victim.0, "V5");
    assert_eq!(KillFeed::line_y(4), 125.0);
}

fn width(s: &str) -> f32 {
    10.0 * s.len() as f32
}

#[test]
fn t58_teammate_headshot() {
    let k = Kill {
        badges: vec![Badge::Headshot],
        ..kill("Mate", "Foe")
    };
    let items = feed_line(&k, width);
    assert_eq!(
        items,
        vec![
            FeedItem::Text {
                x: 5.0,
                text: "Mate".into(),
                colour: [124, 190, 221, 150]
            },
            FeedItem::Icon {
                x: 45.0,
                icon: KillIcon::Weapon(Some("m4a1".into()))
            },
            FeedItem::Badge {
                x: 109.0,
                badge: Badge::Headshot
            },
            FeedItem::Text {
                x: 141.0,
                text: "Foe".into(),
                colour: [218, 115, 115, 150]
            },
        ]
    );
}

#[test]
fn t59_badges_overwrite() {
    let k = Kill {
        badges: vec![Badge::Revenge, Badge::Headshot, Badge::Groin],
        ..kill("Mate", "Foe")
    };
    let items = feed_line(&k, width);
    let badges: Vec<_> = items
        .iter()
        .filter_map(|i| match i {
            FeedItem::Badge { x, badge } => Some((*x, *badge)),
            _ => None,
        })
        .collect();
    assert_eq!(badges, vec![(109.0, Badge::Revenge), (141.0, Badge::Groin)]);
    let icon_end = 45.0 + FEED_ICON_WIDTH;
    let victim_x = items
        .iter()
        .find_map(|i| match i {
            FeedItem::Text { x, text, .. } if text == "Foe" => Some(*x),
            _ => None,
        })
        .unwrap();
    assert_eq!(victim_x - icon_end, 96.0);
    // No killer: the victim's name in both slots, in the victim's colour.
    let world = Kill {
        killer: None,
        icon: KillIcon::Suicide,
        ..kill("", "Foe")
    };
    let items = feed_line(&world, width);
    assert!(matches!(&items[0], FeedItem::Text { text, colour, .. } if text == "Foe" && *colour == feed_colour(false)));
}

// ---------------------------------------------------------------------------
// Score panel and timer (D6, D7)

#[test]
fn t60_tdm_panel() {
    let ([alpha, bravo], highlight) = score_panel([25, 0], 100, Some(0));
    assert_eq!((alpha.score.as_str(), alpha.colour, alpha.bar), ("25", [223, 31, 20, 255], 38.0));
    assert_eq!((bravo.score.as_str(), bravo.colour, bravo.bar), ("0", [170, 170, 170, 255], 0.0));
    assert_eq!(highlight, Some(Vec2::new(344.0, 18.0)));
}

#[test]
fn t61_full_bar() {
    assert_eq!(score_bar(120, 100), 155.0);
    assert_eq!(score_bar(5, 0), 0.0);
}

#[test]
fn t62_round_timer() {
    assert_eq!(round_timer(61900.0), "01 : 01");
    assert_eq!(round_timer(400.0), "00 : 00");
    assert_eq!(round_timer(600000.0), "10 : 00");
    assert_eq!(round_timer(-5.0), "00 : 00");
    assert_eq!(round_timer(6_000_000.0), "100 : 00");
}

#[test]
fn t63_tdm_no_timer() {
    assert_eq!(timer(false, 61900.0), None);
    assert_eq!(timer(true, 61900.0).as_deref(), Some("01 : 01"));
}

// ---------------------------------------------------------------------------
// Name tags (A11)

#[test]
fn t64_enemy_tag_while_aimed() {
    let t = name_tag(TagInputs {
        local_alive: true,
        alive: true,
        aimed: true,
        seen: true,
        ..default()
    })
    .unwrap();
    assert_eq!((t.name, t.hp_bar), ([215, 17, 18, 200], false));
    assert_eq!(name_tag(TagInputs { local_alive: true, alive: true, seen: true, ..default() }), None);
    // In the game.
    let (mut sim, me) = setup(&[RIFLE]);
    let foe = other(&mut sim, me, 5.0, 2, "Foe");
    look_at(&mut sim, me, foe);
    let h = hud(&sim);
    assert_eq!(h.aim_target, Some(foe));
    let tag = h.tags.iter().find(|t| t.entity == foe).expect("tag shown");
    assert_eq!((tag.name.as_str(), tag.tag.name, tag.tag.hp_bar), ("Foe", TAG_ENEMY, false));
    sim.intent(me).yaw += 1.2;
    sim.ticks(1);
    assert!(hud(&sim).tags.iter().all(|t| t.entity != foe), "gone once the ray leaves");
}

#[test]
fn t65_hit_flash() {
    let base = TagInputs {
        local_alive: true,
        alive: true,
        aimed: true,
        seen: true,
        ..default()
    };
    assert_eq!(name_tag(TagInputs { flashing: true, ..base }).unwrap().name, [250, 80, 0, 200]);
    let (mut sim, me) = setup(&[RIFLE]);
    let foe = other(&mut sim, me, 5.0, 2, "Foe");
    look_at(&mut sim, me, foe);
    hurt(&mut sim, foe, Some(me), 0.01, Vec3::NEG_Z);
    sim.ticks(1);
    let colour = |sim: &Sim| hud(sim).tags.iter().find(|t| t.entity == foe).unwrap().tag.name;
    assert_eq!(colour(&sim), TAG_FLASH);
    sim.seconds(0.25);
    assert_eq!(colour(&sim), TAG_ENEMY);
}

#[test]
fn t66_teammate_behind_a_wall() {
    let base = TagInputs {
        local_alive: true,
        teammate: true,
        alive: true,
        ..default()
    };
    let t = name_tag(base).unwrap();
    assert_eq!((t.opacity, t.hp_bar, t.name), (125, false, TAG_MATE));
    let t = name_tag(TagInputs {
        aimed: true,
        seen: true,
        ..base
    })
    .unwrap();
    assert_eq!((t.opacity, t.hp_bar), (255, true));
    // In the game: a wall between.
    let (mut sim, me) = setup(&[RIFLE]);
    let mate = other(&mut sim, me, 6.0, 1, "Mate");
    look_at(&mut sim, me, mate);
    let tag = |sim: &Sim| hud(sim).tags.into_iter().find(|t| t.entity == mate).unwrap().tag;
    assert_eq!((tag(&sim).opacity, tag(&sim).hp_bar), (255, true), "aimed with clear sight");
    let wall_at = sim.position(me) + Vec3::new(0.0, 0.5, -3.0);
    sim.app.world_mut().spawn((
        avian3d::prelude::RigidBody::Static,
        avian3d::prelude::Collider::cuboid(4.0, 4.0, 0.2),
        Transform::from_translation(wall_at),
    ));
    sim.ticks(3);
    assert_eq!((tag(&sim).opacity, tag(&sim).hp_bar), (125, false), "behind the wall, not aimed");
}

#[test]
fn t67_aiming_does_not_change_the_crosshair() {
    let (mut sim, me) = setup(&[RIFLE]);
    let foe = other(&mut sim, me, 5.0, 2, "Foe");
    look_at(&mut sim, me, foe);
    let on = hud(&sim);
    assert_eq!(on.aim_target, Some(foe));
    sim.intent(me).yaw += 1.2;
    sim.ticks(1);
    let off = hud(&sim);
    assert_eq!(off.aim_target, None);
    assert_eq!((on.crosshair.elements, on.crosshair.colour), (off.crosshair.elements, off.crosshair.colour));
}

// ---------------------------------------------------------------------------
// Spread (B): the gap from r (r's stepping is the firing part's,
// weapon_firing.md, and its tests)

#[test]
fn t68_one_shot() {
    let r = 3.0 * (1.0 / 80.0) * 1.3;
    assert!(near(r, 0.04875, 1e-6));
    assert!(near(gap(3, 100, r, 1.0), 7.73, 0.005));
}

#[test]
fn t69_full_auto_cap() {
    assert!(near(gap(3, 100, 0.30, 1.0), 32.1, 1e-4));
}

#[test]
fn t70_turning_in_place() {
    let (mut sim, me) = setup(&[RIFLE]);
    let before = hud(&sim).crosshair;
    for _ in 0..20 {
        sim.intent(me).yaw += 0.3;
        sim.ticks(1);
        assert_eq!(hud(&sim).crosshair, before);
    }
}

// ---------------------------------------------------------------------------
// The HUD's reading of the firing part's data

#[test]
fn spread_read_back_from_the_cone() {
    let limits: toml::Value = toml::from_str(
        "cone = { min = 3, max = 100, inc = 1.3, dec = 0.3 }\nzoomed_cone = { min = 2, max = 30, inc = 1.3, dec = 0.3 }",
    )
    .unwrap();
    let hud = CaHudData {
        duck_perturb: 2,
        move_duck_perturb: 3,
        ..default()
    };
    // The firing part's cone n = 3 + floor(97 r): r back.
    let (p, r) = spread_of(Some(&limits), &hud, 51.0, Stance::default());
    assert_eq!(p, m4());
    assert!(near(r, 48.0 / 97.0, 1e-6));
    // Zoomed: its own limits.
    let (_, r) = spread_of(Some(&limits), &hud, 16.0, zoomed());
    assert!(near(r, 0.5, 1e-6));
    // Without the data: the cone is the gap.
    let (p, r) = spread_of(None, &hud, 7.0, Stance::default());
    assert_eq!((p.min, p.max, r), (7, 7, 0.0));
    assert_eq!(screen().centre(), Vec2::new(512.0, 384.0));
    let _ = HudScreen::default();
}

// ---------------------------------------------------------------------------
// The follow-up answers (71-77)

#[test]
fn t71_hit_indicator_and_feed_on_the_ms_clock() {
    let mut h = HitIndicator::default();
    h.hit(10000.0, true);
    assert!(h.frame(10500.0), "500 is not more than 500: still red");
    assert!(!h.frame(10516.0), "the next frame: green");
    let mut f = KillFeed::default();
    f.push(0.0, kill("K", "A"));
    f.frame(30000.0);
    assert_eq!(f.lines.len(), 1, "shown at 30000");
    f.frame(30016.0);
    assert!(f.lines.is_empty(), "gone on the first frame after");
    // In the game the indicator runs on real time in whole ms: still red
    // 0.49 s on, green after 0.52 s.
    let (mut sim, me) = setup(&[RIFLE]);
    let foe = other(&mut sim, me, 5.0, 2, "Foe");
    hurt(&mut sim, foe, Some(me), 0.01, Vec3::NEG_Z);
    sim.ticks(1);
    let start = sim.app.world().resource::<Time<Real>>().elapsed_secs_f64();
    while sim.app.world().resource::<Time<Real>>().elapsed_secs_f64() - start < 0.49 {
        assert_eq!(hud(&sim).crosshair.colour, HIT_CROSSHAIR);
        sim.ticks(1);
    }
    sim.seconds(0.03);
    assert_eq!(hud(&sim).crosshair.colour, DEFAULT_CROSSHAIR);
}

#[test]
fn t72_kick_on_the_game_clock() {
    // Fired at game time 12.000, a frame at 12.050: D 5, l 10 (case 7).
    let m = Measures::new(3, 100, 0.0, Some((12.050f64 - 12.000) as f32), 0, screen());
    assert!(near(m.d, 5.0, EPS) && near(m.l, 10.0, EPS), "{m:?}");
}

#[test]
fn t73_gap_unrounded() {
    assert!(near(gap(3, 100, 0.04875, 1.0), 7.72875, 1e-4));
    assert!(near(gap(3, 100, 0.30, 1.0), 32.1, 1e-4));
    // In the game: the M4A1's meter at one shot; the gap unrounded, the
    // next shot's cone rounded down.
    use combat_arms::game::weapons::{CaSpread, FireMode};
    let (mut sim, me) = setup(&["combat_arms:m4a1"]);
    let w = active(&sim, me);
    for (r, cone) in [(0.04875, 7.0), (0.30, 32.0)] {
        let now = sim.app.world().resource::<chimera_core::core::SimClock>().now;
        {
            let mut s = sim.app.world_mut().get_mut::<CaSpread>(w).unwrap();
            s.fire = r;
            s.movement = 0.0;
            s.last_shot = Some(now);
        }
        sim.ticks(1);
        let c = hud(&sim).crosshair;
        assert!(near(c.r, r, 1e-6), "r {}", c.r);
        assert!(near(c.measures.unwrap().g, 3.0 + r * 97.0, 1e-3), "gap {:?}", c.measures);
        assert_eq!(sim.app.world().get::<CaSpread>(w).unwrap().cone(FireMode::Normal), cone);
    }
}

#[test]
fn t74_name_tag_anchor() {
    // No models yet, so no name socket: the hull centre, no offset.
    let (mut sim, me) = setup(&[RIFLE]);
    let foe = other(&mut sim, me, 5.0, 2, "Foe");
    look_at(&mut sim, me, foe);
    let tag = hud(&sim).tags.into_iter().find(|t| t.entity == foe).expect("tag shown");
    assert!((tag.anchor - sim.position(foe)).length() < 1e-5, "{} vs {}", tag.anchor, sim.position(foe));
}

#[test]
fn t75_no_distance_limit() {
    assert!(tag_in_front(20000.0));
    assert!(!tag_in_front(-1.0) && !tag_in_front(0.0), "behind the camera: hidden");
    // A teammate far away still has its tag (opacity by visibility).
    let (mut sim, me) = setup(&[RIFLE]);
    let far = 20000.0 * inventory::UNIT;
    let mate = other(&mut sim, me, far, 1, "Mate");
    assert!(hud(&sim).tags.iter().any(|t| t.entity == mate), "shown far away");
}

#[test]
fn t76_fire_mode_icons_from_the_data() {
    // M16A3: primary, RapidFireCount 0, burst toggled (3 rounds).
    assert_eq!(fire_icon(1, None, 0, false), Some(FireIcon::Auto));
    assert_eq!(fire_icon(1, Some(3), 0, false), Some(FireIcon::Burst3));
    // M24 sniper (primary) and M92FS (secondary): RapidFireCount 1.
    assert_eq!(fire_icon(1, None, 1, false), Some(FireIcon::Single));
    assert_eq!(fire_icon(2, None, 1, false), Some(FireIcon::Single));
    assert_eq!(fire_icon(1, None, 1, true), Some(FireIcon::Auto), "continuous fire on");
    // RapidFireCount 3, no burst: none; the M67 (throw class): none.
    assert_eq!(fire_icon(1, None, 3, false), None);
    assert_eq!(fire_icon(4, None, 1, false), None);
}

#[test]
fn t77_panel_by_inventory_class() {
    let panel = |class, bomb, infinite, clip, total| {
        weapon_panel(
            &PanelInputs {
                name: "W".into(),
                class,
                bomb,
                infinite,
                clip,
                total,
                rapid_fire_count: 1,
                ..default()
            },
            true,
        )
    };
    let m67 = panel(4, false, false, 1, 1).expect("the M67's panel");
    assert_eq!(m67.ammo, Some(("001".into(), "000".into())));
    assert_eq!(m67.icon, None);
    let knife = panel(3, false, true, 0, 0).expect("the knife's panel");
    assert_eq!((knife.ammo, knife.icon), (None, None));
    assert_eq!(panel(1, true, false, 30, 120), None, "the mission bomb");
    assert_eq!(panel(0, false, false, 30, 120), None, "in no order list");
}
