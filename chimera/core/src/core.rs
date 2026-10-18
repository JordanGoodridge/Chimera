//! Core vocabulary shared by every game plugin. Keep this small: a concept
//! moves in here only after two or three games need it (README, "rule of
//! three").
//!
//! `Intent` (with its command number, stamped each tick in `SimSet::Commands`:
//! the tick offset by the character's `Seed`), `Velocity`, `MovementState`
//! (with the hull, the moving solid stood on and whether it is on a ladder),
//! `BaseVelocity`, `EntityGravity`, `MovingSolid`, `Health`, `Damage`/`Died`
//! messages (damage applied after weapons, `Died` carries the killing `Damage`;
//! `DamageKind`: bullet, melee, blast...; `Damage::weapon` names a thrown
//! weapon for kill notices; `Damage::force` the push that came with it),
//! `MapDamage` (a map's damage to a player before armour and whole points),
//! `Explosion` (radius damage asked for by map logic or rules: exploding props,
//! the bomb; the weapon it counts as), `Blinded` (a flash's white: hold and
//! fade), `SightBlocker` (a sphere sight doesn't pass: smoke), `Damageable`
//! (takes `Damage` without `Health`: breakables), `Hitgroup`,
//! `Hitbox`/`Hitboxes`, physics layers (`RAGDOLL_LAYER`, also prop pieces;
//! `ITEM_LAYER` for loose items, which slide on their own copy of the
//! displacement mesh with its internal edges fixed (`ITEM_GROUND_LAYER`);
//! gameplay queries mask with `SOLID_LAYERS`, characters' bodies and movement
//! with `CHARACTER_FILTER`, which leaves loose items out), `MaxSpeed`
//! (equipment speed cap), `God`, `Team`, `Seed` (a character's own dice seed
//! for rolls that must repeat run to run: bots' choices and route noise,
//! buying, bullet spread; never entity ids, which shift with every spawn,
//! resources included), `Radio` (a team radio call: sender and the game's
//! command name), `SpawnPoint`, `LocalPlayer`, `UserInfo` (a player's own
//! settings the simulation reads, Source's userinfo: the local player's from
//! its console, a remote player's from its client), `PlayerIdMode`
//! (`mp_playerid`), `SimTick`, `SimClock` (the step's time and length, Source's
//! curtime: what selection, movement and the weapon frame read instead of
//! `Time`, so a replayed command sees its own time), `set_tick_length` (a new
//! map's tick: the fixed clock keeps running except on a client, so its time
//! stays with the frame's `Time<Virtual>`), `NetRole` (standalone, server or
//! client) and the `authoritative` run condition on server-only systems (bots,
//! rules, damage, logic, objectives), the predicted path for network play
//! (docs/plans/active/multiplayer.md): `Predict` schedules (`Select`,
//! `Movement`, `Weapons`, each run from its place in `FixedUpdate` by
//! `run_predicted`, or all in order by `predict` for a replayed command),
//! `FirstTimePredicted` (side effects only the first time a command runs),
//! `PredictedComponents` (components registered with
//! `PredictedAppExt::predicted`, saved and restored per entity; `predicted_net`
//! and `predicted_codec` parts also go to the client predicting them as one
//! blob), `RoundRestarts` (counted up by the rules at each new round; the logic
//! and map layers put the map's entities back), `FreezeTime` (the rules hold
//! everyone still: a round's freeze time), the `SimSet` tick ordering (`Rules`,
//! `Commands`, `Movement`, `Weapons`), `keep_moved_positions` (before each
//! physics step a character's physics position is its movement's, to the bit:
//! the physics would undo steps below its 0.01 mm tolerance while the body is
//! awake); the collision world maps share with movement:
//! `MapBrush`/`MapBrushes` (brush planes for exact swept-box movement, ladder
//! flag), `MapBrushTree` (the BSP tree over the world's brushes: which of two
//! faces hit at the same distance a trace reports), `MapBrushCollider`,
//! `MapTerrain` (terrain triangles as thin brushes, with a grid) and
//! `MapTerrainCollider`, `MapWater` volumes; `Equip` (a map gives a character
//! items), `DamageFilter` (damage kinds a character ignores: a map's no-fall
//! filter); what map entities do to a character's controls (`MapControls`:
//! player_speedmod's movement clock (`time_scale`, read by the movement) and
//! taken buttons, game_ui's "at controls", a camera's freeze, weapons put away,
//! invulnerable; applied to its `Intent` by `apply_map_controls` after the
//! rules, on the server and on the client predicting it: a predicted part; a
//! respawn resets the clock and the camera), `MapView` (the map camera a
//! character views through), `buttons` (Source's command button bits),
//! `ScoreChange` (game_score, applied by the rules); the players' standing
//! every game has: `Score`, `Dead`, `MapLoads`, `Bot` (a computer player),
//! `Npc` (a character that isn't a player: hostages), `ScoreMark` (what a
//! carried item marks on the scoreboard); `BuildVersion` (the app's stamp);
//! `DEFAULT_TICK_HZ`; what the client shell reads of a game's rules without
//! naming them: `JoinChoice` (how a player comes into a game), `MapCycle`,
//! `FadeToBlack` and the `NewGame` hook (`new_game`).
//!
//! `BrushGrid`: a grid over brushes' bounds, the Source movement's broad
//! phase over `MapBrushes` (identical results to testing every brush).

use bevy::prelude::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// The version text's format (`format_version`); the build's own
/// `VERSION` is stamped where the app is built (`mashup::core::VERSION`),
/// so a commit doesn't recompile this crate.
pub mod version;

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
    /// Absolute look angles in radians. Yaw 0 looks down -Z; pitch up is +.
    pub yaw: f32,
    pub pitch: f32,
    pub jump: bool,
    pub crouch: bool,
    pub sprint: bool,
    /// The walk key (Source's +speed). Shift sets it along with `sprint`;
    /// each movement reads the one it has.
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

impl Intent {
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

/// Character velocity in meters per second, owned by the active Movement
/// implementation.
#[derive(Component, Reflect, Default, Clone, Copy, Debug, Deref, DerefMut, Serialize, Deserialize)]
#[reflect(Component)]
pub struct Velocity(pub Vec3);

/// Published by the active Movement implementation every tick. Other slots
/// (weapons, HUD, animation) read this instead of depending on a specific
/// movement implementation.
#[derive(Component, Reflect, Default, Clone, Debug, Serialize, Deserialize)]
#[reflect(Component)]
pub struct MovementState {
    pub on_ground: bool,
    pub crouching: bool,
    pub sprinting: bool,
    /// Eye position relative to the character's origin, in meters.
    pub eye_offset: Vec3,
    /// The collision box relative to the character's origin (engine
    /// space, meters), when the movement has one (Source hulls); zero
    /// when it doesn't (the logic layer then uses the collider's bounds).
    pub hull_min: Vec3,
    pub hull_max: Vec3,
    /// The moving solid (`MovingSolid`) the character stands on, if any.
    /// Not sent to a predicting client (entity ids differ).
    #[serde(skip)]
    pub ground: Option<Entity>,
    /// Climbing a ladder (movements that have them).
    pub on_ladder: bool,
    /// The eye's roll from a view punch (radians; positive tilts the view
    /// clockwise, Source's roll): hard landings kick it.
    pub view_roll: f32,
}

/// A velocity that moves a character without being part of its own
/// velocity (Source base velocity: push triggers, leaving a moving
/// platform), engine space, m/s. Movement implementations that model it
/// read and consume it (specs/source/triggers.md, trigger_push).
#[derive(Component, Reflect, Default, Clone, Copy, Debug, Serialize, Deserialize)]
#[reflect(Component)]
pub struct BaseVelocity {
    pub velocity: Vec3,
    /// Set this tick by a push (Source: FL_BASEVELOCITY); the movement
    /// clears it at the start of its next tick.
    pub touched: bool,
    /// Taken off the ground by game logic (push, teleport) this tick: the
    /// movement must not snap it back down before it moves.
    pub unground: bool,
}

/// Per-character gravity multiplier (Source entity gravity, set by
/// trigger_gravity); 0 means normal.
#[derive(Component, Reflect, Clone, Copy, Debug)]
#[reflect(Component)]
pub struct EntityGravity(pub f32);

impl Default for EntityGravity {
    fn default() -> Self {
        Self(1.0)
    }
}

/// A solid that moves (doors, platforms): its brushes where they are this
/// tick (engine space) and its velocity (m/s), for movement that sweeps
/// brushes exactly. Kept up to date by whoever moves it (the logic layer).
#[derive(Component, Clone, Debug, Default)]
pub struct MovingSolid {
    pub brushes: Vec<MapBrush>,
    pub velocity: Vec3,
    /// Solid to characters right now (doors with "Passable", disabled
    /// func_brush: false).
    pub solid: bool,
}

// Serde: replicated to clients (`net`).
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[reflect(Component)]
pub struct Health {
    /// 1.0 is a standard player's full health (README normalization rules).
    pub current: f32,
    pub max: f32,
}

impl Default for Health {
    fn default() -> Self {
        Self { current: 1.0, max: 1.0 }
    }
}

#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[reflect(Component)]
pub struct Team(pub u8);

/// A character's own dice seed, for rolls that should repeat run to run
/// (bots' route noise and choices, buying, bullet spread). Entity ids
/// won't do: every spawn shifts them, resources included (Bevy 0.19
/// resources are entities). Bots get one from their number and team
/// (`bot::add_bot`); without one, systems fall back to the entity id.
#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialEq, Eq)]
#[reflect(Component)]
pub struct Seed(pub u64);

impl Seed {
    /// The seed of `entity`, or one made from its id without a `Seed`.
    pub fn of(seed: Option<&Seed>, entity: Entity) -> u64 {
        seed.map_or(entity.to_bits(), |s| s.0)
    }
}

/// Where on a body a hit landed (Source hitgroups).
#[derive(Reflect, Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Hitgroup {
    #[default]
    Generic,
    Head,
    Chest,
    Stomach,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
}

/// Damage to apply: normalized like `Health` (1.0 = a standard player's
/// full health).
#[derive(Message, Clone, Debug)]
pub struct Damage {
    pub target: Entity,
    pub attacker: Option<Entity>,
    pub amount: f32,
    pub point: Vec3,
    /// Direction the damage travelled (unit).
    pub dir: Vec3,
    pub hitgroup: Hitgroup,
    /// How it was dealt (breakables scale damage by it).
    pub kind: DamageKind,
    /// The weapon that dealt it when that isn't what the attacker holds
    /// now (a thrown grenade), for kill notices; None: the held one.
    pub weapon: Option<&'static str>,
    /// The push that came with it (kg·m/s, engine space), already given
    /// to a moving body; zero for none. Props read it (a frozen prop
    /// that a strong enough push unfreezes).
    pub force: Vec3,
}

