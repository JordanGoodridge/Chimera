//! specs/combat_arms/view_models.md's test cases (`t04_` is test case
//! 4) that need no install, and the view model in play
//! (`game::view_model`): the state machine on stand-in animation tables
//! with the spec's stock lengths and keys (S: Steam, B: BR 1810.04),
//! stepped at the spec's 60 fps; the sway, placement, flash, field of
//! view and hand-choice maths; and on the greybox, stand-in view models
//! named as the install's (one bone, a `flash` socket) driven by the
//! game. Cases 1, 2 and 5-8 need the install (heavy::view_model).

use std::sync::Arc;

use bevy::prelude::*;
use chimera_core::core::{Health, LocalPlayer};
use chimera_sim::weapon::{self, Inventory, Magazine, data::WeaponDataAppExt, grenade::Throwable};
use chimera_world::{
    greybox::GreyboxMapPlugin,
    harness::Sim,
    map::{
        MapAttachment, MapBone, MapMesh, MapModel, MapViewModel, ViewAnimator, ViewModelOffset, ViewModels,
        anim::{AnimEvent, AnimSet, Animation, Sequence},
        view_model::{aspect_fov, vertical_fov, view_model_fov},
    },
};
use combat_arms::{
    game::{
        TICK_INTERVAL,
        effects::CaShell,
        movement,
        view_model::{
            CaViewKey, CaViewModel, DEFAULT_FOV, VIEW_SCALE, VIEWMODEL_FOV,
            hands::{self, HandProfile},
            machine::{AnimInfo, Class, FireType, Inputs, Rates, Setup, Stage, ViewKey, ViewMachine},
            place::{self, CameraAxes, FlashTimer},
            settings,
            sway::{Sway, SwayInputs},
        },
    },
    harness::CaSim,
};

/// One of the spec's frames (60 fps).
const F: f64 = 1.0 / 60.0;

fn a(name: &str, length: u32, keys: &[(u32, &str)]) -> AnimInfo {
    AnimInfo::new(name, length, keys)
}

/// The M92FS view model (S).
fn m92fs() -> Arc<Vec<AnimInfo>> {
    Arc::new(vec![
        a("CM_HND_SP_DF_Beretta_M92F_FS_CH", 0, &[]),
        a("select", 650, &[(296, "sound_key 3")]),
        a("idle_0", 1566, &[]),
        a("idle_1", 1566, &[]),
        a("prefire", 0, &[]),
        a("fire_0", 180, &[(0, "FIRE_KEY")]),
        a("postfire", 150, &[]),
        a("dryFire", 300, &[(0, "dryfire_key")]),
        a("reload", 1500, &[(226, "sound_key 2"), (453, "sound_key 1"), (962, "sound_key 3")]),
        a("PreRush", 250, &[]),
        a("rush", 1500, &[]),
        a("PostRush", 400, &[]),
    ])
}

/// The M4A1 (B): its `idle_0` with a key at 500 ms for case 32.
fn m4a1() -> Arc<Vec<AnimInfo>> {
    Arc::new(vec![
        a("select", 1200, &[(666, "sound_key 3")]),
        a("idle_0", 1833, &[(500, "")]),
        a("idle_1", 1833, &[]),
        a("prefire", 0, &[]),
        a("fire_0", 105, &[(0, "FIRE_KEY")]),
        a("postfire", 0, &[]),
        a("reload", 2300, &[(403, "sound_key 1"), (834, "sound_key 2"), (1725, "sound_key 3")]),
        a("PreRush", 233, &[]),
        a("rush", 620, &[]),
        a("PostRush", 367, &[]),
    ])
}

/// The M9 knife (B).
fn m9() -> Arc<Vec<AnimInfo>> {
    Arc::new(vec![
        a("select", 450, &[(32, "sound_key 4")]),
        a("idle_0", 2000, &[]),
        a("idle_1", 2000, &[]),
        a("prefire", 1, &[]),
        a("fire_0", 650, &[(155, "FIRE_KEY"), (402, "FIRE_KEY")]),
        a("alt_fire_0", 1100, &[(367, "ALT_FIRE_KEY")]),
        a("postfire", 1, &[]),
        a("PreRush", 167, &[]),
        a("rush", 666, &[]),
        a("PostRush", 167, &[]),
    ])
}

/// The M67 (B).
fn m67() -> Arc<Vec<AnimInfo>> {
    Arc::new(vec![
        a("select", 400, &[]),
        a("idle_0", 933, &[]),
        a("prefire", 700, &[(513, "sound_key 10")]),
        a("fire_0", 67, &[(0, "THROW_KEY")]),
        a("postfire", 366, &[(133, "FIRE_KEY")]),
        a("reload", 400, &[]),
        a("PreRush", 167, &[]),
        a("rush", 666, &[]),
        a("PostRush", 267, &[]),
    ])
}

/// A shotgun shell by shell (the 870's lengths).
fn r870() -> Arc<Vec<AnimInfo>> {
    Arc::new(vec![
        a("select", 1000, &[]),
        a("idle_0", 2000, &[]),
        a("prefire", 0, &[]),
        a("fire_0", 600, &[(0, "FIRE_KEY")]),
        a("PreReload", 333, &[]),
        a("reload", 700, &[]),
        a("PostReload", 900, &[]),
    ])
}

fn setup(class: Class, per_press: u32, clip_size: u32) -> Setup {
    Setup {
        class,
        per_press,
        shells: false,
        clip_size,
        rates: Rates::default(),
    }
}

/// A weapon on the machine, stepped one 60 fps frame per `frame`: the
/// machine's shots take a round, its reloads fill the clip.
struct Run {
    m: ViewMachine,
    clip: u32,
    total: u32,
    /// Frames run (the next frame's update index).
    n: u32,
    /// (update, animation) for every start.
    starts: Vec<(u32, String)>,
    /// Updates that consumed a shot.
    shots: Vec<(u32, FireType)>,
    /// (update whose advance fired it, key).
    keys: Vec<(u32, ViewKey)>,
    reloads: Vec<u32>,
    infinite: bool,
}

impl Run {
    /// Drawn at update 0 (the draw is that update's).
    fn draw(anims: Arc<Vec<AnimInfo>>, s: Setup, clip: u32, total: u32) -> Self {
        Self {
            m: ViewMachine::draw(anims, s, 7),
            clip,
            total,
            n: 0,
            starts: vec![(0, "select".into())],
            shots: Vec::new(),
            keys: Vec::new(),
            reloads: Vec::new(),
            infinite: false,
        }
    }

