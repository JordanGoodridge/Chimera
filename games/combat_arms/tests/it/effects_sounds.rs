//! specs/combat_arms/effects_sounds.md's test cases (`t01_` is test case
//! 1) that need no install: the selection, attenuation and effect rules
//! (`game::effects::rules`, the tables of data/effects.toml and the
//! weapons' `combat_arms:effects` hooks), and the effects in play on the
//! greybox (sounds as `CaSound` messages, decals, tracers, casings,
//! effect requests). The numbers the install's effect file holds (lights,
//! stains, the smoke cloud: cases 21, 24, 32, 34, 35) are checked against
//! it in heavy::effects_sounds.

use bevy::prelude::*;
use chimera_core::core::{Damage, DamageKind, Health, Hitgroup, LocalPlayer};
use chimera_sim::weapon::{self, Inventory, WeaponEvent, WeaponEventKind};
use chimera_world::{
    greybox::GreyboxMapPlugin,
    harness::Sim,
    map::{MapData, MapMesh, sound::SurfaceGrid},
};
use combat_arms::{
    game::{
        TICK_INTERVAL,
        attachments::{CaMods, CaModTable, Part},
        data::weapon_set,
        effects::{
            self, CaFxRequest, CaFxSettings, CaShell, CaTracers, FxKind,
            data::{AmmoKind, CaFxTables, CaWeaponFx, EFFECTS},
            fx::FxJitter,
            rules::{
                self, CaMarks, Casing, CasingHit, CasingStep, DingQueue, Foot, Gait, HurtKind, HurtLimiter, Mark,
                MarkAdded, OwnSteps, StepInputs, TracerCounter, TracerPath,
            },
            sounds::CaSound,
        },
        inventory::UNIT,
        movement,
    },
    harness::CaSim,
};

fn close(a: f32, b: f32, tol: f32, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: {a}, expected {b}");
}

fn t() -> &'static CaFxTables {
    CaFxTables::built_in()
}

/// A weapon's effects hook.
fn wfx(id: &str) -> CaWeaponFx {
    weapon_set().get(id).unwrap().hook(EFFECTS).unwrap()
}

/// A weapon's fitted parts: `names` by their slots, with the weapon's
/// sound radii.
fn mods(names: &[&str], fire: i32, silenced: i32) -> CaMods {
    let table = CaModTable::built_in();
    let mut m = CaMods {
        fire_sound_radius: fire,
        silenced_radius: vec![silenced; 2],
        ..Default::default()
    };
    for n in names {
        let id = table.find(n).unwrap_or_else(|| panic!("no part {n}"));
        let record = table.get(id).unwrap().clone();
        let slot = record.slot().unwrap();
        m.parts[slot] = Some(Part {
            id,
            variant: Some(0),
            record,
            default: false,
        });
    }
    m
}

/// g(d) with the default inner radius.
fn g(d: f32, big_r: f32) -> f32 {
    rules::gain(d, rules::inner_radius(big_r), big_r, 100.0)
}

// ---------------------------------------------------------------------------
// 10. Gain and low-pass

#[test]
fn t01_gain_m4a1_shot() {
    close(rules::inner_radius(2000.0), 140.0, 1e-3, "r");
    for (d, want) in [(100.0, 1.0), (500.0, 0.9334), (1000.0, 0.7166), (1500.0, 0.3553), (1800.0, 0.0691), (1900.0, 0.0)] {
        close(g(d, 2000.0), want, 1e-4, &format!("d = {d}"));
    }
    close(rules::zero_point(140.0, 2000.0), 1865.26, 0.01, "zero point");
    // The others' fire sound of a stock M4A1: FireSnd at FireSndRadius.
    let m4 = wfx("combat_arms:m4a1");
    let s = effects::fire_sound(&m4, Some(&mods(&[], 2000, 1400)), false).unwrap();
    assert_eq!(s, ("guns_snd_ar/m4a1/fire.wav", 2000.0));
}

#[test]
fn t02_gain_silenced_m4a1() {
    let m4 = wfx("combat_arms:m4a1");
    let m = mods(&["Silencer_A_AR"], 2000, 1400);
    let (file, big_r) = effects::fire_sound(&m4, Some(&m), false).unwrap();
    assert_eq!((file, big_r), ("guns_snd_option/ar/silenced1.wav", 1400.0));
    close(rules::inner_radius(1400.0), 98.0, 1e-3, "r");
    for (d, want) in [(500.0, 0.8582), (1000.0, 0.4158), (1300.0, 0.0087)] {
        close(g(d, 1400.0), want, 1e-4, &format!("d = {d}"));
    }
    close(rules::zero_point(98.0, 1400.0), 1305.68, 0.01, "zero point");
    // Your own shot plays the same file, local.
    assert_eq!(effects::fire_sound(&m4, Some(&m), true).unwrap().0, file);
}

#[test]
fn t03_t04_footstep_gain_and_ghillie() {
    let metal = t().surface(20);
    assert_eq!(metal.right, ["chars_snd/footsteps/metal1a.wav", "chars_snd/footsteps/metal2a.wav"]);
    assert_eq!(rules::draw_slot(&metal.right, 2, 0.2), Some("chars_snd/footsteps/metal1a.wav"));
    assert_eq!(rules::draw_slot(&metal.right, 2, 0.7), Some("chars_snd/footsteps/metal2a.wav"));
    close(rules::inner_radius(rules::FOOTSTEP_RADIUS), 84.0, 1e-3, "r");
    for (d, want) in [(500.0, 0.8049), (1000.0, 0.2027), (1150.0, 0.0)] {
        close(g(d, 1200.0), want, 1e-4, &format!("d = {d}"));
    }
    // 4: the ghillie's volume 50.
    close(rules::gain(500.0, 84.0, 1200.0, 50.0), 0.4025, 1e-4, "ghillie");
}