/// Damage a map deals a player (trigger_hurt) before armour and the
/// player's whole points: the weapon layer splits it with the armour worn
/// when `armour` (CS:S takes armour for every damage type but falls and
/// drowning: measured on trigger_hurt, specs/source/triggers.md question
/// 4), then health takes whole points (2.5 is 2, nothing carried). Without
/// a weapon layer (`MapDamageArmor` absent) only the truncation applies
/// (`map_damage_lands`).
#[derive(Message, Clone, Debug)]
pub struct MapDamage {
    pub damage: Damage,
    pub armour: bool,
}

/// The weapon layer turns `MapDamage` into `Damage` itself (with armour).
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct MapDamageArmor;

/// `amount` (normalized: 1.0 is 100 points) down to whole points.
pub fn whole_points(amount: f32) -> f32 {
    (amount * 100.0 + 1e-3).floor() / 100.0
}

/// `MapDamage` without a weapon layer: whole points, no armour.
pub fn map_damage_lands(mut map: MessageReader<MapDamage>, mut damage: MessageWriter<Damage>) {
    for m in map.read() {
        let mut d = m.damage.clone();
        d.amount = whole_points(d.amount);
        if d.amount > 0.0 {
            damage.write(d);
        }
    }
}

impl Damage {
    /// A suicide (the `kill` command: generic damage from the target
    /// itself), which CS:S carries out even in noclip (`Noclip`).
    pub fn is_suicide(&self) -> bool {
        self.attacker == Some(self.target) && self.kind == DamageKind::Generic && self.weapon.is_none()
    }
}

/// An explosion to apply (radius damage and pushes, as a grenade's blast
/// with this magnitude and radius): from map logic (exploding props).
/// `damage` is normalized like `Damage::amount`, `radius` in meters.
#[derive(Message, Clone, Debug)]
pub struct Explosion {
    pub origin: Vec3,
    pub damage: f32,
    pub radius: f32,
    /// Who the damage counts for (kill notices, friendly fire).
    pub attacker: Option<Entity>,
    /// What exploded (it takes none of the damage).
    pub inflictor: Option<Entity>,
    /// A sound entry to play at the origin, if any.
    pub sound: Option<String>,
    /// The weapon kills count as (kill notices); None: a map explosion
    /// (`weapon::grenade::EXPLOSION_WEAPON`).
    pub weapon: Option<&'static str>,
    /// Its scorch mark goes at the next round restart (the bomb's, as in
    /// CS:S; `map::PlaceDecal::round`); others stay with the map.
    pub round_scorch: bool,
    /// Falloff override: a bell of this σ (m) instead of the explosion
    /// rule's (`weapon::grenade::Blast::bell_sigma`); None: the rule's,
    /// its σ scaled with this radius.
    pub bell_sigma: Option<f32>,
    /// Armour override (`Blast::armor_ratio`); None: the rule's.
    pub armor_ratio: Option<f32>,
    /// Nothing between stops or shields it (`Blast::through_walls`).
    pub through_walls: bool,
}

/// How damage was dealt (Source damage types, as far as anything here
/// tells them apart).
#[derive(Reflect, Default, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DamageKind {
    #[default]
    Generic,
    /// Hitscan shots.
    Bullet,
    /// Melee swings (the knife).
    Melee,
    Blast,
    /// Crushed by a mover.
    Crush,
    Fall,
    /// Fire (Source's burn type): env_fire and entity flames.
    Burn,
    /// Drowning: head under water too long (`games::cs_source::drown`).
    Drown,
}

/// Give a character items a map names (`weapon_ak47`, `item_kevlar`, with
/// counts), after taking all its weapons when `strip`: the logic layer's
/// game_player_equip and player_weaponstrip; the weapon layer gives them.
#[derive(Message, Clone, Debug, PartialEq)]
pub struct Equip {
    pub target: Entity,
    pub items: Vec<(String, u32)>,
    pub strip: bool,
}

/// Damage kinds a character takes none of (a map's damage filter on it:
/// bhop and surf maps turn fall damage off this way).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct DamageFilter {
    pub blocked: Vec<DamageKind>,
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

/// What map entities do to a character's controls (specs/source/
/// game_entities.md 1-2, viewcontrol_and_templates.md 1): player_speedmod's
/// movement clock and taken buttons, game_ui's "at controls", a camera's
/// freeze. Applied to its `Intent` each tick before movement
/// (`apply_map_controls`, on the server and on the client predicting it:
/// a predicted part). Absent: nothing changed.
#[derive(Component, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapControls {
    /// Its movement runs this many ticks per tick (player_speedmod's
    /// lagged movement; 0 holds it still). Reset to 1 when it spawns.
    pub time_scale: f32,
    /// Buttons taken away (`buttons` bits), per entity that took them
    /// (a key the logic gives each).
    pub disabled: Vec<(u32, u32)>,
    /// Its movement inputs are zeroed (game_ui "Freeze Player").
    pub at_controls: bool,
    /// Frozen (point_viewcontrol "Freeze Player"): no movement, buttons
    /// or impulse; the view held at these angles (yaw, pitch, radians).
    pub frozen: Option<(f32, f32)>,
    /// Its weapons put away (bits: 1 player_speedmod, 2 game_ui, 4 a
    /// camera): no firing, no view model.
    pub weapon_hidden: u8,
    /// The HUD hidden (player_speedmod "Suppress HUD").
    pub hud_hidden: bool,
    /// Takes no damage (viewing through a point_viewcontrol).
    pub invulnerable: bool,
    /// The buttons of this tick's command after the above (what game_ui
    /// reads): `buttons` bits.
    pub buttons: u32,
    /// Alive last tick (a respawn resets the movement clock).
    pub alive: bool,
    /// Parented to a map entity (SetParent: a kart's seat): the map logic
    /// carries it, its own movement doesn't run, and it neither pushes
    /// nor collides with physics bodies (its physics shadow is off).
    pub parented: bool,
}

impl Default for MapControls {
    fn default() -> Self {
        Self {
            time_scale: 1.0,
            disabled: Vec::new(),
            at_controls: false,
            frozen: None,
            weapon_hidden: 0,
            hud_hidden: false,
            invulnerable: false,
            buttons: 0,
            alive: true,
            parented: false,
        }
    }
}

impl MapControls {
    /// All buttons taken away.
    pub fn disabled_buttons(&self) -> u32 {
        self.disabled.iter().fold(0, |a, (_, b)| a | b)
    }
}

/// Apply each character's `MapControls` to its intent: taken buttons
/// released, movement zeroed at controls, everything but the held view
/// zeroed when frozen, no firing with weapons put away; the buttons left
/// recorded for the logic. A respawn resets the movement clock and brings
/// the HUD back; the taken buttons stay (CS:S, measured 2026-10-17 with a
/// bot: lagged movement 1 and the hide-all HUD bit cleared after a death
/// and respawn, jumping still taken until the speedmod gave it back;
/// game_entities.md question 3); a camera's view and freeze go too (our
/// choice, viewcontrol_and_templates.md open question 2).
pub fn apply_map_controls(
    mut q: Query<(Entity, &mut Intent, &mut MapControls, Option<&Health>, Has<MapView>)>,
    mut commands: Commands,
) {
    for (e, mut intent, mut c, health, viewing) in &mut q {
        let alive = health.is_none_or(|h| h.current > 0.0);
        if alive && !c.alive {
            c.time_scale = 1.0;
            c.hud_hidden = false;
            c.frozen = None;
            c.invulnerable = false;
            c.weapon_hidden &= !4;
            if viewing {
                commands.entity(e).remove::<MapView>();
            }
        }
        if c.alive != alive {
            c.alive = alive;
        }
        let off = c.disabled_buttons();
        let i = &mut *intent;
        if off & buttons::JUMP != 0 {
            i.jump = false;
        }
        if off & buttons::DUCK != 0 {
            i.crouch = false;
        }
        if off & buttons::USE != 0 {
            i.use_key = false;
        }
        if off & buttons::SPEED != 0 {
            i.walk = false;
            i.sprint = false;
        }
        if off & buttons::ATTACK != 0 {
            i.fire = false;
        }
        if off & buttons::ATTACK2 != 0 {
            i.secondary = false;
        }
        if let Some((yaw, pitch)) = c.frozen {
            let (command, select) = (i.command, i.select);
            *i = Intent {
                yaw,
                pitch,
                command,
                select,
                ..default()
            };
        }
        let pressed = buttons::of(i);
        if c.at_controls {
            i.move_axis = Vec2::ZERO;
        }
        if c.weapon_hidden != 0 {
            i.fire = false;
            i.secondary = false;
            i.reload = false;
        }
        if c.buttons != pressed {
            c.buttons = pressed;
        }
    }
}

/// Where a character views the world from instead of its eye (a
/// point_viewcontrol camera; viewcontrol_and_templates.md 1.5): engine
/// space. The server's logic moves it; the client predicting the
/// character hears it with its own state.
#[derive(Component, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapView {
    pub origin: Vec3,
    pub rotation: Quat,
}

/// A map entity changed a character's score (game_score): kills, or with
/// `team` its team's score; `allow_negative` as the entity's flag 1
/// (specs/source/game_entities.md 4). The rules apply it.
#[derive(Message, Clone, Debug, PartialEq)]
pub struct ScoreChange {
    pub target: Entity,
    pub points: i32,
    pub team: bool,
    pub allow_negative: bool,
}

/// Takes `Damage` without having `Health`: something else (the logic
/// layer's breakables) reads the messages aimed at it. Weapons hit it as
/// a plain object (no hitgroups, no flesh sounds).
#[derive(Component, Reflect, Default, Clone, Copy, Debug)]
#[reflect(Component)]
pub struct Damageable;

/// An oriented box on a character's body that shots test (Source
/// hitboxes), in the character's local space: feet at the origin, Y up,
/// facing -Z (yaw 0), meters.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub struct Hitbox {
    pub center: Vec3,
    pub half: Vec3,
    pub rotation: Quat,
    pub group: Hitgroup,
}

impl Hitbox {
    /// Where a ray (local space) enters the box, as a distance along `dir`
    /// (unit), if it does.
    pub fn ray_entry(&self, origin: Vec3, dir: Vec3) -> Option<f32> {
        self.ray_span(origin, dir).map(|(enter, _)| enter)
    }

    /// Where a ray (local space) enters and leaves the box, as distances
    /// along `dir` (unit; the entry clamped at 0), if it crosses it.
    pub fn ray_span(&self, origin: Vec3, dir: Vec3) -> Option<(f32, f32)> {
        let inv = self.rotation.inverse();
        let o = inv * (origin - self.center);
        let d = inv * dir;
        let (mut enter, mut leave) = (f32::MIN, f32::MAX);
        for i in 0..3 {
            if d[i].abs() < 1e-9 {
                if o[i].abs() > self.half[i] {
                    return None;
                }
                continue;
            }
            let (a, b) = ((-self.half[i] - o[i]) / d[i], (self.half[i] - o[i]) / d[i]);
            enter = enter.max(a.min(b));
            leave = leave.min(a.max(b));
        }
        (enter <= leave && leave >= 0.0).then_some((enter.max(0.0), leave))
    }
}

