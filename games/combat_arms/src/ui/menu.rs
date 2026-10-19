//! Combat Arms' in-game menu (Esc; specs/combat_arms/ui_system.md H, M2,
//! S1, I1-I3), in the shell's place (without a game menu the shell's Esc
//! only frees the mouse): a popup read from our layout
//! (`data/ui/ingame_menu.xml`, the game's format), drawn 1:1 in screen px
//! with its first control centred on the screen (⌊W/2⌋ − ⌊w/2⌋), modal
//! (`desk::Desk`'s capture: nothing under it takes a click), its buttons
//! firing on the left button going down with their sounds.
//!
//! - Esc opens and closes it (not while the console is open or chat is
//!   being typed: those take the Esc). The game doesn't pause (no pause
//!   online), the mouse shows while it is open (`MenuStatus::open`, which
//!   the shell reads: binds and the mouse grab wait), and is taken back
//!   when it closes.
//! - Buttons: return to lobby, report a user, start/stop recording,
//!   cancel (closes it).
//!
//! `CaMenuStatePlugin` is the menu without a window (`InGameMenu`, the
//! pointer as `MenuPointer`; tests/ui/ui_system.rs); `CaMenuPlugin` adds
//! the window's pointer, the cursor and the drawing. `ingamemenu` (the
//! console) toggles it, for screenshots.
//!
//! Known shortcuts (the spec's most literal reading where it is unclear):
//! - "Return to lobby" quits the game (there is no lobby yet), so its
//!   label says "Leave Game"; reporting a user and recording have no
//!   flow, so their buttons are disabled (the D look).
//! - While the menu is open the player stops moving and looking, as with
//!   any free mouse in the shell (the spec's Open question 6 asks whether
//!   the real game keeps movement).
//! - Interface sounds are counted (`InGameMenu::played`), not played: the
//!   sound files' roles are in the client interface attributes (keyed).
//! - The popup's root takes the pointer over the whole screen (the file
//!   root's 1024 x 1024 is used for alignment only), so a centred popup
//!   past x 1024 still takes clicks.

use bevy::{prelude::*, window::PrimaryWindow};

use super::{
    CaUi,
    desk::{self, Desk, Event},
    draw::{self, CaFaces},
    layout::{Layout, Sound, xml},
};
use crate::client::{
    fonts::UiFonts,
    front_end::{FrontEndSync, MenuStatus},
};
use crate::console::{ConsoleAppExt, Console};

/// The menu's layout (ours, the game's format).
pub const LAYOUT: &str = include_str!("../../data/ui/ingame_menu.xml");
/// Its first control: the box centred on the screen.
pub const BOX: &str = "InGameMenu";
/// The buttons' command ids (the layout's Cmd_id).
pub const CMD_LOBBY: i32 = 1;
pub const CMD_REPORT: i32 = 2;
pub const CMD_RECORD: i32 = 3;
pub const CMD_CANCEL: i32 = 4;

/// The in-game menu: its popup on a desk, open or not, the screen it is
/// centred on, and the interface sounds it has played.
#[derive(Resource, Clone, Debug)]
pub struct InGameMenu {
    pub desk: Desk,
    pub sheet: usize,
    pub open: bool,
    pub screen: IVec2,
    pub played: Vec<Sound>,
    /// Buttons fired, by name (for tests and the log).
    pub fired: Vec<String>,
}

impl InGameMenu {
    pub fn new(ui: &CaUi) -> Self {
        let layout = xml::read(LAYOUT).expect("data/ui/ingame_menu.xml");
        layout.check_fonts(&ui.fonts).expect("data/ui/ingame_menu.xml fonts");
        let mut desk = Desk::default();
        let sheet = desk.add("ingame_menu", layout, true);
        let mut menu = Self {
            desk,
            sheet,
            open: false,
            screen: IVec2::new(1024, 768),
            played: Vec::new(),
            fired: Vec::new(),
        };
        for name in ["Report", "Record"] {
            if let Some(id) = menu.layout().find(name) {
                menu.layout_mut().widget_mut(id).enabled = false;
            }
        }
        menu.place();
        menu
    }

