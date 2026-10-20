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
