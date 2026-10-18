//! Combat Arms' HUD: the crosshair, health, ammo, and later the rest of
//! its own panels (kill feed, weapon selection, score, mode panels).
//! Defined by specs/combat_arms/hud.md (not written yet).
//!
//! Wave 0: Chimera's fallback HUD as it is (`client::hud`: crosshair with
//! the dynamic gap, health, ammo, hit marker, kill feed), which the shell
//! draws for any game; `CaHudPlugin` adds nothing yet.
//!
//! The seam: `CaHudPlugin` sits right after the shell's fallback HUD
//! (`ui::with_hud`); Combat Arms' own panels are its systems, reading the
//! simulation's components (`core::Health`, `weapon::Inventory`,
//! `weapon::Magazine`) and drawing with the shell's widgets and HUD text
//! (`client::widgets`, `client::hud_text`). Laying a game's HUD out from
//! data (`map::hud::ActiveHud`) and CS:S's HUD panels in
//! games/cs_source/src/ui/ (game_hud.rs, weapon_select.rs, hud_sprites.rs)
//! are lift candidates for the shell once two games draw the same panels.

use bevy::prelude::*;

/// Combat Arms' HUD: the shell's fallback HUD alone for now.
pub struct CaHudPlugin;

impl Plugin for CaHudPlugin {
    fn build(&self, _app: &mut App) {}
}