#[test]
fn t05_server_hurt_sound() {
    let (r, big_r) = rules::server_radii(rules::inner_radius(500.0), 500.0);
    close(r, 17.0 / 255.0 * 500.0, 1e-4, "quantised r");
    for (d, want) in [(100.0, 0.9592), (300.0, 0.5918), (460.0, 0.0335)] {
        close(rules::gain(d, r, big_r, 100.0), want, 1e-4, &format!("d = {d}"));
    }
    assert!(!rules::server_sends(1001.0, 500.0), "beyond 2R: not sent");
    assert!(rules::server_sends(999.0, 500.0));
}

#[test]
fn t06_bullet_impact_on_metal() {
    let record = t().impact("BulletFmj").unwrap();
    let metal = t().surface(20);
    let files: Vec<&str> = (0..4)
        .map(|i| effects::impact_sound(record, metal, AmmoKind::Bullet, (i as f32 + 0.5) / 4.0).unwrap())
        .collect();
    assert_eq!(files, (1..=4).map(|i| format!("guns_snd_impacts/metal_bg/impact{i}.wav")).collect::<Vec<_>>());
    assert_eq!(record.sound_radius, 900.0);
    for (d, want) in [(200.0, 0.9486), (500.0, 0.6488), (800.0, 0.0921)] {
        close(g(d, 900.0), want, 1e-4, &format!("d = {d}"));
    }
}

#[test]
fn t07_frag_detonation_gain() {
    let record = t().impact("handgrenade").unwrap();
    assert_eq!((record.sound.as_str(), record.sound_radius), ("guns_snd_impacts/explosions/gren.wav", 1500.0));
    for (d, want) in [(500.0, 0.8772), (1000.0, 0.4918), (1400.0, 0.0)] {
        close(g(d, 1500.0), want, 1e-4, &format!("d = {d}"));
    }
}

#[test]
fn t08_low_pass() {
    for (d, want) in [(1000.0, 1.0), (1200.0, 1.0), (1350.0, 0.6173), (1500.0, 0.2929), (1650.0, 0.1), (1800.0, 0.1)] {
        close(rules::low_pass(d, 140.0, 2000.0), want, 1e-4, &format!("d = {d}"));
    }
    // Only while d < R.
    assert_eq!(rules::low_pass(2100.0, 140.0, 2000.0), 1.0);
    // Chimera's playback reads the same from the level.
    let level = rules::sound_level(2000.0, UNIT);
    let units = 1350.0 * UNIT / chimera_world::map::sound::METERS_PER_UNIT;
    close(chimera_world::map::sound::distance_cutoff(level, units), 0.6173, 1e-3, "playback");
    close(chimera_world::map::sound::distance_gain(level, units), g(1350.0, 2000.0), 1e-4, "gain");
}

#[test]
fn t09_impact_silence() {
    let record = t().impact("BulletFmj").unwrap();
    let mut rolls = effects::Rolls::default();
    let silent = |id: u16, rolls: &mut effects::Rolls| {
        (0..10_000)
            .filter(|_| effects::impact_sound(record, t().surface(id), AmmoKind::Bullet, rolls.next()).is_none())
            .count()
    };
    let unknown = silent(0, &mut rolls);
    assert!((2300..2700).contains(&unknown), "{unknown}");
    let snow = silent(71, &mut rolls);
    assert!((4800..5200).contains(&snow), "{snow}");
    assert_eq!(silent(20, &mut rolls), 0);
}

// ---------------------------------------------------------------------------
// 4. Tracers

fn cadence(f: u32, shots: u32) -> Vec<u32> {
    let mut c = TracerCounter::default();
    (1..=shots).filter(|_| c.ray(f)).collect()
}

#[test]
fn t10_tracer_cadence() {
    let m4 = wfx("combat_arms:m4a1");
    let f = effects::own_tracer_frequency(t(), &m4, Some(&mods(&[], 2000, 1400))).unwrap();
    assert_eq!(f, 4);
    assert_eq!(cadence(f, 8), [4, 8]);
    let look = effects::tracer_look(t(), &m4, None).unwrap();
    assert_eq!((look.width, look.length, look.velocity), (15.0, 600.0, 6000.0));
    // AK-47 (AR01) and M24 (SR01, F = 1).
    assert_eq!(cadence(t().tracer("AR01").unwrap().frequency, 9), [3, 6, 9]);
    assert_eq!(cadence(t().tracer("SR01").unwrap().frequency, 4), [1, 2, 3, 4]);
    assert_eq!(cadence(0, 3), [1, 2, 3], "F = 0: every shot");
}

#[test]
fn t11_tracer_timing() {
    let p = TracerPath::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -1500.0), 6000.0, 600.0).unwrap();
    close(p.life(), 0.25, 1e-6, "life");
    let (tail, head) = p.ends(0.1).unwrap();
    close(tail, 600.0, 1e-3, "tail at 0.1");
    close(head, 1200.0, 1e-3, "head at 0.1");
    let (tail, head) = p.ends(0.2).unwrap();
    close(tail, 1200.0, 1e-3, "tail at 0.2");
    close(head, 1500.0, 1e-3, "head clamped");
    assert!(p.ends(0.26).is_none());
    assert!(TracerPath::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -90.0), 6000.0, 600.0).is_none(), "90 u: none");
}

#[test]
fn t12_tracer_with_a_dot_sight() {
    let m4 = wfx("combat_arms:m4a1");
    let m = mods(&["DotSight_A_AR"], 2000, 1400);
    let own = effects::own_tracer_frequency(t(), &m4, Some(&m)).unwrap();
    assert_eq!(cadence(own, 9), [3, 6, 9], "the sight's AR01 cadence");
    assert_eq!(effects::tracer_look(t(), &m4, Some(&m)).unwrap(), t().tracer("AR02").unwrap(), "the ammo's look");
    // Others: the server's counter with the ammo's F.
    assert_eq!(cadence(t().tracer(&m4.tracer).unwrap().frequency, 8), [4, 8]);
}

