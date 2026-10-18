//! The Combat Arms app's glue (apps/combat_arms, crate `combat_arms_app`):
//! what composes Chimera and Combat Arms (games/combat_arms) into the game
//! binary (`combat_arms`, main.rs). Wave 0: the skeleton, on the greybox.
//!
//! `ClientPlugin`: the client shell (`chimera_client`) with this app's
//! hooks (`client_hooks`) and Combat Arms' HUD in its place
//! (`combat_arms::ui::with_hud`).
//!
//! `VERSION` and `BUILD`: this build's version and commit, stamped by
//! build.rs (the script every app shares, chimera/core/build_stamp.rs);
//! main.rs inserts `core::BuildVersion`, the hooks carry the commit.
//!
//! Maps: `combat_arms:<name>` from the install (`load_map`, through
//! `combat_arms::game::world`), the greybox otherwise.
//!
//! Known shortcuts: maps load only by id (no downloaded map files, no
//! map offer to network clients); the shell keeps its config (binds, cvars) in the same
//! folder as the CS:S app's (`console::cfg_dir`).

use bevy::prelude::*;
use chimera_client::client::{AppHooks, Args, ClientPlugins};
use chimera_world::{greybox, map::MapData};

/// This build's version (`version` command, bug reports): see
/// `version::format_version`; stamped by build.rs.
pub const VERSION: &str = env!("CHIMERA_VERSION");

/// This build's commit (`git describe`: client hooks, bug reports);
/// stamped by build.rs.
pub const BUILD: &str = env!("CHIMERA_GIT");

/// What this app tells the client shell: its commit, Combat Arms' maps
/// by id (`load_map`), and the greybox's own hooks.
pub fn client_hooks() -> AppHooks {
    AppHooks {
        build: BUILD,
        load_map: load_map,
        load_map_file: |id, _, _| Err(no_maps(id)),
        map_file_bytes: |_| None,
        unload_greybox: greybox::unload,
        respawn_greybox: greybox::respawn,
    }
}

fn no_maps(id: &str) -> String {
    format!("{id}: Combat Arms maps load by id only (combat_arms:<name>), not from a file")
}

/// Map `id` (`combat_arms:<name>`, or a bare name) from the install; the
/// HDR level doesn't apply.
pub fn load_map(id: &str, _hdr_level: u8) -> Result<MapData, String> {
    match id.split_once(':') {
        Some((game, _)) if format!("{game}:") != combat_arms::game::world::MAP_PREFIX => {
            Err(format!("{id}: not a Combat Arms map (combat_arms:<name>)"))
        }
        _ => combat_arms::game::world::load(id),
    }
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

#[cfg(test)]
mod tests {
    #[test]
    fn the_build_is_stamped() {
        // build.rs ran: a commit and a version (or "unknown" without git),
        // which the shell's hooks carry.
        assert!(!super::BUILD.is_empty() && !super::VERSION.is_empty());
        assert_eq!(super::client_hooks().build, super::BUILD);
    }

    /// The wheel's `invnext` / `invprev` switch weapons through the
    /// shell's selection, which Combat Arms' front end uses (no HUD row:
    /// each step switches at once).
    #[test]
    fn the_wheel_steps_through_the_weapons() {
        use bevy::prelude::*;
        use chimera_client::client::weapon_menu::{WeaponMenu, WeaponMenuPlugin};
        use chimera_core::{console::Console, core::LocalPlayer};
        use chimera_sim::weapon::{Inventory, Weapon};
        use chimera_world::{
            greybox::{self, GreyboxMapPlugin},
            harness::Sim,
        };
        use combat_arms::{
            game::{inventory, movement, weapons},
            harness::CaSim,
        };

        let mut sim = Sim::with(CaSim, |app| {
            app.init_resource::<ButtonInput<KeyCode>>()
                .init_resource::<ButtonInput<MouseButton>>()
                .add_plugins((GreyboxMapPlugin, WeaponMenuPlugin));
        });
        let p = sim.spawn_character(greybox::SPAWNS[0], movement::ID);
        sim.app.world_mut().entity_mut(p).insert(LocalPlayer);
        sim.seconds(0.5);
        let active = |sim: &Sim| {
            let w = sim.app.world();
            w.get::<Weapon>(w.get::<Inventory>(p).unwrap().active.unwrap()).unwrap().id
        };
        assert_eq!(active(&sim), weapons::RIFLE);
        // A command, then the selection's pick passed to the intent (as
        // the local input does) until the switch is done.
        let command = |sim: &mut Sim, line: &str| {
            sim.app.world_mut().resource_mut::<Console>().submit(line);
            for _ in 0..90 {
                sim.ticks(1);
                let pick = sim.app.world().resource::<WeaponMenu>().selected_slot();
                sim.intent(p).select = pick;
            }
        };
        command(&mut sim, "invnext");
        assert_eq!(active(&sim), inventory::KNIFE);
        command(&mut sim, "invprev");
        assert_eq!(active(&sim), weapons::RIFLE);
        command(&mut sim, "invprev");
        assert_eq!(active(&sim), inventory::KNIFE, "wraps");
    }
}
