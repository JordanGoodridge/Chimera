//! The original gun game (`originals:gun_game_dust2`, minigames/
//! gun_game_dust2.toml) on the party's simulation (`PartyPartsPlugin` on
//! `combat_arms::harness::CaSim`), headless on the greybox with scripted
//! Combat Arms soldiers: the definition checks against both games' parts
//! and takes the gameplay's rules; a scripted player shooting another
//! with each rung's weapon (its own aim and trigger, the weapon frame's
//! hits) climbs the ladder of CS:S's and Combat Arms' guns to the win; a
//! death to Combat Arms' knife costs a rung.

use avian3d::prelude::Position;
use bevy::prelude::*;
use chimera_core::{
    core::{Dead, God, Score, Team},
    gameplay::Gameplay,
};
use chimera_sim::{
    minigame::{self, MINIGAME, MinigameEvent, ladder::Rung},
    mode::{Match, Phase},
    testkit,
    weapon::{Inventory, Weapon},
};
use chimera_world::{greybox::GreyboxMapPlugin, harness::Sim};
use combat_arms::{game::movement, harness::CaSim};
use lucker_party::{DEFAULT_MINIGAME, PartyPartsPlugin};

/// The party's simulation on the greybox, the gun game started.
fn party() -> Sim {
    let mut sim = Sim::new(CaSim, (GreyboxMapPlugin, PartyPartsPlugin));
    minigame::start(sim.app.world_mut(), DEFAULT_MINIGAME).expect("the gun game starts");
    sim
}

/// A Combat Arms soldier, not yet in the game (the minigame takes it in).
fn soldier(sim: &mut Sim) -> Entity {
    sim.spawn_character(Vec3::new(0.0, 1.0, 0.0), movement::ID)
}

fn place(sim: &mut Sim, e: Entity, at: Vec3) {
    let w = sim.app.world_mut();
    w.get_mut::<Transform>(e).unwrap().translation = at;
    w.get_mut::<Position>(e).unwrap().0 = at;
}

fn held(sim: &Sim, e: Entity) -> Option<&'static str> {
    testkit::active_id(sim.app.world(), e)
}

fn rung(sim: &Sim, e: Entity) -> usize {
    sim.app.world().get::<Rung>(e).map_or(0, |r| r.0)
}

fn phase(sim: &Sim) -> Phase {
    sim.app.world().resource::<Match>().phase
}

fn alive(sim: &Sim, e: Entity) -> bool {
    sim.app.world().get::<Dead>(e).is_none()
}

/// `shooter` aims at `target`'s centre and works the trigger (pressed
/// every other tick, for the semi-automatics) until `target` dies or
/// `secs` pass; whether it died.
fn shoot_until_dead(sim: &mut Sim, shooter: Entity, target: Entity, secs: f64) -> bool {
    let ticks = (secs * sim.tick_hz()) as u64;
    for t in 0..ticks {
        let at = sim.position(target);
        testkit::aim_at(sim, shooter, at);
        sim.intent(shooter).fire = t % 2 == 0;
        sim.ticks(1);
        if !alive(sim, target) {
            sim.intent(shooter).fire = false;
            return true;
        }
    }
    sim.intent(shooter).fire = false;
    false
}

/// `target` back in play, unprotected, `d` m in front of `shooter` (both
/// standing still), the shooter's weapon drawn.
fn line_up(sim: &mut Sim, shooter: Entity, target: Entity, d: f32) {
    for _ in 0..(6.0 * sim.tick_hz()) as u32 {
        if alive(sim, target) && sim.app.world().get::<God>(target).is_none() {
            break;
        }
        sim.ticks(1);
    }
    assert!(alive(sim, target), "respawned");
    let from = sim.position(shooter);
    place(sim, target, from + Vec3::new(0.0, 0.0, -d));
    // Settle, and finish drawing what the last kill gave.
    sim.seconds(1.3);
}

#[test]
fn the_gun_game_checks_against_both_games_and_takes_the_rules() {
    let mut sim = party();
    let g = sim.app.world().resource::<Gameplay>().clone();
    assert_eq!((g.rules, g.hud, g.binds), (MINIGAME, "combat_arms", "combat_arms"));
    assert_eq!(g.tick, combat_arms::game::INFO.tick(), "Combat Arms' tick");
    let a = soldier(&mut sim);
    let b = soldier(&mut sim);
    sim.seconds(1.5);
    assert_eq!(phase(&sim), Phase::Live);
    for p in [a, b] {
        assert_eq!(held(&sim, p), Some(cs_source::game::weapons::GLOCK), "the first rung: CS:S's Glock");
        let ids: Vec<&str> = {
            let w = sim.app.world();
            w.get::<Inventory>(p).unwrap().weapons.iter().filter_map(|x| w.get::<Weapon>(*x)).map(|x| x.id).collect()
        };
        assert!(ids.contains(&combat_arms::game::inventory::KNIFE), "the kit's Combat Arms knife: {ids:?}");
    }
}

