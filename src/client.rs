//! The client shell (engine/client, crate `engine_client`) at its old
//! path, with the app's own: `ClientPlugin`, the shell's plugins with
//! this app's hooks (`client_hooks`: its maps, greybox and commit) and
//! CS:S's HUD among them (`games::cs_source::ui::with_hud`); and CS:S's
//! HUD modules where the shell had them (`game_hud`, `radar`, ...: the
//! front end's), the spectator's HUD beside the shell's `spectate`.
//!
//! App glue, not the shell: the architecture test counts this file as
//! the crate root's (`lib`), which may name the games.

pub use engine_client::client::*;

pub use crate::games::cs_source::ui::{game_hud, hud_sprites, overview, radar, weapon_select, win_panel};

use bevy::prelude::*;

/// Spectating: the shell's state and camera with CS:S's spectator HUD.
pub mod spectate {
    pub use crate::games::cs_source::ui::spectator_hud::*;
    pub use engine_client::client::spectate::*;
}

/// The client as this app runs it: the shell's plugins
/// (`ClientPlugins`) with the app's hooks (`crate::client_hooks`) and
/// CS:S's HUD in its places (`games::cs_source::ui::with_hud`). The app
/// adds CS:S's menus after it (`CsSourceUiPlugin`).
pub struct ClientPlugin {
    pub args: Args,
}

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::games::cs_source::ui::with_hud(
            ClientPlugins {
                args: self.args.clone(),
                hooks: crate::client_hooks(),
            }
            .build(),
        ));
    }
}
