//! Combat Arms' bots (specs/combat_arms/bots.md): the computer soldiers of
//! Bot Deathmatch, as characters whose `Intent` comes from a brain (like
//! CS:S's bots), moving with Combat Arms' movement and firing its weapons.
//!
//! - `roster`: adding bots (`ca_bot_add`, the `MaxPlayer` quota), their
//!   profile and item set by the roster's dice ("Choosing the bots": the
//!   profile probe, the item set's slots by the draw's digits, the lists
//!   padded to different lengths), the team opposite the human (10), the
//!   kit given every life (`weapon::OwnKit`) with the gun type's ammunition
//!   multiplier, the ranged and thrown reach fixed at spawn (8.2, 8.5).
//!   They come in as players do (`rules::flow::enter_game`) and respawn by
//!   the mode's rules (the respawn request after 7 s, `rules::spawns`).
//! - `data`: data/bots.toml, each attribute template's numbers and the
//!   per-weapon AI rows by gun type (`BotLevel`: 0 Easy, 1 Normal, 2
//!   Veteran, 3 training).
//! - `math`: the formulas (rolls, the hit/miss cycle, aim points, the
//!   movement inaccuracy, the reaction delay, camping, weapon choice, the
//!   throw, attack moves, patrol links).
//! - `nav`: the volume path search on the map's navigation areas, which
//!   are generated from the map's collision (`chimera_world::map::
//!   nav_gen`) when the map ships none (`ensure_nav`).
//! - `brain`: the senses (5), goals and states (2: patrol, investigate,
//!   chase, attack, dodge), firing (7, 8), movement in a fight (9).
//!
//! Bots run on the server only, their senses and goals on the server's
//! 20 Hz frames (`grenades::SERVER_FRAME`, the dedicated server's rate:
//! Open question 1), their aim, movement keys and trigger every tick.
//!
//! Known shortcuts (docs/tech-debt.md; the most literal reading where the
//! spec is unclear, listed for its open questions):
//! - Open question 6: bots move and fire through the player's parts (the
//!   brain writes `Intent`, as the task's design asks), not at constant
//!   template speeds confined to AI volumes: they run at Combat Arms'
//!   movement speed (the template's run and jump speeds are data only),
//!   jump and fall as players, and nothing refuses a step off the areas.
//!   Their shots go through the weapon frame, with `weapons::ExactAim`
//!   taking the spread cone, the recoil kick and extra pellets away (one
//!   ray per shot, Quirk 7), so each round goes where the brain aims; the
//!   fire rate is the weapon's own (no fire animation keys, Open question
//!   3), the server's fire delay applies; damage is the player's (zone
//!   factors included, though the spec says AI shooters never get them).
//! - Navigation: no AI volumes are read from the maps (their objects
//!   aren't in `MapData`; the readable WORLDS3 map costarecon_cp has 86
//!   `AIVolume` and 8 `AINodePatrol` objects, so reading them is left for
//!   when `game::world` can hand objects over); the areas are generated
//!   from the collision (0.5 m cells merged into rectangles, rises up to
//!   0.8 m with a jump, `brain::nav_params`), so the 1024-volume limit,
//!   the 48 u link minimum (generated rectangles meet along ragged edges),
//!   doors, ladders and teleport volumes don't apply. Patrol nodes: none
//!   either; the patrol graph is ours (`nav::PatrolGraph`: area centres
//!   7 m apart, linked within 16 m of path), with the spec's link choice
//!   (unvisited, least recently visited, straight-on bonus) and no waits or
//!   guard nodes. A bot that makes no progress for 1.5 s plans again and
//!   jumps (the spec has no unstick logic, Open question 11).
//! - Senses: a character's torso is 0.2 m above its centre, its head 0.6
//!   m (no skeletons); the eye is the movement's; the sight shortcut through
//!   areas is the floor-level straight walk; footsteps are posted every
//!   0.4 s while a character moves on the ground faster than 100 u/s
//!   (no footstep events yet), surface noise 1; "false stimulations" aren't
//!   counted; gunfire is heard by enemies only (ally gunfire off). Grenade
//!   danger comes from grenades thrown by players (not bots) that touched
//!   something or rest, read from `grenades::CaGrenade`; the dodge tries
//!   the five directions away from it at 45 degree steps (the spec's
//!   "6" minus the three toward it).
//! - Not there: placed devices (mines, claymores: no such items yet), so no
//!   mine drops while patrolling and nothing but characters as targets;
//!   bot chat; the "catch breath" stop lasts 1 s; the crouch a teammate
//!   asks for lasts the crouch hold (10 s); a roll is a fast sidestep;
//!   the knife is the player's knife, swung from within 90 u (its reach is
//!   105 u, the bot's 130 u); a reload's attack move starts with the reload
//!   key, not the animation; spawn protection and the respawn wait are the
//!   mode's (4 s, game_modes.md), not the bot base's 3 s; no bodies.
//! - Profiles: the install's profile file isn't read: a bot is
//!   `AI_<Team>_<profile>`, its archetype the profile number mod 3, its
//!   item sets ours (data/bots.toml), the draw ρ our seeded dice (not the
//!   clock plus `rand()`). With no human, a bot joins the smaller team, so
//!   bots fight bots.
//!
//! The lift of CS:S's bot brain into Chimera (L16) is not done: Combat
//! Arms' brain shares no rule with CS:S's (no turn rate, a hit/miss cycle
//! instead of aim error, squared path costs over areas, no team plans), so
//! only the navigation mesh (already Chimera's) and the new mesh generator
//! are shared.

