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
// The client systems.

pub(super) fn load_saved(mut browser: ResMut<ServerBrowser>) {
    if let Some(path) = saved_path() {
        browser.load_from(&path);
    }
}

pub(super) fn save(browser: &ServerBrowser) {
    if let Some(path) = saved_path()
        && let Err(e) = browser.save_to(&path)
    {
        warn!("server browser: saving {}: {e}", path.display());
    }
}

/// A server joined goes first in the history.
pub(super) fn record_history(
    mut events: MessageReader<crate::net::NetEvent>,
    addr: Option<Res<crate::net::client::ServerAddress>>,
    queries: Res<ServerQueries>,
    mut browser: ResMut<ServerBrowser>,
) {
    for e in events.read() {
        if let crate::net::NetEvent::Joined { .. } = e
            && let Some(addr) = &addr
        {
            let name = match queries.result(addr.0).map(|r| &r.state) {
                Some(QueryState::Answered { info, .. }) => info.name.clone(),
                _ => String::new(),
            };
            browser.played(addr.0, &name, unix_now());
            save(&browser);
        }
    }
}

/// The open tab's rows follow the queries; the game's words come with
/// the menu's.
pub(super) fn sync_results(
    queries: Res<ServerQueries>,
    menu: Res<GameMenu>,
    mut browser: ResMut<ServerBrowser>,
    mut seen: Local<Option<u64>>,
) {
    if browser.ui != menu.ui {
        browser.ui = menu.ui.clone();
    }
    if !browser.open {
        *seen = None;
        return;
    }
    let busy = queries.busy();
    if *seen != Some(queries.generation) || browser.refreshing != busy || browser.is_changed() {
        *seen = Some(queries.generation);
        let mut next = browser.clone();
        next.set_results(&queries.results, busy);
        if next != *browser {
            *browser = next;
        }
    }
}

pub(super) fn apply(input: Input, browser: &mut ResMut<ServerBrowser>, console: &mut Console, inbox: &mut RequestInbox) {
    let mut next = (**browser).clone();
    let out = next.handle(input);
    if next != **browser {
        **browser = next;
    }
    for line in out.lines {
        console.submit(line);
    }
    inbox.0.extend(out.requests);
    if out.save {
        save(browser);
    }
    if let Some(text) = out.clipboard
        && let Err(e) = arboard::Clipboard::new().and_then(|mut c| c.set_text(text))
    {
        warn!("clipboard: {e}");
    }
}

/// Queries the browser asked for.
pub(super) fn run_requests(mut inbox: ResMut<RequestInbox>, lan: Res<LanSettings>, mut queries: ResMut<ServerQueries>) {
    for r in inbox.0.drain(..) {
        match r {
            Request::ScanLan => queries.scan_lan(lan.port_range(), &lan.broadcast_addresses()),
            Request::Query(addrs) => {
                for a in addrs {
                    queries.query(a);
                }
            }
        }
    }
}

/// Requests from input systems, run by `run_requests`.
#[derive(Resource, Default)]
pub(super) struct RequestInbox(pub(super) Vec<Request>);

/// Whether the browser takes the keys: open, in front of the menu's
/// dialog (when one is open too), no first-run dialog over it.
pub(super) fn has_keys(
    browser: &ServerBrowser,
    menu: Option<&GameMenu>,
    windows: Option<&Windows>,
    first_run: Option<&crate::ui::first_run::FirstRun>,
) -> bool {
    if !browser.open || first_run.is_some_and(|f| f.open) {
        return false;
    }
    match menu.filter(|m| m.open && m.page != crate::ui::game_menu::Page::Main) {
        Some(m) => windows.is_none_or(|w| w.front(&[WINDOW, m.page.window()]) == Some(WINDOW)),
        None => true,
    }
}

