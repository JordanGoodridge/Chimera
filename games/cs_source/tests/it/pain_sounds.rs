//! What players sound like when hurt and killed (`games::cs_source::pain`,
//! the install's `game_sounds.txt` entries): a body shot with and without
//! kevlar, a leg shot, a headshot with and without a helmet, a death, a
//! bot's hits, and which of them also make the flesh impact (only those
//! without a hurt sound).
//! Each plays from the victim: centred for the victim (its own sound),
//! panned by place for a bystander. Falls: games/cs_source/tests/it/source_movement.rs.

use std::sync::Arc;

use bevy::prelude::*;
use crate::stack::{
    audio::sound::Ear,
    bot::{BotConfig, add_bot},
    core::{Health, Team},
    games::cs_source::{
        TICK_INTERVAL,
        weapons::{AK47, CsWeaponsPlugin, DEAGLE, KNIFE},
    },
    greybox::{self, GreyboxMapPlugin},
    harness::{CsSim, Sim},
    map::{MapSounds, MapSurface, PlaySound, SoundLevel, sound::SoundBank},
    movement::placeholder,
    weapon::{Armor, Inventory, give},
};

#[derive(Resource, Default)]
struct Heard(Vec<PlaySound>);

#[derive(Resource, Default)]
struct Impacts(Vec<String>);

fn hear(mut sounds: MessageReader<PlaySound>, mut heard: ResMut<Heard>, mut impacts: ResMut<Impacts>) {
    for s in sounds.read() {
        if s.entry.starts_with("Player.") {
            heard.0.push(s.clone());
        } else {
            impacts.0.push(s.entry.clone());
        }
    }
}

fn sim() -> Sim {
    let mut sim = Sim::new(CsSim, (GreyboxMapPlugin, CsWeaponsPlugin));
    sim.set_tick_interval(TICK_INTERVAL);
    // Surfaces as the install's: players' hitboxes are `flesh`; `player`
    // (the movement box) has no impact sound.
    let mut sounds = MapSounds::default();
    for (name, impact) in [
        ("flesh", Some("Flesh.BulletImpact")),
        ("player", None),
        ("default", Some("Default.BulletImpact")),
    ] {
        sounds.surfaces.insert(
            name.into(),
            MapSurface {
                bullet_impact: impact.map(String::from),
                ..default()
            },
        );
    }
    sim.app.insert_resource(SoundBank(Arc::new(sounds)));
    sim.app
        .init_resource::<Heard>()
        .init_resource::<Impacts>()
        .add_systems(Last, hear);
    sim
}

fn entries(sim: &Sim) -> Vec<String> {
    sim.app
        .world()
        .resource::<Heard>()
        .0
        .iter()
        .map(|s| s.entry.clone())
        .collect()
}

/// Where a shot is aimed.
#[derive(Clone, Copy)]
enum Part {
    Head,
    Chest,
    Leg,
}
use Part::*;

/// How many flesh impacts were heard.
fn flesh(sim: &Sim) -> usize {
    let impacts = &sim.app.world().resource::<Impacts>().0;
    impacts.iter().filter(|e| *e == "Flesh.BulletImpact").count()
}

