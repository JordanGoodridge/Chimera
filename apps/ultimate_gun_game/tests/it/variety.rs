//! Variety (backlog, gun game playtest 2026-10-28): ladders from every gun
//! of both games that differ widely by seed, grenade rungs near the top
//! whose grenade comes back once thrown and whose kills climb, and a
//! rotation that picks a random map but the last played with a new
//! ladder for it.

use bevy::prelude::*;
use chimera_core::{
    console::Console,
    core::{Damage, DamageKind, Health, Hitgroup},
};
use chimera_sim::{
    minigame::{self, Minigames, auto, ladder::{Ladder, Rung}, rotation::MapHistory},
    testkit,
    weapon::{Inventory, Weapon, grenade::Throwable},
};
use chimera_world::map::LoadedMapName;
use combat_arms::game::movement;
use ultimate_gun_game::{AutoLadder, MINIGAME, RUNGS, SEED, next_seed};

use crate::parts::{rungs, ugg};

#[test]
fn ladders_draw_on_every_gun_and_differ_by_seed() {
    let sim = ugg();
    let caps = auto::report(sim.app.world());
    let refs: Vec<&auto::Capabilities> = caps.iter().collect();
    let ladders: Vec<Vec<&str>> = (0..500).map(|s| auto::ladder(&refs, RUNGS, s)).collect();
    for c in &caps {
        for g in c.guns.iter().map(|g| g.id).chain(c.grenades.iter().copied()) {
            assert!(ladders.iter().any(|l| l.contains(&g)), "{g} never in 500 ladders");
        }
    }
    let grenade = |id: &&str| caps.iter().any(|c| c.grenades.contains(id));
    let shared: Vec<(usize, usize)> = ladders
        .windows(2)
        .map(|w| {
            let both: Vec<&&str> = w[0].iter().filter(|id| w[1].contains(id)).collect();
            (both.len(), both.iter().filter(|id| !grenade(id)).count())
        })
        .collect();
    let mean = |f: fn(&(usize, usize)) -> usize| shared.iter().map(f).sum::<usize>() as f64 / shared.len() as f64;
    let (all, guns) = (mean(|s| s.0), mean(|s| s.1));
    // The guns: about what drawing 10 of ~38 allows; the grenades are
    // two (CS:S's HE, Combat Arms' M67), so ladders often share one.
    assert!(guns < 3.5, "two ladders share {guns:.1} guns on average");
    assert!(all < 5.0, "two ladders share {all:.1} of {RUNGS} rungs on average");
    assert!(shared.iter().all(|s| s.0 < RUNGS - 3), "never nearly the same");
    for l in &ladders {
        assert!(l.iter().any(|id| id.starts_with("cs_source:")) && l.iter().any(|id| id.starts_with("combat_arms:")));
        let last = l.last().unwrap();
        assert!(!caps.iter().any(|c| c.grenades.contains(last)), "a grenade is never last: {l:?}");
    }
}

/// The ladder with a CS:S HE and a Combat Arms frag rung after `first`.
fn grenade_ladder(first: &'static str) -> Ladder {
    Ladder {
        rungs: vec![
            first,
            "cs_source:weapon_hegrenade",
            "combat_arms:m67",
            "cs_source:weapon_awp",
        ],
        demote_on_melee: true,
        refill_ammo: true,
    }
}

fn held_grenade(world: &World, p: Entity, id: &str) -> Option<u32> {
    let inv = world.get::<Inventory>(p)?;
    inv.weapons
        .iter()
        .find(|w| world.get::<Weapon>(**w).is_some_and(|w| w.id == id))
        .and_then(|w| world.get::<Throwable>(*w))
        .map(|t| t.count)
}

