//! Minigames' contract (`chimera_sim::minigame`) on the test kit: a
//! definition as data, checked against what the app registered; started,
//! it takes the gameplay's rules, makes free-for-all teams, waits for its
//! minimum of players, spawns them protected with the rules' weapons; the
//! `chimera:ladder` rules move a killer up a rung (its weapon swapped),
//! a melee death down, and a kill on the last rung wins the match, which
//! then ends; at the time limit the highest rung wins.

use bevy::prelude::*;
use chimera_core::{
    core::{Damage, DamageKind, Dead, God, Hitgroup, Score, SimClock, Team},
    gameplay::Gameplay,
};
use chimera_sim::{
    minigame::{self, InMinigame, MINIGAME, MinigameAppExt, MinigamePlugin, Protected, ladder::Rung},
    mode::{FFA_FIRST, Match, Phase},
    movement::placeholder,
    testkit::{self, PISTOL, RIFLE, SHOTGUN},
};
use chimera_world::{greybox, harness::Sim};

const DEF: &str = r#"
id = "kit:gun_game"
name = "Kit gun game"
players = { min = 2, max = 4 }
maps = ["chimera:greybox"]
teams = "free_for_all"
time_limit = "60s"

[spawn]
respawn = { delay = "1s", protection = "0.5s" }
points = "map"

[rules]
module = "chimera:ladder"
ladder = ["kit:pistol", "kit:rifle", "kit:shotgun"]
demote_on = "melee_death"
"#;

fn sim_with(def: &str) -> Sim {
    Sim::with(chimera_sim::SimBasePlugins, |app| {
        app.add_plugins((greybox::GreyboxMapPlugin, MinigamePlugin));
        testkit::add_weapons(app);
        app.add_minigames(&[("kit_gun_game.toml", def)]).expect("the definition reads");
    })
}

fn started(def: &str) -> Sim {
    let mut sim = sim_with(def);
    minigame::start(sim.app.world_mut(), "kit:gun_game").expect("it starts");
    sim
}

fn player(sim: &mut Sim) -> Entity {
    sim.spawn_character(greybox::SPAWNS[0], placeholder::ID)
}

fn phase(sim: &Sim) -> Phase {
    sim.app.world().resource::<Match>().phase
}

fn rung(sim: &Sim, p: Entity) -> usize {
    sim.app.world().get::<Rung>(p).map_or(0, |r| r.0)
}

/// `by` kills `p` with a hit of `kind`.
fn kill(sim: &mut Sim, p: Entity, by: Entity, kind: DamageKind) {
    let at = sim.position(p);
    sim.app.world_mut().write_message(Damage {
        target: p,
        attacker: Some(by),
        amount: 5.0,
        point: at,
        dir: Vec3::NEG_Z,
        hitgroup: Hitgroup::Chest,
        kind,
        weapon: None,
        force: Vec3::ZERO,
    });
    sim.ticks(2);
}

/// Wait out a respawn's delay and protection (and the drawing).
fn settle(sim: &mut Sim) {
    sim.seconds(1.7);
}

#[test]
fn a_definition_is_checked_against_the_app() {
    let mut sim = sim_with(DEF);
    // A weapon the catalogue lacks, a set the definition doesn't allow, a
    // module nobody registered, teams the module doesn't play.
    for (from, to, says) in [
        ("\"kit:shotgun\"]", "\"kit:nothing\"]", "not in the weapon catalogue"),
        ("time_limit = \"60s\"", "time_limit = \"60s\"\n[slots]\nweapons = [\"other\"]", "not from the sets"),
        ("module = \"chimera:ladder\"", "module = \"kit:unknown\"", "no rules module"),
        ("teams = \"free_for_all\"", "teams = \"two\"", "teams"),
        ("demote_on = \"melee_death\"", "demote_on = \"sometimes\"", "unknown variant"),
    ] {
        let text = DEF.replace(from, to).replace("kit:gun_game", "kit:bad");
        sim.app.add_minigames(&[("bad.toml", &text)]).unwrap();
        let err = minigame::start(sim.app.world_mut(), "kit:bad").map(|_| ()).unwrap_err();
        assert!(err.contains(says), "{from} -> {to}: {err}");
    }
    assert!(minigame::start(sim.app.world_mut(), "kit:nope").is_err());
    let def = minigame::start(sim.app.world_mut(), "kit:gun_game").expect("the good one starts");
    assert_eq!(def.name, "Kit gun game");
    assert_eq!(sim.app.world().resource::<Gameplay>().rules, MINIGAME, "the minigame is the gameplay's rules");
}

