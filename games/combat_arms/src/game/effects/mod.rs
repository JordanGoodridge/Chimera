//! Combat Arms' effects and sounds (specs/combat_arms/effects_sounds.md;
//! section numbers below are its), built where they are shown from the
//! weapon events every world sees (a network client re-traces others'
//! rounds), in `Update`:
//!
//! - **Shots** (`shot_effects`, 1, 2, 4-8, 11): the fire sound (yours
//!   local, others' 3D at their muzzle with the weapon's or the
//!   silencer's radius, `CaMods`: silenced level ℓ plays `SilencedFireSnd`ℓ,
//!   a silenced part without a level nothing); the third-person muzzle
//!   group (`hh_muzzle`, the muzzle part's first) for others when the fire
//!   point is seen; your FireFX group (`test`) at your flash point; a
//!   casing (seen and near fire point, the FireFX ejecting); the tracer
//!   (`rules::TracerCounter`: yours by the last fitted part naming a
//!   tracer, others' by the ammo's; the muzzle part's look, else the
//!   ammo's; first ray only); per ray the impact: the surface type
//!   (characters 6, 4 for a head hit; the world's from `SurfaceGrid`,
//!   meshes tagged by `library::tag_surfaces`; nothing hit: sky, no
//!   impact), the ImpactFX's sound (the surface's list by ammo kind,
//!   one of four slots), its own group, and when the impact is seen the
//!   decal (`rules::CaMarks`: crowding, cap, fade) and the surface's
//!   burst (blood on characters); others' bullets fly by within 600 u
//!   (`rules::flyby`, played at the nearest point of the line); your hits
//!   on characters queue a ding (`rules::DingQueue`, played next frame);
//!   dry fire and knife swings their sounds.
//! - **View-model keys** (`key_sounds`): `sound_key n` plays the weapon's
//!   reload, select, deselect or misc sound (yours local, others' 3D, R
//!   `ReloadSndRadius`/`SelectSndRadius`); `SHELL_CASING` drops a casing
//!   for a weapon whose FireFX doesn't eject.
//! - **Grenades** (`grenade_effects`, 9): the ImpactFX by weapon
//!   (`[grenade]` in data/effects.toml): its sound at the blast (none on
//!   sky); the surface's grenade group when seen (Air in mid-air: none);
//!   the flash's and smoke's own groups always. Bounces (`bounce_sounds`,
//!   the server's): the surface's grenade sound, one per grenade.
//! - **Hurt** (`hurt_sounds`, the server's): a sound by damage kind and
//!   one by hit zone at the victim, radius 500, each kind one at a time
//!   across the server (`rules::HurtLimiter`).
//! - **Footsteps** (`footsteps`): yours by the sway phase
//!   (`rules::OwnSteps`, silent off the run flag), others' by their
//!   gait's animation keys (`rules::footsteps_between`: walking and
//!   crouching silent), the surface under the feet (ladders 200), radius
//!   1200; your jump voice.
//! - **Stepping** (`step_effects`): decals age (frozen while paused),
//!   tracers fly, casings fall and bounce (`rules::Casing`; the first
//!   bounce's sound), effect groups play (`fx::CaFx` on Chimera's
//!   particle pool, from the requests `CaFxRequest`).
//!
//! The front end (`ui::effects`) draws decals, tracers, casings, lights
//! and the first-person muzzle flash; `sounds` plays `CaSound`s.
//!
//! Known shortcuts (the spec's most literal reading where unclear;
//! docs/tech-debt.md):
//! - Third-person points: others' muzzle (flash socket), breach (casing
//!   start) and the sound of their shots are at their eye (no character
//!   models yet: characters.md); hit-zone surfaces for others' shots on a
//!   network client are the body's (6): a client sees no hit zones.
//! - No liquid volumes: nothing is under water (no half-volume impact
//!   sounds, under-water records, bubbles), a ray doesn't go on through
//!   water; exit decals and debris are off (as in all stock records).
//! - The world's surface types come by texture (`library::tag_surfaces`);
//!   props and doors are the world's; movable objects take no decals
//!   (none move yet), and no server-side decals on movers.
//! - Others' footstep animation rate is 1 (no third-person animations
//!   yet: characters.md); a network client takes others' gait from their
//!   movement state as replicated.
//! - Kill call-outs (`EnemyDown`) aren't played (they need the team
//!   filter on each client); map `SoundFX` objects aren't placed (no
//!   object classes yet); interface sounds are the UI's (`ui::desk`).
//! - Critical hits, ricochets, the pusher, screen tints and blast lights
//!   (never at detail 3: Quirk 9) aren't modelled.

pub mod data;
pub mod fx;
pub mod library;
pub mod rules;
pub mod sounds;

use std::collections::HashMap;

use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;

use self::{
    data::{AmmoKind, CaFxTables, CaWeaponFx, EFFECTS, ImpactFx},
    fx::{CaFx, effect_frame},
    rules::{Casing, CasingHit, CasingStep, DingQueue, Gait, HurtKind, HurtLimiter, OwnSteps, StepInputs, TracerCounter, TracerPath},
    sounds::{CHANNEL_FIRE, CHANNEL_GRENADE, CaSound, CaSoundFiles},
};
use super::{
    attachments::{CaMods, MUZZLE, ObserverSound},
    grenades::CaGrenade,
    inventory::UNIT,
    view_model::{CaViewKey, CaViewModel, CaViewParams, machine::ViewKey},
    weapons::CaZoom,
};
use crate::{
    core::{Damage, DamageKind, Health, Hitgroup, Intent, LocalPlayer, MovementState, SOLID_LAYERS, authoritative},
    map::{
        particles::{ParticleMaterials, Particles, WorldTracer},
        sound::SurfaceGrid,
    },
    weapon::{
        Inventory, Weapon, WeaponEvent, WeaponEventKind, aim_of, data::WeaponDataAppExt, grenade::Detonated,
        remote::RemoteShot,
    },
};

