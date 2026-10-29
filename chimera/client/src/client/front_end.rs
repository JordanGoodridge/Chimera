//! The game's front end as the client shell sees it. A game's menus and
//! panels (CS:S's: `games::cs_source::ui`, added by the app) are a part
//! of that game; the shell (console, input, network glue, debug tools)
//! never names them. It tells them what happened through `FrontEnd`'s
//! hooks and reads what they show through `MenuStatus` and `KeyPanels`
//! (an app's own menu over them: `AppMenu`),
//! which the game's UI keeps in step with its own state (`FrontEndSync`).
//! Without a game UI the hooks do nothing and the resources are absent or
//! empty: the shell plays with no menus.

use std::collections::BTreeMap;

use bevy::prelude::*;

/// What the shell tells the game's menus; each game UI plugin sets the
/// hooks it owns (`FrontEnd::register`). Unset hooks do nothing.
#[derive(Resource, Clone, Copy)]
pub struct FrontEnd {
    /// A map is in and played (`map`, joined, a server's map change done):
    /// the menu closes.
    pub entered_game: fn(&mut World),
    /// Out of the game (`disconnect`, dropped): the main menu.
    pub left_game: fn(&mut World),
    /// A server is being joined (its address or name, then its map): the
    /// loading dialog.
    pub joining: fn(&mut World, &str),
    /// Joining (or the game) failed: the dialog says why
    /// (`net::client::JoinProgress::failure`).
    pub show_failure: fn(&mut World),
    /// A map failed to load: the loading dialog goes.
    pub map_load_failed: fn(&mut World),
    /// A game the local player starts (`map`, `--map`): it comes in as
    /// `rules::JoinChoice` says (the game's join menus, or on a team).
    pub start_game: fn(&mut World),
}

impl Default for FrontEnd {
    fn default() -> Self {
        Self {
            entered_game: |_| {},
            left_game: |_| {},
            joining: |_, _| {},
            show_failure: |_| {},
            map_load_failed: |_| {},
            start_game: |_| {},
        }
    }
}

impl FrontEnd {
    /// The hooks registered (the defaults where none are).
    pub fn of(world: &World) -> Self {
        world.get_resource::<Self>().copied().unwrap_or_default()
    }

    /// Set some of the hooks (a game UI plugin, as it builds).
    pub fn register(app: &mut App, set: impl FnOnce(&mut FrontEnd)) {
        let world = app.world_mut();
        let mut hooks = Self::of(world);
        set(&mut hooks);
        world.insert_resource(hooks);
    }
}

pub fn entered_game(w: &mut World) {
    (FrontEnd::of(w).entered_game)(w)
}

pub fn left_game(w: &mut World) {
    (FrontEnd::of(w).left_game)(w)
}

pub fn joining(w: &mut World, label: &str) {
    (FrontEnd::of(w).joining)(w, label)
}

pub fn show_failure(w: &mut World) {
    (FrontEnd::of(w).show_failure)(w)
}

pub fn map_load_failed(w: &mut World) {
    (FrontEnd::of(w).map_load_failed)(w)
}

pub fn start_game(w: &mut World) {
    (FrontEnd::of(w).start_game)(w)
}

/// The game menu (main menu, Esc menu, its dialogs) as the shell needs to
/// know it: present only when the game has one, kept by it.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct MenuStatus {
    /// Open: it owns the mouse and keys.
    pub open: bool,
    /// A map is loaded and played (false at the main menu).
    pub in_game: bool,
    /// A map or a server is loading (the loading dialog).
    pub loading: bool,
    /// The dialog shows why joining (or the game) failed.
    pub failed: bool,
}

/// An app's own menu over the game's (the party apps' Escape menu,
/// `party_kit::esc_menu`): with this resource in, the app owns Esc (the
/// game UI's own Esc menu stays shut), and while `open` the game UI
/// counts it in `MenuStatus::open`, so the mouse and the binds are the
/// menu's as they are under the game's own.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct AppMenu {
    pub open: bool,
}

/// The game's panels open now (CS:S's `buy`, `team`, `radio` and
/// `spectator` menus), by name, each kept by its panel (`sync_panel`).
/// Those that read the number keys say so: while one is open, the slot
/// keys don't switch weapons.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct Panels(pub BTreeMap<&'static str, Panel>);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Panel {
    pub open: bool,
    /// It reads the number keys.
    pub number_keys: bool,
}

impl Panels {
    /// An open panel reads the number keys.
    pub fn number_keys_taken(&self) -> bool {
        self.0.values().any(|p| p.open && p.number_keys)
    }

    pub fn is_open(&self, name: &str) -> bool {
        self.0.get(name).is_some_and(|p| p.open)
    }
}

/// Where a game UI copies its state into `MenuStatus` and `Panels`: in
/// `First`, in `Update` after its own systems, and early in `PostUpdate`;
/// the shell's `PostUpdate` readers run after it.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FrontEndSync;

/// Keep `Panels[name]` as `open` says of panel resource `R` (absent:
/// closed), in `FrontEndSync`; `after` is the panel's last system in
/// `Update`.
pub fn sync_panel<R: Resource, M>(
    app: &mut App,
    name: &'static str,
    number_keys: bool,
    open: fn(&R) -> bool,
    after: impl bevy::ecs::schedule::IntoSystemSet<M>,
) {
    let sync = move |panel: Option<Res<R>>, mut panels: ResMut<Panels>| {
        let now = Panel {
            open: panel.is_some_and(|p| open(&p)),
            number_keys,
        };
        if panels.0.get(name) != Some(&now) {
            panels.0.insert(name, now);
        }
    };
    app.init_resource::<Panels>()
        .add_systems(First, sync.in_set(FrontEndSync))
        .add_systems(Update, sync.in_set(FrontEndSync).after(after))
        .add_systems(PostUpdate, sync.in_set(FrontEndSync));
}