/// A character's hitboxes (see `Hitbox`).
#[derive(Component, Clone, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct Hitboxes(pub Vec<Hitbox>);

/// `chimera_noblock` (ours, a server setting sent to clients, off by
/// default as CS:S has nothing like it): 0 players block each other (CS:S),
/// 1 teammates pass through each other, 2 nobody blocks anybody.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NoBlock(pub u8);

impl NoBlock {
    /// Whether two characters on these teams block each other (None: not a
    /// player on a team, e.g. a hostage: always blocks).
    pub fn blocks(self, a: Option<u8>, b: Option<u8>) -> bool {
        match (self.0, a, b) {
            (_, None, _) | (_, _, None) | (0, ..) => true,
            (1, a, b) => a != b,
            _ => false,
        }
    }
}

/// Takes no damage (the `god` command).
#[derive(Component, Reflect, Default, Clone, Copy, Debug)]
#[reflect(Component)]
pub struct God;

/// Flies through walls: the noclip movement's marker (`chimera_sim`'s
/// `movement::noclip`; Source's noclip move type). Not `God`: CS:S still
/// traces a noclipping player's hitboxes (bullets hit it and go on through
/// as through any body) and hands its damage on, but it takes none, from
/// anything (bullets, blasts, trigger_hurt, the world), nor loses armour;
/// falls count again once it walks (specs/cs_source/movement.md "Noclip
/// and spawning"). `DamageRules` refuses it.
#[derive(Component, Reflect, Default, Clone, Copy, Debug)]
#[reflect(Component)]
pub struct Noclip;

/// A character says a radio command to its team (Counter-Strike's radio):
/// `command` is the game's console name for it (`coverme`, `enemyspot`,
/// ...; `fireinhole` for a thrown grenade). The client resolves it through
/// the map's `map::radio::RadioCommands` and plays it if it hears it.
#[derive(Message, Clone, Debug, PartialEq)]
pub struct Radio {
    pub sender: Entity,
    pub command: String,
}

/// Damage that reached a character's health (after the rules: not
/// refused, not under `God`): for the victim's HUD (the damage
/// indicators). A network client gets its own player's from the server
/// (`net::Hurt`), with `damage.attacker` None and `directed` saying
/// whether someone else dealt it.
#[derive(Message, Clone, Debug)]
pub struct DamageTaken {
    pub damage: Damage,
    /// Dealt by someone other than the victim (a shot, a grenade), so it
    /// has a side to come from.
    pub directed: bool,
}

/// Something's health reached zero.
#[derive(Message, Clone, Debug)]
pub struct Died {
    pub entity: Entity,
    pub attacker: Option<Entity>,
    /// The hit that killed it (direction and point push its ragdoll).
    pub damage: Damage,
}

/// Physics layer of ragdoll bodies. They collide only with the default
/// layer (the world, props), not with characters or each other, and
/// gameplay queries (movement, bullets, sight) leave them out with
/// `SOLID_LAYERS` (specs/cs_source/ragdolls.md 4: ragdolls are debris).
pub const RAGDOLL_LAYER: avian3d::prelude::LayerMask = avian3d::prelude::LayerMask(1 << 1);
/// Every layer but ragdolls, physics shadows (and the loose items' copy
/// of the terrain): the mask for gameplay spatial queries.
pub const SOLID_LAYERS: avian3d::prelude::LayerMask =
    avian3d::prelude::LayerMask(!(1 << 1) & !(1 << 3) & !SHADOW_LAYER.0 & !PHYSICS_CLIP_LAYER.0);
/// Physics layer of brushes solid to physics objects only (Source's
/// func_clip_vphysics, `map::physics_clip`): props and physics brushes
/// collide with them; characters, ragdolls, loose items and every
/// spatial query (`SOLID_LAYERS`, `NOT_SHADOW`) pass through.
pub const PHYSICS_CLIP_LAYER: avian3d::prelude::LayerMask = avian3d::prelude::LayerMask(1 << 5);
/// Physics layer of players' physics shadows (specs/cs_source/physics_props.md
/// 4.1, `map::prop_physics::PhysicsShadow`): invisible bodies that only
/// touch the physics props players push. No spatial query should see
/// them: use `SOLID_LAYERS`, or `NOT_SHADOW` for queries of everything.
pub const SHADOW_LAYER: avian3d::prelude::LayerMask = avian3d::prelude::LayerMask(1 << 4);
/// Every layer but physics shadows (and physics clips, which only
/// physics objects meet).
pub const NOT_SHADOW: avian3d::prelude::LayerMask = avian3d::prelude::LayerMask(!SHADOW_LAYER.0 & !PHYSICS_CLIP_LAYER.0);
/// Physics layer of loose items (dropped weapons): they collide with the
/// world, props and each other but never with characters (Source's
/// weapon collision group), so a weapon thrown from inside its dropper
/// isn't shoved out of them. Shots and other gameplay queries still see
/// them.
pub const ITEM_LAYER: avian3d::prelude::LayerMask = avian3d::prelude::LayerMask(1 << 2);
/// What a character's body collides with: solid things but loose items.
pub const CHARACTER_FILTER: avian3d::prelude::LayerMask = avian3d::prelude::LayerMask(SOLID_LAYERS.0 & !ITEM_LAYER.0);
/// The map's displacement surfaces as loose items collide with them (a
/// copy with its internal edges fixed, `map::spawn_map`); nothing else
/// touches or queries it.
pub const ITEM_GROUND_LAYER: avian3d::prelude::LayerMask = avian3d::prelude::LayerMask(1 << 3);

/// A speed cap the character's equipment imposes (e.g. the held weapon),
/// meters per second. Movement implementations that model it read it.
#[derive(Component, Reflect, Clone, Copy, Debug, Serialize, Deserialize)]
#[reflect(Component)]
pub struct MaxSpeed(pub f32);

/// A place a character can spawn, provided by the Map slot.
#[derive(Component, Reflect, Default, Clone, Copy, Debug)]
#[reflect(Component)]
pub struct SpawnPoint {
    pub team: Option<Team>,
}

/// The character controlled by this machine's keyboard and mouse.
#[derive(Component, Reflect, Default)]
#[reflect(Component)]
pub struct LocalPlayer;

/// A player's own settings the simulation reads (Source's userinfo: the
/// client cvars marked `console::Console::userinfo`, e.g.
/// `cl_autowepswitch`). The local player's come from its console; a
/// network server's remote players' from their clients (`net`, as `name`
/// is sent). Characters without one (bots) use each setting's default
/// behaviour, as its reader says.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
#[reflect(Component)]
pub struct UserInfo(pub std::collections::BTreeMap<String, String>);

impl UserInfo {
    /// A setting as a number (`Some(n != 0)`), None when unset or not a
    /// number.
    pub fn flag(&self, key: &str) -> Option<bool> {
        let v = self.0.get(key)?;
        v.trim().parse::<f32>().ok().map(|n| n != 0.0)
    }
}

/// A character a person plays from another machine (a network server's
/// client): not this machine's `LocalPlayer`, and not a computer player.
#[derive(Component, Reflect, Default, Clone, Copy, Debug)]
#[reflect(Component)]
pub struct RemotePlayer;

/// A computer player (a bot): the game's bot module puts its brain
/// beside it (CS:S's `bot::Bot` requires it). The network counts and
/// kicks bots by it (`net`'s `BotHooks`) without naming a game.
#[derive(Component, Reflect, Default, Clone, Copy, Debug)]
#[reflect(Component)]
pub struct Bot;

/// A character that isn't a player (CS:S's hostages): on no team's count,
/// not among a server's players, and replicated by its game rather than
/// as a player.
#[derive(Component, Reflect, Default, Clone, Copy, Debug)]
#[reflect(Component)]
pub struct Npc;

/// Kills and deaths (the scoreboard's).
#[derive(Component, Default, Clone, Copy, Debug, Reflect)]
#[reflect(Component)]
pub struct Score {
    pub kills: i32,
    pub deaths: u32,
}

/// Dead since this time (fixed-tick seconds).
#[derive(Component, Clone, Copy, Debug)]
pub struct Dead {
    pub since: f64,
}

/// Maps loaded so far (the rules' `new_game`): a map change, or the same
/// map loaded again, counts one. A network server notices a reload of
/// the same map by it (`net::maps`).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapLoads(pub u32);

/// Bits a carried item marks its carrier's scoreboard line with
/// (`net::score_flags`: CS:S's bomb sets `2`), ORed over everything the
/// character carries.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScoreMark(pub u8);

/// This build's version text (the app stamps it: `mashup::core::VERSION`),
/// for what reports it without being rebuilt by every commit (`net`'s
/// `version` command).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildVersion(pub &'static str);

/// The fixed tick rate until a game's map sets its own (the greybox, a
/// game without a tick: `game_registry::GameInfo::tick_interval`).
pub const DEFAULT_TICK_HZ: f64 = 64.0;

/// How a player comes into a game it starts or joins: CS:S's way, the
/// join menus (`Menus`: the server's message of the day, then the team
/// menu, then the class menu; until it picks it is `core::Unassigned`),
/// or straight onto a team, as tests and scripted runs want: `Auto` the
/// smaller team (counter-terrorists on a tie), `Team` that one (`Team(0)`
/// the spectators). The local player's (single player, a listen server's
/// host) and, for a network client, the one it asks the server for
/// (`net`). The game's interactive runs use `Menus`
/// (`client::Args::join_choice`); the default is `Auto`.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JoinChoice {
    Menus,
    #[default]
    Auto,
    Team(Team),
}

impl JoinChoice {
    /// As a userinfo value (`net`'s join handshake): `menus`, `auto`, or
    /// the `jointeam` number.
    pub fn key(self) -> String {
        match self {
            Self::Menus => "menus".into(),
            Self::Auto => "auto".into(),
            Self::Team(t) => (t.0 + 1).to_string(),
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim() {
            "menus" => Some(Self::Menus),
            "auto" | "0" => Some(Self::Auto),
            "1" => Some(Self::Team(Team(0))),
            "2" => Some(Self::Team(Team(1))),
            "3" => Some(Self::Team(Team(2))),
            _ => None,
        }
    }
}

/// The maps played in turn (a game's `mapcycle.txt`, by short name, as
/// `changelevel` takes them) and the one loaded now. Games and the client
/// fill it.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct MapCycle {
    pub maps: Vec<String>,
    pub current: String,
}

impl MapCycle {
    /// The map after the current one (wrapping; the first when the current
    /// one isn't in the cycle); None without a cycle.
    pub fn next(&self) -> Option<String> {
        let i = self.maps.iter().position(|m| m.eq_ignore_ascii_case(&self.current));
        let next = i.map_or(0, |i| (i + 1) % self.maps.len());
        self.maps.get(next).cloned()
    }
}

