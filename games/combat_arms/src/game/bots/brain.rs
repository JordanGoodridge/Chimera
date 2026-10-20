//! The bots' brain (specs/combat_arms/bots.md 1, 2, 4-10): the senses
//! (one sense pass per server frame: the nearest enemy's sight test, else
//! the first heard stimulus; the notice and hearing rolls while relaxed),
//! the goals as states (patrol, investigate, chase, attack, dodge), and
//! what an attacking bot does: face the aim point, wait the reaction
//! delay, fire bursts by the clip, the hit/miss cycle per shot, reload,
//! switch, knife, throw a grenade after its count of shots, attack moves,
//! dodges and crouching. Everything ends in the bot's `Intent` (aim, keys,
//! trigger) and its `Inventory::wanted`; a throw is the grenade spawned
//! with the hang-time solution (`grenades::spawn_grenade`).
//!
//! `post_stimuli` (after the weapons and damage): gunfire from every
//! `Fired` round of a gun (radius: the fitted weapon's fire sound,
//! `attachments::CaMods`), footsteps, the shot's bookkeeping (a new aim
//! for the next shot, the grenade count, letting go of the trigger), and
//! damage (a bot that hurt a player resets its grenade count; a bot hit
//! moves and may dodge). `camping` keeps each human's camping time (5.5).
//!
//! Units: positions in metres; the spec's distances in world units
//! (`inventory::UNIT` metres each) where the code names them so.

use std::collections::{HashMap, HashSet};

use avian3d::prelude::*;
use bevy::{ecs::system::SystemState, prelude::*};

use super::{
    BotSettings, BotState, CaBot, Pending,
    data::{BotData, GunAi},
    map_ai::MapAi,
    math::{self, Choice, MOVES, Move},
    nav,
};
use super::super::{
    attachments::CaMods,
    grenades::{CaGrenade, SERVER_FRAME},
    inventory::UNIT,
    rules::{CaMatch, Phase, spawns::CaHeld},
};
use crate::{
    core::{Damage, Dead, FreezeTime, Health, Intent, MovementState, SimClock, Spectating, Team},
    map::{SeeThrough, nav::NavMesh},
    weapon::{Inventory, Magazine, Weapon, WeaponEvent, WeaponEventKind, grenade::Throwable},
};

/// Sight range (u), `SeeEnemyDistance`.
pub const SIGHT_RANGE: f32 = 3000.0;
/// Inside this (u) the field of view is 180° even when relaxed.
pub const NO_FOV_RADIUS: f32 = 256.0;
/// Relaxed field of view: dot with the eye's forward, and with up.
pub const RELAXED_DOT: f32 = 0.342;
pub const RELAXED_UP_DOT: f32 = 0.5;
/// The sight cooldown while patrolling (s).
pub const SIGHT_COOLDOWN: f64 = 1.5;
/// Hearing: the template's distance for footsteps (u), the footstep radius
/// per unit of surface noise, vertical windows.
pub const HEARING: f32 = 1500.0;
pub const FOOTSTEP_RADIUS: f32 = 256.0;
pub const FOOTSTEP_WINDOW: f32 = 128.0;
pub const GUNFIRE_WINDOW: f32 = 512.0;
/// The ally call's radius (u).
pub const ALLY_CALL: f32 = 512.0;
/// Stimuli lifetimes (s).
pub const GUNFIRE_LIFE: f64 = 1.0;
pub const FOOTSTEP_LIFE: f64 = 0.5;
pub const CALL_LIFE: f64 = 1.0;
/// The reaction re-arm gap (s).
pub const REARM_GAP: f64 = 5.0;
/// The investigate goal's decay time (s).
pub const INVESTIGATE_TIME: f64 = 10.0;
/// The bot's knife reach (u, 8.4: it closes in until within it, its
/// swings reach that far, `inventory::KnifeReach`).
pub const KNIFE_RANGE: f32 = 130.0;
/// The chase path's rotating offset (u).
pub const CHASE_OFFSET: f32 = 96.0;
/// The camping radius (u) and onset (s).
pub const CAMP_RADIUS: f32 = 120.0;
/// Crouch on engaging: chance, hold (s).
pub const CROUCH_CHANCE: f32 = 0.01;
pub const CROUCH_HOLD: f64 = 10.0;
/// Dodging fire: the check interval, the delay after a dodge (s), the
/// roll's distance (u), "aiming at me" both ways.
pub const DODGE_CHECK: f64 = 0.25;
pub const DODGE_DELAY: f64 = 3.0;
pub const ROLL: f32 = 140.0;
pub const AIMING_DOT: f32 = 0.95;
/// A teammate closer than this (u) isn't asked to crouch.
pub const CROUCH_ASK: f32 = 48.0;
/// The bot's radius for its moves (u): the fallback hull's.
pub const RADIUS: f32 = 15.0;
/// The grenade dodge: point at this times the blast radius; notice range
/// beyond the radius (u); moves needed before a new notice (u).
pub const DODGE_FACTOR: f32 = 1.1;
pub const DANGER_MARGIN: f32 = 100.0;
pub const DANGER_MOVED: f32 = 5.0;
/// The throw: hang time (s), gravity (u/s²).
pub const HANG_TIME: f32 = 0.5;
pub const GRAVITY: f32 = -1600.0;
/// The sight points (5.2, Open question 18): the target's object position
/// (its hull centre) and its `Head` node, the eye the bot's own hull x/z
/// at its `Head` node's height; the node's height above the centre is the
/// spec's measured standing pose (`Head` at 29.03 u), m.
pub const TORSO: f32 = 0.0;
pub const HEAD: f32 = 29.03 * UNIT;
pub const EYE: f32 = HEAD;

/// A kind of stimulus (5.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Gunfire,
    AllyCall,
    Footstep,
}

/// A stimulus in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stimulus {
    pub id: u64,
    pub kind: Kind,
    /// Where (m).
    pub at: Vec3,
    /// Its radius (u).
    pub radius: f32,
    pub source: Entity,
    pub team: u8,
    pub until: f64,
}

/// The global stimulus list.
#[derive(Resource, Clone, Debug, Default)]
pub struct Stimuli {
    pub list: Vec<Stimulus>,
    pub next: u64,
}

impl Stimuli {
    /// Post a stimulus (from now for its kind's lifetime).
    pub fn post(&mut self, kind: Kind, at: Vec3, radius: f32, source: Entity, team: u8, now: f64) {
        self.next += 1;
        let life = match kind {
            Kind::Gunfire => GUNFIRE_LIFE,
            Kind::AllyCall => CALL_LIFE,
            Kind::Footstep => FOOTSTEP_LIFE,
        };
        self.list.push(Stimulus {
            id: self.next,
            kind,
            at,
            radius,
            source,
            team,
            until: now + life,
        });
    }
}

/// When each patrol node (area) was last reached (ms), and the chase
/// offsets' shared cycle.
#[derive(Resource, Clone, Debug, Default)]
pub struct PatrolClock {
    pub reached: HashMap<usize, f64>,
    pub chase_cycle: u32,
}

/// The brains' shared copy of the navigation mesh (`ensure_nav`).
#[derive(Resource, Clone)]
pub struct BotNav {
    pub id: usize,
    pub mesh: std::sync::Arc<NavMesh>,
    pub patrol: std::sync::Arc<nav::PatrolGraph>,
}

/// The bots' server frames (one sense pass and goal update each).
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct BotFrames(pub u64);

/// A human's camping time (5.5): within `CAMP_RADIUS` of the anchor.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct CaCamping {
    pub anchor: Vec3,
    pub time: f32,
}

/// A character's leg animation and its time at the last update (the
/// footstep keys passed since).
#[derive(Component, Clone, Debug, Default)]
pub struct StepClock {
    pub name: String,
    pub time: u32,
}

// ---------------------------------------------------------------------------
// Stimuli and camping

