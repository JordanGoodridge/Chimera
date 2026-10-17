//! CS:S's movement on the greybox: its ladder and water tank work with
//! Source movement (the engine's own movement on it: chimera_sim's
//! tests/it/movement.rs).

use bevy::prelude::*;

use crate::stack::{
    greybox::{self, GreyboxMapPlugin},
    harness::{CsSim, Sim},
};

/// The greybox's ladder and water tank work with Source movement, so they
/// can be tried by hand (`--movement cs_source:movement`).
#[test]
fn greybox_ladder_and_tank_with_source_movement() {
    use crate::stack::games::cs_source::movement::{self as source, SourceMovement, SourceMovementPlugin};
    let state = |sim: &Sim, p: Entity| sim.app.world().get::<SourceMovement>(p).unwrap().clone();

    // In front of the north-wall ladder, facing it (-Z is intent yaw 0).
    let mut sim = Sim::new(CsSim, (GreyboxMapPlugin, SourceMovementPlugin));
    let p = sim.spawn_character(
        Vec3::new(greybox::LADDER_X, 1.0, greybox::NORTH_WALL_Z + 0.1 + 0.43),
        source::ID,
    );
    sim.seconds(0.3);
    sim.intent(p).move_axis = Vec2::Y;
    let y0 = sim.position(p).y;
    sim.seconds(1.0);
    assert!(state(&sim, p).ladder.is_some(), "on the ladder");
    assert!(sim.position(p).y - y0 > 3.0, "climbed {} m", sim.position(p).y - y0);

    // Dropped into the tank: under water, sinking slowly.
    let mut sim = Sim::new(CsSim, (GreyboxMapPlugin, SourceMovementPlugin));
    let c = (greybox::TANK_MIN + greybox::TANK_MAX) / 2.0;
    let p = sim.spawn_character(Vec3::new(c.x, 1.3, c.y), source::ID);
    sim.seconds(0.3);
    assert_eq!(state(&sim, p).water_level, 3, "head under water");
    let vy = sim.velocity(p).y;
    assert!(vy < 0.0 && vy > -1.3, "sinking at {vy} m/s (48 units/s at most)");
}

/// A bot added while the local player flies with noclip moves with the
/// match's movement, not the player's current one (it took the player's
/// slot, so `bot_add` in noclip made a bot that flew through walls): it
/// falls onto the floor like any walking character.
#[test]
fn a_bot_added_while_the_player_flies_falls_onto_the_floor() {
    use crate::stack::{
        bot::{BotConfig, add_bot},
        core::{LocalPlayer, Team},
        games::cs_source::{
            movement::{self as source, SourceMovementPlugin},
            weapons::CsWeaponsPlugin,
        },
        movement::noclip,
        slots::{Loadout, MovementSlot},
    };
    for loadout in [false, true] {
        let mut sim = Sim::new(CsSim, (GreyboxMapPlugin, CsWeaponsPlugin, SourceMovementPlugin));
        if loadout {
            sim.app.insert_resource(Loadout { movement: source::ID });
        }
        sim.app.world_mut().resource_mut::<BotConfig>().stop = 1;
        let me = sim.spawn_character(greybox::SPAWNS[0] + Vec3::Y * 3.0, noclip::ID);
        sim.app.world_mut().entity_mut(me).insert((Team(2), LocalPlayer));
        sim.ticks(1);
        let bot = add_bot(sim.app.world_mut(), Team(1)).expect("a bot");
        sim.ticks(1);
        let slot = |sim: &Sim| sim.app.world().get::<MovementSlot>(bot).map(|m| m.0);
        assert_eq!(slot(&sim), Some(source::ID), "loadout {loadout}");
        // Lifted 2 m, it falls back onto the floor.
        let floor = sim.position(bot).y;
        sim.app.world_mut().get_mut::<Transform>(bot).unwrap().translation.y += 2.0;
        if let Some(mut p) = sim.app.world_mut().get_mut::<avian3d::prelude::Position>(bot) {
            p.0.y += 2.0;
        }
        sim.seconds(1.5);
        assert!(sim.state(bot).on_ground, "loadout {loadout}: landed");
        assert!((sim.position(bot).y - floor).abs() < 0.2, "loadout {loadout}: on the floor");
        // The player is still flying: only the bot changed.
        assert_eq!(sim.app.world().get::<MovementSlot>(me).map(|m| m.0), Some(noclip::ID));
    }
}