/// The dead's camera rules, the server's (replicated to clients; set by
/// the rules' cvars, CS:S's defaults measured on its dedicated server
/// 2026-10-15): `mp_forcecamera` (`force_camera`: 1 the dead on a team
/// watch only their team), `spec_freeze_time` (`freeze_hold`, seconds the
/// freeze cam's picture holds) and `spec_freeze_traveltime`
/// (`freeze_travel`, seconds its camera takes to reach the killer; at
/// least `FREEZE_TRAVEL_MIN`). Read by `client::spectate`.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct SpectatorRules {
    pub force_camera: u8,
    pub freeze_hold: f32,
    pub freeze_travel: f32,
}

/// `spec_freeze_traveltime` and `spec_freeze_time` defaults, seconds.
pub const FREEZE_TRAVEL: f32 = 0.7;
pub const FREEZE_HOLD: f32 = 5.0;
/// `spec_freeze_traveltime`'s minimum, seconds.
pub const FREEZE_TRAVEL_MIN: f32 = 0.01;

impl Default for SpectatorRules {
    fn default() -> Self {
        Self {
            force_camera: 1,
            freeze_hold: FREEZE_HOLD,
            freeze_travel: FREEZE_TRAVEL,
        }
    }
}

/// `mp_fadetoblack` (CS:S's default 0): the dead see black
/// (`client::senses`; set by the rules' `mp_fadetoblack`).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FadeToBlack(pub u8);

/// The rules' fresh game on a newly loaded map (CS:S's `rules::new_game`:
/// everyone respawned, rounds from the start), which the rules register so
/// what loads maps (the client's `map` command, the greybox) names none.
#[derive(Resource, Clone, Copy)]
pub struct NewGame(pub fn(&mut World));

/// Start a fresh game on the map just loaded (`NewGame`); nothing without
/// rules.
pub fn new_game(world: &mut World) {
    if let Some(hook) = world.get_resource::<NewGame>().copied() {
        (hook.0)(world);
    }
}

/// A remote player whose client is still loading the map (joining, or
/// after a map change): not in the game yet. It is dead, not solid and
/// at no health, so nothing hits it; the rules don't spawn it or count it
/// on its team until its client has the map (`rules::enter_game`), as
/// CS:S keeps connecting players out of the game.
#[derive(Component, Reflect, Default, Clone, Copy, Debug)]
#[reflect(Component)]
pub struct Connecting;

/// On CS:S's spectator team (`spectate`, `jointeam 1`): no team of its
/// own (`Team(0)`), dead, not solid, no health and no weapons, so it has
/// no body; the rules neither spawn it nor count it for a round's end
/// (`rules::join_spectators`). Its client watches others or flies free
/// (`client::spectate`). Replicated, so every client lists it.
#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[reflect(Component)]
pub struct Spectating;

/// In the game but on no team yet (CS:S's unassigned players): it has not
/// picked one in the team menu. Also `Spectating` (no body, watching,
/// counted on no team); `rules::join_team` or `rules::join_spectators`
/// ends it. Its client shows the join menus (the MOTD, then the team
/// menu: `games::cs_source::ui::join_menus`). Replicated.
#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[reflect(Component)]
pub struct Unassigned;

/// On a team, picked from the team menu after joining the game, but no
/// class (player model) chosen yet: still no body, neither spawned nor
/// counted for a round's end until `joinclass` (`rules::join_class`), as
/// CS:S waits for the class menu. Replicated.
#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[reflect(Component)]
pub struct ChoosingClass;

/// A player's class (CS:S's `joinclass` number: its player model): 1-4
/// the terrorists' (Phoenix Connexion, Elite Crew, Arctic Avengers,
/// Guerilla Warfare), 5-8 the counter-terrorists' (SEAL Team 6, GSG-9,
/// SAS, GIGN). Without one a character wears its team's first model.
/// Replicated; a game's loader draws it (`map::BodyName`).
#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[reflect(Component)]
pub struct PlayerClass(pub u8);

impl PlayerClass {
    /// Classes per team.
    pub const PER_TEAM: u8 = 4;

    /// The team (ours: 1 terrorists, 2 counter-terrorists) a class belongs to.
    pub fn team(self) -> Option<Team> {
        match self.0 {
            1..=4 => Some(Team(1)),
            5..=8 => Some(Team(2)),
            _ => None,
        }
    }
}

/// Number of fixed ticks simulated so far. Tests and tools use it to step
/// the simulation by exact ticks.
#[derive(Resource, Reflect, Default, Clone, Copy, Debug)]
#[reflect(Resource)]
pub struct SimTick(pub u64);

/// The clock of the simulation step running now: Source's `curtime`
/// (tickbase × tick interval) and the tick's length. Code that prediction
/// runs again (`Predict`: weapon selection, movement, the weapon frame)
/// reads time from here, never from `Time`, so a replayed command sees the
/// time it first ran at. Every fixed tick starts by copying `Time<Fixed>`
/// into it (the same values `Time` gives in `FixedUpdate`); a prediction
/// replay sets it per command.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq)]
pub struct SimClock {
    /// The tick (`SimTick`) this time belongs to.
    pub tick: u64,
    /// Seconds since the simulation started.
    pub now: f64,
    /// Length of this tick.
    pub delta: std::time::Duration,
}

impl SimClock {
    /// Length of this tick, s.
    pub fn dt(&self) -> f32 {
        self.delta.as_secs_f32()
    }
}

/// What this process is in a game (docs/plans/active/multiplayer.md):
/// standalone (single player: everything runs, as a listen server with no
/// remote clients), the server, or a client of a remote server.
/// Server-only systems (bots, rules, damage, logic, objectives) run under
/// `authoritative`.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetRole {
    #[default]
    Standalone,
    Server,
    Client,
}

/// A new map's tick length. The fixed clock keeps running (single player
/// and a network server alike), so its time stays with `Time<Virtual>`'s
/// (the clock frame systems and console commands read) and never goes
/// back while clients follow a server to the new map. Starting it over
/// put it behind by however long the game had run (the main menu, the
/// last map): times stamped in a frame or a command (a drop's touch
/// delay, a smoke cloud's age as drawn) were that far ahead of the
/// tick's. A client takes the server's clock anyway.
pub fn set_tick_length(world: &mut World, tick: std::time::Duration) {
    let client = world.get_resource::<NetRole>() == Some(&NetRole::Client);
    match world.get_resource_mut::<Time<Fixed>>() {
        Some(mut fixed) if !client => fixed.set_timestep(tick),
        _ => world.insert_resource(Time::<Fixed>::from_duration(tick)),
    }
}

/// A recording plays (a game's demo playback, `playdemo`): the world
/// shows what the recording says (its characters as puppets, their shots,
/// sounds and effects) and decides no outcome, as a network client draws
/// what its server sends: `authoritative` is false while it is there.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Playback;

/// Run condition: this process owns the game's outcome (standalone or
/// server; not a client, which only predicts its own player and draws
/// what the server sends, nor a recording playing back, `Playback`).
pub fn authoritative(role: Option<Res<NetRole>>, playback: Option<Res<Playback>>) -> bool {
    role.is_none_or(|r| *r != NetRole::Client) && playback.is_none()
}

/// Run condition: others' actions arrive to be shown, not simulated: a
/// network client, or a recording playing back (`Playback`).
pub fn shows_others(role: Option<Res<NetRole>>, playback: Option<Res<Playback>>) -> bool {
    role.is_some_and(|r| *r == NetRole::Client) || playback.is_some()
}

/// Whether the commands running now run for the first time (Source's
/// `IsFirstTimePredicted`). A client re-running commands after a server
/// correction sets it false: side effects (sounds, damage, pushes) happen
/// only the first time. True outside a replay.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirstTimePredicted(pub bool);

impl Default for FirstTimePredicted {
    fn default() -> Self {
        Self(true)
    }
}

/// The parts of a tick prediction runs for a client's own player, as
/// schedules: weapon selection, movement and the weapon frame. A fixed tick
/// runs each from its place in `FixedUpdate` (`weapon::SelectWeapons`,
/// `SimSet::Movement`, `weapon::WeaponFrame`); a replay runs them in order
/// per command (`predict`). Their systems read time from `SimClock`,
/// write side effects only under `FirstTimePredicted`, and act on every
/// entity with the components they need (on a client, only its own player
/// will have them; others are drawn from snapshots).
#[derive(bevy::ecs::schedule::ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Predict {
    Select,
    Movement,
    Weapons,
}

impl Predict {
    pub const ALL: [Predict; 3] = [Predict::Select, Predict::Movement, Predict::Weapons];
}

/// An exclusive system that runs the `stage` schedule, placed in
/// `FixedUpdate` where that stage belongs.
pub fn run_predicted(stage: Predict) -> impl FnMut(&mut World) {
    move |world: &mut World| {
        let _ = world.try_run_schedule(stage);
    }
}

/// Run one command's predicted tick (every `Predict` stage, in order) with
/// the intents, clock and flag already set.
pub fn predict(world: &mut World) {
    for stage in Predict::ALL {
        let _ = world.try_run_schedule(stage);
    }
}

/// The components prediction re-simulates (`PredictedAppExt::predicted`),
/// saved and restored for the entities a client predicts (its player and
/// the weapons it carries): restore the server's state, then replay the
/// commands since.
#[derive(Resource, Default)]
pub struct PredictedComponents(Vec<PredictedComponent>);

type SavedComponent = Option<Box<dyn std::any::Any + Send + Sync>>;

struct PredictedComponent {
    name: &'static str,
    save: fn(&World, Entity) -> SavedComponent,
    restore: fn(&mut World, Entity, &SavedComponent),
    /// How the server sends it to the client that predicts it
    /// (`PredictedAppExt::predicted_net`), if it does.
    net: Option<NetCodec>,
}

/// A predicted component's network form: the server's state of a
/// client's own player is sent as these bytes, compared byte for byte
/// with the client's prediction, and decoded over it on a mismatch.
#[derive(Clone, Copy)]
struct NetCodec {
    encode: fn(&World, Entity) -> Option<Vec<u8>>,
    decode: fn(&mut World, Entity, Option<&[u8]>),
}

/// The length `PredictedComponents::encode` writes for a component the
/// entity doesn't have.
const ABSENT: u16 = u16::MAX;

/// Some entities' predicted components as they were
/// (`PredictedComponents::save`).
#[derive(Default)]
pub struct PredictedSnapshot(Vec<(Entity, Vec<SavedComponent>)>);

