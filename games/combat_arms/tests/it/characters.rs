//! specs/combat_arms/characters.md's test cases, one test each (`t01_` is
//! test case 1). The "install" cases measure the character archives,
//! which are V2-keyed here: they run on the install in `heavy::
//! characters` when a key is configured; here the same paths run on the
//! spec's own data (the measured skeleton as `skeleton::fallback`, the
//! spec's names and lengths as `driver::spec_library`) and on stand-in
//! models written in this file (nodes, sockets, weight sets and keyed
//! animations), so what the install would feed is checked end to end.
//!
//! The driver runs at the game's 15 ms tick unless a case says otherwise.

use std::sync::Arc;

use bevy::prelude::*;
use chimera_core::core::{Damage, DamageKind, Hitgroup, MovementState};
use chimera_sim::weapon::{self, Inventory, Magazine};
use chimera_world::{greybox::GreyboxMapPlugin, harness::Sim};
use combat_arms::{
    game::{
        TICK_INTERVAL,
        characters::{
            CaAnim, CaBody, CaPose,
            death::{self, DeathInputs, DeathKind},
            driver::{self, Driver, Inputs, Jump, Posture},
            models,
            records::{self, Selection},
            skeleton::{self, Posed, Rig},
        },
        inventory::{self, UNIT},
        movement,
        weapons::{CaArmor, FEMALE, MALE, RIFLE},
    },
    harness::CaSim,
};
use lithtech_formats::{
    anim::AnimList,
    ltb::{self, Animation, Channel, Key, Model, Node, Socket, WeightSet},
};

const TICK: u32 = 15;

fn close(a: Vec3, b: Vec3, tol: f32) -> bool {
    (a - b).abs().max_element() <= tol
}

// ---------------------------------------------------------------------
// Model choice (data)

#[test]
fn t01_body_by_face() {
    let d = records::data();
    let (r, figure) = d.effective(&Selection::default());
    assert_eq!((r.file.as_str(), r.skeleton, figure), ("CHARS_M_BODY\\CM_VST_NM_DF_Nuts_JS.ltb", 0, 0));
    let (r, figure) = d.effective(&Selection {
        face: 5,
        ..default()
    });
    assert_eq!((r.id, r.file.as_str(), r.skeleton, figure), (7, "CHARS_M_BODY\\Nomal_W_vest.ltb", 1, 1));
    let (r, _) = d.effective(&Selection {
        model: 2,
        face: 4,
        ..default()
    });
    assert_eq!((r.id, r.file.as_str()), (9, "CHARS_M_BODY\\Heavy_W_vest.ltb"));
    // Male faces keep the record.
    assert_eq!(d.effective(&Selection { face: 3, ..default() }).0.id, 0);
}

#[test]
fn t02_defaults() {
    let d = records::data();
    for model in [-1, 3, 999] {
        assert_eq!(d.effective(&Selection { model, ..default() }).0.id, 0, "model {model}");
    }
    assert_eq!(d.head_file(0, 0), None);
    assert_eq!(d.head_file(0, 1), None);
}

#[test]
fn t03_camouflage_skins() {
    let d = records::data();
    let m = d.model(0).unwrap();
    assert_eq!(
        d.skins(m, 3),
        (Some("CHARS_T_BODY\\Nomal_vest_body_UR.dtx"), Some("CHARS_T_BODY\\Nomal_vest_leg_UR.dtx"))
    );
    assert_eq!(d.skins(m, 9), (Some("CHARS_T_BODY\\Nomal_vest_body.dtx"), Some("CHARS_T_BODY\\Nomal_vest_leg.dtx")));
    // The skin families: camouflage by two, the mark's bit.
    assert_eq!(models::family(3, true), 7);
    assert_eq!(models::family(9, false), 0);
}

#[test]
fn t04_team_marks() {
    let d = records::data();
    assert_eq!(d.team_mark(true, false), "CHARS_T_BODY\\RED_Tie.dtx");
    assert_eq!(d.team_mark(false, false), "CHARS_T_BODY\\Alpha.dtx");
    assert_eq!(d.team_mark(true, true), "CHARS_T_BODY\\Alpha.dtx");
}

#[test]
fn t05_head_pieces_and_goggles() {
    let d = records::data();
    assert_eq!(d.head_file(2, 0), Some("CHARS_M_HEAD\\Combat_Helmet.ltb"));
    assert_eq!(d.head_file(2, 1), Some("CHARS_M_HEAD\\W_Combat_Helmet.ltb"));
    assert_eq!(d.goggle_file(0, 1), Some(("Eyes", "ATTACH_M\\W_Glass.ltb")));
}

// ---------------------------------------------------------------------
// State -> animation, on the spec's names and lengths.

fn drv() -> Driver {
    Driver::new(driver::spec_library(false))
}

fn at(d: &Driver) -> (String, String, String) {
    (d.name_of(&d.upper).into(), d.name_of(&d.lower).into(), d.name_of(&d.crouch).into())
}

fn length(d: &Driver, t: &lithtech_formats::anim::Tracker) -> u32 {
    d.list.entries[t.current].length()
}

fn rifle() -> Inputs {
    Inputs {
        class: 0,
        ..default()
    }
}

fn run(d: &mut Driver, i: &Inputs, n: usize) {
    for _ in 0..n {
        d.update(TICK, i);
    }
}

#[test]
fn t06_standing_still() {
    let mut d = drv();
    let i = Inputs {
        crouch: 0.25,
        ..rifle()
    };
    run(&mut d, &i, 3);
    assert_eq!(at(&d), ("URFRAmS".into(), "LST".into(), "LC".into()));
    assert!(d.upper.looping && d.lower.looping);
    assert_eq!((d.upper.rate, d.lower.rate), (1.0, 1.0));
    assert_eq!(d.crouch.multiplier, 0.25);
}

#[test]
fn t07_walking() {
    let mut d = drv();
    run(&mut d, &Inputs { run: false, forward: true, ..rifle() }, 3);
    assert_eq!(at(&d).0, "URFWAmF");
    assert_eq!(at(&d).1, "LWF");
    let mut d = drv();
    run(&mut d, &Inputs { run: false, ..rifle() }, 3);
    assert_eq!(at(&d).0, "URFRAmS", "the walk name is missing");
}

#[test]
fn t08_running_directions() {
    for (keys, lower, upper) in [
        ((true, false, true, false), "LRFL", "URFRAmF"),
        ((false, true, false, true), "LRBR", "URFRAmB"),
        ((false, false, false, true), "LRR", "URFRAmR"),
    ] {
        let mut d = drv();
        let (forward, back, left, right) = keys;
        run(&mut d, &Inputs { forward, back, left, right, ..rifle() }, 2);
        assert_eq!((at(&d).1.as_str(), at(&d).0.as_str()), (lower, upper));
    }
    let mut d = drv();
    run(&mut d, &Inputs { forward: true, left: true, ..rifle() }, 1);
    assert_eq!(length(&d, &d.lower), 600);
}

