//! The HUD's chat panel, input bar and radio menu
//! (specs/combat_arms/radio_chat.md P1, C2, R2; ui_system.md H): what
//! `game::chat` keeps, shown in the HUD and typed into.
//!
//! - `CaTalkStatePlugin` (no window, the tests') turns the game's lines
//!   (`CaChat::log`) into the spec's queue (`chat::ChatQueue`: stacked
//!   from the template's place, the oldest dropped when too tall, one
//!   oldest per expired line), and the open input and radio menu into
//!   `CaTalk::input` and `CaTalk::radio`. Lines that are string ids
//!   (`$SameSentence`) are resolved with the game's table.
//! - `CaTalkPlugin` adds the drawing (the shell's UI nodes) and the keys:
//!   while the input is open it takes every key (F2, F3, F4 switch the
//!   tab, Esc closes, Enter sends, Up cycles whisper partners, Page Up,
//!   Page Down and Down are swallowed, the rest is typed) and the game
//!   sees none; while a radio menu is open the number keys pick (the
//!   weapon keys are swallowed). Binds (Enter, F2 to F4, B, V, N, T, F8)
//!   are `game::chat`'s commands.
//!
//! Known shortcuts: the panel, the input bar and the radio window are
//! drawn in code at places of ours (the HUD layouts' chat controls were
//! not available, Open question 3): flat boxes, the stand-in font; a
//! message's sender name takes its colour only on a one-line message;
//! IME input and GM paste are not there; the typed text has no caret
//! movement (only the end).

use bevy::{
    input::{
        ButtonState, InputSystems,
        keyboard::{Key, KeyboardInput},
    },
    prelude::*,
    window::CursorOptions,
};

use super::{
    CaUi,
    chat::{self, ChatQueue, GAP, Message, QUEUE_AT, QUEUE_SIZE, TEMPLATE_AT, TEMPLATE_WRAP},
    hud::{HudScreen, PenParams, hud_text, layer, model::Screen, pen, rect, rebuild, scale},
    layout::{Effect, text},
    menu::InGameMenu,
};
use crate::{
    client::canvas::{HAlign, VAlign},
    game::chat::{
        CaChat, CaRadio, ChatUiState, input_key, now_ms, radio_number,
        client::{InputKey, Tab},
        show::Rgba,
    },
};

/// The line template's font (PLACEHOLDER: the HUD layout's isn't read).
pub const FONT: &str = "bold 12";
/// The input bar's place and size, ref px.
pub const INPUT_AT: (f32, f32) = (0.0, 706.0);
pub const INPUT_SIZE: (f32, f32) = (400.0, 22.0);
/// The radio window's top-left and the line step, ref px.
pub const RADIO_AT: (f32, f32) = (24.0, 250.0);
pub const RADIO_STEP: f32 = 18.0;

/// The input bar as shown.
#[derive(Clone, Debug, PartialEq)]
pub struct InputView {
    pub tab: Tab,
    pub text: String,
}

/// The open radio menu as shown: its title and the entries (at most
/// nine).
#[derive(Clone, Debug, PartialEq)]
pub struct RadioView {
    pub title: String,
    pub lines: Vec<String>,
}

/// What the HUD shows of the chat.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct CaTalk {
    pub queue: ChatQueue,
    /// The last `ChatLog` line mirrored into the queue.
    pub seen: u64,
    pub input: Option<InputView>,
    pub radio: Option<RadioView>,
    /// The talk icon (push-to-talk held).
    pub talking: bool,
}

/// The panel's state without a window.
pub struct CaTalkStatePlugin;

impl Plugin for CaTalkStatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CaTalk>()
            .init_resource::<HudScreen>()
            .init_resource::<CaUi>()
            .add_systems(Update, update.after(crate::game::chat::CaChatSet));
    }
}

fn update(world: &mut World) {
    let now = now_ms(world);
    let screen = world.get_resource::<HudScreen>().map_or(Screen::default(), |s| s.0);
    let s = scale(screen);
    let ui = world.resource::<CaUi>().clone();
    let font = ui.fonts.get(FONT).cloned();
    let (lines, input, radio_open, talking) = {
        let c = world.resource::<CaChat>();
        let new: Vec<_> = c.log.lines.iter().filter(|l| l.id > world.resource::<CaTalk>().seen).cloned().collect();
        (new, c.input.clone().map(|t| (c.tab, t)), c.radio_open, c.voice.talking)
    };
    let menu = radio_open.and_then(|class| {
        let data = world.resource::<CaRadio>();
        let (title, entries) = data.menu(class)?;
        Some(RadioView {
            title: title.to_string(),
            lines: entries.iter().map(|e| e.list_text.clone()).collect(),
        })
    });
    let mut talk = world.resource_mut::<CaTalk>();
    for l in lines {
        talk.seen = talk.seen.max(l.id);
        let shown = if l.text.starts_with('$') { ui.strings.resolve(&l.text) } else { l.text.clone() };
        let height = font.as_ref().map_or(14, |f| chat::height(&shown, f, s));
        talk.queue.push(Message {
            text: shown,
            born: l.born,
            life: l.life,
            height,
            colour: l.colour,
            // The name's span is in the unresolved text; kept only for a
            // plain line (ids resolve to other text).
            name: l.name.filter(|_| !l.text.starts_with('$')),
        });
    }
    talk.queue.frame(now);
    let input = input.map(|(tab, text)| InputView { tab, text });
    if talk.input != input {
        talk.input = input;
    }
    if talk.radio != menu {
        talk.radio = menu;
    }
    if talk.talking != talking {
        talk.talking = talking;
    }
}

