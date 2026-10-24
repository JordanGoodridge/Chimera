//! A loaded map's spawn work done ahead, off the main thread (architecture
//! review L4, F6): what `spawn_map` would compute from `MapData` alone
//! (colliders, the props' brushes, the drawer's images) is computed where
//! the map loads (the client's background load, `client::console`), so
//! the frame that swaps the map in only spawns. Each part's spawn phase
//! takes its prepared part (`Prepared::take`) or, without one (a map
//! spawned at startup, a server's change, a test), computes it there with
//! the same function: one path, identical results either way.
//!
//! `MapHooks::prepare` lists the jobs (`map::build` registers the
//! world's, a drawer adds its own: `render`'s images); `preparation` makes each
//! job from the world as it is when the load starts (a drawer reads the
//! device's formats there), `MapPrepare::run` runs them on the loaded
//! data, and `change_map_prepared` swaps the map in with the results.

use std::{
    any::{Any, TypeId},
    collections::BTreeMap,
    sync::Mutex,
};

use bevy::prelude::*;

use super::MapData;

/// Parts of a map's spawn computed ahead, by type.
#[derive(Default)]
pub struct Prepared(Mutex<BTreeMap<TypeId, Box<dyn Any + Send>>>);

impl Prepared {
    /// Keep `part` for its spawn phase.
    pub fn insert<T: Send + 'static>(&mut self, part: T) {
        self.0
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .insert(TypeId::of::<T>(), Box::new(part));
    }

    /// Take the part of type `T`, if it was prepared.
    pub fn take<T: Send + 'static>(&self) -> Option<T> {
        let part = self.0.lock().unwrap_or_else(|e| e.into_inner()).remove(&TypeId::of::<T>())?;
        part.downcast::<T>().ok().map(|b| *b)
    }

    /// How many parts are prepared.
    pub fn len(&self) -> usize {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One job, made from the world when a load starts, run on the loaded map.
pub type PrepareJob = Box<dyn FnOnce(&MapData, &mut Prepared) + Send>;

/// The jobs for one load.
pub struct MapPrepare(Vec<PrepareJob>);

/// The jobs for a load starting now (`MapHooks::prepare`; none without
/// map systems).
pub fn preparation(world: &World) -> MapPrepare {
    MapPrepare(
        world
            .get_resource::<super::MapHooks>()
            .map(|h| h.prepare.iter().map(|make| make(world)).collect())
            .unwrap_or_default(),
    )
}

impl MapPrepare {
    /// Run every job on the loaded map (on the loading thread).
    pub fn run(self, data: &MapData) -> Prepared {
        let mut prepared = Prepared::default();
        for job in self.0 {
            job(data, &mut prepared);
        }
        prepared
    }
}

/// `change_map` with parts prepared for it (`MapPrepare::run` on `data`).
pub fn change_map_prepared(world: &mut World, data: MapData, prepared: Prepared, view: super::MapDebugView) {
    super::change_map_with(world, data, prepared, view);
}

/// Prepare the map waiting to spawn (`PendingMap`: one given at startup)
/// now, on this thread, as a load would: for checking that a prepared
/// spawn is the same as one computed in place (`heavy::map_spawn_dump`).
/// Returns how many parts were prepared.
pub fn prepare_pending(world: &mut World) -> usize {
    let prepare = preparation(world);
    let Some(mut pending) = world.get_resource_mut::<super::PendingMap>() else { return 0 };
    pending.2 = prepare.run(&pending.0);
    pending.2.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_are_taken_once_by_type() {
        let mut p = Prepared::default();
        p.insert(3u32);
        p.insert(String::from("a"));
        assert_eq!(p.len(), 2);
        assert_eq!(p.take::<u32>(), Some(3));
        assert_eq!(p.take::<u32>(), None);
        assert_eq!(p.take::<String>().as_deref(), Some("a"));
        assert!(p.is_empty());
    }
}