#[test]
fn t09_sprint() {
    let mut d = drv();
    run(&mut d, &Inputs { sprint: true, forward: true, ..rifle() }, 2);
    assert_eq!((at(&d).1.as_str(), at(&d).0.as_str()), ("LRRF", "URFRRAmF"));
    assert_eq!(length(&d, &d.lower), 500);
    let mut d = drv();
    run(&mut d, &Inputs { sprint: true, forward: true, right: true, ..rifle() }, 2);
    assert_eq!(at(&d).1, "LRRR");
}

#[test]
fn t10_crouched() {
    let mut d = drv();
    run(&mut d, &Inputs { crouch: 1.0, crouch_key: true, run: false, ..rifle() }, 2);
    assert_eq!(at(&d), ("URFRAmS".into(), "LST".into(), "LC".into()));
    assert_eq!(d.crouch.multiplier, 1.0);
    let mut d = drv();
    run(&mut d, &Inputs { crouch: 1.0, crouch_key: true, run: false, forward: true, ..rifle() }, 2);
    assert_eq!(at(&d), ("URFWAmF".into(), "LWF".into(), "LCF".into()));
    assert_eq!(length(&d, &d.crouch), 1400);
}

#[test]
fn t11_jumps() {
    let mut d = drv();
    run(&mut d, &rifle(), 2);
    d.update(TICK, &Inputs { jumped: true, ..rifle() });
    assert_eq!(at(&d).1, "LSJ");
    assert!(!d.lower.looping);
    assert_eq!(length(&d, &d.lower), 666);
    // 666 ms in: still playing; done after 667.
    d.update(666, &rifle());
    assert_eq!(at(&d).1, "LSJ");
    assert!(!d.lower.done(&d.list));
    d.update(1, &rifle());
    assert_eq!(d.jump, Jump::Falling);
    assert_eq!(at(&d).1, "LST");
    // From running forward.
    let mut d = drv();
    let fwd = Inputs { forward: true, ..rifle() };
    run(&mut d, &fwd, 2);
    d.update(TICK, &Inputs { jumped: true, ..fwd.clone() });
    assert_eq!(at(&d).1, "LJJ");
    // Sprinting: the run jump, never LRRJF.
    let mut d = drv();
    let sprint = Inputs { sprint: true, forward: true, ..rifle() };
    run(&mut d, &sprint, 2);
    d.update(TICK, &Inputs { jumped: true, ..sprint.clone() });
    assert_eq!(at(&d).1, "LJJ");
    // Landing clears the jump.
    d.update(TICK, &Inputs { landed: true, ..sprint.clone() });
    assert_eq!(d.jump, Jump::None);
}

#[test]
fn t12_firing() {
    let mut d = drv();
    run(&mut d, &rifle(), 2);
    let fire = Inputs { fire: true, ..rifle() };
    d.update(TICK, &fire);
    assert_eq!(at(&d).0, "URFRFiS");
    assert!(!d.upper.looping);
    assert_eq!(length(&d, &d.upper), 100);
    // Restarts each time it is done while fire stays held: done after
    // 105 ms of 15 ms ticks, restarted in that update.
    let mut restarts = Vec::new();
    for k in 1..=30 {
        let before = d.upper.time;
        d.update(TICK, &fire);
        if d.upper.time < before {
            restarts.push(k);
        }
        assert_eq!(at(&d).0, "URFRFiS");
    }
    assert_eq!(&restarts[..3], &[7, 14, 21], "{restarts:?}");
    // Running forward: no moving rifle fire in the library, the aim plays.
    let mut d = drv();
    let fwd = Inputs { forward: true, ..rifle() };
    run(&mut d, &fwd, 2);
    run(&mut d, &Inputs { fire: true, ..fwd.clone() }, 10);
    assert_eq!(at(&d).0, "URFRAmF");
    assert_eq!(d.last.posture, Posture::Fire);
}

#[test]
fn t13_reload_and_draw() {
    let mut d = drv();
    run(&mut d, &rifle(), 2);
    d.update(TICK, &Inputs { reloading: true, ..rifle() });
    assert_eq!(at(&d).0, "URFRSReS");
    assert_eq!(length(&d, &d.upper), 1866);
    // Started while moving: kept when the player stops.
    let mut d = drv();
    let fwd = Inputs { forward: true, ..rifle() };
    run(&mut d, &fwd, 2);
    d.update(TICK, &Inputs { reloading: true, ..fwd.clone() });
    assert_eq!(at(&d).0, "URFRReS");
    run(&mut d, &Inputs { reloading: true, ..rifle() }, 5);
    assert_eq!(at(&d).0, "URFRReS");
    // Draws.
    for (moving, sprint, want, len) in [(false, false, "URFRSSeS", Some(733)), (true, false, "URFRSeS", None), (true, true, "URFRRSeS", None)] {
        let mut d = drv();
        let base = Inputs { forward: moving, sprint, ..rifle() };
        run(&mut d, &base, 2);
        d.update(TICK, &Inputs { status: Some(driver::Status::Select), ..base.clone() });
        assert_eq!(at(&d).0, want);
        if let Some(l) = len {
            assert_eq!(length(&d, &d.upper), l);
        }
    }
}

#[test]
fn t14_pistol_and_knife() {
    let mut d = drv();
    let pistol = Inputs { class: 10, forward: true, ..default() };
    run(&mut d, &pistol, 2);
    d.update(TICK, &Inputs { fire: true, ..pistol.clone() });
    assert_eq!(at(&d).0, "UPRFiF");
    for keys in [(false, false, false, false), (true, false, false, false), (false, true, true, false), (false, false, false, true)] {
        let mut d = drv();
        let (forward, back, left, right) = keys;
        let knife = Inputs { class: 14, forward, back, left, right, ..default() };
        run(&mut d, &knife, 2);
        d.update(TICK, &Inputs { fire: true, ..knife.clone() });
        assert_eq!(at(&d).0, "UKRFiS", "{keys:?}");
        assert_eq!(length(&d, &d.upper), 500);
    }
    let mut d = drv();
    let knife = Inputs { class: 14, ..default() };
    run(&mut d, &knife, 2);
    d.update(TICK, &Inputs { alt_fire: true, ..knife.clone() });
    assert_eq!(at(&d).0, "UKRFiS2");
    assert_eq!(length(&d, &d.upper), 800);
}

