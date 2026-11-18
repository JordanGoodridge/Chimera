use std::sync::Arc;

use avian3d::prelude::*;
use bevy::prelude::*;

use super::*;
use crate::{
    character::CAPSULE_HEIGHT,
    character::spawn_character,
    console::{Command, Console, resource_cvar},
    core::{Health, Intent, MapBrush, MapBrushes, MovementState, SimSet, SpawnPoint, Team},
    map::nav::{NavLadder, NavMesh, STEP_HEIGHT, flags},
    weapon::{Inventory, grenade::Projectile},
};

/// `bot_quota` and `bot_quota_mode` (CS:S's): how many bots a network
/// server keeps. `bot_add` (`add_bot`) raises the quota by one and `bot_kick` sets it
/// to 0 (or lowers it by the bots kicked by name), so they stay in step;
/// setting `bot_quota` adds or kicks bots on a server (`keep_quota`).
/// Single player keeps whatever bots it was given (never enforced there).
/// With them, how bots join: `bot_join_after_player`, `bot_difficulty`,
/// `bot_prefix` (CS:S's).
#[derive(Resource, Clone, Debug)]

pub struct BotQuota {
    pub quota: u32,
    /// "normal", "fill" or "match".
    pub mode: String,
    /// 1: the quota adds no bots until a human is in the game (CS:S's
    /// default); `bot_add` still adds one at once.
    pub join_after_player: u8,
    /// The difficulty new bots play at, 0 easy to 3 expert (their name
    /// comes from a profile of that difficulty: `map::bot_profiles`).
    pub difficulty: u8,
    /// Put before every new bot's name ("<prefix> <name>"); empty: none.
    pub prefix: String,
}

impl Default for BotQuota {
    fn default() -> Self {
        Self {
            quota: 0,
            mode: "normal".into(),
            join_after_player: 1,
            difficulty: 1,
            prefix: String::new(),
        }
    }
}

impl BotQuota {
    /// Bots wanted with `humans` players in the game.
    pub fn wanted(&self, humans: usize) -> usize {
        let q = self.quota as usize;
        match self.mode.to_ascii_lowercase().as_str() {
            "fill" => q.saturating_sub(humans),
            "match" => q * humans,
            _ => q,
        }
    }
}

/// Bring the quota in step with the bots there are (a server starting
/// after bots were added in single player keeps them): normal mode only.
pub fn sync_quota(world: &mut World) {
    let bots = world.query_filtered::<(), With<Bot>>().iter(world).count() as u32;
    let Some(mut q) = world.get_resource_mut::<BotQuota>() else {
        return;
    };
    if q.mode.eq_ignore_ascii_case("normal") && q.quota != bots {
        q.quota = bots;
    }
}

/// Remove a bot as a player leaving would go: the bomb it carries dropped
/// where it stands, its other weapons with it.
pub fn kick_bot(world: &mut World, bot: Entity) {
    let weapons = world
        .get::<Inventory>(bot)
        .map(|i| i.weapons.clone())
        .unwrap_or_default();
    for w in weapons {
        if world.get::<crate::objectives::bomb::C4>(w).is_some() {
            crate::weapon::drop::drop_this(world, bot, w, false);
        } else if let Ok(e) = world.get_entity_mut(w) {
            e.despawn();
        }
    }
    if let Ok(e) = world.get_entity_mut(bot) {
        e.despawn();
    }
}

