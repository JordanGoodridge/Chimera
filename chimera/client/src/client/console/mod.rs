//! The in-game console (docs/plans/active/console.md): toggled with `~`.
//! Drop-down output with colours by severity, scrollback, a filter and
//! timestamps; an input line with Tab completion (names, cvar values,
//! command arguments; repeated Tab cycles), ranked fuzzy suggestions as
//! you type, history (Up/Down, persisted, Ctrl+R reverse search), and the
//! usual editing keys. Binds run when it's closed (`+action` while held).
//! Also the CS:S-style overlays (cl_showpos, cl_showfps, snd_show, watch; `net_graph` is `net_graph`'s)
//! and the client's commands (noclip, getpos/setpos, kill, map, quit, ...).
//!
//! The in-game console UI (`~`): completion (names, cvar values, each argument
//! of the command typed: keys, maps, entities, teams, hitgroups, tabs, ...; on
//! the line's last command after `;`), fuzzy suggestions under it with Up/Down
//! picking one (Enter or Tab takes it and adds a space, Esc hides the list), a
//! help line for the command typed (its usage from the help text with the
//! argument being typed marked, a cvar's value, default, range and values),
//! history (Up/Down with an empty line, persisted, Ctrl+R), editing keys
//! (Ctrl+Backspace/W/Delete words, Ctrl+U/K, Ctrl+V paste, Ctrl+L clear,
//! Ctrl+Home/End), output lines that fit the window's height,
//! scrollback/filter/timestamps, drag-selecting text in the output (Ctrl+C copies it; clicks never
//! grab the mouse while open), binds with `+`/`-` actions (F1 and F2
//! vote while a vote waits for ours: `VOTE_KEYS`), log mirroring,
//! overlays (cl_showpos, cl_showfps, snd_show, watch), client commands (noclip,
//! god, getpos/setpos/setang, kill, `chimera_hurtme` (a bullet hit on yourself
//! at a hitbox), map (in place, `map::change_map`; `map greybox` puts the
//! greybox back, `load_greybox`), disconnect (the greybox, then the main menu),
//! toggleconsole, quit), `developer 1` notify lines, the `chimera/console`
//! remote method.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, OnceLock},
};

use bevy::{
    app::AppExit,
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
        mouse::{MouseScrollUnit, MouseWheel},
    },
    log::{BoxedLayer, tracing_subscriber::Layer},
    prelude::*,
    window::CursorOptions,
};

use super::{FirstPersonCamera, fonts::UiFonts, widgets::Interaction};
use crate::{
    console::{Console, ConsoleAppExt, Level, cfg_dir, parse, resource_cvar},
    core::{LocalPlayer, MovementState, Velocity},
    game_registry::{GameInfo, Games},
    map::PlaySound,
};

pub struct ConsoleUiPlugin;

impl Plugin for ConsoleUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ConsoleUi>()
            .init_resource::<Overlays>()
            .init_resource::<HeldActions>()
            .init_resource::<SoundMarks>()
            .add_systems(Startup, (spawn_ui, load_history, autoexec).chain())
            .add_systems(
                Update,
                (
                    toggle,
                    edit.run_if(|ui: Res<ConsoleUi>| ui.open),
                    // Not in the game menu or while typing chat either:
                    // keys there drive those.
                    run_binds.run_if(
                        |ui: Res<ConsoleUi>,
                         menu: Option<Res<super::front_end::MenuStatus>>,
                         chat: Option<Res<super::chat::ChatInput>>| {
                            !ui.open && !menu.is_some_and(|m| m.open) && !chat.is_some_and(|c| c.open.is_some())
                        },
                    ),
                    drain_log,
                    record_sounds,
                    draw_console,
                    select_text,
                    draw_overlays,
                    draw_notify,
                    finish_map_load,
                    draw_sound_marks,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    overlay_text,
                    watch_text,
                    follow_map_cycle.run_if(resource_exists::<crate::core::MapCycle>),
                ),
            )
            .add_systems(Last, save_on_exit);
        overlay_cvars(app);
        client_commands(app);
    }
}

pub mod binds;
pub mod commands;
pub mod overlays;
pub mod ui;
#[cfg(test)]
mod tests;

pub use binds::*;
pub use commands::*;
pub use overlays::*;
pub use ui::*;
