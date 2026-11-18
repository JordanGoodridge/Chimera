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

/// Spawn a bot on `team` at a spawn point (the team's, if the map has
/// them), moving with the match's movement (`game::movement::match_movement`).
pub fn add_bot(world: &mut World, team: Team) -> Option<Entity> {
    let e = spawn_bot(world, team)?;
    if let Some(mut q) = world.get_resource_mut::<BotQuota>() {
        q.quota += 1;
    }
    Some(e)
}

/// `add_bot` without raising `bot_quota` (the quota's own adds).
pub(super) fn spawn_bot(world: &mut World, team: Team) -> Option<Entity> {
    let spawns: Vec<(Transform, Option<Team>)> = world
        .query::<(&Transform, &SpawnPoint)>()
        .iter(world)
        .map(|(t, s)| (*t, s.team))
        .collect();
    let numbers: Vec<u32> = world.query::<&Bot>().iter(world).map(|b| b.number).collect();
    let bots = numbers.len();
    // The lowest free number, so seeds repeat run to run whatever else
    // was spawned (entity ids shift with every spawn).
    let number = (1..).find(|n| !numbers.contains(n)).unwrap_or(1);
    let seed = path::hash64(0xB075_EED5 ^ (team.0 as u64) << 40, number as usize);
    let round = world.get_resource::<crate::core::RoundRestarts>().map_or(0, |r| r.0);
    let own: Vec<&Transform> = spawns.iter().filter(|s| s.1 == Some(team)).map(|s| &s.0).collect();
    let pool: Vec<&Transform> = if own.is_empty() {
        spawns.iter().map(|s| &s.0).collect()
    } else {
        own
    };
    // Skip the first spawn, where the local player starts.
    let at = **pool.get((bots + 1) % pool.len().max(1)).or(pool.first())?;
    // The match's movement, not the local player's current one (a bot
    // added while the player flew with noclip flew too).
    let movement = crate::game::movement::match_movement(world)?;
    let yaw = at.rotation.to_euler(EulerRot::YXZ).0;
    // A name from the game's bot profiles at `bot_difficulty` (one no bot
    // here has), else "Bot <number>".
    let quota = world.get_resource::<BotQuota>().cloned().unwrap_or_default();
    let level = crate::map::bot_profiles::difficulty::of_level(quota.difficulty);
    let taken: Vec<String> = world
        .query::<&Bot>()
        .iter(world)
        .filter_map(|b| b.profile.clone())
        .collect();
    let profile = world
        .get_resource::<crate::map::bot_profiles::BotProfiles>()
        .and_then(|p| p.choose(level, team.0, &taken, seed >> 7))
        .cloned();
    let name = match &profile {
        Some(p) if quota.prefix.trim().is_empty() => p.name.clone(),
        Some(p) => format!("{} {}", quota.prefix.trim(), p.name),
        None => format!("Bot {number}"),
    };
    let difficulty = profile.as_ref().map_or(level, |p| if p.difficulty & level != 0 { level } else { p.difficulty });
    let mut commands = world.commands();
    // Characters aren't rotated; the spawn's facing becomes the look yaw.
    let e = spawn_character(
        &mut commands,
        Transform::from_translation(at.translation),
        team,
        movement,
    );
    commands.entity(e).insert((
        Name::new(name),
        crate::core::Seed(seed),
        {
            let mut bot = Bot {
                number,
                seed,
                profile: profile.map(|p| p.name),
                difficulty,
                ..default()
            };
            bot.reseed(round);
            bot
        },
        Intent { yaw, ..default() },
    ));
    world.flush();
    Some(e)
}

impl Bot {
    /// Its number ("Bot 3").
    pub fn number(&self) -> u32 {
        self.number
    }

    /// Restart the dice for a round: from the bot's seed and the round
    /// number only.
    pub(super) fn reseed(&mut self, round: u32) {
        self.rng = path::hash64(self.seed ^ 0x2545_F491_4F6C_DD1D, round as usize) | 1;
    }

    /// A roll of its dice, 0 to 1 (seeded: `reseed`), for rules outside
    /// the brain (its votes: `game::votes`).
    pub fn roll(&mut self) -> f32 {
        self.rand()
    }