#[test]
fn players_join_on_their_own_teams_and_the_match_waits_for_the_minimum() {
    let mut sim = started(DEF);
    let a = player(&mut sim);
    sim.seconds(0.5);
    assert!(sim.app.world().get::<InMinigame>(a).is_some(), "taken in");
    assert_eq!(phase(&sim), Phase::Waiting, "one player of two");
    assert_eq!(testkit::active_id(sim.app.world(), a), Some(PISTOL), "the first rung's weapon while waiting");
    let b = player(&mut sim);
    sim.ticks(3);
    assert_eq!(phase(&sim), Phase::Live, "two in: live at once");
    let teams: Vec<u8> = [a, b].iter().map(|p| sim.app.world().get::<Team>(*p).unwrap().0).collect();
    assert_eq!(teams, [FFA_FIRST, FFA_FIRST + 1], "a team each");
    let m = sim.app.world().resource::<Match>().clone();
    let now = sim.app.world().resource::<SimClock>().now;
    assert!((m.round_ends.unwrap() - now - 60.0).abs() < 0.1, "the time limit's timer");
    // Spawned at different points, protected for a while.
    assert!(sim.position(a).distance(sim.position(b)) > 1.0);
    assert!(sim.app.world().get::<Protected>(b).is_some() && sim.app.world().get::<God>(b).is_some());
    sim.seconds(0.6);
    assert!(sim.app.world().get::<God>(b).is_none(), "protection over");
}

#[test]
fn kills_climb_the_ladder_melee_deaths_demote_and_the_last_rung_wins() {
    let mut sim = started(DEF);
    let a = player(&mut sim);
    let b = player(&mut sim);
    settle(&mut sim);
    assert_eq!(phase(&sim), Phase::Live);

    // A kill: a rung up, the rifle in hand at once; the victim back
    // after the delay, still on its rung.
    kill(&mut sim, b, a, DamageKind::Bullet);
    assert_eq!(rung(&sim, a), 1);
    assert!(sim.app.world().get::<Dead>(b).is_some());
    sim.seconds(0.5);
    assert_eq!(testkit::active_id(sim.app.world(), a), Some(RIFLE), "swapped");
    let carried = sim.app.world().get::<chimera_sim::weapon::Inventory>(a).unwrap().weapons.len();
    assert_eq!(carried, 1, "the old rung's weapon taken");
    sim.seconds(0.6);
    assert!(sim.app.world().get::<Dead>(b).is_none(), "respawned after 1 s");
    assert_eq!(sim.app.world().get::<Score>(a).unwrap().kills, 1);
    assert_eq!(sim.app.world().get::<Score>(b).unwrap().deaths, 1);

    // b climbs, then a knifes it: b down a rung (and a up).
    settle(&mut sim);
    kill(&mut sim, a, b, DamageKind::Bullet);
    assert_eq!(rung(&sim, b), 1);
    sim.seconds(1.2);
    settle(&mut sim);
    kill(&mut sim, b, a, DamageKind::Melee);
    assert_eq!((rung(&sim, a), rung(&sim, b)), (2, 0), "a melee death demotes, the killer still climbs");
    settle(&mut sim);
    assert_eq!(testkit::active_id(sim.app.world(), b), Some(PISTOL), "the lower rung's weapon at respawn");
    assert_eq!(testkit::active_id(sim.app.world(), a), Some(SHOTGUN));

    // A kill on the last rung wins the match.
    kill(&mut sim, b, a, DamageKind::Bullet);
    let team_a = sim.app.world().get::<Team>(a).unwrap().0;
    assert!(matches!(phase(&sim), Phase::RoundOver { .. }), "{:?}", phase(&sim));
    assert_eq!(sim.app.world().resource::<Match>().winner, Some(team_a));
    sim.seconds(minigame::runtime::ROUND_OVER as f64 + minigame::runtime::INTERMISSION as f64 + 0.2);
    assert_eq!(phase(&sim), Phase::MatchOver);
    assert_eq!(sim.app.world().resource::<Match>().result, Some(Some(team_a)));
    let status = minigame::runtime::status(sim.app.world_mut()).unwrap();
    assert!(status.contains("MatchOver") && status.contains("rung 3/3"), "{status}");

    // `minigame_restart`: everyone again from the first rung.
    minigame::restart(sim.app.world_mut());
    settle(&mut sim);
    assert_eq!(phase(&sim), Phase::Live);
    assert_eq!((rung(&sim, a), rung(&sim, b)), (0, 0));
}