/// Keys: typing and editing in the focused box (Shift selects, Ctrl+A,
/// Ctrl+C, Ctrl+X, Ctrl+V), Tab, Space, arrows, Enter, Esc, Delete, F5.
#[allow(clippy::too_many_arguments)]
pub(super) fn keys(
    mut events: MessageReader<KeyboardInput>,
    held: Res<ButtonInput<KeyCode>>,
    console_ui: Res<crate::client::console::ConsoleUi>,
    mut browser: ResMut<ServerBrowser>,
    mut console: ResMut<Console>,
    mut inbox: ResMut<RequestInbox>,
    (menu, windows, first_run): (
        Option<Res<GameMenu>>,
        Option<Res<Windows>>,
        Option<Res<crate::ui::first_run::FirstRun>>,
    ),
) {
    if console_ui.open || !has_keys(&browser, menu.as_deref(), windows.as_deref(), first_run.as_deref()) {
        events.clear();
        return;
    }
    let shift = held.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let ctrl = held.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]);
    for e in events.read() {
        if e.state != ButtonState::Pressed {
            continue;
        }
        let typing = browser.typing.is_some();
        let input = match (&e.logical_key, e.key_code) {
            (_, KeyCode::Enter | KeyCode::NumpadEnter) => Input::Enter,
            (_, KeyCode::Escape) => Input::Escape,
            (_, KeyCode::Tab) => Input::Tab(if shift { -1 } else { 1 }),
            (_, KeyCode::ArrowUp) => Input::Up,
            (_, KeyCode::ArrowDown) => Input::Down,
            (_, KeyCode::ArrowLeft) if typing => Input::Edit(Edit::Left(shift)),
            (_, KeyCode::ArrowRight) if typing => Input::Edit(Edit::Right(shift)),
            (_, KeyCode::Home) if typing => Input::Edit(Edit::Home(shift)),
            (_, KeyCode::End) if typing => Input::Edit(Edit::End(shift)),
            (_, KeyCode::KeyA) if typing && ctrl => Input::Edit(Edit::SelectAll),
            (_, KeyCode::KeyC) if typing && ctrl => Input::Copy,
            (_, KeyCode::KeyX) if typing && ctrl => Input::Cut,
            (_, KeyCode::KeyV) if typing && ctrl => {
                let text = arboard::Clipboard::new()
                    .and_then(|mut c| c.get_text())
                    .unwrap_or_default();
                Input::Type(text.lines().next().unwrap_or("").to_string())
            }
            (_, KeyCode::PageUp) => Input::Scroll(-(browser.shown_rows() as i32)),
            (_, KeyCode::PageDown) => Input::Scroll(browser.shown_rows() as i32),
            (_, KeyCode::Delete) => Input::Delete,
            (_, KeyCode::F5) => Input::Click(Target::Button(ButtonId::Refresh)),
            (Key::Backspace, _) => Input::Backspace,
            (Key::Space, _) => Input::Space,
            (Key::Character(s), _) if typing && !ctrl => Input::Type(s.to_string()),
            _ => continue,
        };
        apply(input, &mut browser, &mut console, &mut inbox);
    }
}

/// What a UI node stands for.
#[derive(Component, Clone, Copy)]
pub(super) struct Hit(pub(super) Target);

/// The list (the wheel scrolls it).
#[derive(Component)]
pub(super) struct WheelList;

/// A text entry: a click places its caret at the char under it.
#[derive(Component, Clone)]
pub(super) struct TextHit {
    pub(super) field: Field,
    pub(super) shown: String,
    pub(super) width: f32,
}

/// A column header's right edge: dragging it sizes the column.
#[derive(Component, Clone, Copy)]
pub(super) struct ColumnGrip {
    pub(super) column: Column,
    pub(super) width: f32,
}

