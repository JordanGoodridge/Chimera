//! Combat Arms' front end on the client shell (the `ui` feature): `hud`
//! (crosshair and basic HUD) and `visuals` (placeholder looks: box
//! guns). No menus yet: the shell plays without them
//! (`client::front_end`'s hooks unset).
//!
//! `with_hud` puts Combat Arms' HUD among the shell's plugins (after its
//! fallback HUD, `client::hud::HudPlugin`) and the shell's weapon
//! selection (`client::weapon_menu`, as CS:S's: `slot1`..`slot5`,
//! `invnext` / `invprev`, here with no HUD row, so they switch at once;
//! see `game::inventory`), as CS:S's `with_hud` does; `CaUiPlugin` adds
//! the rest after the shell.

pub mod hud;
pub mod visuals;

use bevy::{app::PluginGroupBuilder, prelude::*};

/// The shell's plugins with Combat Arms' HUD in its place, and the
/// shell's weapon selection where CS:S has its own (after the views).
pub fn with_hud(shell: PluginGroupBuilder) -> PluginGroupBuilder {
    shell
        .add_after::<crate::client::hud::HudPlugin>(hud::CaHudPlugin)
        .add_after::<crate::client::view::ViewPlugin>(crate::client::weapon_menu::WeaponMenuPlugin)
}

/// Combat Arms' front end but its HUD: the placeholder visuals.
pub struct CaUiPlugin;

impl Plugin for CaUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(visuals::CaVisualsPlugin);
    }
}