#[test]
fn t15_grenade_throw() {
    let grenade = Inputs { class: 11, ..default() };
    let held = Inputs { fire: true, ..grenade.clone() };
    let mut d = drv();
    run(&mut d, &grenade, 2);
    d.update(TICK, &held);
    assert_eq!(at(&d).0, "UTRFiS_A");
    assert_eq!(length(&d, &d.upper), 267);
    // 267 ms later (done at 268): the 1 ms hold.
    let mut n = 0;
    while at(&d).0 == "UTRFiS_A" {
        d.update(TICK, &held);
        n += 1;
    }
    assert_eq!((at(&d).0.as_str(), n), ("UTRFiS_B", 18));
    run(&mut d, &held, 10);
    assert_eq!(at(&d).0, "UTRFiS_B", "held: never restarted");
    d.update(TICK, &grenade);
    assert_eq!(at(&d).0, "UTRFiS_C");
    assert_eq!(length(&d, &d.upper), 900);
    run(&mut d, &grenade, 59);
    assert_eq!(at(&d).0, "UTRFiS_C");
    run(&mut d, &grenade, 2);
    assert_eq!(at(&d).0, "UTRAmS");
    // The next grenade drawn meanwhile: its select.
    let mut d2 = drv();
    run(&mut d2, &grenade, 2);
    d2.update(TICK, &held);
    while at(&d2).0 != "UTRFiS_C" {
        d2.update(TICK, if at(&d2).0 == "UTRFiS_B" { &grenade } else { &held });
    }
    run(&mut d2, &grenade, 60);
    d2.update(TICK, &Inputs { status: Some(driver::Status::Select), ..grenade.clone() });
    assert_eq!(at(&d2).0, "UTRSSeS");
}

#[test]
fn t16_hit_reactions() {
    for (coin, name, len) in [(true, "D_HIT", 467), (false, "D_HITT", 400)] {
        let mut d = drv();
        run(&mut d, &rifle(), 2);
        let hit = Inputs { status: Some(driver::Status::Hit), damaged: true, coin, ..rifle() };
        d.update(TICK, &hit);
        assert_eq!(at(&d).0, name);
        assert_eq!(length(&d, &d.upper), len);
        assert_eq!(d.list.entries[d.upper.current].animation.interp_ms, 200);
        // Fire held meanwhile: no fire animation until the hit is done
        // (200 ms blend, then its length).
        let fire = Inputs { fire: true, damaged: true, ..rifle() };
        let mut ms = TICK;
        while at(&d).0 == name {
            d.update(TICK, &fire);
            ms += TICK;
        }
        assert_eq!(at(&d).0, "URFRFiS");
        assert!(ms > 200 + len, "{ms}");
    }
    // Spawn-protected: no reaction, the hit status sticks (Quirk 4).
    let mut d = drv();
    run(&mut d, &rifle(), 2);
    d.update(TICK, &Inputs { status: Some(driver::Status::Hit), damaged: true, protected: true, ..rifle() });
    run(&mut d, &Inputs { fire: true, ..rifle() }, 5);
    assert_eq!(at(&d).0, "URFRAmS");
}

#[test]
fn t17_rates() {
    assert_eq!(driver::upper_rate(1866, 1.0, 0), 1.0);
    assert!((driver::upper_rate(1866, 1.0, 2300) - 0.8113).abs() < 1e-4);
    // The reload at HHAniFrame_5 = 2300 lasts 2300 ms.
    let mut d = drv();
    run(&mut d, &rifle(), 2);
    let mut frames = vec![0; 10];
    frames[5] = 2300;
    let reload = Inputs { reloading: true, frames, ..rifle() };
    d.update(TICK, &reload);
    assert!((d.upper.rate - 0.8113).abs() < 1e-4);
    let mut ms = 0;
    // Done (and the aim back) once its 1866 ms ran at that rate.
    while d.name_of(&d.upper) == "URFRSReS" && ms < 10_000 {
        d.update(TICK, &reload);
        ms += TICK;
    }
    assert!((2295..=2320).contains(&ms), "{ms}");
    // The legs at 1.0 whatever the movement.
    let mut d = drv();
    run(&mut d, &Inputs { forward: true, sprint: true, ..rifle() }, 3);
    assert_eq!((d.lower.rate, d.crouch.rate), (1.0, 1.0));
}

// ---------------------------------------------------------------------
// Aim

/// The aim nodes in a chain up the body, each 10 units above the last,
/// a hand 20 units out from each shoulder.
fn aim_chain() -> Posed {
    let names = [
        ("Right_pelvis", None, [0.0, 0.0, 0.0]),
        ("Torso", Some(0), [0.0, 10.0, 0.0]),
        ("Upper_torso", Some(1), [0.0, 20.0, 0.0]),
        ("Neck", Some(2), [0.0, 30.0, 0.0]),
        ("Head", Some(3), [0.0, 35.0, 0.0]),
        ("Left_shoulder", Some(3), [-5.0, 28.0, 0.0]),
        ("Right_shoulder", Some(3), [5.0, 28.0, 0.0]),
        ("Right_hand", Some(6), [5.0, 28.0, 20.0]),
        ("Face", Some(4), [0.0, 40.0, 5.0]),
    ];
    Posed {
        names: names.iter().map(|n| n.0.to_string()).collect(),
        parents: names.iter().map(|n| n.1).collect(),
        globals: names.iter().map(|n| (Vec3::from(n.2), Quat::IDENTITY)).collect(),
        sockets: Vec::new(),
        feet: -50.0,
    }
}

fn male_aim() -> Vec<String> {
    records::data().skeleton(0).aim.clone()
}

fn angle_of(p: &Posed, name: &str) -> f32 {
    p.globals[p.node(name).unwrap()].1.angle_between(Quat::IDENTITY).to_degrees()
}

#[test]
fn t18_twist_shares() {
    let mut p = aim_chain();
    p.aim(&male_aim(), 0.0, 45f32.to_radians(), false);
    assert!((angle_of(&p, "Torso") - 18.53).abs() < 0.01);
    assert!((angle_of(&p, "Upper_torso") - 45.0).abs() < 0.01);
    for n in ["Neck", "Head", "Right_shoulder", "Right_hand"] {
        assert!((angle_of(&p, n) - 45.0).abs() < 0.01, "{n} adds nothing");
    }
    assert_eq!(angle_of(&p, "Right_pelvis"), 0.0);
}

