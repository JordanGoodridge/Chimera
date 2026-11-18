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

// ---------------------------------------------------------------------------
// The model.

/// The browser's tabs, in CS:S's order (the ones Chimera has).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    Internet,
    Favorites,
    History,
    #[default]
    Lan,
}

/// Tab, its `#token`, our word for it.
pub const TABS: [(Tab, &str, &str); 4] = [
    (Tab::Internet, "#ServerBrowser_InternetTab", "Internet"),
    (Tab::Favorites, "#ServerBrowser_FavoritesTab", "Favorites"),
    (Tab::History, "#ServerBrowser_HistoryTab", "History"),
    (Tab::Lan, "#ServerBrowser_LanTab", "Lan"),
];

impl Tab {
    /// Whether it can be opened: Internet needs a master server.
    pub fn enabled(self) -> bool {
        self != Tab::Internet
    }
}

/// The list's columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Column {
    Password,
    Bots,
    Name,
    Game,
    Players,
    Map,
    Latency,
    LastPlayed,
}

/// Latency filter choices, ms (0: all), and their words.
pub const LATENCIES: [(u32, &str, &str); 7] = [
    (0, "#ServerBrowser_All", "<All>"),
    (50, "#ServerBrowser_LessThan50", "< 50"),
    (100, "#ServerBrowser_LessThan100", "< 100"),
    (150, "#ServerBrowser_LessThan150", "< 150"),
    (250, "#ServerBrowser_LessThan250", "< 250"),
    (350, "#ServerBrowser_LessThan350", "< 350"),
    (600, "#ServerBrowser_LessThan600", "< 600"),
];

/// A saved server (favourite or history).
#[derive(Clone, Debug, PartialEq)]
pub struct SavedServer {
    pub addr: SocketAddr,
    /// Its name when last heard.
    pub name: String,
    /// When last joined, Unix seconds (History; 0: never).
    pub last_played: u64,
}

/// The filters (`Filters` toggles them shown).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filters {
    pub shown: bool,
    /// Map names containing this (any case).
    pub map: String,
    /// Servers taking at most this many players ("" or 0: any).
    pub max_players: String,
    /// Index into `LATENCIES`.
    pub latency: usize,
    pub not_empty: bool,
    pub not_full: bool,
    pub no_password: bool,
}

impl Filters {
    fn max_players(&self) -> Option<u32> {
        self.max_players.trim().parse::<u32>().ok().filter(|n| *n > 0)
    }

    /// Whether a row passes. A server that hasn't answered passes (it is
    /// listed as not responding in Favorites and History).
    pub fn pass(&self, row: &ServerRow) -> bool {
        if !row.responded {
            return true;
        }
        let map = self.map.trim().to_lowercase();
        let limit = LATENCIES[self.latency.min(LATENCIES.len() - 1)].0;
        (map.is_empty() || row.map.to_lowercase().contains(&map))
            && self.max_players().is_none_or(|n| row.max_players <= n)
            && (limit == 0 || row.ping.is_some_and(|p| p < limit))
            && (!self.not_empty || row.players > row.bots)
            && (!self.not_full || row.players < row.max_players)
            && (!self.no_password || !row.password)
    }

    pub fn active(&self) -> bool {
        !self.map.trim().is_empty()
            || self.max_players().is_some()
            || self.latency > 0
            || self.not_empty
            || self.not_full
            || self.no_password
    }
}

/// A row of the list.
#[derive(Clone, Debug, PartialEq)]
pub struct ServerRow {
    pub addr: SocketAddr,
    pub name: String,
    pub game: String,
    pub map: String,
    pub players: u32,
    pub max_players: u32,
    pub bots: u32,
    pub password: bool,
    pub ping: Option<u32>,
    /// It answered (else: asking, or not responding).
    pub responded: bool,
    /// Asked and given up on.
    pub not_responding: bool,
    /// Another build: joining would be refused.
    pub other_version: bool,
    pub last_played: u64,
}

impl Default for ServerRow {
    fn default() -> Self {
        Self {
            addr: SocketAddr::from(([0, 0, 0, 0], 0)),
            name: String::new(),
            game: String::new(),
            map: String::new(),
            players: 0,
            max_players: 0,
            bots: 0,
            password: false,
            ping: None,
            responded: false,
            not_responding: false,
            other_version: false,
            last_played: 0,
        }
    }
}

/// A text box that takes typing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    MapFilter,
    MaxPlayers,
    AddAddress,
    Password,
}

/// A dialog over the browser.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Dialog {
    #[default]
    None,
    /// Add a Server: the address typed, the address being looked at
    /// (`Find games at this address...`) and whether its row is selected.
    AddServer {
        text: String,
        looked_up: Option<SocketAddr>,
        selected: bool,
        error: Option<String>,
    },
    /// The server wants a password.
    Password {
        addr: SocketAddr,
        name: String,
        entry: dialog::Dialog,
    },
}