/// One Desert Eagle shot from 8 m at the target's head, chest or leg; the
/// target has `health` (normalized) and `armor`. Returns the sim, the
/// target and a bystander standing 3 m to the target's side.
fn shot(part: Part, armor: Option<Armor>, health: f32) -> (Sim, Entity, Entity) {
    let mut sim = sim();
    let shooter = sim.spawn_character(greybox::SPAWNS[0], placeholder::ID);
    let target = sim.spawn_character(greybox::SPAWNS[0] - Vec3::Z * 8.0, placeholder::ID);
    let bystander = sim.spawn_character(greybox::SPAWNS[0] - Vec3::Z * 8.0 + Vec3::X * 3.0, placeholder::ID);
    sim.app.world_mut().get_mut::<Health>(target).unwrap().current = health;
    if let Some(a) = armor {
        sim.app.world_mut().entity_mut(target).insert(a);
    }
    sim.ticks(1);
    give(sim.app.world_mut(), shooter, DEAGLE).unwrap();
    sim.seconds(1.2);
    assert!(sim.app.world().get::<Inventory>(shooter).unwrap().active.is_some());
    let height =
        1.8 * match part {
            Head => 0.92,
            Chest => 0.72,
            Leg => 0.25,
        } - 0.9;
    let point = sim.position(target) + Vec3::Y * height;
    let eye = sim.position(shooter) + sim.state(shooter).eye_offset;
    let d = point - eye;
    {
        let mut i = sim.intent(shooter);
        i.yaw = (-d.x).atan2(-d.z);
        i.pitch = d.y.atan2(d.xz().length());
    }
    sim.app.world_mut().resource_mut::<Heard>().0.clear();
    sim.app.world_mut().resource_mut::<Impacts>().0.clear();
    sim.intent(shooter).fire = true;
    sim.ticks(1);
    sim.intent(shooter).fire = false;
    sim.seconds(0.2);
    assert!(
        sim.app.world().get::<Health>(target).unwrap().current < health,
        "the shot missed"
    );
    (sim, target, bystander)
}

const KEVLAR: Armor = Armor {
    amount: 1.0,
    helmet: false,
};
const HELMET: Armor = Armor {
    amount: 1.0,
    helmet: true,
};

/// The one sound named `entry`: made by `victim` where it stands, heard
/// centred by the victim and panned toward it by the bystander.
fn check_heard(sim: &Sim, entry: &str, victim: Entity, bystander: Entity) {
    let heard = &sim.app.world().resource::<Heard>().0;
    let all: Vec<_> = heard.iter().filter(|s| s.entry == entry).collect();
    assert_eq!(all.len(), 1, "{entry} once: {:?}", entries(sim));
    let s = all[0];
    assert_eq!(s.source, Some(victim));
    let at = s.at.expect("positional");
    assert!(at.distance(sim.position(victim)) < 0.5, "at the victim");
    let ear = |who: Entity| Ear {
        at: GlobalTransform::from(
            Transform::from_translation(sim.position(who) + sim.state(who).eye_offset).looking_to(Vec3::NEG_Z, Vec3::Y),
        ),
        owner: Some(who),
    };
    let level = SoundLevel::Attenuation(1.0);
    let (l, r, _) = ear(victim).gains(at, s.source, level);
    assert!(l > 0.99 && (l - r).abs() < 1e-6, "victim: centred ({l}, {r})");
    // The bystander is at +x of the victim, facing -z: the victim is on
    // its left.
    let (l, r, _) = ear(bystander).gains(at, s.source, level);
    assert!(l > r && l < 1.0, "bystander: positional ({l}, {r})");
}

// Measured per hit: specs/cs_source/sounds.md "Hurt and death sounds".

#[test]
fn body_shot_without_armour_is_only_the_flesh_impact() {
    let (sim, ..) = shot(Chest, None, 10.0);
    assert!(entries(&sim).is_empty(), "{:?}", entries(&sim));
    assert_eq!(flesh(&sim), 1);
}

#[test]
fn leg_shot_in_kevlar_is_only_the_flesh_impact() {
    let (sim, ..) = shot(Leg, Some(KEVLAR), 10.0);
    assert!(entries(&sim).is_empty(), "{:?}", entries(&sim));
    assert_eq!(flesh(&sim), 1);
}

#[test]
fn body_shot_on_kevlar_plays_only_the_kevlar_hit() {
    let (sim, target, bystander) = shot(Chest, Some(KEVLAR), 10.0);
    check_heard(&sim, "Player.DamageKevlar", target, bystander);
    assert_eq!(entries(&sim), ["Player.DamageKevlar"]);
    assert_eq!(flesh(&sim), 0, "no flesh impact where the vest takes part");
}