#[test]
fn t19_pitch_shares() {
    for (deg, arm, head, server_arm) in [(-20.0f32, 20.0, 21.0, 17.0), (20.0, 20.0, 25.0, 13.0)] {
        let mut p = aim_chain();
        p.aim(&male_aim(), deg.to_radians(), 0.0, false);
        assert!((angle_of(&p, "Right_hand") - arm).abs() < 0.01, "arms {deg}");
        assert!((angle_of(&p, "Face") - head).abs() < 0.01, "head {deg}");
        let mut p = aim_chain();
        p.aim(&male_aim(), deg.to_radians(), 0.0, true);
        assert!((angle_of(&p, "Right_hand") - server_arm).abs() < 0.01, "server arms {deg}");
        // Down bows forward (+Z in the spec's frame).
        let hand = p.globals[p.node("Right_hand").unwrap()].0;
        assert_eq!(hand.y < 28.0, deg > 0.0);
    }
}

#[test]
fn t20_clamp_and_bytes() {
    let mut a = aim_chain();
    let mut b = aim_chain();
    a.aim(&male_aim(), 60f32.to_radians(), 0.0, false);
    b.aim(&male_aim(), 45f32.to_radians(), 0.0, false);
    assert_eq!(a.globals, b.globals);
    assert_eq!(skeleton::aim_byte(30f32.to_radians()), 42);
    assert!((skeleton::from_aim_byte(42).to_degrees() - 29.764).abs() < 1e-3);
    assert_eq!(skeleton::aim_byte(-45f32.to_radians()), -63);
    assert!((skeleton::from_aim_byte(-63).to_degrees() + 44.646).abs() < 1e-3);
}

/// Cases 21 and 22 measure the install's poses: `heavy::characters`. On
/// the spec's skeleton the head and name tag pitch about the neck chain.
#[test]
fn t21_22_aim_moves_the_spec_skeleton() {
    for skel in [0, 1] {
        let s = records::data().skeleton(skel);
        let rest = skeleton::fallback(s, 1.0);
        let head = if skel == 0 { "Head" } else { "Bip01 Head" };
        let mut down = rest.clone();
        down.aim(&s.aim, 30f32.to_radians(), 0.0, false);
        let (a, b) = (rest.sphere(head).unwrap(), down.sphere(head).unwrap());
        assert!(b.y < a.y && b.z > a.z, "{a} -> {b}");
        let mut up = rest.clone();
        up.aim(&s.aim, -30f32.to_radians(), 0.0, false);
        assert!(up.sphere(head).unwrap().y > a.y - 1e-3);
        // Legs never move.
        assert_eq!(down.sphere("Left_foot"), rest.sphere("Left_foot"));
    }
}

// ---------------------------------------------------------------------
// Stand-in models: nodes at given bind positions (identity rotations),
// sockets, the two weight sets, constant animations of given lengths.

