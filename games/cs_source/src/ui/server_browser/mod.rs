//! The server browser (Find Servers, `openserverbrowser`), drawn as
//! CS:S's: a frame over the menu with the Internet, Favorites, History
//! and Lan tabs, the server list (password and bot icons, name, game,
//! players, map, latency; History adds when last played), sorting by a
//! column header, filters (map, latency, max players, has users playing,
//! not full, no password), refresh, Add a Server, Connect (double-click
//! a row too) and the password dialog; laid out from the install's
//! `servers/*.res` (`GameUi::servers`), in its words and icons; built-in
//! sizes and words without the install.
//!
//! Servers are found with `net::query`: the Lan tab scans
//! `net_lan_ports`; Favorites and History ask each saved address.
//! Internet stays greyed until there is a master server to list servers
//! (docs/plans/active/multiplayer.md, slice 8). Favorites and History
//! live in the cfg folder (`serverbrowser.vdf`); a server joined goes
//! into History. Joining is the console's `connect` (with `password`
//! first when the server wants one): the browser closes and the game's
//! connect flow takes over.
//!
//! As the game menu, the model (`ServerBrowser::handle`) turns clicks and
//! keys into console lines and queries, unit-tested without a window.
//!
//! The server browser (Find Servers, `openserverbrowser [tab]`; `serverbrowser
//! <input>` drives it from the console), drawn as CS:S's over the menu from the
//! install's `servers/*.res` layouts (`GameUi::servers`), words and column
//! icons: tabs Internet (greyed: no master server), Favorites, History, Lan;
//! the list (password and bot icons, name with the count, game, players, map,
//! latency; History's last played), sorted by a header click (its arrow),
//! columns sized by dragging a header's edge, filters (map and max players text
//! entries, the latency drop-down, has users playing, not full, no password
//! check boxes; game, location and anti-cheat shown greyed), Tab order and the
//! default button as VGUI's, the frame moved by its title bar and resized from
//! its edges and corner (its list grows, buttons and filters follow their
//! `pinCorner`/`autoResize`), Refresh / Quick refresh, Add a Server (its
//! dialog: add the address, find games at it, add the server found), Connect or
//! a double-click (`connect`, after the password dialog for a server with a
//! password: `password`), Delete takes a server off Favorites or History.
//! Servers come from `net::query::ServerQueries` (Lan: a scan of
//! `net_lan_ports`; Favorites and History: each address asked). Favourites and
//! history in the cfg folder's `serverbrowser.vdf`; a server joined goes first
//! in History. A model (`ServerBrowser::handle`: clicks and keys in, console
//! lines and queries out) and its drawing (the game menu's `Look` and helpers).

use crate::client::dialog;
use crate::client::widgets::Interaction;
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::{
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
        mouse::AccumulatedMouseScroll,
    },
    prelude::*,
    ui::RelativeCursorPosition,
};

use super::game_menu::GameMenu;
use crate::client::{
    fonts::UiFonts,
    widgets::{self, Caret, ComboEvent, ComboKey, ComboList, Edit, FrameSpec, Look, Windows, bevel, label, place},
};
use crate::{
    console::{Console, ConsoleAppExt},
    map::hud::{GameUi, UiControl, UiLayout},
    net::{
        NET_VERSION,
        query::{LanSettings, QueryResult, QueryState, ServerQueries},
    },
};

pub struct ServerBrowserPlugin;

impl Plugin for ServerBrowserPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ServerBrowser>()
            .init_resource::<BrowserIcons>()
            .init_resource::<RequestInbox>()
            .add_systems(Startup, load_saved)
            .add_systems(
                Update,
                (
                    record_history,
                    sync_results,
                    follow_size,
                    keys,
                    pointer,
                    run_requests,
                    draw,
                )
                    .chain()
                    .after(super::game_menu::MenuSystems),
            );
        app.console_command(
            "openserverbrowser",
            "openserverbrowser [lan|favorites|history]: open Find Servers (the server browser).",
            |w, a| {
                let tab = match a.first().map(|s| s.to_lowercase()).as_deref() {
                    None => None,
                    Some("lan") => Some(Tab::Lan),
                    Some("favorites" | "favourites") => Some(Tab::Favorites),
                    Some("history") => Some(Tab::History),
                    Some("internet") => return Err("the Internet tab needs a master server (not yet)".into()),
                    Some(t) => return Err(format!("no tab \"{t}\"")),
                };
                super::game_menu::open_main(w);
                let out = w.resource_mut::<ServerBrowser>().open(tab);
                w.resource_mut::<RequestInbox>().0.extend(out.requests);
                Ok(None)
            },
        )
        .console_command(
            "serverbrowser",
            "serverbrowser <input>: drive the open server browser as a click or key would (testing): \
             row <n> | doubleclick <n> | connect | refresh | quickrefresh | addserver | filters | \
             sort <name|players|map|latency|...> | tab <lan|favorites|history> | type <text> | enter | escape | \
             delete | down | up | latency (opens its list) | hover <n> | pick <n> | check <notfull|notempty|\
             nopassword> | map | maxplayers (their text entries) | next (Tab) | space; prints its rows.",
            |w, a| {
                let input = browser_input(a)?;
                let out = w.resource_mut::<ServerBrowser>().handle(input);
                let mut console = w.resource_mut::<Console>();
                for line in out.lines {
                    console.submit(line);
                }
                w.resource_mut::<RequestInbox>().0.extend(out.requests);
                let b = w.resource::<ServerBrowser>();
                if out.save {
                    save(b);
                }
                Ok(Some(describe_rows(b)))
            },
        );
    }
}


pub mod network;
pub mod storage;
pub mod types;
pub mod ui;
#[cfg(test)]
mod tests;

pub use network::*;
pub use storage::*;
pub use types::*;
pub use ui::*;