#[test]
fn headshot_without_helmet_plays_only_the_headshot() {
    for armor in [None, Some(KEVLAR)] {
        let (sim, target, bystander) = shot(Head, armor, 10.0);
        check_heard(&sim, "Player.DamageHeadShot", target, bystander);
        assert_eq!(entries(&sim), ["Player.DamageHeadShot"]);
        assert_eq!(flesh(&sim), 0, "no flesh impact on a head");
    }
}

#[test]
fn headshot_on_a_helmet_plays_the_helmet() {
    // Unmeasured (a helmeted head): our guess.
    let (sim, target, bystander) = shot(Head, Some(HELMET), 10.0);
    check_heard(&sim, "Player.DamageHelmet", target, bystander);
    assert_eq!(entries(&sim), ["Player.DamageHelmet"]);
    assert_eq!(flesh(&sim), 0);
}

#[test]
fn deaths_play_the_hit_and_then_the_death_sound() {
    // A body shot that kills: kevlar's hit and the death cry.
    let (sim, target, bystander) = shot(Chest, Some(KEVLAR), 0.05);
    check_heard(&sim, "Player.Death", target, bystander);
    assert_eq!(entries(&sim), ["Player.DamageKevlar", "Player.Death"]);
    assert_eq!(flesh(&sim), 0);
    // Bare: the flesh impact and the death cry.
    let (sim, ..) = shot(Chest, None, 0.05);
    assert_eq!(entries(&sim), ["Player.Death"]);
    assert_eq!(flesh(&sim), 1);
    // A killing headshot: the headshot hurt, then the headshot death.
    let (sim, target, bystander) = shot(Head, None, 0.05);
    check_heard(&sim, "Player.DeathHeadShot", target, bystander);
    assert_eq!(entries(&sim), ["Player.DamageHeadShot", "Player.DeathHeadShot"]);
    assert_eq!(flesh(&sim), 0);
    // Through a helmet: its ding and the headshot death.
    let (sim, ..) = shot(Head, Some(HELMET), 0.05);
    assert_eq!(entries(&sim), ["Player.DamageHelmet", "Player.DeathHeadShot"]);
}

#[test]
fn teammates_hits_refused_by_friendly_fire_are_silent() {
    let mut sim = sim();
    let shooter = sim.spawn_character(greybox::SPAWNS[0], placeholder::ID);
    let target = sim.spawn_character(greybox::SPAWNS[0] - Vec3::Z * 8.0, placeholder::ID);
    for e in [shooter, target] {
        sim.app.world_mut().entity_mut(e).insert(Team(1));
    }
    sim.app.world_mut().entity_mut(target).insert(KEVLAR);
    sim.ticks(1);
    give(sim.app.world_mut(), shooter, DEAGLE).unwrap();
    sim.seconds(1.2);
    let point = sim.position(target) + Vec3::Y * (1.8 * 0.72 - 0.9);
    let d = point - (sim.position(shooter) + sim.state(shooter).eye_offset);
    {
        let mut i = sim.intent(shooter);
        i.yaw = (-d.x).atan2(-d.z);
        i.pitch = d.y.atan2(d.xz().length());
        i.fire = true;
    }
    sim.ticks(1);
    sim.intent(shooter).fire = false;
    sim.seconds(0.2);
    assert_eq!(sim.app.world().get::<Health>(target).unwrap().current, 1.0);
    assert!(entries(&sim).is_empty(), "{:?}", entries(&sim));
}

/// Bots' victims sound the same: a bot sprays an AK-47 at a player in
/// kevlar.
#[test]
fn a_bots_hits_play_the_victims_sounds() {
    let mut sim = sim();
    sim.app.insert_resource(crate::stack::slots::Loadout {
        movement: placeholder::ID,
    });
    {
        let mut cfg = sim.app.world_mut().resource_mut::<BotConfig>();
        cfg.grenades = 0;
        cfg.radio = 0;
        cfg.stop = 1;
    }
    let bot = add_bot(sim.app.world_mut(), Team(1)).unwrap();
    sim.app.world_mut().get_mut::<Transform>(bot).unwrap().translation = Vec3::new(0.0, 1.0, 30.0);
    let enemy = sim.spawn_character(Vec3::new(0.0, 1.0, 15.0), placeholder::ID);
    sim.app.world_mut().entity_mut(enemy).insert((
        Team(2),
        Health {
            current: 1000.0,
            max: 1000.0,
        },
        Armor {
            amount: 1000.0,
            helmet: true,
        },
    ));
    sim.ticks(1);
    give(sim.app.world_mut(), bot, AK47).unwrap();
    sim.seconds(5.0);
    let heard = &sim.app.world().resource::<Heard>().0;
    let hurt = heard
        .iter()
        .filter(|s| s.source == Some(enemy) && (s.entry == "Player.DamageKevlar" || s.entry == "Player.DamageHelmet"))
        .count();
    assert!(hurt >= 5, "the bot's hits: {:?}", entries(&sim));
}

