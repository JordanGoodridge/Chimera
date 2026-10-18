//! Combat Arms' parts, one module per subsystem (each its own plugin, so
//! parallel implementation work never edits the same file), and what the
//! game tells the shell about itself (`INFO`). Wave 0: placeholders on
//! Chimera's pieces, each module saying which spec will define it
//! (specs/combat_arms/<name>.md, the names docs/plans/active/combat-arms.md
//! gives them):
//!
//! - `movement`: the movement slot (Chimera's placeholder walking now);
//! - `weapons`: firing (the weapon frame on placeholder data);
//! - `inventory`: slots, switching, what a player starts with, the knife;
//! - `grenades`: thrown weapons (none yet);
//! - `attachments`: weapon attachments (none yet);
//! - `data`: Combat Arms' data files (games/combat_arms/data/) and, later,
//!   the install's attribute files;
//! - `rules`: game modes as rule modules.
//!
//! `CombatArmsPlugins` adds them all (after `crate::SimPlugins`); the
//! HUD is the front end's (`ui::hud`).

pub mod attachments;
pub mod data;
pub mod grenades;
pub mod inventory;
pub mod movement;
pub mod rules;
pub mod weapons;

use bevy::{app::PluginGroupBuilder, prelude::*};

use crate::game_registry::{GameInfo, RegisterGame};

/// The game's id: maps are `combat_arms:<name>`, weapons
/// `combat_arms:<weapon>`, its install's key in mashup.local.toml.
pub const GAME: &str = "combat_arms";

/// The server tick until a spec gives Combat Arms' (PLACEHOLDER: the
/// engine's default, `core::DEFAULT_TICK_HZ`).
pub const TICK_INTERVAL: f64 = 1.0 / crate::core::DEFAULT_TICK_HZ;

/// What the shell needs of Combat Arms (`game_registry`). Nothing reads
/// the install yet: `open` finds no mount (the `data` module will read the
/// install's attribute files through one), there is no map cycle or map
/// import, and the binds are the placeholder table below.
pub static INFO: GameInfo = GameInfo {
    id: GAME,
    short_name: "CA",
    // PLACEHOLDER: no file of the install is read yet.
    content_dir: "",
    tick_interval: TICK_INTERVAL,
    open: |_| Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "Combat Arms' install isn't mounted yet")),
    map_cycle: |_| Vec::new(),
    import_map: |_| Err("Combat Arms maps aren't loaded yet".into()),
    // No such file in an install: the built-in table is used.
    default_binds_cfg: "mashup_placeholder_binds.cfg",
    default_binds: DEFAULT_BINDS,
    movement_cvars: &[],
};

/// PLACEHOLDER binds (not Combat Arms' own: its defaults come with its
/// spec): enough to walk, look, fire, reload and switch weapons.
pub const DEFAULT_BINDS: &[(&str, &str)] = &[
    ("w", "+forward"),
    ("s", "+back"),
    ("a", "+moveleft"),
    ("d", "+moveright"),
    ("space", "+jump"),
    ("ctrl", "+duck"),
    ("shift", "+speed"),
    ("mouse1", "+attack"),
    ("mouse2", "+attack2"),
    ("r", "+reload"),
    ("e", "+use"),
    ("q", "lastinv"),
    ("1", "slot1"),
    ("2", "slot2"),
    ("3", "slot3"),
    ("4", "slot4"),
    ("5", "slot5"),
    ("mwheelup", "invprev"),
    ("mwheeldown", "invnext"),
];

/// Combat Arms' parts, after `crate::SimPlugins`: the game registered
/// (`INFO`), then each subsystem's plugin.
pub struct CombatArmsPlugins;

impl PluginGroup for CombatArmsPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(RegisterGame(&INFO))
            .add(data::CaDataPlugin)
            .add(movement::CaMovementPlugin)
            .add(weapons::CaWeaponsPlugin)
            .add(inventory::CaInventoryPlugin)
            .add(grenades::CaGrenadesPlugin)
            .add(attachments::CaAttachmentsPlugin)
            .add(rules::CaRulesPlugin)
    }
}
