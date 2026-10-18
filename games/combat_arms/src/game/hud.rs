//! The weapon data Combat Arms' HUD reads (specs/combat_arms/hud_crosshair.md):
//! a weapon's crosshair fields (A3 to A8: `CrossHairType`,
//! `CrossHairColor`, `CrossHairOffset`, `DisableCrosshair`,
//! `DuckPerturb`/`MoveDuckPerturb`, `DotSightName`, the wide-fire pellet
//! count) and the mission bomb flag for the weapon panel (D4), as the hook
//! `combat_arms:hud` of the weapon data (docs/weapon-data.md, "Hooks"),
//! which puts them on the weapon as `CaHudData`. Nothing in the
//! simulation reads it: it is here so every weapon document, the server's
//! too, loads with it; the front end (`ui::hud`) draws from it. A weapon
//! without the hook has the defaults (the plain green plus).

use bevy::prelude::*;
use serde::Deserialize;

use crate::weapon::data::WeaponDataAppExt;

/// The hook's name.
pub const HUD_HOOK: &str = "combat_arms:hud";

/// A weapon's HUD data (`combat_arms:hud`). Every field is optional; the
/// defaults are the spec's.
#[derive(Component, Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct CaHudData {
    /// `CrossHairType`: the crosshair shape 1 to 6 (A5); 0 or anything
    /// else: no preference.
    pub crosshair: u8,
    /// `CrossHairColor` (RGB); none: green (0, 255, 0).
    pub crosshair_color: Option<[u8; 3]>,
    /// `CrossHairOffset`: added to the arms' offset in screen pixels,
    /// unscaled (A4).
    pub crosshair_offset: i32,
    /// `DisableCrosshair` 1: no crosshair lines (A7).
    pub disable_crosshair: bool,
    /// `DuckPerturb`, `MoveDuckPerturb`: the crosshair's minimum while
    /// crouching still and moving, when both are above 0 (A3).
    pub duck_perturb: i32,
    pub move_duck_perturb: i32,
    /// `DotSightName`: the weapon's reticle image, by role (A8); no image
    /// is loaded yet (the front end draws a placeholder).
    pub dot_sight: Option<String>,
    /// The wide-fire pellet count (the double circle's minimum, A5).
    pub wide_count: i32,
    /// The mission bomb (`Guntype` 8): no weapon panel. (The panel's
    /// class is the weapon's inventory class, its data `slot`: D4.)
    pub bomb: bool,
}

/// Registers `combat_arms:hud` (before the weapon data loads).
pub struct CaHudDataPlugin;

impl Plugin for CaHudDataPlugin {
    fn build(&self, app: &mut App) {
        app.register_weapon_hook::<CaHudData>(HUD_HOOK, |e, _, p| {
            e.insert(p.clone());
        });
    }
}
