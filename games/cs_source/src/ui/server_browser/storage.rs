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

use super::super::game_menu::GameMenu;
use super::*;
use crate::client::dialog;
use crate::client::widgets::Interaction;
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

/// Where favourites and history are kept: the cfg folder's
/// `serverbrowser.vdf`.
pub fn saved_path() -> Option<PathBuf> {
    crate::console::cfg_dir().map(|d| d.join("serverbrowser.vdf"))
}

pub(super) fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// "5:04 PM"-like time a server was last played: here, how long ago.
pub fn last_played_text(then: u64, now: u64) -> String {
    if then == 0 {
        return String::new();
    }
    let ago = now.saturating_sub(then);
    match ago {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", ago / 60),
        3600..86400 => format!("{} h ago", ago / 3600),
        _ => format!("{} days ago", ago / 86400),
    }
}

pub(super) fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `serverbrowser`'s words as an input.
pub(super) fn browser_input(a: &[String]) -> Result<Input, String> {
    let n = |i: usize| a.get(i).and_then(|v| v.parse::<usize>().ok()).ok_or("a row number");
    let button = |b| Ok(Input::Click(Target::Button(b)));
    match a.first().map(|s| s.to_lowercase()).as_deref() {
        Some("row") => Ok(Input::Click(Target::Row(n(1)?))),
        Some("doubleclick") => Ok(Input::DoubleClick(n(1)?)),
        Some("connect") => button(ButtonId::Connect),
        Some("refresh") => button(ButtonId::Refresh),
        Some("quickrefresh") => button(ButtonId::QuickRefresh),
        Some("addserver") => button(ButtonId::AddServer),
        Some("filters") => button(ButtonId::Filters),
        Some("find") => button(ButtonId::AddFind),
        Some("addselected") => button(ButtonId::AddSelected),
        Some("type") => Ok(Input::Type(a[1..].join(" "))),
        Some("latency") => Ok(Input::Click(Target::Latency)),
        Some("pick") => Ok(Input::Click(Target::ComboItem(n(1)?))),
        Some("hover") => Ok(Input::Hover(Target::ComboItem(n(1)?))),
        Some("check") => match a.get(1).map(|s| s.to_lowercase()).as_deref() {
            Some("notfull") => Ok(Input::Click(Target::Check(Check::NotFull))),
            Some("notempty") => Ok(Input::Click(Target::Check(Check::NotEmpty))),
            Some("nopassword") => Ok(Input::Click(Target::Check(Check::NoPassword))),
            _ => Err("check notfull|notempty|nopassword".into()),
        },
        Some("map") => Ok(Input::Click(Target::Field(Field::MapFilter))),
        Some("maxplayers") => Ok(Input::Click(Target::Field(Field::MaxPlayers))),
        Some("next") => Ok(Input::Tab(1)),
        Some("space") => Ok(Input::Space),
        Some("enter") => Ok(Input::Enter),
        Some("escape") => Ok(Input::Escape),
        Some("delete") => Ok(Input::Delete),
        Some("down") => Ok(Input::Down),
        Some("up") => Ok(Input::Up),
        Some("tab") => match a.get(1).map(|s| s.to_lowercase()).as_deref() {
            Some("lan") => Ok(Input::Click(Target::Tab(Tab::Lan))),
            Some("favorites" | "favourites") => Ok(Input::Click(Target::Tab(Tab::Favorites))),
            Some("history") => Ok(Input::Click(Target::Tab(Tab::History))),
            _ => Err("tab lan|favorites|history".into()),
        },
        Some("sort") => {
            let c = match a.get(1).map(|s| s.to_lowercase()).as_deref() {
                Some("password") => Column::Password,
                Some("bots") => Column::Bots,
                Some("name") => Column::Name,
                Some("game") => Column::Game,
                Some("players") => Column::Players,
                Some("map") => Column::Map,
                Some("latency") => Column::Latency,
                Some("lastplayed") => Column::LastPlayed,
                _ => return Err("sort name|players|map|latency|password|bots|game|lastplayed".into()),
            };
            Ok(Input::Click(Target::Header(c)))
        }
        _ => Err("serverbrowser row <n> | doubleclick <n> | connect | refresh | ... (help serverbrowser)".into()),
    }
}

/// The rows as `serverbrowser` prints them.
pub(super) fn describe_rows(b: &ServerBrowser) -> String {
    let mut out = vec![format!(
        "{} tab, {} of {} servers{}{}",
        TABS.iter().find(|t| t.0 == b.tab).map_or("", |t| t.2),
        b.rows.len(),
        b.total,
        if b.refreshing { ", refreshing" } else { "" },
        match &b.dialog {
            Dialog::None => String::new(),
            Dialog::AddServer { .. } => ", add server dialog".into(),
            Dialog::Password { .. } => ", password dialog".into(),
        }
    )];
    for (i, r) in b.rows.iter().enumerate() {
        out.push(format!(
            "{}{i}: {}  {}  {}/{} ({} bots)  {}  {}{}",
            if b.selected == Some(r.addr) { "*" } else { " " },
            r.addr,
            r.name,
            r.players,
            r.max_players,
            r.bots,
            r.map,
            r.ping.map_or("-".into(), |p| format!("{p} ms")),
            if r.password { "  password" } else { "" }
        ));
    }
    out.join("\n")
}

