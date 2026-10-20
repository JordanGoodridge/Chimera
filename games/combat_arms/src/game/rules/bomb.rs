//! The mission bomb (specs/combat_arms/game_modes.md M3, M9): planting,
//! defusing and the detonation, shared by Search & Destroy and Bombing
//! Run, on the map's bomb sites (`objects::BombZone`).
//!
//! - Plant: a player holding the bomb (`objectives::BOMB`, in hand) and
//!   fire, inside a site's box widened by `plant_margin` (50 u) on each
//!   axis, of a site its own team doesn't defend (`DefusableTeam` is the
//!   other team), while the round counts and is undecided and no bomb was
//!   planted this round, fills the plant gauge (`bomb_plant`, 4 s;
//!   `objectives::CaAction`); letting go of fire, leaving the box or
//!   putting the bomb away empties it. When it fills: the bomb is planted
//!   where the planter stands, its fuse the site's `DetonateTime` (40 s),
//!   the round timer gets the fuse added to what is left, the planter's
//!   bomb is gone, its plants +1. Only one bomb a round.
//! - Defuse: a living player of the site's defending team inside the
//!   planted site's box (no margin) holding use fills the defuse gauge
//!   (the site's `DefuseTime`, 7 s); letting go cancels it (progress
//!   lost). When it fills while the round counts and is undecided: the
//!   defenders win the round, the defuser's defuses +1, the bomb is gone.
//! - Detonation: at the fuse's end, while the round counts and is
//!   undecided, the planting team wins and the planter's detonations +1;
//!   the bomb is gone either way.
//!
//! Known shortcuts: the detonation deals no damage and shows no blast;
//! the sites' `PlantedCommand` isn't run (the other site closes by the
//! one-bomb rule); a plant needn't stand still (Open question 4); a
//! partial defuse keeps nothing.

use bevy::prelude::*;

use super::{
    CaRuleSettings,
    objectives::{self, ActionKind, BOMB, CaObjectives, Planted},
    objects::CaMapObjects,
    scoring::stats,
    state::CaMatch,
};
use crate::core::SimClock;

/// Who planted this round's bomb (the server's; credited at the
/// detonation).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct CaPlanter(pub Option<Entity>);

/// The sites of `family` (S&D's `Doobic`, Bombing Run's `Sabotage`); every
/// site when the map has none of that family.
pub fn zones(world: &World, family: &str) -> Vec<(usize, super::objects::BombZone)> {
    let all = &world.resource::<CaMapObjects>().bomb_zones;
    let mine: Vec<_> = all.iter().cloned().enumerate().filter(|(_, z)| z.family == family).collect();
    if mine.is_empty() {
        all.iter().cloned().enumerate().collect()
    } else {
        mine
    }
}

/// The round counts and is undecided.
fn open(world: &World) -> bool {
    let m = world.resource::<CaMatch>();
    m.counting() && m.winner.is_none()
}

/// One tick of planting, defusing and the fuse on `family`'s sites.
pub fn update(world: &mut World, family: &str) {
    let now = world.resource::<SimClock>().now;
    let o = world.resource::<CaRuleSettings>().objectives.clone();
    let sites = zones(world, family);
    let players = objectives::takers(world);
    let planted = world.resource::<CaObjectives>().planted;
    // Plants.
    for p in &players {
        let site = sites.iter().find(|(_, z)| {
            z.defusers != p.team && z.area.contains(p.at, objectives::units(o.plant_margin))
        });
        let on = planted.is_none()
            && open(world)
            && p.intent.fire
            && site.is_some()
            && objectives::holding(world, p.e, BOMB);
        if objectives::hold_action(world, p.e, ActionKind::Plant, o.bomb_plant, on)
            && let Some((index, z)) = site
        {
            let fuse = if z.detonate_time > 0.0 { z.detonate_time } else { o.bomb_fuse };
            world.resource_mut::<CaObjectives>().planted = Some(Planted {
                zone: *index,
                number: z.number,
                at: p.at,
                team: p.team,
                detonates: now + fuse as f64,
            });
            if let Some(ends) = &mut world.resource_mut::<CaMatch>().round_ends {
                *ends += fuse as f64;
            }
            world.resource_mut::<CaPlanter>().0 = Some(p.e);
            objectives::take_weapon(world, p.e, BOMB);
            let mut c = objectives::carry(world, p.e);
            c.bomb = false;
            objectives::set_carry(world, p.e, c);
            stats(world, p.e).plants += 1;
            info!("bomb planted at site {} ({:?})", z.number, p.e);
            return;
        }
    }
    let Some(bomb) = planted else { return };
    // Defuses.
    let Some(z) = world.resource::<CaMapObjects>().bomb_zones.get(bomb.zone).cloned() else {
        world.resource_mut::<CaObjectives>().planted = None;
        return;
    };
    let length = if z.defuse_time > 0.0 { z.defuse_time } else { o.bomb_defuse };
    for p in &players {
        let on = z.can_defuse && p.team == z.defusers && p.intent.use_key && z.area.contains(p.at, 0.0);
        if objectives::hold_action(world, p.e, ActionKind::Defuse, length, on) {
            world.resource_mut::<CaObjectives>().planted = None;
            if open(world) {
                stats(world, p.e).defuses += 1;
                world.resource_mut::<CaMatch>().win(z.defusers);
                info!("bomb defused by {:?}", p.e);
            }
            return;
        }
    }
    // The fuse.
    if now + 1e-9 >= bomb.detonates {
        world.resource_mut::<CaObjectives>().planted = None;
        if open(world) {
            if let Some(planter) = world.resource::<CaPlanter>().0
                && world.get_entity(planter).is_ok()
            {
                stats(world, planter).detonations += 1;
            }
            world.resource_mut::<CaMatch>().win(bomb.team);
            info!("bomb detonated");
        }
    }
}

/// A bomb is ticking.
pub fn ticking(world: &World) -> bool {
    world.resource::<CaObjectives>().planted.is_some()
}

/// The round's end: every bomb taken from its holder, the planted one
/// gone.
pub fn remove_all(world: &mut World) {
    let holders: Vec<Entity> = super::flow::players(world).into_iter().map(|(e, _)| e).collect();
    for e in holders {
        objectives::take_weapon(world, e, BOMB);
    }
    let mut o = world.resource_mut::<CaObjectives>();
    o.planted = None;
    o.loose_bomb = None;
    world.resource_mut::<CaPlanter>().0 = None;
}
