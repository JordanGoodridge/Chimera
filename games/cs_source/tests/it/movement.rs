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