/// The panel, its input and its keys on the shell.
pub struct CaTalkPlugin;

impl Plugin for CaTalkPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((CaTalkStatePlugin, CaTalkKeysPlugin))
            .add_systems(Startup, |mut commands: Commands| {
                commands.spawn((TalkRoot, layer(60)));
            })
            .add_systems(Update, (sync_ui_state.before(crate::game::chat::CaChatSet), draw.after(update)));
    }
}

/// The keys alone (the tests' too): typing into the input, the radio
/// menu's number keys.
pub struct CaTalkKeysPlugin;

impl Plugin for CaTalkKeysPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ButtonInput<KeyCode>>()
            .add_message::<KeyboardInput>()
            .add_systems(PreUpdate, typing.after(InputSystems));
    }
}

#[derive(Component)]
struct TalkRoot;

/// The front end's facts for the chat: the cursor is grabbed and no menu
/// or console is up.
fn sync_ui_state(
    cursor: Query<&CursorOptions>,
    console: Option<Res<crate::client::console::ConsoleUi>>,
    menu: Option<Res<InGameMenu>>,
    mut state: ResMut<ChatUiState>,
) {
    let grabbed = cursor.iter().next().is_some_and(crate::client::input::cursor_grabbed);
    let busy = console.is_some_and(|c| c.open) || menu.is_some_and(|m| m.open);
    let playing = grabbed && !busy;
    if state.playing != playing {
        state.playing = playing;
    }
}

