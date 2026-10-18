//! The Combat Arms app's glue (apps/combat_arms, crate `combat_arms_app`):
//! what composes Chimera and Combat Arms (games/combat_arms) into the game
//! binary (`combat_arms`, main.rs). Wave 0: the skeleton, on the greybox.
//!
//! `ClientPlugin`: the client shell (`chimera_client`) with this app's
//! hooks (`client_hooks`) and Combat Arms' HUD in its place
//! (`combat_arms::ui::with_hud`).
//!
//! Known shortcuts: no map but the greybox loads (Combat Arms' maps wait
//! for its loader); the shell keeps its config (binds, cvars) in the same
//! folder as the CS:S app's (`console::cfg_dir`); the build carries no
//! version stamp (`core::BuildVersion`, CS:S's app's build.rs).

use bevy::prelude::*;
use chimera_client::client::{AppHooks, Args, ClientPlugins};
use chimera_world::{greybox, map::MapData};

/// What this app tells the client shell: no map loads but the greybox
/// (Combat Arms' maps wait for its loader), and the greybox's own hooks.
pub fn client_hooks() -> AppHooks {
    AppHooks {
        build: "",
        load_map: load_map,
        load_map_file: |id, _, _| Err(no_maps(id)),
        map_file_bytes: |_| None,
        unload_greybox: greybox::unload,
        respawn_greybox: greybox::respawn,
    }
}

fn no_maps(id: &str) -> String {
    format!("{id}: Combat Arms' maps aren't loaded yet (the skeleton plays the greybox only)")
}

/// Map `id` at a `mat_hdr_level`: none yet.
pub fn load_map(id: &str, _hdr_level: u8) -> Result<MapData, String> {
    Err(no_maps(id))
}

/// The client as this app runs it: the shell's plugins with the app's
/// hooks and Combat Arms' HUD (`combat_arms::ui::with_hud`).
pub struct ClientPlugin {
    pub args: Args,
}

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(combat_arms::ui::with_hud(
            ClientPlugins {
                args: self.args.clone(),
                hooks: client_hooks(),
            }
            .build(),
        ));
    }
}
