//! Core vocabulary shared by every game plugin. Keep this small: a concept
//! moves in here only after two or three games need it (README, "rule of
//! three").

use bevy::prelude::*;

pub mod brush_grid;
pub mod character_state;
pub mod damage;
pub mod hitbox;
pub mod intent;
pub mod map_controls;
pub mod prediction;
/// The version text's format (`format_version`); the build's own
/// `VERSION` is stamped where the app is built (`mashup::core::VERSION`),
/// so a commit doesn't recompile this crate.
pub mod version;

pub use brush_grid::*;
pub use character_state::*;
pub use damage::*;
pub use hitbox::*;
pub use intent::*;
pub use map_controls::*;
pub use prediction::*;

#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[reflect(Component)]
pub struct Team(pub u8);

/// A character's own dice seed, for rolls that should repeat run to run
/// (bots' route noise and choices, buying, bullet spread). Entity ids
/// won't do: every spawn shifts them, resources included (since Bevy
/// 0.19 resources are entities). Bots get one from their number and team
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

/// A character says a radio command to its team (Counter-Strike's radio):
/// `command` is the game's console name for it (`coverme`, `enemyspot`,
/// ...; `fireinhole` for a thrown grenade). The client resolves it through
/// the map's `map::radio::RadioCommands` and plays it if it hears it.
#[derive(Message, Clone, Debug, PartialEq)]
pub struct Radio {
    pub sender: Entity,
    pub command: String,
}

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

/// When the dead local player comes back, as the rules know it: set by
/// rules that respawn the dead a fixed time after their death and say so
/// (the minigame runtime: `chimera_sim::minigame::hud::update`, from its
/// definition's `respawn.delay`). `at` is `SimClock` seconds (a network
/// client's own clock, as its `Dead::since`); `delay` the whole wait from
/// the death. None while alive, or when nothing respawns the player on a
/// timer (plain CS:S's and Combat Arms' rules leave it unset: their death
/// views keep their own timings and screens). Read by the shell's death
/// view (`client::spectate`, which fits CS:S's death and freeze cams into
/// the wait) and its countdown (`client::respawn_countdown`).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct RespawnDue(pub Option<RespawnAt>);

/// A timed respawn: at `at` (`SimClock` s), `delay` s after the death.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RespawnAt {
    pub at: f64,
    pub delay: f64,
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

/// What this process is in a game (docs/archive/plans/multiplayer.md):
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

/// `mp_playerid` (the server's, sent to clients): whose names the target
/// ID shows: 0 everyone's, 1 teammates' only, 2 nobody's.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerIdMode(pub u8);

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
        app.init_resource::<ActionRegistry>()
            .register_type::<Intent>()
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
            .add_message::<RegionHit>()
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
            .init_resource::<RespawnDue>()
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
