//! The main menu and the in-game menu (Esc, or `menu [page]`), one menu
//! drawn as CS:S's GameUI draws it. Out of a game (startup without a map,
//! after `disconnect`) it is the main menu: always open, over the game's
//! background picture, the game's title over its entries, in-game entries
//! hidden. In a game: the entries at the left over the darkened game. Each
//! dialog is a VGUI frame (`widgets`: moved by its title bar, brought to
//! the front by a click, closed from its X) laid out from the install's
//! layouts: the options (`OptionsSub*.res` per tab, OK / Cancel / Apply,
//! the keyboard, video and multiplayer tabs' Advanced dialogs, the last
//! listing `cfg/user.scr`), Create Server (its
//! Server, Game and Bot pages: `CreateMultiplayerGame*Page.res`, the Game
//! page's options from `cfg/settings.scr`); controls the game has and
//! mashup lacks are drawn greyed. Ours (settings CS:S's options don't
//! have) are in our own dialog, Lucker Party Options, from our entries.
//! In the install's GameUI scheme (`map::hud::GameUi`); without the
//! install, in built-in colours and words, controls in a column. The game
//! keeps running while it is open; the mouse is free, so the local
//! player's input is ignored (`input::write_local_intent`). Every choice
//! is a console line (`map`, `bot_add`, `bind`, cvars ...), so the menu
//! holds no game logic: the model (`GameMenu::handle`) turns keys and
//! clicks into those lines, and is unit-tested without a window.
//!
//! The main menu and the game menu (one module per dialog: `main_menu.rs` the
//! left-hand entries, `options_pages.rs` the options' tabs and their Advanced
//! dialogs, `create_server.rs`, `loading.rs` the loading and Disconnected
//! dialogs, `extras.rs` ours (Lucker Party Options, Bots, Team); `model.rs` the
//! shared model, `draw.rs` the drawing, `systems.rs` input and the Bevy
//! systems, `tests.rs`): Esc, `menu [page or options tab]`,
//! `gameui_activate`/`gameui_hide`), one implementation drawn as CS:S's GameUI.
//! Out of a game (startup without a map, `Args::starts_in_game`; after
//! `disconnect`) it is the main menu: open for good over the game's background
//! picture (`GameUi::background_for` the window's shape; black without it),
//! in-game entries hidden, a map it starts shown loading in GameUI's loading
//! dialog (`GameUi::loading`, the install's `LoadingDialogNoBanner.res`: the
//! stage the loader reported through `map::loading`, worded by the game's
//! `LoadingProgress_*` strings, a progress bar, Cancel back to the main menu)
//! until `console::finish_map_load` puts the game in (`entered_game`); in a
//! game, over the darkened game, a dialog's frame over the world drawn again,
//! dimmed (`world_behind_dialogs`: while a dialog is open the 3D view draws
//! into a picture shown under the UI and, at 40 %, under each frame, so the
//! menu's entries under it don't show). The same dialog over the main menu's
//! background for network play (`joining`, `dialog_state`): `connect`
//! (connecting, retrieving server info), the server's map changes ("Server is
//! changing level..."), downloading the map (the bar follows the bytes), the
//! map's load, the host's own map change, and "Starting local game server..."
//! when `maxplayers` > 1; failures and refusals in it ("Disconnected",
//! `net_failure_text`: the game's strings for a full server, older or newer
//! server, timeouts and download errors, else the server's words; Close);
//! `mashup_loading_details 1` adds a detailed view under it (`LoadTimeline`:
//! each stage's time, bytes and percentage; the server's name, address, map,
//! players and ping). The entries at the left: the game's `GameMenu.res`
//! entries mashup can do, in its words and order (Resume and Disconnect only in
//! a game, Find Servers opens the server browser (`server_browser.rs`), Create
//! Server, report a bug), then ours after a gap (quick start: rounds on
//! de_dust2 with bots; the greybox test map; bots and team in a game; Lucker
//! Party Options: the settings CS:S's options don't have; console), under the
//! game's title (`gameinfo.txt` title lines in the scheme's title font, else
//! `MASHUP`); each dialog a VGUI frame (`widgets.rs`: moved by its title bar,
//! brought to the front by a click, closed from its X, centred the first time
//! it opens in a session) in the GameUI scheme (`map::hud::GameUi`, read on a
//! thread at startup; built-in colours and words without it), its controls
//! where the install's layout puts them and greyed where mashup lacks what they
//! set (`page_controls`; without the layouts, a column). Dialogs: Create Server
//! (`CreateMultiplayerGame*Page.res` on a property sheet: Server (the map
//! drop-down with `< Random Map >` first, include bots, how many, the
//! difficulty radio buttons: a preset of
//! `bot_reaction`/`bot_aim_error`/`bot_turn_rate`), Game (the install's
//! `cfg/settings.scr` options mashup has, in a scrolled list, plus ours, rounds
//! or deathmatch), Bot (which team bots join, `bot_prefix`,
//! `bot_join_after_player`); Start sets what changed and loads the map,
//! Cancel), bots (`bot_add`, `bot_kick`), team (`jointeam`; Spectate greyed
//! when the server's replicated `mp_allowspectators` is 0), options (512 x 406
//! with OK / Cancel / Apply: changes apply at once, Cancel puts back what
//! changed since it opened or Apply) with tabs: Keyboard (the game's
//! `kb_act.lst` actions plus ours, with their keys; select, then click again,
//! Enter or "Edit key" and press a key, mouse button or wheel; "Clear Key";
//! "Use Defaults"; actions mashup lacks are greyed; "Advanced...": the dialog
//! of `OptionsSubKeyboardAdvancedDlg.res`, fast weapon switch and developer
//! console (`con_enable`), OK keeps, Cancel or Esc puts back), Mouse, Audio,
//! Video (an aspect ratio filtering the resolutions; "Adjust brightness
//! levels...": `OptionsSubVideoGammaDlg.res`, `mat_monitorgamma` with the
//! install's `gamma` test picture; its "Advanced...":
//! `OptionsSubVideoAdvancedDlg.res`, texture detail, antialiasing, filtering,
//! water detail, vertical sync, HDR, field of view; Virtual Reality Mode left
//! out, `draw::LEFT_OUT`), Voice (greyed: no voice chat), Multiplayer (the
//! controls of `OptionsSub*.res` for the archived cvars in `options.rs`:
//! sliders with tick marks, drop-downs, check boxes, the sensitivity's text
//! entry, the crosshair preview; its "Advanced...":
//! `MultiplayerAdvancedDialog.res` listing the install's `cfg/user.scr` options
//! (`GameUi::user_settings`), those mashup has as controls, the rest greyed,
//! set on OK); docs/plans/active/ui-parity.md lists every control, its cvar and
//! why the rest are greyed; `bugreport`; `quit`. Tab and Shift+Tab walk a
//! dialog's controls, Ctrl+Tab its tabs, Enter presses the default button (OK,
//! Start), Space the focused control, Esc cancels; `menuinput` drives it. A
//! model (`GameMenu::handle`: keys and clicks in, console lines out) and its
//! drawing; a new game's bots are added once the `map` load finishes. While
//! open the mouse is free (so the local player's input is ignored), binds don't
//! run, the other menus close; Esc with the console open only closes the
//! console. Text in the GameUI scheme's fonts (`fonts.rs`: Tahoma and Verdana
//! where installed). The interface sounds (`GameUi::sounds`:
//! `ui/buttonrollover` as the pointer comes onto an entry, `ui/buttonclick` and
//! `ui/buttonclickrelease` as one is pressed and let go); no menu music (the
//! install has none for CS:S).