    pub(super) fn rand(&mut self) -> f32 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        (self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Where to aim on a character: chest height above its origin (the
/// origin is the capsule's centre).
pub(super) const AIM_HEIGHT: f32 = 0.35;
/// Fire when the aim is within this many degrees of the target.
pub(super) const FIRE_CONE_DEG: f32 = 3.0;
/// Bots can't see while a flash leaves the screen whiter than this.
pub(super) const BLIND_ALPHA: f32 = 0.5;
/// Seconds between new aim wobbles.
pub(super) const AIM_REROLL: f32 = 0.4;

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn think(
    mut bots: Query<(
        Entity,
        &mut Bot,
        &mut Intent,
        &Transform,
        &MovementState,
        &Team,
        &Health,
        Option<&crate::core::Blinded>,
        Option<&mut Inventory>,
        &crate::core::Velocity,
        Option<&crate::weapon::ViewPunch>,
    )>,
    others: Query<(Entity, &Transform, &Team, &Health), With<Intent>>,
    teamless: Query<(&Transform, &Health), (With<Intent>, Without<Team>)>,
    arms: grenades::Arms,
    triggers: Query<&crate::weapon::Trigger>,
    projectiles: Query<(Entity, &Transform, &Projectile)>,
    smoke: Query<&crate::core::SightBlocker>,
    breakable: Query<(), With<crate::core::Damageable>>,
    spatial: SpatialQuery,
    nav: Option<Res<NavMesh>>,
    brushes: Option<Res<MapBrushes>>,
    cfg: Res<BotConfig>,
    time: Res<Time>,
    tactics: Res<Tactics>,
    vis: Option<Res<crate::map::vis::ActiveVisibility>>,
    round: (
        Option<Res<crate::core::FreezeTime>>,
        Option<Res<crate::objectives::RoundOpen>>,
        Query<&crate::weapon::Hitscan>,
        Query<
            (Entity, &Transform, &Team, &Health, &Intent, &MovementState, Option<&crate::core::Velocity>),
            Without<Bot>,
        >,
    ),
) {
    let brushes = brushes.as_deref().map_or(&[][..], |b| &b.0[..]);
    // Freeze time: it may look around, not walk (the rules hold its
    // intent), so standing still isn't being stuck. Grenades only while
    // the round is open (not frozen, not over).
    let frozen = round.0.is_some_and(|f| f.0);
    let open = round.1.is_none_or(|o| o.0) && !frozen;
    let dt = time.delta_secs();
    let now = time.elapsed_secs_f64();
    // World traces for flash sight (a line) and grenade arcs (the
    // grenade's box swept, half size given): characters aren't in the way.
    let no_nav = NavMesh::default();
    let filter = SpatialQueryFilter::default().with_mask(crate::core::SOLID_LAYERS);
    let not_character = |e: Entity| !others.contains(e);
    let world_trace = |a: Vec3, b: Vec3| -> Option<(Vec3, Vec3)> {
        let d = b - a;
        let dir = Dir3::new(d).ok()?;
        spatial
            .cast_ray_predicate(a, dir, d.length(), true, &filter, &not_character)
            .map(|h| (a + *dir * h.distance, h.normal))
    };
    let box_trace = |a: Vec3, b: Vec3, half: f32| -> Option<(Vec3, Vec3)> {
        let d = b - a;
        let dir = Dir3::new(d).ok()?;
        let config = ShapeCastConfig {
            max_distance: d.length(),
            ignore_origin_penetration: true,
            ..ShapeCastConfig::DEFAULT
        };
        let shape = Collider::cuboid(half * 2.0, half * 2.0, half * 2.0);
        spatial
            .cast_shape_predicate(&shape, a, Quat::IDENTITY, dir, &config, &filter, &not_character)
            .map(|h| (a + *dir * h.distance, h.normal1.normalize_or_zero()))
    };
    // Where every living character looks (teammates a flash must spare).
    // (With the bot's number, 0 for a person: who gives way in a door.)
    let view = |e: Entity, t: &Transform, team: &Team, intent: &Intent, state: &MovementState, v: Vec3, number: u32| {
        (
            e,
            *team,
            grenades::MateView {
                eye: t.translation + state.eye_offset,
                look: intent.look_rotation() * Vec3::NEG_Z,
                velocity: v,
                person: number == 0,
            },
            number,
        )
    };
    let views: Vec<(Entity, Team, grenades::MateView, u32)> = bots
        .iter()
        .filter(|b| b.6.current > 0.0)
        .map(|(e, b, intent, t, state, team, _, _, _, v, _)| view(e, t, team, intent, state, v.0, b.number.max(1)))
        .chain(
            round
                .3
                .iter()
                .filter(|p| p.3.current > 0.0)
                .map(|(e, t, team, _, intent, state, v)| view(e, t, team, intent, state, v.map_or(Vec3::ZERO, |v| v.0), 0)),
        )
        .collect();
    // What every living bot sees: the nearest living enemy in sight, into
    // `Bot::sighted` (taken in the loop below).
    //
    // Nearest first, so the first one in sight is the one: a bot in a crowd
    // traced to every enemy each tick (a full server's lobby on a minigame
    // map, all in each other's PVS: multiplayer.md "Capacity"). The same pick
    // as testing them all (ties in the query's order). An enemy whose
    // visibility cluster the bot's can't see (the map's PVS, which is
    // conservative) is hidden by the world without a trace.
    //
    // The traces are the server's capacity limit; each bot's are read-only
    // and independent of the others' results, so the bots spread over the
    // compute task pool (`par_iter_mut`; docs/performance.md, "Bots' sight
    // in parallel"). Each writes only its own `sighted`, from state nothing
    // in this system changes, so the result is what a serial loop gives
    // whatever the order the threads run in. (Part of `think`, not a system
    // of its own: another system would reorder the unordered ones around it.)
    let vis = vis.as_deref().map(|v| &*v.0);
    let clusters: bevy::ecs::entity::EntityHashMap<u32> = vis
        .map(|v| {
            others
                .iter()
                .filter(|o| o.3.current > 0.0)
                .filter_map(|(e, ot, ..)| Some((e, v.cluster_at(ot.translation + Vec3::Y * AIM_HEIGHT)?)))
                .collect()
        })
        .unwrap_or_default();
    bots.par_iter_mut().for_each(|(me, mut bot, _, t, state, team, health, blinded, _, _, _)| {
        bot.sighted = None;
        if health.current <= 0.0 || blinded.is_some_and(|b| b.alpha_at(now) > BLIND_ALPHA) {
            return;
        }
        let eye = t.translation + state.eye_offset;
        let eye_cluster = vis.and_then(|v| v.cluster_at(eye));
        let mut candidates: Vec<(Entity, Vec3, f32)> = Vec::new();
        for (e, ot, oteam, oh) in &others {
            if e == me || oteam == team || oh.current <= 0.0 {
                continue;
            }
            if let (Some(v), Some(a), Some(b)) = (vis, eye_cluster, clusters.get(&e))
                && v.visible.get(a as usize).is_some_and(|row| !row.is_empty())
                && !v.sees(a, *b)
            {
                continue;
            }
            let aim = ot.translation + Vec3::Y * AIM_HEIGHT;
            candidates.push((e, aim, (aim - eye).length()));
        }
        candidates.sort_by(|a, b| a.2.total_cmp(&b.2));
        let filter = SpatialQueryFilter::from_excluded_entities([me]).with_mask(crate::core::SOLID_LAYERS);
        for (e, aim, dist) in candidates {
            let Ok(dir) = Dir3::new(aim - eye) else { continue };
            let visible = spatial
                .cast_ray(eye, dir, dist, true, &filter)
                .is_none_or(|h| h.entity == e || h.distance >= dist - 0.05);
            // Nor through smoke (specs/cs_source/grenades.md Q16).
            if visible && !smoke.iter().any(|s| s.blocks(eye, aim)) {
                bot.sighted = Some((e, aim, dist));
                break;
            }
        }
    });
    for (me, mut bot, mut intent, t, state, team, health, blinded, mut inv, velocity, punch) in &mut bots {
        if health.current <= 0.0 {
            bot.target = None;
            bot.toss = None;
            bot.avert = None;
            bot.dead = true;
            bot.activity = Activity::Dead;
            continue;
        }
        if bot.dead {
            // Respawned.
            bot.dead = false;
            bot.new_life();
        }
        // The enemy it remembers is dead (or gone): nothing left there.
        if bot
            .lead_who
            .is_some_and(|who| !others.get(who).is_ok_and(|(_, _, _, h)| h.current > 0.0))
        {
            bot.lead = None;
            bot.lead_who = None;
        }
        let eye = t.translation + state.eye_offset;
        // A flashed bot sees nothing until the white is mostly gone, and
        // aims worse while it fades.
        let white = blinded.map_or(0.0, |b| b.alpha_at(now));
        let blind = white > BLIND_ALPHA;
        // The nearest living enemy in sight (the pass above).
        let best = bot.sighted.take();

        // Flashes in flight it sees: now and then look away until they pop.
        bot.watched.retain(|g| projectiles.contains(*g));
        if !blind {
            for (g, gt, p) in &projectiles {
                let flash = matches!(p.effect, crate::weapon::grenade::GrenadeEffect::Flash(_));
                let at = gt.translation;
                if !flash
                    || p.thrower == Some(me)
                    || bot.watched.contains(&g)
                    || at.distance(eye) > grenades::FLASH_WATCH
                    || world_trace(eye, at).is_some()
                    || smoke.iter().any(|s| s.blocks(eye, at))
                {
                    continue;
                }
                bot.watched.push(g);
                if bot.rand() < grenades::FLASH_REACT {
                    let left = (p.fuse - p.ticks as f32 * dt).max(0.0);
                    bot.avert = Some(grenades::Avert {
                        from: at,
                        until: now + left as f64 + 0.3,
                        grenade: Some(g),
                    });
                }
            }
        }
        if let Some(a) = bot.avert.as_mut() {
            match a.grenade.map(|g| projectiles.get(g)) {
                Some(Ok((_, gt, _))) => a.from = gt.translation,
                // Gone: it popped.
                Some(Err(_)) => a.until = now,
                None => {}
            }
            if now >= a.until {
                bot.avert = None;
            }
        }
        if bot.toss.is_none() && bot.plan.as_ref().is_some_and(|(_, until)| now > *until) {
            bot.plan = None;
        }
        // An enemy in sight before the pin is out: forget the throw.
        if best.is_some()
            && let Some(toss) = bot.toss.as_ref()
            && toss.pulled.is_none()
        {
            let previous = toss.previous;
            bot.toss = None;
            if let Some(inv) = inv.as_deref_mut() {
                inv.wanted = previous
                    .filter(|p| inv.weapons.contains(p))
                    .or_else(|| grenades::best_weapon(inv, &arms));
            }
        }

        // Semi-automatics fire once per press: let go every other tick.
        let pressed = intent.fire;
        let semi = inv
            .as_deref()
            .and_then(|i| i.active)
            .and_then(|w| triggers.get(w).ok())
            .is_some_and(|t| !t.automatic);
        // How much of the punch its bullets take.
        let punch_scale = inv
            .as_deref()
            .and_then(|i| i.active)
            .and_then(|w| round.2.get(w).ok())
            .map_or(0.0, |h| h.punch_scale);
        intent.fire = false;
        match best {
            Some((e, aim, _)) => {
                bot.contact = now;
                // Remember where they were (feet) for after they hide.
                bot.lead = Some((
                    aim - Vec3::Y * (AIM_HEIGHT + CAPSULE_HEIGHT / 2.0),
                    time.elapsed_secs_f64(),
                ));
                bot.lead_who = Some(e);
                if bot.target == Some(e) {
                    bot.seen += dt;
                } else {
                    bot.target = Some(e);
                    bot.seen = 0.0;
                }
                bot.wobble.1 -= dt;
                if bot.wobble.1 <= 0.0 {
                    let (a, r) = (bot.rand() * std::f32::consts::TAU, bot.rand().sqrt());
                    let error = cfg.aim_error * (1.0 + FLASHED_AIM * white);
                    bot.wobble = (Vec2::from_angle(a) * r * error.to_radians(), AIM_REROLL);
                }
                let to = aim - eye;
                // Recoil control: aimed so the bullets (eye angles plus
                // the weapon's share of the punch) stay on the target,
                // pulling down against the kick.
                let kick = punch.map_or(Vec2::ZERO, |p| p.0.truncate()) * punch_scale * cfg.recoil_control;
                let want_yaw = (-to.x).atan2(-to.z) + bot.wobble.0.x - kick.y;
                let want_pitch = to.y.atan2(to.xz().length()) + bot.wobble.0.y - kick.x;
                let step = cfg.turn_rate.to_radians() * dt;
                let dyaw = wrap(want_yaw - intent.yaw);
                intent.yaw = wrap(intent.yaw + dyaw.clamp(-step, step));
                intent.pitch += (want_pitch - intent.pitch).clamp(-step, step);
                let off = wrap(want_yaw - intent.yaw).abs().max((want_pitch - intent.pitch).abs());
                intent.fire = cfg.dont_shoot == 0
                    && !bot.frozen
                    && bot.seen >= cfg.reaction
                    && off.to_degrees() < FIRE_CONE_DEG
                    && !(semi && pressed);
            }
            None => {
                bot.target = None;
                bot.seen = 0.0;
            }
        }

        // The round closed (won, or frozen) before the pin came out: no
        // throw.
        if !open && bot.toss.as_ref().is_some_and(|t| t.pulled.is_none()) {
            let previous = bot.toss.take().and_then(|t| t.previous);
            if let Some(inv) = inv.as_deref_mut() {
                inv.wanted = previous
                    .filter(|p| inv.weapons.contains(p))
                    .or_else(|| grenades::best_weapon(inv, &arms));
            }
        }
        // A throw in progress takes over.
        if bot.toss.is_some() {
            bot.activity = Activity::Throwing;
            match inv.as_deref_mut() {
                Some(inv) => {
                    grenades::step_toss(&mut bot, &mut intent, inv, &arms, &cfg, now, dt);
                }
                None => bot.toss = None,
            }
            continue;
        }
        // Not throwing: a grenade in hand goes back for a gun.
        if let Some(inv) = inv.as_deref_mut()
            && inv
                .active
                .is_some_and(|a| arms.get(a).is_ok_and(|(_, t)| t.is_some_and(|t| !t.primed())))
            && inv.wanted.is_none()
            && let Some(gun) = grenades::best_weapon(inv, &arms)
        {
            inv.wanted = Some(gun);
            intent.fire = false;
        }
        // Looking away from a flash: turn the back to it and wait.
        if let Some(a) = bot.avert {
            let away = eye - a.from;
            let want_yaw = (-away.x).atan2(-away.z);
            let step = cfg.turn_rate.to_radians() * dt;
            intent.yaw = wrap(intent.yaw + wrap(want_yaw - intent.yaw).clamp(-step, step));
            intent.pitch -= intent.pitch.clamp(-step, step);
            intent.fire = false;
            intent.move_axis = Vec2::ZERO;
            intent.jump = false;
            bot.activity = Activity::Averting;
            continue;
        }

        // Strafe in place, switching direction every 0.4-1.2 s.
        bot.strafe_left -= dt;
        if bot.strafe_left <= 0.0 {
            bot.strafe_left = 0.4 + 0.8 * bot.rand();
            let r = bot.rand();
            bot.strafe = if r < 0.4 {
                -1.0
            } else if r < 0.8 {
                1.0
            } else {
                0.0
            };
        }
        intent.move_axis = if cfg.stop == 0 && !bot.frozen && bot.target.is_some() {
            Vec2::new(bot.strafe, 0.0)
        } else {
            Vec2::ZERO
        };
        intent.jump = false;
        intent.crouch = false;
        if bot.target.is_some() {
            bot.route.clear();
            bot.activity = Activity::Fighting;
            continue;
        }
        let feet = t.translation - Vec3::Y * CAPSULE_HEIGHT / 2.0;
        // What was seen or heard goes stale, or is where it went.
        if bot
            .lead
            .is_some_and(|(at, when)| now - when > MEMORY || at.distance(feet) < ARRIVED)
        {
            bot.lead = None;
        }
        if let Some((_, when)) = bot.lead {
            bot.contact = bot.contact.max(when);
        }
        if bot
            .assist
            .is_some_and(|(at, when)| now - when > ASSIST_TIME || at.distance(feet) < ARRIVED * 2.0)
        {
            bot.assist = None;
        }
        if bot.roam.is_some_and(|at| at.distance(feet) < ARRIVED) {
            bot.roam = None;
        }
        if cfg.stop != 0 || bot.frozen {
            bot.route.clear();
            bot.goal = None;
            bot.activity = Activity::Idle;
            continue;
        }
        // Without a mesh nothing walks, but throws still happen.
        let nav = nav.as_deref().unwrap_or(&no_nav);
        if let Some(here) = nav.area_at(feet + Vec3::Y * 0.1)
            && bot.area != Some(here)
        {
            bot.prev_area = bot.area;
            bot.area = Some(here);
            bot.entered = feet;
        }
        let plan = tactics.team(*team);
        let mates: Vec<Vec3> = others
            .iter()
            .filter(|(e, _, oteam, oh)| *e != me && *oteam == team && oh.current > 0.0)
            .map(|(_, ot, ..)| ot.translation - Vec3::Y * CAPSULE_HEIGHT / 2.0)
            .collect();
        let goal = choose_goal(&mut bot, me, feet, now, &tactics, plan, nav, &mates);
        bot.goal = goal;
        // Now and then a grenade at the remembered enemy or the site.
        if let Some(inv) = inv.as_deref()
            && cfg.grenades > 0
            && open
            && now >= bot.next_toss_check
        {
            bot.next_toss_check = now + grenades::TOSS_CHECK;
            if now >= bot.next_toss {
                // Gathering before the site: flashes onto where the group
                // comes onto it, smokes between there and the defenders
                // watching it (`tactics::smoke_spot`).
                let staging = plan.is_some_and(|p| p.staging && p.site == bot.site);
                let site = bot.site.and_then(|s| tactics.sites.get(s));
                let approach = |s: &tactics::Site| {
                    s.approaches[0]
                        .iter()
                        .copied()
                        .enumerate()
                        .min_by(|a, b| a.1.distance(feet).total_cmp(&b.1.distance(feet)))
                };
                let smoke = |s: &tactics::Site| {
                    approach(s).map_or(s.point, |(i, a)| s.smokes.get(i).copied().flatten().unwrap_or(a))
                };
                let objective = match bot.activity {
                    _ if staging => site.map(|s| (approach(s).map_or(s.point, |a| a.1), smoke(s))),
                    Activity::ToSite | Activity::Following | Activity::Waiting => site.map(|s| (s.point, smoke(s))),
                    Activity::Roaming if bot.roam_objective => bot.roam.map(|r| (r, r)),
                    _ => None,
                };
                let mates: Vec<grenades::MateView> = views
                    .iter()
                    .filter(|v| v.0 != me && v.1 == *team)
                    .map(|v| v.2)
                    .collect();
                let sight = |a: Vec3, b: Vec3| world_trace(a, b).is_none();
                consider_throw(
                    &mut bot, inv, &arms, &cfg, eye, feet, objective, staging, &mates, &sight, now, dt, &box_trace,
                );
            }
        }
        if bot.toss.is_some() {
            intent.move_axis = Vec2::ZERO;
            continue;
        }
        let params = path::CostParams {
            danger: plan.map(|p| p.danger.as_slice()).filter(|d| d.len() == nav.areas.len()),
            exposure: Some(tactics.exposure.as_slice()).filter(|e| e.len() == nav.areas.len()),
            team_seed: plan.map_or(0, |p| p.seed),
            bot_seed: bot.seed,
            stuck: &tactics.failed_links,
        };
        // Never left hanging on a ladder: with nowhere to go, it finishes
        // the climb (its route's end).
        let goal = goal.or_else(|| state.on_ladder.then(|| bot.route.last().copied()).flatten());
        if frozen {
            // Held still: no progress expected, nothing in the way.
            bot.progress = (feet, 0.0);
            bot.toward.1 = now;
            bot.blocked = 0.0;
        }
        let step = match goal {
            Some(goal) => walk_route(&mut bot, nav, feet, goal, &params, dt, now, state.on_ladder, state.on_ground, brushes),
            // Waiting keeps its route (for comparing with the leader's).
            None if bot.activity == Activity::Waiting => None,
            None => {
                bot.route.clear();
                bot.route_areas.clear();
                None
            }
        };
        // Nowhere left to go on a ladder (a goal reached from it, a step
        // above the floor): step off it.
        let step = step.or_else(|| state.on_ladder.then(|| off_ladder(nav, feet)).flatten());
        // Look at what matters (`choose_look`), turning a little slower
        // than in a fight; walk wherever the route goes, whatever the
        // view.
        let looking = intent.look_rotation() * Vec3::NEG_Z;
        let los = |a: Vec3, b: Vec3| world_trace(a, b).is_none() && !smoke.iter().any(|s| s.blocks(a, b));
        let mut look = choose_look(&mut bot, nav, eye, feet, step.map(|s| s.dir), looking, now, &los);
        let mut pitch_limit = LOOK_PITCH;
        if let Some(step) = step {
            if let Some(r) = step.rungs {
                // A ladder: square to it. Up: looking up, pushing in.
                // Down: level (the movement grabs a ladder along the
                // view), pushing in to grab it, then backing off it.
                let up = step.climb.is_some_and(|c| c.y > feet.y);
                look = eye + r.face + Vec3::Y * if up { 6.0 } else { 0.0 };
                pitch_limit = LADDER_PITCH;
            } else if let Some(rung) = step.climb {
                // A climb or drop with no ladder: face it, looking up (or
                // down) the way to go.
                look = rung + Vec3::Y * (rung.y - feet.y).signum() * 2.0;
                pitch_limit = LADDER_PITCH;
            } else if step.dismount {
                // Off a ladder's top: level, the way out (on the ladder:
                // square to it, `rungs`, above).
                look = eye + step.dir * 2.0;
            } else if bot.blocked > USE_AFTER {
                // Stuck: face the way (a door to open, a ledge to jump).
                look = eye + step.dir * 2.0;
            }
        }
        bot.look_at = Some(look);
        let to = look - eye;
        let want_yaw = (-to.x).atan2(-to.z);
        let want_pitch = to.y.atan2(to.xz().length()).clamp(-pitch_limit, pitch_limit);
        let turn = cfg.turn_rate.to_radians() * SCAN_TURN * dt;
        intent.yaw = wrap(intent.yaw + wrap(want_yaw - intent.yaw).clamp(-turn, turn));
        intent.pitch += (want_pitch - intent.pitch).clamp(-turn, turn);
        if let Some(step) = step {
            // Keep off teammates and other teamless characters
            // (hostages) close by (they block each other in doorways
            // otherwise).
            let mut dir = step.dir;
            let near = others
                .iter()
                .filter(|(e, _, oteam, oh)| *e != me && *oteam == team && oh.current > 0.0)
                .map(|(_, ot, ..)| ot)
                .chain(teamless.iter().filter(|(_, h)| h.current > 0.0).map(|(t, _)| t));
            for ot in near {
                let away = (t.translation - ot.translation).with_y(0.0);
                let d = away.length();
                if d < PERSONAL_SPACE && (ot.translation.y - t.translation.y).abs() < 1.5 {
                    let push = if d > 1e-3 { away / d } else { Vec3::X };
                    dir += push * (1.0 - d / PERSONAL_SPACE) * SEPARATION;
                }
            }
            // Teammates in the way (`give_way`): keep right of one coming
            // the other way (and wait for it, giving way), step round one
            // standing there, don't run into one walking ahead.
            let mut pace = step.pace;
            if step.rungs.is_none() && step.climb.is_none() {
                let mine = views.iter().find(|v| v.0 == me).map_or(0, |v| v.3);
                let feet_eye = t.translation + state.eye_offset;
                for v in views.iter().filter(|v| v.0 != me && v.1 == *team) {
                    let rel = v.2.eye - feet_eye;
                    let Some(way) = give_way(step.dir, rel, v.2.velocity, mine, v.3, bot.number) else {
                        continue;
                    };
                    dir += way.side;
                    pace = pace.min(way.pace);
                }
            }
            let dir = dir.with_y(0.0).normalize_or(step.dir);
            let rot = intent.yaw_rotation();
            let (fwd, right) = (rot * Vec3::NEG_Z, rot * Vec3::X);
            let axis = Vec2::new(dir.dot(right), dir.dot(fwd));
            // Full speed whichever way (the larger key fully pressed); on
            // a ladder, straight at it.
            intent.move_axis = match step.rungs {
                // Stepping off its top sideways: climbing on, and the
                // sideways key toward the way out.
                Some(r) if state.on_ladder && step.dismount => {
                    let perp = Vec3::Y.cross(r.normal);
                    let across = step.target.map_or(0.0, |t| (t - feet).dot(perp));
                    let side = if across.abs() > LADDER_CENTRE {
                        (perp * across.signum()).dot(right).signum()
                    } else {
                        0.0
                    };
                    if fwd.with_y(0.0).normalize_or_zero().dot(r.face) < LADDER_FACING {
                        Vec2::ZERO
                    } else {
                        Vec2::new(side, 1.0)
                    }
                }
                // On the ladder: straight on, stepping sideways back to
                // its middle line.
                Some(r) if state.on_ladder => {
                    let perp = Vec3::Y.cross(r.normal);
                    let off = (feet - r.foot).dot(perp) - r.shift;
                    // (Exactly on a line off the middle: `Bot::ladder_shift`.)
                    let centre = if r.shift == 0.0 { LADDER_CENTRE } else { LADDER_ON_SHIFT };
                    // (Not near the top going up: the ladder lets go with
                    // the sideways speed still on, flinging it off the top.)
                    let near_top = step.climb.is_some_and(|c| c.y > feet.y && c.y - feet.y < LADDER_TOP_STEADY);
                    let side = if off.abs() > centre && !near_top {
                        -(perp * off.signum()).dot(right).signum()
                    } else {
                        0.0
                    };
                    let up = step.climb.is_some_and(|c| c.y > feet.y);
                    if fwd.with_y(0.0).normalize_or_zero().dot(r.face) < LADDER_FACING {
                        // Turned away (aiming at something): hang there
                        // until square again (keys move it along the view).
                        Vec2::ZERO
                    } else {
                        Vec2::new(side, if up { 1.0 } else { -1.0 })
                    }
                }
                // A drop with no mesh ladder: straight on, over the edge.
                // A climb: straight on once facing it (still turning,
                // toward it: forward alone circles it).
                _ if step.rungs.is_none()
                    && step.climb.is_some_and(|c| c.y < feet.y || axis.y > CLIMB_FACING) =>
                {
                    Vec2::Y
                }
                _ => axis / axis.abs().max_element().max(1e-3) * pace,
            };
            // Walking into something: press use now and then (doors),
            // jump it every so often (ledges).
            if velocity.0.xz().length() < BLOCKED_SPEED && !frozen {
                bot.blocked += dt;
            } else {
                bot.blocked = 0.0;
            }
            let b = bot.blocked;
            intent.use_key = b > USE_AFTER && (b / USE_PERIOD) as u32 % 2 == 0;
            let blocked = b > BLOCKED_JUMP && (b - BLOCKED_JUMP) % JUMP_PERIOD < dt * 1.5;
            // Jump is a fresh press each time (held, it jumps once), and
            // never on a ladder (that lets go of it); blocked at one (slow,
            // pressing in or sidestepping at its foot) it doesn't jump
            // either (the jump and the duck after it keep it off).
            let want_jump = ((step.jump && step.climb.is_none()) || (blocked && step.rungs.is_none())) && !state.on_ladder;
            intent.jump = want_jump && !bot.jump_held;
            bot.jump_held = intent.jump;
            if intent.jump && state.on_ground {
                bot.jumped = now;
            }
            // Duck in the air after a jump (a duck-jump clears higher
            // ledges, as CS:S bots do), and under a low ceiling ahead
            // (vents).
            let airborne = !state.on_ground && now - bot.jumped > 0.05 && now - bot.jumped < DUCK_JUMP;
            let ahead = feet + step.dir * LOW_AHEAD;
            let low = |from: Vec3| {
                // Not from inside something (a stair ahead).
                spatial
                    .cast_ray_predicate(
                        from + Vec3::Y * LOW_FROM,
                        Dir3::Y,
                        LOW_CEILING - LOW_FROM,
                        false,
                        &filter,
                        &not_character,
                    )
                    .is_some_and(|h| h.distance > 0.02)
            };
            // (Not for what's ahead when getting on a ladder starting above
            // the floor: that is the underside of what it climbs, a train
            // car, and a ducked box doesn't reach the ladder.)
            let high_ladder = step.rungs.is_some_and(|r| r.foot.y - feet.y > STEP_HEIGHT);
            let low_ceiling = !state.on_ladder && (low(feet) || (low(ahead) && !high_ladder));
            intent.crouch = step.crouch || airborne || low_ceiling;
            // Something breakable in the way (a vent grille, a window):
            // shoot it.
            // How long it has been walking to this route point.
            if let Some(target) = step.target
                && bot.toward.0.distance(target) > 0.3
            {
                bot.toward = (target, now);
            }
            let slow = now - bot.toward.1 > BREAK_SLOW;
            if (b > BREAK_AFTER || slow || step.dismount)
                && cfg.dont_shoot == 0
                && !bot.frozen
                && let Some(target) = step.target
            {
                // Along the way at a few heights, and at the route point.
                let ahead = feet + step.dir * 0.6;
                let aims = [
                    ahead + Vec3::Y * 0.1,
                    ahead + Vec3::Y * 0.5,
                    ahead + Vec3::Y * 1.0,
                    ahead + Vec3::Y * 1.5,
                    target + Vec3::Y * 0.3,
                ];
                let filter = SpatialQueryFilter::from_excluded_entities([me]).with_mask(crate::core::SOLID_LAYERS);
                // (No further than the route point: not a window past it.)
                let reach = BREAK_REACH.min(target.distance(eye) + BREAK_PAST);
                let hit = aims.into_iter().find_map(|aim| {
                    let dir = Dir3::new(aim - eye).ok()?;
                    spatial
                        .cast_ray(eye, dir, reach, true, &filter)
                        .filter(|h| breakable.contains(h.entity))
                        .map(|h| eye + *dir * h.distance)
                });
                if let Some(at) = hit {
                    let d = at - eye;
                    intent.yaw = (-d.x).atan2(-d.z);
                    intent.pitch = d.y.atan2(d.xz().length());
                    intent.fire = !(semi && pressed);
                    bot.look_at = Some(at);
                    if state.on_ladder {
                        // Hang on the ladder until it breaks.
                        intent.move_axis = Vec2::ZERO;
                    }
                }
            }
        } else {
            bot.blocked = 0.0;
            intent.use_key = false;
        }
    }
}

/// Teammates within this ahead (flat, m), and within this of the line
/// walked, are in the way (`give_way`).
pub(super) const WAY_AHEAD: f32 = 2.2;
pub(super) const WAY_WIDE: f32 = 0.9;
/// Faster than this toward the walker: coming the other way; slower than
/// this: standing (m/s).
pub(super) const ONCOMING: f32 = 1.0;
pub(super) const STANDING: f32 = 0.6;
/// How hard it steps aside, and the share of full speed it keeps while
/// giving way, behind someone slower, or passing one standing.
pub(super) const SIDESTEP: f32 = 1.2;
pub(super) const YIELD_PACE: f32 = 0.15;
pub(super) const BEHIND_PACE: f32 = 0.5;