    pub fn layout(&self) -> &Layout {
        &self.desk.sheets[self.sheet].layout
    }

    pub fn layout_mut(&mut self) -> &mut Layout {
        &mut self.desk.sheets[self.sheet].layout
    }

    /// Centre the first control on the screen, 1:1.
    pub fn place(&mut self) {
        let screen = self.screen;
        let l = self.layout_mut();
        if let Some(id) = l.find(BOX) {
            l.centre(id, screen);
        }
    }

    /// Open or close it.
    pub fn set_open(&mut self, open: bool) {
        if open == self.open {
            return;
        }
        self.open = open;
        if open {
            self.place();
            self.desk.show(self.sheet, true);
        } else {
            self.desk.hide(self.sheet);
        }
    }
}

/// The pointer in screen px (None: outside the window).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct MenuPointer(pub Option<IVec2>);

/// The menu's systems.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct MenuSystems;

/// The menu without a window.
pub struct CaMenuStatePlugin;

impl Plugin for CaMenuStatePlugin {
    fn build(&self, app: &mut App) {
        let ui = app.world().get_resource::<CaUi>().cloned().unwrap_or_default();
        app.insert_resource(InGameMenu::new(&ui))
            .insert_resource(ui)
            .init_resource::<MenuPointer>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .add_systems(Update, (keys, pointer, act).chain().in_set(MenuSystems))
            .console_command("ingamemenu", "Open or close Combat Arms' in-game menu (Esc).", |w, _| {
                let mut m = w.resource_mut::<InGameMenu>();
                let open = !m.open;
                m.set_open(open);
                Ok(None)
            });
        // The shell's view of it (`client::front_end`): no main menu, so
        // always in the game.
        app.insert_resource(MenuStatus {
            in_game: true,
            ..default()
        })
        .add_systems(First, sync_status.in_set(FrontEndSync))
        .add_systems(Update, sync_status.in_set(FrontEndSync).after(MenuSystems))
        .add_systems(PostUpdate, sync_status.in_set(FrontEndSync));
    }
}

fn sync_status(menu: Res<InGameMenu>, mut status: ResMut<MenuStatus>) {
    if menu.is_changed() {
        status.set_if_neq(MenuStatus {
            open: menu.open,
            in_game: true,
            ..default()
        });
    }
}

/// Esc opens and closes it, unless the console or chat takes the key.
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    console: Option<Res<crate::client::console::ConsoleUi>>,
    chat: Option<Res<crate::client::chat::ChatInput>>,
    mut menu: ResMut<InGameMenu>,
) {
    let busy = console.is_some_and(|c| c.open) || chat.is_some_and(|c| c.open.is_some());
    if keys.just_pressed(KeyCode::Escape) && !busy {
        let open = !menu.open;
        menu.set_open(open);
    }
}

/// Each widget's hover and held states.
fn states(l: &Layout) -> Vec<(bool, bool)> {
    l.tree
        .order()
        .into_iter()
        .map(|i| (l.widget(i).hover, l.widget(i).held))
        .collect()
}

/// The pointer: hover, and the left button down and up.
fn pointer(pointer: Res<MenuPointer>, mouse: Res<ButtonInput<MouseButton>>, ui: Res<CaUi>, mut menu: ResMut<InGameMenu>) {
    if !menu.open {
        return;
    }
    let Some(p) = pointer.0 else { return };
    let ctx = ui.ctx();
    let m = menu.bypass_change_detection();
    let before = states(m.layout());
    m.desk.pointer_moved(p, &ctx);
    if mouse.just_pressed(MouseButton::Left) {
        m.desk.left_down(p, &ctx);
    }
    if mouse.just_released(MouseButton::Left) {
        m.desk.left_up();
    }
    let after = states(m.layout());
    if before != after || !m.desk.events.is_empty() {
        menu.set_changed();
    }
}

