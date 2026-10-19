//! Combat Arms' first-person view models (specs/combat_arms/view_models.md,
//! in part; section numbers below are its): each weapon whose data names a
//! player-view model (`[models] view`, an archive path of the install,
//! with `view_skins`: the weapon's skins by texture index, 2, and
//! `view_offset`: its `Pos`, 4) gets it loaded with the map
//! (`add_view_models`, from `world::load_from`) as Chimera's
//! `MapViewModel`, converted by `model` (the gun's pieces on the arm
//! skeleton, the `ANI_*` child's animations appended, 1). The model's frame
//! after `model`'s conversion is the camera's (-Z ahead, 4: rotation
//! identity in camera space), its origin at `Pos`, scaled by `VIEW_SCALE`.
//! It is drawn at the local player's first-person camera with its own
//! projection (`settings`: 50 degrees vertical, 5.1). Chimera's view-model
//! driver (`weapon::view_anim`) plays the data's `[anim.sequences]` on the
//! weapon frame's events, by 7's names: `select` on the draw, `idle_0`,
//! `fire_0` per round, `reload`, and `PreReload`/`reload` per
//! shell/`PostReload` (the files spell them in either case).
//!
//! Sequences are non-looping but for the subtle idle (`LOOPING`: 7
//! restarts it each time it ends), so a fire or reload plays once and the
//! driver goes back to idle when it ends.
//!
//! Readable without keys: the player-view archives of pistols, shotguns
//! and machine guns (`GUNS_M_PV_Pistol`, `_SG`, `_MG`); the rifles',
//! SMGs', snipers', knives', grenades' and the hand models' need the V2 key,
//! and a weapon whose view model doesn't load keeps the front end's
//! placeholder box (the map's warnings say which).
//!
//! Known shortcuts (docs/tech-debt.md); written before view_models.md
//! landed, what it says that isn't done yet:
//! - No hand model (1): the arms and sleeves are a separate model from
//!   `CHARS_M_HAND` (V2-keyed), bound by node name, skinned by the
//!   character, face and uniform (2); only the gun's pieces are drawn. A
//!   piece whose skin is in a keyed archive is flat grey (`UNSKINNED`).
//! - Animation (7): no pre-fire (no stock unkeyed gun plays one), no
//!   `postfire` when the trigger is let go, no full idle (`idle_1` after
//!   5-15 s still), no sprint pose (`PreRush`/`rush`/`PostRush`); the
//!   animations play at rate 1, not the fire, reload and draw rates (equal
//!   for the stock data: the frame's times are the animations' lengths);
//!   transitions snap (`model`'s shortcuts); a sequence starts on the
//!   weapon frame's event, not by 7's state machine (Quirk 1's one-frame
//!   switch, the semi-automatic `postfire` cancel).
//! - No sway (6), shot jiggle (4, zero in the stock data), key events (8:
//!   sounds, knife hits), muzzle flash (9), shells (10) or attachments (3).
//! - Projection (5.1): Chimera's view-model camera keeps its 1 cm near
//!   plane (`VIEW_SCALE` makes that the spec's 0.1 u) and no far plane
//!   (the spec's 7 u); `ViewModelSettings` zooms the view model with the
//!   world's zoom (the spec: only the world's FOV changes; scopes hide the
//!   model either way). The world camera's FOV is the shell's (5.2: 75
//!   degrees vertical).
//! - No left-handed mirroring (none in the game).

use std::sync::Arc;

use bevy::prelude::*;

use crate::{
    map::{MapData, MapViewModel, ViewModelSettings},
    weapon::view_anim::ViewAnimPlugin,
};

use super::model;

/// The view model's horizontal field of view at 4:3 (degrees), from 5.1's
/// 50 degrees vertical (`PVModelFOV`): 2 atan(4/3 tan 25 degrees).
pub const VIEWMODEL_FOV: f32 = 63.74;
/// The player's unzoomed horizontal field of view at 4:3 (5.2: 75 degrees
/// vertical, 2 atan(4/3 tan 37.5 degrees)).
pub const DEFAULT_FOV: f32 = 91.31;