/// Clicks (a second click on a row soon after the first joins it), the
/// pointer over an open list, a click into a text entry, a column's
/// edge dragged, the wheel over the list (or the open list).
#[allow(clippy::too_many_arguments)]
pub(super) fn pointer(
    hits: Query<(&Interaction, &Hit), Changed<Interaction>>,
    texts: Query<(&Interaction, &TextHit, &RelativeCursorPosition), Changed<Interaction>>,
    grips: Query<(&Interaction, &ColumnGrip), Changed<Interaction>>,
    lists: Query<&RelativeCursorPosition, With<WheelList>>,
    scroll: Option<Res<AccumulatedMouseScroll>>,
    thumb: Option<Res<widgets::ScrollDrag>>,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Query<&Window, With<bevy::window::PrimaryWindow>>,
    fonts: Res<UiFonts>,
    time: Res<Time<Real>>,
    mut browser: ResMut<ServerBrowser>,
    mut console: ResMut<Console>,
    mut inbox: ResMut<RequestInbox>,
    mut last_click: Local<Option<(usize, f64)>>,
    mut sizing: Local<Option<(Column, f32, f32)>>,
) {
    if !browser.open {
        *sizing = None;
        return;
    }
    let win = window.iter().next();
    let height = win.map_or(720.0, Window::height);
    let scale = (height / 720.0).clamp(0.6, 3.0);
    let now = time.elapsed_secs_f64();
    let mut inputs = Vec::new();
    for (i, hit) in &hits {
        match i {
            Interaction::Hovered => inputs.push(Input::Hover(hit.0)),
            Interaction::Pressed => {
                if let Target::Row(r) = hit.0 {
                    if last_click.is_some_and(|(at, t)| at == r && now - t < 0.4) {
                        inputs.push(Input::DoubleClick(r));
                        *last_click = None;
                        continue;
                    }
                    *last_click = Some((r, now));
                }
                inputs.push(Input::Click(hit.0));
            }
            Interaction::None => {}
        }
    }
    for (i, hit, at) in &texts {
        if *i == Interaction::Pressed
            && let Some(p) = at.normalized
        {
            let font = fonts.source("Default", height, scale, (16.0, false));
            let caret = (browser.typing == Some(hit.field)).then_some(browser.caret);
            let at = widgets::caret_from_click(&fonts, &font, scale, &hit.shown, hit.width, p.x + 0.5, caret);
            inputs.push(Input::Caret(hit.field, at));
        }
    }
    // A column's edge: held, it follows the pointer.
    let x = win.and_then(Window::cursor_position).map(|p| p.x);
    for (i, grip) in &grips {
        if *i == Interaction::Pressed
            && let Some(x) = x
        {
            *sizing = Some((grip.column, grip.width, x));
        }
    }
    if let Some((column, width, from)) = *sizing {
        if !mouse.pressed(MouseButton::Left) {
            *sizing = None;
        } else if let Some(x) = x {
            let w = (width + (x - from) / scale).round();
            if browser.column_width(column, -1.0) != w {
                inputs.push(Input::ColumnWidth(column, w));
            }
        }
    }
    if let Some(scroll) = scroll
        && scroll.delta.y != 0.0
    {
        let notches = widgets::wheel_notches(&scroll);
        if browser.combo.is_some() {
            inputs.push(Input::Scroll(-notches));
        } else if lists.iter().any(|l| l.cursor_over()) {
            inputs.push(Input::Scroll(-notches * 3));
        }
    }
    // A scroll bar's thumb dragged: the list (or the open drop-down's)
    // there, wherever the selection is.
    if let Some(drag) = thumb.filter(|d| d.is_changed())
        && let Some((owner, _, first)) = drag.at
    {
        let now = match (owner, browser.combo) {
            (widgets::COMBO_SCROLL, Some(c)) => Some(c.first),
            (BROWSER_SCROLL, None) => Some(browser.scroll),
            _ => None,
        };
        if let Some(now) = now.filter(|n| *n != first) {
            inputs.push(Input::Scroll(first as i32 - now as i32));
        }
    }
    for input in inputs {
        apply(input, &mut browser, &mut console, &mut inbox);
    }
}

/// The frame made bigger: the list grows with it.
pub(super) fn follow_size(windows: Res<Windows>, mut browser: ResMut<ServerBrowser>, mut seen: Local<u64>) {
    if windows.resized == *seen {
        return;
    }
    *seen = windows.resized;
    let grow = windows
        .size(WINDOW)
        .map_or(Vec2::ZERO, |s| (s - MIN_SIZE).max(Vec2::ZERO));
    if browser.grow != grow {
        browser.grow = grow;
    }
}

