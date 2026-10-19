//! Combat Arms' first-person view models: each weapon whose data names a
//! view model (`[models] view`, an archive path of the install, with
//! `view_skins`: its textures by the model's texture index) gets it
//! loaded with the map (`add_view_models`, from `world::load_from`) as
//! Chimera's `MapViewModel`, converted by `model` (the weapon's pieces on
//! the arms' skeleton, the animation child's sequences included), in the
//! eye's space: LithTech's player-view models have their origin at the eye and
//! face forward, so the model's own frame after `model`'s conversion is
//! the camera's (-Z ahead), scaled by `VIEW_SCALE` metres per unit.
//! Chimera's view-model driver (`weapon::view_anim`) plays the weapon
//! data's `[anim.sequences]` on the weapon frame's events (draw, fire,
//! reload, shell reload, zoom) and the presentation draws it at the local
//! player's first-person camera with `settings`' field of view.
//!
//! Sequences are non-looping but for the idles (`LOOPING`), so a fire or
//! reload plays once and the driver goes back to idle when it ends.
//!
//! Readable without keys: the player-view archives of pistols, shotguns
//! and machine guns (`GUNS_M_PV_Pistol`, `_SG`, `_MG`); the rifles',
//! SMGs', snipers', knives' and grenades' need the V2 key, and a weapon
//! whose view model doesn't load keeps the front end's placeholder box
//! (the map's warnings say which).
//!
//! Known shortcuts (docs/tech-debt.md): there is no view_models.md spec
//! yet (docs/plans/active/combat-arms.md, row 16): the action ->
//! sequence mapping in the data is provisional (the animation names as
//! found in the models, the timings the weapon frame's from the firing,
//! classes and inventory specs, the animations played at their own
//! length, not stretched to the frame's times); the field of view
//! (`VIEWMODEL_FOV`), the scale and the placement are guesses; no bob or
//! sway (Chimera's lift of CS:S's `view_motion` waits for the spec's
//! numbers), no muzzle flash, brass or sockets, no key strings (sounds)
//! from the animations, no holster animation (the holster is instant,
//! inventory.md), no sprint pose animation, no arms (the models carry the
//! arm bones but no arm piece: every piece is skin 1, the weapon), no
//! left-handed mirroring and no skins by attachment; a piece whose skin
//! is in a keyed archive is drawn flat grey (`UNSKINNED`).

use std::sync::Arc;

use bevy::prelude::*;

use crate::{
    map::{MapData, MapViewModel, ViewModelSettings},
    weapon::view_anim::ViewAnimPlugin,
};

use super::model;

/// The view model's horizontal field of view at 4:3 (degrees): the
/// player's own 90 for now (provisional: view_models.md will say).
pub const VIEWMODEL_FOV: f32 = 90.0;
/// The player's unzoomed horizontal field of view at 4:3.
pub const DEFAULT_FOV: f32 = 90.0;

/// Metres per view-model unit (provisional: the player-view skeletons'
/// arms are about 2.8 units from shoulder to wrist, read as an arm's
/// 0.55 m). Only the near plane sees it: the view model has its own
/// camera, so a larger or smaller scale draws the same picture once the
/// model is beyond the view-model camera's 1 cm near plane (the M92FS's
/// hands and gun sit 0.2 to 1 unit ahead of the eye in its idle).
pub const VIEW_SCALE: f32 = 0.2;

/// The colour of a piece whose skin doesn't read (keyed texture
/// archives without the V2 key): a gunmetal grey, so the shape reads.
const UNSKINNED: [u8; 3] = [90, 92, 96];

/// Sequences that loop (by name, lower case): the idles.
const LOOPING: &[&str] = &["idle"];

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
/// the model's texture index), its textures added to `map`.
pub fn load(
    path: &str,
    key: &str,
    skins: &[String],
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
    for s in &mut animations.sequences {
        s.looping = LOOPING.iter().any(|l| s.name.to_lowercase().starts_with(l));
    }
    Ok(MapViewModel {
        key: key.to_string(),
        model: m,
        bones: rig.bones,
        root: rig.root,
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
        match load(path, &w.id, &w.models.view_skins, map, file) {
            Ok(v) => map.view_models.push(v),
            Err(e) => map.warnings.push(format!("{} view model: {e}", w.id)),
        }
    }
}