/// Metres per view-model unit: 0.1, so that Chimera's view-model near
/// plane (1 cm) cuts where the game's (`reallyclose_near`, 0.1 u, 5.1)
/// does. The view model has its own camera, so the scale changes nothing
/// else on screen.
pub const VIEW_SCALE: f32 = 0.1;

/// The colour of a piece whose skin doesn't read (keyed texture
/// archives without the V2 key): a gunmetal grey, so the shape reads.
const UNSKINNED: [u8; 3] = [90, 92, 96];

/// Sequences that loop (by name, lower case): the subtle idle.
const LOOPING: &[&str] = &["idle_0"];

/// The view-model settings Combat Arms starts with.
pub fn settings() -> ViewModelSettings {
    ViewModelSettings {
        fov: VIEWMODEL_FOV,
        default_fov: DEFAULT_FOV,
        right_hand: 1,
        draw: 1,
    }
}

/// Chimera's view-model driver with Combat Arms' settings.
pub struct CaViewModelPlugin;

impl Plugin for CaViewModelPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ViewAnimPlugin>() {
            app.add_plugins(ViewAnimPlugin);
        }
        app.insert_resource(settings());
    }
}

/// View model `path` for weapon `key`, skinned with `skins` (textures by
/// the model's texture index), its origin at `offset` (the game's camera
/// space: units, +Z ahead), its textures added to `map`.
pub fn load(
    path: &str,
    key: &str,
    skins: &[String],
    offset: [f32; 3],
    map: &mut MapData,
    file: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<MapViewModel, String> {
    let loaded = model::load(path, file)?;
    let mut m = model::convert(&loaded, VIEW_SCALE, &|lod| {
        let i = lod.used_textures().first().copied().unwrap_or(-1);
        usize::try_from(i)
            .ok()
            .and_then(|i| skins.get(i))
            .map(|s| model::normalize(s))
            .unwrap_or_default()
    });
    super::world::textures(&mut m.meshes, map, file);
    for mesh in m.meshes.iter_mut().filter(|m| m.texture.is_none()) {
        mesh.color = UNSKINNED;
    }
    // Just built: nothing else holds the rig or its animations.
    let rig = m
        .rig
        .take()
        .and_then(|r| Arc::try_unwrap(r).ok())
        .ok_or_else(|| format!("{path}: no skeleton"))?;
    let mut animations = Arc::try_unwrap(rig.animations).map_err(|_| format!("{path}: animations shared"))?;
    // The origin moves to `offset`: the skeleton's root and the meshes
    // (in the eye's space at the reference pose) alike.
    let offset = Vec3::from(lithtech_formats::ltb::to_rh_vec(offset)) * VIEW_SCALE;
    for p in m.meshes.iter_mut().flat_map(|m| m.positions.iter_mut()) {
        *p = (Vec3::from(*p) + offset).to_array();
    }
    for s in &mut animations.sequences {
        s.looping = LOOPING.iter().any(|l| s.name.to_lowercase().starts_with(l));
    }
    Ok(MapViewModel {
        key: key.to_string(),
        model: m,
        bones: rig.bones,
        root: rig.root.with_translation(offset),
        animations: Some(Arc::new(animations)),
        right_handed: true,
        allow_flipping: false,
        attachments: Vec::new(),
        light_origin: Vec3::ZERO,
    })
}

/// Every Combat Arms weapon's view model that loads, into `map` (a
/// warning for each that doesn't).
pub fn add_view_models(map: &mut MapData, file: &dyn Fn(&str) -> Option<Vec<u8>>) {
    for w in &super::data::weapon_set().weapons {
        let Some(path) = w.models.view.as_deref() else { continue };
        let offset = w.models.view_offset.unwrap_or_default();
        match load(path, &w.id, &w.models.view_skins, offset, map, file) {
            Ok(v) => map.view_models.push(v),
            Err(e) => map.warnings.push(format!("{} view model: {e}", w.id)),
        }
    }
}