pub mod brain;
pub mod data;
pub mod math;
pub mod nav;
pub mod roster;

use bevy::prelude::*;

pub use data::{BotData, GunAi, Level, Template};
pub use roster::{add_bot, auto_team, kick};

use crate::{
    console::{ConsoleAppExt, resource_cvar},
    core::SimSet,
};
use math::{Dice, HitCycle, Tracking};

/// The settings: `MaxPlayer` (bots the server keeps), `BotLevel`.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct BotSettings {
    /// `MaxPlayer`: the number of bots.
    pub quota: u32,
    /// Whether the quota was set (the server keeps it only then).
    pub quota_set: bool,
    /// `BotLevel`: 0 Easy, 1 Normal, 2 Veteran (3: training).
    pub level: u8,
    /// `ca_bot_stop`: 1 keeps every bot's movement keys off (they still
    /// see, aim and fire).
    pub stop: u8,
    /// `ca_bot_dont_shoot`: 1 keeps their fingers off the trigger.
    pub dont_shoot: u8,
    /// The roster's dice seed.
    pub seed: u64,
}

/// What a bot is doing (2).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum BotState {
    /// The idle behaviour: walking the patrol graph.
    #[default]
    Patrol,
    /// Going to a heard spot (or looking at it), until `until`.
    Investigate { spot: Vec3, until: f64, look_only: bool },
    /// Pursuing the target, knowing where it is, until `until`; standing
    /// for breath until `breath`.
    Chase { until: f64, breath: f64 },
    /// Fighting the target since `since`.
    Attack { since: f64 },
    /// Moving away from a grenade to `to`.
    Dodge { to: Vec3, until: f64 },
}

impl BotState {
    /// Patrolling or investigating: the notice rolls apply, the FOV is
    /// relaxed.
    pub fn relaxed(&self) -> bool {
        matches!(self, BotState::Patrol | BotState::Investigate { .. })
    }
}

/// A heard stimulus waiting for its reaction delay (5.1: counted in sense
/// passes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pending {
    pub at_pass: u64,
    pub spot: Vec3,
    pub gunfire: bool,
}

/// A bot: its brain's state. Positions in metres (engine), the spec's
/// numbers in world units where named so.
#[derive(Component, Clone, Debug)]
pub struct CaBot {
    pub profile: usize,
    pub template: Template,
    pub dice: Dice,
    /// The kit: mid primary, long primary, secondary, melee, grenade, mine.
    pub kit: roster::Kit,
    /// This life's kit has been set up (`roster::arm`).
    pub armed: bool,
    /// The ranged and thrown reach (u), fixed at the first spawn.
    pub reach_fixed: bool,
    pub ranged_reach: f32,
    pub thrown_reach: f32,