/// A server keeps `bot_quota` bots (one added or kicked per frame, as
/// CS:S does): added to the team with fewer players, kicked from the one
/// with more (the newest bot there). Humans are every other character
/// but hostages (the host and remote players).
pub(super) fn keep_quota(world: &mut World) {
    let mut bots: Vec<(Entity, Team, u32)> = Vec::new();
    let mut counts = [0usize; 2];
    let mut humans = 0;
    let mut playing = 0;
    for (e, team, bot, (connecting, unassigned)) in world
        .query_filtered::<(
            Entity,
            &Team,
            Option<&Bot>,
            (Has<crate::core::Connecting>, Has<crate::core::Unassigned>),
        ), (With<Intent>, With<Health>, Without<crate::objectives::hostages::Hostage>)>()
        .iter(world)
    {
        if (1..=2).contains(&team.0) {
            counts[team.0 as usize - 1] += 1;
        }
        match bot {
            Some(b) => bots.push((e, *team, b.number)),
            None => {
                humans += 1;
                // In the game: loaded, and on a team or the spectators
                // (not still at the join menus).
                if !connecting && !unassigned {
                    playing += 1;
                }
            }
        }
    }
    let quota = world.resource::<BotQuota>().clone();
    let wanted = quota.wanted(humans);
    // `bot_join_after_player`: none join until someone plays (those there
    // stay).
    let waiting = quota.join_after_player != 0 && playing == 0;
    if bots.len() < wanted && !waiting {
        // Counter-terrorists when even? CS:S picks at random; the
        // terrorists here, as `bot_add` does.
        let team = if counts[1] < counts[0] { Team(2) } else { Team(1) };
        // None without spawn points (a map loading): again next frame.
        let _ = spawn_bot(world, team);
    } else if bots.len() > wanted {
        let team = if counts[0] > counts[1] { Team(1) } else { Team(2) };
        let pick = bots
            .iter()
            .filter(|b| b.1 == team)
            .max_by_key(|b| b.2)
            .or_else(|| bots.iter().max_by_key(|b| b.2))
            .map(|b| b.0);
        if let Some(b) = pick {
            kick_bot(world, b);
        }
    }
}

/// Bot skill presets: reaction seconds, aim error degrees, turn rate
/// degrees per second (`bot_reaction`, `bot_aim_error`, `bot_turn_rate`).
/// Normal is the bots' default (`BotConfig`). The debug UI's skill
/// buttons and a game's new-game dialog (CS:S's Create Server, its four
/// difficulty buttons) pick one.
pub const DIFFICULTIES: [(&str, f32, f32, f32); 4] = [
    ("Easy", 0.6, 5.0, 220.0),
    ("Normal", 0.35, 2.5, 360.0),
    ("Hard", 0.2, 1.5, 540.0),
    ("Expert", 0.12, 0.75, 720.0),
];

#[derive(Resource, Clone, Debug)]
pub struct BotConfig {
    pub stop: u8,
    pub dont_shoot: u8,
    pub reaction: f32,
    pub turn_rate: f32,
    /// Aim wobble, degrees (re-rolled every `AIM_REROLL` seconds).
    pub aim_error: f32,
    /// Grenades: 0 never, 1 now and then, 2 whenever a throw is possible.
    pub grenades: u8,
    /// 1: bots use the team radio (`radio`).
    pub radio: u8,
    /// How much of the recoil a bot's aim takes back while firing: 1
    /// aims so the bullets (view + the weapon's `punch_scale` x punch)
    /// stay on the target, as CS:S bots' sprays do; 0 not at all.
    /// UNMEASURED: how far CS:S bots pull down (docs/tech-debt.md).
    pub recoil_control: f32,
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            stop: 0,
            dont_shoot: 0,
            reaction: 0.35,
            turn_rate: 360.0,
            aim_error: 2.5,
            grenades: 1,
            radio: 1,
            recoil_control: 1.0,
        }
    }
}