#[test]
fn a_grenade_rung_gives_grenades_until_a_kill_which_climbs() {
    let mut sim = ugg();
    minigame::start(sim.app.world_mut(), MINIGAME).expect("the automatic gun game starts");
    let a = sim.spawn_character(Vec3::new(0.0, 1.0, 0.0), movement::ID);
    let b = sim.spawn_character(Vec3::new(30.0, 1.0, 0.0), movement::ID);
    sim.seconds(1.5);
    let first = sim.app.world().resource::<Ladder>().rungs[0];
    let l = grenade_ladder(first);
    sim.app.world_mut().insert_resource(l.clone());
    for (rung, id) in [(1, "cs_source:weapon_hegrenade"), (2, "combat_arms:m67")] {
        minigame::ladder::set_rung(sim.app.world_mut(), a, rung, &l);
        sim.seconds(1.5);
        assert_eq!(testkit::active_id(sim.app.world(), a), Some(id));
        assert!(held_grenade(sim.app.world(), a, id).is_some_and(|n| n >= 1), "{id} given");
        // Three throws, away from both (the thrower kept alive): each
        // one comes back.
        testkit::aim_at(&mut sim, a, Vec3::new(-200.0, 40.0, 0.0));
        for throw in 0..3 {
            sim.app.world_mut().get_mut::<Health>(a).unwrap().current = 100.0;
            sim.intent(a).fire = true;
            sim.seconds(1.2);
            sim.intent(a).fire = false;
            sim.seconds(2.0);
            let w = sim.app.world();
            assert!(w.get::<chimera_core::core::Dead>(a).is_none(), "{id}: the thrower lives");
            assert!(held_grenade(w, a, id).is_some_and(|n| n >= 1), "{id}: back after throw {throw}");
            assert_eq!(testkit::active_id(w, a), Some(id), "{id}: in hand after throw {throw}");
        }
        // Its blast kills: a rung up.
        sim.seconds(3.0);
        let at = sim.position(b);
        sim.app.world_mut().write_message(Damage {
            target: b,
            attacker: Some(a),
            amount: 500.0,
            point: at,
            dir: Vec3::NEG_Z,
            hitgroup: Hitgroup::Generic,
            kind: DamageKind::Blast,
            weapon: Some(l.rungs[rung]),
            force: Vec3::ZERO,
        });
        sim.ticks(3);
        assert_eq!(sim.app.world().get::<Rung>(a).map(|r| r.0), Some(rung + 1), "{id}'s kill climbs");
        sim.seconds(3.0);
    }
}

#[test]
fn a_rotation_picks_a_fresh_map_with_a_new_ladder() {
    let mut sim = ugg();
    minigame::start(sim.app.world_mut(), MINIGAME).expect("the automatic gun game starts");
    let pool = sim.app.world().resource::<Minigames>().get(MINIGAME).unwrap().maps.clone();
    let mut seed = SEED;
    let mut ladder = rungs(sim.app.world());
    let mut here = "chimera:greybox".to_string();
    let mut played = vec![here.clone()];
    for _ in 0..12 {
        let w = sim.app.world_mut();
        w.insert_resource(LoadedMapName(here.clone()));
        let next = chimera_net::minigame_net::next_match(w).expect("a next match");
        assert_ne!(next, here, "never the same map twice");
        assert!(pool.contains(&next), "{next} from the pool {pool:?}");
        // The last min(10, pool - 1) played are skipped.
        let skip = 10.min(pool.len() - 1);
        let recent: Vec<&String> = played.iter().rev().take(skip).collect();
        assert!(!recent.contains(&&next), "{next} was played lately: {recent:?}");
        // A new ladder from the next seed goes with it.
        seed = next_seed(seed);
        let w = sim.app.world_mut();
        assert_eq!(w.resource::<AutoLadder>().seed, seed);
        let now = rungs(w);
        assert_ne!(now, ladder, "a new ladder for {next}");
        assert_eq!(now.len(), RUNGS);
        ladder = now;
        // The map change done (the sim loads no maps).
        w.remove_resource::<chimera_net::minigame_net::MapChange>();
        w.remove_resource::<minigame::MatchHold>();
        played.push(next.clone());
        here = next;
    }
    let history = sim.app.world().resource::<MapHistory>();
    assert_eq!(history.played.last(), played.get(played.len() - 2), "the host keeps what was played");
    // The console's line says the seed.
    let console = sim.app.world().resource::<Console>();
    let said = format!("seed {seed} ");
    assert!(console.output.iter().any(|l| l.text.contains(&said)), "the console says {said}");
}
