//! Headless simulation for tests and tools: no window, no rendering, time
//! advanced by exact fixed ticks. Game-neutral: the simulation's plugins
//! come in as the `base` a `Sim` is built on (a game's set, e.g.
//! `cs_source::harness::CsSim`; the engine's alone, e.g.
//! `chimera_sim::SimBasePlugins`), so a crate's tests use the harness at
//! the layer they test (docs/OBSERVABILITY.md, section 1).
//!
//! `Sim`: a headless app stepped by exact fixed ticks (the network's
//! server-and-clients `NetSim` is `chimera_net::harness`); `add_headless`
//! / `add_headless_with`: the engine without a window, as `Sim` and the
//! dedicated server run it; `Sim::pad` (and `CHIMERA_TEST_PAD=<n>` for every
//! `Sim`) spawns entities and a resource first, so a test can check its
//! result doesn't depend on entity ids; `emulate_mesh_extraction` stands in
//! for the render world's mesh extraction (takes a `RENDER_WORLD`-only
//! mesh's data, `MeshExtraction` keeps the copies and the meshes announced
//! changed after their data was taken: Bevy's "already been extracted"
//! error); `system_order` lists every schedule's systems;
//! `single_threaded_schedules` puts the main world's schedules on one
//! thread (the game, the dedicated server, `perfgate`).

use std::time::Duration;

use avian3d::prelude::*;
use bevy::{prelude::*, time::TimeUpdateStrategy};
use chimera_core::{
    character::character_bundle,
    core::{DEFAULT_TICK_HZ, Intent, MovementState, SimTick, Team, Velocity},
    slots::set_movement,
};

pub struct Sim {
    pub app: App,
}

/// The engine with no window or rendering: what `Sim` runs (stepped by
/// hand) under its base plugins.
pub fn add_headless(app: &mut App) {
    add_headless_with(app, MinimalPlugins.build());
}

/// The same with `minimal` (`MinimalPlugins`, e.g. with a run loop that
/// waits between frames: the dedicated server).
pub fn add_headless_with(app: &mut App, minimal: bevy::app::PluginGroupBuilder) {
    app.add_plugins((
        minimal,
        TransformPlugin,
        // avian watches mesh assets even when no collider uses them.
        AssetPlugin::default(),
        bevy::mesh::MeshPlugin,
        // Interpolation eases rendering between ticks; headless, it
        // would make `Transform` show the previous tick.
        PhysicsPlugins::default()
            .with_collision_hooks::<crate::map::MapCollisionHooks>()
            .build()
            .disable::<PhysicsInterpolationPlugin>(),
    ));
}

/// `Sim::pad`'s resource.
#[derive(Resource)]
struct Padding(#[allow(dead_code)] usize);

/// `CHIMERA_TEST_PAD`, or 0.
fn pad_from_env() -> usize {
    chimera_core::env::var("CHIMERA_TEST_PAD").ok().and_then(|v| v.parse().ok()).unwrap_or(0)
}

impl Sim {
    /// A headless app with the simulation `base` (a game's set of plugins,
    /// or the engine's) plus `plugins` (a map, and any game plugins whose
    /// implementations the test uses).
    pub fn new<B, M>(base: impl bevy::app::Plugins<B>, plugins: impl bevy::app::Plugins<M>) -> Self {
        Self::with(base, |app| {
            app.add_plugins(plugins);
        })
    }

    /// Like `new`, with what `setup` adds after `base` (a `NetSim` builds
    /// several apps from one setup).
    pub fn with<B>(base: impl bevy::app::Plugins<B>, setup: impl FnOnce(&mut App)) -> Self {
        let mut app = App::new();
        add_headless(&mut app);
        app.add_plugins(base);
        setup(&mut app);
        Self::start(app)
    }

    fn start(mut app: App) -> Self {
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / DEFAULT_TICK_HZ,
        )));
        app.finish();
        app.cleanup();
        // Startup, and one tick so the physics spatial index contains the map.
        app.update();
        let mut sim = Self { app };
        sim.pad(pad_from_env());
        sim.ticks(1);
        sim
    }

    /// Spawn `n` empty entities and a resource: every entity spawned
    /// after gets another id (Bevy 0.19 resources are entities too).
    /// Nothing may depend on entity ids; `CHIMERA_TEST_PAD=<n>` pads
    /// every `Sim` this way, to check a test's result doesn't change.
    pub fn pad(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        let world = self.app.world_mut();
        for _ in 0..n {
            world.spawn_empty();
        }
        world.insert_resource(Padding(n));
    }

    /// Run fixed ticks of `secs` from now on (e.g. a game's own tick).
    pub fn set_tick_interval(&mut self, secs: f64) {
        let step = Duration::from_secs_f64(secs);
        self.app.insert_resource(Time::<Fixed>::from_duration(step));
        self.app.insert_resource(TimeUpdateStrategy::ManualDuration(step));
    }

    pub fn tick_hz(&self) -> f64 {
        self.app
            .world()
            .resource::<Time<Fixed>>()
            .timestep()
            .as_secs_f64()
            .recip()
    }

    /// Advance exactly `n` fixed ticks.
    pub fn ticks(&mut self, n: u64) {
        let target = self.tick() + n;
        while self.tick() < target {
            self.app.update();
        }
    }

    /// Advance by whole ticks covering `secs` seconds.
    pub fn seconds(&mut self, secs: f64) {
        self.ticks((secs * self.tick_hz()).round() as u64);
    }

    pub fn tick(&self) -> u64 {
        self.app.world().resource::<SimTick>().0
    }

    pub fn spawn_character(&mut self, position: Vec3, movement: &'static str) -> Entity {
        let world = self.app.world_mut();
        let entity = world
            .spawn(character_bundle(Transform::from_translation(position), Team(0)))
            .id();
        set_movement(entity, movement).apply(world);
        entity
    }

    pub fn set_movement(&mut self, entity: Entity, movement: &'static str) {
        set_movement(entity, movement).apply(self.app.world_mut());
    }

    pub fn intent(&mut self, entity: Entity) -> Mut<'_, Intent> {
        self.app
            .world_mut()
            .get_mut::<Intent>(entity)
            .expect("entity has no Intent")
    }

    pub fn position(&self, entity: Entity) -> Vec3 {
        self.app
            .world()
            .get::<Transform>(entity)
            .expect("no Transform")
            .translation
    }

    pub fn velocity(&self, entity: Entity) -> Vec3 {
        self.app.world().get::<Velocity>(entity).expect("no Velocity").0
    }

    pub fn state(&self, entity: Entity) -> MovementState {
        self.app
            .world()
            .get::<MovementState>(entity)
            .expect("no MovementState")
            .clone()
    }
}