/// The player's effect options (the game's console variables).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct CaFxSettings {
    /// `Tracers`.
    pub tracers: bool,
    /// `ShellCasings`.
    pub shell_casings: bool,
    /// `MarkShow`.
    pub mark_show: bool,
    /// Play fly-bys where the original does (Quirk 6: near the world's
    /// origin); off: at the nearest point of the shot's line.
    pub original_flyby: bool,
}

impl Default for CaFxSettings {
    fn default() -> Self {
        Self {
            tracers: true,
            shell_casings: true,
            mark_show: true,
            original_flyby: false,
        }
    }
}

/// An effect group to play (whether or not the effect file has it: a
/// missing group plays nothing).
#[derive(Message, Clone, Debug, PartialEq)]
pub struct CaFxRequest {
    pub group: String,
    /// Metres.
    pub at: Vec3,
    /// The effect frame's +Z (a surface normal or the shot's direction).
    pub normal: Vec3,
    pub kind: FxKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxKind {
    /// Another's muzzle (third person).
    Muzzle,
    /// Your own FireFX group at your flash point.
    OwnFire,
    /// An ImpactFX's own group (smoke, flash).
    Ammo,
    /// A surface's impact burst (blood on characters).
    Surface,
    /// A surface's grenade explosion.
    Grenade,
}

/// A tracer flying.
#[derive(Clone, Debug, PartialEq)]
pub struct CaTracer {
    /// Units.
    pub path: TracerPath,
    pub look: data::TracerFx,
    pub age: f32,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct CaTracers(pub Vec<CaTracer>);

/// Tracer counters by weapon entity.
#[derive(Resource, Clone, Debug, Default)]
pub struct CaTracerCounters(pub HashMap<Entity, TracerCounter>);

/// A casing (section 8): its motion in units, drawn at its `Transform`
/// (metres) with its FireFX's model.
#[derive(Component, Clone, Debug)]
pub struct CaShell {
    pub owner: Entity,
    pub casing: Casing,
    /// The FireFX record (its model, skin, scale).
    pub fire: String,
    pub serial: u64,
    /// The first bounce's sound: the surface's.
    pub sounded: bool,
}

impl CaShell {
    /// Its scale (`ShellScale` × the growth).
    pub fn scale(&self) -> f32 {
        self.casing.scale()
    }
}

/// Clips' lengths (s) by file, for the server's hurt limit.
#[derive(Resource, Default)]
pub struct CaSoundLengths(pub HashMap<String, f32>);

/// Others' footstep clocks (s of their gait's animation) and last place.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct OthersSteps {
    pub clock: f32,
    pub last: Option<Vec3>,
    pub airborne: bool,
}

/// Where the effects run in `Update` (the front end draws after it).
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CaEffectsSet;

pub struct CaEffectsPlugin;

impl Plugin for CaEffectsPlugin {
    fn build(&self, app: &mut App) {
        app.register_weapon_hook::<CaWeaponFx>(EFFECTS, |e, _, p| {
            e.insert(p.clone());
        })
        .insert_resource(CaFxTables::built_in().clone())
        .init_resource::<CaFxSettings>()
        .init_resource::<rules::CaMarks>()
        .init_resource::<CaTracers>()
        .init_resource::<CaTracerCounters>()
        .init_resource::<CaFx>()
        .init_resource::<DingQueue>()
        .init_resource::<HurtLimiter>()
        .init_resource::<CaSoundFiles>()
        .init_resource::<CaSoundLengths>()
        .add_message::<CaSound>()
        .add_message::<CaFxRequest>()
        .add_message::<RemoteShot>()
        .add_message::<crate::map::GameSound>()
        .add_message::<crate::map::PlaySound>()
        .add_systems(
            Update,
            (
                play_dings,
                shot_effects,
                key_sounds,
                grenade_effects,
                footsteps,
                step_effects,
                spawn_fx,
                sounds::play,
            )
                .chain()
                .in_set(CaEffectsSet)
                .after(crate::map::DriveAnimation),
        )
        .add_systems(
            Update,
            (hurt_sounds, bounce_sounds)
                .run_if(authoritative)
                .in_set(CaEffectsSet)
                .before(sounds::play),
        );
    }
}

// ---------------------------------------------------------------------------
// The camera

/// The camera effects are seen from: the local player's eye and view.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    /// Metres.
    pub at: Vec3,
    pub forward: Vec3,
    pub player: Entity,
}

impl Camera {
    /// The gates (section 2) for a fire point and an impact (metres).
    pub fn gates(&self, fire: Vec3, impact: Vec3, others: bool) -> rules::Gates {
        rules::gates(self.at / UNIT, self.forward, fire / UNIT, impact / UNIT, others)
    }
}

type CameraQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static Transform, Option<&'static MovementState>, &'static Intent), With<LocalPlayer>>;

fn camera(q: &CameraQuery) -> Option<Camera> {
    let (e, t, state, intent) = q.iter().next()?;
    Some(Camera {
        at: t.translation + state.map_or(Vec3::ZERO, |s| s.eye_offset),
        forward: aim_of(intent.yaw, intent.pitch) * Vec3::NEG_Z,
        player: e,
    })
}

