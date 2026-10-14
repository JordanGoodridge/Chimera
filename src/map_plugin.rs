//! Builds `map::MapPlugin`: the map's own systems (`map`), the audio
//! that plays its sounds (`audio`) and what draws it (`render`: the drawn
//! phases of spawning a map), registered in one fixed order (the order
//! decides how Bevy orders systems that aren't ordered explicitly). The
//! plugin's type stays in `map`, so `map::MapPlugin` is what tests and
//! apps add; its registration lives here, above those layers.

use bevy::prelude::*;

use crate::{
    audio,
    map::{MapHooks, MapPlugin, PendingMap, spawn_map},
    render,
};

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        self.build_first(app);
        app.add_plugins(audio::SoundPlugin);
        self.build_rest(app);
        app.add_systems(
            Startup,
            spawn_map::<render::MapAssets>.run_if(resource_exists::<PendingMap>),
        );
        app.init_resource::<MapHooks>();
        app.world_mut().resource_mut::<MapHooks>().spawn = Some(spawn_pending_map);
    }
}

/// `map::change_map`'s spawner: the pending map, drawn by `render`.
fn spawn_pending_map(world: &mut World) {
    if let Err(e) = world.run_system_cached(spawn_map::<render::MapAssets>) {
        error!("spawning the map: {e}");
    }
}
