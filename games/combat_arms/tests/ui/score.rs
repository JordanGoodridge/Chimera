//! The game mode on the HUD (hud_crosshair.md D6, D7) and the scoreboard
//! (ui_system.md, game_modes.md S13), headless: a TDM match on the
//! greybox shows its counters, the local team highlighted and no timer;
//! Elimination Pro shows its round timer; the scoreboard lists a kill.

use bevy::prelude::*;
use chimera_core::core::{Damage, DamageKind, Hitgroup, LocalPlayer, Team};
use chimera_world::{greybox::GreyboxMapPlugin, harness::Sim};
use combat_arms::{
    game::{
        movement,
        rules::{CaMatch, Phase},
        weapons::RIFLE,
    },
    harness::{CaSim, with_mode},
    ui::{
        hud::{
            CaHudStatePlugin,
            model::{SCORE_ALPHA, SCORE_PLAIN},
            score::CaScoreHud,
        },
        scoreboard::{CaScoreboard, CaScoreboardStatePlugin},
    },
};

fn sim(mode: &str) -> (Sim, Entity, Entity) {
    let mut sim = Sim::with(CaSim, |app| {
        app.add_plugins((GreyboxMapPlugin, CaHudStatePlugin, CaScoreboardStatePlugin));
    });
    with_mode(sim.app.world_mut(), mode);
    let me = sim.spawn_character(Vec3::new(0.0, 1.0, 0.0), movement::ID);
    let them = sim.spawn_character(Vec3::new(2.0, 1.0, 0.0), movement::ID);
    sim.app.world_mut().entity_mut(me).insert((Team(1), LocalPlayer, Name::new("me")));
    sim.app.world_mut().entity_mut(them).insert((Team(2), Name::new("them")));
    for _ in 0..60 {
        if sim.app.world().resource::<CaMatch>().phase == Phase::Live {
            break;
        }
        sim.ticks(1);
    }
    (sim, me, them)
}

fn score_hud(sim: &Sim) -> CaScoreHud {
    sim.app.world().resource::<CaScoreHud>().clone()
}

#[test]
fn tdm_panel_counts_kills_without_a_timer() {
    let (mut sim, me, them) = sim("TDM");
    sim.seconds(4.1);
    sim.app.world_mut().write_message(Damage {
        target: them,
        attacker: Some(me),
        amount: 5.0,
        point: Vec3::ZERO,
        dir: Vec3::NEG_Z,
        hitgroup: Hitgroup::Generic,
        kind: DamageKind::Bullet,
        weapon: Some(RIFLE),
        force: Vec3::ZERO,
    });
    sim.ticks(2);
    let hud = score_hud(&sim);
    let panel = hud.panel.expect("a panel in TDM");
    assert_eq!(panel.mode_name, "Elimination");
    assert_eq!(panel.goal, "100 Kills");
    assert_eq!((panel.rows[0].score.as_str(), panel.rows[1].score.as_str()), ("1", "0"));
    assert_eq!(panel.rows[0].colour, SCORE_ALPHA, "our team highlighted");
    assert_eq!(panel.rows[1].colour, SCORE_PLAIN);
    assert_eq!(panel.rows[0].bar, 1.0, "floor(1 / 100 x 155)");
    assert_eq!(panel.highlight, Some(Vec2::new(344.0, 18.0)));
    assert_eq!(hud.timer, None, "TDM has no timer");
    // The scoreboard, while the score key is held.
    sim.app
        .world_mut()
        .resource_mut::<chimera_client::client::console::HeldActions>()
        .showscores = true;
    sim.ticks(1);
    let board = sim.app.world().resource::<CaScoreboard>().clone();
    assert!(board.shown);
    assert_eq!(board.lists[0][0].name, "me");
    assert_eq!((board.lists[0][0].kills, board.lists[1][0].deaths), (1, 1));
    assert_eq!(board.scores, [1, 0]);
}

#[test]
fn elimination_pro_shows_its_round_timer() {
    let (mut sim, ..) = sim("TSV");
    sim.seconds(1.0);
    let timer = score_hud(&sim).timer.expect("a timer in the warm-up");
    assert_eq!(timer, "00 : 08", "10 s warm-up, 1 s and a bit gone");
}

/// game_modes_2.md M11.10, M11.4's warning and M13's lines: Hired Guns'
/// counters and the pocket, the squad's lines; VIP Escort's roles, the
/// exit and the swap.
#[test]
fn modes_part_two_lines() {
    use combat_arms::{
        game::rules::{
            mercs::MercOrder,
            objectives::{Bullion, CaCarry, CaObjectives, Exit},
        },
        ui::hud::score::{objective_lines, squad_lines},
    };
    let mut o = CaObjectives {
        placed: 80,
        banked: [7, 3],
        ..Default::default()
    };
    o.bullion = vec![
        Bullion {
            id: 1,
            at: Vec3::ZERO,
            bars: 1,
            money: 100,
            bag: false,
        },
        Bullion {
            id: 2,
            at: Vec3::ZERO,
            bars: 12,
            money: 1200,
            bag: true,
        },
    ];
    let pocket = |bars: u32| CaCarry {
        bars,
        money: bars as i32 * 100,
        ..Default::default()
    };
    let lines = objective_lines(&o, pocket(11), 0.0);
    assert!(lines.contains(&"Banked 7 - 3, in bags 12, unclaimed 1".to_string()), "{lines:?}");
    assert!(lines.contains(&"Pocket 11 bars".to_string()));
    assert!(lines.contains(&"Speed penalty 3 % with 11 bars".to_string()));
    let lines = objective_lines(&o, pocket(30), 0.0);
    assert!(lines.contains(&"Speed penalty 45 % with 30 bars".to_string()), "{lines:?}");
    let squad = squad_lines(Some(800), Some(MercOrder::FollowMe), &[("Squad_1".into(), 0.5)]);
    assert_eq!(squad, ["Bank 800", "Mercenaries (follow me):", "  Squad_1 50 %"]);
    let esc = CaObjectives {
        escort: 2,
        exit: Some(Exit { opens: 30.0, open: false }),
        rounds_to_swap: 2,
        ..Default::default()
    };
    let lines = objective_lines(&esc, CaCarry::default(), 10.0);
    assert_eq!(lines, ["Bravo escorts the VIP", "The exit opens in 20 s", "Rounds until the swap: 2"]);
}

/// The mode's name and goal on the panel for Hired Guns.
#[test]
fn hired_guns_panel() {
    let (sim, _me, _them) = sim("SQM");
    let hud = score_hud(&sim);
    let panel = hud.panel.expect("a panel");
    assert_eq!((panel.mode_name.as_str(), panel.goal.as_str()), ("Hired Guns", "7 Rounds"));
}