/// What the buttons do; their sounds counted.
fn act(mut menu: ResMut<InGameMenu>, console: Option<ResMut<Console>>) {
    if menu.desk.events.is_empty() {
        return;
    }
    let events = menu.desk.take_events();
    menu.played.extend(desk::sounds(&events));
    let mut console = console;
    for e in events {
        let Event::Fired { name, cmd, .. } = e else { continue };
        menu.fired.push(name);
        match cmd {
            CMD_CANCEL => menu.set_open(false),
            CMD_LOBBY => {
                menu.set_open(false);
                if let Some(c) = console.as_mut() {
                    c.submit("quit");
                }
            }
            _ => {}
        }
    }
}

/// The menu with the window: its pointer, the cursor, the drawing.
pub struct CaMenuPlugin;

impl Plugin for CaMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(CaMenuStatePlugin)
            .add_systems(Startup, spawn)
            // Before the console's toggle: the Esc that closes the console
            // doesn't open the menu.
            .configure_sets(Update, MenuSystems.before(crate::client::console::toggle))
            .add_systems(Update, window_pointer.before(MenuSystems))
            .add_systems(Update, (cursor, draw_menu).after(MenuSystems));
    }
}

/// The window's size and pointer (logical px, as the UI lays out).
fn window_pointer(
    window: Query<&Window, With<PrimaryWindow>>,
    mut pointer: ResMut<MenuPointer>,
    mut menu: ResMut<InGameMenu>,
) {
    let Ok(w) = window.single() else { return };
    let screen = IVec2::new(w.resolution.width() as i32, w.resolution.height() as i32);
    if screen.x > 0 && screen.y > 0 && menu.screen != screen {
        menu.screen = screen;
        menu.place();
    }
    pointer.set_if_neq(MenuPointer(w.cursor_position().map(|p| p.floor().as_ivec2())));
}

/// The mouse shows while it is open and is taken back when it closes
/// (once the button is up, unless the console is open).
fn cursor(
    menu: Res<InGameMenu>,
    mouse: Res<ButtonInput<MouseButton>>,
    ui: Option<Res<crate::client::console::ConsoleUi>>,
    cursor: Option<Single<&mut bevy::window::CursorOptions>>,
    mut regrab: Local<bool>,
) {
    let Some(mut cursor) = cursor else { return };
    if menu.open {
        if crate::client::input::cursor_grabbed(&cursor) {
            crate::client::input::release_cursor(&mut cursor);
        }
        *regrab = true;
    } else if *regrab && !mouse.pressed(MouseButton::Left) {
        *regrab = false;
        if !ui.is_some_and(|u| u.open) {
            crate::client::input::capture_cursor(&mut cursor);
        }
    }
}

#[derive(Component)]
struct MenuRoot;

fn spawn(mut commands: Commands) {
    commands.spawn((
        MenuRoot,
        Node {
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(0.0),
            width: percent(100.0),
            height: percent(100.0),
            ..default()
        },
        // Over the HUD.
        GlobalZIndex(60),
        Visibility::Hidden,
    ));
}

fn draw_menu(
    menu: Res<InGameMenu>,
    ui: Res<CaUi>,
    fonts: Option<Res<UiFonts>>,
    faces: Option<Res<CaFaces>>,
    root: Single<(Entity, &mut Visibility), With<MenuRoot>>,
    mut commands: Commands,
) {
    if !menu.is_changed() && !faces.as_ref().is_some_and(|f| f.is_changed()) {
        return;
    }
    let (root, mut vis) = root.into_inner();
    *vis = if menu.open { Visibility::Visible } else { Visibility::Hidden };
    commands.entity(root).despawn_related::<Children>();
    if !menu.open {
        return;
    }
    let draws = menu.layout().draw(&ui.ctx());
    commands.entity(root).with_children(|p| {
        draw::spawn_draws(p, &draws, &ui.stand_ins, fonts.as_deref(), faces.as_deref());
    });
}