    pub state: BotState,
    pub target: Option<Entity>,
    /// The target's last sight result: visible, only the head.
    pub visible: bool,
    pub head_only: bool,
    pub tracking: Tracking,
    pub cycle: HitCycle,
    /// The current shot's aim offset from the base point (u): None until
    /// drawn for the next shot.
    pub aim_offset: Option<Vec3>,
    /// Exact aim for this shot (inside the full-accuracy zone).
    pub exact: bool,

    // Senses.
    /// Sense passes run.
    pub passes: u64,
    /// The sight cooldown ends (patrolling, 5.2).
    pub sight_cooldown: f64,
    /// The footstep sense's stimulation, the newest step it took.
    pub footsteps: f32,
    pub last_step: u64,
    pub last_gunfire: u64,
    pub last_call: u64,
    pub pending: Option<Pending>,

    // Attack.
    /// When the attack goal last went off; when firing is allowed.
    pub attack_off: f64,
    pub fire_at: f64,
    /// While the target is hidden in an attack: when the attack ends.
    pub chase_timer: Option<f64>,
    pub held: Option<Entity>,
    pub burst_left: u32,
    pub pause_until: f64,
    pub last_clip: Option<u32>,
    /// Shots before a grenade (8.5).
    pub grenades: u32,
    pub next_move: f64,
    pub move_to: Option<Vec3>,
    pub move_until: f64,
    pub dodge_next: f64,
    /// Hit since the last frame (9.2's dodge check).
    pub dodge_hit: bool,
    pub crouch_until: f64,
    /// Let go of the trigger this tick (after a shot, for semi-automatic
    /// weapons).
    pub release: bool,

    // Grenade danger (5.4).
    pub danger: Option<(Entity, Vec3, f64)>,

    // Navigation.
    pub route: Vec<Vec3>,
    pub route_goal: Option<Vec3>,
    pub patrol_from: Option<usize>,
    pub patrol_to: Option<usize>,
    /// The patrol node it last left (not taken back unless a dead end).
    pub patrol_came: Option<usize>,
    pub stuck_check: (f64, Vec3),
    pub jump: bool,
}

impl CaBot {
    pub fn new(profile: usize, template: Template, seed: u64, kit: roster::Kit, data: &BotData) -> Self {
        Self {
            profile,
            template,
            dice: Dice::new(seed),
            kit,
            armed: false,
            reach_fixed: false,
            ranged_reach: 0.0,
            thrown_reach: 0.0,
            state: BotState::Patrol,
            target: None,
            visible: false,
            head_only: false,
            tracking: Tracking::default(),
            cycle: HitCycle::default(),
            aim_offset: None,
            exact: false,
            passes: 0,
            sight_cooldown: 0.0,
            footsteps: 0.0,
            last_step: 0,
            last_gunfire: 0,
            last_call: 0,
            pending: None,
            attack_off: 0.0,
            fire_at: 0.0,
            chase_timer: None,
            held: None,
            burst_left: 0,
            pause_until: 0.0,
            last_clip: None,
            grenades: data.template.grenade_shots.at(template) as u32,
            next_move: 0.0,
            move_to: None,
            move_until: 0.0,
            dodge_next: 0.0,
            dodge_hit: false,
            crouch_until: 0.0,
            release: false,
            danger: None,
            route: Vec::new(),
            route_goal: None,
            patrol_from: None,
            patrol_to: None,
            patrol_came: None,
            stuck_check: (0.0, Vec3::ZERO),
            jump: false,
        }
    }
}

/// The bots, their console and their systems (the server's).
pub struct CaBotsPlugin;

