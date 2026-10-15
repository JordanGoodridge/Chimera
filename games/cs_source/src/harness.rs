//! CS:S on the engine's headless harness (`chimera_world::harness::Sim`,
//! `chimera_net::harness::NetSim`), for its tests, the app's and the
//! tools': `CsSim`, the base they build on (CS:S's simulation,
//! `SimPlugins`, with map logic's random picks seeded so they repeat run
//! to run); and maps for network tests built in code (`TestMaps`,
//! `serve_maps`, `load_level`), loaded as the game's `map` does
//! (`swap_map`).

use std::{sync::Arc, time::Duration};

use bevy::prelude::*;

use crate::{
    logic::LogicSeed,
    map::{LoadedMapName, MapData},
    net::{self, NetEvent},
    swap_map,
};

/// CS:S's simulation (`SimPlugins`: the engine's base, Source's map
/// logic, CS:S's rules, objectives, bots and network part) as the harness
/// runs it, headless and repeatable: map logic's random picks seeded
/// (`logic::LogicSeed`). A `Sim`'s or `NetSim`'s base: `Sim::new(CsSim,
/// GreyboxMapPlugin)`.
#[derive(Clone, Copy, Debug, Default)]
pub struct CsSim;

impl Plugin for CsSim {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::SimPlugins)
            .insert_resource(LogicSeed(Some(0x9e37_79b9_7f4a_7c15)));
    }
}

/// Maps for network tests, built in code from a name: a folder standing
/// for an install (`<install>/maps/<name>.bsp`, any bytes: the file the
/// handshake hashes) and a content cache; `build` makes the map's data
/// (`MapData::file_hash` is set from the file). Map ids are
/// `test:<name>`; `greybox` is the greybox.
#[derive(Resource, Clone)]
pub struct TestMaps {
    pub install: std::path::PathBuf,
    pub cache: Option<std::path::PathBuf>,
    pub build: Arc<dyn Fn(&str) -> MapData + Send + Sync>,
}

impl TestMaps {
    /// The file of map `id` in the install folder.
    pub fn file(&self, id: &str) -> std::path::PathBuf {
        self.install
            .join("maps")
            .join(format!("{}.bsp", net::maps::map_name(id)))
    }

    /// Map `id` built, hashed from `file` (else the install's copy).
    pub fn load(&self, id: &str, file: Option<&std::path::Path>) -> Result<MapData, String> {
        let path = file.map_or_else(|| self.file(id), std::path::Path::to_path_buf);
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut data = (self.build)(net::maps::map_name(id));
        data.name = net::maps::map_name(id).to_string();
        data.file_hash = Some(net::maps::sha256(&bytes));
        Ok(data)
    }
}

/// A test map's tick (the greybox's).
pub const TEST_MAP_TICK: Duration = Duration::from_nanos(15_625_000);

/// Serve and load `maps` in this app as the game does with an install:
/// `net::maps::MapFiles` reads the install folder, and the server's map
/// requests (`NetEvent::LoadMap`) load at once (`load_level`).
pub fn serve_maps(app: &mut App, maps: TestMaps) {
    let read = {
        let maps = maps.clone();
        Arc::new(move |id: &str| std::fs::read(maps.file(id)).ok())
    };
    app.insert_resource(net::maps::MapFiles {
        read,
        cache: maps.cache.clone(),
    })
    .insert_resource(maps)
    .add_systems(Update, load_requested_maps);
}

/// Load map `id` here now (`TestMaps`; `greybox` for the greybox), as the
/// game's `map`/`changelevel` does once its background load is done.
pub fn load_level(world: &mut World, id: &str, file: Option<&std::path::Path>) -> Result<(), String> {
    if id == net::GREYBOX {
        swap_map(world, id, None, TEST_MAP_TICK);
        return Ok(());
    }
    let maps = world.get_resource::<TestMaps>().cloned().ok_or("no TestMaps here")?;
    let data = maps.load(id, file)?;
    swap_map(world, id, Some(data), TEST_MAP_TICK);
    Ok(())
}

/// A server's `changelevel` starting: the map's name changes first (its
/// clients hear "changing level"); `load_level` brings it in.
pub fn begin_level_change(world: &mut World, id: &str) {
    world.insert_resource(LoadedMapName(id.to_string()));
}

/// A test client's map loads take this many frames (a slow machine, a big
/// map): `serve_maps` loads what the server asks for that many updates
/// after it asked. 0 (the default without it): at once.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct MapLoadDelay(pub u32);

/// Map loads waiting for their frame (`MapLoadDelay`): frames left, map,
/// file.
#[derive(Resource, Default)]
struct DelayedLoads(Vec<(u32, String, Option<std::path::PathBuf>)>);

fn load_requested_maps(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<NetEvent>>) {
    let events: Vec<NetEvent> = cursor.read(world.resource::<Messages<NetEvent>>()).cloned().collect();
    let delay = world.get_resource::<MapLoadDelay>().map_or(0, |d| d.0);
    let mut queue = world.remove_resource::<DelayedLoads>().unwrap_or_default();
    for e in events {
        match e {
            NetEvent::LoadMap { map, file } => queue.0.push((delay, map, file)),
            // Leaving drops a load still to come.
            NetEvent::Disconnected(_) => queue.0.clear(),
            _ => {}
        }
    }
    let mut due = Vec::new();
    queue.0.retain_mut(|(left, map, file)| {
        if *left == 0 {
            due.push((map.clone(), file.clone()));
            false
        } else {
            *left -= 1;
            true
        }
    });
    world.insert_resource(queue);
    for (map, file) in due {
        if let Err(err) = load_level(world, &map, file.as_deref()) {
            net::disconnect(world, &format!("couldn't load the server's map {map}: {err}"));
        }
    }
}
