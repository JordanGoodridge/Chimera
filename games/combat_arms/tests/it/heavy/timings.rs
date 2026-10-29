//! Every Combat Arms copy in the gun game's pool (a `class`) fires,
//! reloads and draws on its own view model's animations, from the user's
//! install (docs/plans/active/ca-weapon-timings.md): each weapon with a
//! `combat_arms:timing` table is built as the game builds it (its parts,
//! its inventory item, the timing hook) and its cycle, reload and draw
//! must be its model's `fire_0` / (1 + `FireAniRate`), `reload` and
//! `select`. Prints the counts (copies whose template's numbers already
//! were their own, before; read, after; the read times) and the first
//! few that differ.
//!
//! Needs the V1 key (the weapons file) and, for the rifles', SMGs' and
//! snipers' view models, the V2 key in chimera.local.toml. Skips with a
//! note without the first; models that don't read are counted.
//! `--nocapture` for the counts.

use std::time::Instant;

use bevy::prelude::*;
use chimera_core::mount::config::LocalConfig;
use chimera_sim::weapon::{Magazine, ShellReload, Trigger, Weapon, data::insert_parts};
use combat_arms::game::{
    GAME,
    attachments::fire_animation_rate,
    data::CaData,
    inventory::CaItem,
    weapons::timing::{self, TIMING, TimingParams, ViewTimes},
};

#[test]
fn every_copy_times_on_its_own_view_model() {
    if LocalConfig::load().ok().and_then(|c| c.game_path(GAME)).is_none() {
        eprintln!("skipping: no Combat Arms install configured");
        return;
    }
    let ca = CaData::install();
    if ca.attributes().and_then(|a| a.weapons.as_ref()).is_none() {
        eprintln!("skipping: the install's weapons file isn't readable (the V1 key)");
        return;
    }
    let (mut pool, mut timed, mut unread, mut no_fire, mut own_before, mut own_after) = (0, 0, 0, 0, 0, 0);
    let (mut read_s, mut slowest) = (0.0f64, (0.0f64, String::new()));
    let mut wrong = Vec::new();
    let mut paths = std::collections::HashSet::new();
    let mut world = World::new();
    world.init_resource::<ViewTimes>();
    for d in &ca.weapon_set().weapons {
        if d.class.is_none() {
            continue;
        }
        pool += 1;
        if !d.hooks.contains_key(TIMING) {
            continue;
        }
        timed += 1;
        let path = d.models.view.as_deref().unwrap();
        paths.insert(path.to_string());
        let key = combat_arms::game::model::normalize(path);
        let t = match world.resource::<ViewTimes>().0.get(&key) {
            Some(t) => *t,
            None => {
                let t0 = Instant::now();
                let t = timing::read(path);
                let took = t0.elapsed().as_secs_f64();
                read_s += took;
                if took > slowest.0 {
                    slowest = (took, path.to_string());
                }
                world.resource_mut::<ViewTimes>().0.insert(key, t);
                t
            }
        };
        let Some(t) = t else {
            unread += 1;
            println!("  unread: {} ({path})", d.id);
            continue;
        };
        let Some(fire) = t.fire else {
            no_fire += 1;
            println!("  no fire_0: {} ({path})", d.id);
            continue;
        };
        let item: Option<CaItem> =
            d.hooks.get("combat_arms:inventory").map(|v| v.clone().try_into().expect("inventory table"));
        let rate = item.as_ref().map_or(0.0, |i| i.fire_rate);
        let cycle = fire / fire_animation_rate(rate, 0.0);
        let template = d.trigger.as_ref().map(|tr| tr.cycle);
        if template.is_some_and(|c| (c - cycle).abs() < 1e-6) {
            own_before += 1;
        }
        // Built as the game builds it.
        let mut e = world.spawn_empty();
        insert_parts(d, &mut e);
        if let Some(i) = item {
            e.insert(i);
        }
        timing::apply(&mut e, d, &TimingParams::default());
        let built = e.get::<Trigger>().map(|tr| tr.cycle);
        let reload = e.get::<ShellReload>().map(|s| s.insert).or(e.get::<Magazine>().map(|m| m.reload_time));
        let draw = e.get::<Weapon>().map(|w| w.draw_time);
        let ok = built.is_some_and(|c| (c - cycle).abs() < 1e-6)
            && t.reload.is_none_or(|r| reload == Some(r))
            && t.select.is_none_or(|s| draw.is_some_and(|dr| (dr - s).abs() < 1e-6 || item_select(d) != 0.0));
        if ok {
            own_after += 1;
        } else {
            wrong.push(format!("{}: cycle {built:?} (model {cycle}), reload {reload:?} ({:?}), draw {draw:?} ({:?})", d.id, t.reload, t.select));
        }
        e.despawn();
    }
    println!(
        "pool {pool}; {timed} copies timed by their view model: {own_after} on their own timing (before: {own_before} by chance); \
         {unread} models unread, {no_fire} without fire_0; reading {} distinct models took {read_s:.2} s ({:.1} ms a model, slowest {:.1} ms {})",
        paths.len(),
        1000.0 * read_s / paths.len().max(1) as f64,
        1000.0 * slowest.0,
        slowest.1
    );
    for w in wrong.iter().take(20) {
        println!("  {w}");
    }
    assert!(timed > 0, "no copy has a timing table");
    assert!(wrong.is_empty(), "{} copies don't time on their models", wrong.len());
}

/// A weapon's item select rate (a draw then isn't the bare length).
fn item_select(d: &chimera_sim::weapon::data::WeaponData) -> f32 {
    d.hooks
        .get("combat_arms:inventory")
        .and_then(|v| v.get("select_rate"))
        .and_then(|v| v.as_float())
        .unwrap_or(0.0) as f32
}