    fn frame(&mut self, mut i: Inputs) {
        i.clip = self.clip;
        i.total = self.total;
        let before = (self.m.current(), self.m.time());
        let out = self.m.update(&i);
        if let Some(t) = out.shot {
            self.shots.push((self.n, t));
            if !self.infinite {
                self.clip -= 1;
                self.total -= 1;
            }
        }
        if out.reloaded {
            self.clip = self.total.min(self.m.setup().clip_size);
            self.reloads.push(self.n);
        }
        if out.shell_in {
            self.clip += 1;
        }
        let started = self.m.time() == 0.0 && (self.m.current() != before.0 || before.1 != 0.0);
        if started {
            self.starts.push((self.n, self.m.playing().unwrap_or("").to_string()));
        }
        for k in self.m.advance(F) {
            self.keys.push((self.n, k));
        }
        self.n += 1;
    }

    fn frames(&mut self, n: u32, i: Inputs) {
        for _ in 0..n {
            self.frame(i);
        }
    }

    /// Run until the subtle idle plays, then forget the history (the next
    /// update is update 0).
    fn settle(&mut self) {
        for _ in 0..600 {
            self.frame(Inputs::default());
            if self.m.playing() == Some("idle_0") {
                break;
            }
        }
        assert_eq!(self.m.playing(), Some("idle_0"));
        self.n = 0;
        self.starts.clear();
        self.shots.clear();
        self.keys.clear();
        self.reloads.clear();
    }

    fn started(&self, name: &str) -> Vec<u32> {
        self.starts
            .iter()
            .filter(|(_, n)| n.eq_ignore_ascii_case(name))
            .map(|(u, _)| *u)
            .collect()
    }
}

const FIRE: Inputs = Inputs {
    fire: true,
    alt_fire: false,
    reload: false,
    clip: 0,
    total: 0,
    sprinting: false,
    moving: false,
};
const NONE: Inputs = Inputs {
    fire: false,
    ..FIRE
};
const SPRINT: Inputs = Inputs {
    sprinting: true,
    moving: true,
    ..NONE
};
const RELOAD: Inputs = Inputs {
    reload: true,
    ..NONE
};

fn pistol() -> Setup {
    setup(Class::Secondary, 1, 15)
}

#[test]
fn t22_draw_with_fire_held() {
    let mut r = Run::draw(m92fs(), pistol(), 15, 45);
    r.frames(60, FIRE);
    assert!(r.keys.contains(&(17, ViewKey::Sound(3))), "{:?}", r.keys);
    assert!(!r.keys.iter().any(|k| k.0 < 17 && k.1 == ViewKey::Sound(3)));
    assert_eq!(r.started("fire_0").first(), Some(&41), "{:?}", r.starts);
    assert_eq!(r.shots.first().map(|s| s.0), Some(42));
    assert!((42.0 * F * 1000.0 - 700.0).abs() < 1e-6);
    // Without fire: `idle_0` on update 41.
    let mut r = Run::draw(m92fs(), pistol(), 15, 45);
    r.frames(60, NONE);
    assert_eq!(r.started("idle_0").first(), Some(&41), "{:?}", r.starts);
}

#[test]
fn t23_pistol_postfire_cancel() {
    let mut r = Run::draw(m92fs(), pistol(), 15, 45);
    r.settle();
    r.frame(FIRE);
    r.frames(40, NONE);
    assert_eq!(r.started("fire_0"), vec![1]);
    assert_eq!(r.started("postfire"), vec![12], "{:?}", r.starts);
    assert_eq!(r.started("idle_0"), vec![23], "no press: idle after postfire");
    // A second press while `postfire` plays: `fire_0` at once.
    let mut r = Run::draw(m92fs(), pistol(), 15, 45);
    r.settle();
    r.frame(FIRE);
    r.frames(14, NONE);
    assert_eq!(r.m.playing(), Some("postfire"));
    r.frame(FIRE);
    r.frames(20, NONE);
    assert_eq!(r.started("fire_0"), vec![1, 15], "{:?}", r.starts);
    assert_eq!(r.started("postfire"), vec![12, 26], "the rest of the first post-fire skipped");
    assert_eq!(r.shots.len(), 2);
}

#[test]
fn t24_knife_without_prefire() {
    let mut r = Run::draw(m9(), setup(Class::Melee, 0, 0), 1, 1);
    r.infinite = true;
    r.settle();
    r.frame(FIRE);
    r.frames(40, NONE);
    assert_eq!(r.started("fire_0"), vec![1], "{:?}", r.starts);
    assert!(r.started("prefire").is_empty());
    let hits: Vec<u32> = r.shots.iter().map(|s| s.0).collect();
    assert_eq!(hits, vec![11, 26]);
    // The stab: `alt_fire_0`, its key an alternate hit.
    let mut r = Run::draw(m9(), setup(Class::Melee, 0, 0), 1, 1);
    r.infinite = true;
    r.settle();
    r.frame(Inputs {
        alt_fire: true,
        ..NONE
    });
    r.frames(40, NONE);
    assert_eq!(r.started("alt_fire_0"), vec![1]);
    assert_eq!(r.shots, vec![(24, FireType::Alternate)]);
}

fn grenade() -> Setup {
    setup(Class::Throw, 0, 1)
}

#[test]
fn t25_grenade_release() {
    let mut r = Run::draw(m67(), grenade(), 1, 2);
    r.settle();
    r.frames(30, FIRE);
    r.frames(60, NONE);
    // Held again during the re-ready (ignored until it ends).
    r.frames(20, FIRE);
    assert_eq!(r.started("prefire"), vec![1, 100], "{:?}", r.starts);
    assert_eq!(r.started("fire_0"), vec![44]);
    assert_eq!(r.started("postfire"), vec![49]);
    assert_eq!(r.shots.first().map(|s| s.0), Some(57));
    assert!((57.0 * F * 1000.0 - 950.0).abs() < 1e-6);
    assert_eq!(r.started("reload"), vec![73]);
    assert_eq!(r.reloads, vec![98]);
    // The pin sound and the throw notice.
    assert!(r.keys.iter().any(|k| k.1 == ViewKey::Sound(10)));
    assert_eq!(r.keys.iter().filter(|k| k.1 == ViewKey::Throw).count(), 1);
}