#[test]
fn t13_tracer_with_an_a_silencer() {
    let m4 = wfx("combat_arms:m4a1");
    let m = mods(&["Silencer_A_AR"], 2000, 1400);
    assert_eq!(cadence(effects::own_tracer_frequency(t(), &m4, Some(&m)).unwrap(), 9), [3, 6, 9]);
    let look = effects::tracer_look(t(), &m4, Some(&m)).unwrap();
    assert_eq!(look, t().tracer("AR01").unwrap());
    assert_eq!((look.width, look.length), (25.0, 300.0));
    assert_eq!(cadence(t().tracer(&m4.tracer).unwrap().frequency, 8), [4, 8], "others: the ammo's F");
}

#[test]
fn t14_pistol_with_silencer() {
    let pistol = wfx("combat_arms:beretta_m92fs");
    let m = mods(&["Silencer_A_Pistol"], 1600, 1200);
    assert!(effects::tracer_look(t(), &pistol, Some(&m)).is_none(), "pistol ammo has no tracer");
    assert_eq!(effects::own_tracer_frequency(t(), &pistol, Some(&m)), Some(0));
}

// ---------------------------------------------------------------------------
// 6. Decals

fn settings() -> CaFxSettings {
    CaFxSettings::default()
}

fn hit(id: u16, u: f32) -> effects::Impact {
    effects::impact(
        t().impact("BulletFmj").unwrap(),
        t().surface(id),
        AmmoKind::Bullet,
        true,
        true,
        false,
        &settings(),
        [0.0, u],
    )
}

#[test]
fn t15_decal_size() {
    let w = |id: u16, u: f32| hit(id, u).mark.unwrap().1;
    let metal = hit(20, 0.0).mark.unwrap();
    assert_eq!(metal.0, "fx/i_mark/im_metal.spr");
    close(w(20, 0.0), 2.88, 1e-4, "metal min");
    close(w(20, 1.0), 3.36, 1e-4, "metal max");
    close(w(30, 0.0), 3.84, 1e-4, "wood min");
    close(w(30, 1.0), 4.32, 1e-4, "wood max");
    close(w(11, 0.0), 2.88, 1e-4, "cement min");
    close(w(11, 1.0), 3.84, 1e-4, "cement max");
    close(w(40, 0.0), 2.88, 1e-4, "glass min");
    close(w(40, 1.0), 3.36, 1e-4, "glass max");
    assert!(hit(40, 0.0).burst.is_none(), "glass: no burst");
    assert_eq!(hit(20, 0.0).burst.as_deref(), Some("Impact_Metal"));
}

#[test]
fn t16_decal_life() {
    assert_eq!(rules::mark_alpha(2.9), Some(1.0));
    close(rules::mark_alpha(4.5).unwrap(), 0.5, 1e-6, "4.5 s");
    assert_eq!(rules::mark_alpha(6.01), None);
}

fn mark(at: Vec3, age: f32) -> Mark {
    Mark {
        at,
        normal: Vec3::Y,
        angle: 0.0,
        width: 3.0,
        sprite: String::new(),
        grey: 0.5,
        age,
        serial: 0,
    }
}