/// A bot's brain state; a bot is the engine's `core::Bot` too (the
/// network counts and kicks bots by it).
#[derive(Component, Clone, Debug, Default)]
#[require(crate::core::Bot)]
pub struct Bot {
    pub target: Option<Entity>,
    /// Seconds the current target has been in sight.
    pub seen: f32,
    pub(super) strafe: f32,
    pub(super) strafe_left: f32,
    pub(super) rng: u64,
    /// Points to walk through (engine space), the area each leads into,
    /// and the next one's index.
    pub(super) route: Vec<Vec3>,
    pub(super) route_areas: Vec<usize>,
    pub(super) next: usize,
    /// Seconds until the route is recomputed.
    pub(super) repath: f32,
    /// Where the bot was at the last progress check, and when that was.
    pub(super) progress: (Vec3, f32),
    /// Seconds it has been trying to walk without moving.
    pub(super) blocked: f32,
    /// Jump was pressed last tick (it must be let go between jumps).
    pub(super) jump_held: bool,
    /// The route point it walks to, and since when.
    pub(super) toward: (Vec3, f64),
    /// Progress checks in a row that found it stuck, and the links it
    /// got stuck on lately (from area, to area), costly until then.
    pub(super) stuck_count: u32,
    pub(super) stuck_links: Vec<((usize, usize), f64)>,
    /// Seconds spent at a ladder's foot pressing in without getting on it
    /// (a ladder flush with clip brushes attaches only off its middle).
    pub(super) ladder_pressing: f32,
    /// On a ladder: its feet's height when they last moved up or down,
    /// and seconds since (a ceiling over a ladder's top stops it short).
    pub(super) climbed: Option<(f32, f32)>,
    /// On a ladder: how far off its middle line it climbs (along the
    /// rungs, m): to the side it got on from on a ladder boxed in by
    /// flush faces (`LadderFit`), or after getting stuck under something
    /// on the middle.
    pub(super) ladder_shift: f32,
    /// The last mesh ladder looked at closely (index), and how it fits.
    pub(super) ladder_fit: Option<(usize, Option<LadderFit>)>,
    /// When it last jumped.
    pub(super) jumped: f64,
    /// Current aim offset (yaw, pitch radians) and seconds until re-rolled.
    pub(super) wobble: (Vec2, f32),
    /// Where an enemy was last seen or heard (feet), and when.
    pub lead: Option<(Vec3, f64)>,
    /// Who that was (forgotten once they're dead).
    pub(super) lead_who: Option<Entity>,
    /// Where the bot is roaming to (feet), with nothing better to do.
    pub(super) roam: Option<Vec3>,
    /// The roaming goal is one of the map's objectives.
    pub(super) roam_objective: bool,
    /// A grenade throw in progress, and the latest plan (shown until).
    pub(super) toss: Option<grenades::Toss>,
    pub(super) plan: Option<(GrenadePlan, f64)>,
    /// No throw before this; next look for a chance to throw.
    pub(super) next_toss: f64,
    pub(super) next_toss_check: f64,
    /// Looking away from a flash; flashes already noticed.
    pub(super) avert: Option<grenades::Avert>,
    /// Flashes it held back this life since they would have blinded a
    /// teammate (`grenades::flashes_mate`).
    pub(super) flashes_held: u32,
    pub(super) watched: Vec<Entity>,
    /// What it said on the radio lately.
    pub(super) radio: radio::BotRadio,
    /// Its number ("Bot 3" without a profile): the lowest free one when
    /// added.
    pub(super) number: u32,
    /// The bot profile it took its name from (`map::bot_profiles`), and
    /// the difficulty it plays at (`difficulty::*` bits; `bot_kick hard`).
    pub profile: Option<String>,
    pub difficulty: u8,
    /// Held still and not firing (`bot_freeze <name|number>`, the debug UI's
    /// per-bot row); `bot_unfreeze` lets it go.
    pub frozen: bool,
    /// Route noise seed, fixed per bot: from its number and team, never
    /// its entity id (`core::Seed`). The dice (`rng`) restart from it and
    /// the round number each round.
    pub(super) seed: u64,
    /// The round's orders from `tactics`, and what it does with them in
    /// this life: the site it heads for or holds, its hold spot, when it
    /// got there, how long it holds without contact before moving on,
    /// and whether it gave up holding to hunt.
    pub(super) orders: tactics::Orders,
    pub(super) site: Option<usize>,
    pub(super) hold: Option<Hold>,
    pub(super) hold_since: Option<f64>,
    pub(super) patience: f64,
    pub(super) hunting: bool,
    /// A teammate's call it answers: where they were, and when.
    pub(super) assist: Option<(Vec3, f64)>,
    /// A teammate's radio command it carries out (`radio::obey`), and
    /// until when it presses on without waiting for the group ("Go go
    /// go").
    pub(super) order: Option<radio::Ordered>,
    pub(super) urgent_until: f64,
    /// When it last saw or heard an enemy.
    pub(super) contact: f64,
    /// Nav area it stands in, the one before, and where it came in.
    pub(super) area: Option<usize>,
    pub(super) prev_area: Option<usize>,
    pub(super) entered: Vec3,
    pub(super) corners: look::Corners,
    /// Which watched approach it looks at, until when.
    pub(super) watch: (usize, f64),
    /// Where it is going and where it looks, and what it is doing (for
    /// debug views).
    pub(super) goal: Option<Vec3>,
    pub(super) look_at: Option<Vec3>,
    pub(super) activity: Activity,
    pub(super) dead: bool,
    /// The nearest living enemy in sight this tick (who, aim point, distance):
    /// `think`'s sight pass writes it, its loop takes it.
    pub(super) sighted: Option<(Entity, Vec3, f32)>,
    /// Where its objective is (feet; a bomb target to plant at, the
    /// planted bomb to defuse): walked to before anything else
    /// (`objectives`).
    pub objective: Option<Vec3>,
    /// A spot `objectives` keeps it at, with what it does there (falling
    /// back for a retake, covering a defuse, guarding a planted bomb), and
    /// whether it is there.
    pub(super) guard: Option<(Hold, Activity)>,
    pub(super) guard_at: bool,
    /// Spots given up on the way (a teammate in the way): `objectives`
    /// gives the next one.
    pub(super) guard_skip: usize,
    /// Carries the bomb (`objectives`): the attackers' group follows it.
    pub carrying: bool,
    /// Sent somewhere from outside (`bot_goto`, tests): walked to before
    /// anything else but a fight.
    pub move_to: Option<Vec3>,
}