#[test]
fn t26_grenade_held() {
    let mut r = Run::draw(m67(), grenade(), 1, 2);
    r.settle();
    r.frames(120, FIRE);
    r.frames(20, NONE);
    let loops: Vec<u32> = (0..=14).map(|k| 49 + 5 * k).collect();
    let mut fire0 = r.started("fire_0");
    assert_eq!(fire0.remove(0), 44);
    assert_eq!(fire0, loops, "{:?}", r.starts);
    assert_eq!(r.started("postfire"), vec![124]);
    assert_eq!(r.shots.first().map(|s| s.0), Some(132));
    assert!((132.0 * F * 1000.0 - 2200.0).abs() < 1e-6);
    assert_eq!(r.keys.iter().filter(|k| k.1 == ViewKey::Throw).count(), 16, "a notice per loop");
}

#[test]
fn t27_sprint_cycle() {
    let mut r = Run::draw(m92fs(), pistol(), 15, 45);
    r.settle();
    r.frames(60, SPRINT);
    r.frames(40, FIRE);
    assert_eq!(r.started("PreRush"), vec![0], "{:?}", r.starts);
    assert_eq!(r.started("rush"), vec![16]);
    assert_eq!(r.started("PostRush"), vec![60]);
    assert_eq!(r.started("idle_1"), vec![85]);
    assert!(!r.started("idle_0").contains(&85));
    assert_eq!(r.started("fire_0"), vec![87]);
    assert!(matches!(r.m.stage, Stage::Firing(_)) || r.m.playing() == Some("postfire"));
}

#[test]
fn t28_sprint_mid_reload() {
    let mut r = Run::draw(m4a1(), setup(Class::Primary, 0, 30), 10, 100);
    r.settle();
    r.frame(RELOAD);
    r.frames(59, NONE);
    r.frames(100, SPRINT);
    assert_eq!(r.started("reload"), vec![0]);
    assert_eq!(r.reloads, vec![139]);
    assert_eq!(r.started("PreRush"), vec![139], "{:?}", r.starts);
    assert_eq!(r.clip, 30);
}

#[test]
fn t29_last_round() {
    let mut r = Run::draw(m92fs(), pistol(), 1, 16);
    r.settle();
    r.frames(115, FIRE);
    assert_eq!(r.shots.len(), 1);
    assert_eq!(r.started("fire_0"), vec![1]);
    assert_eq!(r.started("postfire"), vec![12]);
    assert_eq!(r.started("reload"), vec![23], "{:?}", r.starts);
    assert_eq!(r.reloads, vec![114]);
    assert_eq!(r.clip, 15);
}

#[test]
fn t30_dry_fire_not_reached() {
    let mut r = Run::draw(m92fs(), pistol(), 0, 0);
    r.settle();
    for _ in 0..1200 {
        r.frame(FIRE);
        let p = r.m.playing().unwrap();
        assert!(p == "idle_0" || p == "idle_1", "{p}");
    }
    assert!(r.started("dryFire").is_empty() && r.shots.is_empty());
}

#[test]
fn t32_one_frame_pop() {
    let anims = m4a1();
    let mut r = Run::draw(anims.clone(), setup(Class::Primary, 0, 30), 10, 100);
    r.settle();
    r.frames(10, NONE);
    let idle = r.m.find("idle_0").unwrap();
    assert!(r.m.time() > 0.0 && r.m.time() < 500.0);
    let i = Inputs {
        reload: true,
        clip: 10,
        total: 100,
        ..NONE
    };
    r.m.update(&i);
    // Frame n's image: `idle_0`'s key 1 (500 ms).
    assert_eq!(r.m.playing(), Some("reload"));
    assert_eq!(r.m.shown(), Some((idle, 500.0)));
    r.m.advance(F);
    r.m.update(&Inputs {
        reload: false,
        ..i
    });
    let reload = r.m.find("reload").unwrap();
    let (s, t) = r.m.shown().unwrap();
    assert_eq!(s, reload);
    assert!((t - 1000.0 * F).abs() < 1e-9, "{t}");
}

#[test]
fn full_idle_after_a_still_wait() {
    let mut r = Run::draw(m92fs(), pistol(), 15, 45);
    r.settle();
    r.frames(60 * 20, NONE);
    let first = *r.started("idle_1").first().expect("a full idle within 20 s");
    let t = first as f64 * F;
    // The wait (5-15 s from entering idle) ends at an idle_0's end.
    assert!((4.9..=15.0 + 1.6).contains(&t), "{t}");
    // Moving: no full idle.
    let mut r = Run::draw(m92fs(), pistol(), 15, 45);
    r.settle();
    r.frames(60 * 30, Inputs {
        moving: true,
        ..NONE
    });
    assert!(r.started("idle_1").is_empty());
}

#[test]
fn rates_scale_playback() {
    // Draw rate 2: the 650 ms select ends after 326 ms (update 20).
    let fast = Setup {
        rates: Rates {
            draw: 2.0,
            ..Rates::default()
        },
        ..pistol()
    };
    let mut r = Run::draw(m92fs(), fast, 15, 45);
    r.frames(30, NONE);
    assert_eq!(r.started("idle_0"), vec![21], "{:?}", r.starts);
    // Above 2.0: 1.0.
    let capped = Setup {
        rates: Rates {
            draw: 2.5,
            ..Rates::default()
        },
        ..pistol()
    };
    let mut r = Run::draw(m92fs(), capped, 15, 45);
    r.frames(60, NONE);
    assert_eq!(r.started("idle_0"), vec![41]);
    // Reload rate 1.5: 1500 ms in 1000 ms.
    let quick = Setup {
        rates: Rates {
            reload: 1.5,
            ..Rates::default()
        },
        ..pistol()
    };
    let mut r = Run::draw(m92fs(), quick, 10, 45);
    r.settle();
    r.frame(RELOAD);
    r.frames(80, NONE);
    assert_eq!(r.reloads, vec![61], "1001 ms at 1.5: 61 frames");
}

#[test]
fn shell_reload_and_fire_stopping_it() {
    let shotgun = Setup {
        shells: true,
        ..setup(Class::Shotgun, 1, 8)
    };
    let mut r = Run::draw(r870(), shotgun, 5, 20);
    r.settle();
    r.frame(RELOAD);
    r.frames(400, NONE);
    let order: Vec<&str> = r.starts.iter().map(|(_, n)| n.as_str()).collect();
    assert_eq!(order[..6], ["PreReload", "reload", "reload", "reload", "PostReload", "idle_0"], "{order:?}");
    assert_eq!(r.clip, 8);
    // Fire mid-reload stops it at once (no pre-fire for a shotgun).
    let mut r = Run::draw(r870(), shotgun, 5, 20);
    r.settle();
    r.frame(RELOAD);
    r.frames(30, NONE);
    assert_eq!(r.m.playing(), Some("reload"));
    r.frames(3, FIRE);
    assert_eq!(r.m.playing(), Some("fire_0"));
    assert!(r.started("PostReload").is_empty());
}