/// What a click is on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Tab(Tab),
    Header(Column),
    /// A row of the list (index into `rows`).
    Row(usize),
    Scroll(i32),
    Button(ButtonId),
    Field(Field),
    /// The latency filter's combo box (a click opens its list).
    Latency,
    /// An entry of the open list.
    ComboItem(usize),
    /// Anywhere outside the open list (closes it).
    Outside,
    Check(Check),
}

/// A control the keyboard is on (other than a text entry, `typing`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    List,
    Latency,
    Check(Check),
    Button(ButtonId),
}

/// The browser's frame (`widgets::Windows`).
pub const WINDOW: widgets::WindowId = "servers";
/// Its dialogs' frames.
pub(super) const ADD_WINDOW: widgets::WindowId = "addserver";
pub(super) const PASSWORD_WINDOW: widgets::WindowId = "password";

/// The browser's smallest size, scheme pixels (`CServerBrowserDialog`'s
/// minimum: its layout's own size).
pub const MIN_SIZE: Vec2 = Vec2::new(640.0, 384.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    NotEmpty,
    NotFull,
    NoPassword,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonId {
    Connect,
    Refresh,
    QuickRefresh,
    AddServer,
    Filters,
    Close,
    /// Add server dialog: add the typed address to favourites.
    AddOk,
    /// Add server dialog: ask the typed address.
    AddFind,
    /// Add server dialog: add the found server to favourites.
    AddSelected,
    AddCancel,
    PasswordConnect,
    PasswordCancel,
}

/// A key or pointer event.
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    Click(Target),
    /// A second click on the same row soon after: join it.
    DoubleClick(usize),
    Up,
    Down,
    Enter,
    Escape,
    Delete,
    Backspace,
    Type(String),
    Scroll(i32),
    /// The pointer over a control (an open list's entries highlight).
    Hover(Target),
    /// Tab (+1) / Shift+Tab (-1): the next control.
    Tab(i32),
    /// Space: the focused control (ticks, opens the list, presses).
    Space,
    /// An editing key in the text entry typed into.
    Edit(Edit),
    /// Ctrl+C / Ctrl+X there.
    Copy,
    Cut,
    /// A text entry clicked at a char.
    Caret(Field, usize),
    /// A column's header edge dragged: its width (scheme pixels).
    ColumnWidth(Column, f32),
}

/// Queries the browser asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// Scan the LAN (`net_lan_ports`).
    ScanLan,
    /// Ask these servers.
    Query(Vec<SocketAddr>),
}

/// What an input asks for besides the browser's own state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    pub lines: Vec<String>,
    pub requests: Vec<Request>,
    /// Favourites or history changed: write them out.
    pub save: bool,
    /// Text for the clipboard (copied or cut).
    pub clipboard: Option<String>,
}

/// Rows shown at once (without and with the filters), as the list's
/// height allows at `ROW_H`.
pub const ROW_H: f32 = 18.0;
pub const HEADER_H: f32 = 20.0;
/// History and favourites kept at most.
pub const MAX_SAVED: usize = 100;

/// The browser's state.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct ServerBrowser {
    pub open: bool,
    pub tab: Tab,
    pub favorites: Vec<SavedServer>,
    pub history: Vec<SavedServer>,
    pub filters: Filters,
    /// Sort column and descending.
    pub sort: Option<(Column, bool)>,
    /// The selected server.
    pub selected: Option<SocketAddr>,
    /// The first row shown.
    pub scroll: usize,
    pub dialog: Dialog,
    /// The text box typing goes into.
    pub typing: Option<Field>,
    /// Its caret and selection.
    pub caret: Caret,
    /// The control the keyboard is on otherwise (Tab moves it).
    pub focus: Option<Focus>,
    /// The latency filter's open list.
    pub combo: Option<ComboList>,
    /// Columns given a width by dragging their header's edge (scheme
    /// pixels).
    pub widths: Vec<(Column, f32)>,
    /// How much bigger than its layout the frame was made (scheme pixels):
    /// the list grows, the buttons and filters follow its corners.
    pub grow: Vec2,
    /// The open tab's rows, filtered and sorted (`set_results`).
    pub rows: Vec<ServerRow>,
    /// Rows before filtering (for "Servers (n)").
    pub total: usize,
    /// Queries under way.
    pub refreshing: bool,
    /// Results for the add server dialog's address.
    pub found: Option<ServerRow>,
    /// The game's words, when the install has them.
    pub ui: crate::ui::game_menu::UiText,
}

impl ServerBrowser {
    /// The game's text for a `#token`, else `ours`.
    pub fn text(&self, token: &str, ours: &str) -> String {
        self.ui
            .0
            .as_ref()
            .and_then(|u| u.string(token))
            .map_or_else(|| ours.to_string(), str::to_string)
    }

