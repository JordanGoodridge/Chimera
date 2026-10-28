//! The minigame scoreboard (`client::minigame_scoreboard`): hidden until
//! `+showscores` is held while a minigame plays, then a row for every
//! player and bot with name, kills, deaths, the rung's gun, ping and
//! matches won, best rung first; down again on release, and never up
//! without a minigame (plain CS:S keeps its own scoreboard).

use bevy::prelude::*;
use chimera_client::client::{
    console::HeldActions,
    minigame_scoreboard::{MinigameScoreboardPlugin, ScoreboardRoot, ScoreboardRow},
};
use chimera_client::client::minigame_scoreboard::GunNames;
use chimera_net::net::{NetScore, score_flags};
use chimera_core::core::Score;
use chimera_sim::{
    SimBasePlugins,
    minigame::{
        hud::MinigameHud,
        ladder::{Ladder, Rung},
        runtime::InMinigame,
        wins::MatchesWon,
    },
};
use chimera_world::{greybox::GreyboxMapPlugin, harness::Sim};

fn rows(sim: &mut Sim) -> Vec<ScoreboardRow> {
    let w = sim.app.world_mut();
    w.query::<&ScoreboardRow>().iter(w).cloned().collect()
}

fn shown(sim: &mut Sim) -> bool {
    let w = sim.app.world_mut();
    let v = w.query_filtered::<&Visibility, With<ScoreboardRoot>>().single(w).unwrap();
    *v != Visibility::Hidden
}

fn player(sim: &mut Sim, name: &'static str, rung: usize, kills: i32, deaths: u32, won: u32, net: Option<NetScore>) {
    let mut e = sim.app.world_mut().spawn((
        InMinigame,
        Name::new(name),
        Rung(rung),
        Score { kills, deaths },
        MatchesWon(won),
    ));
    if let Some(n) = net {
        e.insert(n);
    }
}

#[test]
fn holding_the_key_lists_every_player_and_bot_best_first() {
    let mut sim = Sim::new(SimBasePlugins, (GreyboxMapPlugin, MinigameScoreboardPlugin));
    sim.app.init_resource::<HeldActions>();
    sim.app.insert_resource(Ladder { rungs: vec!["kit:pistol", "kit:weapon_rifle", "kit:weapon_shotgun"], ..default() });
    sim.app.world_mut().insert_resource(GunNames([("kit:weapon_rifle".to_string(), "K&M Rifle".to_string())].into()));
    player(&mut sim, "Alpha", 1, 4, 2, 3, Some(NetScore { ping: 42, ..default() }));
    player(&mut sim, "Botty", 2, 1, 5, 0, Some(NetScore { flags: score_flags::BOT, ..default() }));
    player(&mut sim, "Cee", 1, 6, 1, 1, None);

    // Not while no minigame plays, whatever the key does.
    sim.app.world_mut().resource_mut::<HeldActions>().showscores = true;
    sim.ticks(3);
    assert!(!shown(&mut sim) && rows(&mut sim).is_empty(), "plain games keep their own scoreboard");

    // A minigame plays but the key is up.
    sim.app.world_mut().insert_resource(MinigameHud { title: "Kit gun game".into(), ..default() });
    sim.app.world_mut().resource_mut::<HeldActions>().showscores = false;
    sim.ticks(3);
    assert!(!shown(&mut sim) && rows(&mut sim).is_empty());

    // Held: a row each, rung then kills decide the order.
    sim.app.world_mut().resource_mut::<HeldActions>().showscores = true;
    sim.ticks(3);
    assert!(shown(&mut sim));
    let got: Vec<(String, i32, u32, String, u16, bool, u32)> =
        rows(&mut sim).into_iter().map(|r| (r.name, r.kills, r.deaths, r.gun, r.ping, r.bot, r.wins)).collect();
    let want = [
        ("Botty", 1, 5, "SHOTGUN", 0, true, 0),
        ("Cee", 6, 1, "K&M Rifle", 0, false, 1),
        ("Alpha", 4, 2, "K&M Rifle", 42, false, 3),
    ]
    .map(|(n, k, d, g, p, b, w)| (n.to_string(), k, d, g.to_string(), p, b, w));
    assert_eq!(got, want);

    // The columns are fixed, non-shrinking widths that fit the board, so
    // no header or value overlaps its neighbour.
    {
        use chimera_client::client::minigame_scoreboard::{ScoreboardHeader, WIDTHS};
        let w = sim.app.world_mut();
        let header = w.query_filtered::<&Children, With<ScoreboardHeader>>().single(w).unwrap().to_vec();
        let widths: Vec<f32> = header
            .iter()
            .map(|c| {
                let n = w.get::<Node>(*c).unwrap();
                assert_eq!(n.flex_shrink, 0.0);
                match n.width {
                    Val::Px(v) => v,
                    other => panic!("column width {other:?}"),
                }
            })
            .collect();
        assert_eq!(widths, WIDTHS);
        let root = w.query_filtered::<&Node, With<ScoreboardRoot>>().single(w).unwrap();
        let Val::Px(board) = root.width else { panic!("board width") };
        assert!(widths.iter().sum::<f32>() < board, "columns fit the board");
        let bg = w.query_filtered::<&BackgroundColor, With<ScoreboardRoot>>().single(w).unwrap();
        assert_eq!(bg.0.alpha(), 1.0, "opaque over the minigame panel");
    }

    // Released: gone.
    sim.app.world_mut().resource_mut::<HeldActions>().showscores = false;
    sim.ticks(3);
    assert!(!shown(&mut sim) && rows(&mut sim).is_empty());
}