/// The surface type at a world point (metres): the nearest tagged drawn
/// triangle within 4 u; none: Unknown (0).
pub fn world_surface(grid: Option<&SurfaceGrid>, at: Vec3) -> u16 {
    grid.and_then(|g| g.nearest(at, 4.0 * UNIT))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// A deterministic stream of draws in 0..1.
#[derive(Default)]
pub struct Rolls(u64);

impl Rolls {
    pub fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

// ---------------------------------------------------------------------------
// Shots

/// What the weapons a shot names say.
type WeaponFxQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Weapon,
        Option<&'static CaWeaponFx>,
        Option<&'static CaMods>,
        Option<&'static CaViewParams>,
        Option<&'static CaZoom>,
    ),
>;

/// The fire sound of a weapon for its own shooter (`own`) or others: the
/// file and radius (u); None: nothing (a silenced part with no level).
pub fn fire_sound<'a>(fx: &'a CaWeaponFx, mods: Option<&CaMods>, own: bool) -> Option<(&'a str, f32)> {
    let plain = (fx.fire_sound.as_str(), mods.map_or(fx.fire_radius, |m| m.fire_sound_radius as f32));
    let Some(m) = mods else {
        return (!plain.0.is_empty()).then_some(plain);
    };
    let level = if own {
        m.fire_sound().silenced
    } else {
        match m.observer_sound() {
            ObserverSound::Plain => None,
            ObserverSound::Silenced { level } => Some(level),
            ObserverSound::Nothing => return None,
        }
    };
    let out = match level {
        Some(l) => (
            fx.silenced_sounds.get(l as usize - 1)?.as_str(),
            m.silenced_radius.get(l as usize - 1).copied().unwrap_or(m.fire_sound_radius) as f32,
        ),
        None => plain,
    };
    (!out.0.is_empty()).then_some(out)
}

/// The tracer frequency F a shooter's own client counts with: the last
/// fitted part (slot order) that names a tracer, else the ammo's.
pub fn own_tracer_frequency(t: &CaFxTables, fx: &CaWeaponFx, mods: Option<&CaMods>) -> Option<u32> {
    let ammo = t.tracer(&fx.tracer).map(|r| r.frequency);
    let part = mods.and_then(|m| {
        m.fitted()
            .filter_map(|p| t.attachment(&p.record.name).and_then(|a| a.tracer.as_deref()))
            .filter_map(|n| t.tracer(n))
            .last()
            .map(|r| r.frequency)
    });
    part.or(ammo)
}

/// The tracer's look: the muzzle part's tracer if it names one, else the
/// ammo's; None when the ammo has no tracer (then nothing is drawn).
pub fn tracer_look<'a>(t: &'a CaFxTables, fx: &CaWeaponFx, mods: Option<&CaMods>) -> Option<&'a data::TracerFx> {
    let ammo = t.tracer(&fx.tracer)?;
    let muzzle = mods
        .and_then(|m| m.part(MUZZLE))
        .and_then(|p| t.attachment(&p.record.name))
        .and_then(|a| a.tracer.as_deref())
        .and_then(|n| t.tracer(n));
    Some(muzzle.unwrap_or(ammo))
}

/// The third-person muzzle group: the muzzle part's, else the weapon's.
pub fn hh_muzzle<'a>(t: &'a CaFxTables, fx: &'a CaWeaponFx, mods: Option<&CaMods>) -> &'a str {
    mods.and_then(|m| m.part(MUZZLE))
        .and_then(|p| t.attachment(&p.record.name))
        .and_then(|a| a.hh_muzzle.as_deref())
        .unwrap_or(&fx.hh_muzzle)
}

/// The first-person muzzle group: the muzzle part's, else the weapon's
/// (`PVMuzzleFXName`, the view model's `muzzle_fx`).
pub fn pv_muzzle<'a>(t: &'a CaFxTables, view: &'a str, mods: Option<&CaMods>) -> &'a str {
    mods.and_then(|m| m.part(MUZZLE))
        .and_then(|p| t.attachment(&p.record.name))
        .and_then(|a| a.pv_muzzle.as_deref())
        .unwrap_or(view)
}

/// The impact sound of a surface for an ammo kind (section 11): one of
/// four slots of its list (an empty slot: silence).
pub fn impact_sound<'a>(record: &'a ImpactFx, surface: &'a data::Surface, kind: AmmoKind, roll: f32) -> Option<&'a str> {
    if record.sound != "surface" {
        return (!record.sound.is_empty()).then_some(record.sound.as_str());
    }
    let list = match kind {
        AmmoKind::Bullet => &surface.bullet,
        AmmoKind::Melee => &surface.melee,
        AmmoKind::Thrown => &surface.projectile,
    };
    rules::draw_slot(list, 4, roll)
}

/// One impact's decisions (section 6), for the systems and the tests.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Impact {
    pub sound: Option<(String, f32)>,
    pub ammo_fx: Option<String>,
    /// Decal sprite and width (u).
    pub mark: Option<(String, f32)>,
    pub burst: Option<String>,
}

/// What an impact on surface `surface` with record `record` makes, by
/// section 6's order and gates. `decals_take`: the hit object takes decals
/// (the static world); `own_far_unzoomed`: your own shot at a far impact
/// while not zoomed; rolls for the sound slot and the decal size.
#[allow(clippy::too_many_arguments)]
pub fn impact(
    record: &ImpactFx,
    surface: &data::Surface,
    kind: AmmoKind,
    seen: bool,
    decals_take: bool,
    own_far_unzoomed: bool,
    settings: &CaFxSettings,
    rolls: [f32; 2],
) -> Impact {
    let mut out = Impact::default();
    if surface.sky {
        return out;
    }
    out.sound = impact_sound(record, surface, kind, rolls[0]).map(|s| (s.to_string(), record.sound_radius));
    if !record.fx.is_empty() && (seen || record.area) {
        out.ammo_fx = Some(record.fx.clone());
    }
    if seen {
        if record.create_mark
            && surface.shows_mark
            && settings.mark_show
            && decals_take
            && !own_far_unzoomed
            && !surface.character
        {
            let sprite = if record.mark == "surface" { surface.mark.clone() } else { record.mark.clone() };
            if !sprite.is_empty() {
                out.mark = Some((sprite, rules::mark_width(record.mark_scale, surface.mark_scale, rolls[1])));
            }
        }
        if record.surface_fx {
            let group = if kind == AmmoKind::Thrown { &surface.grenade_fx } else { &surface.impact_fx };
            if !group.is_empty() {
                out.burst = Some(group.clone());
            }
        }
    }
    out
}

