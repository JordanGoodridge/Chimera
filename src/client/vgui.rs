//! A game's VGUI layouts as the shell drives them: the drawing is the
//! shell's base (`engine_client`'s `vgui`, re-exported here whole); this
//! adds what needs the shell's console and chat: the mouse freed while a
//! game-look menu is open (`VguiOpen`) and grabbed again when it closes,
//! and `keys_live` (whether the menus may read the keyboard).

use bevy::{prelude::*, window::CursorOptions};

pub use engine_client::client::vgui::*;

/// The base's plugin (`VguiOpen`, button lighting), then the mouse grab.
pub struct VguiPlugin;

impl Plugin for VguiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(engine_client::client::vgui::VguiPlugin)
            .add_systems(PostUpdate, cursor.after(super::front_end::FrontEndSync));
    }
}

/// The mouse is free while a game-look menu is open; when it closes, it is
/// grabbed again once the button is up (so the click that closed it isn't
/// a shot), unless the console or the game menu took over.
fn cursor(
    open: Res<VguiOpen>,
    mouse: Res<ButtonInput<MouseButton>>,
    ui: Option<Res<super::console::ConsoleUi>>,
    game_menu: Option<Res<super::front_end::MenuStatus>>,
    cursor: Option<Single<&mut CursorOptions>>,
    mut freed: Local<bool>,
) {
    let Some(mut cursor) = cursor else { return };
    if open.any() {
        if super::input::cursor_grabbed(&cursor) && !ui.as_ref().is_some_and(|u| u.open) {
            super::input::release_cursor(&mut cursor);
        }
        *freed = true;
    } else if *freed && !mouse.pressed(MouseButton::Left) {
        *freed = false;
        let elsewhere = ui.is_some_and(|u| u.open) || game_menu.is_some_and(|m| m.open);
        if !elsewhere {
            super::input::capture_cursor(&mut cursor);
        }
    }
}

/// Whether the menus may read the keyboard: playing (mouse grabbed) or a
/// game-look menu open, and nothing else typing.
pub fn keys_live(
    cursor: &CursorOptions,
    open: &VguiOpen,
    console: Option<&super::console::ConsoleUi>,
    chat: Option<&super::chat::ChatInput>,
    game_menu_open: bool,
) -> bool {
    let busy = console.is_some_and(|c| c.open) || chat.is_some_and(|c| c.open.is_some()) || game_menu_open;
    !busy && (super::input::cursor_grabbed(cursor) || open.any())
}