/// What `emulate_mesh_extraction` saw: each mesh as the render world
/// would hold it, and the meshes it couldn't extract.
#[derive(Resource, Default)]
pub struct MeshExtraction {
    /// The last data extracted per mesh (the render world's copy).
    pub extracted: std::collections::HashMap<AssetId<Mesh>, Mesh>,
    /// Meshes announced as added or modified whose data was already taken:
    /// in a real run, Bevy's "RenderMesh with RenderAssetUsages ==
    /// RENDER_WORLD cannot be extracted: The asset has already been
    /// extracted" error.
    pub failures: Vec<AssetId<Mesh>>,
}

/// Headless stand-in for the render world's mesh extraction, so a test can
/// catch a mesh announced as changed after the render world took its
/// data: each frame (in `Last`, after the asset events go out in
/// `PostUpdate`) a mesh added or modified this frame is extracted as Bevy
/// does: a `RENDER_WORLD`-only one has its data taken from the main world,
/// others are copied. Readers of such meshes look in `MeshExtraction`.
pub fn emulate_mesh_extraction(app: &mut App) {
    app.init_resource::<MeshExtraction>().add_systems(Last, extract_meshes);
}

fn extract_meshes(
    mut events: MessageReader<AssetEvent<Mesh>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    mut out: ResMut<MeshExtraction>,
) {
    let Some(mut meshes) = meshes else {
        events.clear();
        return;
    };
    let mut due: Vec<AssetId<Mesh>> = Vec::new();
    for e in events.read() {
        match e {
            AssetEvent::Added { id } | AssetEvent::Modified { id } if !due.contains(id) => due.push(*id),
            _ => {}
        }
    }
    use bevy::asset::RenderAssetUsages;
    for id in due {
        let Some(mesh) = meshes.get_mut_untracked(id) else { continue };
        if !mesh.asset_usage.contains(RenderAssetUsages::RENDER_WORLD) {
            continue;
        }
        if mesh.asset_usage == RenderAssetUsages::RENDER_WORLD {
            match mesh.take_gpu_data() {
                Ok(data) => {
                    out.extracted.insert(id, data);
                }
                Err(_) => out.failures.push(id),
            }
        } else {
            let copy = mesh.clone();
            out.extracted.insert(id, copy);
        }
    }
}

/// Every schedule's systems in the order they run (built now if they
/// haven't run yet), one schedule after another by name: the spawn dump
/// (`games/cs_source/tests/it/heavy/map_spawn_dump.rs`) and the game's
/// `CHIMERA_SYSTEM_DUMP` (apps/cs_source/src/main.rs), for refactors that
/// move systems between plugins or crates.
pub fn system_order(world: &mut World) -> String {
    use std::fmt::Write as _;
    let mut labels: Vec<(String, bevy::ecs::schedule::InternedScheduleLabel)> = world
        .resource::<Schedules>()
        .iter()
        .map(|(label, s)| (format!("{label:?}"), s.label()))
        .collect();
    labels.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = String::new();
    for (name, label) in labels {
        let _ = writeln!(out, "{name}:");
        world.schedule_scope(label, |world, schedule| {
            schedule.initialize(world).expect("schedule builds");
            for (_, system) in schedule.systems().expect("initialized") {
                let _ = writeln!(out, "  {}", system.name());
            }
        });
    }
    out
}

/// Run the main world's per-frame and fixed schedules on one thread each
/// instead of Bevy's multi-threaded executor: the game (whose main.rs
/// does the render world's too), the dedicated server and `perfgate`.
///
/// A tick runs a few hundred systems, nearly all tiny: handing each to a
/// worker thread and waking the next cost more than the systems did, and
/// much more on a busy machine (docs/performance.md, "Frame time pass";
/// a 32-player server spent half its CPU time on it: multiplayer.md,
/// "Capacity"). Systems that iterate many entities still spread over the
/// task pool themselves.
pub fn single_threaded_schedules(app: &mut App) {
    use bevy::ecs::{
        intern::Interned,
        schedule::{ScheduleLabel, SingleThreadedExecutor},
    };
    let labels: [Interned<dyn ScheduleLabel>; 10] = [
        First.intern(),
        PreUpdate.intern(),
        Update.intern(),
        PostUpdate.intern(),
        Last.intern(),
        FixedFirst.intern(),
        FixedPreUpdate.intern(),
        FixedUpdate.intern(),
        FixedPostUpdate.intern(),
        FixedLast.intern(),
    ];
    for label in labels {
        app.edit_schedule(label, |s| {
            s.set_executor(SingleThreadedExecutor::new());
        });
    }
}