mod create_server;
mod draw;
mod extras;
mod loading;
mod main_menu;
mod model;
mod options_pages;
mod systems;
#[cfg(test)]
mod tests;

pub use self::{create_server::*, draw::MENU_Z, loading::*, main_menu::*, model::*};
pub(in crate::games::cs_source::ui) use self::systems::*;
use self::{draw::*, options_pages::*};

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::{Arc, Mutex},
};

use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
        mouse::AccumulatedMouseScroll,
    },
    prelude::*,
    text::LineBreak,
    ui::RelativeCursorPosition,
    window::CursorOptions,
};

pub(in crate::games::cs_source::ui) use crate::client::widgets::{Look, bevel, label, place};
use super::options::{Place, SETTINGS, SettingKind, TABS, Tab, VALUE_ENTRIES};
use crate::client::{
    binds,
    fonts::UiFonts,
    front_end::{FrontEnd, FrontEndSync, MenuStatus},
    widgets::{
        self, ButtonState as Btn, Caret, ComboEvent, ComboKey, ComboList, Edit, FrameSpec, SliderDrag, SliderOwner,
        SliderTrack, WindowId, Windows,
    },
};
use crate::{
    console::{Console, ConsoleAppExt},
    core::{Intent, Team},
    map::hud::{GameUi, KeyAction, ServerSettingKind, UiControl, UiKind, UiLayout},
};