#[test]
fn t17_decal_crowding() {
    let mut m = CaMarks::default();
    for i in 0..10 {
        m.add(mark(Vec3::new(i as f32 * 0.5, 0.0, 0.0), i as f32 * 0.1));
    }
    assert_eq!(m.add(mark(Vec3::ZERO, 0.0)), MarkAdded::Added);
    assert_eq!(m.marks.len(), 11);
    let target = Vec3::new(4.4, 0.0, 0.0);
    match m.add(mark(target, 0.0)) {
        MarkAdded::Moved(i) => {
            assert_eq!(m.marks[i].at, target);
            // The closest one (at 4.5 u), with its old age.
            close(m.marks[i].age, 0.9, 1e-6, "kept age");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(m.marks.len(), 11);
}

#[test]
fn t18_decal_cap() {
    let mut m = CaMarks::default();
    for i in 0..101 {
        m.add(mark(Vec3::new(i as f32 * 100.0, 0.0, 0.0), 0.0));
    }
    let first = m.marks[0].at;
    assert_eq!(m.add(mark(Vec3::new(0.0, 500.0, 0.0), 0.0)), MarkAdded::Replaced);
    assert_eq!(m.marks.len(), 101);
    assert!(m.marks.iter().all(|x| x.at != first), "the oldest went");
}

#[test]
fn t19_no_decal_on_doors() {
    let door = hit(100, 0.5);
    assert!(door.mark.is_none(), "ShowsMark 0");
    assert_eq!(door.burst.as_deref(), Some("Impact_Wood"));
    assert!(door.sound.unwrap().0.starts_with("guns_snd_impacts/wood/"));
}

#[test]
fn t20_no_decal_on_characters() {
    let body = effects::impact(
        t().impact("BulletFmj").unwrap(),
        t().surface(6),
        AmmoKind::Bullet,
        true,
        false,
        false,
        &settings(),
        [0.1, 0.5],
    );
    assert!(body.mark.is_none());
    assert_eq!(body.burst.as_deref(), Some("Impact_Body"));
    let (file, r) = body.sound.unwrap();
    assert!(file.starts_with("guns_snd_impacts/cloth/"), "{file}");
    assert_eq!(r, 900.0);
    // Two blobs splatting 50 % each: 1.0 expected.
    let mut rolls = effects::Rolls::default();
    let n = 20_000;
    let total: usize = (0..n).map(|_| rules::splats(&[50.0, 50.0], &[rolls.next(), rolls.next()])).sum();
    close(total as f32 / n as f32, 1.0, 0.03, "expected stains");
    // A head hit's group (case 21; its light and stains: heavy).
    let head = effects::impact(
        t().impact("BulletFmj").unwrap(),
        t().surface(4),
        AmmoKind::Bullet,
        true,
        false,
        false,
        &settings(),
        [0.1, 0.5],
    );
    assert_eq!(head.burst.as_deref(), Some("Impact_Head"));
}

// ---------------------------------------------------------------------------
// 2, 3. Muzzle flashes

#[test]
fn t22_others_muzzle_flash() {
    let cam = Vec3::ZERO;
    let fwd = Vec3::NEG_Z;
    let ahead = rules::gates(cam, fwd, Vec3::new(0.0, 0.0, -500.0), Vec3::new(0.0, 0.0, -10.0), true);
    assert!(ahead.fire_seen && !ahead.fire_far);
    let m4 = wfx("combat_arms:m4a1");
    assert_eq!(effects::hh_muzzle(t(), &m4, None), "M4_HH_Muzz");
    let behind = rules::gates(cam, fwd, Vec3::new(0.0, 0.0, 500.0), Vec3::new(0.0, 0.0, 600.0), true);
    assert!(!behind.fire_seen, "no flash behind you");
    close(g(500.0, 2000.0), 0.9334, 1e-4, "its sound");
    // A point at the camera counts as straight up.
    assert!(!rules::seen(cam, fwd, cam));
    assert!(rules::seen(cam, Vec3::Y, cam));
}

#[test]
fn t23_silencer_flash() {
    let m4 = wfx("combat_arms:m4a1");
    let c = mods(&["Silencer_C_AR"], 2000, 1400);
    let a = mods(&["Silencer_A_AR"], 2000, 1400);
    assert_eq!(effects::hh_muzzle(t(), &m4, Some(&c)), "Silenced01_HH_Muzz", "a group the file lacks: nothing");
    assert_eq!(effects::pv_muzzle(t(), "AK47_pv_muzz", Some(&c)), "Silenced01_PV_Muzz");
    assert_eq!(effects::hh_muzzle(t(), &m4, Some(&a)), "WEAPON_HH");
    assert_eq!(effects::hh_muzzle(t(), &m4, Some(&mods(&["Silencer_A_AR_Red"], 2000, 1400))), "Silenced01_HH_Muzz");
    for m in [&c, &a] {
        assert_eq!(
            effects::fire_sound(&m4, Some(m), false).unwrap(),
            ("guns_snd_option/ar/silenced1.wav", 1400.0)
        );
    }
}

#[test]
fn t24_own_muzzle() {
    let params: combat_arms::game::view_model::CaViewParams =
        weapon_set().get("combat_arms:m4a1").unwrap().hook(combat_arms::game::view_model::VIEW).unwrap();
    assert_eq!(effects::pv_muzzle(t(), &params.muzzle_fx, None), "AK47_pv_muzz");
    assert_eq!(rules::FLASH_TIME, 0.05);
    assert_eq!(t().fire(&wfx("combat_arms:m4a1").fire).unwrap().fx, "test", "the smoke puff's group");
}

// ---------------------------------------------------------------------------
// 8. Casings

/// A floor at height 0 (u), metal.
fn floor(from: Vec3, to: Vec3) -> Option<CasingHit> {
    (from.y > 0.0 && to.y <= 0.0).then(|| {
        let f = from.y / (from.y - to.y);
        CasingHit {
            at: from.lerp(to, f),
            normal: Vec3::Y,
            removes: false,
        }
    })
}

#[test]
fn t25_casing() {
    let v = rules::casing_velocity([0.0, 0.0, 0.0], false);
    assert_eq!(v, Vec3::new(50.0, 30.0, 20.0));
    let v = rules::casing_velocity([1.0, 1.0, 1.0], false);
    assert_eq!(v, Vec3::new(75.0, 75.0, 50.0));
    assert_eq!(rules::casing_velocity([0.5, 0.5, 0.5], true).x, -62.5, "left hand: to the left");
    let fire = t().fire("5.56mm").unwrap();
    assert!(fire.shells && fire.shell_model.ends_with("5.56mm.ltb"));
    let mut c = Casing {
        at: Vec3::new(0.0, 60.0, 0.0),
        velocity: Vec3::new(70.0, 50.0, 30.0),
        spin: Vec2::new(5.0, 5.0),
        angles: Vec2::ZERO,
        bounces: 0,
        age: 0.0,
        resting: false,
        in_water: false,
        first_person: false,
    };
    let dt = 1.0 / 60.0;
    let mut steps = Vec::new();
    while c.age < 3.0 {
        if c.age >= 0.5 - 1e-4 && c.age < 0.5 + dt {
            close(c.scale(), 2.0, 0.05, "grown to 2x at 0.5 s");
        }
        let s = c.step(dt, Vec3::ZERO, floor);
        steps.push(s);
        if s == CasingStep::Removed {
            break;
        }
    }
    assert_eq!(steps.iter().filter(|s| **s == CasingStep::FirstBounce).count(), 1, "one sound");
    assert_eq!(*steps.last().unwrap(), CasingStep::Removed);
    close(c.age, 3.0, dt, "removed at 3 s");
    // Its sound: the surface's, R 200.
    let metal = t().surface(20);
    assert_eq!((metal.shell.as_str(), metal.shell_radius), ("guns_snd_shells/metal.wav", 200.0));
    // Seen from 1100 u: no casing.
    let far = rules::gates(Vec3::ZERO, Vec3::NEG_Z, Vec3::new(0.0, 0.0, -1100.0), Vec3::ZERO, true);
    assert!(far.fire_seen && far.fire_far);
    let near = rules::gates(Vec3::ZERO, Vec3::NEG_Z, Vec3::new(0.0, 0.0, -300.0), Vec3::ZERO, true);
    assert!(near.fire_seen && !near.fire_far);
}

#[test]
fn t26_casing_rest() {
    let down = Vec3::new(0.0, -100.0, 0.0);
    assert!(rules::rests(down, 1.0 / 60.0));
    assert!(rules::rests(down, 1.0 / 144.0));
    close((225.0f32 * 144.0).sqrt(), 180.0, 1e-3, "144 fps threshold");
    close((225.0f32 * 30.0).sqrt(), 82.16, 0.01, "30 fps threshold");
    assert!(!rules::rests(down, 1.0 / 30.0));
    // At 30 fps the casing bounces, at most three times.
    let mut c = Casing {
        at: Vec3::new(0.0, 1.0, 0.0),
        velocity: down,
        spin: Vec2::ZERO,
        angles: Vec2::ZERO,
        bounces: 0,
        age: 0.0,
        resting: false,
        in_water: false,
        first_person: false,
    };
    assert_eq!(c.step(1.0 / 30.0, Vec3::ZERO, floor), CasingStep::FirstBounce);
    assert!(!c.resting);
    for _ in 0..200 {
        c.step(1.0 / 30.0, Vec3::ZERO, floor);
    }
    assert!(c.bounces <= 3);
    // At 60 fps it rests at once.
    let mut c2 = Casing {
        at: Vec3::new(0.0, 1.0, 0.0),
        velocity: down,
        ..c.clone()
    };
    c2.bounces = 0;
    c2.resting = false;
    c2.age = 0.0;
    c2.step(1.0 / 60.0, Vec3::ZERO, floor);
    assert!(c2.resting);
}

// ---------------------------------------------------------------------------
// 11. Fly-by, dings, hurt, footsteps

#[test]
fn t27_flyby() {
    let cam = Vec3::ZERO;
    // A line 400 u to the side, its impact 2000 u away.
    let from = Vec3::new(400.0, 0.0, 1000.0);
    let to = Vec3::new(400.0, 0.0, -1960.0);
    let near = rules::flyby(cam, from, to, 900.0).expect("fly-by");
    close(near.distance(cam), 400.0, 1e-3, "nearest point");
    let to_near = Vec3::new(400.0, 0.0, -450.0);
    assert!(rules::flyby(cam, from, to_near, 900.0).is_none(), "impact 600 u away: inside 0.75 x 900");
    // Quirk 6: the original's position near the origin.
    let q = rules::flyby_quirk_position(Vec3::new(5000.0, 0.0, 0.0), Vec3::new(5000.0, 0.0, 300.0));
    close(q.length(), 300.0 + 0.0, 1.01, "about 300 u from the origin");
}

#[test]
fn t28_hit_confirm_queue() {
    let mut q = DingQueue::default();
    let queued = (0..8).filter(|_| q.push()).count();
    assert_eq!(queued, 5);
    assert_eq!(q.drain(), 5);
    assert_eq!(q.drain(), 0);
    assert_eq!(t().sounds.ding, ["guns_snd_impacts/multiding1.wav", "guns_snd_impacts/multiding2.wav"]);
}

#[test]
fn t29_hurt_limit() {
    let mut l = HurtLimiter::default();
    let kinds = effects::hurt_kinds(DamageKind::Bullet, Hitgroup::Head, false);
    assert_eq!(kinds, [Some(HurtKind::Guns), Some(HurtKind::Head)]);
    assert!(l.try_start(HurtKind::Guns, 0.0, 1.0));
    assert!(!l.try_start(HurtKind::Guns, 0.0, 1.0), "the second player's: dropped");
    assert!(l.try_start(HurtKind::Head, 0.0, 1.0), "a separate kind");
    assert!(l.try_start(HurtKind::Guns, 1.0, 1.0), "free again once it ended");
    assert!(effects::hurt_files(t(), HurtKind::Guns)[0].contains("hurtpain_a_male"));
    assert_eq!(effects::hurt_files(t(), HurtKind::Head), ["chars_snd/male/headshot.wav"]);
    assert_eq!(effects::hurt_kinds(DamageKind::Bullet, Hitgroup::Chest, false), [Some(HurtKind::Guns), None]);
    assert_eq!(effects::hurt_kinds(DamageKind::Blast, Hitgroup::Stomach, false)[1], Some(HurtKind::Groin));
}

#[test]
fn t30_others_footsteps() {
    // Running: 50 and 350 ms of each 600 ms loop.
    assert_eq!(rules::footsteps_between(Gait::Run, 0.0, 0.049), 0);
    assert_eq!(rules::footsteps_between(Gait::Run, 0.0, 0.05), 1);
    assert_eq!(rules::footsteps_between(Gait::Run, 0.05, 0.35), 1);
    assert_eq!(rules::footsteps_between(Gait::Run, 0.0, 0.6), 2);
    assert_eq!(rules::footsteps_between(Gait::Run, 0.0, 6.0), 20, "3.33 steps a second");
    assert_eq!(rules::footsteps_between(Gait::Walk, 0.0, 6.0), 0);
    assert_eq!(rules::footsteps_between(Gait::Crouch, 0.0, 6.0), 0);
}

#[test]
fn t31_own_footsteps() {
    let dt = 1.0 / 600.0;
    let run = StepInputs {
        dt,
        moving: true,
        can_step: true,
        run: true,
        jumped: false,
        landed: false,
        alive: true,
    };
    let mut s = OwnSteps::default();
    let mut times = Vec::new();
    for i in 0..1200 {
        for f in s.step(&run) {
            times.push((i as f32 * dt, f));
        }
    }
    assert_eq!(times[0], (0.0, Foot::Right), "one step at once");
    let gaps: Vec<f32> = times[1..].windows(2).map(|w| w[1].0 - w[0].0).collect();
    assert!(gaps.iter().all(|g| (g - 0.357).abs() < 0.003), "{gaps:?}");
    close(1.0 / (2.0 * rules::STEP_PACE_RUN), 0.357, 0.001, "2.8 steps/s");
    // Holding the walk key: none.
    let mut s = OwnSteps::default();
    let walk = StepInputs { run: false, ..run };
    assert!((0..1200).all(|_| s.step(&walk).is_empty()));
}

// ---------------------------------------------------------------------------
// 9. Grenades

#[test]
fn t32_frag_on_stone() {
    let j = FxJitter {
        at: Vec3::ZERO,
        age: 0.0,
        life: 0.6,
        scale: vec![(0.0, 60.0), (1.0, 0.0)],
        inner: 200.0,
        outer: 800.0,
    };
    close(j.amplitude(500.0), 39.0, 1e-3, "60 x 0.65");
    assert_eq!(j.amplitude(800.0), 0.0);
    assert_eq!(j.amplitude(150.0), 60.0);
    let frag = t().impact("handgrenade").unwrap();
    let on = |id: u16, seen: bool| {
        effects::impact(frag, t().surface(id), AmmoKind::Thrown, seen, true, false, &settings(), [0.0, 0.0])
    };
    let stone = on(10, true);
    assert_eq!(stone.burst.as_deref(), Some("Grenade_Stone"));
    assert_eq!(stone.sound.as_ref().unwrap().0, "guns_snd_impacts/explosions/gren.wav");
    assert!(stone.mark.is_none(), "Mark empty: no scorch");
    // Mid-air: Air has no grenade group.
    let air = on(1, true);
    assert!(air.burst.is_none() && air.sound.is_some());
    assert_eq!(on(0, true).burst.as_deref(), Some("Grenade_Dirt"));
}

#[test]
fn t33_frag_behind_you() {
    let gates = rules::gates(Vec3::ZERO, Vec3::NEG_Z, Vec3::new(0.0, 0.0, 500.0), Vec3::new(0.0, 0.0, 500.0), true);
    assert!(!gates.impact_seen);
    let out = effects::impact(
        t().impact("handgrenade").unwrap(),
        t().surface(10),
        AmmoKind::Thrown,
        gates.impact_seen,
        true,
        false,
        &settings(),
        [0.0, 0.0],
    );
    assert!(out.burst.is_none(), "no explosion look");
    assert!(out.sound.is_some());
    close(g(500.0, 1500.0), 0.8772, 1e-4, "its sound");
}

#[test]
fn t34_smoke_cloud_drift() {
    // Friction 0.98 per second: v x 0.02^dt; a particle travels at most
    // v0 / ln 50.
    let (mut x, mut v) = (0.0f32, 1000.0f32);
    let dt = 1.0 / 1000.0;
    for _ in 0..20_000 {
        x += v * dt;
        v *= rules::friction_factor(0.98, dt);
    }
    close(x, 1000.0 / 50f32.ln(), 0.5, "horizontal travel");
    close(1.0 / 50f32.ln(), 0.2556, 1e-4, "factor");
    // Smoke and flash: their own groups always (area ammo).
    let smoke = t().impact("Smoke_Grenade").unwrap();
    let unseen = effects::impact(smoke, t().surface(1), AmmoKind::Thrown, false, true, false, &settings(), [0.0; 2]);
    assert_eq!(unseen.ammo_fx.as_deref(), Some("Smoke_Grenade_01"));
    let flash = t().impact("flashbang_exp").unwrap();
    let unseen = effects::impact(flash, t().surface(1), AmmoKind::Thrown, false, true, false, &settings(), [0.0; 2]);
    assert_eq!(unseen.ammo_fx.as_deref(), Some("Flashbang_Ini"));
    assert_eq!(unseen.sound.unwrap(), ("guns_snd_impacts/explosions/flash.wav".into(), 1500.0));
}

#[test]
fn t36_dry_fire_heard_far() {
    let m4 = wfx("combat_arms:m4a1");
    assert_eq!(m4.dry_fire_sound, "guns_snd_ar/m4a1/dryfire.wav");
    // Others hear it with the fire sound's radius (Quirk 13).
    close(g(1500.0, 2000.0), 0.3553, 1e-4, "gain");
}

#[test]
fn t37_reload_keys() {
    let m4 = wfx("combat_arms:m4a1");
    assert_eq!(m4.key_sound(1), Some(("guns_snd_ar/m4a1/reload1.wav", 300.0)));
    assert_eq!(m4.key_sound(2), Some(("guns_snd_ar/m4a1/reload2.wav", 300.0)));
    assert_eq!(m4.key_sound(3), Some(("guns_snd_ar/m4a1/reload3.wav", 300.0)), "also the select's key 3");
    assert_eq!(m4.key_sound(4), Some(("guns_snd_ar/m4a1/select.wav", 300.0)));
    assert_eq!(m4.key_sound(5), Some(("guns_snd_ar/m4a1/deselect.wav", 300.0)));
    close(rules::zero_point(21.0, 300.0), 280.0, 0.5, "heard to about 280 u");
}

#[test]
fn t38_grenade_bounce_sounds() {
    assert!(t().surface(0).grenade.is_empty(), "unknown: silent");
    let cement = t().surface(11);
    assert_eq!(
        (cement.grenade.as_str(), cement.grenade_radius),
        ("guns_snd_impacts/stone/impactgrenade.wav", 1000.0)
    );
}

#[test]
fn t39_melee_impact() {
    let melee = t().impact("Melee").unwrap();
    assert_eq!(melee.sound_radius, 153.0);
    let wood = t().surface(30);
    let mut rolls = effects::Rolls::default();
    let mut silent = 0;
    for _ in 0..10_000 {
        match effects::impact_sound(melee, wood, AmmoKind::Melee, rolls.next()) {
            None => silent += 1,
            Some(f) => assert!(f == "guns_snd_impacts/melee/wood/impact1.wav" || f == "guns_snd_impacts/melee/wood/impact2.wav"),
        }
    }
    assert!((4800..5200).contains(&silent), "{silent}");
    let knife = wfx("combat_arms:m9_knife");
    assert_eq!((knife.ammo_kind, knife.impact.as_str()), (AmmoKind::Melee, "Melee"));
    let hit = effects::impact(melee, wood, AmmoKind::Melee, true, true, false, &settings(), [0.9, 0.0]);
    assert!(hit.mark.is_none() && hit.burst.is_none(), "no decal, no surface effects");
}

// ---------------------------------------------------------------------------
// In play on the greybox

/// Where characters' feet stand on the greybox (m).
const GREYBOX_FLOOR: f32 = 0.0;

#[derive(Resource, Default)]
struct Heard(Vec<CaSound>);

#[derive(Resource, Default)]
struct Requested(Vec<CaFxRequest>);

fn sim() -> Sim {
    let mut sim = Sim::new(CaSim, GreyboxMapPlugin);
    sim.set_tick_interval(TICK_INTERVAL);
    sim.app.init_resource::<Heard>().init_resource::<Requested>();
    sim.app.add_systems(
        Update,
        (
            |mut r: MessageReader<CaSound>, mut h: ResMut<Heard>| h.0.extend(r.read().cloned()),
            |mut r: MessageReader<CaFxRequest>, mut h: ResMut<Requested>| h.0.extend(r.read().cloned()),
        )
            .after(effects::CaEffectsSet),
    );
    sim
}

fn player(sim: &mut Sim, at: Vec3, yaw: f32) -> Entity {
    let p = sim.spawn_character(at, movement::ID);
    sim.intent(p).yaw = yaw;
    sim.seconds(0.5);
    sim.app.world_mut().entity_mut(p).insert(Health { current: 1.0, max: 1.0 });
    p
}

fn arm(sim: &mut Sim, p: Entity, id: &str) -> Entity {
    let w = weapon::give(sim.app.world_mut(), p, id).unwrap();
    sim.seconds(1.5);
    assert_eq!(sim.app.world().get::<Inventory>(p).unwrap().active, Some(w));
    w
}

fn heard(sim: &mut Sim) -> Vec<CaSound> {
    std::mem::take(&mut sim.app.world_mut().resource_mut::<Heard>().0)
}

fn fire_once(sim: &mut Sim, p: Entity) {
    sim.intent(p).fire = true;
    sim.ticks(1);
    sim.intent(p).fire = false;
    sim.ticks(2);
}

/// The floor typed `ty` (the greybox has no surface types): a plane at
/// the greybox floor's height.
fn type_everything(sim: &mut Sim, ty: u16) {
    let world = sim.app.world_mut();
    let y = GREYBOX_FLOOR;
    let data = MapData {
        meshes: vec![MapMesh {
            positions: vec![[-500.0, y, -500.0], [-500.0, y, 500.0], [500.0, y, 500.0], [500.0, y, -500.0]],
            indices: vec![0, 1, 2, 0, 2, 3],
            surface: Some(ty.to_string()),
            ..Default::default()
        }],
        ..Default::default()
    };
    world.insert_resource(SurfaceGrid::new(&data));
}

#[test]
fn shots_sound_local_for_you_and_3d_for_others() {
    let mut sim = sim();
    let me = player(&mut sim, Vec3::new(20.0, 1.0, 20.0), 0.0);
    sim.app.world_mut().entity_mut(me).insert(LocalPlayer);
    let other = player(&mut sim, Vec3::new(24.0, 1.0, 20.0), 0.0);
    arm(&mut sim, me, "combat_arms:m4a1");
    arm(&mut sim, other, "combat_arms:m4a1");
    heard(&mut sim);
    fire_once(&mut sim, me);
    let mine = heard(&mut sim);
    assert!(mine.iter().any(|s| s.file == "guns_snd_ar/m4a1/fire.wav" && s.at.is_none()), "{mine:?}");
    fire_once(&mut sim, other);
    let theirs = heard(&mut sim);
    let fire = theirs
        .iter()
        .find(|s| s.file == "guns_snd_ar/m4a1/fire.wav")
        .expect("their fire sound");
    assert!(fire.at.is_some());
    assert_eq!(fire.radii, (140.0, 2000.0));
    assert_eq!((fire.source, fire.channel), (Some(other), Some(effects::sounds::CHANNEL_FIRE)));
}

#[test]
fn impacts_make_decals_bursts_and_sounds() {
    let mut sim = sim();
    let me = player(&mut sim, Vec3::new(20.0, 1.0, 20.0), 0.0);
    sim.app.world_mut().entity_mut(me).insert(LocalPlayer);
    arm(&mut sim, me, "combat_arms:m4a1");
    type_everything(&mut sim, 20);
    // Look down at the floor ahead.
    sim.intent(me).pitch = -0.6;
    sim.ticks(2);
    heard(&mut sim);
    sim.app.world_mut().resource_mut::<Requested>().0.clear();
    fire_once(&mut sim, me);
    let sounds = heard(&mut sim);
    assert!(
        sounds.iter().any(|s| s.file.starts_with("guns_snd_impacts/metal_bg/impact") && s.radii.1 == 900.0),
        "{sounds:?}"
    );
    let marks = sim.app.world().resource::<CaMarks>();
    assert_eq!(marks.marks.len(), 1);
    assert_eq!(marks.marks[0].sprite, "fx/i_mark/im_metal.spr");
    let req = std::mem::take(&mut sim.app.world_mut().resource_mut::<Requested>().0);
    assert!(req.iter().any(|r| r.group == "Impact_Metal" && r.kind == FxKind::Surface), "{req:?}");
    assert!(req.iter().any(|r| r.group == "test" && r.kind == FxKind::OwnFire));
    // Decals fade and go after 6 s.
    sim.seconds(6.5);
    assert!(sim.app.world().resource::<CaMarks>().marks.is_empty());
}

#[test]
fn tracers_every_fourth_m4a1_round_and_casings() {
    let mut sim = sim();
    let me = player(&mut sim, Vec3::new(20.0, 1.0, 20.0), 0.0);
    sim.app.world_mut().entity_mut(me).insert(LocalPlayer);
    arm(&mut sim, me, "combat_arms:m4a1");
    // Level, along the floor: impacts beyond 100 u.
    sim.intent(me).pitch = 0.0;
    let mut drawn = 0;
    let mut casings_seen = 0;
    for _ in 0..8 {
        let before = sim.app.world().resource::<CaTracers>().0.len();
        fire_once(&mut sim, me);
        let after = sim.app.world().resource::<CaTracers>().0.len();
        drawn += after.saturating_sub(before).min(1);
        casings_seen = casings_seen.max(sim.app.world_mut().query::<&CaShell>().iter(sim.app.world()).count());
        sim.seconds(0.4);
    }
    assert_eq!(drawn, 2, "shots 4 and 8");
    assert!(casings_seen >= 1);
    sim.seconds(3.5);
    assert_eq!(sim.app.world_mut().query::<&CaShell>().iter(sim.app.world()).count(), 0, "gone after 3 s");
}

#[test]
fn hits_ding_on_the_next_frame() {
    let mut sim = sim();
    let me = player(&mut sim, Vec3::new(20.0, 1.0, 20.0), 0.0);
    sim.app.world_mut().entity_mut(me).insert(LocalPlayer);
    let target = player(&mut sim, Vec3::new(30.0, 1.0, 20.0), 0.0);
    let w = arm(&mut sim, me, "combat_arms:m4a1");
    heard(&mut sim);
    let hit = |n| {
        (0..n)
            .map(|_| WeaponEvent {
                owner: me,
                weapon: w,
                kind: WeaponEventKind::Hit {
                    target,
                    amount: 0.1,
                    hitgroup: Hitgroup::Chest,
                },
                replay: false,
            })
            .collect::<Vec<_>>()
    };
    sim.app.world_mut().write_message_batch(hit(8));
    sim.app.update();
    let now: Vec<CaSound> = heard(&mut sim);
    assert!(now.iter().all(|s| !s.file.contains("multiding")), "not the same frame");
    sim.app.update();
    let dings = heard(&mut sim).into_iter().filter(|s| s.file.contains("multiding")).count();
    assert_eq!(dings, 5, "five slots");
}

#[test]
fn hurt_sounds_one_per_kind_across_the_server() {
    let mut sim = sim();
    let a = player(&mut sim, Vec3::new(20.0, 1.0, 20.0), 0.0);
    let b = player(&mut sim, Vec3::new(26.0, 1.0, 20.0), 0.0);
    heard(&mut sim);
    let hurt = |target, hitgroup| Damage {
        target,
        attacker: None,
        amount: 0.1,
        point: Vec3::new(20.0, 1.5, 20.0),
        dir: Vec3::Z,
        hitgroup,
        kind: DamageKind::Bullet,
        weapon: None,
        force: Vec3::ZERO,
    };
    sim.app.world_mut().write_message_batch([hurt(a, Hitgroup::Chest), hurt(b, Hitgroup::Head)]);
    sim.app.update();
    let s = heard(&mut sim);
    let pains = s.iter().filter(|s| s.file.contains("hurtpain_a")).count();
    assert_eq!(pains, 1, "{s:?}");
    assert!(s.iter().any(|s| s.file.ends_with("headshot.wav")), "a separate kind");
    assert!(s.iter().all(|s| s.server && s.radii.1 == 500.0));
}

#[test]
fn footsteps_by_surface_and_stance() {
    let mut sim = sim();
    let me = player(&mut sim, Vec3::new(20.0, 1.0, 20.0), 0.0);
    sim.app.world_mut().entity_mut(me).insert(LocalPlayer);
    let other = player(&mut sim, Vec3::new(28.0, 1.0, 20.0), 0.0);
    type_everything(&mut sim, 20);
    heard(&mut sim);
    sim.intent(me).move_axis = Vec2::Y;
    sim.intent(other).move_axis = Vec2::Y;
    sim.seconds(1.5);
    let s = heard(&mut sim);
    let mine = s.iter().filter(|s| s.at.is_none() && s.file.contains("footsteps/metal")).count();
    let theirs = s.iter().filter(|s| s.at.is_some() && s.file.contains("footsteps/metal")).count();
    assert!(mine >= 3, "{s:?}");
    assert!(theirs >= 3, "{s:?}");
    assert!(s.iter().filter(|s| s.at.is_some()).all(|s| s.radii.1 == 1200.0));
    // Walking: silent for everyone.
    sim.intent(me).walk = true;
    sim.intent(other).walk = true;
    sim.seconds(0.3);
    heard(&mut sim);
    sim.seconds(1.5);
    assert!(heard(&mut sim).iter().all(|s| !s.file.contains("footsteps")));
}

#[test]
fn reload_keys_play_the_weapons_sounds() {
    let mut sim = sim();
    let me = player(&mut sim, Vec3::new(20.0, 1.0, 20.0), 0.0);
    sim.app.world_mut().entity_mut(me).insert(LocalPlayer);
    arm(&mut sim, me, "combat_arms:m4a1");
    heard(&mut sim);
    sim.app.world_mut().write_message(combat_arms::game::view_model::CaViewKey {
        owner: me,
        key: combat_arms::game::view_model::machine::ViewKey::Sound(2),
    });
    sim.app.update();
    let s = heard(&mut sim);
    assert!(s.iter().any(|s| s.file == "guns_snd_ar/m4a1/reload2.wav" && s.at.is_none()), "{s:?}");
}