#[test]
fn at_the_time_limit_the_highest_rung_wins() {
    let mut sim = started(&DEF.replace("\"60s\"", "\"3s\""));
    let a = player(&mut sim);
    let b = player(&mut sim);
    settle(&mut sim);
    kill(&mut sim, a, b, DamageKind::Bullet);
    sim.seconds(2.5);
    let team_b = sim.app.world().get::<Team>(b).unwrap().0;
    assert!(matches!(phase(&sim), Phase::RoundOver { .. } | Phase::Intermission { .. }), "{:?}", phase(&sim));
    assert_eq!(sim.app.world().resource::<Match>().winner, Some(team_b));
}

#[test]
fn the_hud_data_shows_the_rung_the_leader_and_the_events() {
    use chimera_core::core::LocalPlayer;
    use chimera_sim::minigame::hud::MinigameHud;
    let mut sim = started(DEF);
    let a = player(&mut sim);
    let b = player(&mut sim);
    sim.app.world_mut().entity_mut(a).insert(LocalPlayer);
    settle(&mut sim);
    let text = |h: &MinigameHud| h.lines.iter().map(|l| l.text.clone()).collect::<Vec<_>>();
    let hud = sim.app.world().resource::<MinigameHud>().clone();
    assert_eq!(hud.title, "Kit gun game");
    assert_eq!(text(&hud)[0], "Rung 1/3: {weapon}");
    assert_eq!(hud.lines[0].weapon, Some(PISTOL));
    assert_eq!(text(&hud)[1], "Next: {weapon}");
    assert_eq!(hud.lines[1].weapon, Some(RIFLE));
    assert!(text(&hud).contains(&"Tied for the lead".to_string()), "{:?}", text(&hud));
    assert!(text(&hud).iter().any(|t| t.starts_with("Time ")), "{:?}", text(&hud));

    // A kill: a rung up and the lead, with a notice.
    kill(&mut sim, b, a, DamageKind::Bullet);
    sim.ticks(2);
    let hud = sim.app.world().resource::<MinigameHud>().clone();
    assert_eq!(text(&hud)[0], "Rung 2/3: {weapon}");
    assert!(text(&hud).contains(&"You lead".to_string()), "{:?}", text(&hud));
    assert_eq!(hud.notices.len(), 1, "{:?}", hud.notices);
    assert_eq!(hud.notices[0].0.text, "Rung up! 2/3: {weapon}");
    assert_eq!(hud.notices[0].0.weapon, Some(RIFLE));

    // The other player leads after two kills to a's one.
    settle(&mut sim);
    kill(&mut sim, a, b, DamageKind::Bullet);
    sim.seconds(1.2);
    settle(&mut sim);
    kill(&mut sim, a, b, DamageKind::Bullet);
    sim.ticks(2);
    let hud = sim.app.world().resource::<MinigameHud>().clone();
    let leader = hud.lines.iter().find(|l| l.text.starts_with("Leader:")).expect("b leads");
    assert_eq!(leader.player, Some(b));

    // Notices go after their seconds; none of it without a minigame.
    sim.seconds(chimera_sim::minigame::hud::NOTICE_SECONDS + 1.0);
    assert!(sim.app.world().resource::<MinigameHud>().notices.is_empty());
    sim.app.world_mut().remove_resource::<chimera_sim::minigame::ActiveMinigame>();
    sim.ticks(2);
    assert!(!sim.app.world().resource::<MinigameHud>().playing());
}