impl PredictedComponents {
    /// The registered components' type names, in order.
    pub fn names(&self) -> Vec<&'static str> {
        self.0.iter().map(|c| c.name).collect()
    }

    pub fn save(&self, world: &World, entities: &[Entity]) -> PredictedSnapshot {
        PredictedSnapshot(
            entities
                .iter()
                .map(|e| (*e, self.0.iter().map(|c| (c.save)(world, *e)).collect()))
                .collect(),
        )
    }

    /// The networked predicted components of `entity` as one blob
    /// (`predicted_net`), in name order: per component a little-endian
    /// `u16` length (`u16::MAX`: absent) and its postcard bytes. Equal
    /// blobs are equal states, bit for bit.
    pub fn encode(&self, world: &World, entity: Entity) -> Vec<u8> {
        let mut out = Vec::new();
        for (_, net) in self.net_order() {
            match (net.encode)(world, entity) {
                Some(bytes) if bytes.len() < ABSENT as usize => {
                    out.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
                    out.extend_from_slice(&bytes);
                }
                _ => out.extend_from_slice(&ABSENT.to_le_bytes()),
            }
        }
        out
    }

    /// Write a blob from `encode` over `entity`'s components (inserting
    /// or removing them as the blob says). A blob of another layout fails
    /// and changes nothing.
    pub fn decode(&self, world: &mut World, entity: Entity, blob: &[u8]) -> Result<(), String> {
        let parts = self.split(blob)?;
        for ((_, net), part) in self.net_order().into_iter().zip(parts) {
            (net.decode)(world, entity, part);
        }
        Ok(())
    }

    /// The names of the networked components two blobs disagree on
    /// (`cl_showerror`).
    pub fn differing(&self, a: &[u8], b: &[u8]) -> Vec<&'static str> {
        let (Ok(pa), Ok(pb)) = (self.split(a), self.split(b)) else {
            return vec!["(layout)"];
        };
        self.net_order()
            .into_iter()
            .zip(pa.into_iter().zip(pb))
            .filter(|(_, (x, y))| x != y)
            .map(|((name, _), _)| name)
            .collect()
    }

    /// A blob's parts by component name (None: absent), in its order:
    /// for tools that look inside a prediction error.
    pub fn parts<'a>(&self, blob: &'a [u8]) -> Result<Vec<(&'static str, Option<&'a [u8]>)>, String> {
        Ok(self.net_order().into_iter().map(|(n, _)| n).zip(self.split(blob)?).collect())
    }

    /// The networked components by name.
    fn net_order(&self) -> Vec<(&'static str, NetCodec)> {
        let mut v: Vec<(&'static str, NetCodec)> = self.0.iter().filter_map(|c| Some((c.name, c.net?))).collect();
        v.sort_by_key(|(name, _)| *name);
        v
    }

    fn split<'a>(&self, mut blob: &'a [u8]) -> Result<Vec<Option<&'a [u8]>>, String> {
        let short = || "predicted state blob too short".to_string();
        let mut parts = Vec::new();
        for _ in 0..self.net_order().len() {
            let (len, rest) = blob.split_first_chunk::<2>().ok_or_else(short)?;
            blob = rest;
            let len = u16::from_le_bytes(*len);
            if len == ABSENT {
                parts.push(None);
                continue;
            }
            let (part, rest) = blob.split_at_checked(len as usize).ok_or_else(short)?;
            parts.push(Some(part));
            blob = rest;
        }
        if !blob.is_empty() {
            return Err("predicted state blob too long".into());
        }
        Ok(parts)
    }

    /// Put each saved entity's predicted components back as they were
    /// (removing the ones it didn't have). Entities gone since are skipped.
    pub fn restore(&self, world: &mut World, snapshot: &PredictedSnapshot) {
        for (e, saved) in &snapshot.0 {
            if world.get_entity(*e).is_err() {
                continue;
            }
            for (c, value) in self.0.iter().zip(saved) {
                (c.restore)(world, *e, value);
            }
        }
    }
}

pub trait PredictedAppExt {
    /// Register a component prediction saves and restores.
    fn predicted<C: Component<Mutability = bevy::ecs::component::Mutable> + Clone>(&mut self) -> &mut Self;

    /// The same, and the server sends it to the client predicting it,
    /// which compares it with its own (`PredictedComponents::encode`).
    /// Entity fields stay out of its serde form (`#[serde(skip)]`): ids
    /// differ between server and client.
    fn predicted_net<C: Component<Mutability = bevy::ecs::component::Mutable> + Clone + Serialize + DeserializeOwned>(
        &mut self,
    ) -> &mut Self;

    /// A networked part of the predicted state that isn't one component
    /// (e.g. what a character carries: its inventory and the weapon
    /// entities in it), under `name` (a component already registered with
    /// `predicted` by that name keeps its save and restore): `encode` it
    /// for an entity (None: absent), `decode` it over the entity (None:
    /// absent).
    fn predicted_codec(
        &mut self,
        name: &'static str,
        encode: fn(&World, Entity) -> Option<Vec<u8>>,
        decode: fn(&mut World, Entity, Option<&[u8]>),
    ) -> &mut Self;
}

impl PredictedAppExt for App {
    fn predicted<C: Component<Mutability = bevy::ecs::component::Mutable> + Clone>(&mut self) -> &mut Self {
        register_predicted::<C>(self, None);
        self
    }

    fn predicted_net<C: Component<Mutability = bevy::ecs::component::Mutable> + Clone + Serialize + DeserializeOwned>(
        &mut self,
    ) -> &mut Self {
        register_predicted::<C>(
            self,
            Some(NetCodec {
                encode: |w, e| w.get::<C>(e).and_then(|c| postcard::to_allocvec(c).ok()),
                decode: |w, e, bytes| match bytes.and_then(|b| postcard::from_bytes::<C>(b).ok()) {
                    Some(c) => {
                        w.entity_mut(e).insert(c);
                    }
                    None => {
                        w.entity_mut(e).remove::<C>();
                    }
                },
            }),
        );
        self
    }

    fn predicted_codec(
        &mut self,
        name: &'static str,
        encode: fn(&World, Entity) -> Option<Vec<u8>>,
        decode: fn(&mut World, Entity, Option<&[u8]>),
    ) -> &mut Self {
        let net = Some(NetCodec { encode, decode });
        let mut registry = self.world_mut().get_resource_or_init::<PredictedComponents>();
        if let Some(c) = registry.0.iter_mut().find(|c| c.name == name) {
            c.net = net;
        } else {
            registry.0.push(PredictedComponent {
                name,
                net,
                save: |_, _| None,
                restore: |_, _, _| {},
            });
        }
        self
    }
}

fn register_predicted<C: Component<Mutability = bevy::ecs::component::Mutable> + Clone>(
    app: &mut App,
    net: Option<NetCodec>,
) {
    let name = std::any::type_name::<C>();
    let mut registry = app.world_mut().get_resource_or_init::<PredictedComponents>();
    let save: fn(&World, Entity) -> SavedComponent = |w, e| {
        w.get::<C>(e)
            .map(|c| Box::new(c.clone()) as Box<dyn std::any::Any + Send + Sync>)
    };
    let restore: fn(&mut World, Entity, &SavedComponent) = |w, e, v| match v.as_ref().and_then(|v| v.downcast_ref::<C>()) {
        Some(c) => {
            w.entity_mut(e).insert(c.clone());
        }
        None => {
            w.entity_mut(e).remove::<C>();
        }
    };
    if let Some(c) = registry.0.iter_mut().find(|c| c.name == name) {
        c.net = c.net.or(net);
        c.save = save;
        c.restore = restore;
    } else {
        registry.0.push(PredictedComponent {
            name,
            net,
            save,
            restore,
        });
    }
}

/// Round restarts so far (the rules count one up when a new round
/// starts). Each count puts the map's entities back as they spawned:
/// the logic layer re-creates its world, the map layer makes broken
/// brushes whole and clears gibs (Counter-Strike's round restart).
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoundRestarts(pub u32);

/// The rules hold every character still (a round's freeze time): they
/// may turn, look and buy, not move or shoot. The rules clear the
/// intents (`SimSet::Rules`); bots read it so standing still doesn't
/// count as being stuck.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FreezeTime(pub bool);

/// What a blast or a flash does to a character's hearing for a while
/// (Source: a player DSP preset): what they hear is mixed `mix` toward a
/// muffled copy (low-passed at `cutoff` Hz, times `wet_gain`), and a
/// ringing tone (`ring_hz`, amplitude `ring_gain` times the mix) plays.
/// Full for `hold` seconds, then fading out over `fade` (linearly, or
/// exponentially down to 1 % by its end).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HearingEffect {
    pub hold: f32,
    pub fade: f32,
    pub exponential: bool,
    pub mix: f32,
    pub cutoff: f32,
    pub wet_gain: f32,
    pub ring_hz: f32,
    pub ring_gain: f32,
}

impl HearingEffect {
    /// Strength (0-1) `t` seconds after it started.
    pub fn level(&self, t: f32) -> f32 {
        if t < 0.0 || t >= self.hold + self.fade {
            0.0
        } else if t <= self.hold {
            1.0
        } else {
            let x = (t - self.hold) / self.fade;
            if self.exponential {
                // e^-4.6 = 1 %: then cut.
                (-4.6 * x).exp()
            } else {
                1.0 - x
            }
        }
    }

    /// Seconds until it is over.
    pub fn length(&self) -> f32 {
        self.hold + self.fade
    }
}

/// A character's hearing is hit (`HearingEffect`): clients apply it to
/// what the local player hears.
#[derive(Message, Clone, Copy, Debug)]
pub struct Deafened {
    pub target: Entity,
    pub effect: HearingEffect,
}

/// A character's blindness (a flashbang): the screen goes white at
/// `alpha` (0-1), holds until `fade_start` (seconds, the simulation
/// clock) and fades out linearly by `end`. Clients draw it; bots don't
/// see while it is strong.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Blinded {
    pub alpha: f32,
    pub fade_start: f64,
    pub end: f64,
}

impl Blinded {
    /// Whiteness at `now`.
    pub fn alpha_at(&self, now: f64) -> f32 {
        if now >= self.end {
            0.0
        } else if now <= self.fade_start || self.end <= self.fade_start {
            self.alpha
        } else {
            self.alpha * ((self.end - now) / (self.end - self.fade_start)) as f32
        }
    }

    /// The stronger of this blindness and `other` (a new flash only
    /// raises what is left of the current one).
    pub fn raised(self, other: Blinded, now: f64) -> Blinded {
        if self.alpha_at(now) >= other.alpha && self.end >= other.end {
            self
        } else if other.alpha >= self.alpha_at(now) && other.end >= self.end {
            other
        } else {
            // One is whiter, the other lasts longer: keep both maxima.
            Blinded {
                alpha: self.alpha_at(now).max(other.alpha),
                fade_start: self.fade_start.max(other.fade_start),
                end: self.end.max(other.end),
            }
        }
    }
}

/// Something sight doesn't pass (a smoke cloud): a sphere. A sight line
/// is blocked when more than `radius` of it lies inside (Counter-Strike
/// bots don't see through smoke; the exact rule isn't known, see
/// specs/cs_source/grenades.md Q16).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct SightBlocker {
    pub centre: Vec3,
    pub radius: f32,
}