/// Gunfire, footsteps, the shots' and hits' bookkeeping (see the module
/// doc).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn post_stimuli(
    mut events: MessageReader<WeaponEvent>,
    mut damage: MessageReader<Damage>,
    weapons: Query<(&Weapon, Option<&CaMods>, Has<Magazine>)>,
    mut bots: Query<&mut CaBot>,
    chars: Query<
        (Entity, &Team, &Transform, &MovementState, Option<&super::super::characters::CaAnim>, Option<&StepClock>),
        (Without<Dead>, With<Intent>),
    >,
    intents: Query<(), With<Intent>>,
    filters: Query<&crate::core::DamageFilter>,
    teams: Query<&Team>,
    mut stimuli: ResMut<Stimuli>,
    clock: Res<SimClock>,
    data: Res<BotData>,
    mut commands: Commands,
) {
    let now = clock.now;
    stimuli.list.retain(|s| s.until > now);
    for e in events.read() {
        if e.replay {
            continue;
        }
        let WeaponEventKind::Fired { origin, .. } = e.kind else { continue };
        let Ok((w, fitted, gun)) = weapons.get(e.weapon) else { continue };
        if gun {
            let radius = fitted.map_or(2000, |f| f.fire_sound().radius) as f32;
            let team = teams.get(e.owner).map_or(0, |t| t.0);
            stimuli.post(Kind::Gunfire, origin, radius, e.owner, team, now);
        }
        if let Ok(mut b) = bots.get_mut(e.owner) {
            // The next round no sooner than the fire animation allows
            // (8.3).
            let class = super::super::characters::records::data().weapon(Some(w.id)).ani_type;
            b.next_shot = now + math::fire_interval_ms(class) as f64 / 1000.0;
            b.aim_offset = None;
            b.release = true;
            if gun && data.gun_type(w.id) != Some("Knife") {
                b.grenades = b.grenades.saturating_sub(1);
            }
        }
    }
    for d in damage.read() {
        if d.amount <= 0.0 {
            continue;
        }
        if let Some(a) = d.attacker
            && a != d.target
            && intents.contains(d.target)
            && teams.get(a).ok() != teams.get(d.target).ok()
            && !bots.contains(d.target)
            && !filters.get(d.target).is_ok_and(|f| f.blocked.contains(&d.kind))
            && let Ok(mut b) = bots.get_mut(a)
        {
            b.grenades = data.template.grenade_shots.at(b.template) as u32;
        }
        if let Ok(mut b) = bots.get_mut(d.target) {
            // When hit: an attack move at once, and a dodge if allowed.
            b.next_move = b.next_move.min(now);
            b.jump = false;
            b.passes = b.passes.max(1);
            b.dodge_hit = true;
        }
    }
    // Footsteps: at the keys of each character's (server-side) leg
    // animation (5.3), on the ground or a ladder.
    for (e, team, t, state, anim, step) in &chars {
        let Some(anim) = anim else { continue };
        let d = &anim.driver;
        let name = d.name_of(&d.lower).to_string();
        let time = d.lower.time;
        let length = d.list.entries.get(d.lower.current).map_or(0, |e| e.length());
        let last = step.map(|s| (s.name.clone(), s.time));
        commands.entity(e).insert(StepClock { name: name.clone(), time });
        let Some((last_name, before)) = last.filter(|(n, _)| *n == name) else { continue };
        let _ = last_name;
        if !(state.on_ground || state.on_ladder) {
            continue;
        }
        if math::footstep_keys(&name).iter().any(|k| math::passed_key(before, time, length, *k)) {
            stimuli.post(Kind::Footstep, t.translation, FOOTSTEP_RADIUS, e, team.0, now);
        }
    }
}