    /// Open (on a tab, else the last one); the tab refreshes.
    pub fn open(&mut self, tab: Option<Tab>) -> Outcome {
        self.open = true;
        self.dialog = Dialog::None;
        self.typing = None;
        let mut out = Outcome::default();
        self.set_tab(tab.unwrap_or(self.tab), &mut out);
        out
    }

    fn set_tab(&mut self, tab: Tab, out: &mut Outcome) {
        if !tab.enabled() {
            return;
        }
        self.tab = tab;
        self.scroll = 0;
        self.selected = None;
        self.refresh(out);
    }

    fn saved(&self) -> &[SavedServer] {
        match self.tab {
            Tab::Favorites => &self.favorites,
            Tab::History => &self.history,
            _ => &[],
        }
    }

    fn refresh(&mut self, out: &mut Outcome) {
        match self.tab {
            Tab::Lan => out.requests.push(Request::ScanLan),
            Tab::Favorites | Tab::History => {
                let addrs: Vec<SocketAddr> = self.saved().iter().map(|s| s.addr).collect();
                if !addrs.is_empty() {
                    out.requests.push(Request::Query(addrs));
                }
            }
            Tab::Internet => {}
        }
        self.refreshing = !out.requests.is_empty();
    }

    /// Rows for the open tab from what the queries have heard.
    pub fn set_results(&mut self, results: &[QueryResult], busy: bool) {
        let row_of = |r: &QueryResult, saved: Option<&SavedServer>| -> ServerRow {
            let mut row = ServerRow {
                addr: r.addr,
                name: saved.map(|s| s.name.clone()).unwrap_or_default(),
                last_played: saved.map_or(0, |s| s.last_played),
                ..default()
            };
            match &r.state {
                QueryState::Answered { info, ping_ms } => {
                    row.name = info.name.clone();
                    row.game = info.game.clone();
                    row.map = info.map.clone();
                    row.players = info.players as u32;
                    row.max_players = info.max_players as u32;
                    row.bots = info.bots as u32;
                    row.password = info.password;
                    row.ping = Some(*ping_ms);
                    row.responded = true;
                    row.other_version = info.version != NET_VERSION;
                }
                QueryState::NoResponse => row.not_responding = true,
                QueryState::Pending => {}
            }
            row
        };
        let mut rows: Vec<ServerRow> = match self.tab {
            Tab::Lan => results
                .iter()
                .filter(|r| r.lan && matches!(r.state, QueryState::Answered { .. }))
                .map(|r| row_of(r, None))
                .collect(),
            Tab::Favorites | Tab::History => self
                .saved()
                .iter()
                .map(|s| {
                    let r = results
                        .iter()
                        .find(|r| r.addr == s.addr)
                        .cloned()
                        .unwrap_or(QueryResult {
                            addr: s.addr,
                            state: QueryState::Pending,
                            lan: false,
                        });
                    row_of(&r, Some(s))
                })
                .collect(),
            Tab::Internet => Vec::new(),
        };
        // Names learned go into the saved lists.
        for row in rows.iter().filter(|r| r.responded) {
            for s in self.favorites.iter_mut().chain(self.history.iter_mut()) {
                if s.addr == row.addr && s.name != row.name {
                    s.name = row.name.clone();
                }
            }
        }
        if let Dialog::AddServer {
            looked_up: Some(addr), ..
        } = &self.dialog
        {
            self.found = results.iter().find(|r| r.addr == *addr).map(|r| row_of(r, None));
        }
        self.total = rows.len();
        rows.retain(|r| self.filters.pass(r));
        self.sort_rows(&mut rows);
        self.rows = rows;
        self.refreshing = busy;
        self.scroll = self.scroll.min(self.rows.len().saturating_sub(self.shown_rows()));
    }

