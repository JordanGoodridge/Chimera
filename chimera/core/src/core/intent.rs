use bevy::prelude::*;

/// What a character wants to do this tick. Written by exactly one intent
/// source (local input, a bot brain, later the network) and read by the
/// Movement and Weapon slots. Buttons are level-based ("held"), not edges:
/// implementations that need edges (e.g. no auto-jump) track the previous
/// tick themselves.
#[derive(Component, Reflect, Default, Clone, Debug)]
#[reflect(Component)]
pub struct Intent {
    /// x = right, y = forward, each in [-1, 1]. Relative to `yaw`.
    pub move_axis: Vec2,
    /// Both keys of an axis held at once (x: left and right, y: forward
    /// and back): `move_axis` is 0 there, but a movement that tells this
    /// from no key (Combat Arms: still "moving", no braking) reads it.
    pub opposed: BVec2,
    /// Absolute look angles in radians. Yaw 0 looks down -Z; pitch up is +.
    pub yaw: f32,
    pub pitch: f32,
    pub jump: bool,
    pub crouch: bool,
    pub sprint: bool,
    /// The walk key (Source's +speed). Shift sets it along with `sprint`;
    /// each movement reads the one it has. `+walk` and `+sprint` set one
    /// each (a game whose walk and sprint are two keys binds those).
    pub walk: bool,
    pub fire: bool,
    /// Secondary attack (Source attack2: stab, zoom, burst toggle).
    pub secondary: bool,
    pub reload: bool,
    /// A weapon pick held this tick; the weapon switches when it changes.
    /// A slot key is the slot (0 = first slot: its first weapon, or the
    /// next one in it after the held one); a pick of one weapon in a slot
    /// (the HUD's weapon selection: `invnext`, the second grenade) is
    /// `Intent::pick` (see `Intent::picked`).
    pub select: Option<u8>,
    /// Switch to the previously held weapon (Source `lastinv`).
    pub last_weapon: bool,
    /// The use key (Source `+use`, E): doors, buttons. Held level; the
    /// logic layer acts on presses.
    pub use_key: bool,
    /// Named actions held this tick (`ActionRegistry`): bit `i` is the
    /// action registered `i`th (by name order). A game's own buttons
    /// (`throw`, `ability`, `emote`) ride here beside the fields above,
    /// which stay as Chimera's own names for the common ones.
    pub actions: u32,
    /// This tick's command number (Source's usercmd `command_number`):
    /// shared randoms (shot spread, recoil) are seeded from it. Stamped
    /// every tick after the rules (`SimSet::Commands`, `number_commands`);
    /// intent sources needn't set it.
    pub command: u32,
}

/// Weapon slots a pick can name (`Intent::select`).
pub const PICK_SLOTS: u8 = 10;
/// Weapons per slot a pick can name.
pub const PICK_DEPTH: u8 = 8;

/// Most named actions a game set can register (`Intent::actions` is a
/// `u32`).
pub const MAX_ACTIONS: usize = 32;

/// The named actions of this app (`ActionAppExt::intent_action`): a game
/// or minigame registers `throw` and binds `+throw` (the client reads
/// the keys bound to it, `chimera_client::binds`); the bit rides in
/// `Intent::actions` and the network command. Kept sorted by name, so
/// the server and its clients agree on the bits whatever order the games
/// registered them in (as `PredictedComponents` does).
#[derive(Resource, Default, Clone, Debug, PartialEq)]
pub struct ActionRegistry {
    names: Vec<String>,
}

impl ActionRegistry {
    /// Add an action (idempotent; names are lower-case). Returns false
    /// when the registry is full.
    pub fn register(&mut self, name: &str) -> bool {
        let name = name.trim().to_lowercase();
        match self.names.binary_search(&name) {
            Ok(_) => true,
            Err(_) if self.names.len() >= MAX_ACTIONS => false,
            Err(at) => {
                self.names.insert(at, name);
                true
            }
        }
    }

    /// The registered names, in bit order.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// An action's bit in `Intent::actions` (0: not registered).
    pub fn bit(&self, name: &str) -> u32 {
        let name = name.trim().to_lowercase();
        self.names.binary_search(&name).map_or(0, |i| 1 << i)
    }
}

/// Registering actions from a plugin.
pub trait ActionAppExt {
    /// Register the named action `name` (see `ActionRegistry`).
    fn intent_action(&mut self, name: &str) -> &mut Self;
}

impl ActionAppExt for App {
    fn intent_action(&mut self, name: &str) -> &mut Self {
        if !self.world_mut().get_resource_or_insert_with(ActionRegistry::default).register(name) {
            warn!("action {name}: no room (at most {MAX_ACTIONS} named actions)");
        }
        self
    }
}

impl Intent {
    /// Whether the named action `name` is held.
    pub fn action(&self, registry: &ActionRegistry, name: &str) -> bool {
        self.actions & registry.bit(name) != 0
    }

    /// The `select` value picking the `nth` weapon (from 0, in inventory
    /// order) in `slot`.
    pub fn pick(slot: u8, nth: u8) -> u8 {
        slot + PICK_SLOTS * (nth.min(PICK_DEPTH - 1) + 1)
    }

    /// A `select` value as its slot and, for a pick, the weapon's place in
    /// the slot (None: a slot key).
    pub fn picked(select: u8) -> (u8, Option<u8>) {
        let (slot, n) = (select % PICK_SLOTS, select / PICK_SLOTS);
        (slot, n.checked_sub(1))
    }

    pub fn look_rotation(&self) -> Quat {
        Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0)
    }

    pub fn yaw_rotation(&self) -> Quat {
        Quat::from_rotation_y(self.yaw)
    }
}

/// A character's command buttons as Source numbers them (the `IN_*` bits
/// map entities read and take away: game_ui, player_speedmod).
pub mod buttons {
    pub const ATTACK: u32 = 1;
    pub const JUMP: u32 = 2;
    pub const DUCK: u32 = 4;
    pub const FORWARD: u32 = 8;
    pub const BACK: u32 = 16;
    pub const USE: u32 = 32;
    pub const MOVELEFT: u32 = 512;
    pub const MOVERIGHT: u32 = 1024;
    pub const ATTACK2: u32 = 2048;
    pub const RELOAD: u32 = 8192;
    pub const SPEED: u32 = 131072;
    pub const ZOOM: u32 = 524288;

    /// The buttons an intent holds.
    pub fn of(i: &super::Intent) -> u32 {
        let mut b = 0;
        for (on, bit) in [
            (i.fire, ATTACK),
            (i.jump, JUMP),
            (i.crouch, DUCK),
            (i.move_axis.y > 0.0, FORWARD),
            (i.move_axis.y < 0.0, BACK),
            (i.use_key, USE),
            (i.move_axis.x < 0.0, MOVELEFT),
            (i.move_axis.x > 0.0, MOVERIGHT),
            (i.secondary, ATTACK2),
            (i.reload, RELOAD),
            (i.walk || i.sprint, SPEED),
        ] {
            if on {
                b |= bit;
            }
        }
        b
    }
}