/// Each human's camping time (5.5): grows within `CAMP_RADIUS` of the
/// anchor, starts again (a new anchor) beyond it or at death.
#[allow(clippy::type_complexity)]
pub fn camping(
    mut humans: Query<(Entity, &Transform, Has<Dead>, Option<&mut CaCamping>), (With<Intent>, With<Team>, Without<CaBot>)>,
    clock: Res<SimClock>,
    mut commands: Commands,
) {
    let dt = clock.dt();
    for (e, t, dead, camp) in &mut humans {
        let p = t.translation;
        match camp {
            None => {
                commands.entity(e).insert(CaCamping { anchor: p, time: 0.0 });
            }
            Some(mut c) => {
                if dead || c.anchor.distance(p) / UNIT > CAMP_RADIUS {
                    *c = CaCamping { anchor: p, time: 0.0 };
                } else {
                    c.time += dt;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Navigation mesh

/// The map's navigation mesh: generated from the collision
/// (`map::nav_gen`) once bots play on a map that has none, from its
/// spawn points and characters.
pub fn ensure_nav(world: &mut World) {
    if world.contains_resource::<NavMesh>() {
        // The brains' shared copy, kept with the map's mesh.
        let stale = world.get_resource::<BotNav>().is_none_or(|n| n.id != world.resource::<NavMesh>().areas.len())
            || world.is_resource_changed::<NavMesh>();
        if stale {
            let mesh = world.resource::<NavMesh>().clone();
            // The map's patrol nodes, else ours.
            let patrol = MapAi::of(world)
                .and_then(|ai| ai.patrol_graph(&mesh))
                .unwrap_or_else(|| nav::PatrolGraph::build(&mesh));
            world.insert_resource(BotNav {
                id: mesh.areas.len(),
                patrol: std::sync::Arc::new(patrol),
                mesh: std::sync::Arc::new(mesh),
            });
        }
        return;
    }
    world.remove_resource::<BotNav>();
    if world.resource::<SimClock>().tick < 3 {
        return;
    }
    if world.query_filtered::<(), With<CaBot>>().iter(world).next().is_none() {
        return;
    }
    let mut seeds: Vec<Vec3> = world
        .query_filtered::<&Transform, With<crate::core::SpawnPoint>>()
        .iter(world)
        .map(|t| t.translation)
        .collect();
    seeds.extend(
        world
            .query_filtered::<&Transform, (With<Intent>, Without<Dead>)>()
            .iter(world)
            .map(|t| t.translation - Vec3::Y * 0.8),
    );
    let ignore: HashSet<Entity> = moving_bodies(world);
    // The map's AI volumes where it has them (`map_ai`).
    if let Some(mesh) = MapAi::of(world).and_then(|ai| {
        let mut state = SystemState::<SpatialQuery>::new(world);
        let q = state.get(world).unwrap();
        ai.nav_mesh(UNIT, |min, max| volume_floor(&q, &ignore, min, max))
    }) {
        info!("bots: the map's {} AI volumes as navigation areas", mesh.areas.len());
        world.insert_resource(mesh);
        return;
    }
    let mesh = {
        let mut state = SystemState::<SpatialQuery>::new(world);
        let q = state.get(world).unwrap();
        crate::map::nav_gen::generate(&q, &|e| ignore.contains(&e), &seeds, &nav_params())
    };
    info!("bots: generated a navigation mesh of {} areas", mesh.areas.len());
    world.insert_resource(mesh);
}

/// How far under an AI volume's box its floor may be (m, ours).
pub const FLOOR_BELOW: f32 = 1.5;

/// The floor inside an AI volume's box (`min`, `max`, m): the first
/// surface a ray down from the top of the box meets at its centre, within
/// the box or under it within `FLOOR_BELOW` (costarecon_cp's boxes sit
/// 0.2-0.8 m above their floors); None where there is none.
fn volume_floor(q: &SpatialQuery, ignore: &HashSet<Entity>, min: Vec3, max: Vec3) -> Option<f32> {
    let c = (min + max) * 0.5;
    let top = Vec3::new(c.x, max.y, c.z);
    let reach = max.y - min.y + FLOOR_BELOW;
    q.cast_ray_predicate(top, Dir3::NEG_Y, reach, true, &sight_filter(), &|e| !ignore.contains(&e))
        .map(|h| top.y - h.distance)
}

/// Ours: the highest rise between navigation cells (m), and a rise above
/// this one (m) ahead within `JUMP_AHEAD` (m) makes a walking bot jump.
pub const JUMP_UP: f32 = 0.8;
pub const JUMP_RISE: f32 = 0.35;
pub const JUMP_AHEAD: f32 = 1.2;

/// What sight, the line of fire and the grenade's line meet: the gameplay
/// layers (`core::SOLID_LAYERS`: not players-only clips or ragdolls;
/// traces-only detail does block).
fn sight_filter() -> SpatialQueryFilter {
    SpatialQueryFilter::from_mask(crate::core::SOLID_LAYERS)
}

/// The generator's walker: Chimera's 1.8 m hull, the fallback hull's half
/// width, rises up to `JUMP_UP`.
pub fn nav_params() -> crate::map::nav_gen::NavGenParams {
    let m = super::super::movement::movement_data();
    crate::map::nav_gen::NavGenParams {
        // A stair step or a jump up (the walker jumps up to the next point,
        // `walk`): the maps have ledges a little above the stair step.
        step: JUMP_UP,
        radius: super::super::rules::spawns::HULL.x * m.meters_per_unit,
        max_drop: 3.0,
        ..Default::default()
    }
}

/// Bodies that aren't the world: characters and anything not static.
fn moving_bodies(world: &mut World) -> HashSet<Entity> {
    let mut out: HashSet<Entity> = world.query_filtered::<Entity, With<Intent>>().iter(world).collect();
    out.extend(
        world
            .query::<(Entity, &RigidBody)>()
            .iter(world)
            .filter(|(_, b)| !matches!(b, RigidBody::Static))
            .map(|(e, _)| e),
    );
    out
}

// ---------------------------------------------------------------------------
// The brain

/// A character as the brains see it this tick.
#[derive(Clone, Copy, Debug)]
pub struct Char {
    pub e: Entity,
    pub team: u8,
    /// Hull centre (m).
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub alive: bool,
    pub bot: bool,
    pub crouching_attack: Option<Entity>,
}

impl Char {
    pub fn eye(&self) -> Vec3 {
        self.pos + Vec3::Y * EYE
    }
    pub fn feet(&self) -> Vec3 {
        self.pos - Vec3::Y * 0.9
    }
    pub fn torso(&self) -> Vec3 {
        self.pos + Vec3::Y * TORSO
    }
    pub fn head(&self) -> Vec3 {
        self.pos + Vec3::Y * HEAD
    }
    pub fn forward(&self) -> Vec3 {
        Vec3::new(-self.yaw.sin(), 0.0, -self.yaw.cos())
    }
    pub fn look(&self) -> Vec3 {
        Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0) * Vec3::NEG_Z
    }
}

/// One of a bot's weapons now.
#[derive(Clone, Copy, Debug)]
struct Gun {
    e: Entity,
    id: &'static str,
    clip: u32,
    reserve: u32,
    count: Option<u32>,
}

impl Gun {
    fn has_ammo(&self) -> bool {
        self.count.map_or(self.clip + self.reserve > 0, |c| c > 0)
    }
}

/// A live grenade the bots may dodge.
#[derive(Clone, Copy, Debug)]
struct Danger {
    e: Entity,
    pos: Vec3,
    radius: f32,
    player_thrown: bool,
    team: u8,
    touched: bool,
}

/// What the world gives a brain for a tick.
struct Ctx<'a> {
    now: f64,
    frame: bool,
    q: &'a SpatialQuery<'a, 'a>,
    chars: &'a [Char],
    /// Bodies sight rays pass (characters and moving bodies).
    see_through: &'a HashSet<Entity>,
    /// Moving bodies that aren't characters (the weapon ray passes them).
    loose: &'a HashSet<Entity>,
    mesh: Option<&'a NavMesh>,
    graph: Option<&'a nav::PatrolGraph>,
    data: &'a BotData,
    stimuli: &'a Stimuli,
    patrol: &'a PatrolClock,
    camping: &'a HashMap<Entity, f32>,
    dangers: &'a [Danger],
    settings: &'a BotSettings,
}

/// What a brain does this tick.
#[derive(Default)]
struct Out {
    intent: Option<IntentOut>,
    wanted: Option<Entity>,
    throw: Option<(Entity, Vec3, Vec3)>,
    ask_crouch: Option<Entity>,
    ally_call: Option<Vec3>,
    reached: Option<usize>,
    chase_step: bool,
}

#[derive(Clone, Copy, Default)]
struct IntentOut {
    yaw: f32,
    pitch: f32,
    axis: Vec2,
    fire: bool,
    reload: bool,
    crouch: bool,
    jump: bool,
}

/// Every bot's update this tick (1, "Per-tick order"): senses and goals on
/// the server's frames, aim, keys and the trigger every tick.
pub fn think(world: &mut World) {
    let now = world.resource::<SimClock>().now;
    let frame = {
        let due = ((now + 1e-9) / SERVER_FRAME).floor() as u64;
        let mut f = world.resource_mut::<BotFrames>();
        let run = f.0 < due;
        f.0 = f.0.max(due);
        run
    };
    let bots: Vec<Entity> = world.query_filtered::<Entity, With<CaBot>>().iter(world).collect();
    if bots.is_empty() {
        return;
    }
    let settings = world.resource::<BotSettings>().clone();
    let frozen = world.resource::<FreezeTime>().0
        || world.get_resource::<CaMatch>().is_some_and(|m| {
            matches!(m.phase, Phase::RoundOver { .. } | Phase::Intermission { .. } | Phase::MatchOver)
        });

    let snap = Snap::take(world, now);
    let Snap { chars, see_through, loose, camping, dangers, data, stimuli, patrol, mesh, graph } = &snap;

    // The brains' decisions (the world read only).
    let mut outs: Vec<(Entity, CaBot, Out)> = Vec::new();
    {
        let mut state = SystemState::<SpatialQuery>::new(world);
        let q = state.get(world).unwrap();
        let ctx = Ctx {
            now,
            frame,
            q: &q,
            chars,
            see_through,
            loose,
            mesh: mesh.as_deref(),
            graph: graph.as_deref(),
            data,
            stimuli,
            patrol,
            camping,
            dangers,
            settings: &settings,
        };
        for &e in &bots {
            let Some(me) = chars.iter().find(|c| c.e == e).copied() else { continue };
            let mut bot = world.get::<CaBot>(e).unwrap().clone();
            let guns = guns_of(world, e);
            let active = world.get::<Inventory>(e).and_then(|i| i.active);
            let out = if !me.alive {
                dead(&mut bot);
                Out::default()
            } else if frozen {
                Out {
                    intent: Some(IntentOut {
                        yaw: me.yaw,
                        pitch: me.pitch,
                        ..default()
                    }),
                    ..default()
                }
            } else {
                decide(&ctx, &mut bot, &me, &guns, active)
            };
            outs.push((e, bot, out));
        }
    }

    // Apply.
    for (e, bot, out) in outs {
        if let Some(i) = out.intent {
            let mut intent = world.get_mut::<Intent>(e).unwrap();
            intent.yaw = i.yaw;
            intent.pitch = i.pitch.clamp(-1.55, 1.55);
            intent.move_axis = i.axis;
            intent.fire = i.fire;
            intent.reload = i.reload;
            intent.crouch = i.crouch;
            intent.jump = i.jump;
            intent.sprint = false;
            intent.walk = false;
            intent.secondary = false;
            intent.select = None;
            intent.last_weapon = false;
        }
        if let Some(w) = out.wanted
            && let Some(mut inv) = world.get_mut::<Inventory>(e)
            && inv.active != Some(w)
            && inv.wanted != Some(w)
        {
            inv.wanted = Some(w);
        }
        if let Some((w, from, v)) = out.throw {
            let left = world.get::<Throwable>(w).map_or(0, |t| t.count);
            if left > 0 {
                super::super::grenades::spawn_grenade(world, w, Some(e), from, v);
                if let Some(mut t) = world.get_mut::<Throwable>(w) {
                    t.count -= 1;
                }
            }
        }
        if let Some(mate) = out.ask_crouch
            && let Some(mut m) = world.get_mut::<CaBot>(mate)
        {
            m.crouch_until = m.crouch_until.max(now + CROUCH_HOLD);
        }
        if let Some(at) = out.ally_call {
            let team = world.get::<Team>(e).map_or(0, |t| t.0);
            world.resource_mut::<Stimuli>().post(Kind::AllyCall, at, ALLY_CALL, e, team, now);
        }
        if let Some(a) = out.reached {
            world.resource_mut::<PatrolClock>().reached.insert(a, now * 1000.0);
        }
        if out.chase_step {
            world.resource_mut::<PatrolClock>().chase_cycle += 1;
        }
        *world.get_mut::<CaBot>(e).unwrap() = bot;
    }
}

/// What the brains read of the world this tick.
struct Snap {
    chars: Vec<Char>,
    see_through: HashSet<Entity>,
    loose: HashSet<Entity>,
    camping: HashMap<Entity, f32>,
    dangers: Vec<Danger>,
    data: BotData,
    stimuli: Stimuli,
    patrol: PatrolClock,
    mesh: Option<std::sync::Arc<NavMesh>>,
    graph: Option<std::sync::Arc<nav::PatrolGraph>>,
}

impl Snap {
    fn take(world: &mut World, now: f64) -> Snap {
        let chars: Vec<Char> = world
            .query_filtered::<(Entity, &Team, &Transform, &Intent, &Health, Has<Dead>, Has<CaHeld>, Option<&CaBot>), Without<Spectating>>()
            .iter(world)
            .map(|(e, t, tr, i, h, dead, held, b)| Char {
                e,
                team: t.0,
                pos: tr.translation,
                yaw: i.yaw,
                pitch: i.pitch,
                alive: !dead && !held && h.current > 0.0,
                bot: b.is_some(),
                crouching_attack: b.and_then(|b| {
                    (b.crouch_until > now && matches!(b.state, BotState::Attack { .. })).then_some(b.target).flatten()
                }),
            })
            .collect();
        let loose: HashSet<Entity> = {
            let all = moving_bodies(world);
            let chars: HashSet<Entity> = chars.iter().map(|c| c.e).collect();
            all.difference(&chars).copied().collect()
        };
        // Sight passes characters, moving bodies and see-through
        // surfaces: the map's glass (5.2 step 6; `world::
        // GLASS_SURFACE_TYPES`).
        let mut see_through = moving_bodies(world);
        see_through.extend(world.query_filtered::<Entity, With<SeeThrough>>().iter(world));
        let camping: HashMap<Entity, f32> = world.query::<(Entity, &CaCamping)>().iter(world).map(|(e, c)| (e, c.time)).collect();
        let dangers: Vec<Danger> = world
            .query::<(Entity, &CaGrenade)>()
            .iter(world)
            .map(|(e, g)| {
                let thrower = g.thrower;
                Danger {
                    e,
                    pos: g.pos * g.unit,
                    radius: g.params.area_radius.max(g.params.over_time.map_or(0.0, |o| o.radius)),
                    player_thrown: thrower.is_some_and(|t| !chars.iter().any(|c| c.e == t && c.bot)),
                    team: thrower.and_then(|t| chars.iter().find(|c| c.e == t)).map_or(0, |c| c.team),
                    touched: g.touched || g.resting,
                }
            })
            .collect();
        let data = world.resource::<BotData>().clone();
        let stimuli = world.resource::<Stimuli>().clone();
        let patrol = world.resource::<PatrolClock>().clone();
        let mesh = world.get_resource::<BotNav>().map(|n| n.mesh.clone());
        let graph = world.get_resource::<BotNav>().map(|n| n.patrol.clone());
        Snap {
            chars,
            see_through,
            loose,
            camping,
            dangers,
            data,
            stimuli,
            patrol,
            mesh,
            graph,
        }
    }
}

/// Whether bot `bot` sees `target` now by the sight test (5.2; its state's
/// field of view, no notice roll): None when not, Some(true) when only the
/// head is visible.
pub fn sees(world: &mut World, bot: Entity, target: Entity) -> Option<bool> {
    let now = world.resource::<SimClock>().now;
    let alert = !world.get::<CaBot>(bot)?.state.relaxed();
    let settings = world.resource::<BotSettings>().clone();
    let snap = Snap::take(world, now);
    let mut state = SystemState::<SpatialQuery>::new(world);
    let q = state.get(world).unwrap();
    let ctx = Ctx {
        now,
        frame: false,
        q: &q,
        chars: &snap.chars,
        see_through: &snap.see_through,
        loose: &snap.loose,
        mesh: snap.mesh.as_deref(),
        graph: snap.graph.as_deref(),
        data: &snap.data,
        stimuli: &snap.stimuli,
        patrol: &snap.patrol,
        camping: &snap.camping,
        dangers: &snap.dangers,
        settings: &settings,
    };
    let me = snap.chars.iter().find(|c| c.e == bot)?;
    let t = snap.chars.iter().find(|c| c.e == target)?;
    sight(&ctx, me, t, alert)
}

fn guns_of(world: &World, e: Entity) -> Vec<Gun> {
    let Some(inv) = world.get::<Inventory>(e) else { return Vec::new() };
    inv.weapons
        .iter()
        .filter_map(|&w| {
            let id = world.get::<Weapon>(w)?.id;
            let m = world.get::<Magazine>(w);
            Some(Gun {
                e: w,
                id,
                clip: m.map_or(0, |m| m.clip),
                reserve: m.map_or(0, |m| m.reserve),
                count: world.get::<Throwable>(w).map(|t| t.count),
            })
        })
        .collect()
}

/// A dead bot forgets its fight; its next life's kit is set up again.
fn dead(bot: &mut CaBot) {
    bot.armed = false;
    if matches!(bot.state, BotState::Attack { .. }) {
        bot.attack_off = 0.0f64.max(bot.attack_off);
    }
    bot.state = BotState::Patrol;
    bot.target = None;
    bot.route.clear();
    bot.route_goal = None;
    bot.move_to = None;
    bot.patrol_to = None;
    bot.patrol_from = None;
    bot.patrol_came = None;
    bot.danger = None;
    bot.pending = None;
    bot.aim_offset = None;
    bot.crouch_until = 0.0;
}

fn u(m: f32) -> f32 {
    m / UNIT
}

/// The yaw and pitch looking from `from` at `to` (yaw 0 looks down −Z,
/// positive toward −X).
pub fn look_at(from: Vec3, to: Vec3) -> (f32, f32) {
    let d = to - from;
    let h = Vec2::new(d.x, d.z).length();
    ((-d.x).atan2(-d.z), d.y.atan2(h))
}

/// Movement keys for walking toward `dir` (horizontal) while facing `yaw`.
fn axis_toward(dir: Vec3, yaw: f32) -> Vec2 {
    let d = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
    let forward = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let a = Vec2::new(d.dot(right), d.dot(forward));
    if a.length() > 1e-3 { a / a.x.abs().max(a.y.abs()) } else { Vec2::ZERO }
}

/// The sight test (5.2) from `me` on `t`: None when not seen, else
/// whether only the head was.
fn sight(ctx: &Ctx, me: &Char, t: &Char, alert: bool) -> Option<bool> {
    if !t.alive {
        return None;
    }
    let eye = me.eye();
    let d = u(eye.distance(t.pos));
    if d >= SIGHT_RANGE {
        return None;
    }
    if d > NO_FOV_RADIUS {
        let dir = (t.pos - eye).normalize_or_zero();
        let fwd = me.forward();
        if alert {
            if Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero().dot(fwd) < 0.0 {
                return None;
            }
        } else if dir.dot(fwd) < RELAXED_DOT || dir.y > RELAXED_UP_DOT {
            return None;
        }
    }
    if let Some(mesh) = ctx.mesh
        && nav::straight(mesh, me.feet(), t.feet(), 0.4)
    {
        return Some(false);
    }
    let clear = |to: Vec3| {
        let d = to - eye;
        let len = d.length();
        let Ok(dir) = Dir3::new(d) else { return true };
        ctx.q
            .cast_ray_predicate(eye, dir, len, true, &sight_filter(), &|e| !ctx.see_through.contains(&e))
            .is_none()
    };
    if clear(t.pos) {
        return Some(false);
    }
    clear(t.head()).then_some(true)
}

/// The line of fire from `me`'s eye to `to`: None when clear, Some(Some(e))
/// when character `e` stands in it, Some(None) for the world.
fn fire_line(ctx: &Ctx, me: &Char, target: Entity, to: Vec3) -> Option<Option<Entity>> {
    let eye = me.eye();
    let d = to - eye;
    let dir = Dir3::new(d).ok()?;
    let hit = ctx.q.cast_ray_predicate(eye, dir, d.length(), true, &sight_filter(), &|e| {
        e != me.e && e != target && !ctx.loose.contains(&e)
    })?;
    Some(ctx.chars.iter().any(|c| c.e == hit.entity).then_some(hit.entity))
}

/// The bot's kit weapon in slot `k` (0 mid primary, 1 long, 2 secondary,
/// 3 melee, 4 grenade, 5 mine), as carried now.
fn slot<'g>(bot: &CaBot, guns: &'g [Gun], k: usize) -> Option<&'g Gun> {
    let id = bot.kit[k]?;
    guns.iter().find(|g| g.id == id)
}

/// A weapon's AI row (None: no row).
fn gun_ai(ctx: &Ctx, bot: &CaBot, id: &str) -> Option<GunAi> {
    ctx.data.gun(id, bot.template)
}

fn decide(ctx: &Ctx, bot: &mut CaBot, me: &Char, guns: &[Gun], active: Option<Entity>) -> Out {
    let mut out = Out::default();
    let mut io = IntentOut {
        yaw: me.yaw,
        pitch: me.pitch,
        ..default()
    };
    let t = &ctx.data.template;
    let tp = bot.template;
    // A target that died or left is dropped at once.
    if let Some(target) = bot.target
        && !ctx.chars.iter().any(|c| c.e == target && c.alive)
    {
        bot.target = None;
        if matches!(bot.state, BotState::Attack { .. } | BotState::Chase { .. }) {
            leave_attack(ctx, bot);
            enter_patrol(ctx, bot);
        }
    }

    if ctx.frame {
        sense_pass(ctx, bot, me, guns, &mut out);
        goals(ctx, bot, me, guns);
        grenade_danger(ctx, bot, me);
    }

    let target = bot.target.and_then(|e| ctx.chars.iter().find(|c| c.e == e)).copied();
    match bot.state {
        BotState::Attack { .. } => {
            if let Some(target) = target {
                attack(ctx, bot, me, &target, guns, active, &mut io, &mut out);
            }
        }
        BotState::Chase { breath, .. } => {
            if let Some(target) = target {
                if ctx.now < breath {
                    io.axis = Vec2::ZERO;
                } else {
                    let offset = chase_offset(ctx.patrol.chase_cycle);
                    let goal = target.feet() + offset * UNIT;
                    // Re-planned at the path's end, toward where it is now.
                    if bot.route.is_empty() {
                        bot.route_goal = None;
                        out.chase_step = true;
                    }
                    let diverge = !bot.visible;
                    walk(ctx, bot, me, goal, diverge, &mut io);
                }
                // Fire on the way when in reach and visible.
                if bot.visible && u(me.pos.distance(target.pos)) <= bot.ranged_reach {
                    aim_and_fire(ctx, bot, me, &target, guns, active, &mut io, &mut out);
                }
            }
        }
        BotState::Investigate { spot, look_only, .. } => {
            if look_only || me.feet().xz().distance(spot.xz()) < 0.6 {
                let (yaw, _) = look_at(me.eye(), spot);
                io.yaw = yaw;
                io.pitch = 0.0;
            } else {
                walk(ctx, bot, me, spot, false, &mut io);
            }
        }
        BotState::Dodge { to, .. } => {
            let d = to - me.feet();
            if d.xz().length() > 0.2 {
                if let Some(target) = target.filter(|_| bot.visible) {
                    aim_and_fire(ctx, bot, me, &target, guns, active, &mut io, &mut out);
                    io.axis = axis_toward(d, io.yaw);
                } else {
                    let (yaw, _) = look_at(me.feet(), to);
                    io.yaw = yaw;
                    io.pitch = 0.0;
                    io.axis = Vec2::Y;
                }
            }
        }
        BotState::Patrol => patrol(ctx, bot, me, &mut io, &mut out),
    }
    let _ = t;
    let _ = tp;
    io.crouch = ctx.now < bot.crouch_until;
    io.jump = std::mem::take(&mut bot.jump);
    if ctx.settings.dont_shoot != 0 {
        io.fire = false;
    }
    if ctx.settings.stop != 0 {
        // `ca_bot_stop`: no movement keys (senses, aim and the trigger go
        // on); a patrolling bot keeps its facing.
        io.axis = Vec2::ZERO;
        io.jump = false;
        if bot.state == BotState::Patrol {
            io.yaw = me.yaw;
            io.pitch = me.pitch;
        }
    }
    out.intent = Some(io);
    out
}

fn chase_offset(cycle: u32) -> Vec3 {
    match cycle % 4 {
        0 => Vec3::new(-CHASE_OFFSET, 0.0, 0.0),
        1 => Vec3::new(CHASE_OFFSET, 0.0, 0.0),
        2 => Vec3::new(0.0, 0.0, -CHASE_OFFSET),
        _ => Vec3::new(0.0, 0.0, CHASE_OFFSET),
    }
}

/// One sense pass (5.1).
fn sense_pass(ctx: &Ctx, bot: &mut CaBot, me: &Char, guns: &[Gun], out: &mut Out) {
    bot.passes += 1;
    let t = &ctx.data.template;
    let tp = bot.template;
    let relaxed = bot.state.relaxed();
    let alert = !relaxed;
    // The current target's visibility and the movement inaccuracy (the
    // target update, each pass).
    if let Some(target) = bot.target.and_then(|e| ctx.chars.iter().find(|c| c.e == e)) {
        let seen = sight(ctx, me, target, true);
        bot.visible = seen.is_some();
        bot.head_only = seen == Some(true);
        let gun = held_gun(ctx, bot, guns, me);
        bot.tracking
            .update(target.pos / UNIT, SERVER_FRAME as f32, gun.map_or(0.0, |g| g.c_max), gun.map_or(0.0, |g| g.decay));
    }
    // 1: the nearest enemy that passes the coarse test.
    let mut enemies: Vec<(&Char, f32)> = ctx
        .chars
        .iter()
        .filter(|c| c.alive && c.team != me.team && crate::game::rules::state::plays(c.team))
        .map(|c| (c, u(me.pos.distance(c.pos))))
        .collect();
    enemies.sort_by(|a, b| a.1.total_cmp(&b.1));
    let cooldown_min = t.sight_cooldown_min.at(tp);
    let coarse = enemies
        .iter()
        .find(|(_, d)| !(relaxed && *d > cooldown_min && ctx.now < bot.sight_cooldown) && *d < SIGHT_RANGE + 5000.0);
    if let Some(&(target, d)) = coarse
        && let Some(head_only) = sight(ctx, me, target, alert)
    {
        let seen = if relaxed {
            bot.sight_cooldown = ctx.now + SIGHT_COOLDOWN;
            let camp = ctx.camping.get(&target.e).copied().unwrap_or(0.0);
            let p = math::notice_percent(
                d,
                t.notice_near.at(tp),
                t.notice_far.at(tp),
                t.notice_far_percent.at(tp),
                math::camping_bonus(camp, t.camp_sight_step.at(tp)),
            );
            math::roll(p, bot.dice.r100())
        } else {
            true
        };
        if seen {
            sight_fired(ctx, bot, me, target, d, head_only, guns);
            return;
        }
    }
    // 2: what it hears, in alarm order.
    let my_team = me.team;
    let near = |s: &Stimulus, window: f32| {
        let dy = u((s.at.y - me.pos.y).abs());
        (dy <= window, u(s.at.distance(me.pos)))
    };
    let mut order: Vec<&Stimulus> = ctx.stimuli.list.iter().collect();
    order.sort_by_key(|s| match s.kind {
        Kind::Gunfire => 0,
        Kind::AllyCall => 1,
        Kind::Footstep => 2,
    });
    for s in order {
        if s.source == me.e {
            continue;
        }
        match s.kind {
            Kind::Gunfire => {
                if s.id <= bot.last_gunfire || s.team == my_team {
                    continue;
                }
                let (inside, d) = near(s, GUNFIRE_WINDOW);
                if !inside || d >= s.radius {
                    continue;
                }
                bot.last_gunfire = s.id;
                if relaxed && !math::roll(math::hearing_percent(d, 1000.0, 50.0), bot.dice.r100()) {
                    continue;
                }
                let passes = (bot.dice.range(0.1, 0.3) / 0.01).round() as u64;
                bot.pending = Some(Pending {
                    at_pass: bot.passes + passes,
                    spot: s.at,
                    gunfire: true,
                });
                break;
            }
            Kind::AllyCall => {
                if s.id <= bot.last_call || s.team != my_team {
                    continue;
                }
                let (_, d) = near(s, f32::INFINITY);
                if d >= s.radius {
                    continue;
                }
                bot.last_call = s.id;
                let passes = (bot.dice.range(0.3, 1.0) / 0.01).round() as u64;
                if bot.pending.is_none() {
                    bot.pending = Some(Pending {
                        at_pass: bot.passes + passes,
                        spot: s.at,
                        gunfire: false,
                    });
                }
                break;
            }
            Kind::Footstep => {
                if s.id <= bot.last_step || s.team == my_team {
                    continue;
                }
                let (inside, d) = near(s, FOOTSTEP_WINDOW);
                if !inside || d >= HEARING + s.radius {
                    continue;
                }
                bot.last_step = s.id;
                if relaxed && !math::roll(math::hearing_percent(d, 800.0, 60.0), bot.dice.r100()) {
                    continue;
                }
                bot.last_step_at = Some(s.at);
                if bot.footsteps.add(0.5) {
                    investigate(ctx, bot, me, s.at, false, out);
                }
                break;
            }
        }
    }
    // 3: the records' decay and timers.
    if bot.footsteps.decay(if alert { 0.00125 } else { 0.0025 })
        && let Some(at) = bot.last_step_at
    {
        // A third false stimulation: fully stimulated.
        investigate(ctx, bot, me, at, false, out);
    }
    if let Some(p) = bot.pending
        && bot.passes >= p.at_pass
    {
        bot.pending = None;
        investigate(ctx, bot, me, p.spot, p.gunfire, out);
    }
}

/// A heard spot fires the investigate goal (5.3): only over patrol and
/// investigate; the bot calls its allies when it starts.
fn investigate(ctx: &Ctx, bot: &mut CaBot, me: &Char, spot: Vec3, gunfire: bool, out: &mut Out) {
    if !bot.state.relaxed() {
        return;
    }
    let starting = bot.state == BotState::Patrol;
    // A spot in plain view is only looked at (not a shot, Open question 12).
    let look_only = !gunfire && {
        let eye = me.eye();
        let d = spot + Vec3::Y * 0.5 - eye;
        Dir3::new(d).ok().is_some_and(|dir| {
            ctx.q
                .cast_ray_predicate(eye, dir, d.length(), true, &sight_filter(), &|e| !ctx.see_through.contains(&e))
                .is_none()
        })
    };
    let spot_feet = ctx
        .mesh
        .and_then(|m| m.nearest_area(spot).map(|a| m.areas[a].closest_point(spot)))
        .unwrap_or(spot);
    bot.state = BotState::Investigate {
        spot: spot_feet,
        until: ctx.now + INVESTIGATE_TIME,
        look_only,
    };
    bot.route.clear();
    bot.route_goal = None;
    if starting {
        out.ally_call = Some(me.pos);
        reset_grenades(ctx, bot);
    }
}

fn reset_grenades(ctx: &Ctx, bot: &mut CaBot) {
    bot.grenades = ctx.data.template.grenade_shots.at(bot.template) as u32;
}

fn enter_patrol(ctx: &Ctx, bot: &mut CaBot) {
    bot.state = BotState::Patrol;
    bot.target = None;
    bot.route.clear();
    bot.route_goal = None;
    bot.patrol_to = None;
    reset_grenades(ctx, bot);
}

fn leave_attack(ctx: &Ctx, bot: &mut CaBot) {
    if matches!(bot.state, BotState::Attack { .. }) {
        bot.attack_off = ctx.now;
    }
    bot.move_to = None;
    bot.chase_timer = None;
}

/// Whether a ranged weapon of the kit can be used (has ammunition).
fn ranged_usable(bot: &CaBot, guns: &[Gun]) -> bool {
    (0..3).any(|k| slot(bot, guns, k).is_some_and(Gun::has_ammo))
}

/// Sight fired on `target` at `d` u (6: the last sight that fired is the
/// target).
fn sight_fired(ctx: &Ctx, bot: &mut CaBot, me: &Char, target: &Char, d: f32, head_only: bool, guns: &[Gun]) {
    if bot.target != Some(target.e) {
        bot.target = Some(target.e);
        let c_max = held_gun(ctx, bot, guns, me).map_or(0.0, |g| g.c_max);
        bot.tracking.select(c_max);
        bot.tracking.prev = Some(target.pos / UNIT);
        reset_grenades(ctx, bot);
        bot.aim_offset = None;
    }
    bot.visible = true;
    bot.head_only = head_only;
    let melee = !ranged_usable(bot, guns) && slot(bot, guns, 3).is_some();
    let in_reach = d <= bot.ranged_reach || (melee && d <= KNIFE_RANGE);
    match bot.state {
        BotState::Attack { .. } => bot.chase_timer = None,
        BotState::Dodge { .. } => {}
        _ if in_reach => enter_attack(ctx, bot, target),
        BotState::Chase { .. } => {}
        _ => enter_chase(ctx, bot),
    }
}

/// The attack goal activates (7.2's reaction delay, 9.3's crouch).
fn enter_attack(ctx: &Ctx, bot: &mut CaBot, target: &Char) {
    let t = &ctx.data.template;
    let tp = bot.template;
    if ctx.now - bot.attack_off > REARM_GAP {
        let camp = ctx.camping.get(&target.e).copied().unwrap_or(0.0);
        let delay = math::reaction_delay(t.reaction_min.at(tp), t.reaction_max.at(tp), camp, bot.dice.unit());
        bot.fire_at = ctx.now + delay as f64;
    }
    bot.state = BotState::Attack { since: ctx.now };
    bot.chase_timer = None;
    bot.route.clear();
    bot.route_goal = None;
    bot.next_move = ctx.now + bot.dice.range(t.attack_move_min.at(tp), t.attack_move_max.at(tp)) as f64;
    bot.move_to = None;
    bot.burst_left = 0;
    bot.pause_until = ctx.now;
    bot.aim_offset = None;
    let taken = ctx.chars.iter().any(|c| c.bot && c.crouching_attack == Some(target.e));
    if !taken && bot.dice.unit() < CROUCH_CHANCE {
        bot.crouch_until = ctx.now + CROUCH_HOLD;
    }
}

fn enter_chase(ctx: &Ctx, bot: &mut CaBot) {
    leave_attack(ctx, bot);
    let endurance = ctx.data.template.chase_endurance.at(bot.template) as f64;
    bot.state = BotState::Chase {
        until: ctx.now + endurance,
        breath: 0.0,
    };
    bot.route.clear();
    bot.route_goal = None;
}

/// Goal and state bookkeeping each frame (5.6, 2's decays).
fn goals(ctx: &Ctx, bot: &mut CaBot, me: &Char, guns: &[Gun]) {
    let now = ctx.now;
    let target = bot.target.and_then(|e| ctx.chars.iter().find(|c| c.e == e)).copied();
    match bot.state {
        BotState::Attack { since } => {
            let Some(target) = target else {
                leave_attack(ctx, bot);
                enter_patrol(ctx, bot);
                return;
            };
            if bot.visible {
                bot.chase_timer = None;
            } else {
                let end = *bot
                    .chase_timer
                    .get_or_insert_with(|| now + math::attack_chase_timer(bot.dice.unit()) as f64);
                if now >= end {
                    enter_chase(ctx, bot);
                    return;
                }
            }
            let d = u(me.pos.distance(target.pos));
            let melee = !ranged_usable(bot, guns);
            let reach = if melee { KNIFE_RANGE.max(bot.ranged_reach) } else { bot.ranged_reach };
            if d > reach && now - since >= 1.0 {
                enter_chase(ctx, bot);
            }
        }
        BotState::Chase { until, .. } => {
            if target.is_none() {
                enter_patrol(ctx, bot);
                return;
            }
            if now >= until {
                if bot.visible {
                    let endurance = ctx.data.template.chase_endurance.at(bot.template) as f64;
                    bot.state = BotState::Chase {
                        until: now + 1.0 + endurance,
                        breath: now + 1.0,
                    };
                } else {
                    enter_patrol(ctx, bot);
                }
            }
        }
        BotState::Investigate { until, .. } => {
            if now >= until {
                enter_patrol(ctx, bot);
            }
        }
        BotState::Dodge { until, to } => {
            if now >= until || me.feet().xz().distance(to.xz()) < 0.25 {
                bot.danger = None;
                match target {
                    Some(t) if bot.visible => enter_attack(ctx, bot, &t),
                    _ => enter_patrol(ctx, bot),
                }
            }
        }
        BotState::Patrol => {}
    }
}

/// Grenade danger (5.4).
fn grenade_danger(ctx: &Ctx, bot: &mut CaBot, me: &Char) {
    let t = &ctx.data.template;
    let tp = bot.template;
    if let Some((g, _, _)) = bot.danger
        && !ctx.dangers.iter().any(|d| d.e == g)
    {
        bot.danger = None;
    }
    for g in ctx.dangers {
        if !g.player_thrown || !g.touched || g.team == me.team {
            continue;
        }
        match bot.danger {
            Some((other, _, _)) if other != g.e => continue,
            Some((_, last, _)) if u(last.distance(g.pos)) < DANGER_MOVED => continue,
            _ => {}
        }
        let d = u(g.pos.distance(me.pos));
        if d > g.radius + DANGER_MARGIN {
            continue;
        }
        let to = me.pos - g.pos;
        if let Ok(dir) = Dir3::new(to) {
            let blocked = ctx
                .q
                .cast_ray_predicate(g.pos, dir, to.length(), true, &sight_filter(), &|e| {
                    !ctx.see_through.contains(&e)
                })
                .is_some();
            if blocked {
                continue;
            }
        }
        let delay = bot.dice.range(t.dodge_delay_min.at(tp), t.dodge_delay_max.at(tp)) as f64;
        bot.danger = Some((g.e, g.pos, ctx.now + delay));
    }
    let Some((g, at, when)) = bot.danger else { return };
    if ctx.now < when || matches!(bot.state, BotState::Dodge { .. }) {
        return;
    }
    let Some(danger) = ctx.dangers.iter().find(|d| d.e == g) else { return };
    // Already outside the radius: it stays.
    if u(me.pos.distance(at)) > danger.radius {
        return;
    }
    let away = Vec3::new(me.pos.x - at.x, 0.0, me.pos.z - at.z).normalize_or(Vec3::X);
    let r = danger.radius * DODGE_FACTOR * UNIT;
    for k in math::DODGE_STEPS {
        let dir = Quat::from_rotation_y(k as f32 * std::f32::consts::FRAC_PI_4) * away;
        let mut to = at + dir * r;
        to.y = me.feet().y;
        if move_possible(ctx, me, to) {
            leave_attack(ctx, bot);
            bot.state = BotState::Dodge { to, until: ctx.now + 3.0 };
            return;
        }
    }
}

/// A straight walk through the areas from `me` to `to` (feet level) with
/// no other bot on the line.
fn move_possible(ctx: &Ctx, me: &Char, to: Vec3) -> bool {
    let from = me.feet();
    if let Some(mesh) = ctx.mesh
        && !nav::straight(mesh, from, to, 0.25)
    {
        return false;
    }
    let seg = to - from;
    let len2 = seg.length_squared().max(1e-6);
    !ctx.chars.iter().any(|c| {
        if c.e == me.e || !c.bot || !c.alive {
            return false;
        }
        let p = c.feet();
        let k = ((p - from).dot(seg) / len2).clamp(0.0, 1.0);
        u((from + seg * k).distance(p)) < 2.0 * RADIUS
    })
}

/// The held weapon's AI row, else the chosen one's.
fn held_gun(ctx: &Ctx, bot: &CaBot, guns: &[Gun], _me: &Char) -> Option<GunAi> {
    bot.held
        .and_then(|h| guns.iter().find(|g| g.e == h))
        .or_else(|| slot(bot, guns, 0))
        .and_then(|g| gun_ai(ctx, bot, g.id))
}

/// The attack state (7-9).
#[allow(clippy::too_many_arguments)]
fn attack(ctx: &Ctx, bot: &mut CaBot, me: &Char, target: &Char, guns: &[Gun], active: Option<Entity>, io: &mut IntentOut, out: &mut Out) {
    let t = &ctx.data.template;
    let tp = bot.template;
    let d = u(me.pos.distance(target.pos));
    aim_and_fire(ctx, bot, me, target, guns, active, io, out);
    let gun = held_gun(ctx, bot, guns, me);
    let melee = bot.held.is_some_and(|h| guns.iter().any(|g| g.e == h && Some(g.id) == bot.kit[3]));

    if ctx.frame {
        // Inside reach but beyond the held weapon: move in at once.
        if let Some(g) = gun
            && d > g.range.1
            && !melee
        {
            bot.next_move = bot.next_move.min(ctx.now);
        }
        // 9.1: attack moves.
        if ctx.now >= bot.next_move && !melee {
            let fwd = Vec3::new(target.pos.x - me.pos.x, 0.0, target.pos.z - me.pos.z).normalize_or(me.forward());
            let pick = bot.dice.int(0, 6) as usize;
            let shuffle = t.shuffle.at(tp);
            let flank_back = bot.dice.unit() < 0.5;
            let (rmin, rmax) = gun.map_or((0.0, f32::MAX), |g| g.range);
            let flank_forward = if rmin > 0.0 { (d - rmin).clamp(0.0, 50.0) } else { 50.0 };
            let here = me.feet() / UNIT;
            let tgt = target.feet() / UNIT;
            bot.move_to = None;
            for k in 0..7 {
                let m = MOVES[(pick + k) % 7];
                let mut dest = math::move_target(m, here, fwd, shuffle, RADIUS, flank_back, flank_forward);
                if d > rmax {
                    let dir = (dest - tgt).normalize_or_zero();
                    dest = tgt + dir * 0.8 * rmax;
                } else if matches!(m, Move::FlankLeft | Move::FlankRight) && rmin > 0.0 && dest.distance(tgt) < rmin {
                    dest = tgt + (dest - tgt).normalize_or_zero() * rmin;
                }
                let to = Vec3::new(dest.x * UNIT, me.feet().y, dest.z * UNIT);
                if move_possible(ctx, me, to) {
                    bot.move_to = Some(to);
                    bot.move_until = ctx.now + 2.0;
                    break;
                }
            }
            bot.next_move = ctx.now + bot.dice.range(t.attack_move_min.at(tp), t.attack_move_max.at(tp)) as f64;
        }
        // 9.2: dodging fire.
        let aiming = {
            let to_me = (me.pos - target.pos).normalize_or_zero();
            target.look().dot(to_me) > AIMING_DOT && me.look().dot(-to_me) > AIMING_DOT
        };
        let check = std::mem::take(&mut bot.dodge_hit) || (aiming && bot.passes % 5 == 0);
        if check && ctx.now >= bot.dodge_next && !melee {
            dodge(ctx, bot, me, target);
        }
    }
    // Moving: the attack move, or in to the knife's reach.
    if melee && d > KNIFE_RANGE {
        io.axis = axis_toward(target.pos - me.pos, io.yaw);
    } else if let Some(to) = bot.move_to {
        let d = to - me.feet();
        if d.xz().length() < 0.1 || ctx.now > bot.move_until {
            bot.move_to = None;
        } else {
            io.axis = axis_toward(d, io.yaw);
        }
    }
}

/// 9.2: 70 % shuffle, 30 % roll, a random side, then the other side,
/// then the other action.
fn dodge(ctx: &Ctx, bot: &mut CaBot, me: &Char, target: &Char) {
    let shuffle = ctx.data.template.shuffle.at(bot.template) + RADIUS;
    let fwd = Vec3::new(target.pos.x - me.pos.x, 0.0, target.pos.z - me.pos.z).normalize_or(me.forward());
    let right = fwd.cross(Vec3::Y).normalize_or_zero();
    let first_roll = bot.dice.unit() < 0.3;
    let side = if bot.dice.unit() < 0.5 { 1.0 } else { -1.0 };
    for (roll, s) in [(first_roll, side), (first_roll, -side), (!first_roll, side), (!first_roll, -side)] {
        let dist = if roll { ROLL } else { shuffle };
        let to = me.feet() + right * s * dist * UNIT;
        if move_possible(ctx, me, to) {
            bot.move_to = Some(to);
            bot.move_until = ctx.now + 1.0;
            bot.dodge_next = ctx.now + DODGE_DELAY;
            return;
        }
    }
}

/// Weapon choice, aim and trigger (7, 8).
#[allow(clippy::too_many_arguments)]
fn aim_and_fire(ctx: &Ctx, bot: &mut CaBot, me: &Char, target: &Char, guns: &[Gun], active: Option<Entity>, io: &mut IntentOut, out: &mut Out) {
    let now = ctx.now;
    let d = u(me.pos.distance(target.pos));
    // 8.2: which weapon.
    let choice = |k: usize| {
        slot(bot, guns, k).map(|g| Choice {
            has_ammo: g.has_ammo(),
            range_max: gun_ai(ctx, bot, g.id).map_or(0.0, |a| a.range.1),
        })
    };
    let pick = math::choose_weapon(choice(0), choice(1), choice(2), d);
    let want = slot(bot, guns, pick).or_else(|| slot(bot, guns, 3));
    let current = bot.held.and_then(|h| guns.iter().find(|g| g.e == h));
    let rechoose = match current {
        None => true,
        Some(c) => {
            let ai = gun_ai(ctx, bot, c.id);
            !c.has_ammo() || ai.is_some_and(|a| d < a.range.0 || d > a.range.1) || c.count.is_some()
        }
    };
    if rechoose && let Some(w) = want {
        if bot.held != Some(w.e) {
            bot.burst_left = 0;
            bot.pause_until = now;
            bot.last_clip = None;
        }
        bot.held = Some(w.e);
    }
    let Some(held) = bot.held.and_then(|h| guns.iter().find(|g| g.e == h)).copied() else { return };
    out.wanted = Some(held.e);
    let gun = gun_ai(ctx, bot, held.id).unwrap_or_default();

    // 7.3: the aim point for this shot.
    let base = if bot.head_only { target.head() } else { target.torso() };
    if bot.aim_offset.is_none() {
        let camp = ctx.camping.get(&target.e).copied().unwrap_or(0.0);
        let base_u = base / UNIT;
        let offset = if math::full_accuracy(d, gun.full_accuracy) {
            Vec3::ZERO
        } else {
            let a = math::accuracy(gun.a, camp, bot.head_only);
            let r = bot.dice.r100();
            if bot.cycle.shot(a, r) {
                let draws = Vec3::new(bot.dice.unit(), bot.dice.unit(), bot.dice.unit());
                math::hit_point(base_u, bot.tracking.c, draws) - base_u
            } else {
                let right = bot.dice.unit() < 0.5;
                let up = bot.dice.range(-5.0, 15.0);
                math::miss_point(me.pos / UNIT, base_u, gun.miss_offset, right, up) - base_u
            }
        };
        bot.aim_offset = Some(offset);
    }
    let aim = base + bot.aim_offset.unwrap_or_default() * UNIT;
    let (yaw, pitch) = look_at(me.eye(), aim);
    io.yaw = yaw;
    io.pitch = pitch;

    // 8.5: a grenade after its count of shots.
    if ctx.frame
        && bot.grenades == 0
        && bot.visible
        && !bot.head_only
        && d <= bot.thrown_reach
        && let Some(g) = slot(bot, guns, 4).filter(|g| g.count.unwrap_or(0) > 0)
    {
        let from = me.eye();
        let v = math::throw_velocity(from / UNIT, target.pos / UNIT, HANG_TIME, GRAVITY, 1.0, 0.0);
        out.throw = Some((g.e, from, v * UNIT));
        reset_grenades(ctx, bot);
        return;
    }

    // 8.3: bursts by the clip, reloads.
    let drawn = active == Some(held.e);
    if let Some(prev) = bot.last_clip
        && held.clip < prev
    {
        let spent = prev - held.clip;
        bot.burst_left = bot.burst_left.saturating_sub(spent);
        if bot.burst_left == 0 {
            bot.pause_until = now + bot.dice.range(gun.pause.0, gun.pause.1) as f64;
        }
    }
    bot.last_clip = Some(held.clip);
    if held.count.is_none() && held.clip == 0 && held.reserve > 0 && gun.burst.1 > 0 {
        io.reload = true;
        bot.burst_left = 0;
        bot.pause_until = now;
        bot.next_move = bot.next_move.min(now);
    }
    if bot.burst_left == 0 && now >= bot.pause_until {
        bot.burst_left = bot.dice.int(gun.burst.0.max(1), gun.burst.1.max(1));
    }
    let knife = Some(held.id) == bot.kit[3];
    let in_reach = !knife || d <= KNIFE_RANGE;
    let line = fire_line(ctx, me, target.e, base);
    if let Some(Some(mate)) = line
        && ctx.chars.iter().any(|c| c.e == mate && c.team == me.team)
    {
        // 9.3: a teammate in the line of fire.
        if u(me.pos.distance(ctx.chars.iter().find(|c| c.e == mate).unwrap().pos)) > CROUCH_ASK {
            out.ask_crouch = Some(mate);
        } else if ctx.frame && now >= bot.dodge_next {
            dodge(ctx, bot, me, target);
        }
    }
    let fire = drawn
        && in_reach
        && now >= bot.fire_at
        && now >= bot.next_shot
        && bot.visible
        && line.is_none()
        && (knife || held.clip > 0)
        && (knife || bot.burst_left > 0)
        && now >= bot.pause_until;
    io.fire = fire && !std::mem::take(&mut bot.release);
}

/// Walk toward `goal` (feet, m) along a planned path (3).
fn walk(ctx: &Ctx, bot: &mut CaBot, me: &Char, goal: Vec3, diverge: bool, io: &mut IntentOut) {
    let feet = me.feet();
    let plan = bot.route.is_empty() || bot.route_goal.is_none_or(|g| g.distance(goal) > 1.0 && bot.route.len() <= 1);
    if plan {
        bot.route_goal = Some(goal);
        bot.route = match ctx.mesh {
            Some(mesh) => {
                let (Some(a), Some(b)) = (mesh.nearest_area(feet + Vec3::Y * 0.3), mesh.nearest_area(goal + Vec3::Y * 0.3)) else {
                    return;
                };
                let reserved: HashSet<usize> = if diverge {
                    ctx.chars
                        .iter()
                        .filter(|c| c.bot && c.e != me.e && c.alive)
                        .filter_map(|c| mesh.area_at(c.feet() + Vec3::Y * 0.3))
                        .collect()
                } else {
                    HashSet::new()
                };
                match nav::find(mesh, a, feet, b, UNIT, 0.0, |i| if reserved.contains(&i) { 1.0 + nav::RESERVED } else { 1.0 }) {
                    Some(p) => nav::points(mesh, &p, feet, goal),
                    None => Vec::new(),
                }
            }
            None => vec![goal],
        };
        bot.stuck_check = (ctx.now, feet);
    }
    while let Some(&p) = bot.route.first() {
        if p.xz().distance(feet.xz()) < 0.35 && (p.y - feet.y).abs() < 1.2 {
            bot.route.remove(0);
        } else {
            break;
        }
    }
    let Some(&next) = bot.route.first() else { return };
    let (yaw, _) = look_at(feet, next);
    io.yaw = yaw;
    io.pitch = 0.0;
    io.axis = Vec2::Y;
    // A ledge above a stair step just ahead: jump up it.
    if next.y - feet.y > JUMP_RISE && next.xz().distance(feet.xz()) < JUMP_AHEAD {
        bot.jump = true;
    }
    // Ours: no progress for 1.5 s → plan again and jump.
    if ctx.now - bot.stuck_check.0 > 1.5 {
        if bot.stuck_check.1.distance(feet) < 0.3 {
            bot.route.clear();
            bot.jump = true;
            bot.patrol_to = None;
        }
        bot.stuck_check = (ctx.now, feet);
    }
}

/// The patrol (4): node to node of the patrol graph by the link choice,
/// along planned paths.
fn patrol(ctx: &Ctx, bot: &mut CaBot, me: &Char, io: &mut IntentOut, out: &mut Out) {
    let (Some(_), Some(graph)) = (ctx.mesh, ctx.graph) else { return };
    if graph.nodes.is_empty() {
        return;
    }
    let feet = me.feet();
    if bot.patrol_to.is_none() {
        let here = bot.patrol_from.filter(|&n| n < graph.nodes.len()).or_else(|| graph.nearest(feet));
        let Some(here) = here else { return };
        let from = bot.patrol_came;
        let mut links: Vec<usize> = graph.links[here].iter().copied().filter(|&n| Some(n) != from).collect();
        if links.is_empty() {
            links = graph.links[here].clone();
        }
        if links.is_empty() {
            // A node with no links: head for the nearest other one.
            links = (0..graph.nodes.len()).filter(|&n| n != here).collect();
        }
        let at = graph.points[here];
        let options: Vec<(f64, Vec3)> = links
            .iter()
            .map(|&n| {
                (
                    ctx.patrol.reached.get(&n).copied().unwrap_or(0.0),
                    graph.points[n] - at,
                )
            })
            .collect();
        bot.patrol_to = math::patrol_choice(&options, me.forward()).map(|i| links[i]);
        bot.patrol_came = Some(here);
        bot.route.clear();
        bot.route_goal = None;
        bot.stuck_check = (ctx.now, feet);
    }
    let Some(to) = bot.patrol_to.filter(|&n| n < graph.nodes.len()) else {
        bot.patrol_to = None;
        return;
    };
    let goal = graph.points[to];
    if goal.xz().distance(feet.xz()) < 0.8 && (goal.y - feet.y).abs() < 1.5 {
        out.reached = Some(to);
        bot.patrol_from = Some(to);
        bot.patrol_to = None;
        return;
    }
    walk(ctx, bot, me, goal, false, io);
    if bot.route.is_empty() {
        // No path there: another node next time.
        bot.patrol_from = Some(to);
        bot.patrol_to = None;
    }
}