pub struct GameMenuPlugin;

/// Copy what the shell reads of the menu into `MenuStatus` whenever the
/// menu changes.
fn sync_status(menu: Res<GameMenu>, mut status: ResMut<MenuStatus>) {
    if menu.is_changed() {
        *status = MenuStatus {
            open: menu.open,
            in_game: menu.in_game,
            loading: menu.loading.is_some(),
            failed: menu.failure.is_some(),
        };
    }
}

impl Plugin for GameMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameMenu>()
            .init_resource::<LoadingDetails>()
            .init_resource::<LoadTimeline>()
            .init_resource::<AfterLoad>()
            .init_resource::<RegrabCursor>()
            .init_resource::<MenuUi>()
            .add_systems(Startup, (start_loading_ui, start_session))
            .add_systems(
                Update,
                (
                    ui_loaded,
                    keys.before(crate::client::console::toggle),
                    text_keys,
                    pointer,
                    sync,
                    cursor,
                    after_load,
                    menu_sounds,
                    draw,
                    loading_progress,
                )
                    .chain()
                    .in_set(MenuSystems),
            )
            .add_systems(Update, world_behind_dialogs.after(MenuSystems));
        // The shell's view of the menu (`client::front_end`).
        app.init_resource::<MenuStatus>()
            .add_systems(First, sync_status.in_set(FrontEndSync))
            .add_systems(Update, sync_status.in_set(FrontEndSync).after(MenuSystems))
            .add_systems(PostUpdate, sync_status.in_set(FrontEndSync));
        FrontEnd::register(app, |f| {
            f.entered_game = entered_game;
            f.left_game = left_game;
            f.joining = joining;
            f.show_failure = show_failure;
            f.map_load_failed = map_load_failed;
        });
        crate::console::resource_cvar::<LoadingDetails, u8>(
            app,
            "mashup_loading_details",
            "1: the loading dialog adds a detailed view (each stage's time, bytes and percentage; the server's name, \
             map, players and ping).",
            |d| &mut d.0,
        );
        app.console_command(
            "menu",
            "menu [main|newgame|game|bot|bots|team|options|keyboard|mouse|audio|video|voice|multiplayer|advanced|\
             videoadvanced|gamma|mpadvanced|extras]: open the game menu (Esc) on a page (newgame: Create Server, game and \
             bot its other pages; options: on a tab; advanced: the keyboard tab's Advanced dialog, videoadvanced the \
             video tab's, gamma its brightness dialog, mpadvanced the multiplayer tab's; extras: Lucker Party Options).",
            |w, a| {
                let arg = a.first().map(|s| s.to_lowercase());
                let tab = TABS.iter().find(|(t, ..)| Some(t.page()) == arg.as_deref()).map(|(t, ..)| *t);
                let page = match arg.as_deref() {
                    _ if tab.is_some() => Page::Settings,
                    None | Some("main") => Page::Main,
                    Some("newgame" | "createserver" | "game" | "bot") => Page::NewGame,
                    Some("bots") => Page::Bots,
                    Some("team") => Page::Team,
                    Some("options" | "settings" | "advanced" | "videoadvanced" | "gamma" | "mpadvanced") => Page::Settings,
                    Some("extras") => Page::Extras,
                    Some(p) => return Err(format!("no menu page \"{p}\"")),
                };
                open_menu(w, page);
                let mut menu = w.resource_mut::<GameMenu>();
                if let Some(tab) = tab {
                    menu.set_tab(tab);
                }
                match arg.as_deref() {
                    Some("game") => menu.set_create_tab(1),
                    Some("bot") => menu.set_create_tab(2),
                    Some("advanced") => {
                        menu.set_tab(Tab::Keyboard);
                        menu.press_action(&Action::Advanced);
                    }
                    Some("videoadvanced") => {
                        menu.set_tab(Tab::Video);
                        menu.press_action(&Action::VideoAdvanced);
                    }
                    Some("gamma") => {
                        menu.set_tab(Tab::Video);
                        menu.press_action(&Action::Gamma);
                    }
                    Some("mpadvanced") => {
                        menu.set_tab(Tab::Multiplayer);
                        menu.press_action(&Action::MultiplayerAdvanced);
                    }
                    _ => {}
                }
                Ok(None)
            },
        )
        .console_command(
            "menuinput",
            "menuinput <input>: drive the open game menu as keys and clicks would (screenshots, tests): \
             focus <cvar|map|bots|botcount|botteam|difficulty|aspect> | open (the focused combo box's list) | \
             down | up | enter | space | escape | tab | backtab | type <text> | pick <n> | sheet <n>; prints \
             the focused control.",
            |w, a| {
                let input = menu_input(&w.resource::<GameMenu>(), a)?;
                let mut menu = w.resource_mut::<GameMenu>();
                let mut next = menu.clone();
                let out = next.handle(input);
                *menu = next;
                let state = menu.describe();
                let mut console = w.resource_mut::<Console>();
                for line in out.lines {
                    console.submit(line);
                }
                Ok(Some(state))
            },
        )
        .console_command("gameui_activate", "Open the game menu (Esc).", |w, _| {
            open_menu(w, Page::Main);
            Ok(None)
        })
        .console_command("gameui_hide", "Close the game menu (in a game).", |w, _| {
            let mut menu = w.resource_mut::<GameMenu>();
            if !menu.in_game {
                return Err("the main menu stays open out of a game".into());
            }
            menu.open = false;
            w.resource_mut::<RegrabCursor>().0 = true;
            Ok(None)
        });
    }
}