/// What a bot is doing, for debug views.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Activity {
    #[default]
    Idle,
    Dead,
    Fighting,
    Throwing,
    Averting,
    /// Going where it last saw or heard an enemy.
    Chasing,
    /// Going to a teammate who called.
    Assisting,
    ToSite,
    Following,
    /// Waiting for the group (leader) or for the leader (ahead of it).
    Waiting,
    ToHold,
    Holding,
    Hunting,
    Roaming,
    /// Going where it was sent (`Bot::move_to`).
    Sent,
    /// Falling back to gather for a retake (`objectives::Retake`).
    Retaking,
    /// Covering a teammate's defuse, or guarding a planted bomb.
    Covering,
    /// Carrying out a teammate's radio command (`Bot::order`).
    Obeying,
}

impl Bot {
    /// The route being walked (engine space) and the next point's index.
    pub fn route(&self) -> (&[Vec3], usize) {
        (&self.route, self.next)
    }

    /// Where the bot is going (feet), whatever the reason.
    pub fn goal(&self) -> Option<Vec3> {
        self.goal
    }

    /// The point it looks at while no enemy is in sight.
    pub fn look_target(&self) -> Option<Vec3> {
        self.look_at
    }

    /// The hiding spot it is checking now.
    pub fn checking(&self) -> Option<Vec3> {
        self.corners.current()
    }

    pub fn role(&self) -> Role {
        self.orders.role
    }

    /// The site it heads for or holds (index into `Tactics::sites`).
    pub fn site(&self) -> Option<usize> {
        self.site
    }

    pub fn hold(&self) -> Option<&Hold> {
        self.hold.as_ref()
    }

    pub fn activity(&self) -> Activity {
        self.activity
    }

    /// Who it is answering, if a teammate called.
    pub fn assisting(&self) -> Option<Vec3> {
        self.assist.map(|a| a.0)
    }

    /// Send it somewhere (feet), before anything but a fight; None lets
    /// it go back to its orders. The route is worked out afresh.
    pub fn send_to(&mut self, at: Option<Vec3>) {
        self.move_to = at;
        self.route.clear();
        self.route_areas.clear();
        self.next = 0;
        self.repath = 0.0;
    }

    /// Route left to walk, m.
    pub fn route_left(&self) -> f32 {
        let rest = self.route.get(self.next..).unwrap_or(&[]);
        rest.windows(2).map(|w| w[0].distance(w[1])).sum()
    }

    /// Start a life (a round, a respawn, new orders): forget the last
    /// one and take up the orders.
    pub(super) fn new_life(&mut self) {
        self.route.clear();
        self.route_areas.clear();
        self.next = 0;
        self.lead = None;
        self.lead_who = None;
        self.assist = None;
        self.order = None;
        self.urgent_until = 0.0;
        self.roam = None;
        self.target = None;
        self.toss = None;
        self.plan = None;
        self.avert = None;
        self.guard = None;
        self.guard_at = false;
        self.guard_skip = 0;
        self.site = self.orders.site;
        self.hold = self.orders.hold.clone();
        self.hold_since = None;
        self.hunting = false;
        self.corners = default();
        self.area = None;
        self.prev_area = None;
        self.stuck_count = 0;
        self.stuck_links.clear();
        let (lo, hi) = match self.orders.role {
            Role::Attack => ATTACK_PATIENCE,
            _ => DEFEND_PATIENCE,
        };
        self.patience = lo + (hi - lo) * self.rand() as f64;
    }