impl Plugin for CaBotsPlugin {
    fn build(&self, app: &mut App) {
        let server = crate::core::authoritative;
        app.init_resource::<BotSettings>()
            .init_resource::<BotData>()
            .init_resource::<roster::Roster>()
            .init_resource::<brain::Stimuli>()
            .init_resource::<brain::PatrolClock>()
            .init_resource::<brain::BotFrames>()
            .add_systems(
                FixedUpdate,
                (
                    (roster::keep_quota, brain::ensure_nav, roster::arm, brain::think)
                        .chain()
                        .before(SimSet::Rules)
                        .before(crate::weapon::SelectWeapons),
                    (brain::post_stimuli, brain::camping)
                        .chain()
                        .after(SimSet::Weapons)
                        .after(crate::core::apply_damage),
                )
                    .run_if(server),
            );
        resource_cvar::<BotSettings, u32>(app, "MaxPlayer", "Bots the server keeps in the game (Bot Deathmatch's player count).", |s| {
            &mut s.quota
        });
        resource_cvar::<BotSettings, u8>(
            app,
            "BotLevel",
            "Level of bots added from now on: 0 Easy, 1 Normal, 2 Veteran (3 training).",
            |s| &mut s.level,
        );
        resource_cvar::<BotSettings, u8>(app, "ca_bot_stop", "1: bots stand still (they still see, aim and fire).", |s| &mut s.stop);
        resource_cvar::<BotSettings, u8>(app, "ca_bot_dont_shoot", "1: bots never fire.", |s| &mut s.dont_shoot);
        app.console_command(
            "ca_bot_add",
            "ca_bot_add [alpha|bravo]: add a bot at BotLevel (default team: opposite the human); raises MaxPlayer.",
            |w, a| {
                let team = match a.first().map(|s| s.to_ascii_lowercase()) {
                    None => None,
                    Some(t) if t == "alpha" || t == "1" || t == "jsf" => Some(roster::ALPHA),
                    Some(t) if t == "bravo" || t == "2" || t == "creed" => Some(roster::BRAVO),
                    Some(t) => return Err(format!("bad team {t}")),
                };
                let e = add_bot(w, team).ok_or("no bot added")?;
                {
                    let mut s = w.resource_mut::<BotSettings>();
                    s.quota += 1;
                    s.quota_set = true;
                }
                let name = w.get::<Name>(e).map_or_else(|| e.to_string(), |n| n.as_str().to_string());
                Ok(Some(format!("added {name}")))
            },
        )
        .console_command(
            "ca_bot_kick",
            "ca_bot_kick [all|<name>]: remove every bot (MaxPlayer 0) or the one by that name.",
            |w, a| {
                let wanted = a.join(" ");
                let all = wanted.is_empty() || wanted.eq_ignore_ascii_case("all");
                let bots: Vec<Entity> = w
                    .query_filtered::<(Entity, Option<&Name>), With<CaBot>>()
                    .iter(w)
                    .filter(|(_, n)| all || n.is_some_and(|n| n.as_str().eq_ignore_ascii_case(&wanted)))
                    .map(|(e, _)| e)
                    .collect();
                if !all && bots.is_empty() {
                    return Err(format!("no bot \"{wanted}\""));
                }
                let n = bots.len();
                for b in bots {
                    kick(w, b);
                }
                let mut s = w.resource_mut::<BotSettings>();
                s.quota = if all { 0 } else { s.quota.saturating_sub(n as u32) };
                s.quota_set = true;
                Ok(Some(format!("kicked {n} bots")))
            },
        )
        .console_command("ca_bots", "The bots: name, team, template, state, target, health.", |w, _| {
            let mut lines = Vec::new();
            let mut q = w.query::<(&CaBot, Option<&Name>, &crate::core::Team, &crate::core::Health)>();
            for (b, n, t, h) in q.iter(w) {
                lines.push(format!(
                    "{} team {} {} {:?} target {:?} hp {:.0}",
                    n.map_or("?", |n| n.as_str()),
                    t.0,
                    b.template.short(),
                    b.state,
                    b.target,
                    h.current * 100.0
                ));
            }
            Ok(Some(if lines.is_empty() { "no bots".into() } else { lines.join("\n") }))
        });
    }
}