#[test]
fn automatic_waits_for_postfire_and_reload_after_postfire() {
    let anims = Arc::new(vec![
        a("select", 100, &[]),
        a("idle_0", 1000, &[]),
        a("prefire", 0, &[]),
        a("fire_0", 100, &[(0, "FIRE_KEY")]),
        a("postfire", 200, &[]),
        a("reload", 500, &[]),
    ]);
    let mut r = Run::draw(anims.clone(), setup(Class::Primary, 0, 30), 30, 90);
    r.settle();
    r.frame(FIRE);
    r.frames(9, NONE);
    assert_eq!(r.started("postfire"), vec![9], "{:?}", r.starts);
    // An automatic's new press waits for the post-fire.
    r.frames(3, FIRE);
    assert_eq!(r.m.playing(), Some("postfire"));
    // A reload asked for mid-fire plays after the post-fire.
    let mut r = Run::draw(anims, setup(Class::Primary, 0, 30), 20, 90);
    r.settle();
    r.frame(FIRE);
    r.frame(FIRE);
    r.frame(Inputs {
        reload: true,
        ..FIRE
    });
    r.frames(40, FIRE);
    let order: Vec<&str> = r.starts.iter().map(|(_, n)| n.as_str()).collect();
    assert_eq!(order[..4], ["prefire", "fire_0", "postfire", "reload"], "{order:?}");
}

#[test]
fn grenade_and_sprint() {
    // Sprint during the pin pull: `prefire` plays out and holds; the rest
    // waits for the sprint to end.
    let mut r = Run::draw(m67(), grenade(), 1, 2);
    r.settle();
    r.frames(10, FIRE);
    r.frames(80, Inputs {
        sprinting: true,
        ..FIRE
    });
    assert_eq!(r.m.playing(), Some("prefire"));
    assert!(r.started("PreRush").is_empty() && r.started("fire_0").is_empty());
    r.frames(3, FIRE);
    assert_eq!(r.m.playing(), Some("fire_0"));
}

// Sway (6)

fn running(n: usize, dt: f64, f: impl Fn(usize) -> bool) -> Vec<Sway> {
    let mut s = Sway::default();
    (1..=n)
        .map(|i| {
            s.step(&SwayInputs {
                dt,
                clock: i as f64 * dt,
                active: true,
                run: true,
                moving: f(i),
                speed: 100.0,
                spectating: false,
                weapon: true,
                zoomed: false,
                scale: 1.0,
            });
            s
        })
        .collect()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 2e-6
}

#[test]
fn t13_sway_ramp() {
    let s = running(12, F, |_| true);
    let want = [
        (1, 0.0, 0.0, 0.0, 0.0),
        (2, 0.66987, 0.0, -0.00058, 0.0),
        (3, 2.5, 0.146608, -0.0019592, 0.0010839),
        (4, 5.0, 0.293215, -0.0034227, 0.0041504),
        (5, 6.66667, 0.439823, -0.0038048, 0.0077051),
        (6, 8.33333, 0.586431, -0.0037053, 0.0115233),
        (7, 10.0, 0.733038, -0.0030902, 0.0149178),
        (12, 10.0, 1.466077, 0.0040674, 0.0031187),
    ];
    for (f, amp, phase, w, h) in want {
        let x = s[f - 1];
        assert!((x.amp - amp).abs() < 1e-5 && (x.phase - phase).abs() < 1e-6, "frame {f}: {x:?}");
        assert!((x.w - w).abs() < 1e-6 && (x.h - h).abs() < 1e-6, "frame {f}: {x:?}");
    }
    // The M4A1's origin on frame 7.
    let o = place::model_origin(Vec3::new(0.3, -0.6, 1.15), (s[6].w as f32, s[6].h as f32), 0.0);
    assert!((o - Vec3::new(0.2969098, -0.5850822, 1.15)).length() < 1e-6, "{o}");
}

#[test]
fn t14_sway_steady_state() {
    let s = running(200, F, |_| true);
    let (a, b) = (s[29], s[59]);
    assert!(near(a.phase, 4.105014) && near(a.w, 0.0008368) && near(a.h, 0.0140592), "{a:?}");
    assert!(near(b.phase, 2.220059) && near(b.w, 0.0092186) && near(b.h, -0.0144474), "{b:?}");
    assert!(s.iter().all(|x| x.w.abs() <= 0.010 + 1e-9 && x.h.abs() <= 0.015 + 1e-9));
}

#[test]
fn t15_sway_decay() {
    let s = running(70, F, |i| i <= 60);
    let amps: Vec<f64> = (61..=67).map(|f| s[f - 1].amp).collect();
    for (got, want) in amps.iter().zip([10.0, 8.33333, 6.66667, 5.0, 2.5, 0.66987, 0.0]) {
        assert!((got - want).abs() < 1e-5, "{amps:?}");
    }
    assert!(near(s[66].phase, 3.246312), "{:?}", s[66]);
    assert_eq!(s[67].phase, 0.0);
    assert!(near(s[63].w, 0.0049114) && near(s[63].h, -0.0046586), "{:?}", s[63]);
    assert!(s[66..].iter().all(|x| x.w == 0.0 && x.h == 0.0));
}

#[test]
fn t16_walk_and_crouch_pace() {
    let mut s = Sway::default();
    let mut out = Vec::new();
    for i in 1..=7 {
        s.step(&SwayInputs {
            dt: F,
            clock: i as f64 * F,
            active: true,
            run: false,
            moving: true,
            speed: 100.0,
            weapon: true,
            scale: 1.0,
            ..default()
        });
        out.push(s);
    }
    assert!(near(out[3].phase - out[2].phase, 0.0628319));
    assert!(near(out[2].w, -0.0020823) && near(out[2].h, 0.00047), "{:?}", out[2]);
    assert!(near(out[6].amp, 10.0) && near(out[6].phase, 0.314159));
    assert!(near(out[6].w, -0.0066913) && near(out[6].h, 0.0088168), "{:?}", out[6]);
}