    /// The grenade throw planned or carried out lately.
    pub fn grenade_plan(&self) -> Option<&GrenadePlan> {
        self.plan.as_ref().map(|(p, _)| p)
    }

    /// Flashes held back so far since they would have blinded a teammate.
    pub fn flashes_held(&self) -> u32 {
        self.flashes_held
    }

    /// Throwing a grenade now.
    pub fn throwing(&self) -> bool {
        self.toss.is_some()
    }

    /// Looking away from a flash about to go off: from where.
    pub fn averting(&self) -> Option<Vec3> {
        self.avert.map(|a| a.from)
    }
}

/// Share of roaming goals picked at the map's objectives (bomb sites,
/// hostages) rather than anywhere, so teams meet where CS:S bots do.
pub(super) const OBJECTIVE_SHARE: f32 = 0.7;

/// The map's objective points (engine space, feet height of the volume's
/// bottom or the entity): bomb sites (`func_bomb_target`), hostage rescue
/// zones and hostages.
pub fn objectives(map: &crate::map::MapEntities) -> Vec<Vec3> {
    objectives_of(
        map,
        &[
            "func_bomb_target",
            "func_hostage_rescue",
            "hostage_entity",
        ],
    )
}

/// `objectives` of the given classes only.
pub fn objectives_of(map: &crate::map::MapEntities, classes: &[&str]) -> Vec<Vec3> {
    use crate::map::entities::{entity_rotation, entity_to_engine};
    map.entities
        .iter()
        .filter_map(|e| {
            let class = e.classname();
            if !classes.contains(&class) {
                return None;
            }
            match class {
                "func_bomb_target" | "func_hostage_rescue" => {
                    let points: Vec<Vec3> = e.hulls.iter().flat_map(|h| h.points.iter().copied()).collect();
                    if points.is_empty() {
                        return None;
                    }
                    let lo = points.iter().fold(Vec3::splat(f32::MAX), |a, p| a.min(*p));
                    let hi = points.iter().fold(Vec3::splat(f32::MIN), |a, p| a.max(*p));
                    let local = Vec3::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0, lo.z);
                    Some(entity_to_engine(
                        e.origin() + entity_rotation(e.angles()) * local,
                        map.scale,
                    ))
                }
                "hostage_entity" => Some(entity_to_engine(e.origin(), map.scale)),
                _ => None,
            }
        })
        .collect()
}

/// Seconds a bot keeps chasing what it saw or heard.
pub(super) const MEMORY: f64 = 15.0;
/// A remembered or roaming goal counts as reached within this, m.
pub(super) const ARRIVED: f32 = 2.0;
/// A goal it was sent to counts as reached within half `ARRIVED` and this
/// much above or below, m.
pub(super) const SENT_RISE: f32 = 0.5;
/// A sound is heard when its distance gain at the bot is above this.
pub(super) const HEARING_GAIN: f32 = 0.05;

/// Bots hear enemies' sounds (shots, footsteps, impacts they cause).
pub(super) fn hear(
    mut sounds: MessageReader<crate::map::PlaySound>,
    bank: Option<Res<crate::map::sound::SoundBank>>,
    mut bots: Query<(&mut Bot, &Transform, &Team, &Health)>,
    teams: Query<(&Team, &Transform, Option<&Health>)>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();
    for s in sounds.read() {
        let (Some(source), Some(at)) = (s.source, s.at) else {
            continue;
        };
        let Ok((source_team, source_at, source_health)) = teams.get(source) else {
            continue;
        };
        // Not a dead one's (its last cry, its gun hitting the floor).
        if source_health.is_some_and(|h| h.current <= 0.0) {
            continue;
        }
        let level = bank
            .as_ref()
            .and_then(|b| b.0.entry(&s.entry).map(|e| e.level))
            .unwrap_or(crate::map::SoundLevel::Db(75.0));
        let feet = source_at.translation - Vec3::Y * CAPSULE_HEIGHT / 2.0;
        for (mut bot, t, team, health) in &mut bots {
            if team == source_team || health.current <= 0.0 {
                continue;
            }
            if crate::map::sound::distance_gain(level, t.translation.distance(at)) >= HEARING_GAIN {
                bot.lead = Some((feet, now));
                bot.lead_who = Some(source);
            }
        }
    }
}

