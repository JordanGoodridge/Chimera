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
// Drawing.

#[derive(Component)]
pub(super) struct BrowserRoot;

/// The column icons as images (from `GameUi::server_icons`).
#[derive(Resource, Default)]
pub(super) struct BrowserIcons {
    from: Option<Arc<GameUi>>,
    icons: std::collections::HashMap<String, Handle<Image>>,
}

/// How a control follows its parent's corners as the frame grows
/// (VGUI's `pinCorner`: 0 top left, 1 top right, 2 bottom left, 3 bottom
/// right; `autoResize`: 0 none, 1 right, 2 down, 3 both).
pub(super) fn grown(rect: (f32, f32, f32, f32), pin: u8, resize: u8, grow: Vec2) -> (f32, f32, f32, f32) {
    let (mut x, mut y, mut w, mut h) = rect;
    if pin == 1 || pin == 3 {
        x += grow.x;
    }
    if pin >= 2 {
        y += grow.y;
    }
    if resize & 1 != 0 {
        w += grow.x;
    }
    if resize & 2 != 0 {
        h += grow.y;
    }
    (x, y, w, h)
}

/// A layout's control box (x, y, wide, tall) by name, else `fallback`;
/// moved and sized as the frame grew (its `pinCorner` and `autoResize`,
/// else `fallback_pin`'s).
pub(super) fn rect_in(
    layout: Option<&UiLayout>,
    name: &str,
    fallback: (f32, f32, f32, f32),
    fallback_pin: (u8, u8),
    grow: Vec2,
) -> (f32, f32, f32, f32) {
    let c = layout.and_then(|l| l.get(name));
    let num = |key: &str, or: u8| {
        c.and_then(|c| c.keys.get(key))
            .and_then(|v| v.trim().parse::<u8>().ok())
            .unwrap_or(if c.is_some() { 0 } else { or })
    };
    let rect = rect_of(layout, name, fallback);
    grown(
        rect,
        num("pincorner", fallback_pin.0),
        num("autoresize", fallback_pin.1),
        grow,
    )
}

/// A layout's control box (x, y, wide, tall) by name, else `fallback`.
pub(super) fn rect_of(layout: Option<&UiLayout>, name: &str, fallback: (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
    layout.and_then(|l| l.get(name)).map_or(fallback, |c| {
        let v = |h: crate::map::hud::HudCoord| match h {
            crate::map::hud::HudCoord::Start(v)
            | crate::map::hud::HudCoord::Centre(v)
            | crate::map::hud::HudCoord::End(v) => v,
        };
        (v(c.x), v(c.y), c.wide, c.tall)
    })
}

pub(super) fn control<'a>(layout: Option<&'a UiLayout>, name: &str) -> Option<&'a UiControl> {
    layout.and_then(|l| l.get(name))
}

/// A control's text from the layout, else the token's, else ours.
pub(super) fn text_of(b: &ServerBrowser, layout: Option<&UiLayout>, name: &str, token: &str, ours: &str) -> String {
    control(layout, name)
        .map(|c| c.text.clone())
        .filter(|t| !t.is_empty() && !t.starts_with('#'))
        .unwrap_or_else(|| b.text(token, ours))
}

/// A VGUI button that reports `target` (focused: ringed; `default`: the
/// dialog's Enter button; `latched`: a toggle button held down).
#[allow(clippy::too_many_arguments)]
pub(super) fn button(
    commands: &mut Commands,
    parent: Entity,
    look: &Look,
    rect: (f32, f32, f32, f32),
    text: &str,
    target: Target,
    b: &ServerBrowser,
    (enabled, default, latched): (bool, bool, bool),
) {
    let focused = matches!(target, Target::Button(id) if b.focus == Some(Focus::Button(id)));
    let state = widgets::ButtonState::enabled(enabled)
        .focused(focused)
        .default_button(default)
        .latched(latched);
    widgets::button(commands, parent, look, rect, text, state, -1, Hit(target));
}

/// A text entry that takes typing, with its caret when typed into.
#[allow(clippy::too_many_arguments)]
pub(super) fn text_box(
    commands: &mut Commands,
    parent: Entity,
    look: &Look,
    rect: (f32, f32, f32, f32),
    text: &str,
    field: Field,
    b: &ServerBrowser,
    hidden: bool,
) {
    let caret = (b.typing == Some(field)).then_some(b.caret);
    let shown = if hidden {
        "*".repeat(text.chars().count())
    } else {
        text.to_string()
    };
    widgets::text_entry(
        commands,
        parent,
        look,
        rect,
        text,
        caret,
        hidden,
        true,
        (
            Hit(Target::Field(field)),
            TextHit {
                field,
                shown,
                width: rect.2,
            },
        ),
    );
}