/// The keys: typing into the open input, the radio menu's number keys.
/// Runs before the binds see the keyboard and clears what it used.
fn typing(world: &mut World) {
    // The console owns the keyboard while it is open (it reads the same
    // messages): leave them, and the key state, alone.
    if world.get_resource::<crate::client::console::ConsoleUi>().is_some_and(|c| c.open) {
        return;
    }
    let open = world.resource::<CaChat>().is_open();
    if open {
        // Only an open input takes the messages: draining them every
        // frame would starve the console's typing.
        let events: Vec<KeyboardInput> = world.resource_mut::<Messages<KeyboardInput>>().drain().collect();
        for e in &events {
            if e.state != ButtonState::Pressed {
                continue;
            }
            match (&e.logical_key, e.key_code) {
                (_, KeyCode::Enter | KeyCode::NumpadEnter) => {
                    input_key(world, InputKey::Enter);
                    break;
                }
                (_, KeyCode::Escape) => {
                    input_key(world, InputKey::Esc);
                    break;
                }
                (_, KeyCode::ArrowUp) => input_key(world, InputKey::Up),
                (_, KeyCode::F2) => input_key(world, InputKey::Tab(Tab::All)),
                (_, KeyCode::F3) => input_key(world, InputKey::Tab(Tab::Team)),
                (_, KeyCode::F4) => input_key(world, InputKey::Tab(Tab::Clan)),
                (_, KeyCode::PageUp | KeyCode::PageDown | KeyCode::ArrowDown) => {
                    input_key(world, InputKey::Swallowed)
                }
                (Key::Backspace, _) => world.resource_mut::<CaChat>().backspace(),
                (Key::Space, _) => world.resource_mut::<CaChat>().type_text(" "),
                (Key::Character(s), _) => world.resource_mut::<CaChat>().type_text(s),
                _ => {}
            }
        }
        // The game sees no keys while typing, nor on the frame it closed.
        world.resource_mut::<ButtonInput<KeyCode>>().reset_all();
        return;
    }
    // A radio menu: the number keys pick; the weapon keys never see them.
    if world.resource::<CaChat>().radio_open.is_some() {
        const DIGITS: [(KeyCode, usize); 10] = [
            (KeyCode::Digit1, 1),
            (KeyCode::Digit2, 2),
            (KeyCode::Digit3, 3),
            (KeyCode::Digit4, 4),
            (KeyCode::Digit5, 5),
            (KeyCode::Digit6, 6),
            (KeyCode::Digit7, 7),
            (KeyCode::Digit8, 8),
            (KeyCode::Digit9, 9),
            (KeyCode::Digit0, 0),
        ];
        for (key, n) in DIGITS {
            if world.resource::<ButtonInput<KeyCode>>().just_pressed(key) && radio_number(world, n) {
                world.resource_mut::<ButtonInput<KeyCode>>().clear_just_pressed(key);
            }
            if world.resource::<CaChat>().radio_open.is_none() {
                break;
            }
        }
        // Weapon-select keys are swallowed while the menu is open.
        let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
        for (key, _) in DIGITS {
            keys.clear_just_pressed(key);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    talk: Res<CaTalk>,
    screen: Option<Res<HudScreen>>,
    params: PenParams,
    root: Single<Entity, With<TalkRoot>>,
    mut last: Local<Option<(CaTalk, Screen)>>,
    mut commands: Commands,
) {
    let screen = screen.map_or(Screen::default(), |s| s.0);
    if !rebuild(&mut last, (talk.clone(), screen), *root, &mut commands) {
        return;
    }
    let pen = pen(&params, screen);
    let s = scale(screen);
    let left = (HAlign::Default, VAlign::Default);
    let centred = (HAlign::Default, VAlign::Centre);
    let font = params.0.fonts.get(FONT).cloned();
    commands.entity(*root).with_children(|p| {
        // The queue.
        for (m, top) in talk.queue.messages.iter().zip(talk.queue.tops()) {
            let at = Vec2::new(TEMPLATE_AT.0 as f32, top as f32);
            let size = Vec2::new(TEMPLATE_WRAP as f32, m.height as f32);
            let one_line = m.height <= font.as_ref().map_or(14, |f| text::measure("A", f, 0, s).reference.y);
            match (m.name, &font) {
                (Some((a, b, name_colour)), Some(f)) if one_line && a == 0 && b <= m.text.len() => {
                    // The name in its colour, the rest after it.
                    let width = text::measure(&m.text[..b], f, 0, s).reference.x as f32;
                    hud_text(p, pen, (at, size, Vec2::ZERO), &m.text[..b], FONT, left, Effect::Shadow, name_colour);
                    hud_text(p, pen, (at + Vec2::X * width, size, Vec2::ZERO), &m.text[b..], FONT, left, Effect::Shadow, m.colour);
                }
                _ => hud_text(p, pen, (at, size, Vec2::ZERO), &m.text, FONT, left, Effect::Shadow, m.colour),
            }
        }
        // The input bar: the tabs, then the typed text.
        if let Some(input) = &talk.input {
            let bar = (Vec2::new(INPUT_AT.0, INPUT_AT.1), Vec2::new(INPUT_SIZE.0, INPUT_SIZE.1));
            p.spawn(rect(screen.rect(bar.0, bar.1), Color::srgba(0.04, 0.05, 0.06, 0.8)));
            for (i, (tab, name)) in [(Tab::All, "All"), (Tab::Team, "Team"), (Tab::Clan, "Clan")].into_iter().enumerate() {
                // The active tab is shown disabled (dim).
                let colour: Rgba = if input.tab == tab { [140, 140, 140, 255] } else { [255, 255, 255, 255] };
                let at = bar.0 + Vec2::new(4.0 + 52.0 * i as f32, 0.0);
                hud_text(p, pen, (at, Vec2::new(50.0, bar.1.y), Vec2::ZERO), name, FONT, centred, Effect::Shadow, colour);
            }
            let at = bar.0 + Vec2::new(170.0, 0.0);
            let typed = format!("{}_", input.text);
            hud_text(p, pen, (at, Vec2::new(226.0, bar.1.y), Vec2::ZERO), &typed, FONT, centred, Effect::Shadow, [255, 255, 255, 255]);
        }
        // The radio window: the title, then the numbered entries.
        if let Some(menu) = &talk.radio {
            let at = Vec2::new(RADIO_AT.0, RADIO_AT.1);
            p.spawn(rect(
                screen.rect(at - Vec2::splat(6.0), Vec2::new(220.0, RADIO_STEP * (menu.lines.len() as f32 + 1.0) + 12.0)),
                Color::srgba(0.04, 0.05, 0.06, 0.7),
            ));
            let white: Rgba = [255, 255, 255, 255];
            hud_text(p, pen, (at, Vec2::new(208.0, RADIO_STEP), Vec2::ZERO), &menu.title, FONT, left, Effect::Shadow, white);
            for (i, line) in menu.lines.iter().enumerate() {
                let at = at + Vec2::Y * RADIO_STEP * (i as f32 + 1.0);
                hud_text(p, pen, (at, Vec2::new(208.0, RADIO_STEP), Vec2::ZERO), &format!("{}. {line}", i + 1), FONT, left, Effect::Shadow, white);
            }
        }
        // The talk icon.
        if talk.talking {
            let at = Vec2::new(QUEUE_AT.0 as f32 + QUEUE_SIZE.0 as f32 + 6.0, INPUT_AT.1);
            p.spawn(rect(screen.rect(at, Vec2::new(14.0, 14.0)), Color::srgba(0.2, 0.8, 0.3, 0.9)));
        }
        let _ = GAP;
    });
}