    fn sort_rows(&self, rows: &mut [ServerRow]) {
        // Unsorted: by latency, as Source lists (unanswered last).
        let (column, desc) = self.sort.unwrap_or((Column::Latency, false));
        rows.sort_by(|a, b| {
            use std::cmp::Ordering;
            let by = match column {
                Column::Password => a.password.cmp(&b.password),
                Column::Bots => (a.bots > 0).cmp(&(b.bots > 0)),
                Column::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                Column::Game => a.game.cmp(&b.game),
                Column::Players => a.players.cmp(&b.players),
                Column::Map => a.map.to_lowercase().cmp(&b.map.to_lowercase()),
                Column::Latency => match (a.ping, b.ping) {
                    (Some(x), Some(y)) => x.cmp(&y),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => Ordering::Equal,
                },
                Column::LastPlayed => a.last_played.cmp(&b.last_played),
            };
            let by = if desc { by.reverse() } else { by };
            by.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.addr.cmp(&b.addr))
        });
    }

    /// The list's columns on the open tab: column, header, width (0: the
    /// rest).
    pub fn columns(&self) -> Vec<(Column, String, f32)> {
        let mut c = vec![
            (Column::Password, String::new(), 16.0),
            (Column::Bots, String::new(), 16.0),
            (
                Column::Name,
                self.text("#ServerBrowser_ServersCount", "Servers (%s1)")
                    .replace("%s1", &self.total.to_string()),
                0.0,
            ),
            (Column::Game, self.text("#ServerBrowser_Game", "Game"), 112.0),
            (Column::Players, self.text("#ServerBrowser_Players", "Players"), 55.0),
            (Column::Map, self.text("#ServerBrowser_Map", "Map"), 90.0),
            (Column::Latency, self.text("#ServerBrowser_Latency", "Latency"), 55.0),
        ];
        if self.tab == Tab::History {
            c.push((
                Column::LastPlayed,
                self.text("#ServerBrowser_LastPlayed", "Last played"),
                110.0,
            ));
        }
        c
    }

    /// Rows the list shows at once.
    pub fn shown_rows(&self) -> usize {
        let list_h = if self.filters.shown { 134.0 } else { 226.0 } + self.grow.y;
        ((list_h - HEADER_H - 2.0) / ROW_H).floor().max(1.0) as usize
    }

    /// A column's width: dragged, else its own (0: the rest).
    pub fn column_width(&self, column: Column, own: f32) -> f32 {
        self.widths.iter().find(|(c, _)| *c == column).map_or(own, |(_, w)| *w)
    }

    /// The controls in tab order (`tabPosition`): the page's buttons,
    /// the filters when shown, the list.
    fn tab_order(&self) -> Vec<Option<Focus>> {
        let mut order = vec![
            Some(Focus::Button(ButtonId::Connect)),
            Some(Focus::Button(ButtonId::Refresh)),
            Some(Focus::Button(if self.tab == Tab::Favorites {
                ButtonId::AddServer
            } else {
                ButtonId::QuickRefresh
            })),
            Some(Focus::Button(ButtonId::Filters)),
        ];
        if self.filters.shown {
            order.extend([
                None, // the map text entry
                None, // the max players one
                Some(Focus::Latency),
                Some(Focus::Check(Check::NotFull)),
                Some(Focus::Check(Check::NotEmpty)),
                Some(Focus::Check(Check::NoPassword)),
            ]);
        }
        order.push(Some(Focus::List));
        order
    }

    /// Tab from the control on now.
    fn tab_focus(&mut self, dir: i32) {
        // Text entries stand in the order as None slots, in turn.
        let order = self.tab_order();
        let fields = [Field::MapFilter, Field::MaxPlayers];
        let slots: Vec<(Option<Focus>, Option<Field>)> = {
            let mut k = 0;
            order
                .iter()
                .map(|o| match o {
                    Some(f) => (Some(*f), None),
                    None => {
                        k += 1;
                        (None, Some(fields[k - 1]))
                    }
                })
                .collect()
        };
        let now = slots
            .iter()
            .position(|(f, t)| (f.is_some() && *f == self.focus) || (t.is_some() && *t == self.typing));
        let indices: Vec<usize> = (0..slots.len()).collect();
        let next = widgets::focus_step(&indices, now, dir).unwrap_or(0);
        let (focus, typing) = slots[next];
        self.focus = focus;
        self.set_typing(typing);
    }

    /// Typing goes into a text entry now (all its text selected, as
    /// VGUI's on focus), or none.
    fn set_typing(&mut self, field: Option<Field>) {
        self.typing = field;
        if field.is_some() {
            self.focus = None;
            let n = self.field_mut().map_or(0, |t| t.chars().count());
            self.caret = Caret { at: n, anchor: 0 };
        }
    }

    /// The line for an empty list.
    pub fn empty_text(&self) -> Option<String> {
        if !self.rows.is_empty() || self.refreshing {
            return None;
        }
        Some(match self.tab {
            Tab::Lan => self.text(
                "#ServerBrowser_NoLanServers",
                "There are no servers running on your local network.",
            ),
            Tab::Favorites => self.text(
                "#ServerBrowser_NoFavoriteServers",
                "You currently have no favorite servers selected.",
            ),
            Tab::History => self.text(
                "#ServerBrowser_NoServersPlayed",
                "No servers have been played recently.",
            ),
            Tab::Internet => String::new(),
        })
    }

    /// The status line under the tabs.
    pub fn status(&self) -> String {
        if self.refreshing {
            self.text("#ServerBrowser_RefreshingServerList", "Refreshing server list...")
        } else {
            String::new()
        }
    }

    /// The filters in words (the line next to the Filters button).
    pub fn filter_text(&self) -> String {
        let f = &self.filters;
        let mut parts = Vec::new();
        if !f.map.trim().is_empty() {
            parts.push(format!(
                "{} {}",
                self.text("#ServerBrowser_Map", "Map").to_lowercase(),
                f.map.trim()
            ));
        }
        if f.latency > 0 {
            let (_, token, ours) = LATENCIES[f.latency.min(LATENCIES.len() - 1)];
            parts.push(format!(
                "{} {}",
                self.text("#ServerBrowser_FilterDescLatency", "latency"),
                self.text(token, ours)
            ));
        }
        if let Some(n) = f.max_players() {
            parts.push(format!(
                "{} <= {n}",
                self.text("#ServerBrowser_FilterDescMaxPlayers", "max players")
            ));
        }
        if f.not_full {
            parts.push(self.text("#ServerBrowser_FilterDescNotFull", "is not full"));
        }
        if f.not_empty {
            parts.push(self.text("#ServerBrowser_FilterDescNotEmpty", "is not empty"));
        }
        if f.no_password {
            parts.push(self.text("#ServerBrowser_FilterDescNoPassword", "has no password"));
        }
        parts.join("; ")
    }

    fn selected_row(&self) -> Option<&ServerRow> {
        self.selected.and_then(|a| self.rows.iter().find(|r| r.addr == a))
    }

    /// Join a server: the password dialog first when it wants one.
    fn join(&mut self, addr: SocketAddr, out: &mut Outcome) {
        let row = self
            .rows
            .iter()
            .find(|r| r.addr == addr)
            .cloned()
            .or_else(|| self.found.clone());
        if row.as_ref().is_some_and(|r| r.password) {
            self.dialog = Dialog::Password {
                addr,
                name: row.map(|r| r.name).unwrap_or_default(),
                entry: dialog::Dialog::password("Server Requires Password", "", 128),
            };
            self.set_typing(Some(Field::Password));
            return;
        }
        self.connect(addr, None, out);
    }

    fn connect(&mut self, addr: SocketAddr, password: Option<&str>, out: &mut Outcome) {
        if let Some(p) = password {
            out.lines.push(format!("password {}", crate::console::quote(p)));
        }
        out.lines.push(format!("connect {addr}"));
        self.open = false;
        self.dialog = Dialog::None;
        self.typing = None;
    }

    /// Add a server to the favourites (moved to the end if there).
    pub fn add_favorite(&mut self, addr: SocketAddr, name: &str) {
        self.favorites.retain(|s| s.addr != addr);
        self.favorites.push(SavedServer {
            addr,
            name: name.to_string(),
            last_played: 0,
        });
        if self.favorites.len() > MAX_SAVED {
            self.favorites.remove(0);
        }
    }

    /// A server joined: first in the history.
    pub fn played(&mut self, addr: SocketAddr, name: &str, now: u64) {
        let name = self
            .history
            .iter()
            .chain(&self.favorites)
            .find(|s| s.addr == addr && name.is_empty())
            .map_or(name.to_string(), |s| s.name.clone());
        self.history.retain(|s| s.addr != addr);
        self.history.insert(
            0,
            SavedServer {
                addr,
                name,
                last_played: now,
            },
        );
        self.history.truncate(MAX_SAVED);
    }

    /// Apply an input; what it asks for.
    pub fn handle(&mut self, input: Input) -> Outcome {
        let mut out = Outcome::default();
        if !self.open {
            return out;
        }
        if self.dialog != Dialog::None {
            self.handle_dialog(input, &mut out);
            return out;
        }
        if self.combo.is_some() {
            self.handle_combo(input);
            return out;
        }
        match input {
            Input::Click(Target::Tab(tab)) => self.set_tab(tab, &mut out),
            Input::Click(Target::Header(column)) => {
                self.sort = match self.sort {
                    Some((c, desc)) if c == column => Some((c, !desc)),
                    _ => Some((column, false)),
                };
                let mut rows = std::mem::take(&mut self.rows);
                self.sort_rows(&mut rows);
                self.rows = rows;
            }
            Input::Click(Target::Row(i)) => {
                self.set_typing(None);
                self.focus = Some(Focus::List);
                if let Some(r) = self.rows.get(i) {
                    self.selected = Some(r.addr);
                }
            }
            Input::DoubleClick(i) => {
                if let Some(addr) = self.rows.get(i).map(|r| r.addr) {
                    self.selected = Some(addr);
                    self.join(addr, &mut out);
                }
            }
            Input::Click(Target::Scroll(step)) | Input::Scroll(step) => {
                let max = self.rows.len().saturating_sub(self.shown_rows());
                self.scroll = (self.scroll as i32 + step).clamp(0, max as i32) as usize;
            }
            Input::Click(Target::Button(b)) => {
                self.focus = Some(Focus::Button(b));
                self.set_typing(None);
                self.button(b, &mut out);
            }
            Input::Click(Target::Field(f)) => {
                if self.typing != Some(f) {
                    self.set_typing(Some(f));
                }
            }
            Input::Caret(f, at) => {
                if self.typing == Some(f) {
                    self.caret = Caret { at, anchor: at };
                }
            }
            Input::Click(Target::Latency) => {
                self.set_typing(None);
                self.focus = Some(Focus::Latency);
                self.combo = Some(ComboList::open(0, LATENCIES.len(), self.filters.latency));
            }
            Input::Click(Target::Check(c)) => {
                self.set_typing(None);
                self.focus = Some(Focus::Check(c));
                self.toggle(c);
            }
            Input::Click(Target::ComboItem(_) | Target::Outside) | Input::Hover(_) => {}
            Input::ColumnWidth(column, w) => {
                let w = w.max(16.0).round();
                self.widths.retain(|(c, _)| *c != column);
                self.widths.push((column, w));
            }
            Input::Tab(dir) => self.tab_focus(dir),
            Input::Space => match (self.typing, self.focus) {
                (Some(_), _) => self.edit(Edit::Insert(" ".into())),
                (None, Some(Focus::Latency)) => {
                    self.combo = Some(ComboList::open(0, LATENCIES.len(), self.filters.latency));
                }
                (None, Some(Focus::Check(c))) => self.toggle(c),
                (None, Some(Focus::Button(b))) => self.button(b, &mut out),
                _ => {}
            },
            Input::Up | Input::Down if self.focus == Some(Focus::Latency) => {
                let dir = if input == Input::Up { -1 } else { 1 };
                self.filters.latency = widgets::combo_step(self.filters.latency, LATENCIES.len(), dir);
            }
            Input::Up | Input::Down => {
                self.set_typing(None);
                self.focus = Some(Focus::List);
                let n = self.rows.len();
                if n > 0 {
                    let at = self.selected.and_then(|a| self.rows.iter().position(|r| r.addr == a));
                    let next = match (at, input == Input::Down) {
                        (None, _) => 0,
                        (Some(i), true) => (i + 1).min(n - 1),
                        (Some(i), false) => i.saturating_sub(1),
                    };
                    self.selected = Some(self.rows[next].addr);
                    let shown = self.shown_rows();
                    if next < self.scroll {
                        self.scroll = next;
                    } else if next >= self.scroll + shown {
                        self.scroll = next + 1 - shown;
                    }
                }
            }
            Input::Enter => {
                if self.typing.is_some() {
                    self.set_typing(None);
                } else if let Some(Focus::Button(b)) = self.focus {
                    self.button(b, &mut out);
                } else if let Some(addr) = self.selected_row().map(|r| r.addr) {
                    // Connect, the default button.
                    self.join(addr, &mut out);
                }
            }
            Input::Escape => {
                if self.typing.is_some() {
                    self.set_typing(None);
                } else {
                    self.open = false;
                }
            }
            Input::Delete if self.typing.is_some() => self.edit(Edit::Delete),
            Input::Delete => {
                // Ours: Delete takes the selected server off Favorites or
                // History (CS:S: its right-click menu).
                if let Some(addr) = self.selected {
                    let list = match self.tab {
                        Tab::Favorites => Some(&mut self.favorites),
                        Tab::History => Some(&mut self.history),
                        _ => None,
                    };
                    if let Some(list) = list {
                        list.retain(|s| s.addr != addr);
                        self.rows.retain(|r| r.addr != addr);
                        self.total = self.total.saturating_sub(1);
                        self.selected = None;
                        out.save = true;
                    }
                }
            }
            Input::Backspace => self.edit(Edit::Backspace),
            Input::Type(t) => self.edit(Edit::Insert(t)),
            Input::Edit(e) => self.edit(e),
            Input::Copy | Input::Cut => self.copy(input == Input::Cut, &mut out),
        }
        out
    }

    /// A check box ticked or not.
    fn toggle(&mut self, c: Check) {
        let f = &mut self.filters;
        match c {
            Check::NotEmpty => f.not_empty = !f.not_empty,
            Check::NotFull => f.not_full = !f.not_full,
            Check::NoPassword => f.no_password = !f.no_password,
        }
    }

    /// The latency list open: its keys, the pointer on it, the wheel; a
    /// click elsewhere closes it.
    fn handle_combo(&mut self, input: Input) {
        let Some(mut list) = self.combo else { return };
        let event = match input {
            Input::Up => list.key(ComboKey::Up),
            Input::Down => list.key(ComboKey::Down),
            Input::Enter | Input::Space => list.key(ComboKey::Enter),
            Input::Escape => list.key(ComboKey::Escape),
            Input::Hover(Target::ComboItem(k)) => {
                list.hover(k);
                ComboEvent::Open
            }
            Input::Click(Target::ComboItem(k)) => ComboEvent::Pick(k),
            Input::Click(_) | Input::Tab(_) => ComboEvent::Close,
            Input::Scroll(n) => {
                list.wheel(n);
                ComboEvent::Open
            }
            _ => ComboEvent::Open,
        };
        match event {
            ComboEvent::Open => self.combo = Some(list),
            ComboEvent::Close => self.combo = None,
            ComboEvent::Pick(k) => {
                self.combo = None;
                self.filters.latency = k.min(LATENCIES.len() - 1);
            }
        }
    }

    /// An edit to the text entry typed into.
    fn edit(&mut self, edit: Edit) {
        let mut caret = self.caret;
        if let Some(f) = self.field_mut() {
            caret.edit(f, edit, 128);
        }
        self.caret = caret;
    }

    /// Copy (or cut) the typed text's selection.
    fn copy(&mut self, cut: bool, out: &mut Outcome) {
        // A password never leaves its box.
        if self.typing == Some(Field::Password) {
            return;
        }
        let caret = self.caret;
        let Some(text) = self.field_mut().map(|t| caret.selected(t).to_string()) else {
            return;
        };
        if !text.is_empty() {
            out.clipboard = Some(text);
            if cut {
                self.edit(Edit::Delete);
            }
        }
    }
    fn field_mut(&mut self) -> Option<&mut String> {
        match self.typing? {
            Field::MapFilter => Some(&mut self.filters.map),
            Field::MaxPlayers => Some(&mut self.filters.max_players),
            Field::AddAddress => match &mut self.dialog {
                Dialog::AddServer { text, .. } => Some(text),
                _ => None,
            },
            Field::Password => match &mut self.dialog {
                Dialog::Password { entry, .. } => Some(&mut entry.input),
                _ => None,
            },
        }
    }

    fn button(&mut self, b: ButtonId, out: &mut Outcome) {
        match b {
            ButtonId::Connect => {
                if let Some(addr) = self.selected_row().map(|r| r.addr) {
                    self.join(addr, out);
                }
            }
            ButtonId::Refresh => self.refresh(out),
            ButtonId::QuickRefresh => {
                let addrs: Vec<SocketAddr> = self.rows.iter().map(|r| r.addr).collect();
                if !addrs.is_empty() {
                    out.requests.push(Request::Query(addrs));
                    self.refreshing = true;
                }
            }
            ButtonId::AddServer => {
                self.dialog = Dialog::AddServer {
                    text: String::new(),
                    looked_up: None,
                    selected: false,
                    error: None,
                };
                self.found = None;
                self.set_typing(Some(Field::AddAddress));
            }
            ButtonId::Filters => {
                self.filters.shown = !self.filters.shown;
                self.typing = None;
            }
            ButtonId::Close => self.open = false,
            _ => {}
        }
    }

    fn handle_dialog(&mut self, input: Input, out: &mut Outcome) {
        let close = |b: &mut Self| {
            b.dialog = Dialog::None;
            b.typing = None;
            b.found = None;
        };
        match input {
            Input::Escape
            | Input::Click(Target::Button(ButtonId::AddCancel))
            | Input::Click(Target::Button(ButtonId::PasswordCancel))
            | Input::Click(Target::Button(ButtonId::Close)) => close(self),
            Input::Type(t) => self.edit(Edit::Insert(t)),
            Input::Backspace => self.edit(Edit::Backspace),
            Input::Delete => self.edit(Edit::Delete),
            Input::Edit(e) => self.edit(e),
            Input::Space => self.edit(Edit::Insert(" ".into())),
            Input::Copy | Input::Cut => self.copy(input == Input::Cut, out),
            Input::Click(Target::Field(f)) => {
                if self.typing != Some(f) {
                    self.set_typing(Some(f));
                }
            }
            Input::Caret(f, at) => {
                if self.typing == Some(f) {
                    self.caret = Caret { at, anchor: at };
                }
            }
            Input::Enter | Input::Click(Target::Button(ButtonId::AddOk | ButtonId::PasswordConnect)) => {
                match self.dialog.clone() {
                    Dialog::AddServer { text, .. } => match crate::net::parse_address(&text) {
                        Ok(addr) => {
                            self.add_favorite(addr, "");
                            out.save = true;
                            close(self);
                            if self.tab == Tab::Favorites {
                                self.refresh(out);
                            }
                        }
                        Err(_) => {
                            let e = self.ui_error();
                            if let Dialog::AddServer { error, .. } = &mut self.dialog {
                                *error = Some(e);
                            }
                        }
                    },
                    Dialog::Password { addr, mut entry, .. } => {
                        if let Some(dialog::Outcome::Submit(password)) = entry.handle(dialog::Press::Enter) {
                            self.connect(addr, Some(&password), out);
                        }
                    }
                    Dialog::None => {}
                }
            }
            Input::Click(Target::Button(ButtonId::AddFind)) => {
                if let Dialog::AddServer {
                    text, looked_up, error, ..
                } = &mut self.dialog
                {
                    match crate::net::parse_address(text) {
                        Ok(addr) => {
                            *looked_up = Some(addr);
                            *error = None;
                            out.requests.push(Request::Query(vec![addr]));
                        }
                        Err(_) => *error = Some(String::new()),
                    }
                }
                if matches!(&self.dialog, Dialog::AddServer { error: Some(e), .. } if e.is_empty()) {
                    let e = self.ui_error();
                    if let Dialog::AddServer { error, .. } = &mut self.dialog {
                        *error = Some(e);
                    }
                }
            }
            Input::Click(Target::Row(0)) => {
                if let Dialog::AddServer { selected, .. } = &mut self.dialog {
                    *selected = self.found.is_some();
                }
            }
            // Ours: double-click the server found to join it (a connect
            // to an address).
            Input::DoubleClick(0) => {
                if let Some(addr) = self.found.as_ref().map(|r| r.addr) {
                    self.join(addr, out);
                }
            }
            Input::Click(Target::Button(ButtonId::AddSelected)) => {
                if let (Dialog::AddServer { selected: true, .. }, Some(row)) = (&self.dialog, self.found.clone()) {
                    self.add_favorite(row.addr, &row.name);
                    out.save = true;
                    close(self);
                    if self.tab == Tab::Favorites {
                        self.refresh(out);
                    }
                }
            }
            _ => {}
        }
    }

    fn ui_error(&self) -> String {
        self.text(
            "#ServerBrowser_AddServerError",
            "The server IP address you entered is invalid.",
        )
    }

    // --- Saving.

    /// Favourites and history as a KeyValues file (`serverbrowser.vdf`).
    pub fn saved_text(&self) -> String {
        let mut out = String::from("\"ServerBrowser\"\n{\n");
        for (name, list) in [("Favorites", &self.favorites), ("History", &self.history)] {
            out += &format!("\t\"{name}\"\n\t{{\n");
            for (i, s) in list.iter().enumerate() {
                out += &format!(
                    "\t\t\"{}\"\n\t\t{{\n\t\t\t\"name\"\t\t\"{}\"\n\t\t\t\"address\"\t\t\"{}\"\n\t\t\t\"lastplayed\"\t\t\"{}\"\n\t\t}}\n",
                    i + 1,
                    escape(&s.name),
                    s.addr,
                    s.last_played
                );
            }
            out += "\t}\n";
        }
        out += "}\n";
        out
    }

    /// Read favourites and history from `saved_text`'s format (what it
    /// can; the rest is left out).
    pub fn load_text(&mut self, text: &str) {
        let tokens = crate::mount::keyvalues::tokens(text, crate::mount::keyvalues::Escapes::On);
        let mut i = 0;
        let mut path: Vec<String> = Vec::new();
        let mut current: Vec<(String, String)> = Vec::new();
        let (mut fav, mut hist) = (Vec::new(), Vec::new());
        let mut pending_key: Option<String> = None;
        while i < tokens.len() {
            let t = &tokens[i];
            i += 1;
            match t.as_str() {
                "{" => {
                    path.push(pending_key.take().unwrap_or_default());
                    current.clear();
                }
                "}" => {
                    if path.len() == 3 {
                        let get = |k: &str| {
                            current
                                .iter()
                                .find(|(a, _)| a.eq_ignore_ascii_case(k))
                                .map(|(_, v)| v.clone())
                        };
                        if let Some(addr) = get("address").and_then(|a| a.parse::<SocketAddr>().ok()) {
                            let s = SavedServer {
                                addr,
                                name: get("name").unwrap_or_default(),
                                last_played: get("lastplayed").and_then(|v| v.parse().ok()).unwrap_or(0),
                            };
                            match path[1].to_lowercase().as_str() {
                                "favorites" => fav.push(s),
                                "history" => hist.push(s),
                                _ => {}
                            }
                        }
                    }
                    path.pop();
                }
                _ => {
                    if let Some(key) = pending_key.take() {
                        current.push((key, t.clone()));
                    } else {
                        pending_key = Some(t.clone());
                    }
                }
            }
        }
        fav.truncate(MAX_SAVED);
        hist.truncate(MAX_SAVED);
        self.favorites = fav;
        self.history = hist;
    }

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.saved_text())
    }

    pub fn load_from(&mut self, path: &Path) {
        if let Ok(text) = std::fs::read_to_string(path) {
            self.load_text(&text);
        }
    }
}