struct StandIn {
    nodes: Vec<(&'static str, Option<usize>, [f32; 3])>,
    sockets: Vec<(&'static str, usize, [f32; 3], [f32; 4])>,
}

fn translation(p: [f32; 3]) -> ltb::Mat4 {
    [[1.0, 0.0, 0.0, p[0]], [0.0, 1.0, 0.0, p[1]], [0.0, 0.0, 1.0, p[2]], [0.0, 0.0, 0.0, 1.0]]
}

impl StandIn {
    /// The model with `anims` (name, length ms, root translation).
    fn model(&self, anims: &[(&str, u32)]) -> Model {
        let n = self.nodes.len();
        let local = |j: usize| {
            let (_, parent, p) = self.nodes[j];
            let base = parent.map_or([0.0; 3], |q| self.nodes[q].2);
            [p[0] - base[0], p[1] - base[1], p[2] - base[2]]
        };
        Model {
            version: 25,
            counts: Default::default(),
            command: String::new(),
            radius: 50.0,
            pieces: Vec::new(),
            nodes: self
                .nodes
                .iter()
                .enumerate()
                .map(|(j, (name, parent, p))| Node {
                    name: name.to_string(),
                    stored_index: j as u16,
                    flags: 0,
                    bind: translation(*p),
                    parent: *parent,
                })
                .collect(),
            weight_sets: vec![
                WeightSet {
                    name: "upper".into(),
                    weights: vec![1.0; n],
                },
                WeightSet {
                    name: "Lower".into(),
                    weights: vec![0.0; n],
                },
            ],
            children: Vec::new(),
            animations: anims
                .iter()
                .map(|(name, len)| Animation {
                    dims: if name.starts_with("LC") { [15.0, 40.0, 15.0] } else { [15.0, 50.0, 15.0] },
                    name: name.to_string(),
                    compression: 0,
                    interp_ms: 0,
                    keys: vec![
                        Key {
                            time: 0,
                            string: String::new(),
                        },
                        Key {
                            time: *len,
                            string: String::new(),
                        },
                    ],
                    channels: (0..n)
                        .map(|j| Channel::Transform {
                            positions: vec![local(j)],
                            rotations: vec![[0.0, 0.0, 0.0, 1.0]],
                        })
                        .collect(),
                })
                .collect(),
            sockets: self
                .sockets
                .iter()
                .map(|(name, node, p, q)| Socket {
                    node: *node as u32,
                    name: name.to_string(),
                    rotation: *q,
                    position: *p,
                    scale: [1.0; 3],
                })
                .collect(),
            bindings: Vec::new(),
        }
    }

    fn rig(&self, anims: &[(&str, u32)]) -> Rig {
        let model = self.model(anims);
        let list = AnimList::build(&model, &[]);
        Rig::new(model, list)
    }
}

/// The default male's skeleton by the spec's bind positions: every node
/// sphere's node, the socket spheres' parents, and the sockets.
fn male_stand_in(with_nuts: bool) -> StandIn {
    let s = records::data().skeleton(0);
    let mut nodes: Vec<(&'static str, Option<usize>, [f32; 3])> = Vec::new();
    for n in &s.nodes {
        let parent = n.parent.as_ref().and_then(|p| nodes.iter().position(|m| m.0 == p.as_str()));
        let bind = s.sphere(&n.name).and_then(|sp| sp.bind).unwrap_or(n.at.unwrap_or_default());
        nodes.push((n.name.clone().leak(), parent, bind));
    }
    let mut sockets = Vec::new();
    for sp in &s.spheres {
        if let Some(place) = &sp.socket {
            if sp.name == "Nuts" && !with_nuts {
                continue;
            }
            let parent = nodes.iter().position(|m| m.0 == place.parent.as_str()).unwrap();
            sockets.push((sp.name.clone().leak() as &'static str, parent, place.offset, [0.0, 0.0, 0.0, 1.0]));
        }
    }
    for so in &s.sockets {
        let parent = nodes.iter().position(|m| m.0 == so.parent.as_str()).unwrap();
        sockets.push((so.name.clone().leak() as &'static str, parent, so.offset, so.rotation));
    }
    StandIn { nodes, sockets }
}

const STAND_IN_ANIMS: &[(&str, u32)] = &[("baseAnim", 4800), ("URFRAmS", 4000), ("LST", 4000), ("LC", 4000), ("LRF", 600), ("DT", 3033)];

// ---------------------------------------------------------------------
// Sockets

#[test]
fn t23_right_hand_socket() {
    let s = records::data().skeleton(0).socket("RightHand").unwrap();
    assert_eq!((s.parent.as_str(), s.offset, s.rotation), ("Right_hand", [7.085, 1.661, 1.277], [0.0573, -0.7151, -0.0818, 0.6918]));
    // On a stand-in, the socket is its node's transform times the offset.
    let rig = male_stand_in(true).rig(STAND_IN_ANIMS);
    let bind = rig.bind();
    let hand = bind.globals[bind.node("Right_hand").unwrap()].0;
    let (p, _) = bind.socket("RightHand").unwrap();
    assert!(close(p, hand + Vec3::from(s.offset), 1e-4));
    // And with the node turned: the offset turns with it.
    let mut turned = bind.clone();
    let j = turned.node("Right_hand").unwrap();
    turned.globals[j].1 = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    let (p, _) = turned.socket("RightHand").unwrap();
    assert!(close(p, hand + Vec3::new(1.277, 1.661, -7.085), 1e-4), "{p}");
}

/// Case 24's "stand" pose on a player: the tag sits at P + (8.63, 45.87,
/// 11.51) turned by its yaw (our frame: Z mirrored).
#[test]
fn t24_name_socket() {
    let s = records::data().skeleton(0).socket("NAME_socket").unwrap();
    assert_eq!((s.parent.as_str(), s.offset), ("Head", [16.5, 0.0, 0.906]));
    assert_eq!(records::data().skeleton(1).socket("NAME_socket").unwrap().parent, "Bip01 Head");
    let mut sim = sim();
    let yaw = 0.7;
    let p = player(&mut sim, Vec3::new(0.0, 1.0, 12.0), yaw);
    let world = sim.app.world();
    let pose = world.get::<CaPose>(p).unwrap();
    let (t, state) = (world.get::<Transform>(p).unwrap(), world.get::<MovementState>(p).unwrap());
    let tag = pose.world(t, state, UNIT, pose.name_tag.unwrap());
    let want = t.translation + Quat::from_rotation_y(yaw) * (Vec3::new(8.63, 45.87, -11.51) * UNIT);
    assert!(close(tag, want, 1e-4), "{tag} vs {want}");
}

#[test]
#[allow(clippy::approx_constant)] // the data's quaternions as stored
fn t25_26_sockets() {
    let m = records::data().skeleton(0);
    let f = records::data().skeleton(1);
    let eyes = m.socket("Eyes").unwrap();
    assert_eq!((eyes.parent.as_str(), eyes.offset, eyes.bind), ("Face_Eyelid", [3.011, 0.0, 1.314], [0.03, 41.38, 5.68]));
    let eyes = f.socket("Eyes").unwrap();
    assert_eq!((eyes.parent.as_str(), eyes.offset, eyes.bind), ("Bip01 Head", [-85.255, 0.001, 18.748], [0.08, 2.14, 5.20]));
    let back = m.socket("1W_N").unwrap();
    assert_eq!((back.parent.as_str(), back.offset, back.rotation), ("Upper_torso", [-18.328, 0.0, 0.0], [0.0, 0.0, -0.7071, 0.7071]));
    let left = m.socket("LeftHand").unwrap();
    assert_eq!((left.parent.as_str(), left.offset, left.bind), ("left_hand", [0.0, 0.0, 0.0], [-31.19, 4.32, 2.14]));
    // On the stand-in: the left hand's socket is its node; the backpack's
    // offset turns by the node's turn and its own rotation composes.
    let rig = male_stand_in(true).rig(STAND_IN_ANIMS);
    let bind = rig.bind();
    let (lh, q) = bind.socket("LeftHand").unwrap();
    assert!(close(lh, Vec3::new(-31.19, 4.32, 2.14), 1e-4));
    assert!(q.angle_between(Quat::IDENTITY) < 1e-4, "(0, 0, 0, 0.7071) normalised is identity");
}

/// Case 27: the death drop at the left foot, as the stand pose has it.
#[test]
fn t27_death_drop_at_the_left_foot() {
    for (face, foot) in [(0, Vec3::new(-10.02, -41.49, 13.52)), (5, Vec3::new(-7.73, -39.56, 12.91))] {
        let mut sim = sim();
        let p = player(&mut sim, Vec3::new(0.0, 1.0, 12.0), 0.0);
        sim.app.world_mut().entity_mut(p).insert(Selection { face, ..default() });
        let w = weapon::give(sim.app.world_mut(), p, RIFLE).unwrap();
        sim.seconds(1.3);
        assert_eq!(sim.app.world().get::<Inventory>(p).unwrap().active, Some(w));
        assert_eq!(sim.app.world().get::<CaAnim>(p).unwrap().skeleton, usize::from(face == 5));
        let centre = sim.position(p);
        kill(&mut sim, p, Vec3::NEG_Z, Hitgroup::Chest, DamageKind::Bullet);
        sim.ticks(2);
        let world = sim.app.world_mut();
        let mut q = world.query::<(&Transform, &inventory::Dropped)>();
        let (t, _) = q.single(world).expect("the rifle dropped");
        let want = centre + Vec3::new(foot.x, foot.y, -foot.z) * UNIT;
        assert!(close(t.translation, want, 1e-3), "face {face}: {} vs {want}", t.translation);
    }
}

// ---------------------------------------------------------------------
// Hit spheres

#[test]
fn t28_bind_centres_on_a_stand_in() {
    let rig = male_stand_in(true).rig(STAND_IN_ANIMS);
    let bind = rig.bind();
    for s in &records::data().skeleton(0).spheres {
        if s.radius <= 0.0 || s.absent || s.socket.is_some() {
            continue;
        }
        let at = bind.sphere(&s.name).unwrap_or_else(|| panic!("{}", s.name));
        assert!(close(at, Vec3::from(s.bind.unwrap()), 1e-3), "{}: {at}", s.name);
    }
    // Socket spheres: their node's position plus the offset (identity
    // nodes here; the install's rotations in heavy::characters).
    let nuts = bind.sphere("Nuts").unwrap();
    assert!(close(nuts, Vec3::new(0.0, -7.70, 0.75) + Vec3::new(2.0, 0.0, 6.0), 1e-3));
    let t = MALE.iter().find(|s| s.name == "Upper_torso").unwrap();
    assert_eq!(t.radius, 10.0);
}

/// Case 29's poses (LST at t = 0) are the spec's skeleton: a player at
/// rest has every sphere there.
#[test]
fn t29_lst_centres() {
    let mut sim = sim();
    let p = player(&mut sim, Vec3::new(0.0, 1.0, 12.0), 0.0);
    let world = sim.app.world();
    let pose = world.get::<CaPose>(p).unwrap();
    let skel = records::data().skeleton(0);
    for (s, at) in skel.spheres.iter().zip(&pose.spheres) {
        match (s.at, at) {
            (Some(want), Some(got)) if s.radius > 0.0 => {
                let want = Vec3::new(want[0], want[1] + 50.0, -want[2]);
                assert!(close(*got, want, 1e-3), "{}: {got} vs {want}", s.name);
            }
            _ => assert!(s.radius <= 0.0 || s.absent || at.is_none(), "{}", s.name),
        }
    }
    let head = pose.spheres[skel.spheres.iter().position(|s| s.name == "Head").unwrap()].unwrap();
    assert!(close(head, Vec3::new(6.45, 79.03, -10.90), 1e-3));
}

/// Case 30 (`LRF` at 300 ms) is an install case: on a stand-in whose
/// `LRF` moves the left foot, the sphere follows the animation's time.
#[test]
fn t30_spheres_follow_the_animation() {
    let stand_in = male_stand_in(true);
    let mut model = stand_in.model(STAND_IN_ANIMS);
    let foot = model.node("Left_foot").unwrap();
    let lrf = model.animation("LRF").unwrap();
    let parent = stand_in.nodes[foot].1.unwrap();
    let base = Vec3::from(stand_in.nodes[foot].2) - Vec3::from(stand_in.nodes[parent].2);
    model.animations[lrf].channels[foot] = Channel::Transform {
        positions: vec![base.to_array(), (base + Vec3::new(0.0, 0.0, 20.0)).to_array()],
        rotations: vec![[0.0, 0.0, 0.0, 1.0]],
    };
    // Legs from the lower tracker: a Lower weight on the leg.
    for (j, w) in model.weight_sets[1].weights.iter_mut().enumerate() {
        *w = if j == foot { 1.0 } else { 0.0 };
    }
    let list = AnimList::build(&model, &[]);
    let rig = Rig::new(model, list);
    let mut d = Driver::new(Arc::new(rig.list.clone()));
    let fwd = Inputs { forward: true, ..rifle() };
    d.update(0, &fwd);
    assert_eq!(d.name_of(&d.lower), "LRF");
    let base_pose = rig.posed(&rig.layered(&d.main, &d.upper, &d.lower, &d.crouch, 0.0), 0.0);
    d.update(300, &fwd);
    let later = rig.posed(&rig.layered(&d.main, &d.upper, &d.lower, &d.crouch, 0.0), 0.0);
    let moved = later.sphere("Left_foot").unwrap() - base_pose.sphere("Left_foot").unwrap();
    assert!(close(moved, Vec3::new(0.0, 0.0, 10.0), 1e-3), "{moved}");
}

/// Case 31 (male stand and crouch) is an install case; on the spec's
/// skeleton crouching lowers the head with the hull.
#[test]
fn t31_crouch_lowers_the_spheres() {
    let mut sim = sim();
    let p = player(&mut sim, Vec3::new(0.0, 1.0, 12.0), 0.0);
    let skel = records::data().skeleton(0);
    let head = skel.spheres.iter().position(|s| s.name == "Head").unwrap();
    let stand = sim.app.world().get::<CaPose>(p).unwrap().spheres[head].unwrap();
    sim.intent(p).crouch = true;
    sim.seconds(1.0);
    let crouched = sim.app.world().get::<CaPose>(p).unwrap().spheres[head].unwrap();
    let state = sim.state(p);
    let ratio = (state.hull_max.y - state.hull_min.y) / (100.0 * UNIT);
    assert!(ratio < 0.9, "crouched: {ratio}");
    assert!((crouched.y - stand.y * ratio).abs() < 1e-3, "{stand} -> {crouched}");
}

/// Case 32 (female bind and LST) on the spec's data, case 33 (female
/// crouch) in heavy::characters.
#[test]
fn t32_female_table() {
    let mut sim = sim();
    let p = player(&mut sim, Vec3::new(0.0, 1.0, 12.0), 0.0);
    sim.app.world_mut().entity_mut(p).insert(Selection { face: 6, ..default() });
    sim.ticks(2);
    let world = sim.app.world();
    let pose = world.get::<CaPose>(p).unwrap();
    assert_eq!(pose.skeleton, 1);
    let skel = records::data().skeleton(1);
    let i = skel.spheres.iter().position(|s| s.name == "Headfuck").unwrap();
    assert!(close(pose.spheres[i].unwrap(), Vec3::new(4.12, 83.37, -12.42), 1e-3));
    let i = skel.spheres.iter().position(|s| s.name == "Bip01 Head").unwrap();
    assert!(close(pose.spheres[i].unwrap(), Vec3::new(2.96, 81.08, -10.21), 1e-3));
    assert_eq!(skel.sphere("Headfuck").unwrap().bind, Some([0.08, 92.36, 3.53]));
    let hs = world.get::<combat_arms::game::weapons::HitSpheres>(p).unwrap();
    assert!(std::ptr::eq(hs.0, *FEMALE));
}

#[test]
fn t34_sphere_counts() {
    let count = |table: &[combat_arms::game::weapons::HitSphere]| table.iter().filter(|s| s.radius > 0.0).count();
    let d = records::data();
    assert_eq!(d.skeleton(0).spheres.iter().filter(|s| s.radius > 0.0).count(), 35);
    assert_eq!(count(&MALE), 33, "Left_Shld and Right_Shld never exist");
    assert_eq!(d.skeleton(1).spheres.iter().filter(|s| s.radius > 0.0).count(), 35);
    assert_eq!(count(&FEMALE), 35);
    // A stand-in with the default male's nodes and sockets: 33 found.
    let bind = male_stand_in(true).rig(STAND_IN_ANIMS).bind();
    let found = d.skeleton(0).spheres.iter().filter(|s| s.radius > 0.0 && bind.sphere(&s.name).is_some()).count();
    assert_eq!(found, 33);
}

#[test]
fn t35_no_groin_without_the_socket() {
    let bind = male_stand_in(false).rig(STAND_IN_ANIMS).bind();
    assert!(bind.sphere("Nuts").is_none());
    assert!(bind.sphere("Torso").is_some());
}

// ---------------------------------------------------------------------
// Death and bodies

fn death_of(kind: DeathKind, dir: Vec3, head: bool, groin: bool, coin: bool, female: bool) -> String {
    let list = driver::spec_library(female);
    death::choose(
        &DeathInputs {
            kind,
            dir,
            forward: Vec3::Z,
            right: Vec3::X,
            head,
            groin,
            coin,
            jumping: false,
            special: None,
        },
        |n| list.find(n).is_some(),
    )
}

#[test]
fn t36_front_deaths() {
    let d = Vec3::NEG_Z;
    assert_eq!(death_of(DeathKind::Other, d, true, false, true, false), "DH");
    assert_eq!(death_of(DeathKind::Other, d, true, false, false, false), "DH_1");
    assert_eq!(death_of(DeathKind::Other, d, false, false, true, false), "DT");
    assert_eq!(death_of(DeathKind::Other, d, false, false, false, false), "DT_1");
}

#[test]
fn t37_back_deaths() {
    let d = Vec3::Z;
    assert_eq!(death_of(DeathKind::Other, d, false, false, true, false), "DTB");
    assert_eq!(death_of(DeathKind::Other, d, false, false, false, false), "DTB_1");
    assert_eq!(death_of(DeathKind::Other, d, true, false, true, false), "DHB");
    assert_eq!(death_of(DeathKind::Other, d, true, false, false, false), "DHB_1");
}

#[test]
fn t38_side_deaths() {
    let a = Vec3::new(0.94, 0.0, -0.34);
    assert_eq!(death_of(DeathKind::Other, a, false, false, true, false), "DLSH");
    assert_eq!(death_of(DeathKind::Other, a, true, false, true, false), "DLSH");
    let b = Vec3::new(0.94, 0.0, 0.34);
    assert_eq!(death_of(DeathKind::Other, b, false, false, true, false), "DLT");
    assert_eq!(death_of(DeathKind::Other, b, true, false, true, false), "DLSH");
    let c = Vec3::new(-0.94, 0.0, -0.34);
    assert_eq!(death_of(DeathKind::Other, c, false, false, true, false), "DRT");
    assert_eq!(death_of(DeathKind::Other, c, true, false, true, false), "DRSH");
}

#[test]
fn t39_other_deaths() {
    let d = Vec3::NEG_Z;
    assert_eq!(death_of(DeathKind::Explosion, d, false, false, true, false), "D_BO_F");
    assert_eq!(death_of(DeathKind::Melee, d, false, false, true, false), "D_S");
    assert_eq!(death_of(DeathKind::Poison, d, false, false, true, false), "D_P");
    assert_eq!(death_of(DeathKind::EndlessFall, d, false, false, true, false), "D_W");
    assert_eq!(death_of(DeathKind::Other, d, false, true, true, true), "DC", "female: no NS1");
    // Location 5 (BR data, ours): the groin deaths; location 2 (US): torso.
    assert_eq!(death_of(DeathKind::Other, d, false, true, true, false), "NS1");
    assert_eq!(death_of(DeathKind::Other, d, false, true, false, false), "NS2");
    assert_eq!(death_of(DeathKind::Other, d, false, false, true, false), "DT");
    // A jump forces the coin to 0.
    let list = driver::spec_library(false);
    let jumping = DeathInputs {
        kind: DeathKind::Other,
        dir: d,
        forward: Vec3::Z,
        right: Vec3::X,
        head: false,
        groin: false,
        coin: true,
        jumping: true,
        special: None,
    };
    assert_eq!(death::choose(&jumping, |n| list.find(n).is_some()), "DT_1");
    // The body turns away from the shot: yaw atan(dx/dz), + pi when dz > 0.
    assert!((death::jump_yaw(Vec3::new(0.0, 0.0, -1.0))).abs() < 1e-6);
    assert!((death::jump_yaw(Vec3::new(0.0, 0.0, 1.0)) - std::f32::consts::PI).abs() < 1e-6);
    assert!((death::jump_yaw(Vec3::new(1.0, 0.0, 0.0)) - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
}

#[test]
fn t40_crouched_deaths_stand() {
    // The crouched variants are never chosen: a crouched front torso
    // death is DT / DT_1.
    let d = Vec3::NEG_Z;
    assert_eq!(death_of(DeathKind::Other, d, false, false, true, false), "DT");
    assert_eq!(death_of(DeathKind::Other, d, false, false, false, false), "DT_1");
}

#[test]
fn t41_body_lifetime() {
    let tdm = records::data().body_removal(combat_arms::game::rules::TDM);
    assert_eq!(tdm, Some(5.0));
    let end = 3.033 + 0.001;
    assert!((death::removal_time(end, Some(2.0), tdm).unwrap() - 8.034).abs() < 1e-9);
    assert!((death::removal_time(end, Some(10.0), tdm).unwrap() - 15.0).abs() < 1e-9);
    assert_eq!(death::removal_time(end, None, tdm), None, "not before the respawn");
    assert!((death::removal_time(end, Some(10.0), Some(500.0)).unwrap() - 510.0).abs() < 1e-9, "Combat");
    assert_eq!(death::removal_time(end, Some(10.0), None), None, "other modes: never");
}

/// Case 42: the body 0.1 u above the dead player, without the weapon;
/// the death animation on its owner's main tracker.
#[test]
fn t42_body_creation() {
    let mut sim = sim();
    let p = player(&mut sim, Vec3::new(0.0, 1.0, 12.0), 0.0);
    weapon::give(sim.app.world_mut(), p, RIFLE).unwrap();
    sim.seconds(1.3);
    let centre = sim.position(p);
    // Shot from the front in the chest: DT or DT_1.
    kill(&mut sim, p, Vec3::Z, Hitgroup::Chest, DamageKind::Bullet);
    sim.ticks(2);
    let world = sim.app.world_mut();
    let mut q = world.query::<&CaBody>();
    let body = q.single(world).expect("a body").clone();
    assert!(close(body.at, centre + Vec3::Y * 0.1 * UNIT, 1e-4), "{} vs {centre}", body.at);
    assert!(["DT", "DT_1"].contains(&body.animation.as_str()), "{}", body.animation);
    let anim = world.get::<CaAnim>(p).unwrap();
    assert_eq!(anim.death.as_deref(), Some(body.animation.as_str()));
    assert_eq!(anim.driver.name_of(&anim.driver.main), body.animation);
    assert!(!anim.driver.main.looping);
}

// ---------------------------------------------------------------------
// Scenario: the driver on a player.

fn sim() -> Sim {
    let mut sim = Sim::new(CaSim, GreyboxMapPlugin);
    sim.set_tick_interval(TICK_INTERVAL);
    sim
}

fn player(sim: &mut Sim, at: Vec3, yaw: f32) -> Entity {
    let p = sim.spawn_character(at, movement::ID);
    sim.intent(p).yaw = yaw;
    sim.seconds(0.5);
    sim.app.world_mut().entity_mut(p).insert(CaArmor {
        points: 0.0,
        gear: Vec::new(),
    });
    p
}

fn kill(sim: &mut Sim, p: Entity, dir: Vec3, hitgroup: Hitgroup, kind: DamageKind) {
    let point = sim.position(p);
    sim.app.world_mut().write_message(Damage {
        target: p,
        attacker: None,
        amount: 10.0,
        point,
        dir,
        hitgroup,
        kind,
        weapon: None,
        force: Vec3::ZERO,
    });
}

fn playing(sim: &Sim, p: Entity) -> (String, String, String) {
    let a = sim.app.world().get::<CaAnim>(p).unwrap();
    let (u, l, c) = a.playing();
    (u.into(), l.into(), c.into())
}

/// A player with the rifle: still, walking (the walk key), running,
/// sprinting, firing, reloading; the hit spheres pitch with the look.
#[test]
fn scenario_player_states() {
    let mut sim = sim();
    let p = player(&mut sim, Vec3::new(0.0, 1.0, 12.0), 0.0);
    let w = weapon::give(sim.app.world_mut(), p, RIFLE).unwrap();
    sim.seconds(1.3);
    assert_eq!(playing(&sim, p), ("URFRAmS".into(), "LST".into(), "LC".into()));
    sim.intent(p).move_axis = Vec2::Y;
    sim.seconds(0.3);
    assert_eq!(playing(&sim, p).1, "LRF");
    assert_eq!(playing(&sim, p).0, "URFRAmF");
    sim.intent(p).walk = true;
    sim.seconds(0.3);
    assert_eq!(playing(&sim, p).1, "LWF");
    assert_eq!(playing(&sim, p).0, "URFWAmF");
    sim.intent(p).walk = false;
    sim.intent(p).sprint = true;
    sim.seconds(0.3);
    assert_eq!(playing(&sim, p).1, "LRRF");
    sim.intent(p).sprint = false;
    sim.intent(p).move_axis = Vec2::ZERO;
    sim.seconds(0.5);
    assert_eq!(playing(&sim, p).1, "LST");
    sim.intent(p).fire = true;
    sim.ticks(2);
    assert_eq!(playing(&sim, p).0, "URFRFiS");
    sim.intent(p).fire = false;
    sim.seconds(0.3);
    assert_eq!(playing(&sim, p).0, "URFRAmS");
    sim.app.world_mut().get_mut::<Magazine>(w).unwrap().clip = 10;
    sim.intent(p).reload = true;
    sim.ticks(2);
    sim.intent(p).reload = false;
    assert_eq!(playing(&sim, p).0, "URFRSReS");
    // Looking down lowers the head sphere.
    let head = records::data().skeleton(0).spheres.iter().position(|s| s.name == "Head").unwrap();
    let level = sim.app.world().get::<CaPose>(p).unwrap().spheres[head].unwrap();
    sim.intent(p).pitch = -0.5;
    sim.ticks(1);
    let down = sim.app.world().get::<CaPose>(p).unwrap().spheres[head].unwrap();
    assert!(down.y < level.y - 1.0, "{level} -> {down}");
}

/// With a body model (a stand-in registered for body 1, the "Light"
/// record): the pose is the model's, its spheres on its nodes, its
/// sockets the model's.
#[test]
fn scenario_modelled_body() {
    let file = &records::data().model(1).unwrap().file;
    let mut stand_in = male_stand_in(true);
    // Shift every node by 5 units on X so the model's pose is told from
    // the spec's skeleton.
    for n in &mut stand_in.nodes {
        n.2[0] += 5.0;
    }
    models::register(file, Arc::new(stand_in.rig(STAND_IN_ANIMS)));
    let mut sim = sim();
    let p = player(&mut sim, Vec3::new(0.0, 1.0, 12.0), 0.0);
    sim.app.world_mut().entity_mut(p).insert(Selection { model: 1, ..default() });
    sim.ticks(3);
    let pose = sim.app.world().get::<CaPose>(p).unwrap().clone();
    models::unregister(file);
    assert!(pose.modelled);
    assert_eq!(playing(&sim, p), ("URFRAmS".into(), "LST".into(), "LC".into()));
    let skel = records::data().skeleton(0);
    let torso = skel.spheres.iter().position(|s| s.name == "Torso").unwrap();
    // The stand-in's bind Torso (5, 5.14, 0.74) with its feet at -50.
    assert!(close(pose.spheres[torso].unwrap(), Vec3::new(5.0, 55.14, -0.74), 1e-3), "{:?}", pose.spheres[torso]);
    let groin = skel.spheres.iter().position(|s| s.name == "Nuts").unwrap();
    assert!(pose.spheres[groin].is_some());
    let shoulder = skel.spheres.iter().position(|s| s.name == "Left_Shld").unwrap();
    assert!(pose.spheres[shoulder].is_none());
}

// ---------------------------------------------------------------------
// Network: other players' animation state comes from the server.

/// Client 1 runs forward; client 0 sees its trackers (`CaAnimNet`) and
/// plays them: the legs' `LRF`, the torso's `URFRAmF`.
#[test]
fn net_others_follow_the_server() {
    use chimera_net::{harness::NetSim, net::memory::LinkConditions};
    use combat_arms::game::characters::CaAnimNet;
    let link = LinkConditions {
        latency: std::time::Duration::from_millis(30),
        jitter: std::time::Duration::ZERO,
        loss: 0.0,
    };
    let mut sim = NetSim::new(CaSim, link, 7, 2, |app: &mut App| {
        app.add_plugins(GreyboxMapPlugin);
    });
    sim.set_frame(TICK_INTERVAL);
    sim.until_joined(600);
    sim.ticks(240);
    let me = sim.local_player(0).unwrap();
    let other = sim.local_player(1).unwrap();
    let them = |sim: &mut NetSim| {
        let world = sim.clients[0].app.world_mut();
        let mut q = world.query::<(Entity, &CaAnimNet, &CaAnim)>();
        q.iter(world)
            .filter(|(e, ..)| *e != me)
            .map(|(_, n, a)| (n.lower.0.clone(), a.playing().1.to_string(), a.playing().0.to_string()))
            .next()
            .expect("the other player's state")
    };
    assert_eq!(them(&mut sim), ("LST".into(), "LST".into(), "URFRAmS".into()));
    sim.clients[1].app.world_mut().get_mut::<chimera_core::core::Intent>(other).unwrap().move_axis = Vec2::Y;
    sim.ticks(40);
    assert_eq!(them(&mut sim), ("LRF".into(), "LRF".into(), "URFRAmF".into()));
}