pub(super) fn icon(
    commands: &mut Commands,
    parent: Entity,
    look: &Look,
    (x, y, w, h): (f32, f32, f32, f32),
    image: Option<&Handle<Image>>,
    fallback: &str,
    color: Color,
) {
    match image {
        Some(image) => {
            commands.spawn((
                place(
                    look,
                    x + (w - 16.0).max(0.0) / 2.0,
                    y + (h - 16.0).max(0.0) / 2.0,
                    w.min(16.0),
                    h.min(16.0),
                ),
                ImageNode::new(image.clone()),
                ChildOf(parent),
            ));
        }
        None => {
            label(
                commands,
                parent,
                look,
                (x, y, w, h),
                fallback,
                look.font("DefaultSmall", (12.0, false)),
                color,
                0,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw(
    browser: Res<ServerBrowser>,
    fonts: Res<UiFonts>,
    hud: Option<Res<crate::map::hud::ActiveHud>>,
    shown: Query<Entity, With<BrowserRoot>>,
    windows_q: Query<&Window>,
    mut windows: ResMut<Windows>,
    mut icons: ResMut<BrowserIcons>,
    mut images: ResMut<Assets<Image>>,
    mut last: Local<(Vec2, bool, bool)>,
    mut last_minute: Local<u64>,
    mut commands: Commands,
) {
    let size = windows_q
        .iter()
        .next()
        .map_or(Vec2::new(640.0, 480.0), |w| Vec2::new(w.width(), w.height()));
    let resized = (size - last.0).abs().max_element() > 0.5;
    // History's "min ago" moves on.
    let minute = unix_now() / 60;
    let ticked = browser.open && browser.tab == Tab::History && minute != *last_minute;
    if !browser.is_changed() && !resized && !ticked {
        return;
    }
    // Opened, or a dialog opened: in front.
    let dialog = browser.dialog != Dialog::None;
    if browser.open && !last.1 {
        windows.raise(WINDOW);
    }
    if dialog && !last.2 {
        windows.raise(match browser.dialog {
            Dialog::AddServer { .. } => ADD_WINDOW,
            _ => PASSWORD_WINDOW,
        });
    }
    *last = (size, browser.open, dialog);
    *last_minute = minute;
    for e in &shown {
        commands.entity(e).despawn();
    }
    if !browser.open {
        return;
    }
    // The icons, once the game's look is in.
    let ui = browser.ui.0.clone();
    if let Some(ui) = &ui
        && !icons.from.as_ref().is_some_and(|f| Arc::ptr_eq(f, ui))
    {
        icons.icons = ui
            .server_icons
            .iter()
            .map(|(k, pic)| (k.clone(), crate::ui::game_menu::ui_image(pic, &mut images)))
            .collect();
        icons.from = Some(ui.clone());
    }
    let accent = hud
        .as_ref()
        .and_then(|h| h.0.color("FgColor"))
        .map(|c| c.with_alpha(1.0))
        .unwrap_or(Color::srgb_u8(255, 176, 0));
    let look = Look {
        ui: ui.as_deref(),
        fonts: &fonts,
        s: (size.y / 720.0).clamp(0.6, 3.0),
        height: size.y,
        accent,
    };
    let root = commands
        .spawn((
            BrowserRoot,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                height: percent(100.0),
                ..default()
            },
            // The frames inside stack with the menu's (`widgets`); the
            // root itself catches nothing.
            bevy::ui::FocusPolicy::Pass,
            GlobalZIndex(crate::ui::game_menu::MENU_Z + 1),
        ))
        .id();
    let layouts = ui.as_ref().map(|u| &u.servers);
    let dialog = layouts.and_then(|l| l.get("dialog"));
    let page = layouts.and_then(|l| l.get(if browser.filters.shown { "page_filters" } else { "page" }));
    let grow = browser.grow;
    // The frame: the file's size, wide enough for its tabs; bigger as
    // the user made it.
    let (_, _, fw, fh) = rect_of(dialog, "CServerBrowserDialog", (0.0, 0.0, 640.0, 384.0));
    let (tx, ty, tw, th) = rect_in(dialog, "GameTabs", (8.0, 44.0, 624.0, 306.0), (0, 3), grow);
    let (w, h) = (
        fw.max(tx + tw - grow.x + 8.0) + grow.x,
        fh.max(ty + th - grow.y + 34.0) + grow.y,
    );
    let title = browser.text("#ServerBrowser_Servers", "Servers");
    let parts = widgets::frame(
        &mut commands,
        root,
        &look,
        FrameSpec::new(WINDOW).sizeable(MIN_SIZE),
        (w, h),
        &title,
    );
    let f = parts.frame;
    if let Some(close) = parts.close {
        commands.entity(close).insert(Hit(Target::Button(ButtonId::Close)));
    }
    // The tabs over the page.
    let names: Vec<(String, bool)> = TABS
        .iter()
        .map(|(t, token, ours)| (browser.text(token, ours), t.enabled()))
        .collect();
    let open = TABS.iter().position(|t| t.0 == browser.tab).unwrap_or(0);
    widgets::tabs(&mut commands, f, &look, (tx, ty), &names, open, None, None, |i| {
        Hit(Target::Tab(TABS[i].0))
    });
    // The page: its box, then the list and buttons inside it.
    let (_, py, _, ph) = rect_of(page, "InternetGames", (0.0, 28.0, 624.0, 278.0));
    let page_e = commands
        .spawn((
            Node {
                border: UiRect::all(px(1.0)),
                ..place(&look, tx, ty + py, tw, ph + grow.y)
            },
            bevel(&look, true),
            BackgroundColor(Color::NONE),
            ChildOf(f),
        ))
        .id();
    let list_rect = rect_in(
        page,
        "gamelist",
        (0.0, 8.0, 624.0, if browser.filters.shown { 134.0 } else { 226.0 }),
        (0, 3),
        grow,
    );
    server_list(&mut commands, page_e, &look, &browser, &icons, list_rect);
    page_buttons(&mut commands, page_e, &look, &browser, page);
    if browser.filters.shown {
        filter_controls(&mut commands, page_e, root, &look, &browser, page);
    }
    // The status line.
    let status = rect_in(dialog, "StatusLabel", (11.0, 356.0, 544.0, 24.0), (2, 1), grow);
    label(
        &mut commands,
        f,
        &look,
        status,
        &browser.status(),
        look.default_font(),
        look.text(),
        -1,
    );
    match &browser.dialog {
        Dialog::None => {}
        Dialog::AddServer { .. } => add_server_dialog(&mut commands, root, &look, &browser, &icons),
        Dialog::Password { .. } => password_dialog(&mut commands, root, &look, &browser),
    }
}

/// The list: column headers (sorting, their edges sizing them), rows, a
/// scroll bar, the empty list's line.
pub(super) fn server_list(
    commands: &mut Commands,
    parent: Entity,
    look: &Look,
    b: &ServerBrowser,
    icons: &BrowserIcons,
    (lx, ly, lw, lh): (f32, f32, f32, f32),
) {
    let list = commands
        .spawn((
            Node {
                border: UiRect::all(px(1.0)),
                overflow: Overflow::clip(),
                ..place(look, lx, ly, lw, lh)
            },
            bevel(look, false),
            BackgroundColor(look.color("ListPanel.BgColor", [0, 0, 0, 128])),
            RelativeCursorPosition::default(),
            WheelList,
            ChildOf(parent),
        ))
        .id();
    if b.focus == Some(Focus::List) {
        commands
            .entity(list)
            .insert(Outline::new(px(1.0), px(0.0), look.dark()));
    }
    let bar_w = look.number("ScrollBar.Wide", 17.0);
    let inner = lw - 2.0 - bar_w;
    let columns: Vec<(Column, String, f32)> = b
        .columns()
        .into_iter()
        .map(|(c, t, w)| (c, t, b.column_width(c, w)))
        .collect();
    let fixed: f32 = columns.iter().map(|c| c.2).sum();
    let widths: Vec<f32> = columns
        .iter()
        .map(|c| if c.2 == 0.0 { (inner - fixed).max(60.0) } else { c.2 })
        .collect();
    let font = look.default_font();
    let small = look.font("DefaultSmall", (13.0, false));
    // Headers.
    let mut x = 0.0;
    for ((column, text, _), cw) in columns.iter().zip(&widths) {
        let e = commands
            .spawn((
                Node {
                    border: UiRect::all(px(1.0)),
                    ..place(look, x, 0.0, *cw, HEADER_H)
                },
                bevel(look, true),
                BackgroundColor(look.color("Button.BgColor", [0, 0, 0, 0])),
                Hit(Target::Header(*column)),
                widgets::clickable(),
                ChildOf(list),
            ))
            .id();
        match column {
            Column::Password => icon(
                commands,
                e,
                look,
                (0.0, 0.0, cw - 2.0, HEADER_H - 2.0),
                icons.icons.get("password_column"),
                "P",
                look.text(),
            ),
            Column::Bots => icon(
                commands,
                e,
                look,
                (0.0, 0.0, cw - 2.0, HEADER_H - 2.0),
                icons.icons.get("bots_column"),
                "B",
                look.text(),
            ),
            _ => {
                label(
                    commands,
                    e,
                    look,
                    (4.0, 0.0, cw - 20.0, HEADER_H - 2.0),
                    text,
                    small.clone(),
                    look.text(),
                    -1,
                );
                // The sort arrow at the header's right.
                if let Some((c, desc)) = b.sort
                    && c == *column
                {
                    let arrow = if desc { "\u{25BC}" } else { "\u{25B2}" };
                    label(
                        commands,
                        e,
                        look,
                        (cw - 16.0, 0.0, 12.0, HEADER_H - 2.0),
                        arrow,
                        look.font("Marlett", (8.0, false)),
                        look.text(),
                        0,
                    );
                }
            }
        }
        // Its right edge sizes it (not the one that takes the rest).
        if !matches!(column, Column::Password | Column::Bots | Column::Name) {
            commands.spawn((
                place(look, x + cw - 3.0, 0.0, 6.0, HEADER_H),
                ColumnGrip {
                    column: *column,
                    width: *cw,
                },
                widgets::clickable(),
                ZIndex(3),
                ChildOf(list),
            ));
        }
        x += cw;
    }
    // Rows.
    let shown = b.shown_rows();
    let now = unix_now();
    for (k, row) in b.rows.iter().enumerate().skip(b.scroll).take(shown) {
        let y = HEADER_H + (k - b.scroll) as f32 * ROW_H;
        let selected = b.selected == Some(row.addr);
        let e = commands
            .spawn((
                place(look, 0.0, y, inner, ROW_H),
                BackgroundColor(if selected {
                    look.color("ListPanel.SelectedBgColor", [255, 155, 0, 255])
                } else {
                    Color::NONE
                }),
                Hit(Target::Row(k)),
                widgets::clickable(),
                ChildOf(list),
            ))
            .id();
        let color = if selected {
            look.color("ListPanel.SelectedTextColor", [0, 0, 0, 255])
        } else if row.responded && !row.other_version {
            look.color("ListPanel.TextColor", [221, 221, 221, 255])
        } else {
            look.dull()
        };
        let mut x = 0.0;
        for ((column, ..), cw) in columns.iter().zip(&widths) {
            let cell = (x + 4.0, 0.0, cw - 8.0, ROW_H);
            let text = match column {
                Column::Password => {
                    if row.password {
                        icon(
                            commands,
                            e,
                            look,
                            (x, 0.0, *cw, ROW_H),
                            icons.icons.get("password"),
                            "*",
                            color,
                        );
                    }
                    None
                }
                Column::Bots => {
                    if row.bots > 0 {
                        icon(
                            commands,
                            e,
                            look,
                            (x, 0.0, *cw, ROW_H),
                            icons.icons.get("bots"),
                            "b",
                            color,
                        );
                    }
                    None
                }
                Column::Name => Some(if row.name.is_empty() && !row.responded {
                    if row.not_responding {
                        format!(
                            "{} {}",
                            row.addr,
                            b.text("#ServerBrowser_NotResponding", "< not responding >")
                        )
                    } else {
                        row.addr.to_string()
                    }
                } else {
                    row.name.clone()
                }),
                Column::Game => Some(row.game.clone()),
                Column::Players => row.responded.then(|| format!("{} / {}", row.players, row.max_players)),
                Column::Map => Some(row.map.clone()),
                Column::Latency => Some(match row.ping {
                    Some(p) => p.to_string(),
                    None if row.not_responding => String::new(),
                    None => b.text("#ServerBrowser_PendingPing", "<pending>"),
                }),
                Column::LastPlayed => Some(last_played_text(row.last_played, now)),
            };
            if let Some(t) = text {
                label(commands, e, look, cell, &t, font.clone(), color, -1);
            }
            x += cw;
        }
    }
    if let Some(t) = b.empty_text() {
        label(
            commands,
            list,
            look,
            (0.0, HEADER_H + 8.0, inner, ROW_H * 2.0),
            &t,
            font.clone(),
            look.color("ListPanel.EmptyListInfoTextColor", [221, 221, 221, 255]),
            0,
        );
    }
    scroll_bar(
        commands,
        list,
        look,
        (inner, 0.0, lh - 2.0),
        (b.rows.len(), shown, b.scroll),
    );
}

/// Whose scroll bar thumb (`widgets::ScrollThumb`): the server list.
pub(super) const BROWSER_SCROLL: widgets::WindowId = "servers";

/// The list's scroll bar (`widgets::scroll_bar`): arrows and the track
/// scroll (`Target::Scroll`), the thumb drags (`BROWSER_SCROLL`).
pub(super) fn scroll_bar(
    commands: &mut Commands,
    parent: Entity,
    look: &Look,
    rect: (f32, f32, f32),
    list: (usize, usize, usize),
) {
    widgets::scroll_bar(commands, parent, look, rect, list, (BROWSER_SCROLL, 0), |step| Hit(Target::Scroll(step)));
}

/// The page's buttons: those the open tab shows, where the file puts
/// them (following the frame's bottom right corner as it grows).
pub(super) fn page_buttons(commands: &mut Commands, parent: Entity, look: &Look, b: &ServerBrowser, page: Option<&UiLayout>) {
    let g = b.grow;
    let has_selection = b.selected.is_some_and(|a| b.rows.iter().any(|r| r.addr == a));
    let connect = rect_in(page, "ConnectButton", (550.0, 244.0, 67.0, 24.0), (3, 0), g);
    button(
        commands,
        parent,
        look,
        connect,
        &text_of(b, page, "ConnectButton", "#ServerBrowser_Connect", "Connect"),
        Target::Button(ButtonId::Connect),
        b,
        (has_selection, true, false),
    );
    // Internet's "Refresh all" is the others' "Refresh".
    let refresh = rect_in(page, "RefreshButton", (453.0, 244.0, 95.0, 24.0), (3, 0), g);
    let refresh_text = if b.tab == Tab::Internet {
        text_of(b, page, "RefreshButton", "#ServerBrowser_RefreshAll", "Refresh all")
    } else {
        b.text("#ServerBrowser_Refresh", "Refresh")
    };
    button(
        commands,
        parent,
        look,
        refresh,
        &refresh_text,
        Target::Button(ButtonId::Refresh),
        b,
        (true, false, false),
    );
    let right_x = if b.tab == Tab::Favorites {
        let add = rect_in(page, "AddServerButton", (349.0, 244.0, 100.0, 24.0), (3, 0), g);
        button(
            commands,
            parent,
            look,
            add,
            &text_of(b, page, "AddServerButton", "#ServerBrowser_AddServer", "Add a Server"),
            Target::Button(ButtonId::AddServer),
            b,
            (true, false, false),
        );
        add.0
    } else {
        let quick = rect_in(page, "RefreshQuickButton", (345.0, 244.0, 105.0, 24.0), (3, 0), g);
        button(
            commands,
            parent,
            look,
            quick,
            &text_of(
                b,
                page,
                "RefreshQuickButton",
                "#ServerBrowser_RefreshQuick",
                "Quick refresh",
            ),
            Target::Button(ButtonId::QuickRefresh),
            b,
            (!b.rows.is_empty(), false, false),
        );
        quick.0
    };
    let filter = rect_in(page, "Filter", (140.0, 244.0, 108.0, 24.0), (2, 0), g);
    // The file puts Filters after the simplified list box, which only
    // Internet has: here it starts at the left.
    let filter = (8.0, filter.1, filter.2, filter.3);
    button(
        commands,
        parent,
        look,
        filter,
        &text_of(b, page, "Filter", "#ServerBrowser_Filters", "Filters"),
        Target::Button(ButtonId::Filters),
        b,
        (true, false, b.filters.shown),
    );
    if b.filters.active() {
        let (_, y, _, h) = rect_in(page, "FilterString", (250.0, 244.0, 90.0, 24.0), (2, 1), g);
        let x = filter.0 + filter.2 + 4.0;
        label(
            commands,
            parent,
            look,
            (x, y, (right_x - x - 4.0).max(20.0), h),
            &b.filter_text(),
            look.font("DefaultSmall", (13.0, false)),
            look.dull(),
            -1,
        );
    }
}

/// The filters (shown): game, location and anti-cheat as they are
/// (greyed: LAN servers have none of those), map, latency (a drop-down),
/// max players, the three checks; under the list, following the frame's
/// bottom as it grows.
pub(super) fn filter_controls(
    commands: &mut Commands,
    parent: Entity,
    root: Entity,
    look: &Look,
    b: &ServerBrowser,
    page: Option<&UiLayout>,
) {
    let g = b.grow;
    let at = |name: &str, fallback: (f32, f32, f32, f32)| rect_in(page, name, fallback, (2, 0), g);
    let font = look.default_font();
    for (name, token, ours, fallback) in [
        (
            "GameFilterLabel",
            "#ServerBrowser_Game",
            "Game",
            (12.0, 150.0, 44.0, 24.0),
        ),
        (
            "LocationFilterLabel",
            "#ServerBrowser_Location",
            "Location",
            (234.0, 180.0, 72.0, 24.0),
        ),
        ("MapFilterLabel", "#ServerBrowser_Map", "Map", (12.0, 180.0, 44.0, 24.0)),
        (
            "MaxPlayerFilterLabel",
            "#ServerBrowser_MaxPlayer",
            "Max player count",
            (12.0, 210.0, 144.0, 24.0),
        ),
        (
            "PingFilterLabel",
            "#ServerBrowser_Latency",
            "Latency",
            (234.0, 150.0, 72.0, 24.0),
        ),
        (
            "SecureFilterLabel",
            "#ServerBrowser_AntiCheat",
            "Anti-cheat",
            (236.0, 210.0, 72.0, 24.0),
        ),
    ] {
        let greyed = matches!(name, "LocationFilterLabel" | "SecureFilterLabel");
        let (rect, text) = (at(name, fallback), text_of(b, page, name, token, ours));
        if greyed {
            widgets::engraved(commands, parent, look, rect, &text, font.clone(), 1);
        } else {
            label(commands, parent, look, rect, &text, font.clone(), look.text(), 1);
        }
    }
    let all = b.text("#ServerBrowser_All", "<All>");
    for (name, fallback) in [
        ("GameFilter", (60.0, 150.0, 164.0, 24.0)),
        ("LocationFilter", (311.0, 180.0, 112.0, 24.0)),
        ("SecureFilter", (311.0, 210.0, 112.0, 24.0)),
    ] {
        widgets::combo_box(
            commands,
            parent,
            look,
            at(name, fallback),
            &all,
            false,
            false,
            false,
            (),
        );
    }
    let (_, token, ours) = LATENCIES[b.filters.latency.min(LATENCIES.len() - 1)];
    let ping = at("PingFilter", (311.0, 150.0, 112.0, 24.0));
    widgets::combo_box(
        commands,
        parent,
        look,
        ping,
        &b.text(token, ours),
        true,
        b.combo.is_some(),
        b.focus == Some(Focus::Latency),
        Hit(Target::Latency),
    );
    if let Some(list) = &b.combo {
        let entries: Vec<String> = LATENCIES.iter().map(|(_, t, o)| b.text(t, o)).collect();
        widgets::combo_popup(
            commands,
            parent,
            root,
            look,
            ping,
            &entries,
            list,
            |k| Hit(Target::ComboItem(k)),
            Hit(Target::Outside),
        );
    }
    text_box(
        commands,
        parent,
        look,
        at("MapFilter", (60.0, 180.0, 164.0, 24.0)),
        &b.filters.map,
        Field::MapFilter,
        b,
        false,
    );
    text_box(
        commands,
        parent,
        look,
        at("MaxPlayerFilter", (160.0, 210.0, 64.0, 24.0)),
        &b.filters.max_players,
        Field::MaxPlayers,
        b,
        false,
    );
    for (name, token, ours, fallback, on, which) in [
        (
            "ServerEmptyFilterCheck",
            "#ServerBrowser_HasUsersPlaying",
            "Has users playing",
            (436.0, 170.0, 184.0, 24.0),
            b.filters.not_empty,
            Check::NotEmpty,
        ),
        (
            "ServerFullFilterCheck",
            "#ServerBrowser_ServerNotFull",
            "Server not full",
            (436.0, 150.0, 184.0, 24.0),
            b.filters.not_full,
            Check::NotFull,
        ),
        (
            "NoPasswordFilterCheck",
            "#ServerBrowser_IsNotPasswordProtected",
            "Is not password protected",
            (436.0, 190.0, 184.0, 24.0),
            b.filters.no_password,
            Check::NoPassword,
        ),
    ] {
        let (x, y, w, h) = at(name, fallback);
        widgets::check_button(
            commands,
            parent,
            look,
            (x, y, w.min(624.0 + g.x - x - 4.0), h),
            &text_of(b, page, name, token, ours),
            on,
            true,
            b.focus == Some(Focus::Check(which)),
            Hit(Target::Check(which)),
        );
    }
}

/// A dialog frame of a layout (modal over the browser), its size from the
/// layout, its close box Cancel.
pub(super) fn dialog_frame(
    commands: &mut Commands,
    root: Entity,
    look: &Look,
    (id, layout, name): (widgets::WindowId, Option<&UiLayout>, &str),
    fallback: (f32, f32),
    title: &str,
) -> Entity {
    let (_, _, w, h) = rect_of(layout, name, (0.0, 0.0, fallback.0, fallback.1));
    let parts = widgets::frame(commands, root, look, FrameSpec::new(id).modal(), (w, h), title);
    if let Some(close) = parts.close {
        commands.entity(close).insert(Hit(Target::Button(ButtonId::Close)));
    }
    parts.frame
}

/// Add a Server: the address box, add it or find games at it, the
/// server found (select it to add it; ours: double-click joins it).
pub(super) fn add_server_dialog(commands: &mut Commands, root: Entity, look: &Look, b: &ServerBrowser, icons: &BrowserIcons) {
    let Dialog::AddServer {
        text,
        selected,
        error,
        looked_up,
    } = &b.dialog
    else {
        return;
    };
    let layout = b.ui.0.as_ref().and_then(|u| u.servers.get("add"));
    let title = control(layout, "DialogAddServer")
        .and_then(|c| c.keys.get("title"))
        .map(|t| b.text(t, "Add Server - Servers"))
        .unwrap_or_else(|| b.text("#ServerBrowser_AddServersTitle", "Add Server - Servers"));
    let f = dialog_frame(
        commands,
        root,
        look,
        (ADD_WINDOW, layout, "DialogAddServer"),
        (572.0, 390.0),
        &title,
    );
    let font = look.default_font();
    label(
        commands,
        f,
        look,
        rect_of(layout, "InfoLabel", (22.0, 46.0, 330.0, 20.0)),
        &text_of(
            b,
            layout,
            "InfoLabel",
            "#ServerBrowser_EnterIPofServerToAdd",
            "Enter the IP address of the server you wish to add.",
        ),
        look.font("UiBold", (12.0, true)),
        look.text(),
        -1,
    );
    text_box(
        commands,
        f,
        look,
        rect_of(layout, "ServerNameText", (20.0, 74.0, 330.0, 24.0)),
        text,
        Field::AddAddress,
        b,
        false,
    );
    let has_text = !text.trim().is_empty();
    button(
        commands,
        f,
        look,
        rect_of(layout, "OKButton", (356.0, 74.0, 190.0, 24.0)),
        &text_of(
            b,
            layout,
            "OKButton",
            "#ServerBrowser_AddAddressToFavorites",
            "Add this address to favorites",
        ),
        Target::Button(ButtonId::AddOk),
        b,
        (has_text, true, false),
    );
    button(
        commands,
        f,
        look,
        rect_of(layout, "TestServersButton", (356.0, 102.0, 190.0, 24.0)),
        &text_of(
            b,
            layout,
            "TestServersButton",
            "#ServerBrowser_FindGames",
            "Find games at this address...",
        ),
        Target::Button(ButtonId::AddFind),
        b,
        (has_text, false, false),
    );
    button(
        commands,
        f,
        look,
        rect_of(layout, "CancelButton", (482.0, 131.0, 64.0, 24.0)),
        &text_of(b, layout, "CancelButton", "#ServerBrowser_Cancel", "Cancel"),
        Target::Button(ButtonId::AddCancel),
        b,
        (true, false, false),
    );
    // The examples, one per line (or the address's error).
    let (ex, ey, ew, _) = rect_of(layout, "ExampleLabel", (22.0, 106.0, 328.0, 74.0));
    let examples = match error {
        Some(e) => vec![e.clone()],
        None => text_of(
            b,
            layout,
            "ExampleLabel",
            "#ServerBrowser_Examples",
            "Examples:\ntfc.valvesoftware.com\ncounterstrike.speakeasy.net:27016\n205.158.143.200:27015",
        )
        .replace("\\n", "\n")
        .lines()
        .map(String::from)
        .collect(),
    };
    for (k, line) in examples.iter().enumerate() {
        label(
            commands,
            f,
            look,
            (ex, ey + k as f32 * 16.0, ew, 16.0),
            line,
            font.clone(),
            look.dull(),
            -1,
        );
    }
    // The servers found: a tab over a list.
    let (gx, gy, gw, gh) = rect_of(layout, "GameTabs", (20.0, 175.0, 526.0, 150.0));
    let tab_text = b
        .text("#ServerBrowser_ServersCount", "Servers (%s1)")
        .replace("%s1", &b.found.iter().filter(|r| r.responded).count().to_string());
    widgets::tabs(
        commands,
        f,
        look,
        (gx, gy + 1.0),
        &[(tab_text, true)],
        0,
        Some(110.0),
        None,
        |_| (),
    );
    let (_, sy, _, sh) = rect_of(layout, "Servers", (0.0, 28.0, 526.0, 122.0));
    let list = commands
        .spawn((
            Node {
                border: UiRect::all(px(1.0)),
                ..place(look, gx, gy + sy, gw, sh.min(gh - sy))
            },
            bevel(look, false),
            BackgroundColor(look.color("ListPanel.BgColor", [0, 0, 0, 128])),
            ChildOf(f),
        ))
        .id();
    let headers = [
        (
            b.text("#ServerBrowser_Servers", "Servers"),
            gw - 2.0 - 112.0 - 55.0 - 90.0 - 55.0,
        ),
        (b.text("#ServerBrowser_Game", "Game"), 112.0),
        (b.text("#ServerBrowser_Players", "Players"), 55.0),
        (b.text("#ServerBrowser_Map", "Map"), 90.0),
        (b.text("#ServerBrowser_Latency", "Latency"), 55.0),
    ];
    let small = look.font("DefaultSmall", (13.0, false));
    let mut x = 0.0;
    for (t, cw) in &headers {
        let e = commands
            .spawn((
                Node {
                    border: UiRect::all(px(1.0)),
                    ..place(look, x, 0.0, *cw, HEADER_H)
                },
                bevel(look, true),
                ChildOf(list),
            ))
            .id();
        label(
            commands,
            e,
            look,
            (4.0, 0.0, cw - 8.0, HEADER_H - 2.0),
            t,
            small.clone(),
            look.text(),
            -1,
        );
        x += cw;
    }
    if let (Some(row), Some(_)) = (&b.found, looked_up) {
        let color = if *selected {
            look.color("ListPanel.SelectedTextColor", [0, 0, 0, 255])
        } else {
            look.color("ListPanel.TextColor", [221, 221, 221, 255])
        };
        let e = commands
            .spawn((
                place(look, 0.0, HEADER_H, gw - 2.0, ROW_H),
                BackgroundColor(if *selected {
                    look.color("ListPanel.SelectedBgColor", [255, 155, 0, 255])
                } else {
                    Color::NONE
                }),
                Hit(Target::Row(0)),
                widgets::clickable(),
                ChildOf(list),
            ))
            .id();
        let name = if row.responded {
            row.name.clone()
        } else if row.not_responding {
            b.text("#ServerBrowser_ServerNotResponding", "Server is not responding.")
        } else {
            b.text("#ServerBrowser_RefreshingServerList", "Refreshing server list...")
        };
        let cells = [
            name,
            row.game.clone(),
            if row.responded {
                format!("{} / {}", row.players, row.max_players)
            } else {
                String::new()
            },
            row.map.clone(),
            row.ping.map(|p| p.to_string()).unwrap_or_default(),
        ];
        let mut x = 0.0;
        for (t, (_, cw)) in cells.iter().zip(&headers) {
            label(
                commands,
                e,
                look,
                (x + 4.0, 0.0, cw - 8.0, ROW_H),
                t,
                font.clone(),
                color,
                -1,
            );
            x += cw;
        }
        if row.password {
            icon(
                commands,
                e,
                look,
                (gw - 20.0, 0.0, 16.0, ROW_H),
                icons.icons.get("password"),
                "*",
                color,
            );
        }
    }
    button(
        commands,
        f,
        look,
        rect_of(layout, "SelectedOKButton", (316.0, 340.0, 230.0, 24.0)),
        &text_of(
            b,
            layout,
            "SelectedOKButton",
            "#ServerBrowser_AddSelectedToFavorites",
            "Add selected game server to favorites",
        ),
        Target::Button(ButtonId::AddSelected),
        b,
        (*selected && b.found.as_ref().is_some_and(|r| r.responded), false, false),
    );
}

/// The server wants a password: its name, the password box, Connect.
pub(super) fn password_dialog(commands: &mut Commands, root: Entity, look: &Look, b: &ServerBrowser) {
    let Dialog::Password { name, entry, .. } = &b.dialog else {
        return;
    };
    let layout = b.ui.0.as_ref().and_then(|u| u.servers.get("password"));
    let title = control(layout, "DialogServerPassword")
        .and_then(|c| c.keys.get("title"))
        .map(|t| b.text(t, "Server Requires Password"))
        .unwrap_or_else(|| b.text("#ServerBrowser_ServerRequiresPasswordTitle", "Server Requires Password"));
    let f = dialog_frame(
        commands,
        root,
        look,
        (PASSWORD_WINDOW, layout, "DialogServerPassword"),
        (290.0, 176.0),
        &title,
    );
    let font = look.default_font();
    label(
        commands,
        f,
        look,
        rect_of(layout, "GameLabel", (20.0, 42.0, 252.0, 24.0)),
        name,
        font.clone(),
        look.dull(),
        -1,
    );
    label(
        commands,
        f,
        look,
        rect_of(layout, "InfoLabel", (20.0, 68.0, 252.0, 24.0)),
        &text_of(
            b,
            layout,
            "InfoLabel",
            "#ServerBrowser_PasswordRequired",
            "This server requires a password to join.",
        ),
        font.clone(),
        look.text(),
        -1,
    );
    label(
        commands,
        f,
        look,
        rect_of(layout, "PasswordLabel", (20.0, 95.0, 66.0, 24.0)),
        &text_of(b, layout, "PasswordLabel", "#ServerBrowser_PasswordLabel", "Password"),
        font.clone(),
        look.text(),
        1,
    );
    text_box(
        commands,
        f,
        look,
        rect_of(layout, "PasswordEntry", (86.0, 96.0, 186.0, 24.0)),
        &entry.input,
        Field::Password,
        b,
        true,
    );
    button(
        commands,
        f,
        look,
        rect_of(layout, "ConnectButton", (116.0, 136.0, 74.0, 24.0)),
        &text_of(b, layout, "ConnectButton", "#ServerBrowser_Connect", "Connect"),
        Target::Button(ButtonId::PasswordConnect),
        b,
        (true, true, false),
    );
    button(
        commands,
        f,
        look,
        rect_of(layout, "CancelButton", (198.0, 136.0, 74.0, 24.0)),
        &text_of(b, layout, "CancelButton", "#ServerBrowser_Cancel", "Cancel"),
        Target::Button(ButtonId::PasswordCancel),
        b,
        (true, false, false),
    );
}