/// One knife hit (slash, or a stab) from 1 m on a target with 100 health
/// and `armor`; returns the sim and the target.
fn knifed(stab: bool, armor: Option<Armor>) -> (Sim, Entity) {
    let mut sim = sim();
    let attacker = sim.spawn_character(greybox::SPAWNS[0], placeholder::ID);
    let target = sim.spawn_character(greybox::SPAWNS[0] - Vec3::Z * 1.0, placeholder::ID);
    if let Some(a) = armor {
        sim.app.world_mut().entity_mut(target).insert(a);
    }
    // Facing the attacker (a stab in the back would be 195).
    sim.intent(target).yaw = std::f32::consts::PI;
    sim.ticks(1);
    give(sim.app.world_mut(), attacker, KNIFE).unwrap();
    sim.seconds(1.2);
    let d = sim.position(target) + Vec3::Y * 0.4 - (sim.position(attacker) + sim.state(attacker).eye_offset);
    {
        let mut i = sim.intent(attacker);
        i.yaw = (-d.x).atan2(-d.z);
        i.pitch = d.y.atan2(d.xz().length());
    }
    sim.app.world_mut().resource_mut::<Heard>().0.clear();
    sim.app.world_mut().resource_mut::<Impacts>().0.clear();
    if stab {
        sim.intent(attacker).secondary = true;
    } else {
        sim.intent(attacker).fire = true;
    }
    sim.ticks(1);
    {
        let mut i = sim.intent(attacker);
        i.fire = false;
        i.secondary = false;
    }
    sim.seconds(0.3);
    (sim, target)
}

/// The knife (bullet damage, hitgroup generic; measured 2026-10-17 on
/// the probe server, specs/cs_source/weapons.md M11): on 100 armour a
/// slash takes 17 health and leaves 98 armour, a stab 55 and 95, and the
/// victim sounds `Player.DamageKevlar` (no flesh impact); on a bare
/// player 20 and 65, and the flesh bullet impact.
#[test]
fn the_knife_on_kevlar_plays_the_kevlar_hit_and_splits_like_a_bullet() {
    let full = Armor {
        amount: 1.0,
        helmet: false,
    };
    for (stab, health, armour) in [(false, 83.0, 98.0), (true, 45.0, 95.0)] {
        let (sim, target) = knifed(stab, Some(full));
        let w = sim.app.world();
        let left = w.get::<Health>(target).unwrap().current * 100.0;
        let vest = w.get::<Armor>(target).unwrap().amount * 100.0;
        assert!((left - health).abs() < 1e-3, "stab {stab}: health {left}");
        assert!((vest - armour).abs() < 1e-3, "stab {stab}: armour {vest}");
        assert_eq!(entries(&sim), ["Player.DamageKevlar"], "stab {stab}");
        assert_eq!(flesh(&sim), 0, "stab {stab}");
    }
    for (stab, health) in [(false, 80.0), (true, 35.0)] {
        let (sim, target) = knifed(stab, None);
        let left = sim.app.world().get::<Health>(target).unwrap().current * 100.0;
        assert!((left - health).abs() < 1e-3, "stab {stab}: health {left}");
        assert!(entries(&sim).is_empty(), "{:?}", entries(&sim));
        assert_eq!(flesh(&sim), 1, "the flesh impact, stab {stab}");
    }
}
