//! Combat Arms' front end on the client shell (the `ui` feature): `hud`
//! (crosshair and basic HUD) and `visuals` (placeholder looks: box
//! guns). No menus yet: the shell plays without them
//! (`client::front_end`'s hooks unset).
//!
//! `with_hud` puts Combat Arms' HUD among the shell's plugins (after its
//! fallback HUD, `client::hud::HudPlugin`), as CS:S's `with_hud` does;
//! `CaUiPlugin` adds the rest after the shell.

pub mod hud;
pub mod visuals;

use bevy::{app::PluginGroupBuilder, prelude::*};

/// The shell's plugins with Combat Arms' HUD in its place.
pub fn with_hud(shell: PluginGroupBuilder) -> PluginGroupBuilder {
    shell.add_after::<crate::client::hud::HudPlugin>(hud::CaHudPlugin)
}

/// Combat Arms' front end but its HUD: the placeholder visuals.
pub struct CaUiPlugin;

impl Plugin for CaUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(visuals::CaVisualsPlugin);
    }
}