#[derive(Default)]
struct ShotState {
    rolls: Rolls,
    serial: u64,
}

/// A round that left a barrel this frame: its first ray may carry the
/// tracer.
struct Round {
    fire: Vec3,
    first_ray: bool,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn shot_effects(
    (mut events, mut remote, holders): (MessageReader<WeaponEvent>, MessageReader<RemoteShot>, Query<&Inventory>),
    cameras: CameraQuery,
    weapons: WeaponFxQuery,
    characters: Query<(Has<Health>, Has<Intent>, Option<&CaViewModel>, Option<&Transform>)>,
    grid: Option<Res<SurfaceGrid>>,
    tables: Res<CaFxTables>,
    settings: Res<CaFxSettings>,
    mut marks: ResMut<rules::CaMarks>,
    mut tracers: ResMut<CaTracers>,
    mut counters: ResMut<CaTracerCounters>,
    mut dings: ResMut<DingQueue>,
    mut sounds: MessageWriter<CaSound>,
    mut fx: MessageWriter<CaFxRequest>,
    mut state: Local<ShotState>,
    mut commands: Commands,
) {
    // Others' rounds on a network client arrive as `RemoteShot`s (their
    // traces as shots): each is a round fired.
    let fired: Vec<WeaponEvent> = remote
        .read()
        .filter_map(|r| {
            Some(WeaponEvent {
                owner: r.shooter,
                weapon: holders.get(r.shooter).ok()?.active?,
                kind: WeaponEventKind::Fired {
                    origin: r.origin,
                    yaw: r.yaw,
                    pitch: r.pitch,
                    seed: r.seed,
                    spread: r.spread,
                    mode: r.mode,
                },
                replay: false,
            })
        })
        .collect();
    let events: Vec<&WeaponEvent> = fired.iter().chain(events.read().filter(|e| e.shown())).collect();
    if events.is_empty() {
        return;
    }
    let cam = camera(&cameras);
    let me = cam.map(|c| c.player);
    // Head hits this frame (the blood's surface type).
    let heads: Vec<Entity> = events
        .iter()
        .filter_map(|e| match e.kind {
            WeaponEventKind::Hit {
                target,
                hitgroup: Hitgroup::Head,
                ..
            } => Some(target),
            _ => None,
        })
        .collect();
    let mut rounds: HashMap<Entity, Round> = HashMap::new();
    for e in events {
        let mine = Some(e.owner) == me;
        let Ok((weapon, wfx, mods, view, zoom)) = weapons.get(e.weapon) else { continue };
        let Some(wfx) = wfx else { continue };
        let fire_record = tables.fire(&wfx.fire);
        match &e.kind {
            WeaponEventKind::Fired { origin, yaw, pitch, .. } => {
                let dir = aim_of(*yaw, *pitch) * Vec3::NEG_Z;
                let vm = characters.get(e.owner).ok().and_then(|c| c.2);
                let fire = if mine { vm.map_or(*origin, |v| v.flash_point) } else { *origin };
                rounds.insert(e.owner, Round { fire, first_ray: true });
                // The fire sound.
                if let Some((file, radius)) = fire_sound(wfx, mods, mine) {
                    let silenced = mods.is_some_and(|m| m.silencer_level().is_some());
                    let mut s = if mine { CaSound::local(file) } else { CaSound::at(file, *origin, radius) };
                    if !silenced {
                        s = s.from_source(e.owner, CHANNEL_FIRE);
                    }
                    sounds.write(s);
                }
                let Some(cam) = cam else { continue };
                let gates = cam.gates(fire, fire + dir, !mine);
                if mine {
                    // Your FireFX group at your flash point.
                    if let Some(f) = fire_record.filter(|f| !f.fx.is_empty()) {
                        fx.write(CaFxRequest {
                            group: f.fx.clone(),
                            at: fire,
                            normal: dir,
                            kind: FxKind::OwnFire,
                        });
                    }
                } else if gates.fire_seen {
                    let group = hh_muzzle(&tables, wfx, mods);
                    if !group.is_empty() {
                        fx.write(CaFxRequest {
                            group: group.to_string(),
                            at: fire,
                            normal: dir,
                            kind: FxKind::Muzzle,
                        });
                    }
                }
                // A casing (8).
                if let Some(f) = fire_record
                    && f.shells
                    && settings.shell_casings
                    && gates.fire_seen
                    && !gates.fire_far
                {
                    let breach = if mine { vm.map_or(fire, |v| v.breach_point) } else { fire };
                    // The camera's frame (first person) or the shot's: the
                    // same view here.
                    let frame = aim_of(*yaw, *pitch);
                    state.serial += 1;
                    let serial = state.serial;
                    spawn_casing(&mut commands, &mut state.rolls, e.owner, &wfx.fire, f, breach, frame, mine, serial);
                }
                let _ = (weapon, view);
            }
            WeaponEventKind::Shot { from, to, hit, normal } | WeaponEventKind::ShotContinued { from, to, hit, normal } => {
                let first = matches!(e.kind, WeaponEventKind::Shot { .. });
                let round = rounds.get_mut(&e.owner);
                let fire = round.as_ref().map_or(*from, |r| r.fire);
                // 4: the tracer, once per ray, drawn on a round's first.
                if first && wfx.ammo_kind == AmmoKind::Bullet {
                    let f = if mine {
                        own_tracer_frequency(&tables, wfx, mods)
                    } else {
                        tables.tracer(&wfx.tracer).map(|t| t.frequency)
                    };
                    let first_ray = round.is_some_and(|r| std::mem::take(&mut r.first_ray));
                    if let Some(f) = f
                        && counters.0.entry(e.weapon).or_default().ray(f)
                        && first_ray
                        && settings.tracers
                        && let Some(look) = tracer_look(&tables, wfx, mods)
                        && let Some(path) = TracerPath::new(fire / UNIT, *to / UNIT, look.velocity, look.length)
                    {
                        if tracers.0.len() >= rules::TRACER_LIST_MAX {
                            tracers.0.remove(0);
                        }
                        tracers.0.push(CaTracer {
                            path,
                            look: look.clone(),
                            age: 0.0,
                        });
                    }
                }
                let Some(hit) = hit else { continue };
                let dir = (*to - *from).normalize_or_zero();
                let n = normal.unwrap_or(-dir);
                // The impact point, backed off 1 u along the ray.
                let at = *to - dir * UNIT;
                let (has_health, is_character, ..) = characters.get(*hit).unwrap_or((false, false, None, None));
                let character = has_health && is_character;
                let ty = if character {
                    if heads.contains(hit) { 4 } else { 6 }
                } else {
                    world_surface(grid.as_deref(), *to)
                };
                let surface = tables.surface(ty);
                let Some(record) = tables.impact(&wfx.impact) else { continue };
                let gates = cam.map(|c| c.gates(fire, at, !mine)).unwrap_or_default();
                let zoomed = zoom.is_some_and(|z| z.level > 0);
                let rolls = [state.rolls.next(), state.rolls.next()];
                let out = impact(
                    record,
                    surface,
                    wfx.ammo_kind,
                    gates.impact_seen,
                    !character,
                    mine && gates.impact_far && !zoomed,
                    &settings,
                    rolls,
                );
                emit_impact(&out, at, n, &mut sounds, &mut fx, &mut marks, &mut state.rolls, record);
                // 11: fly-by of others' bullets.
                if first
                    && !mine
                    && wfx.ammo_kind == AmmoKind::Bullet
                    && let Some(cam) = cam
                    && let Some(near) = rules::flyby(cam.at / UNIT, fire / UNIT, *to / UNIT, record.sound_radius)
                    && let Some(file) = rules::draw_any(&tables.sounds.flyby, state.rolls.next())
                {
                    let at = if settings.original_flyby {
                        rules::flyby_quirk_position(cam.at / UNIT, near)
                    } else {
                        near
                    };
                    let [r, big] = tables.sounds.flyby_radii;
                    sounds.write(CaSound {
                        radii: (r, big),
                        ..CaSound::at(file, at * UNIT, big)
                    });
                }
            }
            WeaponEventKind::Swing { at, .. } => {
                if let Some((file, radius)) = fire_sound(wfx, mods, true) {
                    let origin = characters.get(e.owner).ok().and_then(|c| c.3).map_or(Vec3::ZERO, |t| t.translation);
                    sounds.write(if mine { CaSound::local(file) } else { CaSound::at(file, origin, radius) });
                }
                let Some((point, n, hit)) = at else { continue };
                let (has_health, is_character, ..) = characters.get(*hit).unwrap_or((false, false, None, None));
                let ty = if has_health && is_character { 6 } else { world_surface(grid.as_deref(), *point) };
                let Some(record) = tables.impact(&wfx.impact) else { continue };
                let seen = cam.is_some_and(|c| c.gates(*point, *point, !mine).impact_seen);
                let rolls = [state.rolls.next(), state.rolls.next()];
                let out = impact(record, tables.surface(ty), AmmoKind::Melee, seen, false, false, &settings, rolls);
                emit_impact(&out, *point, *n, &mut sounds, &mut fx, &mut marks, &mut state.rolls, record);
            }
            WeaponEventKind::DryFire => {
                if wfx.dry_fire_sound.is_empty() {
                    continue;
                }
                let radius = mods.map_or(wfx.fire_radius, |m| m.fire_sound_radius as f32);
                let origin = characters.get(e.owner).ok().and_then(|c| c.3).map_or(Vec3::ZERO, |t| t.translation);
                sounds.write(if mine {
                    CaSound::local(&wfx.dry_fire_sound)
                } else {
                    CaSound::at(&wfx.dry_fire_sound, origin, radius)
                });
            }
            WeaponEventKind::Hit { target, .. } if mine => {
                let (has_health, is_character, ..) = characters.get(*target).unwrap_or((false, false, None, None));
                if has_health && is_character {
                    dings.push();
                }
            }
            _ => {}
        }
    }
}

/// Write an impact's sound, groups and decal.
#[allow(clippy::too_many_arguments)]
fn emit_impact(
    out: &Impact,
    at: Vec3,
    normal: Vec3,
    sounds: &mut MessageWriter<CaSound>,
    fx: &mut MessageWriter<CaFxRequest>,
    marks: &mut rules::CaMarks,
    rolls: &mut Rolls,
    record: &ImpactFx,
) {
    if let Some((file, radius)) = &out.sound {
        sounds.write(CaSound::at(file, at, *radius));
    }
    if let Some(g) = &out.ammo_fx {
        fx.write(CaFxRequest {
            group: g.clone(),
            at,
            normal,
            kind: FxKind::Ammo,
        });
    }
    if let Some((sprite, width)) = &out.mark {
        marks.add(rules::Mark {
            at: at / UNIT,
            normal,
            angle: rolls.next() * std::f32::consts::TAU,
            width: *width,
            sprite: sprite.clone(),
            grey: record.color_ratio(),
            age: 0.0,
            serial: 0,
        });
    }
    if let Some(g) = &out.burst {
        fx.write(CaFxRequest {
            group: g.clone(),
            at,
            normal,
            kind: FxKind::Surface,
        });
    }
}

/// A casing from `breach` (metres) in the frame of `aim` (the camera's
/// first person, the shot's third).
#[allow(clippy::too_many_arguments)]
fn spawn_casing(
    commands: &mut Commands,
    rolls: &mut Rolls,
    owner: Entity,
    fire: &str,
    record: &data::FireFx,
    breach: Vec3,
    aim: Quat,
    first_person: bool,
    serial: u64,
) {
    let local = if record.velocity != [0.0; 3] {
        Vec3::from(record.velocity)
    } else {
        rules::casing_velocity([rolls.next(), rolls.next(), rolls.next()], false)
    };
    // (right, up, forward) in the frame's axes.
    let velocity = aim * Vec3::X * local.x + aim * Vec3::Y * local.y + aim * Vec3::NEG_Z * local.z;
    let mut spin = Vec2::ZERO;
    for s in [&mut spin.x, &mut spin.y] {
        let v = rules::SHELL_SPIN.0 + (rules::SHELL_SPIN.1 - rules::SHELL_SPIN.0) * rolls.next();
        *s = if rolls.next() < 0.5 { -v } else { v };
    }
    commands.spawn((
        Name::new("Casing"),
        CaShell {
            owner,
            casing: Casing {
                at: breach / UNIT,
                velocity,
                spin,
                angles: Vec2::ZERO,
                bounces: 0,
                age: 0.0,
                resting: false,
                in_water: false,
                first_person,
            },
            fire: fire.to_string(),
            serial,
            sounded: false,
        },
        Transform::from_translation(breach).with_rotation(aim),
    ));
}

// ---------------------------------------------------------------------------
// View-model keys

#[allow(clippy::type_complexity)]
fn key_sounds(
    mut keys: MessageReader<CaViewKey>,
    cameras: CameraQuery,
    owners: Query<(&Inventory, Option<&Transform>, Option<&MovementState>, Option<&CaViewModel>, &Intent)>,
    weapons: Query<(Option<&CaWeaponFx>, Option<&CaMods>)>,
    tables: Res<CaFxTables>,
    settings: Res<CaFxSettings>,
    mut sounds: MessageWriter<CaSound>,
    mut state: Local<ShotState>,
    mut commands: Commands,
) {
    let me = camera(&cameras).map(|c| c.player);
    for k in keys.read() {
        let Ok((inv, t, ms, vm, intent)) = owners.get(k.owner) else { continue };
        let Some((Some(fx), mods)) = inv.active.and_then(|w| weapons.get(w).ok()) else { continue };
        let mine = Some(k.owner) == me;
        match k.key {
            ViewKey::Sound(n) => {
                let Some((file, radius)) = fx.key_sound(n) else { continue };
                let eye = t.map_or(Vec3::ZERO, |t| t.translation) + ms.map_or(Vec3::ZERO, |m| m.eye_offset);
                let at = vm.map_or(eye, |v| v.flash_point);
                sounds.write(if mine { CaSound::local(file) } else { CaSound::at(file, at, radius) });
            }
            ViewKey::ShellCasing if mine => {
                let Some(record) = tables.fire(&fx.fire) else { continue };
                if record.shells || !settings.shell_casings {
                    continue;
                }
                let Some(vm) = vm else { continue };
                state.serial += 1;
                let serial = state.serial;
                let shown = data::FireFx {
                    shells: true,
                    ..record.clone()
                };
                let _ = mods;
                spawn_casing(
                    &mut commands,
                    &mut state.rolls,
                    k.owner,
                    &fx.fire,
                    &shown,
                    vm.breach_point,
                    aim_of(intent.yaw, intent.pitch),
                    true,
                    serial,
                );
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Dings

/// The hit-confirm dings queued last frame (Quirk 11: the next character
/// update), local, one of the two files.
fn play_dings(mut dings: ResMut<DingQueue>, tables: Res<CaFxTables>, mut sounds: MessageWriter<CaSound>, mut rolls: Local<Rolls>) {
    for _ in 0..dings.drain() {
        if let Some(f) = rules::draw_any(&tables.sounds.ding, rolls.next()) {
            sounds.write(CaSound::local(f));
        }
    }
}

// ---------------------------------------------------------------------------
// Grenades

#[allow(clippy::too_many_arguments)]
fn grenade_effects(
    mut detonations: MessageReader<Detonated>,
    cameras: CameraQuery,
    grid: Option<Res<SurfaceGrid>>,
    tables: Res<CaFxTables>,
    settings: Res<CaFxSettings>,
    mut sounds: MessageWriter<CaSound>,
    mut fx: MessageWriter<CaFxRequest>,
    mut marks: ResMut<rules::CaMarks>,
    mut rolls: Local<Rolls>,
) {
    let cam = camera(&cameras);
    for d in detonations.read() {
        let Some(g) = tables.grenade.get(d.weapon) else { continue };
        let Some(record) = tables.impact(&g.impact) else { continue };
        let ty = grid
            .as_deref()
            .and_then(|g| g.nearest(d.at, GRENADE_SURFACE_REACH * UNIT))
            .map_or(1, |s| s.parse().unwrap_or(0));
        let surface = tables.surface(ty);
        let seen = cam.is_some_and(|c| c.gates(d.at, d.at, true).impact_seen);
        let mut out = impact(
            record,
            surface,
            AmmoKind::Thrown,
            seen,
            true,
            false,
            &settings,
            [rolls.next(), rolls.next()],
        );
        // The grenade's look is its surface's grenade group.
        if let Some(b) = out.burst.take() {
            fx.write(CaFxRequest {
                group: b,
                at: d.at,
                normal: Vec3::Y,
                kind: FxKind::Grenade,
            });
        }
        emit_impact(&out, d.at, Vec3::Y, &mut sounds, &mut fx, &mut marks, &mut rolls, record);
    }
}

/// How far (u) a detonation's point may lie from a surface for its type
/// to count (else it is in mid-air: Air).
pub const GRENADE_SURFACE_REACH: f32 = 16.0;

/// The server's bounce sounds: each new contact of a grenade plays its
/// surface's grenade sound (a new one replaces the grenade's last).
fn bounce_sounds(
    grenades: Query<(Entity, &CaGrenade), Changed<CaGrenade>>,
    grid: Option<Res<SurfaceGrid>>,
    tables: Res<CaFxTables>,
    mut seen: Local<HashMap<Entity, usize>>,
    mut sounds: MessageWriter<CaSound>,
) {
    for (e, g) in &grenades {
        let n = seen.entry(e).or_insert(0);
        for c in g.contacts.iter().skip(*n) {
            let at = c.at * g.unit;
            let surface = tables.surface(world_surface(grid.as_deref(), at));
            if surface.grenade.is_empty() {
                continue;
            }
            let radii = rules::server_radii(rules::inner_radius(surface.grenade_radius), surface.grenade_radius);
            sounds.write(sounds::server_sound(&surface.grenade, at, radii).from_source(e, CHANNEL_GRENADE));
        }
        *n = g.contacts.len();
    }
}

// ---------------------------------------------------------------------------
// Hurt

/// The hurt kinds of a hit: by damage type, then by zone.
pub fn hurt_kinds(kind: DamageKind, zone: Hitgroup, gas: bool) -> [Option<HurtKind>; 2] {
    let by_type = match kind {
        DamageKind::Bullet | DamageKind::Melee => Some(HurtKind::Guns),
        DamageKind::Blast => Some(HurtKind::Explosions),
        DamageKind::Burn => Some(HurtKind::Burning),
        DamageKind::Fall => Some(HurtKind::Falling),
        _ if gas => Some(HurtKind::Gas),
        _ => None,
    };
    let by_zone = match zone {
        Hitgroup::Head => Some(HurtKind::Head),
        // The groin's zone.
        Hitgroup::Stomach => Some(HurtKind::Groin),
        _ => None,
    };
    [by_type, by_zone]
}

/// A hurt kind's files.
pub fn hurt_files(t: &CaFxTables, k: HurtKind) -> &[String] {
    let s = &t.sounds;
    match k {
        HurtKind::Guns => &s.guns,
        HurtKind::Explosions => &s.explosions,
        HurtKind::Burning => &s.burning,
        HurtKind::Gas => &s.gas,
        HurtKind::Falling => &s.falling,
        HurtKind::Head => &s.head,
        HurtKind::Groin => &s.groin,
    }
}

/// The length a hurt sound is taken to play when its clip isn't loaded
/// (no install): ours.
pub const HURT_DEFAULT_LENGTH: f32 = 1.0;

#[allow(clippy::too_many_arguments)]
fn hurt_sounds(
    mut damage: MessageReader<Damage>,
    players: Query<(), (With<Intent>, With<Health>)>,
    tables: Res<CaFxTables>,
    lengths: Res<CaSoundLengths>,
    mut limiter: ResMut<HurtLimiter>,
    time: Res<Time<Real>>,
    mut sounds: MessageWriter<CaSound>,
    mut rolls: Local<Rolls>,
) {
    let now = time.elapsed_secs_f64();
    for d in damage.read() {
        if d.amount <= 0.0 || players.get(d.target).is_err() {
            continue;
        }
        let gas = d.weapon.is_some_and(|w| w.ends_with("g7a"));
        for k in hurt_kinds(d.kind, d.hitgroup, gas).into_iter().flatten() {
            let Some(file) = rules::draw_any(hurt_files(&tables, k), rolls.next()) else { continue };
            let length = lengths.0.get(file).copied().unwrap_or(HURT_DEFAULT_LENGTH) as f64;
            if !limiter.try_start(k, now, length) {
                continue;
            }
            let r = tables.sounds.hurt_radius;
            let radii = rules::server_radii(rules::inner_radius(r), r);
            sounds.write(sounds::server_sound(file, d.point, radii));
        }
    }
}

// ---------------------------------------------------------------------------
// Footsteps

#[allow(clippy::type_complexity)]
fn footsteps(
    time: Res<Time>,
    mut players: Query<(
        Entity,
        &Transform,
        Option<&MovementState>,
        Option<&Intent>,
        Option<&Health>,
        Has<LocalPlayer>,
        Option<&mut OwnSteps>,
        Option<&mut OthersSteps>,
    )>,
    grid: Option<Res<SurfaceGrid>>,
    tables: Res<CaFxTables>,
    mut sounds: MessageWriter<CaSound>,
    mut rolls: Local<Rolls>,
    mut last: Local<HashMap<Entity, (Vec3, bool)>>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for (e, t, ms, intent, health, local, own, others) in &mut players {
        let Some(ms) = ms else { continue };
        let alive = health.is_none_or(|h| h.current > 0.0);
        let (before, was_ground) = last.insert(e, (t.translation, ms.on_ground)).unwrap_or((t.translation, ms.on_ground));
        let moved = t.translation.with_y(0.0).distance(before.with_y(0.0)) / UNIT;
        let landed = ms.on_ground && !was_ground;
        let jumped = !ms.on_ground && was_ground && intent.is_some_and(|i| i.jump);
        let feet = t.translation + Vec3::Y * ms.hull_min.y;
        let surface = |foot_roll: f32, rolls: &mut Rolls| -> Option<String> {
            let ty = if ms.on_ladder {
                200
            } else {
                grid.as_deref()
                    .and_then(|g| g.below(feet + Vec3::Y * 50.0 * UNIT, 100.0 * UNIT))
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0)
            };
            let s = tables.surface(ty);
            if s.invisible {
                return None;
            }
            let list = if foot_roll < 0.5 { &s.right } else { &s.left };
            rules::draw_slot(list, 2, rolls.next()).map(str::to_string)
        };
        if local {
            let Some(mut own) = own else {
                commands.entity(e).insert(OwnSteps::default());
                continue;
            };
            let walk = intent.is_some_and(|i| i.walk);
            let inputs = StepInputs {
                dt,
                moving: moved > 0.1 && intent.is_some_and(|i| i.move_axis != Vec2::ZERO),
                can_step: ms.on_ground || ms.on_ladder,
                run: (!walk || ms.sprinting) && !ms.crouching,
                jumped,
                landed,
                alive,
            };
            for foot in own.step(&inputs) {
                let roll = if foot == rules::Foot::Right { 0.0 } else { 1.0 };
                if let Some(f) = surface(roll, &mut rolls) {
                    sounds.write(CaSound::local(f));
                }
            }
            if jumped && alive && let Some(f) = rules::draw_any(&tables.sounds.jump, rolls.next()) {
                sounds.write(CaSound::local(f));
            }
            continue;
        }
        let Some(mut steps) = others else {
            commands.entity(e).insert(OthersSteps::default());
            continue;
        };
        if !alive {
            continue;
        }
        let speed = moved / dt;
        let gait = if !ms.on_ground {
            Gait::Jump
        } else if speed < 10.0 {
            Gait::Still
        } else if ms.crouching {
            Gait::Crouch
        } else if intent.is_some_and(|i| i.walk) && !ms.sprinting {
            Gait::Walk
        } else if ms.sprinting {
            Gait::Sprint
        } else {
            Gait::Run
        };
        let n = match gait {
            // A jump's one key is its landing.
            Gait::Jump => 0,
            _ if landed && steps.airborne => 1,
            _ => rules::footsteps_between(gait, steps.clock, steps.clock + dt),
        };
        steps.airborne = !ms.on_ground;
        steps.clock = if matches!(gait, Gait::Run | Gait::Sprint) { steps.clock + dt } else { 0.0 };
        for i in 0..n {
            let roll = if (steps.clock * 1000.0) as u32 / 300 % 2 == 0 && i == 0 { 0.0 } else { 1.0 };
            if let Some(f) = surface(roll, &mut rolls) {
                sounds.write(CaSound::at(f, feet, rules::FOOTSTEP_RADIUS));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Stepping

#[allow(clippy::too_many_arguments)]
fn step_effects(
    time: Res<Time>,
    mut marks: ResMut<rules::CaMarks>,
    mut tracers: ResMut<CaTracers>,
    mut shells: Query<(Entity, &mut CaShell, &mut Transform)>,
    owners: Query<&Transform, Without<CaShell>>,
    characters: Query<(), With<Intent>>,
    spatial: SpatialQuery,
    grid: Option<Res<SurfaceGrid>>,
    tables: Res<CaFxTables>,
    mut sounds: MessageWriter<CaSound>,
    mut last_owner: Local<HashMap<Entity, Vec3>>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    marks.step(dt);
    for t in &mut tracers.0 {
        t.age += dt;
    }
    tracers.0.retain(|t| t.path.ends(t.age).is_some());
    let filter = SpatialQueryFilter::from_mask(SOLID_LAYERS);
    let mut alive: Vec<(u64, Entity)> = Vec::new();
    let mut moved: HashMap<Entity, Vec3> = HashMap::new();
    for (e, mut s, mut t) in &mut shells {
        // Your own displacement this frame (Quirk 15).
        let carry = *moved.entry(s.owner).or_insert_with(|| {
            let now = owners.get(s.owner).map_or(Vec3::ZERO, |o| o.translation);
            let before = last_owner.get(&s.owner).copied().unwrap_or(now);
            (now - before) / UNIT
        });
        let step = s.casing.step(dt, carry, |from, to| {
            let d = (to - from) * UNIT;
            let dir = Dir3::new(d).ok()?;
            // Characters are passed through.
            let hit = spatial.cast_ray_predicate(from * UNIT, dir, d.length(), true, &filter, &|e| {
                !characters.contains(e)
            })?;
            let at = from + (to - from) * (hit.distance / d.length());
            let surface = tables.surface(world_surface(grid.as_deref(), at * UNIT));
            Some(CasingHit {
                at,
                normal: hit.normal,
                removes: surface.sky || surface.invisible,
            })
        });
        match step {
            CasingStep::Removed => {
                commands.entity(e).despawn();
                continue;
            }
            CasingStep::FirstBounce if !s.casing.in_water && !s.sounded => {
                s.sounded = true;
                let surface = tables.surface(world_surface(grid.as_deref(), s.casing.at * UNIT));
                if !surface.shell.is_empty() {
                    sounds.write(CaSound::at(&surface.shell, s.casing.at * UNIT, surface.shell_radius));
                }
            }
            _ => {}
        }
        t.translation = s.casing.at * UNIT;
        let a = s.casing.angles;
        t.rotation = Quat::from_euler(EulerRot::YXZ, a.x, a.y, 0.0);
        alive.push((s.serial, e));
    }
    for owner in moved.keys() {
        if let Ok(o) = owners.get(*owner) {
            last_owner.insert(*owner, o.translation);
        }
    }
    // The list's cap: the oldest go.
    if alive.len() > rules::SHELL_LIST_MAX {
        alive.sort_unstable();
        for (_, e) in &alive[..alive.len() - rules::SHELL_LIST_MAX] {
            commands.entity(*e).despawn();
        }
    }
}

/// Start the requested groups (when the effect file is loaded) and play
/// the groups on Chimera's particle pool.
#[allow(clippy::too_many_arguments)]
fn spawn_fx(
    time: Res<Time>,
    mut requests: MessageReader<CaFxRequest>,
    mut fx: ResMut<CaFx>,
    particles: Option<ResMut<Particles>>,
    materials: Option<Res<ParticleMaterials>>,
    world: WorldTracer,
) {
    let Some(lib) = library::library() else {
        requests.clear();
        return;
    };
    for r in requests.read() {
        fx.spawn(&lib, &r.group, r.at, effect_frame(r.normal));
    }
    let (Some(mut particles), Some(materials)) = (particles, materials) else { return };
    fx.step(time.delta_secs(), &lib, &mut particles, &materials.0, &world);
}
