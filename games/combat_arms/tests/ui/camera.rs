//! The first-person camera's Combat Arms motion (`ui::effects::
//! camera_motion`): the upward smoothing of specs/combat_arms/movement.md
//! ("Eye (camera) height", test case 53) and effects_sounds.md's camera
//! jitter, on a bare app with the shell's camera stood in for (placed at
//! the eye again before each frame, as the shell's `follow_eye` does).

use std::time::Duration;

use bevy::{prelude::*, time::TimeUpdateStrategy};
use chimera_client::client::FirstPersonCamera;
use chimera_core::core::LocalPlayer;
use combat_arms::{
    game::{
        effects::fx::{CaFx, FxJitter},
        inventory::UNIT,
        movement::CaMovement,
    },
    ui::effects::{camera_motion, jitter_offset},
};

/// The eye above the feet standing (u).
const EYE: f32 = 83.0;

struct Rig {
    app: App,
    player: Entity,
    camera: Entity,
}

impl Rig {
    fn new() -> Self {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)))
            .add_systems(PostUpdate, camera_motion);
        let player = app
            .world_mut()
            .spawn((LocalPlayer, Transform::default(), CaMovement::default()))
            .id();
        let camera = app.world_mut().spawn((FirstPersonCamera, Transform::default(), ChildOf(player))).id();
        let mut rig = Self { app, player, camera };
        // Time's first frames have no length.
        rig.frame();
        rig.frame();
        rig
    }

    /// One rendered frame: the shell puts the camera at the eye, then the
    /// game moves it. The camera's world height (u).
    fn frame(&mut self) -> f32 {
        self.app.world_mut().get_mut::<Transform>(self.camera).unwrap().translation = Vec3::Y * EYE * UNIT;
        self.app.update();
        self.world_camera().y / UNIT
    }

    fn world_camera(&self) -> Vec3 {
        let body = self.app.world().get::<Transform>(self.player).unwrap();
        body.transform_point(self.app.world().get::<Transform>(self.camera).unwrap().translation)
    }

    fn feet(&mut self, y: f32) {
        self.app.world_mut().get_mut::<Transform>(self.player).unwrap().translation.y = y * UNIT;
    }
}

/// Test case 53: standing, 60 fps, the hull steps up 30 u in one tick.
#[test]
fn t53_camera_smoothing_up_a_step() {
    let mut rig = Rig::new();
    let start = rig.frame();
    assert!((start - EYE).abs() < 1e-3);
    rig.feet(30.0);
    let heights: Vec<f32> = (0..10).map(|_| rig.frame() - start).collect();
    let want = [5.0, 9.1667, 12.6389, 15.5324, 17.9437, 20.2770, 22.6103, 24.9437, 27.2770, 30.0];
    for (i, (h, w)) in heights.iter().zip(want).enumerate() {
        assert!((h - w).abs() < 0.01, "frame {}: {h}, expected {w} ({heights:?})", i + 1);
    }
    // Downward moves are immediate.
    rig.feet(0.0);
    assert!((rig.frame() - EYE).abs() < 1e-3);
}

/// No smoothing in a jump.
#[test]
fn no_smoothing_in_a_jump() {
    let mut rig = Rig::new();
    rig.frame();
    rig.app.world_mut().get_mut::<CaMovement>(rig.player).unwrap().in_jump = true;
    rig.feet(30.0);
    assert!((rig.frame() - (30.0 + EYE)).abs() < 1e-3);
}

/// A player moving as another game (Lucker Party's CS:S identity: no
/// `CaMovement`) keeps its own eye: a step or a jump up isn't smoothed
/// (it was, every rise lagging: the CS:S movement felt floaty).
#[test]
fn no_smoothing_without_combat_arms_movement() {
    let mut rig = Rig::new();
    rig.app.world_mut().entity_mut(rig.player).remove::<CaMovement>();
    let start = rig.frame();
    rig.feet(30.0);
    assert!((rig.frame() - (start + 30.0)).abs() < 1e-3, "the camera at the eye at once");
}

/// CamJitter: every frame a random offset in [−a, a] u on each axis, a
/// the scale key now × the falloff (full within the inner radius).
#[test]
fn camera_jitter_moves_the_view() {
    let mut rig = Rig::new();
    let at = rig.world_camera();
    rig.app.world_mut().init_resource::<CaFx>();
    rig.app.world_mut().resource_mut::<CaFx>().jitters.push(FxJitter {
        at,
        age: 0.0,
        life: 0.6,
        scale: vec![(0.0, 60.0), (1.0, 0.0)],
        inner: 200.0,
        outer: 800.0,
    });
    let mut moved = 0;
    for _ in 0..20 {
        rig.frame();
        let d = (rig.world_camera() - at) / UNIT;
        assert!(d.abs().max_element() <= 60.0 + 1e-3, "{d}");
        if d.length() > 1.0 {
            moved += 1;
        }
    }
    assert!(moved >= 15, "{moved} of 20 frames shaken");
    // Gone: the camera stays at the eye.
    rig.app.world_mut().resource_mut::<CaFx>().jitters.clear();
    rig.frame();
    assert!((rig.world_camera() - at).length() < 1e-5);
}

#[test]
fn jitter_offsets_stay_in_the_box() {
    assert_eq!(jitter_offset(60.0, [1.0, -1.0, 0.0]), Vec3::new(60.0, -60.0, 0.0) * UNIT);
    assert_eq!(jitter_offset(10.0, [2.0, -3.0, 0.5]), Vec3::new(10.0, -10.0, 5.0) * UNIT);
}