/// The menu's systems (the server browser runs after them).
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct MenuSystems;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Main,
    /// Create Server (its page: `GameMenu::create_tab`).
    NewGame,
    Bots,
    Team,
    /// The options dialog (its tab: `GameMenu::tab`).
    Settings,
    /// The keyboard tab's Advanced dialog (over the options).
    KeyboardAdvanced,
    /// The video tab's Advanced dialog (over the options).
    VideoAdvanced,
    /// The video tab's brightness dialog (over the options).
    Gamma,
    /// The multiplayer tab's Advanced dialog (over the options; its list
    /// from the install's `cfg/user.scr`).
    MultiplayerAdvanced,
    /// Ours: Lucker Party Options (what CS:S's options don't have).
    Extras,
}

impl Page {
    /// Its frame (`widgets::Windows` keeps where it was dragged).
    pub fn window(self) -> WindowId {
        match self {
            Page::Main => "main",
            Page::NewGame => "createserver",
            Page::Bots => "bots",
            Page::Team => "team",
            Page::Settings => "options",
            Page::KeyboardAdvanced => "keyboardadvanced",
            Page::VideoAdvanced => "videoadvanced",
            Page::Gamma => "gamma",
            Page::MultiplayerAdvanced => "multiplayeradvanced",
            Page::Extras => "extras",
        }
    }

    /// A dialog over the options (modal to them).
    fn over_options(self) -> bool {
        matches!(self, Page::KeyboardAdvanced | Page::VideoAdvanced | Page::Gamma | Page::MultiplayerAdvanced)
    }
}