/// A scripted player shoots the other with every rung's gun, CS:S's and
/// Combat Arms' in turn, to the last rung's kill and the match's end.
#[test]
fn a_full_gun_game_runs_to_a_winner_with_both_games_guns() {
    let mut sim = party();
    let a = soldier(&mut sim);
    let b = soldier(&mut sim);
    sim.seconds(1.0);
    assert_eq!(phase(&sim), Phase::Live);
    let mut fired_with = Vec::new();
    let mut events = Vec::new();
    for kill in 0..10 {
        line_up(&mut sim, a, b, 4.0);
        let gun = held(&sim, a).expect("a weapon in hand");
        assert!(
            shoot_until_dead(&mut sim, a, b, 8.0),
            "kill {kill}: {gun} didn't kill in 8 s (health {:?})",
            sim.app.world().get::<chimera_core::core::Health>(b).map(|h| h.current)
        );
        fired_with.push(gun);
        events.extend(
            sim.app
                .world_mut()
                .resource_mut::<Messages<MinigameEvent>>()
                .drain()
                .map(|e| (e.name, e.value)),
        );
        if kill < 9 {
            assert_eq!(rung(&sim, a), kill + 1, "a rung a kill");
        }
    }
    assert!(fired_with.iter().any(|g| g.starts_with("cs_source:")), "{fired_with:?}");
    assert!(fired_with.iter().any(|g| g.starts_with("combat_arms:")), "{fired_with:?}");
    assert_eq!(fired_with.last(), Some(&cs_source::game::weapons::AWP), "the last rung");
    assert!(events.contains(&("rung_up", 9)), "{events:?}");
    let team_a = sim.app.world().get::<Team>(a).unwrap().0;
    assert_eq!(sim.app.world().resource::<Match>().winner, Some(team_a));
    sim.seconds(9.0);
    assert_eq!(phase(&sim), Phase::MatchOver);
    assert_eq!(sim.app.world().resource::<Match>().result, Some(Some(team_a)));
    assert_eq!(sim.app.world().get::<Score>(a).unwrap().kills, 10);
    assert_eq!(sim.app.world().get::<Score>(b).unwrap().deaths, 10);
}

/// Combat Arms' knife (the kit's) kills a player on the fourth rung: it
/// goes back to the third, CS:S's Desert Eagle at its respawn.
#[test]
fn a_knife_death_costs_a_rung() {
    let mut sim = party();
    let a = soldier(&mut sim);
    let b = soldier(&mut sim);
    sim.seconds(1.0);
    sim.app.world_mut().entity_mut(b).insert(Rung(3));
    // a takes out its knife.
    let knife = {
        let w = sim.app.world();
        let inv = w.get::<Inventory>(a).unwrap();
        inv.weapons
            .iter()
            .copied()
            .find(|x| w.get::<Weapon>(*x).is_some_and(|x| x.id == combat_arms::game::inventory::KNIFE))
            .unwrap()
    };
    sim.app.world_mut().get_mut::<Inventory>(a).unwrap().wanted = Some(knife);
    line_up(&mut sim, a, b, 1.2);
    assert_eq!(held(&sim, a), Some(combat_arms::game::inventory::KNIFE));
    assert!(shoot_until_dead(&mut sim, a, b, 8.0), "knifed");
    assert_eq!(rung(&sim, b), 2, "a rung down");
    assert_eq!(rung(&sim, a), 1, "the killer still climbs");
    line_up(&mut sim, a, b, 4.0);
    assert_eq!(held(&sim, b), Some(cs_source::game::weapons::DEAGLE));
}

/// Combat Arms' bots (the gun game's `bots` slot) come in, each on a team
/// of its own, and fight it out with the ladder's guns: kills move them up.
#[test]
fn combat_arms_bots_play_the_gun_game() {
    let mut sim = party();
    // Eight more start points about the greybox's floor (it has two).
    for x in [-8.0, 8.0] {
        for z in [-25.0, -15.0, -5.0, 5.0] {
            sim.app.world_mut().spawn((
                chimera_core::core::SpawnPoint::default(),
                Transform::from_translation(Vec3::new(x, 1.0, z)),
            ));
        }
    }
    {
        let mut s = sim.app.world_mut().resource_mut::<combat_arms::game::bots::BotSettings>();
        s.quota = 4;
        s.quota_set = true;
    }
    sim.seconds(2.0);
    let bots: Vec<Entity> = {
        let w = sim.app.world_mut();
        w.query_filtered::<Entity, With<combat_arms::game::bots::CaBot>>().iter(w).collect()
    };
    assert_eq!(bots.len(), 4);
    assert_eq!(phase(&sim), Phase::Live);
    let mut teams: Vec<u8> = bots.iter().map(|b| sim.app.world().get::<Team>(*b).unwrap().0).collect();
    teams.sort();
    teams.dedup();
    assert_eq!(teams.len(), 4, "a team each: {teams:?}");
    let in_hand: Vec<Option<&str>> = bots.iter().map(|b| held(&sim, *b)).collect();
    assert!(
        in_hand.iter().all(|h| [Some(cs_source::game::weapons::GLOCK), Some(combat_arms::game::inventory::KNIFE)].contains(h)),
        "the first rung's Glock or the kit's knife: {in_hand:?}"
    );
    let mut kills = 0;
    for _ in 0..90 {
        sim.seconds(1.0);
        kills = bots.iter().map(|b| sim.app.world().get::<Score>(*b).map_or(0, |s| s.kills)).sum::<i32>();
        if kills >= 2 {
            break;
        }
    }
    assert!(kills >= 2, "the bots kill each other ({kills} kills in 90 s)");
    assert!(bots.iter().any(|b| rung(&sim, *b) > 0), "and climb");
}
