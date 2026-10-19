//! Capture the Flag (specs/combat_arms/game_modes.md M4): its test cases
//! 43-49 on `CaSim` and the greybox with the common setup (`mode_kit`),
//! Alpha's flag and zone at (-4, 1, 6), Bravo's at (4, 1, -14).

use bevy::prelude::*;
use chimera_core::core::{DamageKind, Hitgroup};
use chimera_world::harness::Sim;
use combat_arms::game::rules::{CaMatch, Phase, objectives::FlagAt};

use crate::mode_kit::*;

const ALPHA_HOME: Vec3 = Vec3::new(-4.0, 1.0, 6.0);
const BRAVO_HOME: Vec3 = Vec3::new(4.0, 1.0, -14.0);

/// CTF live with `per_side` players a team, the flags placed (set before
/// the round's setup puts them home), everyone unprotected.
fn ctf(per_side: usize, goal: u32) -> (Sim, Vec<Entity>, Vec<Entity>) {
    let mut sim = game("CTF", 12);
    settings(&mut sim).goal = goal;
    let (fa, za) = flag_and_zone(JSF, ALPHA_HOME);
    let (fb, zb) = flag_and_zone(CREED, BRAVO_HOME);
    {
        let mut o = objects(&mut sim);
        o.flags = vec![fa, fb];
        o.flag_zones = vec![za, zb];
    }
    let a: Vec<Entity> = (0..per_side).map(|_| join(&mut sim, JSF)).collect();
    let b: Vec<Entity> = (0..per_side).map(|_| join(&mut sim, CREED)).collect();
    until_live(&mut sim);
    unprotect_all(&mut sim);
    (sim, a, b)
}

fn flag_at(sim: &Sim, team: u8) -> FlagAt {
    objectives(sim).flag(team).unwrap().at
}

/// 43. The round's time limit is TimeLimit x G: G = 5, 900 s; G = 3, 540 s.
#[test]
fn case_43_time_limit() {
    for (goal, want) in [(5, 900.0), (3, 540.0)] {
        let (sim, ..) = ctf(1, goal);
        let left = m(&sim).round_ends.unwrap() - now(&sim);
        assert!((want - 0.1..=want).contains(&left), "G {goal}: {left}");
    }
}

/// 44. An Alpha carrier with the Bravo flag touches the Alpha flag zone,
/// the Alpha flag home: Alpha captures 1, the Bravo flag back at its base,
/// the carrier's captures +1.
#[test]
fn case_44_capture() {
    let (mut sim, a, _b) = ctf(1, 5);
    hold_at(&mut sim, a[0], BRAVO_HOME, 0.05);
    assert_eq!(carry(&sim, a[0]).flag, Some(CREED), "taken");
    assert_eq!(flag_at(&sim, CREED), FlagAt::Carried);
    hold_at(&mut sim, a[0], ALPHA_HOME + Vec3::X * 0.5, 0.05);
    assert_eq!(m(&sim).score, [1, 0]);
    assert_eq!(flag_at(&sim, CREED), FlagAt::Home);
    assert_eq!(stats(&sim, a[0]).captures, 1);
    assert_eq!(carry(&sim, a[0]).flag, None);
}

/// 45. As 44 with the Alpha flag dropped: no capture until an Alpha player
/// touches it (Alpha returns +1, home); then the carrier captures.
#[test]
fn case_45_capture_blocked() {
    let (mut sim, a, b) = ctf(2, 5);
    // Bravo takes Alpha's flag and dies with it away from home.
    let away = Vec3::new(-4.0, 1.0, -4.0);
    hold_at(&mut sim, b[0], ALPHA_HOME, 0.05);
    assert_eq!(carry(&sim, b[0]).flag, Some(JSF));
    put(&mut sim, b[0], away);
    kill_now(&mut sim, b[0], a[1]);
    assert!(matches!(flag_at(&sim, JSF), FlagAt::Dropped { .. }));
    // The carrier at its zone: blocked.
    hold_at(&mut sim, a[0], BRAVO_HOME, 0.05);
    hold_at(&mut sim, a[0], ALPHA_HOME, 0.2);
    assert_eq!(m(&sim).score, [0, 0], "no capture with the flag away");
    assert_eq!(carry(&sim, a[0]).flag, Some(CREED));
    // An Alpha player returns it.
    let dropped = match flag_at(&sim, JSF) {
        FlagAt::Dropped { at, .. } => at,
        f => panic!("{f:?}"),
    };
    hold_at(&mut sim, a[1], dropped, 0.05);
    assert_eq!(flag_at(&sim, JSF), FlagAt::Home);
    assert_eq!(m(&sim).returns, [1, 0]);
    assert_eq!(stats(&sim, a[1]).returns, 1);
    // Now the capture.
    hold_at(&mut sim, a[0], ALPHA_HOME + Vec3::Z * 0.5, 0.05);
    assert_eq!(m(&sim).score, [1, 0]);
}