impl SightBlocker {
    /// Length of the segment `from`-`to` inside the sphere.
    pub fn chord(&self, from: Vec3, to: Vec3) -> f32 {
        let d = to - from;
        let len = d.length();
        if len < 1e-6 || self.radius <= 0.0 {
            return 0.0;
        }
        let dir = d / len;
        let t = (self.centre - from).dot(dir);
        let closest = from + dir * t;
        let h2 = self.radius * self.radius - closest.distance_squared(self.centre);
        if h2 <= 0.0 {
            return 0.0;
        }
        let h = h2.sqrt();
        ((t + h).min(len) - (t - h).max(0.0)).max(0.0)
    }

    pub fn blocks(&self, from: Vec3, to: Vec3) -> bool {
        self.chord(from, to) > self.radius
    }
}

/// `mp_playerid` (the server's, sent to clients): whose names the target
/// ID shows: 0 everyone's, 1 teammates' only, 2 nobody's.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerIdMode(pub u8);

/// Whether characters on the same team hurt each other (CS:S
/// `mp_friendlyfire`, default 0). Team 0 (no team) is never friendly.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FriendlyFire(pub u8);

/// Whether team rules refuse this damage: teammates don't hurt each other
/// unless friendly fire is on (your own damage, e.g. falling, always
/// counts).
pub fn refused_by_team(d: &Damage, teams: &Query<&Team>, friendly_fire: Option<&FriendlyFire>) -> bool {
    let teammate = d
        .attacker
        .filter(|a| *a != d.target)
        .is_some_and(|a| matches!((teams.get(a), teams.get(d.target)), (Ok(x), Ok(y)) if x == y && x.0 != 0));
    teammate && !friendly_fire.is_some_and(|f| f.0 != 0)
}

/// Why damage is refused (`DamageRules::refusal`). What a refused hit
/// still does depends on why (CS:S, measured 2026-10-17;
/// specs/cs_source/weapons.md "Refused hits"): a teammate's hit with
/// friendly fire off does nothing at all (no armour, sound, blood,
/// slowdown or flinch); a noclipping player's hit still bleeds and
/// flinches, nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The target noclips (`Noclip`).
    Noclip,
    /// A teammate's, friendly fire off (`refused_by_team`).
    Team,
    /// The map made the target invulnerable (`MapControls`).
    Invulnerable,
    /// The target's damage filter blocks this kind (`DamageFilter`).
    Filter,
}

/// What decides whether damage is refused before it reaches health:
/// team rules, a map's invulnerability and damage filters, a noclipping
/// target (`Noclip`); and whether it lands (`lands`: not refused, and not
/// on a `God` target, which `apply_damage`'s health query leaves out).
#[derive(bevy::ecs::system::SystemParam)]
pub struct DamageRules<'w, 's> {
    noclip: Query<'w, 's, (), With<Noclip>>,
    god: Query<'w, 's, (), With<God>>,
    teams: Query<'w, 's, &'static Team>,
    filters: Query<'w, 's, &'static DamageFilter>,
    controls: Query<'w, 's, &'static MapControls>,
    friendly_fire: Option<Res<'w, FriendlyFire>>,
}

impl DamageRules<'_, '_> {
    pub fn refuse(&self, d: &Damage) -> bool {
        self.refusal(d).is_some()
    }

    /// Why `d` is refused, if it is.
    pub fn refusal(&self, d: &Damage) -> Option<Refusal> {
        if self.noclip.contains(d.target) && !d.is_suicide() {
            Some(Refusal::Noclip)
        } else if refused_by_team(d, &self.teams, self.friendly_fire.as_deref()) {
            Some(Refusal::Team)
        } else if self.controls.get(d.target).is_ok_and(|c| c.invulnerable) {
            Some(Refusal::Invulnerable)
        } else if self.filters.get(d.target).is_ok_and(|f| f.blocked.contains(&d.kind)) {
            Some(Refusal::Filter)
        } else {
            None
        }
    }

    /// Whether `d` is taken: not refused, the target not under `God`.
    /// Only such hits cost armour (`weapon::HitArmor`), make hurt sounds
    /// and slow the victim down.
    pub fn lands(&self, d: &Damage) -> bool {
        !self.god.contains(d.target) && !self.refuse(d)
    }
}

/// Subtract damage from health; announce deaths once (public so other
/// systems can order themselves after it).
pub fn apply_damage(
    mut damage: MessageReader<Damage>,
    mut health: Query<&mut Health, Without<God>>,
    rules: DamageRules,
    mut died: MessageWriter<Died>,
    mut taken: MessageWriter<DamageTaken>,
) {
    for d in damage.read() {
        if rules.refuse(d) {
            continue;
        }
        let Ok(mut h) = health.get_mut(d.target) else { continue };
        if h.current <= 0.0 {
            continue;
        }
        h.current -= d.amount;
        taken.write(DamageTaken {
            damage: d.clone(),
            directed: d.attacker.is_some_and(|a| a != d.target),
        });
        if h.current <= 0.0 {
            h.current = 0.0;
            died.write(Died {
                entity: d.target,
                attacker: d.attacker,
                damage: d.clone(),
            });
        }
    }
}

/// Ordering of the fixed-tick simulation. Game plugins put their systems in
/// these sets so that, e.g., weapons always see this tick's movement state.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum SimSet {
    /// Match rules (rounds, respawns), before everything that acts on
    /// them this tick.
    Rules,
    /// Intents are final for the tick: each gets its command number.
    Commands,
    Movement,
    Weapons,
}

/// Stamp each intent with this tick's command number: the tick
/// (`SimClock::tick`; a network client's is the server tick its command
/// is for), offset by
/// the character's `Seed` (Source clients number their commands from
/// their own start, so two players firing on the same tick don't share a
/// spread pattern). Never from entity ids.
pub fn number_commands(clock: Res<SimClock>, mut intents: Query<(&mut Intent, Option<&Seed>)>) {
    for (mut intent, seed) in &mut intents {
        let n = (clock.tick as u32).wrapping_add(seed.map_or(0, |s| s.0 as u32));
        if intent.command != n {
            intent.command = n;
        }
    }
}

/// Count the tick and set the clock (`SimClock`) from `Time<Fixed>` (a
/// network client then sets it to the server tick it predicts,
/// `net::predict`).
pub fn start_tick(mut tick: ResMut<SimTick>, mut clock: ResMut<SimClock>, time: Res<Time<Fixed>>) {
    tick.0 += 1;
    *clock = SimClock {
        tick: tick.0,
        now: time.elapsed_secs_f64(),
        delta: time.delta(),
    };
}

/// Before each physics step: the physics' copy of a character's position
/// (`Position`) is where its movement put it, to the bit. The physics
/// takes a moved transform only when it moved more than its tolerance
/// (0.01 mm), and after the step writes its copy back over the transform
/// of every body that is awake: a smaller step of the movement's (the
/// metres-to-units round trip moves a player put at a spawn point by a
/// float step or two) was undone while the body was awake and kept while
/// it slept (half a second after its last bigger move), and kept in a
/// network client's replay, which runs no physics. So the outcome of a
/// tick hung on the physics' sleep state, which a server and its client
/// don't share (found by the soak: a respawned player mispredicted by
/// micrometres on each tick for half a second; games/cs_source/tests/it/net_prediction.rs,
/// `a_float_step_below_the_physics_tolerance_is_predicted`).
///
/// A change marks it changed, as the physics' own sync would (it wakes
/// the body and moves its collider).
pub fn keep_moved_positions(
    mut bodies: Query<
        (&Transform, &mut avian3d::prelude::Position),
        (With<avian3d::prelude::CustomPositionIntegration>, Without<ChildOf>),
    >,
) {
    for (t, mut p) in &mut bodies {
        if p.0.to_array().map(f32::to_bits) != t.translation.to_array().map(f32::to_bits) {
            p.0 = t.translation;
        }
    }
}

pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Intent>()
            .register_type::<Velocity>()
            .register_type::<MovementState>()
            .register_type::<Health>()
            .register_type::<Team>()
            .register_type::<Unassigned>()
            .register_type::<ChoosingClass>()
            .register_type::<PlayerClass>()
            .register_type::<UserInfo>()
            .register_type::<Seed>()
            .register_type::<MaxSpeed>()
            .register_type::<BaseVelocity>()
            .register_type::<EntityGravity>()
            .register_type::<Hitboxes>()
            .register_type::<Damageable>()
            .add_message::<Damage>()
            .add_message::<MapDamage>()
            .add_message::<Equip>()
            .add_message::<Explosion>()
            .add_message::<Deafened>()
            .add_message::<Died>()
            .add_message::<DamageTaken>()
            .add_message::<Radio>()
            .init_resource::<FriendlyFire>()
            .add_systems(FixedUpdate, apply_damage.after(SimSet::Weapons).run_if(authoritative))
            .add_systems(
                FixedUpdate,
                map_damage_lands
                    .after(SimSet::Weapons)
                    .before(apply_damage)
                    .run_if(authoritative.and_then(not(resource_exists::<MapDamageArmor>))),
            )
            .register_type::<SpawnPoint>()
            .register_type::<LocalPlayer>()
            .register_type::<Connecting>()
            .register_type::<Spectating>()
            .register_type::<SimTick>()
            .init_resource::<SimTick>()
            .init_resource::<SimClock>()
            .init_resource::<NetRole>()
            .init_resource::<FirstTimePredicted>()
            .init_resource::<PredictedComponents>()
            .add_systems(FixedFirst, start_tick)
            .init_resource::<RoundRestarts>()
            .init_resource::<FreezeTime>()
            .configure_sets(
                FixedUpdate,
                (SimSet::Rules, SimSet::Commands, SimSet::Movement, SimSet::Weapons).chain(),
            )
            .add_systems(FixedUpdate, number_commands.in_set(SimSet::Commands))
            .add_message::<ScoreChange>()
            .add_systems(
                FixedUpdate,
                apply_map_controls.after(SimSet::Rules).before(SimSet::Commands),
            )
            .predicted_net::<MapControls>()
            .add_systems(FixedUpdate, run_predicted(Predict::Movement).in_set(SimSet::Movement))
            .add_systems(
                FixedPostUpdate,
                keep_moved_positions.before(avian3d::prelude::PhysicsSystems::Prepare),
            )
            .predicted_net::<Transform>()
            .predicted_net::<Velocity>()
            .predicted_net::<MovementState>()
            .predicted_net::<BaseVelocity>()
            .predicted_net::<MaxSpeed>();
        for stage in Predict::ALL {
            app.init_schedule(stage);
        }
    }
}

// Collision world shared by maps and movement: exact brushes for swept-box
// movement, and water volumes.

/// A solid convex volume as planes: a point p is inside when n.p <= d for
/// every plane (n, d). Engine space, meters; `min`/`max` bound it. Games
/// whose movement sweeps boxes against brushes (Source) use these instead
/// of the physics engine's shape casts, which lose precision on large
/// volumes.
#[derive(Clone, Debug)]
pub struct MapBrush {
    pub planes: Vec<(Vec3, f32)>,
    pub min: Vec3,
    pub max: Vec3,
    /// Climbable (Source: ladder contents).
    pub ladder: bool,
    /// Surface property name (footsteps), lower-case, for brushes that
    /// carry their own (props). None: look it up from the world's faces.
    pub surface: Option<String>,
}

