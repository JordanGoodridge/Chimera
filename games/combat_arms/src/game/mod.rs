//! Combat Arms' parts, one module per subsystem (each its own plugin, so
//! parallel implementation work never edits the same file), and what the
//! game tells the shell about itself (`INFO`). Wave 0: placeholders on
//! Chimera's pieces, each module saying which spec will define it
//! (specs/combat_arms/<name>.md, the names docs/plans/active/combat-arms.md
//! gives them):
//!
//! - `movement`: the movement slot, `combat_arms:movement` (movement.md);
//! - `weapons`: firing (the weapon frame on placeholder data);
//! - `inventory`: classes, switching, the loadout, the knife, pickup and
//!   drop (inventory.md, implemented);
//! - `grenades`: thrown weapons (none yet);
//! - `attachments`: weapon attachments (none yet);
//! - `hud`: the weapon data the HUD reads (`combat_arms:hud`: crosshair
//!   fields and the weapon panel's class; hud_crosshair.md);
//! - `data`: Combat Arms' data files (games/combat_arms/data/) and, later,
//!   the install's attribute files;
//! - `mount`: the install's `.rez` archives as a mount (`INFO.open`);
//! - `rules`: game modes as rule modules.
//!
//! `CombatArmsPlugins` adds them all (after `crate::SimPlugins`); the
//! HUD is the front end's (`ui::hud`).

pub mod attachments;
pub mod data;
pub mod grenades;
pub mod hud;
pub mod inventory;
pub mod mount;
pub mod movement;
pub mod rules;
pub mod weapons;

use bevy::{app::PluginGroupBuilder, prelude::*};

use crate::game_registry::{GameInfo, RegisterGame};

/// The game's id: maps are `combat_arms:<name>`, weapons
/// `combat_arms:<weapon>`, its install's key in chimera.local.toml.
pub const GAME: &str = "combat_arms";

/// The game's tick: the movement clock's 15 ms (specs/combat_arms/
/// movement.md, "Tick clock"; the engine's default movement rate).
pub const TICK_INTERVAL: f64 = 0.015;

/// What the shell needs of Combat Arms (`game_registry`). `open` mounts
/// the install's archives (`mount`); nothing reads from it yet (the
/// `data` module will read the install's attribute files through it),
/// there is no map cycle or map import, and the binds are the
/// table below.
pub static INFO: GameInfo = GameInfo {
    id: GAME,
    short_name: "CA",
    // PLACEHOLDER: no file of the install is read yet.
    content_dir: "",
    tick_interval: TICK_INTERVAL,
    open: mount::open,
    map_cycle: |_| Vec::new(),
    import_map: |_| Err("Combat Arms maps aren't loaded yet".into()),
    // No such file in an install: the built-in table is used.
    default_binds_cfg: "chimera_placeholder_binds.cfg",
    default_binds: DEFAULT_BINDS,
    // The defaults never changed.
    old_default_binds: &[],
    movement_cvars: &[],
};

/// Combat Arms' default binds. Weapons, use and sprint are the default
/// profile's (specs/combat_arms/inventory.md 2: class keys 1-5, Q the
/// previous weapon, fire, zoom/knife secondary, R, E, left Shift; nothing
/// on the wheel, 5; key 6, the objective class, waits for a sixth slot
/// command in the shell); walking, jumping and crouching are PLACEHOLDER
/// until movement.md's.
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
            .add(hud::CaHudDataPlugin)
            // Before `weapons`: its hooks must be there when the data
            // loads.
            .add(inventory::CaInventoryPlugin)
            .add(weapons::CaWeaponsPlugin)
            .add(grenades::CaGrenadesPlugin)
            .add(attachments::CaAttachmentsPlugin)
            .add(rules::CaRulesPlugin)
    }
}