#[test]
fn t17_ramp_at_100_fps() {
    let s = running(11, 0.01, |_| true);
    let want = [0.24472, 0.95492, 2.06107, 3.45492, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
    for (f, w) in (2..=11).zip(want) {
        assert!((s[f - 1].amp - w).abs() < 1e-4, "frame {f}: {}", s[f - 1].amp);
    }
}

#[test]
fn t18_frame_rate_gate() {
    let run = |dt: f64, n: usize, s: &mut Sway| {
        for i in 0..n {
            s.step(&SwayInputs {
                dt,
                clock: i as f64 * dt,
                active: true,
                run: true,
                moving: true,
                speed: 20.0,
                weapon: true,
                scale: 1.0,
                ..default()
            });
        }
    };
    let mut s = Sway::default();
    run(F, 30, &mut s);
    assert_eq!(s.amp, 10.0, "60 fps: 0.333 u a frame");
    // 250 fps: 0.08 u a frame, decays within 0.1 s and stays at 0.
    let mut slow = s;
    let mut t = 0.0;
    let dt = 1.0 / 250.0;
    let mut zero_at = None;
    for i in 0..100 {
        t += dt;
        slow.step(&SwayInputs {
            dt,
            clock: 10.0 + t,
            active: true,
            run: true,
            moving: true,
            speed: 20.0,
            weapon: true,
            scale: 1.0,
            ..default()
        });
        if slow.amp == 0.0 && zero_at.is_none() {
            zero_at = Some(i);
        }
        if zero_at.is_some() {
            assert_eq!(slow.amp, 0.0);
        }
    }
    // Counted from the first frame not moving (d = 0 there).
    assert!(zero_at.is_some_and(|i| i as f64 * dt <= 0.1 + 1e-9), "{zero_at:?}");
}

#[test]
fn t19_airborne_freeze() {
    let mut s = running(60, F, |_| true)[59];
    let held = s;
    for i in 0..20 {
        s.step(&SwayInputs {
            dt: F,
            clock: 2.0 + i as f64 * F,
            active: false,
            run: true,
            moving: true,
            speed: 100.0,
            weapon: true,
            scale: 1.0,
            ..default()
        });
        assert_eq!(s, held);
    }
    s.step(&SwayInputs {
        dt: F,
        clock: 3.0,
        active: true,
        run: true,
        moving: true,
        speed: 100.0,
        weapon: true,
        scale: 1.0,
        ..default()
    });
    assert!(near(s.phase, held.phase + std::f64::consts::TAU * 1.4 * F));
    assert_eq!(s.amp, 10.0);
}

#[test]
fn t20_sway_scale() {
    let at = |scale: f64| {
        let mut s = Sway::default();
        for i in 1..=30 {
            s.step(&SwayInputs {
                dt: F,
                clock: i as f64 * F,
                active: true,
                run: true,
                moving: true,
                speed: 100.0,
                weapon: true,
                scale,
                ..default()
            });
        }
        s
    };
    let (full, half, none) = (at(1.0), at(0.5), at(0.0));
    assert!(near(half.w, full.w / 2.0) && near(half.h, full.h / 2.0));
    assert_eq!((none.w, none.h), (0.0, 0.0));
}

// Placement, flash, FOV (4, 5, 9, 10)

#[test]
fn t04_rest_placement() {
    let pos = Vec3::new(-0.1, -0.2, 0.7);
    assert_eq!(place::model_origin(pos, (0.0, 0.0), 0.0), pos);
}

fn facing_z() -> CameraAxes {
    CameraAxes {
        right: Vec3::X,
        up: Vec3::Y,
        forward: Vec3::Z,
    }
}

#[test]
fn t09_world_flash_point() {
    let o = place::flash_offset(
        Vec3::new(-0.1, -0.2, 0.7),
        Vec3::new(2.84, -2.4, 8.559921),
        (0.0, 0.0),
        Vec3::ZERO,
    );
    let f = place::flash_point(Vec3::ZERO, &facing_z(), o);
    assert!((f - Vec3::new(2.74, -2.6, 9.259921)).length() < 1e-4, "{f}");
    // Yawed 90 degrees right: forward +X, right -Z.
    let yawed = CameraAxes {
        right: Vec3::NEG_Z,
        up: Vec3::Y,
        forward: Vec3::X,
    };
    let f = place::flash_point(Vec3::ZERO, &yawed, o);
    assert!((f - Vec3::new(9.259921, -2.6, -2.74)).length() < 1e-4, "{f}");
}

#[test]
fn t10_shell_start() {
    let pos = Vec3::new(0.3, -0.6, 1.15);
    let muzzle = Vec3::new(4.4, -3.55, 7.75);
    let f = place::flash_point(Vec3::ZERO, &facing_z(), place::flash_offset(pos, muzzle, (0.0, 0.0), Vec3::ZERO));
    let m16 = place::breach_point(f, &facing_z(), Vec3::ONE);
    let m4 = place::breach_point(f, &facing_z(), Vec3::ZERO);
    assert!((m16 - Vec3::new(5.7, -3.15, 9.9)).length() < 1e-4, "{m16}");
    assert!((m4 - Vec3::new(4.7, -4.15, 8.9)).length() < 1e-4, "{m4}");
}

#[test]
fn t11_world_and_view_model_fov() {
    for (w, h, world, pv) in [
        (1024.0, 768.0, 91.309, 63.742),
        (1920.0, 1080.0, 107.512, 79.317),
        (1680.0, 1050.0, 101.673, 73.453),
        (1280.0, 1024.0, 87.612, 60.474),
    ] {
        let a = w / h;
        assert!((place::fov_x(75.0, a) - world).abs() < 1e-3, "{w}x{h}");
        assert!((place::fov_x(50.0, a) - pv).abs() < 1e-3, "{w}x{h}");
        // Chimera's view-model camera from our 4:3 settings agrees.
        assert!((aspect_fov(VIEWMODEL_FOV, a) - pv).abs() < 0.01, "{w}x{h}");
    }
    assert!((vertical_fov(VIEWMODEL_FOV) - 50.0).abs() < 0.01);
}

#[test]
fn t21_shot_jiggle() {
    let (pos, muzzle) = (Vec3::new(-0.1, -0.2, 0.7), Vec3::new(2.84, -2.4, 8.559921));
    let recoil = Vec3::new(0.1, 0.2, 0.3);
    // Rolls giving the draws (0.05, -0.1, 0.2).
    let d = place::jiggle_draws(recoil, [0.75, 0.25, 0.833_333_3]);
    assert!((d - Vec3::new(0.05, -0.1, 0.2)).length() < 1e-6, "{d}");
    let j = d.x + d.y + d.z;
    assert!((place::model_origin(pos, (0.0, 0.0), j) - (pos + Vec3::splat(0.15))).length() < 1e-6);
    assert!((place::flash_offset(pos, muzzle, (0.0, 0.0), d) - (pos + muzzle + d)).length() < 1e-6);
    // The next frame, no shot: back.
    assert_eq!(place::model_origin(pos, (0.0, 0.0), 0.0), pos);
    assert_eq!(place::flash_offset(pos, muzzle, (0.0, 0.0), Vec3::ZERO), pos + muzzle);
}

#[test]
fn t31_muzzle_flash_visibility() {
    let mut t = FlashTimer::default();
    let shot = 10.0;
    t.shot(shot);
    let shown: Vec<bool> = (0..6)
        .map(|i| t.update(shot + i as f64 * F, place::FLASH_TIME, true, true, false))
        .collect();
    assert_eq!(shown, [true, true, true, false, false, false]);
    // Zoomed (the model hidden): never.
    let mut t = FlashTimer::default();
    t.shot(shot);
    assert!((0..3).all(|i| !t.update(shot + i as f64 * F, place::FLASH_TIME, false, true, false)));
}

// Hands (1, 2)

#[test]
fn t03_sleeve_texture() {
    let table = hands::builtin();
    let pick = |face, uniform| {
        hands::choose(&table, &HandProfile {
            face,
            uniform,
            character: true,
            ..default()
        })
        .unwrap()
    };
    let c = pick(0, 0);
    assert_eq!(c.model, "chars_m_hand/normal_m_hand.ltb");
    assert_eq!(c.skins[0], "chars_t_hand/hand_00_00.dtx");
    assert_eq!(pick(2, 3).skins[0], "chars_t_hand/hand_02_03.dtx");
    // No character yet: the record's own skins.
    let c = hands::choose(&table, &HandProfile::default()).unwrap();
    assert_eq!(c.skins, vec!["chars_t_hand/hand_00_00.dtx".to_string()]);
    // Hand index -1: no arms; tattoo slots blanked.
    let none = hands::HandTable {
        models: vec![hands::CharacterHands {
            hand_index: -1,
            ..default()
        }],
        ..table.clone()
    };
    assert!(hands::choose(&none, &HandProfile::default()).is_none());
    let tattoo = hands::HandTable {
        hands: vec![hands::HandRecord {
            model: "h.ltb".into(),
            skins: vec!["a.dtx".into(), "PV_L_ARM".into(), "PV_R_ARM".into()],
            ..default()
        }],
        ..table
    };
    let c = hands::choose(&tattoo, &HandProfile::default()).unwrap();
    assert_eq!(c.skins[1..], [hands::NO_TATTOO.to_string(), hands::NO_TATTOO.to_string()]);
}

fn bone(name: &str, parent: Option<usize>, position: Vec3) -> MapBone {
    MapBone {
        name: name.into(),
        parent,
        position,
        rotation: Quat::IDENTITY,
    }
}

/// Case 1 on stand-ins: the hand's bones find the weapon's nodes by name
/// (case ignored), skipping the gun's own nodes between the two arms; its
/// vertices follow into the weapon's bind pose.
#[test]
fn t01_hand_to_weapon_bone_match() {
    let mut weapon = view_model("w", &[]);
    weapon.bones = vec![
        bone("Scene Root", None, Vec3::ZERO),
        bone("Bip01 R Hand", Some(0), Vec3::new(1.0, 0.0, 0.0)),
        bone("Box01", Some(1), Vec3::ZERO),
        bone("Bip01 L Hand", Some(0), Vec3::new(-1.0, 0.0, 0.0)),
    ];
    weapon.model.meshes.push(MapMesh::default());
    let hand_bones = vec![
        bone("scene root", None, Vec3::ZERO),
        bone("BIP01 R HAND", Some(0), Vec3::new(1.0, 0.0, 0.0)),
        // The left hand's bind differs: its vertices move with it.
        bone("Bip01 L Hand", Some(0), Vec3::new(-2.0, 0.0, 0.0)),
    ];
    let mesh = MapMesh {
        positions: vec![[0.1, 0.0, 0.0], [-0.2, 0.0, 0.0]],
        normals: vec![[0.0, 1.0, 0.0]; 2],
        joints: vec![[1, 0, 0, 0], [2, 0, 0, 0]],
        joint_weights: vec![[1.0, 0.0, 0.0, 0.0]; 2],
        ..default()
    };
    let map = hands::attach(&mut weapon, vec![mesh], &hand_bones, 0.1, Vec3::new(0.0, 0.0, -1.0)).unwrap();
    assert_eq!(map, vec![0, 1, 3]);
    let m = weapon.model.meshes.last().unwrap();
    assert_eq!(weapon.model.meshes.len(), 2, "the weapon's piece and the hand's");
    assert_eq!(m.joints, vec![[1, 0, 0, 0], [3, 0, 0, 0]]);
    let p: Vec<Vec3> = m.positions.iter().map(|p| Vec3::from(*p)).collect();
    assert!((p[0] - Vec3::new(0.1, 0.0, -1.0)).length() < 1e-6, "{p:?}");
    assert!((p[1] - Vec3::new(-0.1, 0.0, -1.0)).length() < 1e-6, "{p:?}");
    // A hand bone the weapon lacks.
    let mut bad = hand_bones;
    bad.push(bone("Bip01 Tail", Some(0), Vec3::ZERO));
    assert!(hands::attach(&mut weapon, Vec::new(), &bad, 0.1, Vec3::ZERO).is_err());
}

// In play, on the greybox

const M92FS: &str = "combat_arms:beretta_m92fs";
const R870: &str = "combat_arms:remington_870";
const ZOOMING: &str = "test:m92fs_zoom";
const M67: &str = "combat_arms:m67";

/// The M92FS with a sight (no such stock weapon is unkeyed).
const ZOOM_DOC: &str = r#"
id = "test:m92fs_zoom"
base = "combat_arms:beretta_m92fs"
[hooks."combat_arms:zoom"]
zoom_time = 0.13
[hooks."combat_arms:attachments"]
accepts = ["DotSight_A_Pistol"]
defaults = { sight = "DotSight_A_Pistol" }
"#;

/// A stand-in view model: one bone with a `flash` socket 1 unit ahead,
/// each animation `AnimInfo` played exactly (two frames over its length),
/// its keys as events.
fn view_model(key: &str, anims: &[AnimInfo]) -> MapViewModel {
    let defaults = vec![(Quat::IDENTITY, Vec3::ZERO)];
    let mut set = AnimSet {
        bases: vec![defaults.clone()],
        defaults,
        ..default()
    };
    for (i, info) in anims.iter().enumerate() {
        let secs = (info.length as f32 / 1000.0).max(1e-3);
        set.animations.push(Animation {
            name: info.name.clone(),
            fps: 1.0 / secs,
            frames: 2,
            ..default()
        });
        set.sequences.push(Sequence {
            name: info.name.clone(),
            snap: true,
            grid: (1, 1),
            anims: vec![i],
            bone_weights: vec![1.0],
            events: info
                .keys
                .iter()
                .map(|(t, s)| AnimEvent {
                    cycle: if info.length > 0 { *t as f32 / info.length as f32 } else { 0.0 },
                    name: s.clone(),
                    ..default()
                })
                .collect(),
            ..default()
        });
    }
    MapViewModel {
        key: key.to_string(),
        model: MapModel::default(),
        bones: vec![bone("Scene Root", None, Vec3::ZERO)],
        root: Transform::from_scale(Vec3::splat(VIEW_SCALE)).with_translation(Vec3::new(-0.01, -0.02, -0.07)),
        animations: Some(Arc::new(set)),
        right_handed: true,
        allow_flipping: false,
        attachments: vec![MapAttachment {
            name: "flash".into(),
            bone: 0,
            local: Transform::from_xyz(0.0, 0.0, -1.0),
        }],
        light_origin: Vec3::ZERO,
    }
}

fn sim() -> Sim {
    let mut sim = Sim::with(CaSim, |app| {
        app.add_plugins(GreyboxMapPlugin);
    });
    sim.set_tick_interval(TICK_INTERVAL);
    sim.app.add_weapon_data(&[("zoom", ZOOM_DOC)]).unwrap();
    let pistol: Vec<AnimInfo> = m92fs().iter().cloned().collect();
    let shotgun: Vec<AnimInfo> = r870().iter().cloned().collect();
    let grenade: Vec<AnimInfo> = m67().iter().cloned().collect();
    sim.app.insert_resource(ViewModels(Arc::new(vec![
        view_model(M92FS, &pistol),
        view_model(ZOOMING, &pistol),
        view_model(R870, &shotgun),
        view_model(M67, &grenade),
    ])));
    sim
}

fn player(sim: &mut Sim) -> Entity {
    let p = sim.spawn_character(Vec3::new(30.0, 1.0, 30.0), movement::ID);
    sim.seconds(0.5);
    sim.app
        .world_mut()
        .entity_mut(p)
        .insert(Health { current: 1.0, max: 1.0 });
    p
}

fn playing(sim: &Sim, p: Entity) -> Option<String> {
    let vm = sim.app.world().get::<CaViewModel>(p)?;
    vm.machine.as_ref()?.playing().map(str::to_string)
}

/// The sequence the drawn view model shows.
fn shown(sim: &Sim, p: Entity) -> Option<String> {
    sim.app.world().get::<ViewAnimator>(p)?.sequence().map(str::to_string)
}

fn vm(sim: &Sim, p: Entity) -> &CaViewModel {
    sim.app.world().get::<CaViewModel>(p).unwrap()
}

/// Ticks until `f`, at most `n`; whether it happened.
fn until(sim: &mut Sim, n: u32, f: impl Fn(&Sim) -> bool) -> bool {
    for _ in 0..n {
        if f(sim) {
            return true;
        }
        sim.ticks(1);
    }
    f(sim)
}

fn drawn(sim: &mut Sim, p: Entity, id: &str) -> Entity {
    let w = weapon::give(sim.app.world_mut(), p, id).unwrap();
    sim.ticks(3);
    assert_eq!(shown(sim, p).as_deref(), Some("select"));
    assert!(until(sim, 100, |s| shown(s, p).as_deref() == Some("idle_0")));
    assert_eq!(sim.app.world().get::<Inventory>(p).unwrap().active, Some(w));
    w
}

#[test]
fn plays_the_machine_in_game() {
    let mut sim = sim();
    let p = player(&mut sim);
    let w = drawn(&mut sim, p, M92FS);
    // A tap: `fire_0`, `postfire`, then idle.
    sim.intent(p).fire = true;
    sim.ticks(1);
    sim.intent(p).fire = false;
    assert!(until(&mut sim, 5, |s| shown(s, p).as_deref() == Some("fire_0")));
    assert!(until(&mut sim, 40, |s| shown(s, p).as_deref() == Some("postfire")));
    assert!(until(&mut sim, 40, |s| shown(s, p).as_deref() == Some("idle_0")));
    // An empty clip reloads once the fire sequence is over.
    sim.app.world_mut().get_mut::<Magazine>(w).unwrap().clip = 1;
    sim.intent(p).fire = true;
    sim.ticks(1);
    sim.intent(p).fire = false;
    assert!(until(&mut sim, 60, |s| playing(s, p).as_deref() == Some("reload")));
    assert!(until(&mut sim, 200, |s| playing(s, p).is_some_and(|n| n.starts_with("idle_"))));
    assert_eq!(sim.app.world().get::<Magazine>(w).unwrap().clip, 15);
    // Its rest placement: identity rotation, no sway standing still.
    let o = sim.app.world().get::<ViewModelOffset>(p).unwrap();
    assert_eq!((o.translation, o.rotation), (Vec3::ZERO, Quat::IDENTITY));
}

#[test]
fn key_events_and_flash_and_shells() {
    let mut sim = sim();
    let p = player(&mut sim);
    sim.app.world_mut().entity_mut(p).insert(LocalPlayer);
    #[derive(Resource, Default)]
    struct Keys(Vec<ViewKey>);
    sim.app.init_resource::<Keys>();
    sim.app.add_systems(Update, |mut r: MessageReader<CaViewKey>, mut k: ResMut<Keys>| {
        k.0.extend(r.read().map(|e| e.key.clone()));
    });
    drawn(&mut sim, p, M92FS);
    assert!(sim.app.world().resource::<Keys>().0.contains(&ViewKey::Sound(3)), "the select's sound key");
    sim.intent(p).fire = true;
    sim.ticks(1);
    sim.intent(p).fire = false;
    assert!(until(&mut sim, 3, |s| vm(s, p).flash.shown));
    let v = vm(&sim, p);
    // The flash at the stand-in's socket + 0.5 u ahead (view-model space).
    let at = v.flash_at.expect("flash place");
    assert!(at.z < -0.07, "{at}");
    let flash_point = v.flash_point;
    assert!(until(&mut sim, 8, |s| !vm(s, p).flash.shown), "50 ms");
    let shells: Vec<(CaShell, Transform)> = sim
        .app
        .world_mut()
        .query::<(&CaShell, &Transform)>()
        .iter(sim.app.world())
        .map(|(s, t)| (s.clone(), *t))
        .collect();
    assert_eq!(shells.len(), 1);
    assert!(shells[0].1.translation.distance(flash_point) < 1.0);
    assert!(sim.app.world().resource::<Keys>().0.contains(&ViewKey::Fire));
    sim.seconds(3.5);
    assert_eq!(sim.app.world_mut().query::<&CaShell>().iter(sim.app.world()).count(), 0, "gone after 3 s");
}

#[test]
fn sprint_pose_in_game() {
    let mut sim = sim();
    let p = player(&mut sim);
    drawn(&mut sim, p, M92FS);
    {
        let mut i = sim.intent(p);
        i.sprint = true;
        i.move_axis = Vec2::Y;
    }
    assert!(until(&mut sim, 40, |s| shown(s, p).as_deref() == Some("PreRush")));
    assert!(until(&mut sim, 40, |s| shown(s, p).as_deref() == Some("rush")));
    // Running: the sway moves the model, within its small bounds.
    sim.seconds(0.5);
    let o = sim.app.world().get::<ViewModelOffset>(p).unwrap().translation / VIEW_SCALE;
    assert!(o.length() > 0.0 && o.x.abs() <= 0.0101 && o.y.abs() <= 0.0151, "{o}");
    {
        let mut i = sim.intent(p);
        i.sprint = false;
        i.move_axis = Vec2::ZERO;
    }
    assert!(until(&mut sim, 40, |s| shown(s, p).as_deref() == Some("PostRush")));
    assert!(until(&mut sim, 60, |s| shown(s, p).as_deref() == Some("idle_1")));
}

#[test]
fn shotgun_reloads_shell_by_shell() {
    let mut sim = sim();
    let p = player(&mut sim);
    let w = drawn(&mut sim, p, R870);
    sim.app.world_mut().get_mut::<Magazine>(w).unwrap().clip = 5;
    sim.intent(p).reload = true;
    sim.ticks(1);
    sim.intent(p).reload = false;
    let mut seen: Vec<String> = Vec::new();
    for _ in 0..400 {
        sim.ticks(1);
        if let Some(s) = playing(&sim, p)
            && seen.last() != Some(&s)
        {
            seen.push(s);
        }
    }
    let at = |n: &str| seen.iter().position(|s| s == n);
    let (start, lp, end) = (at("PreReload"), at("reload"), at("PostReload"));
    assert!(start < lp && lp < end && start.is_some(), "{seen:?}");
    assert_eq!(seen.last().map(String::as_str), Some("idle_0"));
}

#[test]
fn t12_zoom_hides_and_leaves_the_fov() {
    let mut sim = sim();
    let p = player(&mut sim);
    drawn(&mut sim, p, ZOOMING);
    let hidden = |s: &Sim| s.app.world().get::<ViewAnimator>(p).unwrap().hidden;
    sim.intent(p).secondary = true;
    sim.ticks(1);
    sim.intent(p).secondary = false;
    assert!(hidden(&sim), "hidden from the press");
    sim.seconds(0.5);
    assert!(hidden(&sim));
    sim.intent(p).secondary = true;
    sim.ticks(1);
    sim.intent(p).secondary = false;
    assert!(hidden(&sim), "until the zoom-out ends");
    assert!(until(&mut sim, 60, |s| !hidden(s)));
    // Shown again unzoomed: the view model's vertical FOV is 50 degrees.
    let fov = view_model_fov(&settings(), DEFAULT_FOV);
    assert!((vertical_fov(fov) - 50.0).abs() < 0.01);
}

#[test]
fn one_frame_pop_in_game() {
    let mut sim = sim();
    let p = player(&mut sim);
    let w = drawn(&mut sim, p, M92FS);
    sim.app.world_mut().get_mut::<Magazine>(w).unwrap().clip = 10;
    sim.ticks(2);
    sim.intent(p).reload = true;
    // The frame whose update starts `reload` still shows `idle_0`.
    assert!(until(&mut sim, 5, |s| playing(s, p).as_deref() == Some("reload")));
    sim.intent(p).reload = false;
    assert_eq!(shown(&sim, p).as_deref(), Some("idle_0"));
    sim.ticks(1);
    assert_eq!(shown(&sim, p).as_deref(), Some("reload"));
}

/// Grenade timings Q5: with grenades left, the throw's `postfire` is
/// followed by the 400 ms re-ready (`reload`), whose end fills the clip.
#[test]
fn grenade_re_ready_after_a_throw() {
    let mut sim = sim();
    let p = player(&mut sim);
    let w = drawn(&mut sim, p, M67);
    {
        let mut t = sim.app.world_mut().get_mut::<Throwable>(w).unwrap();
        t.count = 2;
        t.max = 2;
    }
    sim.intent(p).fire = true;
    sim.ticks(1);
    sim.intent(p).fire = false;
    assert!(until(&mut sim, 100, |s| playing(s, p).as_deref() == Some("postfire")));
    assert!(until(&mut sim, 60, |s| vm(s, p).thrown), "the grenade left the hand");
    assert!(until(&mut sim, 60, |s| playing(s, p).as_deref() == Some("reload")), "the re-ready");
    assert!(until(&mut sim, 60, |s| playing(s, p).is_some_and(|n| n.starts_with("idle_"))));
    assert!(!vm(&sim, p).thrown, "the clip filled");
    assert_eq!(sim.app.world().get::<Throwable>(w).unwrap().count, 1);
}

/// 9: no first-person flash with the camera in a liquid volume (the
/// M92FS has no `UseUWMuzzleFX`).
#[test]
fn no_muzzle_flash_under_water() {
    let mut sim = sim();
    let p = player(&mut sim);
    sim.app.world_mut().entity_mut(p).insert(LocalPlayer);
    drawn(&mut sim, p, M92FS);
    sim.app.world_mut().spawn(combat_arms::game::volumes::CaVolume::liquid_box(
        Vec3::splat(-1000.0),
        Vec3::splat(1000.0),
        0.0,
        Vec3::ZERO,
    ));
    sim.intent(p).fire = true;
    sim.ticks(1);
    sim.intent(p).fire = false;
    assert!(until(&mut sim, 5, |s| vm(s, p).flash.shot_at.is_some()), "it fired");
    for _ in 0..6 {
        assert!(!vm(&sim, p).flash.shown && vm(&sim, p).flash_at.is_none());
        sim.ticks(1);
    }
}