impl MapBrush {
    /// An axis-aligned box.
    pub fn from_box(min: Vec3, max: Vec3) -> Self {
        let planes = vec![
            (Vec3::X, max.x),
            (Vec3::NEG_X, -min.x),
            (Vec3::Y, max.y),
            (Vec3::NEG_Y, -min.y),
            (Vec3::Z, max.z),
            (Vec3::NEG_Z, -min.z),
        ];
        Self {
            planes,
            min,
            max,
            ladder: false,
            surface: None,
        }
    }

    /// Whether a point (engine space) is inside.
    pub fn contains(&self, p: Vec3) -> bool {
        p.cmpge(self.min).all() && p.cmple(self.max).all() && self.planes.iter().all(|(n, d)| n.dot(p) <= *d)
    }

    /// Whether an axis-aligned box (centre, half size) overlaps the brush
    /// by more than `skin` (any space, as long as the brush uses it too).
    pub fn overlaps_box(&self, centre: Vec3, half: Vec3, skin: f32) -> bool {
        self.max.cmpgt(centre - half).all()
            && self.min.cmplt(centre + half).all()
            && self
                .planes
                .iter()
                .all(|(n, d)| n.dot(centre) - (d + n.abs().dot(half)) < -skin)
    }

    /// Sweep an axis-aligned box (half size `half`) with its centre from
    /// `from` to `to`; hits stop `eps` short along the plane. Returns the
    /// fraction travelled and the hit plane's normal when the box hits
    /// (fraction 0 with a zero normal when it starts inside and stays in).
    pub fn sweep_box(&self, half: Vec3, from: Vec3, to: Vec3, eps: f32) -> Option<(f32, Vec3)> {
        let lo = from.min(to) - half - Vec3::splat(eps);
        let hi = from.max(to) + half + Vec3::splat(eps);
        if self.max.cmplt(lo).any() || self.min.cmpgt(hi).any() {
            return None;
        }
        let (mut enter, mut leave) = (-1.0f32, 1.0f32);
        let (mut starts_out, mut gets_out) = (false, false);
        let mut clip = Vec3::ZERO;
        for (n, d) in &self.planes {
            let dist = d + n.abs().dot(half);
            let d1 = n.dot(from) - dist;
            let d2 = n.dot(to) - dist;
            gets_out |= d2 > 0.0;
            starts_out |= d1 > 0.0;
            if d1 > 0.0 && (d2 >= eps || d2 >= d1) {
                return None;
            }
            if d1 <= 0.0 && d2 <= 0.0 {
                continue;
            }
            if d1 > d2 {
                let f = ((d1 - eps) / (d1 - d2)).max(0.0);
                if f > enter {
                    enter = f;
                    clip = *n;
                }
            } else {
                leave = leave.min(((d1 + eps) / (d1 - d2)).min(1.0));
            }
        }
        if !starts_out {
            return (!gets_out).then_some((0.0, Vec3::ZERO));
        }
        (enter < leave && enter > -1.0).then_some((enter.max(0.0), clip))
    }

    /// The brush moved: rotated by `rotation` about the origin, then
    /// moved by `offset` (planes and bounds; `points` are its corners
    /// before the move, for the new bounds).
    pub fn transformed(&self, points: &[Vec3], rotation: Quat, offset: Vec3) -> Self {
        let planes = self
            .planes
            .iter()
            .map(|(n, d)| {
                let n = rotation * *n;
                (n, d + n.dot(offset))
            })
            .collect();
        let (mut min, mut max) = (Vec3::MAX, Vec3::MIN);
        for p in points {
            let q = rotation * *p + offset;
            min = min.min(q);
            max = max.max(q);
        }
        if points.is_empty() {
            (min, max) = (self.min + offset, self.max + offset);
        }
        Self {
            planes,
            min,
            max,
            ladder: self.ladder,
            surface: self.surface.clone(),
        }
    }
}

impl MapBrush {
    /// A convex solid from its face planes (`n . p <= d`, normals
    /// normalized), with the bevel planes a box sweep needs to be exact:
    /// the bounding box's planes and, through each edge, the planes along
    /// the edge and each axis that touch the solid only there (as vbsp
    /// adds to brushes). Without them a box swept by pushing the planes
    /// out stops short of a non-axial edge, in the air beside it: on a
    /// surf ramp built of pieces, a ghost wall at each seam.
    pub fn from_planes(mut planes: Vec<(Vec3, f32)>) -> Self {
        let n = planes.len();
        // Corners: where three planes meet inside all the others.
        let mut corners: Vec<Vec3> = Vec::new();
        for i in 0..n {
            for j in i + 1..n {
                for k in j + 1..n {
                    let ((n1, d1), (n2, d2), (n3, d3)) = (planes[i], planes[j], planes[k]);
                    let denom = n1.dot(n2.cross(n3));
                    if denom.abs() < 1e-6 {
                        continue;
                    }
                    let p = (n2.cross(n3) * d1 + n3.cross(n1) * d2 + n1.cross(n2) * d3) / denom;
                    if planes.iter().all(|(m, d)| m.dot(p) <= d + 1e-3) {
                        corners.push(p);
                    }
                }
            }
        }
        let on = |p: Vec3, (m, d): (Vec3, f32)| (m.dot(p) - d).abs() < 1e-3;
        let min = corners.iter().fold(Vec3::MAX, |a, c| a.min(*c));
        let max = corners.iter().fold(Vec3::MIN, |a, c| a.max(*c));
        let add = |planes: &mut Vec<(Vec3, f32)>, m: Vec3, d: f32| {
            if !planes.iter().any(|(q, e)| q.dot(m) > 0.9999 && (e - d).abs() < 1e-4) {
                planes.push((m, d));
            }
        };
        for (m, d) in Self::from_box(min, max).planes {
            add(&mut planes, m, d);
        }
        // Edges: two face planes that share at least two corners.
        for i in 0..n {
            for j in i + 1..n {
                let ends: Vec<Vec3> = corners
                    .iter()
                    .copied()
                    .filter(|p| on(*p, planes[i]) && on(*p, planes[j]))
                    .collect();
                let (Some(a), Some(b)) = (ends.first(), ends.iter().find(|p| p.distance(ends[0]) > 1e-4)) else {
                    continue;
                };
                let e = *b - *a;
                for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                    let Some(m) = e.cross(axis).try_normalize() else { continue };
                    for m in [m, -m] {
                        let d = m.dot(*a);
                        // A bevel touches the solid along the edge only.
                        if corners.iter().all(|c| m.dot(*c) <= d + 1e-3) {
                            add(&mut planes, m, d);
                        }
                    }
                }
            }
        }
        Self {
            planes,
            min,
            max,
            ladder: false,
            surface: None,
        }
    }

    /// A triangle as a thin solid: the triangle's face (normal `(b - a) x
    /// (c - a)`), a back face `thickness` behind it, its three sides, and
    /// the bevel planes a box sweep needs to be exact (the box's axes and
    /// each edge crossed with each axis), all as supporting planes.
    pub fn from_triangle(a: Vec3, b: Vec3, c: Vec3, thickness: f32) -> Option<Self> {
        let n = (b - a).cross(c - a).try_normalize()?;
        let back = n * -thickness;
        let corners = [a, b, c, a + back, b + back, c + back];
        let edges = [b - a, c - b, a - c, n];
        let mut axes: Vec<Vec3> = vec![Vec3::X, Vec3::Y, Vec3::Z, n];
        for e in &edges[..3] {
            axes.push(n.cross(*e));
        }
        for e in edges {
            for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                axes.push(e.cross(axis));
            }
        }
        let mut planes: Vec<(Vec3, f32)> = Vec::with_capacity(axes.len() * 2);
        for axis in axes {
            let Some(m) = axis.try_normalize() else { continue };
            for m in [m, -m] {
                let d = corners.iter().map(|p| m.dot(*p)).fold(f32::MIN, f32::max);
                // Only true repeats: a bevel a hair off another plane (an
                // edge across a gentle crease, tilted 0.07 degrees from the
                // axis) is a different plane through a different point. Kept
                // apart, a box rests on the crease where it really is (de_dust2's
                // T spawn ridge: 0.038 units lower, as CS:S) and lands on it
                // with the edge's normal, as CS:S does.
                if planes.iter().any(|(q, e)| q.dot(m) > 1.0 - 1e-6 && (e - d).abs() < 1e-5) {
                    continue;
                }
                planes.push((m, d));
            }
        }
        // The face first: sweeps that tie report the earliest plane.
        let face = planes.iter().position(|(m, _)| m.dot(n) > 1.0 - 1e-6)?;
        planes.swap(0, face);
        let min = corners.iter().fold(Vec3::MAX, |acc, p| acc.min(*p));
        let max = corners.iter().fold(Vec3::MIN, |acc, p| acc.max(*p));
        Some(Self {
            planes,
            min,
            max,
            ladder: false,
            surface: None,
        })
    }
}

/// The loaded map's terrain triangles (Source displacements) as thin
/// solids (`MapBrush::from_triangle`), with a grid to find the ones near a
/// box quickly. Movement that sweeps brushes exactly sweeps these the same
/// way instead of casting against the terrain's physics collider.
#[derive(Resource, Clone, Debug, Default)]
pub struct MapTerrain {
    pub brushes: Vec<MapBrush>,
    /// Grid over engine x and z: origin, cell size and counts.
    origin: Vec2,
    cell: f32,
    size: (usize, usize),
    cells: Vec<Vec<u32>>,
}

impl MapTerrain {
    /// Grid cell size, meters.
    const CELL: f32 = 2.0;

    pub fn from_triangles(triangles: impl IntoIterator<Item = [Vec3; 3]>, thickness: f32) -> Self {
        let brushes: Vec<MapBrush> = triangles
            .into_iter()
            .filter_map(|[a, b, c]| MapBrush::from_triangle(a, b, c, thickness))
            .collect();
        if brushes.is_empty() {
            return Self::default();
        }
        let lo = brushes.iter().fold(Vec2::MAX, |acc, b| acc.min(b.min.xz()));
        let hi = brushes.iter().fold(Vec2::MIN, |acc, b| acc.max(b.max.xz()));
        let size = (
            ((hi.x - lo.x) / Self::CELL).floor() as usize + 1,
            ((hi.y - lo.y) / Self::CELL).floor() as usize + 1,
        );
        let mut terrain = Self {
            brushes,
            origin: lo,
            cell: Self::CELL,
            size,
            cells: vec![Vec::new(); size.0 * size.1],
        };
        for i in 0..terrain.brushes.len() {
            let (x0, z0, x1, z1) = terrain.cell_range(terrain.brushes[i].min, terrain.brushes[i].max);
            for z in z0..=z1 {
                for x in x0..=x1 {
                    terrain.cells[z * size.0 + x].push(i as u32);
                }
            }
        }
        terrain
    }

