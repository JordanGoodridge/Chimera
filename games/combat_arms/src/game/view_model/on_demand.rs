//! View models loaded on demand (docs/performance.md, "Combat Arms map
//! loads"): a map loads without them (`world::load_from`), and a weapon's
//! view model loads in the background (`AsyncComputeTaskPool`) the first
//! time a weapon wants drawing: one in the local player's inventory or
//! shown by a character's `ViewAnimator`. Converted once per session
//! (`cached`, kept across map changes), each with its own textures
//! (`map::add_view_model`; drawn on a map and on the greybox alike, where
//! Chimera's drawing has assets of its own: `render::view_model::
//! assets_without_map`, `tests/ui/heavy/view_model.rs`); until it is
//! ready the front end's placeholder box stands in, and `drive` starts
//! the model afresh when it arrives.
//!
//! Runs only where `LoadOnDemand` is (the front end inserts it): a
//! dedicated server draws no view models.
//!
//! Known shortcuts (docs/tech-debt.md): the placeholder box shows for the
//! frames a model takes to load (tens of milliseconds; the first draw of
//! each weapon); the cache keeps every model for the session; nothing
//! preloads a weapon before it is in the local player's inventory or
//! shown (a spectated player's new weapon waits its load too).

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};

use super::{hands, load, load_hands};
use crate::{
    core::LocalPlayer,
    map::{MapData, MapTexture, MapViewModel, ViewAnimator, ViewModels, add_view_model},
    weapon::{Inventory, Weapon},
};

/// Load view models when they are wanted (the front end's).
#[derive(Resource, Default)]
pub struct LoadOnDemand;

/// A weapon's view model as loaded: its textures, which its meshes'
/// indices name.
pub struct LoadedViewModel {
    pub model: MapViewModel,
    pub textures: Arc<Vec<MapTexture>>,
    /// What didn't load with it (the hands): the map's warnings would
    /// have said.
    pub warnings: Vec<String>,
}

type Loaded = Result<Arc<LoadedViewModel>, String>;

fn cache() -> &'static Mutex<HashMap<String, Loaded>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Loaded>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Weapon `id`'s view model if this session loaded it (or failed to).
pub fn cached(id: &str) -> Option<Loaded> {
    cache().lock().ok()?.get(id).cloned()
}

/// Weapon `id`'s view model from the install through `file` (`load`, with
/// the default character's hands when they read), with its own textures;
/// Err when its data names none or it doesn't read.
pub fn load_weapon(id: &str, file: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<LoadedViewModel, String> {
    let w = super::super::data::CaData::install()
        .weapon_set()
        .weapons
        .iter()
        .find(|w| w.id == id)
        .ok_or_else(|| format!("{id}: no such weapon"))?;
    let path = w.models.view.as_deref().ok_or_else(|| format!("{id}: no view model in its data"))?;
    let offset = w.models.view_offset.unwrap_or_default();
    // Its textures: indices into this map's list.
    let mut own = MapData::default();
    let choice = hands::choose(&hands::builtin(), &hands::HandProfile {
        character: true,
        ..default()
    });
    let mut warnings = Vec::new();
    let hand = match choice.as_ref().map(|c| load_hands(c, &mut own, file)) {
        Some(Ok(h)) => Some(h),
        Some(Err(e)) => {
            warnings.push(format!("view-model hands: {e}"));
            None
        }
        None => None,
    };
    let mut v =
        load(path, &w.id, &w.models.view_skins, offset, &mut own, file).map_err(|e| format!("{id} view model: {e}"))?;
    if let Some((meshes, bones)) = hand {
        let at = Vec3::from(lithtech_formats::ltb::to_rh_vec(offset)) * super::VIEW_SCALE;
        if let Err(e) = hands::attach(&mut v, meshes, &bones, super::VIEW_SCALE, at) {
            warnings.push(format!("{id} view-model hands: {e}"));
        }
    }
    Ok(LoadedViewModel {
        model: v,
        textures: Arc::new(own.textures),
        warnings,
    })
}

/// `load_weapon` once per session: the cached result after the first.
pub fn load_cached(id: &str, file: &dyn Fn(&str) -> Option<Vec<u8>>) -> Loaded {
    if let Some(l) = cached(id) {
        return l;
    }
    let loaded = load_weapon(id, file).map(Arc::new);
    if let Ok(mut c) = cache().lock() {
        c.insert(id.to_string(), loaded.clone());
    }
    loaded
}

/// The view models loading.
#[derive(Default)]
pub struct Pending(HashMap<String, Task<Loaded>>);

/// Each wanted weapon's view model (`LoadOnDemand`): from the session's
/// cache at once, else loaded in the background and added when done.
pub fn load_on_demand(
    models: Option<Res<ViewModels>>,
    local: Query<&Inventory, With<LocalPlayer>>,
    shown: Query<&ViewAnimator>,
    weapons: Query<&Weapon>,
    mut pending: Local<Pending>,
    mut commands: Commands,
) {
    let has = |k: &str| models.as_ref().is_some_and(|m| m.get(k).is_some());
    // Finished loads first.
    pending.0.retain(|id, task| {
        let Some(result) = block_on(poll_once(task)) else { return true };
        match &result {
            Ok(l) => {
                for w in &l.warnings {
                    warn!("Combat Arms: {w}");
                }
            }
            Err(e) => warn!("Combat Arms: {e} (the placeholder stands in)"),
        }
        if let Ok(mut c) = cache().lock() {
            c.insert(id.clone(), result);
        }
        false
    });
    // The active weapon first, then the rest of the inventory, then what
    // characters show (a spectated player's).
    let mut wanted: Vec<&str> = Vec::new();
    for inv in &local {
        let ids = inv.active.iter().chain(&inv.weapons).filter_map(|e| weapons.get(*e).ok()).map(|w| w.id);
        wanted.extend(ids);
    }
    wanted.extend(shown.iter().filter_map(|v| v.key.as_deref()));
    let mut seen = Vec::new();
    for id in wanted {
        if seen.contains(&id) || has(id) || pending.0.contains_key(id) {
            continue;
        }
        seen.push(id);
        match cached(id) {
            Some(Ok(l)) => {
                let (model, textures) = (l.model.clone(), l.textures.clone());
                commands.queue(move |world: &mut World| add_view_model(world, model, textures));
            }
            Some(Err(_)) => {}
            None => {
                let key = id.to_string();
                let task = AsyncComputeTaskPool::get().spawn(async move {
                    let mount = super::super::world::mount().ok_or_else(|| format!("{key}: no Combat Arms install"))?;
                    load_weapon(&key, &|p: &str| mount.read(p).ok()).map(Arc::new)
                });
                pending.0.insert(id.to_string(), task);
            }
        }
    }
}