/// 46. A carrier killed at (100, 0, 200) u on flat ground: the flag lies
/// there on the floor; untouched, it goes home 30 s later with no return
/// credit.
#[test]
fn case_46_drop_and_return_timer() {
    let (mut sim, a, b) = ctf(1, 5);
    hold_at(&mut sim, a[0], BRAVO_HOME, 0.05);
    let there = Vec3::new(u(100.0), 1.0, u(200.0));
    put(&mut sim, a[0], there);
    kill_now(&mut sim, a[0], b[0]);
    let t0 = now(&sim);
    let (at, returns) = match flag_at(&sim, CREED) {
        FlagAt::Dropped { at, returns } => (at, returns),
        f => panic!("{f:?}"),
    };
    assert!((at.x - there.x).abs() < 1e-3 && (at.z - there.z).abs() < 1e-3, "{at}");
    assert!(at.y < there.y - 0.5 && at.y > -0.5, "on the floor: {at}");
    assert!((returns - t0 - 30.0).abs() < 0.05);
    sim.seconds(29.8);
    assert!(matches!(flag_at(&sim, CREED), FlagAt::Dropped { .. }));
    sim.seconds(0.3);
    assert_eq!(flag_at(&sim, CREED), FlagAt::Home);
    assert_eq!(m(&sim).returns, [0, 0], "no credit");
}

/// 47. A carrier killed by a kill zone (a volume's damage): the flag home
/// at once.
#[test]
fn case_47_kill_zone() {
    let (mut sim, a, _b) = ctf(1, 5);
    hold_at(&mut sim, a[0], BRAVO_HOME, 0.05);
    put(&mut sim, a[0], Vec3::new(0.0, 1.0, -4.0));
    sim.app.world_mut().write_message(chimera_core::core::Damage {
        target: a[0],
        attacker: None,
        amount: 5.0,
        point: Vec3::ZERO,
        dir: Vec3::ZERO,
        hitgroup: Hitgroup::Generic,
        kind: DamageKind::Generic,
        weapon: None,
        force: Vec3::ZERO,
    });
    sim.ticks(1);
    assert!(dead(&sim, a[0]));
    assert_eq!(flag_at(&sim, CREED), FlagAt::Home);
}

/// 48. Alpha at 4 captures, G = 5, captures again: Alpha wins, round over
/// at the next frame.
#[test]
fn case_48_win() {
    let (mut sim, a, _b) = ctf(1, 5);
    sim.app.world_mut().resource_mut::<CaMatch>().score = [4, 0];
    hold_at(&mut sim, a[0], BRAVO_HOME, 0.05);
    hold_at(&mut sim, a[0], ALPHA_HOME, 0.02);
    assert_eq!(m(&sim).score, [5, 0]);
    assert_eq!(m(&sim).winner, Some(JSF));
    until(&mut sim, 0.1, |p| matches!(p, Phase::RoundOver { .. }));
    until(&mut sim, 7.0, |p| p == Phase::MatchOver);
    assert_eq!(m(&sim).result, Some(Some(JSF)));
}

/// 49. Time-out tie-breaks: captures 2 - 2, returns 3 - 1: Alpha; 2 - 2,
/// 1 - 1, kills 10 - 10, deaths 9 - 11: Alpha (fewer deaths); all equal:
/// a draw.
#[test]
fn case_49_time_out_tie_break() {
    let cases: [([i32; 2], [u32; 2], [u32; 2], [u32; 2], Option<u8>); 3] = [
        ([2, 2], [3, 1], [0, 0], [0, 0], Some(JSF)),
        ([2, 2], [1, 1], [10, 10], [9, 11], Some(JSF)),
        ([2, 2], [1, 1], [10, 10], [9, 9], None),
    ];
    for (score, returns, kills, deaths, want) in cases {
        let (mut sim, ..) = ctf(1, 5);
        {
            let n = now(&sim);
            let mut m = sim.app.world_mut().resource_mut::<CaMatch>();
            m.score = score;
            m.returns = returns;
            m.kills = kills;
            m.deaths = deaths;
            m.round_ends = Some(n);
        }
        until(&mut sim, 0.2, |p| matches!(p, Phase::RoundOver { .. }));
        assert_eq!(m(&sim).results.last().unwrap().winner, want, "{returns:?} {deaths:?}");
    }
}

/// S7's CTF column: an enemy kill counts for the killer's team's kills, a
/// death for the victim's team's deaths.
#[test]
fn team_kills_and_deaths_for_the_tie_breaks() {
    let (mut sim, a, b) = ctf(1, 5);
    kill_now(&mut sim, b[0], a[0]);
    assert_eq!((m(&sim).kills, m(&sim).deaths), ([1, 0], [0, 1]));
    assert_eq!((stats(&sim, a[0]).kills, stats(&sim, b[0]).deaths), (1, 1));
}