    fn cell_range(&self, lo: Vec3, hi: Vec3) -> (usize, usize, usize, usize) {
        let to = |v: f32, o: f32, n: usize| (((v - o) / self.cell).floor().max(0.0) as usize).min(n - 1);
        (
            to(lo.x, self.origin.x, self.size.0),
            to(lo.z, self.origin.y, self.size.1),
            to(hi.x, self.origin.x, self.size.0),
            to(hi.z, self.origin.y, self.size.1),
        )
    }

    /// Indices of brushes whose bounds may overlap `lo`..`hi` (sorted,
    /// no repeats), into `out`.
    pub fn near(&self, lo: Vec3, hi: Vec3, out: &mut Vec<u32>) {
        out.clear();
        if self.cells.is_empty()
            || hi.x < self.origin.x
            || hi.z < self.origin.y
            || lo.x > self.origin.x + self.cell * self.size.0 as f32
            || lo.z > self.origin.y + self.cell * self.size.1 as f32
        {
            return;
        }
        let (x0, z0, x1, z1) = self.cell_range(lo, hi);
        for z in z0..=z1 {
            for x in x0..=x1 {
                out.extend(self.cells[z * self.size.0 + x].iter().copied().filter(|&i| {
                    let b = &self.brushes[i as usize];
                    b.max.cmpge(lo).all() && b.min.cmple(hi).all()
                }));
            }
        }
        out.sort_unstable();
        out.dedup();
    }
}

/// Marks the physics collider built from the terrain triangles
/// (`MapTerrain`), so movement that sweeps them exactly can leave it out
/// of physics queries.
#[derive(Component, Debug)]
pub struct MapTerrainCollider;

/// A water (or slime) volume, for swimming.
#[derive(Clone, Debug)]
pub struct MapWaterVolume {
    pub brush: MapBrush,
    pub slime: bool,
}

/// The loaded map's water volumes (`MapData::water`).
#[derive(Resource, Clone, Debug, Default)]
pub struct MapWater(pub Vec<MapWaterVolume>);

/// The loaded map's brushes (`MapData::collision_brushes`).
#[derive(Resource, Clone, Debug, Default)]
pub struct MapBrushes(pub Vec<MapBrush>);

/// A grid over engine x and z of a list of brushes' bounds (a broad
/// phase): which brushes a box may overlap, without testing them all.
/// Built from `MapBrushes` by whoever sweeps against them (the Source
/// movement keeps one per map: a sweep tested every brush of the map's
/// bounds, ~75 us a player a tick on de_dust2; multiplayer.md
/// "Capacity").
#[derive(Clone, Debug, Default)]
pub struct BrushGrid {
    origin: Vec2,
    cell: f32,
    size: (usize, usize),
    cells: Vec<Vec<u32>>,
    /// Brushes over more than `MAX_CELLS` cells (the sky's walls, big
    /// floors): always candidates.
    big: Vec<u32>,
    /// How many brushes it was built from.
    pub len: usize,
}

impl BrushGrid {
    /// Grid cell size, meters (~157 units).
    const CELL: f32 = 4.0;
    const MAX_CELLS: usize = 64;

    pub fn new(brushes: &[MapBrush]) -> Self {
        if brushes.is_empty() {
            return Self::default();
        }
        let lo = brushes.iter().fold(Vec2::MAX, |acc, b| acc.min(b.min.xz()));
        let hi = brushes.iter().fold(Vec2::MIN, |acc, b| acc.max(b.max.xz()));
        if !lo.is_finite() || !hi.is_finite() {
            return Self {
                big: (0..brushes.len() as u32).collect(),
                len: brushes.len(),
                ..default()
            };
        }
        let size = (
            (((hi.x - lo.x) / Self::CELL).floor() as usize + 1).min(4096),
            (((hi.y - lo.y) / Self::CELL).floor() as usize + 1).min(4096),
        );
        let mut grid = Self {
            origin: lo,
            cell: Self::CELL,
            size,
            cells: vec![Vec::new(); size.0 * size.1],
            big: Vec::new(),
            len: brushes.len(),
        };
        for (i, b) in brushes.iter().enumerate() {
            let (x0, z0, x1, z1) = grid.cell_range(b.min, b.max);
            // Empty or inverted bounds (min above max, NaN): in the always
            // list, where the callers' own bounds tests decide.
            let inverted = x1 < x0 || z1 < z0 || !b.min.is_finite() || !b.max.is_finite();
            if inverted || (x1 - x0 + 1) * (z1 - z0 + 1) > Self::MAX_CELLS {
                grid.big.push(i as u32);
                continue;
            }
            for z in z0..=z1 {
                for x in x0..=x1 {
                    grid.cells[z * size.0 + x].push(i as u32);
                }
            }
        }
        grid
    }

    fn cell_range(&self, lo: Vec3, hi: Vec3) -> (usize, usize, usize, usize) {
        let to = |v: f32, o: f32, n: usize| (((v - o) / self.cell).floor().max(0.0) as usize).min(n - 1);
        (
            to(lo.x, self.origin.x, self.size.0),
            to(lo.z, self.origin.y, self.size.1),
            to(hi.x, self.origin.x, self.size.0),
            to(hi.z, self.origin.y, self.size.1),
        )
    }

    /// Indices of `brushes` (the list it was built from) whose bounds
    /// overlap `lo`..`hi`, bounds included: sorted, no repeats, into
    /// `out`. Every brush the box touches is there, so a sweep testing
    /// these in order gets what testing all of them would.
    pub fn near(&self, brushes: &[MapBrush], lo: Vec3, hi: Vec3, out: &mut Vec<u32>) {
        out.clear();
        let overlaps = |i: &u32| {
            let b = &brushes[*i as usize];
            b.max.cmpge(lo).all() && b.min.cmple(hi).all()
        };
        out.extend(self.big.iter().copied().filter(overlaps));
        // Out of the grid: its edge cells (clamped), which hold whatever
        // lies past them.
        if !self.cells.is_empty() && lo.is_finite() && hi.is_finite() {
            let (x0, z0, x1, z1) = self.cell_range(lo, hi);
            for z in z0..=z1 {
                for x in x0..=x1 {
                    out.extend(self.cells[z * self.size.0 + x].iter().copied().filter(overlaps));
                }
            }
        }
        out.sort_unstable();
        out.dedup();
    }
}

/// The map's BSP tree over the leading `MapBrushes` (Source maps; the
/// world's brushes). A Source trace tests brushes leaf by leaf down this
/// tree, so of two faces hit at the same distance (a ladder flush with a
/// player clip) the one reached first wins. Engine space.
#[derive(Resource, Clone, Debug, Default)]
pub struct MapBrushTree {
    pub nodes: Vec<BrushTreeNode>,
    /// Each leaf's brushes (indices into `MapBrushes`), in stored order.
    pub leaves: Vec<Vec<u32>>,
    /// Root: a node index (>= 0) or leaf -1-index.
    pub root: i32,
}

#[derive(Clone, Debug)]
pub struct BrushTreeNode {
    /// Splitting plane n.p = dist; children[0] is in front of it.
    pub normal: Vec3,
    pub dist: f32,
    pub children: [i32; 2],
}

impl MapBrushTree {
    /// The order a box (half size `half`) swept from centre `from` to `to`
    /// meets the brushes: leaves near the box's path, front to back along
    /// the move (in front of a split plane first when moving along it),
    /// each brush at its first leaf. `slop` widens the box when deciding
    /// which side of a plane it touches.
    pub fn sweep_order(&self, half: Vec3, from: Vec3, to: Vec3, slop: f32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut stack = vec![self.root];
        while let Some(child) = stack.pop() {
            if child < 0 {
                for &b in self.leaves.get((-child - 1) as usize).map_or(&[][..], |l| &l[..]) {
                    if seen.insert(b) {
                        out.push(b);
                    }
                }
                continue;
            }
            let Some(node) = self.nodes.get(child as usize) else { continue };
            let offset = node.normal.abs().dot(half) + slop;
            let (t1, t2) = (node.normal.dot(from) - node.dist, node.normal.dot(to) - node.dist);
            if t1 >= offset && t2 >= offset {
                stack.push(node.children[0]);
            } else if t1 < -offset && t2 < -offset {
                stack.push(node.children[1]);
            } else {
                // The side the move starts on first: the back when moving
                // toward the front; the front when moving along the plane.
                let first = usize::from(t1 < t2);
                stack.push(node.children[first ^ 1]);
                stack.push(node.children[first]);
            }
        }
        out
    }
}

/// Marks the physics collider built from the same brushes, so movement that
/// sweeps `MapBrushes` itself can leave it out of physics queries.
#[derive(Component, Debug)]
pub struct MapBrushCollider;

/// A prop collider's surface property name (lower-case), reported by traces
/// that hit it (footsteps).
#[derive(Component, Clone, Debug)]
pub struct PropSurface(pub String);

#[cfg(test)]
mod brush_grid_tests {
    use super::*;

    /// The grid's candidates are exactly the brushes whose bounds overlap
    /// the box (what testing every brush finds), in index order.
    #[test]
    fn the_grid_finds_what_testing_every_brush_finds() {
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut rand = move |lo: f32, hi: f32| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            lo + (hi - lo) * (seed >> 40) as f32 / (1u64 << 24) as f32
        };
        let mut brushes = Vec::new();
        for i in 0..2000 {
            let at = Vec3::new(rand(-300.0, 300.0), rand(-50.0, 50.0), rand(-300.0, 300.0));
            // Some long walls and floors (the grid's "big" list), and an
            // empty brush (bounds inverted, as a brush with no volume has).
            if i % 97 == 0 {
                brushes.push(MapBrush::from_box(Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)));
                continue;
            }
            let size = if i % 50 == 0 {
                Vec3::new(rand(50.0, 600.0), rand(0.1, 10.0), rand(50.0, 600.0))
            } else {
                Vec3::new(rand(0.1, 8.0), rand(0.1, 8.0), rand(0.1, 8.0))
            };
            brushes.push(MapBrush::from_box(at, at + size));
        }
        let grid = BrushGrid::new(&brushes);
        let mut near = Vec::new();
        for _ in 0..500 {
            // Boxes inside and outside the brushes' extent.
            let lo = Vec3::new(rand(-1000.0, 1000.0), rand(-60.0, 60.0), rand(-1000.0, 1000.0));
            let hi = lo + Vec3::new(rand(0.0, 20.0), rand(0.0, 5.0), rand(0.0, 20.0));
            grid.near(&brushes, lo, hi, &mut near);
            let all: Vec<u32> = brushes
                .iter()
                .enumerate()
                .filter(|(_, b)| !(b.max.cmplt(lo).any() || b.min.cmpgt(hi).any()))
                .map(|(i, _)| i as u32)
                .collect();
            assert_eq!(near, all);
        }
    }
}
